use super::*;

#[test]
fn test_empty_entry() {
    let dir = TempDir::new().unwrap();
    let config = test_config();

    let mut log = Log::new(dir.path(), config).unwrap();

    let entry: [&[u8]; 0] = [];
    let err = write_test_entry(&mut log, &entry).unwrap_err();
    assert!(
        err.to_string().contains("journal entry has no fields"),
        "unexpected empty entry error: {err}"
    );

    assert_eq!(count_journal_files(&dir), 0);
}

#[test]
#[allow(deprecated)]
fn test_write_entry_requires_explicit_monotonic_timestamp() {
    let dir = TempDir::new().unwrap();
    let mut log = Log::new(dir.path(), test_config()).unwrap();

    let entry = [b"MESSAGE=missing monotonic" as &[u8]];
    let err = log.write_entry(&entry, None).unwrap_err();
    assert!(
        err.to_string()
            .contains("entry monotonic timestamp is required"),
        "unexpected missing monotonic error: {err}"
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
    write_test_entry(&mut log, &entry).unwrap();
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
    write_test_entry(&mut log, &entry).unwrap();
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
    write_test_entry(&mut log, &first_entry).unwrap();

    let second_entry = [b"MESSAGE=second" as &[u8], b"PRIORITY=6"];
    let ts = next_test_timestamps().with_entry_realtime_usec(0);
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
    write_test_entry(&mut log, &first_entry).unwrap();

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
    let ts = next_test_timestamps()
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
