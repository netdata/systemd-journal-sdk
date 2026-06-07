use anyhow::{Context, Result, bail};
use clap::Parser;
use journal::netdata::{
    NetdataFunctionProgress, NetdataFunctionRunOptions, NetdataJournalFunction,
};
use serde_json::json;
use std::cell::{Cell, RefCell};
use std::fs::File;
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

struct ProgressRecorder {
    writer: RefCell<Option<File>>,
    cancelled: Cell<bool>,
    reports: Cell<u64>,
    error: RefCell<Option<String>>,
    cancel_after_progress: Option<u64>,
}

impl ProgressRecorder {
    fn new(args: &Args) -> Result<Self> {
        Ok(Self {
            writer: RefCell::new(progress_writer(args)?),
            cancelled: Cell::new(args.cancel_immediately),
            reports: Cell::new(0),
            error: RefCell::new(None),
            cancel_after_progress: args.cancel_after_progress,
        })
    }

    fn handle(&self, progress: NetdataFunctionProgress) {
        if self.error.borrow().is_some() {
            return;
        }
        let reports = self.reports.get().saturating_add(1);
        self.reports.set(reports);
        let mut writer = self.writer.borrow_mut();
        if let Some(writer) = writer.as_mut() {
            self.write_progress_line(writer, progress);
        }
        if self
            .cancel_after_progress
            .is_some_and(|limit| reports >= limit)
        {
            self.cancelled.set(true);
        }
    }

    fn write_progress_line(&self, writer: &mut File, progress: NetdataFunctionProgress) {
        let line = json!({
            "current_file": progress.current_file,
            "total_files": progress.total_files,
            "matched_files": progress.matched_files,
            "skipped_files": progress.skipped_files,
            "elapsed_seconds": progress.elapsed.as_secs_f64(),
            "stats": progress.stats,
        });
        if let Err(err) = serde_json::to_writer(&mut *writer, &line) {
            self.fail(format!("failed to write progress JSON: {err}"));
            return;
        }
        if let Err(err) = writeln!(writer) {
            self.fail(format!("failed to write progress newline: {err}"));
        }
    }

    fn fail(&self, message: String) {
        *self.error.borrow_mut() = Some(message);
        self.cancelled.set(true);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.get()
    }

    fn take_error(self) -> Option<String> {
        self.error.into_inner()
    }
}

fn progress_writer(args: &Args) -> Result<Option<File>> {
    args.progress_jsonl
        .as_ref()
        .map(|path| {
            File::create(path)
                .with_context(|| format!("failed to create progress log {}", path.display()))
        })
        .transpose()
}

fn read_request_stdin() -> Result<Vec<u8>> {
    let mut request = Vec::new();
    std::io::stdin()
        .read_to_end(&mut request)
        .context("failed to read request JSON from stdin")?;
    Ok(request)
}

fn validate_function_name(function_name: &str) -> Result<()> {
    if function_name == "systemd-journal" {
        Ok(())
    } else {
        bail!("unsupported function '{}'", function_name);
    }
}

fn run_function(
    args: &Args,
    request: &[u8],
    options: NetdataFunctionRunOptions<'_>,
) -> Result<serde_json::Value> {
    NetdataJournalFunction::systemd_journal_plugin_compatible()
        .run_directory_request_bytes_with_options(&args.directory, request, options)
        .with_context(|| {
            format!(
                "failed to run function '{}' for {}",
                args.function_name,
                args.directory.display()
            )
        })
}

fn main() -> Result<()> {
    let args = Args::parse();
    validate_function_name(&args.function_name)?;
    let request = read_request_stdin()?;
    let recorder = ProgressRecorder::new(&args)?;
    let mut progress = |progress: NetdataFunctionProgress| {
        recorder.handle(progress);
    };
    let is_cancelled = || recorder.is_cancelled();
    let mut options = NetdataFunctionRunOptions::from_timeout_seconds(args.timeout_seconds);
    if args.progress_jsonl.is_some() || args.cancel_after_progress.is_some() {
        options.progress_callback = Some(&mut progress);
    }
    if args.cancel_immediately || args.cancel_after_progress.is_some() {
        options.cancellation_callback = Some(&is_cancelled);
    }
    let response = run_function(&args, &request, options)?;
    if let Some(err) = recorder.take_error() {
        bail!("{err}");
    }
    serde_json::to_writer(std::io::stdout(), &response)?;
    println!();
    Ok(())
}
