use anyhow::{Context, Result, anyhow};
use clap::Parser;
use journal::{
    Direction, DirectoryReader, FileReader, JournalFile, JournalReader, Mmap, ReaderBounds,
    ReaderOptions, SdJournalEnumerateAvailableData, SdJournalNext,
    SdJournalOpenDirectoryWithOptions, SdJournalOpenFilesWithOptions, SdJournalRestartData,
};
use journal_core::file::{ExperimentalMmapStrategy, HashableObject};
use serde_json::{Value, json};
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
}

#[derive(Default)]
struct Counts {
    records: u64,
    fields: u64,
    bytes: u64,
    checksum: u64,
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
        )?;
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
) -> Result<()> {
    match mode {
        "core-next" => Ok(()),
        "core-offsets" => record_core_offsets(file, reader, counts, offsets),
        "core-payloads" => record_core_payloads(file, reader, counts, offsets, decompressed),
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
            "read_seconds": read_seconds,
            "read_rows_per_second": if read_seconds > 0.0 { counts.records as f64 / read_seconds } else { 0.0 },
            "read_fields_per_second": if read_seconds > 0.0 { counts.fields as f64 / read_seconds } else { 0.0 },
            "read_bytes_per_second": if read_seconds > 0.0 { counts.bytes as f64 / read_seconds } else { 0.0 },
            "inputs": args.inputs,
            "window_size": args.window_size,
            "bounds": args.bounds,
            "mmap_strategy": args.mmap_strategy,
            "timer_excludes": ["fixture generation", "process startup", "external verification"],
            "process_status_before": status_before,
            "process_status_after": status_after,
            "errors": [],
        }))?
    );
    Ok(())
}
