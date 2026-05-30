use anyhow::{Context, Result, anyhow};
use clap::Parser;
use journal::{ExperimentalMmapStrategy, FileReader, ReaderBounds, ReaderOptions};
use journal_core::file::{
    Compression, DEFAULT_COMPRESS_THRESHOLD, EntryField, EntryWriteOptions, FieldNamePolicy,
    JournalFile, JournalFileOptions, JournalState, JournalWriter, MmapMut,
};
use journal_registry::repository::File as RepositoryFile;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const MACHINE_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SEQNUM_ID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const FILE_ID: &str = "cccccccccccccccccccccccccccccccc";
const FALLBACK_BOOT_ID: &str = "dddddddddddddddddddddddddddddddd";
const DEFAULT_WINDOW_SIZE: u64 = 32 * 1024 * 1024;
const DEFAULT_MAX_SIZE_BYTES: u64 = 128 * 1024 * 1024;
const FIELD_HASH_BUCKETS: usize = 1023;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value = "regular")]
    format: String,
    #[arg(long, default_value = "none")]
    compression: String,
    #[arg(long, default_value_t = false)]
    fss: bool,
    #[arg(long, default_value_t = 1_000_000)]
    fss_interval_usec: u64,
    #[arg(long, default_value = "offline")]
    final_state: String,
    #[arg(long, default_value_t = DEFAULT_MAX_SIZE_BYTES)]
    max_size_bytes: u64,
    #[arg(long, default_value_t = DEFAULT_WINDOW_SIZE)]
    window_size: u64,
    #[arg(long, default_value_t = 1)]
    live_publish_every_entries: u64,
}

fn uuid(hex: &str) -> Result<uuid::Uuid> {
    uuid::Uuid::parse_str(hex).with_context(|| format!("invalid UUID {hex}"))
}

fn uuid_from_bytes(bytes: [u8; 16]) -> uuid::Uuid {
    uuid::Uuid::from_bytes(bytes)
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn data_hash_buckets_for_max_size(max_size: u64) -> usize {
    let buckets = max_size / 576;
    buckets.max(2047).min(usize::MAX as u64) as usize
}

fn systemd_fss_start_usec(realtime: u64, interval_usec: u64) -> u64 {
    if interval_usec == 0 {
        return realtime;
    }
    (realtime / interval_usec) * interval_usec
}

fn parse_compression(value: &str) -> Result<Compression> {
    match value {
        "none" => Ok(Compression::None),
        "zstd" => Ok(Compression::Zstd),
        "xz" => Ok(Compression::Xz),
        "lz4" => Ok(Compression::Lz4),
        other => Err(anyhow!("invalid --compression: {other}")),
    }
}

fn create_writer(
    output: &Path,
    boot_id: uuid::Uuid,
    head_seqnum: u64,
    compact: bool,
    compression: Compression,
    max_size_bytes: u64,
    fss: bool,
    fss_start_usec: u64,
    fss_interval_usec: u64,
) -> Result<(JournalFile<MmapMut>, JournalWriter)> {
    let output = absolute_path(output)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let repo_file = RepositoryFile::from_path(&output)
        .ok_or_else(|| anyhow!("journal output path must be absolute and end in .journal"))?;
    let mut options = JournalFileOptions::new(uuid(MACHINE_ID)?, boot_id, uuid(SEQNUM_ID)?)
        .with_file_id(uuid(FILE_ID)?)
        .with_window_size(8 * 1024 * 1024)
        .with_data_hash_table_buckets(data_hash_buckets_for_max_size(max_size_bytes))
        .with_field_hash_table_buckets(FIELD_HASH_BUCKETS)
        .with_keyed_hash(true)
        .with_compression(compression)
        .with_compress_threshold(DEFAULT_COMPRESS_THRESHOLD)
        .with_compact(compact)
        .with_experimental_mmap_strategy(ExperimentalMmapStrategy::Windowed);
    if fss {
        options = options.with_seal(journal_core::seal::SealOptions::new(
            [0u8; 12],
            fss_interval_usec,
            fss_start_usec.max(1),
        ));
    }
    let mut journal_file = JournalFile::<MmapMut>::create(&repo_file, options)?;
    let writer = JournalWriter::new_with_compression(
        &mut journal_file,
        head_seqnum.max(1),
        boot_id,
        compression,
        DEFAULT_COMPRESS_THRESHOLD,
    )?;
    Ok((journal_file, writer))
}

fn finalize(
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
            let archive_path = output.with_file_name("corpus-regenerated@bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-0000000000000001-0000000000000001.journal");
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
        other => Err(anyhow!("invalid --final-state: {other}")),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let compact = match args.format.as_str() {
        "regular" => false,
        "compact" => true,
        other => return Err(anyhow!("invalid --format: {other}")),
    };
    let compression = parse_compression(&args.compression)?;
    let input_metadata = fs::metadata(&args.input)?;
    let output = absolute_path(&args.output)?;
    match fs::remove_file(&output) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }

    let reader_options = ReaderOptions {
        window_size: args.window_size,
        bounds: ReaderBounds::Snapshot,
        mmap_strategy: ExperimentalMmapStrategy::Windowed,
    };
    let mut reader = FileReader::open_with_options(&args.input, reader_options)
        .with_context(|| format!("failed to open {}", args.input.display()))?;
    reader.seek_head();
    let has_first = reader.next()?;
    let first = if has_first {
        Some(reader.get_entry()?)
    } else {
        None
    };
    let boot_id = first
        .as_ref()
        .map(|entry| uuid_from_bytes(entry.boot_id))
        .unwrap_or(uuid(FALLBACK_BOOT_ID)?);
    let head_seqnum = first.as_ref().map(|entry| entry.seqnum).unwrap_or(1);
    let fss_start = first
        .as_ref()
        .map(|entry| systemd_fss_start_usec(entry.realtime, args.fss_interval_usec))
        .unwrap_or(args.fss_interval_usec);

    let (mut journal_file, mut writer) = create_writer(
        &output,
        boot_id,
        head_seqnum,
        compact,
        compression,
        args.max_size_bytes,
        args.fss,
        fss_start,
        args.fss_interval_usec,
    )?;
    writer.set_live_publish_every_entries(args.live_publish_every_entries);

    let write_options = EntryWriteOptions::default().field_name_policy(FieldNamePolicy::Raw);
    let append_start = Instant::now();
    let mut records = 0u64;
    let mut payloads = 0u64;
    let mut payload_bytes = 0u64;
    if let Some(entry) = first {
        let fields: Vec<EntryField<'_>> = entry
            .payloads
            .iter()
            .map(|payload| EntryField::raw(payload.as_slice()))
            .collect();
        writer.add_entry_fields_with_options(
            &mut journal_file,
            fields.iter().copied(),
            entry.realtime,
            entry.monotonic,
            write_options.seqnum(entry.seqnum),
        )?;
        records += 1;
        payloads += entry.payloads.len() as u64;
        payload_bytes += entry
            .payloads
            .iter()
            .map(|payload| payload.len() as u64)
            .sum::<u64>();
    }
    while reader.next()? {
        let entry = reader.get_entry()?;
        let fields: Vec<EntryField<'_>> = entry
            .payloads
            .iter()
            .map(|payload| EntryField::raw(payload.as_slice()))
            .collect();
        writer.add_entry_fields_with_options(
            &mut journal_file,
            fields.iter().copied(),
            entry.realtime,
            entry.monotonic,
            write_options.seqnum(entry.seqnum),
        )?;
        records += 1;
        payloads += entry.payloads.len() as u64;
        payload_bytes += entry
            .payloads
            .iter()
            .map(|payload| payload.len() as u64)
            .sum::<u64>();
    }
    let append_seconds = append_start.elapsed().as_secs_f64();
    let close_start = Instant::now();
    let final_path = finalize(&mut journal_file, &output, &args.final_state)?;
    let close_seconds = close_start.elapsed().as_secs_f64();
    let output_size = fs::metadata(&final_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    println!(
        "{}",
        serde_json::to_string(&json!({
            "driver": "rust",
            "records": records,
            "payloads": payloads,
            "payload_bytes": payload_bytes,
            "input_bytes": input_metadata.len(),
            "generated_bytes": output_size,
            "generated_path": final_path,
            "format": args.format,
            "compression": args.compression,
            "fss": args.fss,
            "fss_start_usec": if args.fss { Some(fss_start) } else { None },
            "fss_interval_usec": if args.fss { Some(args.fss_interval_usec) } else { None },
            "final_state": args.final_state,
            "append_seconds": append_seconds,
            "close_seconds": close_seconds,
            "total_writer_seconds": append_seconds + close_seconds,
            "live_publish_every_entries": args.live_publish_every_entries,
            "errors": [],
        }))?
    );
    Ok(())
}
