use anyhow::{Context, Result, anyhow};
use base64::Engine;
use clap::Parser;
use journal_core::file::{
    Compression, DEFAULT_COMPRESS_THRESHOLD, JournalFile, JournalFileOptions, JournalState,
    JournalWriter, MmapMut,
};
use journal_registry::repository::File as RepositoryFile;
use serde::Deserialize;
use serde_json::json;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const BOOT_ID: &str = "0123456789abcdef0123456789abcdef";
const MACHINE_ID: &str = "fedcba9876543210fedcba9876543210";
const SEQNUM_ID: &str = "22222222222222222222222222222222";
const FILE_ID: &str = "33333333333333333333333333333333";
const DEFAULT_ARCHIVE_REALTIME: u64 = 1_700_000_000_000_000;
const OVERSIZED_LIMIT: u64 = 4 * 1024 * 1024;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    dataset: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    rejection_mode: bool,
    #[arg(long, default_value = "online")]
    final_state: String,
    #[arg(long)]
    compact: bool,
    #[arg(long)]
    max_size_bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
enum ValueDescriptor {
    #[serde(rename = "utf8")]
    Utf8 { text: String },
    #[serde(rename = "bytes")]
    Bytes { base64: String, size: usize },
    #[serde(rename = "repeat")]
    Repeat { byte: u8, size: usize },
}

#[derive(Debug, Deserialize)]
struct FieldRecord {
    name: String,
    value: ValueDescriptor,
}

#[derive(Debug, Deserialize)]
struct AcceptedRecord {
    record_type: String,
    entry_id: String,
    realtime_usec: u64,
    monotonic_usec: u64,
    boot_id: Option<String>,
    fields: Vec<FieldRecord>,
}

#[derive(Debug, Deserialize)]
struct RejectedRecord {
    record_type: String,
    case_id: String,
    expected_error: String,
    input: serde_json::Value,
}

struct IngestOptions<'a> {
    output: &'a Path,
    final_state: &'a str,
    compact: bool,
    max_size_bytes: Option<u64>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if !matches!(args.final_state.as_str(), "online" | "offline" | "archived") {
        return Err(anyhow!("invalid final state: {}", args.final_state));
    }
    let result = if args.rejection_mode {
        ingest_rejections(
            &args.dataset,
            &args.output,
            &args.final_state,
            args.compact,
            args.max_size_bytes,
        )?
    } else {
        ingest_accepted(
            &args.dataset,
            &args.output,
            &args.final_state,
            args.compact,
            args.max_size_bytes,
        )?
    };
    println!("{}", serde_json::to_string(&result)?);
    if result["errors"]
        .as_array()
        .is_some_and(|errors| !errors.is_empty())
    {
        return Err(anyhow!("dataset ingestion failed"));
    }
    Ok(())
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

fn create_writer(
    path: &Path,
    compact: bool,
    max_size_bytes: Option<u64>,
) -> Result<(JournalFile<MmapMut>, JournalWriter)> {
    let path = absolute_path(path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let repo_file = RepositoryFile::from_path(&path)
        .ok_or_else(|| anyhow!("journal path must be absolute and end in .journal"))?;
    let boot_id = uuid(BOOT_ID)?;
    let options = JournalFileOptions::new(uuid(MACHINE_ID)?, boot_id, uuid(SEQNUM_ID)?)
        .with_file_id(uuid(FILE_ID)?)
        .with_window_size(8 * 1024 * 1024)
        .with_keyed_hash(true)
        .with_compression(Compression::None)
        .with_compress_threshold(DEFAULT_COMPRESS_THRESHOLD)
        .with_compact(compact)
        .with_optimized_buckets(None, max_size_bytes);
    let mut journal_file = JournalFile::<MmapMut>::create(&repo_file, options)?;
    let writer = JournalWriter::new_with_compression(
        &mut journal_file,
        1,
        boot_id,
        Compression::None,
        DEFAULT_COMPRESS_THRESHOLD,
    )?;
    Ok((journal_file, writer))
}

fn archive_path_for(output: &Path, head_realtime: u64) -> PathBuf {
    let file_name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("system.journal");
    let prefix = file_name.strip_suffix(".journal").unwrap_or(file_name);
    output.with_file_name(format!(
        "{prefix}@{SEQNUM_ID}-0000000000000001-{head_realtime:016x}.journal"
    ))
}

fn finalize_journal_file(
    journal_file: &mut JournalFile<MmapMut>,
    output: &Path,
    final_state: &str,
    head_realtime: u64,
) -> Result<()> {
    match final_state {
        "online" => {
            journal_file.journal_header_mut().state = JournalState::Online as u8;
            journal_file.sync()?;
        }
        "offline" => {
            journal_file.journal_header_mut().state = JournalState::Offline as u8;
            journal_file.sync()?;
        }
        "archived" => {
            let archive_path = archive_path_for(output, head_realtime);
            match fs::remove_file(&archive_path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
            fs::rename(output, &archive_path)?;
            journal_file.journal_header_mut().state = JournalState::Archived as u8;
            journal_file.sync()?;
        }
        _ => return Err(anyhow!("invalid final state: {final_state}")),
    }
    Ok(())
}

fn materialize_value(value: &ValueDescriptor) -> Result<Vec<u8>> {
    match value {
        ValueDescriptor::Utf8 { text } => Ok(text.as_bytes().to_vec()),
        ValueDescriptor::Bytes { base64, size } => {
            let data = base64::engine::general_purpose::STANDARD.decode(base64)?;
            if data.len() != *size {
                return Err(anyhow!(
                    "bytes size mismatch: expected {size}, got {}",
                    data.len()
                ));
            }
            Ok(data)
        }
        ValueDescriptor::Repeat { byte, size } => Ok(vec![*byte; *size]),
    }
}

fn valid_field_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 || bytes[0].is_ascii_digit() {
        return false;
    }
    bytes
        .iter()
        .all(|b| *b == b'_' || b.is_ascii_uppercase() || b.is_ascii_digit())
}

fn expected_rejection(input: &serde_json::Value) -> Option<&'static str> {
    if let Some(raw) = input.get("raw_payload").and_then(|v| v.as_str()) {
        let Some((name, _value)) = raw.split_once('=') else {
            return Some("EINVAL");
        };
        return (!valid_field_name(name)).then_some("EINVAL");
    }
    let Some(name) = input.get("field_name").and_then(|v| v.as_str()) else {
        return Some("EINVAL");
    };
    if !valid_field_name(name) {
        return Some("EINVAL");
    }
    let Some(value) = input.get("value") else {
        return Some("EINVAL");
    };
    if value.is_null() {
        return Some("EINVAL");
    }
    if value.get("kind").and_then(|v| v.as_str()) == Some("repeat")
        && value.get("size").and_then(|v| v.as_u64()).unwrap_or(0) > OVERSIZED_LIMIT
    {
        return Some("E2BIG");
    }
    None
}

fn ingest_accepted(
    dataset: &Path,
    output: &Path,
    final_state: &str,
    compact: bool,
    max_size_bytes: Option<u64>,
) -> Result<serde_json::Value> {
    let options = IngestOptions {
        output,
        final_state,
        compact,
        max_size_bytes,
    };
    let (mut journal_file, mut writer) =
        create_writer(options.output, options.compact, options.max_size_bytes)?;
    let reader = BufReader::new(File::open(dataset)?);
    let mut stats = IngestStats::default();
    let mut head_realtime = 0;

    for (index, line) in reader.lines().enumerate() {
        let Some(record) = parse_accepted_line(line?)? else {
            continue;
        };
        append_accepted_record(
            index,
            &record,
            &mut journal_file,
            &mut writer,
            &mut stats,
            &mut head_realtime,
        )?;
    }
    finalize_journal_file(
        &mut journal_file,
        options.output,
        options.final_state,
        if head_realtime == 0 {
            DEFAULT_ARCHIVE_REALTIME
        } else {
            head_realtime
        },
    )?;
    Ok(json!({ "records": stats.records, "errors": stats.errors }))
}

fn ingest_rejections(
    dataset: &Path,
    output: &Path,
    final_state: &str,
    compact: bool,
    max_size_bytes: Option<u64>,
) -> Result<serde_json::Value> {
    let options = IngestOptions {
        output,
        final_state,
        compact,
        max_size_bytes,
    };
    let reader = BufReader::new(File::open(dataset)?);
    let mut writer_state: Option<(JournalFile<MmapMut>, JournalWriter)> = None;
    let mut stats = IngestStats::default();

    for (index, line) in reader.lines().enumerate() {
        let Some(record) = parse_rejected_line(line?)? else {
            continue;
        };
        process_rejection_record(index, &record, &options, &mut writer_state, &mut stats)?;
    }
    if let Some((mut journal_file, _writer)) = writer_state {
        finalize_journal_file(
            &mut journal_file,
            options.output,
            options.final_state,
            DEFAULT_ARCHIVE_REALTIME,
        )?;
    }
    Ok(json!({ "records": stats.records, "errors": stats.errors }))
}

#[derive(Default)]
struct IngestStats {
    records: usize,
    errors: Vec<String>,
}

fn parse_accepted_line(line: String) -> Result<Option<AcceptedRecord>> {
    if line.trim().is_empty() {
        return Ok(None);
    }
    let record: AcceptedRecord = serde_json::from_str(&line)?;
    Ok((record.record_type == "accepted").then_some(record))
}

fn parse_rejected_line(line: String) -> Result<Option<RejectedRecord>> {
    if line.trim().is_empty() {
        return Ok(None);
    }
    let record: RejectedRecord = serde_json::from_str(&line)?;
    Ok((record.record_type == "rejected").then_some(record))
}

fn append_accepted_record(
    index: usize,
    record: &AcceptedRecord,
    journal_file: &mut JournalFile<MmapMut>,
    writer: &mut JournalWriter,
    stats: &mut IngestStats,
    head_realtime: &mut u64,
) -> Result<()> {
    let Some(fields) = build_payloads(record, index, &mut stats.errors)? else {
        return Ok(());
    };
    validate_accepted_boot(record, writer)?;
    let field_refs: Vec<&[u8]> = fields.iter().map(Vec::as_slice).collect();
    match writer.add_entry(
        journal_file,
        &field_refs,
        record.realtime_usec,
        record.monotonic_usec,
    ) {
        Ok(()) => {
            if *head_realtime == 0 {
                *head_realtime = record.realtime_usec;
            }
            stats.records += 1;
        }
        Err(err) => stats.errors.push(format!(
            "line {} {}: append failed: {err}",
            index + 1,
            record.entry_id
        )),
    }
    Ok(())
}

fn build_payloads(
    record: &AcceptedRecord,
    index: usize,
    errors: &mut Vec<String>,
) -> Result<Option<Vec<Vec<u8>>>> {
    let mut fields = Vec::with_capacity(record.fields.len());
    for field in &record.fields {
        match accepted_payload(field) {
            Ok(payload) => fields.push(payload),
            Err(err) => {
                errors.push(format!("line {} {}: {err}", index + 1, record.entry_id));
                return Ok(None);
            }
        }
    }
    Ok(Some(fields))
}

fn accepted_payload(field: &FieldRecord) -> Result<Vec<u8>> {
    let value = materialize_value(&field.value)?;
    let mut payload = field.name.as_bytes().to_vec();
    payload.push(b'=');
    payload.extend_from_slice(&value);
    Ok(payload)
}

fn validate_accepted_boot(record: &AcceptedRecord, writer: &JournalWriter) -> Result<()> {
    let entry_boot_id = record.boot_id.as_deref().unwrap_or(BOOT_ID);
    if entry_boot_id != BOOT_ID {
        return Err(anyhow!("dataset boot ID mismatch"));
    }
    if writer.boot_id() != uuid(BOOT_ID)? {
        return Err(anyhow!("writer boot ID mismatch"));
    }
    Ok(())
}

fn process_rejection_record(
    index: usize,
    record: &RejectedRecord,
    options: &IngestOptions<'_>,
    writer_state: &mut Option<(JournalFile<MmapMut>, JournalWriter)>,
    stats: &mut IngestStats,
) -> Result<()> {
    if let Some(got) = expected_rejection(&record.input) {
        record_expected_rejection(index, record, got, stats);
        return Ok(());
    }
    let payload = rejected_payload(record)?;
    if writer_state.is_none() {
        *writer_state = Some(create_writer(
            options.output,
            options.compact,
            options.max_size_bytes,
        )?);
    }
    let (journal_file, writer) = writer_state.as_mut().expect("writer state initialized");
    record_writer_rejection(index, record, journal_file, writer, &payload, stats);
    Ok(())
}

fn record_expected_rejection(
    index: usize,
    record: &RejectedRecord,
    got: &str,
    stats: &mut IngestStats,
) {
    if got == record.expected_error {
        stats.records += 1;
    } else {
        stats.errors.push(format!(
            "line {} {}: got {got}, expected {}",
            index + 1,
            record.case_id,
            record.expected_error
        ));
    }
}

fn rejected_payload(record: &RejectedRecord) -> Result<Vec<u8>> {
    let value: ValueDescriptor = serde_json::from_value(record.input["value"].clone())?;
    let mut payload = record.input["field_name"]
        .as_str()
        .ok_or_else(|| anyhow!("rejection record missing field_name"))?
        .as_bytes()
        .to_vec();
    payload.push(b'=');
    payload.extend_from_slice(&materialize_value(&value)?);
    Ok(payload)
}

fn record_writer_rejection(
    index: usize,
    record: &RejectedRecord,
    journal_file: &mut JournalFile<MmapMut>,
    writer: &mut JournalWriter,
    payload: &[u8],
    stats: &mut IngestStats,
) {
    match writer.add_entry(journal_file, &[payload], 1_700_000_000_000_000, 50_000_000) {
        Ok(()) => stats.errors.push(format!(
            "line {} {}: unexpectedly accepted",
            index + 1,
            record.case_id
        )),
        Err(_) if record.expected_error == "EINVAL" => stats.records += 1,
        Err(_) => stats.errors.push(format!(
            "line {} {}: rejected as EINVAL, expected {}",
            index + 1,
            record.case_id,
            record.expected_error
        )),
    }
}
