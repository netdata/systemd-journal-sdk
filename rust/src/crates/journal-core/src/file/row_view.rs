use super::file::JournalFile;
use super::file_payload::DataPayloadReadContext;
use super::mmap::MemoryMap;
use super::object::EntryItemsType;
use crate::error::{JournalError, Result};
use std::num::NonZeroU64;

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct CurrentRowMetadata {
    pub seqnum_id: [u8; 16],
    pub seqnum: u64,
    pub boot_id: [u8; 16],
    pub monotonic: u64,
    pub realtime: u64,
    pub xor_hash: u64,
}

#[doc(hidden)]
#[derive(Default)]
pub struct CurrentRowView {
    entry_offset: Option<NonZeroU64>,
    metadata: Option<CurrentRowMetadata>,
    data_offsets: Vec<NonZeroU64>,
    payload_context: Option<DataPayloadReadContext>,
    data_index: usize,
    data_state_active: bool,
    decompressed: Vec<u8>,
    row_arena: Vec<u8>,
    row_pins_active: bool,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub enum CurrentRowPayload {
    Borrowed { ptr: *const u8, len: usize },
    Arena { start: usize, end: usize },
}

impl CurrentRowView {
    pub fn entry_offset(&self) -> Option<NonZeroU64> {
        self.entry_offset
    }

    pub fn metadata(&self) -> Option<CurrentRowMetadata> {
        self.metadata
    }

    pub fn decompressed_mut(&mut self) -> &mut Vec<u8> {
        &mut self.decompressed
    }

    pub fn data_offset_count(&self) -> usize {
        self.data_offsets.len()
    }

    pub fn data_offset_at(&self, index: usize) -> Option<NonZeroU64> {
        self.data_offsets.get(index).copied()
    }

    pub fn data_state_active(&self) -> bool {
        self.data_state_active
    }

    pub fn row_pins_active(&self) -> bool {
        self.row_pins_active
    }

    pub fn clear_pins<M: MemoryMap>(&mut self, file: &JournalFile<M>) -> Result<()> {
        if !self.row_pins_active {
            return Ok(());
        }
        file.clear_row_payload_pins()?;
        self.row_pins_active = false;
        Ok(())
    }

    pub fn clear_pins_best_effort<M: MemoryMap>(&mut self, file: &JournalFile<M>) {
        let _ = self.clear_pins(file);
        debug_assert!(
            !self.row_pins_active,
            "row pins must be cleared before resetting or advancing row state"
        );
    }

    pub fn clear_current<M: MemoryMap>(&mut self, file: &JournalFile<M>) -> Result<()> {
        self.clear_pins(file)?;
        self.entry_offset = None;
        self.metadata = None;
        self.data_offsets.clear();
        self.payload_context = None;
        self.data_index = 0;
        self.data_state_active = false;
        self.reset_payload_storage();
        Ok(())
    }

    pub fn clear_current_best_effort<M: MemoryMap>(&mut self, file: &JournalFile<M>) {
        self.clear_pins_best_effort(file);
        self.entry_offset = None;
        self.metadata = None;
        self.data_offsets.clear();
        self.payload_context = None;
        self.data_index = 0;
        self.data_state_active = false;
        self.reset_payload_storage();
    }

    pub fn load_entry<M: MemoryMap>(
        &mut self,
        file: &JournalFile<M>,
        entry_offset: NonZeroU64,
    ) -> Result<CurrentRowMetadata> {
        self.clear_pins(file)?;
        self.reset_payload_storage();
        self.data_offsets.clear();

        let entry = file.entry_ref(entry_offset)?;
        collect_nonzero_entry_offsets(&entry.items, &mut self.data_offsets);
        let header = file.journal_header_ref();
        let metadata = CurrentRowMetadata {
            seqnum_id: header.seqnum_id,
            seqnum: entry.header.seqnum,
            boot_id: entry.header.boot_id,
            monotonic: entry.header.monotonic,
            realtime: entry.header.realtime,
            xor_hash: entry.header.xor_hash,
        };

        self.entry_offset = Some(entry_offset);
        self.metadata = Some(metadata);
        self.payload_context = Some(file.data_payload_read_context());
        self.data_index = 0;
        self.data_state_active = false;
        Ok(metadata)
    }

    pub fn restart_data(&mut self) -> Result<()> {
        if self.entry_offset.is_none() {
            return Err(JournalError::UnsetCursor);
        }
        self.data_index = 0;
        self.data_state_active = true;
        // Keep decompressed capacity as reusable row scratch; only returned
        // compressed payloads belong to the row arena and must be invalidated.
        self.row_arena.clear();
        Ok(())
    }

    #[inline(always)]
    pub fn read_next_payload<M: MemoryMap>(
        &mut self,
        file: &JournalFile<M>,
    ) -> Result<Option<CurrentRowPayload>> {
        let Some(data_offset) = self.data_offsets.get(self.data_index).copied() else {
            self.data_state_active = true;
            return Ok(None);
        };
        self.data_index += 1;
        self.data_state_active = true;

        let context = self.payload_context.ok_or(JournalError::UnsetCursor)?;
        if let Some((ptr, len)) =
            file.raw_data_payload_ptr_row_pinned_if_uncompressed(context, data_offset)?
        {
            self.row_pins_active = true;
            return Ok(Some(CurrentRowPayload::Borrowed { ptr, len }));
        }

        let data = file.data_ref(data_offset)?;
        self.decompressed.clear();
        let len = data.decompress(&mut self.decompressed)?;
        debug_assert_eq!(
            self.decompressed.len(),
            len,
            "decompressors must set the output buffer length before returning"
        );
        let start = self.row_arena.len();
        self.row_arena.extend_from_slice(&self.decompressed[..len]);
        let end = self.row_arena.len();
        Ok(Some(CurrentRowPayload::Arena { start, end }))
    }

    #[inline(always)]
    pub fn read_next_payload_with_offset<M: MemoryMap>(
        &mut self,
        file: &JournalFile<M>,
    ) -> Result<Option<(NonZeroU64, CurrentRowPayload)>> {
        let Some(data_offset) = self.next_data_offset() else {
            return Ok(None);
        };

        let payload = self.read_payload_at(file, data_offset)?;
        Ok(Some((data_offset, payload)))
    }

    #[inline(always)]
    fn next_data_offset(&mut self) -> Option<NonZeroU64> {
        self.data_state_active = true;
        let data_offset = self.data_offsets.get(self.data_index).copied()?;
        self.data_index += 1;
        Some(data_offset)
    }

    #[inline(always)]
    pub fn read_payload_at<M: MemoryMap>(
        &mut self,
        file: &JournalFile<M>,
        data_offset: NonZeroU64,
    ) -> Result<CurrentRowPayload> {
        let context = self.payload_context.ok_or(JournalError::UnsetCursor)?;
        if let Some((ptr, len)) =
            file.raw_data_payload_ptr_row_pinned_if_uncompressed(context, data_offset)?
        {
            self.row_pins_active = true;
            return Ok(CurrentRowPayload::Borrowed { ptr, len });
        }

        let data = file.data_ref(data_offset)?;
        self.decompressed.clear();
        let len = data.decompress(&mut self.decompressed)?;
        debug_assert_eq!(
            self.decompressed.len(),
            len,
            "decompressors must set the output buffer length before returning"
        );
        let start = self.row_arena.len();
        self.row_arena.extend_from_slice(&self.decompressed[..len]);
        let end = self.row_arena.len();
        Ok(CurrentRowPayload::Arena { start, end })
    }

    #[inline(always)]
    pub fn payload_slice(&self, payload: CurrentRowPayload) -> &[u8] {
        match payload {
            CurrentRowPayload::Borrowed { ptr, len } => {
                // SAFETY: CurrentRowView creates borrowed row payloads only through
                // JournalFile's row-pinned mmap path. Row pins are cleared before
                // advancing, seeking, or explicitly resetting row data state.
                // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
                unsafe { std::slice::from_raw_parts(ptr, len) }
            }
            CurrentRowPayload::Arena { start, end } => &self.row_arena[start..end],
        }
    }

    pub fn reset_data_state<M: MemoryMap>(&mut self, file: &JournalFile<M>) -> Result<()> {
        self.clear_pins(file)?;
        self.data_index = 0;
        self.data_state_active = false;
        self.reset_payload_storage();
        Ok(())
    }

    pub fn reset_data_state_best_effort<M: MemoryMap>(&mut self, file: &JournalFile<M>) {
        self.clear_pins_best_effort(file);
        self.data_index = 0;
        self.data_state_active = false;
        self.reset_payload_storage();
    }

    fn reset_payload_storage(&mut self) {
        debug_assert!(
            !self.row_pins_active,
            "row payload storage reset requires row pins to be cleared first"
        );
        self.row_arena.clear();
    }
}

fn collect_nonzero_entry_offsets<B: zerocopy::ByteSlice>(
    items: &EntryItemsType<B>,
    offsets: &mut Vec<NonZeroU64>,
) {
    offsets.clear();
    match items {
        EntryItemsType::Regular(items) => {
            offsets.reserve(items.len());
            offsets.extend(
                items
                    .iter()
                    .filter_map(|item| NonZeroU64::new(item.object_offset)),
            );
        }
        EntryItemsType::Compact(items) => {
            offsets.reserve(items.len());
            offsets.extend(
                items
                    .iter()
                    .filter_map(|item| NonZeroU64::new(item.object_offset as u64)),
            );
        }
    }
}
