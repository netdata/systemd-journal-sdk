use anyhow::{Context, Result, bail};
use clap::Parser;
use journal::netdata::{NetdataFunctionRunOptions, NetdataJournalFunction};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "journal-netdata-function")]
#[command(about = "Run a Netdata-compatible journal function through the SDK")]
struct Args {
    #[arg(long = "test")]
    function_name: String,
    #[arg(long = "dir")]
    directory: PathBuf,
    #[arg(long = "request")]
    request: PathBuf,
    #[arg(long = "timeout", default_value_t = 0)]
    timeout_seconds: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.function_name != "systemd-journal" {
        bail!("unsupported function '{}'", args.function_name);
    }

    let request = std::fs::read(&args.request)
        .with_context(|| format!("failed to read request {}", args.request.display()))?;
    let function = NetdataJournalFunction::systemd_journal_plugin_compatible();
    let options = NetdataFunctionRunOptions::from_timeout_seconds(args.timeout_seconds);
    let response = function
        .run_directory_request_bytes_with_options(&args.directory, &request, options)
        .with_context(|| {
            format!(
                "failed to run function '{}' for {}",
                args.function_name,
                args.directory.display()
            )
        })?;
    serde_json::to_writer(std::io::stdout(), &response)?;
    println!();
    Ok(())
}
