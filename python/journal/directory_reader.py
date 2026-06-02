# Directory reader - iterates across multiple journal files.

import os
from .reader import FileReader
from .compress import is_journal_file_name


class DirectoryReader:
    def __init__(self, path, readers=None):
        self._path = path
        self._readers = readers or []
        self._index = -1
        self._realtime_seek = None
        self._realtime_seek_bound = None
        self._candidates = [None] * len(self._readers)
        self._current_key = None
        self._direction = None
        self._boot_newest = _build_boot_newest(self._readers)

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
        if not os.path.isdir(path):
            raise ValueError(f'not a directory: {path}')

        readers = []
        for journal_path in _collect_journal_files(path):
            try:
                reader = FileReader.open(journal_path)
            except Exception:
                reader = None
            if reader is not None:
                readers.append(reader)

        return DirectoryReader.from_readers(path, readers, allow_empty=True)

    @staticmethod
    def open_files(paths):
        readers = []
        for path in paths:
            if not is_journal_file_name(os.path.basename(path)):
                raise ValueError(f'not a journal file: {path}')
            readers.append(FileReader.open(path))
        return DirectoryReader.from_readers('<files>', readers, allow_empty=False)

    @staticmethod
    def from_readers(path, readers, allow_empty=False):
        if not readers and not allow_empty:
            raise ValueError(f'no readable journal files in {path}')
        readers.sort(key=lambda r: (r.header()['head_entry_realtime'], r.header()['head_entry_seqnum']))
        return DirectoryReader(path, readers)

    def seek_head(self):
        self._realtime_seek = None
        self._realtime_seek_bound = None
        self._index = -1
        self._current_key = None
        self._direction = None
        self._reset_candidates()
        for reader in self._readers:
            reader.seek_head()

    def seek_tail(self):
        self._realtime_seek = None
        self._realtime_seek_bound = None
        self._index = -1
        self._current_key = None
        self._direction = None
        self._reset_candidates()
        for reader in self._readers:
            reader.seek_tail()

    def seek_realtime_usec(self, usec):
        self._realtime_seek = int(usec)
        self._realtime_seek_bound = None
        self._index = -1
        self._current_key = None
        self._direction = None
        self._reset_candidates()

    def step(self):
        return self._step_merged(0)

    def step_back(self):
        return self._step_merged(1)

    def _step_merged(self, direction):
        self._prepare_merge_direction(direction)

        best = None
        for idx in range(len(self._readers)):
            self._fill_candidate(idx, direction)
            candidate = self._candidates[idx]
            if candidate is None:
                continue
            if best is None:
                best = candidate
                continue
            cmp = self._compare_entry_keys(candidate['key'], best['key'])
            if (direction == 0 and cmp < 0) or (direction == 1 and cmp > 0):
                best = candidate

        if best is None:
            self._index = -1
            self._realtime_seek_bound = None
            return False

        self._index = best['reader_index']
        self._current_key = best['key']
        self._candidates[best['reader_index']] = None
        self._realtime_seek_bound = None
        return True

    def _prepare_merge_direction(self, direction):
        if self._realtime_seek is not None:
            usec = self._realtime_seek
            self._realtime_seek = None
            for reader in self._readers:
                reader.seek_realtime_usec(usec)
            self._reset_candidates()
            self._realtime_seek_bound = (usec, direction)
            self._direction = direction
            return

        if self._direction == direction:
            return

        if self._current_key is not None:
            for reader in self._readers:
                reader.seek_realtime_usec(self._current_key['realtime'])
        elif direction == 0:
            for reader in self._readers:
                reader.seek_head()
        else:
            for reader in self._readers:
                reader.seek_tail()

        self._reset_candidates()
        self._direction = direction

    def _fill_candidate(self, reader_index, direction):
        if self._candidates[reader_index] is not None:
            return
        reader = self._readers[reader_index]

        while True:
            ok = reader.step() if direction == 0 else reader.step_back()
            if not ok:
                return
            key = reader.current_entry_key()
            if key is None:
                continue
            if self._realtime_seek_bound is not None:
                usec, seek_direction = self._realtime_seek_bound
                if (seek_direction == 0 and key['realtime'] < usec) or (
                    seek_direction == 1 and key['realtime'] > usec
                ):
                    continue
            if self._current_key is not None:
                cmp = self._compare_entry_keys(key, self._current_key)
                if (direction == 0 and cmp <= 0) or (direction == 1 and cmp >= 0):
                    continue

            self._candidates[reader_index] = {'reader_index': reader_index, 'key': key}
            return

    def _compare_entry_keys(self, a, b):
        if (
            a['boot_id'] == b['boot_id'] and
            a['monotonic'] == b['monotonic'] and
            a['realtime'] == b['realtime'] and
            a['xor_hash'] == b['xor_hash'] and
            a['seqnum_id'] == b['seqnum_id'] and
            a['seqnum'] == b['seqnum']
        ):
            return 0

        if a['seqnum_id'] == b['seqnum_id']:
            cmp = _cmp_int(a['seqnum'], b['seqnum'])
            if cmp != 0:
                return cmp

        if a['boot_id'] == b['boot_id']:
            cmp = _cmp_int(a['monotonic'], b['monotonic'])
            if cmp != 0:
                return cmp
        else:
            cmp = self._compare_boot_ids(a['boot_id'], b['boot_id'])
            if cmp != 0:
                return cmp

        cmp = _cmp_int(a['realtime'], b['realtime'])
        if cmp != 0:
            return cmp
        return _cmp_int(a['xor_hash'], b['xor_hash'])

    def _compare_boot_ids(self, a, b):
        a_newest = self._boot_newest.get(a)
        b_newest = self._boot_newest.get(b)
        if a_newest is None or b_newest is None or a_newest['machine_id'] != b_newest['machine_id']:
            return 0
        return _cmp_int(a_newest['realtime'], b_newest['realtime'])

    def _reset_candidates(self):
        self._candidates = [None] * len(self._readers)

    def next(self):
        return self.step()

    def previous(self):
        return self.step_back()

    def get_entry(self):
        if self._index < 0 or self._index >= len(self._readers):
            return None
        return self._readers[self._index].get_entry()

    def get_realtime_usec(self):
        if self._index < 0 or self._index >= len(self._readers):
            return 0
        return self._readers[self._index].get_realtime_usec()

    def get_cursor(self):
        if self._index < 0 or self._index >= len(self._readers):
            return None
        return self._readers[self._index].get_cursor()

    def test_cursor(self, cursor):
        if self._index < 0 or self._index >= len(self._readers):
            return False
        return self._readers[self._index].test_cursor(cursor)

    def add_match(self, data):
        for r in self._readers:
            r.add_match(data)
        self._reset_merge_state()

    def add_disjunction(self):
        for r in self._readers:
            r.add_disjunction()
        self._reset_merge_state()

    def add_conjunction(self):
        for r in self._readers:
            r.add_conjunction()
        self._reset_merge_state()

    def flush_matches(self):
        for r in self._readers:
            r.flush_matches()
        self._reset_merge_state()

    def enumerate_fields(self):
        fields = set()
        for r in self._readers:
            fields.update(r.enumerate_fields())
        return sorted(fields)

    def query_unique(self, field_name):
        seen = set()
        results = []
        for r in self._readers:
            vals = r.query_unique(field_name)
            for v in vals:
                key = v.hex()
                if key not in seen:
                    seen.add(key)
                    results.append(v)
        return results

    def current_entry_key(self):
        if self._index < 0 or self._index >= len(self._readers):
            return None
        return self._readers[self._index].current_entry_key()

    def visit_entry_payloads(self, visitor):
        if self._index < 0 or self._index >= len(self._readers):
            raise ValueError('no entry at current position')
        return self._readers[self._index].visit_entry_payloads(visitor)

    def collect_entry_payloads(self):
        if self._index < 0 or self._index >= len(self._readers):
            raise ValueError('no entry at current position')
        return self._readers[self._index].collect_entry_payloads()

    def get_entry_payload(self, field_name):
        if self._index < 0 or self._index >= len(self._readers):
            return None
        return self._readers[self._index].get_entry_payload(field_name)

    def get_raw(self, field_name):
        if self._index < 0 or self._index >= len(self._readers):
            return None
        return self._readers[self._index].get_raw(field_name)

    def get_raw_values(self, field_name):
        if self._index < 0 or self._index >= len(self._readers):
            return []
        return self._readers[self._index].get_raw_values(field_name)

    def entry_data_restart(self):
        if self._index < 0 or self._index >= len(self._readers):
            raise ValueError('no entry at current position')
        return self._readers[self._index].entry_data_restart()

    def enumerate_entry_payload(self):
        if self._index < 0 or self._index >= len(self._readers):
            return None
        return self._readers[self._index].enumerate_entry_payload()

    def clear_entry_data_state(self):
        if self._index < 0 or self._index >= len(self._readers):
            return
        self._readers[self._index].clear_entry_data_state()

    def list_boots(self):
        boots = {}
        for r in self._readers:
            h = r.header()
            boot_id = h['tail_entry_boot_id'].hex()
            first = h['head_entry_realtime']
            last = h['tail_entry_realtime']
            if boot_id in boots:
                boots[boot_id] = (min(boots[boot_id][0], first), max(boots[boot_id][1], last))
            else:
                boots[boot_id] = (first, last)

        result = []
        for boot_id, (first_entry, last_entry) in boots.items():
            result.append({
                'index': 0,
                'boot_id': boot_id,
                'first_entry': first_entry,
                'last_entry': last_entry,
            })
        result.sort(key=lambda x: x['first_entry'])
        base = 1 - len(result)
        for i, b in enumerate(result):
            b['index'] = base + i
        return result

    def close(self):
        close_err = None
        for r in self._readers:
            try:
                r.close()
            except Exception as e:
                if close_err is None:
                    close_err = e
        if close_err is not None:
            raise close_err

    def _reset_merge_state(self):
        self._index = -1
        self._current_key = None
        self._direction = None
        self._realtime_seek_bound = None
        self._reset_candidates()


def _collect_journal_files(path):
    files = []
    with os.scandir(path) as scan:
        entries = list(scan)
    for entry in entries:
        if _entry_is_file(entry) and is_journal_file_name(os.path.basename(entry.name)):
            files.append(entry.path)
    for entry in entries:
        if not _is_journal_subdir_name(entry.name) or not _entry_is_dir(entry):
            continue
        try:
            with os.scandir(entry.path) as children:
                for child in children:
                    if _entry_is_file(child) and is_journal_file_name(os.path.basename(child.name)):
                        files.append(child.path)
        except OSError:
            continue
    return sorted(files)


def _entry_is_file(entry):
    try:
        return entry.is_file()
    except OSError:
        return False


def _entry_is_dir(entry):
    try:
        return entry.is_dir()
    except OSError:
        return False


def _is_journal_subdir_name(name):
    if '.' in name:
        return False
    return _id128_string_valid(name)


def _id128_string_valid(value):
    if len(value) == 32:
        return all(ch in '0123456789abcdefABCDEF' for ch in value)
    if len(value) == 36:
        for idx, ch in enumerate(value):
            if idx in (8, 13, 18, 23):
                if ch != '-':
                    return False
            elif ch not in '0123456789abcdefABCDEF':
                return False
        return True
    return False


def _build_boot_newest(readers):
    newest = {}
    for reader in readers:
        header = reader.header()
        boot_id = header['tail_entry_boot_id']
        if boot_id == b'\x00' * 16:
            continue
        current = newest.get(boot_id)
        if current is None or header['tail_entry_monotonic'] > current['monotonic']:
            newest[boot_id] = {
                'machine_id': header['machine_id'],
                'monotonic': header['tail_entry_monotonic'],
                'realtime': header['tail_entry_realtime'],
            }
    return newest


def _cmp_int(a, b):
    return (a > b) - (a < b)
