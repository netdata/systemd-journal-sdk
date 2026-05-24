#!/usr/bin/env python3
"""Package-level tests for the pure-Python journal SDK slice."""

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
PYTHON_ROOT = REPO_ROOT / 'python'
sys.path.insert(0, str(PYTHON_ROOT))

from journal import (  # noqa: E402
    DirectoryReader,
    FileReader,
    Log,
    SdJournalOpen,
    SdJournalQueryUnique,
    Writer,
    export_entry,
    json_entry,
    parse_match_string,
)
from journal.entry import parse_data_object  # noqa: E402
from journal.header import (  # noqa: E402
    DATA_OBJECT_HEADER_SIZE,
    OBJECT_COMPRESSED_ZSTD,
    OBJECT_TYPE_DATA,
    write_object_header,
)


def run(args, *, input_data=None, cwd=REPO_ROOT):
    result = subprocess.run(
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


def test_match_validation():
    for item in ('foobar', '', '=', '=xxxxx'):
        try:
            parse_match_string(item)
        except ValueError:
            pass
        else:
            raise AssertionError(f'expected invalid match rejection for {item!r}')
    parse_match_string('FOOBAR=waldo')


def test_lowercase_field_rejected():
    from journal.writer import _validate_field_name

    for item in ('message=value', 'Priority=value', '_myfield=value'):
        try:
            parse_match_string(item)
        except ValueError:
            pass
        else:
            raise AssertionError(f'expected lowercase match rejection for {item!r}')

    for field in ('message', 'Priority', '_myfield'):
        try:
            _validate_field_name(field)
        except ValueError:
            pass
        else:
            raise AssertionError(f'expected lowercase writer rejection for {field!r}')


def test_live_delay_parser():
    import importlib.util

    path = PYTHON_ROOT / 'cmd/livewriter.py'
    spec = importlib.util.spec_from_file_location('livewriter_for_tests', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    assert module.parse_delay_seconds('0') == 0.0
    assert abs(module.parse_delay_seconds('10ns') - 10e-9) < 1e-15
    assert abs(module.parse_delay_seconds('10us') - 10e-6) < 1e-12
    assert abs(module.parse_delay_seconds('10ms') - 0.01) < 1e-12
    assert abs(module.parse_delay_seconds('2s') - 2.0) < 1e-12


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

        stock_count = run(['journalctl', '--file', path, '--output=json', '--no-pager'])
        assert len([line for line in stock_count.splitlines() if line.strip()]) == 1


def test_writer_exclusive_lock():
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, 'test.journal')
        writer = Writer.create(path)
        try:
            try:
                other = Writer.open(path)
            except BlockingIOError:
                other = None
            else:
                other.close()
                raise AssertionError('expected second writer open to fail while first writer holds lock')
        finally:
            writer.close()


def test_zstd_data_object_parse():
    from compression import zstd

    payload = b'MESSAGE=zstd-data-object'
    compressed = zstd.compress(payload)
    size = DATA_OBJECT_HEADER_SIZE + len(compressed)
    buf = bytearray(size)
    write_object_header(buf, 0, OBJECT_TYPE_DATA, OBJECT_COMPRESSED_ZSTD, size)
    buf[DATA_OBJECT_HEADER_SIZE:] = compressed
    parsed = parse_data_object(buf, 0)
    assert parsed == {'name': b'MESSAGE', 'value': b'zstd-data-object'}


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
        assert log.active_file().endswith('/netdata-test.journal')
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
        reader.close()
        assert seq == [f'{i:06d}'.encode() for i in range(5)]

        stock = run(['journalctl', '--directory', td, '--output=json', '--no-pager'])
        assert len([line for line in stock.splitlines() if line.strip()]) == 5


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


def test_conformance_manifest():
    manifest_path = REPO_ROOT / 'tests/conformance/manifests/conformance-v01.json'
    manifest = json.loads(manifest_path.read_text())
    expected_skips = {'journal-verify-sealed', 'journal-verify-corruption-detection'}
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


def main():
    run([sys.executable, '-m', 'compileall', str(PYTHON_ROOT)])
    test_match_validation()
    test_lowercase_field_rejected()
    test_live_delay_parser()
    test_writer_reader_and_binary_export()
    test_writer_exclusive_lock()
    test_zstd_data_object_parse()
    test_directory_writer_rotation()
    test_facade_unique_binary_values()
    test_conformance_manifest()
    print(f'PASS python package tests ({Path(__file__).relative_to(REPO_ROOT)})')


if __name__ == '__main__':
    main()
