use super::*;

#[derive(Clone)]
struct TestField {
    raw: Vec<u8>,
    name: Vec<u8>,
    value: Vec<u8>,
}

impl TestField {
    fn new(name: Vec<u8>, value: Vec<u8>) -> Self {
        let mut raw = Vec::with_capacity(name.len() + 1 + value.len());
        raw.extend_from_slice(&name);
        raw.push(b'=');
        raw.extend_from_slice(&value);
        Self { raw, name, value }
    }

    fn from_str(name: &str, value: impl AsRef<[u8]>) -> Self {
        Self::new(name.as_bytes().to_vec(), value.as_ref().to_vec())
    }

    fn structured(&self) -> StructuredField<'_> {
        StructuredField::new(&self.name, &self.value)
    }
}

fn make_raw_structured_identity_rows(rows: usize) -> Vec<Vec<TestField>> {
    let mut all = Vec::with_capacity(rows);
    for row in 0..rows {
        let mut fields = Vec::with_capacity(18);
        fields.push(TestField::from_str("TEST_ID", "raw-structured-identity"));
        fields.push(TestField::from_str("PERF_PROFILE", "mixed-cardinality"));
        fields.push(TestField::from_str("EMPTY_VALUE", b""));
        fields.push(TestField::from_str(
            "BINARY_VALUE",
            [0, b'=', (row & 0xff) as u8, 0xff],
        ));

        for offset in 0..6 {
            fields.push(TestField::from_str(
                &format!("LOW_CARD_{offset:02}"),
                format!("low-{offset:02}-{:02}", row % 16),
            ));
        }
        for offset in 0..4 {
            fields.push(TestField::from_str(
                &format!("MED_CARD_{offset:02}"),
                format!("medium-{offset:02}-{:04}", row % 257),
            ));
        }
        for offset in 0..4 {
            fields.push(TestField::from_str(
                &format!("HIGH_CARD_{offset:02}"),
                format!("high-{offset:02}-{row:06}"),
            ));
        }
        all.push(fields);
    }
    all
}

fn write_rows_raw_test_journal(
    path: &Path,
    rows: &[Vec<TestField>],
    options: EntryWriteOptions,
) -> Vec<u8> {
    let repo_file =
        crate::repository::File::from_path(path).expect("test journal path should parse");
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_file_id(test_uuid(5)),
    )
    .expect("create raw corpus journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    let mut entry_fields = Vec::with_capacity(32);
    for (index, row) in rows.iter().enumerate() {
        entry_fields.clear();
        entry_fields.extend(
            row.iter()
                .map(|field| EntryField::raw(field.raw.as_slice())),
        );
        writer
            .add_entry_fields_with_options(
                &mut journal_file,
                entry_fields.iter().copied(),
                1_700_000_060_000_000 + index as u64 * 500,
                100 + index as u64 * 50,
                options,
            )
            .expect("write raw corpus entry");
    }
    journal_file.sync().expect("sync raw corpus journal");
    drop(journal_file);
    std::fs::read(path).expect("read raw corpus journal")
}

fn write_rows_structured_test_journal(
    path: &Path,
    rows: &[Vec<TestField>],
    options: EntryWriteOptions,
) -> Vec<u8> {
    let repo_file =
        crate::repository::File::from_path(path).expect("test journal path should parse");
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_file_id(test_uuid(5)),
    )
    .expect("create structured corpus journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    let mut structured_fields = Vec::with_capacity(32);
    for (index, row) in rows.iter().enumerate() {
        structured_fields.clear();
        structured_fields.extend(row.iter().map(TestField::structured));
        writer
            .add_entry_structured_with_options(
                &mut journal_file,
                &structured_fields,
                1_700_000_060_000_000 + index as u64 * 500,
                100 + index as u64 * 50,
                options,
            )
            .expect("write structured corpus entry");
    }
    journal_file.sync().expect("sync structured corpus journal");
    drop(journal_file);
    std::fs::read(path).expect("read structured corpus journal")
}

fn write_rows_structured_with_live_mode(
    path: &Path,
    rows: &[Vec<TestField>],
    live_publish_every_entries: u64,
) -> Vec<u8> {
    let repo_file =
        crate::repository::File::from_path(path).expect("test journal path should parse");
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_file_id(test_uuid(5)),
    )
    .expect("create live publication mode journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    writer.set_live_publish_every_entries(live_publish_every_entries);

    let mut structured_fields = Vec::with_capacity(32);
    for (index, row) in rows.iter().enumerate() {
        structured_fields.clear();
        structured_fields.extend(row.iter().map(TestField::structured));
        writer
            .add_entry_structured_with_options(
                &mut journal_file,
                &structured_fields,
                1_700_000_060_000_000 + index as u64 * 500,
                100 + index as u64 * 50,
                EntryWriteOptions::default().trusted_unique_payloads(true),
            )
            .expect("write live publication mode entry");
    }
    journal_file
        .sync()
        .expect("sync live publication mode journal");
    drop(journal_file);
    std::fs::read(path).expect("read live publication mode journal")
}

#[test]
fn structured_writer_matches_raw_payload_writer_bytes() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let raw_path = journal_dir.join("raw.journal");
    let structured_path = journal_dir.join("structured.journal");

    let raw_fields = [
        b"MESSAGE=structured parity".as_slice(),
        b"PRIORITY=6".as_slice(),
        b"BINARY=\x00=\x01\xfe\xff".as_slice(),
    ];
    let structured_fields = [
        StructuredField::new(b"MESSAGE", b"structured parity"),
        StructuredField::new(b"PRIORITY", b"6"),
        StructuredField::new(b"BINARY", b"\x00=\x01\xfe\xff"),
    ];

    let raw_bytes = write_raw_test_journal(&raw_path, &raw_fields);
    let structured_bytes = write_structured_test_journal(
        &structured_path,
        &structured_fields,
        EntryWriteOptions::default(),
    );

    assert_eq!(structured_bytes, raw_bytes);

    if !journalctl_available() {
        eprintln!("journalctl not available; skipping structured stock verify");
        return;
    }
    let output = Command::new("journalctl")
        .arg("--verify")
        .arg("--file")
        .arg(&structured_path)
        .output()
        .expect("run journalctl verify");
    assert!(
        output.status.success(),
        "journalctl verify failed for structured file: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn mixed_entry_fields_match_raw_payload_writer_bytes() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let raw_path = journal_dir.join("raw.journal");
    let mixed_path = journal_dir.join("mixed.journal");

    let raw_fields = [
        EntryField::raw(b"MESSAGE=mixed entry"),
        EntryField::raw(b"PRIORITY=6"),
        EntryField::raw(b"BINARY=\x00=\x01\xfe\xff"),
    ];
    let mixed_fields = [
        EntryField::raw(b"MESSAGE=mixed entry"),
        EntryField::structured(b"PRIORITY", b"6"),
        EntryField::structured(b"BINARY", b"\x00=\x01\xfe\xff"),
    ];

    let (raw_bytes, _, _) =
        write_entry_fields_test_journal(&raw_path, &raw_fields, EntryWriteOptions::default());
    let (mixed_bytes, _, _) =
        write_entry_fields_test_journal(&mixed_path, &mixed_fields, EntryWriteOptions::default());

    assert_eq!(mixed_bytes, raw_bytes);
}

#[test]
fn structured_writer_preserves_binary_field_values() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
    )
    .expect("create journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    writer
        .add_entry_structured(
            &mut journal_file,
            &[
                StructuredField::new(b"MESSAGE", b"binary structured"),
                StructuredField::new(b"BINARY", b"\x00=\x01\xfe\xff"),
            ],
            1_700_000_060_000_000,
            100,
        )
        .expect("write structured binary entry");
    journal_file.sync().expect("sync journal");

    let mut entry_offsets = Vec::new();
    journal_file
        .entry_offsets(&mut entry_offsets)
        .expect("collect entry offsets");
    let payloads = journal_file
        .entry_data_objects(entry_offsets[0])
        .expect("entry data iterator")
        .map(|item| item.map(|object| object.raw_payload().to_vec()))
        .collect::<crate::error::Result<Vec<_>>>()
        .expect("read payloads");

    assert!(payloads.iter().any(|p| p == b"MESSAGE=binary structured"));
    assert!(payloads.iter().any(|p| p == b"BINARY=\x00=\x01\xfe\xff"));
}

#[test]
fn structured_writer_rejects_invalid_field_names_before_writing() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
    )
    .expect("create journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

    assert!(
        writer
            .add_entry_structured(
                &mut journal_file,
                &[StructuredField::new(b"not-valid", b"value")],
                1_700_000_060_000_000,
                100,
            )
            .is_err()
    );
    assert_eq!(journal_file.journal_header_ref().n_entries, 0);
}

#[test]
fn writer_field_name_policies_cover_journald_app_and_raw() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    assert_journald_policy(&journal_dir);
    assert_journal_app_policy(&journal_dir);
    assert_raw_policy(&journal_dir);
}

fn assert_journald_policy(journal_dir: &Path) {
    let journald_path = journal_dir.join("journald.journal");
    let (mut journal_file, mut writer) = create_policy_writer(&journald_path, [1, 2, 3], 4);
    writer
        .add_entry_structured(
            &mut journal_file,
            &[
                StructuredField::new(b"MESSAGE", b"trusted fields"),
                StructuredField::new(b"_HOSTNAME", b"synthetic-host"),
            ],
            1_700_002_111_000_000,
            1,
        )
        .expect("journald policy accepts protected fields");
    assert_eq!(journal_file.journal_header_ref().n_entries, 1);
}

fn assert_journal_app_policy(journal_dir: &Path) {
    let app_path = journal_dir.join("journal-app.journal");
    let (mut journal_file, mut writer) = create_policy_writer(&app_path, [5, 6, 7], 8);
    writer
        .add_entry_structured_with_options(
            &mut journal_file,
            &[
                StructuredField::new(b"MESSAGE", b"app valid"),
                StructuredField::new(b"_HOSTNAME", b"drop-host"),
                StructuredField::new(b"lowercase", b"drop-lowercase"),
            ],
            1_700_002_112_000_000,
            1,
            EntryWriteOptions::default().field_name_policy(FieldNamePolicy::JournalApp),
        )
        .expect("journal-app policy drops invalid fields");
    assert_eq!(journal_file.journal_header_ref().n_entries, 1);
    let payloads = first_entry_payloads(&journal_file, "journal-app");
    assert!(payloads.iter().any(|p| p == b"MESSAGE=app valid"));
    assert!(!payloads.iter().any(|p| p.starts_with(b"_HOSTNAME=")));
    assert!(!payloads.iter().any(|p| p.starts_with(b"lowercase=")));
    assert!(
        writer
            .add_entry_structured_with_options(
                &mut journal_file,
                &[StructuredField::new(b"_HOSTNAME", b"drop-only")],
                1_700_002_112_000_001,
                2,
                EntryWriteOptions::default().field_name_policy(FieldNamePolicy::JournalApp),
            )
            .is_err()
    );
}

fn assert_raw_policy(journal_dir: &Path) {
    let raw_path = journal_dir.join("raw.journal");
    let (mut journal_file, mut writer) = create_policy_writer(&raw_path, [9, 10, 11], 12);
    let long_name = vec![b'a'; 1024];
    writer
        .add_entry_fields_with_options(
            &mut journal_file,
            [
                EntryField::structured(b"lowercase", b"ok"),
                EntryField::structured(b"foo.bar", b"dot"),
                EntryField::structured(b"field name", b"space"),
                EntryField::structured(long_name.as_slice(), b"long"),
                EntryField::structured(b"BINARY", b"a\0=b"),
            ],
            1_700_002_113_000_000,
            1,
            EntryWriteOptions::default().field_name_policy(FieldNamePolicy::Raw),
        )
        .expect("raw policy accepts structure-only names");
    assert_raw_policy_payloads(&journal_file);
    assert!(
        writer
            .add_entry_fields_with_options(
                &mut journal_file,
                [EntryField::structured(b"BAD=NAME", b"bad")],
                1_700_002_113_000_001,
                2,
                EntryWriteOptions::default().field_name_policy(FieldNamePolicy::Raw),
            )
            .is_err()
    );
}

fn create_policy_writer(
    path: &Path,
    file_seeds: [u8; 3],
    boot_seed: u8,
) -> (JournalFile<MmapMut>, JournalWriter) {
    let repo_file = crate::repository::File::from_path(path).expect("test journal path");
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(
            test_uuid(file_seeds[0]),
            test_uuid(file_seeds[1]),
            test_uuid(file_seeds[2]),
        ),
    )
    .expect("create journal");
    let writer =
        JournalWriter::new(&mut journal_file, 1, test_uuid(boot_seed)).expect("create writer");
    (journal_file, writer)
}

fn first_entry_payloads(journal_file: &JournalFile<MmapMut>, label: &str) -> Vec<Vec<u8>> {
    let mut entry_offsets = Vec::new();
    journal_file
        .entry_offsets(&mut entry_offsets)
        .unwrap_or_else(|_| panic!("collect {label} entry offsets"));
    journal_file
        .entry_data_objects(entry_offsets[0])
        .unwrap_or_else(|_| panic!("{label} entry data iterator"))
        .map(|item| item.map(|object| object.raw_payload().to_vec()))
        .collect::<crate::error::Result<Vec<_>>>()
        .unwrap_or_else(|_| panic!("read {label} payloads"))
}

fn assert_raw_policy_payloads(journal_file: &JournalFile<MmapMut>) {
    let payloads = first_entry_payloads(journal_file, "raw");
    assert!(payloads.iter().any(|p| p == b"lowercase=ok"));
    assert!(payloads.iter().any(|p| p == b"foo.bar=dot"));
    assert!(payloads.iter().any(|p| p == b"field name=space"));
    assert!(
        payloads
            .iter()
            .any(|p| p == &format!("{}=long", "a".repeat(1024)).into_bytes())
    );
    assert!(payloads.iter().any(|p| p == b"BINARY=a\0=b"));
}

#[test]
fn trusted_unique_payloads_keeps_unique_entry_output_identical() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let default_path = journal_dir.join("default.journal");
    let trusted_path = journal_dir.join("trusted.journal");

    let fields = [
        StructuredField::new(b"MESSAGE", b"trusted unique"),
        StructuredField::new(b"PRIORITY", b"6"),
        StructuredField::new(b"SYSLOG_IDENTIFIER", b"journal-core-test"),
    ];
    let default_bytes =
        write_structured_test_journal(&default_path, &fields, EntryWriteOptions::default());
    let trusted_bytes = write_structured_test_journal(
        &trusted_path,
        &fields,
        EntryWriteOptions::default().trusted_unique_payloads(true),
    );

    assert_eq!(trusted_bytes, default_bytes);
}

#[test]
fn structured_writer_deduplicates_duplicate_payloads_by_default() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let default_path = journal_dir.join("default.journal");
    let trusted_path = journal_dir.join("trusted.journal");

    let fields = [
        EntryField::structured(b"MESSAGE", b"duplicate"),
        EntryField::structured(b"MESSAGE", b"duplicate"),
        EntryField::structured(b"PRIORITY", b"6"),
    ];

    let (_, default_count, default_payloads) =
        write_entry_fields_test_journal(&default_path, &fields, EntryWriteOptions::default());
    assert_eq!(default_count, 2);
    assert_eq!(
        default_payloads
            .iter()
            .filter(|payload| payload.as_slice() == b"MESSAGE=duplicate")
            .count(),
        1
    );

    let (_, trusted_count, trusted_payloads) = write_entry_fields_test_journal(
        &trusted_path,
        &fields,
        EntryWriteOptions::default().trusted_unique_payloads(true),
    );
    assert_eq!(trusted_count, 3);
    assert_eq!(
        trusted_payloads
            .iter()
            .filter(|payload| payload.as_slice() == b"MESSAGE=duplicate")
            .count(),
        2
    );
}

#[test]
fn structured_writer_matches_raw_payload_writer_bytes_across_deterministic_corpus() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let raw_path = journal_dir.join("raw.journal");
    let structured_path = journal_dir.join("structured.journal");
    let rows = make_raw_structured_identity_rows(512);
    let options = EntryWriteOptions::default().trusted_unique_payloads(true);

    let raw_bytes = write_rows_raw_test_journal(&raw_path, &rows, options);
    let structured_bytes = write_rows_structured_test_journal(&structured_path, &rows, options);

    assert_eq!(structured_bytes, raw_bytes);
}

#[test]
fn live_publication_modes_preserve_closed_file_bytes() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let immediate_path = journal_dir.join("immediate.journal");
    let disabled_path = journal_dir.join("disabled.journal");
    let every_n_path = journal_dir.join("every-n.journal");
    let rows = make_raw_structured_identity_rows(65);

    let immediate_bytes = write_rows_structured_with_live_mode(&immediate_path, &rows, 1);
    let disabled_bytes = write_rows_structured_with_live_mode(&disabled_path, &rows, 0);
    let every_n_bytes = write_rows_structured_with_live_mode(&every_n_path, &rows, 8);

    assert_eq!(disabled_bytes, immediate_bytes);
    assert_eq!(every_n_bytes, immediate_bytes);
}
