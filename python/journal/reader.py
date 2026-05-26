# Single journal file reader.
# Reads .journal, .journal~, .journal.zst, .journal~.zst files.
# Uses entry-array-based iteration.

import os
import tempfile
from .binary import read_uint64_le, uuid_to_string, align8, buf_equal
from .header import (
    parse_file_header, parse_object_header,
    HEADER_MIN_SIZE, HEADER_SIZE, OBJECT_TYPE_ENTRY, OBJECT_TYPE_ENTRY_ARRAY,
    OBJECT_TYPE_DATA, OBJECT_HEADER_SIZE, ENTRY_OBJECT_HEADER_SIZE,
    DATA_OBJECT_HEADER_SIZE, OFFSET_ARRAY_OBJECT_HEADER_SIZE,
    REGULAR_ENTRY_ITEM_SIZE, INCOMPATIBLE_KEYED_HASH, INCOMPATIBLE_COMPACT,
    COMPACT_OFFSET_ARRAY_ITEM_SIZE, REGULAR_OFFSET_ARRAY_ITEM_SIZE,
    INCOMPATIBLE_COMPRESSED_ZSTD, INCOMPATIBLE_COMPRESSED_XZ, INCOMPATIBLE_COMPRESSED_LZ4,
)
from .compress import decompress_zst_to_temp, is_zst_file
from .entry import parse_entry_object, parse_data_object
from .hash import parse_match_string


class FileReader:
    def __init__(self, buffer, header, path, cleanup_path=None):
        self._buffer = buffer
        self._header = header
        self._path = path
        self._cleanup_path = cleanup_path
        self._entry_offsets = []
        self._entry_index = -1
        self._direction = 0
        self._filter = None
        self._realtime_seek = None
        self._load_entry_array()

    @staticmethod
    def open(path):
        cleanup_path = None
        buffer = None
        try:
            if is_zst_file(path):
                cleanup_path = decompress_zst_to_temp(path, 'python-sdk-journal')
                with open(cleanup_path, 'rb') as f:
                    buffer = f.read()
            else:
                with open(path, 'rb') as f:
                    buffer = f.read()

            if len(buffer) < HEADER_MIN_SIZE:
                raise ValueError('file too small for journal header')

            header = parse_file_header(buffer)
            if header['header_size'] < HEADER_MIN_SIZE:
                raise ValueError('unsupported journal: header size too small')
            if not (header['incompatible_flags'] & INCOMPATIBLE_KEYED_HASH):
                raise ValueError('unsupported journal: keyed hash required')

            supported = (
                INCOMPATIBLE_KEYED_HASH | INCOMPATIBLE_COMPRESSED_ZSTD |
                INCOMPATIBLE_COMPRESSED_XZ | INCOMPATIBLE_COMPRESSED_LZ4 |
                INCOMPATIBLE_COMPACT
            )
            if header['incompatible_flags'] & ~supported:
                raise ValueError(f'unsupported journal: incompatible flags 0x{header["incompatible_flags"]:x}')

            return FileReader(buffer, header, path, cleanup_path)
        except Exception:
            if cleanup_path:
                try:
                    os.unlink(cleanup_path)
                    os.rmdir(os.path.dirname(cleanup_path))
                except Exception:
                    pass
            raise

    def _load_entry_array(self):
        if self._header['entry_array_offset'] == 0:
            self._entry_offsets = []
            self._entry_index = -1
            return

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
            next_offset = read_uint64_le(self._buffer, offset + 16)
            item_size = self._offset_array_item_size()
            if (obj_size - OFFSET_ARRAY_OBJECT_HEADER_SIZE) % item_size != 0:
                raise ValueError('entry array item payload has invalid compact alignment')
            capacity = (obj_size - OFFSET_ARRAY_OBJECT_HEADER_SIZE) // item_size

            to_read = min(remaining, capacity)
            data_start = offset + OFFSET_ARRAY_OBJECT_HEADER_SIZE

            for i in range(to_read):
                item_offset = data_start + i * item_size
                if self._is_compact():
                    entry_off = int.from_bytes(self._buffer[item_offset:item_offset + COMPACT_OFFSET_ARRAY_ITEM_SIZE], 'little')
                else:
                    entry_off = read_uint64_le(self._buffer, item_offset)
                if entry_off != 0 and self._valid_entry_offset(entry_off):
                    offsets.append(entry_off)

            remaining -= to_read
            offset = next_offset

        self._entry_offsets = offsets
        self._entry_index = -1

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
        self._entry_index = -1
        self._direction = 0
        self._realtime_seek = None

    def seek_tail(self):
        self._entry_index = len(self._entry_offsets)
        self._direction = 1
        self._realtime_seek = None

    def seek_realtime_usec(self, usec):
        self._realtime_seek = int(usec)

    def next(self):
        if self._realtime_seek is not None:
            idx = self._first_realtime_index_at_or_after(self._realtime_seek)
            self._realtime_seek = None
            self._direction = 0
            if idx >= len(self._entry_offsets):
                self._entry_index = len(self._entry_offsets)
                return False
            self._entry_index = idx
            return True
        self._direction = 0
        self._entry_index += 1
        if self._entry_index >= len(self._entry_offsets):
            self._entry_index = len(self._entry_offsets)
            return False
        return True

    def previous(self):
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
        return read_uint64_le(self._buffer, offset + OBJECT_HEADER_SIZE + 8)

    def step(self):
        while self.next():
            if self._filter is None:
                return True
            try:
                entry = self.get_entry()
                if entry and self._filter.matches(entry):
                    return True
            except Exception:
                pass
        return False

    def step_back(self):
        while self.previous():
            if self._filter is None:
                return True
            try:
                entry = self.get_entry()
                if entry and self._filter.matches(entry):
                    return True
            except Exception:
                pass
        return False

    def get_entry(self):
        if self._entry_index < 0 or self._entry_index >= len(self._entry_offsets):
            return None
        return self._read_entry_at(self._entry_offsets[self._entry_index])

    def _read_entry_at(self, offset):
        e = parse_entry_object(self._buffer, offset, self._is_compact())

        fields = {}
        field_values = {}
        payloads = []

        for item in e['items']:
            try:
                do = parse_data_object(self._buffer, item['offset'], self._is_compact())
                name_str = do['name'].decode('utf-8')
                payloads.append(do['name'] + b'=' + do['value'])
                if name_str not in fields:
                    fields[name_str] = do['value']
                if name_str not in field_values:
                    field_values[name_str] = []
                field_values[name_str].append(do['value'])
            except Exception:
                pass

        cursor = self._make_cursor(offset, e)
        return {
            'fields': fields,
            'field_values': field_values,
            'payloads': payloads,
            'seqnum': e['seqnum'],
            'realtime': e['realtime'],
            'monotonic': e['monotonic'],
            'boot_id': e['boot_id'],
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
        return read_uint64_le(self._buffer, offset + OBJECT_HEADER_SIZE + 8)

    def get_cursor(self):
        if self._entry_index < 0 or self._entry_index >= len(self._entry_offsets):
            return None
        offset = self._entry_offsets[self._entry_index]
        e = parse_entry_object(self._buffer, offset, self._is_compact())
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
        seen = set()
        results = []
        for off in self._entry_offsets:
            try:
                entry = self._read_entry_at(off)
                if entry and entry['field_values'].get(field_name):
                    for v in entry['field_values'][field_name]:
                        key = v.hex()
                        if key not in seen:
                            seen.add(key)
                            results.append(v)
            except Exception:
                pass
        return results

    def enumerate_fields(self):
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
        if self._cleanup_path:
            try:
                os.unlink(self._cleanup_path)
                os.rmdir(os.path.dirname(self._cleanup_path))
            except Exception:
                pass
            self._cleanup_path = None
        self._buffer = None

    def _is_compact(self):
        return (self._header['incompatible_flags'] & INCOMPATIBLE_COMPACT) != 0

    def _offset_array_item_size(self):
        return COMPACT_OFFSET_ARRAY_ITEM_SIZE if self._is_compact() else REGULAR_OFFSET_ARRAY_ITEM_SIZE


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
        field = item[:eq].decode('utf-8')
        if field not in by_field:
            field_order.append(field)
            by_field[field] = []
        by_field[field].append(_MatchExpr(field, item[eq + 1:]))
    field_order.sort()
    parts = [by_field[f][0] if len(by_field[f]) == 1 else _OrExpr(by_field[f]) for f in field_order]
    if len(parts) == 1:
        return parts[0]
    return _AndExpr(parts)


def _build_or_expr(level1):
    if len(level1) == 0:
        return None
    if len(level1) == 1:
        return level1[0]
    return _OrExpr(level1)
