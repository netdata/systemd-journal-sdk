use anyhow::{Context, Result, bail};
use clap::Parser;
use journal::netdata::{
    NetdataFunctionProgress, NetdataFunctionRunOptions, NetdataJournalFunction,
};
use serde_json::json;
use std::cell::{Cell, RefCell};
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "journal-netdata-function")]
#[command(about = "Run a Netdata-compatible journal function through the SDK")]
struct Args {
    #[arg(long = "test")]
    function_name: String,
    #[arg(long = "dir")]
    directory: PathBuf,
    #[arg(long = "timeout", default_value_t = 0)]
    timeout_seconds: u64,
    #[arg(long = "progress-jsonl")]
    progress_jsonl: Option<PathBuf>,
    #[arg(long = "cancel-immediately", default_value_t = false)]
    cancel_immediately: bool,
    #[arg(long = "cancel-after-progress")]
    cancel_after_progress: Option<u64>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.function_name != "systemd-journal" {
        bail!("unsupported function '{}'", args.function_name);
    }

    let mut request = Vec::new();
    std::io::stdin()
        .read_to_end(&mut request)
        .context("failed to read request JSON from stdin")?;
    let function = NetdataJournalFunction::systemd_journal_plugin_compatible();
    let mut progress_writer = match &args.progress_jsonl {
        Some(path) => Some(
            std::fs::File::create(path)
                .with_context(|| format!("failed to create progress log {}", path.display()))?,
        ),
        None => None,
    };
    let cancelled = Cell::new(args.cancel_immediately);
    let progress_reports = Cell::new(0u64);
    let progress_error = RefCell::new(None::<String>);
    let cancel_after_progress = args.cancel_after_progress;
    let mut progress = |progress: NetdataFunctionProgress| {
        if progress_error.borrow().is_some() {
            return;
        }
        let reports = progress_reports.get().saturating_add(1);
        progress_reports.set(reports);
        if let Some(writer) = progress_writer.as_mut() {
            let line = json!({
                "current_file": progress.current_file,
                "total_files": progress.total_files,
                "matched_files": progress.matched_files,
                "skipped_files": progress.skipped_files,
                "elapsed_seconds": progress.elapsed.as_secs_f64(),
                "stats": progress.stats,
            });
            if let Err(err) = serde_json::to_writer(&mut *writer, &line) {
                *progress_error.borrow_mut() =
                    Some(format!("failed to write progress JSON: {err}"));
                cancelled.set(true);
                return;
            }
            if let Err(err) = writeln!(writer) {
                *progress_error.borrow_mut() =
                    Some(format!("failed to write progress newline: {err}"));
                cancelled.set(true);
                return;
            }
        }
        if cancel_after_progress.is_some_and(|limit| reports >= limit) {
            cancelled.set(true);
        }
    };
    let is_cancelled = || cancelled.get();
    let mut options = NetdataFunctionRunOptions::from_timeout_seconds(args.timeout_seconds);
    if args.progress_jsonl.is_some() || args.cancel_after_progress.is_some() {
        options.progress_callback = Some(&mut progress);
    }
    if args.cancel_immediately || args.cancel_after_progress.is_some() {
        options.cancellation_callback = Some(&is_cancelled);
    }
    let response = function
        .run_directory_request_bytes_with_options(&args.directory, &request, options)
        .with_context(|| {
            format!(
                "failed to run function '{}' for {}",
                args.function_name,
                args.directory.display()
            )
        })?;
    if let Some(err) = progress_error.into_inner() {
        bail!("{err}");
    }
    serde_json::to_writer(std::io::stdout(), &response)?;
    println!();
    Ok(())
}
