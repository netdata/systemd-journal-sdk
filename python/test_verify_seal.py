from test_support import (
    COMPACT_DATA_OBJECT_HEADER_SIZE,
    COMPATIBLE_SEALED,
    COMPATIBLE_SEALED_CONTINUOUS,
    DATA_OBJECT_HEADER_SIZE,
    FileReader,
    INCOMPATIBLE_COMPACT,
    OBJECT_TYPE_DATA,
    OBJECT_TYPE_TAG,
    PYTHON_ROOT,
    Path,
    REPO_ROOT,
    VALID_FSS_VERIFICATION_KEY,
    Writer,
    evolve,
    gen_mk,
    gen_state0,
    get_epoch,
    get_key,
    json,
    os,
    parse_file_header,
    run,
    seek,
    stat,
    subprocess,
    sys,
    tempfile,
    verify_journal_file_with_key_fails_if_available,
    verify_journal_file_with_key_if_available,
    zstd_available,
)

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


def test_writer_file_permissions_override():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / 'test.journal'
        w = Writer.create(str(path), {'file_mode': 0o600})
        w.close()
        if os.name == 'nt' or sys.platform.startswith(('win', 'msys', 'cygwin')):
            assert path.exists()
        else:
            assert stat.S_IMODE(path.stat().st_mode) == 0o600


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


