use anyhow::{Result, anyhow};
use clap::Parser;
use explorer_query_contract::{
    DirectionSpec, DisplaySpec, FacetReport, FacetValueReport, FieldReport, FilterOp, QueryMode,
    QuerySpec, RowReport, UniqueReport, encode_hex, field_report, read_query, report_for,
    write_report,
};
use journal::{
    Direction, DirectoryReader, ExplorerDisplay, ExplorerFilter, ExplorerFilterKind, ExplorerQuery,
    ExplorerQueryCounters, ExplorerUniqueQuery, FileReader, ReaderOptions,
};
use std::collections::BTreeMap;
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

fn main() -> Result<()> {
    let args = Args::parse();
    let query = read_query(&args.query)?;
    let start = Instant::now();
    let (rows, facets, unique_values, counters) =
        run_optimized(&args.input, &args.surface, &query)?;
    let report = report_for(
        "rust-optimized-explorer-api",
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

fn run_optimized(
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
        "file" => match query.mode {
            QueryMode::Query => {
                let mut reader = FileReader::open_with_options(input, ReaderOptions::snapshot())?;
                let result = reader.explorer_query(&explorer_query(query)?)?;
                Ok((
                    row_reports(result.rows),
                    facet_reports(result.facets),
                    Vec::new(),
                    counters_report(&result.counters),
                ))
            }
            QueryMode::Unique => {
                let mut reader = FileReader::open_with_options(input, ReaderOptions::snapshot())?;
                let result = reader.explorer_unique(&explorer_unique_query(query)?)?;
                Ok((
                    Vec::new(),
                    Vec::new(),
                    unique_reports(result.values),
                    counters_report(&result.counters),
                ))
            }
        },
        "directory" => match query.mode {
            QueryMode::Query => {
                let mut reader =
                    DirectoryReader::open_with_options(input, ReaderOptions::snapshot())?;
                let result = reader.explorer_query(&explorer_query(query)?)?;
                Ok((
                    row_reports(result.rows),
                    facet_reports(result.facets),
                    Vec::new(),
                    counters_report(&result.counters),
                ))
            }
            QueryMode::Unique => {
                let mut reader =
                    DirectoryReader::open_with_options(input, ReaderOptions::snapshot())?;
                let result = reader.explorer_unique(&explorer_unique_query(query)?)?;
                Ok((
                    Vec::new(),
                    Vec::new(),
                    unique_reports(result.values),
                    counters_report(&result.counters),
                ))
            }
        },
        other => Err(anyhow!("unsupported --surface {other}")),
    }
}

fn explorer_query(query: &QuerySpec) -> Result<ExplorerQuery> {
    Ok(ExplorerQuery {
        filters: explorer_filters(query)?,
        facets: query
            .facets
            .iter()
            .map(|field| field.bytes())
            .collect::<Result<Vec<_>>>()?,
        full_text: query
            .full_text
            .as_ref()
            .map(|value| value.bytes())
            .transpose()?,
        display: explorer_display(query)?,
        limit: query.limit,
        direction: explorer_direction(query.direction),
        since_realtime_usec: query.since_realtime_usec,
        until_realtime_usec: query.until_realtime_usec,
    })
}

fn explorer_unique_query(query: &QuerySpec) -> Result<ExplorerUniqueQuery> {
    let Some(field) = query.unique_field.as_ref() else {
        return Err(anyhow!("unique mode requires unique_field"));
    };
    Ok(ExplorerUniqueQuery {
        field: field.bytes()?,
        filters: explorer_filters(query)?,
        limit: query.limit,
        skip: query.unique_skip,
        include_counts: query.unique_include_counts,
        since_realtime_usec: query.since_realtime_usec,
        until_realtime_usec: query.until_realtime_usec,
    })
}

fn explorer_filters(query: &QuerySpec) -> Result<Vec<ExplorerFilter>> {
    query
        .filters
        .iter()
        .map(|filter| {
            let kind = match filter.op {
                FilterOp::In => ExplorerFilterKind::In,
                FilterOp::NotIn => ExplorerFilterKind::NotIn,
            };
            Ok(ExplorerFilter {
                field: filter.field.bytes()?,
                values: filter
                    .values
                    .iter()
                    .map(|value| value.bytes())
                    .collect::<Result<Vec<_>>>()?,
                kind,
            })
        })
        .collect()
}

fn explorer_display(query: &QuerySpec) -> Result<ExplorerDisplay> {
    match query.display {
        DisplaySpec::None => Ok(ExplorerDisplay::None),
        DisplaySpec::All => Ok(ExplorerDisplay::All),
        DisplaySpec::Fields => Ok(ExplorerDisplay::Fields(
            query
                .display_fields
                .iter()
                .map(|field| field.bytes())
                .collect::<Result<Vec<_>>>()?,
        )),
    }
}

fn explorer_direction(direction: DirectionSpec) -> Direction {
    match direction {
        DirectionSpec::Forward => Direction::Forward,
        DirectionSpec::Backward => Direction::Backward,
    }
}

fn row_reports(rows: Vec<journal::ExplorerRow>) -> Vec<RowReport> {
    rows.into_iter()
        .map(|row| RowReport {
            realtime: row.realtime,
            seqnum: row.seqnum,
            cursor: row.cursor,
            fields: row
                .fields
                .into_iter()
                .map(|(name, value)| field_report(&name, &value))
                .collect::<Vec<FieldReport>>(),
        })
        .collect()
}

fn facet_reports(facets: Vec<journal::ExplorerFacet>) -> Vec<FacetReport> {
    facets
        .into_iter()
        .map(|facet| FacetReport {
            field_hex: encode_hex(&facet.field),
            values: facet
                .values
                .into_iter()
                .map(|value| FacetValueReport {
                    value_hex: encode_hex(&value.value),
                    count: value.count,
                })
                .collect(),
        })
        .collect()
}

fn unique_reports(values: Vec<journal::ExplorerUniqueValue>) -> Vec<UniqueReport> {
    values
        .into_iter()
        .map(|value| UniqueReport {
            value_hex: encode_hex(&value.value),
            count: value.count,
        })
        .collect()
}

fn counters_report(counters: &ExplorerQueryCounters) -> BTreeMap<String, u64> {
    BTreeMap::from([
        (
            "candidate_data_refs_visited".to_string(),
            counters.candidate_data_refs_visited,
        ),
        ("candidate_entries".to_string(), counters.candidate_entries),
        (
            "constrained_facet_counts".to_string(),
            counters.constrained_facet_counts,
        ),
        (
            "data_refs_reported".to_string(),
            counters.data_refs_reported,
        ),
        (
            "display_rows_expanded".to_string(),
            counters.display_rows_expanded,
        ),
        (
            "entry_offsets_indexed".to_string(),
            counters.entry_offsets_indexed,
        ),
        (
            "facet_values_materialized".to_string(),
            counters.facet_values_materialized,
        ),
        (
            "field_linkage_fallbacks".to_string(),
            counters.field_linkage_fallbacks,
        ),
        (
            "field_linkage_hits".to_string(),
            counters.field_linkage_hits,
        ),
        (
            "filter_data_objects_examined".to_string(),
            counters.filter_data_objects_examined,
        ),
        (
            "fts_payloads_scanned".to_string(),
            counters.fts_payloads_scanned,
        ),
        (
            "payloads_decompressed".to_string(),
            counters.payloads_decompressed,
        ),
        (
            "payloads_materialized".to_string(),
            counters.payloads_materialized,
        ),
    ])
}
