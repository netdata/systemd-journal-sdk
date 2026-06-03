from test_support import (
    DirectoryReader,
    ENTRY_OBJECT_HEADER_SIZE,
    FIELD_NAME_POLICY_RAW,
    FileReader,
    OBJECT_TYPE_ENTRY,
    SdJournalEnumerateAvailableData,
    SdJournalEnumerateAvailableUnique,
    SdJournalEnumerateField,
    SdJournalGetCursor,
    SdJournalGetData,
    SdJournalGetEntry,
    SdJournalGetMonotonicUsec,
    SdJournalGetSeqnum,
    SdJournalNext,
    SdJournalOpen,
    SdJournalOpenFiles,
    SdJournalPrevious,
    SdJournalQueryUnique,
    SdJournalQueryUniqueState,
    SdJournalRestartData,
    SdJournalRestartFields,
    SdJournalSeekCursor,
    SdJournalSeekRealtimeUsec,
    SdJournalTestCursor,
    Writer,
    _payload_from_field_value,
    collect_nullable,
    export_entry,
    json_entry,
    os,
    reader_module,
    tempfile,
    write_object_header,
    zstd_available,
)

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


