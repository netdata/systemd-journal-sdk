//! Fixed-header inspection for excluded-writer ownership inventories.
use super::mmap::read_file_exact_at;
use super::{HeaderIncompatibleFlags, JournalHeader, JournalState};
use crate::error::{JournalError, Result};
use std::path::Path;
use zerocopy::FromBytes;

/// Reads and validates writer-compatible header metadata, without mapping hash
/// tables or visiting objects. This does not verify indexes or payloads.
#[doc(hidden)]
pub fn read_retention_header(path: &Path) -> Result<(JournalHeader, u64)> {
    let file = std::fs::File::open(path)?;
    let bytes = file.metadata()?.len();
    let mut buffer = [0u8; std::mem::size_of::<JournalHeader>()];
    read_file_exact_at(&file, 0, &mut buffer)?;
    let header = JournalHeader::read_from_prefix(&buffer).unwrap().0;
    validate_writer_compatibility(&header)?;
    header.validated_arena_end(bytes)?;
    header.validate_empty_entry_metadata()?;
    JournalState::try_from(header.state)?;
    validate_retention_metadata(&header)?;
    Ok((header, bytes))
}

fn validate_writer_compatibility(header: &JournalHeader) -> Result<()> {
    if header.signature != *b"LPKSHHRH" {
        return Err(JournalError::InvalidMagicNumber);
    }
    if header.header_size < std::mem::size_of::<JournalHeader>() as u64
        || header.incompatible_flags & !0x1f != 0
        || !header.has_incompatible_flag(HeaderIncompatibleFlags::KeyedHash)
    {
        return Err(JournalError::UnsupportedJournalFile);
    }
    Ok(())
}

fn validate_retention_metadata(header: &JournalHeader) -> Result<()> {
    if header.file_id == [0; 16]
        || header.seqnum_id == [0; 16]
        || header.data_hash_table_offset.is_none()
        || header.field_hash_table_offset.is_none()
        || header.tail_object_offset.is_none()
        || (header.n_entries > 0
            && (header.entry_array_offset.is_none()
                || header.n_entries > header.n_objects
                || header.head_entry_seqnum == 0
                || header.tail_entry_seqnum < header.head_entry_seqnum
                || header.tail_entry_seqnum - header.head_entry_seqnum < header.n_entries - 1
                || header.tail_entry_realtime < header.head_entry_realtime))
    {
        return Err(JournalError::InvalidObjectLocation);
    }
    Ok(())
}
