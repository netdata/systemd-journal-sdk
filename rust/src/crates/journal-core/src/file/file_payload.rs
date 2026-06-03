use super::file::{JournalFile, PayloadParts, validate_offset_alignment};
use super::mmap::{MemoryMap, WindowManager};
use super::object::*;
use crate::error::{JournalError, Result};
use crate::file::value_guard::ValueGuard;
use std::num::NonZeroU64;
use zerocopy::FromBytes;

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct DataPayloadReadContext {
    is_compact: bool,
    header_size: u64,
    arena_end: u64,
    payload_prefix_size: u64,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct DataPayloadObjectInfo {
    size_needed: u64,
    is_compressed: bool,
}

#[derive(Debug, Clone, Copy)]
struct DataLookupResult {
    next_hash_offset: Option<NonZeroU64>,
    matches: bool,
}

#[derive(Debug, Clone, Copy)]
struct DataLookupHeader {
    flags: u8,
    size_needed: u64,
    stored_hash: u64,
    next_hash_offset: Option<NonZeroU64>,
}

impl DataLookupHeader {
    fn is_compressed(self) -> bool {
        (self.flags
            & (ObjectFlags::CompressedZstd as u8
                | ObjectFlags::CompressedLz4 as u8
                | ObjectFlags::CompressedXz as u8))
            != 0
    }
}

impl DataPayloadObjectInfo {
    pub fn is_compressed(self) -> bool {
        self.is_compressed
    }
}

fn parse_data_payload_object_header(header_slice: &[u8]) -> Result<DataPayloadObjectInfo> {
    let object_header =
        ObjectHeader::ref_from_bytes(header_slice).map_err(|_| JournalError::ZerocopyFailure)?;

    if object_header.type_ != ObjectType::Data as u8 {
        return Err(JournalError::InvalidObjectType);
    }

    Ok(DataPayloadObjectInfo {
        size_needed: object_header.validated_size()?,
        is_compressed: object_header.is_compressed(),
    })
}

impl<M: MemoryMap> JournalFile<M> {
    #[doc(hidden)]
    pub fn data_payload_read_context(&self) -> DataPayloadReadContext {
        let journal_header = self.journal_header_ref();
        let is_compact = journal_header.has_incompatible_flag(HeaderIncompatibleFlags::Compact);
        let payload_prefix_size = std::mem::size_of::<DataObjectHeader>() as u64
            + if is_compact {
                std::mem::size_of::<CompactDataFields>() as u64
            } else {
                0
            };
        DataPayloadReadContext {
            is_compact,
            header_size: journal_header.header_size,
            arena_end: journal_header.header_size + journal_header.arena_size,
            payload_prefix_size,
        }
    }

    #[doc(hidden)]
    pub fn visit_data_payload_at<F>(
        &self,
        offset: NonZeroU64,
        decompressed: &mut Vec<u8>,
        visitor: F,
    ) -> Result<()>
    where
        F: FnOnce(&[u8]) -> Result<()>,
    {
        let context = self.data_payload_read_context();
        self.visit_data_payload_at_with_context(context, offset, decompressed, visitor)
    }

    #[doc(hidden)]
    pub fn visit_data_payload_at_with_context<F>(
        &self,
        context: DataPayloadReadContext,
        offset: NonZeroU64,
        decompressed: &mut Vec<u8>,
        visitor: F,
    ) -> Result<()>
    where
        F: FnOnce(&[u8]) -> Result<()>,
    {
        Self::validate_data_payload_offset(context, offset)?;
        self.window_manager.with_mut(|wm| {
            let info = Self::data_payload_info_from_window(wm, context, offset)?;
            let data = Self::data_slice_from_window(wm, offset, info.size_needed)?;
            if !info.is_compressed {
                return visitor(&data[context.payload_prefix_size as usize..]);
            }
            let object = DataObject::from_data(data, context.is_compact)
                .ok_or(JournalError::ZerocopyFailure)?;
            decompressed.clear();
            let len = object.decompress(decompressed)?;
            visitor(&decompressed[..len])
        })
    }

    #[doc(hidden)]
    pub fn data_payload_object_info_at(
        &self,
        context: DataPayloadReadContext,
        offset: NonZeroU64,
    ) -> Result<DataPayloadObjectInfo> {
        validate_offset_alignment(offset)?;
        if offset.get() < context.header_size {
            return Err(JournalError::ObjectExceedsFileBounds);
        }

        self.window_manager
            .with_mut(|wm| Self::data_payload_info_from_window(wm, context, offset))
    }

    fn validate_data_payload_offset(
        context: DataPayloadReadContext,
        offset: NonZeroU64,
    ) -> Result<()> {
        validate_offset_alignment(offset)?;
        if offset.get() < context.header_size {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        Ok(())
    }

    fn data_payload_info_from_window(
        wm: &mut WindowManager<M>,
        context: DataPayloadReadContext,
        offset: NonZeroU64,
    ) -> Result<DataPayloadObjectInfo> {
        let object_header_size = std::mem::size_of::<ObjectHeader>() as u64;
        let header_slice = wm.get_slice(offset.get(), object_header_size)?;
        let info = parse_data_payload_object_header(header_slice)?;
        Self::validate_data_payload_info(context, offset, info)?;
        Ok(info)
    }

    fn validate_data_payload_info(
        context: DataPayloadReadContext,
        offset: NonZeroU64,
        info: DataPayloadObjectInfo,
    ) -> Result<()> {
        let end_offset = offset
            .get()
            .checked_add(info.size_needed)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        if end_offset > context.arena_end {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        if info.size_needed < context.payload_prefix_size {
            return Err(JournalError::InvalidObjectSize(info.size_needed));
        }
        Ok(())
    }

    fn data_slice_from_window<'w>(
        wm: &'w mut WindowManager<M>,
        offset: NonZeroU64,
        size_needed: u64,
    ) -> Result<&'w [u8]> {
        if wm.active_window_contains(offset.get(), size_needed) {
            return Ok(wm.active_slice(offset.get(), size_needed));
        }
        wm.get_slice(offset.get(), size_needed)
    }

    #[doc(hidden)]
    pub fn raw_data_payload_ref_with_info(
        &self,
        context: DataPayloadReadContext,
        offset: NonZeroU64,
        info: DataPayloadObjectInfo,
    ) -> Result<ValueGuard<'_, &[u8]>> {
        validate_offset_alignment(offset)?;
        if offset.get() < context.header_size {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        if info.is_compressed {
            return Err(JournalError::InvalidObjectType);
        }
        if info.size_needed < context.payload_prefix_size {
            return Err(JournalError::InvalidObjectSize(info.size_needed));
        }

        self.window_manager.with_guarded(offset, |wm| {
            if wm.active_window_contains(offset.get(), info.size_needed) {
                let data = wm.active_slice(offset.get(), info.size_needed);
                return Ok(&data[context.payload_prefix_size as usize..]);
            }
            let data = wm.get_slice(offset.get(), info.size_needed)?;
            Ok(&data[context.payload_prefix_size as usize..])
        })
    }

    #[doc(hidden)]
    /// Returns an unguarded pointer to an uncompressed DATA payload.
    ///
    /// The caller must only expose the pointer while it can prove the backing
    /// mmap window will not be remapped or evicted. This is intended for
    /// whole-file mmap row-scoped facade enumeration. Do not call this for
    /// windowed mmap; use `raw_data_payload_ref_with_info()` or copy the
    /// payload instead.
    pub fn raw_data_payload_ptr_with_info_unguarded(
        &self,
        context: DataPayloadReadContext,
        offset: NonZeroU64,
        info: DataPayloadObjectInfo,
    ) -> Result<(*const u8, usize)> {
        validate_offset_alignment(offset)?;
        if offset.get() < context.header_size {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        if info.is_compressed {
            return Err(JournalError::InvalidObjectType);
        }
        if info.size_needed < context.payload_prefix_size {
            return Err(JournalError::InvalidObjectSize(info.size_needed));
        }

        self.window_manager.with_mut(|wm| {
            let data =
                if let Some(data) = wm.active_slice_if_contains(offset.get(), info.size_needed) {
                    data
                } else {
                    wm.get_slice(offset.get(), info.size_needed)?
                };
            let payload = &data[context.payload_prefix_size as usize..];
            Ok((payload.as_ptr(), payload.len()))
        })
    }

    pub fn find_data_offset(&self, hash: u64, payload: &[u8]) -> Result<Option<NonZeroU64>> {
        self.find_data_offset_parts(hash, PayloadParts::raw(payload))
    }

    pub fn find_data_offset_parts(
        &self,
        hash: u64,
        payload: PayloadParts<'_>,
    ) -> Result<Option<NonZeroU64>> {
        let hash_table = self
            .data_hash_table_ref()
            .ok_or(JournalError::MissingHashTable)?;
        let context = self.data_payload_read_context();
        let mut decompression_buffer = Vec::new();
        let mut object_offset = hash_table.hash_item_ref(hash).head_hash_offset;

        while let Some(offset) = object_offset {
            let result = self.data_lookup_result_at(
                context,
                offset,
                hash,
                payload,
                &mut decompression_buffer,
            )?;
            if result.matches {
                return Ok(Some(offset));
            }
            object_offset = result.next_hash_offset;
        }

        Ok(None)
    }

    fn data_lookup_result_at(
        &self,
        context: DataPayloadReadContext,
        offset: NonZeroU64,
        hash: u64,
        payload: PayloadParts<'_>,
        decompression_buffer: &mut Vec<u8>,
    ) -> Result<DataLookupResult> {
        Self::validate_data_payload_offset(context, offset)?;
        self.window_manager.with_mut(|wm| {
            let lookup = Self::data_lookup_header_from_window(wm, context, offset)?;
            if lookup.stored_hash != hash {
                return Ok(DataLookupResult {
                    next_hash_offset: lookup.next_hash_offset,
                    matches: false,
                });
            }

            let data = Self::data_slice_from_window(wm, offset, lookup.size_needed)?;
            let matches = Self::data_lookup_payload_matches(
                context,
                lookup,
                data,
                payload,
                decompression_buffer,
            )?;
            Ok(DataLookupResult {
                next_hash_offset: lookup.next_hash_offset,
                matches,
            })
        })
    }

    fn data_lookup_header_from_window(
        wm: &mut WindowManager<M>,
        context: DataPayloadReadContext,
        offset: NonZeroU64,
    ) -> Result<DataLookupHeader> {
        let header_slice =
            wm.get_slice(offset.get(), std::mem::size_of::<DataObjectHeader>() as u64)?;
        Self::parse_data_lookup_header(context, offset, header_slice)
    }

    fn parse_data_lookup_header(
        context: DataPayloadReadContext,
        offset: NonZeroU64,
        header_slice: &[u8],
    ) -> Result<DataLookupHeader> {
        if header_slice[0] != ObjectType::Data as u8 {
            return Err(JournalError::InvalidObjectType);
        }
        let size_needed = u64::from_le_bytes(header_slice[8..16].try_into().unwrap());
        if size_needed < std::mem::size_of::<DataObjectHeader>() as u64 {
            return Err(JournalError::InvalidObjectSize(size_needed));
        }
        let info = DataPayloadObjectInfo {
            size_needed,
            is_compressed: false,
        };
        Self::validate_data_payload_info(context, offset, info)?;
        Ok(DataLookupHeader {
            flags: header_slice[1],
            size_needed,
            stored_hash: u64::from_le_bytes(header_slice[16..24].try_into().unwrap()),
            next_hash_offset: NonZeroU64::new(u64::from_le_bytes(
                header_slice[24..32].try_into().unwrap(),
            )),
        })
    }

    fn data_lookup_payload_matches(
        context: DataPayloadReadContext,
        lookup: DataLookupHeader,
        data: &[u8],
        payload: PayloadParts<'_>,
        decompression_buffer: &mut Vec<u8>,
    ) -> Result<bool> {
        if lookup.is_compressed() {
            let object = DataObject::from_data(data, context.is_compact)
                .ok_or(JournalError::ZerocopyFailure)?;
            decompression_buffer.clear();
            let len = object.decompress(decompression_buffer)?;
            return Ok(payload.equals_slice(&decompression_buffer[..len]));
        }
        let payload_start = context.payload_prefix_size as usize;
        Ok(payload.equals_slice(&data[payload_start..]))
    }
}
