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

fn read_core(
    path: &Path,
    mode: &str,
    direction: Direction,
    window_size: u64,
    bounds: &str,
    strategy: ExperimentalMmapStrategy,
) -> Result<Counts> {
    let file = open_core(path, window_size, bounds, strategy)?;
    let mut reader = JournalReader::default();
    reader.set_location(match direction {
        Direction::Forward => journal::Location::Head,
        Direction::Backward => journal::Location::Tail,
    });
    let mut counts = Counts::default();
    let mut offsets = Vec::new();
    let mut decompressed = Vec::new();

    loop {
        if !reader.step(&file, direction)? {
            break;
        }
        let realtime = reader.get_realtime_usec(&file)?;
        counts.add_record_marker(realtime);

        match mode {
            "core-next" => {}
            "core-offsets" => {
                offsets.clear();
                reader.entry_data_offsets(&file, &mut offsets)?;
                counts.fields = counts.fields.saturating_add(offsets.len() as u64);
                counts.checksum ^= offsets.len() as u64;
            }
            "core-payloads" => {
                offsets.clear();
                reader.entry_data_offsets(&file, &mut offsets)?;
                for offset in offsets.iter().copied() {
                    let data = file.data_ref(offset)?;
                    let payload = if data.is_compressed() {
                        decompressed.clear();
                        data.decompress(&mut decompressed)?;
                        decompressed.as_slice()
                    } else {
                        data.raw_payload()
                    };
                    counts.add_payload(black_box(payload));
                }
            }
            other => return Err(anyhow!("invalid core mode for file surface: {other}")),
        }
    }

    black_box(counts.checksum);
    Ok(counts)
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

fn read_sdk_file(
    path: &Path,
    mode: &str,
    direction: Direction,
    bounds: &str,
    strategy: ExperimentalMmapStrategy,
    window_size: u64,
) -> Result<Counts> {
    let options = reader_options(bounds, strategy, window_size)?;
    let mut reader = FileReader::open_with_options(path, options)
        .with_context(|| format!("failed to open SDK file reader for {}", path.display()))?;
    match direction {
        Direction::Forward => reader.seek_head(),
        Direction::Backward => reader.seek_tail(),
    }
    let mut counts = Counts::default();
    loop {
        let advanced = match direction {
            Direction::Forward => reader.next()?,
            Direction::Backward => reader.previous()?,
        };
        if !advanced {
            break;
        }
        match mode {
            "sdk-entry" => {
                let entry = reader.get_entry()?;
                counts.add_record_marker(entry.realtime);
                for payload in &entry.payloads {
                    counts.add_payload(black_box(payload));
                }
            }
            "sdk-payloads" => {
                counts.add_record_marker(reader.get_realtime_usec()?);
                reader.visit_entry_payloads(|payload| {
                    counts.add_payload(black_box(payload));
                    Ok(())
                })?;
            }
            other => return Err(anyhow!("invalid SDK file mode: {other}")),
        }
    }
    black_box(counts.checksum);
    Ok(counts)
}

fn read_sdk_directory(
    inputs: &[PathBuf],
    surface: &str,
    mode: &str,
    direction: Direction,
    bounds: &str,
    strategy: ExperimentalMmapStrategy,
    window_size: u64,
) -> Result<Counts> {
    let options = reader_options(bounds, strategy, window_size)?;
    let mut reader = match surface {
        "directory" => {
            if inputs.len() != 1 {
                return Err(anyhow!("directory surface requires exactly one --input"));
            }
            DirectoryReader::open_with_options(&inputs[0], options)?
        }
        "open-files" => DirectoryReader::open_files_with_options(inputs.iter(), options)?,
        other => return Err(anyhow!("invalid SDK directory surface: {other}")),
    };
    match direction {
        Direction::Forward => reader.seek_head(),
        Direction::Backward => reader.seek_tail(),
    }
    let mut counts = Counts::default();
    loop {
        let advanced = match direction {
            Direction::Forward => reader.next()?,
            Direction::Backward => reader.previous()?,
        };
        if !advanced {
            break;
        }
        match mode {
            "sdk-entry" => {
                let entry = reader.get_entry()?;
                counts.add_record_marker(entry.realtime);
                for payload in &entry.payloads {
                    counts.add_payload(black_box(payload));
                }
            }
            "sdk-payloads" => {
                counts.add_record_marker(reader.get_realtime_usec()?);
                reader.visit_entry_payloads(|payload| {
                    counts.add_payload(black_box(payload));
                    Ok(())
                })?;
            }
            other => return Err(anyhow!("invalid SDK directory mode: {other}")),
        }
    }
    black_box(counts.checksum);
    Ok(counts)
}

fn read_facade(
    inputs: &[PathBuf],
    surface: &str,
    mode: &str,
    direction: Direction,
    bounds: &str,
    strategy: ExperimentalMmapStrategy,
    window_size: u64,
) -> Result<Counts> {
    let options = reader_options(bounds, strategy, window_size)?;
    let mut owned_paths = Vec::with_capacity(inputs.len());
    for input in inputs {
        owned_paths.push(
            input
                .to_str()
                .ok_or_else(|| anyhow!("input path is not UTF-8: {}", input.display()))?
                .to_string(),
        );
    }
    let borrowed_paths: Vec<&str> = owned_paths.iter().map(String::as_str).collect();
    let mut journal = match surface {
        "file" | "open-files" => SdJournalOpenFilesWithOptions(&borrowed_paths, 0, options)?,
        "directory" => {
            if borrowed_paths.len() != 1 {
                return Err(anyhow!("directory surface requires exactly one --input"));
            }
            SdJournalOpenDirectoryWithOptions(borrowed_paths[0], 0, options)?
        }
        other => return Err(anyhow!("invalid facade surface: {other}")),
    };

    if direction == Direction::Backward {
        journal.seek_tail();
    } else {
        journal.seek_head();
    }

    let mut counts = Counts::default();
    loop {
        let advanced = match direction {
            Direction::Forward => SdJournalNext(&mut journal)?,
            Direction::Backward => journal.previous()?,
        };
        if advanced == 0 {
            break;
        }
        match mode {
            "facade-next" => {
                counts.add_record_marker(journal.get_realtime_usec()?);
            }
            "facade-data" => {
                let realtime = journal.get_realtime_usec()?;
                counts.add_record_marker(realtime);
                SdJournalRestartData(&mut journal)?;
                while let Some(payload) = SdJournalEnumerateAvailableData(&mut journal)? {
                    counts.add_payload(black_box(payload));
                }
            }
            other => return Err(anyhow!("invalid facade mode: {other}")),
        }
    }
    black_box(counts.checksum);
    Ok(counts)
}

fn run(args: &Args) -> Result<(Counts, f64, Value, Value)> {
    let direction = parse_direction(&args.direction)?;
    let mmap_strategy = parse_mmap_strategy(&args.mmap_strategy)?;
    let status_before = process_status_kb();
    let started = Instant::now();
    let counts = match args.mode.as_str() {
        "core-next" | "core-offsets" | "core-payloads" => {
            if args.surface != "file" || args.inputs.len() != 1 {
                return Err(anyhow!("core modes require --surface file and one --input"));
            }
            read_core(
                &args.inputs[0],
                &args.mode,
                direction,
                args.window_size,
                &args.bounds,
                mmap_strategy,
            )?
        }
        "sdk-entry" | "sdk-payloads" => match args.surface.as_str() {
            "file" => {
                if args.inputs.len() != 1 {
                    return Err(anyhow!("file surface requires exactly one --input"));
                }
                read_sdk_file(
                    &args.inputs[0],
                    &args.mode,
                    direction,
                    &args.bounds,
                    mmap_strategy,
                    args.window_size,
                )?
            }
            "directory" | "open-files" => read_sdk_directory(
                &args.inputs,
                &args.surface,
                &args.mode,
                direction,
                &args.bounds,
                mmap_strategy,
                args.window_size,
            )?,
            other => return Err(anyhow!("invalid --surface for SDK mode: {other}")),
        },
        "facade-next" | "facade-data" => read_facade(
            &args.inputs,
            &args.surface,
            &args.mode,
            direction,
            &args.bounds,
            mmap_strategy,
            args.window_size,
        )?,
        other => return Err(anyhow!("invalid --mode: {other}")),
    };
    let elapsed_seconds = started.elapsed().as_secs_f64();
    let status_after = process_status_kb();
    Ok((counts, elapsed_seconds, status_before, status_after))
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
