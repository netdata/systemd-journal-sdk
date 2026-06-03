use super::file::{
    Compression, JOURNAL_COMPACT_SIZE_MAX, JournalFile, JournalFileOptions, OBJECT_ALIGNMENT,
    map_hash_table, round_up_to_file_size_increment, validate_offset_alignment,
};
use super::mmap::{MemoryMap, MemoryMapMut, WindowManager};
use super::object::*;
use crate::error::{JournalError, Result};
use crate::file::guarded_cell::GuardedCell;
use crate::file::value_guard::ValueGuard;
use std::fs::{File, OpenOptions};
use std::num::NonZeroU64;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use zerocopy::FromBytes;

#[derive(Debug, Clone, Copy)]
struct CreateLayout {
    data_hash_table_size: usize,
    field_hash_table_size: usize,
    data_hash_table_offset: u64,
    field_hash_table_offset: u64,
    data_hash_table_object_offset: u64,
    file_size: u64,
}

#[derive(Debug, Clone, Copy)]
struct MutableObjectContext {
    object_type: ObjectType,
    is_compact: bool,
    arena_end: u64,
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
        if header.header_size < header_size {
            return Err(JournalError::UnsupportedJournalFile);
        }
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
            sanitized_header: None,
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
        self.create_successor_with_file_mode(file, max_file_size, self.current_file_mode())
    }

    pub fn create_successor_with_file_mode(
        &self,
        file: &crate::repository::File,
        max_file_size: Option<u64>,
        file_mode: u32,
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
        .with_file_mode(file_mode)
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
        let fd = Self::open_new_file(file, options.file_mode)?;
        let layout = Self::create_layout(&options)?;
        if options.compact && layout.file_size > JOURNAL_COMPACT_SIZE_MAX {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        fd.set_len(layout.file_size)?;
        let mut header = Self::create_header(&options, layout);
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
        let header_map = Self::create_header_map(&fd, &mut header)?;
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

        jf.write_initial_hash_table_headers(header)?;
        jf.sync()?;
        Ok(jf)
    }

    fn current_file_mode(&self) -> u32 {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(self.file.path()) {
                return metadata.permissions().mode() & 0o777;
            }
        }
        super::file::DEFAULT_JOURNAL_FILE_MODE
    }

    fn open_new_file(file: &crate::repository::File, mode: u32) -> Result<File> {
        let mut open_options = OpenOptions::new();
        open_options
            .create(true)
            .truncate(true)
            .read(true)
            .write(true);
        #[cfg(unix)]
        open_options.mode(mode);
        Ok(open_options.open(file.path())?)
    }

    fn create_layout(options: &JournalFileOptions) -> Result<CreateLayout> {
        let data_hash_table_size =
            options.data_hash_table_buckets * std::mem::size_of::<HashItem>();
        let field_hash_table_size =
            options.field_hash_table_buckets * std::mem::size_of::<HashItem>();
        let field_hash_table_offset = std::mem::size_of::<JournalHeader>() as u64
            + std::mem::size_of::<ObjectHeader>() as u64;
        let data_hash_table_offset = field_hash_table_offset
            + field_hash_table_size as u64
            + std::mem::size_of::<ObjectHeader>() as u64;
        let data_hash_table_object_offset =
            data_hash_table_offset - std::mem::size_of::<ObjectHeader>() as u64;
        let append_offset = data_hash_table_offset + data_hash_table_size as u64;
        let file_size = round_up_to_file_size_increment(append_offset)?;
        Ok(CreateLayout {
            data_hash_table_size,
            field_hash_table_size,
            data_hash_table_offset,
            field_hash_table_offset,
            data_hash_table_object_offset,
            file_size,
        })
    }

    fn create_header(options: &JournalFileOptions, layout: CreateLayout) -> JournalHeader {
        let mut header = JournalHeader::default();
        header.signature = *b"LPKSHHRH";
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
        header.data_hash_table_offset = NonZeroU64::new(layout.data_hash_table_offset);
        header.data_hash_table_size = NonZeroU64::new(layout.data_hash_table_size as u64);
        header.field_hash_table_offset = NonZeroU64::new(layout.field_hash_table_offset);
        header.field_hash_table_size = NonZeroU64::new(layout.field_hash_table_size as u64);
        header.tail_object_offset = NonZeroU64::new(layout.data_hash_table_object_offset);
        header.header_size = std::mem::size_of::<JournalHeader>() as u64;
        header.n_objects = 2;
        header.arena_size = layout.file_size - header.header_size;
        header.machine_id = *options.machine_id.as_bytes();
        header.file_id = *options.file_id.as_bytes();
        header.seqnum_id = *options.seqnum_id.as_bytes();
        header
    }

    fn create_header_map(fd: &File, header: &mut JournalHeader) -> Result<M> {
        let header_size = std::mem::size_of::<JournalHeader>() as u64;
        let mut header_map = M::create(fd, 0, header_size)?;
        {
            let header_mut = JournalHeader::mut_from_prefix(&mut header_map).unwrap().0;
            *header_mut = *header;
            header_mut.state = JournalState::Online as u8;
            header.state = JournalState::Online as u8;
        }
        Ok(header_map)
    }

    fn write_initial_hash_table_headers(&mut self, header: JournalHeader) -> Result<()> {
        self.write_hash_table_object_header(
            header.data_hash_table_offset.unwrap(),
            header.data_hash_table_size.unwrap(),
            ObjectType::DataHashTable,
        )?;
        self.write_hash_table_object_header(
            header.field_hash_table_offset.unwrap(),
            header.field_hash_table_size.unwrap(),
            ObjectType::FieldHashTable,
        )
    }

    fn write_hash_table_object_header(
        &self,
        table_offset: NonZeroU64,
        table_size: NonZeroU64,
        object_type: ObjectType,
    ) -> Result<()> {
        let object_offset =
            NonZeroU64::new(table_offset.get() - std::mem::size_of::<ObjectHeader>() as u64)
                .unwrap();
        let object_header = self.object_header_mut(object_offset)?;
        object_header.type_ = object_type as u8;
        object_header.size = table_size.get() + std::mem::size_of::<ObjectHeader>() as u64;
        Ok(())
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
        let context = self.mutable_object_context(type_, offset)?;
        self.window_manager.with_guarded(offset, |wm| {
            let size_needed = Self::mutable_object_size(wm, context, offset, size)?;
            let data = wm.get_slice_mut(offset.get(), size_needed)?;
            let value =
                T::from_data_mut(data, context.is_compact).ok_or(JournalError::ZerocopyFailure)?;
            Ok(value)
        })
    }

    fn mutable_object_context(
        &self,
        object_type: ObjectType,
        offset: NonZeroU64,
    ) -> Result<MutableObjectContext> {
        validate_offset_alignment(offset)?;
        let journal_header = self.journal_header_ref();
        let header_size = journal_header.header_size;
        if offset.get() < header_size {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        Ok(MutableObjectContext {
            object_type,
            is_compact: journal_header.has_incompatible_flag(HeaderIncompatibleFlags::Compact),
            arena_end: header_size + journal_header.arena_size,
        })
    }

    fn mutable_object_size(
        wm: &mut WindowManager<M>,
        context: MutableObjectContext,
        offset: NonZeroU64,
        size: Option<u64>,
    ) -> Result<u64> {
        match size {
            Some(size) => Self::initialize_mutable_object_header(wm, context, offset, size),
            None => Self::existing_mutable_object_size(wm, context, offset),
        }
    }

    fn initialize_mutable_object_header(
        wm: &mut WindowManager<M>,
        context: MutableObjectContext,
        offset: NonZeroU64,
        size: u64,
    ) -> Result<u64> {
        let header_slice =
            wm.get_slice_mut(offset.get(), std::mem::size_of::<ObjectHeader>() as u64)?;
        let header = ObjectHeader::mut_from_bytes(header_slice)
            .map_err(|_| JournalError::ZerocopyFailure)?;
        header.type_ = context.object_type as u8;
        header.size = size;
        Ok(size)
    }

    fn existing_mutable_object_size(
        wm: &mut WindowManager<M>,
        context: MutableObjectContext,
        offset: NonZeroU64,
    ) -> Result<u64> {
        let header_slice =
            wm.get_slice(offset.get(), std::mem::size_of::<ObjectHeader>() as u64)?;
        let header = ObjectHeader::ref_from_bytes(header_slice)
            .map_err(|_| JournalError::ZerocopyFailure)?;
        if header.type_ != context.object_type as u8 {
            return Err(JournalError::InvalidObjectType);
        }
        let size_needed = header.validated_size()?;
        Self::validate_mutable_object_bounds(context, offset, size_needed)?;
        Ok(size_needed)
    }

    fn validate_mutable_object_bounds(
        context: MutableObjectContext,
        offset: NonZeroU64,
        size_needed: u64,
    ) -> Result<()> {
        let end_offset = offset
            .get()
            .checked_add(size_needed)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        if end_offset > context.arena_end {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        Ok(())
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
