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
    if entry.get('cursor'):
        parts.append(f"__CURSOR={entry['cursor']}\n".encode('utf-8'))
    if entry.get('realtime'):
        parts.append(f"__REALTIME_TIMESTAMP={entry['realtime']}\n".encode('utf-8'))
    if entry.get('monotonic'):
        parts.append(f"__MONOTONIC_TIMESTAMP={entry['monotonic']}\n".encode('utf-8'))
    if entry.get('seqnum'):
        parts.append(f"__SEQNUM={entry['seqnum']}\n".encode('utf-8'))
    if entry.get('boot_id'):
        parts.append(f"_BOOT_ID={uuid_to_string(entry['boot_id'])}\n".encode('utf-8'))

    preferred = ['_MACHINE_ID', '_HOSTNAME', 'PRIORITY', '_TRANSPORT']
    written = {'_BOOT_ID', '__CURSOR', '__REALTIME_TIMESTAMP', '__MONOTONIC_TIMESTAMP', '__SEQNUM'}
    for name in preferred:
        if name in entry['fields'] and name not in written:
            parts.append(_format_export_field(name, entry['fields'][name]))
            written.add(name)

    remaining = sorted(k for k in entry['fields'] if k not in written)
    for name in remaining:
        vals = entry['field_values'].get(name, [entry['fields'][name]])
        for v in vals:
            parts.append(_format_export_field(name, v))

    parts.append(b'\n')
    return b''.join(parts)


def _format_export_field(name, value):
    line = name.encode('utf-8') + b'=' + value
    if _is_printable(value, False):
        return line + b'\n'
    size_bytes = len(value).to_bytes(8, 'little')
    return name.encode('utf-8') + b'\n' + size_bytes + value + b'\n'


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
            try:
                json_vals.append(v.decode('utf-8'))
            except Exception:
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

    @staticmethod
    def open(path):
        name = path.split('/')[-1] if isinstance(path, str) else str(path).split('/')[-1]
        if is_journal_file_name(name):
            reader = FileReader.open(path)
        else:
            reader = DirectoryReader.open(path)
        return SdJournal(reader)

    def close(self):
        self._reader.close()

    def add_match(self, data):
        if isinstance(data, str):
            data = data.encode('latin1')
        match_bytes = parse_match_string(data.decode('latin1') if isinstance(data, bytes) else data)
        self._reader.add_match(match_bytes)

    def add_disjunction(self):
        self._reader.add_disjunction()

    def add_conjunction(self):
        self._reader.add_conjunction()

    def flush_matches(self):
        self._reader.flush_matches()

    def seek_head(self):
        self._reader.seek_head()

    def seek_tail(self):
        self._reader.seek_tail()

    def set_output_mode(self, mode):
        self._output_mode = mode

    def next(self):
        if self._reader.step():
            return 1
        return 0

    def previous(self):
        if hasattr(self._reader, 'step_back'):
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

    def query_unique(self, field_name):
        values = self._reader.query_unique(field_name)
        return [(field_name, v) for v in values]


OUTPUT_MODE_DEFAULT = 'default'
OUTPUT_MODE_JSON = 'json'
OUTPUT_MODE_EXPORT = 'export'


def SdJournalOpen(path, flags):
    if flags != 0:
        raise ValueError('unsupported sd_journal_open flags')
    return SdJournal.open(path)


def SdJournalOpenDirectory(path, flags):
    if flags != 0:
        raise ValueError('unsupported sd_journal_open_directory flags')
    return SdJournal.open(path)


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


def SdJournalPrevious(journal):
    return journal.previous()


def SdJournalSeekHead(journal):
    journal.seek_head()


def SdJournalSeekTail(journal):
    journal.seek_tail()


def SdJournalGetEntry(journal):
    return journal.get_entry()


def SdJournalGetRealtimeUsec(journal):
    return journal.get_realtime_usec()


def SdJournalGetCursor(journal):
    return journal.get_cursor()


def SdJournalTestCursor(journal, cursor):
    return journal.test_cursor(cursor)


def SdJournalEnumerateFields(journal):
    return journal.enumerate_fields()


def SdJournalQueryUnique(journal, field_name):
    return journal.query_unique(field_name)


def SdJournalListBoots(journal):
    return journal.list_boots()


def SdJournalSetOutputMode(journal, mode):
    journal.set_output_mode(mode)


def SdJournalProcessOutput(journal, entry):
    return journal.process_output(entry)