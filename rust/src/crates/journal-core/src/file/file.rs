#![allow(clippy::field_reassign_with_default)]

use super::mmap::{
    ExperimentalMmapStrategy, MemoryMap, MemoryMapMut, WindowManager, WindowManagerStats,
};
use crate::error::{JournalError, Result};
use crate::file::guarded_cell::GuardedCell;
use crate::file::hash;
use crate::file::object::*;
use crate::file::offset_array;
use std::fs::{File, OpenOptions};
use std::marker::PhantomData;
use std::num::NonZeroU64;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::Duration;
use zerocopy::{ByteSlice, FromBytes};

use crate::file::value_guard::ValueGuard;

// Size to pad objects to (8 bytes)
const OBJECT_ALIGNMENT: u64 = 8;
const FILE_SIZE_INCREASE: u64 = 8 * 1024 * 1024;
const JOURNAL_COMPACT_SIZE_MAX: u64 = u32::MAX as u64;
const DEFAULT_MAX_FILE_SIZE: u64 = 128 * 1024 * 1024;
const JOURNAL_FILE_SIZE_MIN: u64 = 512 * 1024;
const PAGE_SIZE: u64 = 4096;
const DEFAULT_DATA_HASH_TABLE_SIZE: usize = 2047;
const DEFAULT_FIELD_HASH_TABLE_SIZE: usize = 1023;
pub const DEFAULT_COMPRESS_THRESHOLD: usize = 512;
pub const MIN_COMPRESS_THRESHOLD: usize = 8;

pub fn normalize_compress_threshold(threshold: usize) -> usize {
    threshold.max(MIN_COMPRESS_THRESHOLD)
}

fn align_to(value: u64, alignment: u64) -> u64 {
    value.saturating_add(alignment.saturating_sub(1)) & !(alignment.saturating_sub(1))
}

fn normalize_journal_max_file_size(max_file_size: Option<u64>, compact: bool) -> u64 {
    let mut size = match max_file_size {
        Some(0) | None => DEFAULT_MAX_FILE_SIZE,
        Some(size) => align_to(size, PAGE_SIZE),
    };
    if compact && size > JOURNAL_COMPACT_SIZE_MAX {
        size = JOURNAL_COMPACT_SIZE_MAX;
    }
    size.max(JOURNAL_FILE_SIZE_MIN)
}

fn data_hash_buckets_for_max_file_size(max_file_size: u64) -> usize {
    let buckets = (max_file_size / 576).max(DEFAULT_DATA_HASH_TABLE_SIZE as u64);
    buckets.min(usize::MAX as u64) as usize
}

/// Validates that an offset is properly aligned for journal objects.
/// Journal objects must be 8-byte aligned.
fn validate_offset_alignment(offset: NonZeroU64) -> Result<()> {
    if offset.get() % OBJECT_ALIGNMENT != 0 {
        return Err(JournalError::MisalignedOffset(offset.get()));
    }
    Ok(())
}

fn round_up_to_file_size_increment(value: u64) -> Result<u64> {
    value
        .checked_add(FILE_SIZE_INCREASE - 1)
        .map(|v| v & !(FILE_SIZE_INCREASE - 1))
        .ok_or(JournalError::ObjectExceedsFileBounds)
}

pub trait BucketVisitor<'a> {
    type Object: JournalObject<&'a [u8]> + HashableObject;
    type Output;

    /// Called for each object in the bucket. Return Some(output) to stop iteration,
    /// or None to continue to the next object.
    fn visit(&mut self, object: &ValueGuard<'a, Self::Object>) -> Result<Option<Self::Output>>;
}

#[derive(Debug, Clone, Copy)]
pub struct PayloadParts<'a> {
    parts: [&'a [u8]; 3],
    len: usize,
    count: usize,
}

impl<'a> PayloadParts<'a> {
    pub fn raw(payload: &'a [u8]) -> Self {
        Self {
            parts: [payload, &[], &[]],
            len: payload.len(),
            count: 1,
        }
    }

    pub fn structured(name: &'a [u8], value: &'a [u8]) -> Self {
        Self {
            parts: [name, b"=", value],
            len: name.len() + 1 + value.len(),
            count: 3,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn iter(&self) -> std::iter::Copied<std::slice::Iter<'_, &'a [u8]>> {
        self.parts[..self.count].iter().copied()
    }

    pub fn as_single_slice(&self) -> Option<&'a [u8]> {
        (self.count == 1).then_some(self.parts[0])
    }

    pub fn equals_slice(&self, other: &[u8]) -> bool {
        if other.len() != self.len {
            return false;
        }

        let mut remaining = other;
        for part in self.iter() {
            let Some((head, tail)) = remaining.split_at_checked(part.len()) else {
                return false;
            };
            if head != part {
                return false;
            }
            remaining = tail;
        }

        remaining.is_empty()
    }

    pub fn copy_to_slice(&self, dst: &mut [u8]) {
        assert_eq!(dst.len(), self.len);
        let mut offset = 0usize;
        for part in self.iter() {
            let end = offset + part.len();
            dst[offset..end].copy_from_slice(part);
            offset = end;
        }
    }

    pub fn to_vec(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(self.len);
        for part in self.iter() {
            payload.extend_from_slice(part);
        }
        payload
    }
}

struct PayloadMatcher<'data, T> {
    payload: PayloadParts<'data>,
    hash: u64,
    decompression_buffer: Vec<u8>,
    _phantom: PhantomData<T>,
}

impl<'data, B: ByteSlice> PayloadMatcher<'data, FieldObject<B>> {
    fn field_matcher(payload: &'data [u8], hash: u64) -> Self {
        Self {
            payload: PayloadParts::raw(payload),
            hash,
            decompression_buffer: Vec::new(),
            _phantom: PhantomData::<FieldObject<B>>,
        }
    }
}

impl<'a, T> BucketVisitor<'a> for PayloadMatcher<'_, T>
where
    T: JournalObject<&'a [u8]> + HashableObject,
{
    type Object = T;
    type Output = NonZeroU64;

    fn visit(&mut self, object: &ValueGuard<'a, Self::Object>) -> Result<Option<Self::Output>> {
        if object.hash() != self.hash {
            return Ok(None);
        }

        let matches = if object.is_compressed() {
            let len = object.decompress(&mut self.decompression_buffer)?;
            self.payload.equals_slice(&self.decompression_buffer[..len])
        } else {
            self.payload.equals_slice(object.raw_payload())
        };

        if matches {
            Ok(Some(object.offset()))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    Xz,
    Lz4,
    Zstd,
}

impl Compression {
    pub fn as_incompatible_flag(&self) -> u32 {
        match self {
            Compression::None => 0,
            Compression::Xz => HeaderIncompatibleFlags::CompressedXz as u32,
            Compression::Lz4 => HeaderIncompatibleFlags::CompressedLz4 as u32,
            Compression::Zstd => HeaderIncompatibleFlags::CompressedZstd as u32,
        }
    }
}

impl Default for Compression {
    fn default() -> Self {
        Compression::None
    }
}

#[derive(Debug, Clone)]
pub struct JournalFileOptions {
    machine_id: uuid::Uuid,
    seqnum_id: uuid::Uuid,
    file_id: uuid::Uuid,
    window_size: u64,
    data_hash_table_buckets: usize,
    field_hash_table_buckets: usize,
    enable_keyed_hash: bool,
    compression: Compression,
    compress_threshold: usize,
    compact: bool,
    experimental_mmap_strategy: ExperimentalMmapStrategy,
    pub seal: Option<crate::seal::SealOptions>,
}

impl JournalFileOptions {
    pub fn new(machine_id: uuid::Uuid, _boot_id: uuid::Uuid, seqnum_id: uuid::Uuid) -> Self {
        let file_id = uuid::Uuid::new_v4();

        Self {
            machine_id,
            seqnum_id,
            file_id,
            window_size: 64 * 1024,
            data_hash_table_buckets: 233_016,
            field_hash_table_buckets: DEFAULT_FIELD_HASH_TABLE_SIZE,
            enable_keyed_hash: true,
            compression: Compression::None,
            compress_threshold: DEFAULT_COMPRESS_THRESHOLD,
            compact: false,
            experimental_mmap_strategy: ExperimentalMmapStrategy::Windowed,
            seal: None,
        }
    }

    /// Creates options with bucket sizes optimized based on previous utilization
    pub fn with_optimized_buckets(
        mut self,
        previous_utilization: Option<BucketUtilization>,
        max_file_size: Option<u64>,
    ) -> Self {
        let _ = previous_utilization;
        let max_file_size = normalize_journal_max_file_size(max_file_size, self.compact);

        self.data_hash_table_buckets = data_hash_buckets_for_max_file_size(max_file_size);
        self.field_hash_table_buckets = DEFAULT_FIELD_HASH_TABLE_SIZE;
        self
    }

    pub fn with_window_size(mut self, size: u64) -> Self {
        assert_eq!(size % OBJECT_ALIGNMENT, 0);
        assert_eq!(size % 4096, 0, "Window size must be page-aligned");
        self.window_size = size;
        self
    }

    pub fn with_data_hash_table_buckets(mut self, buckets: usize) -> Self {
        assert!(buckets > 0, "Hash table buckets must be positive");
        self.data_hash_table_buckets = buckets;
        self
    }

    pub fn with_field_hash_table_buckets(mut self, buckets: usize) -> Self {
        assert!(buckets > 0, "Hash table buckets must be positive");
        self.field_hash_table_buckets = buckets;
        self
    }

    pub fn with_keyed_hash(mut self, enabled: bool) -> Self {
        self.enable_keyed_hash = enabled;
        self
    }

    pub fn with_file_id(mut self, file_id: uuid::Uuid) -> Self {
        self.file_id = file_id;
        self
    }

    pub fn with_compression(mut self, compression: Compression) -> Self {
        self.compression = compression;
        self
    }

    pub fn with_compress_threshold(mut self, threshold: usize) -> Self {
        self.compress_threshold = normalize_compress_threshold(threshold);
        self
    }

    pub fn with_compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    #[doc(hidden)]
    pub fn with_experimental_mmap_strategy(mut self, strategy: ExperimentalMmapStrategy) -> Self {
        self.experimental_mmap_strategy = strategy;
        self
    }

    pub fn with_seal(mut self, seal: crate::seal::SealOptions) -> Self {
        self.seal = Some(seal);
        self
    }

    pub fn compression(&self) -> Compression {
        self.compression
    }

    pub fn compress_threshold(&self) -> usize {
        self.compress_threshold
    }

    pub fn compact(&self) -> bool {
        self.compact
    }

    pub fn create<M: MemoryMapMut>(self, file: &crate::repository::File) -> Result<JournalFile<M>> {
        JournalFile::create(file, self)
    }
}

/// Hash table bucket utilization statistics
#[derive(Debug, Clone, Copy)]
pub struct BucketUtilization {
    pub data_occupied: usize,
    pub data_total: usize,
    pub field_occupied: usize,
    pub field_total: usize,
}

impl BucketUtilization {
    pub fn data_utilization(&self) -> f64 {
        if self.data_total == 0 {
            0.0
        } else {
            self.data_occupied as f64 / self.data_total as f64
        }
    }

    pub fn field_utilization(&self) -> f64 {
        if self.field_total == 0 {
            0.0
        } else {
            self.field_occupied as f64 / self.field_total as f64
        }
    }
}

///
/// A reader for systemd journal files that efficiently maps small regions of the file into memory.
///
/// # Memory Management
///
/// This implementation uses a window-based memory mapping strategy similar to systemd's original
/// implementation. Instead of mapping the entire file, it maintains a small set of memory-mapped
/// windows and reuses them as needed.
///
/// # Concurrency and Safety
///
/// `JournalFile` uses interior mutability to provide a safe API with the following characteristics:
///
/// - The window manager is wrapped in a `GuardedCell` which owns both the `WindowManager` and
///   its guard flag, providing interior mutability with integrated guard-based exclusion.
/// - The guard flag ensures only one object can be active at a time.
/// - Methods like `data_object()` return a `ValueGuard<T>` that automatically releases the guard
///   when dropped.
///
/// This design ensures that memory safety is maintained even though references to memory-mapped
/// regions could be invalidated when new objects are created.
pub struct JournalFile<M: MemoryMap> {
    // The validated File this journal represents
    file: crate::repository::File,

    // Persistent memory maps for journal header and data/field hash tables
    header_map: M,
    sanitized_header: Option<JournalHeader>,
    data_hash_table_map: Option<M>,
    field_hash_table_map: Option<M>,

    // Window manager for other objects (owns the guard flag internally)
    window_manager: GuardedCell<WindowManager<M>>,

    // Forward Secure Sealing options (consumed by JournalWriter on first use)
    pub seal_options: Option<crate::seal::SealOptions>,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct DataPayloadReadContext {
    is_compact: bool,
    header_size: u64,
    arena_end: u64,
    payload_prefix_size: u64,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct DataPayloadObjectInfo {
    size_needed: u64,
    is_compressed: bool,
}

#[derive(Debug, Clone, Copy)]
struct DataLookupResult {
    next_hash_offset: Option<NonZeroU64>,
    matches: bool,
}

impl DataPayloadObjectInfo {
    pub fn is_compressed(self) -> bool {
        self.is_compressed
    }
}

fn parse_data_payload_object_header(header_slice: &[u8]) -> Result<DataPayloadObjectInfo> {
    let object_header =
        ObjectHeader::ref_from_bytes(header_slice).map_err(|_| JournalError::ZerocopyFailure)?;

    if object_header.type_ != ObjectType::Data as u8 {
        return Err(JournalError::InvalidObjectType);
    }

    Ok(DataPayloadObjectInfo {
        size_needed: object_header.validated_size()?,
        is_compressed: object_header.is_compressed(),
    })
}

fn map_hash_table<M: MemoryMap>(
    file: &File,
    header_size: u64,
    offset: Option<NonZeroU64>,
    size: Option<NonZeroU64>,
) -> Result<Option<M>> {
    let (Some(offset), Some(size)) = (offset, size) else {
        return Ok(None);
    };

    let object_header_size = std::mem::size_of::<ObjectHeader>() as u64;
    if offset.get() < header_size + object_header_size {
        return Err(JournalError::InvalidObjectLocation);
    }
    if size.get() <= object_header_size {
        return Err(JournalError::InvalidObjectLocation);
    }

    let offset = offset.get() - object_header_size;
    let size = object_header_size + size.get();
    M::create(file, offset, size).map(Some)
}

fn sanitize_header_for_size(mut header: JournalHeader) -> JournalHeader {
    if header.header_size < 216 {
        header.n_data = 0;
    }
    if header.header_size < 224 {
        header.n_fields = 0;
    }
    if header.header_size < 232 {
        header.n_tags = 0;
    }
    if header.header_size < 240 {
        header.n_entry_arrays = 0;
    }
    if header.header_size < 248 {
        header.data_hash_chain_depth = 0;
    }
    if header.header_size < 256 {
        header.field_hash_chain_depth = 0;
    }
    if header.header_size < 260 {
        header.tail_entry_array_offset = 0;
    }
    if header.header_size < 264 {
        header.tail_entry_array_n_entries = 0;
    }
    if header.header_size < 272 {
        header.tail_entry_offset = 0;
    }
    header
}

impl<M: MemoryMap> JournalFile<M> {
    pub fn visit_bucket<'a, H, V>(
        &'a self,
        hash_table: Option<H>,
        hash: u64,
        mut visitor: V,
    ) -> Result<Option<V::Output>>
    where
        H: HashTable<Object = V::Object>,
        V: BucketVisitor<'a>,
    {
        let hash_table = hash_table.ok_or(JournalError::MissingHashTable)?;
        let bucket = hash_table.hash_item_ref(hash);
        let mut object_offset = bucket.head_hash_offset;

        while let Some(offset) = object_offset {
            let object_guard = self.journal_object_ref::<V::Object>(offset)?;

            if let Some(output) = visitor.visit(&object_guard)? {
                return Ok(Some(output));
            }

            object_offset = object_guard.next_hash_offset();
        }

        Ok(None)
    }

    pub fn open(file: &crate::repository::File, window_size: u64) -> Result<Self> {
        Self::open_repository_file(file.clone(), window_size)
    }

    pub fn open_with_strategy(
        file: &crate::repository::File,
        window_size: u64,
        strategy: ExperimentalMmapStrategy,
    ) -> Result<Self> {
        Self::open_repository_file_with_strategy(file.clone(), window_size, strategy)
    }

    pub fn open_path(path: impl AsRef<Path>, window_size: u64) -> Result<Self> {
        Self::open_path_with_strategy(path, window_size, ExperimentalMmapStrategy::Windowed)
    }

    pub fn open_path_with_strategy(
        path: impl AsRef<Path>,
        window_size: u64,
        strategy: ExperimentalMmapStrategy,
    ) -> Result<Self> {
        let path = path.as_ref();
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let file = crate::repository::File::from_raw_path(&absolute_path)
            .ok_or(JournalError::InvalidFilename)?;
        Self::open_repository_file_with_strategy(file, window_size, strategy)
    }

    pub fn open_snapshot(
        file: &crate::repository::File,
        window_size: u64,
        strategy: ExperimentalMmapStrategy,
    ) -> Result<Self> {
        Self::open_repository_file_snapshot(file.clone(), window_size, strategy)
    }

    pub fn open_path_snapshot(
        path: impl AsRef<Path>,
        window_size: u64,
        strategy: ExperimentalMmapStrategy,
    ) -> Result<Self> {
        let path = path.as_ref();
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let file = crate::repository::File::from_raw_path(&absolute_path)
            .ok_or(JournalError::InvalidFilename)?;
        Self::open_repository_file_snapshot(file, window_size, strategy)
    }

    fn open_repository_file(file: crate::repository::File, window_size: u64) -> Result<Self> {
        Self::open_repository_file_with_strategy(
            file,
            window_size,
            ExperimentalMmapStrategy::Windowed,
        )
    }

    fn open_repository_file_with_strategy(
        file: crate::repository::File,
        window_size: u64,
        strategy: ExperimentalMmapStrategy,
    ) -> Result<Self> {
        Self::open_repository_file_with_window_manager(file, window_size, |fd| {
            WindowManager::new_with_strategy(fd, window_size, 16, strategy)
        })
    }

    fn open_repository_file_snapshot(
        file: crate::repository::File,
        window_size: u64,
        strategy: ExperimentalMmapStrategy,
    ) -> Result<Self> {
        Self::open_repository_file_with_window_manager(file, window_size, |fd| {
            WindowManager::new_snapshot(fd, window_size, 16, strategy)
        })
    }

    fn open_repository_file_with_window_manager<F>(
        file: crate::repository::File,
        window_size: u64,
        window_manager_builder: F,
    ) -> Result<Self>
    where
        F: FnOnce(File) -> Result<WindowManager<M>>,
    {
        debug_assert_eq!(window_size % OBJECT_ALIGNMENT, 0);

        // Open file and check its size
        let fd = OpenOptions::new()
            .read(true)
            .write(false)
            .open(file.path())?;

        // Create a memory map for the header
        let header_size = std::mem::size_of::<JournalHeader>() as u64;
        let header_map = M::create(&fd, 0, header_size)?;
        let header = JournalHeader::ref_from_prefix(&header_map).unwrap().0;
        if header.signature != *b"LPKSHHRH" {
            return Err(JournalError::InvalidMagicNumber);
        }
        let sanitized_header =
            (header.header_size < header_size).then(|| sanitize_header_for_size(*header));

        // Initialize the hash table maps if they exist
        let data_hash_table_map = map_hash_table(
            &fd,
            header.header_size,
            header.data_hash_table_offset,
            header.data_hash_table_size,
        )?;
        let field_hash_table_map = map_hash_table(
            &fd,
            header.header_size,
            header.field_hash_table_offset,
            header.field_hash_table_size,
        )?;

        // Create window manager for the rest of the objects
        let window_manager = GuardedCell::new(window_manager_builder(fd)?);

        Ok(JournalFile {
            file,
            header_map,
            sanitized_header,
            data_hash_table_map,
            field_hash_table_map,
            window_manager,
            seal_options: None,
        })
    }

    pub fn file(&self) -> &crate::repository::File {
        &self.file
    }

    #[doc(hidden)]
    pub fn mmap_stats(&self) -> Result<WindowManagerStats> {
        Ok(self.window_manager.borrow_mut_checked()?.stats())
    }

    pub fn hash(&self, data: &[u8]) -> u64 {
        self.hash_parts(PayloadParts::raw(data))
    }

    pub fn hash_parts(&self, payload: PayloadParts<'_>) -> u64 {
        let is_keyed_hash = self
            .journal_header_ref()
            .has_incompatible_flag(HeaderIncompatibleFlags::KeyedHash);

        hash::journal_hash_data_parts(
            payload.iter(),
            is_keyed_hash,
            if is_keyed_hash {
                Some(&self.journal_header_ref().file_id)
            } else {
                None
            },
        )
    }

    pub fn entry_list(&self) -> Option<offset_array::List> {
        let header = self.journal_header_ref();

        header.entry_array_offset.and_then(|head_offset| {
            std::num::NonZeroUsize::new(header.n_entries as usize)
                .map(|total_items| offset_array::List::new(head_offset, total_items))
        })
    }

    pub fn entry_offsets(&self, offsets: &mut Vec<NonZeroU64>) -> Result<()> {
        if let Some(entry_list) = self.entry_list() {
            entry_list.collect_offsets(self, offsets)?;
        }

        Ok(())
    }

    // Returns the data object offsets of the entry object at the specified
    // offset
    pub fn entry_data_object_offsets(
        &self,
        entry_offset: NonZeroU64,
        offsets: &mut Vec<NonZeroU64>,
    ) -> Result<()> {
        let entry_guard = self.entry_ref(entry_offset)?;
        entry_guard.collect_offsets(offsets)
    }

    pub fn journal_header_ref(&self) -> &JournalHeader {
        if let Some(header) = &self.sanitized_header {
            header
        } else {
            JournalHeader::ref_from_prefix(&self.header_map).unwrap().0
        }
    }

    pub fn data_hash_table_map(&self) -> Option<&M> {
        self.data_hash_table_map.as_ref()
    }
    pub fn field_hash_table_map(&self) -> Option<&M> {
        self.field_hash_table_map.as_ref()
    }

    pub fn data_hash_table_ref(&self) -> Option<DataHashTable<&[u8]>> {
        self.data_hash_table_map
            .as_ref()
            .and_then(|m| DataHashTable::<&[u8]>::from_data(m, false))
    }

    pub fn field_hash_table_ref(&self) -> Option<FieldHashTable<&[u8]>> {
        self.field_hash_table_map
            .as_ref()
            .and_then(|m| FieldHashTable::<&[u8]>::from_data(m, false))
    }

    pub fn object_header_ref(&self, position: NonZeroU64) -> Result<&ObjectHeader> {
        validate_offset_alignment(position)?;
        let size_needed = std::mem::size_of::<ObjectHeader>() as u64;
        let window_manager = self.window_manager.borrow_mut_checked()?;
        let header_slice = window_manager.get_slice(position.get(), size_needed)?;
        ObjectHeader::ref_from_bytes(header_slice).map_err(|_| JournalError::ZerocopyFailure)
    }

    /// Reads raw bytes from the file at the given offset.
    /// Returns a copied Vec so no borrow on the window manager is held.
    pub fn read_bytes_at(&self, offset: u64, size: u64) -> Result<Vec<u8>> {
        validate_offset_alignment(NonZeroU64::new(offset).ok_or(JournalError::InvalidOffset)?)?;
        let window_manager = self.window_manager.borrow_mut_checked()?;
        let src = window_manager.get_slice(offset, size)?;
        Ok(src.to_vec())
    }

    fn journal_object_ref<'a, T>(&'a self, offset: NonZeroU64) -> Result<ValueGuard<'a, T>>
    where
        T: JournalObject<&'a [u8]>,
    {
        let journal_header = self.journal_header_ref();
        let is_compact = journal_header.has_incompatible_flag(HeaderIncompatibleFlags::Compact);
        let header_size = journal_header.header_size;
        let arena_end = header_size + journal_header.arena_size;

        validate_offset_alignment(offset)?;

        // Objects cannot be located in the file header
        if offset.get() < header_size {
            return Err(JournalError::ObjectExceedsFileBounds);
        }

        self.window_manager.with_guarded(offset, |wm| {
            // Get the object header to determine size
            let size_needed = {
                let header_slice =
                    wm.get_slice(offset.get(), std::mem::size_of::<ObjectHeader>() as u64)?;
                let header = ObjectHeader::ref_from_bytes(header_slice)
                    .map_err(|_| JournalError::ZerocopyFailure)?;
                header.validated_size()?
            };

            // Validate that the object doesn't exceed the journal's arena bounds
            let end_offset = offset
                .get()
                .checked_add(size_needed)
                .ok_or(JournalError::ObjectExceedsFileBounds)?;
            if end_offset > arena_end {
                return Err(JournalError::ObjectExceedsFileBounds);
            }

            // Get the full object data
            let data = wm.get_slice(offset.get(), size_needed)?;

            // Parse the object
            let value = T::from_data(data, is_compact).ok_or(JournalError::ZerocopyFailure)?;

            Ok(value)
        })
    }

    pub fn offset_array_ref(
        &self,
        offset: NonZeroU64,
    ) -> Result<ValueGuard<'_, OffsetArrayObject<&[u8]>>> {
        self.journal_object_ref(offset)
    }

    pub fn field_ref(&self, offset: NonZeroU64) -> Result<ValueGuard<'_, FieldObject<&[u8]>>> {
        self.journal_object_ref(offset)
    }

    pub fn entry_ref(&self, offset: NonZeroU64) -> Result<ValueGuard<'_, EntryObject<&[u8]>>> {
        self.journal_object_ref(offset)
    }

    pub fn data_ref(&self, offset: NonZeroU64) -> Result<ValueGuard<'_, DataObject<&[u8]>>> {
        self.journal_object_ref(offset)
    }

    #[doc(hidden)]
    pub fn data_payload_read_context(&self) -> DataPayloadReadContext {
        let journal_header = self.journal_header_ref();
        let is_compact = journal_header.has_incompatible_flag(HeaderIncompatibleFlags::Compact);
        let payload_prefix_size = std::mem::size_of::<DataObjectHeader>() as u64
            + if is_compact {
                std::mem::size_of::<CompactDataFields>() as u64
            } else {
                0
            };
        DataPayloadReadContext {
            is_compact,
            header_size: journal_header.header_size,
            arena_end: journal_header.header_size + journal_header.arena_size,
            payload_prefix_size,
        }
    }

    #[doc(hidden)]
    pub fn visit_data_payload_at<F>(
        &self,
        offset: NonZeroU64,
        decompressed: &mut Vec<u8>,
        visitor: F,
    ) -> Result<()>
    where
        F: FnOnce(&[u8]) -> Result<()>,
    {
        let context = self.data_payload_read_context();
        self.visit_data_payload_at_with_context(context, offset, decompressed, visitor)
    }

    #[doc(hidden)]
    pub fn visit_data_payload_at_with_context<F>(
        &self,
        context: DataPayloadReadContext,
        offset: NonZeroU64,
        decompressed: &mut Vec<u8>,
        visitor: F,
    ) -> Result<()>
    where
        F: FnOnce(&[u8]) -> Result<()>,
    {
        validate_offset_alignment(offset)?;
        if offset.get() < context.header_size {
            return Err(JournalError::ObjectExceedsFileBounds);
        }

        self.window_manager.with_mut(|wm| {
            let object_header_size = std::mem::size_of::<ObjectHeader>() as u64;
            let header_slice = wm.get_slice(offset.get(), object_header_size)?;
            let info = parse_data_payload_object_header(header_slice)?;
            let size_needed = info.size_needed;

            let end_offset = offset
                .get()
                .checked_add(size_needed)
                .ok_or(JournalError::ObjectExceedsFileBounds)?;
            if end_offset > context.arena_end {
                return Err(JournalError::ObjectExceedsFileBounds);
            }

            if size_needed < context.payload_prefix_size {
                return Err(JournalError::InvalidObjectSize(size_needed));
            }

            let data = if let Some(data) = wm.active_slice_if_contains(offset.get(), size_needed) {
                data
            } else {
                wm.get_slice(offset.get(), size_needed)?
            };
            if !info.is_compressed {
                return visitor(&data[context.payload_prefix_size as usize..]);
            }

            let object = DataObject::from_data(data, context.is_compact)
                .ok_or(JournalError::ZerocopyFailure)?;
            decompressed.clear();
            let len = object.decompress(decompressed)?;
            visitor(&decompressed[..len])
        })
    }

    #[doc(hidden)]
    pub fn data_payload_object_info_at(
        &self,
        context: DataPayloadReadContext,
        offset: NonZeroU64,
    ) -> Result<DataPayloadObjectInfo> {
        validate_offset_alignment(offset)?;
        if offset.get() < context.header_size {
            return Err(JournalError::ObjectExceedsFileBounds);
        }

        self.window_manager.with_mut(|wm| {
            let object_header_size = std::mem::size_of::<ObjectHeader>() as u64;
            let header_slice = wm.get_slice(offset.get(), object_header_size)?;
            let info = parse_data_payload_object_header(header_slice)?;
            let size_needed = info.size_needed;

            let end_offset = offset
                .get()
                .checked_add(size_needed)
                .ok_or(JournalError::ObjectExceedsFileBounds)?;
            if end_offset > context.arena_end {
                return Err(JournalError::ObjectExceedsFileBounds);
            }
            if size_needed < context.payload_prefix_size {
                return Err(JournalError::InvalidObjectSize(size_needed));
            }

            Ok(info)
        })
    }

    #[doc(hidden)]
    pub fn raw_data_payload_ref_with_info(
        &self,
        context: DataPayloadReadContext,
        offset: NonZeroU64,
        info: DataPayloadObjectInfo,
    ) -> Result<ValueGuard<'_, &[u8]>> {
        validate_offset_alignment(offset)?;
        if offset.get() < context.header_size {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        if info.is_compressed {
            return Err(JournalError::InvalidObjectType);
        }
        if info.size_needed < context.payload_prefix_size {
            return Err(JournalError::InvalidObjectSize(info.size_needed));
        }

        self.window_manager.with_guarded(offset, |wm| {
            if wm.active_window_contains(offset.get(), info.size_needed) {
                let data = wm.active_slice(offset.get(), info.size_needed);
                return Ok(&data[context.payload_prefix_size as usize..]);
            }
            let data = wm.get_slice(offset.get(), info.size_needed)?;
            Ok(&data[context.payload_prefix_size as usize..])
        })
    }

    #[doc(hidden)]
    /// Returns an unguarded pointer to an uncompressed DATA payload.
    ///
    /// The caller must only expose the pointer while it can prove the backing
    /// mmap window will not be remapped or evicted. This is intended for
    /// whole-file mmap row-scoped facade enumeration. Do not call this for
    /// windowed mmap; use `raw_data_payload_ref_with_info()` or copy the
    /// payload instead.
    pub fn raw_data_payload_ptr_with_info_unguarded(
        &self,
        context: DataPayloadReadContext,
        offset: NonZeroU64,
        info: DataPayloadObjectInfo,
    ) -> Result<(*const u8, usize)> {
        validate_offset_alignment(offset)?;
        if offset.get() < context.header_size {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        if info.is_compressed {
            return Err(JournalError::InvalidObjectType);
        }
        if info.size_needed < context.payload_prefix_size {
            return Err(JournalError::InvalidObjectSize(info.size_needed));
        }

        self.window_manager.with_mut(|wm| {
            let data =
                if let Some(data) = wm.active_slice_if_contains(offset.get(), info.size_needed) {
                    data
                } else {
                    wm.get_slice(offset.get(), info.size_needed)?
                };
            let payload = &data[context.payload_prefix_size as usize..];
            Ok((payload.as_ptr(), payload.len()))
        })
    }

    pub fn tag_ref(&self, offset: NonZeroU64) -> Result<ValueGuard<'_, TagObject<&[u8]>>> {
        self.journal_object_ref(offset)
    }

    pub fn find_data_offset(&self, hash: u64, payload: &[u8]) -> Result<Option<NonZeroU64>> {
        self.find_data_offset_parts(hash, PayloadParts::raw(payload))
    }

    pub fn find_data_offset_parts(
        &self,
        hash: u64,
        payload: PayloadParts<'_>,
    ) -> Result<Option<NonZeroU64>> {
        let hash_table = self
            .data_hash_table_ref()
            .ok_or(JournalError::MissingHashTable)?;
        let context = self.data_payload_read_context();
        let mut decompression_buffer = Vec::new();
        let mut object_offset = hash_table.hash_item_ref(hash).head_hash_offset;

        while let Some(offset) = object_offset {
            let result = self.data_lookup_result_at(
                context,
                offset,
                hash,
                payload,
                &mut decompression_buffer,
            )?;
            if result.matches {
                return Ok(Some(offset));
            }
            object_offset = result.next_hash_offset;
        }

        Ok(None)
    }

    fn data_lookup_result_at(
        &self,
        context: DataPayloadReadContext,
        offset: NonZeroU64,
        hash: u64,
        payload: PayloadParts<'_>,
        decompression_buffer: &mut Vec<u8>,
    ) -> Result<DataLookupResult> {
        validate_offset_alignment(offset)?;
        if offset.get() < context.header_size {
            return Err(JournalError::ObjectExceedsFileBounds);
        }

        self.window_manager.with_mut(|wm| {
            let header_slice =
                wm.get_slice(offset.get(), std::mem::size_of::<DataObjectHeader>() as u64)?;
            if header_slice[0] != ObjectType::Data as u8 {
                return Err(JournalError::InvalidObjectType);
            }

            let flags = header_slice[1];
            let size_needed = u64::from_le_bytes(header_slice[8..16].try_into().unwrap());
            if size_needed < std::mem::size_of::<DataObjectHeader>() as u64 {
                return Err(JournalError::InvalidObjectSize(size_needed));
            }
            if size_needed < context.payload_prefix_size {
                return Err(JournalError::InvalidObjectSize(size_needed));
            }

            let end_offset = offset
                .get()
                .checked_add(size_needed)
                .ok_or(JournalError::ObjectExceedsFileBounds)?;
            if end_offset > context.arena_end {
                return Err(JournalError::ObjectExceedsFileBounds);
            }

            let stored_hash = u64::from_le_bytes(header_slice[16..24].try_into().unwrap());
            let next_hash_offset =
                NonZeroU64::new(u64::from_le_bytes(header_slice[24..32].try_into().unwrap()));
            if stored_hash != hash {
                return Ok(DataLookupResult {
                    next_hash_offset,
                    matches: false,
                });
            }

            let data = if let Some(data) = wm.active_slice_if_contains(offset.get(), size_needed) {
                data
            } else {
                wm.get_slice(offset.get(), size_needed)?
            };
            let payload_start = context.payload_prefix_size as usize;
            let is_compressed = (flags
                & (ObjectFlags::CompressedZstd as u8
                    | ObjectFlags::CompressedLz4 as u8
                    | ObjectFlags::CompressedXz as u8))
                != 0;
            let matches = if is_compressed {
                let object = DataObject::from_data(data, context.is_compact)
                    .ok_or(JournalError::ZerocopyFailure)?;
                decompression_buffer.clear();
                let len = object.decompress(decompression_buffer)?;
                payload.equals_slice(&decompression_buffer[..len])
            } else {
                payload.equals_slice(&data[payload_start..])
            };

            Ok(DataLookupResult {
                next_hash_offset,
                matches,
            })
        })
    }

    pub fn find_field_offset(&self, hash: u64, payload: &[u8]) -> Result<Option<NonZeroU64>> {
        let visitor = PayloadMatcher::field_matcher(payload, hash);
        self.visit_bucket(self.field_hash_table_ref(), hash, visitor)
    }

    /// Run a directed partition point query on a data object's entry array
    ///
    /// This finds the first/last entry (depending on direction) that satisfies the given predicate
    /// in the entry array chain of the data object.
    pub fn data_object_directed_partition_point<F>(
        &self,
        data_offset: NonZeroU64,
        predicate: F,
        direction: offset_array::Direction,
    ) -> Result<Option<NonZeroU64>>
    where
        F: Fn(NonZeroU64) -> Result<bool>,
    {
        let Some(cursor) = self.data_ref(data_offset)?.inlined_cursor() else {
            return Ok(None);
        };

        let Some(best_match) = cursor.directed_partition_point(self, predicate, direction)? else {
            return Ok(None);
        };

        best_match.value(self)
    }

    /// Creates an iterator over all field objects in the field hash table
    pub fn fields(&self) -> FieldIterator<'_, M> {
        // Get the field hash table
        let field_hash_table = self.field_hash_table_ref();

        // Initialize with the first bucket
        let mut iterator = FieldIterator {
            journal: self,
            field_hash_table,
            current_bucket_index: 0,
            next_field_offset: None,
        };

        // Find the first non-empty bucket
        iterator.advance_to_next_nonempty_bucket();

        iterator
    }

    /// Creates an iterator over all DATA objects for the specified field
    pub fn field_data_objects<'a>(
        &'a self,
        field_name: &'a [u8],
    ) -> Result<FieldDataIterator<'a, M>> {
        // Find the field offset by name
        let field_hash = self.hash(field_name);
        let Some(field_offset) = self.find_field_offset(field_hash, field_name)? else {
            return Ok(FieldDataIterator {
                journal: self,
                current_data_offset: None,
            });
        };

        // Get the field object to access its head_data_offset
        let field_guard = self.field_ref(field_offset)?;
        let head_data_offset = field_guard.header.head_data_offset;

        // Create the iterator
        Ok(FieldDataIterator {
            journal: self,
            current_data_offset: head_data_offset,
        })
    }

    /// Creates an iterator over all DATA objects for a specific entry
    pub fn entry_data_objects(&self, entry_offset: NonZeroU64) -> Result<EntryDataIterator<'_, M>> {
        // Get the entry object to determine how many data items it has
        let entry_guard = self.entry_ref(entry_offset)?;

        // Get the total number of items
        let total_items = match &entry_guard.items {
            EntryItemsType::Regular(items) => items.len(),
            EntryItemsType::Compact(items) => items.len(),
        };

        // Create the iterator
        Ok(EntryDataIterator {
            journal: self,
            entry_offset: Some(entry_offset),
            current_index: 0,
            total_items,
        })
    }

    /// Get hash table bucket utilization statistics
    pub fn bucket_utilization(&self) -> Option<BucketUtilization> {
        let data_hash_table = self.data_hash_table_ref()?;
        let data_total = data_hash_table.items.len();
        let data_occupied = data_hash_table
            .items
            .iter()
            .filter(|item| item.head_hash_offset.is_some())
            .count();

        let field_hash_table = self.field_hash_table_ref()?;
        let field_total = field_hash_table.items.len();
        let field_occupied = field_hash_table
            .items
            .iter()
            .filter(|item| item.head_hash_offset.is_some())
            .count();

        Some(BucketUtilization {
            data_occupied,
            data_total,
            field_occupied,
            field_total,
        })
    }

    /// Get the duration covered by all entries in the journal
    /// Returns None if the journal is empty or contains only one entry
    pub fn duration(&self) -> Option<Duration> {
        let header = self.journal_header_ref();

        if header.head_entry_realtime == 0 || header.tail_entry_realtime == 0 {
            return None;
        }

        if header.tail_entry_realtime <= header.head_entry_realtime {
            // Single entry or invalid state
            return None;
        }

        let duration_micros = header.tail_entry_realtime - header.head_entry_realtime;
        Some(Duration::from_micros(duration_micros))
    }
}

impl JournalFile<super::mmap::MmapMut> {
    pub fn open_for_append(file: &crate::repository::File, window_size: u64) -> Result<Self> {
        debug_assert_eq!(window_size % OBJECT_ALIGNMENT, 0);

        let fd = OpenOptions::new()
            .read(true)
            .write(true)
            .open(file.path())?;

        let header_size = std::mem::size_of::<JournalHeader>() as u64;
        let header_map = super::mmap::MmapMut::create(&fd, 0, header_size)?;
        let header = JournalHeader::ref_from_prefix(&header_map).unwrap().0;
        if header.signature != *b"LPKSHHRH" {
            return Err(JournalError::InvalidMagicNumber);
        }
        let sanitized_header =
            (header.header_size < header_size).then(|| sanitize_header_for_size(*header));
        if !header.has_incompatible_flag(HeaderIncompatibleFlags::KeyedHash) {
            return Err(JournalError::UnsupportedJournalFile);
        }

        let data_hash_table_map = map_hash_table(
            &fd,
            header.header_size,
            header.data_hash_table_offset,
            header.data_hash_table_size,
        )?;
        let field_hash_table_map = map_hash_table(
            &fd,
            header.header_size,
            header.field_hash_table_offset,
            header.field_hash_table_size,
        )?;

        let window_manager =
            GuardedCell::new(WindowManager::new_writer_owned(fd, window_size, 32)?);

        Ok(JournalFile {
            file: file.clone(),
            header_map,
            sanitized_header,
            data_hash_table_map,
            field_hash_table_map,
            window_manager,
            seal_options: None,
        })
    }
}

impl<M: MemoryMapMut> JournalFile<M> {
    /// Syncs all file data to disk, ensuring all changes are persisted
    ///
    /// This performs a two-step sync process:
    /// 1. Flushes memory-mapped regions to the file page cache (msync)
    /// 2. Syncs the file page cache to physical disk (fdatasync)
    pub fn sync(&mut self) -> Result<()> {
        // Flush memory-mapped header to file page cache
        self.header_map.flush()?;

        // Sync file page cache to disk
        let (logical_size, header_size) = {
            let header = self.journal_header_ref();
            (header.header_size + header.arena_size, header.header_size)
        };
        let header_bytes = self.header_map[..header_size as usize].to_vec();
        let window_manager = self.window_manager.get_mut();
        window_manager.sync(logical_size, &header_bytes)?;

        Ok(())
    }

    /// Trigger a stock-reader-visible post-change notification after mmap append.
    pub fn post_change(&mut self) -> Result<()> {
        let logical_size = {
            let header = self.journal_header_ref();
            header.header_size + header.arena_size
        };
        self.window_manager.get_mut().post_change(logical_size)
    }

    /// Creates a successor journal file with optimized bucket sizes based on this file's utilization
    pub fn create_successor(
        &self,
        file: &crate::repository::File,
        max_file_size: Option<u64>,
    ) -> Result<Self> {
        let header = self.journal_header_ref();
        let bucket_utilization = self.bucket_utilization();

        let options = JournalFileOptions::new(
            uuid::Uuid::from_bytes(header.machine_id),
            uuid::Uuid::from_bytes(header.tail_entry_boot_id),
            uuid::Uuid::from_bytes(header.seqnum_id),
        )
        .with_window_size(8 * 1024 * 1024)
        .with_keyed_hash(header.has_incompatible_flag(HeaderIncompatibleFlags::KeyedHash))
        .with_compact(header.has_incompatible_flag(HeaderIncompatibleFlags::Compact))
        .with_optimized_buckets(bucket_utilization, max_file_size);

        let options = if header.has_incompatible_flag(HeaderIncompatibleFlags::CompressedZstd) {
            options.with_compression(Compression::Zstd)
        } else if header.has_incompatible_flag(HeaderIncompatibleFlags::CompressedXz) {
            options.with_compression(Compression::Xz)
        } else if header.has_incompatible_flag(HeaderIncompatibleFlags::CompressedLz4) {
            options.with_compression(Compression::Lz4)
        } else {
            options
        };

        Self::create(file, options)
    }

    pub fn create(file: &crate::repository::File, options: JournalFileOptions) -> Result<Self> {
        let mut open_options = OpenOptions::new();
        open_options
            .create(true)
            .truncate(true)
            .read(true)
            .write(true);
        #[cfg(unix)]
        open_options.mode(0o640);
        let fd = open_options.open(file.path())?;

        // Calculate hash table sizes
        let data_hash_table_size =
            options.data_hash_table_buckets * std::mem::size_of::<HashItem>();
        let field_hash_table_size =
            options.field_hash_table_buckets * std::mem::size_of::<HashItem>();

        // Calculate hash table offsets
        // systemd creates FIELD_HASH_TABLE first, then DATA_HASH_TABLE
        let field_hash_table_offset = std::mem::size_of::<JournalHeader>() as u64
            + std::mem::size_of::<ObjectHeader>() as u64;
        let data_hash_table_offset = field_hash_table_offset
            + field_hash_table_size as u64
            + std::mem::size_of::<ObjectHeader>() as u64;

        // Create header with options configuration
        let mut header = JournalHeader::default();
        header.signature = *b"LPKSHHRH";

        // Set flags based on options configuration
        // HEADER_COMPATIBLE_TAIL_ENTRY_BOOT_ID is set for new files (v260+)
        header.compatible_flags = HeaderCompatibleFlags::TailEntryBootId as u32;
        if options.enable_keyed_hash {
            header.incompatible_flags |= HeaderIncompatibleFlags::KeyedHash as u32;
        }
        header.incompatible_flags |= options.compression.as_incompatible_flag();
        if options.compact {
            header.incompatible_flags |= HeaderIncompatibleFlags::Compact as u32;
        }
        if options.seal.is_some() {
            header.compatible_flags |= HeaderCompatibleFlags::Sealed as u32;
            header.compatible_flags |= HeaderCompatibleFlags::SealedContinuous as u32;
        }

        // Set hash table configuration
        header.data_hash_table_offset = NonZeroU64::new(data_hash_table_offset);
        header.data_hash_table_size = NonZeroU64::new(data_hash_table_size as u64);
        header.field_hash_table_offset = NonZeroU64::new(field_hash_table_offset);
        header.field_hash_table_size = NonZeroU64::new(field_hash_table_size as u64);

        // Set other header fields. tail_object_offset points to the last
        // object written (data hash table).
        let data_hash_table_object_offset =
            data_hash_table_offset - std::mem::size_of::<ObjectHeader>() as u64;
        let append_offset = data_hash_table_offset + data_hash_table_size as u64;
        header.tail_object_offset = NonZeroU64::new(data_hash_table_object_offset);
        header.header_size = std::mem::size_of::<JournalHeader>() as u64;
        header.n_objects = 2;
        let file_size = round_up_to_file_size_increment(append_offset)?;
        if options.compact && file_size > JOURNAL_COMPACT_SIZE_MAX {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        fd.set_len(file_size)?;
        header.arena_size = file_size - header.header_size;

        // Set IDs from options
        header.machine_id = *options.machine_id.as_bytes();
        header.file_id = *options.file_id.as_bytes();
        header.seqnum_id = *options.seqnum_id.as_bytes();

        // Create memory maps for hash tables
        let data_hash_table_map = map_hash_table(
            &fd,
            header.header_size,
            header.data_hash_table_offset,
            header.data_hash_table_size,
        )?;
        let field_hash_table_map = map_hash_table(
            &fd,
            header.header_size,
            header.field_hash_table_offset,
            header.field_hash_table_size,
        )?;

        // Create header memory map and write header
        let header_size = std::mem::size_of::<JournalHeader>() as u64;
        let mut header_map = M::create(&fd, 0, header_size)?;
        {
            let header_mut = JournalHeader::mut_from_prefix(&mut header_map).unwrap().0;
            *header_mut = header;
            // Set state to ONLINE as per journal file format spec
            header_mut.state = JournalState::Online as u8;
        }

        // Create window manager for the rest of the objects
        let window_manager = GuardedCell::new(WindowManager::new_writer_owned_with_strategy(
            fd,
            options.window_size,
            32,
            options.experimental_mmap_strategy,
        )?);

        let mut jf = JournalFile {
            file: file.clone(),
            header_map,
            sanitized_header: None,
            data_hash_table_map,
            field_hash_table_map,
            window_manager,
            seal_options: options.seal.clone(),
        };

        // write data hash table object header info
        {
            let offset = NonZeroU64::new(
                header.data_hash_table_offset.unwrap().get()
                    - std::mem::size_of::<ObjectHeader>() as u64,
            )
            .unwrap();
            let size = header.data_hash_table_size.unwrap().get()
                + std::mem::size_of::<ObjectHeader>() as u64;

            let object_header = jf.object_header_mut(offset)?;
            object_header.type_ = ObjectType::DataHashTable as u8;
            object_header.size = size
        }

        // write field hash table object header info
        {
            let offset = NonZeroU64::new(
                header.field_hash_table_offset.unwrap().get()
                    - std::mem::size_of::<ObjectHeader>() as u64,
            )
            .unwrap();
            let size = header.field_hash_table_size.unwrap().get()
                + std::mem::size_of::<ObjectHeader>() as u64;

            let object_header = jf.object_header_mut(offset)?;
            object_header.type_ = ObjectType::FieldHashTable as u8;
            object_header.size = size
        }

        // Sync to ensure the ONLINE state is persisted to disk
        jf.sync()?;

        Ok(jf)
    }

    pub fn journal_header_mut(&mut self) -> &mut JournalHeader {
        JournalHeader::mut_from_prefix(&mut self.header_map)
            .unwrap()
            .0
    }

    pub fn data_hash_table_mut(&mut self) -> Option<DataHashTable<&mut [u8]>> {
        self.data_hash_table_map
            .as_mut()
            .and_then(|m| DataHashTable::<&mut [u8]>::from_data_mut(m, false))
    }

    pub fn field_hash_table_mut(&mut self) -> Option<FieldHashTable<&mut [u8]>> {
        self.field_hash_table_map
            .as_mut()
            .and_then(|m| FieldHashTable::<&mut [u8]>::from_data_mut(m, false))
    }

    #[allow(clippy::mut_from_ref)]
    fn object_header_mut(&self, offset: NonZeroU64) -> Result<&mut ObjectHeader> {
        validate_offset_alignment(offset)?;
        let size_needed = std::mem::size_of::<ObjectHeader>() as u64;
        let window_manager = self.window_manager.borrow_mut_checked()?;
        let header_slice = window_manager.get_slice_mut(offset.get(), size_needed)?;
        ObjectHeader::mut_from_bytes(header_slice).map_err(|_| JournalError::ZerocopyFailure)
    }

    fn journal_object_mut<'a, T>(
        &'a self,
        type_: ObjectType,
        offset: NonZeroU64,
        size: Option<u64>,
    ) -> Result<ValueGuard<'a, T>>
    where
        T: JournalObjectMut<&'a mut [u8]>,
    {
        validate_offset_alignment(offset)?;

        let journal_header = self.journal_header_ref();
        let is_compact = journal_header.has_incompatible_flag(HeaderIncompatibleFlags::Compact);
        let header_size = journal_header.header_size;
        let arena_end = header_size + journal_header.arena_size;

        // Objects cannot be located in the file header
        if offset.get() < header_size {
            return Err(JournalError::ObjectExceedsFileBounds);
        }

        self.window_manager.with_guarded(offset, |wm| {
            // Get or set the size
            let size_needed = match size {
                Some(size) => {
                    // Setting object header for a new object (no bounds check needed,
                    // the file will be extended as necessary)
                    let header_slice =
                        wm.get_slice_mut(offset.get(), std::mem::size_of::<ObjectHeader>() as u64)?;
                    let header = ObjectHeader::mut_from_bytes(header_slice)
                        .map_err(|_| JournalError::ZerocopyFailure)?;
                    header.type_ = type_ as u8;
                    header.size = size;
                    size
                }
                None => {
                    // Reading existing object header
                    let header_slice =
                        wm.get_slice(offset.get(), std::mem::size_of::<ObjectHeader>() as u64)?;
                    let header = ObjectHeader::ref_from_bytes(header_slice)
                        .map_err(|_| JournalError::ZerocopyFailure)?;
                    if header.type_ != type_ as u8 {
                        return Err(JournalError::InvalidObjectType);
                    }
                    let size_needed = header.validated_size()?;

                    // Validate that the object doesn't exceed the journal's arena bounds
                    let end_offset = offset
                        .get()
                        .checked_add(size_needed)
                        .ok_or(JournalError::ObjectExceedsFileBounds)?;
                    if end_offset > arena_end {
                        return Err(JournalError::ObjectExceedsFileBounds);
                    }

                    size_needed
                }
            };

            // Get mutable object data
            let data = wm.get_slice_mut(offset.get(), size_needed)?;

            // Parse the mutable object
            let value = T::from_data_mut(data, is_compact).ok_or(JournalError::ZerocopyFailure)?;

            Ok(value)
        })
    }

    pub fn offset_array_mut(
        &self,
        offset: NonZeroU64,
        capacity: Option<NonZeroU64>,
    ) -> Result<ValueGuard<'_, OffsetArrayObject<&mut [u8]>>> {
        let size = capacity.map(|c| {
            let mut size = std::mem::size_of::<OffsetArrayObjectHeader>() as u64;

            let is_compact = self
                .journal_header_ref()
                .has_incompatible_flag(HeaderIncompatibleFlags::Compact);
            if is_compact {
                size += c.get() * std::mem::size_of::<u32>() as u64;
            } else {
                size += c.get() * std::mem::size_of::<u64>() as u64;
            }

            size
        });

        self.journal_object_mut(ObjectType::EntryArray, offset, size)
    }

    pub fn field_mut(
        &self,
        offset: NonZeroU64,
        size: Option<u64>,
    ) -> Result<ValueGuard<'_, FieldObject<&mut [u8]>>> {
        let size = size.map(|n| std::mem::size_of::<FieldObjectHeader>() as u64 + n);
        self.journal_object_mut(ObjectType::Field, offset, size)
    }

    pub fn entry_mut(
        &self,
        offset: NonZeroU64,
        size: Option<u64>,
    ) -> Result<ValueGuard<'_, EntryObject<&mut [u8]>>> {
        let size = size.map(|n| std::mem::size_of::<EntryObjectHeader>() as u64 + n);
        self.journal_object_mut(ObjectType::Entry, offset, size)
    }

    pub fn data_mut(
        &self,
        offset: NonZeroU64,
        size: Option<u64>,
    ) -> Result<ValueGuard<'_, DataObject<&mut [u8]>>> {
        let size = size.map(|n| {
            let mut size = std::mem::size_of::<DataObjectHeader>() as u64 + n;
            if self
                .journal_header_ref()
                .has_incompatible_flag(HeaderIncompatibleFlags::Compact)
            {
                size += std::mem::size_of::<CompactDataFields>() as u64;
            }
            size
        });
        self.journal_object_mut(ObjectType::Data, offset, size)
    }

    pub fn tag_mut(
        &self,
        offset: NonZeroU64,
        new: bool,
    ) -> Result<ValueGuard<'_, TagObject<&mut [u8]>>> {
        let size = if new {
            Some(std::mem::size_of::<TagObjectHeader>() as u64)
        } else {
            None
        };
        self.journal_object_mut(ObjectType::Tag, offset, size)
    }
}

macro_rules! impl_hash_table_set_tail_offset {
    (
        $method_name:ident,
        $hash_table_ref:ident,
        $hash_table_mut:ident,
        $object_mut:ident
    ) => {
        pub fn $method_name(&mut self, hash: u64, object_offset: NonZeroU64) -> Result<()> {
            let hash_item = {
                let Some(ht) = self.$hash_table_ref() else {
                    return Err(JournalError::MissingHashTable);
                };
                *ht.hash_item_ref(hash)
            };

            if let Some(tail_hash_offset) = hash_item.tail_hash_offset {
                let mut tail_object = self.$object_mut(tail_hash_offset, None)?;
                tail_object.set_next_hash_offset(object_offset);
            }

            let Some(mut ht) = self.$hash_table_mut() else {
                return Err(JournalError::MissingHashTable);
            };

            let hash_item = ht.hash_item_mut(hash);
            if hash_item.head_hash_offset.is_none() {
                hash_item.head_hash_offset = Some(object_offset);
            }
            hash_item.tail_hash_offset = Some(object_offset);

            Ok(())
        }
    };
}

impl<M: MemoryMapMut> JournalFile<M> {
    impl_hash_table_set_tail_offset!(
        data_hash_table_set_tail_offset,
        data_hash_table_ref,
        data_hash_table_mut,
        data_mut
    );

    impl_hash_table_set_tail_offset!(
        field_hash_table_set_tail_offset,
        field_hash_table_ref,
        field_hash_table_mut,
        field_mut
    );
}

/// Iterator that walks through all field objects in the field hash table
pub struct FieldIterator<'a, M: MemoryMap> {
    journal: &'a JournalFile<M>,
    field_hash_table: Option<FieldHashTable<&'a [u8]>>,
    current_bucket_index: usize,
    next_field_offset: Option<NonZeroU64>,
}

impl<M: MemoryMap> FieldIterator<'_, M> {
    /// Advances to the next non-empty bucket
    fn advance_to_next_nonempty_bucket(&mut self) {
        // If we don't have a hash table, there's nothing to iterate
        let Some(hash_table) = &self.field_hash_table else {
            return;
        };

        let items = &hash_table.items;

        // Find the next non-empty bucket
        while self.current_bucket_index < items.len() {
            let bucket = items[self.current_bucket_index];
            if bucket.head_hash_offset.is_some() {
                self.next_field_offset = bucket.head_hash_offset;
                return;
            }
            self.current_bucket_index += 1;
        }

        // No more non-empty buckets
        self.next_field_offset = None;
    }
}

impl<'a, M: MemoryMap> Iterator for FieldIterator<'a, M> {
    type Item = Result<ValueGuard<'a, FieldObject<&'a [u8]>>>;

    fn next(&mut self) -> Option<Self::Item> {
        let offset = self.next_field_offset?;

        match self.journal.field_ref(offset) {
            Ok(field_guard) => {
                // Get the next field offset before we return the guard
                self.next_field_offset = field_guard.header.next_hash_offset;

                // If we've reached the end of the chain, move to the next bucket
                if self.next_field_offset.is_none() {
                    self.current_bucket_index += 1;
                    self.advance_to_next_nonempty_bucket();
                }

                Some(Ok(field_guard))
            }
            Err(e) => {
                self.next_field_offset = None;
                Some(Err(e))
            }
        }
    }
}

/// Iterator that walks through all DATA objects for a specific field
pub struct FieldDataIterator<'a, M: MemoryMap> {
    journal: &'a JournalFile<M>,
    current_data_offset: Option<NonZeroU64>,
}

impl<'a, M: MemoryMap> Iterator for FieldDataIterator<'a, M> {
    type Item = Result<ValueGuard<'a, DataObject<&'a [u8]>>>;

    fn next(&mut self) -> Option<Self::Item> {
        let data_offset = self.current_data_offset?;

        match self.journal.data_ref(data_offset) {
            Ok(data_guard) => {
                // Get the next data offset before we return the guard
                self.current_data_offset = data_guard.header.next_field_offset;
                Some(Ok(data_guard))
            }
            Err(e) => {
                self.current_data_offset = None;
                Some(Err(e))
            }
        }
    }
}

/// Iterator that walks through all DATA objects for a specific entry
pub struct EntryDataIterator<'a, M: MemoryMap> {
    journal: &'a JournalFile<M>,
    entry_offset: Option<NonZeroU64>,
    current_index: usize,
    total_items: usize,
}

impl<'a, M: MemoryMap> Iterator for EntryDataIterator<'a, M> {
    type Item = Result<ValueGuard<'a, DataObject<&'a [u8]>>>;

    fn next(&mut self) -> Option<Self::Item> {
        let entry_offset = self.entry_offset?;

        // If we've reached the end of the data indices, return None
        if self.current_index >= self.total_items {
            return None;
        }

        // Get the entry object to access the data offset
        match self.journal.entry_ref(entry_offset) {
            Ok(entry_guard) => {
                let idx = self.current_index;
                self.current_index += 1;

                let data_offset = match &entry_guard.items {
                    EntryItemsType::Regular(items) => {
                        if idx >= items.len() {
                            return None;
                        }
                        items[idx].object_offset
                    }
                    EntryItemsType::Compact(items) => {
                        if idx >= items.len() {
                            return None;
                        }
                        items[idx].object_offset as u64
                    }
                };

                let data_offset = match NonZeroU64::new(data_offset) {
                    Some(offset) => offset,
                    None => {
                        self.current_index = self.total_items;
                        return Some(Err(JournalError::InvalidOffset));
                    }
                };

                // Drop the entry guard before obtaining the data object
                drop(entry_guard);

                // Try to get the data object
                match self.journal.data_ref(data_offset) {
                    Ok(data_guard) => Some(Ok(data_guard)),
                    Err(e) => {
                        // If we can't read the data, return the error and stop iteration
                        self.current_index = self.total_items;
                        Some(Err(e))
                    }
                }
            }
            Err(e) => {
                // If we can't read the entry, return the error and stop iteration
                self.current_index = self.total_items;
                Some(Err(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::writer::JournalWriter;
    use std::path::PathBuf;
    use std::process::Command;
    use tempfile::TempDir;

    fn test_uuid(seed: u8) -> uuid::Uuid {
        uuid::Uuid::from_bytes([seed; 16])
    }

    #[test]
    fn payload_parts_structured_equals_contiguous_payload() {
        let parts = PayloadParts::structured(b"NAME", b"\x00=VALUE");

        assert!(parts.equals_slice(b"NAME=\x00=VALUE"));
        assert!(!parts.equals_slice(b"NAME=VALUE"));
        assert!(!parts.equals_slice(b"OTHER=\x00=VALUE"));
    }

    #[test]
    fn sanitize_header_for_historical_size_matches_per_field_boundaries() {
        #[derive(Debug)]
        struct Expected {
            header_size: u64,
            n_data: u64,
            n_fields: u64,
            n_tags: u64,
            n_entry_arrays: u64,
            data_hash_chain_depth: u64,
            field_hash_chain_depth: u64,
            tail_entry_array_offset: u32,
            tail_entry_array_n_entries: u32,
            tail_entry_offset: u64,
        }

        let cases = [
            Expected {
                header_size: 208,
                n_data: 0,
                n_fields: 0,
                n_tags: 0,
                n_entry_arrays: 0,
                data_hash_chain_depth: 0,
                field_hash_chain_depth: 0,
                tail_entry_array_offset: 0,
                tail_entry_array_n_entries: 0,
                tail_entry_offset: 0,
            },
            Expected {
                header_size: 216,
                n_data: 11,
                n_fields: 0,
                n_tags: 0,
                n_entry_arrays: 0,
                data_hash_chain_depth: 0,
                field_hash_chain_depth: 0,
                tail_entry_array_offset: 0,
                tail_entry_array_n_entries: 0,
                tail_entry_offset: 0,
            },
            Expected {
                header_size: 220,
                n_data: 11,
                n_fields: 0,
                n_tags: 0,
                n_entry_arrays: 0,
                data_hash_chain_depth: 0,
                field_hash_chain_depth: 0,
                tail_entry_array_offset: 0,
                tail_entry_array_n_entries: 0,
                tail_entry_offset: 0,
            },
            Expected {
                header_size: 224,
                n_data: 11,
                n_fields: 22,
                n_tags: 0,
                n_entry_arrays: 0,
                data_hash_chain_depth: 0,
                field_hash_chain_depth: 0,
                tail_entry_array_offset: 0,
                tail_entry_array_n_entries: 0,
                tail_entry_offset: 0,
            },
            Expected {
                header_size: 232,
                n_data: 11,
                n_fields: 22,
                n_tags: 33,
                n_entry_arrays: 0,
                data_hash_chain_depth: 0,
                field_hash_chain_depth: 0,
                tail_entry_array_offset: 0,
                tail_entry_array_n_entries: 0,
                tail_entry_offset: 0,
            },
            Expected {
                header_size: 240,
                n_data: 11,
                n_fields: 22,
                n_tags: 33,
                n_entry_arrays: 44,
                data_hash_chain_depth: 0,
                field_hash_chain_depth: 0,
                tail_entry_array_offset: 0,
                tail_entry_array_n_entries: 0,
                tail_entry_offset: 0,
            },
            Expected {
                header_size: 248,
                n_data: 11,
                n_fields: 22,
                n_tags: 33,
                n_entry_arrays: 44,
                data_hash_chain_depth: 55,
                field_hash_chain_depth: 0,
                tail_entry_array_offset: 0,
                tail_entry_array_n_entries: 0,
                tail_entry_offset: 0,
            },
            Expected {
                header_size: 250,
                n_data: 11,
                n_fields: 22,
                n_tags: 33,
                n_entry_arrays: 44,
                data_hash_chain_depth: 55,
                field_hash_chain_depth: 0,
                tail_entry_array_offset: 0,
                tail_entry_array_n_entries: 0,
                tail_entry_offset: 0,
            },
            Expected {
                header_size: 256,
                n_data: 11,
                n_fields: 22,
                n_tags: 33,
                n_entry_arrays: 44,
                data_hash_chain_depth: 55,
                field_hash_chain_depth: 66,
                tail_entry_array_offset: 0,
                tail_entry_array_n_entries: 0,
                tail_entry_offset: 0,
            },
            Expected {
                header_size: 260,
                n_data: 11,
                n_fields: 22,
                n_tags: 33,
                n_entry_arrays: 44,
                data_hash_chain_depth: 55,
                field_hash_chain_depth: 66,
                tail_entry_array_offset: 77,
                tail_entry_array_n_entries: 0,
                tail_entry_offset: 0,
            },
            Expected {
                header_size: 264,
                n_data: 11,
                n_fields: 22,
                n_tags: 33,
                n_entry_arrays: 44,
                data_hash_chain_depth: 55,
                field_hash_chain_depth: 66,
                tail_entry_array_offset: 77,
                tail_entry_array_n_entries: 88,
                tail_entry_offset: 0,
            },
            Expected {
                header_size: 268,
                n_data: 11,
                n_fields: 22,
                n_tags: 33,
                n_entry_arrays: 44,
                data_hash_chain_depth: 55,
                field_hash_chain_depth: 66,
                tail_entry_array_offset: 77,
                tail_entry_array_n_entries: 88,
                tail_entry_offset: 0,
            },
            Expected {
                header_size: 272,
                n_data: 11,
                n_fields: 22,
                n_tags: 33,
                n_entry_arrays: 44,
                data_hash_chain_depth: 55,
                field_hash_chain_depth: 66,
                tail_entry_array_offset: 77,
                tail_entry_array_n_entries: 88,
                tail_entry_offset: 99,
            },
            Expected {
                header_size: 300,
                n_data: 11,
                n_fields: 22,
                n_tags: 33,
                n_entry_arrays: 44,
                data_hash_chain_depth: 55,
                field_hash_chain_depth: 66,
                tail_entry_array_offset: 77,
                tail_entry_array_n_entries: 88,
                tail_entry_offset: 99,
            },
        ];

        for expected in cases {
            let sanitized = sanitize_header_for_size(JournalHeader {
                header_size: expected.header_size,
                n_data: 11,
                n_fields: 22,
                n_tags: 33,
                n_entry_arrays: 44,
                data_hash_chain_depth: 55,
                field_hash_chain_depth: 66,
                tail_entry_array_offset: 77,
                tail_entry_array_n_entries: 88,
                tail_entry_offset: 99,
                ..JournalHeader::default()
            });

            assert_eq!(sanitized.n_data, expected.n_data, "{expected:?}");
            assert_eq!(sanitized.n_fields, expected.n_fields, "{expected:?}");
            assert_eq!(sanitized.n_tags, expected.n_tags, "{expected:?}");
            assert_eq!(
                sanitized.n_entry_arrays, expected.n_entry_arrays,
                "{expected:?}"
            );
            assert_eq!(
                sanitized.data_hash_chain_depth, expected.data_hash_chain_depth,
                "{expected:?}"
            );
            assert_eq!(
                sanitized.field_hash_chain_depth, expected.field_hash_chain_depth,
                "{expected:?}"
            );
            assert_eq!(
                sanitized.tail_entry_array_offset, expected.tail_entry_array_offset,
                "{expected:?}"
            );
            assert_eq!(
                sanitized.tail_entry_array_n_entries, expected.tail_entry_array_n_entries,
                "{expected:?}"
            );
            assert_eq!(
                sanitized.tail_entry_offset, expected.tail_entry_offset,
                "{expected:?}"
            );
        }
    }

    #[test]
    fn writer_lock_helper_rejects_second_acquire() {
        use crate::file::lock::WriterLock;

        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");
        let options = JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3));

        let mut writer_lock =
            WriterLock::acquire(path.to_string_lossy().as_ref()).expect("acquire writer lock");
        let _journal_file: JournalFile<crate::file::MmapMut> =
            JournalFile::create(&repo_file, options).expect("create journal");
        match WriterLock::acquire(path.to_string_lossy().as_ref()) {
            Ok(mut lock) => {
                let _ = lock.release();
                panic!("second WriterLock::acquire succeeded while writer lock is held")
            }
            Err(err) => {
                assert_eq!(err.kind(), std::io::ErrorKind::WouldBlock);
            }
        }
        writer_lock.release().expect("release writer lock");
    }

    #[test]
    fn writer_lock_is_disabled_by_default() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");
        let options = JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3));

        let journal_file: JournalFile<crate::file::MmapMut> =
            JournalFile::create(&repo_file, options).expect("create journal");
        drop(journal_file);
        assert!(!PathBuf::from(format!("{}.lock", path.display())).exists());
    }

    #[cfg(unix)]
    #[test]
    fn create_uses_journal_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let _journal_file: JournalFile<crate::file::MmapMut> = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
        )
        .expect("create journal");

        let mode = std::fs::metadata(&path)
            .expect("stat journal")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o640);
    }

    #[test]
    fn open_for_append_rejects_unkeyed_journal_without_mutation() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let mut journal_file: JournalFile<crate::file::MmapMut> = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_keyed_hash(false),
        )
        .expect("create unkeyed journal");
        journal_file.sync().expect("sync unkeyed journal");
        drop(journal_file);

        let before = std::fs::read(&path).expect("read journal before append-open");
        let err =
            match JournalFile::<crate::file::MmapMut>::open_for_append(&repo_file, 8 * 1024 * 1024)
            {
                Ok(_) => panic!("unkeyed journal append-open should be rejected"),
                Err(err) => err,
            };

        assert!(matches!(err, JournalError::UnsupportedJournalFile));
        let after = std::fs::read(&path).expect("read journal after append-open");
        assert_eq!(after, before);
    }

    #[test]
    fn entry_data_iterator_reports_zero_offsets_as_invalid() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
        )
        .expect("create journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        let payloads = [b"MESSAGE=test".as_slice(), b"PRIORITY=6".as_slice()];
        writer
            .add_entry(&mut journal_file, &payloads, 1_000_000, 100)
            .expect("write entry");

        let mut entry_offsets = Vec::new();
        journal_file
            .entry_offsets(&mut entry_offsets)
            .expect("collect entry offsets");
        let entry_offset = *entry_offsets.first().expect("journal entry");

        {
            let mut entry_guard = journal_file
                .entry_mut(entry_offset, None)
                .expect("entry guard");
            match &mut entry_guard.items {
                EntryItemsType::Regular(items) => items[0].object_offset = 0,
                EntryItemsType::Compact(items) => items[0].object_offset = 0,
            }
        }

        let mut iter = journal_file
            .entry_data_objects(entry_offset)
            .expect("entry iterator");
        assert!(matches!(
            iter.next(),
            Some(Err(JournalError::InvalidOffset))
        ));
        assert!(
            iter.next().is_none(),
            "iterator should stop after the error"
        );
    }

    fn first_data_offset(journal_file: &JournalFile<crate::file::MmapMut>) -> NonZeroU64 {
        let mut entry_offsets = Vec::new();
        journal_file
            .entry_offsets(&mut entry_offsets)
            .expect("collect entry offsets");
        let entry_offset = *entry_offsets.first().expect("journal entry");
        let entry = journal_file.entry_ref(entry_offset).expect("entry object");
        match &entry.items {
            EntryItemsType::Regular(items) => {
                NonZeroU64::new(items[0].object_offset).expect("data offset")
            }
            EntryItemsType::Compact(items) => {
                NonZeroU64::new(items[0].object_offset as u64).expect("data offset")
            }
        }
    }

    #[test]
    fn visit_data_payload_at_returns_compact_uncompressed_payload() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_compact(true),
        )
        .expect("create compact journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=compact payload".as_slice()],
                1_000_000,
                100,
            )
            .expect("write compact entry");

        let offset = first_data_offset(&journal_file);
        let mut decompressed = Vec::new();
        let mut observed = Vec::new();
        journal_file
            .visit_data_payload_at(offset, &mut decompressed, |payload| {
                observed.extend_from_slice(payload);
                Ok(())
            })
            .expect("visit payload");

        assert_eq!(observed, b"MESSAGE=compact payload");
    }

    #[test]
    fn visit_data_payload_at_decompresses_payload() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_compression(Compression::Zstd)
                .with_compress_threshold(8),
        )
        .expect("create compressed journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        let payload = format!("MESSAGE={}", "x".repeat(1024));
        writer
            .add_entry(&mut journal_file, &[payload.as_bytes()], 1_000_000, 100)
            .expect("write compressed entry");

        let offset = first_data_offset(&journal_file);
        let mut decompressed = Vec::new();
        let mut observed = Vec::new();
        journal_file
            .visit_data_payload_at(offset, &mut decompressed, |payload| {
                observed.extend_from_slice(payload);
                Ok(())
            })
            .expect("visit payload");

        assert_eq!(observed, payload.as_bytes());
    }

    #[test]
    fn compact_writer_reader_and_stock_journalctl() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_compact(true),
        )
        .expect("create compact journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer
            .add_entry(
                &mut journal_file,
                &[
                    b"MESSAGE=compact entry".as_slice(),
                    b"BINARY=\x00\x01\xfe\xff".as_slice(),
                ],
                1_000_000,
                100,
            )
            .expect("write first compact entry");
        writer
            .add_entry(
                &mut journal_file,
                &[
                    b"MESSAGE=second compact entry".as_slice(),
                    b"PRIORITY=6".as_slice(),
                ],
                1_000_001,
                101,
            )
            .expect("write second compact entry");
        journal_file.sync().expect("sync compact journal");

        assert!(
            journal_file
                .journal_header_ref()
                .has_incompatible_flag(HeaderIncompatibleFlags::Compact)
        );

        let mut entry_offsets = Vec::new();
        journal_file
            .entry_offsets(&mut entry_offsets)
            .expect("collect compact entry offsets");
        assert_eq!(entry_offsets.len(), 2);

        let payloads = journal_file
            .entry_data_objects(entry_offsets[0])
            .expect("compact entry data iterator")
            .map(|item| item.map(|object| object.raw_payload().to_vec()))
            .collect::<Result<Vec<_>>>()
            .expect("read compact data objects");
        assert!(payloads.iter().any(|p| p == b"MESSAGE=compact entry"));
        assert!(payloads.iter().any(|p| p == b"BINARY=\x00\x01\xfe\xff"));

        if !journalctl_available() {
            eprintln!("journalctl not available; skipping stock compact assertions");
            return;
        }

        let output = Command::new("journalctl")
            .arg("--file")
            .arg(&path)
            .arg("--output=json")
            .arg("--no-pager")
            .output()
            .expect("run journalctl compact read");
        assert!(
            output.status.success(),
            "journalctl compact read failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output
                .stdout
                .split(|b| *b == b'\n')
                .filter(|line| !line.is_empty())
                .count(),
            2
        );

        let output = Command::new("journalctl")
            .arg("--verify")
            .arg("--file")
            .arg(&path)
            .arg("--no-pager")
            .output()
            .expect("run journalctl compact verify");
        assert!(
            output.status.success(),
            "journalctl compact verify failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn journalctl_available() -> bool {
        Command::new("journalctl")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    }
}
