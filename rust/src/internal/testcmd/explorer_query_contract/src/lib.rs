use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

pub const QUERY_SCHEMA: &str = "systemd-journal-sdk-explorer-query-v1";
pub const REPORT_SCHEMA: &str = "systemd-journal-sdk-explorer-report-v1";

#[derive(Debug, Clone, Deserialize)]
pub struct QuerySpec {
    pub schema: String,
    pub name: String,
    #[serde(default = "default_mode")]
    pub mode: QueryMode,
    #[serde(default)]
    pub filters: Vec<FilterSpec>,
    #[serde(default)]
    pub facets: Vec<ValueSpec>,
    #[serde(default)]
    pub display: DisplaySpec,
    #[serde(default)]
    pub display_fields: Vec<ValueSpec>,
    #[serde(default)]
    pub full_text: Option<ValueSpec>,
    #[serde(default)]
    pub unique_field: Option<ValueSpec>,
    #[serde(default)]
    pub unique_include_counts: bool,
    #[serde(default)]
    pub unique_skip: usize,
    pub limit: Option<usize>,
    #[serde(default = "default_direction")]
    pub direction: DirectionSpec,
    pub since_realtime_usec: Option<u64>,
    pub until_realtime_usec: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QueryMode {
    Query,
    Unique,
}

fn default_mode() -> QueryMode {
    QueryMode::Query
}

#[derive(Debug, Clone, Deserialize)]
pub struct FilterSpec {
    pub field: ValueSpec,
    pub op: FilterOp,
    #[serde(default)]
    pub values: Vec<ValueSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FilterOp {
    In,
    NotIn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DirectionSpec {
    Forward,
    Backward,
}

fn default_direction() -> DirectionSpec {
    DirectionSpec::Forward
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DisplaySpec {
    None,
    All,
    Fields,
}

impl Default for DisplaySpec {
    fn default() -> Self {
        Self::All
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ValueSpec {
    Text(String),
    Object {
        text: Option<String>,
        hex: Option<String>,
    },
}

impl ValueSpec {
    pub fn bytes(&self) -> Result<Vec<u8>> {
        match self {
            Self::Text(value) => Ok(value.as_bytes().to_vec()),
            Self::Object {
                text: Some(value),
                hex: None,
            } => Ok(value.as_bytes().to_vec()),
            Self::Object {
                text: None,
                hex: Some(value),
            } => decode_hex(value),
            Self::Object {
                text: Some(_),
                hex: Some(_),
            } => Err(anyhow!("value object must not set both text and hex")),
            Self::Object {
                text: None,
                hex: None,
            } => Err(anyhow!("value object must set text or hex")),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QueryReport {
    pub schema: &'static str,
    pub engine: String,
    pub query: String,
    pub input: String,
    pub elapsed_seconds: String,
    pub rows: Vec<RowReport>,
    pub facets: Vec<FacetReport>,
    pub unique_values: Vec<UniqueReport>,
    pub counters: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RowReport {
    pub realtime: u64,
    pub seqnum: u64,
    pub cursor: String,
    pub fields: Vec<FieldReport>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FieldReport {
    pub name_hex: String,
    pub value_hex: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacetReport {
    pub field_hex: String,
    pub values: Vec<FacetValueReport>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacetValueReport {
    pub value_hex: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UniqueReport {
    pub value_hex: String,
    pub count: Option<u64>,
}

pub fn read_query(path: &Path) -> Result<QuerySpec> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read query spec {}", path.display()))?;
    let query: QuerySpec = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse query spec {}", path.display()))?;
    if query.schema != QUERY_SCHEMA {
        return Err(anyhow!(
            "unsupported query schema {} in {}",
            query.schema,
            path.display()
        ));
    }
    Ok(query)
}

pub fn write_report(report: &QueryReport) -> Result<()> {
    serde_json::to_writer_pretty(std::io::stdout(), report)?;
    println!();
    Ok(())
}

pub fn report_for(
    engine: &str,
    query: &QuerySpec,
    input: &Path,
    elapsed: Duration,
    rows: Vec<RowReport>,
    facets: Vec<FacetReport>,
    unique_values: Vec<UniqueReport>,
    counters: BTreeMap<String, u64>,
) -> QueryReport {
    QueryReport {
        schema: REPORT_SCHEMA,
        engine: engine.to_string(),
        query: query.name.clone(),
        input: input.display().to_string(),
        elapsed_seconds: format!("{:.9}", elapsed.as_secs_f64()),
        rows,
        facets,
        unique_values,
        counters,
    }
}

pub fn field_report(name: &[u8], value: &[u8]) -> FieldReport {
    FieldReport {
        name_hex: encode_hex(name),
        value_hex: encode_hex(value),
    }
}

pub fn encode_hex(bytes: &[u8]) -> String {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(LUT[(byte >> 4) as usize] as char);
        out.push(LUT[(byte & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex(raw: &str) -> Result<Vec<u8>> {
    let bytes = raw.as_bytes();
    if bytes.len() % 2 != 0 {
        return Err(anyhow!("hex value length must be even"));
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_nibble(bytes[i])?;
        let lo = hex_nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn hex_nibble(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(anyhow!("invalid hex digit")),
    }
}
