use anyhow::{Context, Result, anyhow};
use clap::Parser;
use journal_core::file::{
    Compression, DEFAULT_COMPRESS_THRESHOLD, EntryField, EntryWriteOptions,
    ExperimentalMmapStrategy, JournalFile, JournalFileOptions, JournalState, JournalWriter,
    MmapMut, StructuredField, WindowManagerStats,
};
use journal_log_writer::{
    Config as LogConfig, EntryTimestamps, Log, LogIdentityMode, RetentionPolicy, RotationPolicy,
};
use journal_registry::repository::File as RepositoryFile;
use journal_registry::{Origin, Source};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const BOOT_ID: &str = "0123456789abcdef0123456789abcdef";
const MACHINE_ID: &str = "fedcba9876543210fedcba9876543210";
const SEQNUM_ID: &str = "22222222222222222222222222222222";
const FILE_ID: &str = "33333333333333333333333333333333";
const BASE_REALTIME_USEC: u64 = 1_700_000_000_000_000;
const BASE_MONOTONIC_USEC: u64 = 50_000_000;
const FIELDS_PER_ROW: usize = 32;
const DEFAULT_MAX_SIZE_BYTES: u64 = 128 * 1024 * 1024;
const FIELD_HASH_BUCKETS: usize = 1023;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value_t = 100_000)]
    rows: usize,
    #[arg(long, default_value = "compact")]
    format: String,
    #[arg(long, default_value = "online")]
    final_state: String,
    #[arg(long, default_value = "direct")]
    surface: String,
    #[arg(long, default_value_t = DEFAULT_MAX_SIZE_BYTES)]
    max_size_bytes: u64,
    #[arg(long, default_value_t = DEFAULT_MAX_SIZE_BYTES)]
    rotation_max_size_bytes: u64,
    #[arg(long, default_value = "raw-payload")]
    api_mode: String,
    #[arg(long, default_value_t = false)]
    trusted_unique_payloads: bool,
    #[arg(long, default_value_t = 1)]
    live_publish_every_entries: u64,
    #[arg(long, hide = true)]
    live_publication: Option<String>,
    #[arg(long, default_value_t = 64)]
    live_publication_interval: u64,
    #[arg(long, default_value = "windowed")]
    mmap_strategy: String,
}

fn uuid(hex: &str) -> Result<uuid::Uuid> {
    uuid::Uuid::parse_str(hex).with_context(|| format!("invalid UUID: {hex}"))
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

#[derive(Clone)]
struct BenchField {
    raw: Vec<u8>,
    name: Vec<u8>,
    value: Vec<u8>,
}

impl BenchField {
    fn new(name: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> Self {
        let name = name.into();
        let value = value.into();
        let mut raw = Vec::with_capacity(name.len() + 1 + value.len());
        raw.extend_from_slice(&name);
        raw.push(b'=');
        raw.extend_from_slice(&value);
        Self { raw, name, value }
    }

    fn structured(&self) -> StructuredField<'_> {
        StructuredField::new(&self.name, &self.value)
    }
}

fn make_rows(rows: usize) -> Vec<Vec<BenchField>> {
    let fixed: Vec<BenchField> = vec![
        BenchField::new("TEST_ID", "deterministic-ingestion-performance"),
        BenchField::new("PERF_PROFILE", "mixed-cardinality-32-fields"),
        BenchField::new("HOST_CLASS", "synthetic-edge"),
        BenchField::new("SOURCE_KIND", "journal-sdk-benchmark"),
    ];
    let mut low_values: Vec<Vec<BenchField>> = Vec::with_capacity(12);
    for offset in 0..12 {
        let mut values = Vec::with_capacity(16);
        for value in 0..16 {
            values.push(BenchField::new(
                format!("LOW_CARD_{offset:02}"),
                format!("low-{offset:02}-{value:02}"),
            ));
        }
        low_values.push(values);
    }
    let mut medium_values: Vec<Vec<BenchField>> = Vec::with_capacity(8);
    for offset in 0..8 {
        let mut values = Vec::with_capacity(2048);
        for value in 0..2048 {
            values.push(BenchField::new(
                format!("MED_CARD_{offset:02}"),
                format!("medium-{offset:02}-{value:04}"),
            ));
        }
        medium_values.push(values);
    }

    let mut all = Vec::with_capacity(rows);
    for row in 0..rows {
        let mut fields = Vec::with_capacity(FIELDS_PER_ROW);
        fields.extend(fixed.iter().cloned());
        for offset in 0..12 {
            fields.push(low_values[offset][row % 16].clone());
        }
        for offset in 0..8 {
            fields.push(medium_values[offset][row % 2048].clone());
        }
        for offset in 0..8 {
            fields.push(BenchField::new(
                format!("HIGH_CARD_{offset:02}"),
                format!("high-{offset:02}-{row:06}"),
            ));
        }
        all.push(fields);
    }
    all
}

fn data_hash_buckets_for_max_size(max_size: u64) -> usize {
    // Keep this driver aligned with journal-core's helper and systemd's
    // max_size * 4 / 768 / 3 formula.
    let buckets = max_size / 576;
    buckets.max(2047).min(usize::MAX as u64) as usize
}

fn resolve_live_publish_every_entries(args: &Args) -> Result<u64> {
    let Some(name) = args.live_publication.as_deref() else {
        return Ok(args.live_publish_every_entries);
    };
    match name {
        "immediate" => Ok(1),
        "disabled" => Ok(0),
        "every-n" => {
            if args.live_publication_interval == 0 {
                Err(anyhow!("--live-publication-interval must be positive"))
            } else {
                Ok(args.live_publication_interval)
            }
        }
        other => Err(anyhow!("invalid --live-publication: {other}")),
    }
}

fn live_publication_name(every_entries: u64) -> String {
    match every_entries {
        0 => "disabled".to_string(),
        1 => "immediate".to_string(),
        n => format!("every-n:{n}"),
    }
}

fn parse_mmap_strategy(name: &str) -> Result<ExperimentalMmapStrategy> {
    match name {
        "windowed" => Ok(ExperimentalMmapStrategy::Windowed),
        "whole-file" => Ok(ExperimentalMmapStrategy::WholeFile),
        other => Err(anyhow!("invalid --mmap-strategy: {other}")),
    }
}

fn mmap_strategy_name(strategy: ExperimentalMmapStrategy) -> &'static str {
    match strategy {
        ExperimentalMmapStrategy::Windowed => "windowed",
        ExperimentalMmapStrategy::WholeFile => "whole-file",
    }
}

fn mmap_stats_json(stats: WindowManagerStats) -> Value {
    json!({
        "strategy": mmap_strategy_name(stats.strategy),
        "file_size": stats.file_size,
        "window_count": stats.window_count,
        "row_pin_count": stats.row_pin_count,
        "row_pin_limit": stats.row_pin_limit,
        "row_overflow_object_count": stats.row_overflow_object_count,
        "current_mapped_bytes": stats.current_mapped_bytes,
        "max_mapped_bytes": stats.max_mapped_bytes,
        "map_count": stats.map_count,
        "remap_count": stats.remap_count,
        "eviction_count": stats.eviction_count,
    })
}

fn process_status_kb() -> Value {
    let Ok(status) = fs::read_to_string("/proc/self/status") else {
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

fn create_writer(
    path: &Path,
    compact: bool,
    max_size_bytes: u64,
    mmap_strategy: ExperimentalMmapStrategy,
) -> Result<(JournalFile<MmapMut>, JournalWriter, usize)> {
    let path = absolute_path(path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let repo_file = RepositoryFile::from_path(&path)
        .ok_or_else(|| anyhow!("journal path must be absolute and end in .journal"))?;
    let boot_id = uuid(BOOT_ID)?;
    let data_hash_buckets = data_hash_buckets_for_max_size(max_size_bytes);
    let options = JournalFileOptions::new(uuid(MACHINE_ID)?, boot_id, uuid(SEQNUM_ID)?)
        .with_file_id(uuid(FILE_ID)?)
        .with_window_size(8 * 1024 * 1024)
        .with_data_hash_table_buckets(data_hash_buckets)
        .with_field_hash_table_buckets(FIELD_HASH_BUCKETS)
        .with_keyed_hash(true)
        .with_compression(Compression::None)
        .with_compress_threshold(DEFAULT_COMPRESS_THRESHOLD)
        .with_compact(compact)
        .with_experimental_mmap_strategy(mmap_strategy);
    let mut journal_file = JournalFile::<MmapMut>::create(&repo_file, options)?;
    let writer = JournalWriter::new_with_compression(
        &mut journal_file,
        1,
        boot_id,
        Compression::None,
        DEFAULT_COMPRESS_THRESHOLD,
    )?;
    Ok((journal_file, writer, data_hash_buckets))
}

fn archive_path_for(output: &Path) -> PathBuf {
    let file_name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("system.journal");
    let prefix = file_name.strip_suffix(".journal").unwrap_or(file_name);
    output.with_file_name(format!(
        "{prefix}@{SEQNUM_ID}-0000000000000001-{BASE_REALTIME_USEC:016x}.journal"
    ))
}

fn finalize_journal_file(
    journal_file: &mut JournalFile<MmapMut>,
    output: &Path,
    final_state: &str,
) -> Result<PathBuf> {
    match final_state {
        "online" => {
            journal_file.journal_header_mut().state = JournalState::Online as u8;
            journal_file.sync()?;
            Ok(output.to_path_buf())
        }
        "offline" => {
            journal_file.journal_header_mut().state = JournalState::Offline as u8;
            journal_file.sync()?;
            Ok(output.to_path_buf())
        }
        "archived" => {
            let archive_path = archive_path_for(output);
            match fs::remove_file(&archive_path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
            fs::rename(output, &archive_path)?;
            journal_file.journal_header_mut().state = JournalState::Archived as u8;
            journal_file.sync()?;
            Ok(archive_path)
        }
        _ => Err(anyhow!("invalid final state: {final_state}")),
    }
}

fn collect_journal_files(root: &Path) -> Result<(Vec<PathBuf>, u64)> {
    fn visit(path: &Path, files: &mut Vec<PathBuf>, total: &mut u64) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                visit(&path, files, total)?;
            } else if path.extension().and_then(|value| value.to_str()) == Some("journal") {
                *total += metadata.len();
                files.push(path);
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    let mut total = 0;
    visit(root, &mut files, &mut total)?;
    Ok((files, total))
}

struct DirectoryRunConfig<'a> {
    output: &'a Path,
    compact: bool,
    api_mode: &'a str,
    max_size_bytes: u64,
    rotation_max_size_bytes: u64,
    live_publish_every_entries: u64,
}

fn run_directory(
    rows: &[Vec<BenchField>],
    cfg: &DirectoryRunConfig<'_>,
    precompute_seconds: f64,
) -> Result<Value> {
    match fs::remove_dir_all(cfg.output) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }
    let mut log = open_directory_log(cfg)?;
    let append_start = Instant::now();
    let records = append_directory_rows(&mut log, rows, cfg.api_mode)?;
    let append_seconds = append_start.elapsed().as_secs_f64();
    let journal_directory = log.journal_directory().to_path_buf();

    let close_start = Instant::now();
    log.close()?;
    let close_seconds = close_start.elapsed().as_secs_f64();

    let (journal_files, journal_size_bytes) = collect_journal_files(cfg.output)?;
    Ok(directory_report(
        cfg,
        records,
        precompute_seconds,
        append_seconds,
        close_seconds,
        journal_directory,
        journal_files,
        journal_size_bytes,
    ))
}

fn open_directory_log(cfg: &DirectoryRunConfig<'_>) -> Result<Log> {
    let origin = Origin {
        machine_id: Some(uuid(MACHINE_ID)?),
        namespace: None,
        source: Source::System,
    };
    let rotation = RotationPolicy::default().with_size_of_journal_file(cfg.rotation_max_size_bytes);
    let config = LogConfig::new(origin, rotation, RetentionPolicy::default())
        .with_compact(cfg.compact)
        .with_identity_mode(LogIdentityMode::Strict)
        .with_boot_id(uuid(BOOT_ID)?)
        .with_live_publish_every_entries(cfg.live_publish_every_entries);
    Ok(Log::new(cfg.output, config)?)
}

fn append_directory_rows(log: &mut Log, rows: &[Vec<BenchField>], api_mode: &str) -> Result<usize> {
    let mut records = 0usize;
    let mut entry_fields: Vec<&[u8]> = Vec::with_capacity(FIELDS_PER_ROW);
    let mut structured_refs: Vec<StructuredField<'_>> = Vec::with_capacity(FIELDS_PER_ROW);
    let structured = api_mode == "structured-field";

    for (index, fields) in rows.iter().enumerate() {
        let timestamps = EntryTimestamps {
            source_realtime_usec: None,
            entry_realtime_usec: Some(BASE_REALTIME_USEC + index as u64 * 500),
            entry_monotonic_usec: Some(BASE_MONOTONIC_USEC + index as u64 * 50),
        };
        append_directory_row(
            log,
            fields,
            timestamps,
            structured,
            &mut entry_fields,
            &mut structured_refs,
        )?;
        records += 1;
    }
    Ok(records)
}

fn append_directory_row<'a>(
    log: &mut Log,
    fields: &'a [BenchField],
    timestamps: EntryTimestamps,
    structured: bool,
    entry_fields: &mut Vec<&'a [u8]>,
    structured_refs: &mut Vec<StructuredField<'a>>,
) -> Result<()> {
    if structured {
        structured_refs.clear();
        structured_refs.extend(fields.iter().map(BenchField::structured));
        log.write_fields_with_options(&*structured_refs, timestamps, EntryWriteOptions::default())?;
    } else {
        entry_fields.clear();
        entry_fields.extend(fields.iter().map(|field| field.raw.as_slice()));
        log.write_entry_with_timestamps(&*entry_fields, timestamps)?;
    }
    Ok(())
}

fn directory_report(
    cfg: &DirectoryRunConfig<'_>,
    records: usize,
    precompute_seconds: f64,
    append_seconds: f64,
    close_seconds: f64,
    journal_directory: PathBuf,
    journal_files: Vec<PathBuf>,
    journal_size_bytes: u64,
) -> Value {
    json!({
        "records": records,
        "fields_per_row": FIELDS_PER_ROW,
        "surface": "directory",
        "append_seconds": append_seconds,
        "append_rows_per_second": if append_seconds > 0.0 { records as f64 / append_seconds } else { 0.0 },
        "close_seconds": close_seconds,
        "total_writer_seconds": append_seconds + close_seconds,
        "journal_size_bytes": journal_size_bytes,
        "journal_path": journal_directory,
        "journal_directory": journal_directory,
        "journal_files": journal_files,
        "format": if cfg.compact { "compact" } else { "regular" },
        "compression": "none",
        "fss": false,
        "api_mode": cfg.api_mode,
        "trusted_unique_payloads": false,
        "live_publication": live_publication_name(cfg.live_publish_every_entries),
        "live_publish_every_entries": cfg.live_publish_every_entries,
        "mmap_strategy": "windowed",
        "data_hash_table_buckets": data_hash_buckets_for_max_size(cfg.max_size_bytes),
        "field_hash_table_buckets": FIELD_HASH_BUCKETS,
        "max_size_bytes": cfg.max_size_bytes,
        "rotation_max_size_bytes": cfg.rotation_max_size_bytes,
        "precompute_seconds": precompute_seconds,
        "append_timer_excludes": ["row generation", "writer creation", "final close/sync", "journal verification"],
        "final_state": "archived",
        "errors": [],
    })
}

fn main() -> Result<()> {
    let args = Args::parse();
    let result = run(args)?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

struct BenchConfig {
    output: PathBuf,
    compact: bool,
    structured: bool,
    live_publish_every_entries: u64,
    mmap_strategy: ExperimentalMmapStrategy,
}

fn run(args: Args) -> Result<Value> {
    let cfg = parse_bench_config(&args)?;
    let precompute_start = Instant::now();
    let rows = make_rows(args.rows);
    let precompute_seconds = precompute_start.elapsed().as_secs_f64();

    if args.surface == "directory" {
        return run_directory(
            &rows,
            &DirectoryRunConfig {
                output: &cfg.output,
                compact: cfg.compact,
                api_mode: &args.api_mode,
                max_size_bytes: args.max_size_bytes,
                rotation_max_size_bytes: args.rotation_max_size_bytes,
                live_publish_every_entries: cfg.live_publish_every_entries,
            },
            precompute_seconds,
        );
    }

    run_direct(&args, &cfg, &rows, precompute_seconds)
}

fn parse_bench_config(args: &Args) -> Result<BenchConfig> {
    let compact = match args.format.as_str() {
        "compact" => true,
        "regular" => false,
        other => return Err(anyhow!("invalid --format: {other}")),
    };
    let structured = match args.api_mode.as_str() {
        "raw-payload" => false,
        "structured-field" => true,
        other => return Err(anyhow!("invalid --api-mode: {other}")),
    };
    if args.surface != "direct" && args.surface != "directory" {
        return Err(anyhow!("invalid --surface: {}", args.surface));
    }
    let output = absolute_path(&args.output)?;
    let _ = fs::remove_file(&output);
    Ok(BenchConfig {
        output,
        compact,
        structured,
        live_publish_every_entries: resolve_live_publish_every_entries(args)?,
        mmap_strategy: parse_mmap_strategy(&args.mmap_strategy)?,
    })
}

fn run_direct(
    args: &Args,
    cfg: &BenchConfig,
    rows: &[Vec<BenchField>],
    precompute_seconds: f64,
) -> Result<Value> {
    let (mut journal_file, mut writer, data_hash_buckets) = create_writer(
        &cfg.output,
        cfg.compact,
        args.max_size_bytes,
        cfg.mmap_strategy,
    )?;
    writer.set_live_publish_every_entries(cfg.live_publish_every_entries);
    let mmap_stats_before_append = journal_file.mmap_stats().map(mmap_stats_json)?;
    let process_status_before_append = process_status_kb();

    let append_start = Instant::now();
    let records = append_direct_rows(args, cfg.structured, rows, &mut journal_file, &mut writer)?;
    let append_seconds = append_start.elapsed().as_secs_f64();
    let mmap_stats_after_append = journal_file.mmap_stats().map(mmap_stats_json)?;
    let process_status_after_append = process_status_kb();

    let close_start = Instant::now();
    let journal_path = finalize_journal_file(&mut journal_file, &cfg.output, &args.final_state)?;
    let close_seconds = close_start.elapsed().as_secs_f64();
    let mmap_stats_after_close = journal_file.mmap_stats().map(mmap_stats_json)?;
    let process_status_after_close = process_status_kb();
    let journal_size_bytes = fs::metadata(&journal_path)?.len();

    Ok(direct_report(DirectReport {
        args,
        cfg,
        records,
        data_hash_buckets,
        precompute_seconds,
        append_seconds,
        close_seconds,
        journal_size_bytes,
        journal_path,
        mmap_stats_before_append,
        mmap_stats_after_append,
        mmap_stats_after_close,
        process_status_before_append,
        process_status_after_append,
        process_status_after_close,
    }))
}

fn append_direct_rows(
    args: &Args,
    structured: bool,
    rows: &[Vec<BenchField>],
    journal_file: &mut JournalFile<MmapMut>,
    writer: &mut JournalWriter,
) -> Result<usize> {
    let mut records = 0usize;
    let mut entry_fields: Vec<EntryField<'_>> = Vec::with_capacity(FIELDS_PER_ROW);
    let mut structured_refs: Vec<StructuredField<'_>> = Vec::with_capacity(FIELDS_PER_ROW);
    let write_options =
        EntryWriteOptions::default().trusted_unique_payloads(args.trusted_unique_payloads);
    for (index, fields) in rows.iter().enumerate() {
        append_direct_row(
            fields,
            index,
            structured,
            journal_file,
            writer,
            write_options,
            &mut entry_fields,
            &mut structured_refs,
        )?;
        records += 1;
    }
    Ok(records)
}

fn append_direct_row<'a>(
    fields: &'a [BenchField],
    index: usize,
    structured: bool,
    journal_file: &mut JournalFile<MmapMut>,
    writer: &mut JournalWriter,
    write_options: EntryWriteOptions,
    entry_fields: &mut Vec<EntryField<'a>>,
    structured_refs: &mut Vec<StructuredField<'a>>,
) -> Result<()> {
    let realtime = BASE_REALTIME_USEC + index as u64 * 500;
    let monotonic = BASE_MONOTONIC_USEC + index as u64 * 50;
    if structured {
        structured_refs.clear();
        structured_refs.extend(fields.iter().map(BenchField::structured));
        writer.add_entry_structured_with_options(
            journal_file,
            &*structured_refs,
            realtime,
            monotonic,
            write_options,
        )?;
    } else {
        entry_fields.clear();
        entry_fields.extend(
            fields
                .iter()
                .map(|field| EntryField::raw(field.raw.as_slice())),
        );
        writer.add_entry_fields_with_options(
            journal_file,
            entry_fields.iter().copied(),
            realtime,
            monotonic,
            write_options,
        )?;
    }
    Ok(())
}

struct DirectReport<'a> {
    args: &'a Args,
    cfg: &'a BenchConfig,
    records: usize,
    data_hash_buckets: usize,
    precompute_seconds: f64,
    append_seconds: f64,
    close_seconds: f64,
    journal_size_bytes: u64,
    journal_path: PathBuf,
    mmap_stats_before_append: Value,
    mmap_stats_after_append: Value,
    mmap_stats_after_close: Value,
    process_status_before_append: Value,
    process_status_after_append: Value,
    process_status_after_close: Value,
}

fn direct_report(report: DirectReport<'_>) -> Value {
    json!({
        "records": report.records,
        "fields_per_row": FIELDS_PER_ROW,
        "surface": "direct",
        "append_seconds": report.append_seconds,
        "append_rows_per_second": if report.append_seconds > 0.0 { report.records as f64 / report.append_seconds } else { 0.0 },
        "close_seconds": report.close_seconds,
        "total_writer_seconds": report.append_seconds + report.close_seconds,
        "precompute_seconds": report.precompute_seconds,
        "journal_size_bytes": report.journal_size_bytes,
        "journal_path": report.journal_path,
        "format": report.args.format,
        "compression": "none",
        "fss": false,
        "api_mode": report.args.api_mode,
        "trusted_unique_payloads": report.args.trusted_unique_payloads,
        "live_publication": live_publication_name(report.cfg.live_publish_every_entries),
        "live_publish_every_entries": report.cfg.live_publish_every_entries,
        "mmap_strategy": mmap_strategy_name(report.cfg.mmap_strategy),
        "mmap_stats_before_append": report.mmap_stats_before_append,
        "mmap_stats_after_append": report.mmap_stats_after_append,
        "mmap_stats_after_close": report.mmap_stats_after_close,
        "process_status_before_append": report.process_status_before_append,
        "process_status_after_append": report.process_status_after_append,
        "process_status_after_close": report.process_status_after_close,
        "data_hash_table_buckets": report.data_hash_buckets,
        "field_hash_table_buckets": FIELD_HASH_BUCKETS,
        "max_size_bytes": report.args.max_size_bytes,
        "append_timer_excludes": ["row generation", "writer creation", "final close/sync", "journal verification"],
        "final_state": report.args.final_state,
        "errors": [],
    })
}
