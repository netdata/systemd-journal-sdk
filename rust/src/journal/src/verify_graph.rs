use journal_core::file::journal_hash_data;
use std::collections::{HashMap, HashSet};
use std::io::Read;

const HEADER_MIN_SIZE: usize = 208;
const OBJECT_HEADER_SIZE: u64 = 16;
const HASH_ITEM_SIZE: u64 = 16;
const DATA_OBJECT_HEADER_SIZE: u64 = 64;
const COMPACT_DATA_OBJECT_HEADER_SIZE: u64 = 72;
const FIELD_OBJECT_HEADER_SIZE: u64 = 40;
const ENTRY_OBJECT_HEADER_SIZE: u64 = 64;
const OFFSET_ARRAY_OBJECT_HEADER_SIZE: u64 = 24;
const REGULAR_ENTRY_ITEM_SIZE: u64 = 16;
const COMPACT_ENTRY_ITEM_SIZE: u64 = 4;
const REGULAR_OFFSET_ARRAY_ITEM_SIZE: u64 = 8;
const COMPACT_OFFSET_ARRAY_ITEM_SIZE: u64 = 4;
const JOURNAL_COMPACT_SIZE_MAX: u64 = u32::MAX as u64;
const MAX_UNCOMPRESSED_DATA_OBJECT_SIZE: usize = 768 * 1024 * 1024;

const OBJECT_TYPE_DATA: u8 = 1;
const OBJECT_TYPE_FIELD: u8 = 2;
const OBJECT_TYPE_ENTRY: u8 = 3;
const OBJECT_TYPE_DATA_HASH_TABLE: u8 = 4;
const OBJECT_TYPE_FIELD_HASH_TABLE: u8 = 5;
const OBJECT_TYPE_ENTRY_ARRAY: u8 = 6;
const OBJECT_TYPE_TAG: u8 = 7;

const OBJECT_COMPRESSED_XZ: u8 = 1 << 0;
const OBJECT_COMPRESSED_LZ4: u8 = 1 << 1;
const OBJECT_COMPRESSED_ZSTD: u8 = 1 << 2;
const OBJECT_COMPRESSED_MASK: u8 =
    OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD;

const INCOMPATIBLE_COMPRESSED_XZ: u32 = 1 << 0;
const INCOMPATIBLE_COMPRESSED_LZ4: u32 = 1 << 1;
const INCOMPATIBLE_KEYED_HASH: u32 = 1 << 2;
const INCOMPATIBLE_COMPRESSED_ZSTD: u32 = 1 << 3;
const INCOMPATIBLE_COMPACT: u32 = 1 << 4;

const COMPATIBLE_SEALED: u32 = 1 << 0;
const COMPATIBLE_TAIL_ENTRY_BOOT_ID: u32 = 1 << 1;
const COMPATIBLE_SEALED_CONTINUOUS: u32 = 1 << 2;
const COMPATIBLE_SUPPORTED_MASK: u32 =
    COMPATIBLE_SEALED | COMPATIBLE_TAIL_ENTRY_BOOT_ID | COMPATIBLE_SEALED_CONTINUOUS;
const TAG_OBJECT_SIZE: u64 = OBJECT_HEADER_SIZE + 8 + 8 + 32;

#[derive(Clone)]
struct Header {
    compatible_flags: u32,
    incompatible_flags: u32,
    state: u8,
    file_id: [u8; 16],
    tail_entry_boot_id: [u8; 16],
    header_size: u64,
    arena_size: u64,
    data_hash_table_offset: u64,
    data_hash_table_size: u64,
    field_hash_table_offset: u64,
    field_hash_table_size: u64,
    tail_object_offset: u64,
    n_objects: u64,
    n_entries: u64,
    tail_entry_seqnum: u64,
    head_entry_seqnum: u64,
    entry_array_offset: u64,
    head_entry_realtime: u64,
    tail_entry_realtime: u64,
    tail_entry_monotonic: u64,
    n_data: u64,
    n_fields: u64,
    n_tags: u64,
    n_entry_arrays: u64,
    tail_entry_offset: u64,
}

#[derive(Clone, Copy)]
struct ObjectHeader {
    typ: u8,
    flags: u8,
    size: u64,
}

#[derive(Clone)]
struct DataObject {
    hash: u64,
    next_hash_offset: u64,
    next_field_offset: u64,
    entry_offset: u64,
    entry_array_offset: u64,
    n_entries: u64,
}

#[derive(Clone)]
struct EntryObject {
    seqnum: u64,
    realtime: u64,
    monotonic: u64,
    boot_id: [u8; 16],
    items: Vec<u64>,
}

#[derive(Clone)]
struct EntryArray {
    next: u64,
    items: Vec<u64>,
}

pub(super) fn verify_object_graph(data: &[u8]) -> Result<(), String> {
    GraphVerifier::new(data).verify()
}

struct GraphVerifier<'a> {
    data: &'a [u8],
    header: Header,
    compact: bool,
    spans: HashMap<u64, ObjectHeader>,
    order: Vec<u64>,
    data_objects: HashMap<u64, DataObject>,
    entry_objects: HashMap<u64, EntryObject>,
    entry_arrays: HashMap<u64, EntryArray>,
    counts: [u64; 8],
    main_entry_array_found: bool,
}

impl<'a> GraphVerifier<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            header: Header::empty(),
            compact: false,
            spans: HashMap::new(),
            order: Vec::new(),
            data_objects: HashMap::new(),
            entry_objects: HashMap::new(),
            entry_arrays: HashMap::new(),
            counts: [0; 8],
            main_entry_array_found: false,
        }
    }

    fn verify(mut self) -> Result<(), String> {
        self.read_header()?;
        self.walk_objects()?;
        self.validate_header_counts()?;
        self.validate_main_entry_array_presence()?;
        self.validate_tail_metadata()?;
        self.validate_global_entry_array()?;
        self.validate_data_hash_table()
    }

    fn read_header(&mut self) -> Result<(), String> {
        if self.data.len() < HEADER_MIN_SIZE {
            return Err("file too small".to_string());
        }
        if &self.data[0..8] != b"LPKSHHRH" {
            return Err("invalid journal signature".to_string());
        }

        let mut header = Header {
            compatible_flags: u32_at(self.data, 8)?,
            incompatible_flags: u32_at(self.data, 12)?,
            state: self.data[16],
            file_id: bytes16_at(self.data, 24)?,
            tail_entry_boot_id: bytes16_at(self.data, 56)?,
            header_size: u64_at(self.data, 88)?,
            arena_size: u64_at(self.data, 96)?,
            data_hash_table_offset: u64_at(self.data, 104)?,
            data_hash_table_size: u64_at(self.data, 112)?,
            field_hash_table_offset: u64_at(self.data, 120)?,
            field_hash_table_size: u64_at(self.data, 128)?,
            tail_object_offset: u64_at(self.data, 136)?,
            n_objects: u64_at(self.data, 144)?,
            n_entries: u64_at(self.data, 152)?,
            tail_entry_seqnum: u64_at(self.data, 160)?,
            head_entry_seqnum: u64_at(self.data, 168)?,
            entry_array_offset: u64_at(self.data, 176)?,
            head_entry_realtime: u64_at(self.data, 184)?,
            tail_entry_realtime: u64_at(self.data, 192)?,
            tail_entry_monotonic: u64_at(self.data, 200)?,
            n_data: 0,
            n_fields: 0,
            n_tags: 0,
            n_entry_arrays: 0,
            tail_entry_offset: 0,
        };
        if header_contains_field(self.data, header.header_size, 216) {
            header.n_data = u64_at(self.data, 208)?;
        }
        if header_contains_field(self.data, header.header_size, 224) {
            header.n_fields = u64_at(self.data, 216)?;
        }
        if header_contains_field(self.data, header.header_size, 232) {
            header.n_tags = u64_at(self.data, 224)?;
        }
        if header_contains_field(self.data, header.header_size, 240) {
            header.n_entry_arrays = u64_at(self.data, 232)?;
        }
        if header_contains_field(self.data, header.header_size, 272) {
            header.tail_entry_offset = u64_at(self.data, 264)?;
        }

        if header.header_size < HEADER_MIN_SIZE as u64 {
            return Err(format!("invalid header_size {}", header.header_size));
        }
        if header.header_size > self.data.len() as u64 {
            return Err(format!(
                "header_size {} exceeds file size",
                header.header_size
            ));
        }
        if header.header_size % 8 != 0 {
            return Err(format!("header_size {} is not aligned", header.header_size));
        }
        if header.arena_size > self.data.len() as u64 - header.header_size {
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
        if self.data[17..24].iter().any(|b| *b != 0) {
            return Err("reserved header bytes are non-zero".to_string());
        }
        if header.incompatible_flags & INCOMPATIBLE_COMPACT != 0
            && self.data.len() as u64 > JOURNAL_COMPACT_SIZE_MAX
        {
            return Err("compact journal exceeds 32-bit size limit".to_string());
        }

        self.compact = header.incompatible_flags & INCOMPATIBLE_COMPACT != 0;
        self.header = header;
        Ok(())
    }

    fn walk_objects(&mut self) -> Result<(), String> {
        let tail = self.header.tail_object_offset;
        if tail == 0 {
            if self.header.n_objects != 0 {
                return Err("tail_object_offset is zero with objects recorded".to_string());
            }
            return Ok(());
        }
        if tail < self.header.header_size {
            return Err("tail_object_offset is before header_size".to_string());
        }

        let mut offset = self.header.header_size;
        let mut entry_seqnum = 0;
        let mut entry_seqnum_set = false;
        let mut entry_monotonic = 0;
        let mut entry_monotonic_set = false;
        let mut entry_boot_id = [0u8; 16];
        let mut entry_realtime = 0;
        let mut entry_realtime_set = false;
        let mut last_tag_realtime = 0;

        loop {
            if offset > tail {
                return Err("object walk skipped past tail_object_offset".to_string());
            }
            if offset > self.data.len() as u64 - OBJECT_HEADER_SIZE {
                return Err(format!(
                    "object header at offset {offset} exceeds file bounds"
                ));
            }
            let flags = byte_at(self.data, offset + 1)?;
            let obj = ObjectHeader {
                typ: byte_at(self.data, offset)?,
                flags,
                size: u64_at_u64(self.data, offset + 8)?,
            };
            let aligned_size = align8_checked(obj.size).ok_or_else(|| {
                format!(
                    "object size {} overflows alignment at offset {offset}",
                    obj.size
                )
            })?;
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
            if aligned_size == 0 || aligned_size > self.data.len() as u64 - offset {
                return Err(format!("object at offset {offset} exceeds file bounds"));
            }
            if offset % 8 != 0 {
                return Err(format!("object offset {offset} is not aligned"));
            }
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

            self.spans.insert(offset, obj);
            self.order.push(offset);
            self.counts[obj.typ as usize] += 1;

            match obj.typ {
                OBJECT_TYPE_DATA => self.parse_data(offset, obj)?,
                OBJECT_TYPE_FIELD => self.parse_field(offset, obj)?,
                OBJECT_TYPE_ENTRY => {
                    let entry = self.parse_entry(offset, obj)?;
                    if self.header.compatible_flags & COMPATIBLE_SEALED != 0
                        && self.counts[OBJECT_TYPE_TAG as usize] == 0
                    {
                        return Err(format!("first entry before first tag at offset {offset}"));
                    }
                    if entry.realtime < last_tag_realtime {
                        return Err(format!("older entry after newer tag at offset {offset}"));
                    }
                    if !entry_seqnum_set && entry.seqnum != self.header.head_entry_seqnum {
                        return Err(format!("head entry seqnum mismatch at offset {offset}"));
                    }
                    if entry_seqnum_set && entry_seqnum >= entry.seqnum {
                        return Err(format!("entry seqnum out of sync at offset {offset}"));
                    }
                    entry_seqnum = entry.seqnum;
                    entry_seqnum_set = true;
                    if entry_monotonic_set
                        && entry.boot_id == entry_boot_id
                        && entry_monotonic > entry.monotonic
                    {
                        return Err(format!("entry monotonic out of sync at offset {offset}"));
                    }
                    entry_monotonic = entry.monotonic;
                    entry_boot_id = entry.boot_id;
                    entry_monotonic_set = true;
                    if !entry_realtime_set && entry.realtime != self.header.head_entry_realtime {
                        return Err(format!("head entry realtime mismatch at offset {offset}"));
                    }
                    entry_realtime = entry.realtime;
                    entry_realtime_set = true;
                }
                OBJECT_TYPE_DATA_HASH_TABLE | OBJECT_TYPE_FIELD_HASH_TABLE => {
                    self.parse_hash_table(offset, obj)?
                }
                OBJECT_TYPE_ENTRY_ARRAY => {
                    self.parse_entry_array(offset, obj)?;
                    if offset == self.header.entry_array_offset {
                        if self.main_entry_array_found {
                            return Err("more than one main entry array".to_string());
                        }
                        self.main_entry_array_found = true;
                    }
                }
                OBJECT_TYPE_TAG => {
                    if self.header.compatible_flags & COMPATIBLE_SEALED == 0 {
                        return Err("TAG object in unsealed file".to_string());
                    }
                    if obj.size != TAG_OBJECT_SIZE {
                        return Err(format!("invalid TAG size at offset {offset}"));
                    }
                    let seqnum = u64_at_u64(self.data, offset + 16)?;
                    if seqnum != self.counts[OBJECT_TYPE_TAG as usize] {
                        return Err(format!("TAG seqnum mismatch at offset {offset}"));
                    }
                    if entry_realtime_set {
                        last_tag_realtime = entry_realtime;
                    }
                }
                _ => unreachable!(),
            }

            if offset == tail {
                break;
            }
            offset += aligned_size;
        }

        if self.order.last().copied() != Some(tail) {
            return Err("tail_object_offset does not point to walked tail".to_string());
        }
        if entry_seqnum_set && entry_seqnum != self.header.tail_entry_seqnum {
            return Err("tail_entry_seqnum mismatch".to_string());
        }
        if entry_monotonic_set
            && self.header.compatible_flags & COMPATIBLE_TAIL_ENTRY_BOOT_ID != 0
            && entry_boot_id == self.header.tail_entry_boot_id
            && entry_monotonic != self.header.tail_entry_monotonic
        {
            return Err("tail_entry_monotonic mismatch".to_string());
        }
        if entry_realtime_set && entry_realtime != self.header.tail_entry_realtime {
            return Err("tail_entry_realtime mismatch".to_string());
        }
        Ok(())
    }

    fn parse_data(&mut self, offset: u64, obj: ObjectHeader) -> Result<(), String> {
        let payload_offset = if self.compact {
            COMPACT_DATA_OBJECT_HEADER_SIZE
        } else {
            DATA_OBJECT_HEADER_SIZE
        };
        if obj.size <= payload_offset {
            return Err(format!("DATA object at offset {offset} has no payload"));
        }
        let payload = slice_u64(self.data, offset + payload_offset, offset + obj.size)?;
        let hash_payload = if obj.flags != 0 {
            decompress_payload(obj.flags, payload)
                .map_err(|err| format!("DATA decompression failed at offset {offset}: {err}"))?
        } else {
            payload.to_vec()
        };
        let stored_hash = u64_at_u64(self.data, offset + 16)?;
        let computed_hash = self.hash(&hash_payload);
        if stored_hash != computed_hash {
            return Err(format!(
                "DATA hash mismatch at offset {offset}: {stored_hash:#x} != {computed_hash:#x}"
            ));
        }
        let entry_offset = u64_at_u64(self.data, offset + 40)?;
        let n_entries = u64_at_u64(self.data, offset + 56)?;
        if (entry_offset == 0) != (n_entries == 0) {
            return Err(format!("DATA object at offset {offset} has bad n_entries"));
        }
        let data = DataObject {
            hash: stored_hash,
            next_hash_offset: u64_at_u64(self.data, offset + 24)?,
            next_field_offset: u64_at_u64(self.data, offset + 32)?,
            entry_offset,
            entry_array_offset: u64_at_u64(self.data, offset + 48)?,
            n_entries,
        };
        self.valid_offset(data.next_hash_offset, "DATA next_hash_offset")?;
        self.valid_offset(data.next_field_offset, "DATA next_field_offset")?;
        self.valid_offset(data.entry_offset, "DATA entry_offset")?;
        self.valid_offset(data.entry_array_offset, "DATA entry_array_offset")?;
        if n_entries < 2 && data.entry_array_offset != 0 {
            return Err(format!(
                "DATA object at offset {offset} has unexpected entry array"
            ));
        }
        if n_entries >= 2 && data.entry_array_offset == 0 {
            return Err(format!(
                "DATA object at offset {offset} is missing entry array"
            ));
        }
        self.data_objects.insert(offset, data);
        Ok(())
    }

    fn parse_field(&mut self, offset: u64, obj: ObjectHeader) -> Result<(), String> {
        if obj.size <= FIELD_OBJECT_HEADER_SIZE {
            return Err(format!("FIELD object at offset {offset} has no payload"));
        }
        let payload = slice_u64(
            self.data,
            offset + FIELD_OBJECT_HEADER_SIZE,
            offset + obj.size,
        )?;
        let stored_hash = u64_at_u64(self.data, offset + 16)?;
        let computed_hash = self.hash(payload);
        if stored_hash != computed_hash {
            return Err(format!(
                "FIELD hash mismatch at offset {offset}: {stored_hash:#x} != {computed_hash:#x}"
            ));
        }
        self.valid_offset(
            u64_at_u64(self.data, offset + 24)?,
            "FIELD next_hash_offset",
        )?;
        self.valid_offset(
            u64_at_u64(self.data, offset + 32)?,
            "FIELD head_data_offset",
        )?;
        Ok(())
    }

    fn parse_entry(&mut self, offset: u64, obj: ObjectHeader) -> Result<EntryObject, String> {
        let item_size = if self.compact {
            COMPACT_ENTRY_ITEM_SIZE
        } else {
            REGULAR_ENTRY_ITEM_SIZE
        };
        if obj.size < ENTRY_OBJECT_HEADER_SIZE {
            return Err(format!("ENTRY object at offset {offset} is too small"));
        }
        if (obj.size - ENTRY_OBJECT_HEADER_SIZE) % item_size != 0 {
            return Err(format!(
                "ENTRY object at offset {offset} has unaligned items"
            ));
        }
        let mut entry = EntryObject {
            seqnum: u64_at_u64(self.data, offset + 16)?,
            realtime: u64_at_u64(self.data, offset + 24)?,
            monotonic: u64_at_u64(self.data, offset + 32)?,
            boot_id: bytes16_at_u64(self.data, offset + 40)?,
            items: Vec::new(),
        };
        if entry.seqnum == 0 {
            return Err(format!("ENTRY object at offset {offset} has zero seqnum"));
        }
        if entry.realtime == 0 {
            return Err(format!("ENTRY object at offset {offset} has zero realtime"));
        }
        let mut item_offset = offset + ENTRY_OBJECT_HEADER_SIZE;
        while item_offset < offset + obj.size {
            let item = if self.compact {
                u32_at_u64(self.data, item_offset)? as u64
            } else {
                u64_at_u64(self.data, item_offset)?
            };
            if item == 0 {
                return Err(format!("ENTRY object at offset {offset} has zero item"));
            }
            self.valid_offset(item, "ENTRY item")?;
            entry.items.push(item);
            item_offset += item_size;
        }
        if entry.items.is_empty() {
            return Err(format!("ENTRY object at offset {offset} has no items"));
        }
        self.entry_objects.insert(offset, entry.clone());
        Ok(entry)
    }

    fn parse_hash_table(&self, offset: u64, obj: ObjectHeader) -> Result<(), String> {
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
            let head = u64_at_u64(self.data, item_offset)?;
            let tail = u64_at_u64(self.data, item_offset + 8)?;
            if (head == 0) != (tail == 0) {
                return Err("hash bucket head/tail mismatch".to_string());
            }
            self.valid_offset(head, "hash bucket head")?;
            self.valid_offset(tail, "hash bucket tail")?;
            item_offset += HASH_ITEM_SIZE;
        }
        Ok(())
    }

    fn parse_entry_array(&mut self, offset: u64, obj: ObjectHeader) -> Result<(), String> {
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
            next: u64_at_u64(self.data, offset + 16)?,
            items: Vec::new(),
        };
        self.valid_offset(array.next, "ENTRY_ARRAY next")?;
        let mut item_offset = offset + OFFSET_ARRAY_OBJECT_HEADER_SIZE;
        while item_offset < offset + obj.size {
            let item = if self.compact {
                u32_at_u64(self.data, item_offset)? as u64
            } else {
                u64_at_u64(self.data, item_offset)?
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

    fn validate_header_counts(&self) -> Result<(), String> {
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

    fn validate_main_entry_array_presence(&self) -> Result<(), String> {
        if self.header.entry_array_offset != 0 && !self.main_entry_array_found {
            return Err("missing main entry array".to_string());
        }
        if self.header.n_entries != 0 && self.header.entry_array_offset == 0 {
            return Err("entry_array_offset is zero with entries recorded".to_string());
        }
        Ok(())
    }

    fn validate_tail_metadata(&self) -> Result<(), String> {
        if self.entry_objects.is_empty() {
            if self.header.n_entries != 0 {
                return Err("entries recorded but no ENTRY objects found".to_string());
            }
            return Ok(());
        }
        let (head_offset, head) = self
            .entry_objects
            .iter()
            .min_by_key(|(_, entry)| entry.seqnum)
            .map(|(offset, entry)| (*offset, entry))
            .ok_or_else(|| "missing head entry".to_string())?;
        let (tail_offset, tail) = self
            .entry_objects
            .iter()
            .max_by_key(|(_, entry)| entry.seqnum)
            .map(|(offset, entry)| (*offset, entry))
            .ok_or_else(|| "missing tail entry".to_string())?;
        if self.header.head_entry_seqnum != head.seqnum {
            return Err("head_entry_seqnum mismatch".to_string());
        }
        if self.header.tail_entry_seqnum != tail.seqnum {
            return Err("tail_entry_seqnum mismatch".to_string());
        }
        if self.header.head_entry_realtime != head.realtime {
            return Err("head_entry_realtime mismatch".to_string());
        }
        if self.header.tail_entry_realtime != tail.realtime {
            return Err("tail_entry_realtime mismatch".to_string());
        }
        if self.header.compatible_flags & COMPATIBLE_TAIL_ENTRY_BOOT_ID != 0 {
            if self.header.tail_entry_monotonic != tail.monotonic {
                return Err("tail_entry_monotonic mismatch".to_string());
            }
            if self.header.tail_entry_boot_id != tail.boot_id {
                return Err("tail_entry_boot_id mismatch".to_string());
            }
        }
        if header_contains_field(self.data, self.header.header_size, 272)
            && self.header.tail_entry_offset != tail_offset
        {
            return Err("tail_entry_offset mismatch".to_string());
        }
        if head_offset == 0 {
            return Err("head entry offset is zero".to_string());
        }
        Ok(())
    }

    fn validate_global_entry_array(&self) -> Result<(), String> {
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

    fn validate_data_hash_table(&self) -> Result<(), String> {
        let table_offset = self.header.data_hash_table_offset;
        let table_size = self.header.data_hash_table_size;
        if table_offset == 0 || table_size == 0 {
            return Ok(());
        }
        let bucket_count = table_size / HASH_ITEM_SIZE;
        for bucket_index in 0..bucket_count {
            let item_offset = table_offset + bucket_index * HASH_ITEM_SIZE;
            let mut current = u64_at_u64(self.data, item_offset)?;
            let tail = u64_at_u64(self.data, item_offset + 8)?;
            let mut last = 0;
            let mut seen = HashSet::new();
            while current != 0 {
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
                last = current;
                current = obj.next_hash_offset;
            }
            if last != tail {
                return Err("data hash bucket tail mismatch".to_string());
            }
        }
        Ok(())
    }

    fn validate_entry_data_links(&self, entry_offset: u64, last_entry: bool) -> Result<(), String> {
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

    fn validate_data_entry_array(&self, data_offset: u64, data: &DataObject) -> Result<(), String> {
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

    fn walk_entry_array_chain(
        &self,
        start_offset: u64,
        used_count: u64,
        label: &str,
    ) -> Result<Vec<u64>, String> {
        if used_count == 0 {
            if start_offset != 0 {
                return Err(format!("{label} has start offset with zero entries"));
            }
            return Ok(Vec::new());
        }
        if start_offset == 0 {
            return Err(format!("{label} is missing"));
        }
        let mut entries = Vec::new();
        let mut remaining = used_count;
        let mut current = start_offset;
        let mut seen = HashSet::new();
        while remaining > 0 {
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
            let used_here = remaining.min(array.items.len() as u64);
            for idx in 0..used_here as usize {
                let item = array.items[idx];
                if item == 0 {
                    return Err(format!("{label} has zero used item"));
                }
                if !self.entry_objects.contains_key(&item) {
                    return Err(format!("{label} references missing ENTRY"));
                }
                entries.push(item);
            }
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

    fn data_object_in_hash_table(&self, data_offset: u64, data_hash: u64) -> bool {
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

    fn data_references_entry(&self, data: &DataObject, entry_offset: u64) -> Result<bool, String> {
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

    fn valid_offset(&self, offset: u64, label: &str) -> Result<(), String> {
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

    fn hash(&self, payload: &[u8]) -> u64 {
        let keyed = self.header.incompatible_flags & INCOMPATIBLE_KEYED_HASH != 0;
        journal_hash_data(
            payload,
            keyed,
            if keyed {
                Some(&self.header.file_id)
            } else {
                None
            },
        )
    }
}

impl Header {
    fn empty() -> Self {
        Self {
            compatible_flags: 0,
            incompatible_flags: 0,
            state: 0,
            file_id: [0; 16],
            tail_entry_boot_id: [0; 16],
            header_size: 0,
            arena_size: 0,
            data_hash_table_offset: 0,
            data_hash_table_size: 0,
            field_hash_table_offset: 0,
            field_hash_table_size: 0,
            tail_object_offset: 0,
            n_objects: 0,
            n_entries: 0,
            tail_entry_seqnum: 0,
            head_entry_seqnum: 0,
            entry_array_offset: 0,
            head_entry_realtime: 0,
            tail_entry_realtime: 0,
            tail_entry_monotonic: 0,
            n_data: 0,
            n_fields: 0,
            n_tags: 0,
            n_entry_arrays: 0,
            tail_entry_offset: 0,
        }
    }
}

fn decompress_payload(flags: u8, payload: &[u8]) -> Result<Vec<u8>, String> {
    if flags & OBJECT_COMPRESSED_ZSTD != 0 {
        let mut decoder =
            ruzstd::decoding::StreamingDecoder::new(payload).map_err(|err| err.to_string())?;
        return read_limited_to_end(&mut decoder);
    }
    if flags & OBJECT_COMPRESSED_XZ != 0 {
        let mut decoder = lzma_rust2::XzReader::new(payload, false);
        return read_limited_to_end(&mut decoder);
    }
    if flags & OBJECT_COMPRESSED_LZ4 != 0 {
        if payload.len() < 8 {
            return Err("lz4 compressed payload too short".to_string());
        }
        let expected = usize::try_from(u64::from_le_bytes(
            payload[0..8]
                .try_into()
                .map_err(|_| "bad lz4 size prefix")?,
        ))
        .map_err(|_| "lz4 decompressed payload too large".to_string())?;
        if expected > MAX_UNCOMPRESSED_DATA_OBJECT_SIZE {
            return Err("lz4 decompressed payload too large".to_string());
        }
        let mut out = vec![0; expected];
        let len = lz4_flex::block::decompress_into(&payload[8..], &mut out)
            .map_err(|err| err.to_string())?;
        if len != expected {
            return Err("lz4 decompressed size mismatch".to_string());
        }
        return Ok(out);
    }
    Ok(payload.to_vec())
}

fn read_limited_to_end<R: Read>(reader: &mut R) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        if out.len() == MAX_UNCOMPRESSED_DATA_OBJECT_SIZE {
            let mut extra = [0u8; 1];
            match reader.read(&mut extra) {
                Ok(0) => return Ok(out),
                Ok(_) => return Err("decompressed payload too large".to_string()),
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(err.to_string()),
            }
        }
        let remaining = MAX_UNCOMPRESSED_DATA_OBJECT_SIZE - out.len();
        let read_len = remaining.min(buf.len());
        match reader.read(&mut buf[..read_len]) {
            Ok(0) => return Ok(out),
            Ok(len) => out.extend_from_slice(&buf[..len]),
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err.to_string()),
        }
    }
}

fn header_contains_field(data: &[u8], header_size: u64, end: usize) -> bool {
    header_size >= end as u64 && data.len() >= end
}

fn align8_checked(value: u64) -> Option<u64> {
    value.checked_add(7).map(|v| v & !7)
}

fn byte_at(data: &[u8], offset: u64) -> Result<u8, String> {
    let offset = usize::try_from(offset).map_err(|_| "offset is not representable".to_string())?;
    data.get(offset)
        .copied()
        .ok_or_else(|| format!("byte read at {offset} exceeds file bounds"))
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32, String> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| format!("uint32 read at {offset} exceeds file bounds"))?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn u32_at_u64(data: &[u8], offset: u64) -> Result<u32, String> {
    let offset = usize::try_from(offset).map_err(|_| "offset is not representable".to_string())?;
    u32_at(data, offset)
}

fn u64_at(data: &[u8], offset: usize) -> Result<u64, String> {
    let bytes = data
        .get(offset..offset + 8)
        .ok_or_else(|| format!("uint64 read at {offset} exceeds file bounds"))?;
    Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
}

fn u64_at_u64(data: &[u8], offset: u64) -> Result<u64, String> {
    let offset = usize::try_from(offset).map_err(|_| "offset is not representable".to_string())?;
    u64_at(data, offset)
}

fn bytes16_at(data: &[u8], offset: usize) -> Result<[u8; 16], String> {
    let bytes = data
        .get(offset..offset + 16)
        .ok_or_else(|| format!("16-byte read at {offset} exceeds file bounds"))?;
    Ok(bytes.try_into().unwrap())
}

fn bytes16_at_u64(data: &[u8], offset: u64) -> Result<[u8; 16], String> {
    let offset = usize::try_from(offset).map_err(|_| "offset is not representable".to_string())?;
    bytes16_at(data, offset)
}

fn slice_u64(data: &[u8], start: u64, end: u64) -> Result<&[u8], String> {
    let start =
        usize::try_from(start).map_err(|_| "start offset is not representable".to_string())?;
    let end = usize::try_from(end).map_err(|_| "end offset is not representable".to_string())?;
    data.get(start..end)
        .ok_or_else(|| format!("slice {start}..{end} exceeds file bounds"))
}
