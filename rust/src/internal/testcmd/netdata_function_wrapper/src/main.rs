use anyhow::{Context, Result, bail};
use clap::Parser;
use journal::netdata::NetdataJournalFunction;
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
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.function_name != "systemd-journal" {
        bail!("unsupported function '{}'", args.function_name);
    }

    let request = std::fs::read(&args.request)
        .with_context(|| format!("failed to read request {}", args.request.display()))?;
    let function = NetdataJournalFunction::systemd_journal();
    let response = function
        .run_directory_request_bytes(&args.directory, &request)
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
