use crate::{
    Bitmap, FieldName, FieldValuePair, Histogram, IndexError, Microseconds, Result, Seconds,
};
use journal_core::collections::{HashMap, HashSet};
use journal_core::file::{JournalFile, Mmap};
use journal_core::repository::File;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;
use tracing::{error, trace};

/// Index for a single journal file, enabling efficient querying and filtering.
///
/// A `FileIndex` contains pre-computed metadata about a journal file:
/// - Time-based histogram for quick time-range queries
/// - Entry offsets sorted by timestamp for binary search
/// - Bitmaps for indexed field=value pairs enabling fast filtering
/// - Field names present in the file
///
/// The index is immutable after creation and represents a snapshot of the journal
/// file at the time it was indexed. For actively-written files, the index may
/// become stale and need rebuilding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "allocative", derive(allocative::Allocative))]
pub struct FileIndex {
    // The file this index was created for
    file: File,
    // Unix timestamp (seconds since epoch) when this index was created
    indexed_at: Seconds,
    // True if the journal file was online (state=1) when indexed
    was_online: bool,
    // The journal file's histogram
    histogram: Histogram,
    // Entry offsets sorted by time
    entry_offsets: Vec<u32>,
    // Set of fields in the file
    file_fields: HashSet<FieldName>,
    // Set of fields that were requested to be indexed
    indexed_fields: HashSet<FieldName>,
    // Bitmap for each indexed field=value pair
    bitmaps: HashMap<FieldValuePair, Bitmap>,
}

impl FileIndex {
    /// Create a new file index.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        file: File,
        indexed_at: Seconds,
        was_online: bool,
        histogram: Histogram,
        entry_offsets: Vec<u32>,
        fields: HashSet<FieldName>,
        indexed_fields: HashSet<FieldName>,
        bitmaps: HashMap<FieldValuePair, Bitmap>,
    ) -> Self {
        Self {
            file,
            indexed_at,
            was_online,
            histogram,
            entry_offsets,
            file_fields: fields,
            indexed_fields,
            bitmaps,
        }
    }

    /// Get the bucket duration granularity of the file's histogram.
    pub fn bucket_duration(&self) -> Seconds {
        Seconds(self.histogram.bucket_duration.get())
    }

    /// Get a reference to the journal file this index represents.
    pub fn file(&self) -> &File {
        &self.file
    }

    /// Get the timestamp when this index was created.
    pub fn indexed_at(&self) -> Seconds {
        self.indexed_at
    }

    /// Check if the journal file was online (actively being written) when indexed.
    pub fn online(&self) -> bool {
        self.was_online
    }

    /// Check if this index is still fresh.
    ///
    /// For files that were online (actively being written) when indexed, the cache
    /// is considered stale after 1 second. For archived/offline files, the cache
    /// is always fresh since they never change.
    pub fn is_fresh(&self) -> bool {
        if self.was_online {
            let now = Seconds::now();
            let age = now.get().saturating_sub(self.indexed_at.get());
            age < 1
        } else {
            // Archived/offline file: always fresh
            true
        }
    }

    /// Get the start time of this file's indexed time range.
    pub fn start_time(&self) -> Seconds {
        self.histogram.start_time()
    }

    /// Get the end time of this file's indexed time range.
    pub fn end_time(&self) -> Seconds {
        self.histogram.end_time()
    }

    /// Get the number of time buckets.
    pub fn num_buckets(&self) -> usize {
        self.histogram.num_buckets()
    }

    /// Get the total count of entries indexed.
    pub fn total_entries(&self) -> usize {
        self.histogram.total_entries()
    }

    /// Get all field names present in this file.
    pub fn fields(&self) -> &HashSet<FieldName> {
        &self.file_fields
    }

    /// Get all indexed field=value pairs with their bitmaps.
    pub fn bitmaps(&self) -> &HashMap<FieldValuePair, Bitmap> {
        &self.bitmaps
    }

    /// Check if a field is indexed.
    pub fn is_indexed(&self, field: &FieldName) -> bool {
        self.indexed_fields.contains(field)
    }

    /// Count entries (from a bitmap) that fall within a time range.
    pub fn count_entries_in_time_range(
        &self,
        bitmap: &Bitmap,
        start_time: Seconds,
        end_time: Seconds,
    ) -> Option<usize> {
        self.histogram
            .count_entries_in_time_range(bitmap, start_time, end_time)
    }
}

/// Direction for iterating through entries
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Iterate forward in time (from older to newer entries)
    #[default]
    Forward,
    /// Iterate backward in time (from newer to older entries)
    Backward,
}

/// Anchor point for starting a log query
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Anchor {
    /// Explicit timestamp in microseconds since epoch
    Timestamp(Microseconds),
    /// Start from the earliest timestamp (minimum start time from file indexes)
    Head,
    /// Start from the latest timestamp (maximum end time from file indexes)
    Tail,
}

/// Parameters for querying log entries from journal files.
///
/// This struct encapsulates all the configuration needed to query log entries,
/// whether from a single file or multiple files.
///
/// Use `LogQueryParamsBuilder` to construct instances of this type.
#[derive(Debug, Clone)]
pub struct LogQueryParams {
    /// Starting point for the query
    anchor: Anchor,
    /// Direction to iterate (Forward or Backward)
    direction: Direction,
    /// Maximum number of entries to return (None means unlimited)
    limit: Option<usize>,
    /// Optional field to use for timestamps (None uses realtime)
    source_timestamp_field: Option<super::FieldName>,
    /// Optional filter to apply to entries
    filter: Option<super::Filter>,
    /// Optional lower time boundary (inclusive) in microseconds
    after: Option<Microseconds>,
    /// Optional upper time boundary (exclusive) in microseconds
    before: Option<Microseconds>,
    /// Optional position to resume from for pagination.
    /// When set, the query will skip the binary search and continue from this position.
    /// The filter must remain unchanged between paginated queries.
    resume_position: Option<usize>,
    /// Optional regex for free text search against entry data objects.
    /// If set, only entries where at least one data object's full payload matches will be returned.
    regex: Option<Regex>,
}

impl LogQueryParams {
    /// Get the anchor point for the query
    pub fn anchor(&self) -> Anchor {
        self.anchor
    }

    /// Get the direction for iterating through entries
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// Get the maximum number of entries to return
    pub fn limit(&self) -> Option<usize> {
        self.limit
    }

    /// Get the source timestamp field
    pub fn source_timestamp_field(&self) -> Option<&super::FieldName> {
        self.source_timestamp_field.as_ref()
    }

    /// Get the filter to apply to entries
    pub fn filter(&self) -> Option<&super::Filter> {
        self.filter.as_ref()
    }

    /// Get the lower time boundary
    pub fn after(&self) -> Option<Microseconds> {
        self.after
    }

    /// Get the upper time boundary
    pub fn before(&self) -> Option<Microseconds> {
        self.before
    }

    /// Get the resume position for pagination
    pub fn resume_position(&self) -> Option<usize> {
        self.resume_position
    }

    /// Get the regex pattern for free text search
    pub fn regex(&self) -> Option<&Regex> {
        self.regex.as_ref()
    }
}

/// Builder for constructing `LogQueryParams` with validation.
///
/// Anchor and direction are required at construction time.
/// Other fields are optional and can be set via builder methods.
#[derive(Debug, Clone)]
pub struct LogQueryParamsBuilder {
    anchor: Anchor,
    direction: Direction,
    limit: Option<usize>,
    source_timestamp_field: Option<super::FieldName>,
    filter: Option<super::Filter>,
    after: Option<Microseconds>,
    before: Option<Microseconds>,
    resume_position: Option<usize>,
    regex_pattern: Option<String>,
}

impl LogQueryParamsBuilder {
    /// Create a new builder with required fields
    ///
    /// # Arguments
    ///
    /// * `anchor` - Starting point for the query
    /// * `direction` - Direction to iterate through entries
    pub fn new(anchor: Anchor, direction: Direction) -> Self {
        Self {
            anchor,
            direction,
            limit: None,
            source_timestamp_field: None,
            filter: None,
            after: None,
            before: None,
            resume_position: None,
            regex_pattern: None,
        }
    }

    /// Set the maximum number of entries to return
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set the source timestamp field
    pub fn with_source_timestamp_field(mut self, field: Option<super::FieldName>) -> Self {
        self.source_timestamp_field = field;
        self
    }

    /// Set the filter
    pub fn with_filter(mut self, filter: super::Filter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Set the lower time boundary
    pub fn with_after(mut self, after: Microseconds) -> Self {
        self.after = Some(after);
        self
    }

    /// Set the upper time boundary
    pub fn with_before(mut self, before: Microseconds) -> Self {
        self.before = Some(before);
        self
    }

    /// Set the resume position for pagination
    pub fn with_resume_position(mut self, position: usize) -> Self {
        self.resume_position = Some(position);
        self
    }

    /// Set the regex pattern for free text search.
    ///
    /// The regex will be matched against the full payload of each data object
    /// (in "FIELD=value" format). Only entries where at least one data object
    /// matches will be returned.
    ///
    /// The pattern will be compiled during `build()`. Invalid patterns will
    /// cause `build()` to return an error.
    pub fn with_regex(mut self, pattern: impl Into<String>) -> Self {
        self.regex_pattern = Some(pattern.into());
        self
    }

    /// Build the LogQueryParams, validating optional constraints
    pub fn build(self) -> Result<LogQueryParams> {
        // Validate time boundaries if both are set
        if let (Some(after), Some(before)) = (self.after, self.before) {
            if after >= before {
                return Err(IndexError::InvalidQueryTimeRange);
            }
        }

        // Compile regex pattern if provided
        let regex = if let Some(pattern) = self.regex_pattern {
            trace!("compiling regex pattern for log query: {:?}", pattern);
            match Regex::new(&pattern) {
                Ok(regex) => {
                    trace!("regex pattern compiled successfully");
                    Some(regex)
                }
                Err(e) => {
                    error!("failed to compile regex pattern {:?}: {}", pattern, e);
                    return Err(IndexError::InvalidRegex);
                }
            }
        } else {
            None
        };

        Ok(LogQueryParams {
            anchor: self.anchor,
            direction: self.direction,
            limit: self.limit,
            source_timestamp_field: self.source_timestamp_field,
            filter: self.filter,
            after: self.after,
            before: self.before,
            resume_position: self.resume_position,
            regex,
        })
    }
}

/// Read a timestamp field value from an entry's data objects.
fn get_timestamp_field(
    journal_file: &JournalFile<Mmap>,
    field_name: &super::FieldName,
    entry_offset: NonZeroU64,
) -> Result<u64> {
    let data_iter = journal_file.entry_data_objects(entry_offset)?;

    for data_result in data_iter {
        let data_object = data_result?;
        match crate::field_types::parse_timestamp(field_name.as_bytes(), &data_object) {
            Ok(timestamp) => return Ok(timestamp),
            Err(IndexError::InvalidFieldPrefix) => {
                continue;
            }
            Err(e) => return Err(e),
        };
    }

    Err(IndexError::MissingFieldName)
}

/// Get the timestamp for an entry at the given offset.
///
/// Attempts to read the source_timestamp_field from the entry's data objects.
/// Falls back to the entry's realtime timestamp if the field is not found.
fn get_entry_timestamp(
    journal_file: &JournalFile<Mmap>,
    source_timestamp_field: Option<&super::FieldName>,
    entry_offset: NonZeroU64,
) -> Result<u64> {
    // Try to read the source timestamp field if specified
    if let Some(field_name) = source_timestamp_field {
        match get_timestamp_field(journal_file, field_name, entry_offset) {
            Ok(timestamp) => return Ok(timestamp),
            Err(IndexError::MissingFieldName) => {
                // Field not found, fall back to realtime timestamp
            }
            Err(e) => return Err(e),
        }
    }

    // Fall back to realtime timestamp
    let entry = journal_file.entry_ref(entry_offset)?;
    Ok(entry.header.realtime)
}

/// Binary search to find the partition point in a slice of entry offsets.
///
/// Returns the index of the first element for which the predicate returns false.
/// The predicate may perform I/O and return errors, which are propagated.
fn partition_point_entries<F>(
    entry_offsets: &[NonZeroU64],
    left: usize,
    right: usize,
    predicate: F,
) -> Result<usize>
where
    F: Fn(NonZeroU64) -> Result<bool>,
{
    let mut left = left;
    let mut right = right;

    debug_assert!(left <= right);
    debug_assert!(right <= entry_offsets.len());

    while left != right {
        let mid = left.midpoint(right);

        if predicate(entry_offsets[mid])? {
            left = mid + 1;
        } else {
            right = mid;
        }
    }

    Ok(left)
}

/// Check if an entry matches a regex pattern
fn entry_matches_regex(
    journal_file: &JournalFile<Mmap>,
    entry_offset: NonZeroU64,
    regex: &Regex,
    data_match_cache: &mut HashMap<NonZeroU64, bool>,
    data_offsets_scratch: &mut Vec<NonZeroU64>,
    scratch_buffer: &mut Vec<u8>,
) -> Result<bool> {
    // Collect all data object offsets for this entry
    data_offsets_scratch.clear();
    {
        let entry = journal_file.entry_ref(entry_offset)?;
        entry.collect_offsets(data_offsets_scratch)?;
    }

    // Check each data object offset
    for data_offset in data_offsets_scratch.iter().copied() {
        // Check cache first
        if let Some(&matches) = data_match_cache.get(&data_offset) {
            if matches {
                return Ok(true);
            }
            continue;
        }

        // Cache miss - load the data object and check if it matches
        let data_object = journal_file.data_ref(data_offset)?;

        let payload_bytes = if data_object.is_compressed() {
            data_object.decompress(scratch_buffer)?;
            &scratch_buffer[..]
        } else {
            data_object.raw_payload()
        };

        let matches = if let Ok(payload_str) = std::str::from_utf8(payload_bytes) {
            regex.is_match(payload_str)
        } else {
            false
        };

        // Update cache
        data_match_cache.insert(data_offset, matches);

        if matches {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Identifies a specific log entry within a journal file.
#[derive(Debug, Clone)]
pub struct LogEntryId {
    /// The journal file containing this entry.
    pub file: File,
    /// Byte offset of the entry within the file.
    pub offset: u64,
    /// Timestamp of the entry in microseconds since epoch.
    pub timestamp: Microseconds,
    /// Position in the filtered entry_offsets vector.
    /// Used for pagination to resume queries at the exact position.
    pub position: usize,
}

impl FileIndex {
    /// Retrieve log entries with filtering.
    ///
    /// This method efficiently retrieves journal entries based on the provided query
    /// parameters. It uses binary search (partition point) to find the starting position,
    /// then iterates in the specified direction.
    ///
    /// # Arguments
    ///
    /// * `file` - The journal file to read timestamps and entries from
    /// * `params` - Query parameters (anchor, direction, limit, filter, boundaries)
    ///
    /// # Returns
    ///
    /// A vector of `LogEntryId` items sorted by time according to direction:
    /// - Forward: Returns entries in ascending time order (oldest to newest after anchor)
    /// - Backward: Returns entries in descending time order (newest to oldest before/at anchor)
    ///
    /// The vector length will not exceed `params.limit`. Returns an empty vector if no
    /// entries match the criteria or if limit is 0.
    pub fn find_log_entries(
        &self,
        file: &File,
        params: &LogQueryParams,
    ) -> Result<Vec<LogEntryId>> {
        let anchor_usec = self.query_anchor_usec(params);
        let bitmap = self.query_bitmap(params);

        if bitmap.is_empty() {
            return Ok(Vec::new());
        }

        let window_size = 32 * 1024 * 1024;
        let journal_file = JournalFile::open(file, window_size)?;
        let entry_offsets = self.candidate_entry_offsets(&bitmap);
        let limit = query_limit(params.limit(), entry_offsets.len());

        if limit == 0 {
            return Ok(Vec::new());
        }

        EntryScanner::new(self, &journal_file, params, anchor_usec, limit).collect(&entry_offsets)
    }

    fn query_anchor_usec(&self, params: &LogQueryParams) -> Microseconds {
        match params.anchor() {
            Anchor::Timestamp(ts) => ts,
            Anchor::Head => self.start_time().to_microseconds(),
            Anchor::Tail => self.end_time().to_microseconds(),
        }
    }

    fn query_bitmap(&self, params: &LogQueryParams) -> Bitmap {
        params
            .filter()
            .map(|f| f.evaluate(self))
            .unwrap_or_else(|| Bitmap::insert_range(0..self.entry_offsets.len() as u32))
    }

    fn candidate_entry_offsets(&self, bitmap: &Bitmap) -> Vec<NonZeroU64> {
        bitmap
            .iter()
            .map(|idx| self.entry_offsets[idx as usize])
            .filter(|offset| *offset != 0)
            .map(|offset| NonZeroU64::new(offset as u64).expect("non-zero offset"))
            .collect()
    }
}

fn query_limit(limit: Option<usize>, candidate_count: usize) -> usize {
    limit.unwrap_or(candidate_count)
}

enum BoundaryDecision {
    Include,
    Skip,
    Stop,
}

fn forward_boundary_decision(timestamp: u64, params: &LogQueryParams) -> BoundaryDecision {
    if params.after().is_some_and(|after| timestamp < after.get()) {
        return BoundaryDecision::Skip;
    }
    if params
        .before()
        .is_some_and(|before| timestamp >= before.get())
    {
        return BoundaryDecision::Stop;
    }
    BoundaryDecision::Include
}

fn backward_boundary_decision(timestamp: u64, params: &LogQueryParams) -> BoundaryDecision {
    if params
        .before()
        .is_some_and(|before| timestamp >= before.get())
    {
        return BoundaryDecision::Skip;
    }
    if params.after().is_some_and(|after| timestamp < after.get()) {
        return BoundaryDecision::Stop;
    }
    BoundaryDecision::Include
}

struct EntryScanner<'a> {
    file_index: &'a FileIndex,
    journal_file: &'a JournalFile<Mmap>,
    params: &'a LogQueryParams,
    anchor_usec: Microseconds,
    limit: usize,
    data_offsets_scratch: Vec<NonZeroU64>,
    data_match_cache: HashMap<NonZeroU64, bool>,
    scratch_buffer: Vec<u8>,
    regex_filtered_count: usize,
}

impl<'a> EntryScanner<'a> {
    fn new(
        file_index: &'a FileIndex,
        journal_file: &'a JournalFile<Mmap>,
        params: &'a LogQueryParams,
        anchor_usec: Microseconds,
        limit: usize,
    ) -> Self {
        if params.regex().is_some() {
            trace!("regex filtering enabled for query");
        }

        Self {
            file_index,
            journal_file,
            params,
            anchor_usec,
            limit,
            data_offsets_scratch: Vec::new(),
            data_match_cache: HashMap::default(),
            scratch_buffer: Vec::new(),
            regex_filtered_count: 0,
        }
    }

    fn collect(mut self, entry_offsets: &[NonZeroU64]) -> Result<Vec<LogEntryId>> {
        let entries = match self.params.direction() {
            Direction::Forward => self.collect_forward(entry_offsets)?,
            Direction::Backward => self.collect_backward(entry_offsets)?,
        };
        self.trace_regex_result(entries.len());
        Ok(entries)
    }

    fn collect_forward(&mut self, entry_offsets: &[NonZeroU64]) -> Result<Vec<LogEntryId>> {
        let mut entries = Vec::with_capacity(self.limit.min(entry_offsets.len()));
        let Some(start_idx) = self.forward_start_index(entry_offsets)? else {
            return Ok(entries);
        };

        for (idx, &entry_offset) in entry_offsets[start_idx..].iter().enumerate() {
            let timestamp = self.entry_timestamp(entry_offset)?;
            match forward_boundary_decision(timestamp, self.params) {
                BoundaryDecision::Include => {}
                BoundaryDecision::Skip => continue,
                BoundaryDecision::Stop => break,
            }
            if !self.matches_regex(entry_offset)? {
                continue;
            }

            entries.push(self.log_entry(entry_offset, timestamp, start_idx + idx));
            if entries.len() >= self.limit {
                break;
            }
        }
        Ok(entries)
    }

    fn collect_backward(&mut self, entry_offsets: &[NonZeroU64]) -> Result<Vec<LogEntryId>> {
        let mut entries = Vec::with_capacity(self.limit.min(entry_offsets.len()));
        let Some(start_idx) = self.backward_start_index(entry_offsets)? else {
            return Ok(entries);
        };

        for (idx, &entry_offset) in entry_offsets[..=start_idx].iter().rev().enumerate() {
            let timestamp = self.entry_timestamp(entry_offset)?;
            match backward_boundary_decision(timestamp, self.params) {
                BoundaryDecision::Include => {}
                BoundaryDecision::Skip => continue,
                BoundaryDecision::Stop => break,
            }
            if !self.matches_regex(entry_offset)? {
                continue;
            }

            entries.push(self.log_entry(entry_offset, timestamp, start_idx - idx));
            if entries.len() >= self.limit {
                break;
            }
        }
        Ok(entries)
    }

    fn forward_start_index(&self, entry_offsets: &[NonZeroU64]) -> Result<Option<usize>> {
        let start_idx = if let Some(resume_pos) = self.params.resume_position() {
            resume_pos + 1
        } else {
            partition_point_entries(entry_offsets, 0, entry_offsets.len(), |entry_offset| {
                Ok(self.entry_timestamp(entry_offset)? < self.anchor_usec.get())
            })?
        };
        Ok((start_idx < entry_offsets.len()).then_some(start_idx))
    }

    fn backward_start_index(&self, entry_offsets: &[NonZeroU64]) -> Result<Option<usize>> {
        if let Some(resume_pos) = self.params.resume_position() {
            return Ok((resume_pos > 0 && resume_pos < entry_offsets.len()).then(|| resume_pos - 1));
        }

        let partition_idx =
            partition_point_entries(entry_offsets, 0, entry_offsets.len(), |entry_offset| {
                Ok(self.entry_timestamp(entry_offset)? <= self.anchor_usec.get())
            })?;
        Ok((partition_idx > 0).then(|| partition_idx - 1))
    }

    fn entry_timestamp(&self, entry_offset: NonZeroU64) -> Result<u64> {
        get_entry_timestamp(
            self.journal_file,
            self.params.source_timestamp_field(),
            entry_offset,
        )
    }

    fn matches_regex(&mut self, entry_offset: NonZeroU64) -> Result<bool> {
        let Some(regex) = self.params.regex() else {
            return Ok(true);
        };
        let matches = entry_matches_regex(
            self.journal_file,
            entry_offset,
            regex,
            &mut self.data_match_cache,
            &mut self.data_offsets_scratch,
            &mut self.scratch_buffer,
        )?;
        if !matches {
            self.regex_filtered_count += 1;
        }
        Ok(matches)
    }

    fn log_entry(&self, entry_offset: NonZeroU64, timestamp: u64, position: usize) -> LogEntryId {
        LogEntryId {
            file: self.file_index.file.clone(),
            offset: entry_offset.get(),
            timestamp: Microseconds(timestamp),
            position,
        }
    }

    fn trace_regex_result(&self, matched_count: usize) {
        if self.params.regex().is_some() {
            trace!(
                "regex filtering complete: {} entries matched, {} entries filtered out",
                matched_count, self.regex_filtered_count
            );
        }
    }
}
