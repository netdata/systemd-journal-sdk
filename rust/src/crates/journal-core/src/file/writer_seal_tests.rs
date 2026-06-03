use super::*;

#[test]
fn sealed_writer_basic_passes_stock_verify() {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping sealed writer stock verify");
        return;
    }
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let seal = test_seal_opts();
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_seal(seal.clone()),
    )
    .expect("create sealed journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    writer
        .add_entry(
            &mut journal_file,
            &[
                b"MESSAGE=hello sealed world".as_slice(),
                b"PRIORITY=6".as_slice(),
            ],
            1_500_000,
            100,
        )
        .expect("write sealed entry");
    journal_file.sync().expect("sync journal");

    let key = verification_key(&seal);
    let output = Command::new("journalctl")
        .arg("--verify")
        .arg("--verify-key")
        .arg(&key)
        .arg("--file")
        .arg(&path)
        .output()
        .expect("run journalctl verify");
    assert!(
        output.status.success(),
        "journalctl verify failed for sealed file: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn sealed_writer_interval_crossing_passes_stock_verify() {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping interval crossing stock verify");
        return;
    }
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let seal = test_seal_opts();
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_seal(seal.clone()),
    )
    .expect("create sealed journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

    // Entry in epoch 0 (realtime == start)
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=epoch0".as_slice()],
            1_000_000,
            100,
        )
        .expect("write epoch 0");
    // Entry in epoch 1 (crosses interval)
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=epoch1".as_slice()],
            2_000_000,
            200,
        )
        .expect("write epoch 1");
    // Entry in epoch 2
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=epoch2".as_slice()],
            3_000_000,
            300,
        )
        .expect("write epoch 2");
    journal_file.sync().expect("sync journal");

    let key = verification_key(&seal);
    let output = Command::new("journalctl")
        .arg("--verify")
        .arg("--verify-key")
        .arg(&key)
        .arg("--file")
        .arg(&path)
        .output()
        .expect("run journalctl verify");
    assert!(
        output.status.success(),
        "journalctl verify failed for interval-crossing sealed file: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn sealed_writer_wrong_key_fails_stock_verify() {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping wrong key verify");
        return;
    }
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let seal = test_seal_opts();
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_seal(seal),
    )
    .expect("create sealed journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=hello".as_slice()],
            1_500_000,
            100,
        )
        .expect("write sealed entry");
    journal_file.sync().expect("sync journal");

    let wrong_key = "000000000000000000000001/1-f4240";
    let output = Command::new("journalctl")
        .arg("--verify")
        .arg("--verify-key")
        .arg(wrong_key)
        .arg("--file")
        .arg(&path)
        .output()
        .expect("run journalctl verify with wrong key");
    assert!(
        !output.status.success(),
        "journalctl verify should fail with wrong key, but succeeded"
    );
}

#[test]
fn sealed_writer_tampered_data_fails_stock_verify() {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping tamper verify");
        return;
    }
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let seal = test_seal_opts();
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_seal(seal.clone()),
    )
    .expect("create sealed journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=hello".as_slice()],
            1_500_000,
            100,
        )
        .expect("write sealed entry");
    journal_file.sync().expect("sync journal");

    // Tamper with a byte in the DATA object payload area
    use std::fs::OpenOptions;
    let mut f = OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open for tamper");
    write_test_bytes_at(&mut f, &[0xff], 512).expect("tamper write");
    drop(f);

    let key = verification_key(&seal);
    let output = Command::new("journalctl")
        .arg("--verify")
        .arg("--verify-key")
        .arg(&key)
        .arg("--file")
        .arg(&path)
        .output()
        .expect("run journalctl verify tampered");
    assert!(
        !output.status.success(),
        "journalctl verify should fail with tampered data, but succeeded"
    );
}

#[test]
fn unsealed_writer_does_not_set_sealed_flags() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let journal_file: JournalFile<MmapMut> = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
    )
    .expect("create unsealed journal");
    let header = journal_file.journal_header_ref();
    assert!(
        !header.has_compatible_flag(HeaderCompatibleFlags::Sealed),
        "unsealed writer set SEALED flag"
    );
    assert!(
        !header.has_compatible_flag(HeaderCompatibleFlags::SealedContinuous),
        "unsealed writer set SEALED_CONTINUOUS flag"
    );
}

#[test]
fn sealed_writer_first_entry_future_epoch_passes_stock_verify() {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping first-entry future-epoch stock verify");
        return;
    }
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let seal = test_seal_opts();
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_seal(seal.clone()),
    )
    .expect("create sealed journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

    // Write the first entry at epoch 2 (realtime = start + 2 * interval = 3_000_000).
    // This exercises FSS epoch-evolution during the first-tag path.
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=future epoch first entry".as_slice()],
            3_000_000,
            100,
        )
        .expect("write first entry at future epoch");
    journal_file.sync().expect("sync journal");

    let key = verification_key(&seal);
    let output = Command::new("journalctl")
        .arg("--verify")
        .arg("--verify-key")
        .arg(&key)
        .arg("--file")
        .arg(&path)
        .output()
        .expect("run journalctl verify");
    assert!(
        output.status.success(),
        "journalctl verify failed for first-entry future-epoch sealed file: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn sealed_writer_entry_before_start_rejected() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let seal = test_seal_opts();
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_seal(seal.clone()),
    )
    .expect("create sealed journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

    // Stock verification rejects entries older than the first tag epoch,
    // so writers must reject this input instead of producing an invalid file.
    assert!(
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=before sealing start".as_slice()],
                500_000,
                100,
            )
            .is_err(),
        "expected before-start entry to be rejected"
    );
}

#[test]
fn sealed_writer_multi_interval_gap_passes_stock_verify() {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping multi-interval gap stock verify");
        return;
    }
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let seal = test_seal_opts();
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_seal(seal.clone()),
    )
    .expect("create sealed journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=epoch0".as_slice()],
            1_000_000,
            100,
        )
        .expect("write epoch 0");
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=epoch5".as_slice()],
            6_000_000,
            200,
        )
        .expect("write epoch 5");
    journal_file.sync().expect("sync journal");

    let key = verification_key(&seal);
    let output = Command::new("journalctl")
        .arg("--verify")
        .arg("--verify-key")
        .arg(&key)
        .arg("--file")
        .arg(&path)
        .output()
        .expect("run journalctl verify");
    assert!(
        output.status.success(),
        "journalctl verify failed for multi-interval gap sealed file: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn sealed_writer_unaligned_start_uses_systemd_epoch_boundary() {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping unaligned-start stock verify");
        return;
    }
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let seal = SealOptions::new([0u8; 12], 1_000_000, 1_702_717);
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_seal(seal.clone()),
    )
    .expect("create sealed journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

    for (idx, realtime) in [1_702_717, 2_100_000, 2_800_000].into_iter().enumerate() {
        let payload = format!("MESSAGE=unaligned-start-{idx}");
        writer
            .add_entry(
                &mut journal_file,
                &[payload.as_bytes()],
                realtime,
                (idx + 1) as u64,
            )
            .expect("write unaligned-start entry");
    }
    journal_file.sync().expect("sync journal");

    let key = verification_key(&seal);
    let output = Command::new("journalctl")
        .arg("--verify")
        .arg("--verify-key")
        .arg(&key)
        .arg("--file")
        .arg(&path)
        .output()
        .expect("run journalctl verify");
    assert!(
        output.status.success(),
        "journalctl verify failed for unaligned-start sealed file: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn sealed_writer_empty_file_passes_stock_verify() {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping empty sealed stock verify");
        return;
    }
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let seal = test_seal_opts();
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_seal(seal.clone()),
    )
    .expect("create sealed journal");
    let _writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    journal_file.sync().expect("sync journal");

    let key = verification_key(&seal);
    let output = Command::new("journalctl")
        .arg("--verify")
        .arg("--verify-key")
        .arg(&key)
        .arg("--file")
        .arg(&path)
        .output()
        .expect("run journalctl verify");
    assert!(
        output.status.success(),
        "journalctl verify failed for empty sealed file: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn compact_sealed_writer_passes_stock_verify() {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping compact+sealed stock verify");
        return;
    }
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let seal = test_seal_opts();
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_compact(true)
            .with_seal(seal.clone()),
    )
    .expect("create compact sealed journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    writer
        .add_entry(
            &mut journal_file,
            &[
                b"MESSAGE=compact sealed entry".as_slice(),
                b"PRIORITY=6".as_slice(),
            ],
            1_500_000,
            100,
        )
        .expect("write compact sealed entry");
    journal_file.sync().expect("sync journal");

    let key = verification_key(&seal);
    let output = Command::new("journalctl")
        .arg("--verify")
        .arg("--verify-key")
        .arg(&key)
        .arg("--file")
        .arg(&path)
        .output()
        .expect("run journalctl verify");
    assert!(
        output.status.success(),
        "journalctl verify failed for compact+sealed file: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
