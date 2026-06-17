from test_support import (
    DATA_OBJECT_HEADER_SIZE,
    FIELD_NAME_POLICY_JOURNAL_APP,
    FILE_SIZE_INCREASE,
    FileReader,
    HEADER_SIZE,
    INCOMPATIBLE_COMPACT,
    INCOMPATIBLE_COMPRESSED_LZ4,
    INCOMPATIBLE_KEYED_HASH,
    OBJECT_COMPRESSED_LZ4,
    OBJECT_COMPRESSED_XZ,
    OBJECT_COMPRESSED_ZSTD,
    OBJECT_TYPE_DATA,
    PYTHON_ROOT,
    Path,
    STATE_ARCHIVED,
    Writer,
    export_entry,
    importlib,
    journal_has_data_object_flag,
    journalctl_file_rows_if_available,
    json_entry,
    os,
    parse_data_object,
    parse_file_header,
    parse_match_string,
    reader_module,
    sip_hash_24,
    tempfile,
    verify_journal_file_fails_if_available,
    verify_journal_file_if_available,
    write_object_header,
    zstd_available,
)

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
    from journal.writer_policy import _validate_field_name_for_policy

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


def test_writer_raw_backward_monotonic_legacy_verification_failure():
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

