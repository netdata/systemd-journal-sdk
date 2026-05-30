# High-level directory writer with rotation and retention.

import os
import re
import time

from ._platform import boot_id_bytes, sync_directory
from .binary import random_uuid, uuid_to_string
from .header import HEADER_SIZE, OBJECT_HEADER_SIZE, STATE_ONLINE, parse_file_header, parse_object_header
from .header import normalize_journal_max_file_size
from .writer import (
    Writer,
    _normalize_field_name_policy,
    _prepare_fields_for_policy,
    _prepare_raw_payloads_for_policy,
    _writer_policy_for_log_policy,
)


DEFAULT_MAX_ENTRIES = 0
DEFAULT_MAX_BYTES = 0
DEFAULT_MAX_DURATION_USEC = 0
DEFAULT_MAX_FILES = 0
DEFAULT_RETENTION_BYTES = 0
DEFAULT_RETENTION_AGE_USEC = 0
DERIVED_ROTATION_FRACTION = 20

LOG_OPEN_LAZY = 'lazy'
LOG_OPEN_EAGER = 'eager'
LOG_IDENTITY_AUTO = 'auto'
LOG_IDENTITY_STRICT = 'strict'
LOG_LIFECYCLE_CREATED = 'created'
LOG_LIFECYCLE_ROTATED = 'rotated'
LOG_LIFECYCLE_DELETED = 'deleted'
LOG_LIFECYCLE_REASON_APPEND = 'append'
LOG_LIFECYCLE_REASON_EAGER_OPEN = 'eager_open'
LOG_LIFECYCLE_REASON_ROTATION = 'rotation'
LOG_LIFECYCLE_REASON_RETENTION = 'retention'


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
        self._open_mode = _normalize_open_mode(config)
        self._identity_mode = _normalize_identity_mode(config)
        self._lifecycle = _normalize_lifecycle(_option(config, 'lifecycle', 'lifecycle_observer', 'lifecycleObserver'))
        self._lifecycle_error_handler = _option(config, 'lifecycle_error_handler', 'lifecycleErrorHandler')
        self._artifact_sizer = _normalize_artifact_sizer(_option(config, 'artifact_sizer', 'artifactSizer'))
        self._compression = config.get('compression', 'none')
        self._compression_threshold_bytes = config.get('compression_threshold_bytes')
        self._compact = config.get('compact') is True or config.get('format') == 'compact'
        self._live_publish_every_entries = _option(config, 'live_publish_every_entries', 'livePublishEveryEntries')
        self._field_name_policy = _normalize_field_name_policy(_option(config, 'field_name_policy', 'fieldNamePolicy'))

        rotation_policy = _option(config, 'rotation_policy', 'rotationPolicy')
        retention_policy = _option(config, 'retention_policy', 'retentionPolicy')

        if rotation_policy is not None:
            self._max_entries = _positive_optional_number(
                _option(rotation_policy, 'max_entries', 'maxEntries'),
                'rotation max entries',
                DEFAULT_MAX_ENTRIES,
            )
            self._max_bytes = _positive_optional_number(
                _option(rotation_policy, 'max_bytes', 'maxBytes', 'max_file_size', 'maxFileSize'),
                'rotation max file size',
                DEFAULT_MAX_BYTES,
            )
            self._max_duration_usec = _positive_optional_number(
                _option(rotation_policy, 'max_duration_usec', 'maxDurationUsec', 'max_duration', 'maxDuration'),
                'rotation max duration',
                DEFAULT_MAX_DURATION_USEC,
            )
        else:
            self._max_entries = config.get('max_entries', config.get('maxEntries', DEFAULT_MAX_ENTRIES))
            self._max_bytes = config.get('max_bytes', config.get('maxBytes', DEFAULT_MAX_BYTES))
            self._max_duration_usec = int(
                config.get('max_duration_usec', config.get('maxDurationUsec', DEFAULT_MAX_DURATION_USEC))
            )

        if retention_policy is not None:
            self._max_files = _positive_optional_number(
                _option(retention_policy, 'max_files', 'maxFiles'),
                'retention max files',
                DEFAULT_MAX_FILES,
            )
            self._max_retention_bytes = _positive_optional_number(
                _option(retention_policy, 'max_bytes', 'maxBytes', 'max_retention_bytes', 'maxRetentionBytes'),
                'retention max bytes',
                DEFAULT_RETENTION_BYTES,
            )
            self._max_retention_age_usec = _positive_optional_number(
                _option(retention_policy, 'max_age_usec', 'maxAgeUsec', 'max_retention_age_usec', 'maxRetentionAgeUsec', 'max_age', 'maxAge'),
                'retention max age',
                DEFAULT_RETENTION_AGE_USEC,
            )
        else:
            self._max_files = config.get('max_files', config.get('maxFiles', DEFAULT_MAX_FILES))
            self._max_retention_bytes = config.get('max_retention_bytes', config.get('maxRetentionBytes', DEFAULT_RETENTION_BYTES))
            self._max_retention_age_usec = int(
                config.get(
                    'max_retention_age_usec',
                    config.get('maxRetentionAgeUsec', DEFAULT_RETENTION_AGE_USEC),
                )
            )
        if self._max_bytes == DEFAULT_MAX_BYTES and self._max_retention_bytes > 0:
            self._max_bytes = normalize_journal_max_file_size(
                max(1, self._max_retention_bytes // DERIVED_ROTATION_FRACTION),
                self._compact,
            )
        if self._max_duration_usec == DEFAULT_MAX_DURATION_USEC and self._max_retention_age_usec > 0:
            self._max_duration_usec = max(
                1,
                (self._max_retention_age_usec + DERIVED_ROTATION_FRACTION - 1) // DERIVED_ROTATION_FRACTION,
            )

        head_seqnum_option = _option(config, 'head_seqnum', 'headSeqnum')
        seqnum_id_option = _option(config, 'seqnum_id', 'seqnumId')
        boot_id_option = _option(config, 'boot_id', 'bootId')
        machine_id_option = _option(config, 'machine_id', 'machineId')
        if self._identity_mode == LOG_IDENTITY_STRICT:
            if machine_id_option is None:
                raise ValueError('strict identity requires machine id')
            if boot_id_option is None:
                raise ValueError('strict identity requires boot id')
        self._next_seqnum = int(head_seqnum_option or 1)
        self._seqnum_id = _uuid_from_config(seqnum_id_option) or random_uuid()
        self._boot_id = _uuid_from_config(boot_id_option) or _read_boot_id() or random_uuid()
        self._machine_id = _uuid_from_config(machine_id_option) or _read_machine_id() or random_uuid()
        self._journal_dir = os.path.join(self._root_path, uuid_to_string(self._machine_id))
        self._active_file = self._systemd_active_path() if self._strict_systemd_naming else None
        self._active_writer = None
        self._closed = False
        self._open_retention_applied = False
        self._last_realtime = 0
        self._last_monotonic = 0

        os.makedirs(self._journal_dir, exist_ok=True)
        chain_state = self._scan_chain_state()
        if head_seqnum_option is None and chain_state['tail_seqnum'] > 0:
            self._next_seqnum = chain_state['tail_seqnum'] + 1
        if seqnum_id_option is None and chain_state['seqnum_id'] is not None:
            self._seqnum_id = chain_state['seqnum_id']
        self._last_realtime = chain_state['tail_realtime']
        if chain_state['tail_boot_id'] == self._boot_id:
            self._last_monotonic = chain_state['tail_monotonic']
        if self._strict_systemd_naming and chain_state['active_file'] is not None:
            self._archive_online_chain_active(chain_state['active_file'])
        if not self._strict_systemd_naming:
            if chain_state['active_file'] is not None:
                self._attach_existing_active(chain_state['active_file'])
        if self._open_mode == LOG_OPEN_EAGER and self._active_writer is None:
            self._open_writer({'realtime_usec': int(time.time() * 1_000_000)}, LOG_LIFECYCLE_REASON_EAGER_OPEN)
        self._apply_retention_on_open()

    def _open_writer(self, opts=None, reason=LOG_LIFECYCLE_REASON_APPEND):
        opts = opts or {}
        if self._active_writer:
            return
        if self._active_file is None:
            head_realtime = opts.get('realtime_usec') or opts.get('realtimeUsec') or int(time.time() * 1_000_000)
            self._active_file = self._chain_path_for(self._seqnum_id, self._next_seqnum, head_realtime)
        if os.path.exists(self._active_file):
            self._active_writer = Writer.open(self._active_file, {
                'live_publish_every_entries': self._live_publish_every_entries,
                'field_name_policy': _writer_policy_for_log_policy(self._field_name_policy),
            })
            if self._active_writer._header['n_entries'] == 0:
                self._discard_empty_opened_writer()
                if self._active_file is None:
                    head_realtime = int((opts or {}).get('realtime_usec') or (opts or {}).get('realtimeUsec') or time.time() * 1_000_000)
                    self._active_file = self._chain_path_for(self._seqnum_id, self._next_seqnum, head_realtime)
            else:
                self._capture_writer_identity()
                return
        if os.path.exists(self._active_file):
            self._active_writer = Writer.open(self._active_file, {
                'live_publish_every_entries': self._live_publish_every_entries,
                'field_name_policy': _writer_policy_for_log_policy(self._field_name_policy),
            })
        else:
            opts = {
                'head_seqnum': self._next_seqnum,
                'machine_id': self._machine_id,
                'compression': self._compression,
                'compact': self._compact,
            }
            if self._max_bytes > 0:
                opts['max_file_size'] = self._max_bytes
            if self._compression_threshold_bytes is not None:
                opts['compression_threshold_bytes'] = self._compression_threshold_bytes
            if self._seqnum_id:
                opts['seqnum_id'] = self._seqnum_id
            if self._boot_id:
                opts['boot_id'] = self._boot_id
            if self._live_publish_every_entries is not None:
                opts['live_publish_every_entries'] = self._live_publish_every_entries
            opts['field_name_policy'] = _writer_policy_for_log_policy(self._field_name_policy)
            self._active_writer = Writer.create(self._active_file, opts)
        self._capture_writer_identity()
        if reason != LOG_LIFECYCLE_REASON_ROTATION:
            self._emit_lifecycle({
                'type': LOG_LIFECYCLE_CREATED,
                'reason': reason,
                'active_path': self._active_file,
                'activePath': self._active_file,
            })

    def _discard_empty_opened_writer(self):
        self._active_writer.close()
        try:
            os.unlink(self._active_file)
        except FileNotFoundError:
            pass
        self._active_writer = None
        if not self._strict_systemd_naming:
            self._active_file = None

    def _attach_existing_active(self, path):
        self._active_file = path
        self._active_writer = Writer.open(path, {
            'live_publish_every_entries': self._live_publish_every_entries,
            'field_name_policy': _writer_policy_for_log_policy(self._field_name_policy),
        })
        if self._active_writer._header['n_entries'] == 0:
            self._discard_empty_opened_writer()
            return
        self._capture_writer_identity()

    def _archive_online_chain_active(self, path):
        writer = Writer.open(path)
        if writer._header['n_entries'] == 0:
            writer.close()
            try:
                os.unlink(path)
            except FileNotFoundError:
                pass
            return
        writer.archive_to(path)

    def _capture_writer_identity(self):
        h = self._active_writer._header
        self._next_seqnum = self._active_writer._next_seqnum
        self._seqnum_id = h['seqnum_id']
        self._boot_id = self._active_writer._boot_id
        self._machine_id = h['machine_id']
        self._last_realtime = h['tail_entry_realtime']
        self._last_monotonic = h['tail_entry_monotonic']

    def append(self, fields, opts=None):
        if self._closed:
            raise ValueError('journal log is closed')
        if len(fields) == 0:
            raise ValueError('empty entry')
        opts = self._entry_options_for_append(opts)
        self._apply_retention_on_open()
        if self._active_writer and self._should_rotate(opts['realtime_usec']):
            self._rotate(opts)
        self._open_writer(opts)
        self._apply_retention_on_open()
        fields = _prepare_fields_for_policy(fields, self._field_name_policy)
        result = self._active_writer.append(self._fields_for_append(fields, opts), opts)
        self._capture_writer_identity()
        return result

    def append_raw(self, payloads, opts=None):
        if self._closed:
            raise ValueError('journal log is closed')
        payloads = _prepare_raw_payloads_for_policy(payloads, self._field_name_policy)
        opts = self._entry_options_for_append(opts)
        self._apply_retention_on_open()
        if self._active_writer and self._should_rotate(opts['realtime_usec']):
            self._rotate(opts)
        self._open_writer(opts)
        self._apply_retention_on_open()
        result = self._active_writer.append_raw(self._payloads_for_append(payloads, opts), opts)
        self._capture_writer_identity()
        return result

    def _should_rotate(self, next_realtime_usec):
        h = self._active_writer._header
        return (
            (self._max_entries > 0 and h['n_entries'] >= self._max_entries) or
            (self._max_bytes > 0 and self._active_writer.current_size() >= self._max_bytes) or
            (
                self._max_duration_usec > 0 and
                h['n_entries'] > 0 and
                h['head_entry_realtime'] > 0 and
                next_realtime_usec >= h['head_entry_realtime'] and
                next_realtime_usec - h['head_entry_realtime'] >= self._max_duration_usec
            )
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
        self._open_writer(opts, LOG_LIFECYCLE_REASON_ROTATION)
        self._emit_lifecycle({
            'type': LOG_LIFECYCLE_ROTATED,
            'reason': LOG_LIFECYCLE_REASON_ROTATION,
            'archived_path': archive_path,
            'archivedPath': archive_path,
            'active_path': self._active_file,
            'activePath': self._active_file,
        })
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
            'tail_realtime': 0,
            'tail_monotonic': 0,
            'tail_boot_id': None,
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
                state['tail_realtime'] = int(header['tail_entry_realtime'])
                state['tail_monotonic'] = int(header['tail_entry_monotonic'])
                state['tail_boot_id'] = header['tail_entry_boot_id']
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
                'size': self._retained_size(path, stat.st_size),
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
                total_bytes += self._retained_size(active_file, os.stat(active_file).st_size)
                active_extra_file = True
        except FileNotFoundError:
            pass

        file_count = len(archives) + (1 if active_extra_file else 0)
        deleted_paths = []
        while self._max_files > 0 and file_count > self._max_files:
            delete_index = next((idx for idx, file in enumerate(archives)
                                 if not active_file or file['path'] != active_file), None)
            if delete_index is None:
                break
            oldest = archives.pop(delete_index)
            try:
                os.unlink(oldest['path'])
                deleted_paths.append(oldest['path'])
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
                deleted_paths.append(oldest['path'])
                total_bytes = max(0, total_bytes - oldest['size'])
            except FileNotFoundError:
                pass

        if self._max_retention_age_usec > 0:
            cutoff = max(0, int(time.time() * 1_000_000) - self._max_retention_age_usec)
            while archives:
                delete_index = next((idx for idx, file in enumerate(archives)
                                     if file['head_realtime'] <= cutoff and
                                     (not active_file or file['path'] != active_file)), None)
                if delete_index is None:
                    break
                oldest = archives.pop(delete_index)
                try:
                    os.unlink(oldest['path'])
                    deleted_paths.append(oldest['path'])
                    total_bytes = max(0, total_bytes - oldest['size'])
                except FileNotFoundError:
                    pass

        _sync_directory(self._journal_dir)
        if deleted_paths:
            self._emit_lifecycle({
                'type': LOG_LIFECYCLE_DELETED,
                'reason': LOG_LIFECYCLE_REASON_RETENTION,
                'deleted_paths': deleted_paths,
                'deletedPaths': deleted_paths,
            })

    def enforce_retention(self):
        if self._closed:
            raise ValueError('journal log is closed')
        self._apply_retention(self._active_file)

    def _apply_retention_on_open(self):
        if self._open_retention_applied or self._active_writer is None:
            return
        self._apply_retention(self._active_file)
        self._open_retention_applied = True

    def _entry_options_for_append(self, opts):
        effective = dict(opts or {})
        if 'realtimeUsec' in effective and 'realtime_usec' not in effective:
            effective['realtime_usec'] = effective['realtimeUsec']
        if 'monotonicUsec' in effective and 'monotonic_usec' not in effective:
            effective['monotonic_usec'] = effective['monotonicUsec']
        if 'sourceRealtimeUsec' in effective and 'source_realtime_usec' not in effective:
            effective['source_realtime_usec'] = effective['sourceRealtimeUsec']
        if 'realtime_usec' not in effective:
            effective['realtime_usec'] = int(time.time() * 1_000_000)
        effective['realtime_usec'] = int(effective['realtime_usec'])
        if effective['realtime_usec'] <= self._last_realtime:
            effective['realtime_usec'] = self._last_realtime + 1
        if 'monotonic_usec' in effective:
            effective['monotonic_usec'] = int(effective['monotonic_usec'])
            if effective['monotonic_usec'] <= self._last_monotonic:
                effective['monotonic_usec'] = self._last_monotonic + 1
        return effective

    def _fields_for_append(self, fields, opts):
        with_metadata = [{
            'name': '_BOOT_ID',
            'value': uuid_to_string(self._entry_boot_id_for_append(opts)),
        }]
        source_realtime = opts.get('source_realtime_usec')
        if source_realtime:
            with_metadata.append({
                'name': '_SOURCE_REALTIME_TIMESTAMP',
                'value': str(int(source_realtime)),
            })
        with_metadata.extend(fields)
        return with_metadata

    def _payloads_for_append(self, payloads, opts):
        with_metadata = [
            f'_BOOT_ID={uuid_to_string(self._entry_boot_id_for_append(opts))}'.encode('ascii'),
        ]
        source_realtime = opts.get('source_realtime_usec')
        if source_realtime:
            with_metadata.append(f'_SOURCE_REALTIME_TIMESTAMP={int(source_realtime)}'.encode('ascii'))
        with_metadata.extend(payloads)
        return with_metadata

    def _entry_boot_id_for_append(self, opts):
        boot_id = opts.get('boot_id')
        if boot_id is None and 'bootId' in opts:
            boot_id = opts['bootId']
        return _uuid_from_config(boot_id) or self._boot_id

    def _retained_size(self, path, fallback):
        size = _committed_journal_size(path, fallback)
        if self._artifact_sizer is None:
            return size
        artifact_size = self._artifact_sizer(path)
        if artifact_size is None:
            return size
        artifact_size = int(artifact_size)
        if artifact_size < 0:
            raise ValueError('artifact size must be non-negative')
        return size + artifact_size

    def _emit_lifecycle(self, event):
        if self._lifecycle is None:
            return
        try:
            self._lifecycle(event)
        except Exception as error:
            if callable(self._lifecycle_error_handler):
                self._lifecycle_error_handler(error, event)

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

    def active_file_path(self):
        return self._active_file or ''

    def active_journal_path(self):
        return self.active_file_path()

    def journal_directory(self):
        return self._journal_dir

    def configured_directory(self):
        return self._root_path

    def machine_id(self):
        return self._machine_id

    def boot_id(self):
        return self._boot_id

    def source_name(self):
        return self._source


def _validate_journal_source(source):
    if source in ('', '.', '..'):
        raise ValueError('invalid journal source')
    for ch in source:
        if not (ch.isascii() and (ch.isalnum() or ch in '_.-')):
            raise ValueError('invalid journal source')


def _option(mapping, *names):
    for name in names:
        if name in mapping:
            return mapping[name]
    return None


def _normalize_open_mode(config):
    value = _option(config, 'open_mode', 'openMode')
    if value is None and (config.get('eager_open') is True or config.get('eagerOpen') is True):
        value = LOG_OPEN_EAGER
    if value in (None, ''):
        return LOG_OPEN_LAZY
    value = str(value).lower()
    if value in (LOG_OPEN_LAZY, LOG_OPEN_EAGER):
        return value
    raise ValueError(f'unsupported log open mode: {value}')


def _normalize_identity_mode(config):
    value = _option(config, 'identity_mode', 'identityMode')
    if value in (None, ''):
        return LOG_IDENTITY_AUTO
    value = str(value).lower()
    if value in (LOG_IDENTITY_AUTO, LOG_IDENTITY_STRICT):
        return value
    raise ValueError(f'unsupported log identity mode: {value}')


def _positive_optional_number(value, label, fallback):
    if value is None:
        return fallback
    value = int(value)
    if value <= 0:
        raise ValueError(f'{label} must be greater than 0')
    return value


def _normalize_lifecycle(value):
    if value is None:
        return None
    if callable(value):
        return value
    callback = getattr(value, 'on_log_lifecycle_event', None)
    if callable(callback):
        return callback
    callback = getattr(value, 'on_lifecycle_event', None)
    if callable(callback):
        return callback
    raise ValueError('lifecycle must be callable or observer object')


def _normalize_artifact_sizer(value):
    if value is None:
        return None
    if callable(value):
        return value
    callback = getattr(value, 'journal_artifact_size', None)
    if callable(callback):
        return callback
    callback = getattr(value, 'JournalArtifactSize', None)
    if callable(callback):
        return callback
    raise ValueError('artifact sizer must be callable or provider object')


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


def _read_boot_id():
    return boot_id_bytes()


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
    return sync_directory(path)
