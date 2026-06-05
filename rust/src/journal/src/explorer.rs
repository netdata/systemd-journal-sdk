use super::*;
use journal_core::file::{DataObject, offset_array::InlinedCursor};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const DEFAULT_HISTOGRAM_TARGET_BUCKETS: usize = 150;
const DEFAULT_TIME_SLACK_USEC: u64 = 120_000_000;
const SOURCE_REALTIME_FIELD: &[u8] = b"_SOURCE_REALTIME_TIMESTAMP";
const UNSET_VALUE: &[u8] = b"-";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerAnchor {
    Auto,
    Head,
    Tail,
    Realtime(u64),
}

impl Default for ExplorerAnchor {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerFieldMode {
    AllValues,
    FirstValue,
}

impl Default for ExplorerFieldMode {
    fn default() -> Self {
        Self::FirstValue
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExplorerStrategy {
    Traversal,
    Index,
    Compare,
}

impl Default for ExplorerStrategy {
    fn default() -> Self {
        Self::Traversal
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerFilter {
    pub field: Vec<u8>,
    pub values: Vec<Vec<u8>>,
}

impl ExplorerFilter {
    pub fn new(
        field: impl Into<Vec<u8>>,
        values: impl IntoIterator<Item = impl Into<Vec<u8>>>,
    ) -> Self {
        Self {
            field: field.into(),
            values: values.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExplorerQuery {
    pub after_realtime_usec: Option<u64>,
    pub before_realtime_usec: Option<u64>,
    pub anchor: ExplorerAnchor,
    pub direction: Direction,
    pub limit: usize,
    pub filters: Vec<ExplorerFilter>,
    pub facets: Vec<Vec<u8>>,
    pub histogram: Option<Vec<u8>>,
    pub histogram_target_buckets: usize,
    pub fts_patterns: Vec<Vec<u8>>,
    pub field_mode: ExplorerFieldMode,
    pub use_source_realtime: bool,
    pub realtime_slack_usec: u64,
}

impl Default for ExplorerQuery {
    fn default() -> Self {
        Self {
            after_realtime_usec: None,
            before_realtime_usec: None,
            anchor: ExplorerAnchor::Auto,
            direction: Direction::Forward,
            limit: 200,
            filters: Vec::new(),
            facets: Vec::new(),
            histogram: None,
            histogram_target_buckets: DEFAULT_HISTOGRAM_TARGET_BUCKETS,
            fts_patterns: Vec::new(),
            field_mode: ExplorerFieldMode::FirstValue,
            use_source_realtime: true,
            realtime_slack_usec: DEFAULT_TIME_SLACK_USEC,
        }
    }
}

impl ExplorerQuery {
    pub fn with_filter(
        mut self,
        field: impl Into<Vec<u8>>,
        values: impl IntoIterator<Item = impl Into<Vec<u8>>>,
    ) -> Self {
        self.filters.push(ExplorerFilter::new(field, values));
        self
    }

    pub fn with_facet(mut self, field: impl Into<Vec<u8>>) -> Self {
        self.facets.push(field.into());
        self
    }

    pub fn with_histogram(mut self, field: impl Into<Vec<u8>>) -> Self {
        self.histogram = Some(field.into());
        self
    }

    pub fn with_fts_pattern(mut self, pattern: impl Into<Vec<u8>>) -> Self {
        self.fts_patterns.push(pattern.into());
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ExplorerStats {
    pub rows_examined: u64,
    pub rows_matched: u64,
    pub facet_rows_matched: u64,
    pub rows_returned: u64,
    pub data_refs_seen: u64,
    pub data_refs_skipped: u64,
    pub data_payloads_loaded: u64,
    pub data_objects_classified: u64,
    pub data_cache_hits: u64,
    pub data_cache_misses: u64,
    pub payloads_decompressed: u64,
    pub fts_scans: u64,
    pub facet_updates: u64,
    pub histogram_updates: u64,
    pub returned_row_expansions: u64,
    pub early_stop_opportunities: u64,
    pub early_stops: u64,
}

#[derive(Debug, Clone)]
pub struct ExplorerRow {
    pub realtime_usec: u64,
    pub cursor: String,
    pub payloads: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct ExplorerHistogramBucket {
    pub start_realtime_usec: u64,
    pub end_realtime_usec: u64,
    pub values: HashMap<Vec<u8>, u64>,
}

#[derive(Debug, Clone)]
pub struct ExplorerHistogram {
    pub field: Vec<u8>,
    pub buckets: Vec<ExplorerHistogramBucket>,
}

#[derive(Debug, Clone, Default)]
pub struct ExplorerComparison {
    pub traversal_duration: Duration,
    pub index_duration: Duration,
    pub traversal_stats: ExplorerStats,
    pub index_stats: ExplorerStats,
}

#[derive(Debug, Clone, Default)]
pub struct ExplorerResult {
    pub rows: Vec<ExplorerRow>,
    pub facets: HashMap<Vec<u8>, HashMap<Vec<u8>, u64>>,
    pub histogram: Option<ExplorerHistogram>,
    pub stats: ExplorerStats,
    pub comparison: Option<ExplorerComparison>,
}

#[derive(Default)]
struct RowScan {
    timestamp: Option<u64>,
    fts_matches: bool,
}

const FACET_PUBLIC: u8 = 0x01;
const FACET_HISTOGRAM: u8 = 0x02;
const FACET_SOURCE_REALTIME: u8 = 0x04;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OffsetClass {
    Irrelevant,
    FtsMatch,
    Value(usize),
}

impl OffsetClass {
    const IRRELEVANT_RAW: usize = 1;
    const FTS_MATCH_RAW: usize = 2;
    const VALUE_BASE: usize = 3;

    fn to_raw(self) -> usize {
        match self {
            Self::Irrelevant => Self::IRRELEVANT_RAW,
            Self::FtsMatch => Self::FTS_MATCH_RAW,
            Self::Value(index) => Self::VALUE_BASE.saturating_add(index),
        }
    }

    fn from_raw(raw: usize) -> Self {
        match raw {
            Self::IRRELEVANT_RAW => Self::Irrelevant,
            Self::FTS_MATCH_RAW => Self::FtsMatch,
            raw => Self::Value(raw.saturating_sub(Self::VALUE_BASE)),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct OffsetClassSlot {
    offset: u64,
    class: usize,
}

#[derive(Debug)]
struct OffsetClassCache {
    slots: Vec<OffsetClassSlot>,
    len: usize,
}

impl Default for OffsetClassCache {
    fn default() -> Self {
        Self {
            slots: vec![OffsetClassSlot::default(); 256],
            len: 0,
        }
    }
}

impl OffsetClassCache {
    fn lookup(&self, offset: NonZeroU64) -> Option<OffsetClass> {
        let mask = self.slots.len().saturating_sub(1);
        let mut index = offset_slot(offset.get()) & mask;
        loop {
            let slot = self.slots[index];
            if slot.offset == 0 {
                return None;
            }
            if slot.offset == offset.get() {
                return Some(OffsetClass::from_raw(slot.class));
            }
            index = (index + 1) & mask;
        }
    }

    fn insert(&mut self, offset: NonZeroU64, class: OffsetClass) {
        if (self.len + 1).saturating_mul(4) >= self.slots.len().saturating_mul(3) {
            self.grow();
        }
        self.insert_raw(offset.get(), class.to_raw());
    }

    fn grow(&mut self) {
        let new_len = self.slots.len().saturating_mul(2).max(256);
        let old = std::mem::replace(&mut self.slots, vec![OffsetClassSlot::default(); new_len]);
        self.len = 0;
        for slot in old {
            if slot.offset != 0 {
                self.insert_raw(slot.offset, slot.class);
            }
        }
    }

    fn insert_raw(&mut self, offset: u64, class: usize) {
        let mask = self.slots.len().saturating_sub(1);
        let mut index = offset_slot(offset) & mask;
        loop {
            if self.slots[index].offset == 0 {
                self.slots[index] = OffsetClassSlot { offset, class };
                self.len += 1;
                return;
            }
            if self.slots[index].offset == offset {
                self.slots[index].class = class;
                return;
            }
            index = (index + 1) & mask;
        }
    }
}

fn offset_slot(offset: u64) -> usize {
    let mut value = offset >> 3;
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51afd7ed558ccd);
    value ^= value >> 33;
    value as usize
}

struct ExplorerAccumulator {
    field_lookup: HashMap<Vec<u8>, usize>,
    fields: Vec<Vec<u8>>,
    flags: Vec<u8>,
    last_seen_row_ids: Vec<u64>,
    unset_counts: Vec<u64>,
    values_by_field: Vec<Vec<usize>>,
    value_counts: Vec<u64>,
    value_field_indices: Vec<usize>,
    value_labels: Vec<Vec<u8>>,
    value_fts_matches: Vec<bool>,
    value_source_realtime: Vec<Option<u64>>,
    value_histogram_buckets: Vec<Option<Vec<u64>>>,
    offset_cache: OffsetClassCache,
    histogram_start_realtime_usec: u64,
    histogram_bucket_width_usec: u64,
    histogram_bucket_count: usize,
    required_identity_count: usize,
}

impl ExplorerAccumulator {
    fn for_main(query: &ExplorerQuery, histogram: Option<&ExplorerHistogram>) -> Self {
        let mut out = Self::new(histogram);
        if let Some(field) = &query.histogram {
            out.add_field(field, FACET_HISTOGRAM);
        }
        if query_needs_source_realtime_main(query) {
            out.add_field(SOURCE_REALTIME_FIELD, FACET_SOURCE_REALTIME);
        }
        out
    }

    fn for_facets(
        query: &ExplorerQuery,
        facet_indices: &[usize],
        include_source_realtime: bool,
    ) -> Self {
        let mut out = Self::new(None);
        for facet_index in facet_indices {
            if let Some(field) = query.facets.get(*facet_index) {
                out.add_field(field, FACET_PUBLIC);
            }
        }
        if include_source_realtime {
            out.add_field(SOURCE_REALTIME_FIELD, FACET_SOURCE_REALTIME);
        }
        out
    }

    fn new(histogram: Option<&ExplorerHistogram>) -> Self {
        Self {
            field_lookup: HashMap::new(),
            fields: Vec::new(),
            flags: Vec::new(),
            last_seen_row_ids: Vec::new(),
            unset_counts: Vec::new(),
            values_by_field: Vec::new(),
            value_counts: Vec::new(),
            value_field_indices: Vec::new(),
            value_labels: Vec::new(),
            value_fts_matches: Vec::new(),
            value_source_realtime: Vec::new(),
            value_histogram_buckets: Vec::new(),
            offset_cache: OffsetClassCache::default(),
            histogram_start_realtime_usec: histogram
                .and_then(|histogram| histogram.buckets.first())
                .map(|bucket| bucket.start_realtime_usec)
                .unwrap_or_default(),
            histogram_bucket_width_usec: histogram
                .and_then(|histogram| histogram.buckets.first())
                .map(|bucket| {
                    bucket
                        .end_realtime_usec
                        .saturating_sub(bucket.start_realtime_usec)
                        .max(1)
                })
                .unwrap_or(1),
            histogram_bucket_count: histogram
                .map(|histogram| histogram.buckets.len())
                .unwrap_or_default(),
            required_identity_count: 0,
        }
    }

    fn add_field(&mut self, field: &[u8], flags: u8) {
        if let Some(index) = self.field_lookup.get(field).copied() {
            let had_required = self.flags[index] != 0;
            self.flags[index] |= flags;
            if !had_required && self.flags[index] != 0 {
                self.required_identity_count += 1;
            }
            return;
        }

        let index = self.fields.len();
        self.field_lookup.insert(field.to_vec(), index);
        self.fields.push(field.to_vec());
        self.flags.push(flags);
        self.last_seen_row_ids.push(0);
        self.unset_counts.push(0);
        self.values_by_field.push(Vec::new());
        if flags != 0 {
            self.required_identity_count += 1;
        }
    }

    fn add_value(
        &mut self,
        field_index: usize,
        _data_offset: NonZeroU64,
        value: &[u8],
        fts_matches: bool,
    ) -> usize {
        let value_index = self.value_counts.len();
        let flags = self.flags[field_index];
        self.value_counts.push(0);
        self.value_field_indices.push(field_index);
        self.value_labels.push(value.to_vec());
        self.value_fts_matches.push(fts_matches);
        self.value_source_realtime
            .push(if flags & FACET_SOURCE_REALTIME != 0 {
                parse_source_realtime(value)
            } else {
                None
            });
        self.value_histogram_buckets
            .push((flags & FACET_HISTOGRAM != 0).then(|| vec![0; self.histogram_bucket_count]));
        self.values_by_field[field_index].push(value_index);
        value_index
    }

    fn mark_field_seen(&mut self, field_index: usize, row_id: u64) -> bool {
        // Duplicate values for one field must not satisfy another required
        // field identity in first-value mode.
        if self.last_seen_row_ids[field_index] == row_id {
            return false;
        }
        self.last_seen_row_ids[field_index] = row_id;
        true
    }

    fn apply_value(
        &mut self,
        value_index: usize,
        realtime_usec: Option<u64>,
        stats: &mut ExplorerStats,
    ) {
        let field_index = self.value_field_indices[value_index];
        let flags = self.flags[field_index];
        if flags & FACET_PUBLIC != 0 {
            self.value_counts[value_index] = self.value_counts[value_index].saturating_add(1);
            stats.facet_updates = stats.facet_updates.saturating_add(1);
        }
        if flags & FACET_HISTOGRAM != 0 {
            if let (Some(realtime_usec), Some(buckets)) = (
                realtime_usec,
                self.value_histogram_buckets[value_index].as_mut(),
            ) {
                if let Some(bucket_index) = histogram_bucket_index_from_bounds(
                    realtime_usec,
                    self.histogram_start_realtime_usec,
                    self.histogram_bucket_width_usec,
                    buckets.len(),
                ) {
                    buckets[bucket_index] = buckets[bucket_index].saturating_add(1);
                    stats.histogram_updates = stats.histogram_updates.saturating_add(1);
                }
            }
        }
    }

    fn finish_facet_row(&mut self, row_id: u64, stats: &mut ExplorerStats) {
        for field_index in 0..self.fields.len() {
            if self.flags[field_index] & FACET_PUBLIC == 0 {
                continue;
            }
            if self.last_seen_row_ids[field_index] != row_id {
                self.unset_counts[field_index] = self.unset_counts[field_index].saturating_add(1);
                stats.facet_updates = stats.facet_updates.saturating_add(1);
            }
        }
    }

    fn finish_facets(self, result: &mut ExplorerResult) {
        for field_index in 0..self.fields.len() {
            if self.flags[field_index] & FACET_PUBLIC == 0 {
                continue;
            }
            let mut values = HashMap::new();
            for value_index in &self.values_by_field[field_index] {
                let count = self.value_counts[*value_index];
                if count != 0 {
                    increment_counter_by(&mut values, &self.value_labels[*value_index], count);
                }
            }
            if self.unset_counts[field_index] != 0 {
                increment_counter_by(&mut values, UNSET_VALUE, self.unset_counts[field_index]);
            }
            result
                .facets
                .insert(self.fields[field_index].clone(), values);
        }
    }

    fn finish_histogram(self, histogram: Option<&mut ExplorerHistogram>) {
        let Some(histogram) = histogram else {
            return;
        };
        for value_index in 0..self.value_histogram_buckets.len() {
            let Some(buckets) = &self.value_histogram_buckets[value_index] else {
                continue;
            };
            for (bucket_index, count) in buckets.iter().enumerate() {
                if *count == 0 {
                    continue;
                }
                if let Some(bucket) = histogram.buckets.get_mut(bucket_index) {
                    increment_counter_by(
                        &mut bucket.values,
                        &self.value_labels[value_index],
                        *count,
                    );
                }
            }
        }
    }
}

impl FileReader {
    pub fn explore(&mut self, query: &ExplorerQuery) -> Result<ExplorerResult> {
        self.explore_with_strategy(query, ExplorerStrategy::Traversal)
    }

    pub fn explore_with_strategy(
        &mut self,
        query: &ExplorerQuery,
        strategy: ExplorerStrategy,
    ) -> Result<ExplorerResult> {
        match strategy {
            ExplorerStrategy::Traversal => self.explore_traversal(query),
            ExplorerStrategy::Index => self.explore_indexed(query),
            ExplorerStrategy::Compare => self.explore_compare(query),
        }
    }

    fn explore_traversal(&mut self, query: &ExplorerQuery) -> Result<ExplorerResult> {
        validate_query(query)?;

        let mut result = ExplorerResult {
            histogram: query
                .histogram
                .as_ref()
                .map(|field| new_histogram(field, query)),
            ..ExplorerResult::default()
        };

        if query_needs_main_pass(query) {
            self.configure_explorer_filters(query, None)?;
            let mut accumulator = ExplorerAccumulator::for_main(query, result.histogram.as_ref());
            self.scan_explorer_main(query, &mut accumulator, &mut result)?;
            accumulator.finish_histogram(result.histogram.as_mut());
        }

        for group in facet_pass_groups(query) {
            self.configure_explorer_filters(query, group.excluded_field.as_deref())?;
            let mut accumulator = ExplorerAccumulator::for_facets(
                query,
                &group.facet_indices,
                facet_pass_needs_source_realtime(query),
            );
            self.scan_explorer_facet(query, &mut accumulator, &mut result.stats)?;
            accumulator.finish_facets(&mut result);
        }

        self.configure_explorer_filters(query, None)?;
        Ok(result)
    }

    fn explore_compare(&mut self, query: &ExplorerQuery) -> Result<ExplorerResult> {
        let traversal_started = Instant::now();
        let traversal = self.explore_traversal(query)?;
        let traversal_duration = traversal_started.elapsed();

        let index_started = Instant::now();
        let mut indexed = self.explore_indexed(query)?;
        let index_duration = index_started.elapsed();

        if !explorer_outputs_match(&traversal, &indexed) {
            return Err(SdkError::VerificationError(
                "indexed explorer output differs from traversal explorer output".to_string(),
            ));
        }
        indexed.comparison = Some(ExplorerComparison {
            traversal_duration,
            index_duration,
            traversal_stats: traversal.stats,
            index_stats: indexed.stats.clone(),
        });
        Ok(indexed)
    }

    fn explore_indexed(&mut self, query: &ExplorerQuery) -> Result<ExplorerResult> {
        validate_query(query)?;
        validate_indexed_query(query)?;

        let mut result = ExplorerResult {
            histogram: query
                .histogram
                .as_ref()
                .map(|field| new_histogram(field, query)),
            ..ExplorerResult::default()
        };

        if query.limit > 0 {
            let mut row_query = query.clone();
            row_query.facets.clear();
            row_query.histogram = None;
            self.configure_explorer_filters(&row_query, None)?;
            let mut accumulator = ExplorerAccumulator::for_main(&row_query, None);
            self.scan_explorer_main(&row_query, &mut accumulator, &mut result)?;
        }

        for group in facet_pass_groups(query) {
            let candidates = self.indexed_candidate_set(query, group.excluded_field.as_deref())?;
            self.inner.with_file(|file| {
                indexed_count_facet_group(file, query, &group, &candidates, &mut result)
            })?;
        }

        if query.histogram.is_some() {
            let candidates = self.indexed_candidate_set(query, None)?;
            self.inner
                .with_file(|file| indexed_count_histogram(file, query, &candidates, &mut result))?;
        }

        self.configure_explorer_filters(query, None)?;
        Ok(result)
    }

    fn indexed_candidate_set(
        &mut self,
        query: &ExplorerQuery,
        excluded_field: Option<&[u8]>,
    ) -> Result<IndexedCandidateSet> {
        if query.filters.is_empty()
            && query.after_realtime_usec.is_none()
            && query.before_realtime_usec.is_none()
        {
            let count = self
                .inner
                .with_file(|file| file.journal_header_ref().n_entries);
            return Ok(IndexedCandidateSet::All { count });
        }

        self.configure_explorer_filters(query, excluded_field)?;
        self.seek_for_explorer(query);
        let mut offsets = HashSet::new();
        while self.step_for_explorer(query.direction)? {
            let Some(metadata) = self.row.metadata() else {
                continue;
            };
            let commit_realtime = metadata.realtime;
            if stop_by_commit_time(query, commit_realtime) {
                break;
            }
            if !timestamp_in_range(query, commit_realtime) {
                continue;
            }
            if let Some(entry_offset) = self.row.entry_offset() {
                offsets.insert(entry_offset);
            }
        }
        Ok(IndexedCandidateSet::Set {
            count: offsets.len() as u64,
            offsets,
        })
    }

    fn configure_explorer_filters(
        &mut self,
        query: &ExplorerQuery,
        excluded_field: Option<&[u8]>,
    ) -> Result<()> {
        self.flush_matches();
        for filter in &query.filters {
            if excluded_field.is_some_and(|field| field == filter.field.as_slice()) {
                continue;
            }
            if filter.values.is_empty() {
                continue;
            }
            for value in &filter.values {
                let payload = payload_from_parts(&filter.field, value);
                self.add_match(&payload);
            }
        }
        Ok(())
    }

    fn scan_explorer_main(
        &mut self,
        query: &ExplorerQuery,
        accumulator: &mut ExplorerAccumulator,
        result: &mut ExplorerResult,
    ) -> Result<()> {
        self.seek_for_explorer(query);
        let mut row_id = 0u64;
        let mut deferred_values = Vec::new();
        while self.step_for_explorer(query.direction)? {
            let Some(metadata) = self.row.metadata() else {
                continue;
            };
            let commit_realtime = metadata.realtime;
            if stop_by_commit_time(query, commit_realtime) {
                break;
            }

            let scan = if accumulator.required_identity_count == 0 && query.fts_patterns.is_empty()
            {
                result.stats.rows_examined = result.stats.rows_examined.saturating_add(1);
                RowScan::default()
            } else {
                row_id = row_id.saturating_add(1);
                deferred_values.clear();
                self.scan_current_row(
                    query,
                    accumulator,
                    row_id,
                    ScanApply::Deferred(&mut deferred_values),
                    &mut result.stats,
                )?
            };
            let effective_realtime = scan.timestamp.unwrap_or(commit_realtime);
            if !timestamp_in_range(query, effective_realtime) {
                continue;
            }
            if !query.fts_patterns.is_empty() && !scan.fts_matches {
                continue;
            }

            result.stats.rows_matched = result.stats.rows_matched.saturating_add(1);
            for value_index in &deferred_values {
                accumulator.apply_value(*value_index, Some(effective_realtime), &mut result.stats);
            }
            if result.rows.len() < query.limit {
                result
                    .rows
                    .push(self.expand_current_explorer_row(effective_realtime, &mut result.stats)?);
            }
        }
        result.stats.rows_returned = result.rows.len() as u64;
        Ok(())
    }

    fn scan_explorer_facet(
        &mut self,
        query: &ExplorerQuery,
        accumulator: &mut ExplorerAccumulator,
        stats: &mut ExplorerStats,
    ) -> Result<()> {
        self.seek_for_explorer(query);
        let defer_apply = query.after_realtime_usec.is_some()
            || query.before_realtime_usec.is_some()
            || !query.fts_patterns.is_empty();
        let mut row_id = 0u64;
        let mut deferred_values = Vec::new();
        while self.step_for_explorer(query.direction)? {
            let Some(metadata) = self.row.metadata() else {
                continue;
            };
            let commit_realtime = metadata.realtime;
            if stop_by_commit_time(query, commit_realtime) {
                break;
            }

            row_id = row_id.saturating_add(1);
            deferred_values.clear();
            let scan = if defer_apply {
                self.scan_current_row(
                    query,
                    accumulator,
                    row_id,
                    ScanApply::Deferred(&mut deferred_values),
                    stats,
                )?
            } else {
                self.scan_current_row(query, accumulator, row_id, ScanApply::Immediate, stats)?
            };
            let effective_realtime = scan.timestamp.unwrap_or(commit_realtime);
            if !timestamp_in_range(query, effective_realtime) {
                continue;
            }
            if !query.fts_patterns.is_empty() && !scan.fts_matches {
                continue;
            }

            stats.facet_rows_matched = stats.facet_rows_matched.saturating_add(1);
            if defer_apply {
                for value_index in &deferred_values {
                    accumulator.apply_value(*value_index, None, stats);
                }
            }
            accumulator.finish_facet_row(row_id, stats);
        }
        Ok(())
    }

    fn scan_current_row(
        &mut self,
        query: &ExplorerQuery,
        accumulator: &mut ExplorerAccumulator,
        row_id: u64,
        mut apply: ScanApply<'_>,
        stats: &mut ExplorerStats,
    ) -> Result<RowScan> {
        stats.rows_examined = stats.rows_examined.saturating_add(1);
        let mut out = RowScan::default();
        let use_first_value = query.field_mode == ExplorerFieldMode::FirstValue;
        let needs_fts = !query.fts_patterns.is_empty();
        let mut fields_missing_from_row = if use_first_value {
            accumulator.required_identity_count
        } else {
            0
        };

        let inner = &mut self.inner;
        let row = &mut self.row;
        inner.with_mut(|fields| {
            fields.reader.release_object_guards();
            row.restart_data()?;
            let result = (|| {
                for index in 0..row.data_offset_count() {
                    let Some(data_offset) = row.data_offset_at(index) else {
                        break;
                    };
                    stats.data_refs_seen = stats.data_refs_seen.saturating_add(1);
                    let class = classify_data_for_accumulator(
                        fields.file,
                        row,
                        data_offset,
                        accumulator,
                        needs_fts,
                        query,
                        stats,
                    )?;

                    match class {
                        OffsetClass::Irrelevant => {
                            stats.data_refs_skipped = stats.data_refs_skipped.saturating_add(1);
                            continue;
                        }
                        OffsetClass::FtsMatch => {
                            out.fts_matches = true;
                            continue;
                        }
                        OffsetClass::Value(value_index) => {
                            if accumulator.value_fts_matches[value_index] {
                                out.fts_matches = true;
                            }
                            let field_index = accumulator.value_field_indices[value_index];
                            let first_for_field = if use_first_value
                                || accumulator.flags[field_index] & FACET_PUBLIC != 0
                            {
                                accumulator.mark_field_seen(field_index, row_id)
                            } else {
                                true
                            };
                            if use_first_value && first_for_field {
                                fields_missing_from_row = fields_missing_from_row.saturating_sub(1);
                            }
                            if !use_first_value || first_for_field {
                                if let Some(timestamp) =
                                    accumulator.value_source_realtime[value_index]
                                {
                                    out.timestamp = Some(timestamp);
                                }
                                match &mut apply {
                                    ScanApply::Immediate => {
                                        accumulator.apply_value(value_index, None, stats)
                                    }
                                    ScanApply::Deferred(values) => values.push(value_index),
                                }
                            }
                        }
                    }

                    if use_first_value && !needs_fts && fields_missing_from_row == 0 {
                        stats.early_stop_opportunities =
                            stats.early_stop_opportunities.saturating_add(1);
                        stats.early_stops = stats.early_stops.saturating_add(1);
                        break;
                    }
                }
                Ok::<_, SdkError>(())
            })();
            row.reset_data_state(fields.file)?;
            result
        })?;
        Ok(out)
    }

    fn seek_for_explorer(&mut self, query: &ExplorerQuery) {
        match query.direction {
            Direction::Forward => match query.anchor {
                ExplorerAnchor::Auto => {
                    if let Some(after) = query.after_realtime_usec {
                        self.seek_realtime(after.saturating_sub(query.realtime_slack_usec));
                    } else {
                        self.seek_head();
                    }
                }
                ExplorerAnchor::Realtime(usec) => self.seek_realtime(usec),
                ExplorerAnchor::Tail => self.seek_tail(),
                ExplorerAnchor::Head => {
                    if let Some(after) = query.after_realtime_usec {
                        self.seek_realtime(after.saturating_sub(query.realtime_slack_usec));
                    } else {
                        self.seek_head();
                    }
                }
            },
            Direction::Backward => match query.anchor {
                ExplorerAnchor::Auto => {
                    if let Some(before) = query.before_realtime_usec {
                        self.seek_realtime(before.saturating_add(query.realtime_slack_usec));
                    } else {
                        self.seek_tail();
                    }
                }
                ExplorerAnchor::Realtime(usec) => self.seek_realtime(usec),
                ExplorerAnchor::Head => self.seek_head(),
                ExplorerAnchor::Tail => {
                    if let Some(before) = query.before_realtime_usec {
                        self.seek_realtime(before.saturating_add(query.realtime_slack_usec));
                    } else {
                        self.seek_tail();
                    }
                }
            },
        }
    }

    fn step_for_explorer(&mut self, direction: Direction) -> Result<bool> {
        match direction {
            Direction::Forward => self.next(),
            Direction::Backward => self.previous(),
        }
    }

    fn expand_current_explorer_row(
        &mut self,
        realtime_usec: u64,
        stats: &mut ExplorerStats,
    ) -> Result<ExplorerRow> {
        let cursor = self.get_cursor()?;
        let mut payloads = Vec::new();
        self.collect_entry_payloads(&mut payloads)?;
        stats.returned_row_expansions = stats.returned_row_expansions.saturating_add(1);
        Ok(ExplorerRow {
            realtime_usec,
            cursor,
            payloads,
        })
    }
}

enum ScanApply<'a> {
    Immediate,
    Deferred(&'a mut Vec<usize>),
}

enum IndexedCandidateSet {
    All {
        count: u64,
    },
    Set {
        count: u64,
        offsets: HashSet<NonZeroU64>,
    },
}

impl IndexedCandidateSet {
    fn count(&self) -> u64 {
        match self {
            Self::All { count } | Self::Set { count, .. } => *count,
        }
    }

    fn contains(&self, entry_offset: NonZeroU64) -> bool {
        match self {
            Self::All { .. } => true,
            Self::Set { offsets, .. } => offsets.contains(&entry_offset),
        }
    }
}

struct FacetPassGroup {
    excluded_field: Option<Vec<u8>>,
    facet_indices: Vec<usize>,
}

fn facet_pass_groups(query: &ExplorerQuery) -> Vec<FacetPassGroup> {
    let filter_fields: HashSet<&[u8]> = query
        .filters
        .iter()
        .map(|filter| filter.field.as_slice())
        .collect();
    let mut groups: Vec<FacetPassGroup> = Vec::new();

    for (index, facet) in query.facets.iter().enumerate() {
        let excluded_field = filter_fields
            .contains(facet.as_slice())
            .then(|| facet.clone());
        if let Some(existing) = groups
            .iter_mut()
            .find(|group| group.excluded_field.as_deref() == excluded_field.as_deref())
        {
            existing.facet_indices.push(index);
        } else {
            groups.push(FacetPassGroup {
                excluded_field,
                facet_indices: vec![index],
            });
        }
    }

    groups
}

fn indexed_count_facet_group(
    file: &JournalFile<Mmap>,
    query: &ExplorerQuery,
    group: &FacetPassGroup,
    candidates: &IndexedCandidateSet,
    result: &mut ExplorerResult,
) -> Result<()> {
    result.stats.facet_rows_matched = result
        .stats
        .facet_rows_matched
        .saturating_add(candidates.count());

    for facet_index in &group.facet_indices {
        let Some(field) = query.facets.get(*facet_index) else {
            continue;
        };
        let mut values = HashMap::new();
        let mut rows_with_field = HashSet::new();
        let mut decompressed = Vec::new();

        for item in file.field_data_objects_with_offsets(field)? {
            let (_, data) = item?;
            let Some((value, cursor)) =
                indexed_value_and_cursor(&data, field, &mut decompressed, &mut result.stats)?
            else {
                continue;
            };
            drop(data);

            let count = indexed_count_facet_entries(
                file,
                cursor,
                candidates,
                &mut rows_with_field,
                &mut result.stats,
            )?;
            if count == 0 {
                continue;
            }
            increment_counter_by(&mut values, &value, count);
            result.stats.facet_updates = result.stats.facet_updates.saturating_add(count);
        }

        let unset = candidates
            .count()
            .saturating_sub(rows_with_field.len() as u64);
        if unset != 0 {
            increment_counter_by(&mut values, UNSET_VALUE, unset);
            result.stats.facet_updates = result.stats.facet_updates.saturating_add(unset);
        }
        result.facets.insert(field.clone(), values);
    }

    Ok(())
}

fn indexed_count_histogram(
    file: &JournalFile<Mmap>,
    query: &ExplorerQuery,
    candidates: &IndexedCandidateSet,
    result: &mut ExplorerResult,
) -> Result<()> {
    let Some(histogram) = result.histogram.as_mut() else {
        return Ok(());
    };
    let field = histogram.field.clone();
    let mut decompressed = Vec::new();

    for item in file.field_data_objects_with_offsets(&field)? {
        let (_, data) = item?;
        let Some((value, cursor)) =
            indexed_value_and_cursor(&data, &field, &mut decompressed, &mut result.stats)?
        else {
            continue;
        };
        drop(data);

        indexed_count_histogram_entries(
            file,
            cursor,
            candidates,
            &value,
            histogram,
            query,
            &mut result.stats,
        )?;
    }

    Ok(())
}

fn indexed_value_and_cursor(
    data: &DataObject<&[u8]>,
    field: &[u8],
    decompressed: &mut Vec<u8>,
    stats: &mut ExplorerStats,
) -> Result<Option<(Vec<u8>, Option<InlinedCursor>)>> {
    stats.data_objects_classified = stats.data_objects_classified.saturating_add(1);
    stats.data_payloads_loaded = stats.data_payloads_loaded.saturating_add(1);
    let payload = if data.is_compressed() {
        decompressed.clear();
        let len = data.decompress(decompressed)?;
        stats.payloads_decompressed = stats.payloads_decompressed.saturating_add(1);
        &decompressed[..len]
    } else {
        data.raw_payload()
    };

    let Some((payload_field, value)) = split_payload_bytes(payload) else {
        return Ok(None);
    };
    if payload_field != field {
        return Ok(None);
    }
    Ok(Some((value.to_vec(), data.inlined_cursor())))
}

fn indexed_count_facet_entries(
    file: &JournalFile<Mmap>,
    cursor: Option<InlinedCursor>,
    candidates: &IndexedCandidateSet,
    rows_with_field: &mut HashSet<NonZeroU64>,
    stats: &mut ExplorerStats,
) -> Result<u64> {
    let mut count = 0u64;
    indexed_visit_entries(file, cursor, |entry_offset| {
        stats.data_refs_seen = stats.data_refs_seen.saturating_add(1);
        if candidates.contains(entry_offset) {
            count = count.saturating_add(1);
            rows_with_field.insert(entry_offset);
        }
        Ok(())
    })?;
    Ok(count)
}

fn indexed_count_histogram_entries(
    file: &JournalFile<Mmap>,
    cursor: Option<InlinedCursor>,
    candidates: &IndexedCandidateSet,
    value: &[u8],
    histogram: &mut ExplorerHistogram,
    query: &ExplorerQuery,
    stats: &mut ExplorerStats,
) -> Result<()> {
    let histogram_start = histogram
        .buckets
        .first()
        .map(|bucket| bucket.start_realtime_usec)
        .unwrap_or_default();
    let histogram_bucket_width = histogram
        .buckets
        .first()
        .map(|bucket| {
            bucket
                .end_realtime_usec
                .saturating_sub(bucket.start_realtime_usec)
                .max(1)
        })
        .unwrap_or(1);
    let histogram_bucket_count = histogram.buckets.len();

    indexed_visit_entries(file, cursor, |entry_offset| {
        stats.data_refs_seen = stats.data_refs_seen.saturating_add(1);
        if !candidates.contains(entry_offset) {
            return Ok(());
        }
        let entry = file.entry_ref(entry_offset)?;
        let realtime = entry.header.realtime;
        drop(entry);
        if !timestamp_in_range(query, realtime) {
            return Ok(());
        }
        let Some(bucket_index) = histogram_bucket_index_from_bounds(
            realtime,
            histogram_start,
            histogram_bucket_width,
            histogram_bucket_count,
        ) else {
            return Ok(());
        };
        if let Some(bucket) = histogram.buckets.get_mut(bucket_index) {
            increment_counter_by(&mut bucket.values, value, 1);
            stats.histogram_updates = stats.histogram_updates.saturating_add(1);
        }
        Ok(())
    })
}

fn indexed_visit_entries<F>(
    file: &JournalFile<Mmap>,
    cursor: Option<InlinedCursor>,
    mut visitor: F,
) -> Result<()>
where
    F: FnMut(NonZeroU64) -> Result<()>,
{
    let Some(mut cursor) = cursor.map(|cursor| cursor.head()) else {
        return Ok(());
    };
    let mut needle = NonZeroU64::MIN;
    while let Some(entry_offset) = cursor.next_until(file, needle)? {
        visitor(entry_offset)?;
        let Some(next) = entry_offset.get().checked_add(1).and_then(NonZeroU64::new) else {
            break;
        };
        needle = next;
    }
    Ok(())
}

fn classify_data_for_accumulator(
    file: &JournalFile<Mmap>,
    row: &mut CurrentRowView,
    data_offset: NonZeroU64,
    accumulator: &mut ExplorerAccumulator,
    needs_fts: bool,
    query: &ExplorerQuery,
    stats: &mut ExplorerStats,
) -> Result<OffsetClass> {
    if let Some(class) = accumulator.offset_cache.lookup(data_offset) {
        stats.data_cache_hits = stats.data_cache_hits.saturating_add(1);
        return Ok(class);
    }

    stats.data_cache_misses = stats.data_cache_misses.saturating_add(1);
    stats.data_payloads_loaded = stats.data_payloads_loaded.saturating_add(1);
    let was_compressed = file.data_ref(data_offset)?.is_compressed();
    let payload = row.read_payload_at(file, data_offset)?;
    if was_compressed {
        stats.payloads_decompressed = stats.payloads_decompressed.saturating_add(1);
    }
    let payload = row.payload_slice(payload);
    let Some((field, value)) = split_payload_bytes(payload) else {
        let class = classify_unstructured_payload(payload, needs_fts, query, stats);
        accumulator.offset_cache.insert(data_offset, class);
        stats.data_objects_classified = stats.data_objects_classified.saturating_add(1);
        return Ok(class);
    };

    let fts_matches = if needs_fts {
        stats.fts_scans = stats.fts_scans.saturating_add(1);
        matches_fts(value, &query.fts_patterns)
    } else {
        false
    };

    let class = if let Some(field_index) = accumulator.field_lookup.get(field).copied() {
        OffsetClass::Value(accumulator.add_value(field_index, data_offset, value, fts_matches))
    } else if fts_matches {
        OffsetClass::FtsMatch
    } else {
        OffsetClass::Irrelevant
    };

    accumulator.offset_cache.insert(data_offset, class);
    stats.data_objects_classified = stats.data_objects_classified.saturating_add(1);
    Ok(class)
}

fn classify_unstructured_payload(
    payload: &[u8],
    needs_fts: bool,
    query: &ExplorerQuery,
    stats: &mut ExplorerStats,
) -> OffsetClass {
    if !needs_fts {
        return OffsetClass::Irrelevant;
    }
    stats.fts_scans = stats.fts_scans.saturating_add(1);
    if matches_fts(payload, &query.fts_patterns) {
        OffsetClass::FtsMatch
    } else {
        OffsetClass::Irrelevant
    }
}

fn histogram_bucket_index_from_bounds(
    realtime_usec: u64,
    start_realtime_usec: u64,
    bucket_width_usec: u64,
    bucket_count: usize,
) -> Option<usize> {
    if bucket_count == 0 {
        return None;
    }
    realtime_usec
        .saturating_sub(start_realtime_usec)
        .checked_div(bucket_width_usec.max(1))
        .map(|index| (index as usize).min(bucket_count - 1))
}

fn validate_query(query: &ExplorerQuery) -> Result<()> {
    if query
        .after_realtime_usec
        .zip(query.before_realtime_usec)
        .is_some_and(|(after, before)| after > before)
    {
        return Err(SdkError::InvalidPath(
            "after_realtime_usec must be <= before_realtime_usec".to_string(),
        ));
    }
    for filter in &query.filters {
        if filter.field.is_empty() || filter.field.contains(&b'=') {
            return Err(SdkError::InvalidPath(
                "filter field must be non-empty and must not contain '='".to_string(),
            ));
        }
    }
    for field in query.facets.iter().chain(query.histogram.iter()) {
        if field.is_empty() || field.contains(&b'=') {
            return Err(SdkError::InvalidPath(
                "facet and histogram fields must be non-empty and must not contain '='".to_string(),
            ));
        }
    }
    let mut seen_facets: HashSet<&[u8]> = HashSet::new();
    for facet in &query.facets {
        if !seen_facets.insert(facet) {
            return Err(SdkError::InvalidPath(
                "facet fields must not be duplicated".to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_indexed_query(query: &ExplorerQuery) -> Result<()> {
    if query.field_mode != ExplorerFieldMode::AllValues {
        return Err(SdkError::Unsupported(
            "indexed explorer strategy requires ExplorerFieldMode::AllValues",
        ));
    }
    if !query.fts_patterns.is_empty() {
        return Err(SdkError::Unsupported(
            "indexed explorer strategy does not support FTS",
        ));
    }
    if query.use_source_realtime
        && (query.after_realtime_usec.is_some()
            || query.before_realtime_usec.is_some()
            || query.histogram.is_some())
    {
        return Err(SdkError::Unsupported(
            "indexed explorer strategy requires commit realtime for time-bounded facets and histograms",
        ));
    }
    Ok(())
}

fn explorer_outputs_match(left: &ExplorerResult, right: &ExplorerResult) -> bool {
    if left.rows.len() != right.rows.len() {
        return false;
    }
    if left.rows.iter().zip(&right.rows).any(|(left, right)| {
        left.realtime_usec != right.realtime_usec
            || left.cursor != right.cursor
            || left.payloads != right.payloads
    }) {
        return false;
    }
    if left.facets != right.facets {
        return false;
    }
    explorer_histograms_match(left.histogram.as_ref(), right.histogram.as_ref())
}

fn explorer_histograms_match(
    left: Option<&ExplorerHistogram>,
    right: Option<&ExplorerHistogram>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            left.field == right.field
                && left.buckets.len() == right.buckets.len()
                && left
                    .buckets
                    .iter()
                    .zip(&right.buckets)
                    .all(|(left, right)| {
                        left.start_realtime_usec == right.start_realtime_usec
                            && left.end_realtime_usec == right.end_realtime_usec
                            && left.values == right.values
                    })
        }
        _ => false,
    }
}

fn query_needs_source_realtime_main(query: &ExplorerQuery) -> bool {
    query.use_source_realtime
        && (query.after_realtime_usec.is_some()
            || query.before_realtime_usec.is_some()
            || query.histogram.is_some()
            || query.limit > 0)
}

fn facet_pass_needs_source_realtime(query: &ExplorerQuery) -> bool {
    query.use_source_realtime
        && (query.after_realtime_usec.is_some() || query.before_realtime_usec.is_some())
}

fn query_needs_main_pass(query: &ExplorerQuery) -> bool {
    query.limit > 0 || query.histogram.is_some()
}

fn payload_from_parts(field: &[u8], value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(field.len() + 1 + value.len());
    out.extend_from_slice(field);
    out.push(b'=');
    out.extend_from_slice(value);
    out
}

fn split_payload_bytes(payload: &[u8]) -> Option<(&[u8], &[u8])> {
    let eq = payload.iter().position(|byte| *byte == b'=')?;
    Some((&payload[..eq], &payload[eq + 1..]))
}

fn parse_source_realtime(value: &[u8]) -> Option<u64> {
    std::str::from_utf8(value).ok()?.parse().ok()
}

fn matches_fts(value: &[u8], patterns: &[Vec<u8>]) -> bool {
    if patterns.is_empty() {
        return true;
    }
    patterns
        .iter()
        .filter(|pattern| !pattern.is_empty())
        .any(|pattern| contains_ascii_case_insensitive(value, pattern))
}

fn contains_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
    })
}

fn timestamp_in_range(query: &ExplorerQuery, timestamp: u64) -> bool {
    if query
        .after_realtime_usec
        .is_some_and(|after| timestamp < after)
    {
        return false;
    }
    if query
        .before_realtime_usec
        .is_some_and(|before| timestamp > before)
    {
        return false;
    }
    true
}

fn stop_by_commit_time(query: &ExplorerQuery, commit_realtime: u64) -> bool {
    match query.direction {
        Direction::Forward => query.before_realtime_usec.is_some_and(|before| {
            commit_realtime > before.saturating_add(query.realtime_slack_usec)
        }),
        Direction::Backward => query
            .after_realtime_usec
            .is_some_and(|after| commit_realtime < after.saturating_sub(query.realtime_slack_usec)),
    }
}

fn new_histogram(field: &[u8], query: &ExplorerQuery) -> ExplorerHistogram {
    let (start, end) = histogram_bounds(query);
    let bucket_count = query.histogram_target_buckets.max(1);
    let width = end
        .saturating_sub(start)
        .checked_div(bucket_count as u64)
        .unwrap_or(0)
        .max(1);
    let mut buckets = Vec::with_capacity(bucket_count);
    for index in 0..bucket_count {
        let bucket_start = start.saturating_add(width.saturating_mul(index as u64));
        let bucket_end = if index + 1 == bucket_count {
            end.saturating_add(1)
        } else {
            bucket_start.saturating_add(width)
        };
        buckets.push(ExplorerHistogramBucket {
            start_realtime_usec: bucket_start,
            end_realtime_usec: bucket_end,
            values: HashMap::new(),
        });
    }
    ExplorerHistogram {
        field: field.to_vec(),
        buckets,
    }
}

fn histogram_bounds(query: &ExplorerQuery) -> (u64, u64) {
    let start = query.after_realtime_usec.unwrap_or(0);
    let end = query
        .before_realtime_usec
        .unwrap_or_else(|| start.saturating_add(3_600_000_000));
    if end <= start {
        (start, start.saturating_add(1))
    } else {
        (start, end)
    }
}

fn increment_counter_by(map: &mut HashMap<Vec<u8>, u64>, value: &[u8], delta: u64) {
    if let Some(count) = map.get_mut(value) {
        *count = count.saturating_add(delta);
    } else {
        map.insert(value.to_vec(), delta);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use journal_core::file::{JournalFileOptions, JournalWriter, MmapMut};
    use journal_core::repository::File as RepoFile;
    use tempfile::TempDir;

    fn test_uuid(seed: u8) -> uuid::Uuid {
        uuid::Uuid::from_bytes([seed; 16])
    }

    fn create_writer(
        path: &std::path::Path,
        compression: Option<(Compression, usize)>,
    ) -> (JournalFile<MmapMut>, JournalWriter) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create journal parent");
        }
        let repo_file = RepoFile::from_path(path).expect("repo file");
        let mut options = JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3));
        if let Some((compression, threshold)) = compression {
            options = options
                .with_compression(compression)
                .with_compress_threshold(threshold);
        }
        let mut file = JournalFile::<MmapMut>::create(&repo_file, options).expect("create journal");
        let writer = if let Some((compression, threshold)) = compression {
            JournalWriter::new_with_compression(&mut file, 1, test_uuid(4), compression, threshold)
                .expect("writer")
        } else {
            JournalWriter::new(&mut file, 1, test_uuid(4)).expect("writer")
        };
        (file, writer)
    }

    fn write_entries(
        path: &std::path::Path,
        compression: Option<(Compression, usize)>,
        entries: &[(&[&[u8]], u64)],
    ) {
        let (mut file, mut writer) = create_writer(path, compression);
        for (payloads, realtime) in entries {
            writer
                .add_entry(&mut file, payloads, *realtime, *realtime)
                .expect("write entry");
        }
        file.sync().expect("sync journal");
    }

    #[test]
    fn explorer_filters_with_or_values_and_and_fields() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("filter.journal");
        write_entries(
            &path,
            None,
            &[
                (&[b"SERVICE=a", b"PRIORITY=3"], 1_000),
                (&[b"SERVICE=b", b"PRIORITY=3"], 2_000),
                (&[b"SERVICE=b", b"PRIORITY=4"], 3_000),
            ],
        );

        let mut reader = FileReader::open(&path).expect("open reader");
        let query = ExplorerQuery {
            filters: vec![
                ExplorerFilter::new(b"SERVICE".to_vec(), [b"a".to_vec(), b"b".to_vec()]),
                ExplorerFilter::new(b"PRIORITY".to_vec(), [b"3".to_vec()]),
            ],
            facets: vec![b"SERVICE".to_vec()],
            limit: 10,
            ..ExplorerQuery::default()
        };

        let result = reader.explore(&query).expect("explore");
        assert_eq!(result.rows.len(), 2);
        let service = result
            .facets
            .get(b"SERVICE".as_slice())
            .expect("service facet");
        assert_eq!(service.get(b"a".as_slice()), Some(&1));
        assert_eq!(service.get(b"b".as_slice()), Some(&1));
        assert!(result.stats.data_cache_misses > 0);
    }

    #[test]
    fn explorer_skips_irrelevant_compressed_data_for_facets() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("compressed.journal");
        let large_message = b"MESSAGE=abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
        write_entries(
            &path,
            Some((Compression::Zstd, 32)),
            &[(&[b"PRIORITY=3", large_message], 1_000)],
        );

        let mut reader = FileReader::open(&path).expect("open reader");
        let query = ExplorerQuery {
            facets: vec![b"PRIORITY".to_vec()],
            limit: 0,
            ..ExplorerQuery::default()
        };

        let result = reader.explore(&query).expect("explore");
        let priority = result
            .facets
            .get(b"PRIORITY".as_slice())
            .expect("priority facet");
        assert_eq!(priority.get(b"3".as_slice()), Some(&1));
        assert_eq!(result.stats.payloads_decompressed, 0);
        assert_eq!(result.stats.data_refs_seen, 1);
        assert_eq!(result.stats.early_stops, 1);
    }

    #[test]
    fn explorer_reuses_classified_data_objects() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("reuse.journal");
        write_entries(
            &path,
            None,
            &[
                (&[b"PRIORITY=3"], 1_000),
                (&[b"PRIORITY=3"], 2_000),
                (&[b"PRIORITY=3"], 3_000),
            ],
        );

        let mut reader = FileReader::open(&path).expect("open reader");
        let query = ExplorerQuery {
            facets: vec![b"PRIORITY".to_vec()],
            limit: 0,
            ..ExplorerQuery::default()
        };

        let result = reader.explore(&query).expect("explore");
        let priority = result
            .facets
            .get(b"PRIORITY".as_slice())
            .expect("priority facet");
        assert_eq!(priority.get(b"3".as_slice()), Some(&3));
        assert!(result.stats.data_cache_hits >= 2);
    }

    #[test]
    fn explorer_groups_facets_with_same_filter_set() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("grouped-facets.journal");
        write_entries(
            &path,
            None,
            &[
                (&[b"SERVICE=a", b"PRIORITY=3"], 1_000),
                (&[b"SERVICE=b", b"PRIORITY=4"], 2_000),
            ],
        );

        let mut reader = FileReader::open(&path).expect("open reader");
        let query = ExplorerQuery {
            facets: vec![b"SERVICE".to_vec(), b"PRIORITY".to_vec()],
            limit: 0,
            ..ExplorerQuery::default()
        };

        let result = reader.explore(&query).expect("explore");
        assert_eq!(result.stats.rows_examined, 2);
        assert_eq!(result.stats.facet_rows_matched, 2);
        assert_eq!(
            result
                .facets
                .get(b"SERVICE".as_slice())
                .and_then(|values| values.get(b"a".as_slice())),
            Some(&1)
        );
        assert_eq!(
            result
                .facets
                .get(b"PRIORITY".as_slice())
                .and_then(|values| values.get(b"4".as_slice())),
            Some(&1)
        );
    }

    #[test]
    fn explorer_same_field_filter_exclusion_counts_filtered_out_facet_values() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("same-field-filter-facet.journal");
        write_entries(
            &path,
            None,
            &[
                (&[b"SERVICE=a", b"PRIORITY=3"], 1_000),
                (&[b"SERVICE=b", b"PRIORITY=3"], 2_000),
                (&[b"SERVICE=a", b"PRIORITY=4"], 3_000),
            ],
        );

        let mut reader = FileReader::open(&path).expect("open reader");
        let query = ExplorerQuery {
            filters: vec![
                ExplorerFilter::new(b"SERVICE".to_vec(), [b"a".to_vec()]),
                ExplorerFilter::new(b"PRIORITY".to_vec(), [b"3".to_vec()]),
            ],
            facets: vec![b"SERVICE".to_vec(), b"PRIORITY".to_vec()],
            limit: 0,
            ..ExplorerQuery::default()
        };

        let result = reader.explore(&query).expect("explore");
        let service = result
            .facets
            .get(b"SERVICE".as_slice())
            .expect("service facet");
        assert_eq!(service.get(b"a".as_slice()), Some(&1));
        assert_eq!(service.get(b"b".as_slice()), Some(&1));

        let priority = result
            .facets
            .get(b"PRIORITY".as_slice())
            .expect("priority facet");
        assert_eq!(priority.get(b"3".as_slice()), Some(&1));
        assert_eq!(priority.get(b"4".as_slice()), Some(&1));
    }

    #[test]
    fn explorer_index_strategy_matches_traversal_for_all_values() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("indexed-all-values.journal");
        write_entries(
            &path,
            None,
            &[
                (&[b"SERVICE=a", b"PRIORITY=3", b"TAG=x"], 1_000),
                (&[b"SERVICE=b", b"PRIORITY=3", b"TAG=x"], 2_000),
                (&[b"SERVICE=a", b"PRIORITY=4", b"TAG=y", b"TAG=z"], 3_000),
                (&[b"PRIORITY=3"], 4_000),
            ],
        );

        let query = ExplorerQuery {
            after_realtime_usec: Some(0),
            before_realtime_usec: Some(5_000),
            filters: vec![ExplorerFilter::new(b"PRIORITY".to_vec(), [b"3".to_vec()])],
            facets: vec![b"SERVICE".to_vec(), b"TAG".to_vec()],
            histogram: Some(b"SERVICE".to_vec()),
            histogram_target_buckets: 2,
            limit: 2,
            field_mode: ExplorerFieldMode::AllValues,
            use_source_realtime: false,
            ..ExplorerQuery::default()
        };

        let mut reader = FileReader::open(&path).expect("open reader");
        let result = reader
            .explore_with_strategy(&query, ExplorerStrategy::Compare)
            .expect("compare");

        let comparison = result.comparison.as_ref().expect("comparison diagnostics");
        assert_eq!(comparison.index_stats, result.stats);
        assert_eq!(comparison.traversal_stats.rows_returned, 2);
        assert_eq!(comparison.index_stats.rows_returned, 2);

        assert_eq!(result.rows.len(), 2);
        let service = result
            .facets
            .get(b"SERVICE".as_slice())
            .expect("service facet");
        assert_eq!(service.get(b"a".as_slice()), Some(&1));
        assert_eq!(service.get(b"b".as_slice()), Some(&1));
        assert_eq!(service.get(UNSET_VALUE), Some(&1));
        let histogram = result.histogram.as_ref().expect("histogram");
        assert_eq!(histogram.buckets.len(), 2);
        assert_eq!(histogram.buckets[0].values.get(b"a".as_slice()), Some(&1));
        assert_eq!(histogram.buckets[0].values.get(b"b".as_slice()), Some(&1));
        assert!(histogram.buckets[1].values.is_empty());
    }

    #[test]
    fn explorer_index_strategy_preserves_same_field_filter_exclusion() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("indexed-same-field-filter.journal");
        write_entries(
            &path,
            None,
            &[
                (&[b"SERVICE=a", b"PRIORITY=3"], 1_000),
                (&[b"SERVICE=b", b"PRIORITY=3"], 2_000),
                (&[b"SERVICE=a", b"PRIORITY=4"], 3_000),
            ],
        );

        let query = ExplorerQuery {
            filters: vec![
                ExplorerFilter::new(b"SERVICE".to_vec(), [b"a".to_vec()]),
                ExplorerFilter::new(b"PRIORITY".to_vec(), [b"3".to_vec()]),
            ],
            facets: vec![b"SERVICE".to_vec(), b"PRIORITY".to_vec()],
            field_mode: ExplorerFieldMode::AllValues,
            use_source_realtime: false,
            ..ExplorerQuery::default()
        };

        let mut reader = FileReader::open(&path).expect("open reader");
        let result = reader
            .explore_with_strategy(&query, ExplorerStrategy::Compare)
            .expect("compare");
        let service = result
            .facets
            .get(b"SERVICE".as_slice())
            .expect("service facet");
        assert_eq!(service.get(b"a".as_slice()), Some(&1));
        assert_eq!(service.get(b"b".as_slice()), Some(&1));
    }

    #[test]
    fn explorer_index_strategy_rejects_first_value_semantics() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("indexed-first-value.journal");
        write_entries(&path, None, &[(&[b"TAG=one", b"TAG=two"], 1_000)]);

        let mut reader = FileReader::open(&path).expect("open reader");
        let err = reader
            .explore_with_strategy(
                &ExplorerQuery {
                    facets: vec![b"TAG".to_vec()],
                    field_mode: ExplorerFieldMode::FirstValue,
                    ..ExplorerQuery::default()
                },
                ExplorerStrategy::Index,
            )
            .expect_err("first-value index strategy should be rejected");

        assert!(matches!(err, SdkError::Unsupported(_)));
    }

    #[test]
    fn explorer_first_value_counts_one_value_per_selected_field() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("first-value.journal");
        write_entries(
            &path,
            None,
            &[(&[b"TAG=one", b"TAG=two", b"SERVICE=a"], 1_000)],
        );

        let mut all_values_reader = FileReader::open(&path).expect("open all-values reader");
        let all_values = all_values_reader
            .explore(&ExplorerQuery {
                facets: vec![b"TAG".to_vec()],
                limit: 0,
                field_mode: ExplorerFieldMode::AllValues,
                ..ExplorerQuery::default()
            })
            .expect("all-values explore");
        let all_tag = all_values
            .facets
            .get(b"TAG".as_slice())
            .expect("all-values tag facet");
        assert_eq!(all_tag.values().sum::<u64>(), 2);
        assert_eq!(all_tag.len(), 2);

        let mut first_value_reader = FileReader::open(&path).expect("open first-value reader");
        let first_value = first_value_reader
            .explore(&ExplorerQuery {
                facets: vec![b"TAG".to_vec()],
                limit: 0,
                field_mode: ExplorerFieldMode::FirstValue,
                ..ExplorerQuery::default()
            })
            .expect("first-value explore");
        let first_tag = first_value
            .facets
            .get(b"TAG".as_slice())
            .expect("first-value tag facet");
        assert_eq!(first_tag.values().sum::<u64>(), 1);
        assert_eq!(first_tag.len(), 1);
        assert_eq!(first_value.stats.early_stops, 1);
    }

    #[test]
    fn explorer_first_value_does_not_double_count_duplicate_facets_or_histogram() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("first-value-no-double-count.journal");
        write_entries(
            &path,
            None,
            &[(
                &[
                    b"_SOURCE_REALTIME_TIMESTAMP=1000",
                    b"TAG=one",
                    b"TAG=two",
                    b"MESSAGE=after-tag",
                ],
                1_000,
            )],
        );

        let mut reader = FileReader::open(&path).expect("open reader");
        let result = reader
            .explore(&ExplorerQuery {
                facets: vec![b"TAG".to_vec()],
                histogram: Some(b"TAG".to_vec()),
                histogram_target_buckets: 1,
                limit: 0,
                ..ExplorerQuery::default()
            })
            .expect("explore");

        assert_eq!(
            result
                .facets
                .get(b"TAG".as_slice())
                .expect("tag facet")
                .values()
                .sum::<u64>(),
            1
        );
        assert_eq!(
            result
                .histogram
                .as_ref()
                .expect("histogram")
                .buckets
                .iter()
                .flat_map(|bucket| bucket.values.values())
                .sum::<u64>(),
            1
        );

        let mut all_values_reader = FileReader::open(&path).expect("open all-values reader");
        let all_values = all_values_reader
            .explore(&ExplorerQuery {
                facets: vec![b"TAG".to_vec()],
                histogram: Some(b"TAG".to_vec()),
                histogram_target_buckets: 1,
                limit: 0,
                field_mode: ExplorerFieldMode::AllValues,
                ..ExplorerQuery::default()
            })
            .expect("all-values explore");

        assert_eq!(
            all_values
                .facets
                .get(b"TAG".as_slice())
                .expect("tag facet")
                .values()
                .sum::<u64>(),
            2
        );
        assert_eq!(
            all_values
                .histogram
                .as_ref()
                .expect("histogram")
                .buckets
                .iter()
                .flat_map(|bucket| bucket.values.values())
                .sum::<u64>(),
            2
        );
    }

    #[test]
    fn explorer_first_value_tracks_required_field_identities() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("first-value-identities.journal");
        write_entries(
            &path,
            None,
            &[(&[b"TAG=one", b"TAG=two", b"SERVICE=a"], 1_000)],
        );

        let mut reader = FileReader::open(&path).expect("open reader");
        let result = reader
            .explore(&ExplorerQuery {
                facets: vec![b"TAG".to_vec(), b"SERVICE".to_vec()],
                limit: 0,
                field_mode: ExplorerFieldMode::FirstValue,
                ..ExplorerQuery::default()
            })
            .expect("explore");

        assert_eq!(
            result
                .facets
                .get(b"TAG".as_slice())
                .expect("tag facet")
                .values()
                .sum::<u64>(),
            1
        );
        assert_eq!(
            result
                .facets
                .get(b"SERVICE".as_slice())
                .and_then(|values| values.get(b"a".as_slice())),
            Some(&1)
        );
        assert_eq!(result.stats.early_stops, 1);
    }

    #[test]
    fn explorer_rejects_duplicate_facet_fields() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("duplicate-facets.journal");
        write_entries(&path, None, &[(&[b"SERVICE=a"], 1_000)]);

        let mut reader = FileReader::open(&path).expect("open reader");
        let err = reader
            .explore(&ExplorerQuery {
                facets: vec![b"SERVICE".to_vec(), b"SERVICE".to_vec()],
                limit: 0,
                ..ExplorerQuery::default()
            })
            .expect_err("duplicate facets rejected");

        assert!(err.to_string().contains("must not be duplicated"));
    }

    #[test]
    fn explorer_empty_result_keeps_requested_facet_with_no_values() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("empty-result.journal");
        write_entries(&path, None, &[(&[b"SERVICE=a", b"PRIORITY=3"], 1_000)]);

        let mut reader = FileReader::open(&path).expect("open reader");
        let result = reader
            .explore(&ExplorerQuery {
                after_realtime_usec: Some(10_000),
                before_realtime_usec: Some(20_000),
                facets: vec![b"SERVICE".to_vec()],
                limit: 10,
                realtime_slack_usec: 0,
                ..ExplorerQuery::default()
            })
            .expect("explore");

        assert!(result.rows.is_empty());
        assert_eq!(result.stats.rows_matched, 0);
        assert!(
            result
                .facets
                .get(b"SERVICE".as_slice())
                .expect("service facet")
                .is_empty()
        );
    }

    #[test]
    fn explorer_facet_time_bounds_do_not_count_slack_rows_without_source_realtime() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("facet-time-bound.journal");
        write_entries(
            &path,
            None,
            &[
                (&[b"SERVICE=before"], 340_000_000),
                (&[b"SERVICE=inside"], 360_000_000),
                (&[b"SERVICE=after"], 400_000_000),
            ],
        );

        let mut reader = FileReader::open(&path).expect("open reader");
        let result = reader
            .explore(&ExplorerQuery {
                after_realtime_usec: Some(350_000_000),
                before_realtime_usec: Some(370_000_000),
                facets: vec![b"SERVICE".to_vec()],
                limit: 0,
                realtime_slack_usec: 20_000_000,
                use_source_realtime: false,
                ..ExplorerQuery::default()
            })
            .expect("explore");

        let service = result
            .facets
            .get(b"SERVICE".as_slice())
            .expect("service facet");
        assert_eq!(service.get(b"inside".as_slice()), Some(&1));
        assert_eq!(service.get(b"before".as_slice()), None);
        assert_eq!(service.get(b"after".as_slice()), None);
        assert_eq!(result.stats.facet_rows_matched, 1);
    }

    #[test]
    fn explorer_fts_disables_first_value_early_stop() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("fts-no-early-stop.journal");
        write_entries(&path, None, &[(&[b"TAG=one", b"MESSAGE=needle"], 1_000)]);

        let mut reader = FileReader::open(&path).expect("open reader");
        let result = reader
            .explore(&ExplorerQuery {
                facets: vec![b"TAG".to_vec()],
                fts_patterns: vec![b"needle".to_vec()],
                limit: 0,
                ..ExplorerQuery::default()
            })
            .expect("explore");

        assert_eq!(result.stats.early_stops, 0);
        assert_eq!(result.stats.data_refs_seen, 2);
        assert_eq!(
            result
                .facets
                .get(b"TAG".as_slice())
                .and_then(|values| values.get(b"one".as_slice())),
            Some(&1)
        );
    }

    #[test]
    fn explorer_auto_anchor_scans_backward_from_tail() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("backward.journal");
        write_entries(
            &path,
            None,
            &[
                (&[b"SERVICE=a", b"PRIORITY=3"], 1_000),
                (&[b"SERVICE=b", b"PRIORITY=4"], 2_000),
            ],
        );

        let mut reader = FileReader::open(&path).expect("open reader");
        let query = ExplorerQuery {
            direction: Direction::Backward,
            limit: 2,
            ..ExplorerQuery::default()
        };

        let result = reader.explore(&query).expect("explore");
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0].realtime_usec, 2_000);
        assert_eq!(result.rows[1].realtime_usec, 1_000);
    }

    #[test]
    fn explorer_backward_time_bound_stops_after_slack_window() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("backward-time-bound.journal");
        write_entries(
            &path,
            None,
            &[
                (&[b"SERVICE=a"], 100_000_000),
                (&[b"SERVICE=b"], 200_000_000),
                (&[b"SERVICE=c"], 300_000_000),
                (&[b"SERVICE=d"], 400_000_000),
                (&[b"SERVICE=e"], 500_000_000),
            ],
        );

        let mut reader = FileReader::open(&path).expect("open reader");
        let query = ExplorerQuery {
            after_realtime_usec: Some(350_000_000),
            direction: Direction::Backward,
            limit: 10,
            realtime_slack_usec: 10_000_000,
            ..ExplorerQuery::default()
        };

        let result = reader.explore(&query).expect("explore");
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0].realtime_usec, 500_000_000);
        assert_eq!(result.rows[1].realtime_usec, 400_000_000);
        assert_eq!(result.stats.rows_examined, 2);
    }

    #[test]
    fn explorer_histogram_and_fts_are_opt_in() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("histogram.journal");
        write_entries(
            &path,
            None,
            &[
                (&[b"MESSAGE=alpha", b"PRIORITY=3"], 1_000),
                (&[b"MESSAGE=beta", b"PRIORITY=4"], 2_000),
            ],
        );

        let mut reader = FileReader::open(&path).expect("open reader");
        let query = ExplorerQuery {
            after_realtime_usec: Some(0),
            before_realtime_usec: Some(3_000),
            histogram: Some(b"PRIORITY".to_vec()),
            histogram_target_buckets: 2,
            fts_patterns: vec![b"alp".to_vec()],
            limit: 10,
            ..ExplorerQuery::default()
        };

        let result = reader.explore(&query).expect("explore");
        assert_eq!(result.rows.len(), 1);
        assert!(result.stats.fts_scans > 0);
        assert_eq!(
            result
                .histogram
                .as_ref()
                .expect("histogram")
                .buckets
                .iter()
                .flat_map(|bucket| bucket.values.values())
                .sum::<u64>(),
            1
        );
    }

    #[test]
    fn explorer_first_value_stops_after_same_data_satisfies_multiple_roles() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("same-data-multiple-roles.journal");
        write_entries(
            &path,
            None,
            &[(
                &[b"_SOURCE_REALTIME_TIMESTAMP=1000", b"MESSAGE=after-source"],
                1_000,
            )],
        );

        let mut reader = FileReader::open(&path).expect("open reader");
        let result = reader
            .explore(&ExplorerQuery {
                histogram: Some(SOURCE_REALTIME_FIELD.to_vec()),
                histogram_target_buckets: 1,
                limit: 0,
                field_mode: ExplorerFieldMode::FirstValue,
                ..ExplorerQuery::default()
            })
            .expect("explore");

        assert_eq!(result.stats.histogram_updates, 1);
        assert_eq!(result.stats.early_stops, 1);
        assert_eq!(
            result
                .histogram
                .as_ref()
                .expect("histogram")
                .buckets
                .iter()
                .flat_map(|bucket| bucket.values.values())
                .sum::<u64>(),
            1
        );
    }
}
