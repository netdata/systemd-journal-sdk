use super::*;

// Paired with Go journal/header_append_target_test.go.

fn patch(path: &Path, offset: u64, value: &[u8]) {
    let mut file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.seek(SeekFrom::Start(offset)).unwrap();
    file.write_all(value).unwrap();
}

fn read_u32(path: &Path, offset: usize) -> u32 {
    let bytes = fs::read(path).unwrap();
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn read_u64(path: &Path, offset: usize) -> u64 {
    let bytes = fs::read(path).unwrap();
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

const LIMIT: u64 = 1 << 55;

/// A named patch that makes a valid file one the writer must not extend.
type AppendTargetCase = (&'static str, fn(&Path));

/// Files systemd's writer refuses to extend and that earlier SDK releases
/// could leave behind.
fn append_target_cases() -> Vec<AppendTargetCase> {
    vec![
        ("header-size-280", |path| {
            let arena = read_u64(path, 96);
            patch(path, 88, &280u64.to_le_bytes());
            patch(path, 96, &(arena - 8).to_le_bytes());
        }),
        ("unknown-compatible-flag", |path| {
            patch(path, 8, &(read_u32(path, 8) | 1 << 3).to_le_bytes())
        }),
        ("no-tail-entry-boot-id-flag", |path| {
            patch(path, 8, &(read_u32(path, 8) & !2).to_le_bytes())
        }),
        ("archived", |path| {
            patch(path, 16, &[JournalState::Archived as u8])
        }),
        ("zero-head-realtime", |path| {
            patch(path, 184, &0u64.to_le_bytes())
        }),
        ("head-realtime-limit", |path| {
            patch(path, 184, &LIMIT.to_le_bytes())
        }),
        ("tail-realtime-limit", |path| {
            patch(path, 192, &LIMIT.to_le_bytes())
        }),
        ("tail-monotonic-limit", |path| {
            patch(path, 200, &LIMIT.to_le_bytes())
        }),
        ("null-tail-boot-id", |path| patch(path, 56, &[0; 16])),
        // This writer cannot restore sealing state, so it does not continue
        // sealed files.
        ("sealed", |path| {
            patch(path, 8, &(read_u32(path, 8) | 1).to_le_bytes())
        }),
    ]
}

#[test]
fn log_sets_aside_active_file_it_cannot_extend() {
    for strict in [false, true] {
        for (name, mutate) in append_target_cases() {
            // Default chain naming already treats an archived file as history.
            if !strict && name == "archived" {
                continue;
            }
            let dir = TempDir::new().unwrap();
            let machine_id = test_machine_id();
            let journal_dir = dir.path().join(machine_id.as_simple().to_string());
            fs::create_dir_all(&journal_dir).unwrap();
            let boot_id = test_boot_id();
            let seqnum_id = uuid::Uuid::from_u128(0x606162636465666768696a6b6c6d6e6f);
            let head_realtime = 1_700_020_300_000_000_u64;
            let path = if strict {
                journal_dir.join("system.journal")
            } else {
                journal_dir.join(format!(
                    "system@{}-{:016x}-{:016x}.journal",
                    seqnum_id.simple(),
                    1,
                    head_realtime
                ))
            };
            write_online_test_journal(&path, machine_id, boot_id, seqnum_id, head_realtime);
            mutate(&path);

            let mut log =
                Log::new(dir.path(), test_config().with_strict_systemd_naming(strict)).unwrap();
            assert_eq!(
                disposed_journal_paths(&journal_dir).len(),
                1,
                "{name}, strict={strict}"
            );
            // An out-of-range tail is not a clock floor for the new file. A fresh
            // Rust chain floors realtime at the current time (SOW-0165).
            let realtime = Microseconds::now().get() + 10_000_000;
            log.write_entry_with_timestamps(
                &[b"MESSAGE=set aside"],
                EntryTimestamps::default()
                    .with_entry_realtime_usec(realtime)
                    .with_entry_monotonic_usec(3),
            )
            .unwrap_or_else(|err| panic!("{name}, strict={strict}: {err:?}"));
            let active = log.active_file().expect("replacement active file");
            let file = File::from_path(Path::new(active.path())).expect("replacement path");
            let journal = JournalFile::<Mmap>::open(&file, 4096).expect("open replacement");
            let header = journal.journal_header_ref();
            assert_eq!(
                (header.head_entry_seqnum, header.tail_entry_seqnum),
                (3, 3),
                "{name}, strict={strict}"
            );
            assert_eq!(
                header.tail_entry_realtime, realtime,
                "{name}, strict={strict}"
            );
        }
    }
}

/// Headers every open refuses; patched into a valid online file.
fn every_open_refusal_cases() -> Vec<AppendTargetCase> {
    vec![
        ("state-3", |path| patch(path, 16, &[3])),
        ("sealed-header-232", |path| {
            let arena = read_u64(path, 96);
            patch(path, 8, &(read_u32(path, 8) | 1).to_le_bytes());
            patch(path, 88, &232u64.to_le_bytes());
            patch(path, 96, &(arena + 40).to_le_bytes());
        }),
    ]
}

fn write_refused_active(journal_dir: &Path, strict: bool, mutate: fn(&Path)) -> std::path::PathBuf {
    fs::create_dir_all(journal_dir).unwrap();
    let seqnum_id = uuid::Uuid::from_u128(0x707172737475767778797a7b7c7d7e7f);
    let head_realtime = 1_700_020_400_000_000_u64;
    let path = if strict {
        journal_dir.join("system.journal")
    } else {
        journal_dir.join(format!(
            "system@{}-{:016x}-{:016x}.journal",
            seqnum_id.simple(),
            1,
            head_realtime
        ))
    };
    write_online_test_journal(
        &path,
        test_machine_id(),
        test_boot_id(),
        seqnum_id,
        head_realtime,
    );
    mutate(&path);
    path
}

// journald sets these aside through -EBADMSG and continues the sequence.
// Paired with Go TestLogSetsAsideActiveFileWithRefusedHeader.
#[test]
fn log_sets_aside_active_file_with_refused_header() {
    for strict in [false, true] {
        for (name, mutate) in every_open_refusal_cases() {
            let dir = TempDir::new().unwrap();
            let journal_dir = dir.path().join(test_machine_id().as_simple().to_string());
            let path = write_refused_active(&journal_dir, strict, mutate);

            let mut log =
                Log::new(dir.path(), test_config().with_strict_systemd_naming(strict)).unwrap();
            // Default chain naming only treats online files as active, so
            // state 3 stays behind as history.
            let expected = usize::from(strict || name != "state-3");
            assert_eq!(
                disposed_journal_paths(&journal_dir).len(),
                expected,
                "{name}, strict={strict}"
            );
            let realtime = Microseconds::now().get() + 10_000_000;
            log.write_entry_with_timestamps(
                &[b"MESSAGE=set aside"],
                EntryTimestamps::default()
                    .with_entry_realtime_usec(realtime)
                    .with_entry_monotonic_usec(3),
            )
            .unwrap_or_else(|err| panic!("{name}, strict={strict}: {err:?}"));
            let active = log.active_file().expect("replacement active file");
            if !strict {
                assert_ne!(active.path(), path.to_str().unwrap(), "{name}");
            }
            let file = File::from_path(Path::new(active.path())).expect("replacement path");
            let journal = JournalFile::<Mmap>::open(&file, 4096).expect("open replacement");
            let header = journal.journal_header_ref();
            assert_eq!(
                (header.head_entry_seqnum, header.tail_entry_seqnum),
                (3, 3),
                "{name}, strict={strict}"
            );
        }
    }
}

// A root Log can neither keep a disposed file nor archive a header every open
// refuses, so it reports the error and leaves the file alone.
// Paired with Go TestRootLogRefusesActiveFileWithRefusedHeader.
#[test]
fn root_log_refuses_active_file_with_refused_header() {
    for (name, mutate) in every_open_refusal_cases() {
        let dir = TempDir::new().unwrap();
        let journal_dir = dir.path().join(test_machine_id().as_simple().to_string());
        let path = write_refused_active(&journal_dir, true, mutate);
        let before = fs::read(&path).unwrap();

        let config = test_config()
            .with_strict_systemd_naming(true)
            .with_root_retention(true);
        assert!(Log::new(dir.path(), config).is_err(), "{name}");
        assert_eq!(fs::read(&path).unwrap(), before, "{name}");
        assert!(disposed_journal_paths(&journal_dir).is_empty(), "{name}");
    }
}

// Paired with Go TestRootLogArchivesActiveFileItCannotExtend. Root inventory
// refuses disposed files, so a root Log archives the file instead.
#[test]
fn root_log_archives_active_file_it_cannot_extend() {
    let mut cases: Vec<AppendTargetCase> = vec![("beyond-4gib", |path| {
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_len((1 << 32) + 4096).unwrap();
    })];
    cases.extend(append_target_cases().into_iter().filter(|(name, _)| {
        // Root inventory already refused head realtime above tail realtime.
        matches!(
            *name,
            "archived"
                | "zero-head-realtime"
                | "tail-realtime-limit"
                | "tail-monotonic-limit"
                | "null-tail-boot-id"
                | "sealed"
        )
    }));
    for (name, mutate) in cases {
        let dir = TempDir::new().unwrap();
        let machine_id = test_machine_id();
        let journal_dir = dir.path().join(machine_id.as_simple().to_string());
        fs::create_dir_all(&journal_dir).unwrap();
        let seqnum_id = uuid::Uuid::from_u128(0x606162636465666768696a6b6c6d6e6f);
        let path = journal_dir.join("system.journal");
        write_online_test_journal(
            &path,
            machine_id,
            test_boot_id(),
            seqnum_id,
            1_700_020_300_000_000,
        );
        mutate(&path);
        let archive = journal_dir.join(format!(
            "system@{}-{:016x}-{:016x}.journal",
            seqnum_id.simple(),
            1,
            read_u64(&path, 184)
        ));

        let config = test_config()
            .with_strict_systemd_naming(true)
            .with_root_retention(true);
        let mut log = Log::new(dir.path(), config).unwrap_or_else(|err| panic!("{name}: {err:?}"));
        assert!(disposed_journal_paths(&journal_dir).is_empty(), "{name}");
        assert!(!path.exists(), "{name}: active path still exists");
        let file = File::from_path(&archive).expect("archive path");
        let journal = JournalFile::<Mmap>::open(&file, 4096)
            .unwrap_or_else(|err| panic!("{name}: open archive {err:?}"));
        let header = journal.journal_header_ref();
        assert_eq!(
            (header.state, header.n_entries),
            (JournalState::Archived as u8, 2),
            "{name}"
        );
        drop(journal);

        let realtime = Microseconds::now().get() + 10_000_000;
        log.write_entry_with_timestamps(
            &[b"MESSAGE=root set aside"],
            EntryTimestamps::default()
                .with_entry_realtime_usec(realtime)
                .with_entry_monotonic_usec(3),
        )
        .unwrap_or_else(|err| panic!("{name}: {err:?}"));
        let journal = JournalFile::<Mmap>::open(&File::from_path(&path).unwrap(), 4096).unwrap();
        let header = journal.journal_header_ref();
        assert_eq!(
            (header.head_entry_seqnum, header.tail_entry_seqnum),
            (3, 3),
            "{name}"
        );
    }
}

#[test]
fn log_refuses_out_of_range_timestamps_without_moving_its_clocks() {
    // A zero realtime cannot reach the writer: Log raises it to its clock floor.
    for (name, realtime, monotonic) in [
        ("realtime-limit", Some(LIMIT), 10),
        ("monotonic-limit", None, LIMIT),
    ] {
        let dir = TempDir::new().unwrap();
        let mut log = Log::new(dir.path(), test_config()).unwrap();
        let mut timestamps = EntryTimestamps::default().with_entry_monotonic_usec(monotonic);
        if let Some(realtime) = realtime {
            timestamps = timestamps.with_entry_realtime_usec(realtime);
        }
        let err = log
            .write_entry_with_timestamps(&[b"MESSAGE=out of range"], timestamps)
            .expect_err(name);
        assert!(
            matches!(err, WriterError::Journal(JournalError::TimestampOutOfRange)),
            "{name}: {err:?}"
        );
        assert_eq!(
            count_journal_files(&dir),
            0,
            "{name}: refused before rotation"
        );
        let valid = Microseconds::now().get() + 10_000_000;
        log.write_entry_with_timestamps(
            &[b"MESSAGE=valid"],
            EntryTimestamps::default()
                .with_entry_realtime_usec(valid)
                .with_entry_monotonic_usec(5),
        )
        .unwrap_or_else(|err| panic!("{name}: {err:?}"));
        let path = journal_file_path(&dir);
        let file = File::from_path(&path).unwrap();
        let journal = JournalFile::<Mmap>::open(&file, 4096).unwrap();
        let header = journal.journal_header_ref();
        assert_eq!(header.tail_entry_realtime, valid, "{name}");
        assert_eq!(header.tail_entry_monotonic, 5, "{name}");
    }
}
