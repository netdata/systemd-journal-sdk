use anyhow::{Result, anyhow};
use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use clap::{Parser, ValueEnum};
use journal::{
    Entry, FacadeError, FileReader, OutputMode, SdJournal, SdJournalAddConjunction,
    SdJournalAddDisjunction, SdJournalAddMatch, SdJournalEnumerateFields, SdJournalGetEntry,
    SdJournalListBoots, SdJournalNext, SdJournalOpen, SdJournalPrevious, SdJournalProcessOutput,
    SdJournalSeekCursor, SdJournalSeekHead, SdJournalSeekRealtimeUsec, SdJournalSeekTail,
    SdJournalSetOutputMode, SdJournalTestCursor, SdJournalVisitUniqueValues, parse_match_string,
    verify_file, verify_file_with_key,
};
use regex::{Regex, RegexBuilder};
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::thread;
use std::time::Duration;

// HEADER_COMPATIBLE_SEALED from systemd journal-def.h
const COMPATIBLE_SEALED: u32 = 1;

// Reasons used by the portable-mode unsupported contract.
fn unsupported_reason(name: &str) -> &'static str {
    match name {
        "machine" => {
            "requires local container or machine journal access; portable mode never connects to a host or container"
        }
        "root" => {
            "requires alternate root filesystem discovery and catalog hierarchy access; portable mode never inspects host rootfs"
        }
        "image" => {
            "requires disk image dissection and mounting; portable mode never mounts or inspects images"
        }
        "image-policy" => "only meaningful with --image= which is not portable",
        "namespace" => {
            "requires systemd journal namespaces; portable mode never discovers host namespaces"
        }
        "synchronize-on-exit" => "requires journald Varlink synchronization on signal exit",
        "sync" => "daemon-only journal synchronization; no journald in portable mode",
        "relinquish-var" => "daemon-only journald storage transition; no journald in portable mode",
        "smart-relinquish-var" => {
            "daemon-only journald storage transition plus host mount inspection"
        }
        "flush" => "daemon-only runtime-to-persistent flush; no journald in portable mode",
        "rotate" => {
            "daemon-only journald rotation request; use --vacuum-* with explicit --directory= instead"
        }
        "list-namespaces" => "requires host journal namespace discovery",
        "list-catalog" => {
            "host catalog database action; portable commands do not read host catalog databases"
        }
        "dump-catalog" => {
            "host catalog database action; portable commands do not read host catalog databases"
        }
        "update-catalog" => {
            "host catalog database mutation; portable commands do not mutate host catalog databases"
        }
        _ => "feature is daemon-only or requires host journal state",
    }
}

#[derive(Parser, Debug)]
#[command(name = "journalctl")]
#[command(about = "Pure-Rust file-backed journalctl subset")]
struct Args {
    #[arg(long = "file")]
    file: Option<PathBuf>,
    #[arg(long = "directory")]
    directory: Option<PathBuf>,
    #[arg(long = "output", default_value = "short")]
    output: OutputModeArg,
    #[arg(long = "list-boots")]
    list_boots: bool,
    #[arg(long = "fields")]
    fields: bool,
    #[arg(short = 'F', long = "field")]
    field: Option<String>,
    #[arg(long = "head")]
    head: Option<usize>,
    #[arg(long = "tail")]
    tail: Option<usize>,
    #[arg(long = "follow")]
    follow: bool,
    #[arg(long = "no-tail")]
    no_tail: bool,
    #[arg(short = 'b', long = "boot", num_args = 0..=1, default_missing_value = "")]
    boot: Option<String>,
    #[arg(short = 'S', long = "since")]
    since: Option<String>,
    #[arg(short = 'U', long = "until")]
    until: Option<String>,
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
    #[arg(long = "verify-key")]
    verify_key: Option<String>,
    #[arg(trailing_var_arg = true)]
    matches: Vec<String>,

    // Parser-recognized v260.1 options that the Rust CLI does not yet
    // dispatch behavior for. They exist so the parser accepts every
    // official long option; downstream validation rejects the ones that
    // are intentionally unsupported in portable mode with a portable
    // unsupported message. This keeps the parser in lock-step with the
    // shared v260.1 manifest under tests/parser-parity/.
    #[arg(long = "system", hide = true)]
    system: bool,
    #[arg(long = "user", hide = true)]
    user: bool,
    #[arg(long = "machine", hide = true)]
    machine: Option<String>,
    #[arg(long = "merge", hide = true)]
    merge: bool,
    #[arg(long = "root", hide = true)]
    root: Option<PathBuf>,
    #[arg(long = "image", hide = true)]
    image: Option<PathBuf>,
    #[arg(long = "image-policy", hide = true)]
    image_policy: Option<String>,
    #[arg(long = "namespace", hide = true)]
    namespace: Option<String>,

    #[arg(long = "cursor", hide = true)]
    cursor: Option<String>,
    #[arg(long = "after-cursor", hide = true)]
    after_cursor: Option<String>,
    #[arg(long = "cursor-file", hide = true)]
    cursor_file: Option<PathBuf>,
    #[arg(long = "this-boot", hide = true)]
    this_boot: bool,
    #[arg(long = "unit", hide = true)]
    unit: Vec<String>,
    #[arg(long = "user-unit", hide = true)]
    user_unit: Vec<String>,
    #[arg(long = "invocation", hide = true)]
    invocation: Option<String>,
    #[arg(long = "identifier", hide = true)]
    identifier: Vec<String>,
    #[arg(long = "exclude-identifier", hide = true)]
    exclude_identifier: Vec<String>,
    #[arg(long = "priority", hide = true)]
    priority: Vec<String>,
    #[arg(long = "facility", hide = true)]
    facility: Vec<String>,
    #[arg(long = "grep", hide = true)]
    grep: Option<String>,
    #[arg(long = "case-sensitive", hide = true, num_args = 0..=1, default_missing_value = "true")]
    case_sensitive: Option<String>,
    #[arg(long = "dmesg", hide = true)]
    dmesg: bool,

    #[arg(short = 'n', long = "lines", hide = true, num_args = 0..=1, default_missing_value = "")]
    lines: Option<String>,
    #[arg(short = 'r', long = "reverse", hide = true)]
    reverse: bool,
    #[arg(long = "show-cursor", hide = true)]
    show_cursor: bool,
    #[arg(long = "utc", hide = true)]
    utc: bool,
    #[arg(short = 'x', long = "catalog", hide = true)]
    catalog: bool,
    #[arg(short = 'W', long = "no-hostname", hide = true)]
    no_hostname: bool,
    #[arg(long = "no-full", hide = true)]
    no_full: bool,
    #[arg(long = "full", hide = true)]
    full: bool,
    #[arg(short = 'a', long = "all", hide = true)]
    all: bool,
    #[arg(long = "truncate-newline", hide = true)]
    truncate_newline: bool,
    #[arg(short = 'q', long = "quiet", hide = true)]
    quiet: bool,
    #[arg(long = "synchronize-on-exit", hide = true)]
    synchronize_on_exit: Option<String>,
    #[arg(long = "no-pager", hide = true)]
    no_pager: bool,
    #[arg(short = 'e', long = "pager-end", hide = true)]
    pager_end: bool,
    #[arg(long = "output-fields", hide = true)]
    output_fields: Option<String>,

    #[arg(long = "interval", hide = true)]
    interval: Option<String>,
    #[arg(long = "force", hide = true)]
    force: bool,
    #[arg(long = "setup-keys", hide = true)]
    setup_keys: bool,

    #[arg(long = "version", hide = true)]
    version: bool,
    #[arg(long = "new-id128", hide = true)]
    new_id128: bool,
    #[arg(long = "list-invocations", hide = true)]
    list_invocations: bool,
    #[arg(long = "list-namespaces", hide = true)]
    list_namespaces: bool,
    #[arg(long = "disk-usage", hide = true)]
    disk_usage: bool,
    #[arg(long = "vacuum-size", hide = true)]
    vacuum_size: Option<String>,
    #[arg(long = "vacuum-files", hide = true)]
    vacuum_files: Option<String>,
    #[arg(long = "vacuum-time", hide = true)]
    vacuum_time: Option<String>,
    #[arg(long = "header", hide = true)]
    header: bool,
    #[arg(long = "list-catalog", hide = true)]
    list_catalog: bool,
    #[arg(long = "dump-catalog", hide = true)]
    dump_catalog: bool,
    #[arg(long = "update-catalog", hide = true)]
    update_catalog: bool,
    #[arg(long = "smart-relinquish-var", hide = true)]
    smart_relinquish_var: bool,
}

#[derive(Debug, Clone, ValueEnum)]
#[value(rename_all = "kebab-case")]
enum OutputModeArg {
    /// Default short journal format.
    Short,
    /// Short format with full timestamp.
    ShortFull,
    /// Short format with ISO timestamp.
    ShortIso,
    /// Short format with precise ISO timestamp.
    ShortIsoPrecise,
    /// Short format with precise timestamp.
    ShortPrecise,
    /// Short format with monotonic timestamp.
    ShortMonotonic,
    /// Short format with monotonic delta.
    ShortDelta,
    /// Short format with Unix timestamp.
    ShortUnix,
    /// Verbose field listing.
    Verbose,
    /// Journal export format.
    Export,
    /// Newline-delimited JSON.
    Json,
    /// Pretty JSON.
    JsonPretty,
    /// Server-sent-event JSON framing.
    JsonSse,
    /// JSON text sequence framing.
    JsonSeq,
    /// MESSAGE-only output.
    Cat,
    /// Short output including unit information.
    WithUnit,
}

impl From<OutputModeArg> for OutputMode {
    fn from(value: OutputModeArg) -> Self {
        match value {
            OutputModeArg::Short
            | OutputModeArg::ShortFull
            | OutputModeArg::ShortIso
            | OutputModeArg::ShortIsoPrecise
            | OutputModeArg::ShortPrecise
            | OutputModeArg::ShortMonotonic
            | OutputModeArg::ShortDelta
            | OutputModeArg::ShortUnix
            | OutputModeArg::Verbose
            | OutputModeArg::Cat
            | OutputModeArg::WithUnit => Self::Default,
            OutputModeArg::Export => Self::Export,
            OutputModeArg::Json
            | OutputModeArg::JsonPretty
            | OutputModeArg::JsonSse
            | OutputModeArg::JsonSeq => Self::Json,
        }
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        exit(1);
    }
}

fn portable_unsupported(feature: &str, reason: &str) -> anyhow::Error {
    anyhow!("journalctl portable mode does not support {feature}: {reason}")
}

fn run() -> Result<()> {
    // nosemgrep: rust.lang.security.args.args -- CLI entry point parses argv; not an authorization boundary.
    let args = Args::parse_from(preprocess_optional_boot_args(std::env::args()));

    // Enforce the v260.1 parser-required interaction rules. These run
    // before any dispatch so the user sees an explicit conflict error.
    enforce_source_exclusivity(&args)?;
    enforce_since_until_order(&args.since, &args.until)?;
    enforce_cursor_source_exclusivity(&args)?;
    enforce_follow_reverse_conflict(&args)?;
    enforce_oldest_lines_conflict(&args)?;
    enforce_boot_merge_conflict(&args)?;

    // Reject intentionally unsupported options with the portable-mode
    // message contract.
    if args.machine.is_some() {
        return Err(portable_unsupported(
            "--machine",
            unsupported_reason("machine"),
        ));
    }
    if args.root.is_some() {
        return Err(portable_unsupported("--root", unsupported_reason("root")));
    }
    if args.image.is_some() {
        return Err(portable_unsupported("--image", unsupported_reason("image")));
    }
    if args.image_policy.is_some() {
        return Err(portable_unsupported(
            "--image-policy",
            unsupported_reason("image-policy"),
        ));
    }
    if args.namespace.is_some() {
        return Err(portable_unsupported(
            "--namespace",
            unsupported_reason("namespace"),
        ));
    }
    if let Some(value) = args.synchronize_on_exit.as_deref() {
        if !value.eq_ignore_ascii_case("false") && !value.eq_ignore_ascii_case("no") {
            return Err(portable_unsupported(
                "--synchronize-on-exit",
                unsupported_reason("synchronize-on-exit"),
            ));
        }
        // false / no is accepted as a no-op per parity matrix.
    }
    if args.sync {
        return Err(portable_unsupported("--sync", unsupported_reason("sync")));
    }
    if args.flush {
        return Err(portable_unsupported("--flush", unsupported_reason("flush")));
    }
    if args.rotate {
        return Err(portable_unsupported(
            "--rotate",
            unsupported_reason("rotate"),
        ));
    }
    if args.relinquish_var {
        return Err(portable_unsupported(
            "--relinquish-var",
            unsupported_reason("relinquish-var"),
        ));
    }
    if args.smart_relinquish_var {
        return Err(portable_unsupported(
            "--smart-relinquish-var",
            unsupported_reason("smart-relinquish-var"),
        ));
    }
    if args.list_namespaces {
        return Err(portable_unsupported(
            "--list-namespaces",
            unsupported_reason("list-namespaces"),
        ));
    }
    if args.list_catalog {
        return Err(portable_unsupported(
            "--list-catalog",
            unsupported_reason("list-catalog"),
        ));
    }
    if args.dump_catalog {
        return Err(portable_unsupported(
            "--dump-catalog",
            unsupported_reason("dump-catalog"),
        ));
    }
    if args.update_catalog {
        return Err(portable_unsupported(
            "--update-catalog",
            unsupported_reason("update-catalog"),
        ));
    }
    if args.setup_keys {
        return Err(portable_unsupported(
            "--setup-keys",
            "FSS key pair generation requires journald integration; portable mode has no host journald",
        ));
    }
    if args.new_id128 {
        return Err(portable_unsupported(
            "--new-id128",
            "deprecated utility action that requires journald integration",
        ));
    }
    if args.disk_usage {
        return Err(portable_unsupported(
            "--disk-usage",
            "requires host journal directory; pass --file or --directory to compute disk usage for explicit input",
        ));
    }
    if args.header {
        return Err(portable_unsupported(
            "--header",
            "header printing requires the journal facade to expose header information for explicit file/directory input",
        ));
    }
    if args.list_invocations {
        return Err(portable_unsupported(
            "--list-invocations",
            "invocation listing requires explicit unit context and journal facade integration",
        ));
    }
    if args.vacuum_size.is_some() || args.vacuum_files.is_some() || args.vacuum_time.is_some() {
        // Maintenance options require --directory=.
        if args.directory.is_none() {
            return Err(portable_unsupported(
                "--vacuum-*",
                "vacuum actions require explicit --directory= input",
            ));
        }
        return Err(portable_unsupported(
            "--vacuum-*",
            "vacuum actions mutate the supplied directory and are not implemented in the portable Rust CLI",
        ));
    }

    // Portable utility actions that do not require a journal file:
    // they print results and exit before any open() is attempted.
    if args.version {
        println!("journalctl (systemd-journal-sdk Rust rewrite)");
        println!("baseline: systemd v260.1 (c0a5a2516d28)");
        println!("portable file-backed mode");
        return Ok(());
    }

    if facility_help_requested(&args.facility) {
        print_facility_help(args.quiet);
        return Ok(());
    }

    let path = args
        .file
        .as_ref()
        .or(args.directory.as_ref())
        .ok_or_else(|| anyhow!("use --file or --directory"))?;

    validate_path_match_arguments(&args.matches)?;

    if args.verify || args.verify_only || args.verify_key.is_some() {
        return run_verify(path, args.verify_key.as_deref());
    }

    let since_usec = parse_optional_timestamp(args.since.as_deref())?;
    let until_usec = parse_optional_timestamp(args.until.as_deref())?;
    let post_filters = CliPostFilters::from_args(&args)?;
    let cursor_control = CursorControl::from_args(&args)?;

    if args.follow {
        let tail = parse_tail_count(args.tail.as_ref(), args.lines.as_deref()).unwrap_or(10);
        return run_follow(
            path,
            &args,
            since_usec,
            until_usec,
            tail,
            &post_filters,
            &cursor_control,
        );
    }

    let mut journal = open_filtered_journal(path, &args)?;

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

    if let Some(field) = args.field.as_deref() {
        let mut stdout = std::io::stdout().lock();
        SdJournalVisitUniqueValues(&mut journal, field, |value| {
            stdout
                .write_all(value)
                .map_err(|err| FacadeError::Other(err.to_string()))?;
            stdout
                .write_all(b"\n")
                .map_err(|err| FacadeError::Other(err.to_string()))?;
            Ok(())
        })
        .map_err(|err| anyhow!("field: {err}"))?;
        return Ok(());
    }

    // If --lines is set, it acts as an alternative --head / --tail.
    if let Some(limit) = parse_lines_limit(args.lines.as_deref())? {
        return match limit {
            LinesLimit::All => show_head_or_all_with_reverse(
                &mut journal,
                None,
                since_usec,
                until_usec,
                args.reverse,
                args.show_cursor,
                &post_filters,
                &cursor_control,
            ),
            LinesLimit::Head(n) => show_head_or_all_with_reverse(
                &mut journal,
                Some(n),
                since_usec,
                until_usec,
                false,
                args.show_cursor,
                &post_filters,
                &cursor_control,
            ),
            LinesLimit::Tail(n) => show_tail_with_reverse(
                &mut journal,
                n,
                since_usec,
                until_usec,
                args.reverse,
                args.show_cursor,
                &post_filters,
                &cursor_control,
            ),
        };
    }

    let reverse = args.reverse;
    if let Some(n) = args.tail {
        show_tail_with_reverse(
            &mut journal,
            n,
            since_usec,
            until_usec,
            reverse,
            args.show_cursor,
            &post_filters,
            &cursor_control,
        )
    } else {
        show_head_or_all_with_reverse(
            &mut journal,
            args.head,
            since_usec,
            until_usec,
            reverse,
            args.show_cursor,
            &post_filters,
            &cursor_control,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinesLimit {
    All,
    Head(usize),
    Tail(usize),
}

/// Parse `--lines=[+]N` per the v260.1 grammar.
/// `--lines` with no value means the official default tail count, 10.
fn parse_lines_limit(value: Option<&str>) -> Result<Option<LinesLimit>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(Some(LinesLimit::Tail(10)));
    }
    if value == "all" {
        return Ok(Some(LinesLimit::All));
    }
    let (oldest, stripped) = if let Some(stripped) = value.strip_prefix('+') {
        (true, stripped)
    } else {
        (false, value)
    };
    let n: usize = stripped
        .parse()
        .map_err(|_| anyhow!("failed to parse --lines value: {value}"))?;
    if oldest {
        Ok(Some(LinesLimit::Head(n)))
    } else {
        Ok(Some(LinesLimit::Tail(n)))
    }
}

/// Resolve the effective tail count for follow mode. Honors `--tail` first,
/// then `--lines`. Returns `None` if neither was supplied.
fn parse_tail_count(tail: Option<&usize>, lines: Option<&str>) -> Option<usize> {
    if let Some(n) = tail {
        return Some(*n);
    }
    match parse_lines_limit(lines).ok().flatten() {
        Some(LinesLimit::Tail(n) | LinesLimit::Head(n)) => Some(n),
        Some(LinesLimit::All) | None => None,
    }
}

#[derive(Debug, Clone)]
struct CursorSeek {
    cursor: String,
    after: bool,
}

#[derive(Debug, Clone)]
struct CursorControl {
    seek: Option<CursorSeek>,
    update_file: Option<PathBuf>,
}

impl CursorControl {
    fn from_args(args: &Args) -> Result<Self> {
        if let Some(cursor) = args.cursor.as_ref() {
            return Ok(Self {
                seek: Some(CursorSeek {
                    cursor: cursor.clone(),
                    after: false,
                }),
                update_file: None,
            });
        }

        if let Some(cursor) = args.after_cursor.as_ref() {
            return Ok(Self {
                seek: Some(CursorSeek {
                    cursor: cursor.clone(),
                    after: true,
                }),
                update_file: None,
            });
        }

        let Some(path) = args.cursor_file.as_ref() else {
            return Ok(Self {
                seek: None,
                update_file: None,
            });
        };

        let cursor = match fs::read_to_string(path) {
            Ok(content) => content.lines().next().unwrap_or("").to_string(),
            Err(err) if err.kind() == ErrorKind::NotFound => String::new(),
            Err(err) => {
                return Err(anyhow!(
                    "Failed to read cursor file {}: {err}",
                    path.display()
                ));
            }
        };

        let seek = (!cursor.is_empty()).then_some(CursorSeek {
            cursor,
            after: true,
        });
        Ok(Self {
            seek,
            update_file: Some(path.clone()),
        })
    }
}

fn enforce_source_exclusivity(args: &Args) -> Result<()> {
    let mut sources = 0usize;
    if args.file.is_some() {
        sources += 1;
    }
    if args.directory.is_some() {
        sources += 1;
    }
    if args.machine.is_some() {
        sources += 1;
    }
    if args.root.is_some() {
        sources += 1;
    }
    if args.image.is_some() {
        sources += 1;
    }
    if sources > 1 {
        return Err(anyhow!(
            "Please specify at most one of -D/--directory=, --file=, -M/--machine=, --root=, --image=."
        ));
    }
    Ok(())
}

fn enforce_since_until_order(since: &Option<String>, until: &Option<String>) -> Result<()> {
    let since_usec = parse_optional_timestamp(since.as_deref())?;
    let until_usec = parse_optional_timestamp(until.as_deref())?;
    if let (Some(s), Some(u)) = (since_usec, until_usec) {
        if s > u {
            return Err(anyhow!("--since= must be before --until=."));
        }
    }
    Ok(())
}

fn enforce_cursor_source_exclusivity(args: &Args) -> Result<()> {
    let mut count = 0;
    if args.cursor.is_some() {
        count += 1;
    }
    if args.after_cursor.is_some() {
        count += 1;
    }
    if args.cursor_file.is_some() {
        count += 1;
    }
    if args.since.is_some() {
        count += 1;
    }
    if count > 1 {
        return Err(anyhow!(
            "Please specify only one of --since=, --cursor=, --cursor-file=, and --after-cursor=."
        ));
    }
    Ok(())
}

fn enforce_follow_reverse_conflict(args: &Args) -> Result<()> {
    if args.follow && args.reverse {
        return Err(anyhow!(
            "Please specify either --reverse or --follow, not both."
        ));
    }
    Ok(())
}

fn enforce_oldest_lines_conflict(args: &Args) -> Result<()> {
    if args.lines.as_deref().is_some_and(|v| v.starts_with('+')) && (args.reverse || args.follow) {
        return Err(anyhow!(
            "--lines=+N is unsupported when --reverse or --follow is specified."
        ));
    }
    Ok(())
}

fn enforce_boot_merge_conflict(args: &Args) -> Result<()> {
    if (args.boot.is_some() || args.this_boot || args.list_boots) && args.merge {
        return Err(anyhow!(
            "Using --boot or --list-boots with --merge is not supported."
        ));
    }
    Ok(())
}

fn preprocess_optional_boot_args<I>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut input = args.into_iter().peekable();
    let mut out = Vec::new();
    while let Some(arg) = input.next() {
        if arg == "--boot" || arg == "-b" {
            if let Some(next) = input.peek() {
                if looks_like_boot_descriptor(next) {
                    let next = input.next().unwrap();
                    out.push(format!("{arg}={next}"));
                    continue;
                }
            }
            out.push(format!("{arg}="));
            continue;
        }
        if arg == "--lines" || arg == "-n" {
            if let Some(next) = input.peek() {
                if looks_like_lines_descriptor(next) {
                    let next = input.next().unwrap();
                    out.push(format!("{arg}={next}"));
                    continue;
                }
            }
            out.push(format!("{arg}="));
            continue;
        }
        out.push(arg);
    }
    out
}

fn looks_like_boot_descriptor(value: &str) -> bool {
    value == "all"
        || value.parse::<isize>().is_ok()
        || parse_boot_id_prefix(value)
            .map(|id| {
                let consumed = if value.as_bytes().get(8) == Some(&b'-') {
                    36
                } else {
                    32
                };
                let rest = &value[consumed..];
                !id.is_empty() && (rest.is_empty() || rest.parse::<isize>().is_ok())
            })
            .unwrap_or(false)
}

fn looks_like_lines_descriptor(value: &str) -> bool {
    if value == "all" {
        return true;
    }
    let value = value.strip_prefix('+').unwrap_or(value);
    !value.is_empty() && value.parse::<usize>().is_ok()
}

fn open_filtered_journal(path: &Path, args: &Args) -> Result<SdJournal> {
    let mut journal =
        SdJournalOpen(&path.to_string_lossy(), 0).map_err(|err| anyhow!("open: {err}"))?;
    let effective_boot = args.boot.as_deref().or(args.this_boot.then_some(""));
    if let Some(boot) = effective_boot {
        if boot.trim() != "all" {
            let boot_id = resolve_boot_id(&mut journal, boot.trim())?;
            if !boot_id.is_empty() {
                let data = parse_match_string(&format!("_BOOT_ID={boot_id}"))
                    .map_err(|err| anyhow!("invalid boot match: {err}"))?;
                SdJournalAddMatch(&mut journal, &data)
                    .map_err(|err| anyhow!("add boot match: {err}"))?;
                SdJournalAddConjunction(&mut journal)
                    .map_err(|err| anyhow!("add boot conjunction: {err}"))?;
            }
        }
    }
    apply_cli_matches(&mut journal, args)?;
    apply_matches(&mut journal, &args.matches)?;
    SdJournalSetOutputMode(&mut journal, args.output.clone().into());
    Ok(journal)
}

fn run_verify(path: &Path, verify_key: Option<&str>) -> Result<()> {
    if verify_key.is_some_and(|key| !valid_verification_key(key)) {
        eprintln!("Failed to parse seed.");
        return Err(anyhow!("failed to parse seed"));
    }

    let mut files = Vec::new();
    let directory_input = path.is_dir();
    if directory_input {
        files = collect_journal_files_for_verify(path)?;
    } else {
        files.push(path.to_path_buf());
    }

    if files.is_empty() {
        if directory_input {
            return Ok(());
        }
        return Err(anyhow!("verify: no journal files found"));
    }

    let mut first_err: Option<anyhow::Error> = None;
    for file in &files {
        let sealed = match is_file_sealed(file) {
            Ok(v) => v,
            Err(err) => {
                if directory_input {
                    continue;
                }
                eprintln!("FAIL: {} ({err})", file.display());
                if first_err.is_none() {
                    first_err = Some(err);
                }
                continue;
            }
        };

        if sealed && verify_key.is_some() {
            match verify_file_with_key(file, verify_key.unwrap()) {
                Ok(()) => eprintln!("PASS: {}", file.display()),
                Err(err) => {
                    eprintln!("FAIL: {} ({err})", file.display());
                    if first_err.is_none() {
                        first_err = Some(anyhow!("{err}"));
                    }
                }
            }
            continue;
        }

        if sealed && verify_key.is_none() {
            eprintln!(
                "Journal file {} has sealing enabled but verification key has not been passed using --verify-key=.",
                file.display()
            );
            eprintln!(
                "FAIL: {} (verification key required for sealed journal file)",
                file.display()
            );
            if first_err.is_none() {
                first_err = Some(anyhow!("verification key required for sealed journal file"));
            }
            continue;
        }

        if let Err(err) = verify_file(file) {
            eprintln!("FAIL: {} ({err})", file.display());
            if first_err.is_none() {
                first_err = Some(anyhow!("{err}"));
            }
            continue;
        }
        eprintln!("PASS: {}", file.display());
    }

    match first_err {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

fn is_file_sealed(path: &Path) -> Result<bool> {
    let reader = FileReader::open(path).map_err(|err| anyhow!("open: {err}"))?;
    Ok(reader.header().compatible_flags & COMPATIBLE_SEALED != 0)
}

fn is_journal_file_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with(".journal")
                || name.ends_with(".journal~")
                || name.ends_with(".journal.zst")
                || name.ends_with(".journal~.zst")
        })
}

fn collect_journal_files_for_verify(path: &Path) -> Result<Vec<PathBuf>> {
    let entries: Vec<_> = std::fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    let mut files = Vec::new();

    for entry in &entries {
        let file_path = entry.path();
        if file_path.is_file() && is_journal_file_name(&file_path) {
            files.push(file_path);
        }
    }

    for entry in &entries {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !is_journal_subdir_name(&name) {
            continue;
        }
        let child_path = entry.path();
        if !child_path.is_dir() {
            continue;
        }
        let Ok(children) = std::fs::read_dir(&child_path) else {
            continue;
        };
        for child in children.flatten() {
            let file_path = child.path();
            if file_path.is_file() && is_journal_file_name(&file_path) {
                files.push(file_path);
            }
        }
    }

    files.sort();
    Ok(files)
}

fn is_journal_subdir_name(name: &str) -> bool {
    if name.contains('.') {
        return false;
    }
    id128_string_valid(name)
}

fn id128_string_valid(s: &str) -> bool {
    match s.len() {
        32 => s.bytes().all(|byte| byte.is_ascii_hexdigit()),
        36 => s.bytes().enumerate().all(|(idx, byte)| {
            if matches!(idx, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        }),
        _ => false,
    }
}

fn valid_verification_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    let mut i = 0usize;
    for _ in 0..12 {
        while i < bytes.len() && bytes[i] == b'-' {
            i += 1;
        }
        if i + 2 > bytes.len() || !is_hex(bytes[i]) || !is_hex(bytes[i + 1]) {
            return false;
        }
        i += 2;
    }
    if i >= bytes.len() || bytes[i] != b'/' {
        return false;
    }
    i += 1;

    let (next, ok) = consume_hex(bytes, i);
    if !ok || next >= bytes.len() || bytes[next] != b'-' {
        return false;
    }
    let (end, ok) = consume_hex(bytes, next + 1);
    ok && end == bytes.len() && bytes[next + 1..end].iter().any(|b| *b != b'0')
}

fn consume_hex(bytes: &[u8], start: usize) -> (usize, bool) {
    let mut i = start;
    while i < bytes.len() && is_hex(bytes[i]) {
        i += 1;
    }
    (i, i > start)
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
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

fn validate_path_match_arguments(matches: &[String]) -> Result<()> {
    for item in matches {
        if item != "+" && !item.contains('=') {
            return Err(portable_unsupported(
                "path match argument",
                "portable mode supports FIELD=VALUE matches and '+' disjunctions only; path matches require host filesystem metadata inspection",
            ));
        }
    }
    Ok(())
}

fn add_field_matches<I, S>(journal: &mut SdJournal, field: &str, values: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut added = false;
    for value in values {
        let data = parse_match_string(&format!("{field}={}", value.as_ref()))
            .map_err(|err| anyhow!("invalid {field} match: {err}"))?;
        SdJournalAddMatch(journal, &data).map_err(|err| anyhow!("add {field} match: {err}"))?;
        added = true;
    }
    if added {
        SdJournalAddConjunction(journal)
            .map_err(|err| anyhow!("add {field} conjunction: {err}"))?;
    }
    Ok(())
}

fn apply_cli_matches(journal: &mut SdJournal, args: &Args) -> Result<()> {
    if args.dmesg {
        add_field_matches(journal, "_TRANSPORT", ["kernel"])?;
    }
    if !args.identifier.is_empty() {
        add_field_matches(
            journal,
            "SYSLOG_IDENTIFIER",
            args.identifier.iter().map(String::as_str),
        )?;
    }

    let priorities = parse_priority_filter(&args.priority)?;
    if !priorities.is_empty() {
        add_field_matches(journal, "PRIORITY", priorities.iter().map(u8::to_string))?;
    }

    let facilities = parse_facility_filter(&args.facility)?;
    if !facilities.is_empty() {
        add_field_matches(
            journal,
            "SYSLOG_FACILITY",
            facilities.iter().map(u8::to_string),
        )?;
    }
    Ok(())
}

#[derive(Debug)]
struct CliPostFilters {
    grep: Option<Regex>,
}

impl CliPostFilters {
    fn from_args(args: &Args) -> Result<Self> {
        Ok(Self {
            // systemd v260.1 parses --exclude-identifier and stores the
            // values, but the file-backed show path never consults them.
            // Keep the option as a parsed no-op for baseline parity.
            grep: compile_grep_filter(args.grep.as_deref(), args.case_sensitive.as_deref())?,
        })
    }

    fn matches(&self, entry: &Entry) -> bool {
        if let Some(regex) = &self.grep {
            let mut matched = false;
            for_each_entry_value(entry, "MESSAGE", |value| {
                if regex.is_match(&String::from_utf8_lossy(value)) {
                    matched = true;
                }
            });
            if !matched {
                return false;
            }
        }
        true
    }
}

fn for_each_entry_value<F>(entry: &Entry, field: &str, mut visitor: F)
where
    F: FnMut(&[u8]),
{
    if let Some(values) = entry.field_values.get(field) {
        for value in values {
            visitor(value);
        }
        return;
    }
    if let Some(value) = entry.fields.get(field) {
        visitor(value);
    }
}

fn compile_grep_filter(
    pattern: Option<&str>,
    case_sensitive: Option<&str>,
) -> Result<Option<Regex>> {
    let Some(pattern) = pattern else {
        return Ok(None);
    };
    let sensitive = match case_sensitive {
        Some(value) => parse_bool_option("--case-sensitive", value)?,
        None => pattern.chars().any(char::is_uppercase),
    };
    RegexBuilder::new(pattern)
        .case_insensitive(!sensitive)
        .build()
        .map(Some)
        .map_err(|err| anyhow!("Bad pattern \"{pattern}\": {err}"))
}

fn parse_bool_option(option: &str, value: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Ok(true),
        "0" | "false" | "no" | "n" | "off" => Ok(false),
        _ => Err(anyhow!("Bad {option}= argument \"{value}\"")),
    }
}

fn parse_priority_filter(values: &[String]) -> Result<Vec<u8>> {
    let Some(value) = values.last() else {
        return Ok(Vec::new());
    };
    if let Some((from, to)) = value.split_once("..") {
        let from = parse_priority_level(from)?;
        let to = parse_priority_level(to)?;
        let start = from.min(to);
        let end = from.max(to);
        return Ok((start..=end).collect());
    }

    let highest = parse_priority_level(value)?;
    Ok((0..=highest).collect())
}

fn parse_priority_level(value: &str) -> Result<u8> {
    let normalized = value.trim().to_ascii_lowercase();
    let level = match normalized.as_str() {
        "emerg" | "panic" => Some(0),
        "alert" => Some(1),
        "crit" | "critical" => Some(2),
        "err" | "error" => Some(3),
        "warning" | "warn" => Some(4),
        "notice" => Some(5),
        "info" => Some(6),
        "debug" => Some(7),
        _ => normalized.parse::<u8>().ok().filter(|value| *value <= 7),
    };
    level.ok_or_else(|| anyhow!("Unknown log level {value}"))
}

const FACILITY_NAMES: [(&str, u8); 20] = [
    ("kern", 0),
    ("user", 1),
    ("mail", 2),
    ("daemon", 3),
    ("auth", 4),
    ("syslog", 5),
    ("lpr", 6),
    ("news", 7),
    ("uucp", 8),
    ("cron", 9),
    ("authpriv", 10),
    ("ftp", 11),
    ("local0", 16),
    ("local1", 17),
    ("local2", 18),
    ("local3", 19),
    ("local4", 20),
    ("local5", 21),
    ("local6", 22),
    ("local7", 23),
];

fn facility_help_requested(values: &[String]) -> bool {
    values
        .iter()
        .flat_map(|value| value.split(','))
        .any(|item| item.trim() == "help")
}

fn print_facility_help(quiet: bool) {
    if !quiet {
        println!("Available facilities:");
    }
    for number in 0..=23 {
        if let Some((name, _)) = FACILITY_NAMES
            .iter()
            .find(|(_, facility)| *facility == number)
        {
            println!("{name}");
        } else {
            println!("{number}");
        }
    }
}

fn parse_facility_filter(values: &[String]) -> Result<Vec<u8>> {
    let mut facilities = Vec::new();
    for value in values {
        for item in value.split(',') {
            let item = item.trim();
            if item.is_empty() || item == "help" {
                continue;
            }
            let facility = parse_facility(item)?;
            if !facilities.contains(&facility) {
                facilities.push(facility);
            }
        }
    }
    facilities.sort_unstable();
    Ok(facilities)
}

fn parse_facility(value: &str) -> Result<u8> {
    if let Ok(number) = value.parse::<u8>() {
        if number <= 23 {
            return Ok(number);
        }
    }
    FACILITY_NAMES
        .iter()
        .find_map(|(name, number)| (*name == value).then_some(*number))
        .ok_or_else(|| anyhow!("Bad --facility= argument \"{value}\"."))
}

fn show_head_or_all_with_reverse(
    journal: &mut SdJournal,
    limit: Option<usize>,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
    reverse: bool,
    show_cursor: bool,
    post_filters: &CliPostFilters,
    cursor_control: &CursorControl,
) -> Result<()> {
    let entries = matching_entries_with_direction(
        journal,
        since_usec,
        until_usec,
        reverse,
        post_filters,
        cursor_control.seek.as_ref(),
    )?;
    let mut stdout = std::io::stdout().lock();
    let mut shown = 0usize;
    let mut last_cursor: Option<String> = None;
    for entry in &entries {
        if limit.is_some_and(|limit| shown >= limit) {
            break;
        }
        let output =
            SdJournalProcessOutput(journal, entry).map_err(|err| anyhow!("output: {err}"))?;
        stdout.write_all(&output)?;
        shown += 1;
        if !entry.cursor.is_empty() {
            last_cursor = Some(entry.cursor.clone());
        }
    }
    finish_cursor_output(
        &mut stdout,
        show_cursor,
        cursor_control.update_file.as_deref(),
        last_cursor.as_deref(),
    )?;
    Ok(())
}

fn show_tail_with_reverse(
    journal: &mut SdJournal,
    limit: usize,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
    reverse: bool,
    show_cursor: bool,
    post_filters: &CliPostFilters,
    cursor_control: &CursorControl,
) -> Result<()> {
    let entries = matching_entries_with_direction(
        journal,
        since_usec,
        until_usec,
        reverse,
        post_filters,
        cursor_control.seek.as_ref(),
    )?;
    let outputs = entries
        .iter()
        .map(|entry| SdJournalProcessOutput(journal, entry).map_err(|err| anyhow!("output: {err}")))
        .collect::<Result<Vec<_>>>()?;
    let start = outputs.len().saturating_sub(limit);
    let mut stdout = std::io::stdout().lock();
    for entry in &outputs[start..] {
        stdout.write_all(entry)?;
    }
    let last_cursor = entries.last().map(|entry| entry.cursor.as_str());
    finish_cursor_output(
        &mut stdout,
        show_cursor,
        cursor_control.update_file.as_deref(),
        last_cursor,
    )?;
    Ok(())
}

fn matching_entries(
    journal: &mut SdJournal,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
    post_filters: &CliPostFilters,
    cursor_seek: Option<&CursorSeek>,
) -> Result<Vec<Entry>> {
    matching_entries_with_direction(
        journal,
        since_usec,
        until_usec,
        false,
        post_filters,
        cursor_seek,
    )
}

fn matching_entries_with_direction(
    journal: &mut SdJournal,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
    reverse: bool,
    post_filters: &CliPostFilters,
    cursor_seek: Option<&CursorSeek>,
) -> Result<Vec<Entry>> {
    if reverse {
        let mut out = Vec::new();
        if let Some(cursor_seek) = cursor_seek {
            if let Some(entry) = seek_cursor_start(journal, cursor_seek, true)? {
                if since_usec.is_none_or(|since| entry.realtime >= since)
                    && until_usec.is_none_or(|until| entry.realtime <= until)
                    && post_filters.matches(&entry)
                {
                    out.push(entry);
                }
            } else {
                return Ok(out);
            }
        } else {
            // Reverse: seek to tail and walk backwards. Bound by --until when
            // supplied; otherwise read until head.
            if let Some(until) = until_usec {
                SdJournalSeekRealtimeUsec(journal, until)
                    .map_err(|err| anyhow!("seek realtime: {err}"))?;
            } else {
                SdJournalSeekTail(journal).map_err(|err| anyhow!("seek tail: {err}"))?;
            }
        }
        loop {
            match SdJournalPrevious(journal).map_err(|err| anyhow!("previous: {err}"))? {
                0 => break,
                _ => {
                    let entry =
                        SdJournalGetEntry(journal).map_err(|err| anyhow!("get entry: {err}"))?;
                    if since_usec.is_some_and(|since| entry.realtime < since) {
                        break;
                    }
                    if since_usec.is_none_or(|since| entry.realtime >= since)
                        && until_usec.is_none_or(|until| entry.realtime <= until)
                        && post_filters.matches(&entry)
                    {
                        out.push(entry);
                    }
                }
            }
        }
        return Ok(out);
    }

    let mut out = Vec::new();
    if let Some(cursor_seek) = cursor_seek {
        if let Some(entry) = seek_cursor_start(journal, cursor_seek, false)? {
            if since_usec.is_none_or(|since| entry.realtime >= since)
                && until_usec.is_none_or(|until| entry.realtime <= until)
                && post_filters.matches(&entry)
            {
                out.push(entry);
            }
        } else {
            return Ok(out);
        }
    } else if let Some(since) = since_usec {
        SdJournalSeekRealtimeUsec(journal, since).map_err(|err| anyhow!("seek realtime: {err}"))?;
    } else {
        SdJournalSeekHead(journal).map_err(|err| anyhow!("seek head: {err}"))?;
    }
    loop {
        match SdJournalNext(journal).map_err(|err| anyhow!("next: {err}"))? {
            0 => break,
            _ => {
                let entry =
                    SdJournalGetEntry(journal).map_err(|err| anyhow!("get entry: {err}"))?;
                if until_usec.is_some_and(|until| entry.realtime > until) {
                    break;
                }
                if since_usec.is_none_or(|since| entry.realtime >= since)
                    && until_usec.is_none_or(|until| entry.realtime <= until)
                    && post_filters.matches(&entry)
                {
                    out.push(entry);
                }
            }
        }
    }
    Ok(out)
}

fn seek_cursor_start(
    journal: &mut SdJournal,
    cursor_seek: &CursorSeek,
    reverse: bool,
) -> Result<Option<Entry>> {
    SdJournalSeekCursor(journal, &cursor_seek.cursor)
        .map_err(|err| anyhow!("seek cursor: {err}"))?;
    if cursor_seek.after {
        match SdJournalTestCursor(journal, &cursor_seek.cursor) {
            Ok(true) => {
                let advanced = if reverse {
                    SdJournalPrevious(journal).map_err(|err| anyhow!("previous: {err}"))?
                } else {
                    SdJournalNext(journal).map_err(|err| anyhow!("next: {err}"))?
                };
                if advanced == 0 {
                    return Ok(None);
                }
            }
            Ok(false) => {}
            Err(FacadeError::NoEntry | FacadeError::EndOfEntries) => return Ok(None),
            Err(err) => return Err(anyhow!("test cursor: {err}")),
        }
    }

    match SdJournalGetEntry(journal) {
        Ok(entry) => Ok(Some(entry)),
        Err(FacadeError::NoEntry | FacadeError::EndOfEntries) => Ok(None),
        Err(err) => Err(anyhow!("get entry: {err}")),
    }
}

fn finish_cursor_output<W: Write>(
    stdout: &mut W,
    show_cursor: bool,
    cursor_file: Option<&Path>,
    cursor: Option<&str>,
) -> Result<()> {
    let Some(cursor) = cursor.filter(|cursor| !cursor.is_empty()) else {
        return Ok(());
    };

    if show_cursor {
        stdout.write_all(b"-- cursor: ")?;
        stdout.write_all(cursor.as_bytes())?;
        stdout.write_all(b"\n")?;
    }

    if let Some(path) = cursor_file {
        write_cursor_file_atomic(path, cursor)?;
    }

    Ok(())
}

fn write_cursor_file_atomic(path: &Path, cursor: &str) -> Result<()> {
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("invalid cursor file path: {}", path.display()))?
        .to_string_lossy();
    let mut tmp = path.to_path_buf();
    tmp.set_file_name(format!(".{file_name}.tmp.{}", std::process::id()));
    fs::write(&tmp, format!("{cursor}\n").as_bytes())
        .map_err(|err| anyhow!("Failed to write new cursor to {}: {err}", path.display()))?;
    fs::rename(&tmp, path)
        .map_err(|err| anyhow!("Failed to write new cursor to {}: {err}", path.display()))?;
    Ok(())
}

#[derive(Clone)]
struct BootEntry {
    boot_id: String,
    first_entry: u64,
    last_entry: u64,
}

fn collect_boots(journal: &mut SdJournal) -> Result<Vec<BootEntry>> {
    use std::collections::HashMap;

    SdJournalSeekHead(journal).map_err(|err| anyhow!("seek head: {err}"))?;
    let mut boots: HashMap<String, BootEntry> = HashMap::new();
    loop {
        match SdJournalNext(journal).map_err(|err| anyhow!("next: {err}"))? {
            0 => break,
            _ => {
                let entry =
                    SdJournalGetEntry(journal).map_err(|err| anyhow!("get entry: {err}"))?;
                let boot_id = hex::encode(entry.boot_id);
                if boot_id.chars().all(|ch| ch == '0') {
                    continue;
                }
                boots
                    .entry(boot_id.clone())
                    .and_modify(|boot| {
                        boot.first_entry = boot.first_entry.min(entry.realtime);
                        boot.last_entry = boot.last_entry.max(entry.realtime);
                    })
                    .or_insert(BootEntry {
                        boot_id,
                        first_entry: entry.realtime,
                        last_entry: entry.realtime,
                    });
            }
        }
    }
    let mut out: Vec<_> = boots.into_values().collect();
    out.sort_by(|a, b| {
        a.first_entry
            .cmp(&b.first_entry)
            .then_with(|| a.boot_id.cmp(&b.boot_id))
    });
    Ok(out)
}

fn resolve_boot_id(journal: &mut SdJournal, descriptor: &str) -> Result<String> {
    if descriptor == "all" {
        return Ok(String::new());
    }
    let (boot_id, offset) = parse_boot_descriptor(descriptor)?;
    let boots = collect_boots(journal)?;
    if boots.is_empty() {
        return Err(anyhow!(
            "no journal boot entry found for the specified boot"
        ));
    }
    let target = if !boot_id.is_empty() {
        let base = boots
            .iter()
            .position(|boot| boot.boot_id == boot_id)
            .ok_or_else(|| {
                anyhow!(
                    "no journal boot entry found for the specified boot ({}{offset:+})",
                    boot_id
                )
            })?;
        base as isize + offset
    } else if offset > 0 {
        offset - 1
    } else {
        boots.len() as isize - 1 + offset
    };
    if target < 0 || target as usize >= boots.len() {
        return Err(anyhow!(
            "no journal boot entry found for the specified boot ({}{offset:+})",
            boot_id
        ));
    }
    Ok(boots[target as usize].boot_id.clone())
}

fn parse_boot_descriptor(descriptor: &str) -> Result<(String, isize)> {
    if descriptor.is_empty() {
        return Ok((String::new(), 0));
    }
    let (boot_id, rest) = if let Some(id) = parse_boot_id_prefix(descriptor) {
        let consumed = if descriptor.as_bytes().get(8) == Some(&b'-') {
            36
        } else {
            32
        };
        (id, &descriptor[consumed..])
    } else {
        (String::new(), descriptor)
    };
    let offset = if rest.is_empty() {
        0
    } else {
        rest.parse::<isize>()
            .map_err(|_| anyhow!("failed to parse boot descriptor: {descriptor}"))?
    };
    Ok((boot_id, offset))
}

fn parse_boot_id_prefix(value: &str) -> Option<String> {
    if value.len() >= 32 && value[..32].bytes().all(|b| b.is_ascii_hexdigit()) {
        return Some(value[..32].to_ascii_lowercase());
    }
    if value.len() >= 36 {
        let candidate = &value[..36];
        let valid = candidate.bytes().enumerate().all(|(idx, b)| {
            if matches!(idx, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        });
        if valid {
            return Some(candidate.replace('-', "").to_ascii_lowercase());
        }
    }
    None
}

fn parse_optional_timestamp(value: Option<&str>) -> Result<Option<u64>> {
    value
        .filter(|v| !v.trim().is_empty())
        .map(parse_timestamp_usec)
        .transpose()
}

fn parse_timestamp_usec(value: &str) -> Result<u64> {
    let value = value.trim();
    match value {
        "now" => return Ok(Local::now().timestamp_micros() as u64),
        "today" | "yesterday" | "tomorrow" => {
            let today = Local::now().date_naive();
            let date = match value {
                "yesterday" => today.pred_opt().ok_or_else(|| anyhow!("date underflow"))?,
                "tomorrow" => today.succ_opt().ok_or_else(|| anyhow!("date overflow"))?,
                _ => today,
            };
            return local_datetime_to_usec(date.and_hms_opt(0, 0, 0).unwrap());
        }
        _ => {}
    }
    if let Some(epoch) = value.strip_prefix('@') {
        return parse_epoch_timestamp_usec(epoch);
    }
    if matches!(value.as_bytes().first(), Some(b'+' | b'-'))
        && !value
            .get(1..5)
            .is_some_and(|year| year.bytes().all(|b| b.is_ascii_digit()))
    {
        let delta = parse_duration_usec(&value[1..])? as i64;
        let now = Local::now().timestamp_micros();
        return Ok(if value.starts_with('+') {
            now + delta
        } else {
            now - delta
        } as u64);
    }
    parse_local_timestamp_usec(value)
}

fn parse_epoch_timestamp_usec(value: &str) -> Result<u64> {
    let (whole, frac) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(anyhow!("failed to parse timestamp: @{value}"));
    }
    let seconds = whole.parse::<u64>()?;
    let mut frac_padded = frac.to_string();
    frac_padded.push_str("000000");
    let usec = frac_padded[..6].parse::<u64>()?;
    Ok(seconds * 1_000_000 + usec)
}

fn parse_local_timestamp_usec(value: &str) -> Result<u64> {
    for fmt in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(value, fmt) {
            return local_datetime_to_usec(dt);
        }
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return local_datetime_to_usec(date.and_hms_opt(0, 0, 0).unwrap());
    }
    for fmt in ["%H:%M:%S%.f", "%H:%M:%S", "%H:%M"] {
        if let Ok(time) = NaiveTime::parse_from_str(value, fmt) {
            let date = Local::now().date_naive();
            return local_datetime_to_usec(date.and_time(time));
        }
    }
    Err(anyhow!("failed to parse timestamp: {value}"))
}

fn local_datetime_to_usec(dt: NaiveDateTime) -> Result<u64> {
    let local = Local
        .from_local_datetime(&dt)
        .earliest()
        .ok_or_else(|| anyhow!("failed to parse local timestamp"))?;
    Ok(local.timestamp_micros() as u64)
}

fn parse_duration_usec(value: &str) -> Result<u64> {
    let mut total = 0_f64;
    let bytes = value.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        if start == i {
            return Err(anyhow!("failed to parse duration: {value}"));
        }
        let number = value[start..i].parse::<f64>()?;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let unit_start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        let unit = if unit_start == i {
            "s"
        } else {
            &value[unit_start..i]
        };
        total += number * duration_unit_multiplier(unit)?;
    }
    if total == 0_f64 {
        return Err(anyhow!("failed to parse duration: {value}"));
    }
    Ok(total as u64)
}

fn duration_unit_multiplier(unit: &str) -> Result<f64> {
    match unit.to_ascii_lowercase().as_str() {
        "us" | "usec" | "usecs" => Ok(1_f64),
        "ms" | "msec" | "msecs" => Ok(1_000_f64),
        "s" | "sec" | "secs" | "second" | "seconds" => Ok(1_000_000_f64),
        "m" | "min" | "mins" | "minute" | "minutes" => Ok(60_000_000_f64),
        "h" | "hr" | "hour" | "hours" => Ok(3_600_000_000_f64),
        "d" | "day" | "days" => Ok(86_400_000_000_f64),
        "w" | "week" | "weeks" => Ok(604_800_000_000_f64),
        _ => Err(anyhow!("failed to parse duration unit: {unit}")),
    }
}

fn scan_follow_snapshot(
    path: &Path,
    args: &Args,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
    post_filters: &CliPostFilters,
    cursor_control: &CursorControl,
) -> Vec<(String, Vec<u8>)> {
    let Ok(mut journal) = open_filtered_journal(path, args) else {
        return Vec::new();
    };
    let Ok(entries) = matching_entries(
        &mut journal,
        since_usec,
        until_usec,
        post_filters,
        cursor_control.seek.as_ref(),
    ) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let output = SdJournalProcessOutput(&mut journal, entry).ok()?;
            Some((entry.cursor.clone(), output))
        })
        .collect()
}

fn run_follow(
    path: &Path,
    args: &Args,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
    tail: usize,
    post_filters: &CliPostFilters,
    cursor_control: &CursorControl,
) -> Result<()> {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    let initial = scan_follow_snapshot(
        path,
        args,
        since_usec,
        until_usec,
        post_filters,
        cursor_control,
    );
    for (cursor, _) in &initial {
        seen.insert(cursor.clone());
    }
    let start = if args.no_tail || since_usec.is_some() {
        0
    } else {
        initial.len().saturating_sub(tail)
    };
    {
        let mut stdout = std::io::stdout().lock();
        for (_, output) in &initial[start..] {
            stdout.write_all(output)?;
        }
    }

    loop {
        thread::sleep(Duration::from_millis(100));
        let snapshot = scan_follow_snapshot(
            path,
            args,
            since_usec,
            until_usec,
            post_filters,
            cursor_control,
        );
        let mut stdout = std::io::stdout().lock();
        for (cursor, output) in snapshot {
            if seen.insert(cursor) {
                stdout.write_all(&output)?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use journal_core::file::{JournalFile, JournalFileOptions, JournalWriter, MmapMut};
    use journal_core::repository::File as RepoFile;
    use journal_core::seal::SealOptions;
    use std::path::Path;

    const VALID_FSS_VERIFICATION_KEY: &str = "c262bd-85187f-0b1b04-877cc5/1c7af8-35a4e900";

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../")
            .canonicalize()
            .expect("repo root")
    }

    #[test]
    fn verify_valid_fixture() {
        let path = repo_root().join("fixtures/systemd/test-data/no-rtc/system.journal.zst");
        run_verify(&path, None).expect("verify should pass");
    }

    #[test]
    fn verify_corrupted_fixture() {
        let path =
            repo_root().join("fixtures/systemd/test-data/corrupted/zstd-truncated-frame.zst");
        let err = run_verify(&path, None).expect_err("verify should fail");
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("corrupt") || msg.to_lowercase().contains("fail"),
            "expected corrupt/fail in error, got: {msg}"
        );
    }

    #[test]
    fn verify_key_unsealed_fixture() {
        let path = repo_root().join("fixtures/systemd/test-data/no-rtc/system.journal.zst");
        run_verify(&path, Some(VALID_FSS_VERIFICATION_KEY)).expect("verify should pass");
    }

    #[test]
    fn verify_key_invalid_seed() {
        let path = repo_root().join("fixtures/systemd/test-data/no-rtc/system.journal.zst");
        let err = run_verify(&path, Some("synthetic-test-key")).expect_err("verify should fail");
        assert!(
            err.to_string().contains("failed to parse seed"),
            "expected parse seed error, got: {err}"
        );
    }

    #[test]
    fn verify_key_empty_seed() {
        let path = repo_root().join("fixtures/systemd/test-data/no-rtc/system.journal.zst");
        let err = run_verify(&path, Some("")).expect_err("verify should fail");
        assert!(
            err.to_string().contains("failed to parse seed"),
            "expected parse seed error, got: {err}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn verify_directory_follows_symlink_and_skips_directories() {
        let fixture = repo_root().join("fixtures/systemd/test-data/no-rtc/system.journal.zst");
        let dir = tempfile::tempdir().expect("create temp dir");
        let linked = dir.path().join("linked.journal.zst");
        std::os::unix::fs::symlink(&fixture, &linked).expect("symlink fixture");
        std::fs::create_dir(dir.path().join("skip.journal.zst")).expect("create skipped dir");

        run_verify(dir.path(), None).expect("directory verification should pass");
    }

    #[test]
    fn verify_directory_empty() {
        let dir = tempfile::tempdir().expect("create temp dir");
        run_verify(dir.path(), None).expect("empty directory verification should pass");
    }

    fn test_uuid(n: u8) -> uuid::Uuid {
        let mut bytes = [0u8; 16];
        bytes[15] = n;
        uuid::Uuid::from_bytes(bytes)
    }

    fn verification_key(opts: &SealOptions) -> String {
        let seed_hex = opts
            .seed
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        let start = opts.start_usec / opts.interval_usec;
        format!(
            "{seed_hex}/{start:x}-{interval:x}",
            interval = opts.interval_usec
        )
    }

    fn write_sealed_file(path: &Path) -> SealOptions {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create journal parent");
        }
        let repo_file = RepoFile::from_path(path)
            .unwrap_or_else(|| panic!("test journal path should parse: {}", path.display()));
        let seal = SealOptions::new([0u8; 12], 1_000_000, 1_000_000);
        let mut journal_file = JournalFile::<MmapMut>::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_seal(seal.clone()),
        )
        .expect("create sealed journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=sealed verify".as_slice()],
                1_500_000,
                100,
            )
            .expect("write sealed entry");
        journal_file.sync().expect("sync journal");
        seal
    }

    #[test]
    fn verify_sealed_without_key_requires_key() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir
            .path()
            .join("00000000-0000-0000-0000-000000000001/system.journal");
        write_sealed_file(&path);
        let err = run_verify(&path, None).expect_err("verify should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("verification key"),
            "expected verification key error, got: {msg}"
        );
    }

    #[test]
    fn verify_key_sealed_passes_and_wrong_key_fails() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir
            .path()
            .join("00000000-0000-0000-0000-000000000001/system.journal");
        let seal = write_sealed_file(&path);
        let key = verification_key(&seal);

        run_verify(&path, Some(&key)).expect("sealed verify with key should pass");
        run_verify(&path, Some("000000000000000000000001/1-f4240"))
            .expect_err("wrong verification key should fail");
    }

    // -- SOW-0121 parser parity tests ----------------------------------
    //
    // Every official systemd v260.1 long option must be recognized by the
    // parser. The set is enumerated by the shared manifest at
    // tests/parser-parity/v260-manifest.json. The list is duplicated
    // here (intentionally) so the parser contract is enforced by Rust
    // unit tests in addition to the shared Python harness.

    const OFFICIAL_LONG_OPTIONS: &[(&str, bool)] = &[
        ("system", false),
        ("user", false),
        ("machine", true),
        ("merge", false),
        ("directory", true),
        ("file", true),
        ("root", true),
        ("image", true),
        ("image-policy", true),
        ("namespace", true),
        ("since", true),
        ("until", true),
        ("cursor", true),
        ("after-cursor", true),
        ("cursor-file", true),
        ("boot", false),
        ("this-boot", false),
        ("unit", true),
        ("user-unit", true),
        ("invocation", true),
        ("identifier", true),
        ("exclude-identifier", true),
        ("priority", true),
        ("facility", true),
        ("grep", true),
        ("case-sensitive", false),
        ("dmesg", false),
        ("output", true),
        ("output-fields", true),
        ("lines", false),
        ("reverse", false),
        ("show-cursor", false),
        ("utc", false),
        ("catalog", false),
        ("no-hostname", false),
        ("no-full", false),
        ("full", false),
        ("all", false),
        ("follow", false),
        ("no-tail", false),
        ("truncate-newline", false),
        ("quiet", false),
        ("synchronize-on-exit", true),
        ("no-pager", false),
        ("pager-end", false),
        ("verify-key", true),
        ("interval", true),
        ("force", false),
        ("setup-keys", false),
        ("help", false),
        ("version", false),
        ("new-id128", false),
        ("fields", false),
        ("field", true),
        ("list-boots", false),
        ("list-invocations", false),
        ("list-namespaces", false),
        ("disk-usage", false),
        ("vacuum-size", true),
        ("vacuum-files", true),
        ("vacuum-time", true),
        ("verify", false),
        ("sync", false),
        ("relinquish-var", false),
        ("smart-relinquish-var", false),
        ("flush", false),
        ("rotate", false),
        ("header", false),
        ("list-catalog", false),
        ("dump-catalog", false),
        ("update-catalog", false),
    ];

    const OFFICIAL_OUTPUT_MODES: &[&str] = &[
        "short",
        "short-full",
        "short-iso",
        "short-iso-precise",
        "short-precise",
        "short-monotonic",
        "short-delta",
        "short-unix",
        "verbose",
        "export",
        "json",
        "json-pretty",
        "json-sse",
        "json-seq",
        "cat",
        "with-unit",
    ];

    /// Run the CLI binary with the supplied argv and capture output.
    /// The binary is launched as a subprocess so we can validate parser
    /// behavior end-to-end without touching the host journal state.
    fn run_cli(args: &[&str]) -> std::process::Output {
        // Prefer the cargo-provided integration-test binary path. Fall
        // back to the workspace debug build for environments where the
        // variable is not exported (e.g. when running `cargo test`
        // directly on the binary target).
        let bin = std::env::var("CARGO_BIN_EXE_journalctl").unwrap_or_else(|_| {
            repo_root()
                .join(".local/cargo-target/debug/journalctl")
                .to_string_lossy()
                .into_owned()
        });
        std::process::Command::new(bin)
            .args(args)
            .output()
            .expect("spawn journalctl binary")
    }

    #[test]
    fn unrecognized_option_is_rejected() {
        let out = run_cli(&["--not-an-official-option"]);
        assert!(
            !out.status.success(),
            "expected non-zero exit for unknown option"
        );
        let combined = String::from_utf8_lossy(&out.stderr).into_owned()
            + &String::from_utf8_lossy(&out.stdout);
        assert!(
            combined.contains("unexpected argument") || combined.contains("unrecognized"),
            "expected unknown-option error, got: {combined}"
        );
    }

    #[test]
    fn every_official_long_option_is_parsed() {
        for (opt, takes_value) in OFFICIAL_LONG_OPTIONS {
            let flag = match (*opt, *takes_value) {
                ("output", true) => "--output=short".to_string(),
                ("synchronize-on-exit", true) => "--synchronize-on-exit=false".to_string(),
                (_, true) => format!("--{opt}=placeholder"),
                (_, false) => format!("--{opt}"),
            };
            let argv: Vec<String> = vec!["journalctl".to_string(), flag];
            let parsed = Args::try_parse_from(&argv);
            if *opt == "help" {
                let err = parsed.expect_err("--help must exit through clap help");
                assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
                continue;
            }
            assert!(
                parsed.is_ok(),
                "parser rejected official option --{opt}: {:?}",
                parsed.err().map(|e| e.to_string())
            );
        }
    }

    #[test]
    fn every_official_output_mode_is_accepted() {
        for mode in OFFICIAL_OUTPUT_MODES {
            let argv: Vec<String> = vec!["journalctl".to_string(), format!("--output={mode}")];
            let parsed = Args::try_parse_from(&argv);
            assert!(
                parsed.is_ok(),
                "parser rejected output mode {mode}: {:?}",
                parsed.err().map(|e| e.to_string())
            );
        }
    }

    #[test]
    fn lines_limit_parser_preserves_systemd_direction() {
        assert_eq!(parse_lines_limit(None).unwrap(), None);
        assert_eq!(
            parse_lines_limit(Some("")).unwrap(),
            Some(LinesLimit::Tail(10))
        );
        assert_eq!(
            parse_lines_limit(Some("25")).unwrap(),
            Some(LinesLimit::Tail(25))
        );
        assert_eq!(
            parse_lines_limit(Some("+25")).unwrap(),
            Some(LinesLimit::Head(25))
        );
        assert_eq!(
            parse_lines_limit(Some("all")).unwrap(),
            Some(LinesLimit::All)
        );
        assert!(parse_lines_limit(Some("not-a-number")).is_err());
    }

    #[test]
    fn lines_optional_argument_does_not_consume_match() {
        let args = preprocess_optional_boot_args([
            "journalctl".to_string(),
            "--lines".to_string(),
            "TEST_ID=journalctl-query".to_string(),
        ]);
        assert_eq!(
            args,
            vec![
                "journalctl".to_string(),
                "--lines=".to_string(),
                "TEST_ID=journalctl-query".to_string()
            ]
        );

        let args = preprocess_optional_boot_args([
            "journalctl".to_string(),
            "--lines".to_string(),
            "+2".to_string(),
            "TEST_ID=journalctl-query".to_string(),
        ]);
        assert_eq!(
            args,
            vec![
                "journalctl".to_string(),
                "--lines=+2".to_string(),
                "TEST_ID=journalctl-query".to_string()
            ]
        );
    }

    #[test]
    fn portable_unsupported_message_format() {
        // For each intentionally unsupported daemon-only action, the
        // binary must exit non-zero and emit the portable-mode message
        // class.
        let unsupported = [
            "--sync",
            "--flush",
            "--rotate",
            "--relinquish-var",
            "--smart-relinquish-var",
            "--list-namespaces",
            "--list-catalog",
            "--dump-catalog",
            "--update-catalog",
        ];
        for opt in unsupported {
            let out = run_cli(&[opt]);
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            assert!(
                !out.status.success(),
                "expected non-zero exit for {opt}, stderr={stderr}"
            );
            assert!(
                stderr.contains("portable mode does not support"),
                "expected portable message for {opt}, stderr={stderr}"
            );
        }
    }

    #[test]
    fn portable_unsupported_for_source_options() {
        let unsupported_with_value = ["--machine", "--root", "--image", "--namespace"];
        for opt in unsupported_with_value {
            let out = run_cli(&[opt, "/dev/null"]);
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            assert!(
                !out.status.success(),
                "expected non-zero exit for {opt}, stderr={stderr}"
            );
            assert!(
                stderr.contains("portable mode does not support"),
                "expected portable message for {opt}, stderr={stderr}"
            );
        }
    }

    #[test]
    fn source_exclusivity_enforced() {
        let out = run_cli(&["--directory=/tmp", "--file=/tmp/x.journal"]);
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(!out.status.success(), "expected non-zero exit");
        assert!(
            stderr.contains("at most one of")
                && stderr.contains("--directory")
                && stderr.contains("--file"),
            "expected source exclusivity error, got: {stderr}"
        );
    }

    #[test]
    fn since_until_order_enforced() {
        let out = run_cli(&[
            "--file=/tmp/x.journal",
            "--since=2020-01-02",
            "--until=2020-01-01",
        ]);
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(!out.status.success(), "expected non-zero exit");
        assert!(
            stderr.contains("--since= must be before --until="),
            "expected since/until order error, got: {stderr}"
        );
    }

    #[test]
    fn follow_reverse_conflict_enforced() {
        let out = run_cli(&["--file=/tmp/x.journal", "--follow", "--reverse"]);
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(!out.status.success(), "expected non-zero exit");
        assert!(
            stderr.contains("either --reverse or --follow, not both"),
            "expected follow/reverse conflict, got: {stderr}"
        );
    }

    #[test]
    fn synchronize_on_exit_false_accepted_as_noop() {
        let out = run_cli(&["--synchronize-on-exit=false"]);
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        // The CLI should not produce a portable unsupported error when
        // --synchronize-on-exit=false is supplied (per parity matrix).
        assert!(
            !stderr.contains("portable mode does not support --synchronize-on-exit"),
            "expected false to be accepted, stderr={stderr}"
        );
    }

    #[test]
    fn synchronize_on_exit_true_rejected() {
        let out = run_cli(&["--synchronize-on-exit=true"]);
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(!out.status.success(), "expected non-zero exit");
        assert!(
            stderr.contains("portable mode does not support --synchronize-on-exit"),
            "expected portable unsupported message, stderr={stderr}"
        );
    }

    #[test]
    fn vacuum_without_directory_is_rejected() {
        let out = run_cli(&["--vacuum-size=1G"]);
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(!out.status.success(), "expected non-zero exit");
        assert!(
            stderr.contains("portable mode does not support --vacuum-*"),
            "expected portable message, stderr={stderr}"
        );
    }

    #[test]
    fn version_prints_baseline_metadata() {
        let out = run_cli(&["--version"]);
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(
            out.status.success(),
            "expected success, stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            stdout.contains("v260.1") && stdout.contains("baseline"),
            "expected version banner, got: {stdout}"
        );
    }

    #[test]
    fn boot_merge_conflict_enforced() {
        let out = run_cli(&["--file=/tmp/x.journal", "--boot", "--merge"]);
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(!out.status.success(), "expected non-zero exit");
        assert!(
            stderr.contains("--boot or --list-boots with --merge is not supported"),
            "expected boot/merge conflict, got: {stderr}"
        );
    }
}
