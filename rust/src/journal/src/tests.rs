use super::*;
use journal_core::file::{JournalFileOptions, JournalWriter, MmapMut};
use journal_core::repository::File as RepoFile;
use journal_core::seal::SealOptions;
use serde_json::Value;
use std::path::{Path, PathBuf};

struct TempPath(PathBuf);

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn parse_match_bytes_accepts_binary_values() {
    let data = b"MESSAGE=\xff\x00binary";
    assert_eq!(parse_match_bytes(data).unwrap(), data);
}

#[test]
fn parse_match_bytes_rejects_invalid_field_names() {
    assert!(parse_match_bytes(b"lower=value").is_err());
    assert!(parse_match_bytes(b"1FIELD=value").is_err());
    assert!(parse_match_bytes(b"=value").is_err());
}

#[test]
fn json_entry_includes_monotonic_timestamp_and_preserves_utf8() {
    let mut fields = HashMap::new();
    fields.insert("MESSAGE".to_string(), "héllo".as_bytes().to_vec());

    let mut field_values = HashMap::new();
    field_values.insert("MESSAGE".to_string(), vec!["héllo".as_bytes().to_vec()]);
    field_values.insert("BINARY".to_string(), vec![vec![0xff, 0x00]]);
    field_values.insert("CONTROL".to_string(), vec![b"abc\x07def".to_vec()]);

    let entry = Entry {
        fields,
        field_values,
        payloads: Vec::new(),
        seqnum: 7,
        realtime: 100,
        monotonic: 42,
        boot_id: [1; 16],
        cursor: "s=1;j=1;c=64;n=7".to_string(),
    };

    let Value::Object(json) = json_entry(&entry) else {
        panic!("entry JSON should be an object");
    };

    assert_eq!(
        json.get("__MONOTONIC_TIMESTAMP"),
        Some(&Value::String("42".to_string()))
    );
    assert_eq!(
        json.get("MESSAGE"),
        Some(&Value::String("héllo".to_string()))
    );
    assert_eq!(
        json.get("BINARY"),
        Some(&Value::Array(vec![Value::from(255), Value::from(0)]))
    );
    assert_eq!(
        json.get("CONTROL"),
        Some(&Value::Array(vec![
            Value::from(97),
            Value::from(98),
            Value::from(99),
            Value::from(7),
            Value::from(100),
            Value::from(101),
            Value::from(102),
        ]))
    );
}

#[test]
fn no_rtc_fixtures_drain_without_tail_object_errors() {
    let fixture_dir = repo_root().join("fixtures/systemd/test-data/no-rtc");
    let mut total_entries = 0usize;
    for entry in std::fs::read_dir(&fixture_dir).expect("fixture directory exists") {
        let path = entry.expect("fixture directory entry").path();
        if !is_journal_file_name(&path) {
            continue;
        }
        let mut reader = FileReader::open(&path).expect("open journal fixture");
        let mut file_entries = 0usize;
        while reader.next().expect("fixture drains cleanly") {
            reader.get_entry().expect("entry is readable");
            file_entries += 1;
        }
        assert!(
            file_entries > 0,
            "expected at least one readable entry in {}",
            path.display()
        );
        total_entries += file_entries;
    }
    assert_eq!(total_entries, 10757);
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repo root")
}

fn test_uuid(n: u8) -> uuid::Uuid {
    let mut bytes = [0u8; 16];
    bytes[15] = n;
    uuid::Uuid::from_bytes(bytes)
}

fn test_seal_opts() -> SealOptions {
    SealOptions::new([0u8; 12], 1_000_000, 1_000_000)
}

fn create_facade_test_writer(path: &Path) -> (JournalFile<MmapMut>, JournalWriter) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create journal parent");
    }
    let repo_file = RepoFile::from_path(path)
        .unwrap_or_else(|| panic!("test journal path should parse: {}", path.display()));
    let mut journal_file = JournalFile::<MmapMut>::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
    )
    .expect("create journal");
    let writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    (journal_file, writer)
}

fn create_facade_compressed_test_writer(path: &Path) -> (JournalFile<MmapMut>, JournalWriter) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create journal parent");
    }
    let repo_file = RepoFile::from_path(path)
        .unwrap_or_else(|| panic!("test journal path should parse: {}", path.display()));
    let mut journal_file = JournalFile::<MmapMut>::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_compression(Compression::Zstd)
            .with_compress_threshold(8),
    )
    .expect("create compressed journal");
    let writer = JournalWriter::new_with_compression(
        &mut journal_file,
        1,
        test_uuid(4),
        Compression::Zstd,
        8,
    )
    .expect("create compressed writer");
    (journal_file, writer)
}

fn write_facade_test_journal(path: &Path) {
    let (mut journal_file, mut writer) = create_facade_test_writer(path);
    writer
        .add_entry(
            &mut journal_file,
            &[
                b"MESSAGE=first".as_slice(),
                b"REPEAT=one".as_slice(),
                b"REPEAT=two".as_slice(),
                b"BIN=\x00\xff".as_slice(),
            ],
            1000,
            11,
        )
        .expect("write first entry");
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=second".as_slice(), b"REPEAT=three".as_slice()],
            1001,
            12,
        )
        .expect("write second entry");
    journal_file.sync().expect("sync journal");
}

fn write_single_entry_journal(
    path: &Path,
    seqnum: u64,
    realtime: u64,
    monotonic: u64,
    payload: &[u8],
) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create journal parent");
    }
    let repo_file = RepoFile::from_path(path)
        .unwrap_or_else(|| panic!("test journal path should parse: {}", path.display()));
    let mut journal_file = JournalFile::<MmapMut>::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
    )
    .expect("create journal");
    let mut writer =
        JournalWriter::new(&mut journal_file, seqnum, test_uuid(4)).expect("create writer");
    writer
        .add_entry(&mut journal_file, &[payload], realtime, monotonic)
        .expect("write entry");
    journal_file.sync().expect("sync journal");
}

fn write_facade_single_message_journal(path: &Path, message: &[u8], realtime: u64) {
    let (mut journal_file, mut writer) = create_facade_test_writer(path);
    let payload = [b"MESSAGE=".as_slice(), message].concat();
    writer
        .add_entry(&mut journal_file, &[payload.as_slice()], realtime, 21)
        .expect("write single message");
    journal_file.sync().expect("sync journal");
}

#[test]
fn raw_writer_backward_monotonic_is_clamped_and_verifies() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/raw-backward-monotonic.journal");
    let (mut journal_file, mut writer) = create_facade_test_writer(&path);
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=raw monotonic first".as_slice()],
            1_700_003_000_000_000,
            10,
        )
        .expect("write first entry");
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=raw monotonic second".as_slice()],
            1_700_003_000_000_001,
            5,
        )
        .expect("write second entry");
    journal_file.sync().expect("sync journal");

    verify_file(&path).expect("clamped same-boot monotonic timestamps should verify");

    let mut journal =
        SdJournalOpenFiles(&[path.to_str().expect("utf8 path")], 0).expect("open files");
    assert_eq!(SdJournalNext(&mut journal).expect("first entry"), 1);
    let (first_monotonic, _boot_id) =
        SdJournalGetMonotonicUsec(&mut journal).expect("first monotonic");
    assert_eq!(first_monotonic, 10);
    assert_eq!(SdJournalNext(&mut journal).expect("second entry"), 1);
    let (second_monotonic, _boot_id) =
        SdJournalGetMonotonicUsec(&mut journal).expect("second monotonic");
    assert_eq!(second_monotonic, 11);
}

#[test]
fn raw_writer_explicit_zero_monotonic_pass_through() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/raw-zero-monotonic.journal");
    let (mut journal_file, mut writer) = create_facade_test_writer(&path);
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=raw zero monotonic".as_slice()],
            1_700_003_000_100_000,
            0,
        )
        .expect("write entry");
    journal_file.sync().expect("sync journal");
    verify_file(&path).expect("zero monotonic first entry should verify");

    let mut journal =
        SdJournalOpenFiles(&[path.to_str().expect("utf8 path")], 0).expect("open files");
    assert_eq!(SdJournalNext(&mut journal).expect("next"), 1);
    let (monotonic, _boot_id) = SdJournalGetMonotonicUsec(&mut journal).expect("monotonic");
    assert_eq!(monotonic, 0);
}

#[test]
fn snapshot_reader_handles_final_partial_mmap_window() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/system.journal");
    write_facade_test_journal(&path);

    let options = ReaderOptions::snapshot().with_window_size(32 * 1024 * 1024);
    let mut reader = FileReader::open_with_options(&path, options).expect("open snapshot reader");
    assert!(reader.next().expect("first entry"));

    let mut payloads = Vec::new();
    reader
        .visit_entry_payloads(|payload| {
            payloads.push(payload.to_vec());
            Ok(())
        })
        .expect("visit current entry payloads");
    assert!(payloads.iter().any(|payload| payload == b"MESSAGE=first"));
    assert!(payloads.iter().any(|payload| payload == b"BIN=\x00\xff"));
}

#[test]
fn snapshot_header_is_fixed_while_live_header_refreshes() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("journals/system.journal");
    let (mut journal_file, mut writer) = create_facade_test_writer(&path);

    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=first".as_slice()],
            1_700_005_000_000_000,
            10,
        )
        .expect("write first entry");
    journal_file.sync().expect("sync first entry");

    let snapshot_reader =
        FileReader::open_with_options(&path, ReaderOptions::snapshot()).expect("open snapshot");
    let live_reader =
        FileReader::open_with_options(&path, ReaderOptions::live()).expect("open live");

    assert_eq!(snapshot_reader.header().tail_entry_seqnum, 1);
    assert_eq!(live_reader.header().tail_entry_seqnum, 1);

    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=second".as_slice()],
            1_700_005_000_000_001,
            11,
        )
        .expect("write second entry");
    journal_file.sync().expect("sync second entry");

    assert_eq!(
        snapshot_reader.header().tail_entry_seqnum,
        1,
        "snapshot header should remain fixed at open time"
    );
    assert_eq!(
        live_reader.header().tail_entry_seqnum,
        2,
        "live header should refresh from the mapped file"
    );
}

#[test]
fn default_reader_options_use_production_window_size() {
    let options = ReaderOptions::default();
    assert_eq!(options.bounds, ReaderBounds::Live);
    assert_eq!(options.mmap_strategy, ExperimentalMmapStrategy::Windowed);
    assert_eq!(options.window_size, DEFAULT_READER_WINDOW_SIZE);
    assert_eq!(options.window_size, 32 * 1024 * 1024);
}

#[test]
fn reader_options_with_bounds_sets_bounds() {
    let options = ReaderOptions::default().with_bounds(ReaderBounds::Snapshot);
    assert_eq!(options.bounds, ReaderBounds::Snapshot);
}

#[test]
fn directory_reader_uses_sequential_path_for_non_overlapping_files() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let first_path = dir.path().join("journals/first.journal");
    let second_path = dir.path().join("journals/second.journal");

    write_single_entry_journal(&first_path, 1, 1_700_004_000_000_000, 10, b"MESSAGE=first");
    write_single_entry_journal(
        &second_path,
        2,
        1_700_004_000_000_001,
        20,
        b"MESSAGE=second",
    );

    let mut reader = DirectoryReader::open_files([&first_path, &second_path]).expect("open files");
    assert!(
        reader.non_overlapping,
        "test files should qualify for sequential directory reads"
    );

    reader.seek_head();
    assert!(reader.next().expect("first entry"));
    assert_eq!(
        reader.get_realtime_usec().expect("first realtime"),
        1_700_004_000_000_000
    );
    assert!(reader.next().expect("second entry"));
    assert_eq!(
        reader.get_realtime_usec().expect("second realtime"),
        1_700_004_000_000_001
    );
    assert!(!reader.next().expect("end"));

    reader.seek_tail();
    assert!(reader.previous().expect("tail entry"));
    assert_eq!(
        reader.get_realtime_usec().expect("tail realtime"),
        1_700_004_000_000_001
    );
    assert!(reader.previous().expect("previous entry"));
    assert_eq!(
        reader.get_realtime_usec().expect("previous realtime"),
        1_700_004_000_000_000
    );
    assert!(!reader.previous().expect("start"));
}

mod facade;
mod verification;
