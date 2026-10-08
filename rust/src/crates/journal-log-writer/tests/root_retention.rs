//! Public root-retention contracts, using real journals and filesystem changes.
use journal_core::file::{
    Direction, JournalFile, JournalFileOptions, JournalReader, JournalState, JournalWriter, Mmap,
    MmapMut, StructuredField,
};
use journal_log_writer::{
    Config, EntryTimestamps, Log, LogArtifactSizer, LogLifecycleEvent, LogLifecycleObserver,
    LogOpenMode, RetentionPolicy, RotationPolicy, WriterError, inspect_root_retention,
};
use journal_registry::{Origin, Source, repository::File};
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use uuid::Uuid;

const MIB: u64 = 1024 * 1024;
const DAY: Duration = Duration::from_secs(24 * 3600);

fn source() -> Source {
    Source::Unknown("history".into())
}
fn machine(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}
fn boot() -> Uuid {
    Uuid::from_bytes([2; 16])
}
fn config() -> Config {
    Config::new(
        Origin {
            machine_id: Some(machine(1)),
            namespace: None,
            source: source(),
        },
        RotationPolicy::default(),
        RetentionPolicy::default(),
    )
    .with_boot_id(boot())
    .with_strict_systemd_naming(true)
    .with_root_retention(true)
}
fn micros(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).unwrap().as_micros() as u64
}
fn append(log: &mut Log, t: SystemTime, raw: bool) {
    let ts = EntryTimestamps::default()
        .with_entry_realtime_usec(micros(t))
        .with_entry_monotonic_usec(1);
    if raw {
        log.write_entry_with_timestamps(&[b"MESSAGE=record"], ts)
            .unwrap();
    } else {
        log.write_fields_with_timestamps(&[StructuredField::new(b"MESSAGE", b"record")], ts)
            .unwrap();
    }
}
fn patch(path: &Path, offset: u64, value: &[u8]) {
    let mut f = fs::OpenOptions::new().write(true).open(path).unwrap();
    f.seek(SeekFrom::Start(offset)).unwrap();
    f.write_all(value).unwrap();
}
fn header_u64(path: &Path, offset: u64) -> u64 {
    let mut f = fs::File::open(path).unwrap();
    let mut buf = [0; 8];
    f.seek(SeekFrom::Start(offset)).unwrap();
    f.read_exact(&mut buf).unwrap();
    u64::from_le_bytes(buf)
}
fn fixture(root: &Path, id: u8, times: &[u64], active: bool) -> PathBuf {
    let dir = root.join(machine(id).simple().to_string());
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("history.journal");
    let file = File::from_path(&path).unwrap();
    let seqnum = Uuid::new_v4();
    let opts = JournalFileOptions::new(machine(id), boot(), seqnum)
        .with_data_hash_table_buckets(32)
        .with_field_hash_table_buckets(16);
    let mut journal = JournalFile::<MmapMut>::create(&file, opts).unwrap();
    let mut writer = JournalWriter::new(&mut journal, 1, boot()).unwrap();
    for (i, &time) in times.iter().enumerate() {
        writer
            .add_entry(&mut journal, &[b"MESSAGE=fixture"], time, i as u64 + 1)
            .unwrap();
    }
    journal.journal_header_mut().state = if active {
        JournalState::Offline
    } else {
        JournalState::Archived
    } as u8;
    journal.sync().unwrap();
    let h = *journal.journal_header_ref();
    drop(writer);
    drop(journal);
    if active {
        path
    } else {
        let archived = dir.join(format!(
            "history@{}-{:016x}-{:016x}.journal",
            seqnum.simple(),
            h.head_entry_seqnum,
            h.head_entry_realtime
        ));
        fs::rename(path, &archived).unwrap();
        archived
    }
}
fn inspect(root: &Path) -> journal_log_writer::RootRetentionInventory {
    inspect_root_retention(root, &source()).unwrap()
}
fn entry_count(path: &Path) -> u64 {
    header_u64(path, 152)
}

#[test]
fn root_retention_across_machines_uses_tail_age_and_full_lengths() {
    let dir = TempDir::new().unwrap();
    let now = SystemTime::now();
    let old = micros(now - 40 * DAY);
    let young = micros(now - 20 * DAY);
    let expired = fixture(dir.path(), 21, &[old], false);
    let retained = fixture(dir.path(), 22, &[old, young], false);
    let retired = fixture(dir.path(), 23, &[old], true);
    assert_eq!(inspect(dir.path()).bytes, 3 * 8 * MIB);
    let mut log = Log::new(dir.path(), config()).unwrap();
    assert!(!retired.exists());
    assert!(log.active_path().is_none());
    log.set_root_retention_policy(
        RetentionPolicy::default().with_duration_of_journal_files(30 * DAY),
    )
    .unwrap();
    let r = log.maintain_root_retention(now).unwrap();
    assert_eq!(r.deleted_files, 2);
    assert!(r.inventory_valid);
    assert!(r.error.is_none());
    assert_eq!(r.inventory.files.len(), 1);
    assert_eq!(r.inventory.files[0].path, retained);
    assert_eq!(r.inventory.bytes, 8 * MIB);
    assert!(!expired.exists());
    assert_eq!(r.attempted_at, now);
    assert_eq!(r.last_successful_at, Some(now));
    log.close_without_retention().unwrap();
}

#[test]
fn root_retention_idle_sweeps_do_not_fragment_and_expiry_leaves_lazy_successor() {
    let dir = TempDir::new().unwrap();
    let now = SystemTime::now();
    let mut log = Log::new(
        dir.path(),
        config().with_rotation_policy(RotationPolicy::default().with_duration_of_journal_file(DAY)),
    )
    .unwrap();
    append(&mut log, now, false);
    let path = log.active_path().unwrap().to_path_buf();
    log.set_root_retention_policy(
        RetentionPolicy::default().with_duration_of_journal_files(30 * DAY),
    )
    .unwrap();
    for hour in 1..=48 {
        let r = log
            .maintain_root_retention(now + Duration::from_secs(hour * 3600))
            .unwrap();
        assert_eq!(r.inventory.files.len(), 1);
        assert_eq!(log.active_path(), Some(path.as_path()));
    }
    let r = log.maintain_root_retention(now + 31 * DAY).unwrap();
    assert_eq!(r.inventory.files.len(), 0);
    assert!(log.active_path().is_none());
    append(&mut log, now + 32 * DAY, true);
    let inv = log.inspect_root_retention().unwrap();
    assert_eq!(inv.files.len(), 1);
    assert_eq!(inv.files[0].head_seqnum, 2);
    assert_eq!(inv.files[0].entries, 1);
    log.close_without_retention().unwrap();
}

#[test]
fn root_retention_policy_rebuilds_derived_geometry_and_preserves_explicit_size() {
    for explicit in [false, true] {
        let dir = TempDir::new().unwrap();
        let now = SystemTime::now();
        let mut cfg = config().with_retention_policy(
            RetentionPolicy::default().with_size_of_journal_files(32 * 1024 * MIB),
        );
        if explicit {
            cfg = cfg.with_rotation_policy(
                RotationPolicy::default().with_size_of_journal_file(128 * MIB),
            );
        }
        let mut log = Log::new(dir.path(), cfg).unwrap();
        append(&mut log, now, false);
        let before = header_u64(log.active_path().unwrap(), 112);
        log.set_root_retention_policy(
            RetentionPolicy::default().with_size_of_journal_files(8 * MIB),
        )
        .unwrap();
        log.maintain_root_retention(now).unwrap();
        append(&mut log, now + Duration::from_secs(1), false);
        let after = header_u64(log.active_path().unwrap(), 112);
        assert_eq!(
            fs::metadata(log.active_path().unwrap()).unwrap().len(),
            8 * MIB
        );
        if explicit {
            assert_eq!(after, before);
        } else {
            assert!(after < before);
        }
        log.close_without_retention().unwrap();
    }
}

#[test]
fn root_retention_every_lazy_successor_runs_cleanup_for_both_append_shapes() {
    for raw in [false, true] {
        let dir = TempDir::new().unwrap();
        let now = SystemTime::now();
        let mut log = Log::new(
            dir.path(),
            config().with_retention_policy(
                RetentionPolicy::default().with_size_of_journal_files(1024 * MIB),
            ),
        )
        .unwrap();
        append(&mut log, now, raw);
        log.set_root_retention_policy(
            RetentionPolicy::default().with_size_of_journal_files(8 * MIB),
        )
        .unwrap();
        let before = log
            .maintain_root_retention(now - Duration::from_secs(1))
            .unwrap();
        assert_eq!(before.inventory.files.len(), 1);
        assert!(log.active_path().is_none());
        append(&mut log, now + Duration::from_secs(1), raw);
        let inv = log.inspect_root_retention().unwrap();
        assert_eq!(inv.files.len(), 1);
        assert_eq!(inv.bytes, 8 * MIB);
        assert!(inv.files[0].active);
        assert_eq!(inv.files[0].entries, 1);
        let last = log.last_root_retention_result().unwrap();
        assert!(last.attempted_at > before.attempted_at);
        assert!(last.error.is_none());
        log.close_without_retention().unwrap();
    }
}

#[test]
fn root_retention_reverted_policy_and_invalid_policy_do_not_mutate_files() {
    let dir = TempDir::new().unwrap();
    let now = SystemTime::now();
    let policy = RetentionPolicy::default().with_size_of_journal_files(1024 * MIB);
    let mut log = Log::new(dir.path(), config().with_retention_policy(policy)).unwrap();
    append(&mut log, now, false);
    let path = log.active_path().unwrap().to_path_buf();
    let file_id = fs::read(&path).unwrap()[24..40].to_vec();
    for invalid in [
        RetentionPolicy::default().with_size_of_journal_files(0),
        RetentionPolicy::default().with_number_of_journal_files(0),
        RetentionPolicy::default().with_duration_of_journal_files(Duration::ZERO),
    ] {
        assert!(log.set_root_retention_policy(invalid).is_err());
        assert_eq!(
            log.root_retention_policy().size_of_journal_files,
            policy.size_of_journal_files
        );
    }
    log.set_root_retention_policy(RetentionPolicy::default().with_size_of_journal_files(8 * MIB))
        .unwrap();
    log.set_root_retention_policy(policy).unwrap();
    log.maintain_root_retention(now).unwrap();
    assert_eq!(log.active_path(), Some(path.as_path()));
    assert_eq!(&fs::read(&path).unwrap()[24..40], file_id);
    assert_eq!(inspect(dir.path()).files.len(), 1);
    log.close_without_retention().unwrap();
}

#[test]
fn root_retention_policy_before_first_append_and_empty_eager_transition_are_lazy() {
    for eager in [false, true] {
        let dir = TempDir::new().unwrap();
        let mut cfg = config();
        if eager {
            cfg = cfg.with_open_mode(LogOpenMode::Eager);
        }
        let mut log = Log::new(dir.path(), cfg).unwrap();
        log.set_root_retention_policy(
            RetentionPolicy::default().with_size_of_journal_files(8 * MIB),
        )
        .unwrap();
        if eager {
            log.maintain_root_retention(SystemTime::now()).unwrap();
            assert!(log.active_path().is_none());
        }
        append(&mut log, SystemTime::now(), false);
        let inv = inspect(dir.path());
        assert_eq!(inv.files.len(), 1);
        assert_eq!(inv.bytes, 8 * MIB);
        assert_eq!(inv.files[0].entries, 1);
        log.close_without_retention().unwrap();
    }
}

#[test]
fn root_retention_size_and_count_evict_oldest_tail_then_path() {
    let dir = TempDir::new().unwrap();
    let now = SystemTime::now();
    let t = micros(now);
    let oldest = fixture(dir.path(), 21, &[t - 100, t - 10], false);
    let newest = fixture(dir.path(), 22, &[t - 99, t - 5], false);
    let mut log = Log::new(
        dir.path(),
        config().with_rotation_policy(RotationPolicy::default().with_size_of_journal_file(8 * MIB)),
    )
    .unwrap();
    append(&mut log, now, true);
    log.set_root_retention_policy(RetentionPolicy::default().with_size_of_journal_files(16 * MIB))
        .unwrap();
    let r = log.maintain_root_retention(now).unwrap();
    assert_eq!(r.inventory.files.len(), 2);
    assert!(!oldest.exists());
    assert!(newest.exists());
    log.set_root_retention_policy(RetentionPolicy::default().with_number_of_journal_files(1))
        .unwrap();
    log.maintain_root_retention(now).unwrap();
    assert!(!newest.exists());
    assert_eq!(inspect(dir.path()).files.len(), 1);
    log.close_without_retention().unwrap();

    let dir = TempDir::new().unwrap();
    let a = fixture(dir.path(), 21, &[t], false);
    let b = fixture(dir.path(), 22, &[t], false);
    let mut log = Log::new(dir.path(), config()).unwrap();
    log.set_root_retention_policy(RetentionPolicy::default().with_number_of_journal_files(1))
        .unwrap();
    log.maintain_root_retention(now).unwrap();
    assert!(!a.exists());
    assert!(b.exists());
    log.close_without_retention().unwrap();
}

#[test]
fn root_retention_explicit_span_survives_policy_updates() {
    let dir = TempDir::new().unwrap();
    let now = SystemTime::now();
    let mut log = Log::new(
        dir.path(),
        config().with_rotation_policy(RotationPolicy::default().with_duration_of_journal_file(DAY)),
    )
    .unwrap();
    append(&mut log, now, false);
    let saved = UNIX_EPOCH + Duration::from_micros(inspect(dir.path()).files[0].head_realtime);
    log.set_root_retention_policy(
        RetentionPolicy::default().with_duration_of_journal_files(30 * DAY),
    )
    .unwrap();
    append(&mut log, saved + DAY - Duration::from_secs(1), false);
    assert_eq!(inspect(dir.path()).files.len(), 1);
    append(&mut log, saved + DAY, true);
    assert_eq!(inspect(dir.path()).files.len(), 2);
    log.close_without_retention().unwrap();
}

#[test]
fn root_inventory_rejects_unsafe_candidates_before_any_pruning() {
    for kind in [
        "quarantine",
        "seqnum",
        "machine",
        "tail",
        "state",
        "truncate",
        "collision",
        "duplicate",
        "machine-file",
        "bad-machine",
        "unknown-flags",
    ] {
        let dir = TempDir::new().unwrap();
        let path = fixture(dir.path(), 21, &[1], false);
        match kind {
            "quarantine" => {
                fs::rename(&path, path.with_extension("journal~")).unwrap();
            }
            "seqnum" => {
                let name = path.file_name().unwrap().to_str().unwrap().replacen(
                    &fs::read(&path).unwrap()[72..88]
                        .iter()
                        .map(|v| format!("{v:02x}"))
                        .collect::<String>(),
                    &"f".repeat(32),
                    1,
                );
                fs::rename(&path, path.with_file_name(name)).unwrap();
            }
            "machine" => patch(&path, 40, &[99]),
            "tail" => patch(&path, 192, &0u64.to_le_bytes()),
            "state" => patch(&path, 16, &[JournalState::Online as u8]),
            "truncate" => fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(272)
                .unwrap(),
            "collision" => {
                fs::copy(&path, path.with_file_name("history.journal")).unwrap();
            }
            "duplicate" => {
                let other = fixture(dir.path(), 22, &[2], false);
                let data = fs::read(&path).unwrap();
                patch(&other, 24, &data[24..40]);
            }
            "machine-file" => fs::write(
                dir.path().join(machine(25).simple().to_string()),
                b"metadata",
            )
            .unwrap(),
            "bad-machine" => {
                fs::rename(path.parent().unwrap(), dir.path().join("not-a-machine")).unwrap();
            }
            "unknown-flags" => patch(&path, 12, &0x8000_0004u32.to_le_bytes()),
            _ => unreachable!(),
        }
        let mut before: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|v| v.unwrap().path())
            .collect();
        before.sort();
        assert!(
            inspect_root_retention(dir.path(), &source()).is_err(),
            "accepted {kind}"
        );
        assert!(
            Log::new(
                dir.path(),
                config().with_retention_policy(
                    RetentionPolicy::default().with_size_of_journal_files(1)
                )
            )
            .is_err(),
            "opened {kind}"
        );
        let mut after: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|v| v.unwrap().path())
            .collect();
        after.sort();
        assert_eq!(before, after, "startup changed evidence for {kind}");
        assert!(
            after
                .iter()
                .any(|p| p.file_name().unwrap() != machine(1).simple().to_string().as_str())
        );
    }
}

#[cfg(unix)]
#[test]
fn root_inventory_rejects_symlinks_and_accepts_unrelated_metadata() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new().unwrap();
    let path = fixture(dir.path(), 21, &[1], false);
    fs::create_dir(dir.path().join("identity")).unwrap();
    fs::write(dir.path().join("identity/status"), b"metadata").unwrap();
    fs::write(dir.path().join("lock"), b"metadata").unwrap();
    assert_eq!(inspect(dir.path()).files.len(), 1);
    let link = path.with_file_name("history.journal");
    symlink(&path, &link).unwrap();
    assert!(inspect_root_retention(dir.path(), &source()).is_err());
    fs::remove_file(link).unwrap();
    symlink(path.parent().unwrap(), dir.path().join("linked")).unwrap();
    assert!(inspect_root_retention(dir.path(), &source()).is_err());
}

#[test]
fn root_inventory_missing_root_and_non_opt_in_are_errors() {
    let dir = TempDir::new().unwrap();
    assert!(inspect_root_retention(&dir.path().join("missing"), &source()).is_err());
    assert!(Log::new(dir.path(), config().with_strict_systemd_naming(false)).is_err());
    let mut log = Log::new(dir.path(), config().with_root_retention(false)).unwrap();
    assert!(log.inspect_root_retention().is_err());
    assert!(log.maintain_root_retention(SystemTime::now()).is_err());
    assert!(
        log.set_root_retention_policy(RetentionPolicy::default())
            .is_err()
    );
    log.close_without_retention().unwrap();
}

#[test]
fn root_inventory_does_not_visit_entry_arrays() {
    let dir = TempDir::new().unwrap();
    let path = fixture(dir.path(), 21, &[1], false);
    let array_offset = header_u64(&path, 176);
    patch(&path, array_offset + 24, &u64::MAX.to_le_bytes());
    let inv = inspect(dir.path());
    assert_eq!(inv.files.len(), 1);
    assert_eq!(inv.files[0].entries, 1);
}

#[test]
fn root_live_inventory_detects_missing_file_or_directory_and_recovers_without_poisoning() {
    for whole_directory in [false, true] {
        let dir = TempDir::new().unwrap();
        let parked = TempDir::new().unwrap();
        let mut log = Log::new(dir.path(), config()).unwrap();
        append(&mut log, SystemTime::now(), true);
        let active = log.active_path().unwrap().to_path_buf();
        let original = if whole_directory {
            active.parent().unwrap().to_path_buf()
        } else {
            active.clone()
        };
        let displaced = parked.path().join(original.file_name().unwrap());
        let last_success = log.last_root_retention_result().unwrap().last_successful_at;
        fs::rename(&original, &displaced).unwrap();
        assert!(log.inspect_root_retention().is_err());
        assert!(log.maintain_root_retention(SystemTime::now()).is_err());
        let r = log.last_root_retention_result().unwrap();
        assert!(!r.inventory_valid);
        assert!(r.error.is_some());
        assert_eq!(r.last_successful_at, last_success);
        assert!(!log.is_poisoned());
        log.sync().unwrap();
        fs::rename(&displaced, &original).unwrap();
        log.maintain_root_retention(SystemTime::now()).unwrap();
        assert!(log.last_root_retention_result().unwrap().inventory_valid);
        log.close_without_retention().unwrap();
    }
}

#[test]
fn root_live_inventory_rejects_identical_header_replacement_inode() {
    let dir = TempDir::new().unwrap();
    let parked = TempDir::new().unwrap();
    let mut log = Log::new(dir.path(), config()).unwrap();
    append(&mut log, SystemTime::now(), false);
    let path = log.active_path().unwrap().to_path_buf();
    let moved = parked.path().join("original.journal");
    log.sync().unwrap();
    fs::rename(&path, &moved).unwrap();
    fs::copy(&moved, &path).unwrap();
    assert_eq!(inspect(dir.path()).files.len(), 1);
    assert!(log.inspect_root_retention().is_err());
    assert!(!log.is_poisoned());
    fs::remove_file(&path).unwrap();
    fs::rename(moved, &path).unwrap();
    log.close_without_retention().unwrap();
}

#[derive(Default)]
struct Events(Mutex<Vec<LogLifecycleEvent>>);
impl LogLifecycleObserver for Events {
    fn on_event(&self, event: &LogLifecycleEvent) {
        self.0.lock().unwrap().push(event.clone());
    }
}

#[test]
fn root_retention_lazy_archive_event_has_no_fabricated_successor() {
    let dir = TempDir::new().unwrap();
    let now = SystemTime::now();
    let observer = Arc::new(Events::default());
    let mut log = Log::new_with_lifecycle_observer(dir.path(), config(), observer.clone()).unwrap();
    append(&mut log, now, true);
    log.set_root_retention_policy(RetentionPolicy::default().with_duration_of_journal_files(DAY))
        .unwrap();
    log.maintain_root_retention(now + 2 * DAY).unwrap();
    assert!(log.active_path().is_none());
    let events = observer.0.lock().unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, LogLifecycleEvent::Archived { .. }))
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, LogLifecycleEvent::Rotated { .. }))
    );
    drop(events);
    log.close_without_retention().unwrap();
}

struct ForbiddenSizer;
impl LogArtifactSizer for ForbiddenSizer {
    fn journal_artifact_size(&self, _: &Path) -> Result<u64, WriterError> {
        panic!("root accounting must never call artifact sizer")
    }
}
#[test]
fn root_retention_artifact_sizer_cannot_bypass_config_validation() {
    let dir = TempDir::new().unwrap();
    assert!(
        Log::new_with_hooks(dir.path(), config(), None, Some(Arc::new(ForbiddenSizer))).is_err()
    );
    let mut log = Log::new(dir.path(), config())
        .unwrap()
        .with_artifact_sizer(Arc::new(ForbiddenSizer));
    assert!(
        log.write_entry_with_timestamps(
            &[b"MESSAGE=blocked"],
            EntryTimestamps::default().with_entry_monotonic_usec(1)
        )
        .is_err()
    );
    assert!(log.active_path().is_none());
    assert!(log.maintain_root_retention(SystemTime::now()).is_err());
    assert!(inspect(dir.path()).files.is_empty());
}

#[test]
fn root_retention_close_without_retention_keeps_history() {
    let dir = TempDir::new().unwrap();
    let now = SystemTime::now();
    let mut log = Log::new(dir.path(), config()).unwrap();
    append(&mut log, now, false);
    log.set_root_retention_policy(
        RetentionPolicy::default().with_duration_of_journal_files(Duration::from_micros(1)),
    )
    .unwrap();
    log.close_without_retention().unwrap();
    let inv = inspect(dir.path());
    assert_eq!(inv.files.len(), 1);
    assert!(!inv.files[0].active);
}

#[test]
fn root_retention_pinned_reader_survives_unlink() {
    let dir = TempDir::new().unwrap();
    let path = fixture(dir.path(), 21, &[1, 2], false);
    let file = JournalFile::<Mmap>::open_path(&path, 8 * MIB).unwrap();
    let mut reader = JournalReader::default();
    let mut log = Log::new(dir.path(), config()).unwrap();
    log.set_root_retention_policy(RetentionPolicy::default().with_duration_of_journal_files(DAY))
        .unwrap();
    log.maintain_root_retention(SystemTime::now()).unwrap();
    assert!(!path.exists());
    let mut seqs = vec![];
    while reader.step(&file, Direction::Forward).unwrap() {
        seqs.push(reader.get_seqnum(&file).unwrap().0);
    }
    assert_eq!(seqs, vec![1, 2]);
    log.close_without_retention().unwrap();
}

#[test]
fn root_retention_new_unsafe_candidate_is_reported_without_poisoning_healthy_append() {
    let dir = TempDir::new().unwrap();
    let now = SystemTime::now();
    let mut log = Log::new(
        dir.path(),
        config().with_rotation_policy(RotationPolicy::default().with_number_of_entries(1)),
    )
    .unwrap();
    append(&mut log, now, false);
    let bad = log.journal_directory().join("history.journal~");
    fs::write(&bad, b"preserve evidence").unwrap();
    append(&mut log, now + Duration::from_secs(1), true);
    assert!(log.last_root_retention_result().unwrap().error.is_some());
    assert!(!log.is_poisoned());
    assert_eq!(entry_count(log.active_path().unwrap()), 1);
    assert_eq!(fs::read(bad).unwrap(), b"preserve evidence");
    assert!(log.close_without_retention().is_ok());
}

#[test]
fn root_live_inventory_rejects_changed_opening_header_identities() {
    for offset in [24, 40, 72] {
        let dir = TempDir::new().unwrap();
        let mut log = Log::new(dir.path(), config()).unwrap();
        append(&mut log, SystemTime::now(), false);
        let path = log.active_path().unwrap().to_path_buf();
        log.sync().unwrap();
        let before = fs::read(&path).unwrap()[offset..offset + 16].to_vec();
        patch(&path, offset as u64, &[99; 16]);
        assert!(
            log.inspect_root_retention().is_err(),
            "accepted identity offset {offset}"
        );
        assert!(log.maintain_root_retention(SystemTime::now()).is_err());
        assert!(!log.is_poisoned());
        patch(&path, offset as u64, &before);
        log.maintain_root_retention(SystemTime::now()).unwrap();
        log.close_without_retention().unwrap();
    }
}

#[test]
fn default_rotation_cleanup_failure_keeps_existing_append_retry_semantics() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct FailingSizer(Arc<AtomicBool>);
    impl LogArtifactSizer for FailingSizer {
        fn journal_artifact_size(&self, _: &Path) -> Result<u64, WriterError> {
            if self.0.load(Ordering::Relaxed) {
                Err(std::io::Error::other("synthetic artifact sizing failure").into())
            } else {
                Ok(0)
            }
        }
    }
    let dir = TempDir::new().unwrap();
    let failing = Arc::new(AtomicBool::new(false));
    let mut log = Log::new_with_hooks(
        dir.path(),
        config()
            .with_root_retention(false)
            .with_rotation_policy(RotationPolicy::default().with_number_of_entries(1)),
        None,
        Some(Arc::new(FailingSizer(failing.clone()))),
    )
    .unwrap();
    append(&mut log, SystemTime::now(), true);
    failing.store(true, Ordering::Relaxed);
    let ts = EntryTimestamps::default().with_entry_monotonic_usec(2);
    assert!(
        log.write_entry_with_timestamps(&[b"MESSAGE=rotation"], ts)
            .is_err()
    );
    assert!(!log.is_poisoned());
    log.write_entry_with_timestamps(&[b"MESSAGE=retry"], ts)
        .unwrap();
    assert_eq!(entry_count(log.active_path().unwrap()), 1);
    log.close_without_retention().unwrap();
}

#[test]
fn root_close_of_unopened_or_empty_log_does_not_apply_new_policy() {
    for eager in [false, true] {
        let dir = TempDir::new().unwrap();
        let retained = fixture(dir.path(), 21, &[1], false);
        let mut cfg = config();
        if eager {
            cfg = cfg.with_open_mode(LogOpenMode::Eager);
        }
        let mut log = Log::new(dir.path(), cfg).unwrap();
        log.set_root_retention_policy(
            RetentionPolicy::default().with_duration_of_journal_files(DAY),
        )
        .unwrap();
        log.close().unwrap();
        assert!(
            retained.exists(),
            "empty/lazy close unexpectedly enforced retention"
        );
        assert_eq!(inspect(dir.path()).files.len(), 1);
    }
}
