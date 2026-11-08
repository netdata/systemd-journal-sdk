use super::{
    EntryItem, FIELD_CACHE_MAX_ENTRIES, FIELD_CACHE_MAX_PAYLOAD_LEN, FieldCache, PayloadParts,
    ResolvedDataLinkState, zstd_frame_with_content_size,
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
fn trusted_unique_duplicate_offsets_use_authoritative_fallback_state() {
    let offset = NonZeroU64::new(64).unwrap();
    let link_state = ResolvedDataLinkState::empty();
    let mut writer = zstd_writer(DEFAULT_COMPRESS_THRESHOLD);
    writer.entry_items = vec![
        EntryItem {
            offset,
            hash: 1,
            link_state: Some(link_state),
        },
        EntryItem {
            offset,
            hash: 1,
            link_state: Some(link_state),
        },
        EntryItem {
            offset,
            hash: 1,
            link_state: Some(link_state),
        },
    ];

    writer.finish_entry_items(true).unwrap();

    assert_eq!(writer.entry_items[0].link_state, Some(link_state));
    assert_eq!(writer.entry_items[1].link_state, None);
    assert_eq!(writer.entry_items[2].link_state, None);
}

#[test]
fn default_duplicate_elimination_keeps_one_resolved_state() {
    let offset = NonZeroU64::new(64).unwrap();
    let link_state = ResolvedDataLinkState::empty();
    let mut writer = zstd_writer(DEFAULT_COMPRESS_THRESHOLD);
    writer.entry_items = vec![
        EntryItem {
            offset,
            hash: 1,
            link_state: Some(link_state),
        },
        EntryItem {
            offset,
            hash: 1,
            link_state: Some(link_state),
        },
    ];

    writer.finish_entry_items(false).unwrap();

    assert_eq!(writer.entry_items.len(), 1);
    assert_eq!(writer.entry_items[0].link_state, Some(link_state));
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
    Compression, DEFAULT_COMPRESS_THRESHOLD, DataPayloadType, HeaderCompatibleFlags,
    HeaderIncompatibleFlags, JournalFileOptions, MIN_COMPRESS_THRESHOLD, MmapMut, ObjectFlags,
    normalize_compress_threshold,
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
        poisoned: false,
        fail_append_stage: 0,
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
        entry_growth_bound: 0,
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
fn new_data_entry_item_starts_with_empty_link_state() {
    let dir = TempDir::new().expect("create temp dir");
    let path = dir.path().join("new-data-link-state.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
    )
    .expect("create link-state journal");
    let mut writer =
        JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create link-state writer");
    let payload = b"MESSAGE=new-data-link-state";

    let item = writer
        .add_data(&mut journal_file, EntryField::raw(payload))
        .expect("add new DATA object");

    assert_eq!(item.link_state, Some(ResolvedDataLinkState::empty()));
    let (_, resolved_state) = journal_file
        .find_data_with_link_state_parts(item.hash, PayloadParts::raw(payload))
        .expect("resolve new DATA object")
        .expect("find new DATA object");
    assert_eq!(resolved_state, ResolvedDataLinkState::empty());
}

#[test]
fn resolved_data_link_state_tracks_regular_and_compact_tail_growth() {
    let dir = TempDir::new().expect("create temp dir");
    let payload = b"MESSAGE=reused-link-state".as_slice();

    for compact in [false, true] {
        let path = dir.path().join(if compact {
            "compact-state.journal"
        } else {
            "regular-state.journal"
        });
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_compact(compact),
        )
        .expect("create link-state journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        let mut head_array_offset = None;

        for entry_count in 1..=6u64 {
            writer
                .add_entry(
                    &mut journal_file,
                    &[payload],
                    1_700_000_060_000_000 + entry_count,
                    100 + entry_count,
                )
                .expect("write repeated DATA entry");

            let hash = journal_file.hash(payload);
            let (_, link_state) = journal_file
                .find_data_with_link_state_parts(hash, PayloadParts::raw(payload))
                .expect("resolve DATA state")
                .expect("find repeated DATA");
            assert_eq!(link_state.n_entries.map(NonZeroU64::get), Some(entry_count));

            if entry_count == 1 {
                assert_eq!(link_state.entry_array_offset, None);
                assert_eq!(link_state.compact_tail, None);
                continue;
            }

            let array_offset = link_state
                .entry_array_offset
                .expect("DATA entry array after promotion");
            assert_eq!(*head_array_offset.get_or_insert(array_offset), array_offset);
            if compact {
                let (tail_offset, tail_entries) =
                    link_state.compact_tail.expect("compact DATA tail state");
                if entry_count <= 5 {
                    assert_eq!(tail_offset, array_offset);
                    assert_eq!(tail_entries, entry_count - 1);
                } else {
                    assert_ne!(tail_offset, array_offset);
                    assert_eq!(tail_entries, 1);
                }
            } else {
                assert_eq!(link_state.compact_tail, None);
            }
        }

        journal_file.sync().expect("sync link-state journal");
        if journalctl_available() {
            let output = Command::new("journalctl")
                .arg("--verify")
                .arg("--file")
                .arg(&path)
                .output()
                .expect("run journalctl verify");
            assert!(
                output.status.success(),
                "journalctl verify failed for link-state journal: stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn trusted_unique_duplicate_fallback_reads_updated_data_state() {
    let dir = TempDir::new().expect("create temp dir");
    let payload = b"MESSAGE=trusted-duplicate".as_slice();

    for compact in [false, true] {
        let path = dir.path().join(if compact {
            "compact-duplicate.journal"
        } else {
            "regular-duplicate.journal"
        });
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_compact(compact),
        )
        .expect("create duplicate-state journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

        for index in 0..2u64 {
            writer
                .add_entry_fields_with_options(
                    &mut journal_file,
                    [EntryField::raw(payload), EntryField::raw(payload)],
                    1_700_000_060_000_000 + index,
                    100 + index,
                    EntryWriteOptions::default().trusted_unique_payloads(true),
                )
                .expect("write trusted duplicate entry");
        }

        let hash = journal_file.hash(payload);
        let (_, link_state) = journal_file
            .find_data_with_link_state_parts(hash, PayloadParts::raw(payload))
            .expect("resolve duplicate DATA state")
            .expect("find duplicate DATA");
        assert_eq!(link_state.n_entries.map(NonZeroU64::get), Some(4));

        let mut entry_offsets = Vec::new();
        journal_file
            .entry_offsets(&mut entry_offsets)
            .expect("collect duplicate entry offsets");
        assert_eq!(entry_offsets.len(), 2);
        for entry_offset in entry_offsets {
            assert_eq!(
                journal_file
                    .entry_ref(entry_offset)
                    .expect("duplicate entry ref")
                    .items
                    .len(),
                2
            );
        }
    }
}

#[test]
fn compact_invalid_cached_tail_falls_back_to_authoritative_array_chain() {
    let dir = TempDir::new().expect("create temp dir");
    let path = dir.path().join("compact-tail-fallback.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_compact(true),
    )
    .expect("create compact fallback journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
    let payload = b"MESSAGE=compact-tail-fallback".as_slice();

    for index in 0..2u64 {
        writer
            .add_entry(
                &mut journal_file,
                &[payload],
                1_700_000_060_000_000 + index,
                100 + index,
            )
            .expect("write compact fallback seed entry");
    }
    let hash = journal_file.hash(payload);
    let (data_offset, original_state) = journal_file
        .find_data_with_link_state_parts(hash, PayloadParts::raw(payload))
        .expect("resolve compact DATA state")
        .expect("find compact DATA");
    let array_offset = original_state
        .entry_array_offset
        .expect("promoted compact DATA array");

    {
        let mut data_guard = journal_file
            .data_mut(data_offset, None)
            .expect("open compact DATA");
        let DataPayloadType::Compact { compact_fields, .. } = &mut data_guard.payload else {
            panic!("expected compact DATA");
        };
        compact_fields.tail_entry_array_offset = 8;
        compact_fields.tail_entry_array_n_entries = 1;
    }

    writer
        .add_entry(&mut journal_file, &[payload], 1_700_000_060_000_002, 102)
        .expect("append through authoritative array fallback");

    let (_, repaired_state) = journal_file
        .find_data_with_link_state_parts(hash, PayloadParts::raw(payload))
        .expect("resolve repaired compact DATA state")
        .expect("find repaired compact DATA");
    assert_eq!(repaired_state.n_entries.map(NonZeroU64::get), Some(3));
    assert_eq!(repaired_state.entry_array_offset, Some(array_offset));
    assert_eq!(repaired_state.compact_tail, Some((array_offset, 2)));
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
fn same_boot_monotonic_is_clamped_by_low_level_writer() {
    let dir = TempDir::new().expect("create temp dir");
    let journal_dir = dir.path().join("journals");
    std::fs::create_dir_all(&journal_dir).expect("create journal dir");
    let path = journal_dir.join("system.journal");
    let repo_file =
        crate::repository::File::from_path(&path).expect("test journal path should parse");

    let boot_id = test_uuid(4);
    let mut journal_file = JournalFile::create(
        &repo_file,
        JournalFileOptions::new(test_uuid(1), boot_id, test_uuid(3)),
    )
    .expect("create journal");
    let mut writer = JournalWriter::new(&mut journal_file, 1, boot_id).expect("create writer");

    writer
        .add_entry_fields_with_options(
            &mut journal_file,
            [EntryField::raw(b"MESSAGE=first")],
            1_700_000_060_000_000,
            10,
            EntryWriteOptions::default(),
        )
        .expect("write first entry");
    writer
        .add_entry_fields_with_options(
            &mut journal_file,
            [EntryField::raw(b"MESSAGE=second")],
            1_700_000_060_000_001,
            5,
            EntryWriteOptions::default(),
        )
        .expect("write clamped entry");

    let mut entry_offsets = Vec::new();
    journal_file
        .entry_offsets(&mut entry_offsets)
        .expect("collect entry offsets");
    let monotonic_values = entry_offsets
        .iter()
        .map(|offset| {
            journal_file
                .entry_ref(*offset)
                .expect("entry ref")
                .header
                .monotonic
        })
        .collect::<Vec<_>>();
    assert_eq!(monotonic_values, vec![10, 11]);
    assert_eq!(journal_file.journal_header_ref().tail_entry_monotonic, 11);
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

#[test]
fn all_mutating_append_stages_poison_writer_and_block_retry() {
    for compact in [false, true] {
        for stage in 1..=4 {
            let dir = TempDir::new().unwrap();
            let path = dir.path().join("failure.journal");
            let repository = crate::repository::File::from_path(&path).unwrap();
            let mut file = JournalFile::create(
                &repository,
                JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                    .with_compact(compact),
            )
            .unwrap();
            let mut writer = JournalWriter::new(&mut file, 1, test_uuid(2)).unwrap();
            writer.fail_append_stage = stage;
            assert!(
                writer
                    .add_entry(&mut file, &[b"MESSAGE=value"], 1_000_000, 1)
                    .is_err()
            );
            assert!(writer.is_poisoned());
            let before = std::fs::read(&path).unwrap();
            writer.fail_append_stage = 0;
            assert!(matches!(
                writer.add_entry(&mut file, &[b"MESSAGE=retry"], 2_000_000, 2),
                Err(JournalError::WriterPoisoned)
            ));
            drop(writer);
            drop(file);
            assert_eq!(before, std::fs::read(path).unwrap());
        }
    }
}

#[test]
fn compact_preflight_failure_without_mutation_allows_retry() {
    for structured in [false, true] {
        for existing_data in [false, true] {
            let dir = TempDir::new().unwrap();
            let path = dir.path().join("preflight.journal");
            let repository = crate::repository::File::from_path(&path).unwrap();
            let mut file = JournalFile::create(
                &repository,
                JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                    .with_compact(true),
            )
            .unwrap();
            let mut writer = JournalWriter::new(&mut file, 1, test_uuid(2)).unwrap();
            writer
                .add_entry(&mut file, &[b"MESSAGE=value"], 1_000_000, 1)
                .unwrap();
            // Exercise the size preflight without allocating a four-gigabyte fixture.
            let actual_offset = writer.append_offset;
            writer.append_offset = NonZeroU64::new(super::JOURNAL_COMPACT_SIZE_MAX - 7).unwrap();
            let before = std::fs::read(&path).unwrap();
            let payload: &[u8] = if existing_data {
                b"MESSAGE=value"
            } else {
                b"MESSAGE=new"
            };
            let field = if structured {
                EntryField::structured(b"MESSAGE", if existing_data { b"value" } else { b"new" })
            } else {
                EntryField::raw(payload)
            };
            assert!(matches!(
                writer.add_entry_fields(&mut file, [field], 2_000_000, 2),
                Err(JournalError::JournalFileFull)
            ));
            assert_eq!(before, std::fs::read(&path).unwrap());
            assert!(
                !writer.is_poisoned(),
                "read-only size rejection poisoned the writer"
            );
            writer.append_offset = actual_offset;
            writer
                .add_entry(&mut file, &[b"MESSAGE=value"], 2_000_000, 2)
                .unwrap();
            assert_eq!(file.journal_header_ref().n_entries, 2);
        }
    }
}

#[test]
fn append_validation_poison_depends_on_prior_publication_mutation() {
    for structured in [false, true] {
        for sealed in [false, true] {
            for (value, realtime, poisoned) in [
                ("value", 1_500_001, false),
                ("new", 1_500_001, true),
                ("value", 2_500_000, sealed),
            ] {
                let dir = TempDir::new().unwrap();
                let path = dir.path().join("validation.journal");
                let repository = crate::repository::File::from_path(&path).unwrap();
                let mut options = JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3));
                if sealed {
                    options = options.with_seal(test_seal_opts());
                }
                let mut file = JournalFile::create(&repository, options).unwrap();
                let mut writer = JournalWriter::new(&mut file, 1, test_uuid(2)).unwrap();
                writer
                    .add_entry(&mut file, &[b"MESSAGE=value"], 1_500_000, 1)
                    .unwrap();
                let before = std::fs::read(&path).unwrap();
                let payload = format!("MESSAGE={value}");
                let fields = if structured {
                    [
                        EntryField::structured(b"MESSAGE", value.as_bytes()),
                        EntryField::structured(b"", b"bad"),
                    ]
                } else {
                    [EntryField::raw(payload.as_bytes()), EntryField::raw(b"bad")]
                };
                assert!(
                    writer
                        .add_entry_fields(&mut file, fields, realtime, 2)
                        .is_err()
                );
                assert_eq!(
                    writer.is_poisoned(),
                    poisoned,
                    "sealed={sealed} value={value} realtime={realtime}"
                );
                if poisoned {
                    assert!(matches!(
                        writer.add_entry(&mut file, &[b"MESSAGE=value"], realtime, 2),
                        Err(JournalError::WriterPoisoned)
                    ));
                } else {
                    assert_eq!(before, std::fs::read(&path).unwrap());
                    writer
                        .add_entry(&mut file, &[b"MESSAGE=value"], realtime, 2)
                        .unwrap();
                    assert_eq!(file.journal_header_ref().n_entries, 2);
                }
            }
        }
    }
}
