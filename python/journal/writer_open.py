import os

from ._platform_io import read_at as _read_fd_at, sync_parent_directory
from .binary import align8, buf_equal, is_zero_uuid, read_uint64_le
from .compress import MAX_UNCOMPRESSED_SIZE
from .header import (
    HEADER_SIZE, INCOMPATIBLE_COMPACT, INCOMPATIBLE_COMPRESSED_LZ4,
    INCOMPATIBLE_COMPRESSED_XZ, INCOMPATIBLE_COMPRESSED_ZSTD,
    INCOMPATIBLE_KEYED_HASH, STATE_ONLINE, parse_file_header, parse_object_header,
)
from .writer_compression import (
    COMPRESSION_LZ4, COMPRESSION_NONE, COMPRESSION_XZ, COMPRESSION_ZSTD,
    DEFAULT_COMPRESS_THRESHOLD, _ensure_lz4_available, _ensure_xz_available,
    _ensure_zstd_available,
)
from .writer_options import _current_time_ms, _normalize_live_publish_every_entries, _uuid_option
from .writer_policy import _normalize_field_name_policy


def _read_object_size_from_fd(fd, offset):
    buf = _read_fd_at(fd, 8, offset + 8)
    return read_uint64_le(buf, 0)


def _sync_parent_directory(path):
    return sync_parent_directory(path)


def _read_append_header(fd):
    header_buf = os.read(fd, HEADER_SIZE)
    if len(header_buf) < HEADER_SIZE:
        raise ValueError('cannot read journal header')
    header = parse_file_header(header_buf)
    return header, _compression_from_append_header(header)


def _compression_from_append_header(header):
    flags = header['incompatible_flags']
    _validate_append_header_flags(header, flags)
    if flags & INCOMPATIBLE_COMPRESSED_XZ:
        _ensure_xz_available()
        return COMPRESSION_XZ
    if flags & INCOMPATIBLE_COMPRESSED_LZ4:
        _ensure_lz4_available()
        return COMPRESSION_LZ4
    if flags & INCOMPATIBLE_COMPRESSED_ZSTD:
        _ensure_zstd_available()
        return COMPRESSION_ZSTD
    return COMPRESSION_NONE


def _validate_append_header_flags(header, flags):
    supported = (
        INCOMPATIBLE_KEYED_HASH | INCOMPATIBLE_COMPRESSED_ZSTD |
        INCOMPATIBLE_COMPRESSED_XZ | INCOMPATIBLE_COMPRESSED_LZ4 |
        INCOMPATIBLE_COMPACT
    )
    if flags & ~supported:
        raise ValueError(f'unsupported journal: incompatible flags 0x{flags:x}')
    if not (flags & INCOMPATIBLE_KEYED_HASH):
        raise ValueError('unsupported journal: keyed hash required')
    if header['header_size'] < HEADER_SIZE:
        raise ValueError('unsupported journal: outdated header')
    if _append_header_missing_hash_tables(header):
        raise ValueError('invalid journal: missing hash tables')


def _append_header_missing_hash_tables(header):
    return (
        header['data_hash_table_offset'] == 0 or
        header['field_hash_table_offset'] == 0 or
        header['tail_object_offset'] == 0
    )


def _configure_opened_writer(writer, fd, header, compression, opts):
    tail_size = _read_object_size_from_fd(fd, header['tail_object_offset'])
    writer._header = header
    writer._append_offset = align8(header['tail_object_offset'] + tail_size)
    writer._next_seqnum = header['tail_entry_seqnum'] + 1
    writer._boot_id = _opened_writer_boot_id(header, opts)
    writer._started = _current_time_ms() - _opened_writer_monotonic_base(header)
    writer._compression = compression
    writer._compress_threshold = DEFAULT_COMPRESS_THRESHOLD
    writer._compact = bool(header['incompatible_flags'] & INCOMPATIBLE_COMPACT)
    writer._live_publish_every_entries = _normalize_live_publish_every_entries(
        opts.get('live_publish_every_entries', opts.get('livePublishEveryEntries'))
    )
    writer._field_name_policy = _normalize_field_name_policy(
        opts.get('field_name_policy', opts.get('fieldNamePolicy'))
    )
    writer._map_arena(max(os.fstat(fd).st_size, header['header_size'] + header['arena_size']))
    writer._header['state'] = STATE_ONLINE
    writer._write_header()


def _opened_writer_boot_id(header, opts):
    boot_id = header['tail_entry_boot_id']
    if is_zero_uuid(boot_id):
        return _uuid_option(opts.get('boot_id', opts.get('bootId')), header['file_id'])
    return boot_id


def _opened_writer_monotonic_base(header):
    if header['tail_entry_monotonic'] > 0:
        return header['tail_entry_monotonic'] // 1000
    return 0
