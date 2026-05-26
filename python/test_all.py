#!/usr/bin/env python3
"""Package-level tests for the pure-Python journal SDK slice."""

import json
import os
import stat
import subprocess
import sys
import tempfile
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
    SdJournalQueryUnique,
    Writer,
    export_entry,
    json_entry,
    parse_match_string,
)
from journal.entry import parse_data_object  # noqa: E402
from journal.header import (  # noqa: E402
    COMPATIBLE_SEALED,
    COMPACT_DATA_OBJECT_HEADER_SIZE,
    DATA_OBJECT_HEADER_SIZE,
    INCOMPATIBLE_COMPACT,
    OBJECT_COMPRESSED_LZ4,
    OBJECT_COMPRESSED_XZ,
    OBJECT_COMPRESSED_ZSTD,
    OBJECT_TYPE_DATA,
    parse_file_header,
    write_object_header,
)
from journal.seal import COMPATIBLE_SEALED_CONTINUOUS, OBJECT_TYPE_TAG  # noqa: E402
from journal.hash import sip_hash_24  # noqa: E402
from journal.fss import gen_mk, gen_state0, evolve, seek, get_key, get_epoch  # noqa: E402


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


def test_siphash_masks_long_message_length():
    key = bytes.fromhex('de5f2812d87b89e81af97cfe8e1423e9')
    payload = b'COMPRESSED_PAYLOAD=' + bytes((i % 26) + 0x41 for i in range(256))
    assert sip_hash_24(key, payload) == 0xf9a795df589b5204


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

        stock = run(['journalctl', '--file', path, '--output=json', '--no-pager'])
        assert len([line for line in stock.splitlines() if line.strip()]) == 2
        run(['journalctl', '--verify', '--file', path, '--no-pager'])


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
        reader.close()
        assert seq == [f'{i:06d}'.encode() for i in range(5)]

        stock = run(['journalctl', '--directory', td, '--output=json', '--no-pager'])
        assert len([line for line in stock.splitlines() if line.strip()]) == 5


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


def test_directory_writer_does_not_enforce_retention_before_first_append():
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
        before = sorted(name for name in os.listdir(journal_dir) if name.endswith('.journal'))
        assert len(before) == 2

        second = Log(td, {**config, 'max_files': 1})
        after = sorted(name for name in os.listdir(journal_dir) if name.endswith('.journal'))
        assert after == before
        second.close()


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
        second.append([{'name': 'MESSAGE', 'value': 'chain-online-reopen-2'}])
        second.close()

        reader = FileReader.open(active_path)
        seqnums = []
        while reader.step():
            seqnums.append(reader.get_entry()['seqnum'])
        reader.close()
        assert seqnums == [1, 2, 3]


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
    from journal.verify import verify_file, VerificationError
    path = REPO_ROOT / 'fixtures/systemd/test-data/corrupted/zstd-truncated-frame.zst'
    try:
        verify_file(str(path))
    except VerificationError as e:
        assert 'corrupt' in str(e).lower(), f"expected 'corrupt' in error, got: {e}"
    else:
        raise AssertionError('expected VerificationError for truncated zstd frame')


def test_verify_file_passes_on_valid_fixture():
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
    valid_path = REPO_ROOT / 'fixtures/systemd/test-data/no-rtc/system.journal.zst'
    corrupt_path = REPO_ROOT / 'fixtures/systemd/test-data/corrupted/zstd-truncated-frame.zst'
    script = PYTHON_ROOT / 'cmd/journalctl.py'

    # --verify valid file
    result = subprocess.run(
        [sys.executable, str(script), '--verify', '--file', str(valid_path)],
        capture_output=True, text=True,
    )
    assert result.returncode == 0, f'--verify valid failed: {result.stderr}'
    assert result.stdout == '', f"expected no stdout, got: {result.stdout}"
    assert 'PASS:' in result.stderr, f"expected PASS in stderr, got: {result.stderr}"

    # --verify-only valid file
    result = subprocess.run(
        [sys.executable, str(script), '--verify-only', '--file', str(valid_path)],
        capture_output=True, text=True,
    )
    assert result.returncode == 0, f'--verify-only valid failed: {result.stderr}'
    assert result.stdout == '', f"expected no stdout, got: {result.stdout}"
    assert 'PASS:' in result.stderr, f"expected PASS in stderr, got: {result.stderr}"

    # --verify directory follows symlinked journals and skips directories
    with tempfile.TemporaryDirectory() as tmpdir:
        tmp_path = Path(tmpdir)
        os.symlink(valid_path, tmp_path / 'linked.journal.zst')
        os.mkdir(tmp_path / 'skip.journal.zst')
        result = subprocess.run(
            [sys.executable, str(script), '--verify', '--directory', str(tmp_path)],
            capture_output=True, text=True,
        )
    assert result.returncode == 0, f'--verify directory failed: {result.stderr}'
    assert result.stdout == '', f"expected no stdout, got: {result.stdout}"
    assert result.stderr.count('PASS:') == 1, f"expected one PASS in stderr, got: {result.stderr}"
    assert 'FAIL:' not in result.stderr, f"expected no FAIL in stderr, got: {result.stderr}"

    # --verify empty directory
    with tempfile.TemporaryDirectory() as tmpdir:
        result = subprocess.run(
            [sys.executable, str(script), '--verify', '--directory', tmpdir],
            capture_output=True, text=True,
        )
    assert result.returncode != 0, 'expected --verify empty directory to fail'
    assert result.stdout == '', f"expected no stdout, got: {result.stdout}"
    assert 'verify: no journal files found' in result.stderr, (
        f"expected no journal files error in stderr, got: {result.stderr}"
    )

    # --verify corrupted file
    result = subprocess.run(
        [sys.executable, str(script), '--verify', '--file', str(corrupt_path)],
        capture_output=True, text=True,
    )
    assert result.returncode != 0, 'expected --verify corrupted to fail'
    assert 'FAIL:' in result.stderr, f"expected FAIL in stderr, got: {result.stderr}"

    # --verify-key unsealed file (valid key parsed, normal verification)
    result = subprocess.run(
        [sys.executable, str(script), '--verify-key', VALID_FSS_VERIFICATION_KEY, '--file', str(valid_path)],
        capture_output=True, text=True,
    )
    assert result.returncode == 0, f'--verify-key unsealed failed: {result.stderr}'
    assert result.stdout == '', f"expected no stdout, got: {result.stdout}"
    assert 'PASS:' in result.stderr, f"expected PASS in stderr, got: {result.stderr}"

    # --verify-key invalid seed
    result = subprocess.run(
        [sys.executable, str(script), '--verify-key', 'synthetic-test-key', '--file', str(valid_path)],
        capture_output=True, text=True,
    )
    assert result.returncode != 0, 'expected --verify-key invalid seed to fail'
    assert result.stdout == '', f"expected no stdout, got: {result.stdout}"
    assert 'Failed to parse seed.' in result.stderr, (
        f"expected parse seed error in stderr, got: {result.stderr}"
    )

    # --verify-key empty seed
    result = subprocess.run(
        [sys.executable, str(script), '--verify-key=', '--file', str(valid_path)],
        capture_output=True, text=True,
    )
    assert result.returncode != 0, 'expected --verify-key empty seed to fail'
    assert result.stdout == '', f"expected no stdout, got: {result.stdout}"
    assert 'Failed to parse seed.' in result.stderr, (
        f"expected parse seed error in stderr, got: {result.stderr}"
    )

    # --verify sealed file without key (key required)
    from journal.compress import decompress_zst_sync
    from journal.header import COMPATIBLE_SEALED
    with tempfile.TemporaryDirectory() as tmpdir:
        tmp_path = Path(tmpdir) / 'sealed.journal'
        with open(valid_path, 'rb') as f:
            decompressed = decompress_zst_sync(f.read())
        buf = bytearray(decompressed)
        flags = int.from_bytes(buf[8:12], 'little')
        flags |= COMPATIBLE_SEALED
        buf[8:12] = flags.to_bytes(4, 'little')
        tmp_path.write_bytes(buf)

        result = subprocess.run(
            [sys.executable, str(script), '--verify', '--file', str(tmp_path)],
            capture_output=True, text=True,
        )
        assert result.returncode != 0, 'expected --verify sealed without key to fail'
        assert 'verification key' in result.stderr, (
            f"expected verification key message in stderr, got: {result.stderr}"
        )
        assert 'PASS:' not in result.stderr, (
            f"sealed file without key should not pass, got: {result.stderr}"
        )

        # --verify-key with real sealed file
        seal_opts = _test_seal_opts()
        from journal.writer import Writer
        sealed_path = Path(tmpdir) / 'sealed-real.journal'
        w = Writer.create(str(sealed_path), opts={'seal': seal_opts})
        w.append([{'name': 'MESSAGE', 'value': b'sealed verify'}], {'realtime_usec': 1500000})
        w.close()
        key = _test_verification_key(seal_opts)

        result = subprocess.run(
            [sys.executable, str(script), '--verify-key', key, '--file', str(sealed_path)],
            capture_output=True, text=True,
        )
        assert result.returncode == 0, f'expected --verify-key sealed to pass, got: {result.stderr}'
        assert 'PASS:' in result.stderr, f"expected PASS in stderr, got: {result.stderr}"

        # wrong key
        wrong_key = '000000000000000000000001/1-f4240'
        result = subprocess.run(
            [sys.executable, str(script), '--verify-key', wrong_key, '--file', str(sealed_path)],
            capture_output=True, text=True,
        )
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
    offset = header['header_size']
    tail_object_offset = header['tail_object_offset']
    compact = bool(header['incompatible_flags'] & INCOMPATIBLE_COMPACT)
    tag_count = 0
    second_tag_offset = 0
    target_payload_offset = 0
    target_object_offset = 0

    while offset + 16 <= len(buf):
        typ = buf[offset]
        size = int.from_bytes(buf[offset + 8:offset + 16], 'little')
        if size < 16:
            raise AssertionError(f'invalid object size {size} at {offset}')
        aligned = (size + 7) & ~7
        if offset + aligned > len(buf):
            raise AssertionError(f'object at {offset} exceeds file')

        if typ == OBJECT_TYPE_TAG:
            tag_count += 1
            if tag_count == 2:
                second_tag_offset = offset
        elif typ == OBJECT_TYPE_DATA:
            payload_offset = COMPACT_DATA_OBJECT_HEADER_SIZE if compact else DATA_OBJECT_HEADER_SIZE
            if size > payload_offset:
                start = offset + payload_offset
                end = offset + size
                if bytes(buf[start:end]) == expected_payload:
                    target_payload_offset = start
                    target_object_offset = offset

        if offset == tail_object_offset:
            break
        offset += aligned

    if target_payload_offset == 0:
        raise AssertionError(f'payload not found: {expected_payload!r}')
    if second_tag_offset == 0:
        raise AssertionError('second TAG not found')
    if target_object_offset >= second_tag_offset:
        raise AssertionError(
            f'DATA object {target_object_offset} is not covered by second TAG {second_tag_offset}'
        )
    buf[target_payload_offset] ^= 0x01
    Path(path).write_bytes(buf)


def test_writer_sealed_basic():
    from journal.writer import Writer
    from journal.seal import SealOptions
    import tempfile
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts()}
        w = Writer.create(str(path), opts)
        w.append([{'name': 'MESSAGE', 'value': 'hello sealed world'}, {'name': 'PRIORITY', 'value': '6'}],
                 {'realtime_usec': 1_500_000})
        w.close()
        key = _test_verification_key(opts['seal'])
        result = subprocess.run(
            ['journalctl', '--verify', '--verify-key', key, '--file', str(path)],
            capture_output=True, text=True,
        )
        assert result.returncode == 0, f'journalctl verify failed: {result.stderr}'
        assert 'PASS:' in result.stderr


def test_writer_sealed_interval_crossing():
    from journal.writer import Writer
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts()}
        w = Writer.create(str(path), opts)
        w.append([{'name': 'MESSAGE', 'value': 'epoch0'}], {'realtime_usec': 1_000_000})
        w.append([{'name': 'MESSAGE', 'value': 'epoch1'}], {'realtime_usec': 2_000_000})
        w.append([{'name': 'MESSAGE', 'value': 'epoch2'}], {'realtime_usec': 3_000_000})
        w.close()
        key = _test_verification_key(opts['seal'])
        result = subprocess.run(
            ['journalctl', '--verify', '--verify-key', key, '--file', str(path)],
            capture_output=True, text=True,
        )
        assert result.returncode == 0, f'journalctl verify failed: {result.stderr}'
        assert 'PASS:' in result.stderr


def test_writer_sealed_first_entry_future_epoch():
    from journal.writer import Writer
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts()}
        w = Writer.create(str(path), opts)
        w.append([{'name': 'MESSAGE', 'value': 'future epoch first entry'}],
                 {'realtime_usec': 3_000_000})
        w.close()
        key = _test_verification_key(opts['seal'])
        result = subprocess.run(
            ['journalctl', '--verify', '--verify-key', key, '--file', str(path)],
            capture_output=True, text=True,
        )
        assert result.returncode == 0, f'journalctl verify first-entry future-epoch failed: {result.stderr}'
        assert 'PASS:' in result.stderr


def test_writer_sealed_entry_before_start_rejected():
    from journal.writer import Writer
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
    from journal.writer import Writer
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts()}
        w = Writer.create(str(path), opts)
        w.append([{'name': 'MESSAGE', 'value': 'epoch0'}], {'realtime_usec': 1_000_000})
        w.append([{'name': 'MESSAGE', 'value': 'epoch5'}], {'realtime_usec': 6_000_000})
        w.close()
        key = _test_verification_key(opts['seal'])
        result = subprocess.run(
            ['journalctl', '--verify', '--verify-key', key, '--file', str(path)],
            capture_output=True, text=True,
        )
        assert result.returncode == 0, f'journalctl verify multi-interval gap failed: {result.stderr}'
        assert 'PASS:' in result.stderr


def test_writer_sealed_empty_file_stock_verify():
    from journal.writer import Writer
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts()}
        w = Writer.create(str(path), opts)
        w.close()
        key = _test_verification_key(opts['seal'])
        result = subprocess.run(
            ['journalctl', '--verify', '--verify-key', key, '--file', str(path)],
            capture_output=True, text=True,
        )
        assert result.returncode == 0, f'journalctl verify empty sealed file failed: {result.stderr}'
        assert 'PASS:' in result.stderr


def test_writer_sealed_wrong_key_fails():
    from journal.writer import Writer
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts()}
        w = Writer.create(str(path), opts)
        w.append([{'name': 'MESSAGE', 'value': 'hello'}], {'realtime_usec': 1_500_000})
        w.close()
        wrong_key = '000000000000000000000001/1-f4240'
        result = subprocess.run(
            ['journalctl', '--verify', '--verify-key', wrong_key, '--file', str(path)],
            capture_output=True, text=True,
        )
        assert result.returncode != 0, 'expected verify to fail with wrong key'


def test_writer_sealed_tampered_data_fails():
    from journal.writer import Writer
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts()}
        w = Writer.create(str(path), opts)
        w.append([{'name': 'MESSAGE', 'value': 'sealed-covered-stock'}], {'realtime_usec': 1_500_000})
        w.append([{'name': 'MESSAGE', 'value': 'later-entry'}], {'realtime_usec': 2_500_000})
        w.close()
        _tamper_data_payload(path, b'MESSAGE=sealed-covered-stock')
        key = _test_verification_key(opts['seal'])
        result = subprocess.run(
            ['journalctl', '--verify', '--verify-key', key, '--file', str(path)],
            capture_output=True, text=True,
        )
        assert result.returncode != 0, (
            f'expected verify to fail with tampered data, got exit {result.returncode}: {result.stderr}'
        )


def test_writer_unsealed_does_not_set_sealed_flags():
    from journal.writer import Writer
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
    from journal.writer import Writer
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        w = Writer.create(str(path))
        w.close()
        assert stat.S_IMODE(path.stat().st_mode) == 0o640


def test_compact_sealed_writer_stock_verify():
    from journal.writer import Writer
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        opts = {'seal': _test_seal_opts(), 'compact': True}
        w = Writer.create(str(path), opts)
        w.append([{'name': 'MESSAGE', 'value': 'compact sealed'}, {'name': 'PRIORITY', 'value': '6'}],
                 {'realtime_usec': 1_500_000})
        w.close()
        key = _test_verification_key(opts['seal'])
        result = subprocess.run(
            ['journalctl', '--verify', '--verify-key', key, '--file', str(path)],
            capture_output=True, text=True,
        )
        assert result.returncode == 0, f'journalctl verify compact+sealed failed: {result.stderr}'
        assert 'PASS:' in result.stderr


def main():
    run([sys.executable, '-m', 'compileall', str(PYTHON_ROOT)])
    test_match_validation()
    test_siphash_masks_long_message_length()
    test_lowercase_field_rejected()
    test_live_delay_parser()
    test_writer_reader_and_binary_export()
    test_writer_head_seqnum_zero_defaults_to_one()
    test_compact_writer_reader_and_stock_verify()
    test_writer_exclusive_lock()
    test_zstd_data_object_parse()
    test_xz_and_lz4_data_object_parse()
    test_directory_writer_rotation()
    test_directory_writer_default_system_chain_naming()
    test_directory_writer_rejects_empty_entry_without_creating_file()
    test_directory_writer_custom_source_naming()
    test_directory_writer_strict_systemd_naming()
    test_directory_writer_keeps_chain_named_active_during_retention()
    test_directory_writer_strict_close_protects_current_archive_from_byte_retention()
    test_directory_writer_close_cleans_up_after_archive_error()
    test_directory_writer_rotation_cleans_up_after_archive_error()
    test_directory_writer_strict_reopen_continues_sequence()
    test_directory_writer_chain_reopen_continues_sequence()
    test_directory_writer_chain_reopens_online_file()
    test_directory_writer_discards_empty_online_file_and_continues_sequence()
    test_directory_writer_zero_rotation_limits_disable_rotation()
    test_facade_unique_binary_values()
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
