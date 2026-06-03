#![allow(unused_imports, dead_code)]

use crate::{
    CompactEntryItem, DataHashTable, DataObject, DataObjectHeader, DataPayloadType, EntryObject,
    EntryObjectHeader, FieldHashTable, FieldObject, FieldObjectHeader, HashItem, HashTable,
    HashTableMut, HashableObject, HashableObjectMut, HeaderIncompatibleFlags, JournalFile,
    JournalFileOptions, JournalHeader, JournalState, ObjectHeader, ObjectType, RegularEntryItem,
    journal_hash_data,
};
use error::{JournalError, Result};
use memmap2::MmapMut;
use rand::{Rng, seq::IndexedRandom};
use std::num::{NonZeroU64, NonZeroUsize};
use std::path::Path;
use window_manager::MemoryMapMut;
use zerocopy::{FromBytes, IntoBytes};

const OBJECT_ALIGNMENT: u64 = 8;

#[derive(Debug, Clone, Copy)]
struct EntryItem {
    offset: NonZeroU64,
    hash: u64,
}

pub struct JournalWriter {
    tail_object_offset: NonZeroU64,
    append_offset: NonZeroU64,
    next_seqnum: u64,
    num_written_objects: u64,
    entry_items: Vec<EntryItem>,
    first_entry_monotonic: Option<u64>,
}

impl JournalWriter {
    /// Get current file size in bytes
    pub fn current_file_size(&self) -> u64 {
        self.append_offset.get()
    }

    /// Get the monotonic timestamp of the first entry written to this file
    pub fn first_entry_monotonic(&self) -> Option<u64> {
        self.first_entry_monotonic
    }

    pub fn new(journal_file: &mut JournalFile<MmapMut>) -> Result<Self> {
        let (append_offset, next_seqnum) = {
            let header = journal_file.journal_header_ref();
            if !header.has_incompatible_flag(HeaderIncompatibleFlags::KeyedHash) {
                return Err(JournalError::UnsupportedJournalFile);
            }

            let Some(tail_object_offset) = header.tail_object_offset else {
                return Err(JournalError::InvalidMagicNumber);
            };

            let tail_object = journal_file.object_header_ref(tail_object_offset)?;

            (
                tail_object_offset.saturating_add(tail_object.size),
                header.tail_entry_seqnum + 1,
            )
        };

        Ok(Self {
            tail_object_offset: journal_file
                .journal_header_ref()
                .tail_object_offset
                .unwrap(),
            append_offset,
            next_seqnum,
            num_written_objects: 0,
            entry_items: Vec::with_capacity(128),
            first_entry_monotonic: None,
        })
    }

    pub fn add_entry(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        items: &[&[u8]],
        realtime: u64,
        monotonic: u64,
        boot_id: [u8; 16],
    ) -> Result<()> {
        let header = journal_file.journal_header_ref();
        if !header.has_incompatible_flag(HeaderIncompatibleFlags::KeyedHash) {
            return Err(JournalError::UnsupportedJournalFile);
        }

        // Write the data/field objects while computing the entry's xor-hash
        // and storing each data object's offset/hash
        let mut xor_hash = 0;
        {
            self.entry_items.clear();
            for payload in items {
                let offset = self.add_data(journal_file, payload)?;
                let hash = {
                    let data_guard = journal_file.data_ref(offset)?;
                    data_guard.hash()
                };

                let entry_item = EntryItem { offset, hash };
                self.entry_items.push(entry_item);

                xor_hash ^= journal_hash_data(payload, true, None);
            }

            self.entry_items
                .sort_unstable_by(|a, b| a.offset.cmp(&b.offset));
            self.entry_items.dedup_by(|a, b| a.offset == b.offset);
        }

        // write the entry itself
        let entry_offset = self.append_offset;
        let entry_size = {
            let size = Some(self.entry_items.len() as u64 * 16);
            let mut entry_guard = journal_file.entry_mut(entry_offset, size)?;

            entry_guard.header.seqnum = self.next_seqnum;
            entry_guard.header.xor_hash = xor_hash;
            entry_guard.header.boot_id = boot_id;
            entry_guard.header.monotonic = monotonic;
            entry_guard.header.realtime = realtime;

            // set each entry item
            for (index, entry_item) in self.entry_items.iter().enumerate() {
                entry_guard
                    .items
                    .set(index, entry_item.offset, Some(entry_item.hash));
            }

            entry_guard.header.object_header.aligned_size()
        };
        self.object_added(entry_offset, entry_size);

        self.append_to_entry_array(journal_file, entry_offset)?;
        for entry_item_index in 0..self.entry_items.len() {
            self.link_data_to_entry(journal_file, entry_offset, entry_item_index)?;
        }

        self.entry_added(
            journal_file.journal_header_mut(),
            entry_offset,
            realtime,
            monotonic,
            boot_id,
        );

        Ok(())
    }

    fn object_added(&mut self, object_offset: NonZeroU64, object_size: u64) {
        self.tail_object_offset = object_offset;
        self.append_offset = object_offset.saturating_add(object_size);
        self.num_written_objects += 1;
    }

    fn entry_added(
        &mut self,
        header: &mut JournalHeader,
        entry_offset: NonZeroU64,
        realtime: u64,
        monotonic: u64,
        boot_id: [u8; 16],
    ) {
        header.n_objects += self.num_written_objects;
        header.tail_object_offset = Some(self.tail_object_offset);

        if header.head_entry_seqnum == 0 {
            header.head_entry_seqnum = self.next_seqnum;
        }
        if header.head_entry_realtime == 0 {
            header.head_entry_realtime = realtime;
        }
        if self.first_entry_monotonic.is_none() {
            self.first_entry_monotonic = Some(monotonic);
        }

        header.tail_entry_seqnum = self.next_seqnum;
        header.tail_entry_realtime = realtime;
        header.tail_entry_monotonic = monotonic;
        header.tail_entry_boot_id = boot_id;
        header.tail_entry_offset = entry_offset.get();
        header.n_entries += 1;

        self.next_seqnum += 1;
        self.num_written_objects = 0;
    }

    fn add_data(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        payload: &[u8],
    ) -> Result<NonZeroU64> {
        let hash = journal_file.hash(payload);

        match journal_file.find_data_offset(hash, payload)? {
            Some(data_offset) => Ok(data_offset),
            None => {
                // We will have to write the new data object at the current
                // tail offset
                let data_offset = self.append_offset;
                let data_size = {
                    let mut data_guard =
                        journal_file.data_mut(data_offset, Some(payload.len() as u64))?;

                    data_guard.header.hash = hash;
                    data_guard.set_payload(payload);
                    data_guard.header.object_header.aligned_size()
                };

                self.object_added(data_offset, data_size);

                // Update hash table
                journal_file.data_hash_table_set_tail_offset(hash, data_offset)?;
                Self::update_data_hash_chain_depth(journal_file, hash)?;
                journal_file.journal_header_mut().n_data += 1;

                // Add the field object, if we have any
                if let Some(equals_pos) = payload.iter().position(|&b| b == b'=') {
                    let field_offset = self.add_field(journal_file, &payload[..equals_pos])?;

                    // Link data object to the linked-list
                    {
                        let head_data_offset = {
                            let field_guard = journal_file.field_ref(field_offset)?;
                            field_guard.header.head_data_offset
                        };

                        let mut data_guard = journal_file.data_mut(data_offset, None)?;
                        data_guard.header.next_field_offset = head_data_offset;
                    }

                    // Link field to the head of the linked list
                    {
                        let mut field_guard = journal_file.field_mut(field_offset, None)?;
                        field_guard.header.head_data_offset = Some(data_offset);
                    };
                }

                Ok(data_offset)
            }
        }
    }

    fn add_field(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        payload: &[u8],
    ) -> Result<NonZeroU64> {
        let hash = journal_file.hash(payload);

        match journal_file.find_field_offset(hash, payload)? {
            Some(field_offset) => Ok(field_offset),
            None => {
                // We will have to write the new field object at the current
                // tail offset
                let field_offset = self.append_offset;
                let field_size = {
                    let mut field_guard =
                        journal_file.field_mut(field_offset, Some(payload.len() as u64))?;

                    field_guard.header.hash = hash;
                    field_guard.set_payload(payload);
                    field_guard.header.object_header.aligned_size()
                };
                self.object_added(field_offset, field_size);

                // Update hash table
                journal_file.field_hash_table_set_tail_offset(hash, field_offset)?;
                let depth = Self::current_field_hash_chain_depth(journal_file, hash)?;
                let max_depth = journal_file
                    .journal_header_ref()
                    .field_hash_chain_depth
                    .max(depth);
                journal_file.journal_header_mut().field_hash_chain_depth = max_depth;
                journal_file.journal_header_mut().n_fields += 1;

                // Return the offset where we wrote the newly added data object
                Ok(field_offset)
            }
        }
    }

    fn allocate_new_array(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        capacity: NonZeroU64,
    ) -> Result<NonZeroU64> {
        // let new_capacity = previous_capacity.saturating_mul(NonZeroU64::new(2).unwrap());

        let array_offset = self.append_offset;
        let array_size = {
            let array_guard = journal_file.offset_array_mut(array_offset, Some(capacity))?;

            array_guard.header.object_header.aligned_size()
        };
        self.object_added(array_offset, array_size);
        journal_file.journal_header_mut().n_entry_arrays += 1;

        Ok(array_offset)
    }

    fn next_entry_array_capacity(index: u64, previous_capacity: u64) -> u64 {
        let mut capacity = previous_capacity;
        if index > capacity {
            capacity = (index + 1) * 2;
        } else {
            capacity *= 2;
        }
        capacity.max(4)
    }

    fn update_data_hash_chain_depth(
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

    fn current_data_hash_chain_depth(
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

    fn current_field_hash_chain_depth(
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

    fn create_initial_entry_array(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        entry_offset: NonZeroU64,
    ) -> Result<()> {
        let array_offset = self.allocate_new_array(journal_file, NonZeroU64::new(4).unwrap())?;
        let mut array_guard = journal_file.offset_array_mut(array_offset, None)?;
        array_guard.set(0, entry_offset)?;
        drop(array_guard);

        let header = journal_file.journal_header_mut();
        header.entry_array_offset = Some(array_offset);
        header.tail_entry_array_offset = array_offset.get() as u32;
        header.tail_entry_array_n_entries = 1;
        Ok(())
    }

    fn find_entry_array_tail(
        journal_file: &JournalFile<MmapMut>,
        entry_array_offset: NonZeroU64,
        entry_count: u64,
    ) -> Result<NonZeroU64> {
        let mut offset = entry_array_offset;
        let mut remaining = entry_count;
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

    fn entry_array_tail_offset(
        journal_file: &JournalFile<MmapMut>,
        entry_array_offset: NonZeroU64,
        entry_count: u64,
    ) -> Result<NonZeroU64> {
        let header_tail = NonZeroU64::new(
            journal_file
                .journal_header_ref()
                .tail_entry_array_offset
                .into(),
        );
        match header_tail {
            Some(offset) => Ok(offset),
            None => Self::find_entry_array_tail(journal_file, entry_array_offset, entry_count),
        }
    }

    fn entry_array_tail_entries(
        journal_file: &JournalFile<MmapMut>,
        entry_array_offset: NonZeroU64,
        tail_offset: NonZeroU64,
        entry_count: u64,
    ) -> Result<u64> {
        let tail_entries = journal_file.journal_header_ref().tail_entry_array_n_entries as u64;
        if tail_entries != 0 {
            return Ok(tail_entries);
        }

        let mut tail_entries = entry_count;
        let mut offset = entry_array_offset;
        while offset != tail_offset {
            let array_guard = journal_file.offset_array_ref(offset)?;
            tail_entries -= array_guard.capacity() as u64;
            offset = array_guard
                .header
                .next_offset_array
                .ok_or(JournalError::InvalidOffsetArrayOffset)?;
        }
        Ok(tail_entries)
    }

    fn append_to_existing_entry_array_tail(
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

    fn append_new_entry_array_tail(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        tail_offset: NonZeroU64,
        entry_count: u64,
        tail_capacity: u64,
        entry_offset: NonZeroU64,
    ) -> Result<()> {
        let new_capacity = Self::next_entry_array_capacity(entry_count, tail_capacity);
        let new_array_offset =
            self.allocate_new_array(journal_file, NonZeroU64::new(new_capacity).unwrap())?;
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

    fn append_to_entry_array(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        entry_offset: NonZeroU64,
    ) -> Result<()> {
        let entry_array_offset = journal_file.journal_header_ref().entry_array_offset;
        let Some(entry_array_offset) = entry_array_offset else {
            return self.create_initial_entry_array(journal_file, entry_offset);
        };
        let entry_count = journal_file.journal_header_ref().n_entries;
        let tail_offset =
            Self::entry_array_tail_offset(journal_file, entry_array_offset, entry_count)?;
        let tail_capacity = {
            let tail_guard = journal_file.offset_array_ref(tail_offset)?;
            tail_guard.capacity() as u64
        };
        let tail_entries = Self::entry_array_tail_entries(
            journal_file,
            entry_array_offset,
            tail_offset,
            entry_count,
        )?;

        if tail_entries < tail_capacity {
            return Self::append_to_existing_entry_array_tail(
                journal_file,
                tail_offset,
                tail_entries,
                entry_offset,
            );
        }

        self.append_new_entry_array_tail(
            journal_file,
            tail_offset,
            entry_count,
            tail_capacity,
            entry_offset,
        )
    }

    fn append_to_data_entry_array(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        mut array_offset: NonZeroU64,
        entry_offset: NonZeroU64,
        current_count: u64,
    ) -> Result<()> {
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
        }

        Ok(())
    }

    fn link_data_to_entry(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        entry_offset: NonZeroU64,
        entry_item_index: usize,
    ) -> Result<()> {
        let data_offset = self.entry_items[entry_item_index].offset;
        let mut data_guard = journal_file.data_mut(data_offset, None)?;

        match data_guard.header.n_entries {
            None => {
                data_guard.header.entry_offset = Some(entry_offset);
                data_guard.header.n_entries = NonZeroU64::new(1);
            }
            Some(n_entries) => {
                match n_entries.get() {
                    0 => {
                        unreachable!();
                    }
                    1 => {
                        drop(data_guard);

                        // Create new entry array with initial capacity
                        let array_capacity = NonZeroU64::new(4).unwrap();
                        let array_offset = self.allocate_new_array(journal_file, array_capacity)?;

                        // Load new array and set its first entry offset
                        {
                            let mut array_guard =
                                journal_file.offset_array_mut(array_offset, None)?;
                            array_guard.set(0, entry_offset)?;
                        }

                        // Update data object to point to the array
                        let mut data_guard = journal_file.data_mut(data_offset, None)?;
                        data_guard.header.entry_array_offset = Some(array_offset);
                        data_guard.header.n_entries = NonZeroU64::new(2);
                    }
                    x => {
                        // There's already an entry array, append to it
                        let current_count = x - 1;
                        let array_offset = data_guard.header.entry_array_offset.unwrap();

                        // Drop the data guard to avoid borrow conflicts
                        drop(data_guard);

                        // Find the tail of the entry array chain and append
                        self.append_to_data_entry_array(
                            journal_file,
                            array_offset,
                            entry_offset,
                            current_count,
                        )?;

                        // Update the count
                        let mut data_guard = journal_file.data_mut(data_offset, None)?;
                        data_guard.header.n_entries = NonZeroU64::new(x + 1);
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Direction, JournalFile, JournalReader, Location};
    use memmap2::Mmap;
    use std::collections::HashMap;
    use tempfile::NamedTempFile;

    fn generate_uuid() -> [u8; 16] {
        use rand::Rng;
        let mut rng = rand::rng();
        rng.random()
    }

    #[test]
    fn test_write_and_read_journal_entries() -> Result<()> {
        let test_data = journal_test_data();
        let temp_file = NamedTempFile::new().map_err(JournalError::Io).unwrap();
        let journal_path = temp_file.path();
        let boot_id = [1; 16];
        let num_entries = test_data.values().next().unwrap().len();
        let iterations = 5000;
        write_repeated_entries(journal_path, &test_data, iterations, boot_id)?;
        verify_written_entries(journal_path, &test_data, num_entries, iterations)?;
        verify_systemd_unit_filter(journal_path, iterations)?;

        println!("✅ All tests passed!");
        Ok(())
    }

    fn journal_test_data() -> HashMap<&'static str, Vec<&'static str>> {
        HashMap::from([
            (
                "MESSAGE",
                vec!["Hello, world!", "Another message", "Final message"],
            ),
            ("PRIORITY", vec!["6", "4", "3"]),
            (
                "_SYSTEMD_UNIT",
                vec!["test.service", "other.service", "test.service"],
            ),
            ("_PID", vec!["1234", "5678", "9999"]),
        ])
    }

    fn write_repeated_entries(
        journal_path: &std::path::Path,
        test_data: &HashMap<&'static str, Vec<&'static str>>,
        iterations: usize,
        boot_id: [u8; 16],
    ) -> Result<()> {
        let options = JournalFileOptions::new(
            generate_uuid(),
            generate_uuid(),
            generate_uuid(),
            generate_uuid(),
        );
        let mut journal_file = JournalFile::create(journal_path, options)?;
        let num_entries = test_data.values().next().unwrap().len();
        for _ in 0..iterations {
            write_one_iteration(&mut journal_file, test_data, num_entries, boot_id)?;
        }
        Ok(())
    }

    fn write_one_iteration(
        journal_file: &mut JournalFile<MmapMut>,
        test_data: &HashMap<&'static str, Vec<&'static str>>,
        num_entries: usize,
        boot_id: [u8; 16],
    ) -> Result<()> {
        let mut writer = JournalWriter::new(journal_file)?;
        for i in 0..num_entries {
            let entry_data = build_entry_data(test_data, i);
            let entry_refs: Vec<&[u8]> = entry_data.iter().map(|v| v.as_slice()).collect();
            let realtime = 1000000 + (i as u64 * 1000);
            let monotonic = 500000 + (i as u64 * 1000);
            writer.add_entry(journal_file, &entry_refs, realtime, monotonic, boot_id)?;
        }
        Ok(())
    }

    fn build_entry_data(
        test_data: &HashMap<&'static str, Vec<&'static str>>,
        index: usize,
    ) -> Vec<Vec<u8>> {
        let value_index = index;
        test_data
            .iter()
            .map(|(key, values)| format!("{}={}", key, values[value_index]).into_bytes())
            .collect()
    }

    fn verify_written_entries(
        journal_path: &std::path::Path,
        test_data: &HashMap<&'static str, Vec<&'static str>>,
        num_entries: usize,
        iterations: usize,
    ) -> Result<()> {
        let journal_file = JournalFile::<Mmap>::open(journal_path, 8 * 1024)?;
        let mut reader = JournalReader::default();

        println!("Header: {:#?}", journal_file.journal_header_ref());
        reader.set_location(Location::Head);

        let mut entries_read = 0;
        while reader.step(&journal_file, Direction::Forward)? {
            verify_read_entry(&journal_file, &mut reader, test_data, entries_read)?;
            entries_read += 1;
        }

        assert_eq!(
            entries_read as usize,
            num_entries * iterations,
            "Number of entries read doesn't match written"
        );
        Ok(())
    }

    fn verify_read_entry<'a>(
        journal_file: &'a JournalFile<Mmap>,
        reader: &mut JournalReader<'a, Mmap>,
        test_data: &HashMap<&'static str, Vec<&'static str>>,
        entries_read: u64,
    ) -> Result<()> {
        println!("Reading entry {}", entries_read);
        assert_eq!(
            reader.get_realtime_usec(journal_file)?,
            1000000 + ((entries_read % 3) * 1000),
            "Realtime mismatch for entry {}",
            entries_read
        );
        let (seqnum, _seqnum_id) = reader.get_seqnum(journal_file)?;
        assert_eq!(
            seqnum,
            entries_read + 1,
            "Sequence number mismatch for entry {}",
            entries_read
        );
        verify_entry_fields(
            &read_entry_fields(journal_file, reader)?,
            test_data,
            entries_read,
        );
        println!("Read entry {}", entries_read);
        Ok(())
    }

    fn read_entry_fields<'a>(
        journal_file: &'a JournalFile<Mmap>,
        reader: &mut JournalReader<'a, Mmap>,
    ) -> Result<HashMap<String, String>> {
        let mut entry_fields = HashMap::new();
        reader.entry_data_restart();
        while let Some(data_guard) = reader.entry_data_enumerate(journal_file)? {
            let payload_str = String::from_utf8_lossy(data_guard.payload_bytes());
            if let Some(eq_pos) = payload_str.find('=') {
                entry_fields.insert(
                    payload_str[..eq_pos].to_string(),
                    payload_str[eq_pos + 1..].to_string(),
                );
            }
        }
        Ok(entry_fields)
    }

    fn verify_entry_fields(
        entry_fields: &HashMap<String, String>,
        test_data: &HashMap<&'static str, Vec<&'static str>>,
        entries_read: u64,
    ) {
        for (key, values) in test_data {
            let expected_value = &values[entries_read as usize % 3];
            let actual_value = entry_fields
                .get(*key)
                .unwrap_or_else(|| panic!("Missing key '{}' in entry {}", key, entries_read));
            assert_eq!(
                actual_value, expected_value,
                "Value mismatch for key '{}' in entry {}",
                key, entries_read
            );
        }
    }

    fn verify_systemd_unit_filter(journal_path: &std::path::Path, iterations: usize) -> Result<()> {
        let journal_file = JournalFile::<Mmap>::open(journal_path, 64 * 1024)?;
        let mut reader = JournalReader::default();
        reader.add_match(b"_SYSTEMD_UNIT=test.service");
        reader.set_location(Location::Head);

        let mut filtered_entries = 0;
        while reader.step(&journal_file, Direction::Forward)? {
            assert!(
                entry_contains_payload(&journal_file, &mut reader, b"_SYSTEMD_UNIT=test.service")?,
                "Filtered entry doesn't contain the expected field"
            );
            filtered_entries += 1;
        }

        assert_eq!(
            filtered_entries,
            2 * iterations,
            "Expected 2 entries with _SYSTEMD_UNIT=test.service per iteration"
        );
        Ok(())
    }

    fn entry_contains_payload<'a>(
        journal_file: &'a JournalFile<Mmap>,
        reader: &mut JournalReader<'a, Mmap>,
        expected: &[u8],
    ) -> Result<bool> {
        reader.entry_data_restart();
        while let Some(data_guard) = reader.entry_data_enumerate(journal_file)? {
            if data_guard.payload_bytes() == expected {
                return Ok(true);
            }
        }
        Ok(false)
    }

    #[test]
    fn test_field_enumeration() -> Result<()> {
        // Create a simple journal with known fields
        let temp_file = NamedTempFile::new().map_err(JournalError::Io)?;
        let journal_path = temp_file.path();

        let test_fields = vec!["MESSAGE", "PRIORITY", "_SYSTEMD_UNIT"];
        let boot_id = [1; 16];

        // Write a single entry with multiple fields
        {
            let options = JournalFileOptions::new(
                generate_uuid(),
                generate_uuid(),
                generate_uuid(),
                generate_uuid(),
            );

            let mut journal_file = JournalFile::create(journal_path, options)?;
            let mut writer = JournalWriter::new(&mut journal_file)?;

            let entry_data = vec![
                b"MESSAGE=Test message".as_slice(),
                b"PRIORITY=6".as_slice(),
                b"_SYSTEMD_UNIT=test.service".as_slice(),
            ];

            writer.add_entry(&mut journal_file, &entry_data, 1000000, 500000, boot_id)?;
        }

        // Read back and enumerate fields
        {
            let journal_file = JournalFile::<Mmap>::open(journal_path, 8 * 1024)?;
            let mut reader = JournalReader::default();

            let mut found_fields = Vec::new();
            reader.fields_restart();

            while let Some(field_guard) = reader.fields_enumerate(&journal_file)? {
                let field_name = String::from_utf8_lossy(field_guard.payload);
                found_fields.push(field_name.to_string());
            }

            // Verify all expected fields were found
            for expected_field in &test_fields {
                assert!(
                    found_fields.contains(&expected_field.to_string()),
                    "Expected field '{}' not found. Found: {:?}",
                    expected_field,
                    found_fields
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_writer_rejects_unkeyed_journal_on_new_without_panic() -> Result<()> {
        let temp_file = NamedTempFile::new().map_err(JournalError::Io)?;
        let journal_path = temp_file.path();

        let options = JournalFileOptions::new(
            generate_uuid(),
            generate_uuid(),
            generate_uuid(),
            generate_uuid(),
        )
        .with_keyed_hash(false);
        let mut journal_file = JournalFile::create(journal_path, options)?;

        let err = match JournalWriter::new(&mut journal_file) {
            Ok(_) => panic!("legacy writer unexpectedly accepted unkeyed journal"),
            Err(err) => err,
        };
        assert!(matches!(err, JournalError::UnsupportedJournalFile));
        assert_eq!(journal_file.journal_header_ref().n_entries, 0);
        assert_eq!(journal_file.journal_header_ref().tail_entry_seqnum, 0);

        Ok(())
    }

    #[test]
    fn test_writer_rejects_unkeyed_journal_on_add_entry_without_mutation() -> Result<()> {
        let temp_file = NamedTempFile::new().map_err(JournalError::Io)?;
        let journal_path = temp_file.path();

        let options = JournalFileOptions::new(
            generate_uuid(),
            generate_uuid(),
            generate_uuid(),
            generate_uuid(),
        );
        let mut journal_file = JournalFile::create(journal_path, options)?;
        let mut writer = JournalWriter::new(&mut journal_file)?;

        journal_file.journal_header_mut().incompatible_flags &=
            !(HeaderIncompatibleFlags::KeyedHash as u32);
        let before_entries = journal_file.journal_header_ref().n_entries;
        let before_tail_seqnum = journal_file.journal_header_ref().tail_entry_seqnum;
        let before_tail_object = journal_file.journal_header_ref().tail_object_offset;

        let entry_data = [b"MESSAGE=blocked".as_slice()];
        let err = writer
            .add_entry(&mut journal_file, &entry_data, 1000000, 500000, [1; 16])
            .unwrap_err();

        assert!(matches!(err, JournalError::UnsupportedJournalFile));
        assert_eq!(journal_file.journal_header_ref().n_entries, before_entries);
        assert_eq!(
            journal_file.journal_header_ref().tail_entry_seqnum,
            before_tail_seqnum
        );
        assert_eq!(
            journal_file.journal_header_ref().tail_object_offset,
            before_tail_object
        );

        Ok(())
    }
}
