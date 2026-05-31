use anyhow::{Context, Result, anyhow};
use clap::{Args, Parser, Subcommand};
use journal::{ExperimentalMmapStrategy, FileReader, ReaderBounds, ReaderOptions, SdkError};
use journal_core::file::{
    Compression, DEFAULT_COMPRESS_THRESHOLD, EntryField, EntryWriteOptions, FieldNamePolicy,
    JournalFile, JournalFileOptions, JournalState, JournalWriter, MmapMut,
};
use journal_registry::repository::File as RepositoryFile;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};

const RAW_READER_SCHEMA: &str = "systemd-journal-sdk-raw-reader-v1";
const RAW_READER_MAGIC: &[u8] = b"systemd-journal-sdk-raw-reader-v1\0";
const SPOOL_SCHEMA: &str = "systemd-journal-sdk-spool-v1";
const MACHINE_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SEQNUM_ID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const FILE_ID: &str = "cccccccccccccccccccccccccccccccc";
const FALLBACK_BOOT_ID: &str = "dddddddddddddddddddddddddddddddd";
const DEFAULT_WINDOW_SIZE: u64 = 32 * 1024 * 1024;
const DEFAULT_MAX_SIZE_BYTES: u64 = 128 * 1024 * 1024;
const FIELD_HASH_BUCKETS: usize = 1023;

#[derive(Parser, Debug)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    RawRead(RawReadArgs),
    DumpSpool(DumpSpoolArgs),
    WriteSpool(WriteSpoolArgs),
}

#[derive(Args, Debug)]
struct RawReadArgs {
    #[arg(long)]
    input: Option<PathBuf>,
    #[arg(long)]
    directory: Option<PathBuf>,
    #[arg(long, default_value_t = 0)]
    limit_files: usize,
    #[arg(long, default_value = "csv")]
    output: String,
    #[arg(long, default_value = "mmap")]
    access: String,
    #[arg(long, default_value_t = DEFAULT_WINDOW_SIZE)]
    window_size: u64,
    #[arg(long, default_value = "sha256")]
    hash: String,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    binary_stats: bool,
}

#[derive(Args, Debug)]
struct DumpSpoolArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, default_value = "-")]
    output: String,
}

#[derive(Args, Debug)]
struct WriteSpoolArgs {
    #[arg(long, default_value = "-")]
    input: String,
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
    #[arg(long, default_value_t = 1)]
    live_publish_every_entries: u64,
}

#[derive(Default)]
struct RawCounts {
    entries: u64,
    payloads: u64,
    payload_bytes: u64,
    binary_payloads: u64,
    payloads_without_separator: u64,
    largest_payload_bytes: u64,
}

#[derive(Default)]
struct SpoolEntry {
    realtime: u64,
    monotonic: u64,
    seqnum: u64,
    boot_id: uuid::Uuid,
    payloads: Vec<Vec<u8>>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::RawRead(args) => raw_read(args),
        Command::DumpSpool(args) => dump_spool(args),
        Command::WriteSpool(args) => write_spool(args),
    }
}

fn raw_read(args: RawReadArgs) -> Result<()> {
    let hash_mode = parse_raw_hash_mode(&args.hash)?;
    let rows = input_paths(
        args.input.as_deref(),
        args.directory.as_deref(),
        args.limit_files,
    )?
    .into_iter()
    .map(|path| {
        raw_read_one(
            &path,
            &args.access,
            args.window_size,
            hash_mode,
            args.binary_stats,
        )
    })
    .collect::<Vec<_>>();
    if args.output == "json" {
        println!("{}", serde_json::to_string(&rows)?);
        return Ok(());
    }
    write_raw_csv(&rows)
}

#[derive(Clone, Copy)]
enum RawHashMode {
    None,
    Sha256,
}

impl RawHashMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Sha256 => "sha256",
        }
    }
}

fn parse_raw_hash_mode(value: &str) -> Result<RawHashMode> {
    match value {
        "none" => Ok(RawHashMode::None),
        "sha256" => Ok(RawHashMode::Sha256),
        other => Err(anyhow!("invalid --hash: {other}")),
    }
}

fn raw_read_one(
    path: &Path,
    access: &str,
    window_size: u64,
    hash_mode: RawHashMode,
    binary_stats: bool,
) -> Value {
    let started = Instant::now();
    let mmap_strategy = match access {
        "mmap" | "windowed" => ExperimentalMmapStrategy::Windowed,
        "whole-file" => ExperimentalMmapStrategy::WholeFile,
        other => return error_row(path, "invalid_access", &format!("invalid access {other}")),
    };
    let options = ReaderOptions {
        window_size,
        bounds: ReaderBounds::Snapshot,
        mmap_strategy,
    };
    let mut reader = match FileReader::open_with_options(path, options) {
        Ok(reader) => reader,
        Err(err) => return error_row(path, "open", &err.to_string()),
    };
    reader.seek_head();
    let mut hash = matches!(hash_mode, RawHashMode::Sha256).then(Sha256::new);
    if let Some(hash) = &mut hash {
        hash.update(RAW_READER_MAGIC);
    }
    let mut counts = RawCounts::default();
    loop {
        match reader.next() {
            Ok(true) => {}
            Ok(false) => break,
            Err(err) => return error_row(path, "step", &err.to_string()),
        }
        if let Some(hash) = &mut hash {
            hash.update(b"E");
            hash.update(counts.entries.to_be_bytes());
        }
        let visit = reader.visit_entry_payloads(|payload| {
            if let Some(hash) = &mut hash {
                hash.update(b"P");
                hash.update((payload.len() as u64).to_be_bytes());
                hash.update(payload);
            }
            counts.payloads += 1;
            counts.payload_bytes += payload.len() as u64;
            counts.largest_payload_bytes = counts.largest_payload_bytes.max(payload.len() as u64);
            if payload_name(payload).is_none() {
                counts.payloads_without_separator += 1;
            }
            if binary_stats && payload_has_binary(payload) {
                counts.binary_payloads += 1;
            }
            Ok::<(), SdkError>(())
        });
        if let Err(err) = visit {
            return error_row(path, "payload", &err.to_string());
        }
        if let Some(hash) = &mut hash {
            hash.update(b"e");
        }
        counts.entries += 1;
    }
    let elapsed = started.elapsed().as_secs_f64();
    json!({
        "schema": RAW_READER_SCHEMA,
        "driver": "rust",
        "status": "ok",
        "hash_mode": hash_mode.as_str(),
        "binary_stats": binary_stats,
        "file_id": sanitized_file_id(path),
        "input_bytes": file_size(path),
        "entries": counts.entries,
        "payloads": counts.payloads,
        "payload_bytes": counts.payload_bytes,
        "binary_payloads": if binary_stats { Some(counts.binary_payloads) } else { None },
        "payloads_without_equals": counts.payloads_without_separator,
        "largest_payload_bytes": counts.largest_payload_bytes,
        "hash": hash.map(|hash| hex::encode(hash.finalize())),
        "elapsed_seconds": elapsed,
        "entries_per_second": rate(counts.entries, elapsed),
        "payloads_per_second": rate(counts.payloads, elapsed),
        "payload_bytes_per_second": rate(counts.payload_bytes, elapsed),
        "input_bytes_per_second": rate(file_size(path), elapsed),
        "reader_path": "raw-payload-visitor",
    })
}

fn dump_spool(args: DumpSpoolArgs) -> Result<()> {
    let mut reader = FileReader::open_with_options(
        &args.input,
        ReaderOptions {
            window_size: DEFAULT_WINDOW_SIZE,
            bounds: ReaderBounds::Snapshot,
            mmap_strategy: ExperimentalMmapStrategy::Windowed,
        },
    )
    .with_context(|| format!("failed to open {}", args.input.display()))?;
    reader.seek_head();
    let output: Box<dyn Write> = if args.output == "-" {
        Box::new(io::stdout().lock())
    } else {
        Box::new(fs::File::create(&args.output)?)
    };
    let mut writer = BufWriter::new(output);
    while reader.next()? {
        let entry = reader.get_entry()?;
        write_text_field(
            &mut writer,
            b"__REALTIME_TIMESTAMP",
            entry.realtime.to_string().as_bytes(),
        )?;
        write_text_field(
            &mut writer,
            b"__MONOTONIC_TIMESTAMP",
            entry.monotonic.to_string().as_bytes(),
        )?;
        write_text_field(
            &mut writer,
            b"__SEQNUM",
            entry.seqnum.to_string().as_bytes(),
        )?;
        write_text_field(
            &mut writer,
            b"__BOOT_ID",
            hex::encode(entry.boot_id).as_bytes(),
        )?;
        for payload in entry.payloads {
            let (name, value) = split_payload(&payload).ok_or_else(|| {
                anyhow!(
                    "payload without '=' in sanitized input {}",
                    sanitized_file_id(&args.input)
                )
            })?;
            write_export_field(&mut writer, name, value)?;
        }
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    Ok(())
}

fn write_spool(args: WriteSpoolArgs) -> Result<()> {
    let compact = match args.format.as_str() {
        "regular" => false,
        "compact" => true,
        other => return Err(anyhow!("invalid --format: {other}")),
    };
    let compression = parse_compression(&args.compression)?;
    let input: Box<dyn Read> = if args.input == "-" {
        Box::new(io::stdin().lock())
    } else {
        Box::new(fs::File::open(&args.input)?)
    };
    let mut parser = SpoolParser::new(input);
    let parse_started = Instant::now();
    let first = parser
        .next_entry()?
        .ok_or_else(|| anyhow!("spool contains no entries"))?;
    let mut parse_seconds = parse_started.elapsed().as_secs_f64();
    if let Some(parent) = args.output.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::remove_file(&args.output) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }
    let boot_id = if first.boot_id.is_nil() {
        uuid(FALLBACK_BOOT_ID)?
    } else {
        first.boot_id
    };
    let create_started = Instant::now();
    let (mut journal_file, mut writer) = create_writer(
        &args.output,
        boot_id,
        first.seqnum.max(1),
        compact,
        compression,
        args.max_size_bytes,
        args.fss,
        systemd_fss_start_usec(first.realtime, args.fss_interval_usec),
        args.fss_interval_usec,
    )?;
    writer.set_live_publish_every_entries(args.live_publish_every_entries);
    let create_seconds = create_started.elapsed().as_secs_f64();
    let mut records = 0u64;
    let mut payloads = 0u64;
    let mut payload_bytes = 0u64;
    let mut append_seconds = 0.0;
    append_entry(
        &mut journal_file,
        &mut writer,
        &first,
        &mut records,
        &mut payloads,
        &mut payload_bytes,
        &mut append_seconds,
    )?;
    loop {
        let started = Instant::now();
        let Some(entry) = parser.next_entry()? else {
            parse_seconds += started.elapsed().as_secs_f64();
            break;
        };
        parse_seconds += started.elapsed().as_secs_f64();
        append_entry(
            &mut journal_file,
            &mut writer,
            &entry,
            &mut records,
            &mut payloads,
            &mut payload_bytes,
            &mut append_seconds,
        )?;
    }
    let close_started = Instant::now();
    finalize(&mut journal_file, &args.output, &args.final_state)?;
    let close_seconds = close_started.elapsed().as_secs_f64();
    let total = parse_seconds + create_seconds + append_seconds + close_seconds;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema": SPOOL_SCHEMA,
            "driver": "rust",
            "status": "ok",
            "records": records,
            "payloads": payloads,
            "payload_bytes": payload_bytes,
            "generated_bytes": file_size(&args.output),
            "format": args.format,
            "compression": args.compression,
            "fss": args.fss,
            "final_state": args.final_state,
            "parse_seconds": parse_seconds,
            "create_seconds": create_seconds,
            "append_seconds": append_seconds,
            "close_seconds": close_seconds,
            "total_seconds": total,
            "append_entries_per_second": rate(records, append_seconds),
            "total_entries_per_second": rate(records, total),
            "append_payloads_per_second": rate(payloads, append_seconds),
            "append_payload_bytes_per_sec": rate(payload_bytes, append_seconds),
        }))?
    );
    Ok(())
}

struct SpoolParser<R: Read> {
    reader: BufReader<R>,
}

impl<R: Read> SpoolParser<R> {
    fn new(reader: R) -> Self {
        Self {
            reader: BufReader::with_capacity(1024 * 1024, reader),
        }
    }

    fn next_entry(&mut self) -> Result<Option<SpoolEntry>> {
        let mut entry = SpoolEntry::default();
        let mut line = Vec::new();
        loop {
            line.clear();
            let read = self.reader.read_until(b'\n', &mut line)?;
            if read == 0 {
                return Ok((!entry.payloads.is_empty()).then_some(entry));
            }
            if line == b"\n" {
                if !entry.payloads.is_empty() {
                    return Ok(Some(entry));
                }
                continue;
            }
            if !line.ends_with(b"\n") {
                return Err(anyhow!("truncated spool field line"));
            }
            line.pop();
            let (name, value) = if let Some(eq) = line.iter().position(|byte| *byte == b'=') {
                (line[..eq].to_vec(), line[eq + 1..].to_vec())
            } else {
                let name = line.clone();
                let mut size_raw = [0u8; 8];
                self.reader.read_exact(&mut size_raw)?;
                let size = u64::from_le_bytes(size_raw);
                if size > 768 * 1024 * 1024 {
                    return Err(anyhow!("spool field exceeds journal DATA size limit"));
                }
                let mut value = vec![0u8; size as usize];
                self.reader.read_exact(&mut value)?;
                let mut trailer = [0u8; 1];
                self.reader.read_exact(&mut trailer)?;
                if trailer[0] != b'\n' {
                    return Err(anyhow!("spool binary field missing newline trailer"));
                }
                (name, value)
            };
            match name.as_slice() {
                b"__REALTIME_TIMESTAMP" => entry.realtime = parse_u64(&value),
                b"__MONOTONIC_TIMESTAMP" => entry.monotonic = parse_u64(&value),
                b"__SEQNUM" => entry.seqnum = parse_u64(&value),
                b"__BOOT_ID" => entry.boot_id = uuid(std::str::from_utf8(&value)?)?,
                _ => {
                    let mut payload = Vec::with_capacity(name.len() + 1 + value.len());
                    payload.extend_from_slice(&name);
                    payload.push(b'=');
                    payload.extend_from_slice(&value);
                    entry.payloads.push(payload);
                }
            }
        }
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

fn append_entry(
    journal_file: &mut JournalFile<MmapMut>,
    writer: &mut JournalWriter,
    entry: &SpoolEntry,
    records: &mut u64,
    payloads: &mut u64,
    payload_bytes: &mut u64,
    append_seconds: &mut f64,
) -> Result<()> {
    let fields = entry
        .payloads
        .iter()
        .map(|payload| EntryField::raw(payload.as_slice()))
        .collect::<Vec<_>>();
    let mut options = EntryWriteOptions::default().field_name_policy(FieldNamePolicy::Raw);
    if entry.seqnum != 0 {
        options = options.seqnum(entry.seqnum);
    }
    if !entry.boot_id.is_nil() {
        options = options.boot_id(entry.boot_id);
    }
    let started = Instant::now();
    writer.add_entry_fields_with_options(
        journal_file,
        fields.iter().copied(),
        entry.realtime,
        entry.monotonic,
        options,
    )?;
    *append_seconds += started.elapsed().as_secs_f64();
    *records += 1;
    *payloads += entry.payloads.len() as u64;
    *payload_bytes += entry
        .payloads
        .iter()
        .map(|payload| payload.len() as u64)
        .sum::<u64>();
    Ok(())
}

fn finalize(
    journal_file: &mut JournalFile<MmapMut>,
    output: &Path,
    final_state: &str,
) -> Result<()> {
    match final_state {
        "online" => journal_file.journal_header_mut().state = JournalState::Online as u8,
        "offline" => journal_file.journal_header_mut().state = JournalState::Offline as u8,
        other => return Err(anyhow!("invalid --final-state: {other}")),
    }
    journal_file.sync()?;
    if fs::metadata(output).is_err() {
        return Err(anyhow!("generated journal disappeared during finalize"));
    }
    Ok(())
}

fn input_paths(
    input: Option<&Path>,
    directory: Option<&Path>,
    limit: usize,
) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if let Some(input) = input {
        paths.push(input.to_path_buf());
    }
    if let Some(directory) = directory {
        for entry in walkdir::WalkDir::new(directory)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.file_type().is_file() && journal_like(entry.path()) {
                paths.push(entry.path().to_path_buf());
            }
        }
    }
    paths.sort();
    if limit > 0 && paths.len() > limit {
        paths.truncate(limit);
    }
    if paths.is_empty() {
        return Err(anyhow!("no input files"));
    }
    Ok(paths)
}

fn journal_like(path: &Path) -> bool {
    let value = path.as_os_str().to_string_lossy();
    value.ends_with(".journal")
        || value.ends_with(".journal~")
        || value.ends_with(".journal.zst")
        || value.ends_with(".journal~.zst")
}

fn write_raw_csv(rows: &[Value]) -> Result<()> {
    let header = [
        "schema",
        "driver",
        "status",
        "file_id",
        "input_bytes",
        "entries",
        "payloads",
        "payload_bytes",
        "binary_payloads",
        "payloads_without_equals",
        "largest_payload_bytes",
        "hash",
        "hash_mode",
        "binary_stats",
        "elapsed_seconds",
        "entries_per_second",
        "payloads_per_second",
        "payload_bytes_per_second",
        "input_bytes_per_second",
        "reader_path",
        "error_class",
        "error_sha256",
    ];
    let mut out = BufWriter::new(io::stdout().lock());
    writeln!(out, "{}", header.join(","))?;
    for row in rows {
        let fields = header
            .iter()
            .map(|key| csv_cell(row.get(*key)))
            .collect::<Vec<_>>();
        writeln!(out, "{}", fields.join(","))?;
    }
    out.flush()?;
    Ok(())
}

fn csv_cell(value: Option<&Value>) -> String {
    let raw = match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(value)) => value.clone(),
        Some(value) => value.to_string(),
    };
    if raw.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", raw.replace('"', "\"\""))
    } else {
        raw
    }
}

fn error_row(path: &Path, class: &str, msg: &str) -> Value {
    json!({
        "schema": RAW_READER_SCHEMA,
        "driver": "rust",
        "status": "failed",
        "file_id": sanitized_file_id(path),
        "error_class": class,
        "error_sha256": hex::encode(Sha256::digest(msg.as_bytes())),
    })
}

fn write_text_field(writer: &mut dyn Write, name: &[u8], value: &[u8]) -> Result<()> {
    writer.write_all(name)?;
    writer.write_all(b"=")?;
    writer.write_all(value)?;
    writer.write_all(b"\n")?;
    Ok(())
}

fn write_export_field(writer: &mut dyn Write, name: &[u8], value: &[u8]) -> Result<()> {
    if payload_has_binary(value) {
        writer.write_all(name)?;
        writer.write_all(b"\n")?;
        writer.write_all(&(value.len() as u64).to_le_bytes())?;
        writer.write_all(value)?;
        writer.write_all(b"\n")?;
        return Ok(());
    }
    write_text_field(writer, name, value)
}

fn split_payload(payload: &[u8]) -> Option<(&[u8], &[u8])> {
    let index = payload.iter().position(|byte| *byte == b'=')?;
    Some((&payload[..index], &payload[index + 1..]))
}

fn payload_name(payload: &[u8]) -> Option<&[u8]> {
    split_payload(payload).map(|(name, _)| name)
}

fn payload_has_binary(payload: &[u8]) -> bool {
    payload.iter().any(|byte| *byte < 32 && *byte != b'\t')
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

fn data_hash_buckets_for_max_size(max_size: u64) -> usize {
    (max_size / 576).max(2047).min(usize::MAX as u64) as usize
}

fn systemd_fss_start_usec(realtime: u64, interval: u64) -> u64 {
    if interval == 0 {
        realtime
    } else {
        (realtime / interval) * interval
    }
}

fn uuid(value: &str) -> Result<uuid::Uuid> {
    uuid::Uuid::parse_str(value).with_context(|| format!("invalid UUID {value}"))
}

fn parse_u64(value: &[u8]) -> u64 {
    std::str::from_utf8(value)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn sanitized_file_id(path: &Path) -> String {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut seed = abs.as_os_str().as_encoded_bytes().to_vec();
    if let Ok(metadata) = fs::metadata(path) {
        seed.extend_from_slice(&metadata.len().to_be_bytes());
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        seed.extend_from_slice(&mtime.to_be_bytes());
    }
    hex::encode(Sha256::digest(&seed))[..24].to_string()
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn rate(value: u64, seconds: f64) -> f64 {
    if seconds <= 0.0 {
        0.0
    } else {
        value as f64 / seconds
    }
}
