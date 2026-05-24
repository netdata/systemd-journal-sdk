# Directory reader - iterates across multiple journal files.

import os
from .reader import FileReader
from .compress import is_journal_file_name


class DirectoryReader:
    def __init__(self, path, readers=None):
        self._path = path
        self._readers = readers or []
        self._index = 0

    @staticmethod
    def open(path):
        if not os.path.isdir(path):
            raise ValueError(f'not a directory: {path}')

        readers = []
        for journal_path in _collect_journal_files(path):
            try:
                readers.append(FileReader.open(journal_path))
            except Exception:
                pass

        if not readers:
            raise ValueError(f'no readable journal files in {path}')

        readers.sort(key=lambda r: (r.header()['head_entry_realtime'], r.header()['head_entry_seqnum']))
        return DirectoryReader(path, readers)

    def seek_head(self):
        self._index = 0
        if self._readers:
            self._readers[0].seek_head()

    def seek_tail(self):
        self._index = len(self._readers) - 1
        if self._readers:
            self._readers[-1].seek_tail()

    def step(self):
        while self._index < len(self._readers):
            if self._readers[self._index].step():
                return True
            self._index += 1
            if self._index < len(self._readers):
                self._readers[self._index].seek_head()
        return False

    def step_back(self):
        while True:
            if self._index >= len(self._readers):
                return False
            if self._readers[self._index].step_back():
                return True
            if self._index == 0:
                return False
            self._index -= 1
            self._readers[self._index].seek_tail()

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

    def add_disjunction(self):
        for r in self._readers:
            r.add_disjunction()

    def add_conjunction(self):
        for r in self._readers:
            r.add_conjunction()

    def flush_matches(self):
        for r in self._readers:
            r.flush_matches()

    def enumerate_fields(self):
        fields = set()
        for r in self._readers:
            r.seek_head()
            while r.next():
                try:
                    entry = r.get_entry()
                    if entry:
                        fields.update(entry['fields'].keys())
                except Exception:
                    pass
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
        for r in self._readers:
            r.close()


def _collect_journal_files(path):
    files = []
    entries = list(os.scandir(path))
    for entry in entries:
        if entry.is_file() and is_journal_file_name(os.path.basename(entry.name)):
            files.append(entry.path)
    for entry in entries:
        if not entry.is_dir():
            continue
        for child in os.scandir(entry.path):
            if child.is_file() and is_journal_file_name(os.path.basename(child.name)):
                files.append(child.path)
    return files
