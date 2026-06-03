use super::*;
use crate::file::MmapMut;
use crate::file::writer::JournalWriter;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

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
