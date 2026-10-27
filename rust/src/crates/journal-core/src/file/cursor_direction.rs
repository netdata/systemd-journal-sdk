use super::{Direction, JournalCursor, JournalFile, Location, filter::FilterExpr, mmap::MemoryMap};
use crate::error::{JournalError, Result};
use std::num::NonZeroU64;

/// Parsed cursor components for indexed directional positioning.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CursorAnchor {
    pub sequence: Option<([u8; 16], u64)>,
    pub monotonic: Option<([u8; 16], u64)>,
    pub realtime: Option<u64>,
}

#[derive(Clone, Copy)]
enum Key {
    Sequence,
    Realtime,
    Monotonic,
    Offset,
}

impl<M: MemoryMap> JournalFile<M> {
    fn cursor_bound(
        &self,
        data: Option<NonZeroU64>,
        key: Key,
        needle: u64,
        direction: Direction,
    ) -> Result<Option<NonZeroU64>> {
        let predicate = |offset: NonZeroU64| {
            let value = if matches!(key, Key::Offset) {
                offset.get()
            } else {
                let entry = self.entry_ref(offset)?;
                match key {
                    Key::Sequence => entry.header.seqnum,
                    Key::Monotonic => entry.header.monotonic,
                    _ => entry.header.realtime,
                }
            };
            Ok(value < needle || direction == Direction::Backward && value == needle)
        };
        if let Some(data) = data {
            return self.data_object_directed_partition_point(data, predicate, direction);
        }
        if self.reader_entry_count() == 0 {
            return Ok(None);
        }
        let list = self
            .entry_list()
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        let Some(cursor) = list.directed_partition_point(self, predicate, direction)? else {
            return Ok(None);
        };
        cursor.value(self)
    }

    fn cursor_boot_data(&self, boot: [u8; 16]) -> Result<Option<NonZeroU64>> {
        let mut payload = [0u8; 41];
        payload[..9].copy_from_slice(b"_BOOT_ID=");
        const HEX: &[u8] = b"0123456789abcdef";
        for (i, byte) in boot.into_iter().enumerate() {
            payload[9 + 2 * i] = HEX[(byte >> 4) as usize];
            payload[10 + 2 * i] = HEX[(byte & 15) as usize];
        }
        self.find_data_offset(self.hash(&payload), &payload)
    }

    fn cursor_monotonic(
        &self,
        data: Option<NonZeroU64>,
        boot: [u8; 16],
        needle: u64,
        direction: Direction,
    ) -> Result<Option<NonZeroU64>> {
        let Some(boot_data) = self.cursor_boot_data(boot)? else {
            return Ok(None);
        };
        let Some(mut offset) =
            self.cursor_bound(Some(boot_data), Key::Monotonic, needle, direction)?
        else {
            return Ok(None);
        };
        let Some(data) = data else {
            return Ok(Some(offset));
        };
        loop {
            let Some(candidate) =
                self.cursor_bound(Some(data), Key::Offset, offset.get(), direction)?
            else {
                return Ok(None);
            };
            if candidate == offset {
                return Ok(Some(offset));
            }
            let Some(next) =
                self.cursor_bound(Some(boot_data), Key::Offset, candidate.get(), direction)?
            else {
                return Ok(None);
            };
            offset = next;
            if offset == candidate {
                return Ok(Some(offset));
            }
        }
    }

    fn cursor_adjacent_boot(
        &self,
        data: Option<NonZeroU64>,
        mut boot: [u8; 16],
        direction: Direction,
    ) -> Result<Option<NonZeroU64>> {
        let mut previous = None;
        loop {
            let Some(boot_data) = self.cursor_boot_data(boot)? else {
                return Ok(None);
            };
            let (needle, opposite) = match direction {
                Direction::Forward => (u64::MAX, Direction::Backward),
                Direction::Backward => (0, Direction::Forward),
            };
            let Some(boundary) =
                self.cursor_bound(Some(boot_data), Key::Offset, needle, opposite)?
            else {
                return Ok(None);
            };
            let needle = match direction {
                Direction::Forward => boundary.get().checked_add(1),
                Direction::Backward => boundary.get().checked_sub(1),
            };
            let Some(needle) = needle else {
                return Ok(None);
            };
            let Some(offset) = self.cursor_bound(None, Key::Offset, needle, direction)? else {
                return Ok(None);
            };
            if data.is_none() {
                return Ok(Some(offset));
            }
            if previous.is_some_and(|old| match direction {
                Direction::Forward => offset <= old,
                Direction::Backward => offset >= old,
            }) {
                return Err(JournalError::InvalidQueryConfiguration);
            }
            previous = Some(offset);
            boot = self.entry_ref(offset)?.header.boot_id;
            let needle = if direction == Direction::Forward {
                0
            } else {
                u64::MAX
            };
            if let Some(candidate) = self.cursor_monotonic(data, boot, needle, direction)? {
                return Ok(Some(candidate));
            }
        }
    }

    pub(super) fn cursor_initial_offset(
        &self,
        data: Option<NonZeroU64>,
        anchor: &CursorAnchor,
        direction: Direction,
    ) -> Result<Option<NonZeroU64>> {
        if let Some((id, seqnum)) = anchor.sequence {
            if id == self.journal_header_ref().seqnum_id {
                return self.cursor_bound(data, Key::Sequence, seqnum, direction);
            }
        }
        if let Some((boot, monotonic)) = anchor.monotonic {
            if let Some(offset) = self.cursor_monotonic(data, boot, monotonic, direction)? {
                return Ok(Some(offset));
            }
        }
        if let Some(realtime) = anchor.realtime {
            return self.cursor_bound(data, Key::Realtime, realtime, direction);
        }
        if let Some((boot, _)) = anchor.monotonic {
            return self.cursor_adjacent_boot(data, boot, direction);
        }
        let needle = if direction == Direction::Forward {
            0
        } else {
            u64::MAX
        };
        self.cursor_bound(data, Key::Offset, needle, direction)
    }
}

impl FilterExpr {
    pub(super) fn seek_cursor_anchor<M: MemoryMap>(
        &mut self,
        file: &JournalFile<M>,
        anchor: &CursorAnchor,
        direction: Direction,
    ) -> Result<Option<NonZeroU64>> {
        self.prepare_recovery(file, false)?;
        match self {
            Self::None => Ok(None),
            Self::Match(data, _) => file.cursor_initial_offset(Some(*data), anchor, direction),
            Self::Disjunction(children) => {
                let mut best: Option<NonZeroU64> = None;
                for child in children {
                    if let Some(offset) = child.seek_cursor_anchor(file, anchor, direction)? {
                        best = Some(match (best, direction) {
                            (Some(old), Direction::Forward) => old.min(offset),
                            (Some(old), Direction::Backward) => old.max(offset),
                            (None, _) => offset,
                        });
                    }
                }
                Ok(best)
            }
            Self::Conjunction(children) => {
                let mut best: Option<NonZeroU64> = None;
                for child in children {
                    let Some(offset) = child.seek_cursor_anchor(file, anchor, direction)? else {
                        return Ok(None);
                    };
                    best = Some(match (best, direction) {
                        (Some(old), Direction::Forward) => old.max(offset),
                        (Some(old), Direction::Backward) => old.min(offset),
                        (None, _) => offset,
                    });
                }
                match best {
                    Some(offset) => self.seek_realtime(file, offset, direction),
                    None => Ok(None),
                }
            }
        }
    }
}

impl JournalCursor {
    pub(super) fn seek_anchor<M: MemoryMap>(
        &mut self,
        file: &JournalFile<M>,
        anchor: &CursorAnchor,
        direction: Direction,
    ) -> Result<bool> {
        loop {
            let before = file.recovery_status();
            let result = if let Some(filter) = self.filter_expr.as_mut() {
                filter.seek_cursor_anchor(file, anchor, direction)
            } else {
                file.cursor_initial_offset(None, anchor, direction)
            };
            let offset = match result {
                Err(err) if file.recover_reader_tail(&err)? => {
                    if let Some(filter) = self.filter_expr.as_mut() {
                        filter.refresh_recovery(file)?;
                    }
                    continue;
                }
                result => result?,
            };
            if before != file.recovery_status() {
                if let Some(filter) = self.filter_expr.as_mut() {
                    filter.refresh_recovery(file)?;
                }
                continue;
            }
            self.set_location(match offset {
                Some(offset) => Location::ResolvedEntry(offset),
                None if direction == Direction::Forward => Location::Tail,
                None => Location::Head,
            });
            return Ok(offset.is_some());
        }
    }
}
