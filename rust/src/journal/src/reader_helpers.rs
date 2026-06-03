use super::*;
use std::collections::{HashMap, HashSet};
use std::fs::File;

pub(super) fn open_journal_file(path: &Path, options: ReaderOptions) -> Result<JournalFile<Mmap>> {
    let file = match options.bounds {
        ReaderBounds::Live => {
            JournalFile::open_path_with_strategy(path, options.window_size, options.mmap_strategy)
        }
        ReaderBounds::Snapshot => {
            JournalFile::open_path_snapshot(path, options.window_size, options.mmap_strategy)
        }
    };
    file.map_err(Into::into)
}

pub(super) fn build_cursor(
    file: &JournalFile<Mmap>,
    reader: &JournalReader<'_, Mmap>,
) -> Result<String> {
    let (seqnum, seqnum_id) = reader.get_seqnum(file)?;
    let offset = reader.get_entry_offset()?;
    let entry = file.entry_ref(offset)?;
    Ok(format_cursor_from_key(DirectoryEntryKey {
        seqnum_id,
        seqnum,
        boot_id: entry.header.boot_id,
        monotonic: entry.header.monotonic,
        realtime: entry.header.realtime,
        xor_hash: entry.header.xor_hash,
    }))
}

pub(super) fn format_cursor_from_key(key: DirectoryEntryKey) -> String {
    format!(
        "s={};j={};c={:016x};n={}",
        hex::encode(key.seqnum_id),
        hex::encode(key.boot_id),
        key.realtime,
        key.seqnum
    )
}

pub(super) fn read_entry_at(
    file: &JournalFile<Mmap>,
    reader: &JournalReader<'_, Mmap>,
    entry_offset: NonZeroU64,
    data_offsets: &mut Vec<NonZeroU64>,
    decompressed: &mut Vec<u8>,
) -> Result<Entry> {
    let (seqnum, realtime, monotonic, boot_id) =
        collect_entry_metadata_and_data_offsets(file, entry_offset, data_offsets)?;

    let mut fields = HashMap::new();
    let mut field_values: HashMap<String, Vec<Vec<u8>>> = HashMap::new();
    let mut payloads = Vec::new();

    payloads.reserve(data_offsets.len());

    for data_offset in data_offsets.iter().copied() {
        let data = match file.data_ref(data_offset) {
            Ok(data) => data,
            Err(err) if recoverable_entry_data_error(&err) => continue,
            Err(err) => return Err(err.into()),
        };
        let payload = if data.is_compressed() {
            decompressed.clear();
            data.decompress(decompressed)?;
            decompressed.as_slice()
        } else {
            data.raw_payload()
        };

        payloads.push(payload.to_vec());
        if let Some(eq) = payload.iter().position(|byte| *byte == b'=') {
            let raw_name = &payload[..eq];
            let value = payload[eq + 1..].to_vec();
            if let Ok(name) = std::str::from_utf8(raw_name) {
                let name = name.to_string();
                fields.insert(name.clone(), value.clone());
                field_values.entry(name).or_default().push(value);
            }
        }
    }

    Ok(Entry {
        fields,
        field_values,
        payloads,
        seqnum,
        realtime,
        monotonic,
        boot_id,
        cursor: build_cursor(file, reader)?,
    })
}

pub(super) fn visit_entry_payload_offsets<F>(
    file: &JournalFile<Mmap>,
    data_offsets: &[NonZeroU64],
    decompressed: &mut Vec<u8>,
    mut visitor: F,
) -> Result<()>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    let context = file.data_payload_read_context();
    for data_offset in data_offsets.iter().copied() {
        let mut visitor_result = Ok(());
        match file.visit_data_payload_at_with_context(
            context,
            data_offset,
            decompressed,
            |payload| {
                visitor_result = visitor(payload);
                Ok(())
            },
        ) {
            Ok(()) => {}
            Err(err) if recoverable_entry_data_error(&err) => continue,
            Err(err) => return Err(err.into()),
        }
        visitor_result?;
    }

    Ok(())
}

pub(super) fn enumerate_file_fields_indexed(file: &JournalFile<Mmap>) -> Result<Vec<String>> {
    let mut fields = HashSet::new();

    for field in file.fields() {
        let field = field?;
        if let Ok(name) = std::str::from_utf8(field.payload.as_ref()) {
            fields.insert(name.to_string());
        }
    }

    let mut out: Vec<_> = fields.into_iter().collect();
    out.sort();
    Ok(out)
}

pub(super) fn enumerate_file_fields_by_scan(reader: &mut FileReader) -> Result<Vec<String>> {
    let mut fields = HashSet::new();
    reader.seek_head();
    while reader.next()? {
        if let Ok(entry) = reader.get_entry() {
            fields.extend(entry.fields.into_keys());
        }
    }
    let mut out: Vec<_> = fields.into_iter().collect();
    out.sort();
    Ok(out)
}

pub(super) fn visit_file_unique_values_indexed<F>(
    file: &JournalFile<Mmap>,
    field_name: &[u8],
    decompressed: &mut Vec<u8>,
    mut visitor: F,
) -> Result<()>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    for data in file.field_data_objects(field_name)? {
        let data = data?;
        let payload = if data.is_compressed() {
            decompressed.clear();
            let len = data.decompress(decompressed)?;
            &decompressed[..len]
        } else {
            data.raw_payload()
        };
        let Some(value) = payload
            .strip_prefix(field_name)
            .and_then(|rest| rest.strip_prefix(b"="))
        else {
            return Err(SdkError::VerificationError(
                "field DATA chain object does not match requested field".to_string(),
            ));
        };
        visitor(value)?;
    }

    Ok(())
}

pub(super) fn collect_entry_metadata_and_data_offsets(
    file: &JournalFile<Mmap>,
    entry_offset: NonZeroU64,
    data_offsets: &mut Vec<NonZeroU64>,
) -> Result<(u64, u64, u64, [u8; 16])> {
    let entry = file.entry_ref(entry_offset)?;
    let metadata = (
        entry.header.seqnum,
        entry.header.realtime,
        entry.header.monotonic,
        entry.header.boot_id,
    );
    collect_offsets_from_entry_items(&entry.items, data_offsets);
    Ok(metadata)
}

pub(super) fn collect_entry_data_offsets(
    file: &JournalFile<Mmap>,
    entry_offset: NonZeroU64,
    data_offsets: &mut Vec<NonZeroU64>,
) -> Result<()> {
    let entry = file.entry_ref(entry_offset)?;
    collect_offsets_from_entry_items(&entry.items, data_offsets);
    Ok(())
}

pub(super) fn collect_offsets_from_entry_items(
    items: &EntryItemsType<&[u8]>,
    data_offsets: &mut Vec<NonZeroU64>,
) {
    data_offsets.clear();
    match items {
        EntryItemsType::Regular(items) => {
            data_offsets.reserve(items.len());
            data_offsets.extend(
                items
                    .iter()
                    .filter_map(|item| NonZeroU64::new(item.object_offset)),
            );
        }
        EntryItemsType::Compact(items) => {
            data_offsets.reserve(items.len());
            data_offsets.extend(
                items
                    .iter()
                    .filter_map(|item| NonZeroU64::new(item.object_offset as u64)),
            );
        }
    }
}

pub(super) fn verify_journal_file_strict(file: &JournalFile<Mmap>) -> Result<()> {
    let mut entry_offsets = Vec::new();
    file.entry_offsets(&mut entry_offsets)
        .map_err(|err| SdkError::VerificationError(format!("entry array walk failed: {err}")))?;

    let mut decompressed = Vec::new();
    let mut last_monotonic = 0_u64;
    let mut last_boot_id = [0_u8; 16];
    let mut monotonic_set = false;
    for entry_offset in entry_offsets {
        let entry = file.entry_ref(entry_offset).map_err(|err| {
            SdkError::VerificationError(format!(
                "entry object at offset {entry_offset} failed: {err}"
            ))
        })?;
        if monotonic_set
            && entry.header.boot_id == last_boot_id
            && last_monotonic > entry.header.monotonic
        {
            return Err(SdkError::VerificationError(format!(
                "entry monotonic out of sync ({} > {})",
                last_monotonic, entry.header.monotonic
            )));
        }
        last_monotonic = entry.header.monotonic;
        last_boot_id = entry.header.boot_id;
        monotonic_set = true;
        drop(entry);

        verify_entry_at_strict(file, entry_offset, &mut decompressed)?;
    }

    Ok(())
}

pub(super) fn verify_entry_at_strict(
    file: &JournalFile<Mmap>,
    entry_offset: NonZeroU64,
    decompressed: &mut Vec<u8>,
) -> Result<()> {
    file.entry_ref(entry_offset).map_err(|err| {
        SdkError::VerificationError(format!(
            "entry object at offset {entry_offset} failed: {err}"
        ))
    })?;

    let data_objects = file.entry_data_objects(entry_offset).map_err(|err| {
        SdkError::VerificationError(format!(
            "entry data list at offset {entry_offset} failed: {err}"
        ))
    })?;

    for data in data_objects {
        let data = data.map_err(|err| {
            SdkError::VerificationError(format!(
                "data object referenced by entry at offset {entry_offset} failed: {err}"
            ))
        })?;

        let flags = data.header.object_header.flags;
        let compression_flags = flags & 0x07;
        if flags & !0x07 != 0 || compression_flags.count_ones() > 1 {
            return Err(SdkError::VerificationError(format!(
                "data object referenced by entry at offset {entry_offset} has unsupported flags 0x{flags:02x}"
            )));
        }

        let payload = if data.is_compressed() {
            decompressed.clear();
            data.decompress(decompressed).map_err(|err| {
                SdkError::VerificationError(format!(
                    "compressed data object referenced by entry at offset {entry_offset} failed: {err}"
                ))
            })?;
            decompressed.as_slice()
        } else {
            data.raw_payload()
        };

        if !payload.contains(&b'=') {
            return Err(SdkError::VerificationError(format!(
                "data object referenced by entry at offset {entry_offset} is missing field separator"
            )));
        }
    }

    Ok(())
}

pub(super) fn recoverable_entry_error(err: &JournalError) -> bool {
    matches!(
        err,
        JournalError::InvalidObjectSize(0) | JournalError::ObjectExceedsFileBounds
    )
}

pub(super) fn recoverable_entry_data_error(err: &JournalError) -> bool {
    matches!(
        err,
        JournalError::InvalidOffset
            | JournalError::InvalidObjectSize(0)
            | JournalError::ObjectExceedsFileBounds
    )
}

pub(super) fn is_zst_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".zst"))
}

pub(super) fn decompress_zst_to_temp(path: &Path, prefix: &str) -> Result<PathBuf> {
    let source = File::open(path)?;
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(source)
        .map_err(|err| SdkError::DecompressionFailed(err.to_string()))?;
    let mut temp_file = tempfile::Builder::new()
        .prefix(prefix)
        .suffix(".journal")
        .tempfile()?;
    std::io::copy(&mut decoder, &mut temp_file)?;
    let (dest, temp_path) = temp_file.keep().map_err(|err| err.error)?;
    drop(dest);
    Ok(temp_path)
}
