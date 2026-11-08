use super::*;
use journal_core::file::{EntryField, EntryWriteOptions};

fn boot_uuid(b: u8) -> uuid::Uuid {
    uuid::Uuid::from_bytes([b; 16])
}

fn boot_hex(b: u8) -> String {
    hex::encode([b; 16])
}

/// Writes entries `(boot, realtime, monotonic)` with that ENTRY boot ID and,
/// when `boot_data` is given, the `_BOOT_ID` payload it returns.
fn write_boot_journal(
    path: &Path,
    seqnum_id: u8,
    entries: &[(u8, u64, u64)],
    boot_data: Option<fn(u8) -> String>,
) {
    let repo = RepoFile::from_path(path).expect("journal path");
    let mut file = JournalFile::<MmapMut>::create(
        &repo,
        JournalFileOptions::new(boot_uuid(1), boot_uuid(entries[0].0), boot_uuid(seqnum_id)),
    )
    .expect("create journal");
    let mut writer = JournalWriter::new(&mut file, 1, boot_uuid(entries[0].0)).expect("writer");
    for &(boot, realtime, monotonic) in entries {
        let boot_payload = boot_data.map(|payload| format!("_BOOT_ID={}", payload(boot)));
        let mut items = vec![EntryField::raw(b"MESSAGE=boot test")];
        if let Some(payload) = &boot_payload {
            items.push(EntryField::raw(payload.as_bytes()));
        }
        writer
            .add_entry_fields_with_options(
                &mut file,
                items,
                realtime,
                monotonic,
                EntryWriteOptions::default().boot_id(boot_uuid(boot)),
            )
            .expect("append");
    }
    file.sync().expect("sync");
}

fn boot(index: i64, id: u8, first: i64, last: i64) -> (i64, String, i64, i64) {
    (index, boot_hex(id), first, last)
}

fn rows(boots: Vec<BootInfo>) -> Vec<(i64, String, i64, i64)> {
    boots
        .into_iter()
        .map(|b| (b.index, b.boot_id, b.first_entry, b.last_entry))
        .collect()
}

/// list_boots reports every boot of a multi-boot journal, as journalctl
/// --list-boots does, not one boot per file.
#[test]
fn list_boots_multi_boot() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("system.journal");
    write_boot_journal(
        &first,
        0x10,
        &[
            (0xa0, 100, 1),
            (0xa0, 110, 2),
            (0xb0, 200, 1),
            (0xb0, 210, 2),
            (0xa0, 300, 3),
        ],
        Some(boot_hex),
    );
    let second = temp.path().join("user-1000.journal");
    write_boot_journal(
        &second,
        0x20,
        &[(0xc0, 150, 1), (0xc0, 160, 2), (0xb0, 220, 3)],
        Some(boot_hex),
    );

    let reader = FileReader::open(&first).unwrap();
    assert_eq!(
        rows(reader.list_boots().unwrap()),
        vec![boot(-1, 0xb0, 200, 210), boot(0, 0xa0, 100, 300)]
    );

    let directory = DirectoryReader::open(temp.path()).unwrap();
    let want = vec![
        boot(-2, 0xc0, 150, 160),
        boot(-1, 0xb0, 200, 220),
        boot(0, 0xa0, 100, 300),
    ];
    assert_eq!(rows(directory.list_boots().unwrap()), want);

    let mut facade = SdJournalOpenDirectory(temp.path().to_str().unwrap(), 0).unwrap();
    assert_eq!(rows(SdJournalListBoots(&mut facade).unwrap()), want);
    let mut facade = SdJournalOpenFiles(&[first.to_str().unwrap()], 0).unwrap();
    assert_eq!(
        rows(SdJournalListBoots(&mut facade).unwrap()),
        vec![boot(-1, 0xb0, 200, 210), boot(0, 0xa0, 100, 300)]
    );
}

/// Journals without `_BOOT_ID` DATA fall back to ENTRY header boot IDs.
#[test]
fn list_boots_falls_back_to_entry_headers() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("system.journal");
    write_boot_journal(
        &path,
        0x30,
        &[(0xd0, 500, 1), (0xd0, 510, 2), (0xe0, 600, 1)],
        None,
    );
    let reader = FileReader::open(&path).unwrap();
    assert_eq!(
        rows(reader.list_boots().unwrap()),
        vec![boot(-1, 0xd0, 500, 510), boot(0, 0xe0, 600, 600)]
    );
}

/// Only canonical `_BOOT_ID` values name a boot: journalctl matches the
/// canonical spelling, so an uppercase value is not one of its boots.
#[test]
fn list_boots_ignores_non_canonical_boot_id_data() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("system.journal");
    write_boot_journal(
        &path,
        0x50,
        &[(0xa0, 100, 1), (0xbb, 200, 1)],
        Some(|b| {
            if b == 0xbb {
                boot_hex(b).to_ascii_uppercase()
            } else {
                boot_hex(b)
            }
        }),
    );
    let reader = FileReader::open(&path).unwrap();
    assert_eq!(
        rows(reader.list_boots().unwrap()),
        vec![boot(0, 0xa0, 100, 100)]
    );
}

/// list_boots leaves the caller's matches and read position alone.
#[test]
fn list_boots_keeps_matches_and_position() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("system.journal");
    write_boot_journal(
        &path,
        0x40,
        // The trailing entry does not match, so a lost match would reach it.
        &[
            (0xa0, 100, 1),
            (0xb0, 200, 1),
            (0xb0, 210, 2),
            (0xa0, 300, 2),
        ],
        Some(boot_hex),
    );
    let mut directory = DirectoryReader::open_files([&path]).unwrap();
    directory.add_match(format!("_BOOT_ID={}", boot_hex(0xb0)).as_bytes());
    assert!(directory.next().unwrap());
    assert_eq!(directory.list_boots().unwrap().len(), 2);
    assert!(directory.next().unwrap());
    assert_eq!(directory.get_realtime_usec().unwrap(), 210);
    assert!(!directory.next().unwrap(), "match was lost");
}

/// Dirty systemd archives list the boots the reader sees, from a fresh reader
/// (a prior full read would already have activated tail recovery).
#[test]
fn list_boots_dirty_archives_fresh_reader() {
    let dir = repo_root().join("fixtures/systemd/test-data/no-rtc");
    let want = vec![
        (
            -3,
            "5ea5fc4f82a14186b5332a788ef9435e".to_string(),
            1666569600994371,
            1666584266223608,
        ),
        (
            -2,
            "bea6864f21ad4c9594c04a99d89948b0".to_string(),
            1666569601005945,
            1666584347230411,
        ),
        (
            -1,
            "4c708e1fd0744336be16f3931aa861fb".to_string(),
            1666569601017222,
            1666584354649355,
        ),
        (
            0,
            "35e8501129134edd9df5267c49f744a4".to_string(),
            1666569601009823,
            1666584438086856,
        ),
    ];
    let directory = DirectoryReader::open(&dir).unwrap();
    assert_eq!(rows(directory.list_boots().unwrap()), want);
    let mut facade = SdJournalOpenDirectory(dir.to_str().unwrap(), 0).unwrap();
    assert_eq!(rows(SdJournalListBoots(&mut facade).unwrap()), want);
}
