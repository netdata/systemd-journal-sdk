#!/usr/bin/env python3
"""Package-level tests for the pure-Python journal SDK slice."""

import importlib.util
import json
import os
import shutil
import stat
import subprocess  # nosec B404 - subprocess is required by harnesses.
import sys
import tempfile
import time
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
PYTHON_ROOT = REPO_ROOT / 'python'
sys.path.insert(0, str(PYTHON_ROOT))
VALID_FSS_VERIFICATION_KEY = 'c262bd-85187f-0b1b04-877cc5/1c7af8-35a4e900'

from journal import (  # noqa: E402
    DirectoryReader,
    FileReader,
    Log,
    SdJournalOpen,
    SdJournalOpenFiles,
    SdJournalNext,
    SdJournalPrevious,
    SdJournalSeekRealtimeUsec,
    SdJournalSeekCursor,
    SdJournalGetEntry,
    SdJournalGetCursor,
    SdJournalTestCursor,
    SdJournalGetSeqnum,
    SdJournalGetMonotonicUsec,
    SdJournalRestartData,
    SdJournalEnumerateAvailableData,
    SdJournalGetData,
    SdJournalQueryUnique,
    SdJournalQueryUniqueState,
    SdJournalEnumerateAvailableUnique,
    SdJournalRestartFields,
    SdJournalEnumerateField,
    FIELD_NAME_POLICY_JOURNAL_APP,
    FIELD_NAME_POLICY_RAW,
    Writer,
    export_entry,
    json_entry,
    parse_match_string,
)
from journal.entry import parse_data_object  # noqa: E402
from journal.facade import _payload_from_field_value  # noqa: E402
from journal import reader as reader_module  # noqa: E402
from journal.header import (  # noqa: E402
    COMPATIBLE_SEALED,
    COMPACT_DATA_OBJECT_HEADER_SIZE,
    DATA_OBJECT_HEADER_SIZE,
    ENTRY_OBJECT_HEADER_SIZE,
    FILE_SIZE_INCREASE,
    HEADER_SIZE,
    INCOMPATIBLE_COMPACT,
    INCOMPATIBLE_COMPRESSED_LZ4,
    INCOMPATIBLE_KEYED_HASH,
    JOURNAL_COMPACT_SIZE_MAX,
    OBJECT_COMPRESSED_LZ4,
    OBJECT_COMPRESSED_XZ,
    OBJECT_COMPRESSED_ZSTD,
    OBJECT_TYPE_DATA,
    OBJECT_TYPE_ENTRY,
    STATE_ARCHIVED,
    DEFAULT_FIELD_HASH_BUCKETS,
    data_hash_buckets_for_max_file_size,
    parse_file_header,
    parse_object_header,
    write_object_header,
)
from journal.seal import COMPATIBLE_SEALED_CONTINUOUS, OBJECT_TYPE_TAG  # noqa: E402
from journal.hash import sip_hash_24  # noqa: E402
from journal.fss import gen_mk, gen_state0, evolve, seek, get_key, get_epoch  # noqa: E402


def run(args, *, input_data=None, cwd=REPO_ROOT):
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    result = subprocess.run(  # nosec B603 - harness uses shell=False command vectors.
        args,
        input=input_data,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        raise AssertionError(
            f'command failed {args}: exit={result.returncode}\n'
            f'stdout={result.stdout.decode(errors="replace")}\n'
            f'stderr={result.stderr.decode(errors="replace")}'
        )
    return result.stdout


def journal_files(directory):
    return sorted(
        str(Path(directory) / name)
        for name in os.listdir(directory)
        if name.endswith('.journal')
    )


def disposed_journal_files(directory):
    return sorted(
        str(Path(directory) / name)
        for name in os.listdir(directory)
        if name.endswith('.journal~')
    )


def clear_keyed_hash_flag(path):
    with open(path, 'r+b') as f:
        f.seek(12)
        flags = int.from_bytes(f.read(4), 'little')
        f.seek(12)
        f.write((flags & ~INCOMPATIBLE_KEYED_HASH).to_bytes(4, 'little'))


def write_header_size(path, size):
    with open(path, 'r+b') as f:
        f.seek(88)
        f.write(int(size).to_bytes(8, 'little'))


def collect_nullable(next_func):
    values = []
    while True:
        value = next_func()
        if value is None:
            return values
        values.append(value)


def journalctl_available():
    return shutil.which('journalctl') is not None


def zstd_available():
    return importlib.util.find_spec('compression.zstd') is not None


def verify_journal_file_if_available(path):
    if not journalctl_available():
        return
    run(['journalctl', '--verify', '--file', path])


def verify_journal_file_fails_if_available(path, expected_text):
    if not journalctl_available():
        return
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    result = subprocess.run(  # nosec B603
        ['journalctl', '--verify', '--file', path],
        cwd=REPO_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode == 0:
        raise AssertionError(f'journalctl --verify unexpectedly passed for {path}')
    output = (result.stdout + result.stderr).decode(errors='replace').lower()
    if expected_text.lower() not in output:
        raise AssertionError(
            f'journalctl --verify output missing {expected_text!r}: {output}'
        )


def verify_journal_file_with_key_if_available(path, key, label='journalctl verify'):
    if not journalctl_available():
        return
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    result = subprocess.run(  # nosec B603
        ['journalctl', '--verify', '--verify-key', key, '--file', str(path)],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, f'{label} failed: {result.stderr}'
    assert 'PASS:' in result.stderr


def verify_journal_file_with_key_fails_if_available(path, key):
    if not journalctl_available():
        return
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    result = subprocess.run(  # nosec B603
        ['journalctl', '--verify', '--verify-key', key, '--file', str(path)],
        capture_output=True,
        text=True,
    )
    assert result.returncode != 0, 'expected verify to fail'


def test_windows_import_safety_without_fcntl():
    script = f"""
import builtins
import sys

sys.path.insert(0, {str(PYTHON_ROOT)!r})
real_import = builtins.__import__

def guarded_import(name, globals=None, locals=None, fromlist=(), level=0):
    if name == 'fcntl':
        raise ModuleNotFoundError("No module named 'fcntl'")
    return real_import(name, globals, locals, fromlist, level)

builtins.__import__ = guarded_import
import journal
assert journal.Writer is not None
assert journal.Log is not None
assert journal.FileReader is not None
print('ok')
"""
    assert run([sys.executable, '-c', script]).strip() == b'ok'


def journalctl_directory_rows_if_available(directory, *matches):
    if not journalctl_available():
        return None
    output = run([
        'journalctl',
        '--directory',
        directory,
        '--output=json',
        '--no-pager',
        *matches,
    ])
    text = output.decode().strip()
    return [] if text == '' else [json.loads(line) for line in text.splitlines()]


def journalctl_file_rows_if_available(path, *matches):
    if not journalctl_available():
        return None
    output = run([
        'journalctl',
        '--file',
        str(path),
        '--output=json',
        '--no-pager',
        *matches,
    ])
    text = output.decode().strip()
    return [] if text == '' else [json.loads(line) for line in text.splitlines()]


def journal_has_data_object_flag(path, flag):
    data = Path(path).read_bytes()
    offset = HEADER_SIZE
    while offset + 16 <= len(data):
        obj = parse_object_header(data, offset)
        if obj is None or obj['type'] == 0 or obj['size'] == 0:
            return False
        if obj['type'] == OBJECT_TYPE_DATA and obj['flags'] & flag:
            return True
        offset = ((offset + obj['size'] + 7) // 8) * 8
    return False


def test_match_validation():
    for item in ('foobar', '', '=', '=xxxxx'):
        try:
            parse_match_string(item)
        except ValueError:
            pass
        else:
            raise AssertionError(f'expected invalid match rejection for {item!r}')
    parse_match_string('FOOBAR=waldo')


def test_siphash_masks_long_message_length():
    key = bytes.fromhex('de5f2812d87b89e81af97cfe8e1423e9')
    payload = b'COMPRESSED_PAYLOAD=' + bytes((i % 26) + 0x41 for i in range(256))
    assert sip_hash_24(key, payload) == 0xf9a795df589b5204


def test_live_publish_every_entries_preserves_closed_file_bytes():
    def write_file(directory, name, every):
        path = os.path.join(directory, f'{name}.journal')
        writer = Writer.create(path, {
            'file_id': bytes.fromhex('40000000000000000000000000000000'),
            'machine_id': bytes.fromhex('10000000000000000000000000000000'),
            'boot_id': bytes.fromhex('20000000000000000000000000000000'),
            'seqnum_id': bytes.fromhex('30000000000000000000000000000000'),
            'data_hash_table_buckets': 64,
            'field_hash_table_buckets': 16,
            'live_publish_every_entries': every,
        })
        for i in range(5):
            writer.append([
                {'name': 'MESSAGE', 'value': f'row-{i:02d}'},
                {'name': 'SYSLOG_IDENTIFIER', 'value': 'python-live-publish-test'},
            ], {
                'realtime_usec': 1_700_000_100_000_000 + i,
                'monotonic_usec': i + 1,
            })
        pending = writer._entries_since_live_publication
        writer.close()
        return Path(path).read_bytes(), pending

    with tempfile.TemporaryDirectory() as td:
        immediate, immediate_pending = write_file(td, 'immediate', 1)
        disabled, disabled_pending = write_file(td, 'disabled', 0)
        every_three, every_three_pending = write_file(td, 'every-three', 3)

    assert immediate_pending == 0
    assert disabled_pending == 0
    assert every_three_pending == 2
    assert disabled == immediate
    assert every_three == immediate


def test_journald_field_policy_validation():
    from journal.writer import _validate_field_name_for_policy

    for item in ('message=value', 'Priority=value', '_myfield=value'):
        try:
            parse_match_string(item)
        except ValueError:
            pass
        else:
            raise AssertionError(f'expected lowercase match rejection for {item!r}')

    for field in ('message', 'Priority', '_myfield'):
        try:
            _validate_field_name_for_policy(field)
        except ValueError:
            pass
        else:
            raise AssertionError(f'expected lowercase writer rejection for {field!r}')

    _validate_field_name_for_policy('_HOSTNAME')
    try:
        _validate_field_name_for_policy('_HOSTNAME', FIELD_NAME_POLICY_JOURNAL_APP)
    except ValueError:
        pass
    else:
        raise AssertionError('expected protected app field rejection')


def test_live_delay_parser():
    path = PYTHON_ROOT / 'cmd/livewriter.py'
    spec = importlib.util.spec_from_file_location('livewriter_for_tests', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    assert module.parse_delay_seconds('0') == 0.0
    assert abs(module.parse_delay_seconds('10ns') - 10e-9) < 1e-15
    assert abs(module.parse_delay_seconds('10us') - 10e-6) < 1e-12
    assert abs(module.parse_delay_seconds('10ms') - 0.01) < 1e-12
    assert abs(module.parse_delay_seconds('2s') - 2.0) < 1e-12


def test_parse_file_header_historical_field_boundaries():
    cases = [
        {'header_size': 208},
        {'header_size': 216, 'n_data': 11},
        {'header_size': 220, 'n_data': 11},
        {'header_size': 224, 'n_data': 11, 'n_fields': 22},
        {'header_size': 232, 'n_data': 11, 'n_fields': 22, 'n_tags': 33},
        {'header_size': 240, 'n_data': 11, 'n_fields': 22, 'n_tags': 33, 'n_entry_arrays': 44},
        {'header_size': 248, 'n_data': 11, 'n_fields': 22, 'n_tags': 33, 'n_entry_arrays': 44, 'data_hash_chain_depth': 55},
        {'header_size': 250, 'n_data': 11, 'n_fields': 22, 'n_tags': 33, 'n_entry_arrays': 44, 'data_hash_chain_depth': 55},
        {'header_size': 256, 'n_data': 11, 'n_fields': 22, 'n_tags': 33, 'n_entry_arrays': 44, 'data_hash_chain_depth': 55, 'field_hash_chain_depth': 66},
        {
            'header_size': 260,
            'n_data': 11,
            'n_fields': 22,
            'n_tags': 33,
            'n_entry_arrays': 44,
            'data_hash_chain_depth': 55,
            'field_hash_chain_depth': 66,
            'tail_entry_array_offset': 77,
        },
        {
            'header_size': 264,
            'n_data': 11,
            'n_fields': 22,
            'n_tags': 33,
            'n_entry_arrays': 44,
            'data_hash_chain_depth': 55,
            'field_hash_chain_depth': 66,
            'tail_entry_array_offset': 77,
            'tail_entry_array_n_entries': 88,
        },
        {
            'header_size': 268,
            'n_data': 11,
            'n_fields': 22,
            'n_tags': 33,
            'n_entry_arrays': 44,
            'data_hash_chain_depth': 55,
            'field_hash_chain_depth': 66,
            'tail_entry_array_offset': 77,
            'tail_entry_array_n_entries': 88,
        },
        {
            'header_size': 272,
            'n_data': 11,
            'n_fields': 22,
            'n_tags': 33,
            'n_entry_arrays': 44,
            'data_hash_chain_depth': 55,
            'field_hash_chain_depth': 66,
            'tail_entry_array_offset': 77,
            'tail_entry_array_n_entries': 88,
            'tail_entry_offset': 99,
        },
        {
            'header_size': 300,
            'n_data': 11,
            'n_fields': 22,
            'n_tags': 33,
            'n_entry_arrays': 44,
            'data_hash_chain_depth': 55,
            'field_hash_chain_depth': 66,
            'tail_entry_array_offset': 77,
            'tail_entry_array_n_entries': 88,
            'tail_entry_offset': 99,
        },
    ]
    fields = [
        'n_data',
        'n_fields',
        'n_tags',
        'n_entry_arrays',
        'data_hash_chain_depth',
        'field_hash_chain_depth',
        'tail_entry_array_offset',
        'tail_entry_array_n_entries',
        'tail_entry_offset',
    ]
    for expected in cases:
        header = parse_file_header(_historical_header_fixture(expected['header_size']))
        for field in fields:
            assert header[field] == expected.get(field, 0), (
                f'{field} for header_size={expected["header_size"]}: '
                f'{header[field]} != {expected.get(field, 0)}'
            )
    try:
        parse_file_header(_historical_header_fixture(300)[:208])
    except ValueError as err:
        assert 'header buffer too small' in str(err)
    else:
        raise AssertionError('future header with truncated known prefix should be rejected')


def _historical_header_fixture(header_size, incompatible_flags=INCOMPATIBLE_KEYED_HASH):
    buf = bytearray(max(HEADER_SIZE, header_size))
    buf[0:8] = b'LPKSHHRH'
    buf[12:16] = int(incompatible_flags).to_bytes(4, 'little')
    buf[88:96] = header_size.to_bytes(8, 'little')
    buf[208:216] = (11).to_bytes(8, 'little')
    buf[216:224] = (22).to_bytes(8, 'little')
    buf[224:232] = (33).to_bytes(8, 'little')
    buf[232:240] = (44).to_bytes(8, 'little')
    buf[240:248] = (55).to_bytes(8, 'little')
    buf[248:256] = (66).to_bytes(8, 'little')
    buf[256:260] = (77).to_bytes(4, 'little')
    buf[260:264] = (88).to_bytes(4, 'little')
    buf[264:272] = (99).to_bytes(8, 'little')
    if header_size < HEADER_SIZE:
        return bytes(buf[:header_size])
    return bytes(buf)


def test_reader_accepts_historical_unkeyed_lz4_header():
    fixture = _historical_header_fixture(240, INCOMPATIBLE_COMPRESSED_LZ4)
    header = parse_file_header(fixture)
    reader_module._ensure_supported_header(header)
    assert not (header['incompatible_flags'] & INCOMPATIBLE_KEYED_HASH)
    assert header['incompatible_flags'] & INCOMPATIBLE_COMPRESSED_LZ4
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'unkeyed-lz4.journal')
        with open(path, 'wb') as f:
            f.write(fixture)
        reader = FileReader.open(path)
        try:
            assert reader.header()['incompatible_flags'] == INCOMPATIBLE_COMPRESSED_LZ4
            assert not reader.step()
        finally:
            reader.close()


def test_writer_reader_and_binary_export():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'test.journal')
        writer = Writer.create(path)
        writer.append([
            {'name': 'MESSAGE', 'value': 'line1\nline2'},
            {'name': 'BINARY', 'value': bytes([0, 1, 0xff, 0xfe])},
        ])
        writer.close()

        reader = FileReader.open(path)
        assert reader.step()
        entry = reader.get_entry()
        assert entry['fields']['MESSAGE'] == b'line1\nline2'
        assert entry['fields']['BINARY'] == bytes([0, 1, 0xff, 0xfe])
        exported = export_entry(entry)
        assert b'BINARY\n\x04\x00\x00\x00\x00\x00\x00\x00\x00\x01\xff\xfe\n' in exported
        encoded = json_entry(entry)
        assert encoded['BINARY'] == [0, 1, 255, 254]
        reader.close()

        stock_rows = journalctl_file_rows_if_available(path)
        if stock_rows is not None:
            assert len(stock_rows) == 1


def test_writer_head_seqnum_zero_defaults_to_one():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'head-zero.journal')
        writer = Writer.create(path, {'head_seqnum': 0})
        writer.append([{'name': 'MESSAGE', 'value': 'head zero'}])
        writer.close()

        reader = FileReader.open(path)
        assert reader.step()
        assert reader.get_entry()['seqnum'] == 1
        reader.close()


def test_writer_raw_backward_monotonic_pass_through_fails_verification():
    from journal import VerificationError, verify_file

    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'raw-backward-monotonic.journal')
        writer = Writer.create(path)
        writer.append(
            [{'name': 'MESSAGE', 'value': 'raw monotonic first'}],
            {'realtime_usec': 1_700_003_000_000_000, 'monotonic_usec': 10},
        )
        writer.append(
            [{'name': 'MESSAGE', 'value': 'raw monotonic second'}],
            {'realtime_usec': 1_700_003_000_000_001, 'monotonic_usec': 5},
        )
        writer.close()

        try:
            verify_file(path)
        except VerificationError as err:
            assert 'monotonic' in str(err).lower()
        else:
            raise AssertionError('expected VerificationError for same-boot backward monotonic timestamps')
        verify_journal_file_fails_if_available(path, 'timestamp out of synchronization')


def test_writer_raw_explicit_zero_monotonic_pass_through():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'raw-zero-monotonic.journal')
        writer = Writer.create(path)
        writer.append(
            [{'name': 'MESSAGE', 'value': 'raw zero monotonic'}],
            {'realtime_usec': 1_700_003_000_100_000, 'monotonic_usec': 0},
        )
        writer.close()
        verify_journal_file_if_available(path)

        reader = FileReader.open(path)
        assert reader.step()
        entry = reader.get_entry()
        reader.close()
        assert entry['monotonic'] == 0


def test_compression_threshold_systemd_policy():
    from journal.writer import DEFAULT_COMPRESS_THRESHOLD, MIN_COMPRESS_THRESHOLD

    if not zstd_available():
        return

    cases = [
        {
            'name': 'default below threshold',
            'options': {},
            'payload_len': DEFAULT_COMPRESS_THRESHOLD - 1,
            'want_threshold': DEFAULT_COMPRESS_THRESHOLD,
            'want_compressed': False,
        },
        {
            'name': 'default exact threshold',
            'options': {},
            'payload_len': DEFAULT_COMPRESS_THRESHOLD,
            'want_threshold': DEFAULT_COMPRESS_THRESHOLD,
            'want_compressed': True,
        },
        {
            'name': 'minimum clamp',
            'options': {'compression_threshold_bytes': 1},
            'payload_len': MIN_COMPRESS_THRESHOLD - 1,
            'want_threshold': MIN_COMPRESS_THRESHOLD,
            'want_compressed': False,
        },
        {
            'name': 'minimum clamp eligible payload',
            'options': {'compression_threshold_bytes': 1},
            'payload_len': DEFAULT_COMPRESS_THRESHOLD,
            'want_threshold': MIN_COMPRESS_THRESHOLD,
            'want_compressed': True,
        },
    ]
    with tempfile.TemporaryDirectory() as td:
        for case in cases:
            path = os.path.join(td, case['name'].replace(' ', '-') + '.journal')
            writer = Writer.create(path, {'compression': 'zstd', **case['options']})
            assert writer._compress_threshold == case['want_threshold']
            writer.append([{'name': 'F', 'value': b'A' * (case['payload_len'] - 2)}])
            writer.close()
            assert journal_has_data_object_flag(path, OBJECT_COMPRESSED_ZSTD) is case['want_compressed']
            verify_journal_file_if_available(path)


def test_compact_writer_reader_and_stock_verify():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'compact.journal')
        writer = Writer.create(path, {'compact': True})
        writer.append([
            {'name': 'MESSAGE', 'value': 'compact entry'},
            {'name': 'BINARY', 'value': bytes([0, 1, 0xfe, 0xff])},
        ])
        writer.append([
            {'name': 'MESSAGE', 'value': 'second compact entry'},
            {'name': 'PRIORITY', 'value': '6'},
        ])
        writer.close()

        reader = FileReader.open(path)
        header = reader.header()
        assert header['incompatible_flags'] & INCOMPATIBLE_COMPACT
        assert reader.step()
        entry = reader.get_entry()
        assert entry['fields']['MESSAGE'] == b'compact entry'
        assert entry['fields']['BINARY'] == bytes([0, 1, 0xfe, 0xff])
        assert reader.step()
        assert reader.get_entry()['fields']['MESSAGE'] == b'second compact entry'
        reader.close()

        stock_rows = journalctl_file_rows_if_available(path)
        if stock_rows is not None:
            assert len(stock_rows) == 2
        verify_journal_file_if_available(path)


def test_compact_writer_grows_arena_past_initial_allocation():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'compact-grown.journal')
        writer = Writer.create(path, {'compact': True})
        for i in range(10):
            writer.append(
                [{'name': 'BLOB', 'value': bytes([i]) * (1024 * 1024)}],
                {'realtime_usec': 1_700_000_050_000_000 + i, 'monotonic_usec': i + 1},
            )
        writer.close()

        with open(path, 'rb') as f:
            header = parse_file_header(f.read(HEADER_SIZE))
        assert header['arena_size'] + HEADER_SIZE > FILE_SIZE_INCREASE
        verify_journal_file_if_available(path)


def test_writer_initial_arena_covers_large_hash_tables():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'large-hash-table.journal')
        writer = Writer.create(path, {
            'compact': True,
            'data_hash_table_buckets': 600_000,
            'field_hash_table_buckets': 1023,
        })
        writer.append(
            [{'name': 'MESSAGE', 'value': b'large hash table'}],
            {'realtime_usec': 1_700_000_060_000_000, 'monotonic_usec': 1},
        )
        writer.close()

        with open(path, 'rb') as f:
            header = parse_file_header(f.read(HEADER_SIZE))
        assert header['arena_size'] + HEADER_SIZE > FILE_SIZE_INCREASE
        verify_journal_file_if_available(path)


def test_writer_exclusive_lock():
    with tempfile.TemporaryDirectory() as td:
        unlocked_path = os.path.join(td, 'unlocked-default.journal')
        unlocked = Writer.create(unlocked_path)
        unlocked.close()
        assert not os.path.exists(unlocked_path + '.lock')

        path = os.path.join(td, 'test.journal')
        from journal.lock import WriterLock
        lock = WriterLock.acquire(path)
        writer = Writer.create(path)
        try:
            try:
                other = WriterLock.acquire(path)
            except BlockingIOError:
                pass
            else:
                other.release()
                raise AssertionError('expected second writer lock acquire to fail while first lock is held')
        finally:
            writer.close()
            lock.release()


def test_writer_lock_portable_owner_without_proc():
    from journal import lock as lock_module

    original_start_time = lock_module.process_start_time
    original_matches = lock_module.process_matches_start_time

    def fake_start_time(pid):
        if int(pid) == os.getpid():
            return 'portable-test-start'
        raise OSError('synthetic missing procfs')

    def fake_matches(pid, start_time):
        return int(pid) == os.getpid() and start_time == 'portable-test-start'

    lock_module.process_start_time = fake_start_time
    lock_module.process_matches_start_time = fake_matches
    try:
        with tempfile.TemporaryDirectory() as td:
            path = os.path.join(td, 'portable-lock.journal')
            lock = lock_module.WriterLock.acquire(path)
            try:
                try:
                    other = lock_module.WriterLock.acquire(path)
                except BlockingIOError:
                    pass
                else:
                    other.release()
                    raise AssertionError('expected portable lock owner to block a second writer')
            finally:
                lock.release()
            assert not os.path.exists(path + '.lock')
    finally:
        lock_module.process_start_time = original_start_time
        lock_module.process_matches_start_time = original_matches


def test_platform_positional_io_fallback_without_pread_pwrite():
    from journal import _platform_io as platform_module

    had_pread = hasattr(os, 'pread')
    had_pwrite = hasattr(os, 'pwrite')
    original_pread = getattr(os, 'pread', None)
    original_pwrite = getattr(os, 'pwrite', None)
    try:
        if had_pread:
            delattr(os, 'pread')
        if had_pwrite:
            delattr(os, 'pwrite')
        with tempfile.TemporaryDirectory() as td:
            path = os.path.join(td, 'positional.bin')
            fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
            try:
                os.write(fd, b'abcdefghij')
                os.lseek(fd, 5, os.SEEK_SET)
                platform_module.write_all_at(fd, b'XYZ', 2)
                assert os.lseek(fd, 0, os.SEEK_CUR) == 5
                assert platform_module.read_at(fd, 5, 0) == b'abXYZ'
                assert os.lseek(fd, 0, os.SEEK_CUR) == 5
            finally:
                os.close(fd)
    finally:
        if had_pread:
            os.pread = original_pread
        if had_pwrite:
            os.pwrite = original_pwrite


def test_platform_directory_sync_skips_windows_directory_handles():
    from journal import _platform_io as platform_module

    original_is_windows = platform_module._IS_WINDOWS
    platform_module._IS_WINDOWS = True
    try:
        with tempfile.TemporaryDirectory() as td:
            assert platform_module.sync_directory(td) is False
            assert platform_module.sync_parent_directory(os.path.join(td, 'x.journal')) is False
    finally:
        platform_module._IS_WINDOWS = original_is_windows


def test_writer_file_arena_fallback_without_mmap():
    from journal import writer as writer_module

    original_mapped_arena = writer_module._MappedArena

    class FailingMappedArena:
        def __init__(self, fd, size):
            raise OSError('synthetic mmap unavailable')

    writer_module._MappedArena = FailingMappedArena
    try:
        with tempfile.TemporaryDirectory() as td:
            path = os.path.join(td, 'file-arena.journal')
            writer = Writer.create(path)
            writer.append([
                {'name': 'MESSAGE', 'value': 'file arena fallback'},
                {'name': 'PRIORITY', 'value': '6'},
            ], {'realtime_usec': 1_700_002_500_000_000, 'monotonic_usec': 1})
            writer.close()

            reader = FileReader.open(path)
            try:
                assert reader.step()
                assert reader.get_entry()['fields']['MESSAGE'] == b'file arena fallback'
            finally:
                reader.close()
            verify_journal_file_if_available(path)
    finally:
        writer_module._MappedArena = original_mapped_arena


def test_writer_archive_closes_before_rename_when_required():
    from journal import writer as writer_module

    original_rename_requires_closed_file = writer_module.rename_requires_closed_file
    writer_module.rename_requires_closed_file = lambda: True
    try:
        with tempfile.TemporaryDirectory() as td:
            active = os.path.join(td, 'active.journal')
            archived = os.path.join(td, 'archived.journal')
            writer = Writer.create(active)
            writer.append([
                {'name': 'MESSAGE', 'value': 'closed rename archive'},
                {'name': 'PRIORITY', 'value': '6'},
            ], {'realtime_usec': 1_700_002_501_000_000, 'monotonic_usec': 1})
            writer.archive_to(archived)

            assert not os.path.exists(active)
            assert os.path.exists(archived)
            assert not os.path.exists(active + '.lock')

            with open(archived, 'rb') as f:
                header = parse_file_header(f.read(HEADER_SIZE))
            assert header['state'] == STATE_ARCHIVED
            verify_journal_file_if_available(archived)
    finally:
        writer_module.rename_requires_closed_file = original_rename_requires_closed_file


def test_zstd_data_object_parse():
    if not zstd_available():
        return
    from compression import zstd

    payload = b'MESSAGE=zstd-data-object'
    compressed = zstd.compress(payload)
    size = DATA_OBJECT_HEADER_SIZE + len(compressed)
    buf = bytearray(size)
    write_object_header(buf, 0, OBJECT_TYPE_DATA, OBJECT_COMPRESSED_ZSTD, size)
    buf[DATA_OBJECT_HEADER_SIZE:] = compressed
    parsed = parse_data_object(buf, 0)
    assert parsed == {'name': b'MESSAGE', 'value': b'zstd-data-object'}


def test_xz_and_lz4_data_object_parse():
    from journal.writer import _lz4_compress, _xz_compress

    payload = b'MESSAGE=' + (b'xz-lz4-data-object' * 8)
    for flag, compressed in (
        (OBJECT_COMPRESSED_XZ, _xz_compress(payload)),
        (OBJECT_COMPRESSED_LZ4, _lz4_compress(payload)),
    ):
        size = DATA_OBJECT_HEADER_SIZE + len(compressed)
        buf = bytearray(size)
        write_object_header(buf, 0, OBJECT_TYPE_DATA, flag, size)
        buf[DATA_OBJECT_HEADER_SIZE:] = compressed
        parsed = parse_data_object(buf, 0)
        assert parsed == {'name': b'MESSAGE', 'value': payload.split(b'=', 1)[1]}


def test_directory_writer_rotation():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'netdata-test',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 2,
            'max_files': 10,
        })
        for i in range(5):
            log.append([
                {'name': 'MESSAGE', 'value': f'dir {i}'},
                {'name': 'LIVE_SEQ', 'value': f'{i:06d}'},
                {'name': 'PRIORITY', 'value': '6'},
            ])
        assert Path(log.active_file()).name.startswith('netdata-test@')
        assert not os.path.exists(os.path.join(log.journal_directory(), 'netdata-test.journal'))
        journal_dir = log.journal_directory()
        log.close()

        names = sorted(os.listdir(journal_dir))
        assert len(names) == 3
        assert all(name.startswith('netdata-test@') and name.endswith('.journal') for name in names)
        assert not os.path.exists(os.path.join(journal_dir, 'netdata-test.journal'))

        reader = DirectoryReader.open(td)
        seq = []
        while reader.step():
            seq.append(reader.get_entry()['fields']['LIVE_SEQ'])
        priorities = reader.query_unique('PRIORITY')
        reader.close()
        assert seq == [f'{i:06d}'.encode() for i in range(5)]
        assert {bytes(v) for v in priorities} == {b'6'}

        stock_rows = journalctl_directory_rows_if_available(td)
        if stock_rows is not None:
            assert len(stock_rows) == 5


def test_writer_field_name_policies():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'journald.journal')
        writer = Writer.create(path)
        writer.append([
            {'name': 'MESSAGE', 'value': 'trusted fields'},
            {'name': '_HOSTNAME', 'value': 'synthetic-host'},
            {'name': '_TRANSPORT', 'value': 'journal'},
        ], {'realtime_usec': 1_700_002_111_000_000, 'monotonic_usec': 1})
        for invalid_name in ('lowercase', 'foo.bar', 'A' * 65, '1FIELD'):
            try:
                writer.append([
                    {'name': invalid_name, 'value': 'invalid'},
                ], {'realtime_usec': 1_700_002_111_000_001, 'monotonic_usec': 2})
            except ValueError as err:
                assert 'invalid field name' in str(err)
            else:
                raise AssertionError(f'expected invalid journald field {invalid_name!r} to fail')
        writer.close()
        verify_journal_file_if_available(path)
        reader = FileReader.open(path)
        assert reader.step()
        entry = reader.get_entry()
        reader.close()
        assert entry['fields']['_HOSTNAME'] == b'synthetic-host'
        assert entry['fields']['_TRANSPORT'] == b'journal'

    with tempfile.TemporaryDirectory() as td:
        for alias in ('systemd', 'app', 'journal_app'):
            try:
                Writer.create(os.path.join(td, f'{alias}.journal'), {'field_name_policy': alias})
            except ValueError as err:
                assert 'unsupported field name policy' in str(err)
            else:
                raise AssertionError(f'expected field policy alias {alias!r} to fail')

    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'journal-app.journal')
        writer = Writer.create(path, {'field_name_policy': FIELD_NAME_POLICY_JOURNAL_APP})
        writer.append([
            {'name': 'MESSAGE', 'value': 'app valid'},
            {'name': '_HOSTNAME', 'value': 'drop-host'},
            {'name': 'lowercase', 'value': 'drop-lowercase'},
        ], {'realtime_usec': 1_700_002_112_000_000, 'monotonic_usec': 1})
        try:
            writer.append([
                {'name': '_HOSTNAME', 'value': 'drop-only'},
            ], {'realtime_usec': 1_700_002_112_000_001, 'monotonic_usec': 2})
        except ValueError as err:
            assert 'empty entry' in str(err)
        else:
            raise AssertionError('expected drop-only journal-app append to fail')
        writer.close()
        verify_journal_file_if_available(path)
        reader = FileReader.open(path)
        assert reader.step()
        entry = reader.get_entry()
        reader.close()
        assert entry['fields']['MESSAGE'] == b'app valid'
        assert '_HOSTNAME' not in entry['fields']
        assert 'lowercase' not in entry['fields']

    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'raw.journal')
        long_name = 'a' * 1024
        writer = Writer.create(path, {'field_name_policy': FIELD_NAME_POLICY_RAW})
        writer.append([
            {'name': 'lowercase', 'value': 'ok'},
            {'name': 'foo.bar', 'value': 'dot'},
            {'name': 'field name', 'value': 'space'},
            {'name': long_name, 'value': 'long'},
            {'name': 'BINARY', 'value': b'a\x00=b'},
        ], {'realtime_usec': 1_700_002_113_000_000, 'monotonic_usec': 1})
        try:
            writer.append([
                {'name': 'BAD=NAME', 'value': 'bad'},
            ], {'realtime_usec': 1_700_002_113_000_001, 'monotonic_usec': 2})
        except ValueError as err:
            assert 'invalid field name' in str(err)
        else:
            raise AssertionError('expected raw field name containing = to fail')
        writer.close()
        reader = FileReader.open(path)
        assert reader.step()
        entry = reader.get_entry()
        reader.close()
        assert entry['fields']['lowercase'] == b'ok'
        assert entry['fields']['foo.bar'] == b'dot'
        assert entry['fields']['field name'] == b'space'
        assert entry['fields'][long_name] == b'long'
        assert entry['fields']['BINARY'] == b'a\x00=b'


def test_writer_append_raw_policies_and_binary_payloads():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'raw-direct.journal')
        writer = Writer.create(path)
        writer.append_raw([
            b'MESSAGE=raw direct',
            b'PRIORITY=6',
            b'BINARY=a\x00=b',
        ], {'realtime_usec': 1_700_002_114_000_000, 'monotonic_usec': 1})
        for payload in (b'MALFORMED', b'=bad'):
            try:
                writer.append_raw([payload], {
                    'realtime_usec': 1_700_002_114_000_001,
                    'monotonic_usec': 2,
                })
            except ValueError:
                pass
            else:
                raise AssertionError(f'expected malformed raw payload {payload!r} to fail')
        writer.close()
        verify_journal_file_if_available(path)
        reader = FileReader.open(path)
        assert reader.step()
        entry = reader.get_entry()
        reader.close()
        assert entry['fields']['MESSAGE'] == b'raw direct'
        assert entry['fields']['BINARY'] == b'a\x00=b'

    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'raw-app.journal')
        writer = Writer.create(path, {'field_name_policy': FIELD_NAME_POLICY_JOURNAL_APP})
        writer.append_raw([
            b'MESSAGE=raw app',
            b'_HOSTNAME=drop-host',
            b'lowercase=drop-lowercase',
        ], {'realtime_usec': 1_700_002_115_000_000, 'monotonic_usec': 1})
        try:
            writer.append_raw([b'_HOSTNAME=drop-only'], {
                'realtime_usec': 1_700_002_115_000_001,
                'monotonic_usec': 2,
            })
        except ValueError as err:
            assert 'empty entry' in str(err)
        else:
            raise AssertionError('expected journal-app raw drop-only append to fail')
        writer.close()
        reader = FileReader.open(path)
        assert reader.step()
        entry = reader.get_entry()
        reader.close()
        assert entry['fields']['MESSAGE'] == b'raw app'
        assert '_HOSTNAME' not in entry['fields']
        assert 'lowercase' not in entry['fields']

    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'raw-policy.journal')
        writer = Writer.create(path, {'field_name_policy': FIELD_NAME_POLICY_RAW})
        writer.append_raw([
            b'lowercase=ok',
            b'field name=space',
            b'BINARY=a\x00=b',
        ], {'realtime_usec': 1_700_002_116_000_000, 'monotonic_usec': 1})
        writer.close()
        reader = FileReader.open(path)
        assert reader.step()
        entry = reader.get_entry()
        reader.close()
        assert entry['fields']['lowercase'] == b'ok'
        assert entry['fields']['field name'] == b'space'
        assert entry['fields']['BINARY'] == b'a\x00=b'


def test_writer_append_raw_matches_structured_bytes():
    opts = {
        'file_id': bytes.fromhex('41000000000000000000000000000000'),
        'machine_id': bytes.fromhex('11000000000000000000000000000000'),
        'boot_id': bytes.fromhex('21000000000000000000000000000000'),
        'seqnum_id': bytes.fromhex('31000000000000000000000000000000'),
        'data_hash_table_buckets': 64,
        'field_hash_table_buckets': 16,
        'live_publish_every_entries': 1,
    }
    entry_opts = {
        'realtime_usec': 1_700_002_117_000_000,
        'monotonic_usec': 5,
    }

    def write_file(directory, name, raw):
        path = os.path.join(directory, f'{name}.journal')
        writer = Writer.create(path, opts)
        if raw:
            writer.append_raw([
                b'MESSAGE=equivalent entry',
                b'PRIORITY=6',
                b'BINARY=a\x00=b=c',
            ], entry_opts)
        else:
            writer.append([
                {'name': 'MESSAGE', 'value': 'equivalent entry'},
                {'name': 'PRIORITY', 'value': '6'},
                {'name': 'BINARY', 'value': b'a\x00=b=c'},
            ], entry_opts)
        writer.close()
        return Path(path).read_bytes()

    with tempfile.TemporaryDirectory() as td:
        structured = write_file(td, 'structured', False)
        raw = write_file(td, 'raw', True)

    assert structured == raw


def test_directory_writer_journald_policy_preserves_protected_fields():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
        })
        log.append([
            {'name': 'MESSAGE', 'value': 'journald policy preserves trusted fields'},
            {'name': 'TEST_ID', 'value': 'journald-field-policy'},
            {'name': '_HOSTNAME', 'value': 'synthetic-host'},
            {'name': '_TRANSPORT', 'value': 'snmptrap'},
        ], {'realtime_usec': 1_700_002_401_000_000, 'monotonic_usec': 10})
        log.sync()

        reader = FileReader.open(log.active_file_path())
        entries = []
        try:
            while reader.step():
                entries.append(reader.get_entry())
        finally:
            reader.close()
        assert len(entries) == 1
        assert entries[0]['fields']['_HOSTNAME'] == b'synthetic-host'
        assert entries[0]['fields']['_TRANSPORT'] == b'snmptrap'

        stock_rows = journalctl_directory_rows_if_available(
            log.journal_directory(),
            'TEST_ID=journald-field-policy',
        )
        if stock_rows is not None:
            assert len(stock_rows) == 1
            assert stock_rows[0]['_HOSTNAME'] == 'synthetic-host'
            assert stock_rows[0]['_TRANSPORT'] == 'snmptrap'
        log.close()
        for name in os.listdir(log.journal_directory()):
            if name.endswith('.journal'):
                verify_journal_file_if_available(os.path.join(log.journal_directory(), name))


def test_directory_writer_journal_app_policy_drops_invalid_fields():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'field_name_policy': FIELD_NAME_POLICY_JOURNAL_APP,
        })
        log.append([
            {'name': 'MESSAGE', 'value': 'journal app keeps valid fields'},
            {'name': 'TEST_ID', 'value': 'journal-app-field-policy'},
            {'name': '_HOSTNAME', 'value': 'dropped-host'},
            {'name': 'foo.bar', 'value': 'dropped-dot'},
        ], {
            'realtime_usec': 1_700_002_402_000_000,
            'monotonic_usec': 20,
        })
        try:
            log.append([
                {'name': '_HOSTNAME', 'value': 'drop-only'},
            ], {
                'realtime_usec': 1_700_002_402_000_001,
                'monotonic_usec': 21,
            })
        except ValueError as err:
            assert 'empty entry' in str(err)
        else:
            raise AssertionError('expected drop-only journal-app append to fail')
        journal_dir = log.journal_directory()
        log.close()

        names = sorted(name for name in os.listdir(journal_dir) if name.endswith('.journal'))
        assert len(names) == 1
        path = os.path.join(journal_dir, names[0])
        verify_journal_file_if_available(path)
        reader = FileReader.open(path)
        entries = []
        try:
            while reader.step():
                entries.append(reader.get_entry())
        finally:
            reader.close()
        assert len(entries) == 1
        assert entries[0]['fields']['MESSAGE'] == b'journal app keeps valid fields'
        assert '_HOSTNAME' not in entries[0]['fields']
        assert 'foo.bar' not in entries[0]['fields']


def test_directory_writer_append_raw_injects_metadata_and_filters_callers():
    with tempfile.TemporaryDirectory() as td:
        boot_id = '0123456789abcdef0123456789abcdef'
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'boot_id': boot_id,
            'field_name_policy': FIELD_NAME_POLICY_JOURNAL_APP,
        })
        log.append_raw([
            b'MESSAGE=raw directory',
            b'TEST_ID=python-log-append-raw',
            b'_HOSTNAME=drop-host',
            b'lowercase=drop-lowercase',
        ], {
            'realtime_usec': 1_700_002_404_000_000,
            'monotonic_usec': 40,
            'source_realtime_usec': 1234,
        })
        journal_dir = log.journal_directory()
        log.close()

        names = sorted(name for name in os.listdir(journal_dir) if name.endswith('.journal'))
        assert len(names) == 1
        path = os.path.join(journal_dir, names[0])
        verify_journal_file_if_available(path)
        reader = FileReader.open(path)
        assert reader.step()
        entry = reader.get_entry()
        reader.close()
        assert entry['fields']['MESSAGE'] == b'raw directory'
        assert entry['fields']['TEST_ID'] == b'python-log-append-raw'
        assert entry['fields']['_BOOT_ID'] == boot_id.encode('ascii')
        assert entry['fields']['_SOURCE_REALTIME_TIMESTAMP'] == b'1234'
        assert '_HOSTNAME' not in entry['fields']
        assert 'lowercase' not in entry['fields']

        rows = journalctl_directory_rows_if_available(
            journal_dir,
            f'_BOOT_ID={boot_id}',
            'TEST_ID=python-log-append-raw',
        )
        if rows is not None:
            assert len(rows) == 1
            assert rows[0]['MESSAGE'] == 'raw directory'


def test_directory_writer_raw_policy_allows_structure_only_field_names():
    with tempfile.TemporaryDirectory() as td:
        long_name = 'a' * 1024
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'field_name_policy': FIELD_NAME_POLICY_RAW,
        })
        log.append([
            {'name': 'lowercase', 'value': 'ok'},
            {'name': 'foo.bar', 'value': 'dot'},
            {'name': 'field name', 'value': 'space'},
            {'name': long_name, 'value': 'long'},
            {'name': 'BINARY', 'value': b'a\x00=b'},
        ], {
            'realtime_usec': 1_700_002_403_000_000,
            'monotonic_usec': 30,
        })
        try:
            log.append([
                {'name': 'BAD=NAME', 'value': 'bad'},
            ], {
                'realtime_usec': 1_700_002_403_000_001,
                'monotonic_usec': 31,
            })
        except ValueError as err:
            assert 'invalid field name' in str(err)
        else:
            raise AssertionError('expected raw field name containing = to fail')
        journal_dir = log.journal_directory()
        log.close()

        names = sorted(name for name in os.listdir(journal_dir) if name.endswith('.journal'))
        assert len(names) == 1
        path = os.path.join(journal_dir, names[0])
        reader = FileReader.open(path)
        entries = []
        try:
            while reader.step():
                entries.append(reader.get_entry())
        finally:
            reader.close()
        assert len(entries) == 1
        assert entries[0]['fields']['lowercase'] == b'ok'
        assert entries[0]['fields']['foo.bar'] == b'dot'
        assert entries[0]['fields']['field name'] == b'space'
        assert entries[0]['fields'][long_name] == b'long'
        assert entries[0]['fields']['BINARY'] == b'a\x00=b'


def test_directory_writer_duration_rotation():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 0,
            'max_bytes': 0,
            'max_duration_usec': 10_000_000,
            'max_files': 10,
        })
        base = 1_700_002_090_000_000
        for i, realtime in enumerate((base, base + 9_999_999, base + 10_000_000)):
            log.append(
                [{'name': 'MESSAGE', 'value': f'duration-rotation-{i}'}],
                {'realtime_usec': realtime, 'monotonic_usec': i + 1},
            )
        journal_dir = log.journal_directory()
        log.close()

        names = sorted(name for name in os.listdir(journal_dir) if name.endswith('.journal'))
        assert len(names) == 2
        counts = []
        for name in names:
            reader = FileReader.open(os.path.join(journal_dir, name))
            counts.append(reader.header()['n_entries'])
            reader.close()
        assert counts == [2, 1]


def test_directory_writer_derives_rotation_defaults_from_retention():
    with tempfile.TemporaryDirectory() as td:
        max_size = 128 * 1024 * 1024
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'retention_policy': {'max_bytes': max_size * 20, 'max_age_usec': 20_000_001},
        })
        assert log._max_bytes == max_size
        assert log._max_duration_usec == 1_000_001
        log.append(
            [{'name': 'MESSAGE', 'value': 'derived rotation defaults'}],
            {'realtime_usec': 1_700_002_091_000_000, 'monotonic_usec': 1},
        )
        journal_dir = log.journal_directory()
        log.close()
        names = sorted(name for name in os.listdir(journal_dir) if name.endswith('.journal'))
        assert len(names) == 1
        reader = FileReader.open(os.path.join(journal_dir, names[0]))
        try:
            header = reader.header()
            assert header['data_hash_table_size'] // 16 == data_hash_buckets_for_max_file_size(max_size)
            assert header['field_hash_table_size'] // 16 == DEFAULT_FIELD_HASH_BUCKETS
        finally:
            reader.close()


def test_directory_writer_derived_size_rotates_from_retention():
    with tempfile.TemporaryDirectory() as td:
        max_size = 16 * 1024 * 1024
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'retention_policy': {'max_bytes': max_size * 20},
        })
        assert log._max_bytes == max_size
        for i in range(12):
            log.append([
                {'name': 'MESSAGE', 'value': f'derived-size-rotation-{i}'},
                {'name': 'PAYLOAD', 'value': f'{i:05d}-' + ('x' * (2 * 1024 * 1024))},
                {'name': 'TEST_ID', 'value': 'derived-size-rotation'},
            ], {
                'realtime_usec': 1_700_002_092_000_000 + i,
                'monotonic_usec': i + 1,
            })
        journal_dir = log.journal_directory()
        log.close()

        names = sorted(name for name in os.listdir(journal_dir) if name.endswith('.journal'))
        assert len(names) >= 2
        entries = 0
        for name in names:
            reader = FileReader.open(os.path.join(journal_dir, name))
            try:
                header = reader.header()
                assert header['data_hash_table_size'] // 16 == data_hash_buckets_for_max_file_size(max_size)
                entries += header['n_entries']
            finally:
                reader.close()
        assert entries == 12


def test_directory_writer_derived_duration_rotates_from_retention():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'retention_policy': {'max_age_usec': 20_000_001},
        })
        base = int(time.time() * 1_000_000)
        for i, realtime in enumerate((base, base + 1_000_000, base + 1_000_001)):
            log.append([
                {'name': 'MESSAGE', 'value': f'derived-duration-rotation-{i}'},
                {'name': 'TEST_ID', 'value': 'derived-duration-rotation'},
            ], {
                'realtime_usec': realtime,
                'monotonic_usec': i + 1,
            })
        journal_dir = log.journal_directory()
        log.close()

        names = sorted(name for name in os.listdir(journal_dir) if name.endswith('.journal'))
        assert len(names) == 2
        counts = []
        for name in names:
            reader = FileReader.open(os.path.join(journal_dir, name))
            try:
                counts.append(reader.header()['n_entries'])
            finally:
                reader.close()
        assert counts == [2, 1]


def test_directory_writer_derived_rotation_small_retention_clamps_to_minimum():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'retention_policy': {'max_bytes': 1_000_000},
        })
        assert log._max_bytes == 512 * 1024
        log.close()


def test_directory_writer_derived_rotation_compact_max_file_size_clamp():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'compact': True,
            'machine_id': '00112233445566778899aabbccddeeff',
            'retention_policy': {'max_bytes': (JOURNAL_COMPACT_SIZE_MAX + 4096) * 20},
        })
        assert log._max_bytes == JOURNAL_COMPACT_SIZE_MAX
        log.close()


def test_directory_writer_explicit_rotation_overrides_retention_defaults():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'rotation_policy': {'max_bytes': 64 * 1024 * 1024, 'max_duration_usec': 2_000_000},
            'retention_policy': {'max_bytes': 128 * 1024 * 1024 * 20, 'max_age_usec': 20_000_000},
        })
        assert log._max_bytes == 64 * 1024 * 1024
        assert log._max_duration_usec == 2_000_000
        log.close()


def test_directory_writer_default_system_chain_naming():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 0,
            'max_bytes': 0,
            'max_files': 10,
        })
        log.append([
            {'name': 'MESSAGE', 'value': 'default system naming'},
        ])
        assert log._next_seqnum == 2
        assert Path(log.active_file()).name.startswith('system@')
        assert not os.path.exists(os.path.join(log.journal_directory(), 'system.journal'))
        log.close()


def test_directory_writer_open_identity_lifecycle_source_timestamp():
    with tempfile.TemporaryDirectory() as td:
        machine_id = bytes.fromhex('00112233445566778899aabbccddeeff')
        boot_id = bytes.fromhex('ffeeddccbbaa99887766554433221100')
        try:
            Log(td, {
                'identity_mode': 'strict',
                'machine_id': machine_id,
            })
        except ValueError as err:
            assert 'strict identity requires boot id' in str(err)
        else:
            raise AssertionError('expected strict identity rejection without boot id')

        events = []

        def record_lifecycle(event):
            events.append(event)

        log = Log(td, {
            'source': 'system',
            'open_mode': 'eager',
            'identity_mode': 'strict',
            'machine_id': machine_id,
            'boot_id': boot_id,
            'lifecycle': record_lifecycle,
        })
        assert log.configured_directory() == td
        assert log.journal_directory() == os.path.join(td, '00112233445566778899aabbccddeeff')
        assert log.machine_id() == machine_id
        assert log.boot_id() == boot_id
        assert log.source_name() == 'system'
        assert log.active_file_path() != ''
        assert len(events) == 1
        assert events[0]['type'] == 'created'
        assert events[0]['reason'] == 'eager_open'
        assert events[0]['active_path'] == log.active_file_path()

        log.append(
            [{'name': 'MESSAGE', 'value': 'timestamp-0'}],
            {'realtime_usec': 1_700_000_100_000_000, 'monotonic_usec': 10, 'source_realtime_usec': 999},
        )
        log.append(
            [{'name': 'MESSAGE', 'value': 'timestamp-1'}],
            {'realtime_usec': 1_700_000_100_000_000, 'monotonic_usec': 10, 'source_realtime_usec': 1000},
        )
        log.append(
            [{'name': 'MESSAGE', 'value': 'timestamp-2'}],
            {'realtime_usec': 1_700_000_100_000_000, 'monotonic_usec': 0, 'source_realtime_usec': 1001},
        )
        log.append(
            [{'name': 'MESSAGE', 'value': 'timestamp-3'}],
            {'realtime_usec': 0, 'monotonic_usec': 13, 'source_realtime_usec': 1002},
        )
        path = log.active_file_path()
        log.close()
        verify_journal_file_if_available(path)

        reader = FileReader.open(path)
        entries = []
        while reader.step():
            entries.append(reader.get_entry())
        reader.close()
        assert len(entries) == 4
        assert [entry['realtime'] for entry in entries] == [
            1_700_000_100_000_000,
            1_700_000_100_000_001,
            1_700_000_100_000_002,
            1_700_000_100_000_003,
        ]
        assert [entry['monotonic'] for entry in entries] == [10, 11, 12, 13]
        assert [
            entry['fields']['_SOURCE_REALTIME_TIMESTAMP'].decode()
            for entry in entries
        ] == ['999', '1000', '1001', '1002']
        assert [
            entry['fields']['_BOOT_ID'].decode()
            for entry in entries
        ] == [boot_id.hex()] * 4


def test_directory_writer_different_boot_does_not_seed_monotonic_clamp_from_previous_tail():
    with tempfile.TemporaryDirectory() as td:
        machine_id = bytes.fromhex('00112233445566778899aabbccddeeff')
        boot_a = bytes.fromhex('aa000000000000000000000000000001')
        boot_b = bytes.fromhex('bb000000000000000000000000000002')

        first = Log(td, {
            'source': 'system',
            'identity_mode': 'strict',
            'machine_id': machine_id,
            'boot_id': boot_a,
        })
        first.append(
            [{'name': 'MESSAGE', 'value': 'cross boot first'}, {'name': 'TEST_ID', 'value': 'cross-boot-monotonic'}],
            {'realtime_usec': 1_700_003_100_000_000, 'monotonic_usec': 100},
        )
        first.close()

        second = Log(td, {
            'source': 'system',
            'identity_mode': 'strict',
            'machine_id': machine_id,
            'boot_id': boot_b,
        })
        second.append(
            [{'name': 'MESSAGE', 'value': 'cross boot second'}, {'name': 'TEST_ID', 'value': 'cross-boot-monotonic'}],
            {'realtime_usec': 1_700_003_100_000_001, 'monotonic_usec': 1},
        )
        second.close()

        entries = []
        for path in journal_files(os.path.join(td, machine_id.hex())):
            verify_journal_file_if_available(path)
            reader = FileReader.open(path)
            while reader.step():
                entry = reader.get_entry()
                if entry['fields'].get('TEST_ID') == b'cross-boot-monotonic':
                    entries.append(entry)
            reader.close()
        entries.sort(key=lambda entry: entry['realtime'])
        assert len(entries) == 2
        assert [entry['monotonic'] for entry in entries] == [100, 1]
        assert [
            entry['boot_id'].hex()
            for entry in entries
        ] == [boot_a.hex(), boot_b.hex()]


def test_directory_writer_explicit_policy_validation():
    with tempfile.TemporaryDirectory() as td:
        try:
            Log(td, {'rotation_policy': {'max_entries': 0}})
        except ValueError as err:
            assert 'rotation max entries' in str(err)
        else:
            raise AssertionError('expected rotation policy validation failure')
        try:
            Log(td, {'retention_policy': {'max_files': 0}})
        except ValueError as err:
            assert 'retention max files' in str(err)
        else:
            raise AssertionError('expected retention policy validation failure')


def test_directory_writer_lifecycle_delete_and_artifact_size():
    with tempfile.TemporaryDirectory() as td:
        events = []
        artifact_calls = []

        def artifact_sizer(path):
            artifact_calls.append(path)
            return 4096

        def record_lifecycle(event):
            events.append(event)

        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 1,
            'retention_policy': {'max_bytes': 1},
            'lifecycle': record_lifecycle,
            'artifact_sizer': artifact_sizer,
        })
        log.append([{'name': 'MESSAGE', 'value': 'artifact-retention-0'}])
        log.append([{'name': 'MESSAGE', 'value': 'artifact-retention-1'}])
        assert artifact_calls
        assert any(event['type'] == 'created' and event['reason'] == 'append' for event in events)
        assert any(event['type'] == 'rotated' for event in events)
        deleted = next(event for event in events if event['type'] == 'deleted')
        assert len(deleted['deleted_paths']) == 1
        names = [name for name in os.listdir(log.journal_directory()) if name.endswith('.journal')]
        assert len(names) == 1
        log.close()


def test_directory_writer_rejects_empty_entry_without_creating_file():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 0,
            'max_bytes': 0,
            'max_files': 10,
        })
        try:
            log.append([])
        except ValueError as err:
            assert 'empty entry' in str(err)
        else:
            raise AssertionError('expected empty entry rejection')
        names = [name for name in os.listdir(log.journal_directory()) if name.endswith('.journal')]
        assert names == []
        log.close()


def test_directory_writer_custom_source_naming():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'custom-source',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 0,
            'max_bytes': 0,
            'max_files': 10,
        })
        log.append([{'name': 'MESSAGE', 'value': 'custom default source'}])
        assert Path(log.active_file()).name.startswith('custom-source@')
        assert not os.path.exists(os.path.join(log.journal_directory(), 'custom-source.journal'))
        log.close()

    with tempfile.TemporaryDirectory() as td:
        strict = Log(td, {
            'source': 'custom-source',
            'machine_id': '00112233445566778899aabbccddeeff',
            'strict_systemd_naming': True,
            'max_entries': 100,
            'max_files': 10,
        })
        strict.append([{'name': 'MESSAGE', 'value': 'custom strict source'}])
        assert Path(strict.active_file()).name == 'custom-source.journal'
        journal_dir = strict.journal_directory()
        strict.close()
        names = sorted(name for name in os.listdir(journal_dir) if name.endswith('.journal'))
        assert len(names) == 1
        assert names[0].startswith('custom-source@')


def test_directory_writer_strict_systemd_naming():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'strict_systemd_naming': True,
            'max_entries': 100,
            'max_files': 10,
        })
        log.append([
            {'name': 'MESSAGE', 'value': 'strict naming'},
            {'name': 'LIVE_SEQ', 'value': '000001'},
        ])
        assert Path(log.active_file()).name == 'system.journal'
        journal_dir = log.journal_directory()
        log.close()

        names = sorted(name for name in os.listdir(journal_dir) if name.endswith('.journal'))
        assert len(names) == 1
        assert names[0].startswith('system@')


def test_directory_writer_lazy_retention_runs_on_first_open():
    with tempfile.TemporaryDirectory() as td:
        config = {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 1,
            'max_files': 0,
        }
        first = Log(td, config)
        for i in range(2):
            first.append([{'name': 'MESSAGE', 'value': f'construction-retention-{i}'}])
        first.close()
        journal_dir = first.journal_directory()
        before = journal_files(journal_dir)
        assert len(before) == 2

        events = []

        def record_lifecycle(event):
            events.append(event)

        second = Log(td, {
            **config,
            'max_files': 1,
            'lifecycle': record_lifecycle,
        })
        after = journal_files(journal_dir)
        assert after == before
        second.append([
            {'name': 'MESSAGE', 'value': 'construction-retention-open'},
            {'name': 'TEST_ID', 'value': 'python-retention-on-open'},
        ])
        assert journal_files(journal_dir) == [second.active_file()]
        assert any(event['type'] == 'deleted' for event in events)
        verify_journal_file_if_available(second.active_file())
        rows = journalctl_directory_rows_if_available(
            journal_dir,
            'TEST_ID=python-retention-on-open',
        )
        if rows is not None:
            assert len(rows) == 1
        second.close()


def test_directory_writer_eager_retention_runs_on_open_for_all_policies():
    for name, retention_options, use_artifacts in (
        ('files', {'max_files': 1}, False),
        ('bytes', {'max_retention_bytes': 1}, True),
        ('age', {'max_retention_age_usec': 1}, False),
    ):
        with tempfile.TemporaryDirectory() as td:
            config = {
                'source': 'system',
                'machine_id': '00112233445566778899aabbccddeeff',
                'max_entries': 1,
                'max_files': 0,
            }
            first = Log(td, config)
            for i in range(3):
                first.append(
                    [{'name': 'MESSAGE', 'value': f'open-retention-{name}-{i}'}],
                    {
                        'realtime_usec': 1_700_002_276_000_000 + i,
                        'monotonic_usec': i + 1,
                    },
                )
            first.close()
            journal_dir = first.journal_directory()
            assert len(journal_files(journal_dir)) == 3

            events = []
            artifact_calls = []

            def artifact_sizer(path):
                artifact_calls.append(path)
                return 4096

            def record_lifecycle(event):
                events.append(event)

            retained = Log(td, {
                **config,
                'max_entries': 0,
                'open_mode': 'eager',
                **retention_options,
                'lifecycle': record_lifecycle,
                'artifact_sizer': artifact_sizer if use_artifacts else None,
            })
            assert journal_files(journal_dir) == [retained.active_file()]
            assert any(
                event['type'] == 'created' and event['reason'] == 'eager_open'
                for event in events
            )
            assert any(event['type'] == 'deleted' for event in events)
            if use_artifacts:
                assert artifact_calls
            verify_journal_file_if_available(retained.active_file())
            retained.close()


def test_directory_writer_enforce_retention_deletes_files_by_age_without_append():
    with tempfile.TemporaryDirectory() as td:
        config = {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 1,
            'max_files': 0,
        }
        first = Log(td, config)
        for i in range(3):
            first.append(
                [{'name': 'MESSAGE', 'value': f'age-retention-{i}'}],
                {'realtime_usec': 1_000_000 + i, 'monotonic_usec': i + 1},
            )
        first.close()
        journal_dir = first.journal_directory()
        assert len([name for name in os.listdir(journal_dir) if name.endswith('.journal')]) == 3

        retained = Log(td, {**config, 'max_entries': 0, 'max_retention_age_usec': 1_000_000})
        assert len([name for name in os.listdir(journal_dir) if name.endswith('.journal')]) == 3
        retained.enforce_retention()
        assert [name for name in os.listdir(journal_dir) if name.endswith('.journal')] == []
        retained.close()


def test_directory_writer_enforce_retention_protects_active_file_by_age():
    with tempfile.TemporaryDirectory() as td:
        config = {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 1,
            'max_files': 0,
        }
        first = Log(td, config)
        for i in range(2):
            first.append(
                [{'name': 'MESSAGE', 'value': f'age-active-retention-{i}'}],
                {'realtime_usec': 1_000_000 + i, 'monotonic_usec': i + 1},
            )
        first.close()
        journal_dir = first.journal_directory()
        assert len([name for name in os.listdir(journal_dir) if name.endswith('.journal')]) == 2

        retained = Log(td, {**config, 'max_entries': 0, 'max_retention_age_usec': 1_000_000})
        retained.append(
            [{'name': 'MESSAGE', 'value': 'age-protected-active'}],
            {'realtime_usec': 1_000_100, 'monotonic_usec': 10},
        )
        active_path = retained.active_file()
        retained.enforce_retention()
        paths = sorted(
            os.path.join(journal_dir, name)
            for name in os.listdir(journal_dir)
            if name.endswith('.journal')
        )
        assert paths == [active_path]
        reader = FileReader.open(active_path)
        assert reader.step() is True
        assert reader.get_entry()['fields']['MESSAGE'] == b'age-protected-active'
        reader.close()
        retained.close()


def test_directory_writer_keeps_chain_named_active_during_retention():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 1,
            'max_files': 1,
            'max_retention_bytes': 1024 * 1024 * 1024,
        })
        for i in range(3):
            log.append([
                {'name': 'MESSAGE', 'value': f'retention-active-{i}'},
            ])
        names = sorted(name for name in os.listdir(log.journal_directory()) if name.endswith('.journal'))
        assert len(names) == 1
        log.close()


def test_directory_writer_strict_close_protects_current_archive_from_byte_retention():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'strict_systemd_naming': True,
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 100,
            'max_files': 10,
            'max_retention_bytes': 1,
        })
        log.append([
            {'name': 'MESSAGE', 'value': 'strict byte retained'},
            {'name': 'TEST_ID', 'value': 'python-strict-byte-retention'},
        ])
        log.close()
        names = sorted(name for name in os.listdir(log.journal_directory()) if name.endswith('.journal'))
        assert len(names) == 1
        reader = FileReader.open(os.path.join(log.journal_directory(), names[0]))
        assert reader.step() is True
        assert reader.get_entry()['fields']['MESSAGE'] == b'strict byte retained'
        reader.close()


def test_directory_writer_close_cleans_up_after_archive_error():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 100,
            'max_files': 10,
        })
        log.append([{'name': 'MESSAGE', 'value': 'archive failure cleanup'}])
        original_archive_to = log._active_writer.archive_to

        def failing_archive_to(path):
            original_archive_to(path)
            raise RuntimeError('synthetic post-archive failure')

        log._active_writer.archive_to = failing_archive_to
        try:
            log.close()
        except RuntimeError as err:
            assert 'synthetic post-archive failure' in str(err)
        else:
            raise AssertionError('expected synthetic archive failure')

        assert log._closed is True
        assert log._active_writer is None
        log.close()


def test_directory_writer_rotation_cleans_up_after_archive_error():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 1,
            'max_files': 10,
        })
        log.append([{'name': 'MESSAGE', 'value': 'rotation failure first'}])
        original_archive_to = log._active_writer.archive_to

        def failing_archive_to(path):
            original_archive_to(path)
            raise RuntimeError('synthetic post-rotation failure')

        log._active_writer.archive_to = failing_archive_to
        try:
            log.append([{'name': 'MESSAGE', 'value': 'rotation failure second'}])
        except RuntimeError as err:
            assert 'synthetic post-rotation failure' in str(err)
        else:
            raise AssertionError('expected synthetic rotation failure')

        assert log._closed is False
        assert log._active_writer is None
        log.append([{'name': 'MESSAGE', 'value': 'rotation failure second'}])
        log.close()

        seqnums = []
        for name in sorted(name for name in os.listdir(log.journal_directory()) if name.endswith('.journal')):
            reader = FileReader.open(os.path.join(log.journal_directory(), name))
            while reader.step():
                seqnums.append(reader.get_entry()['seqnum'])
            reader.close()
        assert seqnums == [1, 2]


def test_directory_writer_strict_reopen_continues_sequence():
    with tempfile.TemporaryDirectory() as td:
        config = {
            'source': 'system',
            'strict_systemd_naming': True,
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 100,
            'max_files': 10,
        }
        first = Log(td, config)
        first.append([{'name': 'MESSAGE', 'value': 'strict-reopen-0'}])
        first.close()
        second = Log(td, config)
        second.append([{'name': 'MESSAGE', 'value': 'strict-reopen-1'}])
        second.close()
        seqnums = []
        for name in sorted(name for name in os.listdir(second.journal_directory()) if name.endswith('.journal')):
            reader = FileReader.open(os.path.join(second.journal_directory(), name))
            while reader.step():
                seqnums.append(reader.get_entry()['seqnum'])
            reader.close()
        assert seqnums == [1, 2]


def test_directory_writer_chain_reopen_continues_sequence():
    with tempfile.TemporaryDirectory() as td:
        config = {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 0,
            'max_bytes': 0,
            'max_files': 10,
        }
        first = Log(td, config)
        first.append([{'name': 'MESSAGE', 'value': 'chain-reopen-0'}])
        first.append([{'name': 'MESSAGE', 'value': 'chain-reopen-1'}])
        first.close()

        second = Log(td, config)
        second.append([{'name': 'MESSAGE', 'value': 'chain-reopen-2'}])
        second.close()

        seqnums = []
        for name in sorted(name for name in os.listdir(second.journal_directory()) if name.endswith('.journal')):
            reader = FileReader.open(os.path.join(second.journal_directory(), name))
            while reader.step():
                seqnums.append(reader.get_entry()['seqnum'])
            reader.close()
        assert seqnums == [1, 2, 3]


def test_directory_writer_chain_reopens_online_file():
    with tempfile.TemporaryDirectory() as td:
        config = {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 0,
            'max_bytes': 0,
            'max_files': 10,
        }
        first = Log(td, config)
        first.append([{'name': 'MESSAGE', 'value': 'chain-online-reopen-0'}])
        first.append([{'name': 'MESSAGE', 'value': 'chain-online-reopen-1'}])
        active_path = first.active_file()
        first._active_writer.close()
        first._active_writer = None
        first._closed = True

        second = Log(td, {**config, 'head_seqnum': 99})
        assert second.active_file() == active_path
        assert second._active_writer is not None
        assert second._next_seqnum == 3
        second.append([{'name': 'MESSAGE', 'value': 'chain-online-reopen-2'}])
        second.close()

        reader = FileReader.open(active_path)
        seqnums = []
        while reader.step():
            seqnums.append(reader.get_entry()['seqnum'])
        reader.close()
        assert seqnums == [1, 2, 3]


def test_directory_writer_auto_identity_has_boot_id_before_lazy_open():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
        })
        assert isinstance(log.boot_id(), bytes)
        assert len(log.boot_id()) == 16
        log.close()


def test_directory_writer_strict_archives_online_chain_active():
    with tempfile.TemporaryDirectory() as td:
        config = {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 0,
            'max_bytes': 0,
            'max_files': 10,
        }
        first = Log(td, config)
        first.append(
            [{'name': 'MESSAGE', 'value': 'strict-migrate-0'}],
            {'realtime_usec': 1_700_002_271_000_000, 'monotonic_usec': 1},
        )
        first.append(
            [{'name': 'MESSAGE', 'value': 'strict-migrate-1'}],
            {'realtime_usec': 1_700_002_271_000_001, 'monotonic_usec': 2},
        )
        chain_path = first.active_file()
        first._active_writer.close()
        first._active_writer = None
        first._closed = True

        strict = Log(td, {**config, 'strict_systemd_naming': True})
        chain_reader = FileReader.open(chain_path)
        assert chain_reader.header()['state'] == STATE_ARCHIVED
        chain_reader.close()

        strict.append(
            [{'name': 'MESSAGE', 'value': 'strict-migrate-2'}],
            {'realtime_usec': 1_700_002_271_000_002, 'monotonic_usec': 3},
        )
        assert Path(strict.active_file()).name == 'system.journal'
        active_reader = FileReader.open(strict.active_file())
        assert active_reader.header()['head_entry_seqnum'] == 3
        assert active_reader.header()['tail_entry_seqnum'] == 3
        active_reader.close()
        strict.close()


def test_directory_writer_replaces_unsupported_chain_active():
    with tempfile.TemporaryDirectory() as td:
        config = {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 0,
            'max_bytes': 0,
            'max_files': 10,
        }
        first = Log(td, config)
        first.append(
            [{'name': 'MESSAGE', 'value': 'replace-chain-0'}],
            {'realtime_usec': 1_700_002_272_000_000, 'monotonic_usec': 1},
        )
        first.append(
            [{'name': 'MESSAGE', 'value': 'replace-chain-1'}],
            {'realtime_usec': 1_700_002_272_000_001, 'monotonic_usec': 2},
        )
        active_path = first.active_file()
        first._active_writer.close()
        first._active_writer = None
        first._closed = True

        clear_keyed_hash_flag(active_path)
        try:
            Writer.open(active_path)
        except ValueError as err:
            assert 'keyed hash required' in str(err)
        else:
            raise AssertionError('expected unsupported chain active rejection')

        second = Log(td, config)
        assert not os.path.exists(active_path)
        assert len(disposed_journal_files(second.journal_directory())) == 1
        second.append(
            [{'name': 'MESSAGE', 'value': 'replace-chain-2'}],
            {'realtime_usec': 1_700_002_272_000_002, 'monotonic_usec': 3},
        )
        assert second.active_file() != active_path
        reader = FileReader.open(second.active_file())
        assert reader.header()['head_entry_seqnum'] == 3
        assert reader.header()['tail_entry_seqnum'] == 3
        reader.close()
        second.close()


def test_directory_writer_replaces_outdated_strict_active():
    with tempfile.TemporaryDirectory() as td:
        config = {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'strict_systemd_naming': True,
            'max_entries': 0,
            'max_bytes': 0,
            'max_files': 10,
        }
        first = Log(td, config)
        first.append(
            [{'name': 'MESSAGE', 'value': 'replace-strict-0'}],
            {'realtime_usec': 1_700_002_273_000_000, 'monotonic_usec': 1},
        )
        first.append(
            [{'name': 'MESSAGE', 'value': 'replace-strict-1'}],
            {'realtime_usec': 1_700_002_273_000_001, 'monotonic_usec': 2},
        )
        active_path = first.active_file()
        first._active_writer.close()
        first._active_writer = None
        first._closed = True

        write_header_size(active_path, HEADER_SIZE - 8)
        try:
            Writer.open(active_path)
        except ValueError as err:
            assert 'outdated header' in str(err)
        else:
            raise AssertionError('expected outdated active rejection')

        second = Log(td, config)
        assert not os.path.exists(active_path)
        assert len(disposed_journal_files(second.journal_directory())) == 1
        second.append(
            [{'name': 'MESSAGE', 'value': 'replace-strict-2'}],
            {'realtime_usec': 1_700_002_273_000_002, 'monotonic_usec': 3},
        )
        assert Path(second.active_file()).name == 'system.journal'
        reader = FileReader.open(second.active_file())
        assert reader.header()['head_entry_seqnum'] == 3
        assert reader.header()['tail_entry_seqnum'] == 3
        reader.close()
        second.close()


def test_directory_writer_discards_empty_online_file_and_continues_sequence():
    with tempfile.TemporaryDirectory() as td:
        config = {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 0,
            'max_bytes': 0,
            'max_files': 10,
        }
        first = Log(td, config)
        first.append([
            {'name': 'MESSAGE', 'value': 'empty-reopen-0'},
            {'name': 'TEST_ID', 'value': 'python-empty-online-reopen'},
        ])
        first.append([
            {'name': 'MESSAGE', 'value': 'empty-reopen-1'},
            {'name': 'TEST_ID', 'value': 'python-empty-online-reopen'},
        ])
        first.close()

        journal_dir = first.journal_directory()
        names = sorted(name for name in os.listdir(journal_dir) if name.endswith('.journal'))
        assert len(names) == 1
        reader = FileReader.open(os.path.join(journal_dir, names[0]))
        header = reader.header()
        next_seqnum = int(header['tail_entry_seqnum']) + 1
        seqnum_id = header['seqnum_id']
        reader.close()

        empty_path = os.path.join(
            journal_dir,
            f'system@{seqnum_id.hex()}-{next_seqnum:016x}-00060a24181e040a.journal',
        )
        empty = Writer.create(empty_path, {
            'machine_id': bytes.fromhex(config['machine_id']),
            'seqnum_id': seqnum_id,
            'head_seqnum': next_seqnum,
        })
        empty.close()

        second = Log(td, config)
        second.append([
            {'name': 'MESSAGE', 'value': 'empty-reopen-2'},
            {'name': 'TEST_ID', 'value': 'python-empty-online-reopen'},
        ])
        second.close()
        assert not os.path.exists(empty_path)

        seqnums = []
        for name in sorted(name for name in os.listdir(journal_dir) if name.endswith('.journal')):
            file_reader = FileReader.open(os.path.join(journal_dir, name))
            while file_reader.step():
                seqnums.append(file_reader.get_entry()['seqnum'])
            file_reader.close()
        assert seqnums == [1, 2, 3]


def test_directory_writer_zero_rotation_limits_disable_rotation():
    with tempfile.TemporaryDirectory() as td:
        log = Log(td, {
            'source': 'system',
            'machine_id': '00112233445566778899aabbccddeeff',
            'max_entries': 0,
            'max_bytes': 0,
            'max_files': 10,
        })
        for i in range(3):
            log.append([{'name': 'MESSAGE', 'value': f'no-rotation-{i}'}])
        log.close()
        names = [name for name in os.listdir(log.journal_directory()) if name.endswith('.journal')]
        assert len(names) == 1


def test_facade_unique_binary_values():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'test.journal')
        writer = Writer.create(path)
        writer.append([{'name': 'BINARY', 'value': bytes([0, 255])}])
        writer.close()
        journal = SdJournalOpen(path, 0)
        values = SdJournalQueryUnique(journal, 'BINARY')
        journal.close()
        assert values == [('BINARY', bytes([0, 255]))]


def test_query_unique_uses_field_index_without_entry_offsets():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'indexed-unique.journal')
        writer = Writer.create(path)
        for priority in ('0', '3', '6', '7'):
            writer.append([
                {'name': 'MESSAGE', 'value': 'irrelevant'},
                {'name': 'PRIORITY', 'value': priority},
            ])
        writer.close()

        reader = FileReader.open(path)
        reader._entry_offsets = []
        try:
            fields = reader.enumerate_fields()
            values = reader.query_unique('PRIORITY')
        finally:
            reader.close()

        assert {'MESSAGE', 'PRIORITY'}.issubset(fields)
        assert {bytes(v) for v in values} == {b'0', b'3', b'6', b'7'}


def test_directory_reader_query_unique_deduplicates_indexed_values_across_files():
    with tempfile.TemporaryDirectory() as td:
        first_path = os.path.join(td, 'unique-first.journal')
        second_path = os.path.join(td, 'unique-second.journal')

        first = Writer.create(first_path)
        first.append([
            {'name': 'MESSAGE', 'value': 'first'},
            {'name': 'PRIORITY', 'value': '6'},
        ])
        first.close()

        second = Writer.create(second_path)
        second.append([
            {'name': 'MESSAGE', 'value': 'second'},
            {'name': 'PRIORITY', 'value': '6'},
        ])
        second.append([
            {'name': 'MESSAGE', 'value': 'third'},
            {'name': 'PRIORITY', 'value': '3'},
        ])
        second.close()

        reader = DirectoryReader.open_files([first_path, second_path])
        try:
            values = reader.query_unique('PRIORITY')
        finally:
            reader.close()

        assert {bytes(v) for v in values} == {b'3', b'6'}


def test_facade_data_payloads_remain_valid_for_current_row():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'facade-row-lifetime.journal')
        writer = Writer.create(path)
        writer.append([
            {'name': 'MESSAGE', 'value': 'first'},
            {'name': 'REPEAT', 'value': 'one'},
            {'name': 'REPEAT', 'value': 'two'},
        ], {'realtime_usec': 1000, 'monotonic_usec': 11})
        writer.close()

        journal = SdJournalOpenFiles([path], 0)
        assert SdJournalNext(journal) == 1
        SdJournalRestartData(journal)
        payloads = collect_nullable(lambda: SdJournalEnumerateAvailableData(journal))
        journal.close()

        assert b'MESSAGE=first' in payloads
        assert b'REPEAT=one' in payloads
        assert b'REPEAT=two' in payloads


def test_facade_compressed_mixed_data_payloads_remain_valid_for_current_row():
    if not zstd_available():
        return
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'facade-compressed-row-lifetime.journal')
        large_value = b'mixed ' * 256
        writer = Writer.create(path, {
            'compression': 'zstd',
            'compression_threshold_bytes': 8,
        })
        writer.append([
            {'name': 'SMALL', 'value': 'x'},
            {'name': 'LARGE', 'value': large_value},
        ], {'realtime_usec': 1000, 'monotonic_usec': 11})
        writer.close()

        journal = SdJournalOpenFiles([path], 0)
        assert SdJournalNext(journal) == 1
        SdJournalRestartData(journal)
        payloads = collect_nullable(lambda: SdJournalEnumerateAvailableData(journal))
        journal.close()

        assert b'SMALL=x' in payloads
        assert b'LARGE=' + large_value in payloads


def test_jf_facade_stateful_reader_operations():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'jf-facade.journal')
        writer = Writer.create(path)
        writer.append([
            {'name': 'MESSAGE', 'value': 'first'},
            {'name': 'REPEAT', 'value': 'one'},
            {'name': 'REPEAT', 'value': 'two'},
            {'name': 'BIN', 'value': bytes([0, 255])},
        ], {'realtime_usec': 1000, 'monotonic_usec': 11})
        writer.append([
            {'name': 'MESSAGE', 'value': 'second'},
            {'name': 'REPEAT', 'value': 'three'},
        ], {'realtime_usec': 1001, 'monotonic_usec': 12})
        writer.close()

        journal = SdJournalOpenFiles([path], 0)
        assert SdJournalNext(journal) == 1
        seqnum, seqnum_id = SdJournalGetSeqnum(journal)
        assert seqnum == 1
        assert seqnum_id
        monotonic, boot_id = SdJournalGetMonotonicUsec(journal)
        assert monotonic == 11
        assert boot_id

        SdJournalRestartData(journal)
        payloads = collect_nullable(lambda: SdJournalEnumerateAvailableData(journal))
        assert b'REPEAT=one' in payloads
        assert b'REPEAT=two' in payloads
        assert b'BIN=\x00\xff' in payloads
        assert SdJournalGetData(journal, 'REPEAT') == b'REPEAT=one'

        SdJournalQueryUniqueState(journal, 'REPEAT')
        unique = collect_nullable(lambda: SdJournalEnumerateAvailableUnique(journal))
        assert b'REPEAT=one' in unique
        assert b'REPEAT=two' in unique
        assert b'REPEAT=three' in unique

        SdJournalRestartFields(journal)
        fields = set(collect_nullable(lambda: SdJournalEnumerateField(journal)))
        assert {'MESSAGE', 'REPEAT', 'BIN'} <= fields

        SdJournalSeekRealtimeUsec(journal, 1001)
        assert SdJournalNext(journal) == 1
        assert SdJournalGetEntry(journal)['fields']['MESSAGE'] == b'second'
        SdJournalSeekRealtimeUsec(journal, 1001)
        assert SdJournalPrevious(journal) == 1
        assert SdJournalGetEntry(journal)['fields']['MESSAGE'] == b'second'
        cursor = SdJournalGetCursor(journal)
        assert SdJournalTestCursor(journal, cursor) is True
        assert SdJournalTestCursor(journal, 'invalid-cursor') is False
        SdJournalSeekRealtimeUsec(journal, 1000)
        assert SdJournalNext(journal) == 1
        assert SdJournalGetEntry(journal)['fields']['MESSAGE'] == b'first'
        SdJournalSeekCursor(journal, cursor)
        assert SdJournalGetEntry(journal)['fields']['MESSAGE'] == b'second'
        journal.close()

        path2 = os.path.join(td, 'jf-facade-second.journal')
        writer2 = Writer.create(path2)
        writer2.append([
            {'name': 'MESSAGE', 'value': 'third'},
            {'name': 'REPEAT', 'value': 'four'},
        ], {'realtime_usec': 1002, 'monotonic_usec': 21})
        writer2.close()

        multi = SdJournalOpenFiles([path2, path], 0)
        messages = []
        while SdJournalNext(multi) == 1:
            messages.append(SdJournalGetEntry(multi)['fields']['MESSAGE'])
        assert messages == [b'first', b'second', b'third']
        SdJournalSeekRealtimeUsec(multi, 1002)
        assert SdJournalPrevious(multi) == 1
        assert SdJournalGetEntry(multi)['fields']['MESSAGE'] == b'third'
        SdJournalSeekRealtimeUsec(multi, 999)
        assert SdJournalPrevious(multi) == 0
        multi.close()


def test_reader_preserves_raw_byte_field_names():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'raw-byte-names.journal')
        invalid_utf8_name = b'\xffRAW'
        nul_name = b'RAW\x00NAME'
        writer = Writer.create(path, {'field_name_policy': FIELD_NAME_POLICY_RAW})
        writer.append([
            {'name': 'MESSAGE', 'value': 'raw byte names'},
            {'name': invalid_utf8_name, 'value': b'invalid utf8'},
            {'name': nul_name, 'value': b'nul name'},
            {'name': 'field name', 'value': b'space'},
            {'name': 'BINARY', 'value': b'a\x00=b'},
        ], {'realtime_usec': 1_700_004_000_000_000, 'monotonic_usec': 1})
        writer.close()

        reader = FileReader.open(path)
        assert reader.step()
        entry = reader.get_entry()
        assert entry['fields']['MESSAGE'] == b'raw byte names'
        assert entry['raw_field_values'][invalid_utf8_name] == [b'invalid utf8']
        assert entry['raw_field_values'][nul_name] == [b'nul name']
        assert entry['raw_field_values'][b'field name'] == [b'space']
        assert reader.get_raw(invalid_utf8_name) == b'invalid utf8'
        assert reader.get_raw(nul_name) == b'nul name'
        assert reader.get_raw_values(b'field name') == [b'space']
        assert any(name == invalid_utf8_name and value == b'invalid utf8'
                   for name, value in entry['raw_fields'])
        assert invalid_utf8_name + b'=invalid utf8' in entry['payloads']
        lossy_name = invalid_utf8_name.decode('utf-8', errors='replace')
        assert lossy_name not in entry['fields']

        payloads = []
        reader.visit_entry_payloads(payloads.append)
        assert invalid_utf8_name + b'=invalid utf8' in payloads
        reader.close()

        exported = export_entry(entry)
        assert invalid_utf8_name + b'=invalid utf8\n' in exported
        encoded = json_entry(entry)
        assert lossy_name not in encoded

        journal = SdJournalOpen(path, 0)
        assert SdJournalNext(journal) == 1
        SdJournalRestartData(journal)
        facade_payloads = collect_nullable(lambda: SdJournalEnumerateAvailableData(journal))
        journal.close()
        assert invalid_utf8_name + b'=invalid utf8' in facade_payloads


def test_python_resource_context_managers_and_bytes_facade_payloads():
    assert _payload_from_field_value(b'MESSAGE', b'hello') == b'MESSAGE=hello'
    assert _payload_from_field_value(bytearray(b'BINARY'), b'\x00value') == b'BINARY=\x00value'

    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'context.journal')
        with Writer.create(path) as writer:
            writer.append([
                {'name': 'MESSAGE', 'value': 'context managers'},
                {'name': 'BINARY', 'value': b'a\x00b'},
            ], {'realtime_usec': 1_700_004_010_000_000, 'monotonic_usec': 1})

        with FileReader.open(path) as reader:
            assert reader.step()
            assert reader.get_raw('BINARY') == b'a\x00b'

        with DirectoryReader.open(td) as reader:
            assert reader.step()
            assert reader.get_raw(b'BINARY') == b'a\x00b'

        with SdJournalOpen(path, 0) as journal:
            assert SdJournalNext(journal) == 1
            assert SdJournalGetData(journal, b'MESSAGE') == b'MESSAGE=context managers'


def test_python_resource_close_hardening():
    class FakeReader:
        def __init__(self, name, fail=False):
            self.name = name
            self.fail = fail
            self.closed = False

        def close(self):
            self.closed = True
            if self.fail:
                raise RuntimeError(f'{self.name} close failed')

    first = FakeReader('first', fail=True)
    second = FakeReader('second')
    directory = DirectoryReader.__new__(DirectoryReader)
    directory._readers = [first, second]
    try:
        directory.close()
    except RuntimeError as e:
        assert 'first close failed' in str(e)
    else:
        raise AssertionError('expected close failure from first reader')
    assert first.closed is True
    assert second.closed is True

    def expect_exception(exc_type, callback):
        try:
            callback()
        except exc_type as err:
            return err
        raise AssertionError(f'expected {exc_type.__name__}')

    reader = FileReader.__new__(FileReader)

    def close_with_failure():
        raise RuntimeError('close failed')

    reader.close = close_with_failure

    def enter_with_body_failure():
        with reader:
            raise ValueError('body failed')

    def enter_without_body_failure():
        with reader:
            pass

    assert str(expect_exception(ValueError, enter_with_body_failure)) == 'body failed'
    assert str(expect_exception(RuntimeError, enter_without_body_failure)) == 'close failed'


def test_file_reader_refresh_failure_preserves_current_mapping():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'refresh-failure.journal')
        writer = Writer.create(path)
        writer.append([
            {'name': 'MESSAGE', 'value': 'refresh guard'},
        ], {'realtime_usec': 1_700_004_015_000_000, 'monotonic_usec': 1})
        writer.close()

        reader = FileReader.open(path)
        original_parse = reader_module.parse_file_header
        old_fd = reader._fd
        old_mmap = reader._mmap
        old_buffer = reader._buffer
        old_offsets = list(reader._entry_offsets)
        try:
            reader.seek_head()
            assert reader.step()
            reader.entry_data_restart()
            assert reader._entry_data_state_active is True
            os.truncate(path, os.path.getsize(path) + 4096)

            def fail_parse(_buffer):
                raise ValueError('forced refresh parse failure')

            reader_module.parse_file_header = fail_parse
            assert reader.refresh() is False
        finally:
            reader_module.parse_file_header = original_parse
        assert reader._fd == old_fd
        assert reader._mmap is old_mmap
        assert reader._buffer is old_buffer
        assert reader._entry_offsets == old_offsets
        assert reader._entry_data_state_active is False
        assert reader._entry_data_offsets == []
        reader.seek_head()
        assert reader.step()
        reader.close()


def test_file_reader_rejects_entry_object_extending_past_buffer():
    reader = FileReader.__new__(FileReader)
    reader._buffer = bytearray(ENTRY_OBJECT_HEADER_SIZE)
    reader._entry_item_size = 16
    reader._compact = False
    write_object_header(reader._buffer, 0, OBJECT_TYPE_ENTRY, 0, ENTRY_OBJECT_HEADER_SIZE + 8)
    try:
        reader._read_entry_metadata_and_offsets(0)
    except ValueError as e:
        assert 'entry object exceeds buffer' in str(e)
    else:
        raise AssertionError('expected oversized entry object rejection')


def test_file_reader_refreshes_published_appends():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'live-reader.journal')
        writer = Writer.create(path, {'live_publish_every_entries': 1})
        try:
            writer.append([
                {'name': 'MESSAGE', 'value': 'first'},
                {'name': 'LIVE_REFRESH', 'value': '1'},
            ], {'realtime_usec': 1_700_004_020_000_000, 'monotonic_usec': 1})
            writer.sync()

            reader = FileReader.open(path)
            try:
                reader.seek_head()
                assert reader.next()
                assert reader.get_entry()['fields']['MESSAGE'] == b'first'
                assert reader.next() is False

                writer.append([
                    {'name': 'MESSAGE', 'value': 'second'},
                    {'name': 'LIVE_REFRESH', 'value': '2'},
                ], {'realtime_usec': 1_700_004_020_000_001, 'monotonic_usec': 2})
                writer.sync()

                assert reader.next()
                assert reader.get_entry()['fields']['MESSAGE'] == b'second'
            finally:
                reader.close()
        finally:
            writer.close()


def test_reader_rejects_non_utf8_match_field_names():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'non-utf8-match.journal')
        writer = Writer.create(path, {'field_name_policy': FIELD_NAME_POLICY_RAW})
        writer.append([
            {'name': b'\xffRAW', 'value': b'value'},
        ], {'realtime_usec': 1_700_004_030_000_000, 'monotonic_usec': 1})
        writer.close()

        reader = FileReader.open(path)
        try:
            try:
                reader.add_match(b'\xffRAW=value')
            except ValueError:
                pass
            else:
                raise AssertionError('expected non-UTF8 match field rejection')
        finally:
            reader.close()


def test_fsprg_vectors():
    vectors_path = REPO_ROOT / 'tests/fss/fixtures/fsprg-vectors-v01.json'
    data = json.loads(vectors_path.read_text())
    params = data['fsprg_params']
    secpar = params['secpar']
    for vec in data['vectors']:
        seed = bytes.fromhex(vec['seed_hex'])
        expected_msk = bytes.fromhex(vec['msk_hex'])
        expected_mpk = bytes.fromhex(vec['mpk_hex'])
        expected_state0 = bytes.fromhex(vec['state0_hex'])

        msk, mpk = gen_mk(seed, secpar)
        assert msk == expected_msk, f'msk mismatch for {vec["seed_desc"]}'
        assert mpk == expected_mpk, f'mpk mismatch for {vec["seed_desc"]}'

        state0 = gen_state0(mpk, seed)
        assert state0 == expected_state0, f'state0 mismatch for {vec["seed_desc"]}'
        assert get_epoch(state0) == 0

        for epoch_vec in vec['epochs']:
            epoch = epoch_vec['epoch']
            evolved = state0
            for _ in range(epoch):
                evolved = evolve(evolved)
            assert evolved == bytes.fromhex(epoch_vec['state_hex']), (
                f'evolve mismatch at epoch {epoch} for {vec["seed_desc"]}'
            )

            seeked = seek(state0, epoch, msk, seed)
            assert seeked == bytes.fromhex(epoch_vec['seek_state_hex']), (
                f'seek mismatch at epoch {epoch} for {vec["seed_desc"]}'
            )

            for key_vec in epoch_vec['keys']:
                key = get_key(evolved, key_vec['keylen'], key_vec['idx'])
                assert key == bytes.fromhex(key_vec['key_hex']), (
                    f'key mismatch idx={key_vec["idx"]} epoch={epoch} for {vec["seed_desc"]}'
                )


def test_conformance_manifest():
    manifest_path = REPO_ROOT / 'tests/conformance/manifests/conformance-v01.json'
    manifest = json.loads(manifest_path.read_text())
    expected_skips = set()
    if not zstd_available():
        expected_skips.update({
            'journal-cursor-test',
            'journal-corruption-append-resilient',
            'journal-export-format',
            'journal-file-header-parse',
            'journal-list-boots',
            'journal-query-unique-fields',
            'journal-stream-directory-iteration',
            'journal-zstd-compressed-read',
            'journal-verify-corruption-detection',
        })
    failures = []
    results = []
    for test_case in manifest['test_suite']['test_cases']:
        stdout = run(
            [sys.executable, str(PYTHON_ROOT / 'adapter.py'), 'run'],
            input_data=json.dumps(test_case).encode(),
        )
        result = json.loads(stdout)
        results.append(result)
        if result['status'] in ('FAIL', 'ERROR'):
            failures.append(result)
        if result['status'] == 'SKIP' and result['test_name'] not in expected_skips:
            result['error'] = 'unexpected SKIP: ' + result.get('note', '')
            failures.append(result)
    assert len(results) == len(manifest['test_suite']['test_cases'])
    if failures:
        raise AssertionError(f'conformance failures: {failures!r}')


def test_verify_file_detects_corruption():
    if not zstd_available():
        return
    from journal.verify import verify_file, VerificationError
    path = REPO_ROOT / 'fixtures/systemd/test-data/corrupted/zstd-truncated-frame.zst'
    try:
        verify_file(str(path))
    except VerificationError as e:
        assert 'corrupt' in str(e).lower(), f"expected 'corrupt' in error, got: {e}"
    else:
        raise AssertionError('expected VerificationError for truncated zstd frame')


def test_verify_file_passes_on_valid_fixture():
    if not zstd_available():
        return
    from journal.verify import verify_file
    path = REPO_ROOT / 'fixtures/systemd/test-data/no-rtc/system.journal.zst'
    verify_file(str(path))  # should not raise


def test_verify_file_with_key_sealed():
    from journal.verify import VerificationError, verify_file_with_key

    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'sealed.journal'
        seal_opts = _test_seal_opts()
        w = Writer.create(str(path), {'seal': seal_opts})
        w.append([{'name': 'MESSAGE', 'value': 'sealed-covered'}], {'realtime_usec': 1_500_000})
        w.append([{'name': 'MESSAGE', 'value': 'later-entry'}], {'realtime_usec': 2_500_000})
        w.close()

        key = _test_verification_key(seal_opts)
        verify_file_with_key(str(path), key)

        if zstd_available():
            from compression import zstd
            zst_path = Path(tmpdir) / 'sealed.journal.zst'
            zst_path.write_bytes(zstd.compress(path.read_bytes()))
            verify_file_with_key(str(zst_path), key)

        try:
            verify_file_with_key(str(path), '000000000000000000000001/1-f4240')
        except VerificationError:
            pass
        else:
            raise AssertionError('expected wrong verification key to fail')

        for bad_key in (
            '000000000000000000000000/10000000000000000-f4240',
            '000000000000000000000000/1-10000000000000000',
        ):
            try:
                verify_file_with_key(str(path), bad_key)
            except VerificationError:
                pass
            else:
                raise AssertionError('expected oversized verification key field to fail')

        _tamper_data_payload(path, b'MESSAGE=sealed-covered')
        try:
            verify_file_with_key(str(path), key)
        except VerificationError:
            pass
        else:
            raise AssertionError('expected authenticated DATA tamper to fail')


def test_journalctl_verify():
    if not zstd_available():
        return
    valid_path = REPO_ROOT / 'fixtures/systemd/test-data/no-rtc/system.journal.zst'
    corrupt_path = REPO_ROOT / 'fixtures/systemd/test-data/corrupted/zstd-truncated-frame.zst'
    script = PYTHON_ROOT / 'cmd/journalctl.py'
    _test_journalctl_verify_valid(script, valid_path)
    _test_journalctl_verify_directory(script, valid_path)
    _test_journalctl_verify_corrupt(script, corrupt_path)
    _test_journalctl_verify_key(script, valid_path)
    _test_journalctl_verify_sealed(script, valid_path)


def _run_journalctl_verify_cmd(script, *args):
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    return subprocess.run(  # nosec B603
        [sys.executable, str(script), *[str(arg) for arg in args]],
        capture_output=True,
        text=True,
    )


def _assert_verify_success(result, label):
    assert result.returncode == 0, f'{label} failed: {result.stderr}'
    assert result.stdout == '', f"expected no stdout, got: {result.stdout}"
    assert 'PASS:' in result.stderr, f"expected PASS in stderr, got: {result.stderr}"


def _assert_verify_failure(result, label, stderr_text=None):
    assert result.returncode != 0, f'expected {label} to fail'
    assert result.stdout == '', f"expected no stdout, got: {result.stdout}"
    if stderr_text is not None:
        assert stderr_text in result.stderr, f"expected {stderr_text!r} in stderr, got: {result.stderr}"


def _test_journalctl_verify_valid(script, valid_path):
    result = _run_journalctl_verify_cmd(script, '--verify', '--file', valid_path)
    assert result.returncode == 0, f'--verify valid failed: {result.stderr}'
    assert result.stdout == '', f"expected no stdout, got: {result.stdout}"
    assert 'PASS:' in result.stderr, f"expected PASS in stderr, got: {result.stderr}"

    result = _run_journalctl_verify_cmd(script, '--verify-only', '--file', valid_path)
    _assert_verify_success(result, '--verify-only valid')


def _test_journalctl_verify_directory(script, valid_path):
    with tempfile.TemporaryDirectory() as tmpdir:
        tmp_path = Path(tmpdir)
        os.symlink(valid_path, tmp_path / 'linked.journal.zst')
        os.mkdir(tmp_path / 'skip.journal.zst')
        result = _run_journalctl_verify_cmd(script, '--verify', '--directory', tmp_path)
    assert result.returncode == 0, f'--verify directory failed: {result.stderr}'
    assert result.stdout == '', f"expected no stdout, got: {result.stdout}"
    assert result.stderr.count('PASS:') == 1, f"expected one PASS in stderr, got: {result.stderr}"
    assert 'FAIL:' not in result.stderr, f"expected no FAIL in stderr, got: {result.stderr}"

    with tempfile.TemporaryDirectory() as tmpdir:
        result = _run_journalctl_verify_cmd(script, '--verify', '--directory', tmpdir)
    assert result.returncode == 0, f'expected --verify empty directory to succeed: {result.stderr}'
    assert result.stdout == '', f"expected no stdout, got: {result.stdout}"
    assert result.stderr == '', f"expected no stderr, got: {result.stderr}"


def _test_journalctl_verify_corrupt(script, corrupt_path):
    result = _run_journalctl_verify_cmd(script, '--verify', '--file', corrupt_path)
    assert result.returncode != 0, 'expected --verify corrupted to fail'
    assert 'FAIL:' in result.stderr, f"expected FAIL in stderr, got: {result.stderr}"


def _test_journalctl_verify_key(script, valid_path):
    result = _run_journalctl_verify_cmd(
        script,
        '--verify-key',
        VALID_FSS_VERIFICATION_KEY,
        '--file',
        valid_path,
    )
    _assert_verify_success(result, '--verify-key unsealed')

    result = _run_journalctl_verify_cmd(script, '--verify-key', 'synthetic-test-key', '--file', valid_path)
    _assert_verify_failure(result, '--verify-key invalid seed', 'Failed to parse seed.')

    result = _run_journalctl_verify_cmd(script, '--verify-key=', '--file', valid_path)
    _assert_verify_failure(result, '--verify-key empty seed', 'Failed to parse seed.')


def _test_journalctl_verify_sealed(script, valid_path):
    from journal.compress import decompress_zst_sync
    with tempfile.TemporaryDirectory() as tmpdir:
        tmp_path = Path(tmpdir) / 'sealed.journal'
        with open(valid_path, 'rb') as f:
            decompressed = decompress_zst_sync(f.read())
        buf = bytearray(decompressed)
        flags = int.from_bytes(buf[8:12], 'little')
        flags |= COMPATIBLE_SEALED
        buf[8:12] = flags.to_bytes(4, 'little')
        tmp_path.write_bytes(buf)

        result = _run_journalctl_verify_cmd(script, '--verify', '--file', tmp_path)
        assert result.returncode != 0, 'expected --verify sealed without key to fail'
        assert 'verification key' in result.stderr, (
            f"expected verification key message in stderr, got: {result.stderr}"
        )
        assert 'PASS:' not in result.stderr, (
            f"sealed file without key should not pass, got: {result.stderr}"
        )

        seal_opts = _test_seal_opts()
        sealed_path = Path(tmpdir) / 'sealed-real.journal'
        w = Writer.create(str(sealed_path), opts={'seal': seal_opts})
        w.append([{'name': 'MESSAGE', 'value': b'sealed verify'}], {'realtime_usec': 1500000})
        w.close()
        key = _test_verification_key(seal_opts)

        result = _run_journalctl_verify_cmd(script, '--verify-key', key, '--file', sealed_path)
        assert result.returncode == 0, f'expected --verify-key sealed to pass, got: {result.stderr}'
        assert 'PASS:' in result.stderr, f"expected PASS in stderr, got: {result.stderr}"

        wrong_key = '000000000000000000000001/1-f4240'
        result = _run_journalctl_verify_cmd(script, '--verify-key', wrong_key, '--file', sealed_path)
        assert result.returncode != 0, 'expected --verify-key with wrong key to fail'
        assert 'FAIL:' in result.stderr, f"expected FAIL in stderr, got: {result.stderr}"


def _test_seal_opts():
    from journal.seal import SealOptions
    return SealOptions(seed=bytes(12), interval_usec=1_000_000, start_usec=1_000_000)


def _test_verification_key(opts):
    start = opts.start_usec // opts.interval_usec
    return f'{opts.seed.hex()}/{start:x}-{opts.interval_usec:x}'


def _tamper_data_payload(path, expected_payload):
    buf = bytearray(Path(path).read_bytes())
    header = parse_file_header(buf)
    scan = _find_tamper_target(buf, header, expected_payload)
    _validate_tamper_target(scan, expected_payload)
    buf[scan['payload_offset']] ^= 0x01
    Path(path).write_bytes(buf)


def _find_tamper_target(buf, header, expected_payload):
    offset = header['header_size']
    tail_object_offset = header['tail_object_offset']
    compact = bool(header['incompatible_flags'] & INCOMPATIBLE_COMPACT)
    scan = {'tag_count': 0, 'second_tag_offset': 0, 'payload_offset': 0, 'object_offset': 0}
    while offset + 16 <= len(buf):
        obj = _read_tamper_scan_object(buf, offset)
        _update_tamper_scan(scan, buf, obj, compact, expected_payload)
        if offset == tail_object_offset:
            break
        offset += obj['aligned']
    return scan


def _read_tamper_scan_object(buf, offset):
    size = int.from_bytes(buf[offset + 8:offset + 16], 'little')
    if size < 16:
        raise AssertionError(f'invalid object size {size} at {offset}')
    aligned = (size + 7) & ~7
    if offset + aligned > len(buf):
        raise AssertionError(f'object at {offset} exceeds file')
    return {'type': buf[offset], 'size': size, 'offset': offset, 'aligned': aligned}


def _update_tamper_scan(scan, buf, obj, compact, expected_payload):
    if obj['type'] == OBJECT_TYPE_TAG:
        _record_tamper_tag(scan, obj['offset'])
    elif obj['type'] == OBJECT_TYPE_DATA:
        _record_matching_tamper_data(scan, buf, obj, compact, expected_payload)


def _record_tamper_tag(scan, offset):
    scan['tag_count'] += 1
    if scan['tag_count'] == 2:
        scan['second_tag_offset'] = offset


def _record_matching_tamper_data(scan, buf, obj, compact, expected_payload):
    payload_offset = COMPACT_DATA_OBJECT_HEADER_SIZE if compact else DATA_OBJECT_HEADER_SIZE
    if obj['size'] <= payload_offset:
        return
    start = obj['offset'] + payload_offset
    end = obj['offset'] + obj['size']
    if bytes(buf[start:end]) == expected_payload:
        scan['payload_offset'] = start
        scan['object_offset'] = obj['offset']


def _validate_tamper_target(scan, expected_payload):
    if scan['payload_offset'] == 0:
        raise AssertionError(f'payload not found: {expected_payload!r}')
    if scan['second_tag_offset'] == 0:
        raise AssertionError('second TAG not found')
    if scan['object_offset'] >= scan['second_tag_offset']:
        raise AssertionError(
            f'DATA object {scan["object_offset"]} is not covered by second TAG {scan["second_tag_offset"]}'
        )


def test_writer_sealed_basic():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts()}
        w = Writer.create(str(path), opts)
        w.append([{'name': 'MESSAGE', 'value': 'hello sealed world'}, {'name': 'PRIORITY', 'value': '6'}],
                 {'realtime_usec': 1_500_000})
        w.close()
        key = _test_verification_key(opts['seal'])
        verify_journal_file_with_key_if_available(path, key)


def test_writer_sealed_interval_crossing():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts()}
        w = Writer.create(str(path), opts)
        w.append([{'name': 'MESSAGE', 'value': 'epoch0'}], {'realtime_usec': 1_000_000})
        w.append([{'name': 'MESSAGE', 'value': 'epoch1'}], {'realtime_usec': 2_000_000})
        w.append([{'name': 'MESSAGE', 'value': 'epoch2'}], {'realtime_usec': 3_000_000})
        w.close()
        key = _test_verification_key(opts['seal'])
        verify_journal_file_with_key_if_available(path, key)


def test_writer_sealed_first_entry_future_epoch():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts()}
        w = Writer.create(str(path), opts)
        w.append([{'name': 'MESSAGE', 'value': 'future epoch first entry'}],
                 {'realtime_usec': 3_000_000})
        w.close()
        key = _test_verification_key(opts['seal'])
        verify_journal_file_with_key_if_available(path, key, 'journalctl verify first-entry future-epoch')


def test_writer_sealed_entry_before_start_rejected():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts()}
        w = Writer.create(str(path), opts)
        try:
            w.append([{'name': 'MESSAGE', 'value': 'before sealing start'}],
                     {'realtime_usec': 500_000})
            assert False, 'expected before-start entry to be rejected'
        except ValueError:
            pass
        finally:
            w.close()


def test_writer_sealed_multi_interval_gap():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts()}
        w = Writer.create(str(path), opts)
        w.append([{'name': 'MESSAGE', 'value': 'epoch0'}], {'realtime_usec': 1_000_000})
        w.append([{'name': 'MESSAGE', 'value': 'epoch5'}], {'realtime_usec': 6_000_000})
        w.close()
        key = _test_verification_key(opts['seal'])
        verify_journal_file_with_key_if_available(path, key, 'journalctl verify multi-interval gap')


def test_writer_sealed_empty_file_stock_verify():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts()}
        w = Writer.create(str(path), opts)
        w.close()
        key = _test_verification_key(opts['seal'])
        verify_journal_file_with_key_if_available(path, key, 'journalctl verify empty sealed file')


def test_writer_sealed_wrong_key_fails():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts()}
        w = Writer.create(str(path), opts)
        w.append([{'name': 'MESSAGE', 'value': 'hello'}], {'realtime_usec': 1_500_000})
        w.close()
        wrong_key = '000000000000000000000001/1-f4240'
        verify_journal_file_with_key_fails_if_available(path, wrong_key)


def test_writer_sealed_tampered_data_fails():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts()}
        w = Writer.create(str(path), opts)
        w.append([{'name': 'MESSAGE', 'value': 'sealed-covered-stock'}], {'realtime_usec': 1_500_000})
        w.append([{'name': 'MESSAGE', 'value': 'later-entry'}], {'realtime_usec': 2_500_000})
        w.close()
        _tamper_data_payload(path, b'MESSAGE=sealed-covered-stock')
        key = _test_verification_key(opts['seal'])
        verify_journal_file_with_key_fails_if_available(path, key)


def test_writer_unsealed_does_not_set_sealed_flags():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        w = Writer.create(str(path))
        w.append([{'name': 'MESSAGE', 'value': 'unsealed'}])
        w.close()
        reader = FileReader.open(str(path))
        header = reader.header()
        reader.close()
        assert not (header['compatible_flags'] & COMPATIBLE_SEALED), 'unsealed writer set SEALED flag'
        assert not (header['compatible_flags'] & COMPATIBLE_SEALED_CONTINUOUS), (
            'unsealed writer set SEALED_CONTINUOUS flag'
        )


def test_writer_file_permissions():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        w = Writer.create(str(path))
        w.close()
        if os.name == 'nt' or sys.platform.startswith(('win', 'msys', 'cygwin')):
            assert path.exists()
        else:
            assert stat.S_IMODE(path.stat().st_mode) == 0o640


def test_compact_sealed_writer_stock_verify():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts(), 'compact': True}
        w = Writer.create(str(path), opts)
        w.append([{'name': 'MESSAGE', 'value': 'compact sealed'}, {'name': 'PRIORITY', 'value': '6'}],
                 {'realtime_usec': 1_500_000})
        w.close()
        key = _test_verification_key(opts['seal'])
        verify_journal_file_with_key_if_available(path, key, 'journalctl verify compact+sealed')


def main():
    run([sys.executable, '-m', 'compileall', str(PYTHON_ROOT)])
    test_windows_import_safety_without_fcntl()
    test_match_validation()
    test_siphash_masks_long_message_length()
    test_live_publish_every_entries_preserves_closed_file_bytes()
    test_journald_field_policy_validation()
    test_live_delay_parser()
    test_parse_file_header_historical_field_boundaries()
    test_reader_accepts_historical_unkeyed_lz4_header()
    test_writer_reader_and_binary_export()
    test_writer_head_seqnum_zero_defaults_to_one()
    test_writer_raw_backward_monotonic_pass_through_fails_verification()
    test_writer_raw_explicit_zero_monotonic_pass_through()
    test_compression_threshold_systemd_policy()
    test_compact_writer_reader_and_stock_verify()
    test_compact_writer_grows_arena_past_initial_allocation()
    test_writer_initial_arena_covers_large_hash_tables()
    test_writer_exclusive_lock()
    test_writer_lock_portable_owner_without_proc()
    test_platform_positional_io_fallback_without_pread_pwrite()
    test_platform_directory_sync_skips_windows_directory_handles()
    test_writer_file_arena_fallback_without_mmap()
    test_writer_archive_closes_before_rename_when_required()
    test_zstd_data_object_parse()
    test_xz_and_lz4_data_object_parse()
    test_directory_writer_rotation()
    test_writer_field_name_policies()
    test_writer_append_raw_policies_and_binary_payloads()
    test_writer_append_raw_matches_structured_bytes()
    test_directory_writer_journald_policy_preserves_protected_fields()
    test_directory_writer_journal_app_policy_drops_invalid_fields()
    test_directory_writer_append_raw_injects_metadata_and_filters_callers()
    test_directory_writer_raw_policy_allows_structure_only_field_names()
    test_directory_writer_duration_rotation()
    test_directory_writer_derives_rotation_defaults_from_retention()
    test_directory_writer_derived_size_rotates_from_retention()
    test_directory_writer_derived_duration_rotates_from_retention()
    test_directory_writer_derived_rotation_small_retention_clamps_to_minimum()
    test_directory_writer_derived_rotation_compact_max_file_size_clamp()
    test_directory_writer_explicit_rotation_overrides_retention_defaults()
    test_directory_writer_default_system_chain_naming()
    test_directory_writer_open_identity_lifecycle_source_timestamp()
    test_directory_writer_different_boot_does_not_seed_monotonic_clamp_from_previous_tail()
    test_directory_writer_explicit_policy_validation()
    test_directory_writer_lifecycle_delete_and_artifact_size()
    test_directory_writer_rejects_empty_entry_without_creating_file()
    test_directory_writer_custom_source_naming()
    test_directory_writer_strict_systemd_naming()
    test_directory_writer_lazy_retention_runs_on_first_open()
    test_directory_writer_eager_retention_runs_on_open_for_all_policies()
    test_directory_writer_enforce_retention_deletes_files_by_age_without_append()
    test_directory_writer_enforce_retention_protects_active_file_by_age()
    test_directory_writer_keeps_chain_named_active_during_retention()
    test_directory_writer_strict_close_protects_current_archive_from_byte_retention()
    test_directory_writer_close_cleans_up_after_archive_error()
    test_directory_writer_rotation_cleans_up_after_archive_error()
    test_directory_writer_strict_reopen_continues_sequence()
    test_directory_writer_chain_reopen_continues_sequence()
    test_directory_writer_chain_reopens_online_file()
    test_directory_writer_auto_identity_has_boot_id_before_lazy_open()
    test_directory_writer_strict_archives_online_chain_active()
    test_directory_writer_replaces_unsupported_chain_active()
    test_directory_writer_replaces_outdated_strict_active()
    test_directory_writer_discards_empty_online_file_and_continues_sequence()
    test_directory_writer_zero_rotation_limits_disable_rotation()
    test_facade_unique_binary_values()
    test_query_unique_uses_field_index_without_entry_offsets()
    test_directory_reader_query_unique_deduplicates_indexed_values_across_files()
    test_facade_data_payloads_remain_valid_for_current_row()
    test_facade_compressed_mixed_data_payloads_remain_valid_for_current_row()
    test_jf_facade_stateful_reader_operations()
    test_reader_preserves_raw_byte_field_names()
    test_python_resource_context_managers_and_bytes_facade_payloads()
    test_python_resource_close_hardening()
    test_file_reader_refresh_failure_preserves_current_mapping()
    test_file_reader_rejects_entry_object_extending_past_buffer()
    test_file_reader_refreshes_published_appends()
    test_reader_rejects_non_utf8_match_field_names()
    test_fsprg_vectors()
    test_verify_file_detects_corruption()
    test_verify_file_passes_on_valid_fixture()
    test_verify_file_with_key_sealed()
    test_journalctl_verify()
    test_writer_sealed_basic()
    test_writer_sealed_interval_crossing()
    test_writer_sealed_first_entry_future_epoch()
    test_writer_sealed_entry_before_start_rejected()
    test_writer_sealed_multi_interval_gap()
    test_writer_sealed_empty_file_stock_verify()
    test_writer_sealed_wrong_key_fails()
    test_writer_sealed_tampered_data_fails()
    test_writer_unsealed_does_not_set_sealed_flags()
    test_writer_file_permissions()
    test_compact_sealed_writer_stock_verify()
    test_conformance_manifest()
    print(f'PASS python package tests ({Path(__file__).relative_to(REPO_ROOT)})')


if __name__ == '__main__':
    main()
