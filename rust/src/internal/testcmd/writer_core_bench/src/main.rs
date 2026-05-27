use anyhow::{Context, Result, anyhow};
use clap::Parser;
use journal_core::file::{
    Compression, DEFAULT_COMPRESS_THRESHOLD, JournalFile, JournalFileOptions, JournalState,
    JournalWriter, MmapMut,
};
use journal_registry::repository::File as RepositoryFile;
use serde_json::json;
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
    #[arg(long, default_value_t = DEFAULT_MAX_SIZE_BYTES)]
    max_size_bytes: u64,
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

fn make_rows(rows: usize) -> Vec<Vec<Vec<u8>>> {
    let fixed: Vec<Vec<u8>> = vec![
        b"TEST_ID=deterministic-ingestion-performance".to_vec(),
        b"PERF_PROFILE=mixed-cardinality-32-fields".to_vec(),
        b"HOST_CLASS=synthetic-edge".to_vec(),
        b"SOURCE_KIND=journal-sdk-benchmark".to_vec(),
    ];
    let mut low_values: Vec<Vec<Vec<u8>>> = Vec::with_capacity(12);
    for offset in 0..12 {
        let mut values = Vec::with_capacity(16);
        for value in 0..16 {
            values.push(format!("LOW_CARD_{offset:02}=low-{offset:02}-{value:02}").into_bytes());
        }
        low_values.push(values);
    }
    let mut medium_values: Vec<Vec<Vec<u8>>> = Vec::with_capacity(8);
    for offset in 0..8 {
        let mut values = Vec::with_capacity(2048);
        for value in 0..2048 {
            values.push(format!("MED_CARD_{offset:02}=medium-{offset:02}-{value:04}").into_bytes());
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
            fields.push(format!("HIGH_CARD_{offset:02}=high-{offset:02}-{row:06}").into_bytes());
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

fn create_writer(
    path: &Path,
    compact: bool,
    max_size_bytes: u64,
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
        .with_compact(compact);
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
            journal_file.release_writer_lock()?;
            Ok(output.to_path_buf())
        }
        "offline" => {
            journal_file.journal_header_mut().state = JournalState::Offline as u8;
            journal_file.sync()?;
            journal_file.release_writer_lock()?;
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
            journal_file.release_writer_lock()?;
            Ok(archive_path)
        }
        _ => Err(anyhow!("invalid final state: {final_state}")),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let compact = match args.format.as_str() {
        "compact" => true,
        "regular" => false,
        other => return Err(anyhow!("invalid --format: {other}")),
    };
    let output = absolute_path(&args.output)?;
    let _ = fs::remove_file(&output);

    let precompute_start = Instant::now();
    let rows = make_rows(args.rows);
    let precompute_seconds = precompute_start.elapsed().as_secs_f64();

    let (mut journal_file, mut writer, data_hash_buckets) =
        create_writer(&output, compact, args.max_size_bytes)?;
    let mut records = 0usize;
    let mut refs: Vec<&[u8]> = Vec::with_capacity(FIELDS_PER_ROW);

    let append_start = Instant::now();
    for (index, fields) in rows.iter().enumerate() {
        refs.clear();
        refs.extend(fields.iter().map(Vec::as_slice));
        writer.add_entry(
            &mut journal_file,
            &refs,
            BASE_REALTIME_USEC + index as u64 * 500,
            BASE_MONOTONIC_USEC + index as u64 * 50,
        )?;
        records += 1;
    }
    let append_seconds = append_start.elapsed().as_secs_f64();

    let close_start = Instant::now();
    let journal_path = finalize_journal_file(&mut journal_file, &output, &args.final_state)?;
    let close_seconds = close_start.elapsed().as_secs_f64();
    let journal_size_bytes = fs::metadata(&journal_path)?.len();

    println!(
        "{}",
        serde_json::to_string(&json!({
            "records": records,
            "fields_per_row": FIELDS_PER_ROW,
            "append_seconds": append_seconds,
            "append_rows_per_second": if append_seconds > 0.0 { records as f64 / append_seconds } else { 0.0 },
            "close_seconds": close_seconds,
            "total_writer_seconds": append_seconds + close_seconds,
            "precompute_seconds": precompute_seconds,
            "journal_size_bytes": journal_size_bytes,
            "journal_path": journal_path,
            "format": args.format,
            "compression": "none",
            "fss": false,
            "api_mode": "raw-payload",
            "data_hash_table_buckets": data_hash_buckets,
            "field_hash_table_buckets": FIELD_HASH_BUCKETS,
            "max_size_bytes": args.max_size_bytes,
            "append_timer_excludes": ["row generation", "writer creation", "final close/sync", "journal verification"],
            "final_state": args.final_state,
            "errors": [],
        }))?
    );
    Ok(())
}
