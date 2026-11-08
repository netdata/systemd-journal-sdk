use std::collections::HashSet;

use journal_core::file::writer::is_journal_field_name_valid;

use super::Entry;

/// systemd's JSON_THRESHOLD: without `show_all`, JSON values whose
/// `NAME=value` length reaches it are printed as null.
const JSON_NULL_THRESHOLD: usize = 4096;

const NULL_SEQNUM_ID: &str = "00000000000000000000000000000000";

const METADATA_NAMES: [&str; 6] = [
    "__CURSOR",
    "__REALTIME_TIMESTAMP",
    "__MONOTONIC_TIMESTAMP",
    "__SEQNUM",
    "__SEQNUM_ID",
    "_BOOT_ID",
];

/// Selects what export and JSON output include, as the journalctl
/// `--output-fields` and `--all` options do.
#[derive(Debug, Clone, Default)]
pub struct EntryOutputOptions {
    /// Limits DATA fields to these names; `None` prints every field. The
    /// metadata fields (`__CURSOR`, `__REALTIME_TIMESTAMP`,
    /// `__MONOTONIC_TIMESTAMP`, `__SEQNUM`, `__SEQNUM_ID`, `_BOOT_ID`) are
    /// always printed.
    pub fields: Option<Vec<String>>,
    /// Keeps JSON values of 4096 bytes or more instead of printing null.
    pub show_all: bool,
}

/// Renders entries in systemd's export and JSON formats (systemd logs-show.c
/// `output_export` and `output_json`). It appends to a caller buffer and
/// reuses internal scratch space, so one encoder serves a whole stream.
#[derive(Debug, Default)]
pub struct EntryEncoder {
    fields: Option<HashSet<Vec<u8>>>,
    show_all: bool,
    /// Keeps invalid and non-UTF-8 field names in export instead of stopping
    /// there, the `export_entry_bytes` contract since SOW-0043.
    lossless: bool,
    meta: Vec<u8>,
    meta_ends: [usize; 6],
    items: Vec<Item>,
}

/// A JSON field: a metadata slot (`index < 6`) or a payload
/// (`index - 6`), with the length of its name.
#[derive(Debug, Clone, Copy)]
struct Item {
    index: usize,
    name_len: usize,
}

impl EntryEncoder {
    pub fn new(options: EntryOutputOptions) -> Self {
        Self {
            fields: options
                .fields
                .map(|names| names.into_iter().map(String::into_bytes).collect()),
            show_all: options.show_all,
            ..Self::default()
        }
    }

    fn selected(&self, name: &[u8]) -> bool {
        self.fields
            .as_ref()
            .is_none_or(|fields| fields.contains(name))
    }

    /// Appends the entry in journal export format. Metadata comes first, then
    /// DATA fields in on-disk order without the `_BOOT_ID` field (the header
    /// boot ID is already printed). A field without `=` or with an invalid
    /// name ends the entry, as systemd treats it as partially written.
    pub fn append_export(&mut self, out: &mut Vec<u8>, entry: &Entry) {
        self.fill_metadata(entry);
        for (slot, name) in METADATA_NAMES.iter().enumerate() {
            // Lossless output must stay parseable for any caller-built cursor.
            if self.lossless {
                append_export_field(out, name.as_bytes(), self.metadata_value(slot));
                continue;
            }
            out.extend_from_slice(name.as_bytes());
            out.push(b'=');
            out.extend_from_slice(self.metadata_value(slot));
            out.push(b'\n');
        }
        let synthesized;
        let payloads = match entry_payloads(entry) {
            Some(payloads) => payloads,
            None => {
                synthesized = synthesize_payloads(entry);
                &synthesized
            }
        };
        for payload in payloads {
            if payload.starts_with(b"_BOOT_ID=") {
                continue;
            }
            let Some(eq) = payload.iter().position(|&b| b == b'=') else {
                if self.lossless {
                    continue;
                }
                break;
            };
            let (name, value) = (&payload[..eq], &payload[eq + 1..]);
            if !self.lossless && !is_journal_field_name_valid(name, true) {
                break;
            }
            if !self.selected(name) {
                continue;
            }
            append_export_field(out, name, value);
        }
        out.push(b'\n');
    }

    /// Appends the entry as one line of compact JSON followed by a newline.
    /// Repeated fields become arrays; non-text values become byte arrays.
    /// Keys are sorted (systemd's order is unspecified).
    pub fn append_json(&mut self, out: &mut Vec<u8>, entry: &Entry) {
        self.write_json(out, entry, false);
        out.push(b'\n');
    }

    /// Appends the entry as systemd's tab-indented pretty JSON followed by a
    /// newline.
    pub fn append_json_pretty(&mut self, out: &mut Vec<u8>, entry: &Entry) {
        self.write_json(out, entry, true);
        out.push(b'\n');
    }

    fn write_json(&mut self, out: &mut Vec<u8>, entry: &Entry, pretty: bool) {
        let synthesized;
        let payloads = match entry_payloads(entry) {
            Some(payloads) => payloads,
            None => {
                synthesized = synthesize_payloads(entry);
                &synthesized
            }
        };
        self.collect_json_items(entry, payloads);
        out.push(b'{');
        let items = &self.items;
        let mut start = 0;
        while start < items.len() {
            let name = self.item_name(items[start], payloads);
            let mut end = start + 1;
            while end < items.len() && self.item_name(items[end], payloads) == name {
                end += 1;
            }
            if start > 0 {
                out.push(b',');
            }
            if pretty {
                out.extend_from_slice(b"\n\t");
            }
            write_json_string(out, name);
            out.extend_from_slice(if pretty { b" : " } else { b":" });
            if end - start == 1 {
                self.write_json_value(out, items[start], payloads, pretty, 1);
            } else {
                out.push(b'[');
                for (i, item) in items[start..end].iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    if pretty {
                        out.extend_from_slice(b"\n\t\t");
                    }
                    self.write_json_value(out, *item, payloads, pretty, 2);
                }
                if pretty {
                    out.extend_from_slice(b"\n\t");
                }
                out.push(b']');
            }
            start = end;
        }
        if pretty {
            out.push(b'\n');
        }
        out.push(b'}');
    }

    /// Collects metadata and selected DATA fields sorted by name, keeping
    /// metadata first and on-disk order among values of one field. Unlike
    /// export, JSON skips an invalid field and continues (systemd
    /// `update_json_data_split`).
    fn collect_json_items(&mut self, entry: &Entry, payloads: &[Vec<u8>]) {
        self.fill_metadata(entry);
        let mut items = std::mem::take(&mut self.items);
        items.clear();
        for (slot, name) in METADATA_NAMES.iter().enumerate() {
            items.push(Item {
                index: slot,
                name_len: name.len(),
            });
        }
        for (i, payload) in payloads.iter().enumerate() {
            let Some(eq) = payload.iter().position(|&b| b == b'=') else {
                continue;
            };
            let name = &payload[..eq];
            if name == b"_BOOT_ID"
                || !is_journal_field_name_valid(name, true)
                || !self.selected(name)
            {
                continue;
            }
            items.push(Item {
                index: METADATA_NAMES.len() + i,
                name_len: eq,
            });
        }
        items.sort_by(|a, b| {
            self.item_name(*a, payloads)
                .cmp(self.item_name(*b, payloads))
        });
        self.items = items;
    }

    fn item_name<'a>(&'a self, item: Item, payloads: &'a [Vec<u8>]) -> &'a [u8] {
        match item.index.checked_sub(METADATA_NAMES.len()) {
            None => METADATA_NAMES[item.index].as_bytes(),
            Some(i) => &payloads[i][..item.name_len],
        }
    }

    fn item_value<'a>(&'a self, item: Item, payloads: &'a [Vec<u8>]) -> &'a [u8] {
        match item.index.checked_sub(METADATA_NAMES.len()) {
            None => self.metadata_value(item.index),
            Some(i) => &payloads[i][item.name_len + 1..],
        }
    }

    fn write_json_value(
        &self,
        out: &mut Vec<u8>,
        item: Item,
        payloads: &[Vec<u8>],
        pretty: bool,
        depth: usize,
    ) {
        let value = self.item_value(item, payloads);
        if !self.show_all && item.name_len + 1 + value.len() >= JSON_NULL_THRESHOLD {
            out.extend_from_slice(b"null");
            return;
        }
        if is_printable_text(value, true) {
            write_json_string(out, value);
            return;
        }
        if value.is_empty() {
            out.extend_from_slice(b"[]");
            return;
        }
        out.push(b'[');
        let mut digits = itoa_buffer();
        for (i, byte) in value.iter().enumerate() {
            if i > 0 {
                out.push(b',');
            }
            if pretty {
                out.push(b'\n');
                out.extend(std::iter::repeat_n(b'\t', depth + 1));
            }
            out.extend_from_slice(format_u8(&mut digits, *byte));
        }
        if pretty {
            out.push(b'\n');
            out.extend(std::iter::repeat_n(b'\t', depth));
        }
        out.push(b']');
    }

    /// Formats the six metadata values systemd prints for every entry into
    /// the scratch buffer.
    fn fill_metadata(&mut self, entry: &Entry) {
        use std::io::Write;
        let buf = &mut self.meta;
        buf.clear();
        buf.extend_from_slice(entry.cursor.as_bytes());
        self.meta_ends[0] = buf.len();
        let _ = write!(buf, "{}", entry.realtime);
        self.meta_ends[1] = buf.len();
        let _ = write!(buf, "{}", entry.monotonic);
        self.meta_ends[2] = buf.len();
        let _ = write!(buf, "{}", entry.seqnum);
        self.meta_ends[3] = buf.len();
        match crate::parse_cursor(&entry.cursor) {
            Ok((seqnum_id, ..)) if !seqnum_id.is_empty() => {
                buf.extend_from_slice(seqnum_id.as_bytes())
            }
            _ => buf.extend_from_slice(NULL_SEQNUM_ID.as_bytes()),
        }
        self.meta_ends[4] = buf.len();
        let mut boot_id = [0u8; 32];
        let _ = hex::encode_to_slice(entry.boot_id, &mut boot_id);
        buf.extend_from_slice(&boot_id);
        self.meta_ends[5] = buf.len();
    }

    fn metadata_value(&self, slot: usize) -> &[u8] {
        let start = if slot == 0 {
            0
        } else {
            self.meta_ends[slot - 1]
        };
        &self.meta[start..self.meta_ends[slot]]
    }
}

fn itoa_buffer() -> [u8; 3] {
    [0; 3]
}

fn format_u8(buf: &mut [u8; 3], value: u8) -> &[u8] {
    let mut n = value;
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + n % 10;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    &buf[i..]
}

/// Escapes as systemd's sd-json does: quote, backslash and C0 controls only,
/// so text bytes are emitted unchanged.
fn write_json_string(out: &mut Vec<u8>, value: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push(b'"');
    for &c in value {
        match c {
            b'"' => out.extend_from_slice(b"\\\""),
            b'\\' => out.extend_from_slice(b"\\\\"),
            0x08 => out.extend_from_slice(b"\\b"),
            0x0c => out.extend_from_slice(b"\\f"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            c if c < b' ' => out.extend_from_slice(&[
                b'\\',
                b'u',
                b'0',
                b'0',
                HEX[usize::from(c >> 4)],
                HEX[usize::from(c & 0xf)],
            ]),
            c => out.push(c),
        }
    }
    out.push(b'"');
}

/// The entry's DATA payloads in on-disk order, or `None` for an entry built
/// by hand with only the field maps.
fn entry_payloads(entry: &Entry) -> Option<&[Vec<u8>]> {
    (!entry.payloads.is_empty() || (entry.field_values.is_empty() && entry.fields.is_empty()))
        .then_some(entry.payloads.as_slice())
}

/// Builds `NAME=value` payloads from the field maps in name order.
fn append_export_field(out: &mut Vec<u8>, name: &[u8], value: &[u8]) {
    out.extend_from_slice(name);
    // `NAME=value` is text exactly when both halves are: `=` is ASCII.
    if is_printable_text(name, false) && is_printable_text(value, false) {
        out.push(b'=');
        out.extend_from_slice(value);
    } else {
        out.push(b'\n');
        out.extend_from_slice(&(value.len() as u64).to_le_bytes());
        out.extend_from_slice(value);
    }
    out.push(b'\n');
}

fn synthesize_payloads(entry: &Entry) -> Vec<Vec<u8>> {
    let mut names: Vec<&String> = entry.field_values.keys().collect();
    names.extend(
        entry
            .fields
            .keys()
            .filter(|name| !entry.field_values.contains_key(*name)),
    );
    names.sort();
    let mut payloads = Vec::new();
    for name in names {
        let single;
        let values: &[Vec<u8>] = match entry.field_values.get(name) {
            Some(values) if !values.is_empty() => values,
            _ => match entry.fields.get(name) {
                Some(value) => {
                    single = [value.clone()];
                    &single
                }
                None => continue,
            },
        };
        for value in values {
            let mut payload = Vec::with_capacity(name.len() + 1 + value.len());
            payload.extend_from_slice(name.as_bytes());
            payload.push(b'=');
            payload.extend_from_slice(value);
            payloads.push(payload);
        }
    }
    payloads
}

/// Reports whether `value` is text that systemd prints as is
/// (`utf8_is_printable_newline`): valid UTF-8 without C0/C1 controls other
/// than tab (and newline when `allow_newline`), or Unicode noncharacters.
pub fn is_printable_text(value: &[u8], allow_newline: bool) -> bool {
    let Ok(text) = std::str::from_utf8(value) else {
        return false;
    };
    text.chars().all(|ch| {
        let cp = ch as u32;
        !((cp < 0x20 && ch != '\t' && (ch != '\n' || !allow_newline))
            || (0x7f..=0x9f).contains(&cp)
            || (0xfdd0..=0xfdef).contains(&cp)
            || cp & 0xfffe == 0xfffe)
    })
}

/// Returns the entry in journal export format with every field. Unlike
/// [`EntryEncoder`], which stops where systemd does, it keeps fields with
/// invalid or non-UTF-8 names so no data is lost.
pub fn export_entry_bytes(entry: &Entry) -> Vec<u8> {
    let mut out = Vec::new();
    EntryEncoder {
        lossless: true,
        ..EntryEncoder::default()
    }
    .append_export(&mut out, entry);
    out
}

pub fn export_entry(entry: &Entry) -> String {
    String::from_utf8_lossy(&export_entry_bytes(entry)).into_owned()
}

/// Returns the entry as a JSON value: text values as strings, other values as
/// byte arrays, repeated fields as arrays. Values are never replaced by null
/// (journalctl `--all`), so no data is lost.
pub fn json_entry(entry: &Entry) -> serde_json::Value {
    let mut encoder = EntryEncoder::new(EntryOutputOptions {
        fields: None,
        show_all: true,
    });
    let synthesized;
    let payloads = match entry_payloads(entry) {
        Some(payloads) => payloads,
        None => {
            synthesized = synthesize_payloads(entry);
            &synthesized
        }
    };
    encoder.collect_json_items(entry, payloads);
    let mut map = serde_json::Map::new();
    let items = &encoder.items;
    let mut start = 0;
    while start < items.len() {
        let name = encoder.item_name(items[start], payloads);
        let mut end = start + 1;
        while end < items.len() && encoder.item_name(items[end], payloads) == name {
            end += 1;
        }
        let mut values: Vec<_> = items[start..end]
            .iter()
            .map(|item| json_map_value(encoder.item_value(*item, payloads)))
            .collect();
        let value = if values.len() == 1 {
            values.remove(0)
        } else {
            serde_json::Value::Array(values)
        };
        map.insert(String::from_utf8_lossy(name).into_owned(), value);
        start = end;
    }
    serde_json::Value::Object(map)
}

fn json_map_value(value: &[u8]) -> serde_json::Value {
    match std::str::from_utf8(value) {
        Ok(text) if is_printable_text(value, true) => serde_json::Value::String(text.to_string()),
        _ => serde_json::Value::Array(
            value
                .iter()
                .map(|byte| serde_json::Value::Number((*byte).into()))
                .collect(),
        ),
    }
}

pub fn format_entry_text(entry: &Entry) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(message) = entry.get("MESSAGE") {
        out.extend_from_slice(message);
    }
    out.push(b'\n');
    out
}
