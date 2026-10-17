use super::mmap::MemoryMap;
use crate::error::{JournalError, Result};
use crate::file::{
    file::JournalFile,
    offset_array::{Direction, InlinedCursor},
};
use std::num::NonZeroU64;

#[derive(Clone, Debug)]
pub enum FilterExpr {
    None,
    Match(NonZeroU64, InlinedCursor),
    Conjunction(Vec<FilterExpr>),
    Disjunction(Vec<FilterExpr>),
}

impl FilterExpr {
    pub(super) fn invalidate_position(&mut self) {
        self.visit_cursors(&mut |cursor| cursor.filter_direction = None);
    }

    pub(super) fn refresh_recovery<M: MemoryMap>(&mut self, file: &JournalFile<M>) -> Result<()> {
        Self::refresh_recovery_batch(std::slice::from_mut(self), file)
    }

    fn refresh_recovery_batch<M: MemoryMap>(
        expressions: &mut [Self],
        file: &JournalFile<M>,
    ) -> Result<()> {
        file.filter_recovery_seen.set(true);
        for expression in expressions.iter_mut() {
            expression.visit_cursors(&mut |cursor| cursor.recovery_pending = true);
        }
        loop {
            let before = (file.recovery_status(), file.recovered_tail_metadata());
            for expression in expressions.iter_mut() {
                expression.refresh_recovery_pass(file)?;
            }
            if before == (file.recovery_status(), file.recovered_tail_metadata()) {
                for expression in expressions.iter_mut() {
                    expression.visit_cursors(&mut |cursor| cursor.recovery_pending = false);
                }
                return Ok(());
            }
        }
    }

    fn visit_cursors(&mut self, visit: &mut impl FnMut(&mut InlinedCursor)) {
        match self {
            Self::None => {}
            Self::Match(_, cursor) => visit(cursor),
            Self::Conjunction(children) | Self::Disjunction(children) => {
                for child in children {
                    child.visit_cursors(visit);
                }
            }
        }
    }

    fn any_cursor(&self, predicate: &impl Fn(&InlinedCursor) -> bool) -> bool {
        match self {
            Self::None => false,
            Self::Match(_, cursor) => predicate(cursor),
            Self::Conjunction(children) | Self::Disjunction(children) => {
                children.iter().any(|child| child.any_cursor(predicate))
            }
        }
    }

    fn refresh_recovery_pass<M: MemoryMap>(&mut self, file: &JournalFile<M>) -> Result<()> {
        match self {
            Self::None => {}
            Self::Match(offset, current) => {
                let mut cursor = match file.reader_data_cursor(*offset)? {
                    Some(cursor) => cursor,
                    None => {
                        let mut cursor = current.clone().empty();
                        cursor.recovered_population = file.reader_recovery_active();
                        cursor
                    }
                };
                cursor.recovery_pending = true;
                cursor.position_pending = true;
                *current = cursor;
            }
            Self::Conjunction(children) | Self::Disjunction(children) => {
                for child in children {
                    child.refresh_recovery_pass(file)?;
                }
            }
        }
        Ok(())
    }
    pub fn head(&mut self) -> &mut Self {
        match self {
            FilterExpr::None => (),
            FilterExpr::Match(_, ic) => {
                *ic = ic.head();
                ic.position_pending = false;
            }
            FilterExpr::Conjunction(filter_exprs) => {
                for filter_expr in filter_exprs.iter_mut() {
                    filter_expr.head();
                }
            }
            FilterExpr::Disjunction(filter_exprs) => {
                for filter_expr in filter_exprs.iter_mut() {
                    filter_expr.head();
                }
            }
        }

        self
    }

    pub fn tail<M: MemoryMap>(&mut self, journal_file: &JournalFile<M>) -> Result<&mut Self> {
        if !journal_file.filter_recovery_seen.get() {
            self.tail_inner(journal_file)?;
            return Ok(self);
        }
        if journal_file.filter_recovery_seen.get()
            && self.any_cursor(&|cursor| cursor.recovery_pending)
        {
            self.refresh_recovery(journal_file)?;
        }
        self.tail_inner(journal_file)?;
        self.visit_cursors(&mut |cursor| cursor.position_pending = false);
        Ok(self)
    }

    fn tail_inner<M: MemoryMap>(&mut self, journal_file: &JournalFile<M>) -> Result<()> {
        match self {
            FilterExpr::None => {}
            FilterExpr::Match(_, ic) => {
                *ic = ic.tail(journal_file)?;
            }
            FilterExpr::Conjunction(filter_exprs) => {
                for filter_expr in filter_exprs.iter_mut() {
                    filter_expr.tail_inner(journal_file)?;
                }
            }
            FilterExpr::Disjunction(filter_exprs) => {
                for filter_expr in filter_exprs.iter_mut() {
                    filter_expr.tail_inner(journal_file)?;
                }
            }
        }

        Ok(())
    }

    fn prepare_recovery<M: MemoryMap>(
        &mut self,
        file: &JournalFile<M>,
        backward: bool,
    ) -> Result<()> {
        if !file.filter_recovery_seen.get() {
            return Ok(());
        }
        if self.any_cursor(&|cursor| cursor.recovery_pending) {
            self.refresh_recovery(file)?;
        }
        if self.any_cursor(&|cursor| cursor.position_pending) {
            if backward {
                self.tail_inner(file)?;
                self.visit_cursors(&mut |cursor| cursor.position_pending = false);
            } else {
                self.head();
            }
        }
        Ok(())
    }

    pub(super) fn seek_realtime<M: MemoryMap>(
        &mut self,
        file: &JournalFile<M>,
        needle: NonZeroU64,
        direction: Direction,
    ) -> Result<Option<NonZeroU64>> {
        self.prepare_recovery(file, false)?;
        self.invalidate_position();
        self.seek_realtime_inner(file, needle, direction)
    }

    fn seek_realtime_inner<M: MemoryMap>(
        &mut self,
        file: &JournalFile<M>,
        mut needle: NonZeroU64,
        direction: Direction,
    ) -> Result<Option<NonZeroU64>> {
        match self {
            Self::None => Ok(None),
            Self::Match(_, cursor) => cursor.seek_realtime(file, needle, direction),
            Self::Conjunction(children) => {
                let initial = needle;
                for i in 0..children.len() {
                    let index = if direction == Direction::Forward {
                        i
                    } else {
                        children.len() - 1 - i
                    };
                    let Some(value) =
                        children[index].seek_realtime_inner(file, needle, direction)?
                    else {
                        return Ok(None);
                    };
                    needle = value;
                }
                if needle == initial {
                    return Ok(Some(needle));
                }
                match direction {
                    Direction::Forward => self.next_inner(file, needle),
                    Direction::Backward => self.previous_inner(file, needle),
                }
            }
            Self::Disjunction(children) => {
                let mut best = None;
                for child in children {
                    if let Some(value) = child.seek_realtime_inner(file, needle, direction)? {
                        best = Some(match (best, direction) {
                            (Some(old), Direction::Forward) => value.min(old),
                            (Some(old), Direction::Backward) => value.max(old),
                            (None, _) => value,
                        });
                    }
                }
                Ok(best)
            }
        }
    }

    // Returns the offset of the next matching entry, if any, with an offset
    // greater or equal to the needle offset.
    pub fn next<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        needle_offset: NonZeroU64,
    ) -> Result<Option<NonZeroU64>> {
        self.prepare_recovery(journal_file, false)?;
        self.next_inner(journal_file, needle_offset)
    }

    fn next_inner<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        needle_offset: NonZeroU64,
    ) -> Result<Option<NonZeroU64>> {
        match self {
            FilterExpr::None => Ok(None),
            FilterExpr::Match(_, ic) => {
                ic.prepare_filter_direction(journal_file, Direction::Forward)?;
                ic.next_until(journal_file, needle_offset)
            }
            FilterExpr::Conjunction(filter_exprs) => {
                let mut needle_offset = needle_offset;

                loop {
                    let previous_offset = needle_offset;

                    for fe in filter_exprs.iter_mut() {
                        if let Some(new_offset) = fe.next_inner(journal_file, needle_offset)? {
                            needle_offset = new_offset;
                        } else {
                            return Ok(None);
                        }
                    }

                    if needle_offset == previous_offset {
                        return Ok(Some(needle_offset));
                    }
                }
            }
            FilterExpr::Disjunction(filter_exprs) => {
                let mut best_offset: Option<NonZeroU64> = None;

                for fe in filter_exprs.iter_mut() {
                    if let Some(fe_offset) = fe.next_inner(journal_file, needle_offset)? {
                        best_offset = match best_offset {
                            Some(offset) => Some(fe_offset.min(offset)),
                            None => Some(fe_offset),
                        };
                    }
                }

                Ok(best_offset)
            }
        }
    }

    // Returns the offset of the previous matching entry, if any, with an offset
    // less or equal to the needle offset.
    pub fn previous<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        needle_offset: NonZeroU64,
    ) -> Result<Option<NonZeroU64>> {
        self.prepare_recovery(journal_file, true)?;
        self.previous_inner(journal_file, needle_offset)
    }

    fn previous_inner<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        needle_offset: NonZeroU64,
    ) -> Result<Option<NonZeroU64>> {
        match self {
            FilterExpr::None => Ok(None),
            FilterExpr::Match(_, ic) => {
                ic.prepare_filter_direction(journal_file, Direction::Backward)?;
                ic.previous_until(journal_file, needle_offset)
            }
            FilterExpr::Conjunction(filter_exprs) => {
                let mut needle_offset = needle_offset;

                loop {
                    let previous_offset = needle_offset;

                    for fe in filter_exprs.iter_mut().rev() {
                        if let Some(new_offset) = fe.previous_inner(journal_file, needle_offset)? {
                            needle_offset = new_offset;
                        } else {
                            return Ok(None);
                        }
                    }

                    if needle_offset == previous_offset {
                        return Ok(Some(needle_offset));
                    }
                }
            }
            FilterExpr::Disjunction(filter_exprs) => {
                let mut best_offset: Option<NonZeroU64> = None;

                for fe in filter_exprs.iter_mut() {
                    if let Some(fe_offset) = fe.previous_inner(journal_file, needle_offset)? {
                        best_offset = match best_offset {
                            Some(offset) => Some(fe_offset.max(offset)),
                            None => Some(fe_offset),
                        };
                    }
                }

                Ok(best_offset)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    Conjunction,
    Disjunction,
}

#[derive(Debug)]
pub struct JournalFilter {
    level0: Vec<FilterExpr>,
    level1: Vec<FilterExpr>,
    current_matches: Vec<Vec<u8>>,
}

impl Default for JournalFilter {
    fn default() -> Self {
        Self {
            level0: Vec::new(),
            level1: Vec::new(),
            current_matches: Vec::new(),
        }
    }
}

impl JournalFilter {
    fn extract_key(kv_pair: &[u8]) -> Option<&[u8]> {
        if let Some(equal_pos) = kv_pair.iter().position(|&b| b == b'=') {
            Some(&kv_pair[..equal_pos])
        } else {
            None
        }
    }

    fn build_match_expr<M: MemoryMap>(
        journal_file: &JournalFile<M>,
        data: &[u8],
    ) -> Result<FilterExpr> {
        let hash = journal_file.hash(data);
        let Some(offset) = journal_file.find_data_offset(hash, data)? else {
            return Ok(FilterExpr::None);
        };
        let Some(ic) = journal_file.reader_data_cursor(offset)? else {
            return Ok(FilterExpr::None);
        };
        Ok(FilterExpr::Match(offset, ic))
    }

    fn matching_key_range_end(&self, start: usize, key: &[u8]) -> usize {
        let mut end = start;
        while end < self.current_matches.len()
            && Self::extract_key(&self.current_matches[end]).unwrap_or(&[]) == key
        {
            end += 1;
        }
        end
    }

    fn convert_key_group<M: MemoryMap>(
        &self,
        journal_file: &JournalFile<M>,
        start: usize,
        end: usize,
    ) -> Result<FilterExpr> {
        if end - start == 1 {
            return Self::build_match_expr(journal_file, self.current_matches[start].as_slice());
        }

        let mut matches = Vec::with_capacity(end - start);
        for idx in start..end {
            let data = self.current_matches[idx].as_slice();
            matches.push(Self::build_match_expr(journal_file, data)?);
        }
        Ok(FilterExpr::Disjunction(matches))
    }

    fn collapse_filter_elements(elements: &mut Vec<FilterExpr>) -> Option<FilterExpr> {
        match elements.len() {
            0 => None,
            1 => Some(elements.remove(0)),
            _ => Some(FilterExpr::Conjunction(std::mem::take(elements))),
        }
    }

    fn convert_current_matches<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
    ) -> Result<Option<FilterExpr>> {
        if self.current_matches.is_empty() {
            return Ok(None);
        }

        let mut elements = Vec::new();
        let mut i = 0;

        while i < self.current_matches.len() {
            let current_key = Self::extract_key(&self.current_matches[i]).unwrap_or(&[]);
            let start = i;
            i = self.matching_key_range_end(start, current_key);
            elements.push(self.convert_key_group(journal_file, start, i)?);
        }

        self.current_matches.clear();
        Ok(Self::collapse_filter_elements(&mut elements))
    }

    pub fn add_match(&mut self, kv_pair: &[u8]) {
        if kv_pair.contains(&b'=') {
            let new_item = kv_pair.to_vec();
            let new_key = Self::extract_key(&new_item).unwrap_or(&[]);

            // Find the insertion position using binary search
            let pos = self
                .current_matches
                .binary_search_by(|item| {
                    let key = Self::extract_key(item).unwrap_or(&[]);
                    key.cmp(new_key)
                })
                .unwrap_or_else(|e| e);

            // Insert at the found position
            self.current_matches.insert(pos, new_item);
        }
    }

    fn commit_current<M: MemoryMap>(&mut self, journal_file: &JournalFile<M>) -> Result<()> {
        if let Some(expr) = self.convert_current_matches(journal_file)? {
            self.level1.push(expr);
        }
        Ok(())
    }

    fn commit_level1(&mut self) {
        match self.level1.len() {
            0 => {}
            1 => self.level0.push(self.level1.remove(0)),
            _ => self
                .level0
                .push(FilterExpr::Disjunction(std::mem::take(&mut self.level1))),
        }
    }

    pub fn set_operation<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        op: LogicalOp,
    ) -> Result<()> {
        self.commit_current(journal_file)?;
        if op == LogicalOp::Conjunction {
            self.commit_level1();
        }
        Ok(())
    }

    pub fn build<M: MemoryMap>(&mut self, journal_file: &JournalFile<M>) -> Result<FilterExpr> {
        self.commit_current(journal_file)?;
        self.commit_level1();

        if journal_file.reader_recovery_active()
            || self.level0.iter().any(|expr| {
                expr.any_cursor(&|cursor| cursor.recovery_pending || cursor.recovered_population)
            })
        {
            FilterExpr::refresh_recovery_batch(&mut self.level0, journal_file)?;
        }
        self.current_matches.clear();
        match self.level0.len() {
            0 => Err(JournalError::MalformedFilter),
            1 => Ok(self.level0.remove(0)),
            _ => Ok(FilterExpr::Conjunction(std::mem::take(&mut self.level0))),
        }
    }
}
