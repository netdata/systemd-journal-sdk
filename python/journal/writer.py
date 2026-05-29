# Journal file writer. Creates regular-by-default keyed-hash journal files.
# Default options are compatible with stock journalctl readers during live append.

import os
import struct
import fcntl
import lzma
import mmap
from .lock import WriterLock
from .binary import (
    read_uint64_le, write_uint64_le, write_uint32_le, write_uint8,
    align8, random_uuid, is_zero_uuid, buf_equal,
)
from .header import (
    serialize_file_header, parse_file_header, parse_object_header, write_object_header,
    HEADER_SIZE, OBJECT_TYPE_DATA, OBJECT_TYPE_ENTRY,
    OBJECT_TYPE_DATA_HASH_TABLE, OBJECT_TYPE_FIELD_HASH_TABLE,
    OBJECT_TYPE_ENTRY_ARRAY, OBJECT_TYPE_FIELD,
    STATE_OFFLINE, STATE_ONLINE, STATE_ARCHIVED,
    INCOMPATIBLE_KEYED_HASH, INCOMPATIBLE_COMPRESSED_ZSTD,
    INCOMPATIBLE_COMPRESSED_XZ, INCOMPATIBLE_COMPRESSED_LZ4, INCOMPATIBLE_COMPACT,
    COMPATIBLE_TAIL_ENTRY_BOOT_ID,
    OBJECT_HEADER_SIZE, ENTRY_OBJECT_HEADER_SIZE, DATA_OBJECT_HEADER_SIZE,
    FIELD_OBJECT_HEADER_SIZE, HASH_ITEM_SIZE, OFFSET_ARRAY_OBJECT_HEADER_SIZE,
    REGULAR_ENTRY_ITEM_SIZE, OBJECT_COMPRESSED_XZ, OBJECT_COMPRESSED_LZ4, OBJECT_COMPRESSED_ZSTD,
    COMPACT_ENTRY_ITEM_SIZE, COMPACT_OFFSET_ARRAY_ITEM_SIZE, REGULAR_OFFSET_ARRAY_ITEM_SIZE,
    COMPACT_DATA_OBJECT_HEADER_SIZE, COMPACT_DATA_TAIL_OFFSET_OFFSET,
    COMPACT_DATA_TAIL_ENTRIES_OFFSET, JOURNAL_COMPACT_SIZE_MAX,
    DEFAULT_DATA_HASH_BUCKETS, DEFAULT_FIELD_HASH_BUCKETS, FILE_SIZE_INCREASE,
    normalize_journal_max_file_size, data_hash_buckets_for_max_file_size,
    INITIAL_ENTRY_ARRAY_CAP, INITIAL_DATA_ENTRY_ARRAY_CAP,
)
from .hash import sip_hash_24, jenkins_hash_64
from .compress import MAX_UNCOMPRESSED_SIZE, decompress_zst_sync, decompress_xz_sync, decompress_lz4_sync
from .seal import SealOptions, SealState, TAG_LENGTH, OBJECT_TYPE_TAG, COMPATIBLE_SEALED, COMPATIBLE_SEALED_CONTINUOUS

COMPRESSION_NONE = 0
COMPRESSION_ZSTD = 1
COMPRESSION_XZ = 2
COMPRESSION_LZ4 = 3
DEFAULT_COMPRESS_THRESHOLD = 512
MIN_COMPRESS_THRESHOLD = 8
FIELD_NAME_POLICY_JOURNALD = 'journald'
FIELD_NAME_POLICY_RAW = 'raw'
FIELD_NAME_POLICY_JOURNAL_APP = 'journal-app'
FIELD_CACHE_MAX_ENTRIES = 1024
FIELD_CACHE_MAX_PAYLOAD_LEN = 128


class _MappedArena:
    def __init__(self, fd, size):
        self._fd = fd
        self._mmap = None
        self._size = 0
        self.resize(size)

    def resize(self, size):
        size = int(size)
        if size <= 0:
            raise ValueError('mapped arena size must be positive')
        if self._mmap is not None and size == self._size:
            return
        os.ftruncate(self._fd, size)
        if self._mmap is None:
            self._mmap = mmap.mmap(self._fd, size, access=mmap.ACCESS_WRITE)
        elif size != self._size:
            try:
                self._mmap.resize(size)
            except Exception:
                self._mmap.flush()
                self._mmap.close()
                self._mmap = None
                self._mmap = mmap.mmap(self._fd, size, access=mmap.ACCESS_WRITE)
        self._size = size

    def read_at(self, offset, size):
        end = int(offset) + int(size)
        if offset < 0 or size < 0 or end > self._size:
            raise ValueError('mapped arena read out of bounds')
        return self._mmap[offset:end]

    def write_at(self, offset, data):
        end = int(offset) + len(data)
        if offset < 0 or end > self._size:
            raise ValueError('mapped arena write out of bounds')
        self._mmap[offset:end] = data

    def flush(self):
        if self._mmap is not None:
            self._mmap.flush()

    def close(self):
        if self._mmap is None:
            return
        self._mmap.close()
        self._mmap = None


class Writer:
    def __init__(self, fd, path, lock):
        self._fd = fd
        self._path = path
        self._lock = lock
        self._header = None
        self._append_offset = 0
        self._next_seqnum = 1
        self._boot_id = None
        self._started = 0
        self._closed = False
        self._compression = COMPRESSION_NONE
        self._compress_threshold = DEFAULT_COMPRESS_THRESHOLD
        self._compact = False
        self._seal = None
        self._live_publish_every_entries = 1
        self._entries_since_live_publication = 0
        self._field_name_policy = FIELD_NAME_POLICY_JOURNALD
        self._arena = None
        self._field_cache = {}

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        try:
            self.close()
        except Exception:
            if exc_type is None:
                raise
        return False

    @staticmethod
    def create(path, opts=None):
        opts = opts or {}
        lock = WriterLock.acquire(path)
        fd = None
        try:
            fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o640)
            _lock_fd(fd)
            os.ftruncate(fd, 0)
            w = Writer(fd, path, lock)
            w._compression = _normalize_compression(opts.get('compression', COMPRESSION_NONE))
            w._compact = opts.get('compact') is True or opts.get('format') == 'compact'
            w._live_publish_every_entries = _normalize_live_publish_every_entries(
                opts.get('live_publish_every_entries', opts.get('livePublishEveryEntries'))
            )
            w._field_name_policy = _normalize_field_name_policy(
                opts.get('field_name_policy', opts.get('fieldNamePolicy'))
            )
            if w._compression == COMPRESSION_ZSTD:
                _ensure_zstd_available()
            elif w._compression == COMPRESSION_XZ:
                _ensure_xz_available()
            elif w._compression == COMPRESSION_LZ4:
                _ensure_lz4_available()
            w._compress_threshold = _normalize_compress_threshold(opts.get('compression_threshold_bytes'))
            seal_opts = opts.get('seal')
            if seal_opts is not None:
                w._seal = SealState(seal_opts)
            w._initialize(opts)
            return w
        except Exception:
            if fd is not None:
                os.close(fd)
            lock.release()
            raise

    @staticmethod
    def open(path, opts=None):
        opts = opts or {}
        lock = WriterLock.acquire(path)
        try:
            fd = os.open(path, os.O_RDWR)
        except Exception:
            lock.release()
            raise
        try:
            _lock_fd(fd)
            header_buf = os.read(fd, HEADER_SIZE)
            if len(header_buf) < HEADER_SIZE:
                raise ValueError('cannot read journal header')

            header = parse_file_header(header_buf)
            flags = header['incompatible_flags']
            supported_writer_incompatible = (
                INCOMPATIBLE_KEYED_HASH | INCOMPATIBLE_COMPRESSED_ZSTD |
                INCOMPATIBLE_COMPRESSED_XZ | INCOMPATIBLE_COMPRESSED_LZ4 |
                INCOMPATIBLE_COMPACT
            )
            if flags & ~supported_writer_incompatible:
                raise ValueError(f'unsupported journal: incompatible flags 0x{flags:x}')
            if not (flags & INCOMPATIBLE_KEYED_HASH):
                raise ValueError('unsupported journal: keyed hash required')
            if header['data_hash_table_offset'] == 0 or header['field_hash_table_offset'] == 0 or header['tail_object_offset'] == 0:
                raise ValueError('invalid journal: missing hash tables')
            if flags & INCOMPATIBLE_COMPRESSED_XZ:
                compression = COMPRESSION_XZ
                _ensure_xz_available()
            elif flags & INCOMPATIBLE_COMPRESSED_LZ4:
                compression = COMPRESSION_LZ4
                _ensure_lz4_available()
            elif flags & INCOMPATIBLE_COMPRESSED_ZSTD:
                compression = COMPRESSION_ZSTD
                _ensure_zstd_available()
            else:
                compression = COMPRESSION_NONE
        except Exception:
            os.close(fd)
            lock.release()
            raise

        try:
            tail_size = _read_object_size_from_fd(fd, header['tail_object_offset'])
            now_ms = _current_time_ms()
            monotonic_base = header['tail_entry_monotonic'] // 1000 if header['tail_entry_monotonic'] > 0 else 0

            w = Writer(fd, path, lock)
            w._header = header
            w._append_offset = align8(header['tail_object_offset'] + tail_size)
            w._next_seqnum = header['tail_entry_seqnum'] + 1
            w._boot_id = header['tail_entry_boot_id']
            if is_zero_uuid(w._boot_id):
                w._boot_id = _read_boot_id() or header['file_id']
            w._started = now_ms - monotonic_base
            w._compression = compression
            w._compress_threshold = DEFAULT_COMPRESS_THRESHOLD
            w._compact = bool(flags & INCOMPATIBLE_COMPACT)
            w._live_publish_every_entries = _normalize_live_publish_every_entries(
                opts.get('live_publish_every_entries', opts.get('livePublishEveryEntries'))
            )
            w._field_name_policy = _normalize_field_name_policy(
                opts.get('field_name_policy', opts.get('fieldNamePolicy'))
            )
            w._map_arena(max(os.fstat(fd).st_size, header['header_size'] + header['arena_size']))

            w._header['state'] = STATE_ONLINE
            w._write_header()
            return w
        except Exception:
            os.close(fd)
            lock.release()
            raise

    def _initialize(self, opts):
        max_file_size = normalize_journal_max_file_size(
            opts.get('max_file_size', opts.get('maxFileSize')),
            self._compact,
        )
        data_buckets = opts.get(
            'data_hash_table_buckets',
            opts.get('dataHashTableBuckets', data_hash_buckets_for_max_file_size(max_file_size)),
        )
        field_buckets = opts.get(
            'field_hash_table_buckets',
            opts.get('fieldHashTableBuckets', DEFAULT_FIELD_HASH_BUCKETS),
        )

        data_size = data_buckets * HASH_ITEM_SIZE
        field_size = field_buckets * HASH_ITEM_SIZE
        # systemd creates FIELD_HASH_TABLE first, then DATA_HASH_TABLE
        field_obj_offset = HEADER_SIZE
        field_offset = field_obj_offset + OBJECT_HEADER_SIZE
        data_obj_offset = align8(field_offset + field_size)
        data_offset = data_obj_offset + OBJECT_HEADER_SIZE
        append_offset = align8(data_offset + data_size)
        file_size = ((append_offset + FILE_SIZE_INCREASE - 1) // FILE_SIZE_INCREASE) * FILE_SIZE_INCREASE
        if self._compact and file_size > JOURNAL_COMPACT_SIZE_MAX:
            raise ValueError('compact journal cannot exceed 4 GiB')

        file_id = _uuid_option(opts.get('file_id'), random_uuid())
        machine_id = _uuid_option(opts.get('machine_id'), random_uuid())
        boot_id = _uuid_option(opts.get('boot_id'), random_uuid())
        seqnum_id = _uuid_option(opts.get('seqnum_id'), random_uuid())

        inc_flags = INCOMPATIBLE_KEYED_HASH
        if self._compression == COMPRESSION_ZSTD:
            inc_flags |= INCOMPATIBLE_COMPRESSED_ZSTD
        elif self._compression == COMPRESSION_XZ:
            inc_flags |= INCOMPATIBLE_COMPRESSED_XZ
        elif self._compression == COMPRESSION_LZ4:
            inc_flags |= INCOMPATIBLE_COMPRESSED_LZ4
        if self._compact:
            inc_flags |= INCOMPATIBLE_COMPACT

        compatible_flags = COMPATIBLE_TAIL_ENTRY_BOOT_ID
        if self._seal is not None:
            compatible_flags |= COMPATIBLE_SEALED | COMPATIBLE_SEALED_CONTINUOUS

        self._header = {
            'signature': 'LPKSHHRH',
            'compatible_flags': compatible_flags,
            'incompatible_flags': inc_flags,
            'state': STATE_ONLINE,
            'file_id': file_id,
            'machine_id': machine_id,
            'tail_entry_boot_id': b'\x00' * 16,
            'seqnum_id': seqnum_id,
            'header_size': HEADER_SIZE,
            'arena_size': file_size - HEADER_SIZE,
            'data_hash_table_offset': data_offset,
            'data_hash_table_size': data_size,
            'field_hash_table_offset': field_offset,
            'field_hash_table_size': field_size,
            'tail_object_offset': data_obj_offset,
            'n_objects': 2,
            'n_entries': 0,
            'tail_entry_seqnum': 0,
            'head_entry_seqnum': 0,
            'entry_array_offset': 0,
            'head_entry_realtime': 0,
            'tail_entry_realtime': 0,
            'tail_entry_monotonic': 0,
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

        self._boot_id = boot_id
        self._append_offset = append_offset
        self._next_seqnum = opts.get('head_seqnum', 1) or 1

        os.ftruncate(self._fd, file_size)
        self._map_arena(file_size)
        self._write_header()

        # systemd writes FIELD hash table first, then DATA hash table
        fht_buf = bytearray(OBJECT_HEADER_SIZE)
        write_object_header(fht_buf, 0, OBJECT_TYPE_FIELD_HASH_TABLE, 0, OBJECT_HEADER_SIZE + field_size)
        self._write_at(field_obj_offset, fht_buf)

        dht_buf = bytearray(OBJECT_HEADER_SIZE)
        write_object_header(dht_buf, 0, OBJECT_TYPE_DATA_HASH_TABLE, 0, OBJECT_HEADER_SIZE + data_size)
        self._write_at(data_obj_offset, dht_buf)

        if self._seal is not None:
            self._append_first_tag()

    def _write_header(self):
        buf = bytearray(HEADER_SIZE)
        serialize_file_header(buf, self._header)
        self._write_at(0, buf)

    def _write_uint64_at(self, offset, value):
        buf = struct.pack('<Q', value)
        self._write_at(offset, buf)

    def _write_uuid_at(self, offset, uuid):
        self._write_at(offset, uuid)

    def _map_arena(self, size):
        self._arena = _MappedArena(self._fd, size)

    def _resize_arena(self, size):
        if self._arena is None:
            self._map_arena(size)
        else:
            self._arena.resize(size)

    def _read_at(self, offset, size):
        if self._arena is not None:
            return self._arena.read_at(offset, size)
        return os.pread(self._fd, size, offset)

    def _write_at(self, offset, data):
        if self._arena is not None:
            self._arena.write_at(offset, data)
        else:
            os.pwrite(self._fd, data, offset)

    def _read_object_size(self, offset):
        buf = self._read_at(offset + 8, 8)
        return read_uint64_le(buf, 0)

    def _read_object_header(self, offset):
        buf = self._read_at(offset, OBJECT_HEADER_SIZE)
        return parse_object_header(buf, 0)

    def append(self, fields, opts=None):
        if self._closed:
            raise ValueError('writer closed')
        fields = _prepare_fields_for_policy(fields, self._field_name_policy)
        payloads = []
        for field in fields:
            name = field['name']
            name_bytes = _field_name_bytes(name)
            value = field['value']
            if isinstance(value, str):
                value = value.encode('utf-8')
            elif isinstance(value, (bytearray, memoryview)):
                value = bytes(value)
            elif not isinstance(value, bytes):
                value = bytes(value)
            payloads.append(name_bytes + b'=' + value)
        return self._append_payloads(payloads, opts)

    def append_raw(self, payloads, opts=None):
        if self._closed:
            raise ValueError('writer closed')
        return self._append_payloads(
            _prepare_raw_payloads_for_policy(payloads, self._field_name_policy),
            opts,
        )

    def _append_payloads(self, payloads, opts=None):
        opts = opts or {}
        if len(payloads) == 0:
            raise ValueError('empty entry')
        now_ms = _current_time_ms()
        realtime = opts['realtime_usec'] if 'realtime_usec' in opts else now_ms * 1000
        monotonic = opts['monotonic_usec'] if 'monotonic_usec' in opts else (now_ms - self._started) * 1000
        boot_id = opts.get('boot_id')
        if boot_id is None or is_zero_uuid(boot_id):
            boot_id = self._boot_id
        if isinstance(boot_id, str):
            boot_id = bytes.fromhex(boot_id)

        self._maybe_append_tag(realtime)

        items = []
        xor_hash = 0
        for payload in payloads:
            off, h = self._add_data(payload)
            items.append({'offset': off, 'hash': h})
            xor_hash ^= jenkins_hash_64(payload)

        items.sort(key=lambda x: x['offset'])
        deduped = [items[0]]
        for i in range(1, len(items)):
            if items[i]['offset'] != deduped[-1]['offset']:
                deduped.append(items[i])

        entry_offset = self._append_offset
        entry_item_size = self._entry_item_size()
        entry_size = ENTRY_OBJECT_HEADER_SIZE + len(deduped) * entry_item_size
        self._ensure_compact_object_fits(entry_offset, entry_size)
        aligned_size = align8(entry_size)
        self._ensure_arena_size(entry_offset + aligned_size)
        entry_buf = bytearray(aligned_size)
        write_object_header(entry_buf, 0, OBJECT_TYPE_ENTRY, 0, entry_size)
        struct.pack_into('<Q', entry_buf, 16, self._next_seqnum)
        struct.pack_into('<Q', entry_buf, 24, realtime)
        struct.pack_into('<Q', entry_buf, 32, monotonic)
        entry_buf[40:56] = boot_id
        struct.pack_into('<Q', entry_buf, 56, xor_hash)
        for i, item in enumerate(deduped):
            off = ENTRY_OBJECT_HEADER_SIZE + i * entry_item_size
            if self._compact:
                self._ensure_compact_offset(item['offset'])
                struct.pack_into('<I', entry_buf, off, item['offset'])
            else:
                struct.pack_into('<Q', entry_buf, off, item['offset'])
                struct.pack_into('<Q', entry_buf, off + 8, item['hash'])
        self._write_at(entry_offset, entry_buf)
        self._object_added(entry_offset, entry_size)

        self._hmac_put_object(entry_offset, OBJECT_TYPE_ENTRY)

        self._publish_object_metadata()
        self._append_to_entry_array(entry_offset)
        for item in deduped:
            self._link_data_to_entry(item['offset'], entry_offset)

        self._entry_added(entry_offset, realtime, monotonic, boot_id)
        self._publish_entry_metadata()
        self._publish_after_entry()

        return {'realtime': realtime, 'seqnum': self._next_seqnum - 1}

    def _hash(self, payload):
        return sip_hash_24(self._header['file_id'], payload)

    def _add_data(self, payload):
        h = self._hash(payload)
        existing = self._find_data(h, payload)
        if existing is not None:
            return existing, h

        offset = self._append_offset

        object_payload = payload
        compression_flag = 0
        if self._compression == COMPRESSION_ZSTD and len(payload) >= self._compress_threshold:
            try:
                compressed = _zstd_compress(payload)
                if len(compressed) < len(payload):
                    object_payload = compressed
                    compression_flag = OBJECT_COMPRESSED_ZSTD
            except Exception:
                pass
        elif self._compression == COMPRESSION_XZ and len(payload) >= self._compress_threshold and len(payload) >= 80:
            try:
                compressed = _xz_compress(payload)
                if len(compressed) < len(payload):
                    object_payload = compressed
                    compression_flag = OBJECT_COMPRESSED_XZ
            except Exception:
                pass
        elif self._compression == COMPRESSION_LZ4 and len(payload) >= self._compress_threshold and len(payload) >= 9:
            try:
                compressed = _lz4_compress(payload)
                if len(compressed) < len(payload):
                    object_payload = compressed
                    compression_flag = OBJECT_COMPRESSED_LZ4
            except Exception:
                pass

        payload_offset = self._data_payload_offset()
        size = payload_offset + len(object_payload)
        self._ensure_compact_object_fits(offset, size)
        aligned_size = align8(size)
        self._ensure_arena_size(offset + aligned_size)
        buf = bytearray(aligned_size)
        write_object_header(buf, 0, OBJECT_TYPE_DATA, compression_flag, size)
        struct.pack_into('<Q', buf, 16, h)
        buf[payload_offset:payload_offset + len(object_payload)] = object_payload
        self._write_at(offset, buf)
        self._object_added(offset, size)

        self._append_hash_item(
            self._header['data_hash_table_offset'],
            self._header['data_hash_table_size'],
            OBJECT_TYPE_DATA, h, offset)
        self._header['n_data'] += 1

        self._hmac_put_object(offset, OBJECT_TYPE_DATA)

        eq_pos = payload.find(b'=')
        if eq_pos > 0:
            field_payload = payload[:eq_pos]
            field_offset = self._add_field(field_payload)
            field_head_data = self._read_field_head_data_offset(field_offset)
            self._write_uint64_at(offset + 32, field_head_data)
            self._write_uint64_at(field_offset + 32, offset)

        return offset, h

    def _add_field(self, payload):
        cached = self._field_cache.get(payload)
        if cached is not None:
            return cached
        h = self._hash(payload)
        existing = self._find_field(h, payload)
        if existing is not None:
            self._cache_field(payload, existing)
            return existing

        offset = self._append_offset
        size = FIELD_OBJECT_HEADER_SIZE + len(payload)
        self._ensure_compact_object_fits(offset, size)
        aligned_size = align8(size)
        self._ensure_arena_size(offset + aligned_size)
        buf = bytearray(aligned_size)
        write_object_header(buf, 0, OBJECT_TYPE_FIELD, 0, size)
        struct.pack_into('<Q', buf, 16, h)
        buf[FIELD_OBJECT_HEADER_SIZE:FIELD_OBJECT_HEADER_SIZE + len(payload)] = payload
        self._write_at(offset, buf)
        self._object_added(offset, size)

        self._append_hash_item(
            self._header['field_hash_table_offset'],
            self._header['field_hash_table_size'],
            OBJECT_TYPE_FIELD, h, offset)
        self._header['n_fields'] += 1

        self._hmac_put_object(offset, OBJECT_TYPE_FIELD)

        self._cache_field(payload, offset)
        return offset

    def _cache_field(self, payload, offset):
        if len(payload) > FIELD_CACHE_MAX_PAYLOAD_LEN:
            return
        if len(self._field_cache) >= FIELD_CACHE_MAX_ENTRIES and payload not in self._field_cache:
            self._field_cache.clear()
        self._field_cache[bytes(payload)] = offset

    def _find_data(self, h, payload):
        n_buckets = self._header['data_hash_table_size'] // HASH_ITEM_SIZE
        bucket_off = self._header['data_hash_table_offset'] + (h % n_buckets) * HASH_ITEM_SIZE
        item = self._read_hash_item(bucket_off)

        depth = 0
        offset = item['head']
        while offset != 0:
            stored = self._read_data_payload(offset)
            if stored and buf_equal(stored, payload):
                return offset
            next_hash = self._read_uint64_at(offset + 24)
            if next_hash != 0:
                depth += 1
                if depth > self._header['data_hash_chain_depth']:
                    self._header['data_hash_chain_depth'] = depth
            offset = next_hash
        return None

    def _find_field(self, h, payload):
        n_buckets = self._header['field_hash_table_size'] // HASH_ITEM_SIZE
        bucket_off = self._header['field_hash_table_offset'] + (h % n_buckets) * HASH_ITEM_SIZE
        item = self._read_hash_item(bucket_off)

        depth = 0
        offset = item['head']
        while offset != 0:
            stored = self._read_field_payload(offset)
            if stored and buf_equal(stored, payload):
                return offset
            next_hash = self._read_uint64_at(offset + 24)
            if next_hash != 0:
                depth += 1
                if depth > self._header['field_hash_chain_depth']:
                    self._header['field_hash_chain_depth'] = depth
            offset = next_hash
        return None

    def _read_hash_item(self, offset):
        buf = self._read_at(offset, HASH_ITEM_SIZE)
        return {
            'head': read_uint64_le(buf, 0),
            'tail': read_uint64_le(buf, 8),
        }

    def _write_hash_item(self, offset, item):
        buf = struct.pack('<QQ', item['head'], item['tail'])
        self._write_at(offset, buf)

    def _read_data_payload(self, offset):
        obj_header = self._read_object_header(offset)
        if not obj_header or obj_header['type'] != OBJECT_TYPE_DATA:
            return None
        obj_size = obj_header['size']
        payload_offset = self._data_payload_offset()
        payload_len = obj_size - payload_offset
        if payload_len <= 0:
            return None
        buf = self._read_at(offset + payload_offset, payload_len)
        flags = obj_header['flags']
        if flags & OBJECT_COMPRESSED_ZSTD:
            return decompress_zst_sync(buf, max_output_size=MAX_UNCOMPRESSED_SIZE)
        if flags & OBJECT_COMPRESSED_XZ:
            return decompress_xz_sync(buf, max_output_size=MAX_UNCOMPRESSED_SIZE)
        if flags & OBJECT_COMPRESSED_LZ4:
            return decompress_lz4_sync(buf)
        return buf

    def _read_field_payload(self, offset):
        obj_size = self._read_object_size(offset)
        payload_len = obj_size - FIELD_OBJECT_HEADER_SIZE
        if payload_len <= 0:
            return None
        buf = self._read_at(offset + FIELD_OBJECT_HEADER_SIZE, payload_len)
        return buf

    def _read_field_head_data_offset(self, offset):
        return self._read_uint64_at(offset + 32)

    def _read_uint64_at(self, offset):
        buf = self._read_at(offset, 8)
        return read_uint64_le(buf, 0)

    def _write_uint32_at(self, offset, value):
        self._write_at(offset, struct.pack('<I', value))

    def _append_hash_item(self, table_offset, table_size, expected_type, h, object_offset):
        n_buckets = table_size // HASH_ITEM_SIZE
        bucket_off = table_offset + (h % n_buckets) * HASH_ITEM_SIZE
        item = self._read_hash_item(bucket_off)

        if item['head'] != 0:
            head = self._read_object_header(item['head'])
            if not head or head['type'] != expected_type:
                raise ValueError('invalid journal: hash bucket object type mismatch')
        if item['tail'] != 0:
            self._write_uint64_at(item['tail'] + 24, object_offset)
        else:
            item['head'] = object_offset
        item['tail'] = object_offset
        self._write_hash_item(bucket_off, item)

    def _object_added(self, offset, size):
        self._header['tail_object_offset'] = offset
        self._append_offset = align8(offset + size)
        self._header['n_objects'] += 1
        self._ensure_arena_size(self._append_offset)

    def _ensure_arena_size(self, required_size):
        current_size = HEADER_SIZE + self._header['arena_size']
        if required_size <= current_size:
            return
        new_size = ((required_size + FILE_SIZE_INCREASE - 1) // FILE_SIZE_INCREASE) * FILE_SIZE_INCREASE
        if self._compact and new_size > JOURNAL_COMPACT_SIZE_MAX:
            raise ValueError('compact journal cannot exceed 4 GiB')
        self._header['arena_size'] = new_size - HEADER_SIZE
        self._resize_arena(new_size)

    def _entry_added(self, entry_offset, realtime, monotonic, boot_id):
        self._header['n_entries'] += 1
        if self._header['head_entry_seqnum'] == 0:
            self._header['head_entry_seqnum'] = self._next_seqnum
        if self._header['head_entry_realtime'] == 0:
            self._header['head_entry_realtime'] = realtime
        self._header['tail_entry_seqnum'] = self._next_seqnum
        self._header['tail_entry_realtime'] = realtime
        self._header['tail_entry_monotonic'] = monotonic
        self._header['tail_entry_boot_id'] = boot_id
        self._header['tail_entry_offset'] = entry_offset
        self._next_seqnum += 1

    def _publish_object_metadata(self):
        self._write_uint64_at(96, self._header['arena_size'])
        self._write_uint64_at(136, self._header['tail_object_offset'])
        self._write_uint64_at(144, self._header['n_objects'])
        self._write_uint64_at(208, self._header['n_data'])
        self._write_uint64_at(216, self._header['n_fields'])
        self._write_uint64_at(232, self._header['n_entry_arrays'])
        self._write_uint64_at(240, self._header['data_hash_chain_depth'])
        self._write_uint64_at(248, self._header['field_hash_chain_depth'])

    def _publish_entry_metadata(self):
        self._write_uuid_at(56, self._header['tail_entry_boot_id'])
        self._write_uint64_at(160, self._header['tail_entry_seqnum'])
        self._write_uint64_at(168, self._header['head_entry_seqnum'])
        self._write_uint64_at(176, self._header['entry_array_offset'])
        self._write_uint64_at(184, self._header['head_entry_realtime'])
        self._write_uint64_at(192, self._header['tail_entry_realtime'])
        self._write_uint64_at(200, self._header['tail_entry_monotonic'])
        self._write_uint32_at(256, self._header['tail_entry_array_offset'])
        self._write_uint32_at(260, self._header['tail_entry_array_n_entries'])
        self._write_uint64_at(264, self._header['tail_entry_offset'])
        self._write_uint64_at(152, self._header['n_entries'])

    def _post_change(self):
        os.ftruncate(self._fd, self._header['header_size'] + self._header['arena_size'])

    def _publish_after_entry(self):
        if self._live_publish_every_entries == 0:
            return
        if self._live_publish_every_entries == 1:
            self._post_change()
            return
        self._entries_since_live_publication += 1
        if self._entries_since_live_publication >= self._live_publish_every_entries:
            self._entries_since_live_publication = 0
            self._post_change()

    def _next_entry_array_capacity(self, index, previous_capacity):
        capacity = previous_capacity
        if index > capacity:
            capacity = (index + 1) * 2
        else:
            capacity *= 2
        return max(capacity, 4)

    def _append_to_entry_array(self, entry_offset):
        if self._header['entry_array_offset'] == 0:
            array_off = self._allocate_offset_array(4)
            self._header['entry_array_offset'] = array_off
            self._header['tail_entry_array_offset'] = array_off
            self._header['tail_entry_array_n_entries'] = 1
            self._write_array_item(array_off, 0, entry_offset)
            return

        tail_offset = self._header['tail_entry_array_offset']
        if tail_offset == 0:
            tail_offset = self._header['entry_array_offset']
            remaining = self._header['n_entries']
            while True:
                cap, next_off = self._read_offset_array_header(tail_offset)
                if remaining < cap or next_off == 0:
                    break
                remaining -= cap
                tail_offset = next_off

        cap, _ = self._read_offset_array_header(tail_offset)
        tail_entries = self._header['tail_entry_array_n_entries']
        if tail_entries == 0:
            tail_entries = self._header['n_entries']
            offset = self._header['entry_array_offset']
            while offset != 0 and offset != tail_offset:
                c, next_off = self._read_offset_array_header(offset)
                tail_entries -= c
                offset = next_off

        if tail_entries < cap:
            self._write_array_item(tail_offset, tail_entries, entry_offset)
            self._header['tail_entry_array_offset'] = tail_offset
            self._header['tail_entry_array_n_entries'] = tail_entries + 1
            return

        new_off = self._allocate_offset_array(self._next_entry_array_capacity(self._header['n_entries'], cap))
        self._write_uint64_at(tail_offset + 16, new_off)
        self._write_array_item(new_off, 0, entry_offset)
        self._header['tail_entry_array_offset'] = new_off
        self._header['tail_entry_array_n_entries'] = 1

    def _read_offset_array_header(self, offset):
        buf = self._read_at(offset, OFFSET_ARRAY_OBJECT_HEADER_SIZE)
        oh = parse_object_header(buf, 0)
        if not oh or oh['type'] != OBJECT_TYPE_ENTRY_ARRAY:
            raise ValueError('invalid entry array object')
        item_size = self._offset_array_item_size()
        if (oh['size'] - OFFSET_ARRAY_OBJECT_HEADER_SIZE) % item_size != 0:
            raise ValueError('invalid entry array object size')
        capacity = (oh['size'] - OFFSET_ARRAY_OBJECT_HEADER_SIZE) // item_size
        next_offset = read_uint64_le(buf, 16)
        return capacity, next_offset

    def _allocate_offset_array(self, capacity):
        offset = self._append_offset
        size = OFFSET_ARRAY_OBJECT_HEADER_SIZE + capacity * self._offset_array_item_size()
        self._ensure_compact_object_fits(offset, size)
        aligned_size = align8(size)
        self._ensure_arena_size(offset + aligned_size)
        buf = bytearray(aligned_size)
        write_object_header(buf, 0, OBJECT_TYPE_ENTRY_ARRAY, 0, size)
        self._write_at(offset, buf)
        self._object_added(offset, size)
        self._header['n_entry_arrays'] += 1
        self._publish_object_metadata()
        self._hmac_put_object(offset, OBJECT_TYPE_ENTRY_ARRAY)
        return offset

    def _write_array_item(self, array_offset, index, entry_offset):
        off = array_offset + OFFSET_ARRAY_OBJECT_HEADER_SIZE + index * self._offset_array_item_size()
        if self._compact:
            self._ensure_compact_offset(entry_offset)
            self._write_uint32_at(off, entry_offset)
        else:
            self._write_uint64_at(off, entry_offset)

    def _link_data_to_entry(self, data_offset, entry_offset):
        n_entries = self._read_uint64_at(data_offset + 56)

        if n_entries == 0:
            self._write_uint64_at(data_offset + 40, entry_offset)
            self._write_uint64_at(data_offset + 56, 1)
        elif n_entries == 1:
            array_off = self._allocate_offset_array(4)
            self._write_array_item(array_off, 0, entry_offset)
            self._write_uint64_at(data_offset + 48, array_off)
            if self._compact:
                self._write_uint32_at(data_offset + COMPACT_DATA_TAIL_OFFSET_OFFSET, array_off)
                self._write_uint32_at(data_offset + COMPACT_DATA_TAIL_ENTRIES_OFFSET, 1)
            self._write_uint64_at(data_offset + 56, 2)
        else:
            entry_array_off = self._read_uint64_at(data_offset + 48)
            if entry_array_off == 0:
                raise ValueError('invalid journal: missing data entry array')
            tail_offset, tail_entries = self._append_to_data_entry_array(entry_array_off, n_entries - 1, entry_offset)
            if self._compact:
                self._write_uint32_at(data_offset + COMPACT_DATA_TAIL_OFFSET_OFFSET, tail_offset)
                self._write_uint32_at(data_offset + COMPACT_DATA_TAIL_ENTRIES_OFFSET, tail_entries)
            self._write_uint64_at(data_offset + 56, n_entries + 1)

    def _append_to_data_entry_array(self, array_offset, current_count, entry_offset):
        remaining = current_count
        offset = array_offset
        while True:
            cap, next_off = self._read_offset_array_header(offset)
            if remaining < cap:
                self._write_array_item(offset, remaining, entry_offset)
                return offset, remaining + 1
            remaining -= cap
            if next_off == 0:
                new_off = self._allocate_offset_array(self._next_entry_array_capacity(current_count, cap))
                self._write_uint64_at(offset + 16, new_off)
                self._write_array_item(new_off, 0, entry_offset)
                return new_off, 1
            offset = next_off

    def _entry_item_size(self):
        return COMPACT_ENTRY_ITEM_SIZE if self._compact else REGULAR_ENTRY_ITEM_SIZE

    def _offset_array_item_size(self):
        return COMPACT_OFFSET_ARRAY_ITEM_SIZE if self._compact else REGULAR_OFFSET_ARRAY_ITEM_SIZE

    def _data_payload_offset(self):
        return COMPACT_DATA_OBJECT_HEADER_SIZE if self._compact else DATA_OBJECT_HEADER_SIZE

    def _ensure_compact_offset(self, offset):
        if self._compact and offset > JOURNAL_COMPACT_SIZE_MAX:
            raise ValueError('compact journal offset exceeds 32-bit range')

    def _ensure_compact_object_fits(self, offset, size):
        if self._compact and (offset > JOURNAL_COMPACT_SIZE_MAX or align8(offset + size) > JOURNAL_COMPACT_SIZE_MAX):
            raise ValueError('compact journal cannot exceed 4 GiB')

    def sync(self):
        if self._closed:
            raise ValueError('writer closed')
        self._write_header()
        if self._arena is not None:
            self._arena.flush()
        os.fsync(self._fd)

    def close(self):
        self._close_with_state(STATE_ONLINE)

    def close_offline(self):
        self._close_with_state(STATE_OFFLINE)

    def _close_with_state(self, state: int):
        if self._closed:
            return
        close_err = None
        try:
            self._header['state'] = state
            self._write_header()
            if self._arena is not None:
                self._arena.flush()
            os.fsync(self._fd)
        except Exception as e:
            close_err = e
        try:
            if self._arena is not None:
                self._arena.close()
                self._arena = None
        except Exception as e:
            if not close_err:
                close_err = e
        try:
            os.close(self._fd)
        except Exception as e:
            if not close_err:
                close_err = e
        try:
            self._lock.release()
        except Exception as e:
            if not close_err:
                close_err = e
        self._lock = None
        self._closed = True
        if close_err:
            raise close_err

    def archive_to(self, path):
        if self._closed:
            raise ValueError('writer closed')
        self._header['state'] = STATE_ARCHIVED
        self._write_header()
        if self._arena is not None:
            self._arena.flush()
        os.fsync(self._fd)
        try:
            if self._path != path:
                os.rename(self._path, path)
            self._path = path
            close_err = None
            try:
                _sync_parent_directory(path)
            except Exception as e:
                close_err = e
            try:
                if self._arena is not None:
                    self._arena.close()
                    self._arena = None
            except Exception as e:
                if not close_err:
                    close_err = e
            try:
                os.close(self._fd)
                self._closed = True
            except Exception as e:
                if not close_err:
                    close_err = e
            try:
                self._lock.release()
            except Exception as e:
                if not close_err:
                    close_err = e
            self._lock = None
            if close_err:
                raise close_err
        except Exception:
            if self._closed:
                raise
            self._header['state'] = STATE_ONLINE
            self._write_header()
            if self._arena is not None:
                self._arena.flush()
            os.fsync(self._fd)
            raise

    def current_size(self):
        return self._append_offset

    # Sealing methods

    def _append_tag(self):
        if self._seal is None:
            return
        self._seal.hmac_start()
        offset = self._append_offset
        size = OBJECT_HEADER_SIZE + 8 + 8 + TAG_LENGTH
        seqnum = self._header['n_tags'] + 1
        epoch = self._seal.get_epoch()
        self._ensure_arena_size(offset + align8(size))
        buf = bytearray(align8(size))
        write_object_header(buf, 0, OBJECT_TYPE_TAG, 0, size)
        struct.pack_into('<Q', buf, OBJECT_HEADER_SIZE, seqnum)
        struct.pack_into('<Q', buf, OBJECT_HEADER_SIZE + 8, epoch)
        self._seal.hmac_write(bytes(buf[:OBJECT_HEADER_SIZE + 16]))
        buf[OBJECT_HEADER_SIZE + 16:OBJECT_HEADER_SIZE + 16 + TAG_LENGTH] = self._seal.hmac_sum()
        self._write_at(offset, buf)
        self._object_added(offset, size)
        self._header['n_tags'] = seqnum
        self._seal.hmac_reset()

    def _append_first_tag(self):
        if self._seal is None:
            return
        self._hmac_put_header()
        self._hmac_put_hash_table_object(self._header['field_hash_table_offset'] - OBJECT_HEADER_SIZE)
        self._hmac_put_hash_table_object(self._header['data_hash_table_offset'] - OBJECT_HEADER_SIZE)
        self._append_tag()

    def _maybe_append_tag(self, realtime):
        if self._seal is None:
            return
        need = self._seal.need_evolve(realtime)
        if not need:
            return
        self._append_tag()
        while True:
            goal = self._seal.get_goal_epoch(realtime)
            epoch = self._seal.get_epoch()
            if epoch >= goal:
                break
            self._seal.evolve_state()
            if self._seal.get_epoch() < goal:
                self._append_tag()

    def _hmac_put_header(self):
        if self._seal is None:
            return
        self._seal.hmac_start()
        header_buf = bytearray(HEADER_SIZE)
        serialize_file_header(header_buf, self._header)
        self._seal.hmac_write(bytes(header_buf[0:16]))
        self._seal.hmac_write(bytes(header_buf[24:56]))
        self._seal.hmac_write(bytes(header_buf[72:96]))
        self._seal.hmac_write(bytes(header_buf[104:136]))

    def _hmac_put_hash_table_object(self, object_start):
        if self._seal is None:
            return
        self._seal.hmac_start()
        buf = self._read_at(object_start, OBJECT_HEADER_SIZE)
        self._seal.hmac_write(buf)

    def _hmac_put_object(self, object_start, typ):
        if self._seal is None:
            return
        self._seal.hmac_start()
        buf = self._read_at(object_start, OBJECT_HEADER_SIZE)
        self._seal.hmac_write(buf)
        obj_size = struct.unpack_from('<Q', buf, 8)[0]
        if typ == OBJECT_TYPE_DATA:
            hash_buf = self._read_at(object_start + 16, 8)
            self._seal.hmac_write(hash_buf)
            payload_offset = self._data_payload_offset()
            payload_size = obj_size - payload_offset
            if payload_size > 0:
                payload = self._read_at(object_start + payload_offset, payload_size)
                self._seal.hmac_write(payload)
        elif typ == OBJECT_TYPE_FIELD:
            hash_buf = self._read_at(object_start + 16, 8)
            self._seal.hmac_write(hash_buf)
            payload_size = obj_size - FIELD_OBJECT_HEADER_SIZE
            if payload_size > 0:
                payload = self._read_at(object_start + FIELD_OBJECT_HEADER_SIZE, payload_size)
                self._seal.hmac_write(payload)
        elif typ == OBJECT_TYPE_ENTRY:
            rest_size = obj_size - OBJECT_HEADER_SIZE
            if rest_size > 0:
                rest = self._read_at(object_start + OBJECT_HEADER_SIZE, rest_size)
                self._seal.hmac_write(rest)
        elif typ in (OBJECT_TYPE_DATA_HASH_TABLE, OBJECT_TYPE_FIELD_HASH_TABLE, OBJECT_TYPE_ENTRY_ARRAY):
            pass
        elif typ == OBJECT_TYPE_TAG:
            meta = self._read_at(object_start + OBJECT_HEADER_SIZE, 16)
            self._seal.hmac_write(meta)


def _read_object_size_from_fd(fd, offset):
    buf = os.pread(fd, 8, offset + 8)
    return read_uint64_le(buf, 0)


def _sync_parent_directory(path):
    dir_fd = os.open(os.path.dirname(path), os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(dir_fd)
    finally:
        os.close(dir_fd)


def _current_time_ms():
    import time
    return int(time.time() * 1000)


def _lock_fd(fd):
    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)


def _normalize_field_name_policy(value):
    if value is None or value == '':
        return FIELD_NAME_POLICY_JOURNALD
    if value == FIELD_NAME_POLICY_JOURNALD:
        return FIELD_NAME_POLICY_JOURNALD
    if value == FIELD_NAME_POLICY_RAW:
        return FIELD_NAME_POLICY_RAW
    if value == FIELD_NAME_POLICY_JOURNAL_APP:
        return FIELD_NAME_POLICY_JOURNAL_APP
    raise ValueError(f'unsupported field name policy: {value}')


def _writer_policy_for_log_policy(policy):
    return FIELD_NAME_POLICY_RAW if _normalize_field_name_policy(policy) == FIELD_NAME_POLICY_RAW else FIELD_NAME_POLICY_JOURNALD


def _prepare_fields_for_policy(fields, policy):
    policy = _normalize_field_name_policy(policy)
    if not fields:
        raise ValueError('empty entry')
    if policy == FIELD_NAME_POLICY_JOURNAL_APP:
        filtered = []
        for field in fields:
            try:
                _validate_field_name_for_policy(field['name'], policy)
            except ValueError:
                continue
            filtered.append(field)
        if not filtered:
            raise ValueError('empty entry')
        return filtered
    for field in fields:
        _validate_field_name_for_policy(field['name'], policy)
    return fields


def _prepare_raw_payloads_for_policy(payloads, policy):
    policy = _normalize_field_name_policy(policy)
    if not payloads:
        raise ValueError('empty entry')
    prepared = []
    for payload in payloads:
        payload = _raw_payload_bytes(payload)
        field_name = _raw_payload_field_name(payload)
        if policy == FIELD_NAME_POLICY_JOURNAL_APP:
            try:
                _validate_field_name_for_policy(field_name, policy)
            except ValueError:
                continue
        else:
            _validate_field_name_for_policy(field_name, policy)
        prepared.append(payload)
    if not prepared:
        raise ValueError('empty entry')
    return prepared


def _raw_payload_bytes(payload):
    if isinstance(payload, bytes):
        return payload
    if isinstance(payload, (bytearray, memoryview)):
        return bytes(payload)
    if isinstance(payload, str):
        return payload.encode('utf-8')
    return bytes(payload)


def _raw_payload_field_name(payload):
    eq = payload.find(b'=')
    if eq < 0:
        raise ValueError('invalid raw payload: missing field separator')
    if eq == 0:
        raise ValueError('invalid field name: empty')
    return payload[:eq]


def _validate_field_name_for_policy(name, policy=FIELD_NAME_POLICY_JOURNALD):
    policy = _normalize_field_name_policy(policy)
    if policy == FIELD_NAME_POLICY_RAW:
        return _validate_raw_field_name(name)
    return _validate_journald_field_name(name, allow_protected=(policy == FIELD_NAME_POLICY_JOURNALD))


def _field_name_bytes(name):
    if isinstance(name, bytes):
        return name
    if isinstance(name, (bytearray, memoryview)):
        return bytes(name)
    return str(name).encode('utf-8')


def _field_name_for_error(name):
    if isinstance(name, (bytes, bytearray, memoryview)):
        return bytes(name).decode('utf-8', errors='replace')
    return str(name)


def _validate_raw_field_name(name):
    data = _field_name_bytes(name)
    if len(data) == 0:
        raise ValueError('invalid field name: empty')
    if b'=' in data:
        raise ValueError(f"invalid field name: contains '=': {_field_name_for_error(name)}")


def _validate_journald_field_name(name, allow_protected=True):
    data = _field_name_bytes(name)
    display = _field_name_for_error(name)
    if len(data) == 0:
        raise ValueError('invalid field name: empty')
    if len(data) > 64:
        raise ValueError(f'invalid field name: too long ({len(data)})')
    if not allow_protected and data[0] == 0x5F:
        raise ValueError(f'invalid field name: protected: {display}')
    if 0x30 <= data[0] <= 0x39:
        raise ValueError(f'invalid field name: starts with digit: {display}')
    for i, code in enumerate(data):
        if code != 0x5F and not (0x41 <= code <= 0x5A) and not (0x30 <= code <= 0x39):
            raise ValueError(f'invalid field name: bad char at {i}: {display}')


def _uuid_option(value, fallback):
    if value is None:
        return fallback
    if isinstance(value, str):
        value = value.replace('-', '')
        return bytes.fromhex(value)
    if isinstance(value, bytearray):
        value = bytes(value)
    if not isinstance(value, bytes) or len(value) != 16:
        raise ValueError('uuid options must be 16 bytes or 32 hex characters')
    return value


def _read_boot_id():
    try:
        with open('/proc/sys/kernel/random/boot_id', 'r', encoding='ascii') as f:
            text = f.read().strip().replace('-', '')
        return bytes.fromhex(text) if len(text) == 32 else None
    except (OSError, ValueError):
        return None


def _normalize_compression(value):
    if value is None or value == COMPRESSION_NONE or value == 'none':
        return COMPRESSION_NONE
    if value == COMPRESSION_ZSTD or value == 'zstd':
        return COMPRESSION_ZSTD
    if value == COMPRESSION_XZ or value == 'xz':
        return COMPRESSION_XZ
    if value == COMPRESSION_LZ4 or value == 'lz4':
        return COMPRESSION_LZ4
    raise ValueError(f'unsupported compression: {value}')


def _normalize_compress_threshold(value):
    if value is None:
        return DEFAULT_COMPRESS_THRESHOLD
    threshold = int(value)
    return max(MIN_COMPRESS_THRESHOLD, threshold)


def _normalize_live_publish_every_entries(value):
    if value is None:
        return 1
    entries = int(value)
    if entries < 0:
        raise ValueError(f'invalid live_publish_every_entries: {value}')
    return entries


def _zstd_compress(payload):
    import compression.zstd
    return compression.zstd.compress(payload)


def _xz_compress(payload):
    return lzma.compress(
        payload,
        format=lzma.FORMAT_XZ,
        check=lzma.CHECK_NONE,
        filters=[{'id': lzma.FILTER_LZMA2, 'preset': 0}],
    )


def _lz4_compress(payload):
    import lz4.block
    compressed = lz4.block.compress(payload, store_size=False)
    size_prefix = struct.pack('<Q', len(payload))
    return size_prefix + compressed


def _ensure_zstd_available():
    import compression.zstd


def _ensure_xz_available():
    import lzma


def _ensure_lz4_available():
    import lz4.block
