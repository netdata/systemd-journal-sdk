use super::io::*;
use super::*;

impl<'a> GraphVerifier<'a> {
    pub(super) fn walk_objects(&mut self) -> Result<(), String> {
        let Some(tail) = self.object_walk_tail()? else {
            return Ok(());
        };

        let mut offset = self.header.header_size;
        let mut state = ObjectWalkState::default();
        loop {
            self.validate_walk_offset(offset, tail)?;
            let (obj, aligned_size) = self.read_walk_object(offset)?;
            self.record_walk_object(offset, obj);
            self.parse_walk_object(offset, obj, &mut state)?;
            if offset == tail {
                break;
            }
            offset = offset
                .checked_add(aligned_size)
                .ok_or_else(|| "object offset overflow".to_string())?;
        }

        self.validate_walk_result(tail, &state)
    }

    pub(super) fn object_walk_tail(&self) -> Result<Option<u64>, String> {
        let tail = self.header.tail_object_offset;
        if tail == 0 {
            if self.header.n_objects != 0 {
                return Err("tail_object_offset is zero with objects recorded".to_string());
            }
            return Ok(None);
        }
        if tail < self.header.header_size {
            return Err("tail_object_offset is before header_size".to_string());
        }
        Ok(Some(tail))
    }

    pub(super) fn validate_walk_offset(&self, offset: u64, tail: u64) -> Result<(), String> {
        if offset > tail {
            return Err("object walk skipped past tail_object_offset".to_string());
        }
        let max_header_offset = self
            .source
            .len()
            .checked_sub(OBJECT_HEADER_SIZE)
            .ok_or_else(|| "file too small for object header".to_string())?;
        if offset > max_header_offset {
            return Err(format!(
                "object header at offset {offset} exceeds file bounds"
            ));
        }
        Ok(())
    }

    pub(super) fn read_walk_object(&self, offset: u64) -> Result<(ObjectHeader, u64), String> {
        let obj = ObjectHeader {
            typ: byte_at(self.source, offset)?,
            flags: byte_at(self.source, offset + 1)?,
            size: u64_at_u64(self.source, offset + 8)?,
        };
        let aligned_size = align8_checked(obj.size).ok_or_else(|| {
            format!(
                "object size {} overflows alignment at offset {offset}",
                obj.size
            )
        })?;
        self.validate_walk_object_envelope(offset, obj, aligned_size)?;
        self.validate_walk_object_flags(offset, obj)?;
        Ok((obj, aligned_size))
    }

    pub(super) fn validate_walk_object_envelope(
        &self,
        offset: u64,
        obj: ObjectHeader,
        aligned_size: u64,
    ) -> Result<(), String> {
        if obj.typ == 0 && obj.size == 0 {
            return Err(format!("zero object before tail at offset {offset}"));
        }
        if !(OBJECT_TYPE_DATA..=OBJECT_TYPE_TAG).contains(&obj.typ) {
            return Err(format!(
                "unknown object type {} at offset {offset}",
                obj.typ
            ));
        }
        if obj.size < OBJECT_HEADER_SIZE {
            return Err(format!(
                "object size {} too small at offset {offset}",
                obj.size
            ));
        }
        if aligned_size == 0 || aligned_size > self.source.len() - offset {
            return Err(format!("object at offset {offset} exceeds file bounds"));
        }
        if offset % 8 != 0 {
            return Err(format!("object offset {offset} is not aligned"));
        }
        Ok(())
    }

    pub(super) fn validate_walk_object_flags(
        &self,
        offset: u64,
        obj: ObjectHeader,
    ) -> Result<(), String> {
        let flags = obj.flags;
        if flags & !OBJECT_COMPRESSED_MASK != 0 {
            return Err(format!(
                "object at offset {offset} has unknown flags 0x{flags:x}"
            ));
        }
        if (flags & OBJECT_COMPRESSED_MASK).count_ones() > 1 {
            return Err(format!(
                "object at offset {offset} has multiple compression flags"
            ));
        }
        if obj.typ != OBJECT_TYPE_DATA && flags != 0 {
            return Err(format!(
                "object type {} at offset {offset} has compression flags",
                obj.typ
            ));
        }
        self.validate_compressed_data_header_flag(offset, flags)
    }

    pub(super) fn validate_compressed_data_header_flag(
        &self,
        offset: u64,
        flags: u8,
    ) -> Result<(), String> {
        if flags & OBJECT_COMPRESSED_XZ != 0
            && self.header.incompatible_flags & INCOMPATIBLE_COMPRESSED_XZ == 0
        {
            return Err(format!(
                "XZ DATA object without matching header flag at offset {offset}"
            ));
        }
        if flags & OBJECT_COMPRESSED_LZ4 != 0
            && self.header.incompatible_flags & INCOMPATIBLE_COMPRESSED_LZ4 == 0
        {
            return Err(format!(
                "LZ4 DATA object without matching header flag at offset {offset}"
            ));
        }
        if flags & OBJECT_COMPRESSED_ZSTD != 0
            && self.header.incompatible_flags & INCOMPATIBLE_COMPRESSED_ZSTD == 0
        {
            return Err(format!(
                "ZSTD DATA object without matching header flag at offset {offset}"
            ));
        }
        Ok(())
    }

    pub(super) fn record_walk_object(&mut self, offset: u64, obj: ObjectHeader) {
        self.spans.insert(offset, obj);
        self.order.push(offset);
        self.counts[obj.typ as usize] += 1;
    }

    pub(super) fn parse_walk_object(
        &mut self,
        offset: u64,
        obj: ObjectHeader,
        state: &mut ObjectWalkState,
    ) -> Result<(), String> {
        match obj.typ {
            OBJECT_TYPE_DATA => self.parse_data(offset, obj),
            OBJECT_TYPE_FIELD => self.parse_field(offset, obj),
            OBJECT_TYPE_ENTRY => self.parse_walk_entry(offset, obj, state),
            OBJECT_TYPE_DATA_HASH_TABLE | OBJECT_TYPE_FIELD_HASH_TABLE => {
                self.parse_hash_table(offset, obj)
            }
            OBJECT_TYPE_ENTRY_ARRAY => self.parse_walk_entry_array(offset, obj),
            OBJECT_TYPE_TAG => self.parse_walk_tag(offset, obj, state),
            _ => unreachable!(),
        }
    }

    pub(super) fn parse_walk_entry(
        &mut self,
        offset: u64,
        obj: ObjectHeader,
        state: &mut ObjectWalkState,
    ) -> Result<(), String> {
        let entry = self.parse_entry(offset, obj)?;
        self.validate_entry_tag_order(offset, &entry, state)?;
        self.validate_entry_seqnum_order(offset, &entry, state)?;
        self.validate_entry_monotonic_order(offset, &entry, state)?;
        self.validate_entry_realtime_order(offset, &entry, state)?;
        Ok(())
    }

    pub(super) fn validate_entry_tag_order(
        &self,
        offset: u64,
        entry: &EntryObject,
        state: &ObjectWalkState,
    ) -> Result<(), String> {
        if self.header.compatible_flags & COMPATIBLE_SEALED != 0
            && self.counts[OBJECT_TYPE_TAG as usize] == 0
        {
            return Err(format!("first entry before first tag at offset {offset}"));
        }
        if entry.realtime < state.last_tag_realtime {
            return Err(format!("older entry after newer tag at offset {offset}"));
        }
        Ok(())
    }

    pub(super) fn validate_entry_seqnum_order(
        &self,
        offset: u64,
        entry: &EntryObject,
        state: &mut ObjectWalkState,
    ) -> Result<(), String> {
        if !state.entry_seqnum_set && entry.seqnum != self.header.head_entry_seqnum {
            return Err(format!("head entry seqnum mismatch at offset {offset}"));
        }
        if state.entry_seqnum_set && state.entry_seqnum >= entry.seqnum {
            return Err(format!("entry seqnum out of sync at offset {offset}"));
        }
        state.entry_seqnum = entry.seqnum;
        state.entry_seqnum_set = true;
        Ok(())
    }

    pub(super) fn validate_entry_monotonic_order(
        &self,
        offset: u64,
        entry: &EntryObject,
        state: &mut ObjectWalkState,
    ) -> Result<(), String> {
        if state.entry_monotonic_set
            && entry.boot_id == state.entry_boot_id
            && state.entry_monotonic > entry.monotonic
        {
            return Err(format!("entry monotonic out of sync at offset {offset}"));
        }
        state.entry_monotonic = entry.monotonic;
        state.entry_boot_id = entry.boot_id;
        state.entry_monotonic_set = true;
        Ok(())
    }

    pub(super) fn validate_entry_realtime_order(
        &self,
        offset: u64,
        entry: &EntryObject,
        state: &mut ObjectWalkState,
    ) -> Result<(), String> {
        if !state.entry_realtime_set && entry.realtime != self.header.head_entry_realtime {
            return Err(format!("head entry realtime mismatch at offset {offset}"));
        }
        state.entry_realtime = entry.realtime;
        state.entry_realtime_set = true;
        Ok(())
    }

    pub(super) fn parse_walk_entry_array(
        &mut self,
        offset: u64,
        obj: ObjectHeader,
    ) -> Result<(), String> {
        self.parse_entry_array(offset, obj)?;
        if offset != self.header.entry_array_offset {
            return Ok(());
        }
        if self.main_entry_array_found {
            return Err("more than one main entry array".to_string());
        }
        self.main_entry_array_found = true;
        Ok(())
    }

    pub(super) fn parse_walk_tag(
        &self,
        offset: u64,
        obj: ObjectHeader,
        state: &mut ObjectWalkState,
    ) -> Result<(), String> {
        if self.header.compatible_flags & COMPATIBLE_SEALED == 0 {
            return Err("TAG object in unsealed file".to_string());
        }
        if obj.size != TAG_OBJECT_SIZE {
            return Err(format!("invalid TAG size at offset {offset}"));
        }
        let seqnum = u64_at_u64(self.source, offset + 16)?;
        if seqnum != self.counts[OBJECT_TYPE_TAG as usize] {
            return Err(format!("TAG seqnum mismatch at offset {offset}"));
        }
        if state.entry_realtime_set {
            state.last_tag_realtime = state.entry_realtime;
        }
        Ok(())
    }

    pub(super) fn validate_walk_result(
        &self,
        tail: u64,
        state: &ObjectWalkState,
    ) -> Result<(), String> {
        if self.order.last().copied() != Some(tail) {
            return Err("tail_object_offset does not point to walked tail".to_string());
        }
        if state.entry_seqnum_set && state.entry_seqnum != self.header.tail_entry_seqnum {
            return Err("tail_entry_seqnum mismatch".to_string());
        }
        if state.entry_monotonic_set
            && self.header.compatible_flags & COMPATIBLE_TAIL_ENTRY_BOOT_ID != 0
            && state.entry_boot_id == self.header.tail_entry_boot_id
            && state.entry_monotonic != self.header.tail_entry_monotonic
        {
            return Err("tail_entry_monotonic mismatch".to_string());
        }
        if state.entry_realtime_set && state.entry_realtime != self.header.tail_entry_realtime {
            return Err("tail_entry_realtime mismatch".to_string());
        }
        Ok(())
    }

    pub(super) fn parse_data(&mut self, offset: u64, obj: ObjectHeader) -> Result<(), String> {
        let payload = self.data_payload(offset, obj)?;
        self.validate_data_hash(offset, payload.as_ref())?;
        let data = self.read_data_object(offset)?;
        self.validate_data_object(offset, &data)?;
        self.data_objects.insert(offset, data);
        Ok(())
    }

    pub(super) fn data_payload(&self, offset: u64, obj: ObjectHeader) -> Result<Vec<u8>, String> {
        let payload_offset = if self.compact {
            COMPACT_DATA_OBJECT_HEADER_SIZE
        } else {
            DATA_OBJECT_HEADER_SIZE
        };
        if obj.size <= payload_offset {
            return Err(format!("DATA object at offset {offset} has no payload"));
        }
        let payload = slice_u64(self.source, offset + payload_offset, offset + obj.size)?;
        if obj.flags == 0 {
            return Ok(payload);
        }
        decompress_payload(obj.flags, &payload)
            .map_err(|err| format!("DATA decompression failed at offset {offset}: {err}"))
    }

    pub(super) fn validate_data_hash(
        &self,
        offset: u64,
        hash_payload: &[u8],
    ) -> Result<(), String> {
        let stored_hash = u64_at_u64(self.source, offset + 16)?;
        let computed_hash = self.hash(hash_payload);
        if stored_hash == computed_hash {
            return Ok(());
        }
        Err(format!(
            "DATA hash mismatch at offset {offset}: {stored_hash:#x} != {computed_hash:#x}"
        ))
    }

    pub(super) fn read_data_object(&self, offset: u64) -> Result<DataObject, String> {
        let entry_offset = u64_at_u64(self.source, offset + 40)?;
        Ok(DataObject {
            hash: u64_at_u64(self.source, offset + 16)?,
            next_hash_offset: u64_at_u64(self.source, offset + 24)?,
            next_field_offset: u64_at_u64(self.source, offset + 32)?,
            entry_offset,
            entry_array_offset: u64_at_u64(self.source, offset + 48)?,
            n_entries: u64_at_u64(self.source, offset + 56)?,
        })
    }

    pub(super) fn validate_data_object(
        &self,
        offset: u64,
        data: &DataObject,
    ) -> Result<(), String> {
        if (data.entry_offset == 0) != (data.n_entries == 0) {
            return Err(format!("DATA object at offset {offset} has bad n_entries"));
        }
        self.valid_offset(data.next_hash_offset, "DATA next_hash_offset")?;
        self.valid_offset(data.next_field_offset, "DATA next_field_offset")?;
        self.valid_offset(data.entry_offset, "DATA entry_offset")?;
        self.valid_offset(data.entry_array_offset, "DATA entry_array_offset")?;
        self.validate_data_entry_array_presence(offset, data)
    }

    pub(super) fn validate_data_entry_array_presence(
        &self,
        offset: u64,
        data: &DataObject,
    ) -> Result<(), String> {
        if data.n_entries < 2 && data.entry_array_offset != 0 {
            return Err(format!(
                "DATA object at offset {offset} has unexpected entry array"
            ));
        }
        if data.n_entries >= 2 && data.entry_array_offset == 0 {
            return Err(format!(
                "DATA object at offset {offset} is missing entry array"
            ));
        }
        Ok(())
    }

    pub(super) fn parse_entry(
        &mut self,
        offset: u64,
        obj: ObjectHeader,
    ) -> Result<EntryObject, String> {
        let item_size = self.entry_item_size();
        self.validate_entry_size(offset, obj, item_size)?;
        let mut entry = self.read_entry_object(offset)?;
        self.read_entry_items(offset, obj, item_size, &mut entry)?;
        self.validate_entry_items(offset, &entry)?;
        self.entry_objects.insert(offset, entry.clone());
        Ok(entry)
    }

    pub(super) fn entry_item_size(&self) -> u64 {
        if self.compact {
            COMPACT_ENTRY_ITEM_SIZE
        } else {
            REGULAR_ENTRY_ITEM_SIZE
        }
    }

    pub(super) fn validate_entry_size(
        &self,
        offset: u64,
        obj: ObjectHeader,
        item_size: u64,
    ) -> Result<(), String> {
        if obj.size < ENTRY_OBJECT_HEADER_SIZE {
            return Err(format!("ENTRY object at offset {offset} is too small"));
        }
        if (obj.size - ENTRY_OBJECT_HEADER_SIZE) % item_size != 0 {
            return Err(format!(
                "ENTRY object at offset {offset} has unaligned items"
            ));
        }
        Ok(())
    }

    pub(super) fn read_entry_object(&self, offset: u64) -> Result<EntryObject, String> {
        let entry = EntryObject {
            seqnum: u64_at_u64(self.source, offset + 16)?,
            realtime: u64_at_u64(self.source, offset + 24)?,
            monotonic: u64_at_u64(self.source, offset + 32)?,
            boot_id: bytes16_at_u64(self.source, offset + 40)?,
            items: Vec::new(),
        };
        if entry.seqnum == 0 {
            return Err(format!("ENTRY object at offset {offset} has zero seqnum"));
        }
        if entry.realtime == 0 {
            return Err(format!("ENTRY object at offset {offset} has zero realtime"));
        }
        Ok(entry)
    }

    pub(super) fn read_entry_items(
        &self,
        offset: u64,
        obj: ObjectHeader,
        item_size: u64,
        entry: &mut EntryObject,
    ) -> Result<(), String> {
        let mut item_offset = offset + ENTRY_OBJECT_HEADER_SIZE;
        while item_offset < offset + obj.size {
            let item = self.read_entry_item(item_offset)?;
            if item == 0 {
                return Err(format!("ENTRY object at offset {offset} has zero item"));
            }
            self.valid_offset(item, "ENTRY item")?;
            entry.items.push(item);
            item_offset += item_size;
        }
        Ok(())
    }

    pub(super) fn read_entry_item(&self, item_offset: u64) -> Result<u64, String> {
        if self.compact {
            return Ok(u32_at_u64(self.source, item_offset)? as u64);
        }
        u64_at_u64(self.source, item_offset)
    }

    pub(super) fn validate_entry_items(
        &self,
        offset: u64,
        entry: &EntryObject,
    ) -> Result<(), String> {
        if entry.items.is_empty() {
            return Err(format!("ENTRY object at offset {offset} has no items"));
        }
        Ok(())
    }

    pub(super) fn parse_field(&mut self, offset: u64, obj: ObjectHeader) -> Result<(), String> {
        if obj.size <= FIELD_OBJECT_HEADER_SIZE {
            return Err(format!("FIELD object at offset {offset} has no payload"));
        }
        let payload = slice_u64(
            self.source,
            offset + FIELD_OBJECT_HEADER_SIZE,
            offset + obj.size,
        )?;
        let stored_hash = u64_at_u64(self.source, offset + 16)?;
        let computed_hash = self.hash(&payload);
        if stored_hash != computed_hash {
            return Err(format!(
                "FIELD hash mismatch at offset {offset}: {stored_hash:#x} != {computed_hash:#x}"
            ));
        }
        self.valid_offset(
            u64_at_u64(self.source, offset + 24)?,
            "FIELD next_hash_offset",
        )?;
        self.valid_offset(
            u64_at_u64(self.source, offset + 32)?,
            "FIELD head_data_offset",
        )?;
        Ok(())
    }

    pub(super) fn parse_hash_table(&self, offset: u64, obj: ObjectHeader) -> Result<(), String> {
        if obj.size < OBJECT_HEADER_SIZE + HASH_ITEM_SIZE {
            return Err(format!("hash table at offset {offset} is too small"));
        }
        if (obj.size - OBJECT_HEADER_SIZE) % HASH_ITEM_SIZE != 0 {
            return Err(format!("hash table at offset {offset} has unaligned items"));
        }
        let (table_offset, table_size) = if obj.typ == OBJECT_TYPE_DATA_HASH_TABLE {
            (
                self.header.data_hash_table_offset,
                self.header.data_hash_table_size,
            )
        } else {
            (
                self.header.field_hash_table_offset,
                self.header.field_hash_table_size,
            )
        };
        if table_offset != offset + OBJECT_HEADER_SIZE {
            return Err(format!(
                "hash table header offset mismatch at offset {offset}"
            ));
        }
        if table_size != obj.size - OBJECT_HEADER_SIZE {
            return Err(format!(
                "hash table header size mismatch at offset {offset}"
            ));
        }
        let mut item_offset = offset + OBJECT_HEADER_SIZE;
        while item_offset < offset + obj.size {
            let head = u64_at_u64(self.source, item_offset)?;
            let tail = u64_at_u64(self.source, item_offset + 8)?;
            if (head == 0) != (tail == 0) {
                return Err("hash bucket head/tail mismatch".to_string());
            }
            self.valid_offset(head, "hash bucket head")?;
            self.valid_offset(tail, "hash bucket tail")?;
            item_offset += HASH_ITEM_SIZE;
        }
        Ok(())
    }

    pub(super) fn parse_entry_array(
        &mut self,
        offset: u64,
        obj: ObjectHeader,
    ) -> Result<(), String> {
        let item_size = if self.compact {
            COMPACT_OFFSET_ARRAY_ITEM_SIZE
        } else {
            REGULAR_OFFSET_ARRAY_ITEM_SIZE
        };
        if obj.size < OFFSET_ARRAY_OBJECT_HEADER_SIZE + item_size {
            return Err(format!(
                "ENTRY_ARRAY object at offset {offset} is too small"
            ));
        }
        if (obj.size - OFFSET_ARRAY_OBJECT_HEADER_SIZE) % item_size != 0 {
            return Err(format!(
                "ENTRY_ARRAY object at offset {offset} has unaligned items"
            ));
        }
        let mut array = EntryArray {
            next: u64_at_u64(self.source, offset + 16)?,
            items: Vec::new(),
        };
        self.valid_offset(array.next, "ENTRY_ARRAY next")?;
        let mut item_offset = offset + OFFSET_ARRAY_OBJECT_HEADER_SIZE;
        while item_offset < offset + obj.size {
            let item = if self.compact {
                u32_at_u64(self.source, item_offset)? as u64
            } else {
                u64_at_u64(self.source, item_offset)?
            };
            if item != 0 {
                self.valid_offset(item, "ENTRY_ARRAY item")?;
            }
            array.items.push(item);
            item_offset += item_size;
        }
        self.entry_arrays.insert(offset, array);
        Ok(())
    }
}
