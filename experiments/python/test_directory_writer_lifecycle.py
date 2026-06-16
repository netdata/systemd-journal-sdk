from test_support import (
    FileReader,
    HEADER_SIZE,
    Log,
    Path,
    STATE_ARCHIVED,
    Writer,
    clear_keyed_hash_flag,
    disposed_journal_files,
    journal_files,
    journalctl_directory_rows_if_available,
    os,
    tempfile,
    verify_journal_file_if_available,
    write_header_size,
)

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


