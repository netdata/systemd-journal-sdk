use super::array_chain::reject_array_revisit;
use super::file::{JournalFile, validate_offset_alignment};
use super::mmap::MemoryMap;
use super::object::*;
use super::value_guard::ValueGuard;
use crate::error::{JournalError, Result};
use std::num::NonZeroU64;
use zerocopy::FromBytes;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum RecoveryReason {
    #[default]
    None,
    MissingEntry,
    MissingEntryArray,
}

/// Observed tail loss, not a certificate of journal integrity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct RecoveryStatus {
    pub reason: RecoveryReason,
    pub first_lost_offset: u64,
    pub declared_entries: u64,
    pub recovered_entries: u64,
}

impl RecoveryStatus {
    pub fn incomplete(self) -> bool {
        self.reason != RecoveryReason::None
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct RecoveredPrefix {
    pub status: RecoveryStatus,
    pub last_entry: Option<NonZeroU64>,
    tail_seqnum: u64,
    tail_realtime: u64,
    tail_monotonic: u64,
    tail_boot_id: [u8; 16],
}

#[derive(Default)]
pub(super) struct ReaderRecoveryCell(
    std::cell::OnceCell<Box<std::cell::Cell<Option<RecoveredPrefix>>>>,
);

impl ReaderRecoveryCell {
    #[inline]
    pub(super) fn get(&self) -> Option<RecoveredPrefix> {
        self.0.get().and_then(|state| state.get())
    }

    pub(super) fn set(&self, value: Option<RecoveredPrefix>) {
        if let Some(state) = self.0.get() {
            state.set(value);
        } else if let Some(prefix) = value {
            self.initialize(prefix);
        }
    }

    #[cold]
    #[inline(never)]
    fn initialize(&self, prefix: RecoveredPrefix) {
        self.0
            .get_or_init(|| Box::new(std::cell::Cell::new(None)))
            .set(Some(prefix));
    }
}

struct ArrayPrefix {
    next: Option<NonZeroU64>,
    available: usize,
    capacity: usize,
    partial: bool,
}

impl<M: MemoryMap> JournalFile<M> {
    fn make_reader_prefix(
        &self,
        reason: RecoveryReason,
        lost: u64,
        count: u64,
        last: Option<NonZeroU64>,
    ) -> Result<RecoveredPrefix> {
        let mut result = RecoveredPrefix {
            status: RecoveryStatus {
                reason,
                first_lost_offset: lost,
                declared_entries: self.journal_header_ref().n_entries,
                recovered_entries: count,
            },
            last_entry: last,
            tail_seqnum: 0,
            tail_realtime: 0,
            tail_monotonic: 0,
            tail_boot_id: [0; 16],
        };
        if let Some(last) = last {
            let mut bytes = [0u8; 64];
            self.read_fresh_bytes_at(last.get(), &mut bytes)?;
            let entry = EntryObjectHeader::ref_from_bytes(&bytes)
                .map_err(|_| JournalError::ZerocopyFailure)?;
            result.tail_seqnum = entry.seqnum;
            result.tail_realtime = entry.realtime;
            result.tail_monotonic = entry.monotonic;
            result.tail_boot_id = entry.boot_id;
        }
        Ok(result)
    }

    #[doc(hidden)]
    pub fn recovered_tail_metadata(&self) -> Option<(u64, u64, u64, [u8; 16])> {
        self.recovery.get().map(|p| {
            (
                p.tail_seqnum,
                p.tail_realtime,
                p.tail_monotonic,
                p.tail_boot_id,
            )
        })
    }
    #[doc(hidden)]
    pub fn set_reader_recovery(&mut self, enabled: bool) {
        self.recovery_enabled = enabled;
        if !enabled {
            self.recovery.set(None);
        }
    }

    pub fn recovery_status(&self) -> RecoveryStatus {
        self.recovery
            .get()
            .map_or_else(RecoveryStatus::default, |prefix| prefix.status)
    }

    #[doc(hidden)]
    pub fn reader_entry_count(&self) -> u64 {
        self.recovery.get().map_or_else(
            || self.journal_header_ref().n_entries,
            |p| p.status.recovered_entries,
        )
    }

    #[doc(hidden)]
    pub fn reader_recovery_active(&self) -> bool {
        self.recovery.get().is_some()
    }

    #[doc(hidden)]
    pub fn take_reader_recovery_change(&self) -> bool {
        self.recovery_changed.replace(false)
    }

    fn recovery_array_prefix(
        &self,
        offset: NonZeroU64,
        extent: u64,
    ) -> Result<Option<ArrayPrefix>> {
        validate_offset_alignment(offset)?;
        let header_size = self.journal_header_ref().header_size;
        if offset.get() < header_size {
            return Err(JournalError::InvalidObjectLocation);
        }
        let arena_end = header_size
            .checked_add(self.journal_header_ref().arena_size)
            .ok_or(JournalError::InvalidOffset)?;
        if offset
            .get()
            .checked_add(24)
            .is_none_or(|end| end > arena_end)
        {
            return Err(JournalError::InvalidObjectLocation);
        }
        let mut bytes = [0u8; 24];
        if offset
            .get()
            .checked_add(bytes.len() as u64)
            .is_none_or(|end| end > extent)
        {
            return Ok(None);
        }
        self.read_fresh_bytes_at(offset.get(), &mut bytes)?;
        let header = OffsetArrayObjectHeader::ref_from_bytes(&bytes)
            .map_err(|_| JournalError::ZerocopyFailure)?;
        if bytes.iter().all(|byte| *byte == 0) {
            return Ok(None);
        }
        if header.object_header.type_ != ObjectType::EntryArray as u8 {
            return Err(JournalError::InvalidObjectType);
        }
        let size = header.object_header.validated_size()?;
        let item_size = if self
            .journal_header_ref()
            .has_incompatible_flag(HeaderIncompatibleFlags::Compact)
        {
            4
        } else {
            8
        };
        if size <= 24 || (size - 24) % item_size != 0 {
            return Err(JournalError::InvalidObjectSize(size));
        }
        let end = offset
            .get()
            .checked_add(size)
            .ok_or(JournalError::InvalidOffset)?;
        let arena_end = self
            .journal_header_ref()
            .header_size
            .checked_add(self.journal_header_ref().arena_size)
            .ok_or(JournalError::InvalidOffset)?;
        if end > arena_end {
            return Err(JournalError::InvalidObjectLocation);
        }
        let capacity =
            usize::try_from((size - 24) / item_size).map_err(|_| JournalError::InvalidOffset)?;
        let available = usize::try_from((extent - offset.get() - 24) / item_size)
            .unwrap_or(usize::MAX)
            .min(capacity);
        Ok(Some(ArrayPrefix {
            next: header.next_offset_array,
            capacity,
            available,
            partial: end > extent,
        }))
    }

    fn recovery_array_slot(&self, array: NonZeroU64, index: usize) -> Result<Option<NonZeroU64>> {
        let compact = self
            .journal_header_ref()
            .has_incompatible_flag(HeaderIncompatibleFlags::Compact);
        let item_size = if compact { 4 } else { 8 };
        let offset = (index as u64)
            .checked_mul(item_size)
            .and_then(|v| array.get().checked_add(24)?.checked_add(v))
            .ok_or(JournalError::InvalidOffset)?;
        let mut bytes = [0u8; 8];
        self.read_fresh_bytes_at(offset, &mut bytes[..item_size as usize])?;
        Ok(NonZeroU64::new(if compact {
            u32::from_le_bytes(bytes[..4].try_into().unwrap()) as u64
        } else {
            u64::from_le_bytes(bytes)
        }))
    }

    fn recovery_entry_present(&self, offset: NonZeroU64, extent: u64) -> Result<bool> {
        validate_offset_alignment(offset)?;
        let header = self.journal_header_ref();
        if offset.get() < header.header_size {
            return Err(JournalError::InvalidObjectLocation);
        }
        let arena_end = header
            .header_size
            .checked_add(header.arena_size)
            .ok_or(JournalError::InvalidOffset)?;
        if offset
            .get()
            .checked_add(16)
            .is_none_or(|end| end > arena_end)
        {
            return Err(JournalError::InvalidObjectLocation);
        }
        if offset.get().checked_add(16).is_none_or(|end| end > extent) {
            return Ok(false);
        }
        let mut bytes = [0u8; 16];
        self.read_fresh_bytes_at(offset.get(), &mut bytes)?;
        let object =
            ObjectHeader::ref_from_bytes(&bytes).map_err(|_| JournalError::ZerocopyFailure)?;
        if object.type_ == 0 && object.size == 0 {
            if bytes.iter().any(|byte| *byte != 0) {
                return Err(JournalError::InvalidObjectType);
            }
            return Ok(false);
        }
        if object.type_ != ObjectType::Entry as u8 {
            return Err(JournalError::InvalidObjectType);
        }
        let size = object.validated_size()?;
        let item_size = if header.has_incompatible_flag(HeaderIncompatibleFlags::Compact) {
            4
        } else {
            16
        };
        if size < 64 || (size - 64) % item_size != 0 {
            return Err(JournalError::InvalidObjectSize(size));
        }
        let end = offset
            .get()
            .checked_add(size)
            .ok_or(JournalError::InvalidOffset)?;
        if end
            > header
                .header_size
                .checked_add(header.arena_size)
                .ok_or(JournalError::InvalidOffset)?
        {
            return Err(JournalError::InvalidObjectLocation);
        }
        Ok(end <= extent)
    }

    fn discover_reader_prefix(&self) -> Result<Option<RecoveredPrefix>> {
        let extent = self.window_manager.borrow_mut_checked()?.reader_extent()?;
        let header = self.journal_header_ref();
        let mut remaining =
            usize::try_from(header.n_entries).map_err(|_| JournalError::InvalidOffset)?;
        let mut offset = header.entry_array_offset;
        let mut count = 0u64;
        let mut last_entry = None;
        let mut loss = None;
        let head = offset;
        let mut highest = offset;
        let compact = header.has_incompatible_flag(HeaderIncompatibleFlags::Compact);
        let item_size = if compact { 4 } else { 8 };
        let mut slots = [0u8; 4096];
        while remaining > 0 {
            let Some(array_offset) = offset else {
                let (reason, lost) = loss.unwrap_or((RecoveryReason::MissingEntryArray, 0));
                return self
                    .make_reader_prefix(reason, lost, count, last_entry)
                    .map(Some);
            };
            let Some(array) = self.recovery_array_prefix(array_offset, extent)? else {
                let (reason, lost) =
                    loss.unwrap_or((RecoveryReason::MissingEntryArray, array_offset.get()));
                return self
                    .make_reader_prefix(reason, lost, count, last_entry)
                    .map(Some);
            };
            let wanted = remaining.min(array.capacity);
            let available = wanted.min(array.available);
            for start in (0..available).step_by(slots.len() / item_size) {
                let count_slots = (available - start).min(slots.len() / item_size);
                let bytes = &mut slots[..count_slots * item_size];
                self.read_fresh_bytes_at(
                    array_offset.get() + 24 + (start * item_size) as u64,
                    bytes,
                )?;
                for slot in bytes.chunks_exact(item_size) {
                    let entry = NonZeroU64::new(if compact {
                        u32::from_le_bytes(slot.try_into().unwrap()) as u64
                    } else {
                        u64::from_le_bytes(slot.try_into().unwrap())
                    });
                    let present = match entry {
                        Some(entry) => self.recovery_entry_present(entry, extent)?,
                        None => false,
                    };
                    if present {
                        if loss.is_some() {
                            return Err(JournalError::InvalidObjectLocation);
                        }
                        if last_entry.is_some_and(|previous| entry <= Some(previous)) {
                            return Err(JournalError::InvalidObjectLocation);
                        }
                        count += 1;
                        last_entry = entry;
                    } else {
                        loss.get_or_insert((
                            RecoveryReason::MissingEntry,
                            entry.map_or(0, NonZeroU64::get),
                        ));
                    }
                }
            }
            if available < wanted || array.partial {
                loss.get_or_insert((RecoveryReason::MissingEntryArray, array_offset.get()));
            }
            remaining -= wanted;
            if remaining > 0 {
                if let Some(next) = array.next {
                    if Some(next) <= highest {
                        reject_array_revisit(head.unwrap(), array_offset, next, |at| {
                            self.recovery_array_prefix(at, extent)?
                                .map(|array| array.next)
                                .ok_or(JournalError::ObjectExceedsFileBounds)
                        })?;
                    } else {
                        highest = Some(next);
                    }
                }
            }
            offset = array.next;
        }
        loss.map(|(reason, lost)| self.make_reader_prefix(reason, lost, count, last_entry))
            .transpose()
    }

    #[doc(hidden)]
    pub fn recover_reader_tail(&self, err: &JournalError) -> Result<bool> {
        if !self.recovery_enabled
            || !matches!(
                err,
                JournalError::ObjectExceedsFileBounds
                    | JournalError::InvalidObjectSize(0)
                    | JournalError::InvalidOffset
                    | JournalError::Io(_)
            )
        {
            return Ok(false);
        }
        if let JournalError::Io(err) = err {
            if err.kind() != std::io::ErrorKind::UnexpectedEof {
                return Ok(false);
            }
        }
        let next = self.discover_reader_prefix()?;
        let changed = next != self.recovery.get();
        self.recovery.set(next);
        if changed {
            self.recovery_changed.set(true);
        }
        Ok(changed)
    }

    #[doc(hidden)]
    pub fn refresh_reader_recovery(&self) -> Result<bool> {
        if !self.recovery_enabled || self.recovery.get().is_none() {
            return Ok(false);
        }
        let next = self.discover_reader_prefix()?;
        let changed = next != self.recovery.get();
        self.recovery.set(next);
        if changed {
            self.recovery_changed.set(true);
        }
        Ok(changed)
    }

    #[inline]
    pub(super) fn reader_offset_array_ref(
        &self,
        offset: NonZeroU64,
    ) -> Result<ValueGuard<'_, OffsetArrayObject<&[u8]>>> {
        if self.recovery.get().is_none() {
            return self.offset_array_ref(offset);
        }
        self.recovered_array_ref(offset)
    }

    #[cold]
    #[inline(never)]
    fn recovered_array_ref(
        &self,
        offset: NonZeroU64,
    ) -> Result<ValueGuard<'_, OffsetArrayObject<&[u8]>>> {
        validate_offset_alignment(offset)?;
        let header = self.journal_header_ref();
        let arena_end = header
            .header_size
            .checked_add(header.arena_size)
            .ok_or(JournalError::InvalidOffset)?;
        if offset.get() < header.header_size
            || offset
                .get()
                .checked_add(24)
                .is_none_or(|end| end > arena_end)
        {
            return Err(JournalError::InvalidObjectLocation);
        }
        let compact = self
            .journal_header_ref()
            .has_incompatible_flag(HeaderIncompatibleFlags::Compact);
        self.window_manager.with_guarded(offset, |wm| {
            let prefix = wm.get_slice(offset.get(), 24)?;
            let header = OffsetArrayObjectHeader::ref_from_bytes(prefix)
                .map_err(|_| JournalError::ZerocopyFailure)?;
            let size = header.object_header.validated_size()?;
            if offset
                .get()
                .checked_add(size)
                .is_none_or(|end| end > arena_end)
            {
                return Err(JournalError::InvalidObjectLocation);
            }
            if header.object_header.type_ != ObjectType::EntryArray as u8 {
                return Err(JournalError::InvalidObjectType);
            }
            let item_size = if compact { 4 } else { 8 };
            if size <= 24 || (size - 24) % item_size != 0 {
                return Err(JournalError::InvalidObjectSize(size));
            }
            let available = wm.cached_file_size().saturating_sub(offset.get());
            let length = size.min(24 + (available.saturating_sub(24) / item_size) * item_size);
            let bytes = wm.get_slice(offset.get(), length)?;
            OffsetArrayObject::from_data(bytes, compact).ok_or(JournalError::ZerocopyFailure)
        })
    }

    pub(super) fn recovered_posting_count(
        &self,
        head: NonZeroU64,
        total: usize,
        mut previous: NonZeroU64,
    ) -> Result<usize> {
        let Some(prefix) = self.recovery.get() else {
            return Ok(total);
        };
        let Some(last) = prefix.last_entry else {
            return Ok(0);
        };
        let extent = self.reader_file_size()?;
        let (mut offset, mut remaining, mut count) = (Some(head), total, 0);
        let mut highest = head;
        while remaining > 0 {
            let Some(at) = offset else {
                if previous == last {
                    return Ok(count);
                }
                return Err(JournalError::ObjectExceedsFileBounds);
            };
            let Some(array) = self.recovery_array_prefix(at, extent)? else {
                if previous == last {
                    return Ok(count);
                }
                return Err(JournalError::ObjectExceedsFileBounds);
            };
            let wanted = remaining.min(array.capacity);
            let available = wanted.min(array.available);
            for index in 0..available {
                let Some(entry) = self.recovery_array_slot(at, index)? else {
                    if previous == last {
                        return Ok(count);
                    }
                    return Err(JournalError::InvalidOffset);
                };
                validate_offset_alignment(entry)?;
                if entry <= previous {
                    return Err(JournalError::InvalidOffset);
                }
                if entry > last {
                    return Ok(count);
                }
                previous = entry;
                count += 1;
            }
            if available < wanted {
                if previous == last {
                    return Ok(count);
                }
                return Err(JournalError::ObjectExceedsFileBounds);
            }
            remaining -= wanted;
            if remaining > 0 {
                if let Some(next) = array.next {
                    if next <= highest {
                        reject_array_revisit(head, at, next, |prior| {
                            self.recovery_array_prefix(prior, extent)?
                                .map(|array| array.next)
                                .ok_or(JournalError::ObjectExceedsFileBounds)
                        })?;
                    } else {
                        highest = next;
                    }
                }
            }
            offset = array.next;
        }
        Ok(count)
    }

    pub(super) fn recovered_last_entry(&self) -> Option<Option<NonZeroU64>> {
        self.recovery.get().map(|p| p.last_entry)
    }
}
