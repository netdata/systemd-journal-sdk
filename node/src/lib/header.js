// Journal file header parsing and writing.
// Layout matches Go format.go exactly.

import { readUint64LE, writeUint64LE } from './binary.js';

export const HEADER_MIN_SIZE = 208;
export const HEADER_SIZE = 272;  // v260+ writer header size

export const STATE_OFFLINE = 0;
export const STATE_ONLINE = 1;
export const STATE_ARCHIVED = 2;

// Incompatible flags (bit positions in incompatible_flags uint32)
export const INCOMPATIBLE_COMPRESSED_XZ = 1 << 0;
export const INCOMPATIBLE_COMPRESSED_LZ4 = 1 << 1;
export const INCOMPATIBLE_KEYED_HASH = 1 << 2;
export const INCOMPATIBLE_COMPRESSED_ZSTD = 1 << 3;
export const INCOMPATIBLE_COMPACT = 1 << 4;

// HEADER_COMPATIBLE_SEALED - set when FSS sealing is enabled
export const COMPATIBLE_SEALED = 1 << 0;
// HEADER_COMPATIBLE_SEALED_CONTINUOUS - set when FSS sealing is continuous
export const COMPATIBLE_SEALED_CONTINUOUS = 1 << 2;
// HEADER_COMPATIBLE_TAIL_ENTRY_BOOT_ID - set for new files (v260+)
export const COMPATIBLE_TAIL_ENTRY_BOOT_ID = 1 << 1;

// Object types (1-based)
export const OBJECT_TYPE_DATA = 1;
export const OBJECT_TYPE_FIELD = 2;
export const OBJECT_TYPE_ENTRY = 3;
export const OBJECT_TYPE_DATA_HASH_TABLE = 4;
export const OBJECT_TYPE_FIELD_HASH_TABLE = 5;
export const OBJECT_TYPE_ENTRY_ARRAY = 6;
export const OBJECT_TYPE_TAG = 7;

// Object header: 16 bytes
// Byte 0: type (uint8), Byte 1: flags (uint8), Bytes 2-7: reserved, Bytes 8-15: size (uint64LE)
export const OBJECT_HEADER_SIZE = 16;

// Object compression flags
export const OBJECT_COMPRESSED_XZ = 1 << 0;
export const OBJECT_COMPRESSED_LZ4 = 1 << 1;
export const OBJECT_COMPRESSED_ZSTD = 1 << 2;

// Entry object header: 64 bytes
// After object header (16): seqnum(8), realtime(8), monotonic(8), boot_id(16), xor_hash(8)
export const ENTRY_OBJECT_HEADER_SIZE = 64;

// Data object header: 64 bytes
// After object header (16): hash(8), next_hash(8), next_field(8), entry_offset(8), entry_array(8), n_entries(8)
export const DATA_OBJECT_HEADER_SIZE = 64;

// Field object header: 40 bytes
// After object header (16): hash(8), next_hash(8), head_data(8)
export const FIELD_OBJECT_HEADER_SIZE = 40;

// Hash item: 16 bytes (head offset + tail offset)
export const HASH_ITEM_SIZE = 16;

// Offset array object header: 24 bytes (16 object header + 8 next_array_offset)
export const OFFSET_ARRAY_OBJECT_HEADER_SIZE = 24;

// Entry items within an entry object: 16 bytes each (offset + hash)
export const REGULAR_ENTRY_ITEM_SIZE = 16;
export const COMPACT_ENTRY_ITEM_SIZE = 4;
export const REGULAR_OFFSET_ARRAY_ITEM_SIZE = 8;
export const COMPACT_OFFSET_ARRAY_ITEM_SIZE = 4;
export const COMPACT_DATA_OBJECT_HEADER_SIZE = DATA_OBJECT_HEADER_SIZE + 8;
export const COMPACT_DATA_TAIL_OFFSET_OFFSET = BigInt(DATA_OBJECT_HEADER_SIZE);
export const COMPACT_DATA_TAIL_ENTRIES_OFFSET = BigInt(DATA_OBJECT_HEADER_SIZE + 4);
export const JOURNAL_COMPACT_SIZE_MAX = 0xffffffffn;

// Default sizes for writer
export const DEFAULT_DATA_HASH_BUCKETS = 233016;
export const DEFAULT_FIELD_HASH_BUCKETS = 1023;
export const DEFAULT_MIN_DATA_HASH_BUCKETS = 2047;
export const DEFAULT_MAX_FILE_SIZE = 128 * 1024 * 1024;
export const JOURNAL_FILE_SIZE_MIN = 512 * 1024;
export const PAGE_SIZE = 4096;
export const FILE_SIZE_INCREASE = 8 * 1024 * 1024;
export const INITIAL_ENTRY_ARRAY_CAP = 4096;
export const INITIAL_DATA_ENTRY_ARRAY_CAP = 64;

export function normalizeJournalMaxFileSize(size, compact = false) {
  let normalized = Number(size || DEFAULT_MAX_FILE_SIZE);
  if (normalized !== DEFAULT_MAX_FILE_SIZE || size) {
    normalized = Math.ceil(Math.max(1, normalized) / PAGE_SIZE) * PAGE_SIZE;
  }
  if (compact && normalized > Number(JOURNAL_COMPACT_SIZE_MAX)) {
    normalized = Number(JOURNAL_COMPACT_SIZE_MAX);
  }
  return Math.max(normalized, JOURNAL_FILE_SIZE_MIN);
}

export function dataHashBucketsForMaxFileSize(maxFileSize) {
  return Math.max(Math.floor(Number(maxFileSize) / 576), DEFAULT_MIN_DATA_HASH_BUCKETS);
}

// Parse an object header from a buffer at the given offset.
export function parseObjectHeader(buf, offset = 0) {
  if (buf.length < offset + OBJECT_HEADER_SIZE) return null;
  return {
    type: buf.readUInt8(offset),
    flags: buf.readUInt8(offset + 1),
    size: readUint64LE(buf, offset + 8),
  };
}

// Write an object header into a buffer at the given offset.
export function writeObjectHeader(buf, offset, type, flags, size) {
  buf.writeUInt8(type, offset);
  buf.writeUInt8(flags, offset + 1);
  // Bytes 2-7 stay zero (reserved)
  writeUint64LE(buf, offset + 8, size);
}

// Parse the file header from a journal buffer.
export function parseFileHeader(buf) {
  const sig = validateFileHeaderPrefix(buf);
  const header = parseFileHeaderBase(buf, sig);
  validateDeclaredHeaderSize(buf, header);
  parseOptionalHeaderFields(buf, header);
  return header;
}

function validateFileHeaderPrefix(buf) {
  if (buf.length < HEADER_MIN_SIZE) {
    throw new Error(`header buffer too small: ${buf.length} < ${HEADER_MIN_SIZE}`);
  }
  const sig = buf.toString('latin1', 0, 8);
  if (sig !== 'LPKSHHRH') {
    throw new Error('invalid journal signature');
  }
  return sig;
}


function parseFileHeaderBase(buf, sig) {
  return {
    signature: sig,
    compatible_flags: buf.readUInt32LE(8),
    incompatible_flags: buf.readUInt32LE(12),
    state: buf.readUInt8(16),
    file_id: Buffer.from(buf.slice(24, 40)),
    machine_id: Buffer.from(buf.slice(40, 56)),
    tail_entry_boot_id: Buffer.from(buf.slice(56, 72)),
    seqnum_id: Buffer.from(buf.slice(72, 88)),
    header_size: readUint64LE(buf, 88),
    arena_size: readUint64LE(buf, 96),
    data_hash_table_offset: readUint64LE(buf, 104),
    data_hash_table_size: readUint64LE(buf, 112),
    field_hash_table_offset: readUint64LE(buf, 120),
    field_hash_table_size: readUint64LE(buf, 128),
    tail_object_offset: readUint64LE(buf, 136),
    n_objects: readUint64LE(buf, 144),
    n_entries: readUint64LE(buf, 152),
    tail_entry_seqnum: readUint64LE(buf, 160),
    head_entry_seqnum: readUint64LE(buf, 168),
    entry_array_offset: readUint64LE(buf, 176),
    head_entry_realtime: readUint64LE(buf, 184),
    tail_entry_realtime: readUint64LE(buf, 192),
    tail_entry_monotonic: readUint64LE(buf, 200),
    n_data: 0n,
    n_fields: 0n,
    n_tags: 0n,
    n_entry_arrays: 0n,
    data_hash_chain_depth: 0n,
    field_hash_chain_depth: 0n,
    tail_entry_array_offset: 0,
    tail_entry_array_n_entries: 0,
    tail_entry_offset: 0n,
  };
}


function validateDeclaredHeaderSize(buf, header) {
  const requiredHeaderSize = Number(
    header.header_size < BigInt(HEADER_SIZE) ? header.header_size : BigInt(HEADER_SIZE),
  );
  if (buf.length < requiredHeaderSize) {
    throw new Error(`header buffer too small: ${buf.length} < ${requiredHeaderSize}`);
  }
}


function parseOptionalHeaderFields(buf, header) {
  parseOptionalU64HeaderFields(buf, header);
  parseOptionalU32HeaderFields(buf, header);
}

function parseOptionalU64HeaderFields(buf, header) {
  for (const [name, offset, end] of OPTIONAL_U64_HEADER_FIELDS) {
    if (headerContainsField(buf, header.header_size, end)) {
      header[name] = readUint64LE(buf, offset);
    }
  }
}

function parseOptionalU32HeaderFields(buf, header) {
  for (const [name, offset, end] of OPTIONAL_U32_HEADER_FIELDS) {
    if (headerContainsField(buf, header.header_size, end)) {
      header[name] = buf.readUInt32LE(offset);
    }
  }
}

const OPTIONAL_U64_HEADER_FIELDS = [
  ['n_data', 208, 216],
  ['n_fields', 216, 224],
  ['n_tags', 224, 232],
  ['n_entry_arrays', 232, 240],
  ['data_hash_chain_depth', 240, 248],
  ['field_hash_chain_depth', 248, 256],
  ['tail_entry_offset', 264, 272],
];

const OPTIONAL_U32_HEADER_FIELDS = [
  ['tail_entry_array_offset', 256, 260],
  ['tail_entry_array_n_entries', 260, 264],
];

function headerContainsField(buf, headerSize, end) {
  return headerSize >= BigInt(end) && buf.length >= end;
}

// Serialize a header object into a v260 272-byte buffer.
export function serializeFileHeader(buf, h) {
  if (buf.length < HEADER_SIZE) {
    throw new Error(`buffer too small for header: ${buf.length}`);
  }
  buf.write('LPKSHHRH', 0, 8, 'latin1');
  buf.writeUInt32LE(h.compatible_flags, 8);
  buf.writeUInt32LE(h.incompatible_flags, 12);
  buf.writeUInt8(h.state, 16);
  // 17-23 reserved
  h.file_id.copy(buf, 24);
  h.machine_id.copy(buf, 40);
  h.tail_entry_boot_id.copy(buf, 56);
  h.seqnum_id.copy(buf, 72);
  writeUint64LE(buf, 88, h.header_size);
  writeUint64LE(buf, 96, h.arena_size);
  writeUint64LE(buf, 104, h.data_hash_table_offset);
  writeUint64LE(buf, 112, h.data_hash_table_size);
  writeUint64LE(buf, 120, h.field_hash_table_offset);
  writeUint64LE(buf, 128, h.field_hash_table_size);
  writeUint64LE(buf, 136, h.tail_object_offset);
  writeUint64LE(buf, 144, h.n_objects);
  writeUint64LE(buf, 152, h.n_entries);
  writeUint64LE(buf, 160, h.tail_entry_seqnum);
  writeUint64LE(buf, 168, h.head_entry_seqnum);
  writeUint64LE(buf, 176, h.entry_array_offset);
  writeUint64LE(buf, 184, h.head_entry_realtime);
  writeUint64LE(buf, 192, h.tail_entry_realtime);
  writeUint64LE(buf, 200, h.tail_entry_monotonic);
  // Added in 187
  writeUint64LE(buf, 208, h.n_data || 0);
  writeUint64LE(buf, 216, h.n_fields || 0);
  // Added in 189
  writeUint64LE(buf, 224, h.n_tags || 0);
  writeUint64LE(buf, 232, h.n_entry_arrays || 0);
  // Added in 246
  writeUint64LE(buf, 240, h.data_hash_chain_depth || 0);
  writeUint64LE(buf, 248, h.field_hash_chain_depth || 0);
  // Added in 252
  buf.writeUInt32LE(h.tail_entry_array_offset || 0, 256);
  buf.writeUInt32LE(h.tail_entry_array_n_entries || 0, 260);
  // Added in 254
  writeUint64LE(buf, 264, h.tail_entry_offset || 0);
}
