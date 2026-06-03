use super::*;
use crate::JournalWriter;
use zerocopy::IntoBytes;

#[derive(Clone, Copy, Debug)]
struct ExpectedSanitizedHeader {
    header_size: u64,
    n_data: u64,
    n_fields: u64,
    n_tags: u64,
    n_entry_arrays: u64,
    data_hash_chain_depth: u64,
    field_hash_chain_depth: u64,
    tail_entry_array_offset: u32,
    tail_entry_array_n_entries: u32,
    tail_entry_offset: u64,
}

const HEADER_SANITIZE_CASES: &[ExpectedSanitizedHeader] = &[
    ExpectedSanitizedHeader {
        header_size: 208,
        n_data: 0,
        n_fields: 0,
        n_tags: 0,
        n_entry_arrays: 0,
        data_hash_chain_depth: 0,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 216,
        n_data: 11,
        n_fields: 0,
        n_tags: 0,
        n_entry_arrays: 0,
        data_hash_chain_depth: 0,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 220,
        n_data: 11,
        n_fields: 0,
        n_tags: 0,
        n_entry_arrays: 0,
        data_hash_chain_depth: 0,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 224,
        n_data: 11,
        n_fields: 22,
        n_tags: 0,
        n_entry_arrays: 0,
        data_hash_chain_depth: 0,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 232,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 0,
        data_hash_chain_depth: 0,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 240,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 0,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 248,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 250,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 0,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 256,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 66,
        tail_entry_array_offset: 0,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 260,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 66,
        tail_entry_array_offset: 77,
        tail_entry_array_n_entries: 0,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 264,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 66,
        tail_entry_array_offset: 77,
        tail_entry_array_n_entries: 88,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 268,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 66,
        tail_entry_array_offset: 77,
        tail_entry_array_n_entries: 88,
        tail_entry_offset: 0,
    },
    ExpectedSanitizedHeader {
        header_size: 272,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 66,
        tail_entry_array_offset: 77,
        tail_entry_array_n_entries: 88,
        tail_entry_offset: 99,
    },
    ExpectedSanitizedHeader {
        header_size: 300,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 66,
        tail_entry_array_offset: 77,
        tail_entry_array_n_entries: 88,
        tail_entry_offset: 99,
    },
];

#[test]
fn sanitize_header_for_historical_size_matches_per_field_boundaries() {
    for expected in HEADER_SANITIZE_CASES {
        assert_sanitized_header(*expected);
    }
}

fn assert_sanitized_header(expected: ExpectedSanitizedHeader) {
    let sanitized = sanitize_header_for_size(JournalHeader {
        header_size: expected.header_size,
        n_data: 11,
        n_fields: 22,
        n_tags: 33,
        n_entry_arrays: 44,
        data_hash_chain_depth: 55,
        field_hash_chain_depth: 66,
        tail_entry_array_offset: 77,
        tail_entry_array_n_entries: 88,
        tail_entry_offset: 99,
        ..JournalHeader::default()
    });

    assert_eq!(sanitized.n_data, expected.n_data, "{expected:?}");
    assert_eq!(sanitized.n_fields, expected.n_fields, "{expected:?}");
    assert_eq!(sanitized.n_tags, expected.n_tags, "{expected:?}");
    assert_eq!(
        sanitized.n_entry_arrays, expected.n_entry_arrays,
        "{expected:?}"
    );
    assert_eq!(
        sanitized.data_hash_chain_depth, expected.data_hash_chain_depth,
        "{expected:?}"
    );
    assert_eq!(
        sanitized.field_hash_chain_depth, expected.field_hash_chain_depth,
        "{expected:?}"
    );
    assert_eq!(
        sanitized.tail_entry_array_offset, expected.tail_entry_array_offset,
        "{expected:?}"
    );
    assert_eq!(
        sanitized.tail_entry_array_n_entries, expected.tail_entry_array_n_entries,
        "{expected:?}"
    );
    assert_eq!(
        sanitized.tail_entry_offset, expected.tail_entry_offset,
        "{expected:?}"
    );
}

fn data_object_bytes(payload: &[u8], flags: u8) -> Vec<u8> {
    let header = DataObjectHeader {
        object_header: ObjectHeader {
            type_: ObjectType::Data as u8,
            flags,
            reserved: [0; 6],
            size: (std::mem::size_of::<DataObjectHeader>() + payload.len()) as u64,
        },
        hash: 0,
        next_hash_offset: None,
        next_field_offset: None,
        entry_offset: None,
        entry_array_offset: None,
        n_entries: None,
    };

    let mut bytes = Vec::with_capacity(header.object_header.size as usize);
    bytes.extend_from_slice(header.as_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

#[test]
fn data_payload_matcher_matches_lz4_compressed_payload() {
    let payload = b"_SYSTEMD_UNIT=netdata.service";
    let compressed = lz4_flex::block::compress(payload);
    let mut stored_payload = Vec::with_capacity(std::mem::size_of::<u64>() + compressed.len());
    stored_payload.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    stored_payload.extend_from_slice(&compressed);

    let bytes = data_object_bytes(&stored_payload, ObjectFlags::CompressedLz4 as u8);
    let object = DataObject::from_data(bytes.as_slice(), false).unwrap();

    let mut matcher = DataPayloadMatcher::new(payload, 0);
    assert!(matcher.payload_matches(&object).unwrap());
}

#[cfg(unix)]
#[test]
fn create_uses_configured_file_mode() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let temp_file = tempfile::NamedTempFile::new().map_err(JournalError::Io)?;
    let options = JournalFileOptions::new([1; 16], [2; 16], [3; 16], [4; 16]).with_file_mode(0o600);
    let journal_file = JournalFile::<memmap2::MmapMut>::create(temp_file.path(), options)?;
    drop(journal_file);

    let mode = std::fs::metadata(temp_file.path())
        .map_err(JournalError::Io)?
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    Ok(())
}

#[test]
fn find_data_offset_matches_lz4_compressed_payload_in_hash_bucket() -> Result<()> {
    let payload = b"_SYSTEMD_UNIT=netdata.service";
    let temp_file = tempfile::NamedTempFile::new().map_err(JournalError::Io)?;
    let options = JournalFileOptions::new([1; 16], [2; 16], [3; 16], [4; 16]);
    let mut journal_file = JournalFile::<memmap2::MmapMut>::create(temp_file.path(), options)?;
    let data_offset = {
        let writer = JournalWriter::new(&mut journal_file)?;
        NonZeroU64::new(writer.current_file_size()).unwrap()
    };
    let hash = journal_file.hash(payload);

    let compressed = lz4_flex::block::compress(payload);
    let mut stored_payload = Vec::with_capacity(std::mem::size_of::<u64>() + compressed.len());
    stored_payload.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    stored_payload.extend_from_slice(&compressed);

    {
        let mut data_guard =
            journal_file.data_mut(data_offset, Some(stored_payload.len() as u64))?;
        data_guard.header.hash = hash;
        data_guard.header.object_header.flags = ObjectFlags::CompressedLz4 as u8;
        data_guard.set_payload(&stored_payload);
    }

    journal_file.data_hash_table_set_tail_offset(hash, data_offset)?;

    assert_eq!(
        journal_file.find_data_offset(hash, payload)?,
        Some(data_offset)
    );
    assert_eq!(
        journal_file.find_data_offset(hash, b"_SYSTEMD_UNIT=sshd.service")?,
        None
    );

    Ok(())
}

#[test]
fn find_data_offset_skips_bad_compressed_payload_in_hash_bucket() -> Result<()> {
    let payload = b"_SYSTEMD_UNIT=netdata.service";
    let temp_file = tempfile::NamedTempFile::new().map_err(JournalError::Io)?;
    let options = JournalFileOptions::new([1; 16], [2; 16], [3; 16], [4; 16]);
    let mut journal_file = JournalFile::<memmap2::MmapMut>::create(temp_file.path(), options)?;
    let bad_offset = {
        let writer = JournalWriter::new(&mut journal_file)?;
        NonZeroU64::new(writer.current_file_size()).unwrap()
    };
    let hash = journal_file.hash(payload);

    let bad_size = {
        let mut data_guard = journal_file.data_mut(bad_offset, Some(5))?;
        data_guard.header.hash = hash;
        data_guard.header.object_header.flags = ObjectFlags::CompressedLz4 as u8;
        data_guard.set_payload(b"short");

        data_guard.header.object_header.aligned_size()
    };
    let good_offset = NonZeroU64::new(bad_offset.get() + bad_size).unwrap();

    let compressed = lz4_flex::block::compress(payload);
    let mut stored_payload = Vec::with_capacity(std::mem::size_of::<u64>() + compressed.len());
    stored_payload.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    stored_payload.extend_from_slice(&compressed);

    {
        let mut data_guard =
            journal_file.data_mut(good_offset, Some(stored_payload.len() as u64))?;
        data_guard.header.hash = hash;
        data_guard.header.object_header.flags = ObjectFlags::CompressedLz4 as u8;
        data_guard.set_payload(&stored_payload);
    }

    journal_file.data_hash_table_set_tail_offset(hash, bad_offset)?;
    journal_file.data_hash_table_set_tail_offset(hash, good_offset)?;

    assert_eq!(
        journal_file.find_data_offset(hash, payload)?,
        Some(good_offset)
    );

    Ok(())
}

#[test]
fn data_payload_matcher_rejects_different_compressed_payload() {
    let payload = b"_SYSTEMD_UNIT=netdata.service";
    let compressed = lz4_flex::block::compress(payload);
    let mut stored_payload = Vec::with_capacity(std::mem::size_of::<u64>() + compressed.len());
    stored_payload.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    stored_payload.extend_from_slice(&compressed);

    let bytes = data_object_bytes(&stored_payload, ObjectFlags::CompressedLz4 as u8);
    let object = DataObject::from_data(bytes.as_slice(), false).unwrap();

    let mut matcher = DataPayloadMatcher::new(b"_SYSTEMD_UNIT=sshd.service", 0);
    assert!(!matcher.payload_matches(&object).unwrap());
}
