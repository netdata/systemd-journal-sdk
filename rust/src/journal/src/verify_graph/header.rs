use super::io::*;
use super::*;

impl<'a> GraphVerifier<'a> {
    pub(super) fn read_header(&mut self) -> Result<(), String> {
        self.validate_header_prefix()?;
        let mut header = self.read_base_header()?;
        self.read_optional_header_fields(&mut header)?;
        self.validate_header(&header)?;
        self.compact = header.incompatible_flags & INCOMPATIBLE_COMPACT != 0;
        self.header = header;
        Ok(())
    }

    pub(super) fn validate_header_prefix(&self) -> Result<(), String> {
        if self.source.len() < HEADER_MIN_SIZE as u64 {
            return Err("file too small".to_string());
        }
        if self.source.read_vec(0, 8)? != b"LPKSHHRH" {
            return Err("invalid journal signature".to_string());
        }
        if self.source.read_vec(17, 7)?.iter().any(|b| *b != 0) {
            return Err("reserved header bytes are non-zero".to_string());
        }
        Ok(())
    }

    pub(super) fn read_base_header(&self) -> Result<Header, String> {
        let mut header = Header::empty();
        self.read_header_flags(&mut header)?;
        self.read_header_ids(&mut header)?;
        self.read_header_layout(&mut header)?;
        self.read_header_hash_tables(&mut header)?;
        self.read_header_object_counters(&mut header)?;
        self.read_header_entry_metadata(&mut header)?;
        Ok(header)
    }

    pub(super) fn read_header_flags(&self, header: &mut Header) -> Result<(), String> {
        header.compatible_flags = u32_at(self.source, 8)?;
        header.incompatible_flags = u32_at(self.source, 12)?;
        header.state = byte_at(self.source, 16)?;
        Ok(())
    }

    pub(super) fn read_header_ids(&self, header: &mut Header) -> Result<(), String> {
        header.file_id = bytes16_at(self.source, 24)?;
        header.tail_entry_boot_id = bytes16_at(self.source, 56)?;
        Ok(())
    }

    pub(super) fn read_header_layout(&self, header: &mut Header) -> Result<(), String> {
        header.header_size = u64_at(self.source, 88)?;
        header.arena_size = u64_at(self.source, 96)?;
        header.tail_object_offset = u64_at(self.source, 136)?;
        Ok(())
    }

    pub(super) fn read_header_hash_tables(&self, header: &mut Header) -> Result<(), String> {
        header.data_hash_table_offset = u64_at(self.source, 104)?;
        header.data_hash_table_size = u64_at(self.source, 112)?;
        header.field_hash_table_offset = u64_at(self.source, 120)?;
        header.field_hash_table_size = u64_at(self.source, 128)?;
        Ok(())
    }

    pub(super) fn read_header_object_counters(&self, header: &mut Header) -> Result<(), String> {
        header.n_objects = u64_at(self.source, 144)?;
        header.n_entries = u64_at(self.source, 152)?;
        Ok(())
    }

    pub(super) fn read_header_entry_metadata(&self, header: &mut Header) -> Result<(), String> {
        header.tail_entry_seqnum = u64_at(self.source, 160)?;
        header.head_entry_seqnum = u64_at(self.source, 168)?;
        header.entry_array_offset = u64_at(self.source, 176)?;
        header.head_entry_realtime = u64_at(self.source, 184)?;
        header.tail_entry_realtime = u64_at(self.source, 192)?;
        header.tail_entry_monotonic = u64_at(self.source, 200)?;
        Ok(())
    }

    pub(super) fn read_optional_header_fields(&self, header: &mut Header) -> Result<(), String> {
        if header_contains_field(self.source, header.header_size, 216) {
            header.n_data = u64_at(self.source, 208)?;
        }
        if header_contains_field(self.source, header.header_size, 224) {
            header.n_fields = u64_at(self.source, 216)?;
        }
        if header_contains_field(self.source, header.header_size, 232) {
            header.n_tags = u64_at(self.source, 224)?;
        }
        if header_contains_field(self.source, header.header_size, 240) {
            header.n_entry_arrays = u64_at(self.source, 232)?;
        }
        if header_contains_field(self.source, header.header_size, 272) {
            header.tail_entry_offset = u64_at(self.source, 264)?;
        }
        Ok(())
    }

    pub(super) fn validate_header(&self, header: &Header) -> Result<(), String> {
        if header.header_size < HEADER_MIN_SIZE as u64 {
            return Err(format!("invalid header_size {}", header.header_size));
        }
        if header.header_size > self.source.len() {
            return Err(format!(
                "header_size {} exceeds file size",
                header.header_size
            ));
        }
        if header.header_size % 8 != 0 {
            return Err(format!("header_size {} is not aligned", header.header_size));
        }
        if header.arena_size > self.source.len() - header.header_size {
            return Err("header_size + arena_size exceeds file size".to_string());
        }
        if !matches!(header.state, 0 | 1 | 2) {
            return Err(format!("invalid journal state {}", header.state));
        }
        if header.compatible_flags & !COMPATIBLE_SUPPORTED_MASK != 0 {
            return Err(format!(
                "unsupported compatible flags 0x{:x}",
                header.compatible_flags
            ));
        }
        if header.incompatible_flags & INCOMPATIBLE_COMPACT != 0
            && self.source.len() > JOURNAL_COMPACT_SIZE_MAX
        {
            return Err("compact journal exceeds 32-bit size limit".to_string());
        }
        Ok(())
    }
}
