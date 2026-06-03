use super::*;

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
