use super::*;
use crate::file::MmapMut;
use crate::file::writer::JournalWriter;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn custom_object_implementations_can_omit_the_wire_type_hint() {
    struct Custom;
    impl JournalObject<&[u8]> for Custom {
        fn from_data(_: &[u8], _: bool) -> Option<Self> {
            Some(Self)
        }
    }
    assert!(<Custom as JournalObject<&[u8]>>::EXPECTED_TYPE.is_none());
}

#[test]
fn writer_counted_arrays_reject_revisit_before_mutation() {
    use std::num::NonZeroU64;
    for kind in 0..3 {
        for descending in [false, true] {
            let dir = TempDir::new().unwrap();
            let path = dir.path().join("system.journal");
            let repo = crate::repository::File::from_path(&path).unwrap();
            let mut file = JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .create::<MmapMut>(&repo)
                .unwrap();
            let mut writer = JournalWriter::new(&mut file, 1, test_uuid(2)).unwrap();
            let a = writer
                .allocate_new_array(&mut file, NonZeroU64::new(4).unwrap())
                .unwrap();
            let b = writer
                .allocate_new_array(&mut file, NonZeroU64::new(8).unwrap())
                .unwrap();
            file.offset_array_mut(a, None)
                .unwrap()
                .header
                .next_offset_array = Some(b);
            file.offset_array_mut(b, None)
                .unwrap()
                .header
                .next_offset_array = Some(a);
            let head = if descending { b } else { a };
            let missing_tail = NonZeroU64::new(1024).unwrap();
            let failed = match kind {
                0 => writer
                    .append_to_data_entry_array(&mut file, head, missing_tail, 13)
                    .is_err(),
                1 => writer.find_entry_array_tail(&file, head, 13).is_err(),
                _ => writer
                    .compute_entry_array_tail_entries(&file, head, missing_tail, 13)
                    .is_err(),
            };
            assert!(failed, "required revisit selected a repeated array");
            assert!(
                file.offset_array_ref(a)
                    .unwrap()
                    .get(1, 4)
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[test]
fn recovered_tail_cursor_requalification_tracks_its_new_position() {
    use crate::file::Mmap;
    use crate::file::filter::FilterExpr;
    use std::io::{Seek, SeekFrom, Write};

    for compact in [false, true] {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("system.journal");
        let repo = crate::repository::File::from_path(&path).unwrap();
        let mut writable = JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_compact(compact)
            .create::<MmapMut>(&repo)
            .unwrap();
        let mut writer = JournalWriter::new(&mut writable, 1, test_uuid(2)).unwrap();
        for i in 1..=3 {
            writer
                .add_entry(&mut writable, &[b"COMMON=all"], i * 1000, i)
                .unwrap();
        }
        let mut entries = Vec::new();
        writable.entry_offsets(&mut entries).unwrap();
        drop(writer);
        drop(writable);
        let mut file = JournalFile::<Mmap>::open_path(&path, 64 * 1024).unwrap();
        file.set_reader_recovery(true);
        let data = file
            .find_data_offset(file.hash(b"COMMON=all"), b"COMMON=all")
            .unwrap()
            .unwrap();
        let cursor = file
            .reader_data_cursor(data)
            .unwrap()
            .unwrap()
            .tail(&file)
            .unwrap();
        let mut disk = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        disk.seek(SeekFrom::Start(entries[2].get())).unwrap();
        disk.write_all(&[0; 16]).unwrap();
        assert!(
            file.recover_reader_tail(&JournalError::ObjectExceedsFileBounds)
                .unwrap()
        );
        assert_eq!(file.recovery_status().recovered_entries, 2);
        let cursor = cursor.for_reader(&file).unwrap().unwrap();
        let mut expression = FilterExpr::Match(data, cursor);
        assert_eq!(
            expression.previous(&file, entries[1]).unwrap(),
            Some(entries[1])
        );
    }
}

// Synthetic reconstruction exercises consistency after injected failures. It
// does not establish an upstream publication sequence or a repair contract.
fn filter_repair_retry(
    compact: bool,
    installed: bool,
    backward: bool,
    compose: bool,
    tail_error: bool,
    zero_data: bool,
) {
    use crate::file::Mmap;
    use crate::file::filter::{FilterExpr, JournalFilter, LogicalOp};
    use std::io::{Seek, SeekFrom, Write};

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("system.journal");
    let repo = crate::repository::File::from_path(&path).unwrap();
    let mut writable = JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
        .with_compact(compact)
        .with_data_hash_table_buckets(64)
        .with_field_hash_table_buckets(16)
        .create::<MmapMut>(&repo)
        .unwrap();
    let mut writer = JournalWriter::new(&mut writable, 1, test_uuid(2)).unwrap();
    writer
        .add_entry(&mut writable, &[b"COMMON=all"], 1_000, 1)
        .unwrap();
    writer
        .add_entry(&mut writable, &[b"COMMON=all", b"TARGET=yes"], 2_000, 2)
        .unwrap();
    let mut entries = Vec::new();
    writable.entry_offsets(&mut entries).unwrap();
    drop(writer);
    drop(writable);
    let original = std::fs::read(&path).unwrap();
    let mut file = JournalFile::<Mmap>::open_path(&path, 64 * 1024).unwrap();
    file.set_reader_recovery(true);
    let target = file
        .find_data_offset(file.hash(b"TARGET=yes"), b"TARGET=yes")
        .unwrap()
        .unwrap();
    assert!(target.get() % 4096 < 4096 - 16);
    let mut filter = JournalFilter::default();
    filter.add_match(b"COMMON=all");
    filter.set_operation(&file, LogicalOp::Conjunction).unwrap();
    filter.add_match(b"TARGET=yes");
    filter.set_operation(&file, LogicalOp::Conjunction).unwrap();

    let mut installed_expression = if installed {
        Some(filter.build(&file).unwrap())
    } else {
        None
    };

    let mut disk = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    disk.set_len(target.get() + 8).unwrap();
    assert!(
        file.recover_reader_tail(&JournalError::ObjectExceedsFileBounds)
            .unwrap()
    );
    assert!(file.recovery_status().incomplete());
    if let Some(expression) = installed_expression.as_mut() {
        assert!(expression.refresh_recovery(&file).is_err());
    } else {
        assert!(filter.build(&file).is_err());
    }

    let mut partial = original.clone();
    let start = target.get() as usize;
    partial[start..start + 16].fill(0);
    let common = file
        .find_data_offset(file.hash(b"COMMON=all"), b"COMMON=all")
        .unwrap()
        .unwrap();
    if zero_data {
        let common = common.get() as usize;
        partial[common + 40..common + 64].fill(0);
    }
    disk.seek(SeekFrom::Start(0)).unwrap();
    disk.write_all(&partial).unwrap();
    if let Some(expression) = installed_expression.as_mut() {
        assert!(expression.refresh_recovery(&file).is_err());
    } else {
        assert!(filter.build(&file).is_err());
    }
    assert!(!file.recovery_status().incomplete());
    disk.seek(SeekFrom::Start(target.get())).unwrap();
    disk.write_all(&original[start..start + 16]).unwrap();
    if zero_data {
        let start = common.get() as usize;
        disk.seek(SeekFrom::Start(common.get())).unwrap();
        disk.write_all(&original[start..start + 64]).unwrap();
    }
    let mut expression = installed_expression.unwrap_or_else(|| filter.build(&file).unwrap());
    if compose {
        let mut clean = JournalFilter::default();
        clean.add_match(b"COMMON=all");
        expression = FilterExpr::Conjunction(vec![clean.build(&file).unwrap(), expression]);
    }
    if tail_error {
        let common = file
            .find_data_offset(file.hash(b"COMMON=all"), b"COMMON=all")
            .unwrap()
            .unwrap();
        let array = file
            .data_ref(common)
            .unwrap()
            .header
            .entry_array_offset
            .unwrap();
        disk.seek(SeekFrom::Start(array.get() + 8)).unwrap();
        disk.write_all(&8u64.to_le_bytes()).unwrap();
        assert!(expression.previous(&file, NonZeroU64::MAX).is_err());
        let start = array.get() as usize + 8;
        disk.seek(SeekFrom::Start(array.get() + 8)).unwrap();
        disk.write_all(&original[start..start + 8]).unwrap();
        let mut append = JournalFile::<MmapMut>::open_for_append(&repo, 64 * 1024).unwrap();
        let mut writer = JournalWriter::new(&mut append, 1, test_uuid(2)).unwrap();
        writer
            .add_entry(&mut append, &[b"COMMON=all", b"TARGET=yes"], 3_000, 3)
            .unwrap();
        drop(writer);
        drop(append);
    }
    let found = if backward {
        expression.previous(&file, NonZeroU64::MAX)
    } else {
        expression.next(&file, entries[1])
    };
    assert_eq!(
        found.unwrap(),
        Some(entries[1]),
        "compact={compact}, installed={installed}, backward={backward}, compose={compose}, tail_error={tail_error}"
    );
}

#[test]
fn pending_filter_retries_all_branches_after_repair_clears_recovery() {
    for compact in [false, true] {
        for installed in [false, true] {
            for backward in [false, true] {
                for compose in [false, true] {
                    for zero_data in [false, true] {
                        filter_repair_retry(
                            compact, installed, backward, compose, false, zero_data,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn recovered_filter_tail_retry_preserves_captured_populations() {
    for compact in [false, true] {
        filter_repair_retry(compact, true, true, true, true, false);
    }
}

#[derive(Clone, Copy, Debug)]
struct ExpectedSanitizedHeader {
    header_size: u64,
    n_data: u64,
    n_fields: u64,
    n_tags: u64,
    n_entry_arrays: u64,
    data_hash_chain_depth: u64,
    field_hash_chain_depth: u64,
    tail_entry_array_offset: u32,
    tail_entry_array_n_entries: u32,
    tail_entry_offset: u64,
}

const HEADER_SANITIZE_CASES: &[ExpectedSanitizedHeader] = &[
    ExpectedSanitizedHeader {
        header_size: 208,
        n_data: 0,
        n_fields: 0,
        n_tags: 0,
        n_entry_arrays: 0,
        data_hash_chain_depth: 0,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 216,
        n_data: 11,
        n_fields: 0,
        n_tags: 0,
        n_entry_arrays: 0,
        data_hash_chain_depth: 0,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 220,
        n_data: 11,
        n_fields: 0,
        n_tags: 0,
        n_entry_arrays: 0,
        data_hash_chain_depth: 0,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 224,
        n_data: 11,
        n_fields: 22,
        n_tags: 0,
        n_entry_arrays: 0,
        data_hash_chain_depth: 0,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 232,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 0,
        data_hash_chain_depth: 0,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 240,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 0,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 248,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 250,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 256,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 66,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 260,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 66,
        tail_entry_array_offset: 77,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 264,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 66,
        tail_entry_array_offset: 77,
        tail_entry_array_n_entries: 88,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 268,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 66,
        tail_entry_array_offset: 77,
        tail_entry_array_n_entries: 88,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 272,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 66,
        tail_entry_array_offset: 77,
        tail_entry_array_n_entries: 88,
        tail_entry_offset: 99,
    },
    ExpectedSanitizedHeader {
        header_size: 300,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 66,
        tail_entry_array_offset: 77,
        tail_entry_array_n_entries: 88,
        tail_entry_offset: 99,
    },
];

fn test_uuid(seed: u8) -> uuid::Uuid {
    uuid::Uuid::from_bytes([seed; 16])
}

#[test]
fn payload_parts_structured_equals_contiguous_payload() {
    let parts = PayloadParts::structured(b"NAME", b"\x00=VALUE");

    assert!(parts.equals_slice(b"NAME=\x00=VALUE"));
    assert!(!parts.equals_slice(b"NAME=VALUE"));
    assert!(!parts.equals_slice(b"OTHER=\x00=VALUE"));
}

#[test]
fn sanitize_header_for_historical_size_matches_per_field_boundaries() {
    for expected in HEADER_SANITIZE_CASES {
        assert_sanitized_header(*expected);
    }
}

fn assert_sanitized_header(expected: ExpectedSanitizedHeader) {
    let sanitized = sanitize_header_for_size(JournalHeader {
        header_size: expected.header_size,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 66,
        tail_entry_array_offset: 77,
        tail_entry_array_n_entries: 88,
        tail_entry_offset: 99,
        ..JournalHeader::default()
    });

    assert_eq!(sanitized.n_data, expected.n_data, "{expected:?}");
    assert_eq!(sanitized.n_fields, expected.n_fields, "{expected:?}");
    assert_eq!(sanitized.n_tags, expected.n_tags, "{expected:?}");
    assert_eq!(
        sanitized.n_entry_arrays, expected.n_entry_arrays,
        "{expected:?}"
    );
    assert_eq!(
        sanitized.data_hash_chain_depth, expected.data_hash_chain_depth,
        "{expected:?}"
    );
    assert_eq!(
        sanitized.field_hash_chain_depth, expected.field_hash_chain_depth,
        "{expected:?}"
    );
    assert_eq!(
        sanitized.tail_entry_array_offset, expected.tail_entry_array_offset,
        "{expected:?}"
    );
    assert_eq!(
        sanitized.tail_entry_array_n_entries, expected.tail_entry_array_n_entries,
        "{expected:?}"
    );
    assert_eq!(
        sanitized.tail_entry_offset, expected.tail_entry_offset,
        "{expected:?}"
    );
}

#[test]
fn writer_lock_helper_rejects_second_acquire() {
    use crate::file::lock::WriterLock;

    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");
    let options = JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3));

    let mut writer_lock =
        WriterLock::acquire(path.to_string_lossy().as_ref()).expect("acquire writer lock");
    let _journal_file: JournalFile<crate::file::MmapMut> =
        JournalFile::create(&repo_file, options).expect("create journal");
    match WriterLock::acquire(path.to_string_lossy().as_ref()) {
        Ok(mut lock) => {
            let _ = lock.release();
            panic!("second WriterLock::acquire succeeded while writer lock is held")
        }
        Err(err) => {
            assert_eq!(err.kind(), std::io::ErrorKind::WouldBlock);
        }
    }
    writer_lock.release().expect("release writer lock");
}

#[test]
fn writer_lock_is_disabled_by_default() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");
    let options = JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3));

    let journal_file: JournalFile<crate::file::MmapMut> =
        JournalFile::create(&repo_file, options).expect("create journal");
    drop(journal_file);
    assert!(!PathBuf::from(format!("{}.lock", path.display())).exists());
}

#[cfg(unix)]
#[test]
fn create_uses_journal_file_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let _journal_file: JournalFile<crate::file::MmapMut> = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
    )
    .expect("create journal");

    let mode = std::fs::metadata(&path)
        .expect("stat journal")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o640);

    let override_path = journal_dir.join("override.journal");
    let override_file =
        crate::repository::File::from_path(&override_path).expect("test journal path should parse");
    let _journal_file: JournalFile<crate::file::MmapMut> = JournalFile::create(
        &override_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_file_mode(0o600),
    )
    .expect("create journal with override mode");

    let override_mode = std::fs::metadata(&override_path)
        .expect("stat override journal")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(override_mode, 0o600);
}

#[test]
fn open_for_append_rejects_unkeyed_journal_without_mutation() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let mut journal_file: JournalFile<crate::file::MmapMut> = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_keyed_hash(false),
    )
    .expect("create unkeyed journal");
    journal_file.sync().expect("sync unkeyed journal");
    drop(journal_file);

    let before = std::fs::read(&path).expect("read journal before append-open");
    let err =
        match JournalFile::<crate::file::MmapMut>::open_for_append(&repo_file, 8 * 1024 * 1024) {
            Ok(_) => panic!("unkeyed journal append-open should be rejected"),
            Err(err) => err,
        };

    assert!(matches!(err, JournalError::UnsupportedJournalFile));
    let after = std::fs::read(&path).expect("read journal after append-open");
    assert_eq!(after, before);
}

#[test]
fn entry_data_iterator_reports_zero_offsets_as_invalid() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
    )
    .expect("create journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    let payloads = [b"MESSAGE=test".as_slice(), b"PRIORITY=6".as_slice()];
    writer
        .add_entry(&mut journal_file, &payloads, 1_000_000, 100)
        .expect("write entry");

    let mut entry_offsets = Vec::new();
    journal_file
        .entry_offsets(&mut entry_offsets)
        .expect("collect entry offsets");
    let entry_offset = *entry_offsets.first().expect("journal entry");

    {
        let mut entry_guard = journal_file
            .entry_mut(entry_offset, None)
            .expect("entry guard");
        match &mut entry_guard.items {
            EntryItemsType::Regular(items) => items[0].object_offset = 0,
            EntryItemsType::Compact(items) => items[0].object_offset = 0,
        }
    }

    let mut iter = journal_file
        .entry_data_objects(entry_offset)
        .expect("entry iterator");
    assert!(matches!(
        iter.next(),
        Some(Err(JournalError::InvalidOffset))
    ));
    assert!(
        iter.next().is_none(),
        "iterator should stop after the error"
    );
}

fn first_data_offset(journal_file: &JournalFile<crate::file::MmapMut>) -> NonZeroU64 {
    let mut entry_offsets = Vec::new();
    journal_file
        .entry_offsets(&mut entry_offsets)
        .expect("collect entry offsets");
    let entry_offset = *entry_offsets.first().expect("journal entry");
    let entry = journal_file.entry_ref(entry_offset).expect("entry object");
    match &entry.items {
        EntryItemsType::Regular(items) => {
            NonZeroU64::new(items[0].object_offset).expect("data offset")
        }
        EntryItemsType::Compact(items) => {
            NonZeroU64::new(items[0].object_offset as u64).expect("data offset")
        }
    }
}

#[test]
fn visit_data_payload_at_returns_compact_uncompressed_payload() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_compact(true),
    )
    .expect("create compact journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=compact payload".as_slice()],
            1_000_000,
            100,
        )
        .expect("write compact entry");

    let offset = first_data_offset(&journal_file);
    let mut decompressed = Vec::new();
    let mut observed = Vec::new();
    journal_file
        .visit_data_payload_at(offset, &mut decompressed, |payload| {
            observed.extend_from_slice(payload);
            Ok(())
        })
        .expect("visit payload");

    assert_eq!(observed, b"MESSAGE=compact payload");
}

#[test]
fn visit_data_payload_at_decompresses_payload() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_compression(Compression::Zstd)
            .with_compress_threshold(8),
    )
    .expect("create compressed journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    let payload = format!("MESSAGE={}", "x".repeat(1024));
    writer
        .add_entry(&mut journal_file, &[payload.as_bytes()], 1_000_000, 100)
        .expect("write compressed entry");

    let offset = first_data_offset(&journal_file);
    let mut decompressed = Vec::new();
    let mut observed = Vec::new();
    journal_file
        .visit_data_payload_at(offset, &mut decompressed, |payload| {
            observed.extend_from_slice(payload);
            Ok(())
        })
        .expect("visit payload");

    assert_eq!(observed, payload.as_bytes());
}

#[test]
fn compact_writer_reader_and_stock_journalctl() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let journal_file = create_compact_test_journal(&path);

    assert_compact_header_and_payloads(&journal_file);
    assert_stock_journalctl_compact_read_and_verify(&path);
}

fn create_compact_test_journal(path: &PathBuf) -> JournalFile<MmapMut> {
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_compact(true),
    )
    .expect("create compact journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    writer
        .add_entry(
            &mut journal_file,
            &[
                b"MESSAGE=compact entry".as_slice(),
                b"BINARY=\x00\x01\xfe\xff".as_slice(),
            ],
            1_000_000,
            100,
        )
        .expect("write first compact entry");
    writer
        .add_entry(
            &mut journal_file,
            &[
                b"MESSAGE=second compact entry".as_slice(),
                b"PRIORITY=6".as_slice(),
            ],
            1_000_001,
            101,
        )
        .expect("write second compact entry");
    journal_file.sync().expect("sync compact journal");
    journal_file
}

fn assert_compact_header_and_payloads(journal_file: &JournalFile<MmapMut>) {
    assert!(
        journal_file
            .journal_header_ref()
            .has_incompatible_flag(HeaderIncompatibleFlags::Compact)
    );

    let mut entry_offsets = Vec::new();
    journal_file
        .entry_offsets(&mut entry_offsets)
        .expect("collect compact entry offsets");
    assert_eq!(entry_offsets.len(), 2);

    let payloads = journal_file
        .entry_data_objects(entry_offsets[0])
        .expect("compact entry data iterator")
        .map(|item| item.map(|object| object.raw_payload().to_vec()))
        .collect::<Result<Vec<_>>>()
        .expect("read compact data objects");
    assert!(payloads.iter().any(|p| p == b"MESSAGE=compact entry"));
    assert!(payloads.iter().any(|p| p == b"BINARY=\x00\x01\xfe\xff"));
}

fn assert_stock_journalctl_compact_read_and_verify(path: &PathBuf) {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping stock compact assertions");
        return;
    }
    assert_stock_journalctl_compact_read(path);
    assert_stock_journalctl_compact_verify(path);
}

fn assert_stock_journalctl_compact_read(path: &PathBuf) {
    let output = Command::new("journalctl")
        .arg("--file")
        .arg(&path)
        .arg("--output=json")
        .arg("--no-pager")
        .output()
        .expect("run journalctl compact read");
    assert!(
        output.status.success(),
        "journalctl compact read failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output
            .stdout
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .count(),
        2
    );
}

fn assert_stock_journalctl_compact_verify(path: &PathBuf) {
    let output = Command::new("journalctl")
        .arg("--verify")
        .arg("--file")
        .arg(&path)
        .arg("--no-pager")
        .output()
        .expect("run journalctl compact verify");
    assert!(
        output.status.success(),
        "journalctl compact verify failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn journalctl_available() -> bool {
    Command::new("journalctl")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn assert_live_growth_entries(reader: &JournalFile<crate::file::Mmap>, expected: usize) {
    let mut offsets = Vec::new();
    reader.entry_offsets(&mut offsets).unwrap();
    assert_eq!(offsets.len(), expected);
    for (index, entry) in offsets.into_iter().enumerate() {
        let mut fields = Vec::new();
        reader
            .entry_data_object_offsets(entry, &mut fields)
            .unwrap();
        assert_eq!(fields.len(), 1);
        reader
            .visit_data_payload_at(fields[0], &mut Vec::new(), |payload| {
                if index == 0 {
                    assert_eq!(payload, b"MESSAGE=seed");
                } else {
                    assert_eq!(payload.len(), 9 * 1024 * 1024);
                    assert!(payload.starts_with(b"MESSAGE="));
                    assert!(payload[8..].iter().all(|byte| *byte == b'x'));
                }
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn live_readers_open_and_follow_during_arena_growth() {
    use crate::file::{EntryField, ExperimentalMmapStrategy, Mmap};

    for compact in [false, true] {
        for strategy in [
            ExperimentalMmapStrategy::Windowed,
            ExperimentalMmapStrategy::WholeFile,
        ] {
            let dir = TempDir::new().unwrap();
            let path = dir.path().join("system.journal");
            let repo = crate::repository::File::from_path(&path).unwrap();
            let mut file = JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_compact(compact)
                .with_data_hash_table_buckets(64)
                .with_field_hash_table_buckets(16)
                .create::<MmapMut>(&repo)
                .unwrap();
            let mut writer = JournalWriter::new(&mut file, 1, test_uuid(2)).unwrap();
            assert_eq!(writer.live_publish_every_entries(), 1);
            writer
                .add_entry(&mut file, &[b"MESSAGE=seed"], 1_700_000_000_000_001, 1)
                .unwrap();
            let open = || JournalFile::<Mmap>::open_path_with_strategy(&path, 64 * 1024, strategy);
            let existing = std::rc::Rc::new(open().unwrap());
            assert_live_growth_entries(&existing, 1);
            let mut payload = b"MESSAGE=".to_vec();
            payload.resize(9 * 1024 * 1024, b'x');
            let observed_growth = std::rc::Rc::new(std::cell::Cell::new(false));
            let during = std::rc::Rc::new(std::cell::RefCell::new(None));
            // The hook runs after DATA preparation, before ENTRY publication.
            {
                let (existing, observed_growth, during, path) = (
                    existing.clone(),
                    observed_growth.clone(),
                    during.clone(),
                    path.clone(),
                );
                crate::file::writer::AFTER_ENTRY_ITEM.with(|hook| {
                    *hook.borrow_mut() = Some(Box::new(move || {
                        let header = existing.journal_header_ref();
                        assert_eq!(header.n_entries, 1);
                        assert!(
                            header.header_size + header.arena_size
                                > std::fs::metadata(&path).unwrap().len()
                        );
                        observed_growth.set(true);
                        assert_live_growth_entries(&existing, 1);
                        let opened = JournalFile::<Mmap>::open_path_with_strategy(
                            &path,
                            64 * 1024,
                            strategy,
                        )
                        .expect("open while writer has prepared a larger DATA object");
                        assert_live_growth_entries(&opened, 1);
                        *during.borrow_mut() = Some(opened);
                    }))
                });
            }
            let result = writer.add_entry_fields(
                &mut file,
                [EntryField::raw(&payload)],
                1_700_000_000_000_002,
                2,
            );
            crate::file::writer::AFTER_ENTRY_ITEM.with(|hook| *hook.borrow_mut() = None);
            result.unwrap();
            assert!(observed_growth.get());
            assert_live_growth_entries(during.borrow().as_ref().unwrap(), 2);
            assert_live_growth_entries(&existing, 2);
            assert_live_growth_entries(&open().unwrap(), 2);
        }
    }
}

#[test]
fn live_open_rejects_unsafe_header_and_table_mappings() {
    for compact in [false, true] {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("seed.journal");
        let repo = crate::repository::File::from_path(&source).unwrap();
        drop(
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_compact(compact)
                .with_data_hash_table_buckets(64)
                .with_field_hash_table_buckets(16)
                .create::<MmapMut>(&repo)
                .unwrap(),
        );
        let original = std::fs::read(&source).unwrap();
        for (name, offset, value) in [
            ("short header", 88, 200),
            ("unaligned header", 88, 273),
            ("header beyond file", 88, original.len() as u64 + 8),
            ("arena overflow", 96, u64::MAX),
            ("missing DATA offset", 104, 0),
            ("missing DATA size", 112, 0),
            ("missing FIELD offset", 120, 0),
            ("missing FIELD size", 128, 0),
            ("unaligned DATA offset", 104, 289),
            ("unaligned FIELD offset", 120, 289),
            ("short DATA table", 112, 8),
            ("partial FIELD bucket", 128, 17),
            (
                "DATA overlaps header",
                104,
                u64::from_le_bytes(original[88..96].try_into().unwrap()),
            ),
            ("FIELD beyond file", 120, original.len() as u64),
            ("DATA offset overflow", 104, u64::MAX - 7),
            ("FIELD size overflow", 128, u64::MAX - 15),
        ] {
            let path = dir.path().join("invalid.journal");
            let mut bytes = original.clone();
            bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
            std::fs::write(&path, bytes).unwrap();
            assert!(
                JournalFile::<crate::file::Mmap>::open_path(&path, 64 * 1024).is_err(),
                "{name}, compact={compact}"
            );
        }
    }
}

// Paired with Go journal/header_open_gate_test.go.
fn header_gate_journal(dir: &TempDir, compact: bool) -> (PathBuf, crate::repository::File) {
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");
    let mut journal_file: JournalFile<MmapMut> = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_compact(compact),
    )
    .expect("create journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("writer");
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=gate".as_slice()],
            1_000_000,
            100,
        )
        .expect("write entry");
    journal_file.sync().expect("sync journal");
    drop(journal_file);
    (path, repo_file)
}

fn patch_header(path: &std::path::Path, offset: u64, value: &[u8]) {
    use std::os::unix::fs::FileExt;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open journal for patch");
    file.write_all_at(value, offset).expect("patch header");
}

fn append_open_error(repo_file: &crate::repository::File) -> JournalError {
    match JournalFile::<MmapMut>::open_for_append(repo_file, 8 * 1024 * 1024) {
        Ok(_) => panic!("append-open should be rejected"),
        Err(err) => err,
    }
}

#[test]
fn open_rejects_unknown_incompatible_flag() {
    let dir = TempDir::new().expect("create temp dir");
    let (path, repo_file) = header_gate_journal(&dir, false);
    let bytes = std::fs::read(&path).expect("read journal");
    let flags = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) | 1 << 5;
    patch_header(&path, 12, &flags.to_le_bytes());
    let before = std::fs::read(&path).expect("read journal before open");

    let read_err = match JournalFile::<crate::file::Mmap>::open(&repo_file, 4096) {
        Ok(_) => panic!("reader open should be rejected"),
        Err(err) => err,
    };
    assert!(
        matches!(read_err, JournalError::UnsupportedJournalFile),
        "{read_err:?}"
    );
    let err = append_open_error(&repo_file);
    assert!(
        matches!(err, JournalError::UnsupportedJournalFile),
        "{err:?}"
    );
    assert_eq!(
        std::fs::read(&path).expect("read journal after open"),
        before
    );
}

#[test]
fn append_open_requires_both_hash_tables() {
    for offset in [104, 120] {
        let dir = TempDir::new().expect("create temp dir");
        let (path, repo_file) = header_gate_journal(&dir, false);
        patch_header(&path, offset, &[0; 16]);
        let before = std::fs::read(&path).expect("read journal before open");
        let err = append_open_error(&repo_file);
        assert!(
            matches!(err, JournalError::InvalidObjectLocation),
            "{offset}: {err:?}"
        );
        assert_eq!(
            std::fs::read(&path).expect("read journal after open"),
            before
        );
    }
}

#[test]
fn append_open_refuses_file_beyond_four_gib() {
    for compact in [false, true] {
        let dir = TempDir::new().expect("create temp dir");
        let (path, repo_file) = header_gate_journal(&dir, compact);
        let original = std::fs::read(&path).expect("read journal");
        let size = crate::file::writer::JOURNAL_COMPACT_SIZE_MAX + 4096;
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("open journal")
            .set_len(size)
            .expect("extend journal");
        let err = append_open_error(&repo_file);
        assert!(
            matches!(err, JournalError::UnsupportedJournalFile),
            "compact={compact}: {err:?}"
        );
        assert_eq!(std::fs::metadata(&path).expect("stat journal").len(), size);
        // The sparse tail is zero by construction; compare the written prefix.
        use std::os::unix::fs::FileExt;
        let mut prefix = vec![0; original.len()];
        std::fs::File::open(&path)
            .expect("open journal")
            .read_exact_at(&mut prefix, 0)
            .expect("read prefix");
        assert_eq!(prefix, original, "compact={compact}");
    }
}

fn read_u64_at(path: &std::path::Path, offset: usize) -> u64 {
    let bytes = std::fs::read(path).expect("read journal");
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn patch_u32(path: &std::path::Path, offset: usize, update: impl Fn(u32) -> u32) {
    let bytes = std::fs::read(path).expect("read journal");
    let value = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    patch_header(path, offset as u64, &update(value).to_le_bytes());
}

type HeaderPatch = fn(&std::path::Path);

// Paired with Go journal/header_append_target_test.go appendTargetCases:
// files systemd's writer refuses to extend and that earlier SDK releases could
// leave behind.
fn append_target_cases() -> Vec<(&'static str, HeaderPatch)> {
    vec![
        ("header-size-280", |path| {
            let arena = read_u64_at(path, 96);
            patch_header(path, 88, &280u64.to_le_bytes());
            patch_header(path, 96, &(arena - 8).to_le_bytes());
        }),
        ("unknown-compatible-flag", |path| {
            patch_u32(path, 8, |v| v | 1 << 3)
        }),
        ("no-tail-entry-boot-id-flag", |path| {
            patch_u32(path, 8, |v| {
                v & !(HeaderCompatibleFlags::TailEntryBootId as u32)
            })
        }),
        ("archived", |path| {
            patch_header(path, 16, &[JournalState::Archived as u8])
        }),
        ("zero-head-realtime", |path| {
            patch_header(path, 184, &0u64.to_le_bytes())
        }),
        ("head-realtime-limit", |path| {
            patch_header(path, 184, &TIMESTAMP_LIMIT_USEC.to_le_bytes())
        }),
        ("tail-realtime-limit", |path| {
            patch_header(path, 192, &TIMESTAMP_LIMIT_USEC.to_le_bytes())
        }),
        ("tail-monotonic-limit", |path| {
            patch_header(path, 200, &TIMESTAMP_LIMIT_USEC.to_le_bytes())
        }),
        ("null-tail-boot-id", |path| patch_header(path, 56, &[0; 16])),
    ]
}

#[test]
fn append_open_refuses_targets_systemd_will_not_extend() {
    for compact in [false, true] {
        for (name, patch) in append_target_cases() {
            let dir = TempDir::new().expect("create temp dir");
            let (path, repo_file) = header_gate_journal(&dir, compact);
            patch(&path);
            let before = std::fs::read(&path).expect("read journal before open");
            let err = append_open_error(&repo_file);
            assert!(
                matches!(err, JournalError::UnsupportedJournalFile),
                "{name}, compact={compact}: {err:?}"
            );
            assert_eq!(
                std::fs::read(&path).expect("read journal after open"),
                before,
                "{name}"
            );
            // The patched 280-byte header is not a real newer layout: its
            // objects still start at 272, which other checks refuse.
            if name == "header-size-280" {
                continue;
            }
            // Readers keep accepting these files.
            JournalFile::<crate::file::Mmap>::open(&repo_file, 4096)
                .unwrap_or_else(|err| panic!("{name}, compact={compact}: reader open {err:?}"));
            // Archive-only opens keep earlier releases' files retainable.
            JournalFile::<MmapMut>::open_for_archive(&repo_file, 8 * 1024 * 1024)
                .unwrap_or_else(|err| panic!("{name}, compact={compact}: archive open {err:?}"));
        }
    }
}

#[test]
fn append_refuses_out_of_range_timestamps_without_mutation() {
    for (name, realtime, monotonic) in [
        ("zero-realtime", 0, 1),
        ("realtime-limit", TIMESTAMP_LIMIT_USEC, 1),
        (
            "monotonic-limit",
            1_700_000_000_000_000,
            TIMESTAMP_LIMIT_USEC,
        ),
    ] {
        let dir = TempDir::new().expect("create temp dir");
        let (path, repo_file) = header_gate_journal(&dir, false);
        let mut journal_file =
            JournalFile::<MmapMut>::open_for_append(&repo_file, 8 * 1024 * 1024).expect("open");
        let mut writer = JournalWriter::new(&mut journal_file, 2, test_uuid(9)).expect("writer");
        journal_file.sync().expect("sync");
        let before = std::fs::read(&path).expect("read before");
        let err = writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=out".as_slice()],
                realtime,
                monotonic,
            )
            .expect_err(name);
        assert!(
            matches!(err, JournalError::TimestampOutOfRange),
            "{name}: {err:?}"
        );
        assert!(!writer.is_poisoned(), "{name}");
        journal_file.sync().expect("sync");
        assert_eq!(std::fs::read(&path).expect("read after"), before, "{name}");
        // The writer stays usable.
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=valid".as_slice()],
                2_000_000,
                200,
            )
            .unwrap_or_else(|err| panic!("{name}: valid append {err:?}"));
    }
}

#[test]
fn reader_open_refuses_unknown_state_and_short_sealed_header() {
    let dir = TempDir::new().expect("create temp dir");
    let (path, repo_file) = header_gate_journal(&dir, false);
    patch_header(&path, 16, &[3]);
    let err = JournalFile::<crate::file::Mmap>::open(&repo_file, 4096)
        .err()
        .expect("state 3 must be refused");
    assert!(
        matches!(err, JournalError::InvalidJournalFileState),
        "{err:?}"
    );

    let dir = TempDir::new().expect("create temp dir");
    let (path, repo_file) = header_gate_journal(&dir, false);
    patch_u32(&path, 8, |v| v | HeaderCompatibleFlags::Sealed as u32);
    let arena = read_u64_at(&path, 96);
    patch_header(&path, 88, &232u64.to_le_bytes());
    patch_header(&path, 96, &(arena + 40).to_le_bytes());
    let err = JournalFile::<crate::file::Mmap>::open(&repo_file, 4096)
        .err()
        .expect("short sealed header must be refused");
    assert!(
        matches!(err, JournalError::InvalidJournalFileState),
        "{err:?}"
    );
}

// Writable opens apply the same every-open refusals as readers, before any
// mutation. Paired with Go TestWritableOpenRefusesUnknownStateAndShortSealedHeader.
#[test]
fn writable_open_refuses_unknown_state_and_short_sealed_header() {
    type Patch = fn(&Path);
    let cases: [(&str, Patch, fn(&JournalError) -> bool); 2] = [
        (
            "state-3",
            |path| patch_header(path, 16, &[3]),
            |err| matches!(err, JournalError::InvalidJournalFileState),
        ),
        (
            "sealed-232",
            |path| {
                patch_u32(path, 8, |v| v | HeaderCompatibleFlags::Sealed as u32);
                let arena = read_u64_at(path, 96);
                patch_header(path, 88, &232u64.to_le_bytes());
                patch_header(path, 96, &(arena + 40).to_le_bytes());
            },
            |err| matches!(err, JournalError::InvalidJournalFileState),
        ),
    ];
    for (name, patch, expected) in cases {
        for archive_only in [false, true] {
            let dir = TempDir::new().expect("create temp dir");
            let (path, repo_file) = header_gate_journal(&dir, false);
            patch(&path);
            let before = std::fs::read(&path).expect("read journal before open");
            let result = if archive_only {
                JournalFile::<MmapMut>::open_for_archive(&repo_file, 8 * 1024 * 1024)
            } else {
                JournalFile::<MmapMut>::open_for_append(&repo_file, 8 * 1024 * 1024)
            };
            let err = result
                .err()
                .unwrap_or_else(|| panic!("{name}, archive_only={archive_only}: must refuse"));
            assert!(
                expected(&err),
                "{name}, archive_only={archive_only}: {err:?}"
            );
            assert_eq!(
                std::fs::read(&path).expect("read journal after open"),
                before,
                "{name}, archive_only={archive_only}"
            );
        }
    }
}

// SDK writers before this check, like systemd's, stored a truncated 32-bit
// tail entry array hint once a regular file grew past 4 GiB. Paired with Go
// TestReadersIgnoreTailArrayHintAboveFourGiB.
#[test]
fn committed_arena_ignores_tail_array_hint_above_four_gib() {
    for large in [false, true] {
        let dir = TempDir::new().expect("create temp dir");
        let (path, repo_file) = header_gate_journal(&dir, false);
        if large {
            let size = crate::file::writer::JOURNAL_COMPACT_SIZE_MAX + 1 + 4096;
            std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .expect("open journal")
                .set_len(size)
                .expect("extend journal");
            patch_header(&path, 96, &(size - 272).to_le_bytes());
        }
        // A truncated hint names an offset below the first entry array.
        patch_header(&path, 256, &8u32.to_le_bytes());
        let journal = JournalFile::<crate::file::Mmap>::open(&repo_file, 4096).expect("open");
        let result = journal.validate_committed_arena();
        assert_eq!(result.is_ok(), large, "large={large}: {result:?}");
        if large {
            // Readers still reach every entry through the entry arrays.
            let mut offsets = Vec::new();
            journal.entry_offsets(&mut offsets).expect("entry offsets");
            assert_eq!(offsets.len() as u64, journal.journal_header_ref().n_entries);
            assert!(!offsets.is_empty());
        }
    }
}

// Paired with Go TestCreateRefusesOversizeLayoutWithoutTouchingTarget.
#[test]
fn create_refuses_oversize_layout_without_touching_target() {
    let dir = TempDir::new().expect("create temp dir");
    let buckets = (crate::file::writer::JOURNAL_COMPACT_SIZE_MAX as usize)
        / std::mem::size_of::<crate::file::HashItem>()
        + 1;
    let options = || {
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_data_hash_table_buckets(buckets)
    };
    for existing in [false, true] {
        let path = dir
            .path()
            .join(format!("system{}.journal", u8::from(existing)));
        if existing {
            std::fs::write(&path, b"keep").expect("write existing file");
        }
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");
        let err = JournalFile::<MmapMut>::create(&repo_file, options())
            .err()
            .expect("layout above 4 GiB must be refused");
        assert!(
            matches!(err, JournalError::ObjectExceedsFileBounds),
            "{err:?}"
        );
        if existing {
            assert_eq!(std::fs::read(&path).expect("read existing"), b"keep");
        } else {
            assert!(!path.exists(), "refused create left a file behind");
        }
    }
}

// One-bucket tables are valid in systemd and the Go reader.
#[test]
fn reader_accepts_single_bucket_hash_tables() {
    let dir = TempDir::new().expect("create temp dir");
    let path = dir.path().join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");
    let mut journal_file: JournalFile<MmapMut> = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_data_hash_table_buckets(1)
            .with_field_hash_table_buckets(1),
    )
    .expect("create journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("writer");
    for i in 0..3u64 {
        let payload = format!("MESSAGE=row-{i}");
        writer
            .add_entry(
                &mut journal_file,
                &[payload.as_bytes()],
                1_000_000 + i,
                100 + i,
            )
            .expect("write entry");
    }
    journal_file.sync().expect("sync journal");
    drop(journal_file);
    let bytes = std::fs::read(&path).expect("read journal");
    assert_eq!(u64::from_le_bytes(bytes[112..120].try_into().unwrap()), 16);
    let journal = JournalFile::<crate::file::Mmap>::open(&repo_file, 4096).expect("open");
    let mut offsets = Vec::new();
    journal.entry_offsets(&mut offsets).expect("entry offsets");
    assert_eq!(offsets.len(), 3);
}

// Paired with Go TestAppendOpenRefusesSealedFile.
#[test]
fn append_open_refuses_sealed_file() {
    let dir = TempDir::new().expect("create temp dir");
    let path = dir.path().join("system.journal");
    let repo_file = crate::repository::File::from_path(&path).expect("test journal path");
    let mut journal_file: JournalFile<MmapMut> = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_seal(crate::seal::SealOptions::new([0; 12], 1_000_000, 1_000_000)),
    )
    .expect("create sealed journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("writer");
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=sealed".as_slice()],
            1_500_000,
            1,
        )
        .expect("write entry");
    journal_file.sync().expect("sync journal");
    drop(writer);
    drop(journal_file);
    let before = std::fs::read(&path).expect("read before");
    let err = append_open_error(&repo_file);
    assert!(
        matches!(err, JournalError::UnsupportedJournalFile),
        "{err:?}"
    );
    assert_eq!(std::fs::read(&path).expect("read after"), before);
    JournalFile::<crate::file::Mmap>::open(&repo_file, 4096).expect("reader open");
}

// Paired with Go TestAppendRefusesEntryThatCouldReachFourGiB. Moves the
// in-memory append offset to the ceiling instead of writing 4 GiB.
#[test]
fn append_refuses_entry_that_could_reach_four_gib() {
    use crate::file::writer::JOURNAL_APPEND_CEILING;
    for compact in [false, true] {
        let dir = TempDir::new().expect("create temp dir");
        let (path, repo_file) = header_gate_journal(&dir, compact);
        let mut journal_file =
            JournalFile::<MmapMut>::open_for_append(&repo_file, 8 * 1024 * 1024).expect("open");
        let mut writer = JournalWriter::new(&mut journal_file, 2, test_uuid(4)).expect("writer");
        journal_file.sync().expect("sync");
        let fields = [crate::file::EntryField::raw(b"MESSAGE=second")];
        let bound = writer
            .bound_entry_growth(&journal_file, &fields, 2_000_000)
            .expect("bound");
        let before = std::fs::read(&path).expect("read before");
        let append_offset = writer.append_offset;
        // One byte short of the room the entry may need.
        writer.append_offset =
            std::num::NonZeroU64::new(JOURNAL_APPEND_CEILING - bound + 8).unwrap();
        let err = writer
            .add_entry_fields(&mut journal_file, fields, 2_000_000, 200)
            .expect_err("entry must not fit");
        assert!(
            matches!(err, JournalError::JournalFileFull),
            "compact={compact}: {err:?}"
        );
        assert!(!writer.is_poisoned(), "compact={compact}");
        journal_file.sync().expect("sync");
        assert_eq!(
            std::fs::read(&path).expect("read after"),
            before,
            "compact={compact}"
        );
        writer.append_offset = append_offset;
        writer
            .add_entry_fields(&mut journal_file, fields, 2_000_000, 200)
            .unwrap_or_else(|err| panic!("compact={compact}: append after refusal {err:?}"));
    }
}

// Paired with Go TestEntryArraysStopDoublingNearTheCeiling.
#[test]
fn entry_arrays_stop_doubling_near_the_ceiling() {
    use crate::file::writer::JOURNAL_APPEND_CEILING;
    let dir = TempDir::new().expect("create temp dir");
    let (_, repo_file) = header_gate_journal(&dir, false);
    let mut journal_file =
        JournalFile::<MmapMut>::open_for_append(&repo_file, 8 * 1024 * 1024).expect("open");
    let mut writer = JournalWriter::new(&mut journal_file, 2, test_uuid(4)).expect("writer");
    writer.entry_growth_bound = 4096;
    writer.append_offset = std::num::NonZeroU64::new(1 << 20).unwrap();
    assert_eq!(writer.entry_array_capacity(&journal_file, 1024), 1024);
    // Room for the reserved growth plus 1023 items, not 1024.
    writer.append_offset =
        std::num::NonZeroU64::new(JOURNAL_APPEND_CEILING - 4096 - (24 + 1023 * 8)).unwrap();
    assert_eq!(writer.entry_array_capacity(&journal_file, 1023), 1023);
    assert_eq!(writer.entry_array_capacity(&journal_file, 1024), 4);
    // Outside an append no reservation applies.
    writer.entry_growth_bound = 0;
    assert_eq!(writer.entry_array_capacity(&journal_file, 1024), 1024);
}
