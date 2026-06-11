# libsystemd-compatible reader facade for Python.

from .reader import FileReader
from .directory_reader import DirectoryReader
from .compress import is_journal_file_name
from .hash import parse_match_string
from .binary import uuid_to_string


def _is_printable(buf, allow_newline=False):
    try:
        text = buf.decode('utf-8')
    except Exception:
        return False
    for ch in text:
        cp = ord(ch)
        if cp < 0x20 and cp != 0x09 and not (allow_newline and cp == 0x0a):
            return False
        if 0x7f <= cp <= 0x9f:
            return False
    return True


def export_entry(entry):
    parts = []
    written = _append_export_metadata(parts, entry)
    _append_preferred_export_fields(parts, entry, written)
    _append_remaining_export_fields(parts, entry, written)
    _append_byte_name_export_fields(parts, entry)
    parts.append(b'\n')
    return b''.join(parts)


def _append_export_metadata(parts, entry):
    written = {'_BOOT_ID', '__CURSOR', '__REALTIME_TIMESTAMP', '__MONOTONIC_TIMESTAMP', '__SEQNUM'}
    for key, field in _export_metadata_fields(entry):
        parts.append(f'{field}={entry[key]}\n'.encode('utf-8'))
    if entry.get('boot_id'):
        parts.append(f"_BOOT_ID={uuid_to_string(entry['boot_id'])}\n".encode('utf-8'))
    return written


def _export_metadata_fields(entry):
    fields = [
        ('cursor', '__CURSOR'),
        ('realtime', '__REALTIME_TIMESTAMP'),
        ('monotonic', '__MONOTONIC_TIMESTAMP'),
        ('seqnum', '__SEQNUM'),
    ]
    return [(key, field) for key, field in fields if entry.get(key)]


def _append_preferred_export_fields(parts, entry, written):
    for name in ['_MACHINE_ID', '_HOSTNAME', 'PRIORITY', '_TRANSPORT']:
        if name in entry['fields'] and name not in written:
            parts.append(_format_export_field(name, entry['fields'][name]))
            written.add(name)


def _append_remaining_export_fields(parts, entry, written):
    for name in sorted(k for k in entry['fields'] if k not in written):
        for value in _entry_field_values(entry, name):
            parts.append(_format_export_field(name, value))


def _entry_field_values(entry, name):
    return entry['field_values'].get(name, [entry['fields'][name]])


def _append_byte_name_export_fields(parts, entry):
    for name, value in _sorted_non_utf8_raw_fields(entry):
        parts.append(_format_export_field_bytes(name, value))


def _sorted_non_utf8_raw_fields(entry):
    fields = [
        (name, value)
        for name, value in entry.get('raw_fields', [])
        if name != b'_BOOT_ID' and not _raw_field_name_is_utf8(name)
    ]
    return sorted(fields, key=lambda item: (item[0], item[1]))


def _raw_field_name_is_utf8(name):
    try:
        name.decode('utf-8')
        return True
    except UnicodeDecodeError:
        return False


def _format_export_field(name, value):
    return _format_export_field_bytes(name.encode('utf-8'), value)


def _format_export_field_bytes(name, value):
    name = bytes(name)
    value = bytes(value)
    line = name + b'=' + value
    if _is_printable(value, False):
        return line + b'\n'
    size_bytes = len(value).to_bytes(8, 'little')
    return name + b'\n' + size_bytes + value + b'\n'


def json_entry(entry):
    result = {}
    written = set()

    if entry.get('cursor'):
        result['__CURSOR'] = entry['cursor']
        written.add('__CURSOR')
    if entry.get('realtime'):
        result['__REALTIME_TIMESTAMP'] = str(entry['realtime'])
        written.add('__REALTIME_TIMESTAMP')
    if entry.get('monotonic'):
        result['__MONOTONIC_TIMESTAMP'] = str(entry['monotonic'])
        written.add('__MONOTONIC_TIMESTAMP')
    if entry.get('seqnum'):
        result['__SEQNUM'] = str(entry['seqnum'])
        written.add('__SEQNUM')
    if entry.get('boot_id'):
        result['_BOOT_ID'] = uuid_to_string(entry['boot_id'])
        written.add('_BOOT_ID')

    remaining = sorted(k for k in entry['fields'] if k not in written)
    for name in remaining:
        vals = entry['field_values'].get(name, [entry['fields'][name]])
        json_vals = []
        for v in vals:
            if _is_printable(v, True):
                json_vals.append(v.decode('utf-8'))
            else:
                json_vals.append(list(v))
        result[name] = json_vals[0] if len(json_vals) == 1 else json_vals
    return result


def text_entry(entry):
    msg = entry['fields'].get('MESSAGE', b'')
    return msg.decode('utf-8', errors='replace') + '\n'


class SdJournal:
    def __init__(self, reader):
        self._reader = reader
        self._output_mode = 'default'
        self._data_items = []
        self._data_index = 0
        self._data_reader_active = False
        self._field_items = []
        self._field_index = 0
        self._unique_items = []
        self._unique_index = 0

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
        name = path.split('/')[-1] if isinstance(path, str) else str(path).split('/')[-1]
        if is_journal_file_name(name):
            return SdJournal.open_file(path)
        return SdJournal.open_directory(path)

    @staticmethod
    def open_file(path):
        return SdJournal(FileReader.open(path))

    @staticmethod
    def open_directory(path):
        return SdJournal(DirectoryReader.open(path))

    @staticmethod
    def open_files(paths):
        if len(paths) == 1:
            return SdJournal.open_file(paths[0])
        return SdJournal(DirectoryReader.open_files(paths))

    def close(self):
        self._reset_iterators()
        self._reader.close()

    def _reset_iterators(self):
        if self._data_reader_active and hasattr(self._reader, 'clear_entry_data_state'):
            self._reader.clear_entry_data_state()
        self._data_items = []
        self._data_index = 0
        self._data_reader_active = False
        self._field_items = []
        self._field_index = 0
        self._unique_items = []
        self._unique_index = 0

    def add_match(self, data):
        if isinstance(data, str):
            data = data.encode('latin1')
        match_bytes = parse_match_string(data.decode('latin1') if isinstance(data, bytes) else data)
        self._reset_iterators()
        self._reader.add_match(match_bytes)

    def add_disjunction(self):
        self._reset_iterators()
        self._reader.add_disjunction()

    def add_conjunction(self):
        self._reset_iterators()
        self._reader.add_conjunction()

    def flush_matches(self):
        self._reset_iterators()
        self._reader.flush_matches()

    def seek_head(self):
        self._reset_iterators()
        self._reader.seek_head()

    def seek_tail(self):
        self._reset_iterators()
        self._reader.seek_tail()

    def seek_realtime_usec(self, usec):
        self._reset_iterators()
        self._reader.seek_realtime_usec(usec)

    def seek_cursor(self, cursor):
        want = _parse_cursor(cursor)
        self.seek_realtime_usec(want['realtime'])
        while self.next():
            entry = self.get_entry()
            got = _parse_cursor(entry['cursor'])
            if got['realtime'] > want['realtime']:
                return
            if got == want:
                return

    def set_output_mode(self, mode):
        self._output_mode = mode

    def next(self):
        self._reset_iterators()
        if self._reader.step():
            return 1
        return 0

    def previous(self):
        self._reset_iterators()
        if not hasattr(self._reader, 'step_back'):
            raise NotImplementedError('reader does not support previous()')
        if self._reader.step_back():
            return 1
        return 0

    def get_entry(self):
        return self._reader.get_entry()

    def get_cursor(self):
        return self._reader.get_cursor()

    def test_cursor(self, cursor):
        return self._reader.test_cursor(cursor)

    def get_realtime_usec(self):
        return self._reader.get_realtime_usec()

    def get_seqnum(self):
        entry = self.get_entry()
        if not entry:
            raise ValueError('no entry at current position')
        seqnum_id = _parse_cursor(entry['cursor'])['seqnum_id']
        return entry['seqnum'], seqnum_id

    def get_monotonic_usec(self):
        entry = self.get_entry()
        if not entry:
            raise ValueError('no entry at current position')
        return entry['monotonic'], entry['boot_id']

    def get_data(self, field_name):
        if hasattr(self._reader, 'get_entry_payload'):
            payload = self._reader.get_entry_payload(field_name)
            if payload is not None:
                return bytes(payload)
        entry = self.get_entry()
        values = None
        if entry:
            if isinstance(field_name, str):
                values = entry['field_values'].get(field_name)
            else:
                values = entry.get('raw_field_values', {}).get(_field_name_to_bytes(field_name))
                string_name = _field_name_to_string_or_none(field_name)
                if values is None and string_name is not None:
                    values = entry['field_values'].get(string_name)
        if not values:
            raise ValueError('data field not found')
        return _payload_from_field_value(field_name, values[0])

    def restart_data(self):
        if hasattr(self._reader, 'entry_data_restart'):
            self._reader.entry_data_restart()
            self._data_items = []
            self._data_index = 0
            self._data_reader_active = True
            return
        entry = self.get_entry()
        if not entry:
            raise ValueError('no entry at current position')
        payloads = entry.get('payloads')
        if payloads is None:
            payloads = _payloads_from_entry(entry)
        self._data_items = list(payloads)
        self._data_index = 0
        self._data_reader_active = False

    def enumerate_available_data(self):
        if self._data_reader_active:
            item = self._reader.enumerate_entry_payload()
            if item is None:
                self._data_reader_active = False
                return None
            return bytes(item)
        if self._data_index >= len(self._data_items):
            return None
        item = self._data_items[self._data_index]
        self._data_index += 1
        return bytes(item)

    def process_output(self, entry):
        if self._output_mode == 'export':
            return export_entry(entry)
        elif self._output_mode == 'json':
            import json
            return json.dumps(json_entry(entry)) + '\n'
        else:
            return text_entry(entry).encode('utf-8')

    def list_boots(self):
        if isinstance(self._reader, DirectoryReader):
            return self._reader.list_boots()
        return []

    def enumerate_fields(self):
        fields = self._reader.enumerate_fields()
        if isinstance(fields, set):
            return sorted(fields)
        return fields

    def restart_fields(self):
        self._field_items = self.enumerate_fields()
        self._field_index = 0

    def enumerate_field(self):
        if self._field_index >= len(self._field_items):
            return None
        item = self._field_items[self._field_index]
        self._field_index += 1
        return item

    def query_unique(self, field_name):
        values = self._reader.query_unique(field_name)
        return [(field_name, v) for v in values]

    def visit_unique_values(self, field_name, visitor):
        seen = set()
        for value in self._reader.query_unique(field_name):
            key = bytes(value)
            if key in seen:
                continue
            seen.add(key)
            visitor(key)
        return None

    def query_unique_state(self, field_name):
        values = self._reader.query_unique(field_name)
        self._unique_items = [_payload_from_field_value(field_name, value) for value in values]
        self._unique_index = 0

    def restart_unique(self):
        self._unique_index = 0

    def enumerate_available_unique(self):
        if self._unique_index >= len(self._unique_items):
            return None
        item = self._unique_items[self._unique_index]
        self._unique_index += 1
        return bytes(item)


OUTPUT_MODE_DEFAULT = 'default'
OUTPUT_MODE_JSON = 'json'
OUTPUT_MODE_EXPORT = 'export'


def SdJournalOpen(path, flags):
    if flags != 0:
        raise ValueError('unsupported sd_journal_open flags')
    return SdJournal.open(path)


def SdJournalOpenFile(path, flags):
    if flags != 0:
        raise ValueError('unsupported sd_journal_open_file flags')
    return SdJournal.open_file(path)


def SdJournalOpenDirectory(path, flags):
    if flags != 0:
        raise ValueError('unsupported sd_journal_open_directory flags')
    return SdJournal.open_directory(path)


def SdJournalOpenFiles(paths, flags):
    if flags != 0:
        raise ValueError('unsupported sd_journal_open_files flags')
    return SdJournal.open_files(paths)


def SdJournalClose(journal):
    journal.close()


def SdJournalAddMatch(journal, data):
    journal.add_match(data)


def SdJournalAddDisjunction(journal):
    journal.add_disjunction()


def SdJournalAddConjunction(journal):
    journal.add_conjunction()


def SdJournalFlushMatches(journal):
    journal.flush_matches()


def SdJournalNext(journal):
    return journal.next()


def SdJournalNextSkip(journal, skip):
    advanced = 0
    for _ in range(skip):
        if journal.next() == 0:
            break
        advanced += 1
    return advanced


def SdJournalPrevious(journal):
    return journal.previous()


def SdJournalPreviousSkip(journal, skip):
    advanced = 0
    for _ in range(skip):
        if journal.previous() == 0:
            break
        advanced += 1
    return advanced


def SdJournalSeekHead(journal):
    journal.seek_head()


def SdJournalSeekTail(journal):
    journal.seek_tail()


def SdJournalSeekRealtimeUsec(journal, usec):
    journal.seek_realtime_usec(usec)


def SdJournalSeekCursor(journal, cursor):
    journal.seek_cursor(cursor)


def SdJournalGetEntry(journal):
    return journal.get_entry()


def SdJournalGetData(journal, field_name):
    return journal.get_data(field_name)


def SdJournalRestartData(journal):
    journal.restart_data()


def SdJournalEnumerateAvailableData(journal):
    return journal.enumerate_available_data()


def SdJournalGetRealtimeUsec(journal):
    return journal.get_realtime_usec()


def SdJournalGetSeqnum(journal):
    return journal.get_seqnum()


def SdJournalGetMonotonicUsec(journal):
    return journal.get_monotonic_usec()


def SdJournalGetCursor(journal):
    return journal.get_cursor()


def SdJournalTestCursor(journal, cursor):
    return journal.test_cursor(cursor)


def SdJournalEnumerateFields(journal):
    return journal.enumerate_fields()


def SdJournalRestartFields(journal):
    journal.restart_fields()


def SdJournalEnumerateField(journal):
    return journal.enumerate_field()


def SdJournalQueryUnique(journal, field_name):
    return journal.query_unique(field_name)


def SdJournalVisitUniqueValues(journal, field_name, visitor):
    """libsystemd-style visitor for unique values of `field_name`.

    Mirrors `rust/src/journal/src/facade.rs::SdJournalVisitUniqueValues`.
    `visitor` is a callable that takes a `bytes` value; it must
    return a falsy value on success (matches the Rust `Result::Ok`)
    or raise to abort enumeration (matches the Rust `Result::Err`
    visitor that propagates the original error).
    """
    journal.visit_unique_values(field_name, visitor)
    return None


def SdJournalQueryUniqueState(journal, field_name):
    journal.query_unique_state(field_name)


def SdJournalRestartUnique(journal):
    journal.restart_unique()


def SdJournalEnumerateAvailableUnique(journal):
    return journal.enumerate_available_unique()


def SdJournalListBoots(journal):
    return journal.list_boots()


def SdJournalSetOutputMode(journal, mode):
    journal.set_output_mode(mode)


def SdJournalProcessOutput(journal, entry):
    return journal.process_output(entry)


def _parse_cursor(cursor):
    parts = {}
    for segment in cursor.split(';'):
        key, sep, value = segment.partition('=')
        if not sep or not key:
            raise ValueError('invalid cursor format')
        parts[key] = value
    if not parts.get('s') or not parts.get('j') or not parts.get('c') or not parts.get('n'):
        raise ValueError('invalid cursor format')
    return {
        'seqnum_id': parts['s'],
        'boot_id': parts['j'],
        'realtime': int(parts['c'], 16),
        'seqnum': int(parts['n'], 10),
    }


def _payload_from_field_value(field_name, value):
    return _field_name_to_bytes(field_name) + b'=' + bytes(value)


def _field_name_to_bytes(field_name):
    if isinstance(field_name, bytes):
        return field_name
    if isinstance(field_name, (bytearray, memoryview)):
        return bytes(field_name)
    return str(field_name).encode('utf-8')


def _field_name_to_string_or_none(field_name):
    if isinstance(field_name, str):
        return field_name
    try:
        return _field_name_to_bytes(field_name).decode('utf-8')
    except UnicodeDecodeError:
        return None


def _payloads_from_entry(entry):
    payloads = []
    for name in sorted(entry.get('field_values', {})):
        for value in entry['field_values'][name]:
            payloads.append(_payload_from_field_value(name, value))
    return payloads
