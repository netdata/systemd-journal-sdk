use anyhow::{Context, Result, anyhow};
use clap::Parser;
use journal::{FileReader, ReaderBounds, ReaderOptions};
use journal_core::file::ExperimentalMmapStrategy;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

const SCHEMA_VERSION: &str = "systemd-journal-sdk-corpus-logical-v1";
const SCHEMA_MAGIC: &[u8] = b"systemd-journal-sdk-corpus-logical-v1\0";
const DEFAULT_WINDOW_SIZE: u64 = 32 * 1024 * 1024;
const METADATA_PAYLOAD_NAMES: &[&[u8]] = &[b"_BOOT_ID"];

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, default_value = "snapshot")]
    bounds: String,
    #[arg(long, default_value = "windowed")]
    mmap_strategy: String,
    #[arg(long, default_value_t = DEFAULT_WINDOW_SIZE)]
    window_size: u64,
}

#[derive(Default)]
struct Counts {
    entries: u64,
    payloads: u64,
    payload_bytes: u64,
    binary_payloads: u64,
    payloads_without_separator: u64,
    entries_with_repeated_field_names: u64,
    repeated_field_name_occurrences: u64,
    largest_payload_bytes: u64,
}

struct CanonicalDigest {
    sha: Sha256,
    counts: Counts,
}

impl CanonicalDigest {
    fn new() -> Self {
        let mut sha = Sha256::new();
        sha.update(SCHEMA_MAGIC);
        Self {
            sha,
            counts: Counts::default(),
        }
    }

    fn update_u64(&mut self, value: u64) {
        self.sha.update(value.to_be_bytes());
    }

    fn update_bytes(&mut self, tag: u8, value: &[u8]) {
        self.sha.update([tag]);
        self.update_u64(value.len() as u64);
        self.sha.update(value);
    }

    fn update_named_bytes(&mut self, tag: u8, name: &[u8], value: &[u8]) {
        self.sha.update([tag]);
        self.update_u64(name.len() as u64);
        self.sha.update(name);
        self.update_u64(value.len() as u64);
        self.sha.update(value);
    }

    fn add_entry(
        &mut self,
        realtime: u64,
        monotonic: u64,
        seqnum: u64,
        boot_id_hex: &str,
        payloads: &mut [Vec<u8>],
    ) {
        self.sha.update(b"E");
        self.update_u64(self.counts.entries);
        self.update_named_bytes(
            b'M',
            b"__REALTIME_TIMESTAMP",
            realtime.to_string().as_bytes(),
        );
        self.update_named_bytes(
            b'M',
            b"__MONOTONIC_TIMESTAMP",
            monotonic.to_string().as_bytes(),
        );
        self.update_named_bytes(b'M', b"__SEQNUM", seqnum.to_string().as_bytes());
        self.update_named_bytes(b'M', b"__BOOT_ID", boot_id_hex.as_bytes());

        let mut seen = HashSet::<Vec<u8>>::new();
        let mut repeated = false;
        let mut repeated_occurrences = 0u64;
        let mut canonical_payloads = Vec::with_capacity(payloads.len());
        for payload in payloads.iter() {
            if payload_name(payload)
                .map(|name| METADATA_PAYLOAD_NAMES.contains(&name))
                .unwrap_or(false)
            {
                continue;
            }
            canonical_payloads.push(payload.clone());
            self.counts.payloads += 1;
            self.counts.payload_bytes += payload.len() as u64;
            self.counts.largest_payload_bytes =
                self.counts.largest_payload_bytes.max(payload.len() as u64);
            if payload.iter().any(|byte| *byte < 32 && *byte != b'\t') {
                self.counts.binary_payloads += 1;
            }
            let Some(name) = payload_name(payload) else {
                self.counts.payloads_without_separator += 1;
                continue;
            };
            if !seen.insert(name.to_vec()) {
                repeated = true;
                repeated_occurrences += 1;
            }
        }
        if repeated {
            self.counts.entries_with_repeated_field_names += 1;
            self.counts.repeated_field_name_occurrences += repeated_occurrences;
        }

        canonical_payloads.sort();
        for payload in canonical_payloads.iter() {
            self.update_bytes(b'P', payload);
        }
        self.sha.update(b"e");
        self.counts.entries += 1;
    }

    fn finish(self) -> serde_json::Value {
        let digest = self.sha.finalize();
        let mut hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            use std::fmt::Write;
            let _ = write!(&mut hex, "{byte:02x}");
        }
        json!({
            "schema": SCHEMA_VERSION,
            "logical_digest": hex,
            "counts": {
                "entries": self.counts.entries,
                "payloads": self.counts.payloads,
                "payload_bytes": self.counts.payload_bytes,
                "binary_payloads": self.counts.binary_payloads,
                "payloads_without_separator": self.counts.payloads_without_separator,
                "entries_with_repeated_field_names": self.counts.entries_with_repeated_field_names,
                "repeated_field_name_occurrences": self.counts.repeated_field_name_occurrences,
                "largest_payload_bytes": self.counts.largest_payload_bytes,
            },
        })
    }
}

fn payload_name(payload: &[u8]) -> Option<&[u8]> {
    let offset = payload.iter().position(|byte| *byte == b'=')?;
    if offset == 0 {
        return None;
    }
    Some(&payload[..offset])
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn parse_bounds(value: &str) -> Result<ReaderBounds> {
    match value {
        "live" => Ok(ReaderBounds::Live),
        "snapshot" => Ok(ReaderBounds::Snapshot),
        other => Err(anyhow!("invalid --bounds: {other}")),
    }
}

fn parse_mmap_strategy(value: &str) -> Result<ExperimentalMmapStrategy> {
    match value {
        "windowed" => Ok(ExperimentalMmapStrategy::Windowed),
        "whole-file" => Ok(ExperimentalMmapStrategy::WholeFile),
        other => Err(anyhow!("invalid --mmap-strategy: {other}")),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let options = ReaderOptions::default()
        .with_window_size(args.window_size)
        .with_bounds(parse_bounds(&args.bounds)?)
        .with_experimental_mmap_strategy(parse_mmap_strategy(&args.mmap_strategy)?);
    let started = Instant::now();
    let mut reader = FileReader::open_with_options(&args.input, options)
        .with_context(|| format!("failed to open {}", args.input.display()))?;
    reader.seek_head();
    let mut digest = CanonicalDigest::new();
    let mut payloads = Vec::new();
    while reader.next()? {
        let entry = reader.get_entry()?;
        payloads.clear();
        payloads.extend(entry.payloads);
        let boot_id_hex = hex_bytes(&entry.boot_id);
        digest.add_entry(
            entry.realtime,
            entry.monotonic,
            entry.seqnum,
            &boot_id_hex,
            &mut payloads,
        );
    }
    let elapsed_seconds = started.elapsed().as_secs_f64();
    let mut result = digest.finish();
    if let Some(object) = result.as_object_mut() {
        object.insert("driver".to_string(), json!("rust"));
        object.insert("elapsed_seconds".to_string(), json!(elapsed_seconds));
        object.insert(
            "input_bytes".to_string(),
            json!(std::fs::metadata(&args.input).map(|m| m.len()).unwrap_or(0)),
        );
    }
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
