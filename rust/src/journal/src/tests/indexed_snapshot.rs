use super::*;
use journal_core::file::{EntryField, EntryWriteOptions};

fn fixture(
    compact: bool,
    compression: Compression,
) -> (
    tempfile::TempDir,
    PathBuf,
    JournalFile<MmapMut>,
    JournalWriter,
) {
    fixture_with_strategy(compact, compression, ExperimentalMmapStrategy::Windowed)
}

fn fixture_with_strategy(
    compact: bool,
    compression: Compression,
    strategy: ExperimentalMmapStrategy,
) -> (
    tempfile::TempDir,
    PathBuf,
    JournalFile<MmapMut>,
    JournalWriter,
) {
    let dir = tempfile::tempdir().unwrap();
    let parent = dir.path().join("01010101010101010101010101010101");
    std::fs::create_dir(&parent).unwrap();
    let path = parent.join("system.journal");
    let file = RepoFile::from_path(&path).unwrap();
    let mut journal = JournalFile::<MmapMut>::create(
        &file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_compact(compact)
            .with_experimental_mmap_strategy(strategy)
            .with_compression(compression)
            .with_compress_threshold(8),
    )
    .unwrap();
    let writer = JournalWriter::new(&mut journal, 1, test_uuid(2)).unwrap();
    (dir, path, journal, writer)
}

fn append(journal: &mut JournalFile<MmapMut>, writer: &mut JournalWriter, index: u64) {
    let text = format!("MESSAGE={} {}", index, "repeated text ".repeat(100));
    writer
        .add_entry_fields_with_options(
            journal,
            [
                EntryField::raw(b"SCHEMA=1"),
                EntryField::raw(b"BUCKET=a"),
                EntryField::raw(b"BUCKET=b"),
                EntryField::raw(b"\xff=\x00\xfe"),
                EntryField::raw(text.as_bytes()),
            ],
            1_000_000 + index,
            index,
            EntryWriteOptions::default().field_name_policy(FieldNamePolicy::Raw),
        )
        .unwrap();
}

#[test]
fn indexed_snapshot_bounds_postings_values_and_payload_lifetimes() {
    for compact in [false, true] {
        for compression in [
            Compression::None,
            Compression::Zstd,
            Compression::Lz4,
            Compression::Xz,
        ] {
            let (_dir, path, mut journal, mut writer) = fixture(compact, compression);
            for index in 1..=17 {
                append(&mut journal, &mut writer, index);
            }
            let control = SnapshotControl::default();
            verify_index(&path, &control).unwrap();
            let mut snapshot = IndexedSnapshot::open(
                &path,
                IndexedSnapshotOptions {
                    reader: ReaderOptions::snapshot().with_window_size(4096),
                    capture_fields: vec![b"BUCKET".to_vec(), b"ABSENT".to_vec()],
                    capture_values: vec![
                        (b"SCHEMA".to_vec(), b"1".to_vec()),
                        (b"SCHEMA".to_vec(), b"absent".to_vec()),
                    ],
                },
                &control,
            )
            .unwrap();
            for index in 18..=400 {
                append(&mut journal, &mut writer, index);
            }
            assert_snapshot_bounds_and_payloads(&mut snapshot, &control);
            assert_snapshot_callback_errors(&mut snapshot, &control);
        }
    }
}

fn assert_snapshot_bounds_and_payloads(
    snapshot: &mut IndexedSnapshot,
    control: &SnapshotControl<'_>,
) {
    assert_eq!(snapshot.entry_count(), 17);
    assert_eq!(
        snapshot.captured_value(b"SCHEMA", b"1").unwrap(),
        CapturedValue {
            present: true,
            entry_count: 17
        }
    );
    assert!(
        !snapshot
            .captured_value(b"SCHEMA", b"absent")
            .unwrap()
            .present
    );
    assert!(snapshot.captured_value(b"SCHEMA", b"undeclared").is_err());
    let mut seen = Vec::new();
    snapshot
        .visit_match(b"SCHEMA", b"1", control, |entry| {
            seen.push(entry.metadata().seqnum);
            let mut binary = false;
            entry.visit_payloads(|payload| {
                binary |= payload == b"\xff=\x00\xfe";
                Ok(())
            })?;
            assert!(binary);
            Ok(())
        })
        .unwrap();
    assert_eq!(seen, (1..=17).collect::<Vec<_>>());
    seen.clear();
    snapshot
        .visit_entries(control, |entry| {
            seen.push(entry.metadata().seqnum);
            Ok(())
        })
        .unwrap();
    assert_eq!(seen, (1..=17).collect::<Vec<_>>());
    let mut count = 0;
    snapshot
        .visit_field(
            b"BUCKET",
            control,
            |_| Ok(true),
            |_| {
                count += 1;
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(count, 34, "multivalued rows deliberately repeat");
    snapshot
        .visit_field(b"ABSENT", control, |_| panic!(), |_| panic!())
        .unwrap();
    assert!(
        snapshot
            .visit_field(b"UNDECLARED", control, |_| Ok(true), |_| Ok(()))
            .is_err()
    );
}

fn assert_snapshot_callback_errors(snapshot: &mut IndexedSnapshot, control: &SnapshotControl<'_>) {
    let cancelled = || true;
    assert!(matches!(
        snapshot.visit_entries(
            &SnapshotControl {
                cancelled: Some(&cancelled)
            },
            |_| Ok(())
        ),
        Err(SdkError::Cancelled)
    ));
    let mut count = 0;
    assert!(
        snapshot
            .visit_match(b"BUCKET", b"a", control, |_| {
                count += 1;
                Err(SdkError::Unsupported("callback"))
            })
            .is_err()
    );
    assert_eq!(count, 1);
}

#[test]
fn indexed_empty_snapshot_stays_empty_after_append() {
    let (_dir, path, mut journal, mut writer) = fixture(false, Compression::None);
    let control = SnapshotControl::default();
    let mut snapshot =
        IndexedSnapshot::open(&path, IndexedSnapshotOptions::default(), &control).unwrap();
    append(&mut journal, &mut writer, 1);
    snapshot
        .visit_entries(&control, |_| panic!("empty snapshot grew"))
        .unwrap();
    snapshot
        .visit_match(b"SCHEMA", b"1", &control, |_| panic!("empty snapshot grew"))
        .unwrap();
}

#[test]
fn strict_index_rejects_last_entry_missing_reverse_link() {
    for compact in [false, true] {
        let (_dir, path, mut journal, mut writer) = fixture(compact, Compression::None);
        for index in 1..=3 {
            append(&mut journal, &mut writer, index);
        }
        let offset = journal
            .find_data_offset(journal.hash(b"SCHEMA=1"), b"SCHEMA=1")
            .unwrap()
            .unwrap();
        {
            let mut data = journal.data_mut(offset, None).unwrap();
            data.header.n_entries = NonZeroU64::new(2);
        }
        journal.sync().unwrap();
        assert!(verify_index(&path, &SnapshotControl::default()).is_err());
    }
}

#[test]
fn writer_failure_poison_preserves_bytes_and_prevalidation_reuses() {
    for compact in [false, true] {
        let (_dir, path, mut journal, mut writer) = fixture(compact, Compression::None);
        assert!(writer.add_entry(&mut journal, &[b"invalid"], 1, 1).is_err());
        assert!(!writer.is_poisoned());
        append(&mut journal, &mut writer, 1);
        assert!(
            writer
                .add_entry(&mut journal, &[b"NEW=data", b"invalid"], 2, 2)
                .is_err()
        );
        assert!(writer.is_poisoned());
        let before = std::fs::read(&path).unwrap();
        assert!(matches!(
            writer.add_entry(&mut journal, &[b"MESSAGE=retry"], 3, 3),
            Err(JournalError::WriterPoisoned)
        ));
        drop(writer);
        drop(journal);
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
}

#[test]
fn indexed_historical_and_whole_file_compressed_fixtures() {
    let control = SnapshotControl::default();
    let dir = repo_root().join("fixtures/systemd/test-data/no-rtc");
    for item in std::fs::read_dir(dir).unwrap() {
        let path = item.unwrap().path();
        if path.file_name().unwrap() != "system.journal.zst" {
            continue;
        }
        let mut snapshot =
            IndexedSnapshot::open(&path, IndexedSnapshotOptions::default(), &control)
                .unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        let mut count = 0;
        snapshot
            .visit_entries(&control, |entry| {
                entry.visit_payloads(|_| Ok(()))?;
                count += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(count, snapshot.entry_count());
        verify_index(&path, &control)
            .unwrap_or_else(|err| panic!("strict {}: {err}", path.display()));
    }
}

#[test]
fn indexed_capture_rejects_unpublished_postings_and_tail_hints() {
    for compact in [false, true] {
        let (_dir, path, mut journal, mut writer) = fixture(compact, Compression::None);
        for index in 1..=5 {
            append(&mut journal, &mut writer, index);
        }
        let before = *journal.journal_header_ref();
        append(&mut journal, &mut writer, 6);
        // Deterministic interruption at the last header publication step:
        // DATA links and tail hints are published, but global count is still old.
        journal.journal_header_mut().n_entries = before.n_entries;
        let options = IndexedSnapshotOptions {
            capture_values: vec![(b"SCHEMA".to_vec(), b"1".to_vec())],
            ..Default::default()
        };
        assert!(
            IndexedSnapshot::open(&path, options.clone(), &SnapshotControl::default()).is_err()
        );
        let header = journal.journal_header_mut();
        header.tail_entry_offset = before.tail_entry_offset;
        header.tail_entry_seqnum = before.tail_entry_seqnum;
        header.tail_entry_realtime = before.tail_entry_realtime;
        header.tail_entry_monotonic = before.tail_entry_monotonic;
        header.tail_entry_array_offset = before.tail_entry_array_offset;
        header.tail_entry_array_n_entries = before.tail_entry_array_n_entries;
        assert!(IndexedSnapshot::open(&path, options, &SnapshotControl::default()).is_err());
        assert!(verify_index(&path, &SnapshotControl::default()).is_err());
    }
}

#[test]
fn indexed_snapshot_survives_concurrent_append_and_array_growth() {
    for compact in [false, true] {
        for initial in [1, 2, 5, 512] {
            let (ready_tx, ready_rx) = std::sync::mpsc::channel();
            let (resume_tx, resume_rx) = std::sync::mpsc::channel();
            let (appended_tx, appended_rx) = std::sync::mpsc::channel();
            let (done_tx, done_rx) = std::sync::mpsc::channel();
            let thread = std::thread::spawn(move || {
                let (_dir, path, mut journal, mut writer) = fixture(compact, Compression::Zstd);
                for index in 1..=initial {
                    append(&mut journal, &mut writer, index);
                }
                ready_tx.send(path).unwrap();
                for batch in 0..8 {
                    resume_rx.recv().unwrap();
                    for index in initial + batch * 512 + 1..=initial + (batch + 1) * 512 {
                        append(&mut journal, &mut writer, index);
                    }
                    appended_tx.send(()).unwrap();
                }
                done_rx.recv().unwrap();
            });
            let path = ready_rx.recv().unwrap();
            let control = SnapshotControl::default();
            let mut snapshot = IndexedSnapshot::open(
                &path,
                IndexedSnapshotOptions {
                    reader: ReaderOptions::snapshot().with_window_size(4096),
                    capture_fields: vec![b"MESSAGE".to_vec()],
                    capture_values: vec![(b"SCHEMA".to_vec(), b"1".to_vec())],
                    ..Default::default()
                },
                &control,
            )
            .unwrap();
            for _ in 0..8 {
                let mut seqnums = Vec::new();
                snapshot
                    .visit_match(b"SCHEMA", b"1", &control, |entry| {
                        if seqnums.is_empty() {
                            // The writer cannot finish this batch before traversal
                            // starts; keep this entry alive across append/remap.
                            resume_tx.send(()).unwrap();
                            entry.visit_payloads(|_| Ok(()))?;
                            appended_rx
                                .recv_timeout(std::time::Duration::from_secs(30))
                                .unwrap();
                        }
                        entry.visit_payloads(|_| Ok(()))?;
                        seqnums.push(entry.metadata().seqnum);
                        Ok(())
                    })
                    .unwrap();
                assert_eq!(seqnums, (1..=initial).collect::<Vec<_>>());
                let mut count = 0;
                snapshot
                    .visit_field(
                        b"MESSAGE",
                        &control,
                        |_| Ok(true),
                        |_| {
                            count += 1;
                            Ok(())
                        },
                    )
                    .unwrap();
                assert_eq!(count, initial);
            }
            done_tx.send(()).unwrap();
            thread.join().unwrap();
        }
    }
}

#[test]
fn strict_index_rejects_empty_entry_metadata_and_field_orphans() {
    let (_dir, path, mut journal, mut writer) = fixture(false, Compression::None);
    journal.journal_header_mut().tail_entry_realtime = 1;
    assert!(verify_index(&path, &SnapshotControl::default()).is_err());
    journal.journal_header_mut().tail_entry_realtime = 0;
    append(&mut journal, &mut writer, 1);
    let field = journal
        .find_field_offset(journal.hash(b"SCHEMA"), b"SCHEMA")
        .unwrap()
        .unwrap();
    journal
        .field_mut(field, None)
        .unwrap()
        .header
        .head_data_offset = None;
    assert!(verify_index(&path, &SnapshotControl::default()).is_err());
}

#[test]
fn indexed_snapshot_legacy_header_boot_metadata() {
    for modern in [false, true] {
        let (_dir, path, mut journal, mut writer) = fixture(false, Compression::None);
        for index in 1..=3 {
            append(&mut journal, &mut writer, index);
        }
        let header = journal.journal_header_mut();
        if !modern {
            header.compatible_flags &= !2;
        }
        header.tail_entry_boot_id = *test_uuid(7).as_bytes();
        header.tail_entry_monotonic = 999;
        let control = SnapshotControl::default();
        let result = IndexedSnapshot::open(&path, IndexedSnapshotOptions::default(), &control);
        if modern {
            assert!(result.is_err());
            continue;
        }
        let mut snapshot = result.unwrap();
        verify_index(&path, &control).unwrap();
        snapshot
            .visit_entries(&control, |entry| {
                assert_eq!(entry.metadata().monotonic, entry.metadata().seqnum);
                assert_eq!(entry.metadata().boot_id, *test_uuid(2).as_bytes());
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn strict_index_rejects_hash_tables_outside_object_tail() {
    for no_objects in [false, true] {
        let (_dir, path, mut journal, _writer) = fixture(false, Compression::None);
        let control = SnapshotControl::default();
        verify_index(&path, &control).unwrap();
        let header = journal.journal_header_mut();
        header.tail_object_offset = if no_objects {
            None
        } else {
            NonZeroU64::new(header.field_hash_table_offset.unwrap().get() - 16)
        };
        header.n_objects = if no_objects { 0 } else { 1 };
        assert!(
            verify_index(&path, &control).is_err(),
            "uncommitted hash table was certified"
        );
    }
}

#[test]
fn indexed_snapshot_archived_state_is_frozen() {
    use journal_log_writer::{Config, EntryTimestamps, Log, RetentionPolicy, RotationPolicy};
    use journal_registry::{Origin, Source};

    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(
        Origin {
            machine_id: Some(test_uuid(1)),
            namespace: None,
            source: Source::System,
        },
        RotationPolicy::default(),
        RetentionPolicy::default(),
    )
    .with_boot_id(test_uuid(2));
    let mut log = Log::new(dir.path(), config).unwrap();
    log.write_entry_with_timestamps(
        &[b"MESSAGE=archive-state"],
        EntryTimestamps::default()
            .with_entry_realtime_usec(1_000_000)
            .with_entry_monotonic_usec(1),
    )
    .unwrap();
    let path = log.active_path().unwrap().to_path_buf();
    let control = SnapshotControl::default();
    let active = IndexedSnapshot::open(&path, Default::default(), &control).unwrap();
    assert!(!active.is_archived());
    log.close().unwrap();
    assert!(!active.is_archived(), "archive changed captured state");
    let archived = IndexedSnapshot::open(&path, Default::default(), &control).unwrap();
    assert!(archived.is_archived());
}

#[test]
fn indexed_snapshot_offline_is_not_archived() {
    let (_dir, path, mut journal, mut writer) = fixture(false, Compression::None);
    append(&mut journal, &mut writer, 1);
    // The low-level format API exposes the offline state directly; the
    // high-level Log lifecycle always closes nonempty journals as archived.
    journal.journal_header_mut().state = journal_core::file::JournalState::Offline as u8;
    let snapshot =
        IndexedSnapshot::open(&path, Default::default(), &SnapshotControl::default()).unwrap();
    assert!(!snapshot.is_archived());
}

#[test]
fn strict_index_accepts_empty_indexless_journal() {
    for compact in [false, true] {
        let (_dir, path, mut journal, _writer) = fixture(compact, Compression::None);
        let header = journal.journal_header_mut();
        header.data_hash_table_offset = None;
        header.data_hash_table_size = None;
        header.field_hash_table_offset = None;
        header.field_hash_table_size = None;
        header.tail_object_offset = None;
        header.n_objects = 0;
        verify_index(&path, &SnapshotControl::default()).unwrap();
    }
}

#[test]
fn strict_index_empty_boot_metadata_respects_compatible_flag() {
    let (_dir, path, mut journal, _writer) = fixture(false, Compression::None);
    journal.journal_header_mut().tail_entry_boot_id = *test_uuid(7).as_bytes();
    let control = SnapshotControl::default();
    assert!(verify_index(&path, &control).is_err());
    assert!(IndexedSnapshot::open(&path, IndexedSnapshotOptions::default(), &control).is_err());
    journal.journal_header_mut().compatible_flags &= !2;
    verify_index(&path, &control).unwrap();
    IndexedSnapshot::open(&path, IndexedSnapshotOptions::default(), &control).unwrap();
}

#[test]
fn indexed_queries_reject_missing_captured_postings() {
    use std::io::{Seek, SeekFrom, Write};
    for compact in [false, true] {
        for damage in ["first array", "slot", "continuation"] {
            let (_dir, path, mut journal, mut writer) = fixture(compact, Compression::None);
            for index in 1..=6 {
                append(&mut journal, &mut writer, index);
            }
            let data = journal
                .find_data_offset(journal.hash(b"SCHEMA=1"), b"SCHEMA=1")
                .unwrap()
                .unwrap();
            let array = journal
                .data_ref(data)
                .unwrap()
                .header
                .entry_array_offset
                .unwrap();
            match damage {
                "first array" => {
                    journal
                        .data_mut(data, None)
                        .unwrap()
                        .header
                        .entry_array_offset = None
                }
                "continuation" => {
                    journal
                        .offset_array_mut(array, None)
                        .unwrap()
                        .header
                        .next_offset_array = None
                }
                _ => {
                    let mut bytes = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
                    bytes.seek(SeekFrom::Start(array.get() + 24)).unwrap();
                    bytes
                        .write_all(if compact { &[0; 4] } else { &[0; 8] })
                        .unwrap();
                }
            }
            let control = SnapshotControl::default();
            let mut snapshot = IndexedSnapshot::open(
                &path,
                IndexedSnapshotOptions {
                    capture_fields: vec![b"SCHEMA".to_vec()],
                    ..Default::default()
                },
                &control,
            )
            .unwrap();
            assert!(
                snapshot
                    .visit_match(b"SCHEMA", b"1", &control, |_| Ok(()))
                    .is_err(),
                "accepted missing {damage}"
            );
            assert!(
                snapshot
                    .visit_field(b"SCHEMA", &control, |_| Ok(true), |_| Ok(()))
                    .is_err(),
                "FIELD accepted missing {damage}"
            );
        }
    }
}

#[test]
fn strict_index_rejects_missing_or_duplicate_populated_tables() {
    use std::io::{Seek, SeekFrom, Write};
    for compact in [false, true] {
        for duplicate in [false, true] {
            let (_dir, path, mut journal, mut writer) = fixture(compact, Compression::None);
            append(&mut journal, &mut writer, 1);
            let header = journal.journal_header_mut();
            if duplicate {
                // Reclassify the FIELD table as a second DATA table.
                let offset = header.field_hash_table_offset.unwrap().get() - 16;
                let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
                file.seek(SeekFrom::Start(offset)).unwrap();
                file.write_all(&[4]).unwrap();
            } else {
                header.data_hash_table_offset = None;
                header.data_hash_table_size = None;
            }
            assert!(verify_index(&path, &SnapshotControl::default()).is_err());
        }
    }
}

const DECLARED_ARENA_DAMAGE: [&str; 6] = [
    "zero",
    "tail-header",
    "tail-object",
    "hash-extent",
    "beyond-file",
    "overflow",
];

fn damage_declared_arena(journal: &mut JournalFile<MmapMut>, damage: &str) {
    let file_size = journal.reader_file_size().unwrap();
    let tail = journal.journal_header_ref().tail_object_offset.unwrap();
    let tail_size = journal.object_header_ref(tail).unwrap().size;
    let header = journal.journal_header_mut();
    header.arena_size = match damage {
        "beyond-file" => file_size + 8 - header.header_size,
        "overflow" => u64::MAX,
        "zero" => 0,
        "tail-header" => tail.get() + 8 - header.header_size,
        "tail-object" => tail.get() + tail_size - 1 - header.header_size,
        "hash-extent" => {
            let end = (tail.get() + tail_size + 7) & !7;
            header.data_hash_table_size =
                NonZeroU64::new(end - header.data_hash_table_offset.unwrap().get() + 16);
            end - header.header_size
        }
        _ => unreachable!(),
    };
}

#[test]
fn strict_index_rejects_objects_outside_declared_arena() {
    for compact in [false, true] {
        for damage in DECLARED_ARENA_DAMAGE {
            let (_dir, path, mut journal, mut writer) = fixture(compact, Compression::None);
            append(&mut journal, &mut writer, 1);
            damage_declared_arena(&mut journal, damage);
            let before = std::fs::read(&path).unwrap();
            assert!(
                verify_index(&path, &SnapshotControl::default()).is_err(),
                "compact={compact} damage={damage}"
            );
            assert_eq!(before, std::fs::read(&path).unwrap());
        }
    }
}

#[test]
fn snapshot_rejects_objects_outside_declared_arena() {
    for compact in [false, true] {
        for damage in DECLARED_ARENA_DAMAGE {
            let (_dir, path, mut journal, mut writer) = fixture(compact, Compression::None);
            append(&mut journal, &mut writer, 1);
            damage_declared_arena(&mut journal, damage);
            let before = std::fs::read(&path).unwrap();
            assert!(
                IndexedSnapshot::open(
                    &path,
                    IndexedSnapshotOptions::default(),
                    &SnapshotControl::default()
                )
                .is_err(),
                "compact={compact} damage={damage}"
            );
            assert_eq!(before, std::fs::read(&path).unwrap());
        }
    }
}

#[test]
fn append_open_rejects_objects_outside_declared_arena_without_mutation() {
    for compact in [false, true] {
        for damage in DECLARED_ARENA_DAMAGE {
            let (_dir, path, mut journal, mut writer) = fixture(compact, Compression::None);
            append(&mut journal, &mut writer, 1);
            damage_declared_arena(&mut journal, damage);
            drop(writer);
            drop(journal);
            let before = std::fs::read(&path).unwrap();
            let result = JournalFile::open_for_append(&RepoFile::from_path(&path).unwrap(), 4096);
            let rejected = result.is_err();
            drop(result);
            assert_eq!(before, std::fs::read(&path).unwrap());
            assert!(rejected, "compact={compact} damage={damage}");
        }
    }
}

#[test]
fn writer_construction_rejects_objects_outside_declared_arena_without_mutation() {
    for compact in [false, true] {
        for damage in DECLARED_ARENA_DAMAGE {
            let (_dir, path, mut journal, mut writer) = fixture(compact, Compression::None);
            append(&mut journal, &mut writer, 1);
            drop(writer);
            damage_declared_arena(&mut journal, damage);
            let before = std::fs::read(&path).unwrap();
            assert!(
                JournalWriter::new(&mut journal, 2, test_uuid(2)).is_err(),
                "compact={compact} damage={damage}"
            );
            assert_eq!(before, std::fs::read(&path).unwrap());
        }
    }
}

#[test]
fn strict_index_allows_empty_inherited_sequence() {
    for compact in [false, true] {
        let (_dir, path, mut journal, _writer) = fixture(compact, Compression::None);
        journal.journal_header_mut().tail_entry_seqnum = 42;
        verify_index(&path, &SnapshotControl::default()).unwrap();
    }
}

#[test]
fn empty_inherited_sequence_snapshot_stays_frozen_after_append() {
    for compact in [false, true] {
        let (_dir, path, mut journal, writer) = fixture(compact, Compression::None);
        drop(writer);
        journal.journal_header_mut().tail_entry_seqnum = 42;
        let control = SnapshotControl::default();
        let mut snapshot = IndexedSnapshot::open(
            &path,
            IndexedSnapshotOptions {
                capture_fields: vec![b"SCHEMA".to_vec()],
                capture_values: vec![(b"SCHEMA".to_vec(), b"1".to_vec())],
                ..Default::default()
            },
            &control,
        )
        .unwrap();
        assert_eq!(snapshot.entry_count(), 0);
        let mut writer = JournalWriter::new(&mut journal, 43, test_uuid(2)).unwrap();
        append(&mut journal, &mut writer, 1);
        assert_eq!(journal.journal_header_ref().tail_entry_seqnum, 43);
        verify_index(&path, &control).unwrap();
        snapshot
            .visit_entries(&control, |_| panic!("empty snapshot grew"))
            .unwrap();
        snapshot
            .visit_match(b"SCHEMA", b"1", &control, |_| panic!("empty snapshot grew"))
            .unwrap();
        snapshot
            .visit_field(
                b"SCHEMA",
                &control,
                |_| Ok(true),
                |_| panic!("empty snapshot grew"),
            )
            .unwrap();
        assert_eq!(
            snapshot.captured_value(b"SCHEMA", b"1").unwrap(),
            CapturedValue::default()
        );
    }
}

#[test]
fn strict_reuse_guard_preserves_invalid_arena_bytes() {
    for compact in [false, true] {
        for damage in DECLARED_ARENA_DAMAGE {
            let (_dir, path, mut journal, mut writer) = fixture(compact, Compression::None);
            append(&mut journal, &mut writer, 1);
            damage_declared_arena(&mut journal, damage);
            drop(writer);
            drop(journal);
            let before = std::fs::read(&path).unwrap();
            let reuse = (|| -> crate::Result<()> {
                verify_index(&path, &SnapshotControl::default())?;
                let mut file =
                    JournalFile::open_for_append(&RepoFile::from_path(&path).unwrap(), 4096)?;
                file.sync()?;
                Ok(())
            })();
            assert!(reuse.is_err());
            assert_eq!(
                before,
                std::fs::read(&path).unwrap(),
                "compact={compact} damage={damage}"
            );
        }
    }
}

#[test]
fn declared_arena_may_exclude_physical_padding() {
    for compact in [false, true] {
        let (_dir, path, mut journal, mut writer) = fixture(compact, Compression::None);
        append(&mut journal, &mut writer, 1);
        let tail = journal.journal_header_ref().tail_object_offset.unwrap();
        let end = (tail.get() + journal.object_header_ref(tail).unwrap().size + 7) & !7;
        let header = journal.journal_header_mut();
        header.arena_size = end - header.header_size;
        assert!(std::fs::metadata(&path).unwrap().len() > end);
        let control = SnapshotControl::default();
        verify_index(&path, &control).unwrap();
        let mut snapshot =
            IndexedSnapshot::open(&path, IndexedSnapshotOptions::default(), &control).unwrap();
        let mut rows = 0;
        snapshot
            .visit_entries(&control, |_| {
                rows += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(rows, 1);
        drop(writer);
        drop(journal);
        let mut reopened =
            JournalFile::open_for_append(&RepoFile::from_path(&path).unwrap(), 4096).unwrap();
        let mut writer = JournalWriter::new(&mut reopened, 2, test_uuid(2)).unwrap();
        append(&mut reopened, &mut writer, 2);
        verify_index(&path, &control).unwrap();
    }
}

#[test]
fn empty_metadata_respects_historical_field_presence() {
    for compact in [false, true] {
        for header_size in [208, 240, 248, 256, 264, 272] {
            let (_dir, path, mut journal, writer) = fixture(compact, Compression::None);
            journal.journal_header_mut().tail_entry_seqnum = 42;
            journal.journal_header_mut().tail_entry_boot_id = *test_uuid(7).as_bytes();
            drop(writer);
            drop(journal);
            let original = std::fs::read(&path).unwrap();
            let shift = 272 - header_size;
            let mut bytes = original[..header_size].to_vec();
            bytes.extend_from_slice(&original[272..]);
            bytes[88..96].copy_from_slice(&(header_size as u64).to_le_bytes());
            for location in [104, 120, 136] {
                let old = u64::from_le_bytes(bytes[location..location + 8].try_into().unwrap());
                bytes[location..location + 8].copy_from_slice(&(old - shift as u64).to_le_bytes());
            }
            std::fs::write(&path, bytes).unwrap();
            let control = SnapshotControl::default();
            let strict = verify_index(&path, &control);
            let snapshot =
                IndexedSnapshot::open(&path, IndexedSnapshotOptions::default(), &control);
            if header_size < 272 {
                strict.unwrap();
                let mut snapshot = snapshot.unwrap();
                assert_eq!(snapshot.entry_count(), 0);
                snapshot
                    .visit_entries(&control, |_| panic!("historical empty snapshot grew"))
                    .unwrap();
            } else {
                assert!(strict.is_err());
                assert!(snapshot.is_err());
            }
            // Compatibility verification retains its existing empty-metadata tolerance.
            verify_file(&path).unwrap();
        }
    }
}

#[test]
fn strict_empty_metadata_rejects_current_file_head_sequence() {
    for compact in [false, true] {
        let (_dir, path, mut journal, _writer) = fixture(compact, Compression::None);
        journal.journal_header_mut().tail_entry_seqnum = 42;
        journal.journal_header_mut().head_entry_seqnum = 1;
        let control = SnapshotControl::default();
        assert!(verify_index(&path, &control).is_err());
        assert!(IndexedSnapshot::open(&path, IndexedSnapshotOptions::default(), &control).is_err());
        verify_file(&path).unwrap();
    }
}

#[test]
fn compact_final_object_padding_may_lie_outside_declared_arena() {
    let (_dir, path, mut journal, mut writer) = fixture(true, Compression::None);
    for index in 1..=3 {
        append(&mut journal, &mut writer, index);
    }
    let tail = journal.journal_header_ref().tail_object_offset.unwrap();
    let size = journal.object_header_ref(tail).unwrap().size;
    assert_ne!(size % 8, 0, "fixture needs unaligned compact ENTRY size");
    let header = journal.journal_header_mut();
    header.arena_size = tail.get() + size - header.header_size;
    let control = SnapshotControl::default();
    verify_index(&path, &control).unwrap();
    let mut snapshot =
        IndexedSnapshot::open(&path, IndexedSnapshotOptions::default(), &control).unwrap();
    let mut rows = 0;
    snapshot
        .visit_entries(&control, |_| {
            rows += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(rows, 3);
}

fn reuse_preserves_physical_allocation(reopen: bool, sync: bool) {
    for strategy in [
        ExperimentalMmapStrategy::Windowed,
        ExperimentalMmapStrategy::WholeFile,
    ] {
        for compact in [false, true] {
            let (_dir, path, mut journal, mut writer) =
                fixture_with_strategy(compact, Compression::None, strategy);
            for index in 1..=3 {
                append(&mut journal, &mut writer, index);
            }
            let tail = journal.journal_header_ref().tail_object_offset.unwrap();
            let raw_end = tail.get() + journal.object_header_ref(tail).unwrap().size;
            let physical_size = std::fs::metadata(&path).unwrap().len();
            assert!(physical_size > raw_end);
            if compact {
                assert_ne!(raw_end % 8, 0);
            }
            let header = journal.journal_header_mut();
            header.arena_size = raw_end - header.header_size;
            drop(writer);
            if reopen {
                drop(journal);
                journal = JournalFile::open_for_append(&RepoFile::from_path(&path).unwrap(), 4096)
                    .unwrap();
            }
            let mut writer = JournalWriter::new(&mut journal, 4, test_uuid(2)).unwrap();
            if sync {
                journal.sync().unwrap();
            } else {
                journal.post_change().unwrap();
            }
            assert_eq!(
                std::fs::metadata(&path).unwrap().len(),
                physical_size,
                "compact={compact} reopen={reopen} sync={sync}"
            );
            verify_index(&path, &SnapshotControl::default()).unwrap();
            append(&mut journal, &mut writer, 4);
            journal.sync().unwrap();
            assert_eq!(std::fs::metadata(&path).unwrap().len(), physical_size);
            verify_index(&path, &SnapshotControl::default()).unwrap();
            let snapshot = IndexedSnapshot::open(
                &path,
                IndexedSnapshotOptions::default(),
                &SnapshotControl::default(),
            )
            .unwrap();
            assert_eq!(snapshot.entry_count(), 4);
        }
    }
}

#[test]
fn append_reopen_sync_preserves_physical_allocation() {
    reuse_preserves_physical_allocation(true, true);
}

#[test]
fn append_reopen_post_change_preserves_physical_allocation() {
    reuse_preserves_physical_allocation(true, false);
}

#[test]
fn existing_writer_sync_preserves_physical_allocation() {
    reuse_preserves_physical_allocation(false, true);
}

#[test]
fn existing_writer_post_change_preserves_physical_allocation() {
    reuse_preserves_physical_allocation(false, false);
}

#[test]
fn rejected_append_open_preserves_short_file_bytes() {
    for compact in [false, true] {
        for size in [8, 208, 271] {
            let (_dir, path, journal, writer) = fixture(compact, Compression::None);
            drop(writer);
            drop(journal);
            let mut bytes = std::fs::read(&path).unwrap();
            bytes.truncate(size);
            std::fs::write(&path, &bytes).unwrap();
            assert!(
                JournalFile::open_for_append(&RepoFile::from_path(&path).unwrap(), 4096).is_err()
            );
            assert_eq!(
                std::fs::read(&path).unwrap(),
                bytes,
                "compact={compact} size={size}"
            );
        }
    }
}

#[test]
fn rejected_reuse_preserves_truncated_tail_bytes() {
    for compact in [false, true] {
        for reopen in [false, true] {
            let (_dir, path, mut journal, mut writer) = fixture(compact, Compression::None);
            for index in 1..=3 {
                append(&mut journal, &mut writer, index);
            }
            let tail = journal
                .journal_header_ref()
                .tail_object_offset
                .unwrap()
                .get();
            let header = journal.journal_header_mut();
            header.arena_size = tail + 16 - header.header_size;
            drop(writer);
            journal.sync().unwrap();
            let mut bytes = std::fs::read(&path).unwrap();
            bytes.truncate((tail + 16) as usize);
            std::fs::write(&path, &bytes).unwrap();
            let file = RepoFile::from_path(&path).unwrap();
            if reopen {
                drop(journal);
                assert!(JournalFile::open_for_append(&file, 4096).is_err());
            } else {
                assert!(JournalWriter::new(&mut journal, 4, test_uuid(2)).is_err());
            }
            assert!(
                std::fs::read(&path).unwrap() == bytes,
                "bytes changed compact={compact} reopen={reopen}"
            );
        }
    }
}

// SDK writers before this check, like systemd's, stored a truncated 32-bit
// tail entry array hint once a regular file grew past 4 GiB. Paired with Go
// TestReadersIgnoreTailArrayHintAboveFourGiB.
#[test]
fn indexed_snapshot_ignores_tail_array_hint_above_four_gib() {
    use std::os::unix::fs::FileExt;
    for large in [false, true] {
        let (_dir, path, mut journal, mut writer) = fixture(false, Compression::None);
        for index in 1..=3 {
            append(&mut journal, &mut writer, index);
        }
        journal.sync().unwrap();
        drop(writer);
        drop(journal);
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        if large {
            let size = u64::from(u32::MAX) + 1 + 4096;
            file.set_len(size).unwrap();
            file.write_all_at(&(size - 272).to_le_bytes(), 96).unwrap();
        }
        // A truncated hint names an offset below the first entry array.
        file.write_all_at(&8u32.to_le_bytes(), 256).unwrap();
        drop(file);
        let result = IndexedSnapshot::open(
            &path,
            IndexedSnapshotOptions::default(),
            &SnapshotControl::default(),
        );
        assert_eq!(result.is_ok(), large, "large={large}: {:?}", result.err());
        if large {
            verify_index(&path, &SnapshotControl::default()).unwrap();
        }
    }
}
