// Journal file header parsing and writing.
// Layout matches Go format.go exactly.

import { readUint64LE, writeUint64LE, writeUint32LE, writeUint8 } from './binary.js';

export const HEADER_SIZE = 208;

export const STATE_OFFLINE = 0;
export const STATE_ONLINE = 1;
export const STATE_ARCHIVED = 2;

// Incompatible flags (bit positions in incompatible_flags uint32)
export const INCOMPATIBLE_COMPRESSED_XZ = 1 << 0;
export const INCOMPATIBLE_COMPRESSED_LZ4 = 1 << 1;
export const INCOMPATIBLE_KEYED_HASH = 1 << 2;
export const INCOMPATIBLE_COMPRESSED_ZSTD = 1 << 3;
export const INCOMPATIBLE_COMPACT = 1 << 4;

// Object types (1-based)
export const OBJECT_TYPE_DATA = 1;
export const OBJECT_TYPE_FIELD = 2;
export const OBJECT_TYPE_ENTRY = 3;
export const OBJECT_TYPE_DATA_HASH_TABLE = 4;
export const OBJECT_TYPE_FIELD_HASH_TABLE = 5;
export const OBJECT_TYPE_ENTRY_ARRAY = 6;

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

// Default sizes for writer
export const DEFAULT_DATA_HASH_BUCKETS = 4096;
export const DEFAULT_FIELD_HASH_BUCKETS = 512;
export const INITIAL_ENTRY_ARRAY_CAP = 4096;
export const INITIAL_DATA_ENTRY_ARRAY_CAP = 64;

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

// Parse the file header from a 208-byte buffer.
export function parseFileHeader(buf) {
  if (buf.length < HEADER_SIZE) {
    throw new Error(`header buffer too small: ${buf.length} < ${HEADER_SIZE}`);
  }
  const sig = buf.toString('latin1', 0, 8);
  if (sig !== 'LPKSHHRH') {
    throw new Error('invalid journal signature');
  }

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
  };
}

// Serialize a header object into a 208-byte buffer.
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
}
