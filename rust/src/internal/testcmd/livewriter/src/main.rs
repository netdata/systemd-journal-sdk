use anyhow::{Result, anyhow};
use clap::Parser;
use journal::{Config, EntryTimestamps, Log, Origin, RetentionPolicy, RotationPolicy, Source};
use journal_core::file::{JournalFile, JournalFileOptions, JournalWriter, MmapMut};
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

    let delay = parse_duration(&args.delay)?;
    if let Some(path) = &args.path {
        return run_file_writer(&args, path, delay);
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
    );
    let mut log = Log::new(dir, config)?;

    const REALTIME_BASE: u64 = 1_700_001_000_000_000;
    for i in 0..args.entries {
        let fields = if args.binary_fixture && i == 0 {
            vec![
                b"TEST_ID=binary-interoperability".to_vec(),
                b"MESSAGE=binary interoperability".to_vec(),
                b"PRIORITY=6".to_vec(),
                b"LIVE_SEQ=000000".to_vec(),
                b"BINARY_PAYLOAD=\x00\x01\x02A\n\x7f\x80\xff".to_vec(),
                b"BINARY_MATCH=abc\x07def".to_vec(),
                b"BINARY_EMPTY=".to_vec(),
            ]
        } else {
            let message = format!("MESSAGE=live-{i:06}");
            let seq = format!("LIVE_SEQ={i:06}");
            vec![
                message.into_bytes(),
                b"PRIORITY=6".to_vec(),
                b"SYSLOG_IDENTIFIER=rust-live-writer".to_vec(),
                seq.into_bytes(),
            ]
        };

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

fn run_file_writer(args: &Args, path: &PathBuf, delay: Duration) -> Result<()> {
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
        .with_keyed_hash(true);
    let mut journal_file = JournalFile::<MmapMut>::create(&repo_file, options)?;
    let mut writer = JournalWriter::new(&mut journal_file, 1, boot_id)?;

    const REALTIME_BASE: u64 = 1_700_001_000_000_000;
    for i in 0..args.entries {
        let fields = if args.binary_fixture && i == 0 {
            vec![
                b"TEST_ID=binary-interoperability".to_vec(),
                b"MESSAGE=binary interoperability".to_vec(),
                b"PRIORITY=6".to_vec(),
                b"LIVE_SEQ=000000".to_vec(),
                b"BINARY_PAYLOAD=\x00\x01\x02A\n\x7f\x80\xff".to_vec(),
                b"BINARY_MATCH=abc\x07def".to_vec(),
                b"BINARY_EMPTY=".to_vec(),
            ]
        } else {
            let message = format!("MESSAGE=live-{i:06}");
            let seq = format!("LIVE_SEQ={i:06}");
            vec![
                message.into_bytes(),
                b"PRIORITY=6".to_vec(),
                b"SYSLOG_IDENTIFIER=rust-live-writer".to_vec(),
                seq.into_bytes(),
            ]
        };

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
