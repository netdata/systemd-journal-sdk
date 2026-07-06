#![allow(unused_imports, clippy::field_reassign_with_default)]

use crate::hash;
use crate::object::*;
use crate::offset_array;
use error::{JournalError, Result};
use std::cell::{RefCell, UnsafeCell};
use std::fs::{File, OpenOptions};
use std::num::NonZero;
use std::num::NonZeroI128;
use std::num::NonZeroU64;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use window_manager::{MemoryMap, MemoryMapMut, WindowManager};
use zerocopy::{FromBytes, SplitByteSlice, SplitByteSliceMut};

#[cfg(debug_assertions)]
use std::backtrace::Backtrace;

use crate::value_guard::ValueGuard;

mod iterators;
pub use iterators::{EntryDataIterator, FieldDataIterator, FieldIterator};
mod matchers;
use matchers::{BucketVisitor, DataPayloadMatcher, PayloadMatcher};

// Size to pad objects to (8 bytes)
const OBJECT_ALIGNMENT: u64 = 8;
pub const DEFAULT_JOURNAL_FILE_MODE: u32 = 0o640;

#[derive(Debug, Clone)]
pub struct JournalFileOptions {
    machine_id: [u8; 16],
    seqnum_id: [u8; 16],
    file_id: [u8; 16],
    window_size: u64,
    data_hash_table_buckets: usize,
    field_hash_table_buckets: usize,
    enable_keyed_hash: bool,
    file_mode: u32,
}

impl JournalFileOptions {
    pub fn new(
        machine_id: [u8; 16],
        _boot_id: [u8; 16],
        seqnum_id: [u8; 16],
        file_id: [u8; 16],
    ) -> Self {
        Self {
            machine_id,
            seqnum_id,
            file_id,
            window_size: 64 * 1024,
            data_hash_table_buckets: 116_508,
            field_hash_table_buckets: 1_023,
            enable_keyed_hash: true,
            file_mode: DEFAULT_JOURNAL_FILE_MODE,
        }
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

    pub fn with_file_mode(mut self, mode: u32) -> Self {
        assert!(
            mode <= 0o777,
            "journal file mode must contain only permission bits"
        );
        self.file_mode = mode;
        self
    }

    pub fn create<M: MemoryMapMut>(self, path: impl AsRef<Path>) -> Result<JournalFile<M>> {
        JournalFile::create(path, self)
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
/// - The window manager is wrapped in an `UnsafeCell` to allow mutation through a shared reference.
/// - A single `RefCell<bool>` guards access to ensure only one object can be active at a time.
/// - Methods like `data_object()` return a `ValueGuard<T>` that automatically releases the lock
///   when dropped.
///
/// This design ensures that memory safety is maintained even though references to memory-mapped
/// regions could be invalidated when new objects are created.
pub struct JournalFile<M: MemoryMap> {
    // Persistent memory maps for journal header and data/field hash tables
    header_map: M,
    sanitized_header: Option<JournalHeader>,
    data_hash_table_map: Option<M>,
    field_hash_table_map: Option<M>,

    // Window manager for other objects
    window_manager: UnsafeCell<WindowManager<M>>,

    // Flag to track if any object is in use
    object_in_use: RefCell<bool>,

    #[cfg(debug_assertions)]
    prev_backtrace: RefCell<Backtrace>,
    #[cfg(debug_assertions)]
    backtrace: RefCell<Backtrace>,
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

    pub fn open(path: impl AsRef<Path>, window_size: u64) -> Result<Self> {
        debug_assert_eq!(window_size % OBJECT_ALIGNMENT, 0);

        // Open file and check its size
        let file = OpenOptions::new().read(true).write(false).open(&path)?;

        // Create a memory map for the header
        let header_size = std::mem::size_of::<JournalHeader>() as u64;
        let header_map = M::create(&file, 0, header_size)?;
        let header = JournalHeader::ref_from_prefix(&header_map).unwrap().0;
        if header.signature != *b"LPKSHHRH" {
            return Err(JournalError::InvalidMagicNumber);
        }
        let sanitized_header =
            (header.header_size < header_size).then(|| sanitize_header_for_size(*header));

        // Initialize the hash table maps if they exist
        let data_hash_table_map = map_hash_table(
            &file,
            header.header_size,
            header.data_hash_table_offset,
            header.data_hash_table_size,
        )?;
        let field_hash_table_map = map_hash_table(
            &file,
            header.header_size,
            header.field_hash_table_offset,
            header.field_hash_table_size,
        )?;

        // Create window manager for the rest of the objects
        let window_manager = UnsafeCell::new(WindowManager::new(file, window_size, 32)?);

        Ok(JournalFile {
            header_map,
            sanitized_header,
            data_hash_table_map,
            field_hash_table_map,
            window_manager,
            object_in_use: RefCell::new(false),

            #[cfg(debug_assertions)]
            prev_backtrace: RefCell::new(Backtrace::capture()),
            #[cfg(debug_assertions)]
            backtrace: RefCell::new(Backtrace::capture()),
        })
    }

    pub fn hash(&self, data: &[u8]) -> u64 {
        let is_keyed_hash = self
            .journal_header_ref()
            .has_incompatible_flag(HeaderIncompatibleFlags::KeyedHash);

        hash::journal_hash_data(
            data,
            is_keyed_hash,
            if is_keyed_hash {
                Some(&self.journal_header_ref().file_id)
            } else {
                None
            },
        )
    }

    pub fn entry_list(&self) -> Option<offset_array::List> {
        let head_offset = self.journal_header_ref().entry_array_offset?;
        let total_items =
            std::num::NonZeroUsize::new(self.journal_header_ref().n_entries as usize)?;
        Some(offset_array::List::new(head_offset, total_items))
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
        let size_needed = std::mem::size_of::<ObjectHeader>() as u64;
        // SAFETY: Read access goes through the internal window manager and the
        // returned header borrow is tied to `self`.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let window_manager = unsafe { &mut *self.window_manager.get() };
        let header_slice = window_manager.get_slice(position.get(), size_needed)?;
        Ok(ObjectHeader::ref_from_bytes(header_slice).unwrap())
    }

    fn object_data_ref(&self, offset: NonZeroU64, size_needed: u64) -> Result<&[u8]> {
        // SAFETY: Read access uses the internal window manager only to obtain a
        // mapped slice whose lifetime is tied to `self`.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let window_manager = unsafe { &mut *self.window_manager.get() };
        let object_slice = window_manager.get_slice(offset.get(), size_needed)?;
        Ok(object_slice)
    }

    fn journal_object_ref<'a, T>(&'a self, offset: NonZeroU64) -> Result<ValueGuard<'a, T>>
    where
        T: JournalObject<&'a [u8]>,
    {
        // Check if any object is already in use
        let mut is_in_use = self.object_in_use.borrow_mut();
        if *is_in_use {
            #[cfg(debug_assertions)]
            {
                eprintln!(
                    "Value is in use. Current Backtrace: {:?}, Previous Backtrace: {:?}",
                    self.backtrace.borrow().to_string(),
                    self.prev_backtrace.borrow().to_string()
                );
            }
            return Err(JournalError::ValueGuardInUse);
        }

        #[cfg(debug_assertions)]
        {
            self.backtrace.swap(&self.prev_backtrace);
            let _ = self.backtrace.replace(Backtrace::force_capture());
        }

        let is_compact = self
            .journal_header_ref()
            .has_incompatible_flag(HeaderIncompatibleFlags::Compact);

        let size_needed = {
            let header = self.object_header_ref(offset)?;
            header.size
        };

        let data = self.object_data_ref(offset, size_needed)?;
        let Some(value) = T::from_data(data, is_compact) else {
            return Err(JournalError::ZerocopyFailure);
        };

        // Mark as in use
        *is_in_use = true;

        Ok(ValueGuard::new(offset, value, &self.object_in_use))
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

    pub fn find_data_offset(&self, hash: u64, payload: &[u8]) -> Result<Option<NonZeroU64>> {
        let visitor = DataPayloadMatcher::new(payload, hash);
        self.visit_bucket(self.data_hash_table_ref(), hash, visitor)
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
                head_data_offset: None,
                current_data_offset: None,
            });
        };

        // Get the field object to access its head_data_offset
        let field_guard = self.field_ref(field_offset)?;
        let head_data_offset = field_guard.header.head_data_offset;

        // Create the iterator
        Ok(FieldDataIterator {
            journal: self,
            head_data_offset,
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
}

const INITIAL_FILE_SIZE: u64 = 8 * 1024 * 1024;

struct HashTableLayout {
    data_size: u64,
    field_size: u64,
    data_offset: u64,
    field_offset: u64,
}

impl HashTableLayout {
    fn from_options(options: &JournalFileOptions) -> Self {
        let data_size = (options.data_hash_table_buckets * std::mem::size_of::<HashItem>()) as u64;
        let field_size =
            (options.field_hash_table_buckets * std::mem::size_of::<HashItem>()) as u64;
        let field_offset = std::mem::size_of::<JournalHeader>() as u64
            + std::mem::size_of::<ObjectHeader>() as u64;
        let data_offset = field_offset + field_size + std::mem::size_of::<ObjectHeader>() as u64;
        Self {
            data_size,
            field_size,
            data_offset,
            field_offset,
        }
    }
}

#[derive(Clone, Copy)]
enum HashTableKind {
    Data,
    Field,
}

fn create_backing_file(path: impl AsRef<Path>, mode: u32) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).read(true).write(true);
    #[cfg(unix)]
    options.mode(mode);
    let file = options.open(path)?;
    file.set_len(INITIAL_FILE_SIZE)?;
    Ok(file)
}

fn initial_header(options: &JournalFileOptions, layout: &HashTableLayout) -> JournalHeader {
    let mut header = JournalHeader::default();
    header.signature = *b"LPKSHHRH";
    header.compatible_flags = HeaderCompatibleFlags::TailEntryBootId as u32;
    if options.enable_keyed_hash {
        header.incompatible_flags |= HeaderIncompatibleFlags::KeyedHash as u32;
    }
    header.data_hash_table_offset = NonZeroU64::new(layout.data_offset);
    header.data_hash_table_size = NonZeroU64::new(layout.data_size);
    header.field_hash_table_offset = NonZeroU64::new(layout.field_offset);
    header.field_hash_table_size = NonZeroU64::new(layout.field_size);
    header.tail_object_offset =
        NonZeroU64::new(layout.data_offset - std::mem::size_of::<ObjectHeader>() as u64);
    header.header_size = std::mem::size_of::<JournalHeader>() as u64;
    header.n_objects = 2;
    header.arena_size = INITIAL_FILE_SIZE - header.header_size;
    header.machine_id = options.machine_id;
    header.file_id = options.file_id;
    header.seqnum_id = options.seqnum_id;
    header
}

fn map_created_hash_table<M: MemoryMapMut>(
    file: &File,
    header: &JournalHeader,
    kind: HashTableKind,
) -> Result<Option<M>> {
    let (offset, size) = hash_table_header_fields(header, kind);
    map_hash_table(file, header.header_size, offset, size)
}

fn create_header_map<M: MemoryMapMut>(file: &File, header: &JournalHeader) -> Result<M> {
    let mut header_map = M::create(file, 0, std::mem::size_of::<JournalHeader>() as u64)?;
    let header_mut = JournalHeader::mut_from_prefix(&mut header_map).unwrap().0;
    *header_mut = *header;
    Ok(header_map)
}

fn write_hash_table_object_header<M: MemoryMapMut>(
    jf: &JournalFile<M>,
    header: &JournalHeader,
    kind: HashTableKind,
) -> Result<()> {
    let (offset, size) = hash_table_object_location(header, kind);
    let object_header = jf.object_header_mut(offset)?;
    object_header.type_ = match kind {
        HashTableKind::Data => ObjectType::DataHashTable as u8,
        HashTableKind::Field => ObjectType::FieldHashTable as u8,
    };
    object_header.size = size;
    Ok(())
}

fn hash_table_header_fields(
    header: &JournalHeader,
    kind: HashTableKind,
) -> (Option<NonZeroU64>, Option<NonZeroU64>) {
    match kind {
        HashTableKind::Data => (header.data_hash_table_offset, header.data_hash_table_size),
        HashTableKind::Field => (header.field_hash_table_offset, header.field_hash_table_size),
    }
}

fn hash_table_object_location(header: &JournalHeader, kind: HashTableKind) -> (NonZeroU64, u64) {
    let (offset, size) = hash_table_header_fields(header, kind);
    let object_header_size = std::mem::size_of::<ObjectHeader>() as u64;
    (
        NonZeroU64::new(offset.unwrap().get() - object_header_size).unwrap(),
        size.unwrap().get() + object_header_size,
    )
}

impl<M: MemoryMapMut> JournalFile<M> {
    pub fn create(path: impl AsRef<Path>, options: JournalFileOptions) -> Result<Self> {
        let file = create_backing_file(path, options.file_mode)?;
        let layout = HashTableLayout::from_options(&options);
        let header = initial_header(&options, &layout);
        let data_hash_table_map = map_created_hash_table(&file, &header, HashTableKind::Data)?;
        let field_hash_table_map = map_created_hash_table(&file, &header, HashTableKind::Field)?;
        let header_map = create_header_map::<M>(&file, &header)?;
        let window_manager = UnsafeCell::new(WindowManager::new(file, options.window_size, 32)?);

        let jf = JournalFile {
            header_map,
            sanitized_header: None,
            data_hash_table_map,
            field_hash_table_map,
            window_manager,
            object_in_use: RefCell::new(false),

            #[cfg(debug_assertions)]
            prev_backtrace: RefCell::new(Backtrace::capture()),
            #[cfg(debug_assertions)]
            backtrace: RefCell::new(Backtrace::capture()),
        };

        write_hash_table_object_header(&jf, &header, HashTableKind::Data)?;
        write_hash_table_object_header(&jf, &header, HashTableKind::Field)?;
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

    fn object_header_mut(&self, offset: NonZeroU64) -> Result<&mut ObjectHeader> {
        let size_needed = std::mem::size_of::<ObjectHeader>() as u64;
        // SAFETY: Mutable object access is serialized by the JournalFile API;
        // this unwrap gives the internal window manager the caller's borrow.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let window_manager = unsafe { &mut *self.window_manager.get() };
        let header_slice = window_manager.get_slice_mut(offset.get(), size_needed)?;
        Ok(ObjectHeader::mut_from_bytes(header_slice).unwrap())
    }

    fn object_data_mut(&self, offset: NonZeroU64, size_needed: u64) -> Result<&mut [u8]> {
        // SAFETY: Mutable access is serialized by JournalFile methods; the
        // returned slice is tied to the caller's borrow of `self`.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let window_manager = unsafe { &mut *self.window_manager.get() };
        let object_slice = window_manager.get_slice_mut(offset.get(), size_needed)?;
        Ok(object_slice)
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
        // Check if any object is already in use
        let mut is_in_use = self.object_in_use.borrow_mut();
        if *is_in_use {
            #[cfg(debug_assertions)]
            {
                eprintln!(
                    "Value is in use. Current Backtrace: {:?}, Previous Backtrace: {:?}",
                    self.backtrace.borrow().to_string(),
                    self.prev_backtrace.borrow().to_string()
                );
            }
            return Err(JournalError::ValueGuardInUse);
        }

        #[cfg(debug_assertions)]
        {
            self.backtrace.swap(&self.prev_backtrace);
            let _ = self.backtrace.replace(Backtrace::force_capture());
        }

        let is_compact = self
            .journal_header_ref()
            .has_incompatible_flag(HeaderIncompatibleFlags::Compact);

        let size_needed = match size {
            Some(size) => {
                let header = self.object_header_mut(offset)?;
                header.type_ = type_ as u8;
                header.size = size;
                size
            }
            None => {
                let header = self.object_header_ref(offset)?;
                if header.type_ != type_ as u8 {
                    return Err(JournalError::InvalidObjectType);
                }
                header.size
            }
        };

        let data = self.object_data_mut(offset, size_needed)?;
        let value = T::from_data_mut(data, is_compact).ok_or(JournalError::ZerocopyFailure)?;

        // Mark as in use
        *is_in_use = true;
        Ok(ValueGuard::new(offset, value, &self.object_in_use))
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

        let offset_array = self.journal_object_mut(ObjectType::EntryArray, offset, size);
        offset_array
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
        let size = size.map(|n| std::mem::size_of::<DataObjectHeader>() as u64 + n);
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

#[cfg(test)]
mod tests;
