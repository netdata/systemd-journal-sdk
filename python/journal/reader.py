# Single journal file reader.
# Reads .journal, .journal~, .journal.zst, .journal~.zst files.
# Uses entry-array-based iteration.

import mmap
import os
import struct
from .binary import uuid_to_string
from .header import (
    parse_file_header, parse_object_header,
    HEADER_MIN_SIZE, OBJECT_TYPE_ENTRY, OBJECT_TYPE_ENTRY_ARRAY,
    OBJECT_TYPE_DATA, OBJECT_TYPE_FIELD, OBJECT_HEADER_SIZE, ENTRY_OBJECT_HEADER_SIZE,
    DATA_OBJECT_HEADER_SIZE, OFFSET_ARRAY_OBJECT_HEADER_SIZE,
    REGULAR_ENTRY_ITEM_SIZE, INCOMPATIBLE_KEYED_HASH, INCOMPATIBLE_COMPACT,
    COMPACT_OFFSET_ARRAY_ITEM_SIZE, REGULAR_OFFSET_ARRAY_ITEM_SIZE,
    INCOMPATIBLE_COMPRESSED_ZSTD, INCOMPATIBLE_COMPRESSED_XZ, INCOMPATIBLE_COMPRESSED_LZ4,
    OBJECT_COMPRESSED_ZSTD, OBJECT_COMPRESSED_XZ, OBJECT_COMPRESSED_LZ4,
    COMPACT_ENTRY_ITEM_SIZE, COMPACT_DATA_OBJECT_HEADER_SIZE,
    FIELD_OBJECT_HEADER_SIZE, HASH_ITEM_SIZE,
)
from .compress import (
    _HAS_ZSTD,
    MAX_UNCOMPRESSED_SIZE,
    decompress_zst_to_temp,
    decompress_zst_sync,
    decompress_xz_sync,
    decompress_lz4_sync,
    is_zst_file,
)
from .hash import jenkins_hash_64, sip_hash_24


class FileReader:
    def __init__(self, buffer, header, path, cleanup_path=None, fd=None, mmap_obj=None):
        self._buffer = buffer
        self._header = header
        self._path = path
        self._cleanup_path = cleanup_path
        self._fd = fd
        self._mmap = mmap_obj
        self._entry_offsets = []
        self._entry_index = -1
        self._direction = 0
        self._filter = None
        self._realtime_seek = None
        self._entry_data_offsets = []
        self._entry_data_offsets_entry = None
        self._entry_data_index = 0
        self._entry_data_state_active = False
        self._compact = (self._header['incompatible_flags'] & INCOMPATIBLE_COMPACT) != 0
        self._entry_item_size = COMPACT_ENTRY_ITEM_SIZE if self._compact else REGULAR_ENTRY_ITEM_SIZE
        self._offset_array_item_size_value = (
            COMPACT_OFFSET_ARRAY_ITEM_SIZE if self._compact else REGULAR_OFFSET_ARRAY_ITEM_SIZE
        )
        self._data_payload_offset_value = (
            COMPACT_DATA_OBJECT_HEADER_SIZE if self._compact else DATA_OBJECT_HEADER_SIZE
        )
        self._load_entry_array()

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
    def open(path):
        cleanup_path = None
        buffer = None
        fd = None
        mapped = None
        try:
            if is_zst_file(path):
                cleanup_path = decompress_zst_to_temp(path, 'python-sdk-journal')
                fd, mapped, buffer = _map_file_readonly(cleanup_path)
            else:
                fd, mapped, buffer = _map_file_readonly(path)

            if len(buffer) < HEADER_MIN_SIZE:
                raise ValueError('file too small for journal header')

            header = parse_file_header(buffer)
            _ensure_supported_header(header)

            return FileReader(buffer, header, path, cleanup_path, fd, mapped)
        except Exception:
            if mapped is not None:
                try:
                    mapped.close()
                except Exception:
                    pass
            if fd is not None:
                try:
                    os.close(fd)
                except Exception:
                    pass
            if cleanup_path:
                try:
                    os.unlink(cleanup_path)
                    os.rmdir(os.path.dirname(cleanup_path))
                except Exception:
                    pass
            raise

    def _load_entry_array(self):
        self._entry_offsets = self._read_entry_array_offsets()
        self._entry_index = -1

    def _read_entry_array_offsets(self):
        if self._header['entry_array_offset'] == 0:
            return []

        offsets = []
        offset = self._header['entry_array_offset']
        remaining = self._header['n_entries']

        while offset != 0 and remaining > 0:
            oh = parse_object_header(self._buffer, offset)
            if not oh or oh['type'] != OBJECT_TYPE_ENTRY_ARRAY:
                break
            obj_size = oh['size']
            if obj_size < OFFSET_ARRAY_OBJECT_HEADER_SIZE:
                break
            next_offset = _UNPACK_U64_FROM(self._buffer, offset + 16)[0]
            item_size = self._offset_array_item_size_value
            if (obj_size - OFFSET_ARRAY_OBJECT_HEADER_SIZE) % item_size != 0:
                raise ValueError('entry array item payload has invalid compact alignment')
            capacity = (obj_size - OFFSET_ARRAY_OBJECT_HEADER_SIZE) // item_size

            to_read = min(remaining, capacity)
            data_start = offset + OFFSET_ARRAY_OBJECT_HEADER_SIZE

            for i in range(to_read):
                item_offset = data_start + i * item_size
                if self._compact:
                    entry_off = _UNPACK_U32_FROM(self._buffer, item_offset)[0]
                else:
                    entry_off = _UNPACK_U64_FROM(self._buffer, item_offset)[0]
                if entry_off != 0 and self._valid_entry_offset(entry_off):
                    offsets.append(entry_off)

            remaining -= to_read
            offset = next_offset

        return offsets

    def refresh(self):
        """Refresh header and entry-array state for active files."""
        return self._refresh_entry_offsets()

    def _refresh_entry_offsets(self):
        if self._cleanup_path or self._fd is None or self._mmap is None:
            return False

        old_offsets = self._entry_offsets
        old_index = self._entry_index
        old_size = len(self._buffer)
        old_fd = self._fd
        old_mmap = self._mmap
        old_buffer = self._buffer
        old_header = self._header
        old_compact = self._compact
        old_entry_item_size = self._entry_item_size
        old_offset_array_item_size = self._offset_array_item_size_value
        old_data_payload_offset = self._data_payload_offset_value
        new_fd = None
        new_mmap = None
        mapped_new_file = False

        try:
            new_size = os.fstat(self._fd).st_size
        except OSError:
            return False
        if new_size <= 0:
            return False

        if new_size != old_size:
            try:
                new_fd, new_mmap, new_buffer = _map_file_readonly(self._path)
            except Exception:
                return False
            mapped_new_file = True
            self._fd = new_fd
            self._mmap = new_mmap
            self._buffer = new_buffer

        try:
            header = parse_file_header(self._buffer)
            _ensure_supported_header(header)
            if (
                new_size == old_size
                and header.get('n_entries') == self._header.get('n_entries')
                and header.get('tail_entry_array_offset') == self._header.get('tail_entry_array_offset')
                and header.get('tail_entry_array_n_entries') == self._header.get('tail_entry_array_n_entries')
            ):
                self._header = header
                self._entry_index = min(old_index, len(self._entry_offsets))
                return False
            self._header = header
            self._compact = (self._header['incompatible_flags'] & INCOMPATIBLE_COMPACT) != 0
            self._entry_item_size = COMPACT_ENTRY_ITEM_SIZE if self._compact else REGULAR_ENTRY_ITEM_SIZE
            self._offset_array_item_size_value = (
                COMPACT_OFFSET_ARRAY_ITEM_SIZE if self._compact else REGULAR_OFFSET_ARRAY_ITEM_SIZE
            )
            self._data_payload_offset_value = (
                COMPACT_DATA_OBJECT_HEADER_SIZE if self._compact else DATA_OBJECT_HEADER_SIZE
            )
            self._entry_offsets = self._read_entry_array_offsets()
        except Exception:
            if mapped_new_file:
                self._fd = old_fd
                self._mmap = old_mmap
                self._buffer = old_buffer
                if new_mmap is not None:
                    try:
                        new_mmap.close()
                    except Exception:
                        pass
                if new_fd is not None:
                    try:
                        os.close(new_fd)
                    except Exception:
                        pass
            self._header = old_header
            self._compact = old_compact
            self._entry_item_size = old_entry_item_size
            self._offset_array_item_size_value = old_offset_array_item_size
            self._data_payload_offset_value = old_data_payload_offset
            self._entry_offsets = old_offsets
            self._entry_index = min(old_index, len(self._entry_offsets))
            self._reset_cached_entry_data_state()
            return False

        if mapped_new_file:
            try:
                old_mmap.close()
            finally:
                os.close(old_fd)
        self._entry_index = min(old_index, len(self._entry_offsets))
        self._reset_cached_entry_data_state()
        return (
            len(self._entry_offsets) != len(old_offsets)
            or (
                bool(self._entry_offsets)
                and bool(old_offsets)
                and self._entry_offsets[-1] != old_offsets[-1]
            )
        )

    def _valid_entry_offset(self, offset):
        off = offset
        if off + OBJECT_HEADER_SIZE > len(self._buffer):
            return False
        oh = parse_object_header(self._buffer, off)
        if not oh:
            return False
        if oh['type'] == 0 and oh['size'] == 0:
            return False
        return oh['type'] == OBJECT_TYPE_ENTRY

    def seek_head(self):
        self._reset_cached_entry_data_state()
        self._entry_index = -1
        self._direction = 0
        self._realtime_seek = None

    def seek_tail(self):
        self._reset_cached_entry_data_state()
        self._entry_index = len(self._entry_offsets)
        self._direction = 1
        self._realtime_seek = None

    def seek_realtime_usec(self, usec):
        self._reset_cached_entry_data_state()
        self._realtime_seek = int(usec)

    def next(self):
        self._reset_cached_entry_data_state()
        if self._realtime_seek is not None:
            idx = self._first_realtime_index_at_or_after(self._realtime_seek)
            if idx >= len(self._entry_offsets) and self._refresh_entry_offsets():
                idx = self._first_realtime_index_at_or_after(self._realtime_seek)
            self._realtime_seek = None
            self._direction = 0
            if idx >= len(self._entry_offsets):
                self._entry_index = len(self._entry_offsets)
                return False
            self._entry_index = idx
            return True
        self._direction = 0
        if self._entry_index >= len(self._entry_offsets):
            next_index = self._entry_index
            if self._refresh_entry_offsets() and next_index < len(self._entry_offsets):
                self._entry_index = next_index
                return True
            self._entry_index = len(self._entry_offsets)
            return False
        self._entry_index += 1
        if self._entry_index >= len(self._entry_offsets):
            next_index = self._entry_index
            if self._refresh_entry_offsets() and next_index < len(self._entry_offsets):
                self._entry_index = next_index
                return True
            self._entry_index = len(self._entry_offsets)
            return False
        return True

    def previous(self):
        self._reset_cached_entry_data_state()
        if self._realtime_seek is not None:
            idx = self._last_realtime_index_at_or_before(self._realtime_seek)
            self._realtime_seek = None
            self._direction = 1
            if idx < 0:
                self._entry_index = -1
                return False
            self._entry_index = idx
            return True
        self._direction = 1
        self._entry_index -= 1
        if self._entry_index < 0:
            self._entry_index = -1
            return False
        return True

    def _first_realtime_index_at_or_after(self, usec):
        lo = 0
        hi = len(self._entry_offsets)
        while lo < hi:
            mid = (lo + hi) // 2
            if self._entry_realtime_at_index(mid) >= usec:
                hi = mid
            else:
                lo = mid + 1
        return lo

    def _last_realtime_index_at_or_before(self, usec):
        lo = 0
        hi = len(self._entry_offsets)
        while lo < hi:
            mid = (lo + hi) // 2
            if self._entry_realtime_at_index(mid) > usec:
                hi = mid
            else:
                lo = mid + 1
        return lo - 1

    def _entry_realtime_at_index(self, index):
        offset = self._entry_offsets[index]
        return _UNPACK_U64_FROM(self._buffer, offset + OBJECT_HEADER_SIZE + 8)[0]

    def step(self):
        while self.next():
            if self._filter is None:
                return True
            try:
                entry = self.get_entry()
            except Exception:
                continue
            if entry and self._filter.matches(entry):
                return True
        return False

    def step_back(self):
        while self.previous():
            if self._filter is None:
                return True
            try:
                entry = self.get_entry()
            except Exception:
                continue
            if entry and self._filter.matches(entry):
                return True
        return False

    def get_entry(self):
        self._invalidate_entry_data_state()
        if self._entry_index < 0 or self._entry_index >= len(self._entry_offsets):
            return None
        return self._read_entry_at(self._entry_offsets[self._entry_index])

    def _read_entry_at(self, offset):
        e, data_offsets = self._read_entry_metadata_and_offsets(offset)

        fields = {}
        field_values = {}
        raw_field_values = {}
        raw_fields = []
        payloads = []

        for data_offset in data_offsets:
            try:
                payload = self._read_data_payload_at(data_offset)
            except Exception:
                continue
            eq_pos = payload.find(b'=')
            if eq_pos < 0:
                continue
            name = bytes(payload[:eq_pos])
            value = bytes(payload[eq_pos + 1:])
            payloads.append(bytes(payload))
            raw_fields.append((name, value))
            if name not in raw_field_values:
                raw_field_values[name] = []
            raw_field_values[name].append(value)
            try:
                name_str = name.decode('utf-8')
            except UnicodeDecodeError:
                continue
            if name_str not in fields:
                fields[name_str] = value
            if name_str not in field_values:
                field_values[name_str] = []
            field_values[name_str].append(value)

        cursor = self._make_cursor(offset, e)
        return {
            'fields': fields,
            'field_values': field_values,
            'raw_fields': raw_fields,
            'raw_field_values': raw_field_values,
            'payloads': payloads,
            'seqnum': e['seqnum'],
            'realtime': e['realtime'],
            'monotonic': e['monotonic'],
            'boot_id': e['boot_id'],
            'xor_hash': e['xor_hash'],
            'cursor': cursor,
        }

    def _make_cursor(self, entry_offset, e):
        seqnum_id = uuid_to_string(self._header['seqnum_id'])
        boot_id = uuid_to_string(e['boot_id'])
        realtime_hex = format(e['realtime'], '016x')
        return f's={seqnum_id};j={boot_id};c={realtime_hex};n={e["seqnum"]}'

    def get_realtime_usec(self):
        if self._entry_index < 0 or self._entry_index >= len(self._entry_offsets):
            return 0
        offset = self._entry_offsets[self._entry_index]
        return _UNPACK_U64_FROM(self._buffer, offset + OBJECT_HEADER_SIZE + 8)[0]

    def get_cursor(self):
        if self._entry_index < 0 or self._entry_index >= len(self._entry_offsets):
            return None
        offset = self._entry_offsets[self._entry_index]
        e, _ = self._read_entry_metadata_and_offsets(offset, include_offsets=False)
        return self._make_cursor(offset, e)

    def test_cursor(self, cursor):
        return self.get_cursor() == cursor

    def add_match(self, data):
        if self._filter is None:
            self._filter = FilterBuilder()
        self._filter.add_match(data)

    def add_disjunction(self):
        if self._filter is None:
            self._filter = FilterBuilder()
        self._filter.add_disjunction()

    def add_conjunction(self):
        if self._filter is None:
            self._filter = FilterBuilder()
        self._filter.add_conjunction()

    def flush_matches(self):
        self._filter = None

    def query_unique(self, field_name):
        raw_key = _field_name_bytes(field_name)
        results = []
        offset = self._find_field_head_data_offset(raw_key)
        while offset:
            data_header = self._read_data_header_at(offset)
            payload = self._read_data_payload_at(offset)
            if len(payload) <= len(raw_key) or payload[:len(raw_key)] != raw_key or payload[len(raw_key)] != 0x3D:
                raise ValueError(f'field data object at offset {offset} does not match requested field')
            value = bytes(payload[len(raw_key) + 1:])
            results.append(value)
            offset = data_header['next_field_offset']
        return results

    def enumerate_fields(self):
        try:
            return self._enumerate_fields_indexed()
        except Exception:
            return self._enumerate_fields_by_entry_scan()

    def _enumerate_fields_indexed(self):
        fields = set()
        table_offset = self._header.get('field_hash_table_offset', 0)
        table_size = self._header.get('field_hash_table_size', 0)
        if table_offset == 0 or table_size < HASH_ITEM_SIZE:
            return self._enumerate_fields_by_entry_scan()
        buckets = table_size // HASH_ITEM_SIZE
        for bucket in range(buckets):
            bucket_offset = table_offset + bucket * HASH_ITEM_SIZE
            if len(self._buffer) < bucket_offset + HASH_ITEM_SIZE:
                raise ValueError('field hash bucket exceeds buffer')
            offset = _UNPACK_U64_FROM(self._buffer, bucket_offset)[0]
            while offset:
                field = self._read_field_object_at(offset)
                try:
                    fields.add(field['payload'].decode('utf-8'))
                except UnicodeDecodeError:
                    pass
                offset = field['next_hash_offset']
        return fields

    def _enumerate_fields_by_entry_scan(self):
        fields = set()
        for off in self._entry_offsets:
            try:
                entry = self._read_entry_at(off)
                if entry:
                    fields.update(entry['fields'].keys())
            except Exception:
                pass
        return fields

    def header(self):
        return self._header

    def close(self):
        self._reset_cached_entry_data_state()
        if self._mmap is not None:
            try:
                self._mmap.close()
            finally:
                self._mmap = None
        if self._fd is not None:
            try:
                os.close(self._fd)
            finally:
                self._fd = None
        if self._cleanup_path:
            try:
                os.unlink(self._cleanup_path)
                os.rmdir(os.path.dirname(self._cleanup_path))
            except Exception:
                pass
            self._cleanup_path = None
        self._buffer = None

    def current_entry_key(self):
        if self._entry_index < 0 or self._entry_index >= len(self._entry_offsets):
            return None
        offset = self._entry_offsets[self._entry_index]
        e, _ = self._read_entry_metadata_and_offsets(offset, include_offsets=False)
        return {
            'seqnum_id': self._header['seqnum_id'],
            'seqnum': e['seqnum'],
            'boot_id': e['boot_id'],
            'monotonic': e['monotonic'],
            'realtime': e['realtime'],
            'xor_hash': e['xor_hash'],
        }

    def visit_entry_payloads(self, visitor):
        self._invalidate_entry_data_state()
        offsets = self._current_entry_data_offsets()
        read_payload = self._read_data_payload_at
        for data_offset in offsets:
            visitor(read_payload(data_offset))

    def collect_entry_payloads(self):
        payloads = []
        self.visit_entry_payloads(payloads.append)
        return payloads

    def get_entry_payload(self, field_name):
        prefix = _field_name_bytes(field_name) + b'='
        found = None

        def visitor(payload):
            nonlocal found
            if found is None and payload.startswith(prefix):
                found = bytes(payload)

        self.visit_entry_payloads(visitor)
        return found

    def get_raw(self, field_name):
        values = self.get_raw_values(field_name)
        return values[0] if values else None

    def get_raw_values(self, field_name):
        entry = self.get_entry()
        if not entry:
            return []
        return list(entry['raw_field_values'].get(_field_name_bytes(field_name), []))

    def entry_data_restart(self):
        self._entry_data_offsets = self._current_entry_data_offsets()
        self._entry_data_index = 0
        self._entry_data_state_active = True

    def enumerate_entry_payload(self):
        if self._entry_data_index >= len(self._entry_data_offsets):
            self.clear_entry_data_state()
            return None
        data_offset = self._entry_data_offsets[self._entry_data_index]
        self._entry_data_index += 1
        self._entry_data_state_active = True
        return self._read_data_payload_at(data_offset)

    def clear_entry_data_state(self):
        self._reset_cached_entry_data_state()

    def _reset_cached_entry_data_state(self):
        self._entry_data_offsets = []
        self._entry_data_offsets_entry = None
        self._entry_data_index = 0
        self._entry_data_state_active = False

    def _invalidate_entry_data_state(self):
        if self._entry_data_state_active:
            self.clear_entry_data_state()

    def _current_entry_data_offsets(self):
        if self._entry_index < 0 or self._entry_index >= len(self._entry_offsets):
            raise ValueError('no entry at current position')
        entry_offset = self._entry_offsets[self._entry_index]
        if self._entry_data_offsets_entry != entry_offset:
            _, offsets = self._read_entry_metadata_and_offsets(entry_offset)
            self._entry_data_offsets = offsets
            self._entry_data_offsets_entry = entry_offset
        return self._entry_data_offsets

    def _read_entry_metadata_and_offsets(self, offset, include_offsets=True):
        if len(self._buffer) < offset + ENTRY_OBJECT_HEADER_SIZE:
            raise ValueError('buffer too small for entry object')
        obj_type = self._buffer[offset]
        if obj_type != OBJECT_TYPE_ENTRY:
            raise ValueError(f'expected ENTRY (type {OBJECT_TYPE_ENTRY}), got type {obj_type} at offset {offset}')
        buf = self._buffer
        unpack_u64 = _UNPACK_U64_FROM
        unpack_u32 = _UNPACK_U32_FROM
        obj_size = unpack_u64(buf, offset + 8)[0]
        if obj_size < ENTRY_OBJECT_HEADER_SIZE:
            raise ValueError(f'entry object too small: {obj_size}')
        if offset + obj_size > len(buf):
            raise ValueError(f'entry object exceeds buffer at offset {offset}')

        e_off = offset + OBJECT_HEADER_SIZE
        entry = {
            'seqnum': unpack_u64(buf, e_off)[0],
            'realtime': unpack_u64(buf, e_off + 8)[0],
            'monotonic': unpack_u64(buf, e_off + 16)[0],
            'boot_id': bytes(buf[e_off + 24:e_off + 40]),
            'xor_hash': unpack_u64(buf, e_off + 40)[0],
        }
        if not include_offsets:
            return entry, []

        item_size = self._entry_item_size
        if (obj_size - ENTRY_OBJECT_HEADER_SIZE) % item_size != 0:
            raise ValueError(f'entry object item payload is not {item_size}-byte aligned')
        n_items = (obj_size - ENTRY_OBJECT_HEADER_SIZE) // item_size
        items_start = offset + ENTRY_OBJECT_HEADER_SIZE
        offsets = []
        for i in range(n_items):
            item_offset = items_start + i * item_size
            if self._compact:
                data_offset = unpack_u32(buf, item_offset)[0]
            else:
                data_offset = unpack_u64(buf, item_offset)[0]
            if data_offset != 0:
                offsets.append(data_offset)
        return entry, offsets

    def _read_data_payload_at(self, offset):
        payload_offset = self._data_payload_offset_value
        buf = self._buffer
        if len(buf) < offset + payload_offset:
            raise ValueError('buffer too small for data object')
        obj_type = buf[offset]
        obj_flags = buf[offset + 1]
        obj_size = _UNPACK_U64_FROM(buf, offset + 8)[0]
        if obj_type != OBJECT_TYPE_DATA:
            raise ValueError(f'expected DATA (type {OBJECT_TYPE_DATA}), got type {obj_type}')
        if obj_size < payload_offset:
            raise ValueError(f'data object too small: {obj_size}')
        if offset + obj_size > len(buf):
            raise ValueError(f'data object exceeds buffer at offset {offset}')
        payload = buf[offset + payload_offset:offset + obj_size]
        if obj_flags == 0:
            return payload
        unsupported = obj_flags & ~(OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD)
        if unsupported != 0:
            raise ValueError(f'unsupported DATA object flags: 0x{obj_flags:x}')
        if obj_flags & OBJECT_COMPRESSED_XZ:
            return decompress_xz_sync(payload, max_output_size=MAX_UNCOMPRESSED_SIZE)
        if obj_flags & OBJECT_COMPRESSED_LZ4:
            return decompress_lz4_sync(payload)
        if obj_flags & OBJECT_COMPRESSED_ZSTD:
            if not _HAS_ZSTD:
                raise RuntimeError('zstd decompression not available')
            return decompress_zst_sync(payload, max_output_size=MAX_UNCOMPRESSED_SIZE)
        return payload

    def _read_data_header_at(self, offset):
        buf = self._buffer
        if len(buf) < offset + DATA_OBJECT_HEADER_SIZE:
            raise ValueError('buffer too small for data object')
        oh = parse_object_header(buf, offset)
        if not oh or oh['type'] != OBJECT_TYPE_DATA or oh['size'] < self._data_payload_offset_value:
            raise ValueError('corrupt DATA object')
        return {
            'hash': _UNPACK_U64_FROM(buf, offset + 16)[0],
            'next_hash_offset': _UNPACK_U64_FROM(buf, offset + 24)[0],
            'next_field_offset': _UNPACK_U64_FROM(buf, offset + 32)[0],
            'entry_offset': _UNPACK_U64_FROM(buf, offset + 40)[0],
            'entry_array_offset': _UNPACK_U64_FROM(buf, offset + 48)[0],
            'n_entries': _UNPACK_U64_FROM(buf, offset + 56)[0],
        }

    def _read_field_object_at(self, offset):
        buf = self._buffer
        if len(buf) < offset + FIELD_OBJECT_HEADER_SIZE:
            raise ValueError('buffer too small for field object')
        oh = parse_object_header(buf, offset)
        if not oh or oh['type'] != OBJECT_TYPE_FIELD or oh['size'] < FIELD_OBJECT_HEADER_SIZE:
            raise ValueError('corrupt FIELD object')
        size = oh['size']
        if offset + size > len(buf):
            raise ValueError(f'field object exceeds buffer at offset {offset}')
        return {
            'hash': _UNPACK_U64_FROM(buf, offset + 16)[0],
            'next_hash_offset': _UNPACK_U64_FROM(buf, offset + 24)[0],
            'head_data_offset': _UNPACK_U64_FROM(buf, offset + 32)[0],
            'payload': bytes(buf[offset + FIELD_OBJECT_HEADER_SIZE:offset + size]),
        }

    def _find_field_head_data_offset(self, field_name):
        table_offset = self._header.get('field_hash_table_offset', 0)
        table_size = self._header.get('field_hash_table_size', 0)
        if table_offset == 0 or table_size < HASH_ITEM_SIZE:
            return 0
        h = self._hash(field_name)
        buckets = table_size // HASH_ITEM_SIZE
        if buckets == 0:
            return 0
        bucket_offset = table_offset + (h % buckets) * HASH_ITEM_SIZE
        if len(self._buffer) < bucket_offset + HASH_ITEM_SIZE:
            raise ValueError('field hash bucket exceeds buffer')
        offset = _UNPACK_U64_FROM(self._buffer, bucket_offset)[0]
        while offset:
            field = self._read_field_object_at(offset)
            if field['hash'] == h and field['payload'] == field_name:
                return field['head_data_offset']
            offset = field['next_hash_offset']
        return 0

    def _hash(self, payload):
        if self._header['incompatible_flags'] & INCOMPATIBLE_KEYED_HASH:
            return sip_hash_24(self._header['file_id'], payload)
        return jenkins_hash_64(payload)

    def _data_payload_offset(self):
        return self._data_payload_offset_value

    def _is_compact(self):
        return self._compact

    def _offset_array_item_size(self):
        return self._offset_array_item_size_value


def _map_file_readonly(path):
    fd = os.open(path, os.O_RDONLY)
    try:
        size = os.fstat(fd).st_size
        if size < HEADER_MIN_SIZE:
            raise ValueError('file too small for journal header')
        mapped = mmap.mmap(fd, 0, access=mmap.ACCESS_READ)
        return fd, mapped, mapped
    except Exception:
        os.close(fd)
        raise


def _ensure_supported_header(header):
    if header['header_size'] < HEADER_MIN_SIZE:
        raise ValueError('unsupported journal: header size too small')

    supported = (
        INCOMPATIBLE_KEYED_HASH | INCOMPATIBLE_COMPRESSED_ZSTD |
        INCOMPATIBLE_COMPRESSED_XZ | INCOMPATIBLE_COMPRESSED_LZ4 |
        INCOMPATIBLE_COMPACT
    )
    if header['incompatible_flags'] & ~supported:
        raise ValueError(f'unsupported journal: incompatible flags 0x{header["incompatible_flags"]:x}')


_UNPACK_U64_FROM = struct.Struct('<Q').unpack_from
_UNPACK_U32_FROM = struct.Struct('<I').unpack_from


def _field_name_bytes(field_name):
    if isinstance(field_name, bytes):
        return field_name
    if isinstance(field_name, (bytearray, memoryview)):
        return bytes(field_name)
    return str(field_name).encode('utf-8')


class FilterBuilder:
    def __init__(self):
        self._level0 = []
        self._level1 = []
        self._current = []

    def add_match(self, data):
        if isinstance(data, str):
            data = data.encode('latin1')
        elif not isinstance(data, (bytes, bytearray, memoryview)):
            data = bytes(data)
        data = bytes(data)
        _match_field_name(data)
        self._current.append(data)

    def add_disjunction(self):
        self._commit_current()

    def add_conjunction(self):
        self._commit_current()
        self._commit_level1()

    def _commit_current(self):
        expr = _build_current_expr(self._current)
        if expr:
            self._level1.append(expr)
        self._current = []

    def _commit_level1(self):
        expr = _build_or_expr(self._level1)
        if expr:
            self._level0.append(expr)
        self._level1 = []

    def matches(self, entry):
        expr = self._final_expr()
        if expr is None:
            return True
        return expr.matches(entry)

    def _final_expr(self):
        l0 = list(self._level0)
        l1 = list(self._level1)
        cur = _build_current_expr(self._current)
        if cur:
            l1.append(cur)
        l1_expr = _build_or_expr(l1)
        if l1_expr:
            l0.append(l1_expr)
        if len(l0) == 0:
            return None
        if len(l0) == 1:
            return l0[0]
        return _AndExpr(l0)


class _MatchExpr:
    def __init__(self, field, value):
        self._field = field
        self._value = value

    def matches(self, entry):
        vals = entry['field_values'].get(self._field)
        if vals:
            return any(self._value == v for v in vals)
        v = entry['fields'].get(self._field)
        return v is not None and self._value == v


class _AndExpr:
    def __init__(self, exprs):
        self._exprs = exprs

    def matches(self, entry):
        return all(e.matches(entry) for e in self._exprs)


class _OrExpr:
    def __init__(self, exprs):
        self._exprs = exprs

    def matches(self, entry):
        return any(e.matches(entry) for e in self._exprs)


_FALSE_EXPR = type('FalseExpr', (), {'matches': lambda s, e: False})()


def _build_current_expr(matches):
    if len(matches) == 0:
        return None
    by_field = {}
    field_order = []
    for item in matches:
        eq = item.find(b'=')
        if eq < 0:
            return _FALSE_EXPR
        field = _match_field_name(item)
        if field not in by_field:
            field_order.append(field)
            by_field[field] = []
        by_field[field].append(_MatchExpr(field, item[eq + 1:]))
    field_order.sort()
    parts = [by_field[f][0] if len(by_field[f]) == 1 else _OrExpr(by_field[f]) for f in field_order]
    if len(parts) == 1:
        return parts[0]
    return _AndExpr(parts)


def _match_field_name(item):
    eq = item.find(b'=')
    if eq < 0:
        raise ValueError('match must contain = separator')
    try:
        field = item[:eq].decode('utf-8')
    except UnicodeDecodeError as e:
        raise ValueError('match field name must be UTF-8') from e
    if field == '':
        raise ValueError('match field name must not be empty')
    return field


def _build_or_expr(level1):
    if len(level1) == 0:
        return None
    if len(level1) == 1:
        return level1[0]
    return _OrExpr(level1)
