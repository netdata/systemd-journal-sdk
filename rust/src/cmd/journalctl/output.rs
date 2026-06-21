use super::{Args, OutputModeArg, format_journal_bytes};
use anyhow::{Result, anyhow};
use chrono::{Local, TimeZone, Utc};
use journal::Entry;
use serde_json::{Map, Value};
#[cfg(unix)]
use std::ffi::CStr;

#[derive(Clone)]
pub(crate) struct OutputOptions {
    mode: OutputModeArg,
    output_fields: Vec<String>,
    output_fields_set: bool,
    utc: bool,
    no_hostname: bool,
    full_width: bool,
    show_all: bool,
    truncate_newline: bool,
    merge: bool,
}

const PRINT_CHAR_THRESHOLD: usize = 300;
const JSON_THRESHOLD: usize = 4096;
const DEFAULT_COLUMNS: usize = 80;

impl OutputOptions {
    pub(crate) fn from_args(args: &Args, full_width: bool) -> Self {
        Self {
            mode: args.output.clone(),
            output_fields: parse_output_fields(args.output_fields.as_deref()),
            output_fields_set: args.output_fields.is_some(),
            utc: args.utc,
            no_hostname: args.no_hostname,
            full_width,
            show_all: args.all,
            truncate_newline: args.truncate_newline,
            merge: args.merge,
        }
    }

    pub(crate) fn mode(&self) -> OutputModeArg {
        self.mode
    }

    pub(crate) fn suppress_boot_separators(&self) -> bool {
        self.merge
    }
}

pub(crate) struct OutputRenderer {
    options: OutputOptions,
    previous_delta: Option<(u64, u64, [u8; 16])>,
}

impl OutputRenderer {
    pub(crate) fn new(options: OutputOptions) -> Self {
        Self {
            options,
            previous_delta: None,
        }
    }

    pub(crate) fn render(&mut self, entry: &Entry) -> Result<Vec<u8>> {
        match self.options.mode {
            OutputModeArg::Short => self.render_short(entry, TimestampMode::Short),
            OutputModeArg::ShortFull => self.render_short(entry, TimestampMode::ShortFull),
            OutputModeArg::ShortIso => self.render_short(entry, TimestampMode::ShortIso),
            OutputModeArg::ShortIsoPrecise => {
                self.render_short(entry, TimestampMode::ShortIsoPrecise)
            }
            OutputModeArg::ShortPrecise => self.render_short(entry, TimestampMode::ShortPrecise),
            OutputModeArg::ShortMonotonic => {
                self.render_short(entry, TimestampMode::ShortMonotonic)
            }
            OutputModeArg::ShortDelta => self.render_short_delta(entry),
            OutputModeArg::ShortUnix => self.render_short(entry, TimestampMode::ShortUnix),
            OutputModeArg::WithUnit => self.render_with_unit(entry),
            OutputModeArg::Cat => Ok(self.render_cat(entry)),
            OutputModeArg::Verbose => self.render_verbose(entry),
            OutputModeArg::Export => Ok(self.render_export(entry)),
            OutputModeArg::Json => self.render_json(entry, JsonFrame::Line),
            OutputModeArg::JsonPretty => self.render_json(entry, JsonFrame::Pretty),
            OutputModeArg::JsonSse => self.render_json(entry, JsonFrame::Sse),
            OutputModeArg::JsonSeq => self.render_json(entry, JsonFrame::Seq),
            OutputModeArg::Help => Ok(Vec::new()),
        }
    }

    pub(crate) fn skips_entry(&self, entry: &Entry) -> bool {
        output_skips_missing_message(self.options.mode)
            && values_for_name(entry, "MESSAGE").is_empty()
    }

    fn render_short(&mut self, entry: &Entry, timestamp_mode: TimestampMode) -> Result<Vec<u8>> {
        let timestamp = format_timestamp(entry, timestamp_mode, self.options.utc)?;
        let label = entry_label(entry, &self.options);
        let prefix = format!("{timestamp} {label}: ");
        let mut out = prefix.clone().into_bytes();
        out.extend_from_slice(&display_message(entry, &self.options, prefix.len()));
        out.push(b'\n');
        Ok(out)
    }

    fn render_with_unit(&mut self, entry: &Entry) -> Result<Vec<u8>> {
        let timestamp = format_timestamp(entry, TimestampMode::ShortFull, self.options.utc)?;
        let label = format_entry_label(
            entry,
            &unit_label(entry).unwrap_or_else(|| base_entry_label(entry)),
            &self.options,
        );
        let prefix = format!("{timestamp} {label}: ");
        let mut out = prefix.clone().into_bytes();
        out.extend_from_slice(&display_message(entry, &self.options, prefix.len()));
        out.push(b'\n');
        Ok(out)
    }

    fn render_short_delta(&mut self, entry: &Entry) -> Result<Vec<u8>> {
        let label = entry_label(entry, &self.options);
        let current_realtime = display_realtime_usec(entry);
        let current_monotonic = display_monotonic_usec(entry);
        let current = (current_realtime, current_monotonic, entry.boot_id);
        let monotonic = format_monotonic(current_monotonic);
        let delta = match self.previous_delta {
            Some((previous_realtime, previous_monotonic, previous_boot)) => {
                let (diff, marker) = if previous_boot == entry.boot_id {
                    (current_monotonic.abs_diff(previous_monotonic), " ")
                } else {
                    (current_realtime.abs_diff(previous_realtime), "*")
                };
                format!(" <{}{}>", format_monotonic(diff), marker)
            }
            None => "                ".to_string(),
        };
        self.previous_delta = Some(current);
        let prefix = format!("[{monotonic}{delta}] {label}: ");
        let mut out = prefix.clone().into_bytes();
        out.extend_from_slice(&display_message(entry, &self.options, prefix.len()));
        out.push(b'\n');
        Ok(out)
    }

    fn render_cat(&self, entry: &Entry) -> Vec<u8> {
        let mut out = Vec::new();
        if self.options.output_fields_set {
            for name in &self.options.output_fields {
                for value in values_for_name(entry, name) {
                    out.extend_from_slice(&display_value(value, self.options.truncate_newline));
                    out.push(b'\n');
                }
            }
            return out;
        }
        for value in values_for_name(entry, "MESSAGE") {
            out.extend_from_slice(&display_value(value, self.options.truncate_newline));
            out.push(b'\n');
        }
        out
    }

    fn render_verbose(&self, entry: &Entry) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        out.extend_from_slice(
            format!(
                "{} [{}]\n",
                format_timestamp(entry, TimestampMode::Verbose, self.options.utc)?,
                entry.cursor
            )
            .as_bytes(),
        );
        for (name, value) in verbose_fields(entry, &self.options) {
            out.extend_from_slice(format!("    {name}=").as_bytes());
            out.extend_from_slice(&display_verbose_value(&name, value, &self.options));
            out.push(b'\n');
        }
        Ok(out)
    }

    fn render_export(&self, entry: &Entry) -> Vec<u8> {
        let mut out = Vec::new();
        let mut metadata = metadata_fields(entry);
        for (name, value) in &mut metadata {
            write_export_field(&mut out, name.as_bytes(), value);
        }
        for (name, value) in entry_fields(entry, &self.options) {
            write_export_field(&mut out, name.as_bytes(), value);
        }
        out.push(b'\n');
        out
    }

    fn render_json(&self, entry: &Entry, frame: JsonFrame) -> Result<Vec<u8>> {
        let object = json_object(entry, &self.options);
        let mut out = Vec::new();
        match frame {
            JsonFrame::Line => {
                serde_json::to_writer(&mut out, &Value::Object(object))
                    .map_err(|err| anyhow!("json output: {err}"))?;
                out.push(b'\n');
            }
            JsonFrame::Pretty => {
                write_systemd_pretty_json(&mut out, &Value::Object(object))?;
                out.push(b'\n');
            }
            JsonFrame::Sse => {
                out.extend_from_slice(b"data: ");
                serde_json::to_writer(&mut out, &Value::Object(object))
                    .map_err(|err| anyhow!("json output: {err}"))?;
                out.extend_from_slice(b"\n\n");
            }
            JsonFrame::Seq => {
                out.push(0x1e);
                serde_json::to_writer(&mut out, &Value::Object(object))
                    .map_err(|err| anyhow!("json output: {err}"))?;
                out.push(b'\n');
            }
        }
        Ok(out)
    }
}

#[derive(Clone, Copy)]
enum TimestampMode {
    Short,
    ShortFull,
    ShortIso,
    ShortIsoPrecise,
    ShortPrecise,
    ShortMonotonic,
    ShortUnix,
    Verbose,
}

enum JsonFrame {
    Line,
    Pretty,
    Sse,
    Seq,
}

fn parse_output_fields(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(str::to_string)
        .collect()
}

fn format_timestamp(entry: &Entry, mode: TimestampMode, utc: bool) -> Result<String> {
    if matches!(mode, TimestampMode::ShortMonotonic) {
        return Ok(format!(
            "[{}]",
            format_monotonic(display_monotonic_usec(entry))
        ));
    }
    let realtime = display_realtime_usec(entry);
    if matches!(mode, TimestampMode::ShortUnix) {
        return Ok(format!(
            "{}.{:06}",
            realtime / 1_000_000,
            realtime % 1_000_000
        ));
    }

    let secs = (realtime / 1_000_000) as i64;
    let nanos = ((realtime % 1_000_000) * 1000) as u32;
    let needs_zone_name = matches!(mode, TimestampMode::ShortFull | TimestampMode::Verbose);
    let pattern = match mode {
        TimestampMode::Short => "%b %d %H:%M:%S",
        TimestampMode::ShortFull => "%a %Y-%m-%d %H:%M:%S",
        TimestampMode::ShortIso => "%Y-%m-%dT%H:%M:%S%:z",
        TimestampMode::ShortIsoPrecise => "%Y-%m-%dT%H:%M:%S%.6f%:z",
        TimestampMode::ShortPrecise => "%b %d %H:%M:%S%.6f",
        TimestampMode::Verbose => "%a %Y-%m-%d %H:%M:%S%.6f",
        TimestampMode::ShortMonotonic | TimestampMode::ShortUnix => unreachable!(),
    };

    if utc {
        let dt = Utc
            .timestamp_opt(secs, nanos)
            .single()
            .ok_or_else(|| anyhow!("invalid realtime timestamp"))?;
        let formatted = dt.format(pattern).to_string();
        return Ok(if needs_zone_name {
            format!("{formatted} UTC")
        } else {
            formatted
        });
    }
    let dt = Local
        .timestamp_opt(secs, nanos)
        .single()
        .ok_or_else(|| anyhow!("invalid realtime timestamp"))?;
    let formatted = dt.format(pattern).to_string();
    if needs_zone_name {
        let zone = local_timezone_name(secs).unwrap_or_else(|| dt.format("%Z").to_string());
        return Ok(format!("{formatted} {zone}"));
    }
    Ok(formatted)
}

fn output_skips_missing_message(mode: OutputModeArg) -> bool {
    matches!(
        mode,
        OutputModeArg::Short
            | OutputModeArg::ShortFull
            | OutputModeArg::ShortIso
            | OutputModeArg::ShortIsoPrecise
            | OutputModeArg::ShortPrecise
            | OutputModeArg::ShortMonotonic
            | OutputModeArg::ShortDelta
            | OutputModeArg::ShortUnix
            | OutputModeArg::WithUnit
    )
}

fn display_realtime_usec(entry: &Entry) -> u64 {
    source_realtime_usec(entry).unwrap_or(entry.realtime)
}

fn display_monotonic_usec(entry: &Entry) -> u64 {
    source_realtime_usec(entry)
        .map(|source| map_clock_usec(entry.monotonic, entry.realtime, source))
        .unwrap_or(entry.monotonic)
}

fn source_realtime_usec(entry: &Entry) -> Option<u64> {
    values_for_name(entry, "_SOURCE_REALTIME_TIMESTAMP")
        .first()
        .and_then(|value| std::str::from_utf8(value).ok())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
}

fn map_clock_usec(value: u64, from: u64, to: u64) -> u64 {
    let mapped = value as i128 + to as i128 - from as i128;
    mapped.clamp(0, u64::MAX as i128) as u64
}

pub(crate) fn format_header_timestamp(usec: u64) -> Result<String> {
    if usec == 0 {
        return Ok("n/a".to_string());
    }
    let secs = (usec / 1_000_000) as i64;
    let nanos = ((usec % 1_000_000) * 1000) as u32;
    let dt = Local
        .timestamp_opt(secs, nanos)
        .single()
        .ok_or_else(|| anyhow!("invalid realtime timestamp"))?;
    let formatted = dt.format("%a %Y-%m-%d %H:%M:%S").to_string();
    let zone = local_timezone_name(secs).unwrap_or_else(|| dt.format("%Z").to_string());
    Ok(format!("{formatted} {zone}"))
}

fn format_monotonic(usec: u64) -> String {
    format!("{:>5}.{:06}", usec / 1_000_000, usec % 1_000_000)
}

#[cfg(unix)]
fn local_timezone_name(secs: i64) -> Option<String> {
    let timestamp = libc::time_t::try_from(secs).ok()?;
    let mut local_tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: localtime_r initializes local_tm for the provided timestamp
    // and returns a null pointer on failure.
    let ptr = unsafe { libc::localtime_r(&timestamp, local_tm.as_mut_ptr()) };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: localtime_r returned success, so local_tm is initialized.
    let local_tm = unsafe { local_tm.assume_init() };
    if local_tm.tm_zone.is_null() {
        return None;
    }
    // SAFETY: tm_zone is a NUL-terminated C string owned by the C runtime.
    unsafe { CStr::from_ptr(local_tm.tm_zone) }
        .to_str()
        .ok()
        .map(str::to_string)
}

#[cfg(not(unix))]
fn local_timezone_name(_secs: i64) -> Option<String> {
    None
}

fn entry_label(entry: &Entry, options: &OutputOptions) -> String {
    format_entry_label(entry, &base_entry_label(entry), options)
}

fn base_entry_label(entry: &Entry) -> String {
    first_string(entry, "SYSLOG_IDENTIFIER")
        .or_else(|| first_string(entry, "_COMM"))
        .or_else(|| first_string(entry, "_EXE"))
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_entry_label(entry: &Entry, label: &str, options: &OutputOptions) -> String {
    let mut parts = Vec::new();
    if !options.no_hostname {
        if let Some(hostname) = first_string(entry, "_HOSTNAME") {
            parts.push(hostname);
        }
    }
    let mut label = if label.is_empty() {
        "unknown".to_string()
    } else {
        label.to_string()
    };
    if let Some(pid) = first_string(entry, "_PID") {
        label.push('[');
        label.push_str(&pid);
        label.push(']');
    } else if let Some(pid) = first_string(entry, "SYSLOG_PID") {
        label.push('[');
        label.push_str(&pid);
        label.push(']');
    }
    parts.push(label);
    parts.join(" ")
}

fn unit_label(entry: &Entry) -> Option<String> {
    for name in [
        "_SYSTEMD_UNIT",
        "_SYSTEMD_USER_UNIT",
        "UNIT",
        "USER_UNIT",
        "OBJECT_SYSTEMD_UNIT",
        "OBJECT_SYSTEMD_USER_UNIT",
    ] {
        if let Some(value) = first_string(entry, name) {
            return Some(value);
        }
    }
    None
}

fn display_message(entry: &Entry, options: &OutputOptions, prefix_columns: usize) -> Vec<u8> {
    values_for_name(entry, "MESSAGE")
        .first()
        .map(|value| {
            let value = if options.truncate_newline {
                truncate_at_newline(value)
            } else {
                *value
            };
            if !options.show_all && !journal_text_printable(value) {
                return blob_data(value.len()).into_bytes();
            }
            let mut out = indent_continuation_lines(c_string_bytes(value), prefix_columns);
            if !options.show_all && !options.full_width {
                out = ellipsize_line(&out, prefix_columns);
            }
            out
        })
        .unwrap_or_default()
}

fn indent_continuation_lines(value: &[u8], prefix_columns: usize) -> Vec<u8> {
    let newline_count = value.iter().filter(|byte| **byte == b'\n').count();
    if newline_count == 0 {
        return value.to_vec();
    }
    let mut out = Vec::with_capacity(value.len() + newline_count * prefix_columns);
    for (idx, byte) in value.iter().enumerate() {
        out.push(*byte);
        if *byte == b'\n' && idx + 1 < value.len() {
            out.extend(std::iter::repeat(b' ').take(prefix_columns));
        }
    }
    out
}

fn first_string(entry: &Entry, name: &str) -> Option<String> {
    values_for_name(entry, name)
        .first()
        .map(|value| String::from_utf8_lossy(value).into_owned())
}

fn values_for_name<'a>(entry: &'a Entry, name: &str) -> Vec<&'a [u8]> {
    if let Some(values) = entry.field_values.get(name) {
        if !values.is_empty() {
            return values.iter().map(Vec::as_slice).collect();
        }
    }
    entry
        .fields
        .get(name)
        .map(|value| vec![value.as_slice()])
        .unwrap_or_default()
}

fn display_value(value: &[u8], truncate_newline: bool) -> Vec<u8> {
    if !truncate_newline {
        return value.to_vec();
    }
    truncate_at_newline(value).to_vec()
}

fn display_verbose_value(name: &str, value: &[u8], options: &OutputOptions) -> Vec<u8> {
    if options.show_all {
        return c_string_bytes(value).to_vec();
    }
    if !journal_text_printable(value)
        || (!options.full_width && name.len() + 1 + value.len() >= PRINT_CHAR_THRESHOLD)
    {
        return blob_data(value.len()).into_bytes();
    }
    value.to_vec()
}

fn truncate_at_newline(value: &[u8]) -> &[u8] {
    let end = value
        .iter()
        .position(|byte| *byte == b'\n')
        .unwrap_or(value.len());
    &value[..end]
}

fn c_string_bytes(value: &[u8]) -> &[u8] {
    let end = value
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(value.len());
    &value[..end]
}

fn ellipsize_line(value: &[u8], prefix_columns: usize) -> Vec<u8> {
    let limit = DEFAULT_COLUMNS.saturating_sub(prefix_columns + 1);
    if value.len() <= limit {
        return value.to_vec();
    }
    let mut end = limit;
    if let Ok(text) = std::str::from_utf8(value) {
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
    }
    let mut out = value[..end].to_vec();
    out.extend_from_slice("…".as_bytes());
    out
}

fn journal_text_printable(value: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(value) else {
        return false;
    };
    text.chars().all(|ch| {
        let cp = ch as u32;
        (cp >= 0x20 || ch == '\t' || ch == '\n') && !(0x7f..=0x9f).contains(&cp)
    })
}

fn journal_export_text_printable(value: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(value) else {
        return false;
    };
    text.chars().all(|ch| {
        let cp = ch as u32;
        (cp >= 0x20 || ch == '\t') && !(0x7f..=0x9f).contains(&cp)
    })
}

fn blob_data(size: usize) -> String {
    format!("[{} blob data]", format_journal_bytes(size as u64))
}

fn metadata_fields(entry: &Entry) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    if !entry.cursor.is_empty() {
        out.push(("__CURSOR".to_string(), entry.cursor.as_bytes().to_vec()));
    }
    out.push((
        "__REALTIME_TIMESTAMP".to_string(),
        entry.realtime.to_string().into_bytes(),
    ));
    out.push((
        "__MONOTONIC_TIMESTAMP".to_string(),
        entry.monotonic.to_string().into_bytes(),
    ));
    out.push((
        "__SEQNUM".to_string(),
        entry.seqnum.to_string().into_bytes(),
    ));
    if let Some(seqnum_id) = cursor_component(&entry.cursor, "s") {
        out.push(("__SEQNUM_ID".to_string(), seqnum_id.into_bytes()));
    }
    out.push((
        "_BOOT_ID".to_string(),
        hex::encode(entry.boot_id).into_bytes(),
    ));
    out
}

fn cursor_component(cursor: &str, name: &str) -> Option<String> {
    cursor.split(';').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key == name).then(|| value.to_string())
    })
}

fn verbose_fields<'a>(entry: &'a Entry, options: &OutputOptions) -> Vec<(String, &'a [u8])> {
    if options.output_fields_set {
        return selected_entry_fields(entry, &options.output_fields);
    }
    entry
        .raw_fields()
        .map(|field| {
            (
                String::from_utf8_lossy(field.name).into_owned(),
                field.value,
            )
        })
        .collect()
}

fn entry_fields<'a>(entry: &'a Entry, options: &OutputOptions) -> Vec<(String, &'a [u8])> {
    if options.output_fields_set {
        return selected_entry_fields(entry, &options.output_fields)
            .into_iter()
            .filter(|(name, _)| !is_metadata_field(name))
            .collect();
    }
    entry
        .raw_fields()
        .filter(|field| field.name != b"_BOOT_ID")
        .map(|field| {
            (
                String::from_utf8_lossy(field.name).into_owned(),
                field.value,
            )
        })
        .collect()
}

fn selected_entry_fields<'a>(entry: &'a Entry, names: &[String]) -> Vec<(String, &'a [u8])> {
    let mut out = Vec::new();
    for name in names {
        for value in values_for_name(entry, name) {
            out.push((name.clone(), value));
        }
    }
    out
}

fn is_metadata_field(name: &str) -> bool {
    matches!(
        name,
        "__CURSOR"
            | "__REALTIME_TIMESTAMP"
            | "__MONOTONIC_TIMESTAMP"
            | "__SEQNUM"
            | "__SEQNUM_ID"
            | "_BOOT_ID"
    )
}

fn json_object(entry: &Entry, options: &OutputOptions) -> Map<String, Value> {
    let mut map = Map::new();
    for (name, value) in metadata_fields(entry) {
        add_json_value(&mut map, name, &value, options.show_all);
    }
    for (name, value) in entry_fields(entry, options) {
        add_json_value(&mut map, name, value, options.show_all);
    }
    map
}

fn add_json_value(map: &mut Map<String, Value>, name: String, value: &[u8], show_all: bool) {
    let value = json_value_for_bytes(&name, value, show_all);
    match map.remove(&name) {
        Some(Value::Array(mut values)) => {
            values.push(value);
            map.insert(name, Value::Array(values));
        }
        Some(existing) => {
            map.insert(name, Value::Array(vec![existing, value]));
        }
        None => {
            map.insert(name, value);
        }
    }
}

fn json_value_for_bytes(name: &str, value: &[u8], show_all: bool) -> Value {
    if !show_all && name.len() + 1 + value.len() >= JSON_THRESHOLD {
        return Value::Null;
    }
    if let Ok(text) = std::str::from_utf8(value) {
        if text.chars().all(|ch| {
            let cp = ch as u32;
            (cp >= 0x20 || ch == '\t' || ch == '\n') && !(0x7f..=0x9f).contains(&cp)
        }) {
            return Value::String(text.to_string());
        }
    }
    Value::Array(
        value
            .iter()
            .map(|byte| Value::Number((*byte).into()))
            .collect(),
    )
}

fn write_export_field(out: &mut Vec<u8>, name: &[u8], value: &[u8]) {
    let mut text = Vec::with_capacity(name.len() + 1 + value.len());
    text.extend_from_slice(name);
    text.push(b'=');
    text.extend_from_slice(value);
    if journal_export_text_printable(&text) {
        out.extend_from_slice(&text);
        out.push(b'\n');
        return;
    }
    out.extend_from_slice(name);
    out.push(b'\n');
    out.extend_from_slice(&(value.len() as u64).to_le_bytes());
    out.extend_from_slice(value);
    out.push(b'\n');
}

fn write_systemd_pretty_json(out: &mut Vec<u8>, value: &Value) -> Result<()> {
    write_systemd_pretty_json_at(out, value, 0)
}

fn write_systemd_pretty_json_at(out: &mut Vec<u8>, value: &Value, depth: usize) -> Result<()> {
    match value {
        Value::Object(map) => {
            out.push(b'{');
            if !map.is_empty() {
                out.push(b'\n');
            }
            for (idx, (key, value)) in map.iter().enumerate() {
                write_json_tabs(out, depth + 1);
                serde_json::to_writer(&mut *out, key)
                    .map_err(|err| anyhow!("json output: {err}"))?;
                out.extend_from_slice(b" : ");
                write_systemd_pretty_json_at(out, value, depth + 1)?;
                if idx + 1 != map.len() {
                    out.push(b',');
                }
                out.push(b'\n');
            }
            if !map.is_empty() {
                write_json_tabs(out, depth);
            }
            out.push(b'}');
        }
        Value::Array(values) => {
            out.push(b'[');
            if !values.is_empty() {
                out.push(b'\n');
            }
            for (idx, value) in values.iter().enumerate() {
                write_json_tabs(out, depth + 1);
                write_systemd_pretty_json_at(out, value, depth + 1)?;
                if idx + 1 != values.len() {
                    out.push(b',');
                }
                out.push(b'\n');
            }
            if !values.is_empty() {
                write_json_tabs(out, depth);
            }
            out.push(b']');
        }
        _ => {
            serde_json::to_writer(&mut *out, value).map_err(|err| anyhow!("json output: {err}"))?;
        }
    }
    Ok(())
}

fn write_json_tabs(out: &mut Vec<u8>, depth: usize) {
    out.extend(std::iter::repeat(b'\t').take(depth));
}
