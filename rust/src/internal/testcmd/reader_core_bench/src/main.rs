use anyhow::{Context, Result, anyhow};
use clap::Parser;
use journal::{
    Direction, DirectoryReader, ExplorerFieldMode, ExplorerFilter, ExplorerQuery, ExplorerStrategy,
    FileReader, JournalFile, JournalReader, Mmap, ReaderBounds, ReaderOptions,
    SdJournalEnumerateAvailableData, SdJournalNext, SdJournalOpenDirectoryWithOptions,
    SdJournalOpenFilesWithOptions, SdJournalRestartData,
};
use journal_core::file::{ExperimentalMmapStrategy, HashableObject};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::hint::black_box;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::time::Instant;

const DEFAULT_WINDOW_SIZE: u64 = 32 * 1024 * 1024;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long = "input", required = true)]
    inputs: Vec<PathBuf>,
    #[arg(long, default_value = "core-payloads")]
    mode: String,
    #[arg(long, default_value = "file")]
    surface: String,
    #[arg(long, default_value = "forward")]
    direction: String,
    #[arg(long, default_value_t = DEFAULT_WINDOW_SIZE)]
    window_size: u64,
    #[arg(long, default_value = "live")]
    bounds: String,
    #[arg(long, default_value = "windowed")]
    mmap_strategy: String,
    #[arg(long = "explorer-facet")]
    explorer_facets: Vec<String>,
    #[arg(long = "explorer-filter")]
    explorer_filters: Vec<String>,
    #[arg(long = "explorer-histogram")]
    explorer_histogram: Option<String>,
    #[arg(long, default_value_t = 0)]
    explorer_limit: usize,
    #[arg(long = "explorer-fts")]
    explorer_fts_patterns: Vec<String>,
    #[arg(long, default_value = "first-value")]
    explorer_field_mode: String,
    #[arg(long, default_value_t = false)]
    explorer_use_source_realtime: bool,
    #[arg(long, default_value = "traversal")]
    explorer_strategy: String,
    #[arg(long)]
    explorer_after_usec: Option<u64>,
    #[arg(long)]
    explorer_before_usec: Option<u64>,
}

#[derive(Default)]
struct Counts {
    records: u64,
    fields: u64,
    bytes: u64,
    checksum: u64,
    extra: serde_json::Map<String, Value>,
}

impl Counts {
    fn add_payload(&mut self, payload: &[u8]) {
        self.fields = self.fields.saturating_add(1);
        self.bytes = self.bytes.saturating_add(payload.len() as u64);
        self.checksum = checksum_payload(self.checksum, payload);
    }

    fn add_record_marker(&mut self, value: u64) {
        self.records = self.records.saturating_add(1);
        self.checksum = self.checksum.rotate_left(7) ^ value;
    }
}

struct ReadConfig<'a> {
    mode: &'a str,
    surface: &'a str,
    direction: Direction,
    bounds: &'a str,
    strategy: ExperimentalMmapStrategy,
    window_size: u64,
    explorer_facets: &'a [String],
    explorer_filters: &'a [String],
    explorer_histogram: Option<&'a str>,
    explorer_limit: usize,
    explorer_fts_patterns: &'a [String],
    explorer_field_mode: ExplorerFieldMode,
    explorer_use_source_realtime: bool,
    explorer_strategy: ExplorerStrategy,
    explorer_after_usec: Option<u64>,
    explorer_before_usec: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct CompressedDataSample {
    refs: u64,
    compressed_bytes: u64,
    decompressed_bytes: u64,
    is_zstd: bool,
    is_lz4: bool,
    is_xz: bool,
}

#[derive(Default)]
struct CompressionStats {
    compressed: HashMap<NonZeroU64, CompressedDataSample>,
    total_refs: u64,
    uncompressed_refs: u64,
    compressed_refs: u64,
    compressed_repeat_refs: u64,
    compressed_zstd_refs: u64,
    compressed_lz4_refs: u64,
    compressed_xz_refs: u64,
    compressed_bytes_all_refs: u64,
}

impl CompressionStats {
    fn record_uncompressed(&mut self) {
        self.total_refs = self.total_refs.saturating_add(1);
        self.uncompressed_refs = self.uncompressed_refs.saturating_add(1);
    }

    fn record_compressed(
        &mut self,
        offset: NonZeroU64,
        sample: CompressedDataSample,
    ) -> Result<()> {
        self.total_refs = self.total_refs.saturating_add(1);
        self.compressed_refs = self.compressed_refs.saturating_add(1);
        self.compressed_bytes_all_refs = self
            .compressed_bytes_all_refs
            .saturating_add(sample.compressed_bytes);
        if sample.is_zstd {
            self.compressed_zstd_refs = self.compressed_zstd_refs.saturating_add(1);
        }
        if sample.is_lz4 {
            self.compressed_lz4_refs = self.compressed_lz4_refs.saturating_add(1);
        }
        if sample.is_xz {
            self.compressed_xz_refs = self.compressed_xz_refs.saturating_add(1);
        }

        if let Some(existing) = self.compressed.get_mut(&offset) {
            existing.refs = existing.refs.saturating_add(1);
            self.compressed_repeat_refs = self.compressed_repeat_refs.saturating_add(1);
            return Ok(());
        }

        self.compressed
            .insert(offset, CompressedDataSample { refs: 1, ..sample });
        Ok(())
    }

    fn to_json(&self) -> Value {
        let unique_compressed_refs = self.compressed.len() as u64;
        let compressed_reuse_ratio = if self.compressed_refs > 0 {
            self.compressed_repeat_refs as f64 / self.compressed_refs as f64
        } else {
            0.0
        };
        let compressed_bytes_unique = self
            .compressed
            .values()
            .map(|sample| sample.compressed_bytes)
            .sum::<u64>();
        let decompressed_bytes_unique = self
            .compressed
            .values()
            .map(|sample| sample.decompressed_bytes)
            .sum::<u64>();
        let decompressed_bytes_all_refs = self
            .compressed
            .values()
            .map(|sample| sample.decompressed_bytes.saturating_mul(sample.refs))
            .sum::<u64>();
        let reusable_offsets = self
            .compressed
            .values()
            .filter(|sample| sample.refs > 1)
            .count() as u64;
        let max_refs_per_compressed_offset = self
            .compressed
            .values()
            .map(|sample| sample.refs)
            .max()
            .unwrap_or(0);

        json!({
            "total_data_refs": self.total_refs,
            "uncompressed_data_refs": self.uncompressed_refs,
            "compressed_data_refs": self.compressed_refs,
            "unique_compressed_offsets": unique_compressed_refs,
            "compressed_repeat_refs": self.compressed_repeat_refs,
            "compressed_reuse_ratio": compressed_reuse_ratio,
            "reusable_compressed_offsets": reusable_offsets,
            "max_refs_per_compressed_offset": max_refs_per_compressed_offset,
            "compressed_zstd_refs": self.compressed_zstd_refs,
            "compressed_lz4_refs": self.compressed_lz4_refs,
            "compressed_xz_refs": self.compressed_xz_refs,
            "compressed_bytes_all_refs": self.compressed_bytes_all_refs,
            "compressed_bytes_unique": compressed_bytes_unique,
            "decompressed_bytes_unique": decompressed_bytes_unique,
            "decompressed_bytes_all_refs": decompressed_bytes_all_refs,
            "avoided_decompressed_bytes_if_unique_cache": decompressed_bytes_all_refs.saturating_sub(decompressed_bytes_unique),
        })
    }
}

fn checksum_payload(mut checksum: u64, payload: &[u8]) -> u64 {
    checksum = checksum.rotate_left(5) ^ payload.len() as u64;
    if let Some(first) = payload.first() {
        checksum ^= (*first as u64) << 8;
    }
    if let Some(last) = payload.last() {
        checksum ^= *last as u64;
    }
    checksum
}

fn checksum_bytes(mut checksum: u64, bytes: &[u8]) -> u64 {
    checksum = checksum.rotate_left(11) ^ bytes.len() as u64;
    for byte in bytes.iter().take(8) {
        checksum = checksum.rotate_left(3) ^ *byte as u64;
    }
    if let Some(last) = bytes.last() {
        checksum ^= (*last as u64) << 17;
    }
    checksum
}

fn split_benchmark_payload(payload: &[u8]) -> Option<(&[u8], &[u8])> {
    let eq = payload.iter().position(|byte| *byte == b'=')?;
    Some((&payload[..eq], &payload[eq + 1..]))
}

fn increment_facet_count(
    facets: &mut HashMap<Vec<u8>, HashMap<Vec<u8>, u64>>,
    field: &[u8],
    value: &[u8],
) {
    facets
        .entry(field.to_vec())
        .or_default()
        .entry(value.to_vec())
        .and_modify(|count| *count = count.saturating_add(1))
        .or_insert(1);
}

fn facet_summary(facets: &HashMap<Vec<u8>, HashMap<Vec<u8>, u64>>) -> Value {
    let mut facet_count = 0u64;
    let mut value_count = 0u64;
    let mut total_updates = 0u64;
    let mut checksum = 0u64;
    let mut fields: Vec<_> = facets.iter().collect();
    fields.sort_by(|(left, _), (right, _)| left.cmp(right));

    for (field, values) in fields {
        facet_count = facet_count.saturating_add(1);
        checksum = checksum_bytes(checksum, field);
        let mut sorted_values: Vec<_> = values.iter().collect();
        sorted_values.sort_by(|(left, _), (right, _)| left.cmp(right));
        value_count = value_count.saturating_add(sorted_values.len() as u64);
        for (value, count) in sorted_values {
            checksum = checksum_bytes(checksum, value) ^ count;
            total_updates = total_updates.saturating_add(*count);
        }
    }

    json!({
        "facet_fields": facet_count,
        "facet_values": value_count,
        "facet_updates": total_updates,
        "facet_checksum": checksum,
    })
}

fn parse_direction(value: &str) -> Result<Direction> {
    match value {
        "forward" => Ok(Direction::Forward),
        "backward" => Ok(Direction::Backward),
        other => Err(anyhow!("invalid --direction: {other}")),
    }
}

fn parse_mmap_strategy(value: &str) -> Result<ExperimentalMmapStrategy> {
    match value {
        "windowed" => Ok(ExperimentalMmapStrategy::Windowed),
        "whole-file" => Ok(ExperimentalMmapStrategy::WholeFile),
        other => Err(anyhow!("invalid --mmap-strategy: {other}")),
    }
}

fn parse_explorer_field_mode(value: &str) -> Result<ExplorerFieldMode> {
    match value {
        "all-values" => Ok(ExplorerFieldMode::AllValues),
        "first-value" => Ok(ExplorerFieldMode::FirstValue),
        other => Err(anyhow!("invalid --explorer-field-mode: {other}")),
    }
}

fn parse_explorer_strategy(value: &str) -> Result<ExplorerStrategy> {
    match value {
        "traversal" => Ok(ExplorerStrategy::Traversal),
        "index" => Ok(ExplorerStrategy::Index),
        "compare" => Ok(ExplorerStrategy::Compare),
        other => Err(anyhow!("invalid --explorer-strategy: {other}")),
    }
}

fn defaulted_facets(cfg: &ReadConfig<'_>) -> Vec<Vec<u8>> {
    cfg.explorer_facets
        .iter()
        .map(|field| field.as_bytes().to_vec())
        .collect()
}

fn parse_explorer_filters(raw_filters: &[String]) -> Result<Vec<ExplorerFilter>> {
    let mut grouped: HashMap<Vec<u8>, Vec<Vec<u8>>> = HashMap::new();
    for raw_filter in raw_filters {
        let Some((field, value)) = raw_filter.split_once('=') else {
            return Err(anyhow!(
                "--explorer-filter must be FIELD=VALUE: {raw_filter}"
            ));
        };
        if field.is_empty() || field.as_bytes().contains(&b'=') {
            return Err(anyhow!("invalid --explorer-filter field: {raw_filter}"));
        }
        grouped
            .entry(field.as_bytes().to_vec())
            .or_default()
            .push(value.as_bytes().to_vec());
    }

    Ok(grouped
        .into_iter()
        .map(|(field, values)| ExplorerFilter { field, values })
        .collect())
}

fn explorer_query_from_config(cfg: &ReadConfig<'_>) -> Result<ExplorerQuery> {
    Ok(ExplorerQuery {
        after_realtime_usec: cfg.explorer_after_usec,
        before_realtime_usec: cfg.explorer_before_usec,
        direction: cfg.direction,
        limit: cfg.explorer_limit,
        filters: parse_explorer_filters(cfg.explorer_filters)?,
        facets: defaulted_facets(cfg),
        histogram: cfg
            .explorer_histogram
            .map(|field| field.as_bytes().to_vec()),
        fts_patterns: cfg
            .explorer_fts_patterns
            .iter()
            .map(|pattern| pattern.as_bytes().to_vec())
            .collect(),
        field_mode: cfg.explorer_field_mode,
        use_source_realtime: cfg.explorer_use_source_realtime,
        ..ExplorerQuery::default()
    })
}

fn process_status_kb() -> Value {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return json!({});
    };
    let wanted = [
        "VmSize", "VmPeak", "VmRSS", "VmHWM", "RssAnon", "RssFile", "RssShmem", "VmData", "VmStk",
        "VmExe", "VmLib", "VmPTE",
    ];
    let mut object = serde_json::Map::new();
    for line in status.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if !wanted.contains(&key) {
            continue;
        }
        let Some(kb) = value
            .split_whitespace()
            .next()
            .and_then(|raw| raw.parse::<u64>().ok())
        else {
            continue;
        };
        object.insert(format!("{key}_kb"), json!(kb));
    }
    Value::Object(object)
}

fn open_core(
    path: &Path,
    window_size: u64,
    bounds: &str,
    strategy: ExperimentalMmapStrategy,
) -> Result<JournalFile<Mmap>> {
    let result = match bounds {
        "live" => JournalFile::open_path_with_strategy(path, window_size, strategy),
        "snapshot" => JournalFile::open_path_snapshot(path, window_size, strategy),
        other => return Err(anyhow!("invalid --bounds: {other}")),
    };
    result.with_context(|| format!("failed to open journal file {}", path.display()))
}

fn read_core(path: &Path, cfg: &ReadConfig<'_>) -> Result<Counts> {
    let file = open_core(path, cfg.window_size, cfg.bounds, cfg.strategy)?;
    let mut reader = JournalReader::default();
    reader.set_location(match cfg.direction {
        Direction::Forward => journal::Location::Head,
        Direction::Backward => journal::Location::Tail,
    });
    let mut counts = Counts::default();
    let mut offsets = Vec::new();
    let mut decompressed = Vec::new();
    let mut compression_stats = CompressionStats::default();

    while reader.step(&file, cfg.direction)? {
        let realtime = reader.get_realtime_usec(&file)?;
        counts.add_record_marker(realtime);
        record_core_mode(
            cfg.mode,
            &file,
            &reader,
            &mut counts,
            &mut offsets,
            &mut decompressed,
            &mut compression_stats,
        )?;
    }
    if cfg.mode == "core-compressed-stats" {
        counts
            .extra
            .insert("compression_stats".to_string(), compression_stats.to_json());
    }

    black_box(counts.checksum);
    Ok(counts)
}

fn record_core_mode(
    mode: &str,
    file: &JournalFile<Mmap>,
    reader: &JournalReader<'_, Mmap>,
    counts: &mut Counts,
    offsets: &mut Vec<NonZeroU64>,
    decompressed: &mut Vec<u8>,
    compression_stats: &mut CompressionStats,
) -> Result<()> {
    match mode {
        "core-next" => Ok(()),
        "core-offsets" => record_core_offsets(file, reader, counts, offsets),
        "core-payloads" => record_core_payloads(file, reader, counts, offsets, decompressed),
        "core-compressed-stats" => record_core_compressed_stats(
            file,
            reader,
            counts,
            offsets,
            decompressed,
            compression_stats,
        ),
        other => Err(anyhow!("invalid core mode for file surface: {other}")),
    }
}

fn record_core_offsets(
    file: &JournalFile<Mmap>,
    reader: &JournalReader<'_, Mmap>,
    counts: &mut Counts,
    offsets: &mut Vec<NonZeroU64>,
) -> Result<()> {
    offsets.clear();
    reader.entry_data_offsets(file, offsets)?;
    counts.fields = counts.fields.saturating_add(offsets.len() as u64);
    counts.checksum ^= offsets.len() as u64;
    Ok(())
}

fn record_core_compressed_stats(
    file: &JournalFile<Mmap>,
    reader: &JournalReader<'_, Mmap>,
    counts: &mut Counts,
    offsets: &mut Vec<NonZeroU64>,
    decompressed: &mut Vec<u8>,
    stats: &mut CompressionStats,
) -> Result<()> {
    offsets.clear();
    reader.entry_data_offsets(file, offsets)?;
    counts.fields = counts.fields.saturating_add(offsets.len() as u64);
    counts.checksum ^= offsets.len() as u64;

    for offset in offsets.iter().copied() {
        let data = file.data_ref(offset)?;
        if !data.is_compressed() {
            stats.record_uncompressed();
            continue;
        }

        decompressed.clear();
        let len = data.decompress(decompressed)?;
        stats.record_compressed(
            offset,
            CompressedDataSample {
                refs: 1,
                compressed_bytes: data.raw_payload().len() as u64,
                decompressed_bytes: len as u64,
                is_zstd: data.zstd_compressed(),
                is_lz4: data.lz4_compressed(),
                is_xz: data.xz_compressed(),
            },
        )?;
    }

    Ok(())
}

fn record_core_payloads(
    file: &JournalFile<Mmap>,
    reader: &JournalReader<'_, Mmap>,
    counts: &mut Counts,
    offsets: &mut Vec<NonZeroU64>,
    decompressed: &mut Vec<u8>,
) -> Result<()> {
    offsets.clear();
    reader.entry_data_offsets(file, offsets)?;
    for offset in offsets.iter().copied() {
        let data = file.data_ref(offset)?;
        let payload = if data.is_compressed() {
            decompressed.clear();
            data.decompress(decompressed)?;
            decompressed.as_slice()
        } else {
            data.raw_payload()
        };
        counts.add_payload(black_box(payload));
    }
    Ok(())
}

fn reader_options(
    bounds: &str,
    strategy: ExperimentalMmapStrategy,
    window_size: u64,
) -> Result<ReaderOptions> {
    let bounds = match bounds {
        "live" => ReaderBounds::Live,
        "snapshot" => ReaderBounds::Snapshot,
        other => return Err(anyhow!("invalid --bounds: {other}")),
    };
    Ok(ReaderOptions {
        window_size,
        bounds,
        mmap_strategy: strategy,
    })
}

fn read_sdk_file(path: &Path, cfg: &ReadConfig<'_>) -> Result<Counts> {
    let options = reader_options(cfg.bounds, cfg.strategy, cfg.window_size)?;
    let mut reader = FileReader::open_with_options(path, options)
        .with_context(|| format!("failed to open SDK file reader for {}", path.display()))?;
    match cfg.direction {
        Direction::Forward => reader.seek_head(),
        Direction::Backward => reader.seek_tail(),
    }
    let mut counts = Counts::default();
    while step_file_reader(&mut reader, cfg.direction)? {
        record_file_reader_mode(cfg.mode, &mut reader, &mut counts)?;
    }
    black_box(counts.checksum);
    Ok(counts)
}

fn configure_reader_filters(reader: &mut FileReader, cfg: &ReadConfig<'_>) -> Result<()> {
    reader.flush_matches();
    for filter in parse_explorer_filters(cfg.explorer_filters)? {
        for value in filter.values {
            let mut payload = Vec::with_capacity(filter.field.len() + 1 + value.len());
            payload.extend_from_slice(&filter.field);
            payload.push(b'=');
            payload.extend_from_slice(&value);
            reader.add_match(&payload);
        }
    }
    Ok(())
}

fn realtime_in_explorer_range(cfg: &ReadConfig<'_>, realtime: u64) -> bool {
    !cfg.explorer_after_usec
        .is_some_and(|after| realtime < after)
        && !cfg
            .explorer_before_usec
            .is_some_and(|before| realtime > before)
}

fn record_facet_scan_payload(
    payload: &[u8],
    facet_set: &std::collections::HashSet<Vec<u8>>,
    facets: &mut HashMap<Vec<u8>, HashMap<Vec<u8>, u64>>,
    counts: &mut Counts,
) {
    counts.add_payload(black_box(payload));
    let Some((field, value)) = split_benchmark_payload(payload) else {
        return;
    };
    if facet_set.contains(field) {
        increment_facet_count(facets, field, value);
    }
}

fn read_sdk_facet_scan_file(path: &Path, cfg: &ReadConfig<'_>) -> Result<Counts> {
    let options = reader_options(cfg.bounds, cfg.strategy, cfg.window_size)?;
    let mut reader = FileReader::open_with_options(path, options).with_context(|| {
        format!(
            "failed to open SDK facet-scan file reader for {}",
            path.display()
        )
    })?;
    configure_reader_filters(&mut reader, cfg)?;
    match cfg.direction {
        Direction::Forward => reader.seek_head(),
        Direction::Backward => reader.seek_tail(),
    }

    let facet_fields = defaulted_facets(cfg);
    let facet_set: std::collections::HashSet<Vec<u8>> = facet_fields.iter().cloned().collect();
    let mut facets: HashMap<Vec<u8>, HashMap<Vec<u8>, u64>> = HashMap::new();
    let mut counts = Counts::default();

    while step_file_reader(&mut reader, cfg.direction)? {
        let realtime = reader.get_realtime_usec()?;
        if !realtime_in_explorer_range(cfg, realtime) {
            continue;
        }
        counts.add_record_marker(realtime);
        reader.visit_entry_payloads(|payload| {
            record_facet_scan_payload(payload, &facet_set, &mut facets, &mut counts);
            Ok(())
        })?;
    }

    counts
        .extra
        .insert("facet_summary".to_string(), facet_summary(&facets));
    black_box(counts.checksum);
    Ok(counts)
}

fn read_explorer_query_file(path: &Path, cfg: &ReadConfig<'_>) -> Result<Counts> {
    let options = reader_options(cfg.bounds, cfg.strategy, cfg.window_size)?;
    let mut reader = FileReader::open_with_options(path, options)
        .with_context(|| format!("failed to open explorer file reader for {}", path.display()))?;
    let query = explorer_query_from_config(cfg)?;
    let result = reader.explore_with_strategy(&query, cfg.explorer_strategy)?;

    let logical_records = result
        .stats
        .rows_examined
        .max(result.stats.facet_rows_matched)
        .max(result.stats.rows_matched)
        .max(result.stats.histogram_updates);
    let mut counts = Counts {
        records: logical_records,
        fields: result.stats.data_refs_seen,
        ..Counts::default()
    };
    for row in &result.rows {
        counts.checksum = counts.checksum.rotate_left(7) ^ row.realtime_usec;
        for payload in &row.payloads {
            counts.bytes = counts.bytes.saturating_add(payload.len() as u64);
            counts.checksum = checksum_payload(counts.checksum, black_box(payload));
        }
    }
    let facet_summary = facet_summary(&result.facets);
    let histogram_summary = result.histogram.as_ref().map(|histogram| {
        let mut checksum = checksum_bytes(0, &histogram.field);
        let mut value_updates = 0u64;
        for bucket in &histogram.buckets {
            checksum ^= bucket.start_realtime_usec.rotate_left(13);
            checksum ^= bucket.end_realtime_usec.rotate_left(17);
            let mut values: Vec<_> = bucket.values.iter().collect();
            values.sort_by(|(left, _), (right, _)| left.cmp(right));
            for (value, count) in values {
                checksum = checksum_bytes(checksum, value) ^ count;
                value_updates = value_updates.saturating_add(*count);
            }
        }
        json!({
            "field_checksum": checksum_bytes(0, &histogram.field),
            "buckets": histogram.buckets.len(),
            "value_updates": value_updates,
            "histogram_checksum": checksum,
        })
    });

    counts
        .extra
        .insert("facet_summary".to_string(), facet_summary);
    if let Some(comparison) = &result.comparison {
        counts.extra.insert(
            "explorer_comparison".to_string(),
            json!({
                "traversal_duration_ns": u64::try_from(comparison.traversal_duration.as_nanos()).unwrap_or(u64::MAX),
                "index_duration_ns": u64::try_from(comparison.index_duration.as_nanos()).unwrap_or(u64::MAX),
                "traversal_stats": &comparison.traversal_stats,
                "index_stats": &comparison.index_stats,
            }),
        );
    }
    counts.extra.insert(
        "explorer_stats".to_string(),
        serde_json::to_value(&result.stats).unwrap_or_else(|_| json!({})),
    );
    if let Some(summary) = histogram_summary {
        counts
            .extra
            .insert("histogram_summary".to_string(), summary);
    }
    black_box(counts.checksum);
    Ok(counts)
}

fn step_file_reader(reader: &mut FileReader, direction: Direction) -> Result<bool> {
    Ok(match direction {
        Direction::Forward => reader.next(),
        Direction::Backward => reader.previous(),
    }?)
}

fn record_file_reader_mode(mode: &str, reader: &mut FileReader, counts: &mut Counts) -> Result<()> {
    match mode {
        "sdk-entry" => {
            let entry = reader.get_entry()?;
            counts.add_record_marker(entry.realtime);
            for payload in &entry.payloads {
                counts.add_payload(black_box(payload));
            }
            Ok(())
        }
        "sdk-payloads" => record_payload_visitor(reader.get_realtime_usec()?, counts, |visitor| {
            reader.visit_entry_payloads(visitor)
        }),
        other => Err(anyhow!("invalid SDK file mode: {other}")),
    }
}

fn read_sdk_directory(inputs: &[PathBuf], cfg: &ReadConfig<'_>) -> Result<Counts> {
    let options = reader_options(cfg.bounds, cfg.strategy, cfg.window_size)?;
    let mut reader = match cfg.surface {
        "directory" => {
            if inputs.len() != 1 {
                return Err(anyhow!("directory surface requires exactly one --input"));
            }
            DirectoryReader::open_with_options(&inputs[0], options)?
        }
        "open-files" => DirectoryReader::open_files_with_options(inputs.iter(), options)?,
        other => return Err(anyhow!("invalid SDK directory surface: {other}")),
    };
    match cfg.direction {
        Direction::Forward => reader.seek_head(),
        Direction::Backward => reader.seek_tail(),
    }
    let mut counts = Counts::default();
    while step_directory_reader(&mut reader, cfg.direction)? {
        record_directory_reader_mode(cfg.mode, &mut reader, &mut counts)?;
    }
    black_box(counts.checksum);
    Ok(counts)
}

fn step_directory_reader(reader: &mut DirectoryReader, direction: Direction) -> Result<bool> {
    Ok(match direction {
        Direction::Forward => reader.next(),
        Direction::Backward => reader.previous(),
    }?)
}

fn record_directory_reader_mode(
    mode: &str,
    reader: &mut DirectoryReader,
    counts: &mut Counts,
) -> Result<()> {
    match mode {
        "sdk-entry" => {
            let entry = reader.get_entry()?;
            counts.add_record_marker(entry.realtime);
            for payload in &entry.payloads {
                counts.add_payload(black_box(payload));
            }
            Ok(())
        }
        "sdk-payloads" => record_payload_visitor(reader.get_realtime_usec()?, counts, |visitor| {
            reader.visit_entry_payloads(visitor)
        }),
        other => Err(anyhow!("invalid SDK directory mode: {other}")),
    }
}

fn record_payload_visitor<F>(realtime: u64, counts: &mut Counts, visit: F) -> Result<()>
where
    F: FnOnce(&mut dyn FnMut(&[u8]) -> journal::Result<()>) -> journal::Result<()>,
{
    counts.add_record_marker(realtime);
    visit(&mut |payload| {
        counts.add_payload(black_box(payload));
        Ok(())
    })?;
    Ok(())
}

fn read_facade(inputs: &[PathBuf], cfg: &ReadConfig<'_>) -> Result<Counts> {
    let options = reader_options(cfg.bounds, cfg.strategy, cfg.window_size)?;
    let owned_paths = utf8_input_paths(inputs)?;
    let borrowed_paths = borrowed_input_paths(&owned_paths);
    let mut journal = open_facade_reader(&borrowed_paths, cfg.surface, options)?;
    seek_facade_reader(&mut journal, cfg.direction);

    let mut counts = Counts::default();
    while step_facade_reader(&mut journal, cfg.direction)? {
        record_facade_mode(cfg.mode, &mut journal, &mut counts)?;
    }
    black_box(counts.checksum);
    Ok(counts)
}

fn utf8_input_paths(inputs: &[PathBuf]) -> Result<Vec<String>> {
    let mut owned_paths = Vec::with_capacity(inputs.len());
    for input in inputs {
        let path = input
            .to_str()
            .ok_or_else(|| anyhow!("input path is not UTF-8: {}", input.display()))?;
        owned_paths.push(path.to_string());
    }
    Ok(owned_paths)
}

fn borrowed_input_paths(owned_paths: &[String]) -> Vec<&str> {
    owned_paths.iter().map(String::as_str).collect()
}

fn open_facade_reader(
    paths: &[&str],
    surface: &str,
    options: ReaderOptions,
) -> Result<journal::SdJournal> {
    match surface {
        "file" | "open-files" => Ok(SdJournalOpenFilesWithOptions(paths, 0, options)?),
        "directory" => {
            if paths.len() != 1 {
                return Err(anyhow!("directory surface requires exactly one --input"));
            }
            Ok(SdJournalOpenDirectoryWithOptions(paths[0], 0, options)?)
        }
        other => Err(anyhow!("invalid facade surface: {other}")),
    }
}

fn seek_facade_reader(journal: &mut journal::SdJournal, direction: Direction) {
    if direction == Direction::Backward {
        journal.seek_tail();
    } else {
        journal.seek_head();
    }
}

fn step_facade_reader(journal: &mut journal::SdJournal, direction: Direction) -> Result<bool> {
    let advanced = match direction {
        Direction::Forward => SdJournalNext(journal)?,
        Direction::Backward => journal.previous()?,
    };
    Ok(advanced != 0)
}

fn record_facade_mode(
    mode: &str,
    journal: &mut journal::SdJournal,
    counts: &mut Counts,
) -> Result<()> {
    match mode {
        "facade-next" => {
            counts.add_record_marker(journal.get_realtime_usec()?);
            Ok(())
        }
        "facade-data" => {
            counts.add_record_marker(journal.get_realtime_usec()?);
            SdJournalRestartData(journal)?;
            while let Some(payload) = SdJournalEnumerateAvailableData(journal)? {
                counts.add_payload(black_box(payload));
            }
            Ok(())
        }
        other => Err(anyhow!("invalid facade mode: {other}")),
    }
}

fn run(args: &Args) -> Result<(Counts, f64, Value, Value)> {
    let cfg = ReadConfig {
        mode: &args.mode,
        surface: &args.surface,
        direction: parse_direction(&args.direction)?,
        bounds: &args.bounds,
        strategy: parse_mmap_strategy(&args.mmap_strategy)?,
        window_size: args.window_size,
        explorer_facets: &args.explorer_facets,
        explorer_filters: &args.explorer_filters,
        explorer_histogram: args.explorer_histogram.as_deref(),
        explorer_limit: args.explorer_limit,
        explorer_fts_patterns: &args.explorer_fts_patterns,
        explorer_field_mode: parse_explorer_field_mode(&args.explorer_field_mode)?,
        explorer_use_source_realtime: args.explorer_use_source_realtime,
        explorer_strategy: parse_explorer_strategy(&args.explorer_strategy)?,
        explorer_after_usec: args.explorer_after_usec,
        explorer_before_usec: args.explorer_before_usec,
    };
    let status_before = process_status_kb();
    let started = Instant::now();
    let counts = dispatch_read(&args.inputs, &cfg)?;
    let elapsed_seconds = started.elapsed().as_secs_f64();
    let status_after = process_status_kb();
    Ok((counts, elapsed_seconds, status_before, status_after))
}

fn dispatch_read(inputs: &[PathBuf], cfg: &ReadConfig<'_>) -> Result<Counts> {
    match cfg.mode {
        "core-next" | "core-offsets" | "core-payloads" => {
            require_single_file_input(inputs, cfg.surface, "core modes")?;
            read_core(&inputs[0], cfg)
        }
        "core-compressed-stats" => {
            require_single_file_input(inputs, cfg.surface, "core modes")?;
            read_core(&inputs[0], cfg)
        }
        "sdk-facet-scan" => {
            require_single_file_input(inputs, cfg.surface, "SDK facet scan mode")?;
            read_sdk_facet_scan_file(&inputs[0], cfg)
        }
        "explorer-query" => {
            require_single_file_input(inputs, cfg.surface, "explorer query mode")?;
            read_explorer_query_file(&inputs[0], cfg)
        }
        "sdk-entry" | "sdk-payloads" => dispatch_sdk_read(inputs, cfg),
        "facade-next" | "facade-data" => read_facade(inputs, cfg),
        other => Err(anyhow!("invalid --mode: {other}")),
    }
}

fn dispatch_sdk_read(inputs: &[PathBuf], cfg: &ReadConfig<'_>) -> Result<Counts> {
    match cfg.surface {
        "file" => {
            require_single_file_input(inputs, cfg.surface, "file surface")?;
            read_sdk_file(&inputs[0], cfg)
        }
        "directory" | "open-files" => read_sdk_directory(inputs, cfg),
        other => Err(anyhow!("invalid --surface for SDK mode: {other}")),
    }
}

fn require_single_file_input(inputs: &[PathBuf], surface: &str, context: &str) -> Result<()> {
    if surface != "file" || inputs.len() != 1 {
        return Err(anyhow!("{context} require --surface file and one --input"));
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let (counts, read_seconds, status_before, status_after) = run(&args)?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "language": "rust",
            "surface": args.surface,
            "mode": args.mode,
            "direction": args.direction,
            "records": counts.records,
            "fields": counts.fields,
            "bytes": counts.bytes,
            "checksum": counts.checksum,
            "extra": counts.extra,
            "read_seconds": read_seconds,
            "read_rows_per_second": if read_seconds > 0.0 { counts.records as f64 / read_seconds } else { 0.0 },
            "read_fields_per_second": if read_seconds > 0.0 { counts.fields as f64 / read_seconds } else { 0.0 },
            "read_bytes_per_second": if read_seconds > 0.0 { counts.bytes as f64 / read_seconds } else { 0.0 },
            "inputs": args.inputs,
            "window_size": args.window_size,
            "bounds": args.bounds,
            "mmap_strategy": args.mmap_strategy,
            "explorer": {
                "facets": args.explorer_facets,
                "filters": args.explorer_filters,
                "histogram": args.explorer_histogram,
                "limit": args.explorer_limit,
                "fts_patterns": args.explorer_fts_patterns,
                "field_mode": args.explorer_field_mode,
                "use_source_realtime": args.explorer_use_source_realtime,
                "strategy": args.explorer_strategy,
                "after_usec": args.explorer_after_usec,
                "before_usec": args.explorer_before_usec,
            },
            "timer_excludes": ["fixture generation", "process startup", "external verification"],
            "process_status_before": status_before,
            "process_status_after": status_after,
            "errors": [],
        }))?
    );
    Ok(())
}
