use anyhow::{Result, anyhow};
use clap::Parser;
use explorer_query_contract::{
    DirectionSpec, DisplaySpec, FacetReport, FacetValueReport, FieldReport, FilterOp, QueryMode,
    QuerySpec, RowReport, UniqueReport, encode_hex, field_report, read_query, report_for,
    write_report,
};
use journal::{Direction, DirectoryReader, Entry, FileReader, RawField, ReaderOptions};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    query: PathBuf,
    #[arg(long, default_value = "file")]
    surface: String,
}

trait ExistingReader {
    fn seek_head(&mut self);
    fn seek_tail(&mut self);
    fn next(&mut self) -> journal::Result<bool>;
    fn previous(&mut self) -> journal::Result<bool>;
    fn get_entry(&mut self) -> journal::Result<Entry>;
}

impl ExistingReader for FileReader {
    fn seek_head(&mut self) {
        FileReader::seek_head(self);
    }

    fn seek_tail(&mut self) {
        FileReader::seek_tail(self);
    }

    fn next(&mut self) -> journal::Result<bool> {
        FileReader::next(self)
    }

    fn previous(&mut self) -> journal::Result<bool> {
        FileReader::previous(self)
    }

    fn get_entry(&mut self) -> journal::Result<Entry> {
        FileReader::get_entry(self)
    }
}

impl ExistingReader for DirectoryReader {
    fn seek_head(&mut self) {
        DirectoryReader::seek_head(self);
    }

    fn seek_tail(&mut self) {
        DirectoryReader::seek_tail(self);
    }

    fn next(&mut self) -> journal::Result<bool> {
        DirectoryReader::next(self)
    }

    fn previous(&mut self) -> journal::Result<bool> {
        DirectoryReader::previous(self)
    }

    fn get_entry(&mut self) -> journal::Result<Entry> {
        DirectoryReader::get_entry(self)
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let query = read_query(&args.query)?;
    let start = Instant::now();
    let (rows, facets, unique_values, counters) = run_baseline(&args.input, &args.surface, &query)?;
    let report = report_for(
        "rust-baseline-existing-api",
        &query,
        &args.input,
        start.elapsed(),
        rows,
        facets,
        unique_values,
        counters,
    );
    write_report(&report)
}

fn run_baseline(
    input: &Path,
    surface: &str,
    query: &QuerySpec,
) -> Result<(
    Vec<RowReport>,
    Vec<FacetReport>,
    Vec<UniqueReport>,
    BTreeMap<String, u64>,
)> {
    match surface {
        "file" => {
            let mut reader = FileReader::open_with_options(input, ReaderOptions::snapshot())?;
            execute_reader(&mut reader, query)
        }
        "directory" => {
            let mut reader = DirectoryReader::open_with_options(input, ReaderOptions::snapshot())?;
            execute_reader(&mut reader, query)
        }
        other => Err(anyhow!("unsupported --surface {other}")),
    }
}

fn execute_reader<R: ExistingReader>(
    reader: &mut R,
    query: &QuerySpec,
) -> Result<(
    Vec<RowReport>,
    Vec<FacetReport>,
    Vec<UniqueReport>,
    BTreeMap<String, u64>,
)> {
    match query.direction {
        DirectionSpec::Forward => reader.seek_head(),
        DirectionSpec::Backward => reader.seek_tail(),
    }

    let mut rows = Vec::new();
    let mut facet_maps: HashMap<Vec<u8>, HashMap<Vec<u8>, u64>> = HashMap::new();
    let mut unique_map: HashMap<Vec<u8>, u64> = HashMap::new();
    let unique_field = query
        .unique_field
        .as_ref()
        .map(|field| field.bytes())
        .transpose()?;
    let mut counters = BTreeMap::new();

    loop {
        let advanced = match query.direction {
            DirectionSpec::Forward => reader.next()?,
            DirectionSpec::Backward => reader.previous()?,
        };
        if !advanced {
            break;
        }
        *counters.entry("entries_read".to_string()).or_default() += 1;
        let entry = reader.get_entry()?;
        *counters.entry("entries_expanded".to_string()).or_default() += 1;
        *counters.entry("payloads_seen".to_string()).or_default() += entry.payloads.len() as u64;

        if !time_matches(query, entry.realtime) {
            continue;
        }
        if !entry_matches_filters(&entry, query)? {
            continue;
        }
        if let Some(needle) = query
            .full_text
            .as_ref()
            .map(|value| value.bytes())
            .transpose()?
        {
            if !needle.is_empty() {
                if !entry
                    .payloads
                    .iter()
                    .any(|payload| contains_bytes(payload, &needle))
                {
                    continue;
                }
                *counters
                    .entry("fts_payloads_scanned".to_string())
                    .or_default() += entry.payloads.len() as u64;
            }
        }

        for facet in &query.facets {
            let field = facet.bytes()?;
            for value in entry.get_raw_values(&field) {
                *facet_maps
                    .entry(field.clone())
                    .or_default()
                    .entry(value.to_vec())
                    .or_default() += 1;
            }
        }

        if query.mode == QueryMode::Unique {
            if let Some(field) = unique_field.as_ref() {
                for value in entry.get_raw_values(field) {
                    *unique_map.entry(value.to_vec()).or_default() += 1;
                }
            }
            continue;
        }

        if query.limit.map_or(true, |limit| rows.len() < limit) {
            rows.push(row_report(&entry, query)?);
        }
    }

    let facets = facet_reports(facet_maps);
    let unique_values = unique_reports(unique_map, query);
    Ok((rows, facets, unique_values, counters))
}

fn entry_matches_filters(entry: &Entry, query: &QuerySpec) -> Result<bool> {
    for filter in &query.filters {
        let field = filter.field.bytes()?;
        let values: Vec<Vec<u8>> = filter
            .values
            .iter()
            .map(|value| value.bytes())
            .collect::<Result<Vec<_>>>()?;
        let matched = values
            .iter()
            .any(|value| entry.get_raw_values(&field).iter().any(|got| got == value));
        match filter.op {
            FilterOp::In if !matched => return Ok(false),
            FilterOp::NotIn if matched => return Ok(false),
            _ => {}
        }
    }
    Ok(true)
}

fn row_report(entry: &Entry, query: &QuerySpec) -> Result<RowReport> {
    let fields = match query.display {
        DisplaySpec::None => Vec::new(),
        DisplaySpec::All => entry.raw_fields().map(raw_field_report).collect(),
        DisplaySpec::Fields => {
            let selected: Vec<Vec<u8>> = query
                .display_fields
                .iter()
                .map(|field| field.bytes())
                .collect::<Result<Vec<_>>>()?;
            entry
                .raw_fields()
                .filter(|field| {
                    selected
                        .iter()
                        .any(|selected| selected.as_slice() == field.name)
                })
                .map(raw_field_report)
                .collect()
        }
    };
    Ok(RowReport {
        realtime: entry.realtime,
        seqnum: entry.seqnum,
        cursor: entry.cursor.clone(),
        fields,
    })
}

fn raw_field_report(field: RawField<'_>) -> FieldReport {
    field_report(field.name, field.value)
}

fn time_matches(query: &QuerySpec, realtime: u64) -> bool {
    if query
        .since_realtime_usec
        .is_some_and(|since| realtime < since)
    {
        return false;
    }
    if query
        .until_realtime_usec
        .is_some_and(|until| realtime >= until)
    {
        return false;
    }
    true
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn facet_reports(facet_maps: HashMap<Vec<u8>, HashMap<Vec<u8>, u64>>) -> Vec<FacetReport> {
    let mut facets: Vec<_> = facet_maps
        .into_iter()
        .map(|(field, values)| {
            let mut values: Vec<_> = values
                .into_iter()
                .map(|(value, count)| FacetValueReport {
                    value_hex: encode_hex(&value),
                    count,
                })
                .collect();
            values.sort_by(|a, b| a.value_hex.cmp(&b.value_hex));
            FacetReport {
                field_hex: encode_hex(&field),
                values,
            }
        })
        .collect();
    facets.sort_by(|a, b| a.field_hex.cmp(&b.field_hex));
    facets
}

fn unique_reports(unique_map: HashMap<Vec<u8>, u64>, query: &QuerySpec) -> Vec<UniqueReport> {
    let mut values: Vec<_> = unique_map
        .into_iter()
        .map(|(value, count)| UniqueReport {
            value_hex: encode_hex(&value),
            count: query.unique_include_counts.then_some(count),
        })
        .collect();
    values.sort_by(|a, b| a.value_hex.cmp(&b.value_hex));
    let start = query.unique_skip.min(values.len());
    let end = query
        .limit
        .map(|limit| start.saturating_add(limit).min(values.len()))
        .unwrap_or(values.len());
    values[start..end].to_vec()
}

fn _direction_for_query(query: &QuerySpec) -> Direction {
    match query.direction {
        DirectionSpec::Forward => Direction::Forward,
        DirectionSpec::Backward => Direction::Backward,
    }
}
