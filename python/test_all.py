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
    DATA_OBJECT_HEADER_SIZE,
    INCOMPATIBLE_COMPACT,
    OBJECT_COMPRESSED_LZ4,
    OBJECT_COMPRESSED_XZ,
    OBJECT_COMPRESSED_ZSTD,
    OBJECT_TYPE_DATA,
    write_object_header,
)
from journal.seal import COMPATIBLE_SEALED_CONTINUOUS  # noqa: E402
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
    expected_skips = {'journal-verify-sealed'}
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

        result = subprocess.run(
            [sys.executable, str(script), '--verify-key', VALID_FSS_VERIFICATION_KEY, '--file', str(tmp_path)],
            capture_output=True, text=True,
        )
        assert result.returncode != 0, 'expected --verify-key sealed to fail'
        assert 'not yet implemented' in result.stderr, (
            f"expected 'not yet implemented' in stderr, got: {result.stderr}"
        )


def _test_seal_opts():
    from journal.seal import SealOptions
    return SealOptions(seed=bytes(12), interval_usec=1_000_000, start_usec=1_000_000)


def _test_verification_key(opts):
    start = opts.start_usec // opts.interval_usec
    return f'{opts.seed.hex()}/{start:x}-{opts.interval_usec:x}'


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
        wrong_key = '000000000000000000000001/1-1000000'
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
        w.append([{'name': 'MESSAGE', 'value': 'tamper test'}], {'realtime_usec': 1_500_000})
        w.close()
        with open(path, 'r+b') as f:
            f.seek(512)
            f.write(b'\xff')
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
    test_compact_writer_reader_and_stock_verify()
    test_writer_exclusive_lock()
    test_zstd_data_object_parse()
    test_xz_and_lz4_data_object_parse()
    test_directory_writer_rotation()
    test_facade_unique_binary_values()
    test_fsprg_vectors()
    test_verify_file_detects_corruption()
    test_verify_file_passes_on_valid_fixture()
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
