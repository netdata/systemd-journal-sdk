# Journal entry and object parsing helpers.

from .binary import read_uint64_le
from .header import (
    OBJECT_TYPE_ENTRY, OBJECT_TYPE_DATA, OBJECT_HEADER_SIZE,
    ENTRY_OBJECT_HEADER_SIZE, DATA_OBJECT_HEADER_SIZE,
    OBJECT_COMPRESSED_ZSTD, OBJECT_COMPRESSED_XZ, OBJECT_COMPRESSED_LZ4,
    REGULAR_ENTRY_ITEM_SIZE,
)
from .compress import _HAS_ZSTD, decompress_zst_sync


def parse_entry_object(buf, offset):
    obj_type = buf[offset]
    if obj_type != OBJECT_TYPE_ENTRY:
        raise ValueError(f'expected ENTRY (type {OBJECT_TYPE_ENTRY}), got type {obj_type} at offset {offset}')
    obj_size = read_uint64_le(buf, offset + 8)
    if obj_size < ENTRY_OBJECT_HEADER_SIZE:
        raise ValueError(f'entry object too small: {obj_size}')

    e_off = offset + OBJECT_HEADER_SIZE
    seqnum = read_uint64_le(buf, e_off)
    realtime = read_uint64_le(buf, e_off + 8)
    monotonic = read_uint64_le(buf, e_off + 16)
    boot_id = bytes(buf[e_off + 24:e_off + 40])
    xor_hash = read_uint64_le(buf, e_off + 40)

    items_start = offset + ENTRY_OBJECT_HEADER_SIZE
    n_items = (obj_size - ENTRY_OBJECT_HEADER_SIZE) // REGULAR_ENTRY_ITEM_SIZE
    items = []
    for i in range(n_items):
        i_off = items_start + i * REGULAR_ENTRY_ITEM_SIZE
        data_offset = read_uint64_le(buf, i_off)
        data_hash = read_uint64_le(buf, i_off + 8)
        if data_offset != 0:
            items.append({'offset': data_offset, 'hash': data_hash})

    return {
        'seqnum': seqnum,
        'realtime': realtime,
        'monotonic': monotonic,
        'boot_id': boot_id,
        'xor_hash': xor_hash,
        'items': items,
    }


def parse_data_object(buf, offset):
    if len(buf) < offset + DATA_OBJECT_HEADER_SIZE:
        raise ValueError('buffer too small for data object')
    obj_type = buf[offset]
    obj_flags = buf[offset + 1]
    obj_size = read_uint64_le(buf, offset + 8)

    if obj_type != OBJECT_TYPE_DATA:
        raise ValueError(f'expected DATA (type {OBJECT_TYPE_DATA}), got type {obj_type}')
    if obj_size < DATA_OBJECT_HEADER_SIZE:
        raise ValueError(f'data object too small: {obj_size}')

    payload = buf[offset + DATA_OBJECT_HEADER_SIZE:offset + obj_size]

    unsupported = obj_flags & ~(OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD)
    if unsupported != 0:
        raise ValueError(f'unsupported DATA object flags: 0x{obj_flags:x}')
    if obj_flags & (OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4):
        raise ValueError(f'unsupported DATA object compression flags: 0x{obj_flags:x}')
    if obj_flags & OBJECT_COMPRESSED_ZSTD:
        if not _HAS_ZSTD:
            raise RuntimeError('zstd decompression not available')
        payload = _decompress_zstd(payload)

    eq_pos = payload.find(b'=')
    if eq_pos < 0:
        raise ValueError('DATA object missing field separator')

    return {
        'name': payload[:eq_pos],
        'value': payload[eq_pos + 1:],
    }


def _decompress_zstd(data):
    return decompress_zst_sync(data)
