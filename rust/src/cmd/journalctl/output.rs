use super::colors::{self, Colors, MatchRange};
use super::{Args, OutputModeArg, format_journal_bytes};
use anyhow::{Result, anyhow};
use journal::Entry;
use serde_json::{Map, Value};
use std::borrow::Cow;
use std::sync::OnceLock;

#[derive(Clone)]
pub(crate) struct OutputOptions {
    mode: OutputModeArg,
    output_fields: Vec<String>,
    output_fields_set: bool,
    invalid_cat_field: Option<Vec<u8>>,
    utc: bool,
    no_hostname: bool,
    full_width: bool,
    show_all: bool,
    truncate_newline: bool,
    merge: bool,
    colors: Colors,
    urlify: bool,
}

const PRINT_CHAR_THRESHOLD: usize = 300;
const JSON_THRESHOLD: usize = 4096;
const DEFAULT_COLUMNS: usize = 80;

pub(super) fn valid_field_name(name: &str) -> bool {
    valid_field_bytes(name.as_bytes())
}

fn valid_field_bytes(name: &[u8]) -> bool {
    !name.is_empty()
        && !name.starts_with(b"__")
        && name
            .iter()
            .all(|&b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

impl OutputOptions {
    pub(crate) fn from_args(args: &Args, full_width: bool) -> Self {
        let (output_fields, output_fields_set, invalid_cat_field) =
            parse_output_fields(&args.output_fields);
        let colors = Colors::detect();
        Self {
            mode: args.output.clone(),
            output_fields,
            output_fields_set,
            invalid_cat_field,
            utc: args.utc,
            no_hostname: args.no_hostname,
            full_width,
            show_all: args.all,
            truncate_newline: args.truncate_newline,
            merge: args.merge,
            colors,
            urlify: super::output_links::enabled(colors),
        }
    }

    pub(crate) fn mode(&self) -> OutputModeArg {
        self.mode
    }

    pub(crate) fn suppress_boot_separators(&self) -> bool {
        self.merge
    }

    pub(super) fn colors(&self) -> Colors {
        self.colors
    }

    pub(super) fn urlify(&self) -> bool {
        self.urlify
    }
}

pub(crate) struct OutputRenderer {
    options: OutputOptions,
    previous_delta: Option<(u64, u64, [u8; 16])>,
    columns: usize,
    highlight: MatchRange,
}

impl OutputRenderer {
    pub(crate) fn new(options: OutputOptions) -> Self {
        let columns = if !options.full_width
            && !options.show_all
            && output_skips_missing_message(options.mode)
        {
            std::env::var("COLUMNS")
                .ok()
                .and_then(|value| super::parse_native_u64(&value))
                .filter(|n| (1..=65535).contains(n))
                .map(|n| n as usize)
                .or_else(super::terminal::stdout_columns)
                .unwrap_or(DEFAULT_COLUMNS)
        } else {
            DEFAULT_COLUMNS
        };
        Self {
            options,
            previous_delta: None,
            columns,
            highlight: MatchRange::default(),
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
            OutputModeArg::Cat => self.render_cat(entry),
            OutputModeArg::Verbose => self.render_verbose(entry),
            OutputModeArg::Export => Ok(self.render_export(entry)),
            OutputModeArg::Json => self.render_json(entry, JsonFrame::Line),
            OutputModeArg::JsonPretty => self.render_json(entry, JsonFrame::Pretty),
            OutputModeArg::JsonSse => self.render_json(entry, JsonFrame::Sse),
            OutputModeArg::JsonSeq => self.render_json(entry, JsonFrame::Seq),
            OutputModeArg::Help => Ok(Vec::new()),
        }
    }

    pub(super) fn set_highlight(&mut self, highlight: MatchRange) {
        self.highlight = highlight;
    }

    pub(super) fn colors(&self) -> Colors {
        self.options.colors
    }

    pub(crate) fn skips_entry(&self, entry: &Entry) -> bool {
        output_skips_missing_message(self.options.mode)
            && values_for_name(entry, "MESSAGE").is_empty()
    }

    fn render_short(&mut self, entry: &Entry, timestamp_mode: TimestampMode) -> Result<Vec<u8>> {
        let timestamp = format_timestamp(entry, timestamp_mode, self.options.utc)?;
        let (header, columns) = short_header(entry, &self.options, false);
        let prefix_columns = timestamp.len() + columns + 2;
        let mut out = timestamp.into_bytes();
        out.extend_from_slice(&header);
        out.extend_from_slice(b": ");
        out.extend_from_slice(&display_message(
            entry,
            &self.options,
            prefix_columns,
            self.columns,
            self.highlight,
        ));
        out.push(b'\n');
        Ok(out)
    }

    fn render_with_unit(&mut self, entry: &Entry) -> Result<Vec<u8>> {
        let timestamp = format_timestamp(entry, TimestampMode::ShortFull, self.options.utc)?;
        let (header, columns) = short_header(entry, &self.options, true);
        let prefix_columns = timestamp.len() + columns + 2;
        let mut out = timestamp.into_bytes();
        out.extend_from_slice(&header);
        out.extend_from_slice(b": ");
        out.extend_from_slice(&display_message(
            entry,
            &self.options,
            prefix_columns,
            self.columns,
            self.highlight,
        ));
        out.push(b'\n');
        Ok(out)
    }

    fn render_short_delta(&mut self, entry: &Entry) -> Result<Vec<u8>> {
        let current_realtime = display_realtime_usec(entry);
        let current_monotonic = display_monotonic_usec(entry);
        let current = (current_realtime, current_monotonic, entry.boot_id);
        let monotonic = format_monotonic(current_monotonic);
        let delta = match self.previous_delta {
            Some((previous_realtime, previous_monotonic, previous_boot)) => {
                let (diff, marker) = if previous_boot == entry.boot_id {
                    (current_monotonic.saturating_sub(previous_monotonic), " ")
                } else {
                    (current_realtime.saturating_sub(previous_realtime), "*")
                };
                format!(" <{}{}>", format_monotonic(diff), marker)
            }
            None => "                ".to_string(),
        };
        self.previous_delta = Some(current);
        let timestamp = format!("[{monotonic}{delta}]");
        let (header, columns) = short_header(entry, &self.options, false);
        let prefix_columns = timestamp.len() + columns + 2;
        let mut out = timestamp.into_bytes();
        out.extend_from_slice(&header);
        out.extend_from_slice(b": ");
        out.extend_from_slice(&display_message(
            entry,
            &self.options,
            prefix_columns,
            self.columns,
            self.highlight,
        ));
        out.push(b'\n');
        Ok(out)
    }

    fn render_cat(&mut self, entry: &Entry) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        if let Some(name) = &self.options.invalid_cat_field {
            return Err(anyhow!(
                "Invalid field name: {}",
                String::from_utf8_lossy(name)
            ));
        }
        if self.options.output_fields_set {
            for name in &self.options.output_fields {
                if let Some(value) = values_for_name(entry, name).first() {
                    if self.options.colors.enabled() {
                        self.append_colored_cat_field(&mut out, entry, name, value);
                    } else {
                        out.extend_from_slice(value);
                    }
                    out.push(b'\n');
                }
            }
        } else if let Some(value) = values_for_name(entry, "MESSAGE").first() {
            if self.options.colors.enabled() {
                self.append_colored_cat_field(&mut out, entry, "MESSAGE", value);
            } else {
                out.extend_from_slice(value);
            }
            out.push(b'\n');
        }
        Ok(out)
    }

    fn append_colored_cat_field(&self, out: &mut Vec<u8>, entry: &Entry, name: &str, value: &[u8]) {
        let (on, off, highlight) = self.options.colors.log(entry_priority(entry, true), false);
        out.extend_from_slice(on.as_bytes());
        if name == "MESSAGE" {
            let span = self.highlight;
            out.extend_from_slice(&value[..span.start]);
            out.extend_from_slice(highlight.as_bytes());
            out.extend_from_slice(&value[span.start..span.end]);
            out.extend_from_slice(on.as_bytes());
            out.extend_from_slice(&value[span.end..]);
        } else {
            out.extend_from_slice(value);
        }
        out.extend_from_slice(off.as_bytes());
    }

    fn render_verbose(&self, entry: &Entry) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        if self.options.colors.enabled() {
            let underline = if self.options.colors.underline {
                "\x1b[0;4m"
            } else {
                ""
            };
            let timestamp = format_timestamp(entry, TimestampMode::Verbose, self.options.utc)?;
            let grey = self.options.colors.grey();
            out.extend_from_slice(
                format!(
                    "{underline}{timestamp}{} {grey}[{}]{grey}\n",
                    colors::NORMAL,
                    entry.cursor
                )
                .as_bytes(),
            );
        } else {
            out.extend_from_slice(
                format!(
                    "{} [{}]\n",
                    format_timestamp(entry, TimestampMode::Verbose, self.options.utc)?,
                    entry.cursor
                )
                .as_bytes(),
            );
        }
        for (name, value) in verbose_fields(entry, &self.options) {
            let (on, off) = if !self.options.colors.enabled() {
                ("", "")
            } else if name == "MESSAGE" {
                (colors::HIGHLIGHT, colors::NORMAL)
            } else if name.starts_with('_') {
                (colors::GREEN, colors::NORMAL)
            } else {
                ("", "")
            };
            out.extend_from_slice(format!("    {on}{name}=").as_bytes());
            let mut displayed = display_verbose_value(&name, value, &self.options);
            if self.options.colors.enabled()
                && name == "CONFIG_FILE"
                && (self.options.show_all
                    || (self.options.full_width
                        || name.len() + value.len() + 1 < PRINT_CHAR_THRESHOLD)
                        && journal_text_printable(value))
            {
                if let Some(linked) = super::output_links::file(value, self.options.urlify) {
                    displayed = full_message_with_nul(&linked, 4 + name.len() + 1);
                }
            }
            out.extend_from_slice(&displayed);
            let printable = off.is_empty()
                || self.options.show_all
                || (self.options.full_width || name.len() + value.len() + 1 < PRINT_CHAR_THRESHOLD)
                    && journal_text_printable(value);
            if !printable {
                out.extend_from_slice(off.as_bytes());
            }
            out.push(b'\n');
            if printable {
                out.extend_from_slice(off.as_bytes());
            }
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
        if self.options.colors.enabled() {
            let start = match frame {
                JsonFrame::Sse => 6,
                JsonFrame::Seq => 1,
                _ => 0,
            };
            let mut colored = out[..start].to_vec();
            colored.extend_from_slice(&colors::json(&out[start..]));
            return Ok(colored);
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

#[derive(Clone, Copy)]
enum JsonFrame {
    Line,
    Pretty,
    Sse,
    Seq,
}

fn parse_output_fields(values: &[std::ffi::OsString]) -> (Vec<String>, bool, Option<Vec<u8>>) {
    let mut names = Vec::new();
    let mut set = false;
    let mut invalid_cat = None;
    for value in values {
        for field in value
            .as_encoded_bytes()
            .split(|&b| b == b',')
            .filter(|field| !field.is_empty())
        {
            set = true;
            if invalid_cat.is_none() && !valid_field_bytes(field) {
                invalid_cat = Some(field.to_vec());
            }
            if let Ok(name) = std::str::from_utf8(field) {
                if !names.iter().any(|existing| existing == name) {
                    names.push(name.to_string());
                }
            }
        }
    }
    (names, set, invalid_cat)
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
            "{:10}.{:06}",
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
        TimestampMode::ShortIso => "%Y-%m-%dT%H:%M:%S",
        TimestampMode::ShortIsoPrecise => "%Y-%m-%dT%H:%M:%S%.6f",
        TimestampMode::ShortPrecise => "%b %d %H:%M:%S%.6f",
        TimestampMode::Verbose => "%a %Y-%m-%d %H:%M:%S%.6f",
        TimestampMode::ShortMonotonic | TimestampMode::ShortUnix => unreachable!(),
    };

    rendering_zone().format_display(
        secs,
        nanos,
        utc,
        pattern,
        needs_zone_name,
        matches!(
            mode,
            TimestampMode::ShortIso | TimestampMode::ShortIsoPrecise
        ),
    )
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
    last_value_for_name(entry, "_SOURCE_REALTIME_TIMESTAMP")
        .and_then(|value| std::str::from_utf8(c_string_bytes(value)).ok())
        .and_then(super::parse_native_u64)
        .filter(|value| *value > 0 && *value < 1 << 55)
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
    rendering_zone().format_display(secs, nanos, false, "%a %Y-%m-%d %H:%M:%S", true, false)
}

fn format_monotonic(usec: u64) -> String {
    format!("{:>5}.{:06}", usec / 1_000_000, usec % 1_000_000)
}

fn rendering_zone() -> &'static super::timestamp_zone::Zone {
    static ZONE: OnceLock<super::timestamp_zone::Zone> = OnceLock::new();
    ZONE.get_or_init(super::timestamp_zone::Zone::local)
}

pub(super) fn last_value_for_name<'a>(entry: &'a Entry, name: &str) -> Option<&'a [u8]> {
    entry
        .field_values
        .get(name)
        .and_then(|values| values.last())
        .or_else(|| entry.fields.get(name))
        .map(Vec::as_slice)
}

fn short_field<'a>(entry: &'a Entry, name: &str, options: &OutputOptions) -> Option<&'a [u8]> {
    last_value_for_name(entry, name).filter(|value| short_field_printable(value, options))
}

fn short_field_printable(value: &[u8], options: &OutputOptions) -> bool {
    options.show_all || value.len() < PRINT_CHAR_THRESHOLD && journal_text_printable(value)
}

fn short_header(entry: &Entry, options: &OutputOptions, with_unit: bool) -> (Vec<u8>, usize) {
    let mut out = Vec::new();
    let mut columns = 0;
    if !options.no_hostname {
        if let Some(value) = short_field(entry, "_HOSTNAME", options) {
            out.push(b' ');
            out.extend_from_slice(c_string_bytes(value));
            columns += value.len() + 1;
        }
    }
    let mut unit_shown = false;
    if with_unit {
        let unit = last_value_for_name(entry, "_SYSTEMD_UNIT");
        let user_unit = last_value_for_name(entry, "_SYSTEMD_USER_UNIT");
        if unit.is_some_and(|v| short_field_printable(v, options))
            || user_unit.is_some_and(|v| short_field_printable(v, options))
        {
            if let Some(value) = unit {
                out.push(b' ');
                out.extend_from_slice(c_string_bytes(value));
                columns += value.len() + 1;
            }
            if let Some(value) = user_unit {
                out.push(if unit.is_some() { b'/' } else { b' ' });
                out.extend_from_slice(c_string_bytes(value));
                // v262 accounts the system-unit length here as well.
                columns += unit.map_or(0, |v| v.len()) + 1;
            }
            unit_shown = true;
        }
    }
    if !unit_shown {
        if let Some(value) = short_field(entry, "SYSLOG_IDENTIFIER", options)
            .or_else(|| short_field(entry, "_COMM", options))
        {
            out.push(b' ');
            out.extend_from_slice(c_string_bytes(value));
            columns += value.len() + 1;
        } else {
            out.extend_from_slice(b" unknown");
        }
    }
    if let Some(value) =
        short_field(entry, "_PID", options).or_else(|| short_field(entry, "SYSLOG_PID", options))
    {
        out.push(b'[');
        out.extend_from_slice(c_string_bytes(value));
        out.push(b']');
        columns += value.len() + 2;
    }
    (out, columns)
}

fn display_message(
    entry: &Entry,
    options: &OutputOptions,
    prefix_columns: usize,
    columns: usize,
    highlight: MatchRange,
) -> Vec<u8> {
    if options.colors.enabled() || options.urlify {
        return display_colored_message(entry, options, prefix_columns, columns, highlight);
    }
    last_value_for_name(entry, "MESSAGE")
        .map(|value| {
            let (prepared, printable) = if options.show_all {
                (Cow::Borrowed(value), true)
            } else {
                prepare_short_message(value, options.truncate_newline)
            };
            let value = if options.show_all && options.truncate_newline {
                truncate_at_newline(&prepared)
            } else {
                &prepared
            };
            if !printable {
                return blob_data(value.len()).into_bytes();
            }
            if options.show_all && value.contains(&0) {
                return full_message_with_nul(value, prefix_columns);
            }
            if !options.show_all && !options.full_width {
                return shortened_message(value, prefix_columns, columns);
            }
            let mut out = indent_continuation_lines(value, prefix_columns);
            if out.last() == Some(&b'\n') {
                out.pop();
            }
            out
        })
        .unwrap_or_default()
}

fn entry_priority(entry: &Entry, first: bool) -> u8 {
    let value = if first {
        values_for_name(entry, "PRIORITY").first().copied()
    } else {
        last_value_for_name(entry, "PRIORITY")
    };
    match value {
        Some([byte @ b'0'..=b'7']) => byte - b'0',
        _ => 6,
    }
}

fn advance_highlight(original: MatchRange, shift: &mut MatchRange, at: usize, size: usize) {
    if at < original.start {
        shift.start += size;
    }
    if at < original.end {
        shift.end += size;
    }
}

fn display_colored_message(
    entry: &Entry,
    options: &OutputOptions,
    prefix: usize,
    columns: usize,
    mut span: MatchRange,
) -> Vec<u8> {
    let Some(value) = last_value_for_name(entry, "MESSAGE") else {
        return Vec::new();
    };
    let prepared = if !options.show_all {
        match value.iter().position(|b| b"\t\r\x1b".contains(b)) {
            Some(start) => Cow::Owned(strip_short_controls_with_highlight(
                value,
                start,
                Some(&mut span),
            )),
            None => Cow::Borrowed(value),
        }
    } else {
        Cow::Borrowed(value)
    };
    let value = if options.truncate_newline {
        truncate_at_newline(&prepared)
    } else {
        &prepared
    };
    let mut out = if options.urlify {
        short_field(entry, "DOCUMENTATION", options)
            .map(|url| super::output_links::documentation(url, ellipsis_marker() == "…"))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    if !options.show_all && !journal_text_printable(value) {
        out.extend_from_slice(blob_data(value.len()).as_bytes());
        return out;
    }
    let mut message = Cow::Borrowed(value);
    if let Some(config) = last_value_for_name(entry, "CONFIG_FILE") {
        if value.starts_with(config)
            && (value.len() == config.len() || matches!(value.get(config.len()), Some(b':' | b' ')))
            && (span.start == 0 || span.start > config.len())
        {
            if let Some(mut linked) = super::output_links::file(config, options.urlify)
                .filter(|linked| linked.len() >= config.len())
            {
                let shift = linked.len() - config.len();
                linked.extend_from_slice(&value[config.len()..]);
                message = Cow::Owned(linked);
                span.start += shift;
                span.end += shift;
            }
        }
    }
    let value = message.as_ref();
    let audit =
        last_value_for_name(entry, "_TRANSPORT").is_some_and(|v| c_string_bytes(v) == b"audit");
    let (on, off, highlight) = options.colors.log(entry_priority(entry, false), audit);
    let (mut start, mut line) = (0, 0);
    while start < value.len() {
        let end = value[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(value.len(), |n| start + n);
        let part = &value[start..end];
        let tail = line == 2 || end + 1 >= PRINT_CHAR_THRESHOLD;
        let shorten = !options.show_all
            && !options.full_width
            && (prefix + part.len() + 1 >= columns || tail);
        if line > 0 {
            out.push(b'\n');
        }
        if shorten && columns.saturating_sub(prefix) < 3 {
            out.extend_from_slice(b"...");
        } else {
            if line > 0 {
                out.extend(std::iter::repeat_n(b' ', prefix));
            }
            out.extend_from_slice(on.as_bytes());
            if !shorten {
                if start <= span.start && span.start < part.len() {
                    out.extend_from_slice(c_string_bytes(&part[..span.start]));
                    out.extend_from_slice(highlight.as_bytes());
                    out.extend_from_slice(c_string_bytes(
                        &part[span.start..span.end.min(part.len())],
                    ));
                    if part.len() > span.end {
                        out.extend_from_slice(on.as_bytes());
                        out.extend_from_slice(c_string_bytes(&part[span.end..]));
                    }
                } else {
                    out.extend_from_slice(c_string_bytes(part));
                }
            } else if columns - prefix > part.len() + 3 {
                out.extend_from_slice(c_string_bytes(part));
                out.extend_from_slice(b"...");
            } else {
                let percent = if tail { 100 } else { 90 };
                out.extend_from_slice(c_string_bytes(&ellipsize_line(
                    part,
                    columns - prefix,
                    percent,
                )));
            }
            out.extend_from_slice(off.as_bytes());
        }
        if shorten && tail {
            break;
        }
        start = end + 1;
        line += 1;
    }
    out
}

fn prepare_short_message(value: &[u8], truncate: bool) -> (Cow<'_, [u8]>, bool) {
    if let Ok(text) = std::str::from_utf8(value) {
        let mut printable = true;
        for (index, ch) in text.char_indices() {
            if matches!(ch, '\x1b' | '\t' | '\r') {
                let mut out = strip_short_controls(value, index);
                if truncate {
                    out.truncate(truncate_at_newline(&out).len());
                }
                let printable = journal_text_printable(&out);
                return (Cow::Owned(out), printable);
            }
            if truncate && matches!(ch, '\n' | '\0') {
                return (Cow::Borrowed(&value[..index]), printable);
            }
            let cp = ch as u32;
            printable &= (cp >= 0x20 || ch == '\n') && !(0x7f..=0x9f).contains(&cp);
        }
        return (Cow::Borrowed(value), printable);
    }
    let mut out = match value
        .iter()
        .position(|b| matches!(b, b'\x1b' | b'\t' | b'\r'))
    {
        Some(index) => Cow::Owned(strip_short_controls(value, index)),
        None => Cow::Borrowed(value),
    };
    if truncate {
        let len = truncate_at_newline(&out).len();
        match &mut out {
            Cow::Owned(value) => value.truncate(len),
            Cow::Borrowed(value) => *value = &value[..len],
        }
    }
    let printable = journal_text_printable(&out);
    (out, printable)
}

fn strip_short_controls(value: &[u8], start: usize) -> Vec<u8> {
    strip_short_controls_with_highlight(value, start, None)
}

fn strip_short_controls_with_highlight(
    value: &[u8],
    start: usize,
    highlight: Option<&mut MatchRange>,
) -> Vec<u8> {
    let original = highlight.as_deref().copied().unwrap_or_default();
    let mut shift = MatchRange::default();
    let mut out = Vec::with_capacity(value.len());
    out.extend_from_slice(&value[..start]);
    let mut i = start;
    while i < value.len() {
        match value[i] {
            b'\t' => {
                out.extend_from_slice(b"        ");
                advance_highlight(original, &mut shift, i, 7);
                i += 1;
            }
            b'\r' => {
                let mut end = i + 1;
                while end < value.len() && value[end] == b'\r' {
                    end += 1;
                }
                if end < value.len() && value[end] != b'\n' {
                    out.extend_from_slice(&value[i..end]);
                }
                i = end;
            }
            b'\x1b' if i + 1 < value.len() => {
                let mut end = i + 2;
                let mut complete = false;
                let (mut invalid_at, mut size) = (i + 1, 1);
                match value[i + 1] {
                    b'[' => {
                        size = 2;
                        while end < value.len() {
                            let b = value[end];
                            end += 1;
                            if b == b'm' {
                                complete = true;
                                break;
                            }
                            if b != 0 && b != b';' && b != b':' && !b.is_ascii_digit() {
                                invalid_at = end - 1;
                                break;
                            }
                            invalid_at = end;
                        }
                    }
                    b']' => {
                        size = 2;
                        while end < value.len() {
                            let b = value[end];
                            end += 1;
                            if b == 7 {
                                complete = true;
                                break;
                            }
                            if b == 27 {
                                invalid_at = end;
                                if value.get(end) == Some(&b'\\') {
                                    end += 1;
                                    complete = true;
                                }
                                break;
                            }
                            if !(32..127).contains(&b) {
                                invalid_at = end - 1;
                                break;
                            }
                            invalid_at = end;
                        }
                    }
                    _ => {}
                }
                if complete {
                    i = end;
                } else {
                    // Native preserves malformed sequences and reprocesses their body.
                    advance_highlight(original, &mut shift, invalid_at, size);
                    out.extend_from_slice(&value[i..i + 2]);
                    i += 2;
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    if let Some(highlight) = highlight {
        highlight.start += shift.start;
        highlight.end += shift.end;
    }
    out
}

fn full_message_with_nul(value: &[u8], prefix_columns: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for (index, line) in value
        .strip_suffix(b"\n")
        .unwrap_or(value)
        .split(|&b| b == b'\n')
        .enumerate()
    {
        if index > 0 {
            out.push(b'\n');
            out.extend(std::iter::repeat(b' ').take(prefix_columns));
        }
        out.extend_from_slice(c_string_bytes(line));
    }
    out
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

fn display_verbose_value(name: &str, value: &[u8], options: &OutputOptions) -> Vec<u8> {
    if !options.show_all
        && (!journal_text_printable(value)
            || (!options.full_width && name.len() + 1 + value.len() >= PRINT_CHAR_THRESHOLD))
    {
        return blob_data(value.len()).into_bytes();
    }
    if value.contains(&b'\n') || value.contains(&0) {
        return full_message_with_nul(value, 4 + name.len() + 1);
    }
    value.to_vec()
}

fn truncate_at_newline(value: &[u8]) -> &[u8] {
    let end = value
        .iter()
        .position(|byte| matches!(byte, b'\n' | b'\r' | 0))
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

fn ellipsis_marker() -> &'static str {
    static MARKER: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    MARKER.get_or_init(|| {
        let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
            .iter()
            .find_map(|name| std::env::var(name).ok().filter(|s| !s.is_empty()))
            .unwrap_or_default();
        if locale.to_ascii_lowercase().contains("utf") {
            "…"
        } else {
            "..."
        }
    })
}

fn shortened_message(value: &[u8], prefix: usize, columns: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut line = 0;
    while start < value.len() {
        let end = value[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|n| start + n)
            .unwrap_or(value.len());
        let part = &value[start..end];
        let tail = line == 2 || end + 1 >= PRINT_CHAR_THRESHOLD;
        let shorten = prefix + part.len() + 1 >= columns || tail;
        if line > 0 {
            out.push(b'\n');
        }
        if shorten && columns.saturating_sub(prefix) < 3 {
            out.extend_from_slice(b"...");
        } else {
            if line > 0 {
                out.extend(std::iter::repeat(b' ').take(prefix));
            }
            if !shorten {
                out.extend_from_slice(part);
            } else if columns - prefix > part.len() + 3 {
                out.extend_from_slice(part);
                out.extend_from_slice(b"...");
            } else {
                out.extend_from_slice(&ellipsize_line(
                    part,
                    columns - prefix,
                    if tail { 100 } else { 90 },
                ));
            }
        }
        if tail {
            break;
        }
        start = end + 1;
        line += 1;
    }
    out
}

fn ellipsize_line(value: &[u8], columns: usize, percent: usize) -> Vec<u8> {
    if value.is_ascii() {
        if value.len() <= columns {
            return value.to_vec();
        }
        let marker = ellipsis_marker();
        let marker_width = if marker == "..." { 3 } else { 1 };
        let left = ((columns - marker_width) * percent + 50) / 100;
        let right = columns - marker_width - left;
        let mut out = value[..left].to_vec();
        out.extend_from_slice(marker.as_bytes());
        out.extend_from_slice(&value[value.len() - right..]);
        return out;
    }
    let text = std::str::from_utf8(value).expect("printable message is UTF8");
    let (mut left, mut right, mut used) = (0, text.len(), 0);
    let budget = (columns - 1) * percent / 100;
    for ch in text.chars() {
        let width = journal_char_width(ch);
        if used + width > budget {
            break;
        }
        used += width;
        left += ch.len_utf8();
    }
    for ch in text[left..].chars().rev() {
        if used >= columns {
            break;
        }
        let width = journal_char_width(ch);
        if used + width > columns {
            break;
        }
        used += width;
        right -= ch.len_utf8();
    }
    if left >= right {
        return value.to_vec();
    }
    if used >= columns {
        if right < text.len() {
            right += text[right..].chars().next().unwrap().len_utf8();
        } else if left > 0 {
            left -= text[..left].chars().next_back().unwrap().len_utf8();
        }
    }
    let mut out = value[..left].to_vec();
    out.extend_from_slice("…".as_bytes());
    out.extend_from_slice(&value[right..]);
    out
}

// v262 fixes character width to these intervals.
fn journal_char_width(ch: char) -> usize {
    if matches!(ch as u32,
        0x1100..=0x115F |
        0x2329..=0x232A |
        0x2E80..=0x2E99 |
        0x2E9B..=0x2EF3 |
        0x2F00..=0x2FD5 |
        0x2FF0..=0x2FFB |
        0x3000..=0x303E |
        0x3041..=0x3096 |
        0x3099..=0x30FF |
        0x3105..=0x312D |
        0x3131..=0x318E |
        0x3190..=0x31BA |
        0x31C0..=0x31E3 |
        0x31F0..=0x321E |
        0x3220..=0x3247 |
        0x3250..=0x32FE |
        0x3300..=0x4DBF |
        0x4E00..=0xA48C |
        0xA490..=0xA4C6 |
        0xA960..=0xA97C |
        0xAC00..=0xD7A3 |
        0xF900..=0xFAFF |
        0xFE10..=0xFE19 |
        0xFE30..=0xFE52 |
        0xFE54..=0xFE66 |
        0xFE68..=0xFE6B |
        0xFF01..=0xFF60 |
        0xFFE0..=0xFFE6 |
        0x1B000..=0x1B001 |
        0x1F200..=0x1F202 |
        0x1F210..=0x1F23A |
        0x1F240..=0x1F248 |
        0x1F250..=0x1F251 |
        0x1F300..=0x1F567 |
        0x20000..=0x2FFFD |
        0x30000..=0x3FFFD
    ) {
        2
    } else {
        1
    }
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
        // Scalar byte arrays contain numbers; repeated values never do.
        Some(Value::Array(mut values)) if values.first().is_some_and(|v| !v.is_number()) => {
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
