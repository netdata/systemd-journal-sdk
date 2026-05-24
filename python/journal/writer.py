# Journal file writer. Creates regular, non-compact, keyed-hash journal files.
# Compatible with stock journalctl readers during live append.

import os
import struct
import fcntl
import lzma
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
    INCOMPATIBLE_COMPRESSED_XZ, INCOMPATIBLE_COMPRESSED_LZ4,
    COMPATIBLE_TAIL_ENTRY_BOOT_ID,
    OBJECT_HEADER_SIZE, ENTRY_OBJECT_HEADER_SIZE, DATA_OBJECT_HEADER_SIZE,
    FIELD_OBJECT_HEADER_SIZE, HASH_ITEM_SIZE, OFFSET_ARRAY_OBJECT_HEADER_SIZE,
    REGULAR_ENTRY_ITEM_SIZE, OBJECT_COMPRESSED_XZ, OBJECT_COMPRESSED_LZ4, OBJECT_COMPRESSED_ZSTD,
    DEFAULT_DATA_HASH_BUCKETS, DEFAULT_FIELD_HASH_BUCKETS, FILE_SIZE_INCREASE,
    INITIAL_ENTRY_ARRAY_CAP, INITIAL_DATA_ENTRY_ARRAY_CAP,
)
from .hash import sip_hash_24, jenkins_hash_64
from .compress import decompress_zst_sync, decompress_xz_sync, decompress_lz4_sync

COMPRESSION_NONE = 0
COMPRESSION_ZSTD = 1
COMPRESSION_XZ = 2
COMPRESSION_LZ4 = 3
DEFAULT_COMPRESS_THRESHOLD = 64


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

    @staticmethod
    def create(path, opts=None):
        opts = opts or {}
        lock = WriterLock.acquire(path)
        fd = None
        try:
            fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o644)
            _lock_fd(fd)
            os.ftruncate(fd, 0)
            w = Writer(fd, path, lock)
            w._compression = _normalize_compression(opts.get('compression', COMPRESSION_NONE))
            if w._compression == COMPRESSION_ZSTD:
                _ensure_zstd_available()
            elif w._compression == COMPRESSION_XZ:
                _ensure_xz_available()
            elif w._compression == COMPRESSION_LZ4:
                _ensure_lz4_available()
            w._compress_threshold = opts.get('compression_threshold_bytes', DEFAULT_COMPRESS_THRESHOLD)
            w._initialize(opts)
            return w
        except Exception:
            if fd is not None:
                os.close(fd)
            lock.release()
            raise

    @staticmethod
    def open(path):
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
                INCOMPATIBLE_COMPRESSED_XZ | INCOMPATIBLE_COMPRESSED_LZ4
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
                w._boot_id = header['file_id']
            w._started = now_ms - monotonic_base
            w._compression = compression
            w._compress_threshold = DEFAULT_COMPRESS_THRESHOLD

            w._header['state'] = STATE_ONLINE
            w._write_header()
            return w
        except Exception:
            os.close(fd)
            lock.release()
            raise

    def _initialize(self, opts):
        data_buckets = opts.get('data_hash_table_buckets', DEFAULT_DATA_HASH_BUCKETS)
        field_buckets = opts.get('field_hash_table_buckets', DEFAULT_FIELD_HASH_BUCKETS)

        data_size = data_buckets * HASH_ITEM_SIZE
        field_size = field_buckets * HASH_ITEM_SIZE
        # systemd creates FIELD_HASH_TABLE first, then DATA_HASH_TABLE
        field_obj_offset = HEADER_SIZE
        field_offset = field_obj_offset + OBJECT_HEADER_SIZE
        data_obj_offset = align8(field_offset + field_size)
        data_offset = data_obj_offset + OBJECT_HEADER_SIZE
        append_offset = align8(data_offset + data_size)
        file_size = FILE_SIZE_INCREASE

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

        self._header = {
            'signature': 'LPKSHHRH',
            'compatible_flags': COMPATIBLE_TAIL_ENTRY_BOOT_ID,  # v260+ sets TAIL_ENTRY_BOOT_ID
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
        self._next_seqnum = opts.get('head_seqnum', 1)

        os.ftruncate(self._fd, file_size)
        self._write_header()

        # systemd writes FIELD hash table first, then DATA hash table
        fht_buf = bytearray(OBJECT_HEADER_SIZE)
        write_object_header(fht_buf, 0, OBJECT_TYPE_FIELD_HASH_TABLE, 0, OBJECT_HEADER_SIZE + field_size)
        os.pwrite(self._fd, fht_buf, field_obj_offset)

        dht_buf = bytearray(OBJECT_HEADER_SIZE)
        write_object_header(dht_buf, 0, OBJECT_TYPE_DATA_HASH_TABLE, 0, OBJECT_HEADER_SIZE + data_size)
        os.pwrite(self._fd, dht_buf, data_obj_offset)

    def _write_header(self):
        buf = bytearray(HEADER_SIZE)
        serialize_file_header(buf, self._header)
        os.pwrite(self._fd, buf, 0)

    def _write_uint64_at(self, offset, value):
        buf = struct.pack('<Q', value)
        os.pwrite(self._fd, buf, offset)

    def _write_uuid_at(self, offset, uuid):
        os.pwrite(self._fd, uuid, offset)

    def append(self, fields, opts=None):
        opts = opts or {}
        if self._closed:
            raise ValueError('writer closed')
        if len(fields) == 0:
            raise ValueError('empty entry')

        now_ms = _current_time_ms()
        realtime = opts.get('realtime_usec', now_ms * 1000)
        monotonic = opts.get('monotonic_usec', (now_ms - self._started) * 1000)
        boot_id = opts.get('boot_id')
        if boot_id is None or is_zero_uuid(boot_id):
            boot_id = self._boot_id
        if isinstance(boot_id, str):
            boot_id = bytes.fromhex(boot_id)

        payloads = []
        for field in fields:
            name = field['name']
            value = field['value']
            if isinstance(value, str):
                value = value.encode('utf-8')
            elif isinstance(value, (bytearray, memoryview)):
                value = bytes(value)
            elif not isinstance(value, bytes):
                value = bytes(value)
            _validate_field_name(name)
            payload = name.encode('utf-8') + b'=' + value
            payloads.append(payload)

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
        entry_size = ENTRY_OBJECT_HEADER_SIZE + len(deduped) * REGULAR_ENTRY_ITEM_SIZE
        aligned_size = align8(entry_size)
        entry_buf = bytearray(aligned_size)
        write_object_header(entry_buf, 0, OBJECT_TYPE_ENTRY, 0, entry_size)
        struct.pack_into('<Q', entry_buf, 16, self._next_seqnum)
        struct.pack_into('<Q', entry_buf, 24, realtime)
        struct.pack_into('<Q', entry_buf, 32, monotonic)
        entry_buf[40:56] = boot_id
        struct.pack_into('<Q', entry_buf, 56, xor_hash)
        for i, item in enumerate(deduped):
            off = ENTRY_OBJECT_HEADER_SIZE + i * REGULAR_ENTRY_ITEM_SIZE
            struct.pack_into('<Q', entry_buf, off, item['offset'])
            struct.pack_into('<Q', entry_buf, off + 8, item['hash'])
        os.pwrite(self._fd, entry_buf, entry_offset)
        self._object_added(entry_offset, entry_size)

        self._publish_object_metadata()
        self._append_to_entry_array(entry_offset)
        for item in deduped:
            self._link_data_to_entry(item['offset'], entry_offset)

        self._entry_added(entry_offset, realtime, monotonic, boot_id)
        self._publish_entry_metadata()

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

        size = DATA_OBJECT_HEADER_SIZE + len(object_payload)
        aligned_size = align8(size)
        buf = bytearray(aligned_size)
        write_object_header(buf, 0, OBJECT_TYPE_DATA, compression_flag, size)
        struct.pack_into('<Q', buf, 16, h)
        buf[DATA_OBJECT_HEADER_SIZE:DATA_OBJECT_HEADER_SIZE + len(object_payload)] = object_payload
        os.pwrite(self._fd, buf, offset)
        self._object_added(offset, size)

        self._append_hash_item(
            self._header['data_hash_table_offset'],
            self._header['data_hash_table_size'],
            OBJECT_TYPE_DATA, h, offset)
        self._header['n_data'] += 1

        eq_pos = payload.find(b'=')
        if eq_pos > 0:
            field_payload = payload[:eq_pos]
            field_offset = self._add_field(field_payload)
            field_head_data = self._read_field_head_data_offset(field_offset)
            self._write_uint64_at(offset + 32, field_head_data)
            self._write_uint64_at(field_offset + 32, offset)

        return offset, h

    def _add_field(self, payload):
        h = self._hash(payload)
        existing = self._find_field(h, payload)
        if existing is not None:
            return existing

        offset = self._append_offset
        size = FIELD_OBJECT_HEADER_SIZE + len(payload)
        aligned_size = align8(size)
        buf = bytearray(aligned_size)
        write_object_header(buf, 0, OBJECT_TYPE_FIELD, 0, size)
        struct.pack_into('<Q', buf, 16, h)
        buf[FIELD_OBJECT_HEADER_SIZE:FIELD_OBJECT_HEADER_SIZE + len(payload)] = payload
        os.pwrite(self._fd, buf, offset)
        self._object_added(offset, size)

        self._append_hash_item(
            self._header['field_hash_table_offset'],
            self._header['field_hash_table_size'],
            OBJECT_TYPE_FIELD, h, offset)
        self._header['n_fields'] += 1
        return offset

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
        buf = os.pread(self._fd, HASH_ITEM_SIZE, offset)
        return {
            'head': read_uint64_le(buf, 0),
            'tail': read_uint64_le(buf, 8),
        }

    def _write_hash_item(self, offset, item):
        buf = struct.pack('<QQ', item['head'], item['tail'])
        os.pwrite(self._fd, buf, offset)

    def _read_data_payload(self, offset):
        obj_header = _read_object_header_from_fd(self._fd, offset)
        if not obj_header or obj_header['type'] != OBJECT_TYPE_DATA:
            return None
        obj_size = obj_header['size']
        payload_len = obj_size - DATA_OBJECT_HEADER_SIZE
        if payload_len <= 0:
            return None
        buf = os.pread(self._fd, payload_len, offset + DATA_OBJECT_HEADER_SIZE)
        flags = obj_header['flags']
        if flags & OBJECT_COMPRESSED_ZSTD:
            return decompress_zst_sync(buf)
        if flags & OBJECT_COMPRESSED_XZ:
            return decompress_xz_sync(buf)
        if flags & OBJECT_COMPRESSED_LZ4:
            return decompress_lz4_sync(buf)
        return buf

    def _read_field_payload(self, offset):
        obj_size = _read_object_size_from_fd(self._fd, offset)
        payload_len = obj_size - FIELD_OBJECT_HEADER_SIZE
        if payload_len <= 0:
            return None
        buf = os.pread(self._fd, payload_len, offset + FIELD_OBJECT_HEADER_SIZE)
        return buf

    def _read_field_head_data_offset(self, offset):
        return self._read_uint64_at(offset + 32)

    def _read_uint64_at(self, offset):
        buf = os.pread(self._fd, 8, offset)
        return read_uint64_le(buf, 0)

    def _write_uint32_at(self, offset, value):
        os.pwrite(self._fd, struct.pack('<I', value), offset)

    def _append_hash_item(self, table_offset, table_size, expected_type, h, object_offset):
        n_buckets = table_size // HASH_ITEM_SIZE
        bucket_off = table_offset + (h % n_buckets) * HASH_ITEM_SIZE
        item = self._read_hash_item(bucket_off)

        if item['head'] != 0:
            head = _read_object_header_from_fd(self._fd, item['head'])
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
        buf = os.pread(self._fd, OFFSET_ARRAY_OBJECT_HEADER_SIZE, offset)
        oh = parse_object_header(buf, 0)
        if not oh or oh['type'] != OBJECT_TYPE_ENTRY_ARRAY:
            raise ValueError('invalid entry array object')
        capacity = (oh['size'] - OFFSET_ARRAY_OBJECT_HEADER_SIZE) // 8
        next_offset = read_uint64_le(buf, 16)
        return capacity, next_offset

    def _allocate_offset_array(self, capacity):
        offset = self._append_offset
        size = OFFSET_ARRAY_OBJECT_HEADER_SIZE + capacity * 8
        aligned_size = align8(size)
        buf = bytearray(aligned_size)
        write_object_header(buf, 0, OBJECT_TYPE_ENTRY_ARRAY, 0, size)
        os.pwrite(self._fd, buf, offset)
        self._object_added(offset, size)
        self._header['n_entry_arrays'] += 1
        self._publish_object_metadata()
        return offset

    def _write_array_item(self, array_offset, index, entry_offset):
        off = array_offset + OFFSET_ARRAY_OBJECT_HEADER_SIZE + index * 8
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
            self._write_uint64_at(data_offset + 56, 2)
        else:
            entry_array_off = self._read_uint64_at(data_offset + 48)
            if entry_array_off == 0:
                raise ValueError('invalid journal: missing data entry array')
            self._append_to_data_entry_array(entry_array_off, n_entries - 1, entry_offset)
            self._write_uint64_at(data_offset + 56, n_entries + 1)

    def _append_to_data_entry_array(self, array_offset, current_count, entry_offset):
        remaining = current_count
        offset = array_offset
        while True:
            cap, next_off = self._read_offset_array_header(offset)
            if remaining < cap:
                self._write_array_item(offset, remaining, entry_offset)
                return
            remaining -= cap
            if next_off == 0:
                new_off = self._allocate_offset_array(self._next_entry_array_capacity(current_count, cap))
                self._write_uint64_at(offset + 16, new_off)
                self._write_array_item(new_off, 0, entry_offset)
                return
            offset = next_off

    def sync(self):
        if self._closed:
            raise ValueError('writer closed')
        self._write_header()
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
            os.fsync(self._fd)
        except Exception as e:
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
        os.fsync(self._fd)
        try:
            os.rename(self._path, path)
            self._path = path
            close_err = None
            try:
                _sync_parent_directory(path)
            except Exception as e:
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
        except Exception:
            if self._closed:
                raise
            self._header['state'] = STATE_ONLINE
            self._write_header()
            os.fsync(self._fd)
            raise

    def current_size(self):
        return self._append_offset


def _read_object_size_from_fd(fd, offset):
    buf = os.pread(fd, 8, offset + 8)
    return read_uint64_le(buf, 0)


def _read_object_header_from_fd(fd, offset):
    buf = os.pread(fd, OBJECT_HEADER_SIZE, offset)
    return parse_object_header(buf, 0)


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


def _validate_field_name(name):
    if not name or len(name) == 0:
        raise ValueError('invalid field name: empty')
    if len(name) > 64:
        raise ValueError(f'invalid field name: too long ({len(name)})')
    if name[0] >= '0' and name[0] <= '9':
        raise ValueError(f'invalid field name: starts with digit: {name}')
    for i, c in enumerate(name):
        code = ord(c)
        if code != 0x5F and not (0x41 <= code <= 0x5A) and not (0x30 <= code <= 0x39):
            raise ValueError(f'invalid field name: bad char at {i}: {name}')


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
