//! Journal file indexing infrastructure.
//!
//! This module provides infrastructure for indexing journal files:
//! - Batch parallel indexing with time budget enforcement
//! - Cache builder for file indexes

use crate::{
    cache::{FileIndexCache, FileIndexKey},
    error::{EngineError, Result},
    query_time_range::QueryTimeRange,
};
use journal_index::{FileIndex, FileIndexer, IndexingLimits, Seconds};
use journal_registry::Registry;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use tokio_util::sync::CancellationToken;
use tracing::{error, trace};

const MAX_BATCH_INDEX_THREADS: usize = 4;

// ============================================================================
// File Index Cache Builder
// ============================================================================

/// Builder for constructing a FileIndexCache with custom configuration.
pub struct FileIndexCacheBuilder {
    cache_path: Option<std::path::PathBuf>,
    memory_capacity: Option<usize>,
    disk_capacity: Option<usize>,
    block_size: Option<usize>,
    enable_disk_cache: bool,
}

impl FileIndexCacheBuilder {
    /// Creates a new builder with no configuration.
    ///
    /// All options use defaults if not explicitly set:
    /// - Cache path: temp directory + "journal-engine-cache"
    /// - Memory capacity: 128 entries
    /// - Disk capacity: 16 MB
    /// - Block size: 4 MB
    pub fn new() -> Self {
        Self {
            cache_path: None,
            memory_capacity: None,
            disk_capacity: None,
            block_size: None,
            enable_disk_cache: true,
        }
    }

    /// Sets the cache directory path.
    pub fn with_cache_path(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.cache_path = Some(path.into());
        self
    }

    /// Sets the memory capacity (number of items to keep in memory).
    pub fn with_memory_capacity(mut self, capacity: usize) -> Self {
        self.memory_capacity = Some(capacity);
        self
    }

    /// Sets the disk capacity in bytes.
    pub fn with_disk_capacity(mut self, capacity: usize) -> Self {
        self.disk_capacity = Some(capacity);
        self
    }

    /// Sets the block size in bytes.
    pub fn with_block_size(mut self, size: usize) -> Self {
        self.block_size = Some(size);
        self
    }

    /// Disables the disk-backed cache and keeps indexes in memory only.
    pub fn without_disk_cache(mut self) -> Self {
        self.enable_disk_cache = false;
        self
    }

    /// Builds the FileIndexCache with the configured settings.
    pub async fn build(self) -> Result<FileIndexCache> {
        use foyer::HybridCacheBuilder;

        let memory_capacity = self.memory_capacity.unwrap_or(128);
        let memory = HybridCacheBuilder::new()
            .with_name("file-index-cache")
            .with_policy(foyer::HybridCachePolicy::WriteOnInsertion)
            .memory(memory_capacity)
            .with_shards(4);

        if !self.enable_disk_cache {
            return memory.storage().build().await.map_err(Into::into);
        }

        use foyer::{
            BlockEngineBuilder, DeviceBuilder, FsDeviceBuilder, IoEngineBuilder,
            PsyncIoEngineBuilder,
        };

        let cache_path = self
            .cache_path
            // nosemgrep: rust.lang.security.temp-dir.temp-dir -- caller-configurable non-sensitive disk cache default.
            .unwrap_or_else(|| std::env::temp_dir().join("journal-engine-cache"));
        let disk_capacity = self.disk_capacity.unwrap_or(16 * 1024 * 1024);
        let block_size = self.block_size.unwrap_or(4 * 1024 * 1024);

        std::fs::create_dir_all(&cache_path).map_err(|e| {
            EngineError::Io(std::io::Error::other(format!(
                "Failed to create cache directory: {}",
                e
            )))
        })?;

        let cache = memory
            .storage()
            .with_io_engine(PsyncIoEngineBuilder::new().build().await?)
            .with_engine_config(
                BlockEngineBuilder::new(
                    FsDeviceBuilder::new(&cache_path)
                        .with_capacity(disk_capacity)
                        .build()?,
                )
                .with_block_size(block_size),
            )
            .build()
            .await?;

        Ok(cache)
    }
}

impl Default for FileIndexCacheBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test(flavor = "current_thread")]
    async fn build_without_disk_cache_does_not_create_disk_cache_files() {
        let tmp = tempdir().expect("tempdir");
        let cache_path = tmp.path().join("foyer-cache");
        let cache = FileIndexCacheBuilder::new()
            .with_cache_path(&cache_path)
            .with_memory_capacity(4)
            .without_disk_cache()
            .build()
            .await
            .expect("build in-memory file index cache");

        cache
            .close()
            .await
            .expect("close in-memory file index cache");
        assert!(
            !cache_path.exists(),
            "expected memory-only file index cache to avoid creating {}",
            cache_path.display()
        );
    }
}

// ============================================================================
// Batch Processing
// ============================================================================

/// Batch computes file indexes in parallel using rayon, with cache checking and time budget enforcement.
///
/// This function:
/// 1. Checks cache for all keys upfront
/// 2. Identifies cache misses
/// 3. Uses tokio::task to compute missing indexes in parallel
/// 4. Inserts newly computed indexes into cache
/// 5. Returns all results (cached + newly computed)
///
/// # Arguments
/// * `cache` - The file index cache
/// * `registry` - Registry to update with file metadata
/// * `keys` - Vector of (file, facets, source_timestamp_field) to fetch/compute indexes for
/// * `time_range` - Query time range for bucket duration calculation
/// * `cancellation` - Token to signal cancellation from the caller
/// * `indexing_limits` - Configuration limits for indexing (cardinality, payload size)
/// * `progress_counter` - Optional atomic counter incremented after each file is indexed
///
/// # Returns
/// Vector of responses for each key. Successful responses contain the file index.
/// If cancelled, returns Cancelled error.
pub async fn batch_compute_file_indexes(
    cache: &FileIndexCache,
    registry: &Registry,
    keys: Vec<FileIndexKey>,
    time_range: &QueryTimeRange,
    cancellation: CancellationToken,
    indexing_limits: IndexingLimits,
    progress_counter: Option<Arc<AtomicUsize>>,
) -> Result<Vec<(FileIndexKey, FileIndex)>> {
    let bucket_duration = time_range.bucket_duration_seconds();
    let cache_lookup_results = lookup_cached_indexes(cache, &keys, &cancellation).await?;
    let CachePartition {
        mut responses,
        keys_to_compute,
        stats,
    } = partition_cache_results(cache_lookup_results, keys.len(), bucket_duration);

    if cancellation.is_cancelled() {
        return Err(EngineError::Cancelled);
    }

    trace!(
        "phase 2 summary: hits={}, misses={}, stale={}, incompatible_bucket={}",
        stats.cache_hits, stats.cache_misses, stats.stale_entries, stats.incompatible_bucket
    );

    let computed_results = compute_missing_indexes(
        keys_to_compute,
        bucket_duration,
        cancellation.clone(),
        indexing_limits,
        progress_counter,
    )
    .await?;

    store_computed_indexes(registry, cache, &mut responses, computed_results);
    Ok(responses)
}

async fn lookup_cached_indexes(
    cache: &FileIndexCache,
    keys: &[FileIndexKey],
    cancellation: &CancellationToken,
) -> Result<Vec<(FileIndexKey, Result<Option<FileIndex>>)>> {
    let cache_lookup_futures = keys.iter().map(|key| {
        let key_clone = key.clone();
        async move {
            let cached = cache
                .get(&key_clone)
                .await
                .map(|entry| entry.map(|e| e.value().clone()))
                .map_err(|e| e.into());
            (key_clone, cached)
        }
    });

    tokio::select! {
        results = futures::future::join_all(cache_lookup_futures) => Ok(results),
        _ = cancellation.cancelled() => Err(EngineError::Cancelled),
    }
}

#[derive(Default)]
struct CacheStats {
    cache_hits: usize,
    cache_misses: usize,
    stale_entries: usize,
    incompatible_bucket: usize,
}

struct CachePartition {
    responses: Vec<(FileIndexKey, FileIndex)>,
    keys_to_compute: Vec<FileIndexKey>,
    stats: CacheStats,
}

fn partition_cache_results(
    cache_lookup_results: Vec<(FileIndexKey, Result<Option<FileIndex>>)>,
    key_count: usize,
    bucket_duration: Seconds,
) -> CachePartition {
    let mut partition = CachePartition {
        responses: Vec::with_capacity(key_count),
        keys_to_compute: Vec::new(),
        stats: CacheStats::default(),
    };

    for (key, cache_lookup_result) in cache_lookup_results {
        partition_cache_result(key, cache_lookup_result, bucket_duration, &mut partition);
    }

    partition
}

fn partition_cache_result(
    key: FileIndexKey,
    cache_lookup_result: Result<Option<FileIndex>>,
    bucket_duration: Seconds,
    partition: &mut CachePartition,
) {
    match cache_lookup_result {
        Ok(Some(file_index)) => partition_cached_index(key, file_index, bucket_duration, partition),
        Ok(None) => {
            partition.stats.cache_misses += 1;
            partition.keys_to_compute.push(key);
        }
        Err(e) => {
            error!("cached file index lookup error {}", e);
        }
    }
}

fn partition_cached_index(
    key: FileIndexKey,
    file_index: FileIndex,
    bucket_duration: Seconds,
    partition: &mut CachePartition,
) {
    let fresh = file_index.is_fresh();
    let bucket_ok = compatible_bucket_duration(&file_index, bucket_duration);

    if fresh && bucket_ok {
        partition.stats.cache_hits += 1;
        partition.responses.push((key, file_index));
        return;
    }

    if !fresh {
        partition.stats.stale_entries += 1;
    }
    if !bucket_ok {
        partition.stats.incompatible_bucket += 1;
    }
    partition.keys_to_compute.push(key);
}

fn compatible_bucket_duration(file_index: &FileIndex, bucket_duration: Seconds) -> bool {
    file_index.bucket_duration() <= bucket_duration
        && bucket_duration.is_multiple_of(file_index.bucket_duration())
}

async fn compute_missing_indexes(
    keys_to_compute: Vec<FileIndexKey>,
    bucket_duration: Seconds,
    cancellation: CancellationToken,
    indexing_limits: IndexingLimits,
    progress_counter: Option<Arc<AtomicUsize>>,
) -> Result<Vec<(FileIndexKey, Result<FileIndex>)>> {
    let compute_threads = compute_thread_count(keys_to_compute.len());
    let cancellation_for_select = cancellation.clone();
    let compute_task = tokio::task::spawn_blocking(move || {
        compute_missing_indexes_blocking(
            keys_to_compute,
            bucket_duration,
            cancellation,
            indexing_limits,
            progress_counter,
            compute_threads,
        )
    });

    tokio::select! {
        result = compute_task => match result {
            Ok(result) => result,
            Err(e) => Err(EngineError::Io(std::io::Error::other(format!(
                "Blocking task panicked: {}",
                e
            )))),
        },
        _ = cancellation_for_select.cancelled() => Err(EngineError::Cancelled),
    }
}

fn compute_thread_count(key_count: usize) -> usize {
    key_count.max(1).min(
        std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1)
            .min(MAX_BATCH_INDEX_THREADS),
    )
}

fn compute_missing_indexes_blocking(
    keys_to_compute: Vec<FileIndexKey>,
    bucket_duration: Seconds,
    cancellation: CancellationToken,
    indexing_limits: IndexingLimits,
    progress_counter: Option<Arc<AtomicUsize>>,
    compute_threads: usize,
) -> Result<Vec<(FileIndexKey, Result<FileIndex>)>> {
    use rayon::prelude::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    let cancelled = Arc::new(AtomicBool::new(false));
    let thread_pool = build_index_thread_pool(compute_threads)?;

    Ok(thread_pool.install(|| {
        keys_to_compute
            .into_par_iter()
            .map(|key| {
                compute_one_index(
                    key,
                    bucket_duration,
                    &cancellation,
                    indexing_limits,
                    progress_counter.as_ref(),
                    &cancelled,
                )
            })
            .collect::<Vec<(FileIndexKey, Result<FileIndex>)>>()
    }))
}

fn build_index_thread_pool(compute_threads: usize) -> Result<rayon::ThreadPool> {
    // Build a bounded local pool per call instead of using Rayon’s global pool.
    // The global pool previously stayed alive after rebuild/indexing and kept a
    // full worker set plus allocator arenas resident in the plugin process.
    rayon::ThreadPoolBuilder::new()
        .num_threads(compute_threads)
        .build()
        .map_err(|err| {
            EngineError::Io(std::io::Error::other(format!(
                "failed to build rayon index pool: {}",
                err
            )))
        })
}

fn compute_one_index(
    key: FileIndexKey,
    bucket_duration: Seconds,
    cancellation: &CancellationToken,
    indexing_limits: IndexingLimits,
    progress_counter: Option<&Arc<AtomicUsize>>,
    cancelled: &std::sync::atomic::AtomicBool,
) -> (FileIndexKey, Result<FileIndex>) {
    if cancellation.is_cancelled() || cancelled.load(std::sync::atomic::Ordering::Relaxed) {
        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        return (key, Err(EngineError::Cancelled));
    }

    let result = index_one_file(&key, bucket_duration, indexing_limits);
    if result.is_ok()
        && let Some(counter) = progress_counter
    {
        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    (key, result)
}

fn index_one_file(
    key: &FileIndexKey,
    bucket_duration: Seconds,
    indexing_limits: IndexingLimits,
) -> Result<FileIndex> {
    FileIndexer::new(indexing_limits)
        .index(
            &key.file,
            key.source_timestamp_field.as_ref(),
            key.facets.as_slice(),
            bucket_duration,
        )
        .map_err(|e| e.into())
}

fn store_computed_indexes(
    registry: &Registry,
    cache: &FileIndexCache,
    responses: &mut Vec<(FileIndexKey, FileIndex)>,
    computed_results: Vec<(FileIndexKey, Result<FileIndex>)>,
) {
    for (key, response) in computed_results {
        match response {
            Ok(index) => {
                update_registry_time_range(registry, &key, &index);
                cache.insert(key.clone(), index.clone());
                responses.push((key, index));
            }
            Err(e) => {
                error!(
                    "file index computation failed for file={}: {}",
                    key.file.path(),
                    e
                );
            }
        }
    }
}

fn update_registry_time_range(registry: &Registry, key: &FileIndexKey, index: &FileIndex) {
    registry.update_time_range(
        &key.file,
        index.start_time(),
        index.end_time(),
        index.indexed_at(),
        index.online(),
    );
}
