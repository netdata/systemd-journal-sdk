# High-level directory writer with rotation and retention.

import os
import re
import time

from .binary import random_uuid, uuid_to_string
from .header import HEADER_SIZE, OBJECT_HEADER_SIZE, STATE_ONLINE, parse_file_header, parse_object_header
from .writer import Writer


DEFAULT_MAX_ENTRIES = 0
DEFAULT_MAX_BYTES = 0
DEFAULT_MAX_FILES = 0
DEFAULT_RETENTION_BYTES = 0


class Log:
    def __init__(self, path, config=None):
        config = config or {}
        if not path:
            raise ValueError('invalid journal directory')
        self._root_path = path
        self._source = config.get('source', 'system')
        _validate_journal_source(self._source)
        self._strict_systemd_naming = (
            config.get('strict_systemd_naming') is True or
            config.get('strictSystemdNaming') is True
        )

        self._max_entries = config.get('max_entries', DEFAULT_MAX_ENTRIES)
        self._max_bytes = config.get('max_bytes', DEFAULT_MAX_BYTES)
        self._max_files = config.get('max_files', DEFAULT_MAX_FILES)
        self._max_retention_bytes = config.get('max_retention_bytes', DEFAULT_RETENTION_BYTES)

        self._next_seqnum = int(config.get('head_seqnum', 1))
        self._seqnum_id = _uuid_from_config(config.get('seqnum_id')) or random_uuid()
        self._boot_id = _uuid_from_config(config.get('boot_id'))
        self._machine_id = _uuid_from_config(config.get('machine_id')) or _read_machine_id() or random_uuid()
        self._compression = config.get('compression', 'none')
        self._compression_threshold_bytes = config.get('compression_threshold_bytes')
        self._compact = config.get('compact') is True or config.get('format') == 'compact'
        self._journal_dir = os.path.join(self._root_path, uuid_to_string(self._machine_id))
        self._active_file = self._systemd_active_path() if self._strict_systemd_naming else None
        self._active_writer = None
        self._closed = False

        os.makedirs(self._journal_dir, exist_ok=True)
        chain_state = self._scan_chain_state()
        if 'head_seqnum' not in config and chain_state['tail_seqnum'] > 0:
            self._next_seqnum = chain_state['tail_seqnum'] + 1
        if 'seqnum_id' not in config and chain_state['seqnum_id'] is not None:
            self._seqnum_id = chain_state['seqnum_id']
        if not self._strict_systemd_naming:
            if chain_state['active_file'] is not None:
                self._active_file = chain_state['active_file']

    def _open_writer(self, opts=None):
        opts = opts or {}
        if self._active_writer:
            return
        if self._active_file is None:
            head_realtime = opts.get('realtime_usec') or opts.get('realtimeUsec') or int(time.time() * 1_000_000)
            self._active_file = self._chain_path_for(self._seqnum_id, self._next_seqnum, head_realtime)
        if os.path.exists(self._active_file):
            self._active_writer = Writer.open(self._active_file)
            if self._active_writer._header['n_entries'] == 0:
                self._discard_empty_opened_writer()
                if self._active_file is None:
                    head_realtime = int((opts or {}).get('realtime_usec') or (opts or {}).get('realtimeUsec') or time.time() * 1_000_000)
                    self._active_file = self._chain_path_for(self._seqnum_id, self._next_seqnum, head_realtime)
            else:
                self._capture_writer_identity()
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

    def _discard_empty_opened_writer(self):
        self._active_writer.close()
        try:
            os.unlink(self._active_file)
        except FileNotFoundError:
            pass
        self._active_writer = None
        if not self._strict_systemd_naming:
            self._active_file = None

    def _capture_writer_identity(self):
        h = self._active_writer._header
        self._next_seqnum = self._active_writer._next_seqnum
        self._seqnum_id = h['seqnum_id']
        self._boot_id = self._active_writer._boot_id
        self._machine_id = h['machine_id']

    def append(self, fields, opts=None):
        if self._closed:
            raise ValueError('journal log is closed')
        if len(fields) == 0:
            raise ValueError('empty entry')
        if self._active_writer and self._should_rotate():
            self._rotate(opts)
        self._open_writer(opts)
        result = self._active_writer.append(fields, opts)
        self._capture_writer_identity()
        return result

    def _should_rotate(self):
        h = self._active_writer._header
        return (
            (self._max_entries > 0 and h['n_entries'] >= self._max_entries) or
            (self._max_bytes > 0 and self._active_writer.current_size() >= self._max_bytes)
        )

    def _rotate(self, opts=None):
        if not self._active_writer:
            return
        h = self._active_writer._header
        self._capture_writer_identity()
        archive_path = self._archive_path_for(h) if self._strict_systemd_naming else self._active_file
        try:
            self._active_writer.archive_to(archive_path)
        except Exception:
            if self._active_writer._closed:
                self._active_writer = None
                self._active_file = self._systemd_active_path() if self._strict_systemd_naming else None
            raise
        self._active_writer = None
        self._active_file = self._systemd_active_path() if self._strict_systemd_naming else None
        self._open_writer(opts)
        self._apply_retention(self._active_file)

    def _archive_path_for(self, header):
        return self._chain_path_for(
            header['seqnum_id'],
            header['head_entry_seqnum'],
            header['head_entry_realtime'],
        )

    def _systemd_active_path(self):
        return os.path.join(self._journal_dir, f'{self._source}.journal')

    def _chain_path_for(self, seqnum_id, head_seqnum, head_realtime):
        return os.path.join(
            self._journal_dir,
            f'{self._source}@{uuid_to_string(seqnum_id)}-'
            f'{_hex64(head_seqnum)}-{_hex64(head_realtime)}.journal',
        )

    def _scan_chain_state(self):
        state = {
            'tail_seqnum': 0,
            'seqnum_id': None,
            'active_file': None,
            'active_tail_seqnum': 0,
            'active_head_realtime': 0,
        }
        for name in os.listdir(self._journal_dir):
            if _parse_archive_name(name, self._source) is None:
                continue
            path = os.path.join(self._journal_dir, name)
            try:
                with open(path, 'rb') as f:
                    header = parse_file_header(f.read(HEADER_SIZE))
            except Exception:
                continue
            if int(header['tail_entry_seqnum']) > state['tail_seqnum']:
                state['tail_seqnum'] = int(header['tail_entry_seqnum'])
                state['seqnum_id'] = header['seqnum_id']
            if (
                header['state'] == STATE_ONLINE and
                (
                    state['active_file'] is None or
                    int(header['tail_entry_seqnum']) > state['active_tail_seqnum'] or
                    (
                        int(header['tail_entry_seqnum']) == state['active_tail_seqnum'] and
                        int(header['head_entry_realtime']) > state['active_head_realtime']
                    )
                )
            ):
                state['active_file'] = path
                state['active_tail_seqnum'] = int(header['tail_entry_seqnum'])
                state['active_head_realtime'] = int(header['head_entry_realtime'])
        return state

    def _apply_retention(self, protected_file=None):
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
                'size': _committed_journal_size(path, stat.st_size),
                'head_seqnum': parsed['head_seqnum'],
                'head_realtime': parsed['head_realtime'],
            })

        archives.sort(key=lambda f: (f['head_realtime'], f['head_seqnum'], f['path']))
        active_file = self._active_file if protected_file is None else protected_file
        active_in_archives = False
        total_bytes = 0
        for file in archives:
            if active_file and file['path'] == active_file:
                active_in_archives = True
            total_bytes += file['size']
        active_extra_file = False
        try:
            if active_file and not active_in_archives:
                total_bytes += _committed_journal_size(active_file, os.stat(active_file).st_size)
                active_extra_file = True
        except FileNotFoundError:
            pass

        file_count = len(archives) + (1 if active_extra_file else 0)
        while self._max_files > 0 and file_count > self._max_files:
            delete_index = next((idx for idx, file in enumerate(archives)
                                 if not active_file or file['path'] != active_file), None)
            if delete_index is None:
                break
            oldest = archives.pop(delete_index)
            try:
                os.unlink(oldest['path'])
                total_bytes = max(0, total_bytes - oldest['size'])
                file_count -= 1
            except FileNotFoundError:
                pass

        while self._max_retention_bytes > 0 and total_bytes > self._max_retention_bytes and archives:
            delete_index = next((idx for idx, file in enumerate(archives)
                                 if not active_file or file['path'] != active_file), None)
            if delete_index is None:
                break
            oldest = archives.pop(delete_index)
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
            if self._active_writer._header['n_entries'] == 0 and self._strict_systemd_naming:
                try:
                    self._active_writer.close()
                    try:
                        os.unlink(self._active_file)
                    except FileNotFoundError:
                        pass
                except Exception:
                    if self._active_writer._closed:
                        self._active_writer = None
                        self._closed = True
                    raise
            else:
                self._capture_writer_identity()
                archive_path = (
                    self._archive_path_for(self._active_writer._header)
                    if self._strict_systemd_naming else self._active_file
                )
                try:
                    self._active_writer.archive_to(archive_path)
                except Exception:
                    if self._active_writer._closed:
                        self._active_file = archive_path
                        self._active_writer = None
                        self._closed = True
                    raise
                self._active_file = archive_path
                self._active_writer = None
                self._closed = True
                self._apply_retention(archive_path)
                return
            self._active_writer = None
        self._closed = True

    def active_file(self):
        return self._active_file or self._chain_path_for(self._seqnum_id, self._next_seqnum, 0)

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


def _committed_journal_size(path, fallback):
    try:
        with open(path, 'rb') as f:
            header = parse_file_header(f.read(HEADER_SIZE))
            tail_object_offset = int(header['tail_object_offset'])
            if tail_object_offset == 0:
                return fallback
            f.seek(tail_object_offset)
            obj = parse_object_header(f.read(OBJECT_HEADER_SIZE))
            if obj is None:
                return fallback
            return _align8(tail_object_offset + int(obj['size']))
    except Exception:
        return fallback


def _align8(value):
    return (int(value) + 7) & ~7


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
