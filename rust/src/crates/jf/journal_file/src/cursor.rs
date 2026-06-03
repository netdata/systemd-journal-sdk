use crate::{file::JournalFile, filter::FilterExpr, offset_array, offset_array::Direction};
use error::{JournalError, Result};
use std::num::NonZeroU64;
use window_manager::MemoryMap;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Location {
    Head,
    Tail,
    Realtime(u64),
    Monotonic(u64, [u8; 16]),
    Seqnum(u64, Option<[u8; 16]>),
    XorHash(u64),
    ResolvedEntry(NonZeroU64),
}

impl Default for Location {
    fn default() -> Self {
        Self::Head
    }
}

#[derive(Debug)]
pub struct JournalCursor {
    pub location: Location,
    pub filter_expr: Option<FilterExpr>,
    pub array_cursor: Option<offset_array::Cursor>,
}

impl JournalCursor {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            location: Location::Head,
            filter_expr: None,
            array_cursor: None,
        }
    }

    pub fn set_location(&mut self, location: Location) {
        self.location = location;
        self.array_cursor = None;
    }

    pub fn set_filter(&mut self, filter_expr: FilterExpr) {
        self.filter_expr = Some(filter_expr);
        // FIXME: should we set cursor to None?
    }

    pub fn clear_filter(&mut self) {
        self.filter_expr = None;
        self.array_cursor = None;
        self.set_location(Location::Head);
    }

    pub fn step<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        direction: Direction,
    ) -> Result<bool> {
        let new_location = if self.filter_expr.is_some() {
            self.resolve_filter_location(journal_file, direction)?
        } else {
            self.resolve_array_cursor(journal_file, direction)?
        };

        if let Some(location) = new_location {
            self.location = location;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn position(&self) -> Result<NonZeroU64> {
        match self.location {
            Location::ResolvedEntry(entry_offset) => Ok(entry_offset),
            _ => Err(JournalError::UnsetCursor),
        }
    }

    fn entry_list<M: MemoryMap>(journal_file: &JournalFile<M>) -> Result<offset_array::List> {
        journal_file
            .entry_list()
            .ok_or(JournalError::InvalidOffsetArrayOffset)
    }

    fn store_array_cursor_value<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        cursor: offset_array::Cursor,
    ) -> Result<Option<Location>> {
        let Some(offset) = cursor.value(journal_file)? else {
            return Ok(None);
        };
        self.array_cursor = Some(cursor);
        Ok(Some(Location::ResolvedEntry(offset)))
    }

    fn resolve_head_array<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
    ) -> Result<Option<Location>> {
        let cursor = Self::entry_list(journal_file)?.cursor_head();
        self.store_array_cursor_value(journal_file, cursor)
    }

    fn resolve_tail_array<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
    ) -> Result<Option<Location>> {
        let entry_list = Self::entry_list(journal_file)?;
        let cursor = entry_list.cursor_tail(journal_file)?;
        self.store_array_cursor_value(journal_file, cursor)
    }

    fn realtime_array_cursor<M: MemoryMap>(
        journal_file: &JournalFile<M>,
        realtime: u64,
    ) -> Result<offset_array::Cursor> {
        let entry_list = Self::entry_list(journal_file)?;
        let predicate = |entry_offset| {
            let entry_object = journal_file.entry_ref(entry_offset)?;
            Ok(entry_object.header.realtime < realtime)
        };

        entry_list
            .directed_partition_point(journal_file, predicate, Direction::Forward)?
            .map(Ok)
            .unwrap_or_else(|| entry_list.cursor_tail(journal_file))
    }

    fn resolve_realtime_array<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        realtime: u64,
    ) -> Result<Option<Location>> {
        let cursor = Self::realtime_array_cursor(journal_file, realtime)?;
        self.store_array_cursor_value(journal_file, cursor)
    }

    fn step_array_cursor<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        direction: Direction,
    ) -> Result<Option<Location>> {
        let cursor = self.array_cursor.ok_or(JournalError::UnsetCursor)?;
        let cursor = match direction {
            Direction::Forward => cursor.next(journal_file)?,
            Direction::Backward => cursor.previous(journal_file)?,
        };
        let Some(cursor) = cursor else {
            return Ok(None);
        };
        self.store_array_cursor_value(journal_file, cursor)
    }

    fn resolve_array_cursor<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        direction: Direction,
    ) -> Result<Option<Location>> {
        match (self.location, direction) {
            (Location::Head, Direction::Forward) => self.resolve_head_array(journal_file),
            (Location::Head, Direction::Backward) => Ok(None),
            (Location::Tail, Direction::Forward) => Ok(None),
            (Location::Tail, Direction::Backward) => self.resolve_tail_array(journal_file),
            (Location::Realtime(realtime), _) => {
                self.resolve_realtime_array(journal_file, realtime)
            }
            (Location::ResolvedEntry(_), direction) => {
                self.step_array_cursor(journal_file, direction)
            }
            _ => Err(JournalError::InvalidQueryConfiguration),
        }
    }

    fn filter_expr_mut(&mut self) -> Result<&mut FilterExpr> {
        self.filter_expr
            .as_mut()
            .ok_or(JournalError::InvalidQueryConfiguration)
    }

    fn filter_head<M: MemoryMap>(
        filter_expr: &mut FilterExpr,
        journal_file: &JournalFile<M>,
    ) -> Result<Option<Location>> {
        Ok(filter_expr
            .head()
            .next(journal_file, NonZeroU64::MIN)?
            .map(Location::ResolvedEntry))
    }

    fn filter_tail<M: MemoryMap>(
        filter_expr: &mut FilterExpr,
        journal_file: &JournalFile<M>,
    ) -> Result<Option<Location>> {
        Ok(filter_expr
            .tail(journal_file)?
            .previous(journal_file, NonZeroU64::MAX)?
            .map(Location::ResolvedEntry))
    }

    fn resolve_filter_at_offset<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        direction: Direction,
        entry_offset: NonZeroU64,
    ) -> Result<Option<Location>> {
        let filter_expr = self.filter_expr_mut()?;
        match direction {
            Direction::Forward => Ok(filter_expr
                .head()
                .next(journal_file, entry_offset)?
                .map(Location::ResolvedEntry)),
            Direction::Backward => Ok(filter_expr
                .tail(journal_file)?
                .previous(journal_file, entry_offset)?
                .map(Location::ResolvedEntry)),
        }
    }

    fn resolve_filter_realtime<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        realtime: u64,
        direction: Direction,
    ) -> Result<Option<Location>> {
        let cursor = Self::realtime_array_cursor(journal_file, realtime)?;
        let Some(entry_offset) = cursor.value(journal_file)? else {
            return Ok(None);
        };
        self.resolve_filter_at_offset(journal_file, direction, entry_offset)
    }

    fn filter_after_resolved<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        location_offset: NonZeroU64,
    ) -> Result<Option<Location>> {
        Ok(self
            .filter_expr_mut()?
            .next(journal_file, location_offset.saturating_add(1))?
            .map(Location::ResolvedEntry))
    }

    fn filter_before_resolved<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        location_offset: NonZeroU64,
    ) -> Result<Option<Location>> {
        let Some(needle_offset) = NonZeroU64::new(location_offset.get() - 1) else {
            return Ok(None);
        };
        Ok(self
            .filter_expr_mut()?
            .previous(journal_file, needle_offset)?
            .map(Location::ResolvedEntry))
    }

    fn resolve_filter_location<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        direction: Direction,
    ) -> Result<Option<Location>> {
        match (self.location, direction) {
            (Location::Head, Direction::Forward) => {
                Self::filter_head(self.filter_expr_mut()?, journal_file)
            }
            (Location::Head, Direction::Backward) => Ok(None),
            (Location::Tail, Direction::Forward) => Ok(None),
            (Location::Tail, Direction::Backward) => {
                Self::filter_tail(self.filter_expr_mut()?, journal_file)
            }
            (Location::Realtime(realtime), direction) => {
                self.resolve_filter_realtime(journal_file, realtime, direction)
            }
            (Location::ResolvedEntry(offset), Direction::Forward) => {
                self.filter_after_resolved(journal_file, offset)
            }
            (Location::ResolvedEntry(offset), Direction::Backward) => {
                self.filter_before_resolved(journal_file, offset)
            }
            _ => Err(JournalError::InvalidQueryConfiguration),
        }
    }
}
