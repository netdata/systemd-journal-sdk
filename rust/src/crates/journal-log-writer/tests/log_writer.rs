//! Integration tests for journal log writer
//!
//! Tests cover:
//! - Basic entry writing
//! - File rotation (size-based, count-based)
//! - Retention policies

use journal_common::{Microseconds, monotonic_now};
use journal_core::{
    error::JournalError,
    file::{
        DEFAULT_COMPRESS_THRESHOLD, FieldNamePolicy, HeaderIncompatibleFlags, JournalFile,
        JournalFileOptions, JournalState, JournalWriter, MIN_COMPRESS_THRESHOLD, Mmap, MmapMut,
        StructuredField,
    },
};
use journal_log_writer::{
    Config, EntryTimestamps, Log, LogArtifactSizer, LogIdentityMode, LogLifecycleEvent,
    LogLifecycleObserver, LogLifecycleReason, LogOpenMode, RetentionPolicy, RotationPolicy,
    WriterError,
};
use journal_registry::{Origin, repository::File};
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use tempfile::TempDir;

/// Helper to create a default test config
fn test_machine_id() -> uuid::Uuid {
    uuid::Uuid::parse_str("00112233445566778899aabbccddeeff").unwrap()
}

fn test_boot_id() -> uuid::Uuid {
    uuid::Uuid::parse_str("ffeeddccbbaa99887766554433221100").unwrap()
}

fn test_config() -> Config {
    let origin = Origin {
        machine_id: Some(test_machine_id()),
        namespace: None,
        source: journal_registry::Source::System,
    };

    Config::new(
        origin,
        RotationPolicy::default(),
        RetentionPolicy::default(),
    )
    .with_boot_id(test_boot_id())
}

#[test]
fn config_uses_systemd_compression_threshold_policy() {
    let config = test_config();
    assert_eq!(config.compression_threshold, DEFAULT_COMPRESS_THRESHOLD);

    let clamped = test_config().with_compression_threshold(1);
    assert_eq!(clamped.compression_threshold, MIN_COMPRESS_THRESHOLD);
}

/// Helper to count journal files in a directory
fn count_journal_files(dir: &TempDir) -> usize {
    let machine_id = test_machine_id();
    let journal_dir = dir.path().join(machine_id.as_simple().to_string());

    fs::read_dir(&journal_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "journal")
                .unwrap_or(false)
        })
        .count()
}

fn journal_file_path(dir: &TempDir) -> PathBuf {
    let machine_id = test_machine_id();
    let journal_dir = dir.path().join(machine_id.as_simple().to_string());

    let journal_files: Vec<_> = fs::read_dir(&journal_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "journal")
                .unwrap_or(false)
        })
        .collect();

    assert_eq!(
        journal_files.len(),
        1,
        "expected exactly one journal file in {:?}",
        journal_dir
    );
    journal_files[0].path()
}

fn journal_file_paths(dir: &TempDir) -> Vec<PathBuf> {
    let machine_id = test_machine_id();
    let journal_dir = dir.path().join(machine_id.as_simple().to_string());

    let mut journal_files: Vec<_> = fs::read_dir(&journal_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "journal")
                .unwrap_or(false)
        })
        .map(|e| e.path())
        .collect();
    journal_files.sort();
    journal_files
}

fn disposed_journal_paths(journal_dir: &Path) -> Vec<PathBuf> {
    let mut journal_files: Vec<_> = fs::read_dir(journal_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.ends_with(".journal~"))
                .unwrap_or(false)
        })
        .map(|e| e.path())
        .collect();
    journal_files.sort();
    journal_files
}

fn write_online_test_journal(
    path: &Path,
    machine_id: uuid::Uuid,
    boot_id: uuid::Uuid,
    seqnum_id: uuid::Uuid,
    head_realtime: u64,
) {
    let file = File::from_path(path).expect("journal path should parse");
    let options = JournalFileOptions::new(machine_id, boot_id, seqnum_id).with_keyed_hash(true);
    let mut journal = JournalFile::<MmapMut>::create(&file, options).unwrap();
    let mut writer = JournalWriter::new(&mut journal, 1, boot_id).unwrap();
    writer
        .add_entry(
            &mut journal,
            &[b"MESSAGE=replaceable active 0"],
            head_realtime,
            1,
        )
        .unwrap();
    writer
        .add_entry(
            &mut journal,
            &[b"MESSAGE=replaceable active 1"],
            head_realtime + 1,
            2,
        )
        .unwrap();
    journal.sync().unwrap();
}

fn clear_keyed_hash_flag(path: &Path) {
    const INCOMPATIBLE_FLAGS_OFFSET: u64 = 12;
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut buf = [0u8; 4];
    file.seek(SeekFrom::Start(INCOMPATIBLE_FLAGS_OFFSET))
        .unwrap();
    file.read_exact(&mut buf).unwrap();
    let mut flags = u32::from_le_bytes(buf);
    flags &= !(HeaderIncompatibleFlags::KeyedHash as u32);
    file.seek(SeekFrom::Start(INCOMPATIBLE_FLAGS_OFFSET))
        .unwrap();
    file.write_all(&flags.to_le_bytes()).unwrap();
}

fn write_header_size(path: &Path, header_size: u64) {
    const HEADER_SIZE_OFFSET: u64 = 88;
    let mut file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.seek(SeekFrom::Start(HEADER_SIZE_OFFSET)).unwrap();
    file.write_all(&header_size.to_le_bytes()).unwrap();
}

fn write_data_hash_table_offset(path: &Path, offset: u64) {
    const DATA_HASH_TABLE_OFFSET: u64 = 104;
    let mut file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.seek(SeekFrom::Start(DATA_HASH_TABLE_OFFSET)).unwrap();
    file.write_all(&offset.to_le_bytes()).unwrap();
}

fn single_entry_payloads(path: &Path) -> Vec<Vec<u8>> {
    let file = File::from_path(path).expect("journal path should parse");
    let journal = JournalFile::<Mmap>::open(&file, 4096).expect("open journal");
    let mut entry_offsets = Vec::new();
    journal
        .entry_offsets(&mut entry_offsets)
        .expect("collect entry offsets");
    assert_eq!(entry_offsets.len(), 1, "unexpected entry count");
    journal
        .entry_data_objects(entry_offsets[0])
        .expect("entry data iterator")
        .map(|item| item.map(|object| object.raw_payload().to_vec()))
        .collect::<journal_core::error::Result<Vec<_>>>()
        .expect("read entry payloads")
}

fn read_journal_json(path: &Path) -> Vec<serde_json::Value> {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping journalctl-backed assertions");
        return Vec::new();
    }

    let output = Command::new("journalctl")
        .arg("--output=json")
        .arg("--file")
        .arg(path)
        .output()
        .expect("failed to run journalctl");
    assert!(output.status.success(), "journalctl should succeed");

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect()
}

fn verify_journalctl_file(path: &Path) {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping journalctl verify assertion");
        return;
    }

    let output = Command::new("journalctl")
        .arg("--verify")
        .arg("--file")
        .arg(path)
        .output()
        .expect("failed to run journalctl --verify");
    assert!(
        output.status.success(),
        "journalctl --verify should succeed for {}:\n{}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn read_journal_directory_json(
    directory: &Path,
    matches: &[&str],
) -> Option<Vec<serde_json::Value>> {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping journalctl directory assertion");
        return None;
    }

    let output = Command::new("journalctl")
        .arg("--directory")
        .arg(directory)
        .arg("--output=json")
        .arg("--no-pager")
        .args(matches)
        .output()
        .expect("failed to run journalctl --directory");
    assert!(
        output.status.success(),
        "journalctl --directory should succeed for {}:\n{}",
        directory.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let rows = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect();
    Some(rows)
}

fn journalctl_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("journalctl")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    })
}

#[derive(Default)]
struct RecordingObserver {
    events: Mutex<Vec<LogLifecycleEvent>>,
}

impl LogLifecycleObserver for RecordingObserver {
    fn on_event(&self, event: &LogLifecycleEvent) {
        self.events
            .lock()
            .expect("lock observer events")
            .push(event.clone());
    }
}

#[derive(Default)]
struct FixedArtifactSizer {
    calls: Mutex<Vec<PathBuf>>,
}

impl LogArtifactSizer for FixedArtifactSizer {
    fn journal_artifact_size(&self, journal_path: &Path) -> journal_log_writer::Result<u64> {
        self.calls
            .lock()
            .expect("lock artifact calls")
            .push(journal_path.to_path_buf());
        Ok(4096)
    }
}

#[test]
fn test_default_active_filename_uses_netdata_chain_naming() {
    let dir = TempDir::new().unwrap();
    let mut log = Log::new(dir.path(), test_config()).unwrap();

    log.write_entry(&[b"MESSAGE=default chain naming"], None)
        .unwrap();

    let active = log.active_file().expect("active file after write");
    let name = Path::new(active.path())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap();
    assert!(
        name.starts_with("system@") && name.ends_with(".journal"),
        "active filename should use Netdata chain naming, got {name}"
    );

    let machine_id = test_machine_id();
    let strict_path = dir
        .path()
        .join(machine_id.as_simple().to_string())
        .join("system.journal");
    assert!(
        !strict_path.exists(),
        "default naming must not create system.journal"
    );
}

#[test]
fn test_open_identity_accessors_and_created_lifecycle_event() {
    let dir = TempDir::new().unwrap();
    let machine_id = uuid::Uuid::parse_str("00112233445566778899aabbccddeeff").unwrap();
    let boot_id = uuid::Uuid::parse_str("ffeeddccbbaa99887766554433221100").unwrap();

    let strict_missing_boot = Config::new(
        Origin {
            machine_id: Some(machine_id),
            namespace: None,
            source: journal_registry::Source::System,
        },
        RotationPolicy::default(),
        RetentionPolicy::default(),
    )
    .with_identity_mode(LogIdentityMode::Strict);
    let err = match Log::new(dir.path(), strict_missing_boot) {
        Ok(_) => panic!("expected strict identity failure without boot id"),
        Err(err) => err,
    };
    assert!(matches!(err, WriterError::MachineId(_)));

    let observer = Arc::new(RecordingObserver::default());
    let config = Config::new(
        Origin {
            machine_id: Some(machine_id),
            namespace: None,
            source: journal_registry::Source::System,
        },
        RotationPolicy::default(),
        RetentionPolicy::default(),
    )
    .with_identity_mode(LogIdentityMode::Strict)
    .with_boot_id(boot_id)
    .with_open_mode(LogOpenMode::Eager);
    let log = Log::new_with_lifecycle_observer(dir.path(), config, observer.clone()).unwrap();

    assert_eq!(log.configured_directory(), dir.path());
    assert_eq!(
        log.journal_directory(),
        dir.path()
            .join(machine_id.as_simple().to_string())
            .as_path()
    );
    assert_eq!(log.machine_id(), machine_id);
    assert_eq!(log.boot_id(), boot_id);
    assert!(matches!(log.source(), journal_registry::Source::System));
    assert!(log.active_path().is_some());

    let events = observer.events.lock().expect("lock observer events");
    let created = events
        .iter()
        .find_map(|event| match event {
            LogLifecycleEvent::Created { active, reason } => Some((active, reason)),
            _ => None,
        })
        .expect("expected eager creation event");
    assert_eq!(*created.1, LogLifecycleReason::EagerOpen);
    assert_eq!(Some(Path::new(created.0.path())), log.active_path());
}

#[test]
fn test_explicit_policy_zero_values_are_rejected() {
    let dir = TempDir::new().unwrap();
    let err = match Log::new(
        dir.path(),
        Config::new(
            Origin {
                machine_id: None,
                namespace: None,
                source: journal_registry::Source::System,
            },
            RotationPolicy::default().with_number_of_entries(0),
            RetentionPolicy::default(),
        ),
    ) {
        Ok(_) => panic!("expected rotation policy validation failure"),
        Err(err) => err,
    };
    assert!(matches!(err, WriterError::InvalidConfig(_)));

    let err = match Log::new(
        dir.path(),
        Config::new(
            Origin {
                machine_id: None,
                namespace: None,
                source: journal_registry::Source::System,
            },
            RotationPolicy::default(),
            RetentionPolicy::default().with_number_of_journal_files(0),
        ),
    ) {
        Ok(_) => panic!("expected retention policy validation failure"),
        Err(err) => err,
    };
    assert!(matches!(err, WriterError::InvalidConfig(_)));
}

#[test]
fn test_default_chain_reopen_preserves_sequence_identity() {
    let dir = TempDir::new().unwrap();
    {
        let mut log = Log::new(dir.path(), test_config()).unwrap();
        log.write_entry(&[b"MESSAGE=chain reopen 0"], None).unwrap();
        log.write_entry(&[b"MESSAGE=chain reopen 1"], None).unwrap();
        log.sync().unwrap();
    }
    {
        let mut log = Log::new(dir.path(), test_config()).unwrap();
        log.write_entry(&[b"MESSAGE=chain reopen 2"], None).unwrap();
        log.sync().unwrap();
    }

    let paths = journal_file_paths(&dir);
    assert_eq!(paths.len(), 2, "expected one reopened successor file");

    let mut seqnum_id = None;
    let expected_heads = [1, 3];
    for (idx, path) in paths.iter().enumerate() {
        let file = File::from_path(path).expect("journal path should parse");
        let journal = JournalFile::<Mmap>::open(&file, 4096).expect("open journal");
        let header = journal.journal_header_ref();
        if let Some(seqnum_id) = seqnum_id {
            assert_eq!(header.seqnum_id, seqnum_id, "seqnum id should resume");
        } else {
            seqnum_id = Some(header.seqnum_id);
        }
        assert_eq!(
            header.head_entry_seqnum, expected_heads[idx],
            "head sequence should continue across reopen"
        );
    }
}

#[test]
fn test_default_chain_reopens_online_file() {
    let dir = TempDir::new().unwrap();
    let machine_id = test_machine_id();
    let journal_dir = dir.path().join(machine_id.as_simple().to_string());
    fs::create_dir_all(&journal_dir).unwrap();

    let boot_id = uuid::Uuid::from_u128(0x101112131415161718191a1b1c1d1e1f);
    let seqnum_id = uuid::Uuid::from_u128(0x303132333435363738393a3b3c3d3e3f);
    let head_realtime = 1_700_010_000_000_000_u64;
    let path = journal_dir.join(format!(
        "system@{}-{:016x}-{:016x}.journal",
        seqnum_id.simple(),
        1,
        head_realtime
    ));
    let file = File::from_path(&path).expect("journal path should parse");
    let options = JournalFileOptions::new(machine_id, boot_id, seqnum_id).with_keyed_hash(true);
    let mut journal = JournalFile::<MmapMut>::create(&file, options).unwrap();
    let mut writer = JournalWriter::new(&mut journal, 1, boot_id).unwrap();
    writer
        .add_entry(
            &mut journal,
            &[b"MESSAGE=online reopen 0"],
            head_realtime,
            1,
        )
        .unwrap();
    writer
        .add_entry(
            &mut journal,
            &[b"MESSAGE=online reopen 1"],
            head_realtime + 1,
            2,
        )
        .unwrap();
    journal.sync().unwrap();
    drop(journal);

    let corrupt_path = journal_dir.join(format!(
        "system@{}-{:016x}-{:016x}.journal",
        uuid::Uuid::from_u128(0x202122232425262728292a2b2c2d2e2f).simple(),
        0,
        0
    ));
    fs::write(&corrupt_path, b"not a journal").unwrap();

    let config =
        test_config().with_rotation_policy(RotationPolicy::default().with_number_of_entries(3));
    let mut log = Log::new(dir.path(), config).unwrap();
    let active = log.active_file().expect("active file after reopen");
    assert_eq!(active.path(), path.to_str().unwrap());

    log.write_entry_with_timestamps(
        &[b"MESSAGE=online reopen 2"],
        EntryTimestamps::default()
            .with_entry_realtime_usec(head_realtime + 2)
            .with_entry_monotonic_usec(3),
    )
    .unwrap();
    log.write_entry_with_timestamps(
        &[b"MESSAGE=online reopen 3"],
        EntryTimestamps::default()
            .with_entry_realtime_usec(head_realtime + 3)
            .with_entry_monotonic_usec(4),
    )
    .unwrap();
    log.sync().unwrap();

    let paths = journal_file_paths(&dir);
    let valid_paths: Vec<_> = paths
        .into_iter()
        .filter(|path| path != &corrupt_path)
        .collect();
    assert_eq!(
        valid_paths.len(),
        2,
        "reopened file should rotate at count limit"
    );
    let first_file = File::from_path(&valid_paths[0]).expect("journal path should parse");
    let first_journal = JournalFile::<Mmap>::open(&first_file, 4096).expect("open first journal");
    assert_eq!(first_journal.journal_header_ref().tail_entry_seqnum, 3);
    let second_file = File::from_path(&valid_paths[1]).expect("journal path should parse");
    let second_journal =
        JournalFile::<Mmap>::open(&second_file, 4096).expect("open second journal");
    assert_eq!(second_journal.journal_header_ref().head_entry_seqnum, 4);
}

#[test]
fn test_default_chain_discards_empty_online_file_and_continues_sequence() {
    let dir = TempDir::new().unwrap();
    {
        let mut log = Log::new(dir.path(), test_config()).unwrap();
        log.write_entry(&[b"MESSAGE=empty reopen 0"], None).unwrap();
        log.write_entry(&[b"MESSAGE=empty reopen 1"], None).unwrap();
        log.close().unwrap();
    }

    let machine_id = test_machine_id();
    let journal_dir = dir.path().join(machine_id.as_simple().to_string());
    let paths = journal_file_paths(&dir);
    assert_eq!(paths.len(), 1, "expected one initial archive");
    let first_file = File::from_path(&paths[0]).expect("journal path should parse");
    let first_journal = JournalFile::<Mmap>::open(&first_file, 4096).expect("open first journal");
    let first_header = first_journal.journal_header_ref();
    let seqnum_id = uuid::Uuid::from_bytes(first_header.seqnum_id);
    let next_seqnum = first_header.tail_entry_seqnum + 1;
    drop(first_journal);

    let empty_path = journal_dir.join(format!(
        "system@{}-{:016x}-{:016x}.journal",
        seqnum_id.simple(),
        next_seqnum,
        1_700_010_000_000_010_u64
    ));
    let empty_file = File::from_path(&empty_path).expect("empty journal path should parse");
    let boot_id = test_boot_id();
    let options = JournalFileOptions::new(machine_id, boot_id, seqnum_id).with_keyed_hash(true);
    let empty_journal = JournalFile::<MmapMut>::create(&empty_file, options).unwrap();
    drop(empty_journal);

    {
        let mut log = Log::new(dir.path(), test_config()).unwrap();
        log.write_entry(&[b"MESSAGE=empty reopen 2"], None).unwrap();
        log.close().unwrap();
    }

    assert!(
        !empty_path.exists(),
        "empty online file should be discarded before append"
    );
    let paths = journal_file_paths(&dir);
    assert_eq!(paths.len(), 2, "expected original and successor archives");
    let expected_heads = [1, 3];
    let expected_tails = [2, 3];
    for (idx, path) in paths.iter().enumerate() {
        let file = File::from_path(path).expect("journal path should parse");
        let journal = JournalFile::<Mmap>::open(&file, 4096).expect("open journal");
        let header = journal.journal_header_ref();
        assert_eq!(header.head_entry_seqnum, expected_heads[idx]);
        assert_eq!(header.tail_entry_seqnum, expected_tails[idx]);
    }
}

#[test]
fn test_strict_systemd_naming_uses_system_journal_active() {
    let dir = TempDir::new().unwrap();
    let config = test_config().with_strict_systemd_naming(true);
    let mut log = Log::new(dir.path(), config).unwrap();

    log.write_entry(&[b"MESSAGE=strict systemd naming"], None)
        .unwrap();

    let active = log.active_file().expect("active file after write");
    let name = Path::new(active.path())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap();
    assert_eq!(name, "system.journal");
}

#[test]
fn test_strict_systemd_naming_reopens_existing_system_journal() {
    let dir = TempDir::new().unwrap();
    let machine_id = test_machine_id();
    let journal_dir = dir.path().join(machine_id.as_simple().to_string());
    fs::create_dir_all(&journal_dir).unwrap();

    let boot_id = uuid::Uuid::from_u128(0x1112131415161718191a1b1c1d1e1f20);
    let seqnum_id = uuid::Uuid::from_u128(0x2122232425262728292a2b2c2d2e2f30);
    let head_realtime = 1_700_020_000_000_000_u64;
    let path = journal_dir.join("system.journal");
    let file = File::from_path(&path).expect("system journal path should parse");
    let options = JournalFileOptions::new(machine_id, boot_id, seqnum_id).with_keyed_hash(true);
    let mut journal = JournalFile::<MmapMut>::create(&file, options).unwrap();
    let mut writer = JournalWriter::new(&mut journal, 1, boot_id).unwrap();
    writer
        .add_entry(
            &mut journal,
            &[b"MESSAGE=strict stale reopen 0"],
            head_realtime,
            1,
        )
        .unwrap();
    journal.sync().unwrap();
    drop(journal);

    let config = test_config().with_strict_systemd_naming(true);
    let mut log = Log::new(dir.path(), config).unwrap();
    assert_eq!(
        log.active_file().expect("strict stale active file").path(),
        path.to_str().unwrap()
    );

    log.write_entry_with_timestamps(
        &[b"MESSAGE=strict stale reopen 1"],
        EntryTimestamps::default()
            .with_entry_realtime_usec(head_realtime + 1)
            .with_entry_monotonic_usec(2),
    )
    .unwrap();
    log.close().unwrap();

    let paths = journal_file_paths(&dir);
    assert_eq!(
        paths.len(),
        1,
        "strict close should archive the reopened file"
    );
    let archived = File::from_path(&paths[0]).expect("archived journal path should parse");
    let journal = JournalFile::<Mmap>::open(&archived, 4096).expect("open archived journal");
    let header = journal.journal_header_ref();
    assert_eq!(header.seqnum_id, seqnum_id.as_bytes().to_owned());
    assert_eq!(header.tail_entry_seqnum, 2);
}

#[test]
fn test_strict_systemd_naming_archives_online_chain_active() {
    let dir = TempDir::new().unwrap();
    let machine_id = test_machine_id();
    let journal_dir = dir.path().join(machine_id.as_simple().to_string());
    fs::create_dir_all(&journal_dir).unwrap();

    let boot_id = uuid::Uuid::from_u128(0x1112131415161718191a1b1c1d1e1f21);
    let seqnum_id = uuid::Uuid::from_u128(0x2122232425262728292a2b2c2d2e2f31);
    let head_realtime = 1_700_025_000_000_000_u64;
    let chain_path = journal_dir.join(format!(
        "system@{}-{:016x}-{:016x}.journal",
        seqnum_id.simple(),
        1,
        head_realtime
    ));
    let chain_file = File::from_path(&chain_path).expect("chain path should parse");
    let options = JournalFileOptions::new(machine_id, boot_id, seqnum_id).with_keyed_hash(true);
    let mut journal = JournalFile::<MmapMut>::create(&chain_file, options).unwrap();
    let mut writer = JournalWriter::new(&mut journal, 1, boot_id).unwrap();
    for seqnum in 1..=2 {
        let message = format!("MESSAGE=strict migrate {seqnum}");
        writer
            .add_entry(
                &mut journal,
                &[message.as_bytes()],
                head_realtime + seqnum,
                seqnum,
            )
            .unwrap();
    }
    journal.sync().unwrap();
    drop(journal);

    let mut log = Log::new(dir.path(), test_config().with_strict_systemd_naming(true)).unwrap();
    let reopened = JournalFile::<Mmap>::open(&chain_file, 4096).expect("open archived chain");
    assert_eq!(
        reopened.journal_header_ref().state,
        JournalState::Archived as u8,
        "strict mode must not leave the chain-named file ONLINE"
    );

    log.write_entry_with_timestamps(
        &[b"MESSAGE=strict migrate 3"],
        EntryTimestamps::default()
            .with_entry_realtime_usec(head_realtime + 3)
            .with_entry_monotonic_usec(3),
    )
    .unwrap();
    let active = log.active_file().expect("strict active after append");
    let active_name = Path::new(active.path())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap();
    assert_eq!(active_name, "system.journal");
    let active_journal = JournalFile::<Mmap>::open(active, 4096).expect("open strict active");
    let active_header = active_journal.journal_header_ref();
    assert_eq!(active_header.head_entry_seqnum, 3);
    assert_eq!(active_header.tail_entry_seqnum, 3);
}

#[test]
fn test_default_chain_tail_ignores_lower_strict_system_journal() {
    let dir = TempDir::new().unwrap();
    let machine_id = test_machine_id();
    let journal_dir = dir.path().join(machine_id.as_simple().to_string());
    fs::create_dir_all(&journal_dir).unwrap();

    let chain_boot_id = uuid::Uuid::from_u128(0x3132333435363738393a3b3c3d3e3f40);
    let chain_seqnum_id = uuid::Uuid::from_u128(0x4142434445464748494a4b4c4d4e4f50);
    let chain_head_realtime = 1_700_030_000_000_000_u64;
    let chain_path = journal_dir.join(format!(
        "system@{}-{:016x}-{:016x}.journal",
        chain_seqnum_id.simple(),
        1,
        chain_head_realtime
    ));
    let chain_file = File::from_path(&chain_path).expect("chain path should parse");
    let chain_options =
        JournalFileOptions::new(machine_id, chain_boot_id, chain_seqnum_id).with_keyed_hash(true);
    let mut chain_journal = JournalFile::<MmapMut>::create(&chain_file, chain_options).unwrap();
    let mut chain_writer = JournalWriter::new(&mut chain_journal, 1, chain_boot_id).unwrap();
    for seqnum in 1..=5 {
        let message = format!("MESSAGE=chain tail {seqnum}");
        chain_writer
            .add_entry(
                &mut chain_journal,
                &[message.as_bytes()],
                chain_head_realtime + seqnum,
                seqnum,
            )
            .unwrap();
    }
    chain_journal.sync().unwrap();
    drop(chain_journal);

    let strict_boot_id = uuid::Uuid::from_u128(0x5152535455565758595a5b5c5d5e5f60);
    let strict_seqnum_id = uuid::Uuid::from_u128(0x6162636465666768696a6b6c6d6e6f70);
    let strict_path = journal_dir.join("system.journal");
    let strict_file = File::from_path(&strict_path).expect("strict path should parse");
    let strict_options =
        JournalFileOptions::new(machine_id, strict_boot_id, strict_seqnum_id).with_keyed_hash(true);
    let mut strict_journal = JournalFile::<MmapMut>::create(&strict_file, strict_options).unwrap();
    let mut strict_writer = JournalWriter::new(&mut strict_journal, 1, strict_boot_id).unwrap();
    for seqnum in 1..=2 {
        let message = format!("MESSAGE=strict tail {seqnum}");
        strict_writer
            .add_entry(
                &mut strict_journal,
                &[message.as_bytes()],
                chain_head_realtime + seqnum,
                seqnum,
            )
            .unwrap();
    }
    strict_journal.sync().unwrap();
    drop(strict_journal);

    let mut log = Log::new(dir.path(), test_config()).unwrap();
    log.write_entry_with_timestamps(
        &[b"MESSAGE=default chain continues"],
        EntryTimestamps::default()
            .with_entry_realtime_usec(chain_head_realtime + 6)
            .with_entry_monotonic_usec(6),
    )
    .unwrap();
    log.sync().unwrap();

    let reopened = JournalFile::<Mmap>::open(&chain_file, 4096).expect("open chain journal");
    let header = reopened.journal_header_ref();
    assert_eq!(header.seqnum_id, chain_seqnum_id.as_bytes().to_owned());
    assert_eq!(header.tail_entry_seqnum, 6);
}

#[test]
fn test_custom_source_naming_is_honored_in_default_and_strict_modes() {
    let dir = TempDir::new().unwrap();
    let origin = Origin {
        machine_id: None,
        namespace: None,
        source: journal_registry::Source::Unknown("custom-source".to_string()),
    };
    let mut log = Log::new(
        dir.path(),
        Config::new(
            origin.clone(),
            RotationPolicy::default(),
            RetentionPolicy::default(),
        ),
    )
    .unwrap();
    log.write_entry(&[b"MESSAGE=custom default source"], None)
        .unwrap();
    let active = log.active_file().expect("active default file");
    let name = Path::new(active.path())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap();
    assert!(
        name.starts_with("custom-source@"),
        "default custom source filename should use custom-source@, got {name}"
    );

    let strict_dir = TempDir::new().unwrap();
    let mut strict_log = Log::new(
        strict_dir.path(),
        Config::new(
            origin,
            RotationPolicy::default(),
            RetentionPolicy::default(),
        )
        .with_strict_systemd_naming(true),
    )
    .unwrap();
    strict_log
        .write_entry(&[b"MESSAGE=custom strict source"], None)
        .unwrap();
    let active = strict_log.active_file().expect("active strict file");
    let name = Path::new(active.path())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap();
    assert_eq!(name, "custom-source.journal");
}

fn parse_u64_field(row: &serde_json::Value, key: &str) -> Option<u64> {
    row.get(key)?.as_str()?.parse::<u64>().ok()
}

#[test]
fn test_write_single_entry() {
    let dir = TempDir::new().unwrap();
    let config = test_config();

    let mut log = Log::new(dir.path(), config).unwrap();

    let entry = [b"MESSAGE=Hello, World!" as &[u8], b"PRIORITY=6"];

    log.write_entry(&entry, None).unwrap();
    log.sync().unwrap();

    // Verify file was created
    assert_eq!(count_journal_files(&dir), 1);
}

#[test]
fn test_write_multiple_entries() {
    let dir = TempDir::new().unwrap();
    let config = test_config();

    let mut log = Log::new(dir.path(), config).unwrap();

    // Write 10 entries
    for i in 0..10 {
        let message = format!("MESSAGE=Entry {}", i);
        let entry = [message.as_bytes(), b"PRIORITY=6"];
        log.write_entry(&entry, None).unwrap();
    }

    log.sync().unwrap();

    // Should still be 1 file
    assert_eq!(count_journal_files(&dir), 1);
}

#[test]
fn test_rotation_by_entry_count() {
    let dir = TempDir::new().unwrap();

    // Rotate after 5 entries
    let rotation = RotationPolicy::default().with_number_of_entries(5);
    let config = test_config().with_rotation_policy(rotation);

    let mut log = Log::new(dir.path(), config).unwrap();

    // Write 12 entries (should create 3 files: 5 + 5 + 2)
    for i in 0..12 {
        let message = format!("MESSAGE=Entry {}", i);
        let entry = [message.as_bytes(), b"PRIORITY=6"];
        log.write_entry(&entry, None).unwrap();
    }

    log.sync().unwrap();

    assert_eq!(count_journal_files(&dir), 3);
}

#[test]
fn test_rotation_by_file_size() {
    let dir = TempDir::new().unwrap();

    // Rotate at ~50KB (small for testing)
    let rotation = RotationPolicy::default().with_size_of_journal_file(50 * 1024);
    let config = test_config().with_rotation_policy(rotation);

    let mut log = Log::new(dir.path(), config).unwrap();

    // Write entries with large messages to trigger size-based rotation
    for i in 0..100 {
        let message = format!(
            "MESSAGE=Entry {} with lots of padding: {}",
            i,
            "x".repeat(1000)
        );
        let entry = [message.as_bytes(), b"PRIORITY=6"];
        log.write_entry(&entry, None).unwrap();
    }

    log.sync().unwrap();

    // Should have rotated at least once
    assert!(count_journal_files(&dir) > 1);
}

#[test]
fn test_rotation_by_duration() {
    let dir = TempDir::new().unwrap();
    let base = 1_900_000_000_000_000_u64;
    let rotation =
        RotationPolicy::default().with_duration_of_journal_file(std::time::Duration::from_secs(10));
    let config = test_config().with_rotation_policy(rotation);

    let mut log = Log::new(dir.path(), config).unwrap();
    for (index, realtime) in [base, base + 9_999_999, base + 10_000_000]
        .into_iter()
        .enumerate()
    {
        let message = format!("MESSAGE=duration rotation {index}");
        log.write_entry_with_timestamps(
            &[message.as_bytes()],
            EntryTimestamps::default()
                .with_entry_realtime_usec(realtime)
                .with_entry_monotonic_usec(index as u64 + 1),
        )
        .unwrap();
    }
    log.close().unwrap();

    let files = journal_file_paths(&dir);
    assert_eq!(
        files.len(),
        2,
        "duration rotation should split entries across two files"
    );
    let entry_counts: Vec<u64> = files
        .iter()
        .map(|path| {
            let file = File::from_path(path).expect("duration rotation path should parse");
            let journal = JournalFile::<Mmap>::open(&file, 4096).expect("open duration journal");
            journal.journal_header_ref().n_entries
        })
        .collect();
    assert_eq!(entry_counts, vec![2, 1]);
}

#[test]
fn test_rotation_defaults_derive_from_retention_policy() {
    let dir = TempDir::new().unwrap();
    let max_size = 128 * 1024 * 1024_u64;
    let retention = RetentionPolicy::default()
        .with_size_of_journal_files(max_size * 20)
        .with_duration_of_journal_files(std::time::Duration::from_secs(20));
    let config = test_config().with_retention_policy(retention);

    let mut log = Log::new(dir.path(), config).unwrap();
    let base = 1_900_000_100_000_000_u64;
    for (index, realtime) in [base, base + 999_999, base + 1_000_000]
        .into_iter()
        .enumerate()
    {
        let message = format!("MESSAGE=derived rotation defaults {index}");
        log.write_entry_with_timestamps(
            &[message.as_bytes()],
            EntryTimestamps::default()
                .with_entry_realtime_usec(realtime)
                .with_entry_monotonic_usec(index as u64 + 1),
        )
        .unwrap();
    }
    log.close().unwrap();

    let files = journal_file_paths(&dir);
    assert_eq!(files.len(), 2, "derived duration should rotate at 1s");
    let file = File::from_path(&files[0]).expect("derived rotation path should parse");
    let journal = JournalFile::<Mmap>::open(&file, 4096).expect("open derived rotation journal");
    let header = journal.journal_header_ref();
    assert_eq!(
        header.data_hash_table_size.expect("data hash table").get() / 16,
        max_size / 576
    );
    assert_eq!(
        header
            .field_hash_table_size
            .expect("field hash table")
            .get()
            / 16,
        1023
    );
}

#[test]
fn test_derived_duration_rounds_up_to_microsecond() {
    let dir = TempDir::new().unwrap();
    let base = 1_900_000_110_000_000_u64;
    let retention = RetentionPolicy::default()
        .with_duration_of_journal_files(std::time::Duration::from_micros(20_000_001));
    let config = test_config().with_retention_policy(retention);

    let mut log = Log::new(dir.path(), config).unwrap();
    for (index, realtime) in [base, base + 1_000_000, base + 1_000_001]
        .into_iter()
        .enumerate()
    {
        let message = format!("MESSAGE=derived duration ceiling {index}");
        log.write_entry_with_timestamps(
            &[message.as_bytes()],
            EntryTimestamps::default()
                .with_entry_realtime_usec(realtime)
                .with_entry_monotonic_usec(index as u64 + 1),
        )
        .unwrap();
    }
    log.close().unwrap();

    let files = journal_file_paths(&dir);
    assert_eq!(
        files.len(),
        2,
        "ceil-derived duration should rotate on third entry"
    );
    let entry_counts: Vec<u64> = files
        .iter()
        .map(|path| {
            let file = File::from_path(path).expect("derived duration path should parse");
            let journal =
                JournalFile::<Mmap>::open(&file, 4096).expect("open derived duration journal");
            journal.journal_header_ref().n_entries
        })
        .collect();
    assert_eq!(entry_counts, vec![2, 1]);
}

#[test]
fn test_derived_size_rotation_from_retention_policy() {
    let dir = TempDir::new().unwrap();
    let max_size = 16 * 1024 * 1024_u64;
    let retention = RetentionPolicy::default().with_size_of_journal_files(max_size * 20);
    let config = test_config().with_retention_policy(retention);

    let mut log = Log::new(dir.path(), config).unwrap();
    for index in 0..12 {
        let message = format!("MESSAGE=derived size rotation {index}");
        let payload = format!("PAYLOAD={index:05}-{}", "x".repeat(2 * 1024 * 1024));
        log.write_entry_with_timestamps(
            &[
                message.as_bytes(),
                payload.as_bytes(),
                b"TEST_ID=derived-size-rotation",
            ],
            EntryTimestamps::default()
                .with_entry_realtime_usec(1_900_000_120_000_000 + index)
                .with_entry_monotonic_usec(index + 1),
        )
        .unwrap();
    }
    log.close().unwrap();

    let files = journal_file_paths(&dir);
    assert!(
        files.len() >= 2,
        "derived size should rotate; files={files:?}"
    );
    let mut entries = 0;
    for path in files {
        let file = File::from_path(&path).expect("derived size path should parse");
        let journal = JournalFile::<Mmap>::open(&file, 4096).expect("open derived size journal");
        let header = journal.journal_header_ref();
        entries += header.n_entries;
        assert_eq!(
            header.data_hash_table_size.expect("data hash table").get() / 16,
            max_size / 576
        );
    }
    assert_eq!(entries, 12);
}

#[test]
fn test_derived_rotation_small_retention_clamps_to_minimum() {
    let dir = TempDir::new().unwrap();
    let retention = RetentionPolicy::default().with_size_of_journal_files(1_000_000);
    let config = test_config().with_retention_policy(retention);

    let mut log = Log::new(dir.path(), config).unwrap();
    log.write_entry(&[b"MESSAGE=small retention clamp"], None)
        .unwrap();
    log.close().unwrap();

    let path = journal_file_path(&dir);
    let file = File::from_path(&path).expect("small retention clamp path should parse");
    let journal =
        JournalFile::<Mmap>::open(&file, 4096).expect("open small retention clamp journal");
    let header = journal.journal_header_ref();
    assert_eq!(
        header.data_hash_table_size.expect("data hash table").get() / 16,
        2047
    );
}

#[test]
fn test_derived_rotation_compact_max_file_size_clamp() {
    let dir = TempDir::new().unwrap();
    let compact_max = u32::MAX as u64;
    let retention = RetentionPolicy::default()
        .with_size_of_journal_files(compact_max.saturating_add(4096) * 20);
    let config = test_config()
        .with_retention_policy(retention)
        .with_compact(true);

    let mut log = Log::new(dir.path(), config).unwrap();
    log.write_entry(&[b"MESSAGE=compact derived clamp"], None)
        .unwrap();
    log.close().unwrap();

    let path = journal_file_path(&dir);
    let file = File::from_path(&path).expect("compact derived clamp path should parse");
    let journal =
        JournalFile::<Mmap>::open(&file, 4096).expect("open compact derived clamp journal");
    let header = journal.journal_header_ref();
    assert_eq!(
        header.data_hash_table_size.expect("data hash table").get() / 16,
        compact_max / 576
    );
}

#[test]
fn test_explicit_rotation_overrides_retention_defaults() {
    let dir = TempDir::new().unwrap();
    let explicit_size = 64 * 1024 * 1024_u64;
    let rotation = RotationPolicy::default()
        .with_size_of_journal_file(explicit_size)
        .with_duration_of_journal_file(std::time::Duration::from_secs(2));
    let retention = RetentionPolicy::default()
        .with_size_of_journal_files(128 * 1024 * 1024_u64 * 20)
        .with_duration_of_journal_files(std::time::Duration::from_secs(20));
    let config = test_config()
        .with_rotation_policy(rotation)
        .with_retention_policy(retention);

    let mut log = Log::new(dir.path(), config).unwrap();
    log.write_entry(&[b"MESSAGE=explicit rotation override"], None)
        .unwrap();
    log.close().unwrap();

    let path = journal_file_path(&dir);
    let file = File::from_path(&path).expect("explicit rotation path should parse");
    let journal = JournalFile::<Mmap>::open(&file, 4096).expect("open explicit rotation journal");
    let header = journal.journal_header_ref();
    assert_eq!(
        header.data_hash_table_size.expect("data hash table").get() / 16,
        explicit_size / 576
    );
    assert_eq!(
        header
            .field_hash_table_size
            .expect("field hash table")
            .get()
            / 16,
        1023
    );
}

#[test]
fn test_compact_rotation_preserves_compact_format() {
    let dir = TempDir::new().unwrap();

    let rotation = RotationPolicy::default().with_number_of_entries(1);
    let config = test_config()
        .with_rotation_policy(rotation)
        .with_compact(true);
    let mut log = Log::new(dir.path(), config).unwrap();

    for i in 0..3 {
        let message = format!("MESSAGE=compact rotated entry {}", i);
        let entry = [message.as_bytes(), b"PRIORITY=6"];
        log.write_entry(&entry, None).unwrap();
    }
    log.sync().unwrap();

    let paths = journal_file_paths(&dir);
    assert!(
        paths.len() > 1,
        "expected compact log to rotate, got {:?}",
        paths
    );

    for path in paths {
        let file = File::from_path(&path).expect("rotated journal path should parse");
        let journal = JournalFile::<Mmap>::open(&file, 4096).expect("open rotated journal");
        assert!(
            journal
                .journal_header_ref()
                .has_incompatible_flag(HeaderIncompatibleFlags::Compact),
            "rotated journal should remain compact: {}",
            path.display()
        );
    }
}

#[test]
fn test_retention_by_file_count() {
    let dir = TempDir::new().unwrap();

    // Rotate after 3 entries, keep max 2 files
    let rotation = RotationPolicy::default().with_number_of_entries(3);
    let retention = RetentionPolicy::default().with_number_of_journal_files(2);
    let config = test_config()
        .with_rotation_policy(rotation)
        .with_retention_policy(retention);

    let mut log = Log::new(dir.path(), config).unwrap();

    // Write 10 entries (should create 4 files, but keep only 2)
    for i in 0..10 {
        let message = format!("MESSAGE=Entry {}", i);
        let entry = [message.as_bytes(), b"PRIORITY=6"];
        log.write_entry(&entry, None).unwrap();
    }

    log.sync().unwrap();

    let file_count = count_journal_files(&dir);
    assert_eq!(file_count, 2, "retention should include the current file");
}

#[test]
fn test_retention_by_file_count_counts_current_file() {
    let dir = TempDir::new().unwrap();

    let config = test_config()
        .with_rotation_policy(RotationPolicy::default().with_number_of_entries(1))
        .with_retention_policy(RetentionPolicy::default().with_number_of_journal_files(1));

    let mut log = Log::new(dir.path(), config).unwrap();
    for i in 0..3 {
        let message = format!("MESSAGE=current retention {i}");
        log.write_entry(&[message.as_bytes()], None).unwrap();
    }
    log.sync().unwrap();

    assert_eq!(
        count_journal_files(&dir),
        1,
        "retention must keep only the tracked current file when max_files=1"
    );
}

#[test]
fn test_strict_systemd_retention_protects_current_file_by_size() {
    let dir = TempDir::new().unwrap();

    let config = test_config()
        .with_strict_systemd_naming(true)
        .with_rotation_policy(RotationPolicy::default().with_number_of_entries(1))
        .with_retention_policy(RetentionPolicy::default().with_size_of_journal_files(1));

    let mut log = Log::new(dir.path(), config).unwrap();
    log.write_entry(&[b"MESSAGE=strict retention 0"], None)
        .unwrap();
    log.write_entry(&[b"MESSAGE=strict retention 1"], None)
        .unwrap();
    log.sync().unwrap();

    let paths = journal_file_paths(&dir);
    assert_eq!(
        paths.len(),
        1,
        "strict retention must protect the post-rotation current file"
    );
    assert_eq!(paths[0].file_name().unwrap(), "system.journal");
}

#[test]
fn test_strict_systemd_close_renames_and_reopen_continues_sequence() {
    let dir = TempDir::new().unwrap();
    let config = test_config().with_strict_systemd_naming(true);

    let mut first = Log::new(dir.path(), config.clone()).unwrap();
    first
        .write_entry_with_timestamps(
            &[b"MESSAGE=strict close 0"],
            EntryTimestamps::default()
                .with_entry_realtime_usec(1_700_020_000_000_000)
                .with_entry_monotonic_usec(1),
        )
        .unwrap();
    first.close().unwrap();

    let paths = journal_file_paths(&dir);
    assert_eq!(paths.len(), 1);
    assert!(
        paths[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("system@"),
        "strict close should archive-rename system.journal"
    );

    let mut second = Log::new(dir.path(), config).unwrap();
    second
        .write_entry_with_timestamps(
            &[b"MESSAGE=strict close 1"],
            EntryTimestamps::default()
                .with_entry_realtime_usec(1_700_020_000_000_001)
                .with_entry_monotonic_usec(2),
        )
        .unwrap();
    second.close().unwrap();

    let paths = journal_file_paths(&dir);
    let mut seqnums = Vec::new();
    for path in paths {
        let file = File::from_path(&path).expect("journal path should parse");
        let journal = JournalFile::<Mmap>::open(&file, 4096).expect("open journal");
        let header = journal.journal_header_ref();
        if header.n_entries > 0 {
            seqnums.push(header.tail_entry_seqnum);
        }
    }
    assert_eq!(seqnums, vec![1, 2]);
}

#[test]
fn test_default_chain_replaces_unsupported_online_active_file() {
    let dir = TempDir::new().unwrap();
    let machine_id = test_machine_id();
    let journal_dir = dir.path().join(machine_id.as_simple().to_string());
    fs::create_dir_all(&journal_dir).unwrap();

    let boot_id = test_boot_id();
    let seqnum_id = uuid::Uuid::from_u128(0x404142434445464748494a4b4c4d4e4f);
    let head_realtime = 1_700_020_100_000_000_u64;
    let path = journal_dir.join(format!(
        "system@{}-{:016x}-{:016x}.journal",
        seqnum_id.simple(),
        1,
        head_realtime
    ));
    write_online_test_journal(&path, machine_id, boot_id, seqnum_id, head_realtime);
    clear_keyed_hash_flag(&path);
    write_data_hash_table_offset(&path, fs::metadata(&path).unwrap().len() + 4096);

    let mut log = Log::new(dir.path(), test_config()).unwrap();
    assert!(
        !path.exists(),
        "unsupported online active file should be moved out of the way"
    );
    assert_eq!(
        disposed_journal_paths(&journal_dir).len(),
        1,
        "unsupported active file should be retained as .journal~"
    );
    log.write_entry_with_timestamps(
        &[b"MESSAGE=replaced default active"],
        EntryTimestamps::default()
            .with_entry_realtime_usec(head_realtime + 2)
            .with_entry_monotonic_usec(3),
    )
    .unwrap();
    let active = log.active_file().expect("replacement active file");
    assert_ne!(active.path(), path.to_str().unwrap());

    let file = File::from_path(Path::new(active.path())).expect("replacement journal path");
    let journal = JournalFile::<Mmap>::open(&file, 4096).expect("open replacement journal");
    let header = journal.journal_header_ref();
    assert_eq!(header.head_entry_seqnum, 3);
    assert_eq!(header.tail_entry_seqnum, 3);
}

#[test]
fn test_strict_systemd_replaces_outdated_active_file() {
    let dir = TempDir::new().unwrap();
    let machine_id = test_machine_id();
    let journal_dir = dir.path().join(machine_id.as_simple().to_string());
    fs::create_dir_all(&journal_dir).unwrap();

    let boot_id = test_boot_id();
    let seqnum_id = uuid::Uuid::from_u128(0x505152535455565758595a5b5c5d5e5f);
    let head_realtime = 1_700_020_200_000_000_u64;
    let path = journal_dir.join("system.journal");
    write_online_test_journal(&path, machine_id, boot_id, seqnum_id, head_realtime);
    write_header_size(&path, 264);

    let mut log = Log::new(dir.path(), test_config().with_strict_systemd_naming(true)).unwrap();
    assert!(
        !path.exists(),
        "outdated strict active file should be moved out of the way before append"
    );
    assert_eq!(
        disposed_journal_paths(&journal_dir).len(),
        1,
        "outdated active file should be retained as .journal~"
    );
    log.write_entry_with_timestamps(
        &[b"MESSAGE=replaced strict active"],
        EntryTimestamps::default()
            .with_entry_realtime_usec(head_realtime + 2)
            .with_entry_monotonic_usec(3),
    )
    .unwrap();
    let active = log.active_file().expect("replacement active file");
    assert_eq!(
        Path::new(active.path()).file_name().unwrap(),
        "system.journal"
    );

    let file = File::from_path(Path::new(active.path())).expect("replacement journal path");
    let journal = JournalFile::<Mmap>::open(&file, 4096).expect("open replacement journal");
    let header = journal.journal_header_ref();
    assert_eq!(header.head_entry_seqnum, 3);
    assert_eq!(header.tail_entry_seqnum, 3);
}

#[test]
fn test_retention_by_total_size() {
    let dir = TempDir::new().unwrap();

    // Rotate after 5 entries, keep max 2 files based on actual data size
    // Note: Journal files pre-allocate space (sparse files), but retention
    // is based on actual data written (append_offset), not logical file size
    let rotation = RotationPolicy::default().with_number_of_entries(5);

    // Each small entry is ~50-100 bytes, plus journal overhead (~4KB per file)
    // Set limit to ~12KB to allow 2-3 files before triggering retention
    let retention = RetentionPolicy::default().with_size_of_journal_files(12 * 1024);

    let config = test_config()
        .with_rotation_policy(rotation)
        .with_retention_policy(retention);

    let mut log = Log::new(dir.path(), config).unwrap();

    // Write 20 entries (creates 4 files of 5 entries each)
    for i in 0..20 {
        let message = format!("MESSAGE=Entry {}", i);
        let entry = [message.as_bytes(), b"PRIORITY=6"];
        log.write_entry(&entry, None).unwrap();
    }

    log.sync().unwrap();

    let file_count = count_journal_files(&dir);

    // Should have rotated (4 files), but retention should limit to 3
    // (oldest file deleted when total data size exceeds 12KB limit)
    assert!(
        file_count <= 3,
        "Size-based retention should limit files, got {}",
        file_count
    );
}

#[test]
fn test_enforce_retention_deletes_files_by_age_without_append() {
    let dir = TempDir::new().unwrap();
    let config =
        test_config().with_rotation_policy(RotationPolicy::default().with_number_of_entries(1));

    let mut first = Log::new(dir.path(), config).unwrap();
    first
        .write_entry(&[b"MESSAGE=age retention 0"], None)
        .unwrap();
    first
        .write_entry(&[b"MESSAGE=age retention 1"], None)
        .unwrap();
    first
        .write_entry(&[b"MESSAGE=age retention 2"], None)
        .unwrap();
    first.close().unwrap();
    assert_eq!(count_journal_files(&dir), 3);
    std::thread::sleep(std::time::Duration::from_millis(2));

    let retained_config = test_config().with_retention_policy(
        RetentionPolicy::default()
            .with_duration_of_journal_files(std::time::Duration::from_micros(1)),
    );
    let mut retained = Log::new(dir.path(), retained_config).unwrap();
    assert_eq!(
        count_journal_files(&dir),
        3,
        "construction must not enforce age retention"
    );
    retained.enforce_retention().unwrap();
    assert_eq!(
        count_journal_files(&dir),
        0,
        "explicit age retention should delete expired archived files"
    );
}

#[test]
fn test_lazy_retention_runs_on_first_open() {
    let dir = TempDir::new().unwrap();
    let config =
        test_config().with_rotation_policy(RotationPolicy::default().with_number_of_entries(1));

    let mut first = Log::new(dir.path(), config).unwrap();
    first
        .write_entry(&[b"MESSAGE=construction retention 0"], None)
        .unwrap();
    first
        .write_entry(&[b"MESSAGE=construction retention 1"], None)
        .unwrap();
    first.close().unwrap();
    let before = journal_file_paths(&dir);
    assert_eq!(before.len(), 2);

    let observer = Arc::new(RecordingObserver::default());
    let retained_config = test_config()
        .with_retention_policy(RetentionPolicy::default().with_number_of_journal_files(1));
    let mut retained =
        Log::new_with_lifecycle_observer(dir.path(), retained_config, observer.clone()).unwrap();
    assert_eq!(
        journal_file_paths(&dir),
        before,
        "lazy construction must not enforce retention before the writer opens"
    );

    retained
        .write_entry(
            &[
                b"MESSAGE=construction retention open",
                b"TEST_ID=rust-retention-on-open",
            ],
            None,
        )
        .unwrap();
    let active_path = PathBuf::from(retained.active_file().expect("active after append").path());
    assert_eq!(
        journal_file_paths(&dir),
        vec![active_path.clone()],
        "first lazy open should enforce retention and keep only the active file"
    );
    verify_journalctl_file(&active_path);
    if let Some(rows) = read_journal_directory_json(
        retained.journal_directory(),
        &["TEST_ID=rust-retention-on-open"],
    ) {
        assert_eq!(rows.len(), 1);
    }
    let events = observer.events.lock().expect("lock observer events");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, LogLifecycleEvent::RetainedDeleted { .. })),
        "first lazy open should report retention deletion"
    );
}

#[test]
fn test_eager_retention_runs_on_open_for_all_policies() {
    for (name, retention, use_artifacts) in [
        (
            "files",
            RetentionPolicy::default().with_number_of_journal_files(1),
            false,
        ),
        (
            "bytes",
            RetentionPolicy::default().with_size_of_journal_files(1),
            true,
        ),
        (
            "age",
            RetentionPolicy::default()
                .with_duration_of_journal_files(std::time::Duration::from_micros(1)),
            false,
        ),
    ] {
        let dir = TempDir::new().unwrap();
        let config =
            test_config().with_rotation_policy(RotationPolicy::default().with_number_of_entries(1));

        let mut first = Log::new(dir.path(), config).unwrap();
        for i in 0..3 {
            let message = format!("MESSAGE=open retention {name} {i}");
            first.write_entry(&[message.as_bytes()], None).unwrap();
        }
        first.close().unwrap();
        assert_eq!(count_journal_files(&dir), 3);
        std::thread::sleep(std::time::Duration::from_millis(2));

        let observer = Arc::new(RecordingObserver::default());
        let retained_config = test_config()
            .with_open_mode(LogOpenMode::Eager)
            .with_retention_policy(retention);
        let sizer = Arc::new(FixedArtifactSizer::default());
        let retained = if use_artifacts {
            Log::new_with_hooks(
                dir.path(),
                retained_config,
                Some(observer.clone()),
                Some(sizer.clone()),
            )
        } else {
            Log::new_with_lifecycle_observer(dir.path(), retained_config, observer.clone())
        }
        .unwrap();
        let active_path = retained
            .active_path()
            .expect("eager active path after construction")
            .to_path_buf();
        assert_eq!(
            journal_file_paths(&dir),
            vec![active_path.clone()],
            "eager open retention should keep only the active file for {name}"
        );
        verify_journalctl_file(&active_path);
        let events = observer.events.lock().expect("lock observer events");
        assert!(
            events
                .iter()
                .any(|event| matches!(event, LogLifecycleEvent::Created { reason, .. } if *reason == LogLifecycleReason::EagerOpen)),
            "eager open should report active creation for {name}"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, LogLifecycleEvent::RetainedDeleted { .. })),
            "eager open should report retention deletion for {name}"
        );
        if use_artifacts {
            assert!(
                !sizer.calls.lock().expect("lock artifact calls").is_empty(),
                "artifact sizer should be called during open-time byte retention"
            );
        }
    }
}

#[test]
fn test_enforce_retention_protects_active_file_by_age() {
    let dir = TempDir::new().unwrap();
    let config =
        test_config().with_rotation_policy(RotationPolicy::default().with_number_of_entries(1));

    let mut first = Log::new(dir.path(), config).unwrap();
    first
        .write_entry(&[b"MESSAGE=age active retention 0"], None)
        .unwrap();
    first
        .write_entry(&[b"MESSAGE=age active retention 1"], None)
        .unwrap();
    first.close().unwrap();
    assert_eq!(count_journal_files(&dir), 2);
    std::thread::sleep(std::time::Duration::from_millis(2));

    let retained_config = test_config().with_retention_policy(
        RetentionPolicy::default()
            .with_duration_of_journal_files(std::time::Duration::from_micros(1)),
    );
    let mut retained = Log::new(dir.path(), retained_config).unwrap();
    retained
        .write_entry(&[b"MESSAGE=age protected active"], None)
        .unwrap();
    let active_path = retained
        .active_file()
        .expect("active file after append")
        .path();
    let active_path = PathBuf::from(active_path);
    std::thread::sleep(std::time::Duration::from_millis(2));

    retained.enforce_retention().unwrap();
    let paths = journal_file_paths(&dir);
    assert_eq!(
        paths,
        vec![active_path.clone()],
        "age retention must delete expired archives but keep the active file"
    );
    assert!(active_path.exists());
    retained.close().unwrap();
}

#[test]
fn test_empty_entry() {
    let dir = TempDir::new().unwrap();
    let config = test_config();

    let mut log = Log::new(dir.path(), config).unwrap();

    let entry: [&[u8]; 0] = [];
    let err = log.write_entry(&entry, None).unwrap_err();
    assert!(
        err.to_string().contains("journal entry has no fields"),
        "unexpected empty entry error: {err}"
    );

    assert_eq!(count_journal_files(&dir), 0);
}

#[test]
fn test_boot_id_injection() {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping test_boot_id_injection");
        return;
    }

    let dir = TempDir::new().unwrap();
    let config = test_config();

    let mut log = Log::new(dir.path(), config).unwrap();

    // Write a single entry
    let entry = [b"MESSAGE=Test entry" as &[u8], b"PRIORITY=6"];
    log.write_entry(&entry, None).unwrap();
    log.sync().unwrap();

    // Find the created journal file
    let machine_id = test_machine_id();
    let journal_dir = dir.path().join(machine_id.as_simple().to_string());
    let journal_files: Vec<_> = fs::read_dir(&journal_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "journal")
                .unwrap_or(false)
        })
        .collect();

    assert_eq!(
        journal_files.len(),
        1,
        "Should have created exactly one journal file"
    );

    let journal_path = journal_files[0].path();
    let boot_id = test_boot_id();
    let expected_boot_id = boot_id.as_simple().to_string();

    // Use journalctl to verify _BOOT_ID field is present
    let output = Command::new("journalctl")
        .arg("--output=json")
        .arg("--file")
        .arg(&journal_path)
        .output()
        .expect("Failed to run journalctl");

    assert!(output.status.success(), "journalctl should succeed");

    let output_str = String::from_utf8_lossy(&output.stdout);

    // Check that the output contains the expected _BOOT_ID field
    let boot_id_field = format!("\"_BOOT_ID\":\"{}\"", expected_boot_id);
    assert!(
        output_str.contains(&boot_id_field),
        "_BOOT_ID field with value {} should be present in journal entry output",
        expected_boot_id
    );
}

#[test]
fn test_write_uses_machine_id_subdirectory() {
    let dir = TempDir::new().unwrap();
    let target_dir = dir.path().join("flows_raw");
    fs::create_dir_all(&target_dir).unwrap();
    let mut log = Log::new(&target_dir, test_config()).unwrap();

    let entry = [b"MESSAGE=machine id suffix" as &[u8], b"PRIORITY=6"];
    log.write_entry(&entry, None).unwrap();
    log.sync().unwrap();

    let root_files: Vec<_> = fs::read_dir(&target_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "journal")
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(
        root_files.len(),
        0,
        "expected no .journal files directly in configured directory"
    );

    let machine_id = test_machine_id();
    let machine_id_dir = target_dir.join(machine_id.as_simple().to_string());
    assert!(
        machine_id_dir.is_dir(),
        "machine-id subdirectory should be created under configured directory"
    );

    let machine_id_files: Vec<_> = fs::read_dir(&machine_id_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "journal")
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(
        machine_id_files.len(),
        1,
        "expected one .journal file under the machine-id directory"
    );
}

#[test]
fn test_entry_realtime_override_is_clamped_monotonic() {
    if !journalctl_available() {
        eprintln!(
            "journalctl not available; skipping test_entry_realtime_override_is_clamped_monotonic"
        );
        return;
    }

    let dir = TempDir::new().unwrap();
    let config = test_config();
    let mut log = Log::new(dir.path(), config).unwrap();

    let first_entry = [b"MESSAGE=first" as &[u8], b"PRIORITY=6"];
    log.write_entry(&first_entry, None).unwrap();

    let second_entry = [b"MESSAGE=second" as &[u8], b"PRIORITY=6"];
    let ts = EntryTimestamps::default().with_entry_realtime_usec(0);
    log.write_entry_with_timestamps(&second_entry, ts).unwrap();
    log.sync().unwrap();

    let path = journal_file_path(&dir);
    verify_journalctl_file(&path);
    let rows = read_journal_json(&path);

    let mut first_rt = None;
    let mut second_rt = None;
    for row in rows {
        match row.get("MESSAGE").and_then(|v| v.as_str()) {
            Some("first") => first_rt = parse_u64_field(&row, "__REALTIME_TIMESTAMP"),
            Some("second") => second_rt = parse_u64_field(&row, "__REALTIME_TIMESTAMP"),
            _ => {}
        }
    }

    let first_rt = first_rt.expect("missing first entry realtime timestamp");
    let second_rt = second_rt.expect("missing second entry realtime timestamp");
    assert_eq!(
        second_rt,
        first_rt + 1,
        "second realtime timestamp must be clamped to first + 1"
    );
}

#[test]
fn test_entry_monotonic_override_is_clamped_monotonic() {
    if !journalctl_available() {
        eprintln!(
            "journalctl not available; skipping test_entry_monotonic_override_is_clamped_monotonic"
        );
        return;
    }

    let dir = TempDir::new().unwrap();
    let config = test_config();
    let mut log = Log::new(dir.path(), config).unwrap();

    let first_entry = [b"MESSAGE=mono-first" as &[u8], b"PRIORITY=6"];
    log.write_entry(&first_entry, None).unwrap();

    let second_entry = [b"MESSAGE=mono-second" as &[u8], b"PRIORITY=6"];
    let ts = EntryTimestamps::default().with_entry_monotonic_usec(0);
    log.write_entry_with_timestamps(&second_entry, ts).unwrap();
    log.sync().unwrap();

    let path = journal_file_path(&dir);
    verify_journalctl_file(&path);
    let rows = read_journal_json(&path);

    let mut first_mono = None;
    let mut second_mono = None;
    for row in rows {
        match row.get("MESSAGE").and_then(|v| v.as_str()) {
            Some("mono-first") => first_mono = parse_u64_field(&row, "__MONOTONIC_TIMESTAMP"),
            Some("mono-second") => second_mono = parse_u64_field(&row, "__MONOTONIC_TIMESTAMP"),
            _ => {}
        }
    }

    let first_mono = first_mono.expect("missing first entry monotonic timestamp");
    let second_mono = second_mono.expect("missing second entry monotonic timestamp");
    assert_eq!(
        second_mono,
        first_mono + 1,
        "second monotonic timestamp must be clamped to first + 1"
    );
}

#[test]
fn test_source_timestamp_is_preserved_with_entry_override() {
    if !journalctl_available() {
        eprintln!(
            "journalctl not available; skipping test_source_timestamp_is_preserved_with_entry_override"
        );
        return;
    }

    let dir = TempDir::new().unwrap();
    let config = test_config();
    let mut log = Log::new(dir.path(), config).unwrap();

    let source_ts = 123_456_u64;
    let entry = [b"MESSAGE=source-ts" as &[u8], b"PRIORITY=6"];
    let ts = EntryTimestamps::default()
        .with_entry_realtime_usec(1)
        .with_source_realtime_usec(source_ts);
    log.write_entry_with_timestamps(&entry, ts).unwrap();
    log.sync().unwrap();

    let rows = read_journal_json(&journal_file_path(&dir));
    let row = rows
        .iter()
        .find(|row| row.get("MESSAGE").and_then(|v| v.as_str()) == Some("source-ts"))
        .expect("missing source-ts entry");

    let stored_source_ts = parse_u64_field(row, "_SOURCE_REALTIME_TIMESTAMP")
        .expect("missing _SOURCE_REALTIME_TIMESTAMP");
    assert_eq!(stored_source_ts, source_ts);
}

#[test]
fn test_monotonic_override_remains_strict_after_restart() {
    if !journalctl_available() {
        eprintln!(
            "journalctl not available; skipping test_monotonic_override_remains_strict_after_restart"
        );
        return;
    }

    let dir = TempDir::new().unwrap();
    let config = test_config();

    let first_monotonic = 1_000_000_u64;
    {
        let mut log = Log::new(dir.path(), config).unwrap();
        let first = [b"MESSAGE=restart-first" as &[u8], b"PRIORITY=6"];
        let ts = EntryTimestamps::default()
            .with_entry_realtime_usec(first_monotonic)
            .with_entry_monotonic_usec(first_monotonic);
        log.write_entry_with_timestamps(&first, ts).unwrap();
        log.sync().unwrap();
    }

    {
        let mut log = Log::new(dir.path(), test_config()).unwrap();
        let second = [b"MESSAGE=restart-second" as &[u8], b"PRIORITY=6"];
        // Equal monotonic override must still be bumped above the persisted tail value.
        let ts = EntryTimestamps::default()
            .with_entry_realtime_usec(1)
            .with_entry_monotonic_usec(first_monotonic);
        log.write_entry_with_timestamps(&second, ts).unwrap();
        log.sync().unwrap();
    }

    let mut first_seen = None;
    let mut second_seen = None;

    let paths = journal_file_paths(&dir);
    for file in &paths {
        verify_journalctl_file(file);
    }
    for file in paths {
        for row in read_journal_json(&file) {
            match row.get("MESSAGE").and_then(|v| v.as_str()) {
                Some("restart-first") => {
                    first_seen = parse_u64_field(&row, "__MONOTONIC_TIMESTAMP");
                }
                Some("restart-second") => {
                    second_seen = parse_u64_field(&row, "__MONOTONIC_TIMESTAMP");
                }
                _ => {}
            }
        }
    }

    let first_seen = first_seen.expect("missing first entry monotonic timestamp");
    let second_seen = second_seen.expect("missing second entry monotonic timestamp");
    assert!(
        second_seen > first_seen,
        "second monotonic timestamp must be strictly greater after restart ({} !> {})",
        second_seen,
        first_seen
    );
}

#[test]
fn test_different_boot_does_not_seed_monotonic_clamp_from_previous_tail() {
    if !journalctl_available() {
        eprintln!(
            "journalctl not available; skipping test_different_boot_does_not_seed_monotonic_clamp_from_previous_tail"
        );
        return;
    }

    let dir = TempDir::new().unwrap();
    let machine_id = uuid::Uuid::parse_str("00112233445566778899aabbccddeeff").unwrap();
    let boot_a = uuid::Uuid::parse_str("aa000000000000000000000000000001").unwrap();
    let boot_b = uuid::Uuid::parse_str("bb000000000000000000000000000002").unwrap();

    write_cross_boot_monotonic_entry(
        &dir,
        machine_id,
        boot_a,
        b"MESSAGE=cross boot first",
        1_700_003_100_000_000,
        100,
    );
    write_cross_boot_monotonic_entry(
        &dir,
        machine_id,
        boot_b,
        b"MESSAGE=cross boot second",
        1_700_003_100_000_001,
        1,
    );

    let mut rows = read_cross_boot_monotonic_rows(&dir, machine_id);
    rows.sort_by_key(|row| parse_u64_field(row, "__REALTIME_TIMESTAMP").unwrap_or(0));
    assert_eq!(rows.len(), 2, "expected two cross-boot rows");
    assert_cross_boot_monotonic_values(&rows, boot_a, boot_b);
}

fn write_cross_boot_monotonic_entry(
    dir: &TempDir,
    machine_id: uuid::Uuid,
    boot_id: uuid::Uuid,
    message: &'static [u8],
    realtime: u64,
    monotonic: u64,
) {
    let origin = Origin {
        machine_id: Some(machine_id),
        namespace: None,
        source: journal_registry::Source::System,
    };
    let config = Config::new(
        origin,
        RotationPolicy::default(),
        RetentionPolicy::default(),
    )
    .with_identity_mode(LogIdentityMode::Strict)
    .with_boot_id(boot_id);
    let mut log = Log::new(dir.path(), config).unwrap();
    let entry = [message, b"TEST_ID=cross-boot-monotonic"];
    let ts = EntryTimestamps::default()
        .with_entry_realtime_usec(realtime)
        .with_entry_monotonic_usec(monotonic);
    log.write_entry_with_timestamps(&entry, ts).unwrap();
    log.sync().unwrap();
}

fn read_cross_boot_monotonic_rows(dir: &TempDir, machine_id: uuid::Uuid) -> Vec<serde_json::Value> {
    let journal_dir = dir.path().join(machine_id.as_simple().to_string());
    let mut rows = Vec::new();
    for path in sorted_journal_paths(&journal_dir) {
        verify_journalctl_file(&path);
        rows.extend(read_journal_json(&path).into_iter().filter(|row| {
            row.get("TEST_ID").and_then(|v| v.as_str()) == Some("cross-boot-monotonic")
        }));
    }
    rows
}

fn sorted_journal_paths(journal_dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(journal_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "journal")
                .unwrap_or(false)
        })
        .map(|e| e.path())
        .collect();
    paths.sort();
    paths
}

fn assert_cross_boot_monotonic_values(
    rows: &[serde_json::Value],
    boot_a: uuid::Uuid,
    boot_b: uuid::Uuid,
) {
    let monotonics: Vec<_> = rows
        .iter()
        .map(|row| parse_u64_field(row, "__MONOTONIC_TIMESTAMP").unwrap())
        .collect();
    assert_eq!(monotonics, vec![100, 1]);

    let boot_ids: Vec<_> = rows
        .iter()
        .map(|row| {
            row.get("_BOOT_ID")
                .and_then(|v| v.as_str())
                .expect("missing _BOOT_ID")
                .to_string()
        })
        .collect();
    assert_eq!(
        boot_ids,
        vec![
            boot_a.as_simple().to_string(),
            boot_b.as_simple().to_string()
        ]
    );
}

#[test]
fn test_log_journald_policy_preserves_protected_fields() {
    if !journalctl_available() {
        eprintln!(
            "journalctl not available; skipping test_log_journald_policy_preserves_protected_fields"
        );
        return;
    }

    let dir = TempDir::new().unwrap();
    let config = test_config();
    let mut log = Log::new(dir.path(), config).unwrap();

    let entry = [
        b"MESSAGE=journald policy preserves trusted fields" as &[u8],
        b"TEST_ID=journald-field-policy",
        b"_HOSTNAME=synthetic-host",
        b"_TRANSPORT=snmptrap",
    ];
    let realtime_override = Microseconds::now().get().saturating_add(1_000_000);
    let monotonic_override = monotonic_now()
        .expect("read monotonic clock")
        .get()
        .saturating_add(1_000_000);
    let ts = EntryTimestamps::default()
        .with_entry_realtime_usec(realtime_override)
        .with_entry_monotonic_usec(monotonic_override);
    log.write_entry_with_timestamps(&entry, ts).unwrap();
    log.sync().unwrap();

    let rows = read_journal_json(&journal_file_path(&dir));
    let data_row = rows
        .iter()
        .find(|row| row.get("TEST_ID").and_then(|v| v.as_str()) == Some("journald-field-policy"))
        .expect("missing data row");

    assert_eq!(
        data_row.get("_HOSTNAME").and_then(|v| v.as_str()),
        Some("synthetic-host")
    );
    assert_eq!(
        data_row.get("_TRANSPORT").and_then(|v| v.as_str()),
        Some("snmptrap")
    );
    assert_eq!(
        parse_u64_field(data_row, "__REALTIME_TIMESTAMP"),
        Some(realtime_override)
    );
    assert_eq!(
        parse_u64_field(data_row, "__MONOTONIC_TIMESTAMP"),
        Some(monotonic_override)
    );
}

#[test]
fn test_log_journal_app_policy_drops_invalid_fields() {
    if !journalctl_available() {
        eprintln!(
            "journalctl not available; skipping test_log_journal_app_policy_drops_invalid_fields"
        );
        return;
    }

    let dir = TempDir::new().unwrap();
    let config = test_config().with_field_name_policy(FieldNamePolicy::JournalApp);
    let mut log = Log::new(dir.path(), config).unwrap();

    let entry = [
        b"MESSAGE=journal app keeps valid fields" as &[u8],
        b"TEST_ID=journal-app-field-policy",
        b"_HOSTNAME=dropped-host",
        b"foo.bar=dropped-dot",
    ];
    let ts = EntryTimestamps::default()
        .with_entry_realtime_usec(1_700_002_402_000_000)
        .with_entry_monotonic_usec(20);
    log.write_entry_with_timestamps(&entry, ts).unwrap();

    let drop_only = [b"_HOSTNAME=drop-only" as &[u8]];
    let err = log
        .write_entry_with_timestamps(
            &drop_only,
            EntryTimestamps::default()
                .with_entry_realtime_usec(1_700_002_402_000_001)
                .with_entry_monotonic_usec(21),
        )
        .expect_err("drop-only journal-app entry should fail");
    assert!(matches!(err, WriterError::EmptyEntry));

    let malformed = [b"NO_EQUALS" as &[u8]];
    let err = log
        .write_entry_with_timestamps(
            &malformed,
            EntryTimestamps::default()
                .with_entry_realtime_usec(1_700_002_402_000_002)
                .with_entry_monotonic_usec(22),
        )
        .expect_err("malformed journal-app raw payload should fail");
    assert!(matches!(
        err,
        WriterError::Journal(JournalError::InvalidField)
    ));

    let empty_name = [b"=bad" as &[u8]];
    let err = log
        .write_entry_with_timestamps(
            &empty_name,
            EntryTimestamps::default()
                .with_entry_realtime_usec(1_700_002_402_000_003)
                .with_entry_monotonic_usec(23),
        )
        .expect_err("empty journal-app raw field name should fail");
    assert!(matches!(
        err,
        WriterError::Journal(JournalError::InvalidField)
    ));

    log.sync().unwrap();
    drop(log);

    let paths = journal_file_paths(&dir);
    assert_eq!(paths.len(), 1, "expected one journal-app file");
    let rows = read_journal_json(&paths[0]);
    assert_eq!(rows.len(), 1, "unexpected row count in {:?}", paths[0]);
    let data_row = &rows[0];
    assert_eq!(
        data_row.get("MESSAGE").and_then(|v| v.as_str()),
        Some("journal app keeps valid fields")
    );
    assert!(data_row.get("_HOSTNAME").is_none());
    assert!(data_row.get("foo.bar").is_none());
}

#[test]
fn test_log_raw_policy_allows_structure_only_field_names() {
    let dir = TempDir::new().unwrap();
    let config = test_config().with_field_name_policy(FieldNamePolicy::Raw);
    let mut log = Log::new(dir.path(), config).unwrap();

    let long_payload = format!("{}=long", "a".repeat(1024)).into_bytes();
    let entry = [
        b"lowercase=ok" as &[u8],
        b"foo.bar=dot",
        b"field name=space",
        long_payload.as_slice(),
        b"BINARY=a\0=b",
    ];
    let ts = EntryTimestamps::default()
        .with_entry_realtime_usec(1_700_002_403_000_000)
        .with_entry_monotonic_usec(30);
    log.write_entry_with_timestamps(&entry, ts).unwrap();

    let invalid = [b"=bad" as &[u8]];
    log.write_entry_with_timestamps(
        &invalid,
        EntryTimestamps::default()
            .with_entry_realtime_usec(1_700_002_403_000_001)
            .with_entry_monotonic_usec(31),
    )
    .expect_err("empty raw field name should fail");
    let missing_separator = [b"NO_EQUALS" as &[u8]];
    log.write_entry_with_timestamps(
        &missing_separator,
        EntryTimestamps::default()
            .with_entry_realtime_usec(1_700_002_403_000_002)
            .with_entry_monotonic_usec(32),
    )
    .expect_err("raw payload without '=' should fail");

    log.sync().unwrap();
    drop(log);

    let paths = journal_file_paths(&dir);
    assert_eq!(paths.len(), 1, "expected one raw-policy file");
    let payloads = single_entry_payloads(&paths[0]);
    assert!(payloads.iter().any(|p| p == b"lowercase=ok"));
    assert!(payloads.iter().any(|p| p == b"foo.bar=dot"));
    assert!(payloads.iter().any(|p| p == b"field name=space"));
    assert!(payloads.iter().any(|p| p == &long_payload));
    assert!(payloads.iter().any(|p| p == b"BINARY=a\0=b"));
}

#[test]
fn test_log_write_fields_respects_journal_app_and_raw_policies() {
    let app_dir = TempDir::new().unwrap();
    let app_config = test_config().with_field_name_policy(FieldNamePolicy::JournalApp);
    let mut app_log = Log::new(app_dir.path(), app_config).unwrap();
    let app_fields = [
        StructuredField::new(b"MESSAGE", b"structured app valid"),
        StructuredField::new(b"_HOSTNAME", b"drop-host"),
        StructuredField::new(b"foo.bar", b"drop-dot"),
    ];
    app_log
        .write_fields_with_timestamps(
            &app_fields,
            EntryTimestamps::default()
                .with_entry_realtime_usec(1_700_002_404_000_000)
                .with_entry_monotonic_usec(40),
        )
        .unwrap();
    let app_drop_only = [StructuredField::new(b"_HOSTNAME", b"drop-only")];
    let err = app_log
        .write_fields_with_timestamps(
            &app_drop_only,
            EntryTimestamps::default()
                .with_entry_realtime_usec(1_700_002_404_000_001)
                .with_entry_monotonic_usec(41),
        )
        .expect_err("structured journal-app drop-only entry should fail");
    assert!(matches!(err, WriterError::EmptyEntry));
    app_log.sync().unwrap();
    drop(app_log);

    let app_paths = journal_file_paths(&app_dir);
    assert_eq!(
        app_paths.len(),
        1,
        "expected one structured app-policy file"
    );
    let app_payloads = single_entry_payloads(&app_paths[0]);
    assert!(
        app_payloads
            .iter()
            .any(|p| p == b"MESSAGE=structured app valid")
    );
    assert!(!app_payloads.iter().any(|p| p.starts_with(b"_HOSTNAME=")));
    assert!(!app_payloads.iter().any(|p| p.starts_with(b"foo.bar=")));

    let raw_dir = TempDir::new().unwrap();
    let raw_config = test_config().with_field_name_policy(FieldNamePolicy::Raw);
    let mut raw_log = Log::new(raw_dir.path(), raw_config).unwrap();
    let long_name = "a".repeat(1024);
    let long_payload = format!("{}=long", long_name).into_bytes();
    let raw_fields = [
        StructuredField::new(b"lowercase", b"ok"),
        StructuredField::new(b"foo.bar", b"dot"),
        StructuredField::new(b"field name", b"space"),
        StructuredField::new(long_name.as_bytes(), b"long"),
        StructuredField::new(b"BINARY", b"a\0=b"),
    ];
    raw_log
        .write_fields_with_timestamps(
            &raw_fields,
            EntryTimestamps::default()
                .with_entry_realtime_usec(1_700_002_405_000_000)
                .with_entry_monotonic_usec(50),
        )
        .unwrap();
    let invalid_raw = [StructuredField::new(b"BAD=NAME", b"bad")];
    raw_log
        .write_fields_with_timestamps(
            &invalid_raw,
            EntryTimestamps::default()
                .with_entry_realtime_usec(1_700_002_405_000_001)
                .with_entry_monotonic_usec(51),
        )
        .expect_err("structured raw field name containing '=' should fail");
    raw_log.sync().unwrap();
    drop(raw_log);

    let raw_paths = journal_file_paths(&raw_dir);
    assert_eq!(
        raw_paths.len(),
        1,
        "expected one structured raw-policy file"
    );
    let raw_payloads = single_entry_payloads(&raw_paths[0]);
    assert!(raw_payloads.iter().any(|p| p == b"lowercase=ok"));
    assert!(raw_payloads.iter().any(|p| p == b"foo.bar=dot"));
    assert!(raw_payloads.iter().any(|p| p == b"field name=space"));
    assert!(raw_payloads.iter().any(|p| p == &long_payload));
    assert!(raw_payloads.iter().any(|p| p == b"BINARY=a\0=b"));
}

#[test]
fn test_lifecycle_observer_reports_rotation_and_retention_deletion() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let config = Config::new(
        Origin {
            machine_id: Some(test_machine_id()),
            namespace: None,
            source: journal_registry::Source::System,
        },
        RotationPolicy::default().with_number_of_entries(1),
        RetentionPolicy::default().with_number_of_journal_files(1),
    );
    let observer = Arc::new(RecordingObserver::default());
    let mut log = Log::new(dir.path(), config)
        .expect("create log")
        .with_lifecycle_observer(observer.clone());

    log.write_entry(&[b"MESSAGE=one"], None)
        .expect("write first entry");
    log.write_entry(&[b"MESSAGE=two"], None)
        .expect("write second entry");
    log.write_entry(&[b"MESSAGE=three"], None)
        .expect("write third entry");

    let events = observer
        .events
        .lock()
        .expect("lock observer events")
        .clone();
    let rotation_count = events
        .iter()
        .filter(|event| matches!(event, LogLifecycleEvent::Rotated { .. }))
        .count();
    let deleted_files = events
        .iter()
        .find_map(|event| match event {
            LogLifecycleEvent::RetainedDeleted { files } => Some(files.clone()),
            _ => None,
        })
        .unwrap_or_default();

    assert_eq!(
        rotation_count, 2,
        "expected two rotations after three writes"
    );
    assert_eq!(deleted_files.len(), 1, "expected one retained deletion");
    assert!(
        !Path::new(deleted_files[0].path()).exists(),
        "retained file should be gone from disk: {}",
        deleted_files[0].path()
    );
}

#[test]
fn test_artifact_sizer_contributes_to_retention_bytes() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let config = Config::new(
        Origin {
            machine_id: Some(test_machine_id()),
            namespace: None,
            source: journal_registry::Source::System,
        },
        RotationPolicy::default().with_number_of_entries(1),
        RetentionPolicy::default().with_size_of_journal_files(1),
    );
    let observer = Arc::new(RecordingObserver::default());
    let sizer = Arc::new(FixedArtifactSizer::default());
    let mut log = Log::new(dir.path(), config)
        .expect("create log")
        .with_lifecycle_observer(observer.clone())
        .with_artifact_sizer(sizer.clone());

    log.write_entry(&[b"MESSAGE=artifact-retention-0"], None)
        .expect("write first entry");
    log.write_entry(&[b"MESSAGE=artifact-retention-1"], None)
        .expect("write second entry");

    assert!(
        !sizer.calls.lock().expect("lock artifact calls").is_empty(),
        "artifact sizer should be consulted during retention"
    );
    let events = observer.events.lock().expect("lock observer events");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, LogLifecycleEvent::Created { .. })),
        "first append should report active creation"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, LogLifecycleEvent::Rotated { .. })),
        "second append should rotate"
    );
    let deleted = events
        .iter()
        .find_map(|event| match event {
            LogLifecycleEvent::RetainedDeleted { files } => Some(files),
            _ => None,
        })
        .expect("artifact-inclusive retention should delete old archive");
    assert_eq!(deleted.len(), 1);
}

#[test]
fn test_lifecycle_observer_reports_missing_retention_deletions() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let config = Config::new(
        Origin {
            machine_id: Some(test_machine_id()),
            namespace: None,
            source: journal_registry::Source::System,
        },
        RotationPolicy::default().with_number_of_entries(1),
        RetentionPolicy::default().with_number_of_journal_files(1),
    );
    let observer = Arc::new(RecordingObserver::default());
    let mut log = Log::new(dir.path(), config)
        .expect("create log")
        .with_lifecycle_observer(observer.clone());

    log.write_entry(&[b"MESSAGE=one"], None)
        .expect("write first entry");
    log.write_entry(&[b"MESSAGE=two"], None)
        .expect("write second entry");

    let archived_path = journal_file_paths(&dir)
        .into_iter()
        .find(|path| path.to_string_lossy().contains('@'))
        .expect("archived path after first rotation");
    fs::remove_file(&archived_path).expect("remove archived file before retention");

    log.write_entry(&[b"MESSAGE=three"], None)
        .expect("write third entry");

    let events = observer.events.lock().expect("lock observer events");
    let retained = events
        .iter()
        .filter_map(|event| match event {
            LogLifecycleEvent::RetainedDeleted { files } => Some(files),
            _ => None,
        })
        .flatten()
        .collect::<Vec<_>>();

    assert!(
        retained
            .iter()
            .any(|file| Path::new(file.path()) == archived_path),
        "files removed from chain/accounting must still be reported for retention follow-up"
    );
}
