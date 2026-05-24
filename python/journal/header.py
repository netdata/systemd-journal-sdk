# Journal file header parsing and writing.
# Layout matches Go/Node.js format exactly.

import struct

from .binary import read_uint64_le, write_uint64_le, write_uint32_le, write_uint8

HEADER_MIN_SIZE = 208
HEADER_SIZE = 272  # v260+ writer header size

STATE_OFFLINE = 0
STATE_ONLINE = 1
STATE_ARCHIVED = 2

INCOMPATIBLE_COMPRESSED_XZ = 1 << 0
INCOMPATIBLE_COMPRESSED_LZ4 = 1 << 1
INCOMPATIBLE_KEYED_HASH = 1 << 2
INCOMPATIBLE_COMPRESSED_ZSTD = 1 << 3
INCOMPATIBLE_COMPACT = 1 << 4

# HEADER_COMPATIBLE_TAIL_ENTRY_BOOT_ID - set for new files (v260+)
COMPATIBLE_TAIL_ENTRY_BOOT_ID = 1 << 1

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

DEFAULT_DATA_HASH_BUCKETS = 116508
DEFAULT_FIELD_HASH_BUCKETS = 1023
FILE_SIZE_INCREASE = 8 * 1024 * 1024
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
    if len(buf) < HEADER_MIN_SIZE:
        raise ValueError(f'header buffer too small: {len(buf)} < {HEADER_MIN_SIZE}')
    sig = buf[0:8].decode('latin1')
    if sig != 'LPKSHHRH':
        raise ValueError('invalid journal signature')
    header = {
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
        'n_data': 0,
        'n_fields': 0,
        'n_tags': 0,
        'n_entry_arrays': 0,
        'data_hash_chain_depth': 0,
        'field_hash_chain_depth': 0,
        'tail_entry_array_offset': 0,
        'tail_entry_array_n_entries': 0,
        'tail_entry_offset': 0,
    }
    if header['header_size'] >= HEADER_SIZE:
        if len(buf) < HEADER_SIZE:
            raise ValueError(f'header buffer too small: {len(buf)} < {HEADER_SIZE}')
        header['n_data'] = read_uint64_le(buf, 208)
        header['n_fields'] = read_uint64_le(buf, 216)
        header['n_tags'] = read_uint64_le(buf, 224)
        header['n_entry_arrays'] = read_uint64_le(buf, 232)
        header['data_hash_chain_depth'] = read_uint64_le(buf, 240)
        header['field_hash_chain_depth'] = read_uint64_le(buf, 248)
        header['tail_entry_array_offset'] = int.from_bytes(buf[256:260], 'little')
        header['tail_entry_array_n_entries'] = int.from_bytes(buf[260:264], 'little')
        header['tail_entry_offset'] = read_uint64_le(buf, 264)
    return header


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
    # Added in 187
    write_uint64_le(buf, 208, h.get('n_data', 0))
    write_uint64_le(buf, 216, h.get('n_fields', 0))
    # Added in 189
    write_uint64_le(buf, 224, h.get('n_tags', 0))
    write_uint64_le(buf, 232, h.get('n_entry_arrays', 0))
    # Added in 246
    write_uint64_le(buf, 240, h.get('data_hash_chain_depth', 0))
    write_uint64_le(buf, 248, h.get('field_hash_chain_depth', 0))
    # Added in 252
    write_uint32_le(buf, 256, h.get('tail_entry_array_offset', 0))
    write_uint32_le(buf, 260, h.get('tail_entry_array_n_entries', 0))
    # Added in 254
    write_uint64_le(buf, 264, h.get('tail_entry_offset', 0))
