//! Histogram functionality for generating time-series data from journal files.
//!
//! This module provides types and services for computing histograms of journal log entries
//! over time ranges, with support for filtering and faceted field indexing.

use crate::{cache::FileIndexKey, error::Result, facets::Facets};
use journal_core::collections::HashSet;
use journal_index::{Bitmap, FieldName, FieldValuePair, FileIndex, Filter, Seconds};
use lru::LruCache;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::Duration;

#[allow(unused_imports)]
use tracing::{debug, error};

/// Calculate the appropriate bucket duration for a given time range.
///
/// This function determines the bucket size that will result in approximately
/// 50-100 buckets for the given time range. The bucket durations are selected
/// from a predefined set of "nice" values (1s, 2s, 5s, 10s, 1m, 5m, 1h, etc.)
/// to make the resulting histograms easy to interpret.
///
/// # Arguments
/// * `time_range_duration` - The duration of the time range in seconds
///
/// # Returns
/// The bucket duration in seconds
pub fn calculate_bucket_duration(time_range_duration: u32) -> u32 {
    const MINUTE: Duration = Duration::from_secs(60);
    const HOUR: Duration = Duration::from_secs(60 * MINUTE.as_secs());
    const DAY: Duration = Duration::from_secs(24 * HOUR.as_secs());

    const VALID_DURATIONS: &[Duration] = &[
        // Seconds
        Duration::from_secs(1),
        Duration::from_secs(2),
        Duration::from_secs(5),
        Duration::from_secs(10),
        Duration::from_secs(15),
        Duration::from_secs(30),
        // Minutes
        MINUTE,
        Duration::from_secs(2 * MINUTE.as_secs()),
        Duration::from_secs(3 * MINUTE.as_secs()),
        Duration::from_secs(5 * MINUTE.as_secs()),
        Duration::from_secs(10 * MINUTE.as_secs()),
        Duration::from_secs(15 * MINUTE.as_secs()),
        Duration::from_secs(30 * MINUTE.as_secs()),
        // Hours
        HOUR,
        Duration::from_secs(2 * HOUR.as_secs()),
        Duration::from_secs(6 * HOUR.as_secs()),
        Duration::from_secs(8 * HOUR.as_secs()),
        Duration::from_secs(12 * HOUR.as_secs()),
        // Days
        DAY,
        Duration::from_secs(2 * DAY.as_secs()),
        Duration::from_secs(3 * DAY.as_secs()),
        Duration::from_secs(5 * DAY.as_secs()),
        Duration::from_secs(7 * DAY.as_secs()),
        Duration::from_secs(14 * DAY.as_secs()),
        Duration::from_secs(30 * DAY.as_secs()),
    ];

    VALID_DURATIONS
        .iter()
        .rev()
        .find(|&&bucket_width| time_range_duration as u64 / bucket_width.as_secs() >= 50)
        .map(|d| d.as_secs())
        .unwrap_or(1) as u32
}

/// A bucket request contains a [start, end) time range along with the
/// filter that should be applied.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct BucketRequest {
    /// Start time of the bucket request
    pub start: Seconds,
    /// End time of the bucket request
    pub end: Seconds,
    /// Facets to use for file index
    pub facets: Facets,
    /// Applied filter expression
    pub filter_expr: Filter,
}

impl BucketRequest {
    /// The duration of the bucket request in seconds
    pub fn duration(&self) -> Seconds {
        self.end - self.start
    }
}

/// A bucket response containing aggregated field value counts.
#[derive(Debug, Clone)]
pub struct BucketResponse {
    /// Maps field=value pairs to (unfiltered, filtered) counts
    pub fv_counts: HashMap<FieldValuePair, (usize, usize)>,
    /// Set of fields that are not indexed
    pub unindexed_fields: HashSet<FieldName>,
    /// Total entry counts (unfiltered, filtered) in this bucket across all files
    pub total_entries: (usize, usize),
}

impl BucketResponse {
    /// Creates a new empty bucket response.
    pub(crate) fn new() -> Self {
        Self {
            fv_counts: HashMap::default(),
            unindexed_fields: HashSet::default(),
            total_entries: (0, 0),
        }
    }

    /// Get all indexed field names from this bucket response.
    pub fn indexed_fields(&self) -> HashSet<FieldName> {
        self.fv_counts
            .keys()
            .map(|pair| pair.extract_field())
            .collect()
    }
}

/// Represents a histogram of journal log entries over time.
///
/// A histogram contains bucketed data where each bucket represents a time range
/// and holds aggregated counts of field values and filtering results.
#[derive(Debug, Clone)]
pub struct Histogram {
    pub buckets: Vec<(BucketRequest, BucketResponse)>,
}

impl Histogram {
    /// Returns the start time of the histogram (first bucket's start time).
    pub fn start_time(&self) -> Seconds {
        let bucket_request = &self
            .buckets
            .first()
            .expect("histogram with at least one bucket")
            .0;
        bucket_request.start
    }

    /// Returns the end time of the histogram (last bucket's end time).
    pub fn end_time(&self) -> Seconds {
        let bucket_request = &self
            .buckets
            .last()
            .expect("histogram with at least one bucket")
            .0;
        bucket_request.end
    }

    /// Returns the duration of each bucket in seconds.
    pub fn bucket_duration(&self) -> Seconds {
        self.buckets
            .first()
            .expect("histogram with at least one bucket")
            .0
            .duration()
    }

    /// Returns all discovered field names from the histogram buckets in a deterministic order.
    pub fn discovered_fields(&self) -> Vec<FieldName> {
        // Collect all unique fields from all buckets
        let mut fields = HashSet::default();
        for (_, bucket_response) in &self.buckets {
            fields.extend(bucket_response.indexed_fields());
            fields.extend(bucket_response.unindexed_fields.iter().cloned());
        }

        let mut v: Vec<FieldName> = fields.into_iter().collect();
        v.sort();
        v
    }
}

/// Engine for computing histograms from journal files.
///
/// The engine maintains caches and resources for efficiently computing histograms
/// across multiple queries. It can be reused for multiple histogram computations.
pub struct HistogramEngine {
    responses: RwLock<LruCache<BucketRequest, BucketResponse>>,
}

impl HistogramEngine {
    /// Creates a new HistogramEngine with a default capacity of 1000 bucket responses.
    pub fn new() -> Self {
        Self::with_capacity(1000)
    }

    /// Creates a new HistogramEngine with the specified cache capacity.
    ///
    /// The capacity determines how many bucket responses will be cached before
    /// old entries are evicted using an LRU policy.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            responses: RwLock::new(LruCache::new(
                NonZeroUsize::new(capacity).expect("capacity must be non-zero"),
            )),
        }
    }

    /// Compute a histogram from pre-indexed files.
    ///
    /// This method allows you to compute histograms from file indexes that have
    /// already been loaded, avoiding redundant cache lookups and file discoveries.
    ///
    /// # Arguments
    /// * `indexed_files` - Pre-computed file indexes
    /// * `time_range` - Query time range with aligned boundaries and bucket duration
    /// * `facets` - Fields to index
    /// * `filter_expr` - Filter expression to apply
    pub fn compute_from_indexes(
        &self,
        indexed_files: &[(FileIndexKey, FileIndex)],
        time_range: &crate::QueryTimeRange,
        facets: &[String],
        filter_expr: &Filter,
    ) -> Result<Histogram> {
        let facets = Facets::new(facets);
        let bucket_requests = bucket_requests_for(time_range, &facets, filter_expr);
        let buckets_to_compute = self.buckets_to_compute(&bucket_requests);

        if buckets_to_compute.is_empty() {
            return Ok(self.histogram_from_cache(bucket_requests));
        }

        let (new_responses, bucket_cacheable) =
            compute_bucket_responses(indexed_files, &buckets_to_compute);
        self.cache_computed_responses(&new_responses, &bucket_cacheable);

        Ok(self.histogram_from_responses(bucket_requests, &new_responses))
    }

    fn buckets_to_compute(&self, bucket_requests: &[BucketRequest]) -> Vec<BucketRequest> {
        let responses = self.responses.read();
        bucket_requests
            .iter()
            .filter(|br| !responses.contains(br))
            .cloned()
            .collect()
    }

    fn cache_computed_responses(
        &self,
        new_responses: &HashMap<BucketRequest, BucketResponse>,
        bucket_cacheable: &HashMap<BucketRequest, bool>,
    ) {
        let mut responses_guard = self.responses.write();
        for (bucket_request, response) in new_responses {
            if bucket_cacheable
                .get(bucket_request)
                .copied()
                .unwrap_or(false)
            {
                responses_guard.put(bucket_request.clone(), response.clone());
            }
        }
    }

    fn histogram_from_responses(
        &self,
        bucket_requests: Vec<BucketRequest>,
        new_responses: &HashMap<BucketRequest, BucketResponse>,
    ) -> Histogram {
        let mut responses_guard = self.responses.write();
        let buckets = bucket_requests
            .into_iter()
            .filter_map(|bucket_request| {
                responses_guard
                    .get(&bucket_request)
                    .cloned()
                    .or_else(|| new_responses.get(&bucket_request).cloned())
                    .map(|response| (bucket_request, response))
            })
            .collect();

        Histogram { buckets }
    }

    fn histogram_from_cache(&self, bucket_requests: Vec<BucketRequest>) -> Histogram {
        let mut responses = self.responses.write();
        let buckets = bucket_requests
            .into_iter()
            .filter_map(|bucket_request| {
                responses
                    .get(&bucket_request)
                    .map(|response| (bucket_request, response.clone()))
            })
            .collect();

        Histogram { buckets }
    }
}

fn bucket_requests_for(
    time_range: &crate::QueryTimeRange,
    facets: &Facets,
    filter_expr: &Filter,
) -> Vec<BucketRequest> {
    time_range
        .buckets()
        .map(|(start, end)| BucketRequest {
            start: Seconds(start),
            end: Seconds(end),
            facets: facets.clone(),
            filter_expr: filter_expr.clone(),
        })
        .collect()
}

fn compute_bucket_responses(
    indexed_files: &[(FileIndexKey, FileIndex)],
    buckets_to_compute: &[BucketRequest],
) -> (
    HashMap<BucketRequest, BucketResponse>,
    HashMap<BucketRequest, bool>,
) {
    let mut new_responses = empty_bucket_responses(buckets_to_compute);
    let mut bucket_cacheable = initially_cacheable_buckets(buckets_to_compute);

    for (_, file_index) in indexed_files {
        process_file_buckets(
            file_index,
            buckets_to_compute,
            &mut new_responses,
            &mut bucket_cacheable,
        );
    }

    (new_responses, bucket_cacheable)
}

fn empty_bucket_responses(
    bucket_requests: &[BucketRequest],
) -> HashMap<BucketRequest, BucketResponse> {
    bucket_requests
        .iter()
        .map(|br| (br.clone(), BucketResponse::new()))
        .collect()
}

fn initially_cacheable_buckets(bucket_requests: &[BucketRequest]) -> HashMap<BucketRequest, bool> {
    bucket_requests
        .iter()
        .map(|br| (br.clone(), true))
        .collect()
}

fn process_file_buckets(
    file_index: &FileIndex,
    bucket_requests: &[BucketRequest],
    responses: &mut HashMap<BucketRequest, BucketResponse>,
    bucket_cacheable: &mut HashMap<BucketRequest, bool>,
) {
    for bucket_request in bucket_requests {
        let Some(response) = responses.get_mut(bucket_request) else {
            continue;
        };
        if !file_overlaps_bucket(file_index, bucket_request) {
            continue;
        }
        if file_index.online() {
            bucket_cacheable.insert(bucket_request.clone(), false);
        }

        let filter_bitmap = filter_bitmap_for_bucket(file_index, bucket_request);
        update_bucket_totals(file_index, bucket_request, filter_bitmap.as_ref(), response);
        record_unindexed_fields(file_index, response);
        count_indexed_field_values(file_index, bucket_request, filter_bitmap.as_ref(), response);
    }
}

fn file_overlaps_bucket(file_index: &FileIndex, bucket_request: &BucketRequest) -> bool {
    file_index.start_time() < bucket_request.end && file_index.end_time() > bucket_request.start
}

fn filter_bitmap_for_bucket(
    file_index: &FileIndex,
    bucket_request: &BucketRequest,
) -> Option<Bitmap> {
    (!bucket_request.filter_expr.is_none()).then(|| bucket_request.filter_expr.evaluate(file_index))
}

fn update_bucket_totals(
    file_index: &FileIndex,
    bucket_request: &BucketRequest,
    filter_bitmap: Option<&Bitmap>,
    response: &mut BucketResponse,
) {
    let all_entries = Bitmap::insert_range(0..file_index.total_entries() as u32);
    let unfiltered_total = count_entries(file_index, &all_entries, bucket_request);
    let filtered_total = filter_bitmap
        .map(|bitmap| count_entries(file_index, bitmap, bucket_request))
        .unwrap_or(unfiltered_total);

    response.total_entries.0 += unfiltered_total;
    response.total_entries.1 += filtered_total;
}

fn count_entries(file_index: &FileIndex, bitmap: &Bitmap, bucket_request: &BucketRequest) -> usize {
    file_index
        .count_entries_in_time_range(bitmap, bucket_request.start, bucket_request.end)
        .unwrap_or(0)
}

fn record_unindexed_fields(file_index: &FileIndex, response: &mut BucketResponse) {
    for field in file_index.fields() {
        if !file_index.is_indexed(field)
            && let Some(field_name) = FieldName::new(field)
        {
            response.unindexed_fields.insert(field_name);
        }
    }
}

fn count_indexed_field_values(
    file_index: &FileIndex,
    bucket_request: &BucketRequest,
    filter_bitmap: Option<&Bitmap>,
    response: &mut BucketResponse,
) {
    for (indexed_field, field_bitmap) in file_index.bitmaps() {
        let unfiltered_count = count_entries(file_index, field_bitmap, bucket_request);
        let filtered_count = filtered_field_count(
            file_index,
            bucket_request,
            field_bitmap,
            filter_bitmap,
            unfiltered_count,
        );
        add_field_counts(response, indexed_field, unfiltered_count, filtered_count);
    }
}

fn filtered_field_count(
    file_index: &FileIndex,
    bucket_request: &BucketRequest,
    field_bitmap: &Bitmap,
    filter_bitmap: Option<&Bitmap>,
    unfiltered_count: usize,
) -> usize {
    let Some(filter_bitmap) = filter_bitmap else {
        return unfiltered_count;
    };
    let filtered_bitmap = field_bitmap & filter_bitmap;
    count_entries(file_index, &filtered_bitmap, bucket_request)
}

fn add_field_counts(
    response: &mut BucketResponse,
    indexed_field: &FieldValuePair,
    unfiltered_count: usize,
    filtered_count: usize,
) {
    if let Some(pair) = FieldValuePair::parse(indexed_field) {
        let counts = response.fv_counts.entry(pair).or_insert((0, 0));
        counts.0 += unfiltered_count;
        counts.1 += filtered_count;
    }
}
