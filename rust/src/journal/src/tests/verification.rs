use super::*;

fn verification_key(opts: &SealOptions) -> String {
    let seed_hex = opts
        .seed
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    let start = opts.start_usec / opts.interval_usec;
    format!(
        "{seed_hex}/{start:x}-{interval:x}",
        interval = opts.interval_usec
    )
}

fn write_sealed_verify_file(path: &Path) -> SealOptions {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create journal parent");
    }
    let repo_file = RepoFile::from_path(path)
        .unwrap_or_else(|| panic!("test journal path should parse: {}", path.display()));
    let seal = test_seal_opts();
    let mut journal_file = JournalFile::<MmapMut>::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_seal(seal.clone()),
    )
    .expect("create sealed journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=sealed-covered".as_slice()],
            1_500_000,
            100,
        )
        .expect("write covered entry");
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=later-entry".as_slice()],
            2_500_000,
            200,
        )
        .expect("write later entry");
    journal_file.sync().expect("sync journal");
    seal
}

fn tamper_data_payload(path: &Path, payload: &[u8]) {
    let mut data = std::fs::read(path).expect("read journal bytes");
    let header_size = u64::from_le_bytes(data[88..96].try_into().unwrap());
    let tail_object_offset = u64::from_le_bytes(data[136..144].try_into().unwrap());
    let incompatible_flags = u32::from_le_bytes(data[12..16].try_into().unwrap());
    let payload_offset = if incompatible_flags & INCOMPATIBLE_COMPACT != 0 {
        COMPACT_DATA_OBJECT_HEADER_SIZE
    } else {
        DATA_OBJECT_HEADER_SIZE
    };

    let mut offset = header_size;
    let mut tag_count = 0usize;
    let mut second_tag_offset = 0u64;
    let mut target_payload_offset = 0u64;
    let mut target_object_offset = 0u64;

    loop {
        assert!(
            offset + OBJECT_HEADER_SIZE <= data.len() as u64,
            "object header at {offset} exceeds file"
        );
        let typ = data[offset as usize];
        let size = u64::from_le_bytes(
            data[(offset as usize + 8)..(offset as usize + 16)]
                .try_into()
                .unwrap(),
        );
        assert!(
            size >= OBJECT_HEADER_SIZE,
            "invalid object size {size} at {offset}"
        );
        let aligned_size = align8(size);
        assert!(
            offset + aligned_size <= data.len() as u64,
            "object at {offset} exceeds file"
        );

        if typ == OBJECT_TYPE_TAG {
            tag_count += 1;
            if tag_count == 2 {
                second_tag_offset = offset;
            }
        } else if typ == OBJECT_TYPE_DATA && size > payload_offset {
            let start = (offset + payload_offset) as usize;
            let end = (offset + size) as usize;
            if &data[start..end] == payload {
                target_payload_offset = start as u64;
                target_object_offset = offset;
            }
        }

        if offset == tail_object_offset {
            break;
        }
        offset += aligned_size;
    }

    assert_ne!(target_payload_offset, 0, "payload not found");
    assert_ne!(second_tag_offset, 0, "second TAG not found");
    assert!(
        target_object_offset < second_tag_offset,
        "DATA object {target_object_offset} is not covered by second TAG {second_tag_offset}"
    );
    data[target_payload_offset as usize] ^= 0x01;
    std::fs::write(path, data).expect("write tampered journal bytes");
}

#[test]
fn verify_file_detects_corruption() {
    let path = repo_root().join("fixtures/systemd/test-data/corrupted/zstd-truncated-frame.zst");
    let err = verify_file(&path).expect_err("expected verification error for truncated zstd frame");
    let msg = err.to_string();
    assert!(
        msg.to_lowercase().contains("corrupt"),
        "expected error to contain 'corrupt', got: {msg}"
    );
}

#[test]
fn verify_file_passes_on_valid_fixture() {
    let path = repo_root().join("fixtures/systemd/test-data/no-rtc/system.journal.zst");
    verify_file(&path).expect("expected verification to pass for valid fixture");
}

#[test]
fn verify_file_with_key_validates_sealed_hmacs() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir
        .path()
        .join("00000000-0000-0000-0000-000000000001/system.journal");
    let seal = write_sealed_verify_file(&path);
    let key = verification_key(&seal);

    verify_file_with_key(&path, &key).expect("sealed verification should pass");
    let zst_path = dir.path().join("sealed.journal.zst");
    let source = std::fs::read(&path).expect("read sealed journal");
    let compressed = ruzstd::encoding::compress_to_vec(
        source.as_slice(),
        ruzstd::encoding::CompressionLevel::Fastest,
    );
    std::fs::write(&zst_path, compressed).expect("write compressed sealed journal");
    verify_file_with_key(&zst_path, &key).expect("compressed sealed verification should pass");

    verify_file_with_key(&path, "000000000000000000000001/1-f4240")
        .expect_err("wrong key should fail");

    tamper_data_payload(&path, b"MESSAGE=sealed-covered");
    verify_file_with_key(&path, &key).expect_err("authenticated DATA tamper should fail");
}

#[test]
fn verify_file_with_key_rejects_aligned_size_overflow() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir
        .path()
        .join("00000000-0000-0000-0000-000000000001/system.journal");
    let seal = write_sealed_verify_file(&path);
    let key = verification_key(&seal);

    let mut data = std::fs::read(&path).expect("read sealed journal");
    let header_size = u64::from_le_bytes(data[88..96].try_into().unwrap()) as usize;
    data[header_size + 8..header_size + 16].copy_from_slice(&u64::MAX.to_le_bytes());
    std::fs::write(&path, data).expect("write malformed journal");

    let err = verify_file_with_key(&path, &key)
        .expect_err("aligned-size overflow should fail verification");
    let msg = err.to_string();
    assert!(
        msg.contains("overflows alignment"),
        "expected alignment overflow error, got: {msg}"
    );
}

#[test]
fn verify_file_with_key_rejects_short_sealed_header_without_panic() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir
        .path()
        .join("00000000-0000-0000-0000-000000000001/system.journal");
    std::fs::create_dir_all(path.parent().expect("journal parent")).expect("create parent");

    let mut data = vec![0u8; HEADER_MIN_SIZE as usize];
    data[0..8].copy_from_slice(b"LPKSHHRH");
    data[8..12].copy_from_slice(&1u32.to_le_bytes());
    data[16] = 1;
    data[88..96].copy_from_slice(&HEADER_MIN_SIZE.to_le_bytes());
    std::fs::write(&path, data).expect("write short sealed header");

    let err = verify_file_with_key(&path, "000000000000000000000000/1-f4240")
        .expect_err("short sealed header should not verify");
    let msg = err.to_string();
    assert!(
        msg.contains("open/decompression failed")
            || msg.contains("corrupt")
            || msg.contains("verification"),
        "expected controlled verification error, got: {msg}"
    );
}

#[test]
fn verify_file_rejects_referenced_zero_sized_data_object() {
    let fixture = repo_root().join("fixtures/systemd/test-data/no-rtc/system.journal.zst");
    let path = decompress_zst_to_temp(&fixture, "rust-sdk-verify-corrupt")
        .expect("decompress fixture to temporary journal");
    let _cleanup = TempPath(path.clone());

    let data_offset = {
        let reader = FileReader::open(&path).expect("open decompressed journal");
        reader.inner.with_file(|file| {
            let mut entry_offsets = Vec::new();
            file.entry_offsets(&mut entry_offsets)
                .expect("collect entry offsets");
            let mut data_offsets = Vec::new();
            file.entry_data_object_offsets(entry_offsets[0], &mut data_offsets)
                .expect("collect data offsets");
            data_offsets[0]
        })
    };

    let mut bytes = std::fs::read(&path).expect("read journal bytes");
    let size_start = data_offset.get() as usize + 8;
    bytes[size_start..size_start + 8].copy_from_slice(&0u64.to_le_bytes());
    std::fs::write(&path, bytes).expect("write corrupted journal bytes");

    let err = verify_file(&path)
        .expect_err("strict verification should reject referenced zero-sized data object");
    let msg = err.to_string();
    assert!(
        msg.to_lowercase().contains("corrupt"),
        "expected error to contain 'corrupt', got: {msg}"
    );
    assert!(
        msg.contains("data object") || msg.contains("object size"),
        "expected strict data-object or object-size error, got: {msg}"
    );
}
