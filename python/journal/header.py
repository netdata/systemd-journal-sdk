# Journal file header parsing and writing.
# Layout matches Go/Node.js format exactly.

import struct

from .binary import read_uint64_le, write_uint64_le, write_uint32_le, write_uint8

HEADER_SIZE = 208

STATE_OFFLINE = 0
STATE_ONLINE = 1
STATE_ARCHIVED = 2

INCOMPATIBLE_COMPRESSED_XZ = 1 << 0
INCOMPATIBLE_COMPRESSED_LZ4 = 1 << 1
INCOMPATIBLE_KEYED_HASH = 1 << 2
INCOMPATIBLE_COMPRESSED_ZSTD = 1 << 3
INCOMPATIBLE_COMPACT = 1 << 4

OBJECT_TYPE_DATA = 1
OBJECT_TYPE_FIELD = 2
OBJECT_TYPE_ENTRY = 3
OBJECT_TYPE_DATA_HASH_TABLE = 4
OBJECT_TYPE_FIELD_HASH_TABLE = 5
OBJECT_TYPE_ENTRY_ARRAY = 6

OBJECT_HEADER_SIZE = 16

OBJECT_COMPRESSED_XZ = 1 << 0
OBJECT_COMPRESSED_LZ4 = 1 << 1
OBJECT_COMPRESSED_ZSTD = 1 << 2

ENTRY_OBJECT_HEADER_SIZE = 64
DATA_OBJECT_HEADER_SIZE = 64
FIELD_OBJECT_HEADER_SIZE = 40
HASH_ITEM_SIZE = 16
OFFSET_ARRAY_OBJECT_HEADER_SIZE = 24
REGULAR_ENTRY_ITEM_SIZE = 16

DEFAULT_DATA_HASH_BUCKETS = 4096
DEFAULT_FIELD_HASH_BUCKETS = 512
INITIAL_ENTRY_ARRAY_CAP = 4096
INITIAL_DATA_ENTRY_ARRAY_CAP = 64


def parse_object_header(buf, offset=0):
    if len(buf) < offset + OBJECT_HEADER_SIZE:
        return None
    return {
        'type': buf[offset],
        'flags': buf[offset + 1],
        'size': read_uint64_le(buf, offset + 8),
    }


def write_object_header(buf, offset, obj_type, flags, size):
    buf[offset] = obj_type
    buf[offset + 1] = flags
    write_uint64_le(buf, offset + 8, size)


def parse_file_header(buf):
    if len(buf) < HEADER_SIZE:
        raise ValueError(f'header buffer too small: {len(buf)} < {HEADER_SIZE}')
    sig = buf[0:8].decode('latin1')
    if sig != 'LPKSHHRH':
        raise ValueError('invalid journal signature')
    return {
        'signature': sig,
        'compatible_flags': int.from_bytes(buf[8:12], 'little'),
        'incompatible_flags': int.from_bytes(buf[12:16], 'little'),
        'state': buf[16],
        'file_id': bytes(buf[24:40]),
        'machine_id': bytes(buf[40:56]),
        'tail_entry_boot_id': bytes(buf[56:72]),
        'seqnum_id': bytes(buf[72:88]),
        'header_size': read_uint64_le(buf, 88),
        'arena_size': read_uint64_le(buf, 96),
        'data_hash_table_offset': read_uint64_le(buf, 104),
        'data_hash_table_size': read_uint64_le(buf, 112),
        'field_hash_table_offset': read_uint64_le(buf, 120),
        'field_hash_table_size': read_uint64_le(buf, 128),
        'tail_object_offset': read_uint64_le(buf, 136),
        'n_objects': read_uint64_le(buf, 144),
        'n_entries': read_uint64_le(buf, 152),
        'tail_entry_seqnum': read_uint64_le(buf, 160),
        'head_entry_seqnum': read_uint64_le(buf, 168),
        'entry_array_offset': read_uint64_le(buf, 176),
        'head_entry_realtime': read_uint64_le(buf, 184),
        'tail_entry_realtime': read_uint64_le(buf, 192),
        'tail_entry_monotonic': read_uint64_le(buf, 200),
    }


def serialize_file_header(buf, h):
    if len(buf) < HEADER_SIZE:
        raise ValueError(f'buffer too small for header: {len(buf)}')
    buf[0:8] = b'LPKSHHRH'
    struct.pack_into('<I', buf, 8, h.get('compatible_flags', 0))
    struct.pack_into('<I', buf, 12, h.get('incompatible_flags', 0))
    buf[16] = h.get('state', STATE_OFFLINE)
    buf[24:40] = h.get('file_id', b'\x00' * 16)
    buf[40:56] = h.get('machine_id', b'\x00' * 16)
    buf[56:72] = h.get('tail_entry_boot_id', b'\x00' * 16)
    buf[72:88] = h.get('seqnum_id', b'\x00' * 16)
    write_uint64_le(buf, 88, h.get('header_size', HEADER_SIZE))
    write_uint64_le(buf, 96, h.get('arena_size', 0))
    write_uint64_le(buf, 104, h.get('data_hash_table_offset', 0))
    write_uint64_le(buf, 112, h.get('data_hash_table_size', 0))
    write_uint64_le(buf, 120, h.get('field_hash_table_offset', 0))
    write_uint64_le(buf, 128, h.get('field_hash_table_size', 0))
    write_uint64_le(buf, 136, h.get('tail_object_offset', 0))
    write_uint64_le(buf, 144, h.get('n_objects', 0))
    write_uint64_le(buf, 152, h.get('n_entries', 0))
    write_uint64_le(buf, 160, h.get('tail_entry_seqnum', 0))
    write_uint64_le(buf, 168, h.get('head_entry_seqnum', 0))
    write_uint64_le(buf, 176, h.get('entry_array_offset', 0))
    write_uint64_le(buf, 184, h.get('head_entry_realtime', 0))
    write_uint64_le(buf, 192, h.get('tail_entry_realtime', 0))
    write_uint64_le(buf, 200, h.get('tail_entry_monotonic', 0))
