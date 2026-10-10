//! Root lifecycle rejection must preserve every file before releasing ownership.
use journal_core::file::StructuredField;
use journal_log_writer::{
    Config, EntryTimestamps, Log, LogLifecycleEvent, LogLifecycleObserver, LogOpenMode,
    RetentionPolicy, RotationPolicy, WriterError, inspect_root_retention,
};
use journal_registry::{Origin, Source};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use uuid::Uuid;

fn source() -> Source {
    Source::Unknown("history".into())
}

fn config() -> Config {
    Config::new(
        Origin {
            machine_id: Some(Uuid::from_bytes([1; 16])),
            namespace: None,
            source: source(),
        },
        RotationPolicy::default(),
        RetentionPolicy::default(),
    )
    .with_boot_id(Uuid::from_bytes([2; 16]))
    .with_strict_systemd_naming(true)
    .with_root_retention(true)
}

fn append(log: &mut Log, time: SystemTime, raw: bool) -> Result<(), WriterError> {
    let timestamps = EntryTimestamps::default()
        .with_entry_realtime_usec(time.duration_since(UNIX_EPOCH).unwrap().as_micros() as u64)
        .with_entry_monotonic_usec(1);
    if raw {
        log.write_entry_with_timestamps(&[b"MESSAGE=record"], timestamps)
    } else {
        log.write_fields_with_timestamps(&[StructuredField::new(b"MESSAGE", b"record")], timestamps)
    }
}

#[derive(Default)]
struct Events(Mutex<Vec<LogLifecycleEvent>>);

impl LogLifecycleObserver for Events {
    fn on_event(&self, event: &LogLifecycleEvent) {
        self.0.lock().unwrap().push(event.clone());
    }
}

impl Events {
    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }

    fn assert_empty(&self) {
        assert!(self.0.lock().unwrap().is_empty());
    }

    fn assert_rotated(&self) {
        let events = self.0.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], LogLifecycleEvent::Rotated { .. }));
    }
}

type Snapshot = BTreeMap<PathBuf, Option<Vec<u8>>>;

fn snapshot(path: &Path) -> Snapshot {
    fn walk(root: &Path, path: &Path, files: &mut Snapshot) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if entry.file_type().unwrap().is_dir() {
                files.insert(relative, None);
                walk(root, &path, files);
            } else {
                files.insert(relative, Some(fs::read(path).unwrap()));
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(path, path, &mut files);
    files
}

fn assert_preserved(root: &Path, before: &Snapshot) {
    let after = snapshot(root);
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>()
    );
    for (path, bytes) in before {
        assert!(
            after.get(path) == Some(bytes),
            "changed evidence: {}",
            path.display()
        );
    }
}

#[test]
fn root_rotation_rejects_replaced_active_before_mutation() {
    for raw in [false, true] {
        for duration in [false, true] {
            let dir = TempDir::new().unwrap();
            let root = dir.path().join("root");
            let policy = if duration {
                RotationPolicy::default().with_duration_of_journal_file(Duration::from_secs(2))
            } else {
                RotationPolicy::default().with_number_of_entries(2)
            };
            let events = Arc::new(Events::default());
            let mut log = Log::new_with_lifecycle_observer(
                &root,
                config().with_rotation_policy(policy),
                events.clone(),
            )
            .unwrap();
            let now = SystemTime::now();
            append(&mut log, now, raw).unwrap();
            log.sync().unwrap();
            let active = log.active_path().unwrap().to_path_buf();
            let earlier = dir.path().join("earlier.bin");
            fs::copy(&active, &earlier).unwrap();
            append(&mut log, now + Duration::from_secs(1), raw).unwrap();
            log.sync().unwrap();
            let parked = dir.path().join("original.bin");
            fs::rename(&active, &parked).unwrap();
            fs::copy(&earlier, &active).unwrap();
            assert!(log.inspect_root_retention().is_err());
            events.clear();
            let before = snapshot(dir.path());
            let result = append(&mut log, now + Duration::from_secs(2), raw);
            assert!(result.is_err(), "rotation accepted a replaced active path");
            assert_preserved(dir.path(), &before);
            assert!(!log.is_poisoned());
            events.assert_empty();
            fs::remove_file(&active).unwrap();
            fs::rename(&parked, &active).unwrap();
            append(&mut log, now + Duration::from_secs(2), raw).unwrap();
            events.assert_rotated();
            assert_eq!(
                log.inspect_root_retention()
                    .unwrap()
                    .files
                    .iter()
                    .map(|f| f.entries)
                    .sum::<u64>(),
                3
            );
            log.close_without_retention().unwrap();
        }
    }
}

// Generate an actual two-entry archive and an earlier one-entry active with
// the same archive identity, then restore the archive after the writer opens.
fn collision_log(root: &Path, events: Arc<Events>, raw: bool) -> (Log, PathBuf) {
    let mut log = Log::new(root, config()).unwrap();
    let now = SystemTime::now();
    append(&mut log, now, raw).unwrap();
    log.sync().unwrap();
    let active = log.active_path().unwrap().to_path_buf();
    let earlier = fs::read(&active).unwrap();
    append(&mut log, now + Duration::from_secs(1), raw).unwrap();
    log.close_without_retention().unwrap();
    let archive = inspect_root_retention(root, &source()).unwrap().files[0]
        .path
        .clone();
    let archived = fs::read(&archive).unwrap();
    fs::remove_file(&archive).unwrap();
    fs::write(&active, earlier).unwrap();
    let log = Log::new_with_lifecycle_observer(
        root,
        config().with_rotation_policy(RotationPolicy::default().with_number_of_entries(1)),
        events,
    )
    .unwrap();
    fs::write(&archive, archived).unwrap();
    (log, archive)
}

#[test]
fn root_rotation_rejects_occupied_archive_before_mutation() {
    for raw in [false, true] {
        let dir = TempDir::new().unwrap();
        let events = Arc::new(Events::default());
        let (mut log, archive) = collision_log(dir.path(), events.clone(), raw);
        assert!(log.inspect_root_retention().is_err());
        events.clear();
        let before = snapshot(dir.path());
        let result = append(&mut log, SystemTime::now() + Duration::from_secs(2), raw);
        assert!(result.is_err(), "rotation replaced an existing archive");
        assert_preserved(dir.path(), &before);
        assert!(!log.is_poisoned());
        events.assert_empty();
        fs::remove_file(archive).unwrap();
        append(&mut log, SystemTime::now() + Duration::from_secs(2), raw).unwrap();
        events.assert_rotated();
        log.close_without_retention().unwrap();
    }
}

#[test]
fn root_empty_writer_waits_for_first_entry_before_size_rotation() {
    for raw in [false, true] {
        let dir = TempDir::new().unwrap();
        let events = Arc::new(Events::default());
        let mut log = Log::new_with_lifecycle_observer(
            dir.path(),
            config()
                .with_open_mode(LogOpenMode::Eager)
                .with_rotation_policy(RotationPolicy::default().with_size_of_journal_file(1)),
            events.clone(),
        )
        .unwrap();
        log.set_root_retention_policy(RetentionPolicy::default())
            .unwrap();
        assert_eq!(log.inspect_root_retention().unwrap().files[0].entries, 0);
        events.clear();
        append(&mut log, SystemTime::now(), raw).unwrap();
        let inventory = log.inspect_root_retention().unwrap();
        assert_eq!(inventory.files.len(), 1);
        assert_eq!(inventory.files[0].entries, 1);
        events.assert_empty();
        append(&mut log, SystemTime::now(), raw).unwrap();
        events.assert_rotated();
        let inventory = log.inspect_root_retention().unwrap();
        assert_eq!(inventory.files.len(), 2);
        assert!(inventory.files.iter().all(|file| file.entries == 1));
        log.close_without_retention().unwrap();
    }
}

#[derive(Clone, Copy)]
enum Finish {
    Close,
    WithoutRetention,
    Drop,
}

fn reject_finish(log: Log, finish: Finish) {
    match finish {
        Finish::Close => assert!(log.close().is_err()),
        Finish::WithoutRetention => assert!(log.close_without_retention().is_err()),
        Finish::Drop => drop(log),
    }
}

#[test]
fn root_shutdown_preserves_replaced_empty_and_nonempty_actives() {
    for empty in [false, true] {
        for finish in [Finish::Close, Finish::WithoutRetention, Finish::Drop] {
            let dir = TempDir::new().unwrap();
            let root = dir.path().join("root");
            let events = Arc::new(Events::default());
            let mut log = Log::new_with_lifecycle_observer(
                &root,
                config().with_open_mode(LogOpenMode::Eager),
                events.clone(),
            )
            .unwrap();
            if !empty {
                append(&mut log, SystemTime::now(), true).unwrap();
            }
            log.sync().unwrap();
            let active = log.active_path().unwrap().to_path_buf();
            let parked = dir.path().join("original.bin");
            fs::rename(&active, &parked).unwrap();
            fs::copy(&parked, &active).unwrap();
            events.clear();
            let before = snapshot(dir.path());
            reject_finish(log, finish);
            assert_preserved(dir.path(), &before);
            events.assert_empty();
        }
    }
}

#[test]
fn root_shutdown_preserves_occupied_archive() {
    for finish in [Finish::Close, Finish::WithoutRetention, Finish::Drop] {
        let dir = TempDir::new().unwrap();
        let events = Arc::new(Events::default());
        let (log, _) = collision_log(dir.path(), events.clone(), true);
        events.clear();
        let before = snapshot(dir.path());
        reject_finish(log, finish);
        assert_preserved(dir.path(), &before);
        events.assert_empty();
    }
}

fn lazy_log(root: &Path, events: Arc<Events>, maintenance: bool) -> (Log, PathBuf, Vec<u8>) {
    let mut log = Log::new_with_lifecycle_observer(root, config(), events.clone()).unwrap();
    let now = SystemTime::now();
    append(&mut log, now, true).unwrap();
    log.sync().unwrap();
    let active = log.active_path().unwrap().to_path_buf();
    let bytes = fs::read(&active).unwrap();
    if maintenance {
        log.set_root_retention_policy(
            RetentionPolicy::default().with_duration_of_journal_files(Duration::from_secs(1)),
        )
        .unwrap();
        log.maintain_root_retention(now + Duration::from_secs(2))
            .unwrap();
    } else {
        log.close_without_retention().unwrap();
        log = Log::new_with_lifecycle_observer(root, config(), events).unwrap();
    }
    assert!(log.active_path().is_none());
    (log, active, bytes)
}

#[test]
fn root_lazy_creation_rejects_restored_active_before_truncation() {
    for raw in [false, true] {
        for maintenance in [false, true] {
            let dir = TempDir::new().unwrap();
            let events = Arc::new(Events::default());
            let (mut log, active, bytes) = lazy_log(dir.path(), events.clone(), maintenance);
            fs::write(&active, bytes).unwrap();
            events.clear();
            let before = snapshot(dir.path());
            let result = append(&mut log, SystemTime::now() + Duration::from_secs(3), raw);
            assert!(
                result.is_err(),
                "lazy creation truncated an unexpected active"
            );
            assert_preserved(dir.path(), &before);
            assert!(!log.is_poisoned());
            assert!(log.active_path().is_none());
            events.assert_empty();
            fs::remove_file(&active).unwrap();
            append(&mut log, SystemTime::now() + Duration::from_secs(3), raw).unwrap();
            assert!(matches!(
                events.0.lock().unwrap().as_slice(),
                [LogLifecycleEvent::Created { .. }]
            ));
            log.close_without_retention().unwrap();
        }
    }
}
