use super::mmap::MemoryMap;
use crate::error::{JournalError, Result};
use crate::file::{file::JournalFile, offset_array::InlinedCursor};
use std::num::NonZeroU64;

#[derive(Clone, Debug)]
pub enum FilterExpr {
    None,
    Match(NonZeroU64, InlinedCursor),
    Conjunction(Vec<FilterExpr>),
    Disjunction(Vec<FilterExpr>),
}

impl FilterExpr {
    pub fn head(&mut self) -> &mut Self {
        match self {
            FilterExpr::None => (),
            FilterExpr::Match(_, ic) => {
                *ic = ic.head();
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
        match self {
            FilterExpr::None => {}
            FilterExpr::Match(_, ic) => {
                *ic = ic.tail(journal_file)?;
            }
            FilterExpr::Conjunction(filter_exprs) => {
                for filter_expr in filter_exprs.iter_mut() {
                    filter_expr.tail(journal_file)?;
                }
            }
            FilterExpr::Disjunction(filter_exprs) => {
                for filter_expr in filter_exprs.iter_mut() {
                    filter_expr.tail(journal_file)?;
                }
            }
        }

        Ok(self)
    }

    // Returns the offset of the next matching entry, if any, with an offset
    // greater or equal to the needle offset.
    pub fn next<M: MemoryMap>(
        &mut self,
        journal_file: &JournalFile<M>,
        needle_offset: NonZeroU64,
    ) -> Result<Option<NonZeroU64>> {
        match self {
            FilterExpr::None => Ok(None),
            FilterExpr::Match(_, ic) => ic.next_until(journal_file, needle_offset),
            FilterExpr::Conjunction(filter_exprs) => {
                let mut needle_offset = needle_offset;

                loop {
                    let previous_offset = needle_offset;

                    for fe in filter_exprs.iter_mut() {
                        if let Some(new_offset) = fe.next(journal_file, needle_offset)? {
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
                    if let Some(fe_offset) = fe.next(journal_file, needle_offset)? {
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
        match self {
            FilterExpr::None => Ok(None),
            FilterExpr::Match(_, ic) => ic.previous_until(journal_file, needle_offset),
            FilterExpr::Conjunction(filter_exprs) => {
                let mut needle_offset = needle_offset;

                loop {
                    let previous_offset = needle_offset;

                    for fe in filter_exprs.iter_mut().rev() {
                        if let Some(new_offset) = fe.previous(journal_file, needle_offset)? {
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
                    if let Some(fe_offset) = fe.previous(journal_file, needle_offset)? {
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

            // Find all matches with the same key
            while i < self.current_matches.len()
                && Self::extract_key(&self.current_matches[i]).unwrap_or(&[]) == current_key
            {
                i += 1;
            }

            // If we have multiple values for this key, create a disjunction
            if i - start > 1 {
                let mut matches = Vec::with_capacity(i - start);
                for idx in start..i {
                    let data = self.current_matches[idx].as_slice();
                    let hash = journal_file.hash(data);

                    let match_expr = match journal_file.find_data_offset(hash, data)? {
                        Some(offset) => match journal_file.data_ref(offset)?.inlined_cursor() {
                            Some(ic) => FilterExpr::Match(offset, ic),
                            None => FilterExpr::None,
                        },
                        None => FilterExpr::None,
                    };
                    matches.push(match_expr);
                }
                elements.push(FilterExpr::Disjunction(matches));
            } else {
                let data = self.current_matches[start].as_slice();
                let hash = journal_file.hash(data);

                let match_expr = match journal_file.find_data_offset(hash, data)? {
                    Some(offset) => match journal_file.data_ref(offset)?.inlined_cursor() {
                        Some(ic) => FilterExpr::Match(offset, ic),
                        None => FilterExpr::None,
                    },
                    None => FilterExpr::None,
                };
                elements.push(match_expr);
            }
        }

        self.current_matches.clear();

        match elements.len() {
            0 => Ok(None),
            1 => Ok(Some(elements.remove(0))),
            _ => Ok(Some(FilterExpr::Conjunction(elements))),
        }
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

        self.current_matches.clear();
        match self.level0.len() {
            0 => Err(JournalError::MalformedFilter),
            1 => Ok(self.level0.remove(0)),
            _ => Ok(FilterExpr::Conjunction(std::mem::take(&mut self.level0))),
        }
    }
}
