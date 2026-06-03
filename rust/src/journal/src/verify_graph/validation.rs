use super::io::*;
use super::*;

impl<'a> GraphVerifier<'a> {
    pub(super) fn validate_header_counts(&self) -> Result<(), String> {
        let expected = [
            (
                "n_objects",
                self.order.len() as u64,
                self.header.n_objects,
                152,
            ),
            (
                "n_entries",
                self.counts[OBJECT_TYPE_ENTRY as usize],
                self.header.n_entries,
                160,
            ),
            (
                "n_data",
                self.counts[OBJECT_TYPE_DATA as usize],
                self.header.n_data,
                216,
            ),
            (
                "n_fields",
                self.counts[OBJECT_TYPE_FIELD as usize],
                self.header.n_fields,
                224,
            ),
            (
                "n_tags",
                self.counts[OBJECT_TYPE_TAG as usize],
                self.header.n_tags,
                232,
            ),
            (
                "n_entry_arrays",
                self.counts[OBJECT_TYPE_ENTRY_ARRAY as usize],
                self.header.n_entry_arrays,
                240,
            ),
        ];
        for (name, walked, header_value, end) in expected {
            if header_contains_field(self.data, self.header.header_size, end)
                && walked != header_value
            {
                return Err(format!(
                    "header {name} mismatch: got {header_value}, walked {walked}"
                ));
            }
        }
        Ok(())
    }

    pub(super) fn validate_main_entry_array_presence(&self) -> Result<(), String> {
        if self.header.entry_array_offset != 0 && !self.main_entry_array_found {
            return Err("missing main entry array".to_string());
        }
        if self.header.n_entries != 0 && self.header.entry_array_offset == 0 {
            return Err("entry_array_offset is zero with entries recorded".to_string());
        }
        Ok(())
    }

    pub(super) fn validate_tail_metadata(&self) -> Result<(), String> {
        if self.entry_objects.is_empty() {
            return self.validate_empty_tail_metadata();
        }
        let (head_offset, head) = self.entry_by_min_seqnum()?;
        let (tail_offset, tail) = self.entry_by_max_seqnum()?;
        self.validate_head_tail_entry_numbers(head, tail)?;
        self.validate_head_tail_entry_times(head, tail)?;
        self.validate_tail_boot_metadata(tail)?;
        self.validate_tail_entry_offset(tail_offset)?;
        self.validate_head_entry_offset(head_offset)
    }

    pub(super) fn validate_empty_tail_metadata(&self) -> Result<(), String> {
        if self.header.n_entries != 0 {
            return Err("entries recorded but no ENTRY objects found".to_string());
        }
        Ok(())
    }

    pub(super) fn entry_by_min_seqnum(&self) -> Result<(u64, &EntryObject), String> {
        self.entry_objects
            .iter()
            .min_by_key(|(_, entry)| entry.seqnum)
            .map(|(offset, entry)| (*offset, entry))
            .ok_or_else(|| "missing head entry".to_string())
    }

    pub(super) fn entry_by_max_seqnum(&self) -> Result<(u64, &EntryObject), String> {
        self.entry_objects
            .iter()
            .max_by_key(|(_, entry)| entry.seqnum)
            .map(|(offset, entry)| (*offset, entry))
            .ok_or_else(|| "missing tail entry".to_string())
    }

    pub(super) fn validate_head_tail_entry_numbers(
        &self,
        head: &EntryObject,
        tail: &EntryObject,
    ) -> Result<(), String> {
        if self.header.head_entry_seqnum != head.seqnum {
            return Err("head_entry_seqnum mismatch".to_string());
        }
        if self.header.tail_entry_seqnum != tail.seqnum {
            return Err("tail_entry_seqnum mismatch".to_string());
        }
        Ok(())
    }

    pub(super) fn validate_head_tail_entry_times(
        &self,
        head: &EntryObject,
        tail: &EntryObject,
    ) -> Result<(), String> {
        if self.header.head_entry_realtime != head.realtime {
            return Err("head_entry_realtime mismatch".to_string());
        }
        if self.header.tail_entry_realtime != tail.realtime {
            return Err("tail_entry_realtime mismatch".to_string());
        }
        Ok(())
    }

    pub(super) fn validate_tail_boot_metadata(&self, tail: &EntryObject) -> Result<(), String> {
        if self.header.compatible_flags & COMPATIBLE_TAIL_ENTRY_BOOT_ID == 0 {
            return Ok(());
        }
        if self.header.tail_entry_monotonic != tail.monotonic {
            return Err("tail_entry_monotonic mismatch".to_string());
        }
        if self.header.tail_entry_boot_id != tail.boot_id {
            return Err("tail_entry_boot_id mismatch".to_string());
        }
        Ok(())
    }

    pub(super) fn validate_tail_entry_offset(&self, tail_offset: u64) -> Result<(), String> {
        if header_contains_field(self.data, self.header.header_size, 272)
            && self.header.tail_entry_offset != tail_offset
        {
            return Err("tail_entry_offset mismatch".to_string());
        }
        Ok(())
    }

    pub(super) fn validate_head_entry_offset(&self, head_offset: u64) -> Result<(), String> {
        if head_offset == 0 {
            return Err("head entry offset is zero".to_string());
        }
        Ok(())
    }

    pub(super) fn validate_global_entry_array(&self) -> Result<(), String> {
        let entries = self.walk_entry_array_chain(
            self.header.entry_array_offset,
            self.header.n_entries,
            "global entry array",
        )?;
        if entries.len() as u64 != self.header.n_entries {
            return Err("global entry array count mismatch".to_string());
        }
        let mut last = 0;
        for (idx, entry_offset) in entries.iter().enumerate() {
            if *entry_offset <= last {
                return Err("global entry array is not sorted".to_string());
            }
            if !self.entry_objects.contains_key(entry_offset) {
                return Err("global entry array references missing ENTRY".to_string());
            }
            last = *entry_offset;
            self.validate_entry_data_links(*entry_offset, idx + 1 == entries.len())?;
        }
        Ok(())
    }

    pub(super) fn validate_data_hash_table(&self) -> Result<(), String> {
        let table_offset = self.header.data_hash_table_offset;
        let table_size = self.header.data_hash_table_size;
        if table_offset == 0 || table_size == 0 {
            return Ok(());
        }
        let bucket_count = table_size / HASH_ITEM_SIZE;
        for bucket_index in 0..bucket_count {
            self.validate_data_hash_bucket(table_offset, bucket_count, bucket_index)?;
        }
        Ok(())
    }

    pub(super) fn validate_data_hash_bucket(
        &self,
        table_offset: u64,
        bucket_count: u64,
        bucket_index: u64,
    ) -> Result<(), String> {
        let item_offset = table_offset + bucket_index * HASH_ITEM_SIZE;
        let mut current = u64_at_u64(self.data, item_offset)?;
        let tail = u64_at_u64(self.data, item_offset + 8)?;
        let mut last = 0;
        let mut seen = HashSet::new();
        while current != 0 {
            current = self.validate_data_hash_bucket_item(
                current,
                bucket_count,
                bucket_index,
                &mut seen,
                &mut last,
            )?;
        }
        if last != tail {
            return Err("data hash bucket tail mismatch".to_string());
        }
        Ok(())
    }

    pub(super) fn validate_data_hash_bucket_item(
        &self,
        current: u64,
        bucket_count: u64,
        bucket_index: u64,
        seen: &mut HashSet<u64>,
        last: &mut u64,
    ) -> Result<u64, String> {
        if !seen.insert(current) {
            return Err("data hash chain cycle".to_string());
        }
        let obj = self
            .data_objects
            .get(&current)
            .ok_or_else(|| "data hash chain references missing DATA".to_string())?;
        if obj.hash % bucket_count != bucket_index {
            return Err("data hash bucket mismatch".to_string());
        }
        self.validate_data_entry_array(current, obj)?;
        if obj.next_hash_offset != 0 && obj.next_hash_offset <= current {
            return Err("data hash chain points backwards".to_string());
        }
        *last = current;
        Ok(obj.next_hash_offset)
    }

    pub(super) fn validate_entry_data_links(
        &self,
        entry_offset: u64,
        last_entry: bool,
    ) -> Result<(), String> {
        let entry = self
            .entry_objects
            .get(&entry_offset)
            .ok_or_else(|| "entry is missing".to_string())?;
        for data_offset in &entry.items {
            let data = self
                .data_objects
                .get(data_offset)
                .ok_or_else(|| "entry references missing DATA object".to_string())?;
            if !self.data_object_in_hash_table(*data_offset, data.hash) {
                return Err("entry DATA object missing from hash table".to_string());
            }
            if !self.data_references_entry(data, entry_offset)? && !last_entry {
                return Err("entry not referenced by linked DATA object".to_string());
            }
        }
        Ok(())
    }

    pub(super) fn validate_data_entry_array(
        &self,
        data_offset: u64,
        data: &DataObject,
    ) -> Result<(), String> {
        if data.n_entries == 0 {
            return Ok(());
        }
        if !self.entry_objects.contains_key(&data.entry_offset) {
            return Err("DATA inline entry is missing".to_string());
        }
        let mut last = data.entry_offset;
        if data.entry_array_offset != 0 && data.n_entries < 2 {
            return Err("DATA entry array present with fewer than two entries".to_string());
        }
        for entry_offset in self.walk_entry_array_chain(
            data.entry_array_offset,
            data.n_entries - 1,
            &format!("DATA {data_offset} entry array"),
        )? {
            if entry_offset <= last {
                return Err("DATA entry array is not sorted".to_string());
            }
            last = entry_offset;
        }
        Ok(())
    }

    pub(super) fn walk_entry_array_chain(
        &self,
        start_offset: u64,
        used_count: u64,
        label: &str,
    ) -> Result<Vec<u64>, String> {
        if let Some(empty) = self.empty_entry_array_chain(start_offset, used_count, label)? {
            return Ok(empty);
        }
        let mut entries = Vec::new();
        let mut remaining = used_count;
        let mut current = start_offset;
        let mut seen = HashSet::new();
        while remaining > 0 {
            let array = self.entry_array_chain_item(current, label, &mut seen)?;
            let used_here =
                self.copy_used_entry_array_items(array, remaining, label, &mut entries)?;
            remaining -= used_here;
            if remaining == 0 {
                break;
            }
            if array.next == 0 {
                return Err(format!("{label} ended early"));
            }
            current = array.next;
        }
        Ok(entries)
    }

    pub(super) fn empty_entry_array_chain(
        &self,
        start_offset: u64,
        used_count: u64,
        label: &str,
    ) -> Result<Option<Vec<u64>>, String> {
        if used_count != 0 {
            if start_offset == 0 {
                return Err(format!("{label} is missing"));
            }
            return Ok(None);
        }
        if start_offset != 0 {
            return Err(format!("{label} has start offset with zero entries"));
        }
        Ok(Some(Vec::new()))
    }

    pub(super) fn entry_array_chain_item<'b>(
        &'b self,
        current: u64,
        label: &str,
        seen: &mut HashSet<u64>,
    ) -> Result<&'b EntryArray, String> {
        if !seen.insert(current) {
            return Err(format!("{label} has a cycle"));
        }
        let array = self
            .entry_arrays
            .get(&current)
            .ok_or_else(|| format!("{label} references missing ENTRY_ARRAY"))?;
        if array.next != 0 && array.next <= current {
            return Err(format!("{label} next pointer is not increasing"));
        }
        Ok(array)
    }

    pub(super) fn copy_used_entry_array_items(
        &self,
        array: &EntryArray,
        remaining: u64,
        label: &str,
        entries: &mut Vec<u64>,
    ) -> Result<u64, String> {
        let used_here = remaining.min(array.items.len() as u64);
        for idx in 0..used_here as usize {
            self.copy_used_entry_array_item(array.items[idx], label, entries)?;
        }
        Ok(used_here)
    }

    pub(super) fn copy_used_entry_array_item(
        &self,
        item: u64,
        label: &str,
        entries: &mut Vec<u64>,
    ) -> Result<(), String> {
        if item == 0 {
            return Err(format!("{label} has zero used item"));
        }
        if !self.entry_objects.contains_key(&item) {
            return Err(format!("{label} references missing ENTRY"));
        }
        entries.push(item);
        Ok(())
    }

    pub(super) fn data_object_in_hash_table(&self, data_offset: u64, data_hash: u64) -> bool {
        let table_offset = self.header.data_hash_table_offset;
        let table_size = self.header.data_hash_table_size;
        if table_offset == 0 || table_size == 0 {
            return false;
        }
        let bucket_count = table_size / HASH_ITEM_SIZE;
        let bucket = data_hash % bucket_count;
        let mut current = match u64_at_u64(self.data, table_offset + bucket * HASH_ITEM_SIZE) {
            Ok(value) => value,
            Err(_) => return false,
        };
        let mut seen = HashSet::new();
        while current != 0 {
            if !seen.insert(current) {
                return false;
            }
            if current == data_offset {
                return true;
            }
            let Some(obj) = self.data_objects.get(&current) else {
                return false;
            };
            current = obj.next_hash_offset;
        }
        false
    }

    pub(super) fn data_references_entry(
        &self,
        data: &DataObject,
        entry_offset: u64,
    ) -> Result<bool, String> {
        if data.entry_offset == entry_offset {
            return Ok(true);
        }
        for item in self.walk_entry_array_chain(
            data.entry_array_offset,
            data.n_entries.saturating_sub(1),
            "DATA entry array lookup",
        )? {
            if item == entry_offset {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn valid_offset(&self, offset: u64, label: &str) -> Result<(), String> {
        if offset == 0 {
            return Ok(());
        }
        if offset % 8 != 0 {
            return Err(format!("{label} offset {offset} is not aligned"));
        }
        if offset < self.header.header_size || offset > self.header.tail_object_offset {
            return Err(format!("{label} offset {offset} outside object range"));
        }
        Ok(())
    }
}
