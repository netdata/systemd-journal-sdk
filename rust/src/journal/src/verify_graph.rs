use io::VerifyByteSource;
use journal_core::file::journal_hash_data;
use std::collections::{HashMap, HashSet};

mod hash;
mod header;
mod io;
mod validation;
mod walk;

pub(super) use io::VerifyByteSource as ByteSource;

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

#[derive(Default)]
struct ObjectWalkState {
    entry_seqnum: u64,
    entry_seqnum_set: bool,
    entry_monotonic: u64,
    entry_monotonic_set: bool,
    entry_boot_id: [u8; 16],
    entry_realtime: u64,
    entry_realtime_set: bool,
    last_tag_realtime: u64,
}

pub(super) fn verify_object_graph_source(source: &dyn VerifyByteSource) -> Result<(), String> {
    GraphVerifier::new(source).verify()
}

struct GraphVerifier<'a> {
    source: &'a dyn VerifyByteSource,
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
    fn new(source: &'a dyn VerifyByteSource) -> Self {
        Self {
            source,
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
