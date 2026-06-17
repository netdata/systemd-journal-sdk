use super::*;

#[test]
fn test_write_single_entry() {
    let dir = TempDir::new().unwrap();
    let config = test_config();

    let mut log = Log::new(dir.path(), config).unwrap();

    let entry = [b"MESSAGE=Hello, World!" as &[u8], b"PRIORITY=6"];

    write_test_entry(&mut log, &entry).unwrap();
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
        write_test_entry(&mut log, &entry).unwrap();
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
        write_test_entry(&mut log, &entry).unwrap();
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
        write_test_entry(&mut log, &entry).unwrap();
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
    write_test_entry(&mut log, &[b"MESSAGE=small retention clamp"]).unwrap();
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
    write_test_entry(&mut log, &[b"MESSAGE=compact derived clamp"]).unwrap();
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
    write_test_entry(&mut log, &[b"MESSAGE=explicit rotation override"]).unwrap();
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
        write_test_entry(&mut log, &entry).unwrap();
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
        write_test_entry(&mut log, &entry).unwrap();
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
        write_test_entry(&mut log, &[message.as_bytes()]).unwrap();
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
    write_test_entry(&mut log, &[b"MESSAGE=strict retention 0"]).unwrap();
    write_test_entry(&mut log, &[b"MESSAGE=strict retention 1"]).unwrap();
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
        write_test_entry(&mut log, &entry).unwrap();
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
    write_test_entry(&mut first, &[b"MESSAGE=age retention 0"]).unwrap();
    write_test_entry(&mut first, &[b"MESSAGE=age retention 1"]).unwrap();
    write_test_entry(&mut first, &[b"MESSAGE=age retention 2"]).unwrap();
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
    write_test_entry(&mut first, &[b"MESSAGE=construction retention 0"]).unwrap();
    write_test_entry(&mut first, &[b"MESSAGE=construction retention 1"]).unwrap();
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

    write_test_entry(
        &mut retained,
        &[
            b"MESSAGE=construction retention open",
            b"TEST_ID=rust-retention-on-open",
        ],
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
            write_test_entry(&mut first, &[message.as_bytes()]).unwrap();
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
    write_test_entry(&mut first, &[b"MESSAGE=age active retention 0"]).unwrap();
    write_test_entry(&mut first, &[b"MESSAGE=age active retention 1"]).unwrap();
    first.close().unwrap();
    assert_eq!(count_journal_files(&dir), 2);
    std::thread::sleep(std::time::Duration::from_millis(2));

    let retained_config = test_config().with_retention_policy(
        RetentionPolicy::default()
            .with_duration_of_journal_files(std::time::Duration::from_micros(1)),
    );
    let mut retained = Log::new(dir.path(), retained_config).unwrap();
    write_test_entry(&mut retained, &[b"MESSAGE=age protected active"]).unwrap();
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
