# High-level directory writer with rotation and retention.

import os
import re

from .binary import random_uuid, uuid_to_string
from .writer import Writer


DEFAULT_MAX_ENTRIES = 100000
DEFAULT_MAX_BYTES = 128 * 1024 * 1024
DEFAULT_MAX_FILES = 10
DEFAULT_RETENTION_BYTES = 1024 * 1024 * 1024


class Log:
    def __init__(self, path, config=None):
        config = config or {}
        if not path:
            raise ValueError('invalid journal directory')
        self._root_path = path
        self._source = config.get('source', 'system')
        _validate_journal_source(self._source)

        self._max_entries = config.get('max_entries', DEFAULT_MAX_ENTRIES)
        self._max_bytes = config.get('max_bytes', DEFAULT_MAX_BYTES)
        self._max_files = config.get('max_files', DEFAULT_MAX_FILES)
        self._max_retention_bytes = config.get('max_retention_bytes', DEFAULT_RETENTION_BYTES)

        self._next_seqnum = int(config.get('head_seqnum', 1))
        self._seqnum_id = _uuid_from_config(config.get('seqnum_id'))
        self._boot_id = _uuid_from_config(config.get('boot_id'))
        self._machine_id = _uuid_from_config(config.get('machine_id')) or _read_machine_id() or random_uuid()
        self._compression = config.get('compression', 'none')
        self._compression_threshold_bytes = config.get('compression_threshold_bytes')
        self._compact = config.get('compact') is True or config.get('format') == 'compact'
        self._journal_dir = os.path.join(self._root_path, uuid_to_string(self._machine_id))
        self._active_file = os.path.join(self._journal_dir, f'{self._source}.journal')
        self._active_writer = None
        self._closed = False

        os.makedirs(self._journal_dir, exist_ok=True)

    def _open_writer(self):
        if self._active_writer:
            return
        if os.path.exists(self._active_file):
            self._active_writer = Writer.open(self._active_file)
        else:
            opts = {
                'head_seqnum': self._next_seqnum,
                'machine_id': self._machine_id,
                'compression': self._compression,
                'compact': self._compact,
            }
            if self._compression_threshold_bytes is not None:
                opts['compression_threshold_bytes'] = self._compression_threshold_bytes
            if self._seqnum_id:
                opts['seqnum_id'] = self._seqnum_id
            if self._boot_id:
                opts['boot_id'] = self._boot_id
            self._active_writer = Writer.create(self._active_file, opts)
        self._capture_writer_identity()

    def _capture_writer_identity(self):
        h = self._active_writer._header
        self._next_seqnum = self._active_writer._next_seqnum
        self._seqnum_id = h['seqnum_id']
        self._boot_id = self._active_writer._boot_id
        self._machine_id = h['machine_id']

    def append(self, fields, opts=None):
        if self._closed:
            raise ValueError('journal log is closed')
        self._open_writer()
        result = self._active_writer.append(fields, opts)
        self._capture_writer_identity()
        h = self._active_writer._header
        if h['n_entries'] >= self._max_entries or self._active_writer.current_size() >= self._max_bytes:
            self._rotate()
        return result

    def _rotate(self):
        if not self._active_writer:
            return
        h = self._active_writer._header
        self._capture_writer_identity()
        archive_path = self._archive_path_for(h)
        self._active_writer.archive_to(archive_path)
        self._active_writer = None
        self._apply_retention()
        self._active_file = os.path.join(self._journal_dir, f'{self._source}.journal')

    def _archive_path_for(self, header):
        return os.path.join(
            self._journal_dir,
            f'{self._source}@{uuid_to_string(header["seqnum_id"])}-'
            f'{_hex64(header["head_entry_seqnum"])}-{_hex64(header["head_entry_realtime"])}.journal',
        )

    def _apply_retention(self):
        archives = []
        for name in os.listdir(self._journal_dir):
            parsed = _parse_archive_name(name, self._source)
            if parsed is None:
                continue
            path = os.path.join(self._journal_dir, name)
            try:
                stat = os.stat(path)
            except FileNotFoundError:
                continue
            archives.append({
                'path': path,
                'size': stat.st_size,
                'head_seqnum': parsed['head_seqnum'],
                'head_realtime': parsed['head_realtime'],
            })

        archives.sort(key=lambda f: (f['head_realtime'], f['head_seqnum'], f['path']))
        total_bytes = sum(f['size'] for f in archives)
        try:
            total_bytes += os.stat(self._active_file).st_size
        except FileNotFoundError:
            pass

        while len(archives) > self._max_files:
            oldest = archives.pop(0)
            try:
                os.unlink(oldest['path'])
                total_bytes = max(0, total_bytes - oldest['size'])
            except FileNotFoundError:
                pass

        while total_bytes > self._max_retention_bytes and archives:
            oldest = archives.pop(0)
            try:
                os.unlink(oldest['path'])
                total_bytes = max(0, total_bytes - oldest['size'])
            except FileNotFoundError:
                pass

        _sync_directory(self._journal_dir)

    def sync(self):
        if self._closed:
            raise ValueError('journal log is closed')
        if self._active_writer:
            self._active_writer.sync()

    def close(self):
        if self._closed:
            return
        if self._active_writer:
            if self._active_writer._header['n_entries'] == 0:
                self._active_writer.close()
                try:
                    os.unlink(self._active_file)
                except FileNotFoundError:
                    pass
            else:
                self._capture_writer_identity()
                self._active_writer.archive_to(self._archive_path_for(self._active_writer._header))
                self._apply_retention()
            self._active_writer = None
        self._closed = True

    def active_file(self):
        return self._active_file

    def journal_directory(self):
        return self._journal_dir


def _validate_journal_source(source):
    if source in ('', '.', '..'):
        raise ValueError('invalid journal source')
    for ch in source:
        if not (ch.isascii() and (ch.isalnum() or ch in '_.-')):
            raise ValueError('invalid journal source')


def _uuid_from_config(value):
    if value is None:
        return None
    if isinstance(value, str):
        return bytes.fromhex(value.replace('-', ''))
    if isinstance(value, bytearray):
        value = bytes(value)
    if not isinstance(value, bytes) or len(value) != 16:
        raise ValueError('uuid values must be 16 bytes or 32 hex characters')
    return value


def _read_machine_id():
    try:
        with open('/etc/machine-id', 'r', encoding='utf-8') as f:
            text = f.read().strip()
    except OSError:
        return None
    if re.fullmatch(r'[0-9a-fA-F]{32}', text):
        return bytes.fromhex(text)
    return None


def _parse_archive_name(name, source):
    if not name.endswith('.journal'):
        return None
    stem = name[:-len('.journal')]
    prefix = f'{source}@'
    if not stem.startswith(prefix):
        return None
    parts = stem[len(prefix):].split('-')
    if len(parts) != 3:
        return None
    if not re.fullmatch(r'[0-9a-fA-F]{32}', parts[0]):
        return None
    if not re.fullmatch(r'[0-9a-fA-F]{16}', parts[1]):
        return None
    if not re.fullmatch(r'[0-9a-fA-F]{16}', parts[2]):
        return None
    return {
        'head_seqnum': int(parts[1], 16),
        'head_realtime': int(parts[2], 16),
    }


def _hex64(value):
    return f'{int(value):016x}'


def _sync_directory(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)
