use anyhow::{Result, anyhow};
use clap::{Parser, ValueEnum};
use journal::{
    FacadeError, OutputMode, SdJournal, SdJournalAddDisjunction, SdJournalAddMatch,
    SdJournalEnumerateFields, SdJournalGetEntry, SdJournalListBoots, SdJournalNext, SdJournalOpen,
    SdJournalPrevious, SdJournalProcessOutput, SdJournalSeekHead, SdJournalSeekTail,
    SdJournalSetOutputMode, parse_match_string,
};
use std::io::Write;
use std::path::PathBuf;
use std::process::exit;

#[derive(Parser, Debug)]
#[command(name = "journalctl")]
#[command(about = "Pure-Rust file-backed journalctl subset")]
struct Args {
    #[arg(long = "file")]
    file: Option<PathBuf>,
    #[arg(long = "directory")]
    directory: Option<PathBuf>,
    #[arg(long = "output", default_value = "default")]
    output: OutputModeArg,
    #[arg(long = "list-boots")]
    list_boots: bool,
    #[arg(long = "fields")]
    fields: bool,
    #[arg(long = "head")]
    head: Option<usize>,
    #[arg(long = "tail")]
    tail: Option<usize>,
    #[arg(long = "follow")]
    follow: bool,
    #[arg(long = "sync")]
    sync: bool,
    #[arg(long = "flush")]
    flush: bool,
    #[arg(long = "rotate")]
    rotate: bool,
    #[arg(long = "relinquish-var")]
    relinquish_var: bool,
    #[arg(long = "verify")]
    verify: bool,
    #[arg(long = "verify-only")]
    verify_only: bool,
    #[arg(trailing_var_arg = true)]
    matches: Vec<String>,
}

#[derive(Debug, Clone, ValueEnum)]
enum OutputModeArg {
    #[value(alias = "short")]
    Default,
    Json,
    Export,
}

impl From<OutputModeArg> for OutputMode {
    fn from(value: OutputModeArg) -> Self {
        match value {
            OutputModeArg::Default => Self::Default,
            OutputModeArg::Json => Self::Json,
            OutputModeArg::Export => Self::Export,
        }
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    if args.follow
        || args.sync
        || args.flush
        || args.rotate
        || args.relinquish_var
        || args.verify
        || args.verify_only
    {
        return Err(anyhow!("{}", FacadeError::Unsupported));
    }

    let path = args
        .file
        .as_ref()
        .or(args.directory.as_ref())
        .ok_or_else(|| anyhow!("use --file or --directory"))?;

    let mut journal =
        SdJournalOpen(&path.to_string_lossy(), 0).map_err(|err| anyhow!("open: {err}"))?;
    apply_matches(&mut journal, &args.matches)?;
    SdJournalSetOutputMode(&mut journal, args.output.into());

    if args.list_boots {
        for boot in SdJournalListBoots(&mut journal).map_err(|err| anyhow!("list boots: {err}"))? {
            println!(
                "{:>2} {} {} {}",
                boot.index, boot.boot_id, boot.first_entry, boot.last_entry
            );
        }
        return Ok(());
    }

    if args.fields {
        let mut fields =
            SdJournalEnumerateFields(&mut journal).map_err(|err| anyhow!("fields: {err}"))?;
        fields.sort();
        for field in fields {
            println!("{field}");
        }
        return Ok(());
    }

    if let Some(n) = args.tail {
        show_tail(&mut journal, n)
    } else {
        show_head_or_all(&mut journal, args.head)
    }
}

fn apply_matches(journal: &mut SdJournal, matches: &[String]) -> Result<()> {
    for item in matches {
        if item == "+" {
            SdJournalAddDisjunction(journal).map_err(|err| anyhow!("add disjunction: {err}"))?;
            continue;
        }
        let data =
            parse_match_string(item).map_err(|err| anyhow!("invalid match {item}: {err}"))?;
        SdJournalAddMatch(journal, &data).map_err(|err| anyhow!("add match: {err}"))?;
    }
    Ok(())
}

fn show_head_or_all(journal: &mut SdJournal, limit: Option<usize>) -> Result<()> {
    SdJournalSeekHead(journal).map_err(|err| anyhow!("seek head: {err}"))?;
    let mut shown = 0usize;
    loop {
        if limit.is_some_and(|limit| shown >= limit) {
            return Ok(());
        }
        match SdJournalNext(journal).map_err(|err| anyhow!("next: {err}"))? {
            0 => return Ok(()),
            _ => {
                write_current(journal)?;
                shown += 1;
            }
        }
    }
}

fn show_tail(journal: &mut SdJournal, limit: usize) -> Result<()> {
    SdJournalSeekTail(journal).map_err(|err| anyhow!("seek tail: {err}"))?;
    let mut entries = Vec::new();
    loop {
        if entries.len() >= limit {
            break;
        }
        match SdJournalPrevious(journal).map_err(|err| anyhow!("previous: {err}"))? {
            0 => break,
            _ => {
                let entry =
                    SdJournalGetEntry(journal).map_err(|err| anyhow!("get entry: {err}"))?;
                let output = SdJournalProcessOutput(journal, &entry)
                    .map_err(|err| anyhow!("output: {err}"))?;
                entries.push(output);
            }
        }
    }

    let mut stdout = std::io::stdout().lock();
    for entry in entries.iter().rev() {
        stdout.write_all(entry)?;
    }
    Ok(())
}

fn write_current(journal: &mut SdJournal) -> Result<()> {
    let entry = SdJournalGetEntry(journal).map_err(|err| anyhow!("get entry: {err}"))?;
    let output = SdJournalProcessOutput(journal, &entry).map_err(|err| anyhow!("output: {err}"))?;
    std::io::stdout().lock().write_all(&output)?;
    Ok(())
}
