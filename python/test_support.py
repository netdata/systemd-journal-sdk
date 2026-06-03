#!/usr/bin/env python3
"""Package-level tests for the pure-Python journal SDK slice."""

import importlib.util
import json
import os
import shutil
import stat
import subprocess  # nosec B404
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
    result = subprocess.run(  # nosec B603
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


__all__ = (
    'COMPACT_DATA_OBJECT_HEADER_SIZE',
    'COMPATIBLE_SEALED',
    'COMPATIBLE_SEALED_CONTINUOUS',
    'DATA_OBJECT_HEADER_SIZE',
    'DEFAULT_FIELD_HASH_BUCKETS',
    'DirectoryReader',
    'ENTRY_OBJECT_HEADER_SIZE',
    'FIELD_NAME_POLICY_JOURNAL_APP',
    'FIELD_NAME_POLICY_RAW',
    'FILE_SIZE_INCREASE',
    'FileReader',
    'HEADER_SIZE',
    'INCOMPATIBLE_COMPACT',
    'INCOMPATIBLE_COMPRESSED_LZ4',
    'INCOMPATIBLE_KEYED_HASH',
    'JOURNAL_COMPACT_SIZE_MAX',
    'Log',
    'OBJECT_COMPRESSED_LZ4',
    'OBJECT_COMPRESSED_XZ',
    'OBJECT_COMPRESSED_ZSTD',
    'OBJECT_TYPE_DATA',
    'OBJECT_TYPE_ENTRY',
    'OBJECT_TYPE_TAG',
    'PYTHON_ROOT',
    'Path',
    'REPO_ROOT',
    'STATE_ARCHIVED',
    'SdJournalEnumerateAvailableData',
    'SdJournalEnumerateAvailableUnique',
    'SdJournalEnumerateField',
    'SdJournalGetCursor',
    'SdJournalGetData',
    'SdJournalGetEntry',
    'SdJournalGetMonotonicUsec',
    'SdJournalGetSeqnum',
    'SdJournalNext',
    'SdJournalOpen',
    'SdJournalOpenFiles',
    'SdJournalPrevious',
    'SdJournalQueryUnique',
    'SdJournalQueryUniqueState',
    'SdJournalRestartData',
    'SdJournalRestartFields',
    'SdJournalSeekCursor',
    'SdJournalSeekRealtimeUsec',
    'SdJournalTestCursor',
    'VALID_FSS_VERIFICATION_KEY',
    'Writer',
    '_payload_from_field_value',
    'clear_keyed_hash_flag',
    'collect_nullable',
    'data_hash_buckets_for_max_file_size',
    'disposed_journal_files',
    'evolve',
    'export_entry',
    'gen_mk',
    'gen_state0',
    'get_epoch',
    'get_key',
    'importlib',
    'journal_files',
    'journal_has_data_object_flag',
    'journalctl_available',
    'journalctl_directory_rows_if_available',
    'journalctl_file_rows_if_available',
    'json',
    'json_entry',
    'os',
    'parse_data_object',
    'parse_file_header',
    'parse_match_string',
    'parse_object_header',
    'reader_module',
    'run',
    'seek',
    'shutil',
    'sip_hash_24',
    'stat',
    'subprocess',
    'sys',
    'tempfile',
    'test_windows_import_safety_without_fcntl',
    'time',
    'verify_journal_file_fails_if_available',
    'verify_journal_file_if_available',
    'verify_journal_file_with_key_fails_if_available',
    'verify_journal_file_with_key_if_available',
    'write_header_size',
    'write_object_header',
    'zstd_available',
)
