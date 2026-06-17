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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tempfile::TempDir;

static NEXT_TEST_MONOTONIC: AtomicU64 = AtomicU64::new(1);

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

fn next_test_timestamps() -> EntryTimestamps {
    EntryTimestamps::default()
        .with_entry_monotonic_usec(NEXT_TEST_MONOTONIC.fetch_add(1, Ordering::Relaxed))
}

fn write_test_entry(log: &mut Log, items: &[&[u8]]) -> Result<(), WriterError> {
    log.write_entry_with_timestamps(items, next_test_timestamps())
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

fn parse_u64_field(row: &serde_json::Value, key: &str) -> Option<u64> {
    row.get(key)?.as_str()?.parse::<u64>().ok()
}

#[path = "log_writer/entries_policy.rs"]
mod entries_policy;
#[path = "log_writer/lifecycle.rs"]
mod lifecycle;
#[path = "log_writer/naming.rs"]
mod naming;
#[path = "log_writer/rotation_retention.rs"]
mod rotation_retention;
