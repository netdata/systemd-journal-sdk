#![allow(clippy::field_reassign_with_default)]

pub use super::file_iterators::{
    EntryDataIterator, FieldDataIterator, FieldDataOffsetIterator, FieldIterator,
};
pub use super::file_payload::{DataPayloadObjectInfo, DataPayloadReadContext};
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
use std::path::Path;
use std::time::Duration;
use zerocopy::{ByteSlice, FromBytes};

use crate::file::value_guard::ValueGuard;

// Size to pad objects to (8 bytes)
pub(super) const OBJECT_ALIGNMENT: u64 = 8;
const FILE_SIZE_INCREASE: u64 = 8 * 1024 * 1024;
pub(super) const JOURNAL_COMPACT_SIZE_MAX: u64 = u32::MAX as u64;
const DEFAULT_MAX_FILE_SIZE: u64 = 128 * 1024 * 1024;
const JOURNAL_FILE_SIZE_MIN: u64 = 512 * 1024;
const PAGE_SIZE: u64 = 4096;
const DEFAULT_DATA_HASH_TABLE_SIZE: usize = 2047;
const DEFAULT_FIELD_HASH_TABLE_SIZE: usize = 1023;
pub const DEFAULT_COMPRESS_THRESHOLD: usize = 512;
pub const MIN_COMPRESS_THRESHOLD: usize = 8;
pub const DEFAULT_JOURNAL_FILE_MODE: u32 = 0o640;

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
pub(super) fn validate_offset_alignment(offset: NonZeroU64) -> Result<()> {
    if offset.get() % OBJECT_ALIGNMENT != 0 {
        return Err(JournalError::MisalignedOffset(offset.get()));
    }
    Ok(())
}

pub(super) fn round_up_to_file_size_increment(value: u64) -> Result<u64> {
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
    pub(super) machine_id: uuid::Uuid,
    pub(super) seqnum_id: uuid::Uuid,
    pub(super) file_id: uuid::Uuid,
    pub(super) window_size: u64,
    pub(super) data_hash_table_buckets: usize,
    pub(super) field_hash_table_buckets: usize,
    pub(super) enable_keyed_hash: bool,
    pub(super) compression: Compression,
    pub(super) compress_threshold: usize,
    pub(super) compact: bool,
    pub(super) file_mode: u32,
    pub(super) experimental_mmap_strategy: ExperimentalMmapStrategy,
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
            file_mode: DEFAULT_JOURNAL_FILE_MODE,
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

    pub fn with_file_mode(mut self, mode: u32) -> Self {
        assert!(
            mode <= 0o777,
            "journal file mode must contain only permission bits"
        );
        self.file_mode = mode;
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

    pub fn file_mode(&self) -> u32 {
        self.file_mode
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
    pub(super) file: crate::repository::File,

    // Persistent memory maps for journal header and data/field hash tables
    pub(super) header_map: M,
    pub(super) sanitized_header: Option<JournalHeader>,
    pub(super) data_hash_table_map: Option<M>,
    pub(super) field_hash_table_map: Option<M>,

    // Window manager for other objects (owns the guard flag internally)
    pub(super) window_manager: GuardedCell<WindowManager<M>>,

    // Forward Secure Sealing options (consumed by JournalWriter on first use)
    pub seal_options: Option<crate::seal::SealOptions>,
}

pub(super) fn map_hash_table<M: MemoryMap>(
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

    #[doc(hidden)]
    pub fn reader_file_size(&self) -> Result<u64> {
        Ok(self.window_manager.borrow_mut_checked()?.stats().file_size)
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

    #[doc(hidden)]
    pub fn read_unaligned_bytes_at(&self, offset: u64, size: u64) -> Result<Vec<u8>> {
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

    pub fn tag_ref(&self, offset: NonZeroU64) -> Result<ValueGuard<'_, TagObject<&[u8]>>> {
        self.journal_object_ref(offset)
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

    /// Creates an iterator over all DATA objects for the specified field,
    /// including the on-disk DATA object offset.
    pub fn field_data_objects_with_offsets<'a>(
        &'a self,
        field_name: &'a [u8],
    ) -> Result<FieldDataOffsetIterator<'a, M>> {
        let field_hash = self.hash(field_name);
        let Some(field_offset) = self.find_field_offset(field_hash, field_name)? else {
            return Ok(FieldDataOffsetIterator {
                journal: self,
                current_data_offset: None,
            });
        };

        let field_guard = self.field_ref(field_offset)?;
        let head_data_offset = field_guard.header.head_data_offset;

        Ok(FieldDataOffsetIterator {
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

#[cfg(test)]
#[path = "file_tests.rs"]
mod tests;
