use super::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[test]
fn facade_uncompressed_data_uses_mmap_payload_for_whole_file_reader() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/system.journal");
    write_facade_test_journal(&path);

    let mut reader = FileReader::open_with_options(
        &path,
        ReaderOptions::snapshot()
            .with_experimental_mmap_strategy(ExperimentalMmapStrategy::WholeFile),
    )
    .expect("open reader");
    assert!(reader.next().expect("first entry"));
    reader.entry_data_restart().expect("restart data");
    let first_offset = reader.row.data_offset_at(0).expect("first offset");
    let (returned_ptr, returned_len, returned_payload) = {
        let payload = reader
            .enumerate_entry_payload()
            .expect("enumerate data")
            .expect("first payload");
        (payload.as_ptr(), payload.len(), payload.to_vec())
    };
    reader.clear_entry_data_state();

    let (mmap_ptr, mmap_len, mmap_payload) = reader.inner.with(|fields| {
        let data = fields.file.data_ref(first_offset).expect("data ref");
        (
            data.raw_payload().as_ptr(),
            data.raw_payload().len(),
            data.raw_payload().to_vec(),
        )
    });

    assert_eq!(returned_payload, mmap_payload);
    assert_eq!(returned_len, mmap_len);
    assert_eq!(returned_ptr, mmap_ptr);
}

#[test]
fn facade_uncompressed_windowed_data_remains_valid_for_current_row() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/system.journal");
    write_facade_test_journal(&path);

    let mut reader = FileReader::open(&path).expect("open reader");
    assert!(reader.next().expect("first entry"));
    reader.entry_data_restart().expect("restart data");
    let first_offset = reader.row.data_offset_at(0).expect("first offset");

    let (first_ptr, first_len) = {
        let payload = reader
            .enumerate_entry_payload()
            .expect("enumerate first data")
            .expect("first payload");
        assert_eq!(payload, b"MESSAGE=first");
        (payload.as_ptr(), payload.len())
    };

    let second = reader
        .enumerate_entry_payload()
        .expect("enumerate second data")
        .expect("second payload")
        .to_vec();
    assert_eq!(second, b"REPEAT=one");
    while reader
        .enumerate_entry_payload()
        .expect("enumerate rest")
        .is_some()
    {}

    // SAFETY: This test intentionally checks that the previously returned
    // row payload remains valid until the reader advances to another row.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let first_after_end = unsafe { std::slice::from_raw_parts(first_ptr, first_len) };
    assert_eq!(first_after_end, b"MESSAGE=first");

    let mmap_ptr = reader.inner.with(|fields| {
        let data = fields.file.data_ref(first_offset).expect("data ref");
        data.raw_payload().as_ptr()
    });
    assert_eq!(
        first_ptr, mmap_ptr,
        "windowed mmap facade payloads must stay borrowed from row-pinned mmap storage"
    );
}

#[test]
fn facade_uncompressed_windowed_row_pins_are_bounded_under_window_pressure() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/window-pressure.journal");
    let (mut journal_file, mut writer) = create_facade_test_writer(&path);
    let payloads: Vec<Vec<u8>> = (0..24)
        .map(|idx| format!("FIELD_{idx:02}={}", "x".repeat(5000)).into_bytes())
        .collect();
    let payload_refs: Vec<&[u8]> = payloads.iter().map(Vec::as_slice).collect();
    writer
        .add_entry(&mut journal_file, &payload_refs, 1000, 11)
        .expect("write pressure entry");
    journal_file.sync().expect("sync pressure journal");

    let mut reader =
        FileReader::open_with_options(&path, ReaderOptions::snapshot().with_window_size(4096))
            .expect("open pressure reader");
    assert!(reader.next().expect("first entry"));
    reader.entry_data_restart().expect("restart data");

    let (first_ptr, first_len, first_expected) = {
        let payload = reader
            .enumerate_entry_payload()
            .expect("enumerate first data")
            .expect("first payload");
        (payload.as_ptr(), payload.len(), payload.to_vec())
    };

    let mut count = 1;
    while reader
        .enumerate_entry_payload()
        .expect("enumerate pressure row")
        .is_some()
    {
        count += 1;
    }
    assert_eq!(count, payloads.len());

    let stats = reader
        .inner
        .with_file(|file| file.mmap_stats())
        .expect("mmap stats");
    assert!(
        stats.row_pin_count <= stats.row_pin_limit,
        "row pins must stay bounded by the normal rolling-window cache"
    );
    assert_eq!(
        stats.row_pin_count, stats.row_pin_limit,
        "hostile pressure row should hit the row-pin cap"
    );
    assert!(
        stats.row_overflow_object_count > 0,
        "hostile pressure row should use row-scoped overflow storage"
    );

    // SAFETY: This intentionally verifies the current-row lifetime guarantee
    // after later payload fetches forced additional rolling mmap windows.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let first_after_pressure = unsafe { std::slice::from_raw_parts(first_ptr, first_len) };
    assert_eq!(first_after_pressure, first_expected.as_slice());
    assert!(
        reader.row.row_pins_active(),
        "current row should keep mmap windows pinned while payload pointers are row-valid"
    );

    assert!(!reader.next().expect("advance past pressure row"));
    assert!(
        !reader.row.row_pins_active(),
        "leaving the current row must clear row-pinned mmap windows"
    );
}

#[test]
fn visit_entry_payloads_clears_row_pins_when_visitor_returns_error() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/system.journal");
    write_facade_test_journal(&path);

    let mut reader = FileReader::open_with_options(
        &path,
        ReaderOptions::snapshot()
            .with_experimental_mmap_strategy(ExperimentalMmapStrategy::WholeFile),
    )
    .expect("open reader");
    assert!(reader.next().expect("first entry"));

    let err = reader
        .visit_entry_payloads(|payload| {
            assert_eq!(payload, b"MESSAGE=first");
            Err(SdkError::Unsupported("intentional visitor error"))
        })
        .expect_err("visitor error should propagate");
    assert!(matches!(
        err,
        SdkError::Unsupported("intentional visitor error")
    ));
    assert!(
        !reader.row.row_pins_active(),
        "visitor errors must clear row-pinned mmap windows"
    );
    assert!(
        !reader.row.data_state_active(),
        "visitor errors must clear active data enumeration state"
    );

    reader
        .entry_data_restart()
        .expect("restart data after error");
    let payload = reader
        .enumerate_entry_payload()
        .expect("enumerate after visitor error")
        .expect("first payload after restart");
    assert_eq!(payload, b"MESSAGE=first");
}

#[test]
fn file_reader_steps_forward_and_backward_across_entry_array_nodes() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/many-entry-arrays.journal");
    let (mut journal_file, mut writer) = create_facade_test_writer(&path);

    for idx in 0..80u64 {
        let message = format!("MESSAGE=row-{idx:02}");
        writer
            .add_entry(
                &mut journal_file,
                &[message.as_bytes()],
                10_000 + idx,
                20_000 + idx,
            )
            .expect("write entry");
    }
    journal_file.sync().expect("sync journal");

    let mut reader = FileReader::open(&path).expect("open reader");
    let mut forward = Vec::new();
    while reader.next().expect("next entry") {
        forward.push(reader.get_seqnum().expect("seqnum").0);
    }
    assert_eq!(forward, (1..=80).collect::<Vec<_>>());

    reader.seek_tail();
    let mut backward = Vec::new();
    while reader.previous().expect("previous entry") {
        backward.push(reader.get_seqnum().expect("seqnum").0);
    }
    assert_eq!(backward, (1..=80).rev().collect::<Vec<_>>());
}

#[test]
fn facade_compressed_data_payloads_remain_valid_for_current_row() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/system.journal");
    let (mut journal_file, mut writer) = create_facade_compressed_test_writer(&path);
    let first_payload = format!("FIRST={}", "a".repeat(2048));
    let second_payload = format!("SECOND={}", "b".repeat(2047));
    assert_eq!(first_payload.len(), second_payload.len());
    writer
        .add_entry(
            &mut journal_file,
            &[first_payload.as_bytes(), second_payload.as_bytes()],
            1000,
            11,
        )
        .expect("write compressed entry");
    journal_file.sync().expect("sync compressed journal");

    let mut reader = FileReader::open(&path).expect("open reader");
    assert!(reader.next().expect("first entry"));
    reader.entry_data_restart().expect("restart data");

    let (first_ptr, first_len) = {
        let payload = reader
            .enumerate_entry_payload()
            .expect("enumerate first data")
            .expect("first payload");
        assert_eq!(payload, first_payload.as_bytes());
        (payload.as_ptr(), payload.len())
    };

    let second = reader
        .enumerate_entry_payload()
        .expect("enumerate second data")
        .expect("second payload")
        .to_vec();
    assert_eq!(second, second_payload.as_bytes());
    assert!(
        reader
            .enumerate_entry_payload()
            .expect("enumerate end")
            .is_none()
    );

    // SAFETY: This test intentionally checks that compressed row payload
    // storage remains valid while enumerating the current row.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let first_after_second = unsafe { std::slice::from_raw_parts(first_ptr, first_len) };
    assert_eq!(first_after_second, first_payload.as_bytes());
}

#[test]
fn facade_whole_file_row_handles_mixed_compressed_and_uncompressed_payloads() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/system.journal");
    let (mut journal_file, mut writer) = create_facade_compressed_test_writer(&path);
    let small_payload = b"SMALL=x".to_vec();
    let large_payload = format!("LARGE={}", "mixed ".repeat(256)).into_bytes();
    writer
        .add_entry(
            &mut journal_file,
            &[small_payload.as_slice(), large_payload.as_slice()],
            1000,
            11,
        )
        .expect("write mixed compressed entry");
    journal_file.sync().expect("sync mixed compressed journal");

    let mut reader = FileReader::open_with_options(
        &path,
        ReaderOptions::snapshot()
            .with_experimental_mmap_strategy(ExperimentalMmapStrategy::WholeFile),
    )
    .expect("open whole-file reader");
    assert!(reader.next().expect("first entry"));
    reader.entry_data_restart().expect("restart data");
    let small_offset = reader.row.data_offset_at(0).expect("small offset");

    let (small_ptr, small_len) = {
        let payload = reader
            .enumerate_entry_payload()
            .expect("enumerate small data")
            .expect("small payload");
        assert_eq!(payload, small_payload.as_slice());
        (payload.as_ptr(), payload.len())
    };
    let (large_ptr, large_len) = {
        let payload = reader
            .enumerate_entry_payload()
            .expect("enumerate large data")
            .expect("large payload");
        assert_eq!(payload, large_payload.as_slice());
        (payload.as_ptr(), payload.len())
    };
    assert!(
        reader
            .enumerate_entry_payload()
            .expect("enumerate end")
            .is_none()
    );

    // SAFETY: This test intentionally checks that both current-row payload
    // pointers remain valid after the row enumeration reaches EOF.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let small_after_end = unsafe { std::slice::from_raw_parts(small_ptr, small_len) };
    // SAFETY: Same current-row lifetime check as `small_after_end`.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let large_after_end = unsafe { std::slice::from_raw_parts(large_ptr, large_len) };
    assert_eq!(small_after_end, small_payload.as_slice());
    assert_eq!(large_after_end, large_payload.as_slice());

    let mmap_ptr = reader.inner.with(|fields| {
        let data = fields.file.data_ref(small_offset).expect("data ref");
        data.raw_payload().as_ptr()
    });
    assert_eq!(
        small_ptr, mmap_ptr,
        "small uncompressed payload should remain borrowed from whole-file mmap"
    );
}

#[test]
fn file_reader_seek_clears_cached_entry_payload_offsets() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/system.journal");
    write_facade_test_journal(&path);

    let mut reader = FileReader::open(&path).expect("open reader");
    assert!(reader.next().expect("first entry"));
    assert!(
        reader
            .enumerate_entry_payload()
            .expect("enumerate first payload")
            .is_some()
    );

    reader.seek_tail();
    assert!(
        reader
            .enumerate_entry_payload()
            .expect("enumerate after seek")
            .is_none()
    );
}

#[test]
fn file_reader_query_unique_uses_field_index() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/indexed-unique.journal");
    let (mut journal_file, mut writer) = create_facade_test_writer(&path);
    for (index, priority) in [b"0", b"3", b"6", b"7"].into_iter().enumerate() {
        let payload = [b"PRIORITY=".as_slice(), priority.as_slice()].concat();
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=irrelevant".as_slice(), payload.as_slice()],
                2_000 + index as u64,
                20 + index as u64,
            )
            .expect("write entry");
    }
    journal_file.sync().expect("sync journal");

    let mut reader = FileReader::open(&path).expect("open reader");
    let fields = reader.enumerate_fields().expect("enumerate fields");
    assert!(fields.iter().any(|field| field == "MESSAGE"));
    assert!(fields.iter().any(|field| field == "PRIORITY"));

    let values = reader.query_unique("PRIORITY").expect("query unique");
    let got: HashSet<Vec<u8>> = values.into_iter().collect();
    let want: HashSet<Vec<u8>> = [b"0", b"3", b"6", b"7"]
        .into_iter()
        .map(|value| value.to_vec())
        .collect();
    assert_eq!(got, want);

    let mut visited = Vec::new();
    reader
        .visit_unique_values("PRIORITY", |value| {
            visited.push(value.to_vec());
            Ok(())
        })
        .expect("visit unique");
    let got: HashSet<Vec<u8>> = visited.into_iter().collect();
    assert_eq!(got, want);
}

#[test]
fn directory_reader_query_unique_deduplicates_indexed_values_across_files() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let first_path = dir.path().join("journals/unique-first.journal");
    let second_path = dir.path().join("journals/unique-second.journal");
    let (mut first_file, mut first_writer) = create_facade_test_writer(&first_path);
    let (mut second_file, mut second_writer) = create_facade_test_writer(&second_path);

    first_writer
        .add_entry(
            &mut first_file,
            &[b"MESSAGE=first".as_slice(), b"PRIORITY=6".as_slice()],
            2_100,
            21,
        )
        .expect("write first");
    second_writer
        .add_entry(
            &mut second_file,
            &[b"MESSAGE=second".as_slice(), b"PRIORITY=6".as_slice()],
            2_200,
            22,
        )
        .expect("write second");
    second_writer
        .add_entry(
            &mut second_file,
            &[b"MESSAGE=third".as_slice(), b"PRIORITY=3".as_slice()],
            2_300,
            23,
        )
        .expect("write third");
    first_file.sync().expect("sync first");
    second_file.sync().expect("sync second");

    let mut reader = DirectoryReader::open_files([&first_path, &second_path]).expect("open files");
    let values = reader.query_unique("PRIORITY").expect("query unique");
    let got: HashSet<Vec<u8>> = values.into_iter().collect();
    let want: HashSet<Vec<u8>> = [b"3", b"6"].into_iter().map(|v| v.to_vec()).collect();
    assert_eq!(got, want);

    let mut visited = Vec::new();
    reader
        .visit_unique_values("PRIORITY", |value| {
            visited.push(value.to_vec());
            Ok(())
        })
        .expect("visit unique");
    let got: HashSet<Vec<u8>> = visited.into_iter().collect();
    assert_eq!(got, want);

    reader
        .query_unique_state("PRIORITY")
        .expect("query direct stateful unique");
    let first_payload = reader
        .enumerate_unique_payload()
        .expect("enumerate first direct unique")
        .expect("first direct unique exists");
    reader.seek_head();
    let mut direct_stateful = vec![first_payload];
    while let Some(payload) = reader
        .enumerate_unique_payload()
        .expect("enumerate direct unique after seek")
    {
        direct_stateful.push(payload);
    }
    let direct_want: HashSet<Vec<u8>> = [b"PRIORITY=3".to_vec(), b"PRIORITY=6".to_vec()]
        .into_iter()
        .collect();
    assert_eq!(direct_stateful.len(), direct_want.len());
    let got: HashSet<Vec<u8>> = direct_stateful.into_iter().collect();
    assert_eq!(got, direct_want);

    reader.restart_unique_state();
    reader.seek_tail();
    let mut restarted_direct = Vec::new();
    while let Some(payload) = reader
        .enumerate_unique_payload()
        .expect("enumerate restarted direct unique after seek")
    {
        restarted_direct.push(payload);
    }
    assert_eq!(restarted_direct.len(), direct_want.len());
    let got: HashSet<Vec<u8>> = restarted_direct.into_iter().collect();
    assert_eq!(got, direct_want);

    let mut journal = SdJournalOpenFiles(
        &[
            first_path.to_str().expect("first path"),
            second_path.to_str().expect("second path"),
        ],
        0,
    )
    .expect("open facade files");
    SdJournalQueryUniqueState(&mut journal, "PRIORITY").expect("query facade unique state");
    let mut stateful = Vec::new();
    while let Some(payload) =
        SdJournalEnumerateAvailableUnique(&mut journal).expect("enumerate facade unique")
    {
        stateful.push(payload);
    }
    let want: HashSet<Vec<u8>> = [b"PRIORITY=3".to_vec(), b"PRIORITY=6".to_vec()]
        .into_iter()
        .collect();
    assert_eq!(stateful.len(), want.len());
    let got: HashSet<Vec<u8>> = stateful.into_iter().collect();
    assert_eq!(got, want);

    SdJournalRestartUnique(&mut journal).expect("restart facade unique state");
    let mut restarted = Vec::new();
    while let Some(payload) =
        SdJournalEnumerateAvailableUnique(&mut journal).expect("enumerate restarted unique")
    {
        restarted.push(payload);
    }
    assert_eq!(restarted.len(), want.len());
    let got: HashSet<Vec<u8>> = restarted.into_iter().collect();
    assert_eq!(got, want);
}

#[test]
fn jf_facade_stateful_reader_operations() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/system.journal");
    write_facade_test_journal(&path);

    let mut journal =
        SdJournalOpenFiles(&[path.to_str().expect("utf8 path")], 0).expect("open files");
    assert_stateful_facade_current_entry(&mut journal);
    assert_stateful_facade_data_enumeration(&mut journal);
    assert_stateful_facade_unique_and_field_enumeration(&mut journal);
    assert_stateful_facade_cursor_navigation(&mut journal);
    assert_stateful_facade_multi_file_navigation(&dir, &path);
    assert_stateful_facade_match_cache_invalidation(&dir, &path);
}

fn assert_stateful_facade_current_entry(journal: &mut SdJournal) {
    assert_eq!(SdJournalNext(journal).expect("next"), 1);
    let (seqnum, seqnum_id) = SdJournalGetSeqnum(journal).expect("seqnum");
    assert_eq!(seqnum, 1);
    assert_ne!(seqnum_id, [0; 16]);
    let (monotonic, boot_id) = SdJournalGetMonotonicUsec(journal).expect("monotonic");
    assert_eq!(monotonic, 11);
    assert_ne!(boot_id, [0; 16]);

    SdJournalRestartData(journal).expect("restart data for interleaved calls");
    let first_payload = SdJournalEnumerateAvailableData(journal)
        .expect("enumerate first data")
        .expect("first data exists");
    assert!(!first_payload.is_empty());
    assert_eq!(
        SdJournalGetRealtimeUsec(journal).expect("interleaved realtime"),
        1000
    );
    assert!(
        !SdJournalGetCursor(journal)
            .expect("interleaved cursor")
            .is_empty()
    );
    assert_eq!(
        SdJournalGetData(journal, "REPEAT").expect("interleaved get data"),
        b"REPEAT=one"
    );
    assert_eq!(
        SdJournalGetEntry(journal)
            .expect("interleaved get entry")
            .get_str("MESSAGE"),
        Some("first")
    );
}

fn assert_stateful_facade_data_enumeration(journal: &mut SdJournal) {
    SdJournalRestartData(journal).expect("restart data");
    let mut payloads = Vec::new();
    while let Some(payload) = SdJournalEnumerateAvailableData(journal).expect("enumerate data") {
        payloads.push(payload.to_vec());
    }
    assert!(payloads.iter().any(|payload| payload == b"REPEAT=one"));
    assert!(payloads.iter().any(|payload| payload == b"REPEAT=two"));
    assert!(payloads.iter().any(|payload| payload == b"BIN=\x00\xff"));
    SdJournalRestartData(journal).expect("restart data again");
    let mut restarted_payloads = Vec::new();
    while let Some(payload) =
        SdJournalEnumerateAvailableData(journal).expect("enumerate restarted data")
    {
        restarted_payloads.push(payload.to_vec());
    }
    assert_eq!(payloads, restarted_payloads);
    assert_eq!(
        SdJournalGetData(journal, "REPEAT").expect("get data"),
        b"REPEAT=one"
    );
}

fn assert_stateful_facade_unique_and_field_enumeration(journal: &mut SdJournal) {
    let direct_unique = SdJournalQueryUnique(journal, "BIN").expect("query unique");
    assert_eq!(direct_unique.len(), 1);
    assert_eq!(direct_unique[0].0, "BIN");
    assert_eq!(direct_unique[0].1, b"\x00\xff");

    SdJournalQueryUniqueState(journal, "REPEAT").expect("query unique state");
    let mut unique = Vec::new();
    while let Some(payload) = SdJournalEnumerateAvailableUnique(journal).expect("enumerate unique")
    {
        unique.push(payload);
    }
    assert!(unique.iter().any(|payload| payload == b"REPEAT=one"));
    assert!(unique.iter().any(|payload| payload == b"REPEAT=two"));
    assert!(unique.iter().any(|payload| payload == b"REPEAT=three"));

    SdJournalRestartUnique(journal).expect("restart unique");
    let restarted = SdJournalEnumerateAvailableUnique(journal)
        .expect("enumerate restarted unique")
        .expect("restarted unique exists");
    assert!(
        unique.iter().any(|payload| payload == &restarted),
        "restart should enumerate the same FIELD=value payload set"
    );

    SdJournalRestartFields(journal).expect("restart fields");
    let mut fields = HashSet::new();
    while let Some(field) = SdJournalEnumerateField(journal).expect("enumerate field") {
        fields.insert(field);
    }
    assert!(fields.contains("MESSAGE"));
    assert!(fields.contains("REPEAT"));
    assert!(fields.contains("BIN"));
}

fn assert_stateful_facade_cursor_navigation(journal: &mut SdJournal) {
    SdJournalSeekRealtimeUsec(journal, 1001).expect("seek realtime forward");
    assert_eq!(SdJournalNext(journal).expect("next after realtime"), 1);
    let entry = SdJournalGetEntry(journal).expect("entry after realtime");
    assert_eq!(entry.get_str("MESSAGE"), Some("second"));

    SdJournalSeekRealtimeUsec(journal, 1001).expect("seek realtime backward");
    assert_eq!(
        SdJournalPrevious(journal).expect("previous after realtime"),
        1
    );
    let entry = SdJournalGetEntry(journal).expect("entry after reverse realtime");
    assert_eq!(entry.get_str("MESSAGE"), Some("second"));

    let cursor = SdJournalGetCursor(journal).expect("cursor");
    assert!(SdJournalTestCursor(journal, &cursor).expect("test current cursor"));
    assert!(!SdJournalTestCursor(journal, "invalid-cursor").expect("test invalid cursor"));
    assert!(matches!(
        SdJournalSeekCursor(journal, "invalid-cursor"),
        Err(FacadeError::InvalidCursor)
    ));
    SdJournalSeekRealtimeUsec(journal, 1000).expect("seek first by realtime");
    assert_eq!(SdJournalNext(journal).expect("next to first"), 1);
    let entry = SdJournalGetEntry(journal).expect("first entry");
    assert_eq!(entry.get_str("MESSAGE"), Some("first"));
    let first_cursor = SdJournalGetCursor(journal).expect("first cursor");
    let first_realtime =
        SdJournalGetRealtimeUsec(journal).expect("first realtime after cursor seek");
    SdJournalSeekCursor(journal, &cursor).expect("seek cursor back to second");
    let entry = SdJournalGetEntry(journal).expect("entry after cursor seek");
    assert_eq!(entry.get_str("MESSAGE"), Some("second"));
    let partial_first_cursor = partial_seqnum_cursor(&first_cursor);
    SdJournalSeekCursor(journal, &partial_first_cursor).expect("seek partial first cursor");
    assert!(SdJournalTestCursor(journal, &partial_first_cursor).expect("test partial cursor"));
    let entry = SdJournalGetEntry(journal).expect("entry after partial cursor seek");
    assert_eq!(entry.get_str("MESSAGE"), Some("first"));
    let missing_cursor = cursor_with_missing_seqnum(&first_cursor);
    SdJournalSeekCursor(journal, &missing_cursor)
        .expect("valid missing cursor is accepted as a seek location");
    assert!(
        SdJournalGetEntry(journal).is_err(),
        "future same-source seqnum cursor should leave no current row"
    );
    let different_seqnum_id_cursor = "s=00000000000000000000000000000000;i=f423f";
    SdJournalSeekCursor(journal, different_seqnum_id_cursor)
        .expect("different seqnum id cursor seeks to head");
    let entry = SdJournalGetEntry(journal).expect("entry after different seqnum id cursor");
    assert_eq!(entry.get_str("MESSAGE"), Some("first"));
    assert_eq!(
        SdJournalGetRealtimeUsec(journal).expect("realtime after different seqnum id cursor"),
        first_realtime
    );
}

fn cursor_with_missing_seqnum(cursor: &str) -> String {
    let mut parts: Vec<String> = cursor.split(';').map(ToString::to_string).collect();
    for part in &mut parts {
        if part.starts_with("i=") {
            *part = "i=f423f".to_string();
            return parts.join(";");
        }
        if part.starts_with("n=") {
            *part = "n=999999".to_string();
            return parts.join(";");
        }
    }
    panic!("cursor has seqnum segment: {cursor}");
}

fn partial_seqnum_cursor(cursor: &str) -> String {
    let mut seqnum_id = None;
    let mut seqnum = None;
    for part in cursor.split(';') {
        if part.starts_with("s=") {
            seqnum_id = Some(part);
        }
        if part.starts_with("i=") {
            seqnum = Some(part);
        }
    }
    format!(
        "{};{}",
        seqnum_id.expect("cursor has seqnum id segment"),
        seqnum.expect("cursor has seqnum segment")
    )
}

fn assert_stateful_facade_multi_file_navigation(dir: &tempfile::TempDir, path: &Path) {
    let path2 = facade_second_journal_path(dir);
    let mut multi = SdJournalOpenFiles(
        &[
            path2.to_str().expect("utf8 second path"),
            path.to_str().expect("utf8 first path"),
        ],
        0,
    )
    .expect("open multiple files");

    let mut messages = Vec::new();
    while SdJournalNext(&mut multi).expect("multi next") == 1 {
        let entry = SdJournalGetEntry(&mut multi).expect("multi entry");
        messages.push(entry.get_str("MESSAGE").unwrap_or("").to_string());
    }
    // systemd compares same-source seqnums before realtime when interleaving files.
    assert_eq!(messages, vec!["first", "third", "second"]);

    let mut cursor_multi = SdJournalOpenFiles(
        &[
            path2.to_str().expect("utf8 second path"),
            path.to_str().expect("utf8 first path"),
        ],
        0,
    )
    .expect("open cursor multiple files");
    assert_eq!(
        SdJournalNext(&mut cursor_multi).expect("cursor multi first"),
        1
    );
    let multi_first_cursor = SdJournalGetCursor(&cursor_multi).expect("cursor multi first cursor");
    assert_eq!(
        SdJournalNext(&mut cursor_multi).expect("cursor multi second"),
        1
    );
    let multi_second_cursor =
        SdJournalGetCursor(&cursor_multi).expect("cursor multi second cursor");
    let entry = SdJournalGetEntry(&mut cursor_multi).expect("cursor multi second entry");
    assert_eq!(entry.get_str("MESSAGE"), Some("third"));
    SdJournalSeekCursor(&mut cursor_multi, &multi_second_cursor)
        .expect("directory cursor seek to found entry");
    assert!(
        SdJournalTestCursor(&cursor_multi, &multi_second_cursor)
            .expect("directory cursor seek landed on found cursor")
    );
    let entry = SdJournalGetEntry(&mut cursor_multi).expect("directory entry after cursor seek");
    assert_eq!(entry.get_str("MESSAGE"), Some("third"));
    let missing_cursor = cursor_with_missing_seqnum(&multi_first_cursor);
    SdJournalSeekCursor(&mut cursor_multi, &missing_cursor)
        .expect("directory valid missing cursor is accepted as a seek location");
    assert!(
        SdJournalGetEntry(&mut cursor_multi).is_err(),
        "directory future same-source seqnum cursor should leave no current row"
    );

    SdJournalSeekRealtimeUsec(&mut multi, 1002).expect("multi seek realtime backward");
    assert_eq!(SdJournalPrevious(&mut multi).expect("multi previous"), 1);
    let entry = SdJournalGetEntry(&mut multi).expect("multi entry after seek");
    assert_eq!(entry.get_str("MESSAGE"), Some("second"));

    SdJournalSeekRealtimeUsec(&mut multi, 999).expect("multi seek before range");
    assert_eq!(
        SdJournalPrevious(&mut multi).expect("multi previous before range"),
        0
    );
}

fn assert_stateful_facade_match_cache_invalidation(dir: &tempfile::TempDir, path: &Path) {
    let path2 = facade_second_journal_path(dir);
    let mut filtered_multi = SdJournalOpenFiles(
        &[
            path2.to_str().expect("utf8 second path"),
            path.to_str().expect("utf8 first path"),
        ],
        0,
    )
    .expect("open filtered multiple files");
    assert_eq!(
        SdJournalNext(&mut filtered_multi).expect("filtered first"),
        1
    );
    let entry = SdJournalGetEntry(&mut filtered_multi).expect("filtered first entry");
    assert_eq!(entry.get_str("MESSAGE"), Some("first"));
    // The first unfiltered step caches candidates from other files; match
    // mutation must discard those cached candidates before continuing.
    SdJournalAddMatch(&mut filtered_multi, b"MESSAGE=second").expect("filtered add match");
    assert_eq!(
        SdJournalNext(&mut filtered_multi).expect("filtered next"),
        1
    );
    let entry = SdJournalGetEntry(&mut filtered_multi).expect("filtered entry");
    assert_eq!(entry.get_str("MESSAGE"), Some("second"));
    assert_eq!(SdJournalNext(&mut filtered_multi).expect("filtered end"), 0);
}

fn facade_second_journal_path(dir: &tempfile::TempDir) -> PathBuf {
    let path = dir.path().join("journals/user.journal");
    if !path.exists() {
        write_facade_single_message_journal(&path, b"third", 1002);
    }
    path
}

#[test]
fn jf_facade_data_enumeration_handles_compressed_payloads() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/system.journal");
    let (mut journal_file, mut writer) = create_facade_compressed_test_writer(&path);
    let compressed_payload = format!("MESSAGE={}", "compressed ".repeat(128));
    writer
        .add_entry(
            &mut journal_file,
            &[compressed_payload.as_bytes()],
            1000,
            11,
        )
        .expect("write compressed entry");
    journal_file.sync().expect("sync compressed journal");

    let mut journal =
        SdJournalOpenFiles(&[path.to_str().expect("utf8 path")], 0).expect("open files");
    assert_eq!(SdJournalNext(&mut journal).expect("next"), 1);
    SdJournalRestartData(&mut journal).expect("restart data");
    let mut payloads = Vec::new();
    while let Some(payload) = SdJournalEnumerateAvailableData(&mut journal).expect("enumerate data")
    {
        payloads.push(payload.to_vec());
    }

    assert_eq!(payloads, vec![compressed_payload.into_bytes()]);
}

#[test]
fn jf_facade_unique_state_handles_compressed_payloads_and_restart() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/system.journal");
    let (mut journal_file, mut writer) = create_facade_compressed_test_writer(&path);
    let compressed_payload = format!("MESSAGE={}", "compressed unique ".repeat(128));
    writer
        .add_entry(
            &mut journal_file,
            &[compressed_payload.as_bytes()],
            1000,
            11,
        )
        .expect("write compressed entry");
    journal_file.sync().expect("sync compressed journal");

    let mut journal =
        SdJournalOpenFiles(&[path.to_str().expect("utf8 path")], 0).expect("open files");
    SdJournalQueryUniqueState(&mut journal, "NO_SUCH_FIELD").expect("query missing unique field");
    assert_eq!(
        SdJournalEnumerateAvailableUnique(&mut journal).expect("enumerate missing unique"),
        None
    );

    SdJournalQueryUniqueState(&mut journal, "MESSAGE").expect("query unique state");
    let first = SdJournalEnumerateAvailableUnique(&mut journal)
        .expect("enumerate compressed unique")
        .expect("compressed unique exists");
    assert_eq!(first, compressed_payload.as_bytes());
    assert_eq!(
        SdJournalEnumerateAvailableUnique(&mut journal).expect("enumerate unique end"),
        None
    );

    SdJournalRestartUnique(&mut journal).expect("restart compressed unique");
    let restarted = SdJournalEnumerateAvailableUnique(&mut journal)
        .expect("enumerate restarted compressed unique")
        .expect("restarted compressed unique exists");
    assert_eq!(restarted, compressed_payload.as_bytes());
}

#[test]
fn reader_preserves_raw_byte_field_names() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/raw-byte-names.journal");
    let invalid_utf8_name = invalid_utf8_raw_name();
    let nul_name = nul_raw_name();
    write_raw_byte_name_journal(&path, &invalid_utf8_name, nul_name);

    let mut reader = FileReader::open(&path).expect("open raw byte-name journal");
    assert!(reader.next().expect("read first entry"));
    let entry = reader.get_entry().expect("get raw byte-name entry");
    assert_raw_byte_name_accessors(&entry, &invalid_utf8_name, nul_name);
    assert_raw_byte_name_payload(&entry, &invalid_utf8_name);
    let lossy_name = String::from_utf8_lossy(&invalid_utf8_name).into_owned();
    assert_lossy_raw_name_is_not_invented(&entry, &lossy_name);
    assert_export_preserves_raw_byte_name(&entry, &invalid_utf8_name);
    assert_json_omits_lossy_raw_name(&entry, &lossy_name);
}

fn write_raw_byte_name_journal(path: &Path, invalid_utf8_name: &[u8], nul_name: &[u8]) {
    let (mut journal_file, mut writer) = create_facade_test_writer(path);
    let binary_value = [0x61_u8, 0, 0x3d, 0x62];
    writer
        .add_entry_fields_with_options(
            &mut journal_file,
            [
                journal_core::file::EntryField::structured(b"MESSAGE", b"raw byte names"),
                journal_core::file::EntryField::structured(invalid_utf8_name, b"invalid utf8"),
                journal_core::file::EntryField::structured(nul_name, b"nul name"),
                journal_core::file::EntryField::structured(b"field name", b"space"),
                journal_core::file::EntryField::structured(b"BINARY", &binary_value),
            ],
            1_700_004_000_000_000,
            1,
            journal_core::file::EntryWriteOptions::default()
                .field_name_policy(journal_core::file::FieldNamePolicy::Raw),
        )
        .expect("write raw byte-name entry");
    journal_file.sync().expect("sync raw byte-name journal");
}

fn invalid_utf8_raw_name() -> Vec<u8> {
    vec![0xff, 0x52, 0x41, 0x57]
}

fn nul_raw_name() -> &'static [u8] {
    &[0x52, 0x41, 0x57, 0, 0x4e, 0x41, 0x4d, 0x45]
}

fn binary_raw_value() -> &'static [u8] {
    &[0x61, 0, 0x3d, 0x62]
}

fn assert_raw_byte_name_accessors(entry: &Entry, invalid_utf8_name: &[u8], nul_name: &[u8]) {
    assert_eq!(entry.get("MESSAGE"), Some(b"raw byte names".as_slice()));
    assert_eq!(
        entry.get_raw(invalid_utf8_name),
        Some(b"invalid utf8".as_slice())
    );
    assert_eq!(entry.get_raw(nul_name), Some(b"nul name".as_slice()));
    assert_eq!(entry.get_raw(b"BINARY"), Some(binary_raw_value()));
    assert_eq!(
        entry.get_raw_values(b"field name"),
        vec![b"space".as_slice()]
    );
}

fn assert_raw_byte_name_payload(entry: &Entry, invalid_utf8_name: &[u8]) {
    assert!(
        entry
            .raw_fields()
            .any(|field| { field.name == invalid_utf8_name && field.value == b"invalid utf8" })
    );
    assert!(entry.payloads.iter().any(|payload| {
        let mut expected = invalid_utf8_name.to_vec();
        expected.push(b'=');
        expected.extend_from_slice(b"invalid utf8");
        payload == &expected
    }));
}

fn assert_lossy_raw_name_is_not_invented(entry: &Entry, lossy_name: &str) {
    assert!(
        !entry.fields.contains_key(lossy_name),
        "string convenience map must not invent lossy RAW field names"
    );
}

fn assert_export_preserves_raw_byte_name(entry: &Entry, invalid_utf8_name: &[u8]) {
    let export = export_entry_bytes(&entry);
    let mut expected_export = invalid_utf8_name.to_vec();
    expected_export.push(b'=');
    expected_export.extend_from_slice(b"invalid utf8\n");
    assert!(
        export
            .windows(expected_export.len())
            .any(|window| window == expected_export.as_slice()),
        "export output should preserve non-UTF8 RAW field names as bytes"
    );
}

fn assert_json_omits_lossy_raw_name(entry: &Entry, lossy_name: &str) {
    let serde_json::Value::Object(json) = json_entry(&entry) else {
        panic!("entry JSON should be an object");
    };
    assert!(
        !json.contains_key(lossy_name),
        "JSON output must not invent lossy RAW field names"
    );
}
