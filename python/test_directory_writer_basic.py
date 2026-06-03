from test_support import (
    DEFAULT_FIELD_HASH_BUCKETS,
    DirectoryReader,
    FIELD_NAME_POLICY_JOURNAL_APP,
    FIELD_NAME_POLICY_RAW,
    FileReader,
    JOURNAL_COMPACT_SIZE_MAX,
    Log,
    Path,
    Writer,
    data_hash_buckets_for_max_file_size,
    journalctl_directory_rows_if_available,
    os,
    tempfile,
    time,
    verify_journal_file_if_available,
)

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


