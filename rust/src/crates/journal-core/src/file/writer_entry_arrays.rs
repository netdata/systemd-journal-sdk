use super::mmap::MmapMut;
use super::object::*;
use super::writer::{JOURNAL_COMPACT_SIZE_MAX, JournalWriter, OBJECT_ALIGNMENT};
use crate::error::{JournalError, Result};
use crate::file::JournalFile;
use std::num::NonZeroU64;

impl JournalWriter {
    pub(super) fn allocate_new_array(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        capacity: NonZeroU64,
    ) -> Result<NonZeroU64> {
        // let new_capacity = previous_capacity.saturating_mul(NonZeroU64::new(2).unwrap());

        let array_offset = self.append_offset;
        let is_compact = Self::is_compact(journal_file);
        Self::ensure_compact_object_fits(
            is_compact,
            array_offset,
            std::mem::size_of::<crate::file::OffsetArrayObjectHeader>() as u64
                + capacity.get() * Self::offset_array_item_size(is_compact),
        )?;
        let array_size = {
            let array_guard = journal_file.offset_array_mut(array_offset, Some(capacity))?;

            array_guard.header.object_header.aligned_size()
        };
        self.hmac_put_object(journal_file, array_offset.get(), ObjectType::EntryArray)?;
        self.object_added(journal_file, array_offset, array_size)?;
        journal_file.journal_header_mut().n_entry_arrays += 1;

        Ok(array_offset)
    }

    pub(super) fn next_entry_array_capacity(index: u64, previous_capacity: u64) -> u64 {
        let mut capacity = previous_capacity;
        if index > capacity {
            capacity = (index + 1) * 2;
        } else {
            capacity *= 2;
        }
        capacity.max(4)
    }

    pub(super) fn update_data_hash_chain_depth(
        journal_file: &mut JournalFile<MmapMut>,
        hash: u64,
    ) -> Result<()> {
        let depth = Self::current_data_hash_chain_depth(journal_file, hash)?;
        let max_depth = journal_file
            .journal_header_ref()
            .data_hash_chain_depth
            .max(depth);
        journal_file.journal_header_mut().data_hash_chain_depth = max_depth;
        Ok(())
    }

    pub(super) fn current_data_hash_chain_depth(
        journal_file: &JournalFile<MmapMut>,
        hash: u64,
    ) -> Result<u64> {
        let Some(hash_table) = journal_file.data_hash_table_ref() else {
            return Err(JournalError::MissingHashTable);
        };
        let mut depth = 0;
        let mut object_offset = hash_table.hash_item_ref(hash).head_hash_offset;
        while let Some(offset) = object_offset {
            let object = journal_file.data_ref(offset)?;
            object_offset = object.header.next_hash_offset;
            if object_offset.is_some() {
                depth += 1;
            }
        }
        Ok(depth)
    }

    pub(super) fn current_field_hash_chain_depth(
        journal_file: &JournalFile<MmapMut>,
        hash: u64,
    ) -> Result<u64> {
        let Some(hash_table) = journal_file.field_hash_table_ref() else {
            return Err(JournalError::MissingHashTable);
        };
        let mut depth = 0;
        let mut object_offset = hash_table.hash_item_ref(hash).head_hash_offset;
        while let Some(offset) = object_offset {
            let object = journal_file.field_ref(offset)?;
            object_offset = object.header.next_hash_offset;
            if object_offset.is_some() {
                depth += 1;
            }
        }
        Ok(depth)
    }

    pub(super) fn append_to_entry_array(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        entry_offset: NonZeroU64,
    ) -> Result<()> {
        let is_compact = Self::is_compact(journal_file);
        Self::ensure_compact_offset(is_compact, entry_offset)?;
        let Some(entry_array_offset) = journal_file.journal_header_ref().entry_array_offset else {
            return self.append_first_entry_array(journal_file, entry_offset);
        };
        let entry_count = journal_file.journal_header_ref().n_entries;
        let tail_offset =
            self.entry_array_tail_offset(journal_file, entry_array_offset, entry_count)?;
        let tail_capacity = self.offset_array_capacity(journal_file, tail_offset)?;
        let tail_entries = self.entry_array_tail_entries(
            journal_file,
            entry_array_offset,
            tail_offset,
            entry_count,
        )?;

        if tail_entries < tail_capacity {
            return self.append_to_entry_array_tail(
                journal_file,
                tail_offset,
                tail_entries,
                entry_offset,
            );
        }
        self.grow_entry_array_tail(
            journal_file,
            tail_offset,
            entry_count,
            tail_capacity,
            entry_offset,
        )
    }

    pub(super) fn append_first_entry_array(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        entry_offset: NonZeroU64,
    ) -> Result<()> {
        let array_offset = self.allocate_new_array(journal_file, NonZeroU64::new(4).unwrap())?;
        {
            let mut array_guard = journal_file.offset_array_mut(array_offset, None)?;
            array_guard.set(0, entry_offset)?;
        }
        let header = journal_file.journal_header_mut();
        header.entry_array_offset = Some(array_offset);
        header.tail_entry_array_offset = array_offset.get() as u32;
        header.tail_entry_array_n_entries = 1;
        Ok(())
    }

    pub(super) fn entry_array_tail_offset(
        &self,
        journal_file: &JournalFile<MmapMut>,
        entry_array_offset: NonZeroU64,
        entry_count: u64,
    ) -> Result<NonZeroU64> {
        if let Some(tail_offset) = NonZeroU64::new(
            journal_file
                .journal_header_ref()
                .tail_entry_array_offset
                .into(),
        ) {
            return Ok(tail_offset);
        }
        self.find_entry_array_tail(journal_file, entry_array_offset, entry_count)
    }

    pub(super) fn find_entry_array_tail(
        &self,
        journal_file: &JournalFile<MmapMut>,
        mut offset: NonZeroU64,
        mut remaining: u64,
    ) -> Result<NonZeroU64> {
        loop {
            let array_guard = journal_file.offset_array_ref(offset)?;
            let capacity = array_guard.capacity() as u64;
            if remaining < capacity || array_guard.header.next_offset_array.is_none() {
                return Ok(offset);
            }
            remaining -= capacity;
            offset = array_guard
                .header
                .next_offset_array
                .ok_or(JournalError::InvalidOffsetArrayOffset)?;
        }
    }

    pub(super) fn offset_array_capacity(
        &self,
        journal_file: &JournalFile<MmapMut>,
        offset: NonZeroU64,
    ) -> Result<u64> {
        let array_guard = journal_file.offset_array_ref(offset)?;
        Ok(array_guard.capacity() as u64)
    }

    pub(super) fn entry_array_tail_entries(
        &self,
        journal_file: &JournalFile<MmapMut>,
        entry_array_offset: NonZeroU64,
        tail_offset: NonZeroU64,
        entry_count: u64,
    ) -> Result<u64> {
        let cached = journal_file.journal_header_ref().tail_entry_array_n_entries as u64;
        if cached != 0 {
            return Ok(cached);
        }
        self.compute_entry_array_tail_entries(
            journal_file,
            entry_array_offset,
            tail_offset,
            entry_count,
        )
    }

    pub(super) fn compute_entry_array_tail_entries(
        &self,
        journal_file: &JournalFile<MmapMut>,
        mut offset: NonZeroU64,
        tail_offset: NonZeroU64,
        mut entries: u64,
    ) -> Result<u64> {
        while offset != tail_offset {
            let array_guard = journal_file.offset_array_ref(offset)?;
            entries -= array_guard.capacity() as u64;
            offset = array_guard
                .header
                .next_offset_array
                .ok_or(JournalError::InvalidOffsetArrayOffset)?;
        }
        Ok(entries)
    }

    pub(super) fn append_to_entry_array_tail(
        &self,
        journal_file: &mut JournalFile<MmapMut>,
        tail_offset: NonZeroU64,
        tail_entries: u64,
        entry_offset: NonZeroU64,
    ) -> Result<()> {
        let mut tail_guard = journal_file.offset_array_mut(tail_offset, None)?;
        tail_guard.set(tail_entries as usize, entry_offset)?;
        drop(tail_guard);
        let header = journal_file.journal_header_mut();
        header.tail_entry_array_offset = tail_offset.get() as u32;
        header.tail_entry_array_n_entries = (tail_entries + 1) as u32;
        Ok(())
    }

    pub(super) fn grow_entry_array_tail(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        tail_offset: NonZeroU64,
        entry_count: u64,
        tail_capacity: u64,
        entry_offset: NonZeroU64,
    ) -> Result<()> {
        let new_capacity =
            NonZeroU64::new(Self::next_entry_array_capacity(entry_count, tail_capacity)).unwrap();
        let new_array_offset = self.allocate_new_array(journal_file, new_capacity)?;
        {
            let mut tail_guard = journal_file.offset_array_mut(tail_offset, None)?;
            tail_guard.header.next_offset_array = Some(new_array_offset);
        }
        {
            let mut new_array_guard = journal_file.offset_array_mut(new_array_offset, None)?;
            new_array_guard.set(0, entry_offset)?;
        }
        let header = journal_file.journal_header_mut();
        header.tail_entry_array_offset = new_array_offset.get() as u32;
        header.tail_entry_array_n_entries = 1;

        Ok(())
    }

    pub(super) fn append_to_data_entry_array(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        mut array_offset: NonZeroU64,
        entry_offset: NonZeroU64,
        current_count: u64,
    ) -> Result<(NonZeroU64, u64)> {
        let is_compact = Self::is_compact(journal_file);
        Self::ensure_compact_offset(is_compact, entry_offset)?;

        // Navigate to the tail of the array chain
        let mut current_index = 0u64;
        #[allow(unused_assignments)]
        let mut tail_offset = array_offset;

        loop {
            let array_guard = journal_file.offset_array_ref(array_offset)?;
            let capacity = array_guard.capacity() as u64;

            if current_index + capacity >= current_count {
                // This is the tail array
                tail_offset = array_offset;
                break;
            }

            current_index += capacity;

            let Some(next_offset) = array_guard.header.next_offset_array else {
                // This shouldn't happen if counts are correct
                return Err(JournalError::InvalidOffsetArrayOffset);
            };

            array_offset = next_offset;
        }

        // Try to add to the tail array
        let tail_capacity = {
            let tail_guard = journal_file.offset_array_ref(tail_offset)?;
            tail_guard.capacity() as u64
        };

        let entries_in_tail = current_count - current_index;

        if entries_in_tail < tail_capacity {
            // There's space in the tail array
            let mut tail_guard = journal_file.offset_array_mut(tail_offset, None)?;
            tail_guard.set(entries_in_tail as usize, entry_offset)?;
            Ok((tail_offset, entries_in_tail + 1))
        } else {
            // Need to create a new array
            let new_capacity = NonZeroU64::new(Self::next_entry_array_capacity(
                current_count,
                tail_capacity,
            ))
            .unwrap();
            let new_array_offset = self.allocate_new_array(journal_file, new_capacity)?;

            // Link the old tail to the new array
            let mut tail_guard = journal_file.offset_array_mut(tail_offset, None)?;
            tail_guard.header.next_offset_array = Some(new_array_offset);
            drop(tail_guard);

            // Add entry to the new array
            let mut new_array_guard = journal_file.offset_array_mut(new_array_offset, None)?;
            new_array_guard.set(0, entry_offset)?;
            Ok((new_array_offset, 1))
        }
    }

    pub(super) fn append_to_data_entry_array_tail(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        tail_offset: NonZeroU64,
        tail_entries: u64,
        entry_offset: NonZeroU64,
        current_count: u64,
    ) -> Result<Option<(NonZeroU64, u64)>> {
        if tail_entries == 0 || tail_entries > current_count {
            return Ok(None);
        }
        let Some(tail_capacity) = self.compact_data_tail_capacity(journal_file, tail_offset)?
        else {
            return Ok(None);
        };
        if tail_entries > tail_capacity {
            return Ok(None);
        }
        if tail_entries < tail_capacity {
            return self.append_to_existing_data_tail(
                journal_file,
                tail_offset,
                tail_entries,
                entry_offset,
            );
        }
        self.grow_data_entry_array_tail(
            journal_file,
            tail_offset,
            current_count,
            tail_capacity,
            entry_offset,
        )
    }

    pub(super) fn compact_data_tail_capacity(
        &self,
        journal_file: &JournalFile<MmapMut>,
        tail_offset: NonZeroU64,
    ) -> Result<Option<u64>> {
        let tail_guard = match journal_file.offset_array_ref(tail_offset) {
            Ok(guard) => guard,
            Err(_) => return Ok(None),
        };
        if tail_guard.header.next_offset_array.is_some() {
            return Ok(None);
        }
        Ok(Some(tail_guard.capacity() as u64))
    }

    pub(super) fn append_to_existing_data_tail(
        &self,
        journal_file: &mut JournalFile<MmapMut>,
        tail_offset: NonZeroU64,
        tail_entries: u64,
        entry_offset: NonZeroU64,
    ) -> Result<Option<(NonZeroU64, u64)>> {
        let mut tail_guard = journal_file.offset_array_mut(tail_offset, None)?;
        tail_guard.set(tail_entries as usize, entry_offset)?;
        Ok(Some((tail_offset, tail_entries + 1)))
    }

    pub(super) fn grow_data_entry_array_tail(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        tail_offset: NonZeroU64,
        current_count: u64,
        tail_capacity: u64,
        entry_offset: NonZeroU64,
    ) -> Result<Option<(NonZeroU64, u64)>> {
        let new_capacity = NonZeroU64::new(Self::next_entry_array_capacity(
            current_count,
            tail_capacity,
        ))
        .unwrap();
        let new_array_offset = self.allocate_new_array(journal_file, new_capacity)?;

        {
            let mut tail_guard = journal_file.offset_array_mut(tail_offset, None)?;
            tail_guard.header.next_offset_array = Some(new_array_offset);
        }
        {
            let mut new_array_guard = journal_file.offset_array_mut(new_array_offset, None)?;
            new_array_guard.set(0, entry_offset)?;
        }

        Ok(Some((new_array_offset, 1)))
    }

    pub(super) fn link_data_to_entry(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        entry_offset: NonZeroU64,
        entry_item_index: usize,
    ) -> Result<()> {
        let data_offset = self.entry_items[entry_item_index].offset;
        let mut data_guard = journal_file.data_mut(data_offset, None)?;
        let Some(n_entries) = data_guard.header.n_entries else {
            return Self::link_data_first_entry(&mut data_guard, entry_offset);
        };
        let n_entries = n_entries.get();
        if n_entries == 0 {
            unreachable!();
        }
        if n_entries == 1 {
            drop(data_guard);
            return self.promote_data_entry_array(journal_file, data_offset, entry_offset);
        }

        let array_offset = data_guard
            .header
            .entry_array_offset
            .ok_or(JournalError::InvalidOffsetArrayOffset)?;
        let is_compact = Self::is_compact(journal_file);
        let compact_tail = Self::compact_data_tail(&data_guard);
        drop(data_guard);
        self.append_data_entry_array_link(
            journal_file,
            data_offset,
            array_offset,
            entry_offset,
            n_entries,
            is_compact,
            compact_tail,
        )
    }

    pub(super) fn link_data_first_entry(
        data_guard: &mut DataObject<&mut [u8]>,
        entry_offset: NonZeroU64,
    ) -> Result<()> {
        data_guard.header.entry_offset = Some(entry_offset);
        data_guard.header.n_entries = NonZeroU64::new(1);
        Ok(())
    }

    pub(super) fn promote_data_entry_array(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        data_offset: NonZeroU64,
        entry_offset: NonZeroU64,
    ) -> Result<()> {
        let array_offset = self.allocate_new_array(journal_file, NonZeroU64::new(4).unwrap())?;
        {
            let mut array_guard = journal_file.offset_array_mut(array_offset, None)?;
            array_guard.set(0, entry_offset)?;
        }
        let is_compact = Self::is_compact(journal_file);
        let mut data_guard = journal_file.data_mut(data_offset, None)?;
        data_guard.header.entry_array_offset = Some(array_offset);
        if is_compact {
            Self::set_compact_data_tail(&mut data_guard, array_offset, 1)?;
        }
        data_guard.header.n_entries = NonZeroU64::new(2);
        Ok(())
    }

    pub(super) fn append_data_entry_array_link(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        data_offset: NonZeroU64,
        array_offset: NonZeroU64,
        entry_offset: NonZeroU64,
        n_entries: u64,
        is_compact: bool,
        compact_tail: Option<(NonZeroU64, u64)>,
    ) -> Result<()> {
        let current_count = n_entries - 1;
        let (tail_offset, tail_entries) = self.append_data_entry_array_link_tail(
            journal_file,
            array_offset,
            entry_offset,
            current_count,
            is_compact,
            compact_tail,
        )?;
        let mut data_guard = journal_file.data_mut(data_offset, None)?;
        if is_compact {
            Self::set_compact_data_tail(&mut data_guard, tail_offset, tail_entries)?;
        }
        data_guard.header.n_entries = NonZeroU64::new(n_entries + 1);
        Ok(())
    }

    pub(super) fn append_data_entry_array_link_tail(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        array_offset: NonZeroU64,
        entry_offset: NonZeroU64,
        current_count: u64,
        is_compact: bool,
        compact_tail: Option<(NonZeroU64, u64)>,
    ) -> Result<(NonZeroU64, u64)> {
        if let (true, Some((tail_offset, tail_entries))) = (is_compact, compact_tail) {
            if let Some(result) = self.append_to_data_entry_array_tail(
                journal_file,
                tail_offset,
                tail_entries,
                entry_offset,
                current_count,
            )? {
                return Ok(result);
            }
        }
        self.append_to_data_entry_array(journal_file, array_offset, entry_offset, current_count)
    }

    pub(super) fn compact_data_tail(
        data_guard: &DataObject<&mut [u8]>,
    ) -> Option<(NonZeroU64, u64)> {
        match &data_guard.payload {
            DataPayloadType::Compact { compact_fields, .. } => {
                let tail_offset = NonZeroU64::new(compact_fields.tail_entry_array_offset as u64)?;
                let tail_entries = compact_fields.tail_entry_array_n_entries as u64;
                (tail_entries != 0).then_some((tail_offset, tail_entries))
            }
            DataPayloadType::Regular(_) => None,
        }
    }

    pub(super) fn is_compact(journal_file: &JournalFile<MmapMut>) -> bool {
        journal_file
            .journal_header_ref()
            .has_incompatible_flag(HeaderIncompatibleFlags::Compact)
    }

    pub(super) fn entry_item_size(is_compact: bool) -> u64 {
        if is_compact {
            std::mem::size_of::<CompactEntryItem>() as u64
        } else {
            std::mem::size_of::<RegularEntryItem>() as u64
        }
    }

    pub(super) fn offset_array_item_size(is_compact: bool) -> u64 {
        if is_compact {
            std::mem::size_of::<u32>() as u64
        } else {
            std::mem::size_of::<u64>() as u64
        }
    }

    pub(super) fn data_object_size(is_compact: bool, payload_size: u64) -> u64 {
        let mut size = std::mem::size_of::<DataObjectHeader>() as u64 + payload_size;
        if is_compact {
            size += std::mem::size_of::<CompactDataFields>() as u64;
        }
        size
    }

    pub(super) fn ensure_compact_offset(is_compact: bool, offset: NonZeroU64) -> Result<()> {
        if is_compact && offset.get() > JOURNAL_COMPACT_SIZE_MAX {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        Ok(())
    }

    pub(super) fn ensure_compact_object_fits(
        is_compact: bool,
        offset: NonZeroU64,
        object_size: u64,
    ) -> Result<()> {
        if !is_compact {
            return Ok(());
        }

        let end_offset = offset
            .get()
            .checked_add(object_size)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        let aligned_end = (end_offset + (OBJECT_ALIGNMENT - 1)) & !(OBJECT_ALIGNMENT - 1);
        if offset.get() > JOURNAL_COMPACT_SIZE_MAX || aligned_end > JOURNAL_COMPACT_SIZE_MAX {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        Ok(())
    }

    pub(super) fn set_compact_data_tail(
        data_guard: &mut DataObject<&mut [u8]>,
        tail_offset: NonZeroU64,
        tail_entries: u64,
    ) -> Result<()> {
        match &mut data_guard.payload {
            DataPayloadType::Compact { compact_fields, .. } => {
                compact_fields.tail_entry_array_offset = u32::try_from(tail_offset.get())
                    .map_err(|_| JournalError::ObjectExceedsFileBounds)?;
                compact_fields.tail_entry_array_n_entries = u32::try_from(tail_entries)
                    .map_err(|_| JournalError::ObjectExceedsFileBounds)?;
                Ok(())
            }
            DataPayloadType::Regular(_) => Err(JournalError::InvalidObjectType),
        }
    }
}
