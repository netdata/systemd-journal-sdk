use super::{
    FIELD_CACHE_MAX_ENTRIES, FIELD_CACHE_MAX_PAYLOAD_LEN, FieldCache, PayloadParts,
    zstd_frame_with_content_size,
};
use std::io::{Cursor, Read};
use std::num::NonZeroU64;

#[test]
fn field_cache_hits_exact_field_names() {
    let mut cache = FieldCache::new();
    let offset = NonZeroU64::new(8).unwrap();

    cache.insert(b"FIELD", offset);

    assert_eq!(cache.get(b"FIELD"), Some(offset));
    assert_eq!(cache.get(b"OTHER"), None);
}

#[test]
fn field_cache_skips_oversized_field_names() {
    let mut cache = FieldCache::new();
    let offset = NonZeroU64::new(16).unwrap();
    let oversized = vec![0x78_u8; FIELD_CACHE_MAX_PAYLOAD_LEN + 1];

    cache.insert(&oversized, offset);

    assert!(cache.get(&oversized).is_none());
    assert_eq!(cache.len(), 0);
}

#[test]
fn field_cache_stays_bounded_after_capacity_is_exceeded() {
    let mut cache = FieldCache::new();

    for index in 0..FIELD_CACHE_MAX_ENTRIES {
        let key = format!("FIELD_{index}");
        cache.insert(key.as_bytes(), NonZeroU64::new((index + 1) as u64).unwrap());
    }

    assert_eq!(cache.len(), FIELD_CACHE_MAX_ENTRIES);

    cache.insert(b"FIELD_OVERFLOW", NonZeroU64::new(9_999).unwrap());

    assert_eq!(
        cache.get(b"FIELD_OVERFLOW"),
        Some(NonZeroU64::new(9_999).unwrap())
    );
    assert!(cache.get(b"FIELD_0").is_none());
    assert!(cache.len() <= FIELD_CACHE_MAX_ENTRIES);
}

#[test]
fn zstd_frame_with_content_size_adds_decodable_frame_size() {
    let payload: Vec<u8> = (0..275usize).map(|index| (index % 26) as u8 + 65).collect();
    let frame = ruzstd::encoding::compress_to_vec(
        Cursor::new(payload.as_slice()),
        ruzstd::encoding::CompressionLevel::Fastest,
    );

    assert_eq!(&frame[..4], &[0x28, 0xb5, 0x2f, 0xfd]);
    assert_eq!(frame[4] >> 6, 0);
    assert_eq!(frame[4] & (1 << 5), 0);

    let patched = zstd_frame_with_content_size(frame, payload.len());

    assert_eq!(&patched[..4], &[0x28, 0xb5, 0x2f, 0xfd]);
    assert_eq!(patched[4] >> 6, 1);
    assert_ne!(patched[4] & (1 << 5), 0);
    assert_eq!(
        u16::from_le_bytes([patched[5], patched[6]]) as usize + 256,
        payload.len()
    );

    let mut decoder = ruzstd::decoding::StreamingDecoder::new(patched.as_slice()).unwrap();
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded).unwrap();

    assert_eq!(decoded, payload);
}

#[test]
fn zstd_frame_with_content_size_leaves_unsupported_frames_unchanged() {
    let invalid = vec![0, 1, 2, 3, 4, 5];
    assert_eq!(zstd_frame_with_content_size(invalid.clone(), 16), invalid);

    let payload = b"FRAME_CONTENT_SIZE_ALREADY_SET";
    let frame = ruzstd::encoding::compress_to_vec(
        Cursor::new(payload.as_slice()),
        ruzstd::encoding::CompressionLevel::Fastest,
    );
    let patched = zstd_frame_with_content_size(frame.clone(), payload.len());
    assert_eq!(
        zstd_frame_with_content_size(patched.clone(), payload.len()),
        patched
    );

    let mut dictionary_frame = frame;
    dictionary_frame[4] |= 1;
    assert_eq!(
        zstd_frame_with_content_size(dictionary_frame.clone(), payload.len()),
        dictionary_frame
    );
}

// ------------------------------------------------------------------
// Sealed writer tests
// ------------------------------------------------------------------

use super::{
    EntryField, EntryWriteOptions, FieldNamePolicy, JournalFile, JournalWriter, StructuredField,
};
use crate::error::JournalError;
use crate::file::{
    Compression, DEFAULT_COMPRESS_THRESHOLD, HeaderCompatibleFlags, HeaderIncompatibleFlags,
    JournalFileOptions, MIN_COMPRESS_THRESHOLD, MmapMut, ObjectFlags, normalize_compress_threshold,
};
use crate::seal::SealOptions;
#[cfg(unix)]
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn test_uuid(n: u8) -> uuid::Uuid {
    let mut bytes = [0u8; 16];
    bytes[15] = n;
    uuid::Uuid::from_bytes(bytes)
}

fn test_seal_opts() -> SealOptions {
    SealOptions::new([0u8; 12], 1_000_000, 1_000_000)
}

fn write_test_bytes_at(file: &mut std::fs::File, bytes: &[u8], offset: u64) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        file.write_all_at(bytes, offset)
    }

    #[cfg(not(unix))]
    {
        use std::io::{Seek, SeekFrom, Write};

        file.seek(SeekFrom::Start(offset))?;
        file.write_all(bytes)
    }
}

fn zstd_writer(threshold: usize) -> JournalWriter {
    JournalWriter {
        tail_object_offset: NonZeroU64::new(8).unwrap(),
        append_offset: NonZeroU64::new(16).unwrap(),
        next_seqnum: 1,
        num_written_objects: 0,
        first_tag_written: false,
        entry_items: Vec::new(),
        field_cache: FieldCache::new(),
        first_entry_monotonic: None,
        boot_id: test_uuid(4),
        compression: Compression::Zstd,
        compress_threshold: normalize_compress_threshold(threshold),
        live_publish_every_entries: 1,
        entries_since_live_publication: 0,
        seal: None,
    }
}

fn payload_with_total_len(len: usize) -> Vec<u8> {
    let mut payload = Vec::from([70_u8, 61]);
    payload.resize(len, 65);
    payload
}

#[test]
fn compression_threshold_matches_systemd_default_boundary() {
    let writer = zstd_writer(DEFAULT_COMPRESS_THRESHOLD);
    let below = payload_with_total_len(DEFAULT_COMPRESS_THRESHOLD - 1);
    let exact = payload_with_total_len(DEFAULT_COMPRESS_THRESHOLD);

    let below_payload = writer.stored_data_payload(PayloadParts::raw(&below));
    let stored_exact = writer.stored_data_payload(PayloadParts::raw(&exact));

    assert_eq!(below_payload.object_flags(), 0);
    assert_eq!(
        stored_exact.object_flags(),
        ObjectFlags::CompressedZstd as u8
    );
    assert!(stored_exact.len() < exact.len());
}

#[test]
fn compression_threshold_clamps_to_systemd_minimum() {
    assert_eq!(
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).compress_threshold(),
        DEFAULT_COMPRESS_THRESHOLD
    );
    assert_eq!(normalize_compress_threshold(0), MIN_COMPRESS_THRESHOLD);
    assert_eq!(normalize_compress_threshold(1), MIN_COMPRESS_THRESHOLD);
    assert_eq!(
        normalize_compress_threshold(MIN_COMPRESS_THRESHOLD),
        MIN_COMPRESS_THRESHOLD
    );
    assert_eq!(zstd_writer(1).compress_threshold, MIN_COMPRESS_THRESHOLD);
    assert_eq!(
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_compress_threshold(1)
            .compress_threshold(),
        MIN_COMPRESS_THRESHOLD
    );

    let writer = zstd_writer(1);
    let small = payload_with_total_len(MIN_COMPRESS_THRESHOLD - 1);
    let small_payload = writer.stored_data_payload(PayloadParts::raw(&small));
    assert_eq!(small_payload.object_flags(), 0);

    let payload = payload_with_total_len(DEFAULT_COMPRESS_THRESHOLD);
    let stored_payload = writer.stored_data_payload(PayloadParts::raw(&payload));
    assert_eq!(
        stored_payload.object_flags(),
        ObjectFlags::CompressedZstd as u8
    );
}

#[test]
fn writer_constructor_rejects_unkeyed_journal_without_panic() {
    let dir = TempDir::new().expect("create temp dir");
    let path = dir.path().join("unkeyed.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_keyed_hash(false),
    )
    .expect("create unkeyed journal");

    let err = match JournalWriter::new(&mut journal_file, 1, test_uuid(4)) {
        Ok(_) => panic!("unkeyed journal writer construction should be rejected"),
        Err(err) => err,
    };

    assert!(matches!(err, JournalError::UnsupportedJournalFile));
}

#[test]
fn writer_add_entry_rejects_unkeyed_journal_without_mutation() {
    let dir = TempDir::new().expect("create temp dir");
    let path = dir.path().join("unkeyed-after-construction.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
    )
    .expect("create journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

    journal_file.journal_header_mut().incompatible_flags &=
        !(HeaderIncompatibleFlags::KeyedHash as u32);
    let before_entries = journal_file.journal_header_ref().n_entries;
    let before_tail_seqnum = journal_file.journal_header_ref().tail_entry_seqnum;
    let before_tail_object = journal_file.journal_header_ref().tail_object_offset;

    let err = writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=blocked".as_slice()],
            1_700_000_060_000_000,
            100,
        )
        .unwrap_err();

    assert!(matches!(err, JournalError::UnsupportedJournalFile));
    assert_eq!(journal_file.journal_header_ref().n_entries, before_entries);
    assert_eq!(
        journal_file.journal_header_ref().tail_entry_seqnum,
        before_tail_seqnum
    );
    assert_eq!(
        journal_file.journal_header_ref().tail_object_offset,
        before_tail_object
    );
}

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

fn journalctl_available() -> bool {
    Command::new("journalctl").arg("--version").output().is_ok()
}

fn write_raw_test_journal(path: &Path, fields: &[&[u8]]) -> Vec<u8> {
    let repo_file =
        crate::repository::File::from_path(path).expect("test journal path should parse");
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_file_id(test_uuid(5)),
    )
    .expect("create raw journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    writer
        .add_entry(&mut journal_file, fields, 1_700_000_060_000_000, 100)
        .expect("write raw entry");
    journal_file.sync().expect("sync raw journal");
    drop(journal_file);
    std::fs::read(path).expect("read raw journal")
}

fn write_structured_test_journal(
    path: &Path,
    fields: &[StructuredField<'_>],
    options: EntryWriteOptions,
) -> Vec<u8> {
    let repo_file =
        crate::repository::File::from_path(path).expect("test journal path should parse");
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_file_id(test_uuid(5)),
    )
    .expect("create structured journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    writer
        .add_entry_structured_with_options(
            &mut journal_file,
            fields,
            1_700_000_060_000_000,
            100,
            options,
        )
        .expect("write structured entry");
    journal_file.sync().expect("sync structured journal");
    drop(journal_file);
    std::fs::read(path).expect("read structured journal")
}

fn write_entry_fields_test_journal(
    path: &Path,
    fields: &[EntryField<'_>],
    options: EntryWriteOptions,
) -> (Vec<u8>, usize, Vec<Vec<u8>>) {
    let repo_file =
        crate::repository::File::from_path(path).expect("test journal path should parse");
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_file_id(test_uuid(5)),
    )
    .expect("create entry-fields journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    writer
        .add_entry_fields_with_options(
            &mut journal_file,
            fields.iter().copied(),
            1_700_000_060_000_000,
            100,
            options,
        )
        .expect("write entry fields");

    let mut entry_offsets = Vec::new();
    journal_file
        .entry_offsets(&mut entry_offsets)
        .expect("collect entry offsets");
    let entry_offset = entry_offsets[0];
    let entry_item_count = {
        let entry = journal_file.entry_ref(entry_offset).expect("entry ref");
        entry.items.len()
    };
    let payloads = journal_file
        .entry_data_objects(entry_offset)
        .expect("entry data iterator")
        .map(|item| item.map(|object| object.raw_payload().to_vec()))
        .collect::<crate::error::Result<Vec<_>>>()
        .expect("read payloads");

    journal_file.sync().expect("sync entry-fields journal");
    drop(journal_file);
    (
        std::fs::read(path).expect("read entry-fields journal"),
        entry_item_count,
        payloads,
    )
}

#[test]
fn entry_seqnum_override_preserves_gaps() {
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
    let mut writer =
        JournalWriter::new(&mut journal_file, 10, test_uuid(4)).expect("create writer");

    for (idx, seqnum) in [10, 12, 20].into_iter().enumerate() {
        let payload = format!("MESSAGE=seqnum-{seqnum}");
        writer
            .add_entry_fields_with_options(
                &mut journal_file,
                [EntryField::raw(payload.as_bytes())],
                1_700_000_060_000_000 + idx as u64,
                idx as u64 + 1,
                EntryWriteOptions::default().seqnum(seqnum),
            )
            .expect("write entry with seqnum override");
    }
    assert!(
        writer
            .add_entry_fields_with_options(
                &mut journal_file,
                [EntryField::raw(b"MESSAGE=backwards")],
                1_700_000_060_000_010,
                10,
                EntryWriteOptions::default().seqnum(19),
            )
            .is_err(),
        "writer accepted a backwards seqnum override"
    );

    let header = journal_file.journal_header_ref();
    assert_eq!(header.head_entry_seqnum, 10);
    assert_eq!(header.tail_entry_seqnum, 20);
    assert_eq!(writer.next_seqnum(), 21);

    let mut entry_offsets = Vec::new();
    journal_file
        .entry_offsets(&mut entry_offsets)
        .expect("collect entry offsets");
    let seqnums = entry_offsets
        .iter()
        .map(|offset| {
            journal_file
                .entry_ref(*offset)
                .expect("entry ref")
                .header
                .seqnum
        })
        .collect::<Vec<_>>();
    assert_eq!(seqnums, vec![10, 12, 20]);
}

#[test]
fn entry_boot_id_override_preserves_multiboot_ordering() {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping multiboot stock verify");
        return;
    }
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let boot_a = test_uuid(4);
    let boot_b = test_uuid(5);
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), boot_a, test_uuid(3)),
    )
    .expect("create journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, boot_a).expect("create writer");

    let entries = [
        (boot_a, 1_700_000_060_000_000, 100),
        (boot_a, 1_700_000_060_000_001, 200),
        (boot_b, 1_700_000_060_000_002, 50),
    ];
    for (idx, (boot_id, realtime, monotonic)) in entries.into_iter().enumerate() {
        let payload = format!("MESSAGE=boot-override-{idx}");
        writer
            .add_entry_fields_with_options(
                &mut journal_file,
                [EntryField::raw(payload.as_bytes())],
                realtime,
                monotonic,
                EntryWriteOptions::default().boot_id(boot_id),
            )
            .expect("write entry with boot override");
    }
    journal_file.sync().expect("sync journal");

    let mut entry_offsets = Vec::new();
    journal_file
        .entry_offsets(&mut entry_offsets)
        .expect("collect entry offsets");
    let boot_ids = entry_offsets
        .iter()
        .map(|offset| {
            journal_file
                .entry_ref(*offset)
                .expect("entry ref")
                .header
                .boot_id
        })
        .collect::<Vec<_>>();
    assert_eq!(
        boot_ids,
        vec![*boot_a.as_bytes(), *boot_a.as_bytes(), *boot_b.as_bytes()]
    );
    assert_eq!(
        journal_file.journal_header_ref().tail_entry_boot_id,
        *boot_b.as_bytes()
    );

    let output = Command::new("journalctl")
        .arg("--verify")
        .arg("--file")
        .arg(&path)
        .output()
        .expect("run journalctl verify");
    assert!(
        output.status.success(),
        "journalctl verify failed for multiboot boot-id override: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[path = "writer_structured_tests.rs"]
mod structured_tests;

#[path = "writer_seal_tests.rs"]
mod seal_tests;

#[test]
fn compact_writer_grows_arena_past_initial_allocation() {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping compact arena growth stock verify");
        return;
    }
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_compact(true),
    )
    .expect("create compact journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

    for index in 0..10u8 {
        let mut payload = b"BLOB=".to_vec();
        payload.resize(payload.len() + 1024 * 1024, index);
        writer
            .add_entry(
                &mut journal_file,
                &[payload.as_slice()],
                2_000_000 + u64::from(index),
                100 + u64::from(index),
            )
            .expect("write large compact entry");
    }
    journal_file.sync().expect("sync compact journal");

    let header = journal_file.journal_header_ref();
    assert!(
        header.header_size + header.arena_size > super::FILE_SIZE_INCREASE,
        "arena size did not grow past the initial allocation"
    );

    let output = Command::new("journalctl")
        .arg("--verify")
        .arg("--file")
        .arg(&path)
        .output()
        .expect("run journalctl verify");
    assert!(
        output.status.success(),
        "journalctl verify failed for grown compact file: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn writer_initial_arena_covers_large_hash_tables() {
    if !journalctl_available() {
        eprintln!("journalctl not available; skipping large hash table stock verify");
        return;
    }
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
            .with_compact(true)
            .with_data_hash_table_buckets(600_000)
            .with_field_hash_table_buckets(1_023),
    )
    .expect("create journal with large hash tables");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    writer
        .add_entry(
            &mut journal_file,
            &[b"MESSAGE=large hash table".as_slice()],
            1_700_000_060_000_000,
            1,
        )
        .expect("write entry after large hash table initialization");
    journal_file.sync().expect("sync journal");

    let header = journal_file.journal_header_ref();
    assert!(
        header.header_size + header.arena_size > super::FILE_SIZE_INCREASE,
        "initial arena did not cover large hash tables"
    );

    let output = Command::new("journalctl")
        .arg("--verify")
        .arg("--file")
        .arg(&path)
        .output()
        .expect("run journalctl verify");
    assert!(
        output.status.success(),
        "journalctl verify failed for large-hash-table file: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
