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

        let mut collected = Vec::new();
        reader.collect_entry_payloads(&mut collected).unwrap();
        assert_eq!(collected.len(), 1 + usize::from(payload_kept), "{name}");
        reader.entry_data_restart().unwrap();
        let mut enumerated = 0;
        while reader.enumerate_entry_payload().unwrap().is_some() {
            enumerated += 1;
        }
        assert_eq!(enumerated, 1 + usize::from(payload_kept), "{name}");
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

// Paired with Go TestMissingMainEntryArrayHidesEveryEntry.
#[test]
fn missing_main_entry_array_hides_every_entry() {
    for filtered in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = journal_path(dir.path());
        write_entry(&path, &[b"MESSAGE=hidden"]);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[176..184].fill(0);
        std::fs::write(&path, bytes).unwrap();

        let mut reader = DirectoryReader::open_files([&path]).unwrap();
        if filtered {
            reader.add_match(b"MESSAGE=hidden");
        }
        assert!(!reader.next().unwrap(), "filtered={filtered}");
    }
}

// Paired with Go TestDamagedCompressedDataCostsTheFieldNotTheEntry.
#[test]
fn damaged_compressed_data_costs_the_field() {
    use journal_core::file::Compression;

    for compression in [Compression::Zstd, Compression::Xz, Compression::Lz4] {
        let dir = tempfile::tempdir().unwrap();
        let path = journal_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let repo_file = RepoFile::from_path(&path).expect("journal path");
        let mut file = JournalFile::<MmapMut>::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_compression(compression)
                .with_compress_threshold(16),
        )
        .expect("create journal");
        let mut writer = JournalWriter::new(&mut file, 1, test_uuid(4)).expect("writer");
        let broken = [b"BROKEN=".as_slice(), &[b'z'; 512]].concat();
        writer
            .add_entry(
                &mut file,
                &[b"MESSAGE=kept", &broken],
                1_700_000_000_000_000,
                1,
            )
            .expect("write entry");
        file.sync().expect("sync");
        drop(file);
        corrupt_compressed_data(&path, matches!(compression, Compression::Lz4));

        let mut reader = DirectoryReader::open_files([&path]).unwrap();
        assert!(reader.next().unwrap(), "{compression:?}");
        let entry = reader
            .get_entry()
            .unwrap_or_else(|e| panic!("{compression:?}: {e:?}"));
        assert_eq!(entry.fields["MESSAGE"], b"kept", "{compression:?}");
        assert_eq!(entry.payloads.len(), 1, "{compression:?}");
        let mut collected = Vec::new();
        reader.collect_entry_payloads(&mut collected).unwrap();
        assert_eq!(collected.len(), 1, "{compression:?}");
        assert!(
            crate::verify_file(&path).is_err(),
            "{compression:?}: verification accepted damaged DATA"
        );
    }
}

// Paired with Go TestExplorerSkipsDamagedCompressedData.
#[test]
fn explorer_skips_damaged_compressed_data() {
    use journal_core::file::Compression;

    let dir = tempfile::tempdir().unwrap();
    let path = journal_path(dir.path());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let repo_file = RepoFile::from_path(&path).expect("journal path");
    let mut file = JournalFile::<MmapMut>::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_compression(Compression::Zstd)
            .with_compress_threshold(32),
    )
    .expect("create journal");
    let mut writer = JournalWriter::new(&mut file, 1, test_uuid(4)).expect("writer");
    let large = [b"MESSAGE=".as_slice(), &[b'z'; 512]].concat();
    writer
        .add_entry(&mut file, &[b"PRIORITY=3", &large], 1_000, 1)
        .expect("write entry");
    file.sync().expect("sync");
    drop(file);
    corrupt_compressed_data(&path, false);

    let mut reader = FileReader::open(&path).expect("open reader");
    let query = ExplorerQuery {
        facets: vec![b"PRIORITY".to_vec(), b"MESSAGE".to_vec()],
        use_source_realtime: false,
        limit: 10,
        ..ExplorerQuery::default()
    };
    let result = reader.explore(&query).expect("explore");
    let priority = result
        .facets
        .get(b"PRIORITY".as_slice())
        .expect("priority facet");
    assert_eq!(priority.get(b"3".as_slice()), Some(&1));
    assert_eq!(result.rows.len(), 1);
}

/// Breaks the only compressed DATA object: lz4 by its stored size, the others
/// by their trailing checksum or footer.
fn corrupt_compressed_data(path: &Path, lz4: bool) {
    let mut bytes = std::fs::read(path).unwrap();
    let u64_at = |b: &[u8], at: usize| u64::from_le_bytes(b[at..at + 8].try_into().unwrap());
    let mut offset = u64_at(&bytes, 88) as usize;
    let tail = u64_at(&bytes, 136) as usize;
    while offset != 0 && offset <= tail {
        let size = u64_at(&bytes, offset + 8) as usize;
        if bytes[offset] == 1 && bytes[offset + 1] & 7 != 0 {
            let (start, end) = (offset + OBJECT_HEADER_TO_PAYLOAD, offset + size);
            if lz4 {
                let stored = u64_at(&bytes, start) + 1;
                bytes[start..start + 8].copy_from_slice(&stored.to_le_bytes());
            } else {
                bytes[end - 8..end].fill(0);
            }
            std::fs::write(path, bytes).unwrap();
            return;
        }
        offset += (size + 7) & !7;
    }
    panic!("no compressed DATA object");
}
