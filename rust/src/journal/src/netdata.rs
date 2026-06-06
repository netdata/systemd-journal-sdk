use crate::{
    Direction, ExplorerAnchor, ExplorerControl, ExplorerFieldMode, ExplorerFilter,
    ExplorerHistogram, ExplorerProgress, ExplorerQuery, ExplorerResult, ExplorerRow, ExplorerStats,
    ExplorerStopReason, ExplorerStrategy, FileReader, ReaderOptions, Result, SdkError,
};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
#[cfg(unix)]
use std::ffi::CStr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_FUNCTION_NAME: &str = "systemd-journal";
const DEFAULT_ITEMS_TO_RETURN: usize = 200;
const DEFAULT_TIME_WINDOW_SECONDS: i64 = 3600;
const DEFAULT_ITEMS_SAMPLING: u64 = 1_000_000;
const DATA_ONLY_CHECK_EVERY_ROWS: u64 = 128;
const API_RELATIVE_TIME_MAX_SECONDS: i64 = 3 * 365 * 86_400;
const NETDATA_MISSING_AFTER_RELATIVE_SECONDS: i64 = 600;
const DEFAULT_HISTOGRAM_BUCKETS: usize = 150;
const EFFECTIVELY_DISABLED_TIMEOUT_SECONDS: u64 = 100 * 365 * 24 * 60 * 60;
const NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC: u64 = 5_000_000;
const NETDATA_JOURNAL_VS_REALTIME_DELTA_MAX_USEC: u64 = 2 * 60 * 1_000_000;
const NETDATA_EMPTY_STRING_FACET_HASH_ID: &str = "CzGfAU2z3TC";
const NETDATA_UNAVAILABLE_FIELD_LABEL: &str = "[unavailable field]";
const NETDATA_FACET_MAX_VALUE_LENGTH: usize = 8192;
const SOURCE_TYPE_ALL: u64 = 1 << 0;
const SOURCE_TYPE_LOCAL_ALL: u64 = 1 << 1;
const SOURCE_TYPE_REMOTE_ALL: u64 = 1 << 2;
const SOURCE_TYPE_LOCAL_SYSTEM: u64 = 1 << 3;
const SOURCE_TYPE_LOCAL_USER: u64 = 1 << 4;
const SOURCE_TYPE_LOCAL_NAMESPACE: u64 = 1 << 5;
const SOURCE_TYPE_LOCAL_OTHER: u64 = 1 << 6;

const NETDATA_ACCEPTED_PARAMS: &[&str] = &[
    "info",
    "__logs_sources",
    "after",
    "before",
    "anchor",
    "direction",
    "last",
    "query",
    "facets",
    "histogram",
    "if_modified_since",
    "data_only",
    "delta",
    "tail",
    "sampling",
    "slice",
];

const SYSTEMD_DEFAULT_VIEW_KEYS: &[&str] = &[
    "_HOSTNAME",
    "ND_JOURNAL_PROCESS",
    "MESSAGE",
    "PRIORITY",
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
    "_HOSTNAME",
    "PRIORITY",
    "SYSLOG_FACILITY",
    "ERRNO",
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
    "_AUDIT_LOGINUID",
    "OBJECT_AUDIT_LOGINUID",
    "CODE_FILE",
    "_SYSTEMD_UNIT",
    "_SYSTEMD_USER_SLICE",
    "CODE_FUNC",
    "_TRANSPORT",
    "_COMM",
    "_RUNTIME_SCOPE",
    "_MACHINE_ID",
    "_SYSTEMD_SLICE",
    "UNIT_RESULT",
    "_SYSTEMD_CGROUP",
    "_EXE",
    "_SYSTEMD_USER_UNIT",
    "_SYSTEMD_SESSION",
    "COREDUMP_CGROUP",
    "COREDUMP_USER_UNIT",
    "COREDUMP_UNIT",
    "COREDUMP_SIGNAL_NAME",
    "COREDUMP_COMM",
    "_UDEV_DEVNODE",
    "_KERNEL_SUBSYSTEM",
    "OBJECT_EXE",
    "OBJECT_SYSTEMD_CGROUP",
    "OBJECT_COMM",
    "OBJECT_SYSTEMD_UNIT",
    "OBJECT_SYSTEMD_USER_UNIT",
    "_SELINUX_CONTEXT",
    "_NAMESPACE",
    "OBJECT_SYSTEMD_SESSION",
    "CONTAINER_ID",
    "CONTAINER_NAME",
    "CONTAINER_TAG",
    "IMAGE_NAME",
    "ND_NIDL_NODE",
    "ND_NIDL_CONTEXT",
    "ND_LOG_SOURCE",
    "ND_ALERT_NAME",
    "ND_ALERT_CLASS",
    "ND_ALERT_COMPONENT",
    "ND_ALERT_TYPE",
    "ND_ALERT_STATUS",
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

#[derive(Debug, Default)]
pub struct DisplayContext {
    boot_first_realtime: BTreeMap<Vec<u8>, u64>,
}

#[derive(Debug, Clone, Copy)]
pub enum DisplayScope {
    Data,
    Facet,
    Histogram,
}

pub trait NetdataFunctionProfile {
    fn field_display_value(
        &self,
        _context: &DisplayContext,
        _scope: DisplayScope,
        _field: &str,
        value: &[u8],
    ) -> Value {
        Value::String(String::from_utf8_lossy(value).into_owned())
    }

    fn facet_option_name(&self, context: &DisplayContext, field: &str, raw_value: &[u8]) -> String {
        match self.field_display_value(context, DisplayScope::Facet, field, raw_value) {
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

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemdJournalPluginProfile;

impl NetdataFunctionProfile for SystemdJournalProfile {
    fn field_display_value(
        &self,
        context: &DisplayContext,
        scope: DisplayScope,
        field: &str,
        value: &[u8],
    ) -> Value {
        systemd_field_display_value(context, scope, field, value, false)
    }
}

impl NetdataFunctionProfile for SystemdJournalPluginProfile {
    fn field_display_value(
        &self,
        context: &DisplayContext,
        scope: DisplayScope,
        field: &str,
        value: &[u8],
    ) -> Value {
        systemd_field_display_value(context, scope, field, value, true)
    }
}

#[derive(Debug, Clone)]
pub struct NetdataJournalFunction<P = SystemdJournalProfile> {
    config: NetdataFunctionConfig,
    profile: P,
}

#[derive(Debug, Clone)]
pub struct NetdataFunctionProgress {
    pub current_file: usize,
    pub total_files: usize,
    pub matched_files: u64,
    pub skipped_files: u64,
    pub stats: ExplorerStats,
    pub elapsed: Duration,
}

pub struct NetdataFunctionRunOptions<'a> {
    pub timeout: Option<Duration>,
    pub progress_callback: Option<&'a mut dyn FnMut(NetdataFunctionProgress)>,
    pub cancellation_callback: Option<&'a dyn Fn() -> bool>,
    pub progress_interval: Duration,
}

impl NetdataFunctionRunOptions<'_> {
    pub fn from_timeout_seconds(seconds: u64) -> Self {
        let seconds = if seconds == 0 {
            EFFECTIVELY_DISABLED_TIMEOUT_SECONDS
        } else {
            seconds
        };
        Self {
            timeout: Some(Duration::from_secs(seconds)),
            progress_callback: None,
            cancellation_callback: None,
            progress_interval: Duration::from_millis(250),
        }
    }
}

impl Default for NetdataFunctionRunOptions<'_> {
    fn default() -> Self {
        Self {
            timeout: Some(Duration::from_secs(EFFECTIVELY_DISABLED_TIMEOUT_SECONDS)),
            progress_callback: None,
            cancellation_callback: None,
            progress_interval: Duration::from_millis(250),
        }
    }
}

impl NetdataJournalFunction<SystemdJournalProfile> {
    pub fn systemd_journal() -> Self {
        Self {
            config: NetdataFunctionConfig::systemd_journal(),
            profile: SystemdJournalProfile,
        }
    }
}

impl NetdataJournalFunction<SystemdJournalPluginProfile> {
    pub fn systemd_journal_plugin_compatible() -> Self {
        Self {
            config: NetdataFunctionConfig::systemd_journal(),
            profile: SystemdJournalPluginProfile,
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
        self.run_directory_request_json_with_options(
            directory,
            request,
            NetdataFunctionRunOptions::default(),
        )
    }

    pub fn run_directory_request_json_with_options(
        &self,
        directory: &Path,
        request: &Value,
        mut options: NetdataFunctionRunOptions<'_>,
    ) -> Result<Value> {
        let request = NetdataRequest::parse(request, &self.config)?;
        let mut paths = collect_journal_files(directory)?;
        if request.info {
            return Ok(self.info_response(request.echo, &paths));
        }

        paths.retain(|path| request.matches_source(path));
        sort_journal_files_for_request(&mut paths, request.direction, self.config.reader_options);
        let deadline = options.timeout.map(|timeout| Instant::now() + timeout);
        let mut combined = self.explore_paths(&paths, &request, deadline, &mut options)?;
        if !request.data_only {
            combined.add_zero_count_facet_values_from_files(
                &request.facets,
                self.config.reader_options,
            );
        }
        if !request.data_only && !combined.partial && !request.filters.is_empty() {
            let mut vocabulary_request = request.clone();
            vocabulary_request.filters.clear();
            vocabulary_request.histogram = None;
            vocabulary_request.limit = 0;
            let vocabulary =
                self.explore_paths(&paths, &vocabulary_request, deadline, &mut options)?;
            combined.add_zero_count_facet_values(&vocabulary.facets);
        }
        Ok(self.query_response(request, paths, combined))
    }

    pub fn run_directory_request_bytes(&self, directory: &Path, request: &[u8]) -> Result<Value> {
        self.run_directory_request_bytes_with_options(
            directory,
            request,
            NetdataFunctionRunOptions::default(),
        )
    }

    pub fn run_directory_request_bytes_with_options(
        &self,
        directory: &Path,
        request: &[u8],
        options: NetdataFunctionRunOptions<'_>,
    ) -> Result<Value> {
        let request: Value = serde_json::from_slice(request).map_err(|err| {
            SdkError::InvalidPath(format!("invalid Netdata function JSON: {err}"))
        })?;
        self.run_directory_request_json_with_options(directory, &request, options)
    }

    fn explore_paths(
        &self,
        paths: &[PathBuf],
        request: &NetdataRequest,
        deadline: Option<Instant>,
        options: &mut NetdataFunctionRunOptions<'_>,
    ) -> Result<CombinedResult> {
        let query = request.to_explorer_query();
        let mut combined = CombinedResult::default();
        let started = Instant::now();
        let total_files = paths.len();
        for (file_index, path) in paths.iter().enumerate() {
            if options
                .cancellation_callback
                .is_some_and(|is_cancelled| is_cancelled())
            {
                combined.partial = true;
                combined.cancelled = true;
                break;
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                combined.partial = true;
                combined.timed_out = true;
                break;
            }
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
            if !file_may_overlap_request(reader.header(), request) {
                continue;
            }
            combined.matched_files = combined.matched_files.saturating_add(1);
            combined.matched_paths.push(path.clone());
            if !request.data_only {
                match reader.enumerate_fields_indexed() {
                    Ok(fields) => combined.add_column_fields(fields),
                    Err(err) => combined.file_errors.push(format!(
                        "{}: FIELD index enumeration failed: {err}",
                        path.display()
                    )),
                }
            }
            let explored = {
                let mut explorer_progress = |progress: ExplorerProgress| {
                    let stats = merged_progress_stats(&combined.stats, &progress.stats);
                    if let Some(callback) = options.progress_callback.as_deref_mut() {
                        callback(NetdataFunctionProgress {
                            current_file: file_index + 1,
                            total_files,
                            matched_files: combined.matched_files,
                            skipped_files: combined.skipped_files,
                            stats,
                            elapsed: started.elapsed(),
                        });
                    }
                };
                let mut control = ExplorerControl::new();
                control.set_deadline(deadline);
                control.set_cancellation_callback(options.cancellation_callback);
                control.set_progress_interval(options.progress_interval);
                control.set_progress_callback(Some(&mut explorer_progress));
                let result = reader.explore_with_strategy_cursor_rows_controlled(
                    &query,
                    self.config.explorer_strategy,
                    &mut control,
                );
                let stop_reason = control.stop_reason();
                result.map(|result| (result, stop_reason))
            };
            let (result, stop_reason) = match explored {
                Ok(result) => result,
                Err(err) => {
                    combined.skipped_files = combined.skipped_files.saturating_add(1);
                    combined
                        .file_errors
                        .push(format!("{}: {err}", path.display()));
                    continue;
                }
            };
            combined.merge(path, result, query.direction, query.limit);
            if let Some(callback) = options.progress_callback.as_deref_mut() {
                callback(NetdataFunctionProgress {
                    current_file: file_index + 1,
                    total_files,
                    matched_files: combined.matched_files,
                    skipped_files: combined.skipped_files,
                    stats: combined.stats.clone(),
                    elapsed: started.elapsed(),
                });
            }
            if let Some(reason) = stop_reason {
                combined.partial = true;
                match reason {
                    ExplorerStopReason::TimedOut => combined.timed_out = true,
                    ExplorerStopReason::Cancelled => combined.cancelled = true,
                }
                break;
            }
        }
        combined.expand_row_payloads(self.config.reader_options);
        Ok(combined)
    }

    fn info_response(&self, echo: Value, paths: &[PathBuf]) -> Value {
        json!({
            "_request": echo,
            "versions": { "netdata_function_api": 1, "sdk": env!("CARGO_PKG_VERSION") },
            "v": 3,
            "accepted_params": self.accepted_params_from_fields(&[]),
            "required_params": self.required_source_params(paths),
            "show_ids": true,
            "has_history": true,
            "pagination": {
                "enabled": true,
                "key": "anchor",
                "column": "timestamp",
                "units": "timestamp_usec",
            },
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
        let not_modified = request.if_modified_since_usec != 0
            && !combined.partial
            && combined.stats.last_realtime_usec <= request.if_modified_since_usec;
        if combined.cancelled {
            return netdata_function_error(499, "Request cancelled.");
        }
        if not_modified {
            return netdata_function_error(304, "No new data since the previous call.");
        }
        let columns = self.build_columns(
            &request,
            &combined.rows,
            &combined.facets,
            &combined.column_fields,
        );
        let context = DisplayContext {
            boot_first_realtime: collect_boot_first_realtime(&paths, self.config.reader_options),
        };
        let data = self.build_data_rows(&context, &columns.order, &combined.rows);
        let facets = self.build_facets(&context, &request.facets, &combined.facets);
        let histogram = combined.histogram.as_ref().map(|histogram| {
            self.build_histogram(&context, histogram, combined.facets.get(&histogram.field))
        });
        let returned = data.len() as u64;
        let status = 200;
        let message = if combined.timed_out {
            timeout_message()
        } else {
            Value::String("OK".to_string())
        };
        let items = json!({
            "evaluated": combined.stats.rows_examined,
            "matched": combined.stats.rows_matched,
            "unsampled": 0,
            "estimated": 0,
            "returned": returned,
            "max_to_return": request.limit as u64,
            "before": 0,
            "after": combined.stats.rows_matched.saturating_sub(returned),
        });

        let mut response = json!({
            "_request": request.echo,
            "versions": { "netdata_function_api": 1, "sdk": env!("CARGO_PKG_VERSION") },
            "_journal_files": {
                "matched": combined.matched_files,
                "skipped": combined.skipped_files,
                "errors": combined.file_errors,
            },
            "status": status,
            "partial": combined.partial,
            "type": "table",
            "show_ids": true,
            "has_history": true,
            "pagination": {
                "enabled": true,
                "key": "anchor",
                "column": "timestamp",
                "units": "timestamp_usec",
            },
            "columns": columns.map,
            "data": data,
            "_stats": {
                "sdk_explorer": combined.stats,
            },
            "expires": if request.data_only {
                unix_now_seconds().saturating_add(3600)
            } else {
                0
            },
            "_sampling": {
                "enabled": request.sampling != 0,
                "sampled": 0,
                "unsampled": 0,
                "estimated": 0,
            }
        });

        let object = response
            .as_object_mut()
            .expect("Netdata response root must be an object");
        if !request.data_only {
            object.insert("message".to_string(), message);
            object.insert("update_every".to_string(), Value::from(1));
            object.insert("help".to_string(), Value::Null);
            object.insert(
                "accepted_params".to_string(),
                self.accepted_params_from_fields(&request.facet_field_names()),
            );
            object.insert("default_sort_column".to_string(), Value::from("timestamp"));
            object.insert("default_charts".to_string(), Value::Array(Vec::new()));
            object.insert(
                "available_histograms".to_string(),
                self.available_histograms(&request, &combined),
            );
        } else if request.histogram.is_some() {
            object.insert(
                "available_histograms".to_string(),
                self.available_histograms(&request, &combined),
            );
        }

        if !request.data_only || request.tail {
            object.insert(
                "last_modified".to_string(),
                Value::from(combined.stats.last_realtime_usec),
            );
        }

        if !request.data_only || request.delta {
            let facets_key = if request.data_only {
                "facets_delta"
            } else {
                "facets"
            };
            let histogram_key = if request.data_only {
                "histogram_delta"
            } else {
                "histogram"
            };
            let items_key = if request.data_only {
                "items_delta"
            } else {
                "items"
            };
            object.insert(facets_key.to_string(), facets);
            object.insert(histogram_key.to_string(), histogram.unwrap_or(Value::Null));
            object.insert(items_key.to_string(), items);
        }

        response
    }

    fn build_columns(
        &self,
        request: &NetdataRequest,
        rows: &[LocatedRow],
        facets: &BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, u64>>,
        column_fields: &BTreeSet<String>,
    ) -> Columns {
        let mut order = vec!["timestamp".to_string(), "rowOptions".to_string()];
        push_unique_many(&mut order, &self.config.default_view_keys);
        push_unique_many(&mut order, &request.facet_field_names());
        if let Some(histogram) = &request.histogram {
            push_unique(&mut order, histogram);
        }
        for field in column_fields {
            push_unique(&mut order, field);
        }

        for (field, values) in facets {
            if !facet_group_is_reportable(values) {
                continue;
            }
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

    fn build_data_rows(
        &self,
        context: &DisplayContext,
        column_order: &[String],
        rows: &[LocatedRow],
    ) -> Vec<Value> {
        rows.iter()
            .map(|located| {
                let fields = row_fields(located);
                let mut row = Vec::with_capacity(column_order.len());
                for column in column_order {
                    let value = match column.as_str() {
                        "timestamp" => Value::from(located.row.realtime_usec),
                        "rowOptions" => self.profile.row_options(&fields),
                        field => first_value(&fields, field)
                            .map(|value| {
                                self.profile.field_display_value(
                                    context,
                                    DisplayScope::Data,
                                    field,
                                    value,
                                )
                            })
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
        context: &DisplayContext,
        requested: &[Vec<u8>],
        facets: &BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, u64>>,
    ) -> Value {
        let mut out = Vec::new();
        for (order, field) in requested.iter().enumerate() {
            let values = facets.get(field);
            let field_name = String::from_utf8_lossy(field).into_owned();
            let mut options: Vec<_> = values
                .into_iter()
                .flat_map(|values| values.iter())
                .filter(|(value, count)| {
                    (!value.is_empty() && value.as_slice() != b"-")
                        || (**count == 0 && value.is_empty())
                })
                .map(|(value, count)| {
                    if *count == 0 && value.is_empty() {
                        return json!({
                            "id": NETDATA_EMPTY_STRING_FACET_HASH_ID,
                            "name": NETDATA_UNAVAILABLE_FIELD_LABEL,
                            "count": count,
                        });
                    }
                    json!({
                        "id": String::from_utf8_lossy(value).into_owned(),
                        "name": self.profile.facet_option_name(context, &field_name, value),
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

    fn build_histogram(
        &self,
        context: &DisplayContext,
        histogram: &ExplorerHistogram,
        known_values: Option<&BTreeMap<Vec<u8>, u64>>,
    ) -> Value {
        let field = String::from_utf8_lossy(&histogram.field).into_owned();
        let mut dimension_ids = BTreeSet::new();
        let mut buckets = Vec::with_capacity(histogram.buckets.len());
        for bucket in &histogram.buckets {
            let mut values = BTreeMap::new();
            for (value, count) in &bucket.values {
                add_netdata_facet_count(&mut values, value, *count);
            }
            for value in values.keys() {
                dimension_ids.insert(value.clone());
            }
            buckets.push((bucket.start_realtime_usec, values));
        }
        let actual_dimension_ids = dimension_ids.clone();
        if let Some(known_values) = known_values {
            for value in known_values.keys() {
                if value.is_empty() || value.as_slice() == b"-" {
                    continue;
                }
                dimension_ids.insert(value.clone());
            }
        }
        let dimension_ids: Vec<Vec<u8>> = dimension_ids.into_iter().collect();
        let labels: Vec<Value> = std::iter::once(Value::String("time".to_string()))
            .chain(dimension_ids.iter().map(|value| {
                match self.profile.field_display_value(
                    context,
                    DisplayScope::Histogram,
                    &field,
                    value,
                ) {
                    Value::String(value) => Value::String(value),
                    other => Value::String(other.to_string()),
                }
            }))
            .collect();
        let data: Vec<Value> = buckets
            .iter()
            .map(|(start_realtime_usec, values)| {
                let mut point = Vec::with_capacity(dimension_ids.len() + 1);
                point.push(Value::from(start_realtime_usec / 1000));
                for value in &dimension_ids {
                    let count = values
                        .get(value)
                        .copied()
                        .map(Value::from)
                        .unwrap_or_else(|| {
                            if actual_dimension_ids.contains(value) {
                                Value::from(0)
                            } else {
                                Value::Null
                            }
                        });
                    point.push(Value::Array(vec![count, Value::from(0), Value::from(0)]));
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

    fn accepted_params_from_fields(&self, fields: &[String]) -> Value {
        NETDATA_ACCEPTED_PARAMS
            .iter()
            .copied()
            .chain(fields.iter().map(String::as_str))
            .map(|field| Value::String(field.to_string()))
            .collect()
    }

    fn required_source_params(&self, paths: &[PathBuf]) -> Value {
        let summary = JournalSourceSummary::from_paths(paths, self.config.reader_options);
        let options: Vec<Value> = ["all", "all-local-logs", "all-local-system-logs"]
            .into_iter()
            .map(|id| {
                json!({
                    "id": id,
                    "name": id,
                    "info": summary.info(),
                    "pill": human_binary_size(summary.total_size),
                })
            })
            .collect();
        json!([{
            "id": "__logs_sources",
            "name": "Journal Sources",
            "help": "Select the logs source to query",
            "type": "multiselect",
            "options": options,
        }])
    }

    fn available_histograms(&self, request: &NetdataRequest, combined: &CombinedResult) -> Value {
        let mut fields = request.facet_field_names();
        for field in combined.reportable_facet_fields(&request.facets) {
            push_unique(&mut fields, &field);
        }
        for field in &request.facets {
            if let Ok(field) = std::str::from_utf8(field) {
                push_unique(&mut fields, field);
            }
        }
        if request.data_only {
            if let Some(histogram) = &request.histogram {
                push_unique(&mut fields, histogram);
            }
        }
        let mut sorted = fields.clone();
        sorted.sort_by(|left, right| netdata_reorder_key(left).cmp(&netdata_reorder_key(right)));
        let order_by_field: BTreeMap<String, usize> = sorted
            .into_iter()
            .enumerate()
            .map(|(index, field)| (field, index + 1))
            .collect();

        fields
            .into_iter()
            .map(|field| {
                let order = order_by_field.get(&field).copied().unwrap_or(0);
                json!({
                    "id": field,
                    "name": field,
                    "order": order,
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
struct NetdataRequest {
    info: bool,
    echo: Value,
    after_realtime_usec: Option<u64>,
    before_realtime_usec: Option<u64>,
    if_modified_since_usec: u64,
    anchor: ExplorerAnchor,
    direction: Direction,
    limit: usize,
    data_only: bool,
    delta: bool,
    tail: bool,
    sampling: u64,
    source_type: u64,
    exact_sources: Vec<String>,
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
        let mut direction = match get_str(object, "direction").unwrap_or("backward") {
            "forward" | "forwards" | "next" => Direction::Forward,
            _ => Direction::Backward,
        };
        let if_modified_since_usec = get_u64(object, "if_modified_since").unwrap_or_default();
        let data_only = get_bool(object, "data_only").unwrap_or(false);
        let delta = data_only && get_bool(object, "delta").unwrap_or(false);
        let tail =
            data_only && if_modified_since_usec != 0 && get_bool(object, "tail").unwrap_or(false);
        let sampling = get_u64(object, "sampling").unwrap_or(DEFAULT_ITEMS_SAMPLING);
        let mut anchor = get_u64(object, "anchor")
            .map(normalize_timestamp_to_usec)
            .map(ExplorerAnchor::Realtime)
            .unwrap_or(ExplorerAnchor::Auto);
        if tail && matches!(anchor, ExplorerAnchor::Realtime(_)) {
            direction = Direction::Backward;
        }
        if let ExplorerAnchor::Realtime(anchor_usec) = anchor {
            let out_of_range = after_realtime_usec.is_some_and(|after| anchor_usec < after)
                || before_realtime_usec.is_some_and(|before| anchor_usec > before);
            if out_of_range {
                anchor = ExplorerAnchor::Auto;
                direction = Direction::Backward;
            }
        }
        let limit = get_u64(object, "last")
            .filter(|value| *value != 0)
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_ITEMS_TO_RETURN);
        let requested_facets = parse_string_array(object.get("facets"));
        let facets = requested_facets
            .clone()
            .unwrap_or_else(|| config.default_facets.clone())
            .into_iter()
            .map(Vec::from)
            .collect();
        let requested_histogram = get_str(object, "histogram")
            .filter(|histogram| !histogram.is_empty())
            .map(ToOwned::to_owned);
        let histogram = requested_histogram
            .clone()
            .or_else(|| config.default_histogram.clone());
        let requested_query = get_str(object, "query")
            .filter(|query| !query.is_empty())
            .map(ToOwned::to_owned);
        let fts_patterns = requested_query
            .as_deref()
            .map(|query| vec![query.as_bytes().to_vec()])
            .unwrap_or_default();
        let source_selection = parse_source_selection(object.get("selections"));
        let filters = parse_filters(object.get("selections"));

        let echo = normalized_request_echo(
            info,
            after_realtime_usec,
            before_realtime_usec,
            if_modified_since_usec,
            anchor,
            direction,
            limit,
            data_only,
            delta,
            tail,
            sampling,
            source_selection.source_type,
            requested_facets.as_deref(),
            object.get("selections"),
            requested_histogram.as_deref(),
            requested_query.as_deref(),
        );

        Ok(Self {
            info,
            echo,
            after_realtime_usec,
            before_realtime_usec,
            if_modified_since_usec,
            anchor,
            direction,
            limit,
            data_only,
            delta,
            tail,
            sampling,
            source_type: source_selection.source_type,
            exact_sources: source_selection.exact_sources,
            filters,
            facets,
            histogram,
            fts_patterns,
        })
    }

    fn facet_field_names(&self) -> Vec<String> {
        self.facets
            .iter()
            .filter_map(|field| String::from_utf8(field.clone()).ok())
            .collect()
    }

    fn matches_source(&self, path: &Path) -> bool {
        if self.source_type == SOURCE_TYPE_ALL && self.exact_sources.is_empty() {
            return true;
        }
        let file_source_type = journal_file_source_type(path);
        if file_source_type & self.source_type != 0 {
            return true;
        }
        if self.exact_sources.is_empty() {
            return false;
        }
        let source_name = journal_file_exact_source_name(path);
        self.exact_sources
            .iter()
            .any(|source| source_name.as_deref() == Some(source.as_str()))
    }

    fn to_explorer_query(&self) -> ExplorerQuery {
        let analysis_enabled = !self.data_only || self.delta;
        let tail_anchor = self.tail && matches!(self.anchor, ExplorerAnchor::Realtime(_));
        ExplorerQuery {
            after_realtime_usec: self.after_realtime_usec,
            before_realtime_usec: self.before_realtime_usec,
            anchor: self.anchor,
            direction: self.direction,
            limit: self.limit,
            filters: self.filters.clone(),
            facets: analysis_enabled
                .then(|| self.facets.clone())
                .unwrap_or_default(),
            histogram: analysis_enabled
                .then(|| {
                    self.histogram
                        .as_ref()
                        .map(|field| field.as_bytes().to_vec())
                })
                .flatten(),
            histogram_target_buckets: DEFAULT_HISTOGRAM_BUCKETS,
            fts_patterns: self.fts_patterns.clone(),
            field_mode: ExplorerFieldMode::FirstValue,
            exclude_facet_field_filters: false,
            use_source_realtime: true,
            realtime_slack_usec: NETDATA_JOURNAL_VS_REALTIME_DELTA_DEFAULT_USEC,
            stop_when_rows_full: self.data_only && !tail_anchor,
            stop_when_rows_full_check_every: DATA_ONLY_CHECK_EVERY_ROWS,
            debug_collect_column_fields_by_row_traversal: false,
        }
    }
}

#[derive(Debug, Clone)]
struct LocatedRow {
    file_path: PathBuf,
    row: ExplorerRow,
}

#[derive(Debug, Default)]
struct JournalSourceSummary {
    files: u64,
    total_size: u64,
    first_realtime_usec: Option<u64>,
    last_realtime_usec: Option<u64>,
}

impl JournalSourceSummary {
    fn from_paths(paths: &[PathBuf], reader_options: ReaderOptions) -> Self {
        let mut summary = Self::default();
        for path in paths {
            if let Ok(metadata) = std::fs::metadata(path) {
                summary.files = summary.files.saturating_add(1);
                summary.total_size = summary.total_size.saturating_add(metadata.len());
            }
            let Ok(reader) = FileReader::open_with_options(path, reader_options) else {
                continue;
            };
            let header = reader.header();
            if header.head_entry_realtime != 0 {
                summary.first_realtime_usec = Some(
                    summary
                        .first_realtime_usec
                        .map_or(header.head_entry_realtime, |current| {
                            current.min(header.head_entry_realtime)
                        }),
                );
            }
            if header.tail_entry_realtime != 0 {
                summary.last_realtime_usec = Some(
                    summary
                        .last_realtime_usec
                        .map_or(header.tail_entry_realtime, |current| {
                            current.max(header.tail_entry_realtime)
                        }),
                );
            }
        }
        summary
    }

    fn info(&self) -> String {
        let coverage = match (self.first_realtime_usec, self.last_realtime_usec) {
            (Some(first), Some(last)) if last >= first => {
                human_duration_seconds((last - first) / 1_000_000)
            }
            _ => "0s".to_string(),
        };
        let last_entry = self
            .last_realtime_usec
            .and_then(|usec| DateTime::<Utc>::from_timestamp((usec / 1_000_000) as i64, 0))
            .map(|datetime| datetime.format("%Y-%m-%dT%H:%M:%SZ").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        format!(
            "{} files, total size {}, covering {}, last entry at {}",
            self.files,
            human_binary_size(self.total_size),
            coverage,
            last_entry
        )
    }
}

fn expand_located_row_payloads(
    located: &mut LocatedRow,
    reader_options: ReaderOptions,
) -> Result<()> {
    let mut reader = FileReader::open_with_options(&located.file_path, reader_options)?;
    reader.seek_cursor(&located.row.cursor)?;
    if !reader.test_cursor(&located.row.cursor)? {
        return Err(SdkError::InvalidCursor(format!(
            "selected row cursor is no longer available: {}",
            located.row.cursor
        )));
    }
    reader.collect_entry_payloads(&mut located.row.payloads)
}

#[derive(Debug, Default)]
struct CombinedResult {
    rows: Vec<LocatedRow>,
    facets: BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, u64>>,
    histogram: Option<ExplorerHistogram>,
    column_fields: BTreeSet<String>,
    stats: ExplorerStats,
    matched_files: u64,
    matched_paths: Vec<PathBuf>,
    skipped_files: u64,
    file_errors: Vec<String>,
    partial: bool,
    timed_out: bool,
    cancelled: bool,
}

impl CombinedResult {
    fn merge(&mut self, path: &Path, result: ExplorerResult, direction: Direction, limit: usize) {
        self.merge_stats(result.stats);
        for row in result.rows {
            self.rows.push(LocatedRow {
                file_path: path.to_path_buf(),
                row,
            });
        }
        for field in result.column_fields {
            if let Ok(field) = String::from_utf8(field) {
                self.column_fields.insert(field);
            }
        }
        for (field, values) in result.facets {
            let target = self.facets.entry(field).or_default();
            for (value, count) in values {
                add_netdata_facet_count(target, &value, count);
            }
        }
        if let Some(histogram) = result.histogram {
            merge_histogram(&mut self.histogram, histogram);
        }
        self.sort_and_limit(direction, limit);
    }

    fn add_column_fields<I>(&mut self, fields: I)
    where
        I: IntoIterator<Item = String>,
    {
        self.column_fields.extend(fields);
    }

    fn sort_and_limit(&mut self, direction: Direction, limit: usize) {
        match direction {
            Direction::Forward => self.rows.sort_by_key(|row| row.row.realtime_usec),
            Direction::Backward => self
                .rows
                .sort_by(|left, right| right.row.realtime_usec.cmp(&left.row.realtime_usec)),
        }
        make_row_timestamps_unique(&mut self.rows, direction);
        if self.rows.len() > limit {
            self.rows.truncate(limit);
        }
        self.stats.rows_returned = self.rows.len() as u64;
    }

    fn expand_row_payloads(&mut self, reader_options: ReaderOptions) {
        if self.rows.is_empty() {
            self.stats.rows_returned = 0;
            return;
        }

        let mut rows = Vec::with_capacity(self.rows.len());
        for mut located in self.rows.drain(..) {
            if !located.row.payloads.is_empty() {
                rows.push(located);
                continue;
            }
            match expand_located_row_payloads(&mut located, reader_options) {
                Ok(()) => {
                    self.stats.returned_row_expansions =
                        self.stats.returned_row_expansions.saturating_add(1);
                    rows.push(located);
                }
                Err(err) => {
                    self.partial = true;
                    self.file_errors
                        .push(format!("{}: {err}", located.file_path.display()));
                }
            }
        }
        self.rows = rows;
        self.stats.rows_returned = self.rows.len() as u64;
    }

    fn merge_stats(&mut self, stats: ExplorerStats) {
        self.stats.rows_examined = self.stats.rows_examined.saturating_add(stats.rows_examined);
        self.stats.rows_matched = self.stats.rows_matched.saturating_add(stats.rows_matched);
        self.stats.facet_rows_matched = self
            .stats
            .facet_rows_matched
            .saturating_add(stats.facet_rows_matched);
        self.stats.rows_returned = self.stats.rows_returned.saturating_add(stats.rows_returned);
        if stats.last_realtime_usec > self.stats.last_realtime_usec {
            self.stats.last_realtime_usec = stats.last_realtime_usec;
        }
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
                add_netdata_facet_count(target, value, 0);
            }
        }
    }

    fn add_zero_count_facet_values_from_files(
        &mut self,
        fields: &[Vec<u8>],
        reader_options: ReaderOptions,
    ) {
        for path in &self.matched_paths {
            let Ok(mut reader) = FileReader::open_with_options(path, reader_options) else {
                continue;
            };
            for field in fields {
                let Ok(field_name) = std::str::from_utf8(field) else {
                    continue;
                };
                let Ok(values) = reader.query_unique(field_name) else {
                    continue;
                };
                let target = self.facets.entry(field.clone()).or_default();
                for value in values {
                    add_netdata_facet_count(target, &value, 0);
                }
            }
        }
    }

    fn reportable_facet_fields(&self, requested: &[Vec<u8>]) -> Vec<String> {
        requested
            .iter()
            .filter(|field| {
                self.facets
                    .get(*field)
                    .is_some_and(facet_group_is_reportable)
            })
            .filter_map(|field| String::from_utf8(field.clone()).ok())
            .collect()
    }
}

fn netdata_function_error(status: u64, message: &str) -> Value {
    json!({
        "status": status,
        "errorMessage": message,
    })
}

fn timeout_message() -> Value {
    json!({
        "title": "Query timed-out, incomplete data. ",
        "status": "warning",
        "description": "QUERY TIMEOUT: The query timed out and may not include all the data of the selected window. ",
    })
}

fn merged_progress_stats(completed: &ExplorerStats, current: &ExplorerStats) -> ExplorerStats {
    let mut stats = completed.clone();
    stats.rows_examined = stats.rows_examined.saturating_add(current.rows_examined);
    stats.rows_matched = stats.rows_matched.saturating_add(current.rows_matched);
    stats.facet_rows_matched = stats
        .facet_rows_matched
        .saturating_add(current.facet_rows_matched);
    stats.rows_returned = stats.rows_returned.saturating_add(current.rows_returned);
    if current.last_realtime_usec > stats.last_realtime_usec {
        stats.last_realtime_usec = current.last_realtime_usec;
    }
    stats.data_refs_seen = stats.data_refs_seen.saturating_add(current.data_refs_seen);
    stats.data_refs_skipped = stats
        .data_refs_skipped
        .saturating_add(current.data_refs_skipped);
    stats.data_payloads_loaded = stats
        .data_payloads_loaded
        .saturating_add(current.data_payloads_loaded);
    stats.data_objects_classified = stats
        .data_objects_classified
        .saturating_add(current.data_objects_classified);
    stats.data_cache_hits = stats
        .data_cache_hits
        .saturating_add(current.data_cache_hits);
    stats.data_cache_misses = stats
        .data_cache_misses
        .saturating_add(current.data_cache_misses);
    stats.payloads_decompressed = stats
        .payloads_decompressed
        .saturating_add(current.payloads_decompressed);
    stats.fts_scans = stats.fts_scans.saturating_add(current.fts_scans);
    stats.facet_updates = stats.facet_updates.saturating_add(current.facet_updates);
    stats.histogram_updates = stats
        .histogram_updates
        .saturating_add(current.histogram_updates);
    stats.returned_row_expansions = stats
        .returned_row_expansions
        .saturating_add(current.returned_row_expansions);
    stats.early_stop_opportunities = stats
        .early_stop_opportunities
        .saturating_add(current.early_stop_opportunities);
    stats.early_stops = stats.early_stops.saturating_add(current.early_stops);
    stats
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

fn facet_group_is_reportable(values: &BTreeMap<Vec<u8>, u64>) -> bool {
    values
        .iter()
        .any(|(value, count)| *count != 0 && !value.is_empty() && value.as_slice() != b"-")
}

fn netdata_facet_value(value: &[u8]) -> &[u8] {
    if value.len() > NETDATA_FACET_MAX_VALUE_LENGTH {
        &value[..NETDATA_FACET_MAX_VALUE_LENGTH]
    } else {
        value
    }
}

fn add_netdata_facet_count(target: &mut BTreeMap<Vec<u8>, u64>, value: &[u8], count: u64) {
    *target
        .entry(netdata_facet_value(value).to_vec())
        .or_default() += count;
}

fn file_may_overlap_request(header: crate::FileHeader, request: &NetdataRequest) -> bool {
    if header.tail_entry_realtime == 0 {
        return true;
    }

    let first = header
        .head_entry_realtime
        .saturating_sub(NETDATA_JOURNAL_VS_REALTIME_DELTA_MAX_USEC);
    let last = header
        .tail_entry_realtime
        .saturating_add(NETDATA_JOURNAL_VS_REALTIME_DELTA_MAX_USEC);

    if request
        .after_realtime_usec
        .is_some_and(|after| last < after)
    {
        return false;
    }
    if request
        .before_realtime_usec
        .is_some_and(|before| first > before)
    {
        return false;
    }

    true
}

fn collect_boot_first_realtime(
    paths: &[PathBuf],
    reader_options: ReaderOptions,
) -> BTreeMap<Vec<u8>, u64> {
    let mut out = BTreeMap::new();
    for path in paths {
        let Ok(mut reader) = FileReader::open_with_options(path, reader_options) else {
            continue;
        };
        let Ok(boot_ids) = reader.query_unique("_BOOT_ID") else {
            continue;
        };
        for boot_id in boot_ids {
            let mut match_payload = b"_BOOT_ID=".to_vec();
            match_payload.extend_from_slice(&boot_id);
            reader.flush_matches();
            reader.add_match(&match_payload);
            reader.seek_head();
            if !reader.next().unwrap_or(false) {
                continue;
            }
            if let Ok(realtime) = reader.get_realtime_usec() {
                record_boot_first_realtime(&mut out, boot_id, realtime);
            }
        }
        reader.flush_matches();
    }
    out
}

fn record_boot_first_realtime(
    target: &mut BTreeMap<Vec<u8>, u64>,
    boot_id: Vec<u8>,
    realtime_usec: u64,
) {
    let existing = target.entry(boot_id).or_insert(realtime_usec);
    if realtime_usec < *existing {
        *existing = realtime_usec;
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
    let base = first_value(fields, "CONTAINER_NAME")
        .or_else(|| first_value(fields, "SYSLOG_IDENTIFIER"))
        .or_else(|| first_value(fields, "_COMM"))
        .map(|value| String::from_utf8_lossy(value).into_owned())
        .unwrap_or_default();
    if base.is_empty() {
        return "-".to_string();
    }
    let pid = first_value(fields, "_PID").map(|value| String::from_utf8_lossy(value).into_owned());
    match pid {
        Some(pid) if !pid.is_empty() => format!("{base}[{pid}]"),
        _ => base,
    }
}

fn make_row_timestamps_unique(rows: &mut [LocatedRow], direction: Direction) {
    let mut last_from = 0u64;
    let mut last_to = 0u64;
    let mut initialized = false;
    for row in rows {
        let timestamp = row.row.realtime_usec;
        if initialized && timestamp >= last_from && timestamp <= last_to {
            match direction {
                Direction::Backward => {
                    last_from = last_from.saturating_sub(1);
                    row.row.realtime_usec = last_from;
                }
                Direction::Forward => {
                    last_to = last_to.saturating_add(1);
                    row.row.realtime_usec = last_to;
                }
            }
        } else {
            last_from = timestamp;
            last_to = timestamp;
            initialized = true;
        }
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
        "_HOSTNAME" => (true, "facet", false),
        "ND_JOURNAL_PROCESS" | "MESSAGE" => (true, "none", key == "MESSAGE"),
        "ND_JOURNAL_FILE" | "_SOURCE_REALTIME_TIMESTAMP" => (false, "none", false),
        _ if systemd_column_is_facet(key) => (false, "facet", false),
        _ => (false, "none", false),
    };
    let column_type = if key == "timestamp" {
        "timestamp"
    } else if key == "rowOptions" {
        "none"
    } else {
        "string"
    };
    let visualization = if key == "rowOptions" {
        "rowOptions"
    } else {
        "value"
    };
    let mut metadata = json!({
        "index": index,
        "unique_key": key == "timestamp",
        "name": if key == "timestamp" { "Timestamp" } else { key },
        "visible": visible,
        "type": column_type,
        "visualization": visualization,
        "value_options": {
            "transform": if key == "timestamp" { "datetime_usec" } else { "none" },
            "decimal_points": 0,
            "default_value": if key == "timestamp" || key == "rowOptions" {
                Value::Null
            } else {
                Value::String("-".to_string())
            },
        },
        "sort": "ascending",
        "sortable": false,
        "sticky": false,
        "summary": "count",
        "filter": filter,
        "full_width": full_width,
        "wrap": key != "rowOptions",
        "default_expanded_filter": matches!(key, "PRIORITY" | "SYSLOG_FACILITY" | "MESSAGE_ID"),
    });
    if key == "rowOptions" {
        if let Some(object) = metadata.as_object_mut() {
            object.insert("dummy".to_string(), Value::Bool(true));
        }
    }
    metadata
}

fn systemd_column_is_facet(key: &str) -> bool {
    if key == "MESSAGE_ID" {
        return true;
    }
    if key.contains("MESSAGE") || key.contains("TIMESTAMP") || key.starts_with("__") {
        return false;
    }
    true
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
        if matches!(field.as_str(), "query" | "source" | "__logs_sources") {
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

#[derive(Debug, Clone)]
struct SourceSelection {
    source_type: u64,
    exact_sources: Vec<String>,
}

fn parse_source_selection(value: Option<&Value>) -> SourceSelection {
    let mut selection = SourceSelection {
        source_type: SOURCE_TYPE_ALL,
        exact_sources: Vec::new(),
    };
    let Some(Value::Object(selections)) = value else {
        return selection;
    };
    let Some(values) = parse_string_array(selections.get("__logs_sources")) else {
        return selection;
    };
    selection.source_type = 0;
    for value in values {
        match source_type_for_name(&value) {
            Some(source_type) => selection.source_type |= source_type,
            None => selection.exact_sources.push(value),
        }
    }
    selection
}

fn source_type_for_name(value: &str) -> Option<u64> {
    match value {
        "all" => Some(SOURCE_TYPE_ALL),
        "all-local-logs" => Some(SOURCE_TYPE_LOCAL_ALL),
        "all-remote-systems" => Some(SOURCE_TYPE_REMOTE_ALL),
        "all-local-system-logs" => Some(SOURCE_TYPE_LOCAL_SYSTEM),
        "all-local-user-logs" => Some(SOURCE_TYPE_LOCAL_USER),
        "all-local-namespaces" => Some(SOURCE_TYPE_LOCAL_NAMESPACE),
        "all-uncategorized" => Some(SOURCE_TYPE_LOCAL_OTHER),
        _ => None,
    }
}

fn journal_file_source_type(path: &Path) -> u64 {
    let text = path.to_string_lossy();
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return SOURCE_TYPE_ALL | SOURCE_TYPE_LOCAL_ALL | SOURCE_TYPE_LOCAL_OTHER;
    };
    if text.contains("/remote/") {
        return SOURCE_TYPE_ALL | SOURCE_TYPE_REMOTE_ALL;
    }
    if local_namespace_source_name(path).is_some() {
        return SOURCE_TYPE_ALL | SOURCE_TYPE_LOCAL_ALL | SOURCE_TYPE_LOCAL_NAMESPACE;
    }
    if name.starts_with("system") {
        return SOURCE_TYPE_ALL | SOURCE_TYPE_LOCAL_ALL | SOURCE_TYPE_LOCAL_SYSTEM;
    }
    if name.starts_with("user") {
        return SOURCE_TYPE_ALL | SOURCE_TYPE_LOCAL_ALL | SOURCE_TYPE_LOCAL_USER;
    }
    SOURCE_TYPE_ALL | SOURCE_TYPE_LOCAL_ALL | SOURCE_TYPE_LOCAL_OTHER
}

fn local_namespace_source_name(path: &Path) -> Option<String> {
    let parent = path.parent()?.file_name()?.to_str()?;
    let (_, namespace) = parent.rsplit_once('.')?;
    (!namespace.is_empty()).then(|| format!("namespace-{namespace}"))
}

fn journal_file_exact_source_name(path: &Path) -> Option<String> {
    let text = path.to_string_lossy();
    if text.contains("/remote/") {
        let name = path.file_name()?.to_str()?;
        let source = name
            .split_once('@')
            .map(|(prefix, _)| prefix)
            .unwrap_or_else(|| {
                name.strip_suffix(".journal~.zst")
                    .or_else(|| name.strip_suffix(".journal.zst"))
                    .or_else(|| name.strip_suffix(".journal~"))
                    .or_else(|| name.strip_suffix(".journal"))
                    .unwrap_or(name)
            });
        return source.starts_with("remote-").then(|| source.to_string());
    }
    local_namespace_source_name(path)
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
    let mut after = after.unwrap_or(0);
    let mut before = before.unwrap_or(0);

    if after == 0 && before == 0 {
        before = now_seconds;
        after = before.saturating_sub(DEFAULT_TIME_WINDOW_SECONDS);
    } else {
        (after, before) = relative_window_to_absolute(now_seconds, after, before);
    }

    if after > before {
        std::mem::swap(&mut after, &mut before);
    }
    if after == before {
        after = before.saturating_sub(DEFAULT_TIME_WINDOW_SECONDS);
    }

    (
        Some(normalize_timestamp_to_usec_with_rounding(
            after.max(0) as u64,
            false,
        )),
        Some(normalize_timestamp_to_usec_with_rounding(
            before.max(0) as u64,
            true,
        )),
    )
}

fn relative_window_to_absolute(now_seconds: i64, after: i64, before: i64) -> (i64, i64) {
    let mut after = after;
    let mut before = before;

    if before.unsigned_abs() <= API_RELATIVE_TIME_MAX_SECONDS as u64 {
        if before > 0 {
            before = -before;
        }
        before = now_seconds.saturating_add(before);
    }

    if after.unsigned_abs() <= API_RELATIVE_TIME_MAX_SECONDS as u64 {
        if after > 0 {
            after = -after;
        }
        if after == 0 {
            after = -NETDATA_MISSING_AFTER_RELATIVE_SECONDS;
        }
        after = before.saturating_add(after).saturating_add(1);
    }

    if after > before {
        std::mem::swap(&mut after, &mut before);
    }

    if before > now_seconds {
        let delta = before.saturating_sub(now_seconds);
        before = before.saturating_sub(delta);
        after = after.saturating_sub(delta);
    }

    (after, before)
}

fn normalized_request_echo(
    info: bool,
    after_realtime_usec: Option<u64>,
    before_realtime_usec: Option<u64>,
    if_modified_since_usec: u64,
    anchor: ExplorerAnchor,
    direction: Direction,
    limit: usize,
    data_only: bool,
    delta: bool,
    tail: bool,
    sampling: u64,
    source_type: u64,
    requested_facets: Option<&[String]>,
    selections: Option<&Value>,
    histogram: Option<&str>,
    query: Option<&str>,
) -> Value {
    let anchor_usec = match anchor {
        ExplorerAnchor::Realtime(usec) => usec,
        ExplorerAnchor::Auto | ExplorerAnchor::Head | ExplorerAnchor::Tail => 0,
    };
    let mut out = json!({
        "info": info,
        "slice": true,
        "data_only": data_only,
        "delta": delta,
        "tail": tail,
        "sampling": sampling,
        "source_type": source_type,
        "after": after_realtime_usec.unwrap_or(0) / 1_000_000,
        "before": before_realtime_usec.unwrap_or(0) / 1_000_000,
        "if_modified_since": if_modified_since_usec,
        "anchor": anchor_usec,
        "direction": match direction {
            Direction::Forward => "forward",
            Direction::Backward => "backward",
        },
        "last": limit,
        "query": query,
        "histogram": histogram,
    });
    if let Some(facets) = requested_facets {
        out.as_object_mut()
            .expect("Netdata request echo root must be an object")
            .insert(
                "facets".to_string(),
                facets
                    .iter()
                    .map(|field| Value::String(field.clone()))
                    .collect(),
            );
    }
    if let Some(Value::Object(selections)) = selections {
        let mut selections = selections.clone();
        if let Some(Value::Array(sources)) = selections.get_mut("__logs_sources") {
            for source in sources {
                *source = Value::Null;
            }
        }
        out.as_object_mut()
            .expect("Netdata request echo root must be an object")
            .insert("selections".to_string(), Value::Object(selections));
    }
    out
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

fn human_binary_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{}{}", bytes, UNITS[unit])
    } else if value.fract() == 0.0 {
        format!("{value:.0}{}", UNITS[unit])
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

fn human_duration_seconds(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    let mut parts = Vec::new();
    if hours != 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes != 0 {
        parts.push(format!("{minutes}m"));
    }
    if seconds != 0 || parts.is_empty() {
        parts.push(format!("{seconds}s"));
    }
    parts.join(" ")
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct JournalFileOrderInfo {
    msg_first_realtime_usec: u64,
    msg_last_realtime_usec: u64,
    file_last_modified_usec: u64,
}

fn sort_journal_files_for_request(
    paths: &mut [PathBuf],
    direction: Direction,
    reader_options: ReaderOptions,
) {
    let mut ordered: Vec<_> = paths
        .iter()
        .cloned()
        .map(|path| {
            let info = journal_file_order_info(&path, reader_options);
            (path, info)
        })
        .collect();
    ordered.sort_by(|(left_path, left_info), (right_path, right_info)| {
        compare_journal_file_order(left_info, right_info, direction)
            .then_with(|| left_path.cmp(right_path))
    });
    for (target, (path, _)) in paths.iter_mut().zip(ordered) {
        *target = path;
    }
}

fn journal_file_order_info(path: &Path, reader_options: ReaderOptions) -> JournalFileOrderInfo {
    let file_last_modified_usec = std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_micros().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default();

    let Ok(reader) = FileReader::open_with_options(path, reader_options) else {
        return JournalFileOrderInfo {
            msg_first_realtime_usec: 0,
            msg_last_realtime_usec: file_last_modified_usec,
            file_last_modified_usec,
        };
    };
    let header = reader.header();
    JournalFileOrderInfo {
        msg_first_realtime_usec: header.head_entry_realtime,
        msg_last_realtime_usec: if header.tail_entry_realtime == 0 {
            file_last_modified_usec
        } else {
            header.tail_entry_realtime
        },
        file_last_modified_usec,
    }
}

fn compare_journal_file_order(
    left: &JournalFileOrderInfo,
    right: &JournalFileOrderInfo,
    direction: Direction,
) -> Ordering {
    let backward = right
        .msg_last_realtime_usec
        .cmp(&left.msg_last_realtime_usec)
        .then_with(|| {
            right
                .file_last_modified_usec
                .cmp(&left.file_last_modified_usec)
        })
        .then_with(|| {
            right
                .msg_first_realtime_usec
                .cmp(&left.msg_first_realtime_usec)
        });
    match direction {
        Direction::Backward => backward,
        Direction::Forward => backward.reverse(),
    }
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

fn netdata_reorder_key(value: &str) -> String {
    value
        .trim_start_matches(|character: char| character.is_ascii_punctuation())
        .to_ascii_lowercase()
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

enum TimestampPrecision {
    Seconds,
    Micros,
}

fn format_realtime_usec(timestamp: u64, precision: TimestampPrecision) -> String {
    let seconds = (timestamp / 1_000_000) as i64;
    let micros = (timestamp % 1_000_000) as u32;
    DateTime::<Utc>::from_timestamp(seconds, micros.saturating_mul(1000))
        .map(|datetime| match precision {
            TimestampPrecision::Seconds => datetime.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            TimestampPrecision::Micros => datetime.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
        })
        .unwrap_or_else(|| timestamp.to_string())
}

fn priority_name(raw: &str) -> Option<&'static str> {
    match parse_priority(raw)? {
        0 => Some("panic"),
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
        "panic" | "emergency" | "emerg" => Some("0"),
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

fn errno_name(raw: &str) -> Option<String> {
    let errno = raw.parse::<u32>().ok()?;
    let name = match errno {
        1 => "EPERM",
        2 => "ENOENT",
        3 => "ESRCH",
        4 => "EINTR",
        5 => "EIO",
        6 => "ENXIO",
        7 => "E2BIG",
        8 => "ENOEXEC",
        9 => "EBADF",
        10 => "ECHILD",
        11 => "EAGAIN",
        12 => "ENOMEM",
        13 => "EACCES",
        14 => "EFAULT",
        15 => "ENOTBLK",
        16 => "EBUSY",
        17 => "EEXIST",
        18 => "EXDEV",
        19 => "ENODEV",
        20 => "ENOTDIR",
        21 => "EISDIR",
        22 => "EINVAL",
        23 => "ENFILE",
        24 => "EMFILE",
        25 => "ENOTTY",
        26 => "ETXTBSY",
        27 => "EFBIG",
        28 => "ENOSPC",
        29 => "ESPIPE",
        30 => "EROFS",
        31 => "EMLINK",
        32 => "EPIPE",
        33 => "EDOM",
        34 => "ERANGE",
        35 => "EDEADLK",
        36 => "ENAMETOOLONG",
        37 => "ENOLCK",
        38 => "ENOSYS",
        39 => "ENOTEMPTY",
        40 => "ELOOP",
        42 => "ENOMSG",
        43 => "EIDRM",
        44 => "ECHRNG",
        45 => "EL2NSYNC",
        46 => "EL3HLT",
        47 => "EL3RST",
        48 => "ELNRNG",
        49 => "EUNATCH",
        50 => "ENOCSI",
        51 => "EL2HLT",
        52 => "EBADE",
        53 => "EBADR",
        54 => "EXFULL",
        55 => "ENOANO",
        56 => "EBADRQC",
        57 => "EBADSLT",
        59 => "EBFONT",
        60 => "ENOSTR",
        61 => "ENODATA",
        62 => "ETIME",
        63 => "ENOSR",
        64 => "ENONET",
        65 => "ENOPKG",
        66 => "EREMOTE",
        67 => "ENOLINK",
        68 => "EADV",
        69 => "ESRMNT",
        70 => "ECOMM",
        71 => "EPROTO",
        72 => "EMULTIHOP",
        73 => "EDOTDOT",
        74 => "EBADMSG",
        75 => "EOVERFLOW",
        76 => "ENOTUNIQ",
        77 => "EBADFD",
        78 => "EREMCHG",
        79 => "ELIBACC",
        80 => "ELIBBAD",
        81 => "ELIBSCN",
        82 => "ELIBMAX",
        83 => "ELIBEXEC",
        84 => "EILSEQ",
        85 => "ERESTART",
        86 => "ESTRPIPE",
        87 => "EUSERS",
        88 => "ENOTSOCK",
        89 => "EDESTADDRREQ",
        90 => "EMSGSIZE",
        91 => "EPROTOTYPE",
        92 => "ENOPROTOOPT",
        93 => "EPROTONOSUPPORT",
        94 => "ESOCKTNOSUPPORT",
        95 => "ENOTSUP",
        96 => "EPFNOSUPPORT",
        97 => "EAFNOSUPPORT",
        98 => "EADDRINUSE",
        99 => "EADDRNOTAVAIL",
        100 => "ENETDOWN",
        101 => "ENETUNREACH",
        102 => "ENETRESET",
        103 => "ECONNABORTED",
        104 => "ECONNRESET",
        105 => "ENOBUFS",
        106 => "EISCONN",
        107 => "ENOTCONN",
        108 => "ESHUTDOWN",
        109 => "ETOOMANYREFS",
        110 => "ETIMEDOUT",
        111 => "ECONNREFUSED",
        112 => "EHOSTDOWN",
        113 => "EHOSTUNREACH",
        114 => "EALREADY",
        115 => "EINPROGRESS",
        116 => "ESTALE",
        117 => "EUCLEAN",
        118 => "ENOTNAM",
        119 => "ENAVAIL",
        120 => "EISNAM",
        121 => "EREMOTEIO",
        122 => "EDQUOT",
        123 => "ENOMEDIUM",
        124 => "EMEDIUMTYPE",
        125 => "ECANCELED",
        126 => "ENOKEY",
        127 => "EKEYEXPIRED",
        128 => "EKEYREVOKED",
        129 => "EKEYREJECTED",
        130 => "EOWNERDEAD",
        131 => "ENOTRECOVERABLE",
        132 => "ERFKILL",
        133 => "EHWPOISON",
        _ => return None,
    };
    Some(format!("{errno} ({name})"))
}

fn cap_effective_display(raw: &str) -> String {
    if !raw.bytes().next().is_some_and(|byte| byte.is_ascii_digit()) {
        return raw.to_string();
    }
    let Ok(value) = u64::from_str_radix(raw, 16) else {
        return raw.to_string();
    };
    if value == 0 {
        return raw.to_string();
    }
    const CAPABILITIES: &[&str] = &[
        "CHOWN",
        "DAC_OVERRIDE",
        "DAC_READ_SEARCH",
        "FOWNER",
        "FSETID",
        "KILL",
        "SETGID",
        "SETUID",
        "SETPCAP",
        "LINUX_IMMUTABLE",
        "NET_BIND_SERVICE",
        "NET_BROADCAST",
        "NET_ADMIN",
        "NET_RAW",
        "IPC_LOCK",
        "IPC_OWNER",
        "SYS_MODULE",
        "SYS_RAWIO",
        "SYS_CHROOT",
        "SYS_PTRACE",
        "SYS_PACCT",
        "SYS_ADMIN",
        "SYS_BOOT",
        "SYS_NICE",
        "SYS_RESOURCE",
        "SYS_TIME",
        "SYS_TTY_CONFIG",
        "MKNOD",
        "LEASE",
        "AUDIT_WRITE",
        "AUDIT_CONTROL",
        "SETFCAP",
        "MAC_OVERRIDE",
        "MAC_ADMIN",
        "SYSLOG",
        "WAKE_ALARM",
        "BLOCK_SUSPEND",
        "AUDIT_READ",
        "PERFMON",
        "BPF",
        "CHECKPOINT_RESTORE",
    ];
    let names: Vec<&str> = CAPABILITIES
        .iter()
        .enumerate()
        .filter_map(|(index, name)| ((value & (1u64 << index)) != 0).then_some(*name))
        .collect();
    if names.is_empty() {
        raw.to_string()
    } else {
        format!("{raw} ({})", names.join(" | "))
    }
}

fn systemd_field_display_value(
    context: &DisplayContext,
    scope: DisplayScope,
    field: &str,
    value: &[u8],
    resolve_user_group_names: bool,
) -> Value {
    let raw = String::from_utf8_lossy(value);
    match field {
        "PRIORITY" => Value::String(priority_name(&raw).unwrap_or(&raw).to_string()),
        "SYSLOG_FACILITY" => Value::String(syslog_facility_name(&raw).unwrap_or(&raw).to_string()),
        "ERRNO" => Value::String(errno_name(&raw).unwrap_or_else(|| raw.to_string())),
        "MESSAGE_ID" => Value::String(match (message_id_name(&raw), scope) {
            (Some(name), DisplayScope::Data) => format!("{raw} ({name})"),
            (Some(name), DisplayScope::Facet | DisplayScope::Histogram) => name.to_string(),
            (None, _) => raw.into_owned(),
        }),
        "_BOOT_ID" => Value::String(match (context.boot_first_realtime.get(value), scope) {
            (Some(timestamp), DisplayScope::Data) => format!(
                "{} ({})  ",
                raw,
                format_realtime_usec(*timestamp, TimestampPrecision::Seconds)
            ),
            (Some(timestamp), DisplayScope::Facet | DisplayScope::Histogram) => {
                format_realtime_usec(*timestamp, TimestampPrecision::Seconds)
            }
            (None, _) => raw.into_owned(),
        }),
        "_UID"
        | "_SYSTEMD_OWNER_UID"
        | "OBJECT_SYSTEMD_OWNER_UID"
        | "OBJECT_UID"
        | "_AUDIT_LOGINUID"
        | "OBJECT_AUDIT_LOGINUID" => {
            if resolve_user_group_names {
                Value::String(resolve_uid_name(&raw).unwrap_or_else(|| raw.into_owned()))
            } else {
                Value::String(raw.into_owned())
            }
        }
        "_GID" | "OBJECT_GID" => {
            if resolve_user_group_names {
                Value::String(resolve_gid_name(&raw).unwrap_or_else(|| raw.into_owned()))
            } else {
                Value::String(raw.into_owned())
            }
        }
        "_CAP_EFFECTIVE" => Value::String(cap_effective_display(&raw)),
        "_SOURCE_REALTIME_TIMESTAMP" => Value::String(match raw.parse::<u64>() {
            Ok(timestamp) if timestamp != 0 => {
                format!(
                    "{} ({})",
                    raw,
                    format_realtime_usec(timestamp, TimestampPrecision::Micros)
                )
            }
            _ => raw.into_owned(),
        }),
        _ => Value::String(raw.into_owned()),
    }
}

#[cfg(unix)]
fn resolve_uid_name(raw: &str) -> Option<String> {
    let uid = raw.parse::<libc::uid_t>().ok()?;
    let mut pwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0i8; 16_384];
    let rc = unsafe {
        libc::getpwuid_r(
            uid,
            pwd.as_mut_ptr(),
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut result,
        )
    };
    if rc != 0 || result.is_null() {
        return None;
    }
    let pwd = unsafe { pwd.assume_init() };
    Some(
        unsafe { CStr::from_ptr(pwd.pw_name) }
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(not(unix))]
fn resolve_uid_name(_raw: &str) -> Option<String> {
    None
}

#[cfg(unix)]
fn resolve_gid_name(raw: &str) -> Option<String> {
    let gid = raw.parse::<libc::gid_t>().ok()?;
    let mut grp = std::mem::MaybeUninit::<libc::group>::uninit();
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0i8; 16_384];
    let rc = unsafe {
        libc::getgrgid_r(
            gid,
            grp.as_mut_ptr(),
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut result,
        )
    };
    if rc != 0 || result.is_null() {
        return None;
    }
    let grp = unsafe { grp.assume_init() };
    Some(
        unsafe { CStr::from_ptr(grp.gr_name) }
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(not(unix))]
fn resolve_gid_name(_raw: &str) -> Option<String> {
    None
}

fn message_id_name(raw: &str) -> Option<&'static str> {
    match raw {
        "f77379a8490b408bbe5f6940505a777b" => Some("Journal started"),
        "d93fb3c9c24d451a97cea615ce59c00b" => Some("Journal stopped"),
        "a596d6fe7bfa4994828e72309e95d61e" => Some("Journal messages suppressed"),
        "e9bf28e6e834481bb6f48f548ad13606" => Some("Journal messages missed"),
        "ec387f577b844b8fa948f33cad9a75e6" => Some("Journal disk space usage"),
        "fc2e22bc6ee647b6b90729ab34a250b1" => Some("Coredump"),
        "5aadd8e954dc4b1a8c954d63fd9e1137" => Some("Coredump truncated"),
        "1f4e0a44a88649939aaea34fc6da8c95" => Some("Backtrace"),
        "8d45620c1a4348dbb17410da57c60c66" => Some("User Session created"),
        "3354939424b4456d9802ca8333ed424a" => Some("User Session terminated"),
        "fcbefc5da23d428093f97c82a9290f7b" => Some("Seat started"),
        "e7852bfe46784ed0accde04bc864c2d5" => Some("Seat removed"),
        "24d8d4452573402496068381a6312df2" => Some("VM or container started"),
        "58432bd3bace477cb514b56381b8a758" => Some("VM or container stopped"),
        "c7a787079b354eaaa9e77b371893cd27" => Some("Time change"),
        "45f82f4aef7a4bbf942ce861d1f20990" => Some("Timezone change"),
        "50876a9db00f4c40bde1a2ad381c3a1b" => Some("System configuration issues"),
        "b07a249cd024414a82dd00cd181378ff" => Some("System start-up completed"),
        "eed00a68ffd84e31882105fd973abdd1" => Some("User start-up completed"),
        "6bbd95ee977941e497c48be27c254128" => Some("Sleep start"),
        "8811e6df2a8e40f58a94cea26f8ebf14" => Some("Sleep stop"),
        "98268866d1d54a499c4e98921d93bc40" => Some("System shutdown initiated"),
        "c14aaf76ec284a5fa1f105f88dfb061c" => Some("System factory reset initiated"),
        "d9ec5e95e4b646aaaea2fd05214edbda" => Some("Container init crashed"),
        "3ed0163e868a4417ab8b9e210407a96c" => Some("System reboot failed after crash"),
        "645c735537634ae0a32b15a7c6cba7d4" => Some("Init execution froze"),
        "5addb3a06a734d3396b794bf98fb2d01" => Some("Init crashed no coredump"),
        "5c9e98de4ab94c6a9d04d0ad793bd903" => Some("Init crashed no fork"),
        "5e6f1f5e4db64a0eaee3368249d20b94" => Some("Init crashed unknown signal"),
        "83f84b35ee264f74a3896a9717af34cb" => Some("Init crashed systemd signal"),
        "3a73a98baf5b4b199929e3226c0be783" => Some("Init crashed process signal"),
        "2ed18d4f78ca47f0a9bc25271c26adb4" => Some("Init crashed waitpid failed"),
        "56b1cd96f24246c5b607666fda952356" => Some("Init crashed coredump failed"),
        "4ac7566d4d7548f4981f629a28f0f829" => Some("Init crashed coredump"),
        "38e8b1e039ad469291b18b44c553a5b7" => Some("Crash shell failed to fork"),
        "872729b47dbe473eb768ccecd477beda" => Some("Crash shell failed to execute"),
        "658a67adc1c940b3b3316e7e8628834a" => Some("Selinux failed"),
        "e6f456bd92004d9580160b2207555186" => Some("Battery low warning"),
        "267437d33fdd41099ad76221cc24a335" => Some("Battery low powering off"),
        "79e05b67bc4545d1922fe47107ee60c5" => Some("Manager mainloop failed"),
        "dbb136b10ef4457ba47a795d62f108c9" => Some("Manager no xdgdir path"),
        "ed158c2df8884fa584eead2d902c1032" => {
            Some("Init failed to drop capability bounding set of usermode")
        }
        "42695b500df048298bee37159caa9f2e" => Some("Init failed to drop capability bounding set"),
        "bfc2430724ab44499735b4f94cca9295" => Some("User manager can't disable new privileges"),
        "59288af523be43a28d494e41e26e4510" => Some("Manager failed to start default target"),
        "689b4fcc97b4486ea5da92db69c9e314" => Some("Manager failed to isolate default target"),
        "5ed836f1766f4a8a9fc5da45aae23b29" => {
            Some("Manager failed to collect passed file descriptors")
        }
        "6a40fbfbd2ba4b8db02fb40c9cd090d7" => Some("Init failed to fix up environment variables"),
        "0e54470984ac419689743d957a119e2e" => Some("Manager failed to allocate"),
        "d67fa9f847aa4b048a2ae33535331adb" => Some("Manager failed to write Smack"),
        "af55a6f75b544431b72649f36ff6d62c" => Some("System shutdown critical error"),
        "d18e0339efb24a068d9c1060221048c2" => Some("Init failed to fork off valgrind"),
        "7d4958e842da4a758f6c1cdc7b36dcc5" => Some("Unit starting"),
        "39f53479d3a045ac8e11786248231fbf" => Some("Unit started"),
        "be02cf6855d2428ba40df7e9d022f03d" => Some("Unit failed"),
        "de5b426a63be47a7b6ac3eaac82e2f6f" => Some("Unit stopping"),
        "9d1aaa27d60140bd96365438aad20286" => Some("Unit stopped"),
        "d34d037fff1847e6ae669a370e694725" => Some("Unit reloading"),
        "7b05ebc668384222baa8881179cfda54" => Some("Unit reloaded"),
        "5eb03494b6584870a536b337290809b3" => Some("Unit restart scheduled"),
        "ae8f7b866b0347b9af31fe1c80b127c0" => Some("Unit resources"),
        "7ad2d189f7e94e70a38c781354912448" => Some("Unit success"),
        "0e4284a0caca4bfc81c0bb6786972673" => Some("Unit skipped"),
        "d9b373ed55a64feb8242e02dbe79a49c" => Some("Unit failure result"),
        "641257651c1b4ec9a8624d7a40a9e1e7" => Some("Process execution failed"),
        "98e322203f7a4ed290d09fe03c09fe15" => Some("Unit process exited"),
        "0027229ca0644181a76c4e92458afa2e" => Some("Syslog forward missed"),
        "1dee0369c7fc4736b7099b38ecb46ee7" => Some("Mount point is not empty"),
        "d989611b15e44c9dbf31e3c81256e4ed" => Some("Unit oomd kill"),
        "fe6faa94e7774663a0da52717891d8ef" => Some("Unit out of memory"),
        "b72ea4a2881545a0b50e200e55b9b06f" => Some("Lid opened"),
        "b72ea4a2881545a0b50e200e55b9b070" => Some("Lid closed"),
        "f5f416b862074b28927a48c3ba7d51ff" => Some("System docked"),
        "51e171bd585248568110144c517cca53" => Some("System undocked"),
        "b72ea4a2881545a0b50e200e55b9b071" => Some("Power key"),
        "3e0117101eb243c1b9a50db3494ab10b" => Some("Power key long press"),
        "9fa9d2c012134ec385451ffe316f97d0" => Some("Reboot key"),
        "f1c59a58c9d943668965c337caec5975" => Some("Reboot key long press"),
        "b72ea4a2881545a0b50e200e55b9b072" => Some("Suspend key"),
        "bfdaf6d312ab4007bc1fe40a15df78e8" => Some("Suspend key long press"),
        "b72ea4a2881545a0b50e200e55b9b073" => Some("Hibernate key"),
        "167836df6f7f428e98147227b2dc8945" => Some("Hibernate key long press"),
        "c772d24e9a884cbeb9ea12625c306c01" => Some("Invalid configuration"),
        "1675d7f172174098b1108bf8c7dc8f5d" => Some("DNSSEC validation failed"),
        "4d4408cfd0d144859184d1e65d7c8a65" => Some("DNSSEC trust anchor revoked"),
        "36db2dfa5a9045e1bd4af5f93e1cf057" => Some("DNSSEC turned off"),
        "b61fdac612e94b9182285b998843061f" => Some("Username unsafe"),
        "1b3bb94037f04bbf81028e135a12d293" => Some("Mount point path not suitable"),
        "010190138f494e29a0ef6669749531aa" => Some("Device path not suitable"),
        "b480325f9c394a7b802c231e51a2752c" => Some("Nobody user unsuitable"),
        "1c0454c1bd2241e0ac6fefb4bc631433" => Some("Systemd udev settle deprecated"),
        "7c8a41f37b764941a0e1780b1be2f037" => Some("Time initial sync"),
        "7db73c8af0d94eeb822ae04323fe6ab6" => Some("Time initial bump"),
        "9e7066279dc8403da79ce4b1a69064b2" => Some("Shutdown scheduled"),
        "249f6fb9e6e2428c96f3f0875681ffa3" => Some("Shutdown canceled"),
        "3f7d5ef3e54f4302b4f0b143bb270cab" => Some("TPM PCR Extended"),
        "f9b0be465ad540d0850ad32172d57c21" => Some("Memory Trimmed"),
        "a8fa8dacdb1d443e9503b8be367a6adb" => Some("SysV Service Found"),
        "187c62eb1e7f463bb530394f52cb090f" => Some("Portable Service attached"),
        "76c5c754d628490d8ecba4c9d042112b" => Some("Portable Service detached"),
        "9cf56b8baf9546cf9478783a8de42113" => {
            Some("systemd-networkd sysctl changed by foreign process")
        }
        "ad7089f928ac4f7ea00c07457d47ba8a" => Some("SRK into TPM authorization failure"),
        "b2bcbaf5edf948e093ce50bbea0e81ec" => Some("Secure Attention Key (SAK) was pressed"),
        "7fc63312330b479bb32e598d47cef1a8" => Some("dbus activate no unit"),
        "ee9799dab1e24d81b7bee7759a543e1b" => Some("dbus activate masked unit"),
        "a0fa58cafd6f4f0c8d003d16ccf9e797" => Some("dbus broker exited"),
        "c8c6cde1c488439aba371a664353d9d8" => Some("dbus dirwatch"),
        "8af3357071af4153af414daae07d38e7" => Some("dbus dispatch stats"),
        "199d4300277f495f84ba4028c984214c" => Some("dbus no sopeergroup"),
        "b209c0d9d1764ab38d13b8e00d1784d6" => Some("dbus protocol violation"),
        "6fa70fa776044fa28be7a21daf42a108" => Some("dbus receive failed"),
        "0ce0fa61d1a9433dabd67417f6b8e535" => Some("dbus service failed open"),
        "24dc708d9e6a4226a3efe2033bb744de" => Some("dbus service invalid"),
        "f15d2347662d483ea9bcd8aa1a691d28" => Some("dbus sighup"),
        "0ce153587afa4095832d233c17a88001" => Some("Gnome SM startup succeeded"),
        "10dd2dc188b54a5e98970f56499d1f73" => Some("Gnome SM unrecoverable failure"),
        "f3ea493c22934e26811cd62abe8e203a" => Some("Gnome shell started"),
        "c7b39b1e006b464599465e105b361485" => Some("Flatpak cache"),
        "75ba3deb0af041a9a46272ff85d9e73e" => Some("Flathub pulls"),
        "f02bce89a54e4efab3a94a797d26204a" => Some("Flathub pull errors"),
        "dd11929c788e48bdbb6276fb5f26b08a" => Some("Boltd starting"),
        "1e6061a9fbd44501b3ccc368119f2b69" => Some("Netdata startup"),
        "ed4cdb8f1beb4ad3b57cb3cae2d162fa" => Some("Netdata connection from child"),
        "6e2e3839067648968b646045dbf28d66" => Some("Netdata connection to parent"),
        "9ce0cb58ab8b44df82c4bf1ad9ee22de" => Some("Netdata alert transition"),
        "6db0018e83e34320ae2a659d78019fb7" => Some("Netdata alert notification"),
        "23e93dfccbf64e11aac858b9410d8a82" => Some("Netdata fatal message"),
        "8ddaf5ba33a74078b609250db1e951f3" => Some("Sensor state transition"),
        "ec87a56120d5431bace51e2fb8bba243" => Some("Netdata log flood protection"),
        "acb33cb95778476baac702eb7e4e151d" => Some("Netdata Cloud connection"),
        "d1f59606dd4d41e3b217a0cfcae8e632" => Some("Netdata extreme cardinality"),
        "02f47d350af5449197bf7a95b605a468" => Some("Netdata exit reason"),
        "4fdf40816c124623a032b7fe73beacb8" => Some("Netdata dynamic configuration"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExplorerHistogramBucket;
    use journal_core::file::{JournalFile, JournalFileOptions, JournalWriter, MmapMut};
    use journal_core::repository::File as RepoFile;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn test_uuid(seed: u8) -> uuid::Uuid {
        uuid::Uuid::from_bytes([seed; 16])
    }

    fn write_netdata_test_journal(directory: &std::path::Path, count: usize) {
        std::fs::create_dir_all(directory).expect("create journal dir");
        let path = directory.join("netdata-api-test.journal");
        let repo_file = RepoFile::from_path(&path).expect("repo file");
        let options = JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3));
        let mut file = JournalFile::<MmapMut>::create(&repo_file, options).expect("create journal");
        let mut writer = JournalWriter::new(&mut file, 1, test_uuid(4)).expect("writer");
        for index in 0..count {
            let message = format!("MESSAGE=row-{index}");
            let service = if index % 2 == 0 {
                b"SERVICE=even".as_slice()
            } else {
                b"SERVICE=odd".as_slice()
            };
            let payloads: [&[u8]; 3] = [message.as_bytes(), service, b"PRIORITY=6"];
            let realtime = 1_700_000_000_000_000u64.saturating_add(index as u64);
            writer
                .add_entry(&mut file, &payloads, realtime, realtime)
                .expect("write entry");
        }
        file.sync().expect("sync journal");
    }

    #[test]
    fn parses_netdata_selections_as_and_fields_or_values() {
        let request = json!({
            "after": 200_000_000,
            "before": 200_000_100,
            "direction": "forward",
            "last": 25,
            "facets": ["PRIORITY"],
            "selections": {
                "PRIORITY": ["warning", "error"],
                "_HOSTNAME": ["node-a"],
                "__logs_sources": ["all-local-system-logs"],
            }
        });

        let parsed = NetdataRequest::parse(&request, &NetdataFunctionConfig::systemd_journal())
            .expect("parse request");
        assert_eq!(parsed.after_realtime_usec, Some(200_000_000_000_000));
        assert_eq!(parsed.before_realtime_usec, Some(200_000_100_999_999));
        assert_eq!(parsed.direction, Direction::Forward);
        assert_eq!(parsed.limit, 25);
        assert_eq!(parsed.filters.len(), 2);
        assert_eq!(parsed.filters[0].field, b"PRIORITY");
        assert_eq!(parsed.filters[0].values, vec![b"4".to_vec(), b"3".to_vec()]);
        assert_eq!(parsed.filters[1].field, b"_HOSTNAME");
        assert_eq!(parsed.filters[1].values, vec![b"node-a".to_vec()]);
    }

    #[test]
    fn netdata_requests_never_enable_debug_row_traversal_column_collection() {
        let request = json!({
            "facets": ["PRIORITY", "_HOSTNAME"],
            "histogram": "PRIORITY",
            "last": 25
        });

        let parsed = NetdataRequest::parse(&request, &NetdataFunctionConfig::systemd_journal())
            .expect("parse request");
        let query = parsed.to_explorer_query();

        assert!(!query.debug_collect_column_fields_by_row_traversal);
    }

    #[test]
    fn netdata_function_api_reports_progress() {
        let dir = TempDir::new().expect("tempdir");
        write_netdata_test_journal(dir.path(), 9_000);
        let request = json!({
            "after": 1_700_000_000,
            "before": 1_700_000_010,
            "facets": ["SERVICE"],
            "histogram": "SERVICE",
            "last": 0
        });
        let function = NetdataJournalFunction::systemd_journal_plugin_compatible();
        let mut reports = 0u64;
        let mut progress = |progress: NetdataFunctionProgress| {
            reports = reports.saturating_add(1);
            assert_eq!(progress.current_file, 1);
            assert_eq!(progress.total_files, 1);
            assert!(progress.stats.rows_examined <= 9_000);
        };
        let mut options = NetdataFunctionRunOptions::from_timeout_seconds(0);
        options.progress_interval = Duration::ZERO;
        options.progress_callback = Some(&mut progress);

        let response = function
            .run_directory_request_json_with_options(dir.path(), &request, options)
            .expect("run function");

        assert_eq!(response["status"], 200);
        assert!(reports > 0);
        assert_eq!(response["last_modified"], 1_700_000_000_008_999u64);
    }

    #[test]
    fn netdata_function_api_reports_file_end_progress_for_small_scans() {
        let dir = TempDir::new().expect("tempdir");
        write_netdata_test_journal(dir.path(), 10);
        let request = json!({
            "after": 1_700_000_000,
            "before": 1_700_000_010,
            "facets": ["SERVICE"],
            "histogram": "SERVICE",
            "last": 0
        });
        let function = NetdataJournalFunction::systemd_journal_plugin_compatible();
        let mut reports = 0u64;
        let mut last_rows_examined = 0u64;
        let mut progress = |progress: NetdataFunctionProgress| {
            reports = reports.saturating_add(1);
            last_rows_examined = progress.stats.rows_examined;
        };
        let mut options = NetdataFunctionRunOptions::from_timeout_seconds(0);
        options.progress_callback = Some(&mut progress);

        let response = function
            .run_directory_request_json_with_options(dir.path(), &request, options)
            .expect("run function");

        assert_eq!(response["status"], 200);
        assert_eq!(reports, 1);
        assert_eq!(last_rows_examined, 10);
    }

    #[test]
    fn netdata_function_api_reports_cancellation() {
        let dir = TempDir::new().expect("tempdir");
        write_netdata_test_journal(dir.path(), 9_000);
        let request = json!({
            "after": 1_700_000_000,
            "before": 1_700_000_010,
            "facets": ["SERVICE"],
            "histogram": "SERVICE",
            "last": 0
        });
        let function = NetdataJournalFunction::systemd_journal_plugin_compatible();
        let is_cancelled = || true;
        let mut options = NetdataFunctionRunOptions::from_timeout_seconds(0);
        options.cancellation_callback = Some(&is_cancelled);

        let response = function
            .run_directory_request_json_with_options(dir.path(), &request, options)
            .expect("run function");

        assert_eq!(response["status"], 499);
        assert_eq!(response["errorMessage"], "Request cancelled.");
        assert_eq!(
            response.as_object().expect("response object").len(),
            2,
            "plugin-compatible function errors only include status and errorMessage"
        );
    }

    #[test]
    fn netdata_function_api_reports_timeout_as_partial_table() {
        let dir = TempDir::new().expect("tempdir");
        write_netdata_test_journal(dir.path(), 10);
        let request = json!({
            "after": 1_700_000_000,
            "before": 1_700_000_010,
            "facets": ["SERVICE"],
            "histogram": "SERVICE",
            "last": 0
        });
        let function = NetdataJournalFunction::systemd_journal_plugin_compatible();
        let options = NetdataFunctionRunOptions {
            timeout: Some(Duration::ZERO),
            ..NetdataFunctionRunOptions::default()
        };

        let response = function
            .run_directory_request_json_with_options(dir.path(), &request, options)
            .expect("run function");

        assert_eq!(response["status"], 200);
        assert_eq!(response["partial"], true);
        assert_eq!(response["message"]["status"], "warning");
        assert_eq!(
            response["message"]["title"],
            "Query timed-out, incomplete data. "
        );
    }

    #[test]
    fn normalizes_missing_time_window_to_last_hour_like_plugin() {
        assert_eq!(
            normalize_time_window(1_000_000_000, None, None),
            (Some(999_996_400_000_000), Some(1_000_000_000_999_999))
        );
    }

    #[test]
    fn normalizes_inverted_time_window_like_plugin() {
        assert_eq!(
            normalize_time_window(1_000_000_000, Some(200_000_100), Some(200_000_000)),
            (Some(200_000_000_000_000), Some(200_000_100_999_999))
        );
    }

    #[test]
    fn normalizes_equal_time_window_like_plugin() {
        assert_eq!(
            normalize_time_window(1_000_000_000, Some(200_000_000), Some(200_000_000)),
            (Some(199_996_400_000_000), Some(200_000_000_999_999))
        );
    }

    #[test]
    fn normalizes_relative_time_window_like_plugin() {
        assert_eq!(
            normalize_time_window(1_000_000_000, Some(100), Some(200)),
            (Some(999_999_701_000_000), Some(999_999_800_999_999))
        );
    }

    #[test]
    fn normalizes_missing_after_with_supplied_before_like_plugin() {
        assert_eq!(
            normalize_time_window(1_000_000_000, None, Some(200_000_000)),
            (Some(199_999_401_000_000), Some(200_000_000_999_999))
        );
    }

    #[test]
    fn systemd_profile_transforms_priority_and_facility_for_display() {
        let profile = SystemdJournalProfile;
        let context = DisplayContext::default();
        assert_eq!(
            profile.field_display_value(&context, DisplayScope::Data, "PRIORITY", b"7"),
            json!("debug")
        );
        assert_eq!(
            profile.field_display_value(&context, DisplayScope::Data, "SYSLOG_FACILITY", b"3"),
            json!("daemon")
        );
        assert_eq!(priority_to_row_severity(b"3"), "critical");
        assert_eq!(priority_to_row_severity(b"6"), "normal");
    }

    #[test]
    fn dynamic_process_name_matches_plugin_fallback_order() {
        let mut fields = BTreeMap::new();
        fields.insert("SYSLOG_IDENTIFIER".to_string(), vec![b"syslog".to_vec()]);
        fields.insert("_COMM".to_string(), vec![b"comm".to_vec()]);
        fields.insert("_PID".to_string(), vec![b"42".to_vec()]);
        fields.insert("SYSLOG_PID".to_string(), vec![b"99".to_vec()]);
        assert_eq!(dynamic_process_name(&fields), "syslog[42]");

        fields.insert("CONTAINER_NAME".to_string(), vec![b"container".to_vec()]);
        assert_eq!(dynamic_process_name(&fields), "container[42]");

        fields.remove("CONTAINER_NAME");
        fields.remove("SYSLOG_IDENTIFIER");
        fields.remove("_PID");
        assert_eq!(dynamic_process_name(&fields), "comm");

        fields.remove("_COMM");
        fields.insert("_EXE".to_string(), vec![b"/usr/bin/app".to_vec()]);
        assert_eq!(dynamic_process_name(&fields), "-");
    }

    #[test]
    fn facet_values_are_truncated_and_collapsed_like_plugin() {
        let prefix = vec![b'a'; NETDATA_FACET_MAX_VALUE_LENGTH];
        let mut first = prefix.clone();
        first.extend_from_slice(b"-first");
        let mut second = prefix.clone();
        second.extend_from_slice(b"-second");

        let mut values = BTreeMap::new();
        add_netdata_facet_count(&mut values, &first, 2);
        add_netdata_facet_count(&mut values, &second, 3);

        assert_eq!(values.len(), 1);
        assert_eq!(values.get(&prefix), Some(&5));
    }

    #[test]
    fn histogram_values_are_truncated_and_collapsed_like_plugin() {
        let prefix = vec![b'b'; NETDATA_FACET_MAX_VALUE_LENGTH];
        let mut first = prefix.clone();
        first.extend_from_slice(b"-first");
        let mut second = prefix.clone();
        second.extend_from_slice(b"-second");

        let mut values = HashMap::new();
        values.insert(first, 2);
        values.insert(second, 3);
        let histogram = ExplorerHistogram {
            field: b"TEST_FIELD".to_vec(),
            buckets: vec![ExplorerHistogramBucket {
                start_realtime_usec: 1_000_000,
                end_realtime_usec: 2_000_000,
                values,
            }],
        };

        let function = NetdataJournalFunction::systemd_journal();
        let rendered = function.build_histogram(&DisplayContext::default(), &histogram, None);
        let labels = rendered["chart"]["result"]["labels"]
            .as_array()
            .expect("labels");
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[1], Value::String(String::from_utf8(prefix).unwrap()));
        assert_eq!(rendered["chart"]["result"]["data"][0][1][0], json!(5));
    }

    #[test]
    fn duplicate_row_timestamps_match_plugin_direction_adjustment() {
        let mut backward = vec![
            test_located_row(100),
            test_located_row(100),
            test_located_row(100),
            test_located_row(90),
        ];
        make_row_timestamps_unique(&mut backward, Direction::Backward);
        assert_eq!(
            backward
                .iter()
                .map(|row| row.row.realtime_usec)
                .collect::<Vec<_>>(),
            vec![100, 99, 98, 90]
        );

        let mut forward = vec![
            test_located_row(90),
            test_located_row(100),
            test_located_row(100),
            test_located_row(100),
        ];
        make_row_timestamps_unique(&mut forward, Direction::Forward);
        assert_eq!(
            forward
                .iter()
                .map(|row| row.row.realtime_usec)
                .collect::<Vec<_>>(),
            vec![90, 100, 101, 102]
        );
    }

    #[test]
    fn systemd_profile_keeps_user_group_ids_raw_by_default() {
        let context = DisplayContext::default();
        let profile = SystemdJournalProfile;
        assert_eq!(
            profile.field_display_value(&context, DisplayScope::Facet, "_UID", b"0"),
            json!("0")
        );
        assert_eq!(
            profile.field_display_value(&context, DisplayScope::Facet, "_GID", b"0"),
            json!("0")
        );
    }

    #[cfg(unix)]
    #[test]
    fn plugin_compatible_profile_resolves_user_group_ids_explicitly() {
        let context = DisplayContext::default();
        let profile = SystemdJournalPluginProfile;
        assert_eq!(
            profile.field_display_value(&context, DisplayScope::Facet, "_UID", b"0"),
            json!("root")
        );
        assert_eq!(
            profile.field_display_value(&context, DisplayScope::Facet, "_GID", b"0"),
            json!("root")
        );
    }

    #[test]
    fn file_overlap_uses_netdata_max_realtime_slack() {
        let file_first_seconds = 200_000_000u64;
        let file_last_seconds = 200_000_100u64;
        let slack_seconds = NETDATA_JOURNAL_VS_REALTIME_DELTA_MAX_USEC / 1_000_000;
        let header = crate::FileHeader {
            signature: *b"LPKSHHRH",
            compatible_flags: 0,
            incompatible_flags: 0,
            state: 0,
            header_size: 0,
            head_entry_realtime: file_first_seconds * 1_000_000,
            tail_entry_realtime: file_last_seconds * 1_000_000,
            head_entry_seqnum: 0,
            tail_entry_seqnum: 0,
            tail_entry_boot_id: [0; 16],
            seqnum_id: [0; 16],
        };
        let config = NetdataFunctionConfig::systemd_journal();

        let inside_slack = NetdataRequest::parse(
            &json!({
                "after": file_last_seconds + slack_seconds - 1,
                "before": file_last_seconds + slack_seconds + 500
            }),
            &config,
        )
        .expect("parse request");
        assert!(file_may_overlap_request(header, &inside_slack));

        let outside_slack = NetdataRequest::parse(
            &json!({
                "after": file_last_seconds + slack_seconds + 1,
                "before": file_last_seconds + slack_seconds + 500
            }),
            &config,
        )
        .expect("parse request");
        assert!(!file_may_overlap_request(header, &outside_slack));
    }

    #[test]
    fn journal_file_order_matches_plugin_comparator_shape() {
        let older = JournalFileOrderInfo {
            msg_first_realtime_usec: 100,
            msg_last_realtime_usec: 200,
            file_last_modified_usec: 200,
        };
        let newer = JournalFileOrderInfo {
            msg_first_realtime_usec: 100,
            msg_last_realtime_usec: 300,
            file_last_modified_usec: 100,
        };
        assert_eq!(
            compare_journal_file_order(&newer, &older, Direction::Backward),
            Ordering::Less
        );
        assert_eq!(
            compare_journal_file_order(&newer, &older, Direction::Forward),
            Ordering::Greater
        );

        let newer_mtime = JournalFileOrderInfo {
            msg_first_realtime_usec: 100,
            msg_last_realtime_usec: 200,
            file_last_modified_usec: 300,
        };
        assert_eq!(
            compare_journal_file_order(&newer_mtime, &older, Direction::Backward),
            Ordering::Less
        );

        let newer_first = JournalFileOrderInfo {
            msg_first_realtime_usec: 150,
            msg_last_realtime_usec: 200,
            file_last_modified_usec: 200,
        };
        assert_eq!(
            compare_journal_file_order(&newer_first, &older, Direction::Backward),
            Ordering::Less
        );
    }

    #[test]
    fn boot_first_realtime_keeps_earliest_timestamp_like_plugin() {
        let mut boot_first = BTreeMap::new();
        record_boot_first_realtime(&mut boot_first, b"boot-a".to_vec(), 300);
        record_boot_first_realtime(&mut boot_first, b"boot-a".to_vec(), 100);
        record_boot_first_realtime(&mut boot_first, b"boot-a".to_vec(), 200);

        assert_eq!(boot_first.get(b"boot-a".as_slice()), Some(&100));
    }

    #[test]
    fn source_selection_echoes_and_filters_known_groups() {
        let config = NetdataFunctionConfig::systemd_journal();
        let request = NetdataRequest::parse(
            &json!({
                "selections": {
                    "__logs_sources": ["all-local-system-logs"]
                }
            }),
            &config,
        )
        .expect("parse source-filtered request");

        assert_eq!(request.source_type, SOURCE_TYPE_LOCAL_SYSTEM);
        assert_eq!(
            request.echo.get("source_type").and_then(Value::as_u64),
            Some(SOURCE_TYPE_LOCAL_SYSTEM)
        );
        assert!(
            request
                .echo
                .pointer("/selections/__logs_sources/0")
                .is_some_and(Value::is_null)
        );
        assert!(request.matches_source(Path::new("/var/log/journal/machine/system.journal")));
        assert!(!request.matches_source(Path::new("/var/log/journal/machine/user-1000.journal")));
    }

    #[test]
    fn source_classification_matches_plugin_filename_shape() {
        assert_eq!(
            journal_file_source_type(Path::new("/var/log/journal/machine/system.journal")),
            SOURCE_TYPE_ALL | SOURCE_TYPE_LOCAL_ALL | SOURCE_TYPE_LOCAL_SYSTEM
        );
        assert_eq!(
            journal_file_source_type(Path::new("/var/log/journal/machine/user-1000.journal")),
            SOURCE_TYPE_ALL | SOURCE_TYPE_LOCAL_ALL | SOURCE_TYPE_LOCAL_USER
        );
        assert_eq!(
            journal_file_source_type(Path::new("/var/log/journal/machine/other.journal")),
            SOURCE_TYPE_ALL | SOURCE_TYPE_LOCAL_ALL | SOURCE_TYPE_LOCAL_OTHER
        );
        assert_eq!(
            journal_file_source_type(Path::new(
                "/var/log/journal/machine.namespace/system.journal"
            )),
            SOURCE_TYPE_ALL | SOURCE_TYPE_LOCAL_ALL | SOURCE_TYPE_LOCAL_NAMESPACE
        );
        assert_eq!(
            journal_file_source_type(Path::new(
                "/var/log/journal/remote/remote-host-a@machine.journal"
            )),
            SOURCE_TYPE_ALL | SOURCE_TYPE_REMOTE_ALL
        );
    }

    #[test]
    fn exact_source_names_follow_plugin_prefixes() {
        assert_eq!(
            journal_file_exact_source_name(Path::new(
                "/var/log/journal/machine.namespace/system.journal"
            ))
            .as_deref(),
            Some("namespace-namespace")
        );
        assert_eq!(
            journal_file_exact_source_name(Path::new(
                "/var/log/journal/remote/remote-host-a@machine.journal"
            ))
            .as_deref(),
            Some("remote-host-a")
        );
        assert_eq!(
            journal_file_exact_source_name(Path::new(
                "/var/log/journal/remote/remote-host-b.journal~.zst"
            ))
            .as_deref(),
            Some("remote-host-b")
        );
    }

    #[test]
    fn disposed_journal_extension_matches_plugin_scan_contract() {
        assert!(is_journal_file_name(Path::new("active.journal")));
        assert!(is_journal_file_name(Path::new("archived.journal~")));
        assert!(is_journal_file_name(Path::new("active.journal.zst")));
        assert!(is_journal_file_name(Path::new("archived.journal~.zst")));
    }

    fn test_located_row(realtime_usec: u64) -> LocatedRow {
        LocatedRow {
            file_path: PathBuf::from("test.journal"),
            row: ExplorerRow {
                realtime_usec,
                cursor: String::new(),
                payloads: Vec::new(),
            },
        }
    }
}
