use anyhow::{Result, anyhow};
use clap::Parser;
use journal::{Config, EntryTimestamps, Log, Origin, RetentionPolicy, RotationPolicy, Source};
use journal_core::file::{Compression, JournalFile, JournalFileOptions, JournalWriter, MmapMut};
use journal_core::repository::File as RepositoryFile;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long = "path", conflicts_with = "dir")]
    path: Option<PathBuf>,
    #[arg(long = "dir")]
    dir: Option<PathBuf>,
    #[arg(long = "ready-file")]
    ready_file: PathBuf,
    #[arg(long = "entries", default_value_t = 1000)]
    entries: usize,
    #[arg(long = "delay", default_value = "1ms")]
    delay: String,
    #[arg(long = "sync-every", default_value_t = 25)]
    sync_every: usize,
    #[arg(long = "crash-after", default_value_t = 0)]
    crash_after: usize,
    #[arg(long = "binary-fixture", default_value_t = false)]
    binary_fixture: bool,
    #[arg(long = "zstd-fixture", default_value_t = false)]
    zstd_fixture: bool,
    #[arg(long = "xz-fixture", default_value_t = false)]
    xz_fixture: bool,
    #[arg(long = "lz4-fixture", default_value_t = false)]
    lz4_fixture: bool,
    #[arg(long = "compression", default_value = "none")]
    compression: String,
    #[arg(
        long = "compression-threshold-bytes",
        alias = "compress-threshold",
        default_value_t = 64
    )]
    compression_threshold: usize,
    #[arg(long = "compact", default_value_t = false)]
    compact: bool,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    if args.entries == 0 {
        return Err(anyhow!("entries must be positive"));
    }

    let compression = match args.compression.as_str() {
        "none" => Compression::None,
        "xz" => Compression::Xz,
        "lz4" => Compression::Lz4,
        "zstd" => Compression::Zstd,
        other => return Err(anyhow!("unknown compression: {other}")),
    };
    let delay = parse_duration(&args.delay)?;
    if let Some(path) = &args.path {
        return run_file_writer(&args, path, delay, compression);
    }
    let Some(dir) = &args.dir else {
        return Err(anyhow!("either --path or --dir is required"));
    };
    let origin = Origin {
        machine_id: None,
        namespace: None,
        source: Source::System,
    };
    let config = Config::new(
        origin,
        RotationPolicy::default(),
        RetentionPolicy::default(),
    )
    .with_compression(compression)
    .with_compression_threshold(args.compression_threshold)
    .with_compact(args.compact);
    let mut log = Log::new(dir, config)?;

    const REALTIME_BASE: u64 = 1_700_001_000_000_000;
    for i in 0..args.entries {
        let fields = fields_for_entry(
            i,
            args.binary_fixture,
            args.zstd_fixture,
            args.xz_fixture,
            args.lz4_fixture,
        );

        let fields_refs: Vec<&[u8]> = fields.iter().map(|v| v.as_slice()).collect();
        log.write_entry_with_timestamps(
            &fields_refs,
            EntryTimestamps {
                entry_realtime_usec: Some(REALTIME_BASE + i as u64),
                entry_monotonic_usec: Some(i as u64 + 1),
                source_realtime_usec: None,
            },
        )?;

        if i == 0 {
            log.sync()?;
            log.active_file()
                .ok_or_else(|| anyhow!("active journal file missing after first write"))?;
            fs::write(&args.ready_file, b"ready\n")?;
        } else if args.sync_every > 0 && (i + 1) % args.sync_every == 0 {
            log.sync()?;
        }

        if args.crash_after > 0 && i + 1 >= args.crash_after {
            std::process::exit(17);
        }
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
    }

    log.sync()?;
    Ok(())
}

fn run_file_writer(
    args: &Args,
    path: &PathBuf,
    delay: Duration,
    compression: Compression,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let repo_file = RepositoryFile::from_path(path)
        .ok_or_else(|| anyhow!("journal path must be absolute and end in .journal"))?;
    let machine_id = uuid::Uuid::new_v4();
    let boot_id = uuid::Uuid::new_v4();
    let seqnum_id = uuid::Uuid::new_v4();
    let options = JournalFileOptions::new(machine_id, boot_id, seqnum_id)
        .with_window_size(8 * 1024 * 1024)
        .with_keyed_hash(true)
        .with_compression(compression)
        .with_compress_threshold(args.compression_threshold)
        .with_compact(args.compact);
    let mut journal_file = JournalFile::<MmapMut>::create(&repo_file, options)?;
    let mut writer = JournalWriter::new_with_compression(
        &mut journal_file,
        1,
        boot_id,
        compression,
        args.compression_threshold,
    )?;

    const REALTIME_BASE: u64 = 1_700_001_000_000_000;
    for i in 0..args.entries {
        let fields = fields_for_entry(
            i,
            args.binary_fixture,
            args.zstd_fixture,
            args.xz_fixture,
            args.lz4_fixture,
        );

        let fields_refs: Vec<&[u8]> = fields.iter().map(|v| v.as_slice()).collect();
        writer.add_entry(
            &mut journal_file,
            &fields_refs,
            REALTIME_BASE + i as u64,
            i as u64 + 1,
        )?;

        if i == 0 {
            journal_file.sync()?;
            fs::write(&args.ready_file, b"ready\n")?;
        } else if args.sync_every > 0 && (i + 1) % args.sync_every == 0 {
            journal_file.sync()?;
        }

        if args.crash_after > 0 && i + 1 >= args.crash_after {
            std::process::exit(17);
        }
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
    }

    journal_file.sync()?;
    Ok(())
}

fn fields_for_entry(
    index: usize,
    binary_fixture: bool,
    zstd_fixture: bool,
    xz_fixture: bool,
    lz4_fixture: bool,
) -> Vec<Vec<u8>> {
    if binary_fixture && index == 0 {
        return vec![
            b"TEST_ID=binary-interoperability".to_vec(),
            b"MESSAGE=binary interoperability".to_vec(),
            b"PRIORITY=6".to_vec(),
            b"LIVE_SEQ=000000".to_vec(),
            b"BINARY_PAYLOAD=\x00\x01\x02A\n\x7f\x80\xff".to_vec(),
            b"BINARY_MATCH=abc\x07def".to_vec(),
            b"BINARY_EMPTY=".to_vec(),
        ];
    }

    if zstd_fixture && index == 0 {
        let large_payload: Vec<u8> = (0..256usize).map(|i| (i % 26) as u8 + b'A').collect();
        let mut compressed_payload = b"COMPRESSED_PAYLOAD=".to_vec();
        compressed_payload.extend_from_slice(&large_payload);
        let mut compressed_match = b"COMPRESSED_MATCH=".to_vec();
        compressed_match.extend_from_slice(&large_payload[..32]);
        return vec![
            b"TEST_ID=zstd-interoperability".to_vec(),
            b"MESSAGE=zstd interoperability".to_vec(),
            b"PRIORITY=6".to_vec(),
            b"LIVE_SEQ=000000".to_vec(),
            compressed_payload,
            compressed_match,
        ];
    }

    if xz_fixture && index == 0 {
        let large_payload: Vec<u8> = (0..256usize).map(|i| (i % 26) as u8 + b'A').collect();
        let mut compressed_payload = b"COMPRESSED_PAYLOAD=".to_vec();
        compressed_payload.extend_from_slice(&large_payload);
        let mut compressed_match = b"COMPRESSED_MATCH=".to_vec();
        compressed_match.extend_from_slice(&large_payload[..32]);
        return vec![
            b"TEST_ID=xz-interoperability".to_vec(),
            b"MESSAGE=xz interoperability".to_vec(),
            b"PRIORITY=6".to_vec(),
            b"LIVE_SEQ=000000".to_vec(),
            compressed_payload,
            compressed_match,
        ];
    }

    if lz4_fixture && index == 0 {
        let large_payload: Vec<u8> = (0..256usize).map(|i| (i % 26) as u8 + b'A').collect();
        let mut compressed_payload = b"COMPRESSED_PAYLOAD=".to_vec();
        compressed_payload.extend_from_slice(&large_payload);
        let mut compressed_match = b"COMPRESSED_MATCH=".to_vec();
        compressed_match.extend_from_slice(&large_payload[..32]);
        return vec![
            b"TEST_ID=lz4-interoperability".to_vec(),
            b"MESSAGE=lz4 interoperability".to_vec(),
            b"PRIORITY=6".to_vec(),
            b"LIVE_SEQ=000000".to_vec(),
            compressed_payload,
            compressed_match,
        ];
    }

    let message = format!("MESSAGE=live-{index:06}");
    let seq = format!("LIVE_SEQ={index:06}");
    vec![
        message.into_bytes(),
        b"PRIORITY=6".to_vec(),
        b"SYSLOG_IDENTIFIER=rust-live-writer".to_vec(),
        seq.into_bytes(),
    ]
}

fn parse_duration(input: &str) -> Result<Duration> {
    if input == "0" {
        return Ok(Duration::ZERO);
    }
    if let Some(value) = input.strip_suffix("ms") {
        return Ok(Duration::from_millis(value.parse()?));
    }
    if let Some(value) = input.strip_suffix("us") {
        return Ok(Duration::from_micros(value.parse()?));
    }
    if let Some(value) = input.strip_suffix("ns") {
        return Ok(Duration::from_nanos(value.parse()?));
    }
    if let Some(value) = input.strip_suffix('s') {
        return Ok(Duration::from_secs(value.parse()?));
    }
    Err(anyhow!("invalid delay duration: {input}"))
}
