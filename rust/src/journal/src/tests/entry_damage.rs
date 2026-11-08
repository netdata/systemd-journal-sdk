//! Paired with Go `reader_entry_damage_test.go`.

use super::*;

const OBJECT_HEADER_TO_PAYLOAD: usize = 64;

fn write_entry(path: &Path, payloads: &[&[u8]]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let repo_file = RepoFile::from_path(path).expect("journal path");
    let mut file = JournalFile::<MmapMut>::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
    )
    .expect("create journal");
    let mut writer = JournalWriter::new(&mut file, 1, test_uuid(4)).expect("writer");
    writer
        .add_entry(&mut file, payloads, 1_700_000_000_000_000, 1)
        .expect("write entry");
    file.sync().expect("sync");
}

fn damage(path: &Path, patch: fn(&mut [u8], usize)) {
    let mut bytes = std::fs::read(path).unwrap();
    let needle = b"BROKEN=1";
    let at = bytes
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("payload in file");
    patch(&mut bytes, at);
    std::fs::write(path, bytes).unwrap();
}

fn journal_path(dir: &Path) -> PathBuf {
    dir.join(format!("{}/system.journal", test_uuid(1).simple()))
}

#[test]
fn damaged_data_object_costs_the_field_not_the_entry() {
    type Patch = fn(&mut [u8], usize);
    let cases: [(&str, Patch, bool); 3] = [
        ("no-separator", |b, at| b[at + 6] = b'_', true),
        (
            "wrong-type",
            |b, at| b[at - OBJECT_HEADER_TO_PAYLOAD] = 3,
            false,
        ),
        (
            "oversized",
            |b, at| {
                let size = at - OBJECT_HEADER_TO_PAYLOAD + 8;
                b[size..size + 8].copy_from_slice(&(1u64 << 40).to_le_bytes());
            },
            false,
        ),
    ];
    for (name, patch, payload_kept) in cases {
        let dir = tempfile::tempdir().unwrap();
        let path = journal_path(dir.path());
        write_entry(&path, &[b"MESSAGE=kept", b"BROKEN=1"]);
        damage(&path, patch);

        let mut reader = DirectoryReader::open_files([&path]).unwrap();
        assert!(reader.next().unwrap(), "{name}");
        let entry = reader
            .get_entry()
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(entry.fields["MESSAGE"], b"kept", "{name}");
        assert!(!entry.fields.contains_key("BROKEN"), "{name}");
        assert_eq!(
            entry.payloads.len(),
            1 + usize::from(payload_kept),
            "{name}"
        );
    }
}

#[test]
fn repeated_field_keeps_the_first_value() {
    let dir = tempfile::tempdir().unwrap();
    let path = journal_path(dir.path());
    write_entry(&path, &[b"KEY=a", b"KEY=b"]);
    let mut reader = DirectoryReader::open_files([&path]).unwrap();
    assert!(reader.next().unwrap());
    let entry = reader.get_entry().unwrap();
    let first = entry.field_values["KEY"][0].clone();
    assert_eq!(entry.fields["KEY"], first);
    assert_eq!(entry.field_values["KEY"].len(), 2);
}

#[test]
fn open_files_opens_a_repeated_path_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = journal_path(dir.path());
    write_entry(&path, &[b"MESSAGE=once"]);
    let mut reader = DirectoryReader::open_files([&path, &path]).unwrap();
    assert!(reader.next().unwrap());
    assert!(!reader.next().unwrap(), "the same file was read twice");
}
