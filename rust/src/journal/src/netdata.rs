use crate::{
    Direction, ExplorerAnchor, ExplorerFieldMode, ExplorerFilter, ExplorerHistogram, ExplorerQuery,
    ExplorerResult, ExplorerRow, ExplorerStats, ExplorerStrategy, FileReader, ReaderOptions,
    Result, SdkError,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_FUNCTION_NAME: &str = "systemd-journal";
const DEFAULT_ITEMS_TO_RETURN: usize = 200;
const DEFAULT_TIME_WINDOW_SECONDS: i64 = 3600;
const DEFAULT_HISTOGRAM_BUCKETS: usize = 250;

const SYSTEMD_DEFAULT_VIEW_KEYS: &[&str] = &[
    "PRIORITY",
    "_HOSTNAME",
    "ND_JOURNAL_PROCESS",
    "MESSAGE",
    "SYSLOG_FACILITY",
    "ERRNO",
    "ND_JOURNAL_FILE",
    "SYSLOG_IDENTIFIER",
    "UNIT",
    "USER_UNIT",
    "MESSAGE_ID",
    "_BOOT_ID",
    "_SYSTEMD_OWNER_UID",
    "_UID",
    "OBJECT_SYSTEMD_OWNER_UID",
    "OBJECT_UID",
    "_GID",
    "OBJECT_GID",
    "_CAP_EFFECTIVE",
    "_AUDIT_LOGINUID",
    "OBJECT_AUDIT_LOGINUID",
    "_SOURCE_REALTIME_TIMESTAMP",
];

const SYSTEMD_DEFAULT_FACETS: &[&str] = &[
    "MESSAGE_ID",
    "PRIORITY",
    "CODE_FILE",
    "CODE_FUNC",
    "ERRNO",
    "SYSLOG_FACILITY",
    "SYSLOG_IDENTIFIER",
    "UNIT",
    "USER_UNIT",
    "UNIT_RESULT",
    "_UID",
    "_GID",
    "_COMM",
    "_EXE",
    "_CAP_EFFECTIVE",
    "_AUDIT_LOGINUID",
    "_SYSTEMD_CGROUP",
    "_SYSTEMD_SLICE",
    "_SYSTEMD_UNIT",
    "_SYSTEMD_USER_UNIT",
    "_SYSTEMD_USER_SLICE",
    "_SYSTEMD_SESSION",
    "_SYSTEMD_OWNER_UID",
    "_SELINUX_CONTEXT",
    "_BOOT_ID",
    "_MACHINE_ID",
    "_HOSTNAME",
    "_TRANSPORT",
    "_STREAM_ID",
    "_NAMESPACE",
    "_RUNTIME_SCOPE",
];

#[derive(Debug, Clone)]
pub struct NetdataFunctionConfig {
    pub function_name: String,
    pub default_facets: Vec<String>,
    pub default_view_keys: Vec<String>,
    pub default_histogram: Option<String>,
    pub reader_options: ReaderOptions,
    pub explorer_strategy: ExplorerStrategy,
}

impl NetdataFunctionConfig {
    pub fn systemd_journal() -> Self {
        Self {
            function_name: DEFAULT_FUNCTION_NAME.to_string(),
            default_facets: SYSTEMD_DEFAULT_FACETS
                .iter()
                .map(|field| (*field).to_string())
                .collect(),
            default_view_keys: SYSTEMD_DEFAULT_VIEW_KEYS
                .iter()
                .map(|field| (*field).to_string())
                .collect(),
            default_histogram: Some("PRIORITY".to_string()),
            reader_options: ReaderOptions::snapshot(),
            explorer_strategy: ExplorerStrategy::Traversal,
        }
    }
}

impl Default for NetdataFunctionConfig {
    fn default() -> Self {
        Self::systemd_journal()
    }
}

pub trait NetdataFunctionProfile {
    fn field_display_value(&self, _field: &str, value: &[u8]) -> Value {
        Value::String(String::from_utf8_lossy(value).into_owned())
    }

    fn facet_option_name(&self, field: &str, raw_value: &[u8]) -> String {
        match self.field_display_value(field, raw_value) {
            Value::String(value) => value,
            other => other.to_string(),
        }
    }

    fn row_options(&self, fields: &BTreeMap<String, Vec<Vec<u8>>>) -> Value {
        if let Some(priority) = first_value(fields, "PRIORITY") {
            return json!({ "severity": priority_to_row_severity(priority) });
        }
        json!({ "severity": "normal" })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemdJournalProfile;

impl NetdataFunctionProfile for SystemdJournalProfile {
    fn field_display_value(&self, field: &str, value: &[u8]) -> Value {
        let raw = String::from_utf8_lossy(value);
        match field {
            "PRIORITY" => Value::String(priority_name(&raw).unwrap_or(&raw).to_string()),
            "SYSLOG_FACILITY" => {
                Value::String(syslog_facility_name(&raw).unwrap_or(&raw).to_string())
            }
            _ => Value::String(raw.into_owned()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetdataJournalFunction<P = SystemdJournalProfile> {
    config: NetdataFunctionConfig,
    profile: P,
}

impl NetdataJournalFunction<SystemdJournalProfile> {
    pub fn systemd_journal() -> Self {
        Self {
            config: NetdataFunctionConfig::systemd_journal(),
            profile: SystemdJournalProfile,
        }
    }
}

impl<P> NetdataJournalFunction<P>
where
    P: NetdataFunctionProfile,
{
    pub fn new(config: NetdataFunctionConfig, profile: P) -> Self {
        Self { config, profile }
    }

    pub fn run_directory_request_json(&self, directory: &Path, request: &Value) -> Result<Value> {
        let request = NetdataRequest::parse(request, &self.config)?;
        if request.info {
            return Ok(self.info_response());
        }

        let paths = collect_journal_files(directory)?;
        let mut combined = self.explore_paths(&paths, &request)?;
        if !request.filters.is_empty() {
            let mut vocabulary_request = request.clone();
            vocabulary_request.filters.clear();
            vocabulary_request.histogram = None;
            vocabulary_request.limit = 0;
            let vocabulary = self.explore_paths(&paths, &vocabulary_request)?;
            combined.add_zero_count_facet_values(&vocabulary.facets);
        }
        Ok(self.query_response(request, paths, combined))
    }

    pub fn run_directory_request_bytes(&self, directory: &Path, request: &[u8]) -> Result<Value> {
        let request: Value = serde_json::from_slice(request).map_err(|err| {
            SdkError::InvalidPath(format!("invalid Netdata function JSON: {err}"))
        })?;
        self.run_directory_request_json(directory, &request)
    }

    fn explore_paths(&self, paths: &[PathBuf], request: &NetdataRequest) -> Result<CombinedResult> {
        let query = request.to_explorer_query();
        let mut combined = CombinedResult::default();
        for path in paths {
            let mut reader = match FileReader::open_with_options(path, self.config.reader_options) {
                Ok(reader) => reader,
                Err(err) => {
                    combined.skipped_files = combined.skipped_files.saturating_add(1);
                    combined
                        .file_errors
                        .push(format!("{}: {err}", path.display()));
                    continue;
                }
            };
            let result = match reader.explore_with_strategy(&query, self.config.explorer_strategy) {
                Ok(result) => result,
                Err(err) => {
                    combined.skipped_files = combined.skipped_files.saturating_add(1);
                    combined
                        .file_errors
                        .push(format!("{}: {err}", path.display()));
                    continue;
                }
            };
            combined.merge(path, result, query.direction);
        }
        combined.sort_and_limit(query.direction, query.limit);
        Ok(combined)
    }

    fn info_response(&self) -> Value {
        json!({
            "_request": { "info": true },
            "versions": { "netdata_function_api": 1, "sdk": env!("CARGO_PKG_VERSION") },
            "v": 3,
            "accepted_params": [
                "info", "delta", "tail", "slice", "data_only", "sampling", "after",
                "before", "if_modified_since", "anchor", "last", "direction", "query",
                "histogram", "facets", "selections"
            ],
            "required_params": [],
            "show_ids": false,
            "has_history": true,
            "pagination": true,
            "status": 200,
            "type": "table",
            "help": "Netdata-compatible journal log function backed by the systemd journal SDK"
        })
    }

    fn query_response(
        &self,
        request: NetdataRequest,
        paths: Vec<PathBuf>,
        combined: CombinedResult,
    ) -> Value {
        let columns = self.build_columns(&request, &combined.rows, &combined.facets);
        let data = self.build_data_rows(&columns.order, &combined.rows);
        let facets = self.build_facets(&request.facets, &combined.facets);
        let histogram = combined
            .histogram
            .as_ref()
            .map(|histogram| self.build_histogram(histogram));
        let returned = data.len() as u64;

        json!({
            "_request": request.echo,
            "versions": { "netdata_function_api": 1, "sdk": env!("CARGO_PKG_VERSION") },
            "_journal_files": {
                "matched": paths.len(),
                "skipped": combined.skipped_files,
                "errors": combined.file_errors,
            },
            "status": 200,
            "partial": false,
            "type": "table",
            "message": "OK",
            "update_every": 1,
            "help": null,
            "last_modified": 0,
            "show_ids": false,
            "has_history": true,
            "pagination": true,
            "accepted_params": [
                "info", "delta", "tail", "slice", "data_only", "sampling", "after",
                "before", "if_modified_since", "anchor", "last", "direction", "query",
                "histogram", "facets", "selections"
            ],
            "facets": facets,
            "columns": columns.map,
            "data": data,
            "default_sort_column": "timestamp",
            "default_charts": [],
            "available_histograms": self.available_histograms(&request),
            "histogram": histogram,
            "items": {
                "evaluated": combined.stats.rows_examined,
                "matched": combined.stats.rows_matched,
                "unsampled": 0,
                "estimated": 0,
                "returned": returned,
                "max_to_return": request.limit as u64,
                "before": 0,
                "after": combined.stats.rows_matched.saturating_sub(returned),
            },
            "_stats": {
                "sdk_explorer": combined.stats,
            },
            "expires": 0,
            "_sampling": { "enabled": false }
        })
    }

    fn build_columns(
        &self,
        request: &NetdataRequest,
        rows: &[LocatedRow],
        facets: &BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, u64>>,
    ) -> Columns {
        let mut order = vec!["timestamp".to_string(), "rowOptions".to_string()];
        push_unique_many(&mut order, &self.config.default_view_keys);
        push_unique_many(&mut order, &request.facets_as_strings());
        if let Some(histogram) = &request.histogram {
            push_unique(&mut order, histogram);
        }

        for field in facets.keys() {
            push_unique(&mut order, &String::from_utf8_lossy(field));
        }
        for row in rows {
            let fields = row_fields(row);
            for field in fields.keys() {
                push_unique(&mut order, field);
            }
        }

        let mut map = Map::new();
        for (index, key) in order.iter().enumerate() {
            map.insert(key.clone(), column_metadata(key, index));
        }
        Columns { order, map }
    }

    fn build_data_rows(&self, column_order: &[String], rows: &[LocatedRow]) -> Vec<Value> {
        rows.iter()
            .map(|located| {
                let fields = row_fields(located);
                let mut row = Vec::with_capacity(column_order.len());
                for column in column_order {
                    let value = match column.as_str() {
                        "timestamp" => Value::from(located.row.realtime_usec),
                        "rowOptions" => self.profile.row_options(&fields),
                        field => first_value(&fields, field)
                            .map(|value| self.profile.field_display_value(field, value))
                            .unwrap_or(Value::Null),
                    };
                    row.push(value);
                }
                Value::Array(row)
            })
            .collect()
    }

    fn build_facets(
        &self,
        requested: &[Vec<u8>],
        facets: &BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, u64>>,
    ) -> Value {
        let mut out = Vec::new();
        for (order, field) in requested.iter().enumerate() {
            let Some(values) = facets.get(field) else {
                continue;
            };
            let field_name = String::from_utf8_lossy(field).into_owned();
            let mut options: Vec<_> = values
                .iter()
                .filter(|(value, _)| value.as_slice() != b"-")
                .map(|(value, count)| {
                    json!({
                        "id": String::from_utf8_lossy(value).into_owned(),
                        "name": self.profile.facet_option_name(&field_name, value),
                        "count": count,
                    })
                })
                .collect();
            sort_facet_options(&field_name, &mut options);
            for (idx, option) in options.iter_mut().enumerate() {
                if let Some(object) = option.as_object_mut() {
                    object.insert("order".to_string(), Value::from((idx + 1) as u64));
                }
            }
            out.push(json!({
                "id": field_name,
                "name": String::from_utf8_lossy(field).into_owned(),
                "order": order + 1,
                "options": options,
            }));
        }
        Value::Array(out)
    }

    fn build_histogram(&self, histogram: &ExplorerHistogram) -> Value {
        let field = String::from_utf8_lossy(&histogram.field).into_owned();
        let mut dimension_ids = BTreeSet::new();
        for bucket in &histogram.buckets {
            for value in bucket.values.keys() {
                dimension_ids.insert(value.clone());
            }
        }
        let dimension_ids: Vec<Vec<u8>> = dimension_ids.into_iter().collect();
        let labels: Vec<Value> = std::iter::once(Value::String("time".to_string()))
            .chain(
                dimension_ids
                    .iter()
                    .map(|value| Value::String(self.profile.facet_option_name(&field, value))),
            )
            .collect();
        let data: Vec<Value> = histogram
            .buckets
            .iter()
            .map(|bucket| {
                let mut point = Vec::with_capacity(dimension_ids.len() + 1);
                point.push(Value::from(bucket.start_realtime_usec / 1000));
                for value in &dimension_ids {
                    point.push(json!([
                        bucket.values.get(value).copied().unwrap_or(0),
                        0,
                        0
                    ]));
                }
                Value::Array(point)
            })
            .collect();

        json!({
            "id": field,
            "name": field,
            "chart": {
                "result": {
                    "labels": labels,
                    "point": { "value": 0, "arp": 1, "pa": 2 },
                    "data": data,
                },
                "view": {
                    "title": format!("Events Distribution by {}", String::from_utf8_lossy(&histogram.field)),
                    "update_every": histogram_update_every_seconds(histogram),
                    "units": "events",
                    "chart_type": "stackedBar",
                }
            }
        })
    }

    fn available_histograms(&self, request: &NetdataRequest) -> Value {
        let histogram = request
            .histogram
            .as_deref()
            .or(self.config.default_histogram.as_deref())
            .unwrap_or("PRIORITY");
        json!([{ "id": histogram, "name": histogram }])
    }
}

#[derive(Debug, Clone)]
struct NetdataRequest {
    info: bool,
    echo: Value,
    after_realtime_usec: Option<u64>,
    before_realtime_usec: Option<u64>,
    anchor: ExplorerAnchor,
    direction: Direction,
    limit: usize,
    filters: Vec<ExplorerFilter>,
    facets: Vec<Vec<u8>>,
    histogram: Option<String>,
    fts_patterns: Vec<Vec<u8>>,
}

impl NetdataRequest {
    fn parse(value: &Value, config: &NetdataFunctionConfig) -> Result<Self> {
        let object = value.as_object().ok_or_else(|| {
            SdkError::InvalidPath("Netdata function request must be a JSON object".to_string())
        })?;
        let now_seconds = unix_now_seconds();
        let info = get_bool(object, "info").unwrap_or(false);
        let after = get_i64(object, "after");
        let before = get_i64(object, "before");
        let (after_realtime_usec, before_realtime_usec) =
            normalize_time_window(now_seconds, after, before);
        let direction = match get_str(object, "direction").unwrap_or("backward") {
            "forward" | "forwards" | "next" => Direction::Forward,
            _ => Direction::Backward,
        };
        let anchor = get_u64(object, "anchor")
            .map(normalize_timestamp_to_usec)
            .map(ExplorerAnchor::Realtime)
            .unwrap_or(ExplorerAnchor::Auto);
        let limit = get_u64(object, "last")
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_ITEMS_TO_RETURN);
        let facets = parse_string_array(object.get("facets"))
            .unwrap_or_else(|| config.default_facets.clone())
            .into_iter()
            .map(Vec::from)
            .collect();
        let histogram = get_str(object, "histogram")
            .map(ToOwned::to_owned)
            .or_else(|| config.default_histogram.clone());
        let fts_patterns = get_str(object, "query")
            .filter(|query| !query.is_empty())
            .map(|query| vec![query.as_bytes().to_vec()])
            .unwrap_or_default();
        let filters = parse_filters(object.get("selections"));

        Ok(Self {
            info,
            echo: value.clone(),
            after_realtime_usec,
            before_realtime_usec,
            anchor,
            direction,
            limit,
            filters,
            facets,
            histogram,
            fts_patterns,
        })
    }

    fn to_explorer_query(&self) -> ExplorerQuery {
        ExplorerQuery {
            after_realtime_usec: self.after_realtime_usec,
            before_realtime_usec: self.before_realtime_usec,
            anchor: self.anchor,
            direction: self.direction,
            limit: self.limit,
            filters: self.filters.clone(),
            facets: self.facets.clone(),
            histogram: self
                .histogram
                .as_ref()
                .map(|field| field.as_bytes().to_vec()),
            histogram_target_buckets: DEFAULT_HISTOGRAM_BUCKETS,
            fts_patterns: self.fts_patterns.clone(),
            field_mode: ExplorerFieldMode::FirstValue,
            exclude_facet_field_filters: false,
            use_source_realtime: true,
            realtime_slack_usec: 120_000_000,
        }
    }

    fn facets_as_strings(&self) -> Vec<String> {
        self.facets
            .iter()
            .map(|field| String::from_utf8_lossy(field).into_owned())
            .collect()
    }
}

#[derive(Debug, Clone)]
struct LocatedRow {
    file_path: PathBuf,
    row: ExplorerRow,
}

#[derive(Debug, Default)]
struct CombinedResult {
    rows: Vec<LocatedRow>,
    facets: BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, u64>>,
    histogram: Option<ExplorerHistogram>,
    stats: ExplorerStats,
    skipped_files: u64,
    file_errors: Vec<String>,
}

impl CombinedResult {
    fn merge(&mut self, path: &Path, result: ExplorerResult, direction: Direction) {
        self.merge_stats(result.stats);
        for row in result.rows {
            self.rows.push(LocatedRow {
                file_path: path.to_path_buf(),
                row,
            });
        }
        for (field, values) in result.facets {
            let target = self.facets.entry(field).or_default();
            for (value, count) in values {
                *target.entry(value).or_default() += count;
            }
        }
        if let Some(histogram) = result.histogram {
            merge_histogram(&mut self.histogram, histogram);
        }
        self.sort_and_limit(direction, usize::MAX);
    }

    fn sort_and_limit(&mut self, direction: Direction, limit: usize) {
        match direction {
            Direction::Forward => self.rows.sort_by_key(|row| row.row.realtime_usec),
            Direction::Backward => self
                .rows
                .sort_by(|left, right| right.row.realtime_usec.cmp(&left.row.realtime_usec)),
        }
        if self.rows.len() > limit {
            self.rows.truncate(limit);
        }
    }

    fn merge_stats(&mut self, stats: ExplorerStats) {
        self.stats.rows_examined = self.stats.rows_examined.saturating_add(stats.rows_examined);
        self.stats.rows_matched = self.stats.rows_matched.saturating_add(stats.rows_matched);
        self.stats.facet_rows_matched = self
            .stats
            .facet_rows_matched
            .saturating_add(stats.facet_rows_matched);
        self.stats.rows_returned = self.stats.rows_returned.saturating_add(stats.rows_returned);
        self.stats.data_refs_seen = self
            .stats
            .data_refs_seen
            .saturating_add(stats.data_refs_seen);
        self.stats.data_refs_skipped = self
            .stats
            .data_refs_skipped
            .saturating_add(stats.data_refs_skipped);
        self.stats.data_payloads_loaded = self
            .stats
            .data_payloads_loaded
            .saturating_add(stats.data_payloads_loaded);
        self.stats.data_objects_classified = self
            .stats
            .data_objects_classified
            .saturating_add(stats.data_objects_classified);
        self.stats.data_cache_hits = self
            .stats
            .data_cache_hits
            .saturating_add(stats.data_cache_hits);
        self.stats.data_cache_misses = self
            .stats
            .data_cache_misses
            .saturating_add(stats.data_cache_misses);
        self.stats.payloads_decompressed = self
            .stats
            .payloads_decompressed
            .saturating_add(stats.payloads_decompressed);
        self.stats.fts_scans = self.stats.fts_scans.saturating_add(stats.fts_scans);
        self.stats.facet_updates = self.stats.facet_updates.saturating_add(stats.facet_updates);
        self.stats.histogram_updates = self
            .stats
            .histogram_updates
            .saturating_add(stats.histogram_updates);
        self.stats.returned_row_expansions = self
            .stats
            .returned_row_expansions
            .saturating_add(stats.returned_row_expansions);
        self.stats.early_stop_opportunities = self
            .stats
            .early_stop_opportunities
            .saturating_add(stats.early_stop_opportunities);
        self.stats.early_stops = self.stats.early_stops.saturating_add(stats.early_stops);
    }

    fn add_zero_count_facet_values(
        &mut self,
        vocabulary: &BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, u64>>,
    ) {
        for (field, values) in vocabulary {
            let target = self.facets.entry(field.clone()).or_default();
            for value in values.keys() {
                target.entry(value.clone()).or_insert(0);
            }
        }
    }
}

struct Columns {
    order: Vec<String>,
    map: Map<String, Value>,
}

fn merge_histogram(target: &mut Option<ExplorerHistogram>, source: ExplorerHistogram) {
    let Some(target) = target else {
        *target = Some(source);
        return;
    };
    for (target_bucket, source_bucket) in target.buckets.iter_mut().zip(source.buckets) {
        for (value, count) in source_bucket.values {
            *target_bucket.values.entry(value).or_default() += count;
        }
    }
}

fn row_fields(row: &LocatedRow) -> BTreeMap<String, Vec<Vec<u8>>> {
    let mut fields = BTreeMap::new();
    for payload in &row.row.payloads {
        let Some((field, value)) = split_payload(payload) else {
            continue;
        };
        fields
            .entry(String::from_utf8_lossy(field).into_owned())
            .or_insert_with(Vec::new)
            .push(value.to_vec());
    }
    fields.insert(
        "ND_JOURNAL_FILE".to_string(),
        vec![row.file_path.display().to_string().into_bytes()],
    );
    if !fields.contains_key("ND_JOURNAL_PROCESS") {
        let process = dynamic_process_name(&fields);
        if !process.is_empty() {
            fields.insert("ND_JOURNAL_PROCESS".to_string(), vec![process.into_bytes()]);
        }
    }
    fields
}

fn dynamic_process_name(fields: &BTreeMap<String, Vec<Vec<u8>>>) -> String {
    let base = first_value(fields, "SYSLOG_IDENTIFIER")
        .or_else(|| first_value(fields, "_COMM"))
        .or_else(|| first_value(fields, "_EXE"))
        .map(|value| String::from_utf8_lossy(value).into_owned())
        .unwrap_or_default();
    if base.is_empty() {
        return base;
    }
    let pid = first_value(fields, "SYSLOG_PID")
        .or_else(|| first_value(fields, "_PID"))
        .map(|value| String::from_utf8_lossy(value).into_owned());
    match pid {
        Some(pid) if !pid.is_empty() => format!("{base}[{pid}]"),
        _ => base,
    }
}

fn first_value<'a>(fields: &'a BTreeMap<String, Vec<Vec<u8>>>, field: &str) -> Option<&'a [u8]> {
    fields
        .get(field)
        .and_then(|values| values.first())
        .map(Vec::as_slice)
}

fn split_payload(payload: &[u8]) -> Option<(&[u8], &[u8])> {
    let split = payload.iter().position(|byte| *byte == b'=')?;
    Some((&payload[..split], &payload[split + 1..]))
}

fn column_metadata(key: &str, index: usize) -> Value {
    let (visible, filter, full_width) = match key {
        "timestamp" => (true, "range", false),
        "rowOptions" => (false, "none", false),
        "_HOSTNAME" | "ND_JOURNAL_PROCESS" | "MESSAGE" => (true, "none", key == "MESSAGE"),
        "ND_JOURNAL_FILE" | "_SOURCE_REALTIME_TIMESTAMP" => (false, "none", false),
        _ => (false, "facet", false),
    };
    let column_type = if key == "timestamp" {
        "timestamp"
    } else {
        "string"
    };
    let visualization = if key == "rowOptions" {
        "rowOptions"
    } else {
        "value"
    };
    json!({
        "index": index,
        "unique_key": key == "timestamp",
        "name": key,
        "visible": visible,
        "type": column_type,
        "visualization": visualization,
        "value_options": {
            "transform": if key == "timestamp" { "datetime_usec" } else { "none" },
            "decimal_points": 0,
            "default_value": if key == "timestamp" { Value::Null } else { Value::String("-".to_string()) },
        },
        "sort": "ascending",
        "sortable": false,
        "sticky": false,
        "summary": "count",
        "filter": filter,
        "full_width": full_width,
        "wrap": key != "rowOptions",
        "default_expanded_filter": matches!(key, "PRIORITY" | "SYSLOG_FACILITY" | "MESSAGE_ID"),
    })
}

fn sort_facet_options(field: &str, options: &mut [Value]) {
    options.sort_by(|left, right| {
        let left_id = left.get("id").and_then(Value::as_str).unwrap_or_default();
        let right_id = right.get("id").and_then(Value::as_str).unwrap_or_default();
        if field == "PRIORITY" {
            return parse_priority(left_id).cmp(&parse_priority(right_id));
        }
        let left_count = left
            .get("count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let right_count = right
            .get("count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        right_count
            .cmp(&left_count)
            .then_with(|| left_id.cmp(right_id))
    });
}

fn parse_filters(value: Option<&Value>) -> Vec<ExplorerFilter> {
    let Some(Value::Object(selections)) = value else {
        return Vec::new();
    };
    let mut filters = Vec::new();
    for (field, values) in selections {
        if matches!(field.as_str(), "query" | "source") {
            continue;
        }
        let Some(values) = parse_string_array(Some(values)) else {
            continue;
        };
        filters.push(ExplorerFilter::new(
            field.as_bytes().to_vec(),
            values
                .into_iter()
                .map(|value| normalize_filter_value(field, &value)),
        ));
    }
    filters
}

fn normalize_filter_value(field: &str, value: &str) -> Vec<u8> {
    if field == "PRIORITY" {
        if let Some(priority) = priority_name_to_number(value) {
            return priority.as_bytes().to_vec();
        }
    }
    value.as_bytes().to_vec()
}

fn parse_string_array(value: Option<&Value>) -> Option<Vec<String>> {
    let Value::Array(items) = value? else {
        return None;
    };
    Some(
        items
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
    )
}

fn get_bool(object: &Map<String, Value>, key: &str) -> Option<bool> {
    object.get(key).and_then(Value::as_bool)
}

fn get_i64(object: &Map<String, Value>, key: &str) -> Option<i64> {
    object.get(key).and_then(Value::as_i64)
}

fn get_u64(object: &Map<String, Value>, key: &str) -> Option<u64> {
    object.get(key).and_then(Value::as_u64)
}

fn get_str<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    object.get(key).and_then(Value::as_str)
}

fn normalize_time_window(
    now_seconds: i64,
    after: Option<i64>,
    before: Option<i64>,
) -> (Option<u64>, Option<u64>) {
    let before = before.unwrap_or(now_seconds);
    let after = after.unwrap_or(before.saturating_sub(DEFAULT_TIME_WINDOW_SECONDS));
    (
        Some(normalize_after_timestamp_to_usec(now_seconds, after)),
        Some(normalize_before_timestamp_to_usec(now_seconds, before)),
    )
}

fn normalize_after_timestamp_to_usec(now_seconds: i64, value: i64) -> u64 {
    normalize_signed_timestamp_to_usec(now_seconds, value, false)
}

fn normalize_before_timestamp_to_usec(now_seconds: i64, value: i64) -> u64 {
    normalize_signed_timestamp_to_usec(now_seconds, value, true)
}

fn normalize_signed_timestamp_to_usec(now_seconds: i64, value: i64, end_of_second: bool) -> u64 {
    let absolute = if value < 0 {
        now_seconds.saturating_add(value)
    } else {
        value
    };
    normalize_timestamp_to_usec_with_rounding(absolute.max(0) as u64, end_of_second)
}

fn normalize_timestamp_to_usec(value: u64) -> u64 {
    normalize_timestamp_to_usec_with_rounding(value, false)
}

fn normalize_timestamp_to_usec_with_rounding(value: u64, end_of_second: bool) -> u64 {
    if value >= 1_000_000_000_000 {
        value
    } else if end_of_second {
        value.saturating_mul(1_000_000).saturating_add(999_999)
    } else {
        value.saturating_mul(1_000_000)
    }
}

fn unix_now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn collect_journal_files(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.is_dir() {
        return Err(SdkError::InvalidPath(format!(
            "not a directory: {}",
            path.display()
        )));
    }
    let mut files = Vec::new();
    let entries: Vec<_> = std::fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    for entry in &entries {
        let file_path = entry.path();
        if file_path.is_file() && is_journal_file_name(&file_path) {
            files.push(file_path);
        }
    }
    for entry in &entries {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !is_journal_subdir_name(&name) {
            continue;
        }
        let child_path = entry.path();
        if !child_path.is_dir() {
            continue;
        }
        let Ok(children) = std::fs::read_dir(&child_path) else {
            continue;
        };
        for child in children.flatten() {
            let file_path = child.path();
            if file_path.is_file() && is_journal_file_name(&file_path) {
                files.push(file_path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn is_journal_file_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with(".journal")
                || name.ends_with(".journal~")
                || name.ends_with(".journal.zst")
                || name.ends_with(".journal~.zst")
        })
}

fn is_journal_subdir_name(name: &str) -> bool {
    if name.contains('.') {
        return false;
    }
    match name.len() {
        32 => name.bytes().all(|byte| byte.is_ascii_hexdigit()),
        36 => name.bytes().enumerate().all(|(idx, byte)| {
            if matches!(idx, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        }),
        _ => false,
    }
}

fn push_unique_many(target: &mut Vec<String>, values: &[String]) {
    for value in values {
        push_unique(target, value);
    }
}

fn push_unique(target: &mut Vec<String>, value: impl AsRef<str>) {
    let value = value.as_ref();
    if !target.iter().any(|existing| existing == value) {
        target.push(value.to_string());
    }
}

fn histogram_update_every_seconds(histogram: &ExplorerHistogram) -> u64 {
    histogram
        .buckets
        .first()
        .map(|bucket| {
            bucket
                .end_realtime_usec
                .saturating_sub(bucket.start_realtime_usec)
                .checked_div(1_000_000)
                .unwrap_or(1)
                .max(1)
        })
        .unwrap_or(1)
}

fn priority_name(raw: &str) -> Option<&'static str> {
    match parse_priority(raw)? {
        0 => Some("emergency"),
        1 => Some("alert"),
        2 => Some("critical"),
        3 => Some("error"),
        4 => Some("warning"),
        5 => Some("notice"),
        6 => Some("info"),
        7 => Some("debug"),
        _ => None,
    }
}

fn priority_name_to_number(value: &str) -> Option<&'static str> {
    match value {
        "emergency" | "emerg" => Some("0"),
        "alert" => Some("1"),
        "critical" | "crit" => Some("2"),
        "error" | "err" => Some("3"),
        "warning" | "warn" => Some("4"),
        "notice" => Some("5"),
        "info" => Some("6"),
        "debug" => Some("7"),
        _ => None,
    }
}

fn parse_priority(raw: &str) -> Option<u8> {
    raw.parse::<u8>().ok()
}

fn priority_to_row_severity(raw: &[u8]) -> &'static str {
    let raw = String::from_utf8_lossy(raw);
    match parse_priority(&raw) {
        Some(priority) if priority <= 3 => "critical",
        Some(4) => "warning",
        Some(5) => "notice",
        Some(priority) if priority >= 7 => "debug",
        _ => "normal",
    }
}

fn syslog_facility_name(raw: &str) -> Option<&'static str> {
    match raw.parse::<u8>().ok()? {
        0 => Some("kern"),
        1 => Some("user"),
        2 => Some("mail"),
        3 => Some("daemon"),
        4 => Some("auth"),
        5 => Some("syslog"),
        6 => Some("lpr"),
        7 => Some("news"),
        8 => Some("uucp"),
        9 => Some("cron"),
        10 => Some("authpriv"),
        11 => Some("ftp"),
        12 => Some("ntp"),
        13 => Some("security"),
        14 => Some("console"),
        15 => Some("solaris-cron"),
        16 => Some("local0"),
        17 => Some("local1"),
        18 => Some("local2"),
        19 => Some("local3"),
        20 => Some("local4"),
        21 => Some("local5"),
        22 => Some("local6"),
        23 => Some("local7"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_netdata_selections_as_and_fields_or_values() {
        let request = json!({
            "after": 100,
            "before": 200,
            "direction": "forward",
            "last": 25,
            "facets": ["PRIORITY"],
            "selections": {
                "PRIORITY": ["warning", "error"],
                "_HOSTNAME": ["node-a"],
            }
        });

        let parsed = NetdataRequest::parse(&request, &NetdataFunctionConfig::systemd_journal())
            .expect("parse request");
        assert_eq!(parsed.after_realtime_usec, Some(100_000_000));
        assert_eq!(parsed.before_realtime_usec, Some(200_999_999));
        assert_eq!(parsed.direction, Direction::Forward);
        assert_eq!(parsed.limit, 25);
        assert_eq!(parsed.filters.len(), 2);
        assert_eq!(parsed.filters[0].field, b"PRIORITY");
        assert_eq!(parsed.filters[0].values, vec![b"4".to_vec(), b"3".to_vec()]);
        assert_eq!(parsed.filters[1].field, b"_HOSTNAME");
        assert_eq!(parsed.filters[1].values, vec![b"node-a".to_vec()]);
    }

    #[test]
    fn systemd_profile_transforms_priority_and_facility_for_display() {
        let profile = SystemdJournalProfile;
        assert_eq!(
            profile.field_display_value("PRIORITY", b"7"),
            json!("debug")
        );
        assert_eq!(
            profile.field_display_value("SYSLOG_FACILITY", b"3"),
            json!("daemon")
        );
        assert_eq!(priority_to_row_severity(b"3"), "critical");
        assert_eq!(priority_to_row_severity(b"6"), "normal");
    }
}
