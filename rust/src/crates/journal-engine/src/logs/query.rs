//! Log querying from indexed journal files.
//!
//! This module provides the `LogQuery` builder for efficiently querying and
//! merging log entries from multiple indexed journal files, as well as
//! functions for extracting raw field data from journal entries.

use crate::error::Result;
use journal_core::file::{JournalFile, Mmap};
use journal_index::{
    Anchor, Direction, FieldName, FieldValuePair, FileIndex, Filter, LogEntryId, LogQueryParams,
    LogQueryParamsBuilder, Microseconds,
};
use journal_registry::File;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU64;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio_util::sync::CancellationToken;
use tracing::warn;

/// Pagination state for multi-file log queries.
///
/// This tracks the position in each file where we stopped reading,
/// allowing queries to resume efficiently without re-scanning entries.
///
/// The state is tied to a specific query configuration (filter, anchor, direction, etc).
/// Changing the query parameters while using the same pagination state will produce
/// undefined results.
#[derive(Debug, Clone, Default)]
pub struct PaginationState {
    /// Maps each file to the last position we read from it
    pub file_positions: HashMap<File, usize>,
}

/// Builder for configuring and executing log queries from indexed journal files.
///
/// This builder allows you to specify:
/// - Direction (forward/backward in time)
/// - Anchor timestamp (starting point)
/// - Limit (maximum entries to retrieve)
/// - Source timestamp field (which field to use for timestamps)
/// - Filter (to match specific entries)
///
/// # Example
///
/// ```ignore
/// use journal_index::{Anchor, Direction};
/// use journal_function::logs::LogQuery;
///
/// let entries = LogQuery::new(&file_indexes, Anchor::Head, Direction::Forward)
///     .with_limit(100)
///     .execute();
/// ```
pub struct LogQuery<'a> {
    file_indexes: &'a [FileIndex],
    builder: LogQueryParamsBuilder,
    cancellation: Option<CancellationToken>,
    progress: Option<Arc<AtomicUsize>>,
    output_fields: Option<HashSet<String>>,
}

impl<'a> LogQuery<'a> {
    /// Create a new log query builder with required parameters.
    ///
    /// # Arguments
    ///
    /// * `file_indexes` - Journal file indexes to query
    /// * `anchor` - Starting point for the query (Head, Tail, or specific timestamp)
    /// * `direction` - Direction to iterate (Forward or Backward)
    ///
    /// # Optional Configuration
    ///
    /// Use builder methods to set optional parameters:
    /// - Limit: None (unlimited)
    /// - Source timestamp field: _SOURCE_REALTIME_TIMESTAMP
    /// - Filter: None
    pub fn new(file_indexes: &'a [FileIndex], anchor: Anchor, direction: Direction) -> Self {
        Self {
            file_indexes,
            builder: LogQueryParamsBuilder::new(anchor, direction).with_source_timestamp_field(
                Some(FieldName::new_unchecked("_SOURCE_REALTIME_TIMESTAMP")),
            ),
            cancellation: None,
            progress: None,
            output_fields: None,
        }
    }

    /// Set the maximum number of log entries to retrieve (optional).
    ///
    /// If not set (None), all matching entries will be retrieved.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.builder = self.builder.with_limit(limit);
        self
    }

    /// Set the source timestamp field to use for entry timestamps (optional).
    ///
    /// Pass `None` to use the entry's realtime timestamp from the journal header.
    /// Pass `Some(field_name)` to use a custom timestamp field from the entry data.
    pub fn with_source_timestamp_field(mut self, field: Option<FieldName>) -> Self {
        self.builder = self.builder.with_source_timestamp_field(field);
        self
    }

    /// Set a filter to apply to log entries (optional).
    ///
    /// Only entries matching the filter will be included in the results.
    pub fn with_filter(mut self, filter: Filter) -> Self {
        self.builder = self.builder.with_filter(filter);
        self
    }

    /// Set the lower time boundary (inclusive) in microseconds (optional).
    ///
    /// Only entries with timestamp >= after_usec will be included.
    /// This enforces a hard boundary regardless of anchor or limit.
    pub fn with_after_usec(mut self, after: u64) -> Self {
        self.builder = self.builder.with_after(Microseconds(after));
        self
    }

    /// Set the upper time boundary (exclusive) in microseconds (optional).
    ///
    /// Only entries with timestamp < before_usec will be included.
    /// This enforces a hard boundary regardless of anchor or limit.
    pub fn with_before_usec(mut self, before: u64) -> Self {
        self.builder = self.builder.with_before(Microseconds(before));
        self
    }

    /// Set a regex pattern for full-text search (optional).
    ///
    /// Only entries where at least one data object (in "FIELD=value" format)
    /// matches the regex will be included in the results.
    ///
    /// The pattern will be compiled when the query is executed. Invalid patterns
    /// will cause execute() to return an error.
    pub fn with_regex(mut self, pattern: impl Into<String>) -> Self {
        self.builder = self.builder.with_regex(pattern);
        self
    }

    /// Set a cancellation token for the query (optional).
    ///
    /// When set, the query will check the token before processing each file
    /// and return early with partial results if cancelled.
    pub fn with_cancellation(mut self, token: CancellationToken) -> Self {
        self.cancellation = Some(token);
        self
    }

    /// Set a progress counter for the query (optional).
    ///
    /// When set, the counter is incremented (via `fetch_add`) after each file
    /// is processed in `retrieve_log_entries`.
    pub fn with_progress(mut self, counter: Arc<AtomicUsize>) -> Self {
        self.progress = Some(counter);
        self
    }

    /// Limit returned field-value pairs to the requested on-disk field names.
    pub fn with_output_fields<I, S>(mut self, fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.output_fields = Some(fields.into_iter().map(Into::into).collect());
        self
    }

    /// Execute the query and return log entries.
    ///
    /// This consumes the builder and returns a vector of log entries sorted by timestamp
    /// according to the configured direction.
    ///
    /// # Errors
    ///
    /// Returns an error if anchor or direction were not set, or if time boundaries are invalid.
    pub fn execute(self) -> Result<Vec<LogEntryData>> {
        let params = self.builder.build()?;
        let output_fields = self.output_fields;
        let (log_entry_ids, _state) = retrieve_log_entries(
            self.file_indexes.to_vec(),
            params,
            None,
            self.cancellation.as_ref(),
            self.progress.as_ref(),
        );

        extract_entry_data(&log_entry_ids, output_fields.as_ref())
    }

    /// Execute the query with pagination support.
    ///
    /// This consumes the builder and returns a page of log entries along with
    /// pagination state that can be used to retrieve the next page.
    ///
    /// # Arguments
    ///
    /// * `state` - Optional pagination state from a previous query. Pass `None` for the first page.
    ///
    /// # Returns
    ///
    /// Returns a tuple of (log entry data, new pagination state). If the pagination state
    /// is empty (no file positions tracked), there are no more results.
    ///
    /// # Errors
    ///
    /// Returns an error if anchor or direction were not set, or if time boundaries are invalid.
    pub fn execute_page(
        self,
        state: Option<&PaginationState>,
    ) -> Result<(Vec<LogEntryData>, PaginationState)> {
        let params = self.builder.build()?;
        let output_fields = self.output_fields;
        let (log_entry_ids, new_state) = retrieve_log_entries(
            self.file_indexes.to_vec(),
            params,
            state,
            self.cancellation.as_ref(),
            self.progress.as_ref(),
        );

        let data = extract_entry_data(&log_entry_ids, output_fields.as_ref())?;
        Ok((data, new_state))
    }
}

/// Retrieve and merge log entries from multiple indexed journal files.
///
/// This function efficiently retrieves log entries from multiple journal files,
/// merging them in timestamp order while respecting the limit constraint.
///
/// # Arguments
///
/// * `file_indexes` - Vector of indexed journal files to retrieve from
/// * `params` - Query parameters (anchor, direction, limit, filter, boundaries)
/// * `state` - Optional pagination state to resume from previous query
///
/// # Returns
///
/// A tuple of (log entries, new pagination state). The entries are sorted by timestamp
/// and limited to `params.limit`. The new state can be used to resume the query.
fn retrieve_log_entries(
    file_indexes: Vec<FileIndex>,
    params: LogQueryParams,
    state: Option<&PaginationState>,
    cancellation: Option<&CancellationToken>,
    progress: Option<&Arc<AtomicUsize>>,
) -> (Vec<LogEntryId>, PaginationState) {
    // Handle edge cases
    if params.limit() == Some(0) || file_indexes.is_empty() {
        return (Vec::new(), PaginationState::default());
    }

    let anchor_usec = multi_file_anchor_usec(&file_indexes, params.anchor());
    let mut relevant_indexes =
        relevant_file_indexes(&file_indexes, params.direction(), anchor_usec);

    if let Some(counter) = progress {
        let filtered = file_indexes.len() - relevant_indexes.len();
        counter.fetch_add(filtered, Ordering::Relaxed);
    }

    if relevant_indexes.is_empty() {
        return (Vec::new(), PaginationState::default());
    }

    sort_relevant_indexes(&mut relevant_indexes, params.direction());

    let (limit, mut collected_entries) = collection_limit_and_buffer(params.limit());
    let mut new_state = state.cloned().unwrap_or_default();

    for file_index in relevant_indexes {
        if query_cancelled(cancellation, &new_state) {
            break;
        }

        mark_file_processed(progress);

        if should_prune_file(file_index, &collected_entries, limit, params.direction()) {
            break;
        }

        if let Some(new_entries) = query_file_entries(file_index, &params, state) {
            collected_entries =
                merge_log_entries(collected_entries, new_entries, limit, params.direction());
        }
    }

    update_pagination_state(&mut new_state, &collected_entries, params.direction());

    (collected_entries, new_state)
}

fn multi_file_anchor_usec(file_indexes: &[FileIndex], anchor: Anchor) -> u64 {
    match anchor {
        Anchor::Timestamp(ts) => ts.get(),
        Anchor::Head => file_indexes
            .iter()
            .map(|fi| fi.start_time().to_microseconds().get())
            .min()
            .unwrap_or(0),
        Anchor::Tail => file_indexes
            .iter()
            .map(|fi| fi.end_time().to_microseconds().get())
            .max()
            .unwrap_or(0),
    }
}

fn relevant_file_indexes(
    file_indexes: &[FileIndex],
    direction: Direction,
    anchor_usec: u64,
) -> Vec<&FileIndex> {
    file_indexes
        .iter()
        .filter(|fi| file_can_contain_anchor(fi, direction, anchor_usec))
        .collect()
}

fn file_can_contain_anchor(file_index: &FileIndex, direction: Direction, anchor_usec: u64) -> bool {
    match direction {
        Direction::Forward => file_index.end_time().to_microseconds().get() >= anchor_usec,
        Direction::Backward => file_index.start_time().to_microseconds().get() <= anchor_usec,
    }
}

fn sort_relevant_indexes(file_indexes: &mut [&FileIndex], direction: Direction) {
    match direction {
        Direction::Forward => file_indexes.sort_by_key(|fi| fi.start_time()),
        Direction::Backward => file_indexes.sort_by_key(|fi| std::cmp::Reverse(fi.end_time())),
    }
}

fn collection_limit_and_buffer(limit: Option<usize>) -> (usize, Vec<LogEntryId>) {
    match limit {
        Some(limit) => (limit, Vec::with_capacity(limit)),
        None => (usize::MAX, Vec::with_capacity(200)),
    }
}

fn query_cancelled(cancellation: Option<&CancellationToken>, state: &PaginationState) -> bool {
    let Some(token) = cancellation else {
        return false;
    };
    if !token.is_cancelled() {
        return false;
    }
    warn!(
        "log query cancelled after processing {} files, returning partial results",
        state.file_positions.len()
    );
    true
}

fn mark_file_processed(progress: Option<&Arc<AtomicUsize>>) {
    if let Some(counter) = progress {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

fn should_prune_file(
    file_index: &FileIndex,
    collected_entries: &[LogEntryId],
    limit: usize,
    direction: Direction,
) -> bool {
    collected_entries.len() >= limit
        && can_prune_file(file_index, collected_entries, direction).unwrap_or(false)
}

fn query_file_entries(
    file_index: &FileIndex,
    params: &LogQueryParams,
    state: Option<&PaginationState>,
) -> Option<Vec<LogEntryId>> {
    let file = file_index.file();
    let file_params = params_for_file(file_index, params, state);
    match file_index.find_log_entries(file, &file_params) {
        Ok(entries) if entries.is_empty() => None,
        Ok(entries) => Some(entries),
        Err(e) => {
            warn!(file = file.path(), "failed to retrieve log entries: {e}");
            None
        }
    }
}

fn params_for_file(
    file_index: &FileIndex,
    params: &LogQueryParams,
    state: Option<&PaginationState>,
) -> LogQueryParams {
    let Some(pos) = state.and_then(|s| s.file_positions.get(file_index.file()).copied()) else {
        return params.clone();
    };

    let mut builder = LogQueryParamsBuilder::new(params.anchor(), params.direction());
    if let Some(limit) = params.limit() {
        builder = builder.with_limit(limit);
    }
    if let Some(field) = params.source_timestamp_field() {
        builder = builder.with_source_timestamp_field(Some(field.clone()));
    }
    if let Some(filter) = params.filter() {
        builder = builder.with_filter(filter.clone());
    }
    if let Some(after) = params.after() {
        builder = builder.with_after(after);
    }
    if let Some(before) = params.before() {
        builder = builder.with_before(before);
    }
    if let Some(regex) = params.regex() {
        builder = builder.with_regex(regex.as_str());
    }

    builder
        .with_resume_position(pos)
        .build()
        .expect("resume params copied from validated query params")
}

fn update_pagination_state(
    state: &mut PaginationState,
    entries: &[LogEntryId],
    direction: Direction,
) {
    for entry in entries {
        state
            .file_positions
            .entry(entry.file.clone())
            .and_modify(|pos| {
                *pos = next_resume_position(*pos, entry.position, direction);
            })
            .or_insert(entry.position);
    }
}

fn next_resume_position(current: usize, candidate: usize, direction: Direction) -> usize {
    match direction {
        Direction::Forward => current.max(candidate),
        Direction::Backward => current.min(candidate),
    }
}

/// Check if we can prune (skip) a file based on its time range and current results.
///
/// Returns Some(true) if we should break early, Some(false) if we should continue,
/// or None if we can't determine (shouldn't happen with a full result set).
fn can_prune_file(
    file_index: &FileIndex,
    result: &[LogEntryId],
    direction: Direction,
) -> Option<bool> {
    match direction {
        Direction::Forward => {
            // For forward: if file starts after our latest entry, skip all remaining files
            let max_timestamp = result.last()?.timestamp.get();
            Some(file_index.start_time().to_microseconds().get() > max_timestamp)
        }
        Direction::Backward => {
            // For backward: if file ends before our earliest entry, skip all remaining files
            let min_timestamp = result.first()?.timestamp.get();
            Some(file_index.end_time().to_microseconds().get() < min_timestamp)
        }
    }
}

/// Merges two sorted vectors into a single sorted vector with at most `limit` elements.
///
/// This function performs a two-pointer merge, which is efficient for combining
/// sorted sequences. It only retains the smallest/largest `limit` entries by timestamp
/// depending on the direction.
///
/// # Arguments
///
/// * `a` - First sorted vector
/// * `b` - Second sorted vector
/// * `limit` - Maximum number of elements in the result
/// * `direction` - Direction determines ascending (Forward) or descending (Backward) order
///
/// # Returns
///
/// A new vector containing the merged and limited results
fn merge_log_entries(
    a: Vec<LogEntryId>,
    b: Vec<LogEntryId>,
    limit: usize,
    direction: Direction,
) -> Vec<LogEntryId> {
    // Handle simple cases
    if a.is_empty() {
        return b.into_iter().take(limit).collect();
    }
    if b.is_empty() {
        return a.into_iter().take(limit).collect();
    }

    // Allocate result vector with appropriate capacity — cap at actual data size
    // to avoid capacity overflow when limit is usize::MAX (no limit set).
    let mut result = Vec::with_capacity(a.len().saturating_add(b.len()).min(limit));
    let mut i = 0;
    let mut j = 0;

    // Two-pointer merge: always take the appropriate element based on direction
    while result.len() < limit {
        let take_from_a = match (i < a.len(), j < b.len()) {
            (true, false) => true,
            (false, true) => false,
            (false, false) => break,
            (true, true) => match direction {
                Direction::Forward => a[i].timestamp <= b[j].timestamp,
                Direction::Backward => a[i].timestamp >= b[j].timestamp,
            },
        };

        if take_from_a {
            result.push(a[i].clone());
            i += 1;
        } else {
            result.push(b[j].clone());
            j += 1;
        }
    }

    result
}

fn is_projected(raw_field_name: &str, output_fields: Option<&HashSet<String>>) -> bool {
    output_fields.map_or(true, |projected| projected.contains(raw_field_name))
}

/// Raw field data extracted from a journal entry.
///
/// This is an intermediate representation between a `LogEntryId` (which only contains
/// a file offset) and format-specific structures like `Table`, Arrow `RecordBatch`,
/// or columnar data.
///
/// The fields are stored as `FieldValuePair` objects, which efficiently store the
/// field name and value with a cached split position for fast access.
#[derive(Debug, Clone)]
pub struct LogEntryData {
    /// Timestamp of the entry in microseconds since epoch
    pub timestamp: u64,
    /// All field=value pairs in this entry
    pub fields: Vec<FieldValuePair>,
}

/// Extracts raw field data from multiple log entries efficiently.
///
/// This function groups entries by file and processes them in batches,
/// minimizing file open/close overhead. It reads the journal files and
/// extracts all field=value pairs without applying any transformations.
///
/// # Arguments
///
/// * `log_entries` - Slice of log entry IDs to extract data from
///
/// # Returns
///
/// A vector of `LogEntryData` in the same order as the input entries
fn extract_entry_data(
    log_entries: &[LogEntryId],
    output_fields: Option<&HashSet<String>>,
) -> Result<Vec<LogEntryData>> {
    let entries_by_file = entries_grouped_by_file(log_entries);
    let mut result = vec![None; log_entries.len()];
    let mut decompress_buf = Vec::new();

    for (file, file_entries) in entries_by_file {
        let journal_file = JournalFile::<Mmap>::open(file, 8 * 1024 * 1024)?;
        let mut data_offsets = Vec::new();

        for (original_idx, entry) in file_entries {
            let fields = read_entry_fields(
                &journal_file,
                entry,
                output_fields,
                &mut data_offsets,
                &mut decompress_buf,
            )?;
            result[original_idx] = Some(LogEntryData {
                timestamp: entry.timestamp.get(),
                fields,
            });
        }
    }

    Ok(result.into_iter().flatten().collect())
}

fn entries_grouped_by_file(
    log_entries: &[LogEntryId],
) -> HashMap<&File, Vec<(usize, &LogEntryId)>> {
    let mut entries_by_file: HashMap<&File, Vec<(usize, &LogEntryId)>> = HashMap::new();
    for (idx, entry) in log_entries.iter().enumerate() {
        entries_by_file
            .entry(&entry.file)
            .or_default()
            .push((idx, entry));
    }
    entries_by_file
}

fn read_entry_fields(
    journal_file: &JournalFile<Mmap>,
    entry: &LogEntryId,
    output_fields: Option<&HashSet<String>>,
    data_offsets: &mut Vec<NonZeroU64>,
    decompress_buf: &mut Vec<u8>,
) -> Result<Vec<FieldValuePair>> {
    let entry_offset =
        NonZeroU64::new(entry.offset).ok_or(journal_core::JournalError::InvalidOffset)?;
    collect_entry_data_offsets(journal_file, entry_offset, data_offsets)?;

    let mut fields = Vec::new();
    for data_offset in data_offsets.iter().copied() {
        if let Some(pair) =
            read_projected_pair(journal_file, data_offset, output_fields, decompress_buf)?
        {
            fields.push(pair);
        }
    }
    Ok(fields)
}

fn collect_entry_data_offsets(
    journal_file: &JournalFile<Mmap>,
    entry_offset: NonZeroU64,
    data_offsets: &mut Vec<NonZeroU64>,
) -> Result<()> {
    data_offsets.clear();
    let entry_guard = journal_file.entry_ref(entry_offset)?;
    entry_guard.collect_offsets(data_offsets)?;
    Ok(())
}

fn read_projected_pair(
    journal_file: &JournalFile<Mmap>,
    data_offset: NonZeroU64,
    output_fields: Option<&HashSet<String>>,
    decompress_buf: &mut Vec<u8>,
) -> Result<Option<FieldValuePair>> {
    let data_guard = journal_file.data_ref(data_offset)?;
    let payload_bytes = if data_guard.is_compressed() {
        data_guard.decompress(decompress_buf)?;
        &decompress_buf[..]
    } else {
        data_guard.raw_payload()
    };

    let payload_str = String::from_utf8_lossy(payload_bytes);
    let Some(pair) = FieldValuePair::parse(&payload_str) else {
        return Ok(None);
    };
    Ok(is_projected(pair.field(), output_fields).then_some(pair))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projected_fields(fields: &[&str]) -> HashSet<String> {
        fields.iter().map(|field| (*field).to_string()).collect()
    }

    #[test]
    fn projection_accepts_raw_systemd_field_name() {
        let projected = projected_fields(&["_SYSTEMD_UNIT"]);

        assert!(is_projected("_SYSTEMD_UNIT", Some(&projected)));
    }

    #[test]
    fn projection_rejects_unmatched_field_names() {
        let projected = projected_fields(&["service.name"]);

        assert!(!is_projected("_SYSTEMD_UNIT", Some(&projected)));
    }

    #[test]
    fn projection_accepts_all_fields_without_projection_filter() {
        assert!(is_projected("_SYSTEMD_UNIT", None));
    }
}
