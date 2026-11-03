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

/// Files systemd's writer refuses to extend and that earlier SDK releases
/// could leave behind.
fn append_target_cases() -> Vec<(&'static str, fn(&Path))> {
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
