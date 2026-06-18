# Single journal file reader.
# Reads .journal, .journal~, .journal.zst, .journal~.zst files.
# Uses entry-array-based iteration.

import contextlib
import os
from .binary import uuid_to_string
from .header import (
    parse_file_header, parse_object_header,
    HEADER_MIN_SIZE, HEADER_SIZE, OBJECT_TYPE_ENTRY, OBJECT_TYPE_ENTRY_ARRAY,
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
    stream_zst_to_temp,
    decompress_zst_sync,
    decompress_xz_sync,
    decompress_lz4_sync,
    is_zst_file,
)
from .hash import jenkins_hash_64, sip_hash_24
from .reader_access import open_reader_accessor
from .reader_access import READER_BOUNDS_LIVE


class FileReader:
    def __init__(self, accessor, header, path, cleanup_path=None):
        self._accessor = accessor
        self._header = header
        self._path = path
        self._cleanup_path = cleanup_path
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
    def open(path, *, options=None):
        cleanup_path = None
        accessor = None
        try:
            open_path = path
            if is_zst_file(path):
                cleanup_path = stream_zst_to_temp(path, prefix='python-sdk-journal')
                open_path = cleanup_path

            accessor = open_reader_accessor(open_path, options)
            if accessor.size() < HEADER_MIN_SIZE:
                raise ValueError('file too small for journal header')

            header = parse_file_header(accessor.read_bytes(0, min(accessor.size(), HEADER_SIZE)))
            _ensure_supported_header(header)

            return FileReader(accessor, header, path, cleanup_path)
        except Exception:
            if accessor is not None:
                with contextlib.suppress(Exception):
                    accessor.close()
            if cleanup_path:
                with contextlib.suppress(Exception):
                    os.unlink(cleanup_path)
                    os.rmdir(os.path.dirname(cleanup_path))
            raise

    def selected_access_mode(self):
        return self._accessor.selected_access_mode()

    def access_stats(self):
        return self._accessor.stats()

    def _visible_size(self):
        return self._accessor.size()

    def _temp_view(self, offset, size):
        return self._accessor.temp_view(offset, size)

    def _read_bytes(self, offset, size):
        return self._accessor.read_bytes(offset, size)

    def _u8(self, offset):
        return self._accessor.u8(offset)

    def _u32(self, offset):
        return self._accessor.u32(offset)

    def _u64(self, offset):
        return self._accessor.u64(offset)

    def _object_header_at(self, offset):
        return parse_object_header(self._temp_view(offset, OBJECT_HEADER_SIZE), 0)

    def _leave_current_row(self):
        self._reset_cached_entry_data_state()
        if self._accessor is not None:
            self._accessor.clear_row()

    def _load_entry_array(self):
        self._entry_offsets = self._read_entry_array_offsets()
        self._entry_index = -1

    def _read_entry_array_offsets(self):
        if self._header['entry_array_offset'] == 0:
            return []

        offsets = []
        offset = self._header['entry_array_offset']
        remaining = self._header['n_entries']
        seen = set()

        while offset != 0 and remaining > 0:
            if offset in seen:
                raise ValueError('entry array chain contains a cycle')
            seen.add(offset)

            array = self._read_entry_array_object(offset)
            if array is None:
                break

            to_read = min(remaining, array['capacity'])
            self._append_entry_array_offsets(offsets, array, to_read)
            remaining -= to_read
            offset = self._next_entry_array_offset(offset, array['next_offset'])

        return offsets

    def _read_entry_array_object(self, offset):
        oh = self._object_header_at(offset)
        if not oh or oh['type'] != OBJECT_TYPE_ENTRY_ARRAY:
            return None

        obj_size = oh['size']
        if obj_size <= OFFSET_ARRAY_OBJECT_HEADER_SIZE:
            raise ValueError('entry array object has no item capacity')

        item_size = self._offset_array_item_size_value
        item_bytes = obj_size - OFFSET_ARRAY_OBJECT_HEADER_SIZE
        if item_bytes % item_size != 0:
            raise ValueError('entry array item payload has invalid compact alignment')

        return {
            'data_start': offset + OFFSET_ARRAY_OBJECT_HEADER_SIZE,
            'capacity': item_bytes // item_size,
            'item_size': item_size,
            'next_offset': self._u64(offset + 16),
        }

    def _append_entry_array_offsets(self, offsets, array, to_read):
        for i in range(to_read):
            item_offset = array['data_start'] + i * array['item_size']
            entry_off = self._read_entry_array_item_offset(item_offset)
            if entry_off != 0 and self._valid_entry_offset(entry_off):
                offsets.append(entry_off)

    def _read_entry_array_item_offset(self, item_offset):
        if self._compact:
            return self._u32(item_offset)
        return self._u64(item_offset)

    def _next_entry_array_offset(self, current_offset, next_offset):
        if next_offset != 0 and next_offset <= current_offset:
            raise ValueError('entry array chain next pointer is not increasing')
        return next_offset

    def refresh(self):
        """Refresh header and entry-array state for active files."""
        return self._refresh_entry_offsets()

    def _refresh_entry_offsets(self):
        if not self._can_refresh():
            return False

        snapshot = self._refresh_snapshot()

        try:
            _, new_size = self._accessor.refresh_visible_bounds()
            unchanged = self._load_refresh_header_and_offsets(new_size, snapshot)
            if unchanged:
                return False
        except Exception:
            self._restore_refresh_snapshot(snapshot)
            return False

        self._entry_index = min(snapshot['index'], len(self._entry_offsets))
        return self._entry_offsets_changed(snapshot['offsets'])

    def _can_refresh(self):
        return (
            not self._cleanup_path
            and self._accessor is not None
            and self._accessor.bounds_mode() == READER_BOUNDS_LIVE
        )

    def _refresh_snapshot(self):
        return {
            'offsets': self._entry_offsets,
            'index': self._entry_index,
            'size': self._visible_size(),
            'visible_bounds': self._accessor.snapshot_visible_bounds(),
            'header': self._header,
            'compact': self._compact,
            'entry_item_size': self._entry_item_size,
            'offset_array_item_size': self._offset_array_item_size_value,
            'data_payload_offset': self._data_payload_offset_value,
        }

    def _load_refresh_header_and_offsets(self, new_size, snapshot):
        header = parse_file_header(self._read_bytes(0, min(self._visible_size(), HEADER_SIZE)))
        _ensure_supported_header(header)
        if self._refresh_header_unchanged(header, new_size, snapshot):
            self._header = header
            self._entry_index = min(snapshot['index'], len(self._entry_offsets))
            return True

        self._header = header
        self._refresh_layout_from_header()
        self._entry_offsets = self._read_entry_array_offsets()
        return False

    def _refresh_header_unchanged(self, header, new_size, snapshot):
        return (
            new_size == snapshot['size']
            and header.get('n_entries') == self._header.get('n_entries')
            and header.get('tail_entry_array_offset') == self._header.get('tail_entry_array_offset')
            and header.get('tail_entry_array_n_entries') == self._header.get('tail_entry_array_n_entries')
        )

    def _refresh_layout_from_header(self):
        self._compact = (self._header['incompatible_flags'] & INCOMPATIBLE_COMPACT) != 0
        self._entry_item_size = COMPACT_ENTRY_ITEM_SIZE if self._compact else REGULAR_ENTRY_ITEM_SIZE
        self._offset_array_item_size_value = (
            COMPACT_OFFSET_ARRAY_ITEM_SIZE if self._compact else REGULAR_OFFSET_ARRAY_ITEM_SIZE
        )
        self._data_payload_offset_value = (
            COMPACT_DATA_OBJECT_HEADER_SIZE if self._compact else DATA_OBJECT_HEADER_SIZE
        )

    def _restore_refresh_snapshot(self, snapshot):
        self._accessor.restore_visible_bounds(snapshot['visible_bounds'])
        self._header = snapshot['header']
        self._compact = snapshot['compact']
        self._entry_item_size = snapshot['entry_item_size']
        self._offset_array_item_size_value = snapshot['offset_array_item_size']
        self._data_payload_offset_value = snapshot['data_payload_offset']
        self._entry_offsets = snapshot['offsets']
        self._entry_index = min(snapshot['index'], len(self._entry_offsets))

    def _entry_offsets_changed(self, old_offsets):
        if len(self._entry_offsets) != len(old_offsets):
            return True
        return bool(self._entry_offsets) and bool(old_offsets) and self._entry_offsets[-1] != old_offsets[-1]

    def _valid_entry_offset(self, offset):
        off = offset
        if off + OBJECT_HEADER_SIZE > self._visible_size():
            return False
        oh = self._object_header_at(off)
        if not oh:
            return False
        if oh['type'] == 0 and oh['size'] == 0:
            return False
        return oh['type'] == OBJECT_TYPE_ENTRY

    def seek_head(self):
        self._leave_current_row()
        self._entry_index = -1
        self._direction = 0
        self._realtime_seek = None

    def seek_tail(self):
        self._leave_current_row()
        self._entry_index = len(self._entry_offsets)
        self._direction = 1
        self._realtime_seek = None

    def seek_realtime_usec(self, usec):
        self._leave_current_row()
        self._realtime_seek = int(usec)

    def next(self):
        self._leave_current_row()
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
        self._leave_current_row()
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
        return self._u64(offset + OBJECT_HEADER_SIZE + 8)

    def step(self):
        while self.next():
            if self._filter is None:
                return True
            try:
                entry = self.get_entry()
            except Exception:
                entry = None
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
                entry = None
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
                payload = None
            if payload is None:
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
        return self._u64(offset + OBJECT_HEADER_SIZE + 8)

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
            if self._visible_size() < bucket_offset + HASH_ITEM_SIZE:
                raise ValueError('field hash bucket exceeds buffer')
            offset = self._u64(bucket_offset)
            while offset:
                field = self._read_field_object_at(offset)
                try:
                    field_name = field['payload'].decode('utf-8')
                except UnicodeDecodeError:
                    field_name = None
                if field_name is not None:
                    fields.add(field_name)
                offset = field['next_hash_offset']
        return fields

    def _enumerate_fields_by_entry_scan(self):
        fields = set()
        for off in self._entry_offsets:
            try:
                entry = self._read_entry_at(off)
            except Exception:
                entry = None
            if entry:
                fields.update(entry['fields'].keys())
        return fields

    def header(self):
        return self._header

    def close(self):
        self._leave_current_row()
        if self._accessor is not None:
            try:
                self._accessor.close()
            finally:
                self._accessor = None
        if self._cleanup_path:
            with contextlib.suppress(Exception):
                os.unlink(self._cleanup_path)
                os.rmdir(os.path.dirname(self._cleanup_path))
            self._cleanup_path = None

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
        return self._read_data_payload_row(data_offset)

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
        if self._visible_size() < offset + ENTRY_OBJECT_HEADER_SIZE:
            raise ValueError('buffer too small for entry object')
        obj_type = self._u8(offset)
        if obj_type != OBJECT_TYPE_ENTRY:
            raise ValueError(f'expected ENTRY (type {OBJECT_TYPE_ENTRY}), got type {obj_type} at offset {offset}')
        obj_size = self._u64(offset + 8)
        if obj_size < ENTRY_OBJECT_HEADER_SIZE:
            raise ValueError(f'entry object too small: {obj_size}')
        if offset + obj_size > self._visible_size():
            raise ValueError(f'entry object exceeds buffer at offset {offset}')

        e_off = offset + OBJECT_HEADER_SIZE
        entry = {
            'seqnum': self._u64(e_off),
            'realtime': self._u64(e_off + 8),
            'monotonic': self._u64(e_off + 16),
            'boot_id': self._read_bytes(e_off + 24, 16),
            'xor_hash': self._u64(e_off + 40),
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
                data_offset = self._u32(item_offset)
            else:
                data_offset = self._u64(item_offset)
            if data_offset != 0:
                offsets.append(data_offset)
        return entry, offsets

    def _read_data_payload_at(self, offset):
        return bytes(self._read_data_payload_temp(offset))

    def _read_data_payload_temp(self, offset):
        return self._read_data_payload(offset, row_lifetime=False)

    def _read_data_payload_row(self, offset):
        return self._read_data_payload(offset, row_lifetime=True)

    def _read_data_payload(self, offset, row_lifetime):
        payload_offset = self._data_payload_offset_value
        obj_flags, payload_size, payload_offset_abs = self._data_payload_bounds(offset, payload_offset)
        if obj_flags == 0:
            return self._payload_view(payload_offset_abs, payload_size, row_lifetime)
        payload = self._accessor.temp_view(payload_offset_abs, payload_size)
        return self._decompress_data_payload(obj_flags, payload, row_lifetime)

    def _data_payload_bounds(self, offset, payload_offset):
        if self._visible_size() < offset + payload_offset:
            raise ValueError('buffer too small for data object')
        obj_type = self._u8(offset)
        obj_flags = self._u8(offset + 1)
        obj_size = self._u64(offset + 8)
        if obj_type != OBJECT_TYPE_DATA:
            raise ValueError(f'expected DATA (type {OBJECT_TYPE_DATA}), got type {obj_type}')
        if obj_size < payload_offset:
            raise ValueError(f'data object too small: {obj_size}')
        if offset + obj_size > self._visible_size():
            raise ValueError(f'data object exceeds buffer at offset {offset}')
        return obj_flags, obj_size - payload_offset, offset + payload_offset

    def _payload_view(self, payload_offset_abs, payload_size, row_lifetime):
        if row_lifetime:
            return self._accessor.row_view(payload_offset_abs, payload_size)
        return self._accessor.temp_view(payload_offset_abs, payload_size)

    def _decompress_data_payload(self, obj_flags, payload, row_lifetime):
        unsupported = obj_flags & ~(OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD)
        if unsupported != 0:
            raise ValueError(f'unsupported DATA object flags: 0x{obj_flags:x}')
        if obj_flags & OBJECT_COMPRESSED_XZ:
            data = decompress_xz_sync(payload, max_output_size=MAX_UNCOMPRESSED_SIZE)
            return self._accessor.row_bytes(data) if row_lifetime else data
        if obj_flags & OBJECT_COMPRESSED_LZ4:
            data = decompress_lz4_sync(payload)
            return self._accessor.row_bytes(data) if row_lifetime else data
        if obj_flags & OBJECT_COMPRESSED_ZSTD:
            if not _HAS_ZSTD:
                raise RuntimeError('zstd decompression not available')
            data = decompress_zst_sync(payload, max_output_size=MAX_UNCOMPRESSED_SIZE)
            return self._accessor.row_bytes(data) if row_lifetime else data
        return self._accessor.row_bytes(payload) if row_lifetime else payload

    def _read_data_header_at(self, offset):
        if self._visible_size() < offset + DATA_OBJECT_HEADER_SIZE:
            raise ValueError('buffer too small for data object')
        oh = self._object_header_at(offset)
        if not oh or oh['type'] != OBJECT_TYPE_DATA or oh['size'] < self._data_payload_offset_value:
            raise ValueError('corrupt DATA object')
        return {
            'hash': self._u64(offset + 16),
            'next_hash_offset': self._u64(offset + 24),
            'next_field_offset': self._u64(offset + 32),
            'entry_offset': self._u64(offset + 40),
            'entry_array_offset': self._u64(offset + 48),
            'n_entries': self._u64(offset + 56),
        }

    def _read_field_object_at(self, offset):
        if self._visible_size() < offset + FIELD_OBJECT_HEADER_SIZE:
            raise ValueError('buffer too small for field object')
        oh = self._object_header_at(offset)
        if not oh or oh['type'] != OBJECT_TYPE_FIELD or oh['size'] < FIELD_OBJECT_HEADER_SIZE:
            raise ValueError('corrupt FIELD object')
        size = oh['size']
        if offset + size > self._visible_size():
            raise ValueError(f'field object exceeds buffer at offset {offset}')
        return {
            'hash': self._u64(offset + 16),
            'next_hash_offset': self._u64(offset + 24),
            'head_data_offset': self._u64(offset + 32),
            'payload': self._read_bytes(offset + FIELD_OBJECT_HEADER_SIZE, size - FIELD_OBJECT_HEADER_SIZE),
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
        if self._visible_size() < bucket_offset + HASH_ITEM_SIZE:
            raise ValueError('field hash bucket exceeds buffer')
        offset = self._u64(bucket_offset)
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

    def _UNPACK_U64(self, offset):
        return self._u64(offset)

    def _data_object_was_compressed(self, offset):
        """Return True if the DATA object header carries a compression flag."""

        if self._visible_size() < offset + DATA_OBJECT_HEADER_SIZE:
            return False
        return bool(self._u8(offset + 1) & (
            OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD
        ))

    def _index_for_entry_offset(self, entry_offset):
        """Linear search the entry array for the given entry offset.
        Returns the index or None.
        """

        try:
            return self._entry_offsets.index(entry_offset)
        except ValueError:
            return None

    def _position_at_index(self, index, direction):
        """Position the reader on `index` and set the direction."""

        self._leave_current_row()
        self._entry_index = index
        self._direction = 0 if direction == 0 else 1
        self._realtime_seek = None

    def _entry_realtime_at_offset(self, entry_offset):
        return self._u64(entry_offset + OBJECT_HEADER_SIZE + 8)

    def explore(self, query):
        """Run an explorer query with the default Traversal strategy.

        Mirrors `rust::FileReader::explore` (L1203-1205). The full
        query semantics, defaults, and validation live in
        `journal.explorer`; this method only dispatches.
        """

        from .explorer import ExplorerStrategy, _explore_file_reader
        return _explore_file_reader(self, query, ExplorerStrategy.TRAVERSAL, None)

    def explore_with_strategy(self, query, strategy):
        """Run an explorer query with the requested strategy.

        Mirrors `rust::FileReader::explore_with_strategy` (L1207-1213).
        """

        from .explorer import _explore_file_reader
        return _explore_file_reader(self, query, strategy, None)

    def explore_with_strategy_and_control(self, query, strategy, control):
        """Run an explorer query with the requested strategy and
        a control object. Mirrors
        `rust::FileReader::explore_with_strategy_and_control`
        (L1215-1228).
        """

        from .explorer import _explore_file_reader
        return _explore_file_reader(self, query, strategy, control)


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
