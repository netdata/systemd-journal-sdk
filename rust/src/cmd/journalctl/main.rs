mod output;

use anyhow::{Result, anyhow};
use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use clap::{Parser, ValueEnum};
use journal::{
    Entry, FacadeError, FileHeader, FileReader, OutputMode, SdJournal, SdJournalAddConjunction,
    SdJournalAddDisjunction, SdJournalAddMatch, SdJournalEnumerateFields, SdJournalFlushMatches,
    SdJournalGetEntry, SdJournalNext, SdJournalOpenDirectory, SdJournalOpenFiles,
    SdJournalPrevious, SdJournalSeekCursor, SdJournalSeekHead, SdJournalSeekRealtimeUsec,
    SdJournalSeekTail, SdJournalSetOutputMode, SdJournalTestCursor, SdJournalVisitUniqueValues,
    parse_match_string, verify_file, verify_file_with_key,
};
use output::{OutputOptions, OutputRenderer};
use regex::{Regex, RegexBuilder};
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// HEADER_COMPATIBLE_SEALED from systemd journal-def.h
const COMPATIBLE_SEALED: u32 = 1;
const COMPATIBLE_TAIL_ENTRY_BOOT_ID: u32 = 1 << 1;
const COMPATIBLE_SEALED_CONTINUOUS: u32 = 1 << 2;
const INCOMPATIBLE_COMPRESSED_XZ: u32 = 1 << 0;
const INCOMPATIBLE_COMPRESSED_LZ4: u32 = 1 << 1;
const INCOMPATIBLE_KEYED_HASH: u32 = 1 << 2;
const INCOMPATIBLE_COMPRESSED_ZSTD: u32 = 1 << 3;
const INCOMPATIBLE_COMPACT: u32 = 1 << 4;
const HASH_ITEM_SIZE: u64 = 16;
const JOURNAL_HEADER_SIZE: u64 = 272;
const JOURNAL_HEADER_N_ENTRIES_OFFSET: u64 = 152;
const HEADER_CHAIN_DEPTH_MAX: u64 = 100;
const COREDUMP_MESSAGE_ID: &str = "fc2e22bc6ee647b6b90729ab34a250b1";
const INVOCATION_ID_FIELDS: [&str; 4] = [
    "_SYSTEMD_INVOCATION_ID",
    "OBJECT_SYSTEMD_INVOCATION_ID",
    "INVOCATION_ID",
    "USER_INVOCATION_ID",
];
const SYSTEM_UNIT_FIELDS_FULL: &[&str] = &[
    "_SYSTEMD_UNIT",
    "UNIT",
    "OBJECT_SYSTEMD_UNIT",
    "COREDUMP_UNIT",
    "_SYSTEMD_SLICE",
];
const USER_UNIT_FIELDS_FULL: &[&str] = &[
    "_SYSTEMD_USER_UNIT",
    "USER_UNIT",
    "OBJECT_SYSTEMD_USER_UNIT",
    "COREDUMP_USER_UNIT",
    "_SYSTEMD_USER_SLICE",
];
const UNIT_SUFFIXES: &[&str] = &[
    ".automount",
    ".device",
    ".mount",
    ".path",
    ".scope",
    ".service",
    ".slice",
    ".socket",
    ".swap",
    ".target",
    ".timer",
];
const OUTPUT_MODE_HELP_LIST: &[&str] = &[
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

const OFFICIAL_OPTION_SURFACE_HELP: &str = r#"Official systemd v260.1 option reference:
  --system                    Show system journals when host-backed; no-op for explicit files.
  --user                      Show user journals when host-backed; no-op for explicit files.
  -M, --machine=CONTAINER     Unsupported: requires host/container journal access.
  -m, --merge                 Merge all available explicit input files.
  -D, --directory=DIR         Read journal files from DIR.
  -i, --file=FILE             Read the specified journal file.
  --root=ROOT                 Unsupported: requires alternate root filesystem discovery.
  --image=IMAGE               Unsupported: requires disk image mounting.
  --image-policy=POLICY       Unsupported without --image.
  --namespace=NAMESPACE       Unsupported: requires host journal namespaces.
  -S, --since=TIME            Show entries not older than TIME.
  -U, --until=TIME            Show entries not newer than TIME.
  -c, --cursor=CURSOR         Start at the specified cursor.
  --after-cursor=CURSOR       Start after the specified cursor when it matches exactly.
  --cursor-file=FILE          Read/update a cursor file.
  -b, --boot[=ID|OFFSET|all]  Restrict output to a boot.
  --this-boot                 Restrict output to the current boot selector.
  -u, --unit=UNIT             Match system unit fields.
  --user-unit=UNIT            Match user unit fields.
  --invocation=ID             Match a specific invocation ID.
  -I                          Match the latest invocation for the selected unit.
  -t, --identifier=SYSLOG_IDENTIFIER  Match syslog identifier.
  -T, --exclude-identifier=SYSLOG_IDENTIFIER  Exclude syslog identifier at render time.
  -p, --priority=RANGE        Match priority/facility priority range.
  --facility=RANGE            Match syslog facility range.
  -g, --grep=PATTERN          Filter MESSAGE by pattern.
  --case-sensitive[=BOOL]     Control grep case sensitivity.
  -k, --dmesg                 Match kernel transport.
  -o, --output=MODE           Select output mode.
  --output-fields=FIELDS      Select fields for verbose/export/json/cat modes.
  -n, --lines[=N|+N|all]      Select newest, oldest, or all rows.
  -r, --reverse               Show newest rows first.
  --show-cursor               Print the cursor after output.
  --utc                       Render timestamps in UTC.
  -x, --catalog               Accepted for CLI parity; catalogs are host-backed.
  -W, --no-hostname           Suppress host names in short output.
  --no-full                   Ellipsize long fields.
  -l, --full                  Show long fields without ellipsizing.
  -a, --all                   Show all field bytes where supported.
  -f, --follow                Follow explicit files/directories by portable polling.
  --no-tail                   Do not imply tail mode with --follow.
  --truncate-newline          Truncate MESSAGE at the first newline.
  -q, --quiet                 Suppress status/separator output.
  --synchronize-on-exit[=BOOL] Unsupported when true: requires journald Varlink.
  --no-pager                  Accepted for CLI parity; output is never paged.
  -e, --pager-end             Start near the end.
  --verify-key=KEY            Verify sealed journals with KEY.
  --interval=TIME             Accepted for setup-keys parser parity.
  --force                     Accepted for setup-keys parser parity.
  --setup-keys                Unsupported: host FSS key setup.
  -h, --help                  Print help.
  --version                   Print rewrite and baseline version.
  --new-id128                 Print a new ID128.
  -N, --fields                List field names.
  -F, --field=FIELD           List unique values for FIELD.
  --list-boots                List boots from explicit input.
  --list-invocations          List invocations for the selected unit context.
  --list-namespaces           Unsupported: requires host namespace discovery.
  --disk-usage                Report disk usage for explicit input.
  --vacuum-size=BYTES         Vacuum archived files in an explicit directory.
  --vacuum-files=N            Vacuum archived files by count in an explicit directory.
  --vacuum-time=TIME          Vacuum archived files by age in an explicit directory.
  --verify                    Verify explicit input files.
  --sync                      Unsupported: daemon-only journal synchronization.
  --relinquish-var            Unsupported: daemon-only storage transition.
  --smart-relinquish-var      Unsupported: daemon-only storage transition.
  --flush                     Unsupported: daemon-only runtime-to-persistent flush.
  --rotate                    Unsupported: daemon-only rotation request.
  --header                    Print journal file headers.
  --list-catalog              Unsupported: host catalog database.
  --dump-catalog              Unsupported: host catalog database.
  --update-catalog            Unsupported: host catalog database mutation."#;

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
#[command(after_help = OFFICIAL_OPTION_SURFACE_HELP)]
struct Args {
    #[arg(short = 'i', long = "file")]
    file: Vec<PathBuf>,
    #[arg(short = 'D', long = "directory")]
    directory: Option<PathBuf>,
    #[arg(short = 'o', long = "output", default_value = "short")]
    output: OutputModeArg,
    #[arg(long = "list-boots")]
    list_boots: bool,
    #[arg(short = 'N', long = "fields")]
    fields: bool,
    #[arg(short = 'F', long = "field")]
    field: Option<String>,
    #[arg(long = "head")]
    head: Option<usize>,
    #[arg(long = "tail")]
    tail: Option<usize>,
    #[arg(short = 'f', long = "follow")]
    follow: bool,
    #[arg(long = "no-tail")]
    no_tail: bool,
    #[arg(short = 'b', long = "boot", num_args = 0..=1, default_missing_value = "0")]
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
    matches: Vec<String>,

    // Parser-recognized v260.1 options that the Rust CLI does not yet
    // dispatch behavior for. They exist so the parser accepts every
    // official long option; downstream validation rejects the ones that
    // are intentionally unsupported in portable mode with a portable
    // unsupported message. This keeps the parser in lock-step with the
    // shared v260.1 manifest under tests/parser-parity/.
    #[arg(long = "system")]
    system: bool,
    #[arg(long = "user")]
    user: bool,
    #[arg(short = 'M', long = "machine")]
    machine: Option<String>,
    #[arg(short = 'm', long = "merge")]
    merge: bool,
    #[arg(long = "root")]
    root: Option<PathBuf>,
    #[arg(long = "image")]
    image: Option<PathBuf>,
    #[arg(long = "image-policy")]
    image_policy: Option<String>,
    #[arg(long = "namespace")]
    namespace: Option<String>,

    #[arg(short = 'c', long = "cursor")]
    cursor: Option<String>,
    #[arg(long = "after-cursor")]
    after_cursor: Option<String>,
    #[arg(long = "cursor-file")]
    cursor_file: Option<PathBuf>,
    #[arg(long = "this-boot")]
    this_boot: bool,
    #[arg(short = 'u', long = "unit")]
    unit: Vec<String>,
    #[arg(long = "user-unit")]
    user_unit: Vec<String>,
    #[arg(long = "invocation")]
    invocation: Option<String>,
    #[arg(short = 'I')]
    invocation_latest: bool,
    #[arg(short = 't', long = "identifier")]
    identifier: Vec<String>,
    #[arg(short = 'T', long = "exclude-identifier")]
    exclude_identifier: Vec<String>,
    #[arg(short = 'p', long = "priority")]
    priority: Vec<String>,
    #[arg(long = "facility")]
    facility: Vec<String>,
    #[arg(short = 'g', long = "grep")]
    grep: Option<String>,
    #[arg(long = "case-sensitive", num_args = 0..=1, default_missing_value = "true")]
    case_sensitive: Option<String>,
    #[arg(short = 'k', long = "dmesg")]
    dmesg: bool,

    #[arg(short = 'n', long = "lines", num_args = 0..=1, default_missing_value = "10")]
    lines: Option<String>,
    #[arg(short = 'r', long = "reverse")]
    reverse: bool,
    #[arg(long = "show-cursor")]
    show_cursor: bool,
    #[arg(long = "utc")]
    utc: bool,
    #[arg(short = 'x', long = "catalog")]
    catalog: bool,
    #[arg(short = 'W', long = "no-hostname")]
    no_hostname: bool,
    #[arg(long = "no-full")]
    no_full: bool,
    #[arg(short = 'l', long = "full")]
    full: bool,
    #[arg(short = 'a', long = "all")]
    all: bool,
    #[arg(long = "truncate-newline")]
    truncate_newline: bool,
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
    #[arg(long = "synchronize-on-exit")]
    synchronize_on_exit: Option<String>,
    #[arg(long = "no-pager")]
    no_pager: bool,
    #[arg(short = 'e', long = "pager-end")]
    pager_end: bool,
    #[arg(long = "output-fields")]
    output_fields: Option<String>,

    #[arg(long = "interval")]
    interval: Option<String>,
    #[arg(long = "force")]
    force: bool,
    #[arg(long = "setup-keys")]
    setup_keys: bool,

    #[arg(long = "version")]
    version: bool,
    #[arg(long = "new-id128")]
    new_id128: bool,
    #[arg(long = "list-invocations")]
    list_invocations: bool,
    #[arg(long = "list-namespaces")]
    list_namespaces: bool,
    #[arg(long = "disk-usage")]
    disk_usage: bool,
    #[arg(long = "vacuum-size")]
    vacuum_size: Option<String>,
    #[arg(long = "vacuum-files")]
    vacuum_files: Option<String>,
    #[arg(long = "vacuum-time")]
    vacuum_time: Option<String>,
    #[arg(long = "header")]
    header: bool,
    #[arg(long = "list-catalog")]
    list_catalog: bool,
    #[arg(long = "dump-catalog")]
    dump_catalog: bool,
    #[arg(long = "update-catalog")]
    update_catalog: bool,
    #[arg(long = "smart-relinquish-var")]
    smart_relinquish_var: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
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
    /// Print the official output mode list and exit.
    Help,
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
            OutputModeArg::Help => Self::Default,
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

fn enforce_stock_invalid_short_equals(raw_args: &[String]) -> Result<()> {
    for arg in raw_args.iter().skip(1) {
        if arg == "--" {
            break;
        }
        if arg.starts_with("-n=") {
            let value = &arg[2..];
            return Err(anyhow!("Failed to parse --lines='{value}'."));
        }
    }
    Ok(())
}

fn run() -> Result<()> {
    // nosemgrep: rust.lang.security.args.args -- CLI entry point parses argv; not an authorization boundary.
    let raw_args: Vec<String> = std::env::args().collect();
    enforce_stock_invalid_short_equals(&raw_args)?;
    let full_width = resolve_full_width(&raw_args);
    let args = Args::parse_from(preprocess_optional_boot_args(raw_args));
    if matches!(args.output, OutputModeArg::Help) {
        print_output_mode_help();
        return Ok(());
    }

    // Enforce the v260.1 parser-required interaction rules. These run
    // before any dispatch so the user sees an explicit conflict error.
    enforce_source_exclusivity(&args)?;
    enforce_since_until_order(&args.since, &args.until)?;
    enforce_cursor_source_exclusivity(&args)?;
    enforce_follow_reverse_conflict(&args)?;
    enforce_oldest_lines_conflict(&args)?;
    enforce_case_sensitive_value(&args)?;
    enforce_boot_descriptor_value(&args)?;

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

    enforce_action_argument_restriction(&args)?;
    enforce_boot_merge_conflict(&args)?;
    enforce_portable_unsupported(&args)?;

    // Portable utility actions that do not require a journal file:
    // they print results and exit before any open() is attempted.
    if args.new_id128 {
        print_new_id128();
        return Ok(());
    }

    if args.disk_usage && args.file.is_empty() && args.directory.is_none() {
        return Err(portable_unsupported(
            "--disk-usage",
            "requires host journal directory; pass --file or --directory to compute disk usage for explicit input",
        ));
    }

    let input = CliInput::from_args(&args)?;

    validate_path_match_arguments(&args.matches)?;

    if args.disk_usage {
        return run_disk_usage_input(&input);
    }

    if has_vacuum_flags(&args) {
        let Some(path) = args.directory.as_ref() else {
            return Err(portable_unsupported(
                "--vacuum-*",
                "vacuum actions require explicit --directory= input",
            ));
        };
        return run_vacuum(path, &args);
    }

    if args.header {
        return run_header_input(&input);
    }

    if args.list_invocations {
        return run_list_invocations(&input, &args);
    }

    if args.verify || args.verify_only || args.verify_key.is_some() {
        return run_verify_input(&input, args.verify_key.as_deref());
    }
    if args.list_boots {
        return run_list_boots(&input, &args);
    }

    let since_usec = parse_optional_timestamp(args.since.as_deref())?;
    let until_usec = parse_optional_timestamp(args.until.as_deref())?;
    let post_filters = CliPostFilters::from_args(&args)?;
    let cursor_control = CursorControl::from_args(&args)?;
    let output_options = OutputOptions::from_args(&args, full_width);

    if args.follow {
        let tail = parse_tail_count(args.tail.as_ref(), args.lines.as_deref()).unwrap_or(10);
        return run_follow(
            &input,
            &args,
            since_usec,
            until_usec,
            tail,
            &post_filters,
            &cursor_control,
            &output_options,
        );
    }

    let mut journal = open_filtered_journal(&input, &args, None)?;

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
    let grep_tail_reverse = grep_tail_implies_reverse(&args)?;
    if let Some(limit) = parse_lines_limit(args.lines.as_deref())? {
        return match limit {
            LinesLimit::All => show_head_or_all_with_reverse(
                &mut journal,
                None,
                since_usec,
                until_usec,
                args.reverse,
                args.show_cursor,
                effective_quiet(&args),
                &post_filters,
                &cursor_control,
                &output_options,
            ),
            LinesLimit::Head(n) => show_head_or_all_with_reverse(
                &mut journal,
                Some(n),
                since_usec,
                until_usec,
                false,
                args.show_cursor,
                effective_quiet(&args),
                &post_filters,
                &cursor_control,
                &output_options,
            ),
            LinesLimit::Tail(n) => show_tail_with_reverse(
                &mut journal,
                n,
                since_usec,
                until_usec,
                args.reverse || grep_tail_reverse,
                args.show_cursor,
                effective_quiet(&args),
                &post_filters,
                &cursor_control,
                &output_options,
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
            effective_quiet(&args),
            &post_filters,
            &cursor_control,
            &output_options,
        )
    } else {
        if args.pager_end && args.head.is_none() {
            return show_tail_with_reverse(
                &mut journal,
                1000,
                since_usec,
                until_usec,
                reverse,
                args.show_cursor,
                effective_quiet(&args),
                &post_filters,
                &cursor_control,
                &output_options,
            );
        }
        show_head_or_all_with_reverse(
            &mut journal,
            args.head,
            since_usec,
            until_usec,
            reverse,
            args.show_cursor,
            effective_quiet(&args),
            &post_filters,
            &cursor_control,
            &output_options,
        )
    }
}

fn resolve_full_width(args: &[String]) -> bool {
    let mut full_width = true;
    for arg in args {
        match arg.as_str() {
            "--no-full" => full_width = false,
            "--full" | "-l" => full_width = true,
            _ => {}
        }
    }
    full_width
}

fn print_output_mode_help() {
    for mode in OUTPUT_MODE_HELP_LIST {
        println!("{mode}");
    }
}

#[derive(Debug, Clone)]
enum CliInput {
    Directory(PathBuf),
    Files(Vec<PathBuf>),
}

impl CliInput {
    fn from_args(args: &Args) -> Result<Self> {
        if let Some(directory) = args.directory.as_ref() {
            return Ok(Self::Directory(directory.clone()));
        }
        let files = resolve_file_inputs(&args.file)?;
        if !files.is_empty() {
            return Ok(Self::Files(files));
        }
        Err(portable_unsupported(
            "default journal source",
            "default host journal discovery is not portable; pass --file or --directory",
        ))
    }

    fn open_journal(&self) -> Result<SdJournal> {
        match self {
            Self::Directory(path) => SdJournalOpenDirectory(&path.to_string_lossy(), 0)
                .map_err(|err| anyhow!("open: {err}")),
            Self::Files(paths) => {
                let strings: Vec<String> = paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect();
                let refs: Vec<&str> = strings.iter().map(String::as_str).collect();
                SdJournalOpenFiles(&refs, 0).map_err(|err| anyhow!("open: {err}"))
            }
        }
    }

    fn journal_files(&self, context: &str) -> Result<(Vec<PathBuf>, bool)> {
        match self {
            Self::Files(paths) => Ok((paths.clone(), false)),
            Self::Directory(path) => collect_journal_files_for_verify(path)
                .map(|files| (files, true))
                .map_err(|err| anyhow!("{context}: read directory {}: {err}", path.display())),
        }
    }
}

fn resolve_file_inputs(values: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for value in values {
        if value == Path::new("-") {
            return Err(portable_unsupported(
                "--file=-",
                "stdin-backed journals require seekable mmap-capable file descriptors and are not supported in portable mode",
            ));
        }
        let matches = expand_glob_path(value);
        if matches.is_empty() {
            push_unique_file(&mut files, &mut seen, value.clone());
        } else {
            for path in matches {
                push_unique_file(&mut files, &mut seen, path);
            }
        }
    }
    Ok(files)
}

fn push_unique_file(files: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, path: PathBuf) {
    let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if seen.insert(key) {
        files.push(path);
    }
}

fn expand_glob_path(pattern: &Path) -> Vec<PathBuf> {
    let pattern_text = pattern.to_string_lossy();
    if !is_glob_pattern(&pattern_text) {
        return Vec::new();
    }

    let mut prefixes = Vec::<PathBuf>::new();
    for component in pattern.components() {
        match component {
            std::path::Component::Prefix(prefix) => {
                prefixes = vec![PathBuf::from(prefix.as_os_str())];
            }
            std::path::Component::RootDir => {
                if prefixes.is_empty() {
                    prefixes.push(PathBuf::from(std::path::MAIN_SEPARATOR.to_string()));
                } else {
                    for prefix in &mut prefixes {
                        prefix.push(std::path::MAIN_SEPARATOR.to_string());
                    }
                }
            }
            std::path::Component::CurDir => {
                if prefixes.is_empty() {
                    prefixes.push(PathBuf::from("."));
                } else {
                    for prefix in &mut prefixes {
                        prefix.push(".");
                    }
                }
            }
            std::path::Component::ParentDir => {
                if prefixes.is_empty() {
                    prefixes.push(PathBuf::from(".."));
                } else {
                    for prefix in &mut prefixes {
                        prefix.push("..");
                    }
                }
            }
            std::path::Component::Normal(component) => {
                if prefixes.is_empty() {
                    prefixes.push(PathBuf::new());
                }
                let component_pattern = component.to_string_lossy();
                if is_glob_pattern(&component_pattern) {
                    let mut next = Vec::new();
                    for prefix in &prefixes {
                        let dir = if prefix.as_os_str().is_empty() {
                            Path::new(".")
                        } else {
                            prefix.as_path()
                        };
                        let Ok(entries) = fs::read_dir(dir) else {
                            continue;
                        };
                        for entry in entries.flatten() {
                            let name = entry.file_name();
                            let name_text = name.to_string_lossy();
                            if name_text.starts_with('.') && !component_pattern.starts_with('.') {
                                continue;
                            }
                            if glob_pattern_matches(&component_pattern, &name_text) {
                                let mut path = prefix.clone();
                                path.push(name);
                                next.push(path);
                            }
                        }
                    }
                    prefixes = next;
                } else {
                    for prefix in &mut prefixes {
                        prefix.push(component);
                    }
                }
            }
        }
    }
    prefixes.sort();
    prefixes
}

fn effective_quiet(args: &Args) -> bool {
    args.quiet
        || matches!(
            args.output,
            OutputModeArg::Export
                | OutputModeArg::Json
                | OutputModeArg::JsonPretty
                | OutputModeArg::JsonSse
                | OutputModeArg::JsonSeq
                | OutputModeArg::Cat
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinesLimit {
    All,
    Head(usize),
    Tail(usize),
}

/// Parse `--lines=[+]N` per the v260.1 grammar.
/// Bare `--lines` is normalized to the official default tail count, 10.
/// Explicit empty `--lines=` is invalid, matching systemd v260.1.
fn parse_lines_limit(value: Option<&str>) -> Result<Option<LinesLimit>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty() {
        return Err(anyhow!("Failed to parse --lines=''."));
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
        .map_err(|_| anyhow!("Failed to parse --lines='{value}'."))?;
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

fn grep_tail_implies_reverse(args: &Args) -> Result<bool> {
    if args.grep.is_none() || args.follow {
        return Ok(false);
    }
    let Some(limit) = parse_lines_limit(args.lines.as_deref())? else {
        return Ok(false);
    };
    Ok(matches!(limit, LinesLimit::Tail(_)))
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
    if !args.file.is_empty() {
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
    if is_show_action(args)
        && matches!(
            parse_lines_limit(args.lines.as_deref())?,
            Some(LinesLimit::Head(_))
        )
        && (args.reverse || args.follow)
    {
        return Err(anyhow!(
            "--lines=+N is unsupported when --reverse or --follow is specified."
        ));
    }
    Ok(())
}

fn is_show_action(args: &Args) -> bool {
    !args.version
        && !args.new_id128
        && !args.fields
        && args.field.is_none()
        && !args.list_boots
        && !args.list_invocations
        && !args.disk_usage
        && !has_vacuum_flags(args)
        && !args.header
        && !args.verify
        && !args.verify_only
        && args.verify_key.is_none()
        && !args.sync
        && !args.flush
        && !args.rotate
        && !args.relinquish_var
        && !args.smart_relinquish_var
        && !args.list_namespaces
        && !args.list_catalog
        && !args.dump_catalog
        && !args.update_catalog
        && !args.setup_keys
}

fn enforce_case_sensitive_value(args: &Args) -> Result<()> {
    if let Some(value) = args.case_sensitive.as_deref() {
        parse_bool_option("--case-sensitive", value)?;
    }
    Ok(())
}

fn enforce_boot_descriptor_value(args: &Args) -> Result<()> {
    let Some(value) = args.boot.as_deref().map(str::trim) else {
        return Ok(());
    };
    if value == "all" {
        return Ok(());
    }
    parse_boot_descriptor(value)?;
    Ok(())
}

fn enforce_boot_merge_conflict(args: &Args) -> Result<()> {
    let boot_conflicts_with_merge =
        matches!(args.boot.as_deref().map(str::trim), Some(value) if value != "all");
    if (boot_conflicts_with_merge || args.this_boot || args.list_boots) && args.merge {
        return Err(anyhow!(
            "Using --boot or --list-boots with --merge is not supported."
        ));
    }
    Ok(())
}

fn enforce_action_argument_restriction(args: &Args) -> Result<()> {
    if args.matches.is_empty() || !action_rejects_arguments(args) {
        return Ok(());
    }
    Err(anyhow!(
        "Extraneous arguments starting with '{}'",
        args.matches[0]
    ))
}

fn action_rejects_arguments(args: &Args) -> bool {
    args.new_id128
        || args.setup_keys
        || args.update_catalog
        || args.header
        || args.verify
        || args.verify_only
        || args.verify_key.is_some()
        || args.disk_usage
        || args.list_boots
        || args.fields
        || args.field.is_some()
        || args.list_invocations
        || args.list_namespaces
        || args.flush
        || args.relinquish_var
        || args.smart_relinquish_var
        || args.sync
        || args.rotate
        || has_vacuum_flags(args)
}

fn enforce_portable_unsupported(args: &Args) -> Result<()> {
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
        if parse_bool_option("--synchronize-on-exit", value)? {
            return Err(portable_unsupported(
                "--synchronize-on-exit",
                unsupported_reason("synchronize-on-exit"),
            ));
        }
        // false values are accepted as no-ops per parity matrix.
    }
    if args.sync {
        return Err(portable_unsupported("--sync", unsupported_reason("sync")));
    }
    if args.flush {
        return Err(portable_unsupported("--flush", unsupported_reason("flush")));
    }
    if args.rotate && has_vacuum_flags(args) {
        return Err(portable_unsupported(
            "--rotate with --vacuum-*",
            "official rotate-and-vacuum action requires journald rotation; portable mode can only vacuum explicit directories without rotation",
        ));
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
    if has_vacuum_flags(args) && args.directory.is_none() {
        return Err(portable_unsupported(
            "--vacuum-*",
            "vacuum actions require explicit --directory= input",
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
            out.push(format!("{arg}=0"));
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
            out.push(format!("{arg}=10"));
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

fn open_filtered_journal(
    input: &CliInput,
    args: &Args,
    boot_descriptor_override: Option<&str>,
) -> Result<SdJournal> {
    let mut journal = input.open_journal()?;
    if let Some(invocation_id) = resolve_invocation_filter(input, args)? {
        add_invocation_matches(&mut journal, &invocation_id)?;
    } else {
        apply_boot_match(&mut journal, args, boot_descriptor_override)?;
        apply_cli_matches(&mut journal, args)?;
    }
    apply_matches(&mut journal, &args.matches)?;
    SdJournalSetOutputMode(&mut journal, args.output.clone().into());
    Ok(journal)
}

#[cfg(test)]
fn run_verify(path: &Path, verify_key: Option<&str>) -> Result<()> {
    let input = if path.is_dir() {
        CliInput::Directory(path.to_path_buf())
    } else {
        CliInput::Files(vec![path.to_path_buf()])
    };
    run_verify_input(&input, verify_key)
}

fn run_verify_input(input: &CliInput, verify_key: Option<&str>) -> Result<()> {
    if verify_key.is_some_and(|key| !valid_verification_key(key)) {
        eprintln!("Failed to parse seed.");
        return Err(anyhow!("failed to parse seed"));
    }

    let (files, directory_input) = input.journal_files("verify")?;

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

fn print_new_id128() {
    let id = uuid::Uuid::new_v4();
    let simple = id.simple().to_string();
    let macro_bytes = id
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(",");

    println!("As string:");
    println!("{simple}");
    println!();
    println!("As UUID:");
    println!("{}", id.hyphenated());
    println!();
    println!("As systemd-id128(1) macro:");
    println!("#define XYZ SD_ID128_MAKE({macro_bytes})");
    println!();
    println!("As Python constant:");
    println!(">>> import uuid");
    println!(">>> XYZ = uuid.UUID('{simple}')");
}

fn run_disk_usage_input(input: &CliInput) -> Result<()> {
    let (files, _) = input.journal_files("disk usage")?;
    let mut bytes = 0u64;
    for file in files {
        bytes = bytes.saturating_add(allocated_file_bytes(&file)?);
    }
    println!(
        "Archived and active journals take up {} in the file system.",
        format_journal_bytes(bytes)
    );
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct VacuumOptions {
    max_use: u64,
    max_files: u64,
    max_retention_usec: u64,
}

#[derive(Debug, Clone)]
struct VacuumCandidate {
    path: PathBuf,
    name: String,
    usage: u64,
    seqnum_id: [u8; 16],
    seqnum: u64,
    realtime: u64,
    have_seqno: bool,
}

fn has_vacuum_flags(args: &Args) -> bool {
    args.vacuum_size.is_some() || args.vacuum_files.is_some() || args.vacuum_time.is_some()
}

fn run_vacuum(path: &Path, args: &Args) -> Result<()> {
    let opts = parse_vacuum_options(args)?;
    if opts.max_use == 0 && opts.max_files == 0 && opts.max_retention_usec == 0 {
        return Ok(());
    }
    let metadata =
        fs::metadata(path).map_err(|err| anyhow!("vacuum: {}: {err}", path.display()))?;
    if !metadata.is_dir() {
        return Err(anyhow!("vacuum: {} is not a directory", path.display()));
    }
    vacuum_directory(path, opts, args.quiet)
}

fn parse_vacuum_options(args: &Args) -> Result<VacuumOptions> {
    let max_use = match args.vacuum_size.as_deref() {
        Some(value) => parse_vacuum_size(value)?,
        None => 0,
    };
    let max_files = match args.vacuum_files.as_deref() {
        Some(value) => value
            .trim()
            .parse::<u64>()
            .map_err(|_| anyhow!("failed to parse --vacuum-files value: {value}"))?,
        None => 0,
    };
    let max_retention_usec = match args.vacuum_time.as_deref() {
        Some(value) => parse_duration_usec_allow_zero(value.trim())
            .map_err(|_| anyhow!("failed to parse --vacuum-time value: {value}"))?,
        None => 0,
    };
    Ok(VacuumOptions {
        max_use,
        max_files,
        max_retention_usec,
    })
}

fn parse_vacuum_size(value: &str) -> Result<u64> {
    let re = Regex::new(r"^\s*(\d+(?:\.\d+)?)\s*([A-Za-z]*)\s*$")?;
    let captures = re
        .captures(value)
        .ok_or_else(|| anyhow!("failed to parse --vacuum-size value: {value}"))?;
    let number = captures[1].parse::<f64>()?;
    let unit = captures
        .get(2)
        .map(|m| m.as_str().to_ascii_lowercase())
        .unwrap_or_default();
    let multiplier = match unit.as_str() {
        "" | "b" | "byte" | "bytes" => 1_u64,
        "k" | "kb" | "kib" => 1024,
        "m" | "mb" | "mib" => 1024_u64.pow(2),
        "g" | "gb" | "gib" => 1024_u64.pow(3),
        "t" | "tb" | "tib" => 1024_u64.pow(4),
        "p" | "pb" | "pib" => 1024_u64.pow(5),
        "e" | "eb" | "eib" => 1024_u64.pow(6),
        _ => return Err(anyhow!("failed to parse --vacuum-size value: {value}")),
    };
    if number < 0_f64 || number > u64::MAX as f64 / multiplier as f64 {
        return Err(anyhow!("failed to parse --vacuum-size value: {value}"));
    }
    Ok((number * multiplier as f64) as u64)
}

fn vacuum_directory(dir: &Path, opts: VacuumOptions, quiet: bool) -> Result<()> {
    let archived_re =
        Regex::new(r"^.+@([0-9A-Fa-f]{32})-([0-9A-Fa-f]{16})-([0-9A-Fa-f]{16})\.journal$")?;
    let corrupt_re = Regex::new(r"^.+@([0-9A-Fa-f]{16})-([0-9A-Fa-f]{16})\.journal~$")?;

    let mut candidates = Vec::new();
    let mut active_files = 0_u64;
    let mut sum = 0_u64;
    let mut freed = 0_u64;

    for entry in fs::read_dir(dir)
        .map_err(|err| anyhow!("vacuum: read directory {}: {err}", dir.display()))?
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            _ => continue,
        };
        let usage = allocated_bytes(&metadata);
        let (candidate, protected) =
            parse_vacuum_candidate(&archived_re, &corrupt_re, path.clone(), &name, usage);
        if protected {
            active_files = active_files.saturating_add(1);
            sum = sum.saturating_add(usage);
            continue;
        }
        let Some(mut candidate) = candidate else {
            continue;
        };

        match vacuum_journal_file_empty(&path, &metadata) {
            Ok(true) => {
                match fs::remove_file(&path) {
                    Ok(()) => {
                        freed = freed.saturating_add(usage);
                        if !quiet {
                            eprintln!(
                                "Deleted empty archived journal {}/{} ({}).",
                                dir.display(),
                                name,
                                format_journal_bytes(usage)
                            );
                        }
                    }
                    Err(err) if err.kind() == ErrorKind::NotFound => {}
                    Err(err) if !quiet => {
                        eprintln!(
                            "Failed to delete empty archived journal {}/{}: {err}",
                            dir.display(),
                            name
                        );
                    }
                    Err(_) => {}
                }
                continue;
            }
            Ok(false) => {}
            Err(_) => continue,
        }

        patch_vacuum_realtime(&mut candidate, &metadata);
        candidates.push(candidate);
        sum = sum.saturating_add(usage);
    }

    candidates.sort_by(vacuum_candidate_cmp);
    let retention_limit = if opts.max_retention_usec > 0 {
        current_realtime_usec().saturating_sub(opts.max_retention_usec)
    } else {
        0
    };

    for (idx, candidate) in candidates.iter().enumerate() {
        let left = active_files.saturating_add((candidates.len() - idx) as u64);
        if (opts.max_retention_usec == 0 || candidate.realtime >= retention_limit)
            && (opts.max_use == 0 || sum <= opts.max_use)
            && (opts.max_files == 0 || left <= opts.max_files)
        {
            break;
        }

        match fs::remove_file(&candidate.path) {
            Ok(()) => {
                freed = freed.saturating_add(candidate.usage);
                sum = sum.saturating_sub(candidate.usage);
                if !quiet {
                    eprintln!(
                        "Deleted archived journal {}/{} ({}).",
                        dir.display(),
                        candidate.name,
                        format_journal_bytes(candidate.usage)
                    );
                }
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) if !quiet => {
                eprintln!(
                    "Failed to delete archived journal {}/{}: {err}",
                    dir.display(),
                    candidate.name
                );
            }
            Err(_) => {}
        }
    }

    if !quiet {
        eprintln!(
            "Vacuuming done, freed {} of archived journals from {}.",
            format_journal_bytes(freed),
            dir.display()
        );
    }
    Ok(())
}

fn parse_vacuum_candidate(
    archived_re: &Regex,
    corrupt_re: &Regex,
    path: PathBuf,
    name: &str,
    usage: u64,
) -> (Option<VacuumCandidate>, bool) {
    if name.ends_with(".journal") {
        let Some(captures) = archived_re.captures(name) else {
            return (None, true);
        };
        let Ok(seqnum_id) = parse_id128_hex(&captures[1]) else {
            return (None, true);
        };
        let Ok(seqnum) = u64::from_str_radix(&captures[2], 16) else {
            return (None, true);
        };
        let Ok(realtime) = u64::from_str_radix(&captures[3], 16) else {
            return (None, true);
        };
        return (
            Some(VacuumCandidate {
                path,
                name: name.to_owned(),
                usage,
                seqnum_id,
                seqnum,
                realtime,
                have_seqno: true,
            }),
            false,
        );
    }
    if name.ends_with(".journal~") {
        let Some(captures) = corrupt_re.captures(name) else {
            return (None, true);
        };
        let Ok(realtime) = u64::from_str_radix(&captures[1], 16) else {
            return (None, true);
        };
        return (
            Some(VacuumCandidate {
                path,
                name: name.to_owned(),
                usage,
                seqnum_id: [0u8; 16],
                seqnum: 0,
                realtime,
                have_seqno: false,
            }),
            false,
        );
    }
    (None, false)
}

fn parse_id128_hex(value: &str) -> Result<[u8; 16]> {
    let decoded = hex::decode(value)?;
    decoded
        .try_into()
        .map_err(|_| anyhow!("failed to parse id128"))
}

fn vacuum_candidate_cmp(a: &VacuumCandidate, b: &VacuumCandidate) -> std::cmp::Ordering {
    if a.have_seqno && b.have_seqno && a.seqnum_id == b.seqnum_id {
        return a.seqnum.cmp(&b.seqnum);
    }
    match a.realtime.cmp(&b.realtime) {
        std::cmp::Ordering::Equal => {}
        order => return order,
    }
    if a.have_seqno && b.have_seqno {
        match a.seqnum_id.cmp(&b.seqnum_id) {
            std::cmp::Ordering::Equal => {}
            order => return order,
        }
    }
    a.name.cmp(&b.name)
}

fn vacuum_journal_file_empty(path: &Path, metadata: &fs::Metadata) -> Result<bool> {
    if metadata.len() < JOURNAL_HEADER_SIZE {
        return Ok(true);
    }
    let mut file = open_vacuum_candidate(path)?;
    file.seek(SeekFrom::Start(JOURNAL_HEADER_N_ENTRIES_OFFSET))?;
    let mut buf = [0u8; 8];
    file.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf) == 0)
}

fn open_vacuum_candidate(path: &Path) -> Result<fs::File> {
    let mut opts = fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
    }
    opts.open(path)
        .map_err(|err| anyhow!("vacuum: open {}: {err}", path.display()))
}

fn patch_vacuum_realtime(candidate: &mut VacuumCandidate, metadata: &fs::Metadata) {
    if let Some(usec) = metadata.modified().ok().and_then(system_time_to_usec) {
        if usec > 0 && usec < candidate.realtime {
            candidate.realtime = usec;
        }
    }
}

fn current_realtime_usec() -> u64 {
    system_time_to_usec(SystemTime::now()).unwrap_or(0)
}

fn system_time_to_usec(value: SystemTime) -> Option<u64> {
    let duration = value.duration_since(UNIX_EPOCH).ok()?;
    Some(
        duration
            .as_secs()
            .saturating_mul(1_000_000)
            .saturating_add(u64::from(duration.subsec_micros())),
    )
}

fn run_header_input(input: &CliInput) -> Result<()> {
    let (files, _) = input.journal_files("header")?;
    for (idx, file) in files.iter().enumerate() {
        if idx > 0 {
            println!();
        }
        let reader =
            FileReader::open(file).map_err(|err| anyhow!("header: {}: {err}", file.display()))?;
        let usage = allocated_file_bytes(file)?;
        print_header(file, &reader.header(), usage)?;
    }
    Ok(())
}

fn print_header(path: &Path, header: &FileHeader, disk_usage: u64) -> Result<()> {
    let data_buckets = header.data_hash_table_size / HASH_ITEM_SIZE;
    let field_buckets = header.field_hash_table_size / HASH_ITEM_SIZE;
    println!("File path: {}", path.display());
    println!("File ID: {}", hex::encode(header.file_id));
    println!("Machine ID: {}", hex::encode(header.machine_id));
    println!("Boot ID: {}", hex::encode(header.tail_entry_boot_id));
    println!("Sequential number ID: {}", hex::encode(header.seqnum_id));
    println!("State: {}", header_state_name(header.state));
    println!(
        "Compatible flags:{}",
        compatible_flag_text(header.compatible_flags)
    );
    println!(
        "Incompatible flags:{}",
        incompatible_flag_text(header.incompatible_flags)
    );
    println!("Header size: {}", header.header_size);
    println!("Arena size: {}", header.arena_size);
    println!("Data hash table size: {data_buckets}");
    println!("Field hash table size: {field_buckets}");
    println!(
        "Rotate suggested: {}",
        yes_no(header_rotate_suggested(header, data_buckets, field_buckets))
    );
    println!(
        "Head sequential number: {} ({:x})",
        header.head_entry_seqnum, header.head_entry_seqnum
    );
    println!(
        "Tail sequential number: {} ({:x})",
        header.tail_entry_seqnum, header.tail_entry_seqnum
    );
    println!(
        "Head realtime timestamp: {} ({:x})",
        output::format_header_timestamp(header.head_entry_realtime)?,
        header.head_entry_realtime
    );
    println!(
        "Tail realtime timestamp: {} ({:x})",
        output::format_header_timestamp(header.tail_entry_realtime)?,
        header.tail_entry_realtime
    );
    println!(
        "Tail monotonic timestamp: {} ({:x})",
        format_header_timespan(header.tail_entry_monotonic),
        header.tail_entry_monotonic
    );
    println!("Objects: {}", header.n_objects);
    println!("Entry objects: {}", header.n_entries);
    if header_contains(header.header_size, 216) {
        println!("Data objects: {}", header.n_data);
        println!(
            "Data hash table fill: {:.1}%",
            fill_percent(header.n_data, data_buckets)
        );
    }
    if header_contains(header.header_size, 224) {
        println!("Field objects: {}", header.n_fields);
        println!(
            "Field hash table fill: {:.1}%",
            fill_percent(header.n_fields, field_buckets)
        );
    }
    if header_contains(header.header_size, 232) {
        println!("Tag objects: {}", header.n_tags);
    }
    if header_contains(header.header_size, 240) {
        println!("Entry array objects: {}", header.n_entry_arrays);
    }
    if header_contains(header.header_size, 256) {
        println!(
            "Deepest field hash chain: {}",
            header.field_hash_chain_depth
        );
    }
    if header_contains(header.header_size, 248) {
        println!("Deepest data hash chain: {}", header.data_hash_chain_depth);
    }
    println!("Disk usage: {}", format_journal_bytes(disk_usage));
    Ok(())
}

fn run_list_boots(input: &CliInput, args: &Args) -> Result<()> {
    let mut journal = input.open_journal()?;
    let boots = collect_boots(&mut journal)?;
    if boots.is_empty() {
        return Err(anyhow!("No boot found."));
    }
    let rows = select_boot_rows(&boots, args)?;
    if !args.quiet {
        println!("IDX BOOT ID                          FIRST ENTRY                 LAST ENTRY");
    }
    let index_width = if args.quiet {
        rows.iter()
            .map(|boot| boot.index.to_string().len())
            .max()
            .unwrap_or(1)
    } else {
        3
    };
    for boot in &rows {
        println!(
            "{:>width$} {} {} {}",
            boot.index,
            boot.boot_id,
            output::format_header_timestamp(boot.first_entry)?,
            output::format_header_timestamp(boot.last_entry)?,
            width = index_width,
        );
    }
    Ok(())
}

fn run_list_invocations(input: &CliInput, args: &Args) -> Result<()> {
    let invocations = collect_invocations_from_input(input, args, Some("--list-invocations"))?;
    if invocations.is_empty() {
        return Err(anyhow!("No invocation ID found."));
    }
    let rows = select_invocation_rows(&invocations, args)?;
    if !args.quiet {
        println!("IDX INVOCATION ID                    FIRST ENTRY                 LAST ENTRY");
    }
    let quiet_index_width = if args.quiet {
        rows.iter()
            .map(|(row_index, _)| row_index.to_string().len())
            .max()
            .unwrap_or(1)
    } else {
        1
    };
    for (row_index, entry) in rows {
        if args.quiet {
            println!(
                "{:>width$} {} {} {}",
                row_index,
                entry.id,
                output::format_header_timestamp(entry.first_usec)?,
                output::format_header_timestamp(entry.last_usec)?,
                width = quiet_index_width,
            );
        } else {
            println!(
                "{:>3} {:<32} {} {}",
                row_index,
                entry.id,
                output::format_header_timestamp(entry.first_usec)?,
                output::format_header_timestamp(entry.last_usec)?,
            );
        }
    }
    Ok(())
}

fn header_state_name(state: u8) -> &'static str {
    match state {
        0 => "OFFLINE",
        1 => "ONLINE",
        2 => "ARCHIVED",
        _ => "UNKNOWN",
    }
}

fn compatible_flag_text(flags: u32) -> String {
    let mut parts = Vec::new();
    if flags & COMPATIBLE_SEALED != 0 {
        parts.push("SEALED");
    }
    if flags & COMPATIBLE_SEALED_CONTINUOUS != 0 {
        parts.push("SEALED_CONTINUOUS");
    }
    if flags & COMPATIBLE_TAIL_ENTRY_BOOT_ID != 0 {
        parts.push("TAIL_ENTRY_BOOT_ID");
    }
    if flags & !(COMPATIBLE_SEALED | COMPATIBLE_SEALED_CONTINUOUS | COMPATIBLE_TAIL_ENTRY_BOOT_ID)
        != 0
    {
        parts.push("???");
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" {}", parts.join(" "))
    }
}

fn incompatible_flag_text(flags: u32) -> String {
    let mut parts = Vec::new();
    if flags & INCOMPATIBLE_COMPRESSED_XZ != 0 {
        parts.push("COMPRESSED-XZ");
    }
    if flags & INCOMPATIBLE_COMPRESSED_LZ4 != 0 {
        parts.push("COMPRESSED-LZ4");
    }
    if flags & INCOMPATIBLE_COMPRESSED_ZSTD != 0 {
        parts.push("COMPRESSED-ZSTD");
    }
    if flags & INCOMPATIBLE_KEYED_HASH != 0 {
        parts.push("KEYED-HASH");
    }
    if flags & INCOMPATIBLE_COMPACT != 0 {
        parts.push("COMPACT");
    }
    if flags
        & !(INCOMPATIBLE_COMPRESSED_XZ
            | INCOMPATIBLE_COMPRESSED_LZ4
            | INCOMPATIBLE_COMPRESSED_ZSTD
            | INCOMPATIBLE_KEYED_HASH
            | INCOMPATIBLE_COMPACT)
        != 0
    {
        parts.push("???");
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" {}", parts.join(" "))
    }
}

fn header_contains(header_size: u64, end: u64) -> bool {
    header_size >= end
}

fn fill_percent(count: u64, buckets: u64) -> f64 {
    if buckets == 0 {
        return 0.0;
    }
    100.0 * count as f64 / buckets as f64
}

fn header_rotate_suggested(header: &FileHeader, data_buckets: u64, field_buckets: u64) -> bool {
    if header.header_size < JOURNAL_HEADER_SIZE {
        return true;
    }
    if data_buckets > 0 && header.n_data.saturating_mul(4) > data_buckets.saturating_mul(3) {
        return true;
    }
    if field_buckets > 0 && header.n_fields.saturating_mul(4) > field_buckets.saturating_mul(3) {
        return true;
    }
    if header.data_hash_chain_depth > HEADER_CHAIN_DEPTH_MAX
        || header.field_hash_chain_depth > HEADER_CHAIN_DEPTH_MAX
    {
        return true;
    }
    header.n_data > 0 && header.n_fields == 0
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn format_header_timespan(usec: u64) -> String {
    if usec < 1_000 {
        return format!("{usec}us");
    }
    if usec < 1_000_000 {
        return format!("{}ms", usec / 1_000);
    }
    if usec < 60_000_000 {
        return format!("{}s", usec / 1_000_000);
    }
    if usec < 3_600_000_000 {
        return format!("{}min", usec / 60_000_000);
    }
    if usec < 86_400_000_000 {
        return format!("{}h", usec / 3_600_000_000);
    }
    format!("{}d", usec / 86_400_000_000)
}

fn select_invocation_rows<'a>(
    invocations: &'a [InvocationEntry],
    args: &Args,
) -> Result<Vec<(isize, &'a InvocationEntry)>> {
    let (selected, first_index) = match parse_lines_limit(args.lines.as_deref())? {
        None => (invocations, 1 - invocations.len() as isize),
        Some(LinesLimit::All) => (invocations, 1 - invocations.len() as isize),
        Some(LinesLimit::Head(count)) => {
            let count = count.min(invocations.len());
            (&invocations[..count], 1)
        }
        Some(LinesLimit::Tail(count)) => {
            let count = count.min(invocations.len());
            let rows = &invocations[invocations.len() - count..];
            (rows, 1 - rows.len() as isize)
        }
    };
    let mut rows: Vec<_> = selected
        .iter()
        .enumerate()
        .map(|(idx, entry)| (first_index + idx as isize, entry))
        .collect();
    if args.reverse {
        rows.reverse();
    }
    Ok(rows)
}

fn allocated_file_bytes(path: &Path) -> Result<u64> {
    let metadata =
        fs::metadata(path).map_err(|err| anyhow!("disk usage: {}: {err}", path.display()))?;
    Ok(allocated_bytes(&metadata))
}

#[cfg(unix)]
fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    metadata.len()
}

fn format_journal_bytes(bytes: u64) -> String {
    const UNITS: [(&str, u64); 6] = [
        ("E", 1024_u64.pow(6)),
        ("P", 1024_u64.pow(5)),
        ("T", 1024_u64.pow(4)),
        ("G", 1024_u64.pow(3)),
        ("M", 1024_u64.pow(2)),
        ("K", 1024),
    ];

    for (idx, (suffix, factor)) in UNITS.iter().enumerate() {
        if bytes >= *factor {
            let remainder = if idx != UNITS.len() - 1 {
                let lower_factor = UNITS[idx + 1].1;
                (bytes / lower_factor * 10 / 1024) % 10
            } else {
                (bytes * 10 / factor) % 10
            };
            if remainder > 0 {
                return format!("{}.{remainder}{suffix}", bytes / factor);
            }
            return format!("{}{suffix}", bytes / factor);
        }
    }

    format!("{bytes}B")
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

fn add_invocation_matches(journal: &mut SdJournal, id: &str) -> Result<()> {
    for (idx, field) in [
        "_SYSTEMD_INVOCATION_ID",
        "OBJECT_SYSTEMD_INVOCATION_ID",
        "INVOCATION_ID",
        "USER_INVOCATION_ID",
    ]
    .into_iter()
    .enumerate()
    {
        if idx > 0 {
            SdJournalAddDisjunction(journal)
                .map_err(|err| anyhow!("add invocation disjunction: {err}"))?;
        }
        add_match_pair(journal, field, id)?;
    }
    SdJournalAddConjunction(journal).map_err(|err| anyhow!("add invocation conjunction: {err}"))?;
    Ok(())
}

fn add_match_pair(journal: &mut SdJournal, field: &str, value: &str) -> Result<()> {
    let data = parse_match_string(&format!("{field}={value}"))
        .map_err(|err| anyhow!("invalid {field} match: {err}"))?;
    SdJournalAddMatch(journal, &data).map_err(|err| anyhow!("add {field} match: {err}"))?;
    Ok(())
}

fn add_match_group(journal: &mut SdJournal, pairs: &[(&str, &str)]) -> Result<()> {
    for (field, value) in pairs {
        add_match_pair(journal, field, value)?;
    }
    Ok(())
}

fn add_unit_disjunction(journal: &mut SdJournal) -> Result<()> {
    SdJournalAddDisjunction(journal).map_err(|err| anyhow!("add unit disjunction: {err}"))
}

fn add_unit_conjunction(journal: &mut SdJournal) -> Result<()> {
    SdJournalAddConjunction(journal).map_err(|err| anyhow!("add unit conjunction: {err}"))
}

fn current_uid_string() -> Option<String> {
    #[cfg(unix)]
    {
        // CLI compatibility only: stock --user-unit defaults to the current UID.
        Some(unsafe { libc::getuid() }.to_string())
    }
    #[cfg(not(unix))]
    {
        None
    }
}

fn add_impossible_match(journal: &mut SdJournal, reason: &str) -> Result<()> {
    add_match_pair(journal, "__JOURNALCTL_NEVER_MATCH", reason)?;
    add_unit_conjunction(journal)
}

fn effective_boot_descriptor(args: &Args) -> Option<&str> {
    if let Some(boot) = args.boot.as_deref() {
        return Some(boot);
    }
    if args.this_boot {
        return Some("0");
    }
    if !args.merge && (args.follow || args.dmesg || args.pager_end) {
        return Some("0");
    }
    None
}

fn apply_boot_match(
    journal: &mut SdJournal,
    args: &Args,
    boot_descriptor_override: Option<&str>,
) -> Result<()> {
    let effective_boot = boot_descriptor_override.or_else(|| effective_boot_descriptor(args));
    let Some(boot) = effective_boot else {
        return Ok(());
    };
    if boot.trim() == "all" {
        return Ok(());
    }
    let boot_id = resolve_boot_id(journal, boot.trim())?;
    if !boot_id.is_empty() {
        let data = parse_match_string(&format!("_BOOT_ID={boot_id}"))
            .map_err(|err| anyhow!("invalid boot match: {err}"))?;
        SdJournalAddMatch(journal, &data).map_err(|err| anyhow!("add boot match: {err}"))?;
        SdJournalAddConjunction(journal).map_err(|err| anyhow!("add boot conjunction: {err}"))?;
    }
    Ok(())
}

fn add_journalctl_unit_matches(
    journal: &mut SdJournal,
    system_units: &[String],
    user_units: &[String],
) -> Result<()> {
    if system_units.is_empty() && user_units.is_empty() {
        return Ok(());
    }

    let mut added = false;
    let system_units = expand_unit_specs(journal, system_units, SYSTEM_UNIT_FIELDS_FULL)?;
    for unit in &system_units {
        add_system_unit_match_groups(journal, unit)?;
        added = true;
    }

    let user_units = expand_unit_specs(journal, user_units, USER_UNIT_FIELDS_FULL)?;
    let uid = current_uid_string();
    for unit in &user_units {
        add_user_unit_match_groups(journal, unit, uid.as_deref())?;
        added = true;
    }

    if added {
        add_unit_conjunction(journal)
    } else {
        add_impossible_match(journal, "unit-glob")
    }
}

fn add_system_unit_match_groups(journal: &mut SdJournal, unit: &str) -> Result<()> {
    add_match_group(journal, &[("_SYSTEMD_UNIT", unit)])?;
    add_unit_disjunction(journal)?;

    add_match_group(
        journal,
        &[("_SYSTEMD_CGROUP", "/init.scope"), ("UNIT", unit)],
    )?;
    add_unit_disjunction(journal)?;

    add_match_group(journal, &[("_UID", "0"), ("OBJECT_SYSTEMD_UNIT", unit)])?;
    add_unit_disjunction(journal)?;

    add_match_group(
        journal,
        &[("MESSAGE_ID", COREDUMP_MESSAGE_ID), ("COREDUMP_UNIT", unit)],
    )?;

    if unit.ends_with(".slice") {
        add_unit_disjunction(journal)?;
        add_match_group(journal, &[("_SYSTEMD_SLICE", unit)])?;
    }

    add_unit_disjunction(journal)
}

fn add_user_unit_match_groups(
    journal: &mut SdJournal,
    unit: &str,
    uid: Option<&str>,
) -> Result<()> {
    add_user_unit_match_group(journal, &[("_SYSTEMD_USER_UNIT", unit)], uid, false)?;
    add_unit_disjunction(journal)?;

    add_user_unit_match_group(journal, &[("USER_UNIT", unit)], uid, false)?;
    add_unit_disjunction(journal)?;

    add_user_unit_match_group(journal, &[("OBJECT_SYSTEMD_USER_UNIT", unit)], uid, true)?;
    add_unit_disjunction(journal)?;

    add_user_unit_match_group(journal, &[("COREDUMP_USER_UNIT", unit)], uid, true)?;

    if unit.ends_with(".slice") {
        add_unit_disjunction(journal)?;
        add_user_unit_match_group(journal, &[("_SYSTEMD_USER_SLICE", unit)], uid, false)?;
    }

    add_unit_disjunction(journal)
}

fn add_user_unit_match_group(
    journal: &mut SdJournal,
    pairs: &[(&str, &str)],
    uid: Option<&str>,
    include_root_uid: bool,
) -> Result<()> {
    add_match_group(journal, pairs)?;
    if let Some(uid) = uid {
        add_match_pair(journal, "_UID", uid)?;
        if include_root_uid {
            add_match_pair(journal, "_UID", "0")?;
        }
    }
    Ok(())
}

fn expand_unit_specs(
    journal: &mut SdJournal,
    specs: &[String],
    fields: &[&str],
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut patterns = Vec::new();

    for spec in specs {
        let unit = mangle_unit_name(spec);
        if is_glob_pattern(&unit) {
            patterns.push(unit);
        } else if seen.insert(unit.clone()) {
            out.push(unit);
        }
    }

    if patterns.is_empty() {
        return Ok(out);
    }

    for field in fields {
        SdJournalVisitUniqueValues(journal, field, |value| {
            let value = String::from_utf8_lossy(value);
            if patterns
                .iter()
                .any(|pattern| glob_pattern_matches(pattern, &value))
                && seen.insert(value.to_string())
            {
                out.push(value.to_string());
            }
            Ok(())
        })
        .map_err(|err| anyhow!("query possible units for {field}: {err}"))?;
    }

    Ok(out)
}

fn mangle_unit_name(value: &str) -> String {
    let value = value.trim();
    if UNIT_SUFFIXES.iter().any(|suffix| value.ends_with(suffix)) {
        value.to_string()
    } else {
        format!("{value}.service")
    }
}

fn is_glob_pattern(value: &str) -> bool {
    value.contains(['*', '?', '['])
}

fn glob_pattern_matches(pattern: &str, value: &str) -> bool {
    Regex::new(&glob_pattern_to_regex(pattern)).is_ok_and(|regex| regex.is_match(value))
}

fn glob_pattern_to_regex(pattern: &str) -> String {
    let mut out = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            '[' => {
                let mut class = String::from("[");
                let mut closed = false;
                if chars
                    .peek()
                    .is_some_and(|next| *next == '!' || *next == '^')
                {
                    chars.next();
                    class.push('^');
                }
                while let Some(next) = chars.next() {
                    class.push(next);
                    if next == ']' {
                        closed = true;
                        break;
                    }
                }
                if closed {
                    out.push_str(&class);
                } else {
                    out.push_str("\\[");
                    out.push_str(&regex::escape(&class[1..]));
                }
            }
            _ => out.push_str(&regex::escape(&ch.to_string())),
        }
    }
    out.push('$');
    out
}

fn apply_cli_matches(journal: &mut SdJournal, args: &Args) -> Result<()> {
    let (system_units, user_units) = effective_unit_specs(args);
    add_journalctl_unit_matches(journal, &system_units, &user_units)?;

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

fn effective_unit_specs(args: &Args) -> (Vec<String>, Vec<String>) {
    let mut system_units = args.unit.clone();
    let mut user_units = args.user_unit.clone();
    if args.user && !system_units.is_empty() {
        user_units.extend(std::mem::take(&mut system_units));
    }
    (system_units, user_units)
}

#[derive(Debug)]
struct CliPostFilters {
    grep: Option<Regex>,
    exclude_identifiers: std::collections::HashSet<String>,
}

impl CliPostFilters {
    fn from_args(args: &Args) -> Result<Self> {
        let exclude_identifiers = if output_uses_exclude_identifier(args.output) {
            args.exclude_identifier.iter().cloned().collect()
        } else {
            std::collections::HashSet::new()
        };
        Ok(Self {
            grep: compile_grep_filter(args.grep.as_deref(), args.case_sensitive.as_deref())?,
            exclude_identifiers,
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

    fn excludes_entry(&self, entry: &Entry) -> bool {
        if self.exclude_identifiers.is_empty() {
            return false;
        }
        let mut excluded = false;
        for_each_entry_value(entry, "SYSLOG_IDENTIFIER", |value| {
            if self
                .exclude_identifiers
                .contains(&String::from_utf8_lossy(value).into_owned())
            {
                excluded = true;
            }
        });
        excluded
    }
}

fn output_uses_exclude_identifier(mode: OutputModeArg) -> bool {
    matches!(
        mode,
        OutputModeArg::Short
            | OutputModeArg::ShortFull
            | OutputModeArg::ShortIso
            | OutputModeArg::ShortIsoPrecise
            | OutputModeArg::ShortPrecise
            | OutputModeArg::ShortMonotonic
            | OutputModeArg::ShortDelta
            | OutputModeArg::ShortUnix
            | OutputModeArg::WithUnit
    )
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
        "1" | "true" | "yes" | "y" | "t" | "on" => Ok(true),
        "0" | "false" | "no" | "n" | "f" | "off" => Ok(false),
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

struct BootSeparatorState {
    enabled: bool,
    previous_boot: Option<[u8; 16]>,
}

impl BootSeparatorState {
    fn new(mode: OutputModeArg, quiet: bool, suppressed: bool) -> Self {
        Self {
            enabled: !quiet && !suppressed && output_emits_boot_separators(mode),
            previous_boot: None,
        }
    }

    fn before_entry<W: Write>(&mut self, stdout: &mut W, entry: &Entry) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if self
            .previous_boot
            .is_some_and(|previous| previous != entry.boot_id)
        {
            writeln!(stdout, "-- Boot {} --", hex::encode(entry.boot_id))?;
        }
        self.previous_boot = Some(entry.boot_id);
        Ok(())
    }
}

fn output_emits_boot_separators(mode: OutputModeArg) -> bool {
    matches!(
        mode,
        OutputModeArg::Short
            | OutputModeArg::ShortFull
            | OutputModeArg::ShortIso
            | OutputModeArg::ShortIsoPrecise
            | OutputModeArg::ShortPrecise
            | OutputModeArg::ShortMonotonic
            | OutputModeArg::ShortDelta
            | OutputModeArg::ShortUnix
            | OutputModeArg::Verbose
            | OutputModeArg::WithUnit
    )
}

fn render_entry<W: Write>(
    stdout: &mut W,
    renderer: &mut OutputRenderer,
    boot_separators: &mut BootSeparatorState,
    post_filters: &CliPostFilters,
    entry: &Entry,
) -> Result<bool> {
    boot_separators.before_entry(stdout, entry)?;
    if post_filters.excludes_entry(entry) {
        return Ok(false);
    }
    if renderer.skips_entry(entry) {
        return Ok(false);
    }
    let output = renderer.render(entry)?;
    stdout.write_all(&output)?;
    Ok(true)
}

fn show_head_or_all_with_reverse(
    journal: &mut SdJournal,
    limit: Option<usize>,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
    reverse: bool,
    show_cursor: bool,
    quiet: bool,
    post_filters: &CliPostFilters,
    cursor_control: &CursorControl,
    output_options: &OutputOptions,
) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    let mut renderer = OutputRenderer::new(output_options.clone());
    let mut boot_separators = BootSeparatorState::new(
        output_options.mode(),
        quiet,
        output_options.suppress_boot_separators(),
    );
    let mut shown = 0usize;
    let mut last_cursor: Option<String> = None;
    if reverse {
        if limit.is_none() {
            let (count, cursor) = stream_previous_output_events(
                journal,
                since_usec,
                until_usec,
                post_filters,
                cursor_control.seek.as_ref(),
                output_options,
                quiet,
                &mut stdout,
                &mut renderer,
            )?;
            print_no_entries(&mut stdout, count, quiet)?;
            finish_cursor_output(
                &mut stdout,
                show_cursor,
                cursor_control.update_file.as_deref(),
                cursor.as_deref(),
            )?;
            return Ok(());
        }
        let (events, count, cursor) = previous_output_events(
            journal,
            since_usec,
            until_usec,
            post_filters,
            cursor_control.seek.as_ref(),
            limit.unwrap_or(0),
            output_options,
            quiet,
        )?;
        let mut disabled_boot_separators =
            BootSeparatorState::new(output_options.mode(), quiet, true);
        for event in events {
            match event {
                ReverseOutputEvent::Boot(boot_id) => {
                    writeln!(stdout, "-- Boot {} --", hex::encode(boot_id))?;
                }
                ReverseOutputEvent::Entry(entry) => {
                    render_entry(
                        &mut stdout,
                        &mut renderer,
                        &mut disabled_boot_separators,
                        post_filters,
                        &entry,
                    )?;
                }
            }
        }
        print_no_entries(&mut stdout, count, quiet)?;
        finish_cursor_output(
            &mut stdout,
            show_cursor,
            cursor_control.update_file.as_deref(),
            cursor.as_deref(),
        )?;
        return Ok(());
    }
    for_each_matching_entry_with_direction(
        journal,
        since_usec,
        until_usec,
        reverse,
        post_filters,
        cursor_control.seek.as_ref(),
        |entry| {
            if limit.is_some_and(|limit| shown >= limit) {
                return Ok(false);
            }
            render_entry(
                &mut stdout,
                &mut renderer,
                &mut boot_separators,
                post_filters,
                &entry,
            )?;
            shown += 1;
            if !entry.cursor.is_empty() {
                last_cursor = Some(entry.cursor.clone());
            }
            Ok(true)
        },
    )?;
    print_no_entries(&mut stdout, shown, quiet)?;
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
    quiet: bool,
    post_filters: &CliPostFilters,
    cursor_control: &CursorControl,
    output_options: &OutputOptions,
) -> Result<()> {
    if limit == 0 {
        if let Some(cursor_seek) = cursor_control.seek.as_ref() {
            let _ = seek_cursor_start(journal, cursor_seek, false)?;
        }
        let mut stdout = std::io::stdout().lock();
        print_no_entries(&mut stdout, 0, quiet)?;
        finish_cursor_output(
            &mut stdout,
            show_cursor,
            cursor_control.update_file.as_deref(),
            None,
        )?;
        return Ok(());
    }

    let mut selected = VecDeque::new();
    if reverse {
        let (events, count, last_cursor) = previous_output_events(
            journal,
            since_usec,
            until_usec,
            post_filters,
            cursor_control.seek.as_ref(),
            limit,
            output_options,
            quiet,
        )?;
        let mut stdout = std::io::stdout().lock();
        let mut renderer = OutputRenderer::new(output_options.clone());
        let mut disabled_boot_separators =
            BootSeparatorState::new(output_options.mode(), quiet, true);
        for event in events {
            match event {
                ReverseOutputEvent::Boot(boot_id) => {
                    writeln!(stdout, "-- Boot {} --", hex::encode(boot_id))?;
                }
                ReverseOutputEvent::Entry(entry) => {
                    render_entry(
                        &mut stdout,
                        &mut renderer,
                        &mut disabled_boot_separators,
                        post_filters,
                        &entry,
                    )?;
                }
            }
        }
        print_no_entries(&mut stdout, count, quiet)?;
        finish_cursor_output(
            &mut stdout,
            show_cursor,
            cursor_control.update_file.as_deref(),
            last_cursor.as_deref(),
        )?;
        return Ok(());
    } else if cursor_control.seek.is_none() {
        for_each_matching_entry_with_direction(
            journal,
            since_usec,
            until_usec,
            true,
            post_filters,
            None,
            |entry| {
                if selected.len() >= limit {
                    return Ok(false);
                }
                selected.push_front(entry);
                Ok(true)
            },
        )?;
    } else {
        for_each_matching_entry_with_direction(
            journal,
            since_usec,
            until_usec,
            false,
            post_filters,
            cursor_control.seek.as_ref(),
            |entry| {
                if limit > 0 && selected.len() == limit {
                    selected.pop_front();
                }
                if limit > 0 {
                    selected.push_back(entry);
                }
                Ok(true)
            },
        )?;
    }
    let mut stdout = std::io::stdout().lock();
    let mut renderer = OutputRenderer::new(output_options.clone());
    let mut boot_separators = BootSeparatorState::new(
        output_options.mode(),
        quiet,
        output_options.suppress_boot_separators(),
    );
    let mut last_cursor: Option<String> = None;
    for entry in &selected {
        render_entry(
            &mut stdout,
            &mut renderer,
            &mut boot_separators,
            post_filters,
            entry,
        )?;
        if !entry.cursor.is_empty() {
            last_cursor = Some(entry.cursor.clone());
        }
    }
    print_no_entries(&mut stdout, selected.len(), quiet)?;
    finish_cursor_output(
        &mut stdout,
        show_cursor,
        cursor_control.update_file.as_deref(),
        last_cursor.as_deref(),
    )?;
    Ok(())
}

fn print_no_entries<W: Write>(stdout: &mut W, count: usize, quiet: bool) -> Result<()> {
    if count == 0 && !quiet {
        stdout.write_all(b"-- No entries --\n")?;
    }
    Ok(())
}

enum ReverseOutputEvent {
    Boot([u8; 16]),
    Entry(Entry),
}

fn stream_previous_output_events<W: Write>(
    journal: &mut SdJournal,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
    post_filters: &CliPostFilters,
    cursor_seek: Option<&CursorSeek>,
    output_options: &OutputOptions,
    quiet: bool,
    stdout: &mut W,
    renderer: &mut OutputRenderer,
) -> Result<(usize, Option<String>)> {
    let mut boot_context = BootSeparatorState::new(
        output_options.mode(),
        quiet,
        output_options.suppress_boot_separators(),
    );
    let mut disabled_boot_separators = BootSeparatorState::new(output_options.mode(), quiet, true);
    let mut pending_boot = None;
    let mut count = 0usize;
    let mut last_cursor = None;

    let mut visit = |entry: Entry| -> Result<bool> {
        if since_usec.is_some_and(|since| entry.realtime < since) {
            return Ok(false);
        }
        if boot_context.enabled {
            if boot_context
                .previous_boot
                .is_some_and(|previous| previous != entry.boot_id)
            {
                pending_boot = Some(entry.boot_id);
            }
            boot_context.previous_boot = Some(entry.boot_id);
        }
        if since_usec.is_none_or(|since| entry.realtime >= since)
            && until_usec.is_none_or(|until| entry.realtime <= until)
            && post_filters.matches(&entry)
        {
            if let Some(boot_id) = pending_boot.take() {
                writeln!(stdout, "-- Boot {} --", hex::encode(boot_id))?;
            }
            render_entry(
                stdout,
                renderer,
                &mut disabled_boot_separators,
                post_filters,
                &entry,
            )?;
            if !entry.cursor.is_empty() {
                last_cursor = Some(entry.cursor.clone());
            }
            count += 1;
        }
        Ok(true)
    };

    if let Some(cursor_seek) = cursor_seek {
        if let Some(entry) = seek_cursor_start(journal, cursor_seek, true)? {
            if !visit(entry)? {
                return Ok((count, last_cursor));
            }
        } else {
            return Ok((count, last_cursor));
        }
    } else if let Some(until) = until_usec {
        SdJournalSeekRealtimeUsec(journal, until).map_err(|err| anyhow!("seek realtime: {err}"))?;
    } else {
        SdJournalSeekTail(journal).map_err(|err| anyhow!("seek tail: {err}"))?;
    }

    loop {
        match SdJournalPrevious(journal).map_err(|err| anyhow!("previous: {err}"))? {
            0 => break,
            _ => {
                let entry =
                    SdJournalGetEntry(journal).map_err(|err| anyhow!("get entry: {err}"))?;
                if !visit(entry)? {
                    break;
                }
            }
        }
    }
    if count > 0 {
        if let Some(boot_id) = pending_boot {
            writeln!(stdout, "-- Boot {} --", hex::encode(boot_id))?;
        }
    }
    Ok((count, last_cursor))
}

fn previous_output_events(
    journal: &mut SdJournal,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
    post_filters: &CliPostFilters,
    cursor_seek: Option<&CursorSeek>,
    limit: usize,
    output_options: &OutputOptions,
    quiet: bool,
) -> Result<(Vec<ReverseOutputEvent>, usize, Option<String>)> {
    let mut boot_context = BootSeparatorState::new(
        output_options.mode(),
        quiet,
        output_options.suppress_boot_separators(),
    );
    let mut pending_boot = None;
    let mut events = Vec::new();
    let mut count = 0usize;
    let mut last_cursor = None;

    let mut visit = |entry: Entry| -> Result<bool> {
        if since_usec.is_some_and(|since| entry.realtime < since) {
            return Ok(false);
        }
        if boot_context.enabled {
            if boot_context
                .previous_boot
                .is_some_and(|previous| previous != entry.boot_id)
            {
                pending_boot = Some(entry.boot_id);
            }
            boot_context.previous_boot = Some(entry.boot_id);
        }
        if since_usec.is_none_or(|since| entry.realtime >= since)
            && until_usec.is_none_or(|until| entry.realtime <= until)
            && post_filters.matches(&entry)
        {
            if let Some(boot_id) = pending_boot.take() {
                events.push(ReverseOutputEvent::Boot(boot_id));
            }
            if !entry.cursor.is_empty() {
                last_cursor = Some(entry.cursor.clone());
            }
            events.push(ReverseOutputEvent::Entry(entry));
            count += 1;
            if limit > 0 && count >= limit {
                return Ok(false);
            }
        }
        Ok(true)
    };

    if let Some(cursor_seek) = cursor_seek {
        if let Some(entry) = seek_cursor_start(journal, cursor_seek, true)? {
            if !visit(entry)? {
                return Ok((events, count, last_cursor));
            }
        } else {
            return Ok((events, count, last_cursor));
        }
    } else if let Some(until) = until_usec {
        SdJournalSeekRealtimeUsec(journal, until).map_err(|err| anyhow!("seek realtime: {err}"))?;
    } else {
        SdJournalSeekTail(journal).map_err(|err| anyhow!("seek tail: {err}"))?;
    }

    loop {
        match SdJournalPrevious(journal).map_err(|err| anyhow!("previous: {err}"))? {
            0 => break,
            _ => {
                let entry =
                    SdJournalGetEntry(journal).map_err(|err| anyhow!("get entry: {err}"))?;
                if !visit(entry)? {
                    return Ok((events, count, last_cursor));
                }
            }
        }
    }
    if count > 0 {
        if let Some(boot_id) = pending_boot {
            events.push(ReverseOutputEvent::Boot(boot_id));
        }
    }
    Ok((events, count, last_cursor))
}

fn for_each_matching_entry_with_direction<F>(
    journal: &mut SdJournal,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
    reverse: bool,
    post_filters: &CliPostFilters,
    cursor_seek: Option<&CursorSeek>,
    mut visitor: F,
) -> Result<()>
where
    F: FnMut(Entry) -> Result<bool>,
{
    if reverse {
        if let Some(cursor_seek) = cursor_seek {
            if let Some(entry) = seek_cursor_start(journal, cursor_seek, true)? {
                if since_usec.is_none_or(|since| entry.realtime >= since)
                    && until_usec.is_none_or(|until| entry.realtime <= until)
                    && post_filters.matches(&entry)
                {
                    if !visitor(entry)? {
                        return Ok(());
                    }
                }
            } else {
                return Ok(());
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
                        if !visitor(entry)? {
                            return Ok(());
                        }
                    }
                }
            }
        }
        return Ok(());
    }

    if let Some(cursor_seek) = cursor_seek {
        if let Some(entry) = seek_cursor_start(journal, cursor_seek, false)? {
            if since_usec.is_none_or(|since| entry.realtime >= since)
                && until_usec.is_none_or(|until| entry.realtime <= until)
                && post_filters.matches(&entry)
            {
                if !visitor(entry)? {
                    return Ok(());
                }
            }
        } else {
            return Ok(());
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
                    if !visitor(entry)? {
                        return Ok(());
                    }
                }
            }
        }
    }
    Ok(())
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

fn update_cursor_file(cursor_control: &CursorControl, cursor: Option<&str>) -> Result<()> {
    let Some(path) = cursor_control.update_file.as_deref() else {
        return Ok(());
    };
    let Some(cursor) = cursor.filter(|cursor| !cursor.is_empty()) else {
        return Ok(());
    };
    write_cursor_file_atomic(path, cursor)
}

fn write_cursor_file_atomic(path: &Path, cursor: &str) -> Result<()> {
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("invalid cursor file path: {}", path.display()))?
        .to_string_lossy();
    let dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut last_err = None;
    let mut opened = None;
    for _ in 0..16 {
        let tmp = dir.join(format!(
            ".{file_name}.tmp.{}",
            uuid::Uuid::new_v4().simple()
        ));
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.custom_flags(libc::O_CLOEXEC);
            opts.mode(0o600);
        }
        match opts.open(&tmp) {
            Ok(file) => {
                opened = Some((tmp, file));
                break;
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                last_err = Some(err);
                continue;
            }
            Err(err) => {
                return Err(anyhow!(
                    "Failed to write new cursor to {}: {err}",
                    path.display()
                ));
            }
        }
    }
    let (tmp, mut file) = opened.ok_or_else(|| {
        anyhow!(
            "Failed to write new cursor to {}: {}",
            path.display(),
            last_err
                .as_ref()
                .map(|err| err.to_string())
                .unwrap_or_else(|| "temporary file name collision".to_string())
        )
    })?;
    if let Err(err) = file.write_all(format!("{cursor}\n").as_bytes()) {
        let _ = fs::remove_file(&tmp);
        return Err(anyhow!(
            "Failed to write new cursor to {}: {err}",
            path.display()
        ));
    }
    drop(file);
    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(anyhow!(
            "Failed to write new cursor to {}: {err}",
            path.display()
        ));
    }
    Ok(())
}

#[derive(Clone)]
struct BootEntry {
    index: isize,
    boot_id: String,
    first_entry: u64,
    last_entry: u64,
}

fn collect_boots(journal: &mut SdJournal) -> Result<Vec<BootEntry>> {
    match collect_boots_indexed(journal) {
        Ok(boots) if !boots.is_empty() => return Ok(boots),
        Ok(_) | Err(_) => {
            SdJournalFlushMatches(journal).map_err(|err| anyhow!("flush boot matches: {err}"))?;
        }
    }
    collect_boots_by_scan(journal)
}

fn collect_boots_indexed(journal: &mut SdJournal) -> Result<Vec<BootEntry>> {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    let mut boot_ids = Vec::new();
    SdJournalVisitUniqueValues(journal, "_BOOT_ID", |value| {
        let Ok(text) = std::str::from_utf8(value) else {
            return Ok(());
        };
        let Some(id) = parse_boot_id_prefix(text) else {
            return Ok(());
        };
        if id.chars().all(|ch| ch == '0') || !seen.insert(id.clone()) {
            return Ok(());
        }
        boot_ids.push(id);
        Ok(())
    })
    .map_err(|err| anyhow!("query _BOOT_ID values: {err}"))?;

    let mut out = Vec::with_capacity(boot_ids.len());
    for boot_id in boot_ids {
        SdJournalFlushMatches(journal).map_err(|err| anyhow!("flush boot matches: {err}"))?;
        add_match_pair(journal, "_BOOT_ID", &boot_id)?;
        SdJournalAddConjunction(journal).map_err(|err| anyhow!("add boot conjunction: {err}"))?;

        SdJournalSeekHead(journal).map_err(|err| anyhow!("seek boot head: {err}"))?;
        if SdJournalNext(journal).map_err(|err| anyhow!("next boot entry: {err}"))? == 0 {
            continue;
        }
        let first = SdJournalGetEntry(journal)
            .map_err(|err| anyhow!("get first boot entry: {err}"))?
            .realtime;

        SdJournalSeekTail(journal).map_err(|err| anyhow!("seek boot tail: {err}"))?;
        if SdJournalPrevious(journal).map_err(|err| anyhow!("previous boot entry: {err}"))? == 0 {
            continue;
        }
        let last = SdJournalGetEntry(journal)
            .map_err(|err| anyhow!("get last boot entry: {err}"))?
            .realtime;

        out.push(BootEntry {
            index: 0,
            boot_id,
            first_entry: first,
            last_entry: last,
        });
    }
    SdJournalFlushMatches(journal).map_err(|err| anyhow!("flush boot matches: {err}"))?;
    out.sort_by(|a, b| {
        a.last_entry
            .cmp(&b.last_entry)
            .then_with(|| a.first_entry.cmp(&b.first_entry))
            .then_with(|| a.boot_id.cmp(&b.boot_id))
    });
    Ok(out)
}

fn collect_boots_by_scan(journal: &mut SdJournal) -> Result<Vec<BootEntry>> {
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
                        index: 0,
                        boot_id,
                        first_entry: entry.realtime,
                        last_entry: entry.realtime,
                    });
            }
        }
    }
    let mut out: Vec<_> = boots.into_values().collect();
    out.sort_by(|a, b| {
        a.last_entry
            .cmp(&b.last_entry)
            .then_with(|| a.first_entry.cmp(&b.first_entry))
            .then_with(|| a.boot_id.cmp(&b.boot_id))
    });
    Ok(out)
}

fn select_boot_rows(boots: &[BootEntry], args: &Args) -> Result<Vec<BootEntry>> {
    let mut rows = boots;
    let mut first_index = 1 - boots.len() as isize;
    if let Some(limit) = parse_lines_limit(args.lines.as_deref())? {
        match limit {
            LinesLimit::All => {}
            LinesLimit::Head(count) => {
                let count = count.min(boots.len());
                rows = &boots[..count];
                first_index = 1;
            }
            LinesLimit::Tail(count) => {
                let count = count.min(boots.len());
                rows = &boots[boots.len() - count..];
                first_index = 1 - count as isize;
            }
        }
    }
    let mut out: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(idx, boot)| {
            let mut boot = boot.clone();
            boot.index = first_index + idx as isize;
            boot
        })
        .collect();
    if args.reverse {
        out.reverse();
    }
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
        return Err(anyhow!("failed to parse boot descriptor: {descriptor}"));
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

#[derive(Clone)]
struct InvocationEntry {
    id: String,
    first_usec: u64,
    last_usec: u64,
}

fn effective_invocation_descriptor(args: &Args) -> Option<&str> {
    if let Some(value) = args.invocation.as_deref() {
        return Some(value.trim());
    }
    args.invocation_latest.then_some("0")
}

fn parse_invocation_descriptor(descriptor: &str) -> Result<Option<(String, isize)>> {
    if descriptor == "all" {
        return Ok(None);
    }
    parse_boot_descriptor(descriptor)
        .map(Some)
        .map_err(|_| anyhow!("failed to parse invocation descriptor: {descriptor}"))
}

fn resolve_invocation_filter(input: &CliInput, args: &Args) -> Result<Option<String>> {
    let Some(descriptor) = effective_invocation_descriptor(args) else {
        return Ok(None);
    };
    let Some((id, offset)) = parse_invocation_descriptor(descriptor)? else {
        return Ok(None);
    };

    let invocation_unit_option = if id.is_empty() || offset != 0 {
        Some("-I/--invocation= with an offset")
    } else {
        None
    };
    let invocations = collect_invocations_from_input(input, args, invocation_unit_option)?;
    let target = if !id.is_empty() {
        invocations
            .iter()
            .position(|entry| entry.id == id)
            .map(|base| base as isize + offset)
    } else if offset > 0 {
        Some(offset - 1)
    } else {
        Some(invocations.len() as isize - 1 + offset)
    };

    let Some(target) = target else {
        return Err(anyhow!(
            "No journal entry found for the invocation ({}{offset:+}).",
            id
        ));
    };
    if target < 0 || target as usize >= invocations.len() {
        return Err(anyhow!(
            "No journal entry found for the invocation ({}{offset:+}).",
            id
        ));
    }
    Ok(Some(invocations[target as usize].id.clone()))
}

fn apply_single_invocation_unit(
    journal: &mut SdJournal,
    args: &Args,
    option_name: &str,
) -> Result<()> {
    let (system_units, user_units) = single_invocation_unit(journal, args, option_name)?;
    add_journalctl_unit_matches(journal, &system_units, &user_units)
}

fn single_invocation_unit(
    journal: &mut SdJournal,
    args: &Args,
    option_name: &str,
) -> Result<(Vec<String>, Vec<String>)> {
    let (system_specs, user_specs) = effective_unit_specs(args);
    let count = system_specs.len() + user_specs.len();
    if count == 0 {
        return Err(anyhow!(
            "Using {option_name} requires a unit. Please specify a unit name with -u/--unit=/--user-unit=."
        ));
    }
    if count > 1 {
        return Err(anyhow!(
            "Using {option_name} with multiple units is not supported."
        ));
    }
    if system_specs.len() == 1 {
        let units = expand_unit_specs(journal, &system_specs, SYSTEM_UNIT_FIELDS_FULL)?;
        let query = mangle_unit_name(&system_specs[0]);
        if units.is_empty() {
            return Err(anyhow!("No matching unit found for '{query}' in journal."));
        }
        if units.len() > 1 {
            return Err(anyhow!(
                "Multiple matching units found for '{query}' in journal."
            ));
        }
        return Ok((units, Vec::new()));
    }
    let units = expand_unit_specs(journal, &user_specs, USER_UNIT_FIELDS_FULL)?;
    let query = mangle_unit_name(&user_specs[0]);
    if units.is_empty() {
        return Err(anyhow!("No matching unit found for '{query}' in journal."));
    }
    if units.len() > 1 {
        return Err(anyhow!(
            "Multiple matching units found for '{query}' in journal."
        ));
    }
    Ok((Vec::new(), units))
}

fn collect_invocations_from_input(
    input: &CliInput,
    args: &Args,
    unit_option: Option<&str>,
) -> Result<Vec<InvocationEntry>> {
    let mut indexed_journal = open_invocation_scope_journal(input, args, unit_option)?;
    let candidate_ids = collect_invocation_candidate_ids(&mut indexed_journal)?;

    let mut out = Vec::new();
    for id in candidate_ids {
        if let Some(entry) = invocation_bounds_for_id(input, args, unit_option, &id)? {
            out.push(entry);
        }
    }
    out.sort_by(|a, b| {
        a.first_usec
            .cmp(&b.first_usec)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(out)
}

fn open_invocation_scope_journal(
    input: &CliInput,
    args: &Args,
    unit_option: Option<&str>,
) -> Result<SdJournal> {
    let mut journal = input.open_journal()?;
    apply_boot_match(&mut journal, args, None)?;
    if let Some(option_name) = unit_option {
        apply_single_invocation_unit(&mut journal, args, option_name)?;
    }
    Ok(journal)
}

fn collect_invocation_candidate_ids(journal: &mut SdJournal) -> Result<Vec<String>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for field in INVOCATION_ID_FIELDS {
        SdJournalVisitUniqueValues(journal, field, |value| {
            let Ok(text) = std::str::from_utf8(value) else {
                return Ok(());
            };
            let Some(id) = parse_invocation_id_value(text) else {
                return Ok(());
            };
            if !id.chars().all(|ch| ch == '0') && seen.insert(id.clone()) {
                out.push(id);
            }
            Ok(())
        })
        .map_err(|err| anyhow!("visit unique invocation values for {field}: {err}"))?;
    }
    Ok(out)
}

fn invocation_bounds_for_id(
    input: &CliInput,
    args: &Args,
    unit_option: Option<&str>,
    id: &str,
) -> Result<Option<InvocationEntry>> {
    let mut first_journal = open_invocation_scope_journal(input, args, unit_option)?;
    add_invocation_matches(&mut first_journal, id)?;
    SdJournalSeekHead(&mut first_journal).map_err(|err| anyhow!("seek invocation head: {err}"))?;
    if SdJournalNext(&mut first_journal).map_err(|err| anyhow!("next invocation: {err}"))? == 0 {
        return Ok(None);
    }
    let first_entry = SdJournalGetEntry(&mut first_journal)
        .map_err(|err| anyhow!("get invocation entry: {err}"))?;

    let mut last_journal = open_invocation_scope_journal(input, args, unit_option)?;
    add_invocation_matches(&mut last_journal, id)?;
    SdJournalSeekTail(&mut last_journal).map_err(|err| anyhow!("seek invocation tail: {err}"))?;
    if SdJournalPrevious(&mut last_journal).map_err(|err| anyhow!("previous invocation: {err}"))?
        == 0
    {
        return Ok(None);
    }
    let last_entry = SdJournalGetEntry(&mut last_journal)
        .map_err(|err| anyhow!("get invocation entry: {err}"))?;

    Ok(Some(InvocationEntry {
        id: id.to_string(),
        first_usec: first_entry.realtime,
        last_usec: last_entry.realtime,
    }))
}

fn parse_invocation_id_value(value: &str) -> Option<String> {
    let clean = value.trim().replace('-', "");
    if clean.len() == 32 && clean.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Some(clean.to_ascii_lowercase());
    }
    None
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
    if let Some(duration) = value.strip_suffix(" ago") {
        let delta = parse_duration_usec(duration.trim())? as i64;
        let now = Local::now().timestamp_micros();
        return Ok(now.saturating_sub(delta) as u64);
    }
    let bytes = value.as_bytes();
    let signed_date_prefix = bytes.len() >= 6
        && matches!(bytes[0], b'+' | b'-')
        && bytes[1..5].iter().all(|b| b.is_ascii_digit())
        && bytes[5] == b'-';
    if matches!(bytes.first(), Some(b'+' | b'-')) && !signed_date_prefix {
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
    if has_compact_timezone_offset(value) {
        return Err(anyhow!("failed to parse timestamp: {value}"));
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
        return Ok(dt.timestamp_micros() as u64);
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f%:z",
        "%Y-%m-%dT%H:%M:%S%:z",
        "%Y-%m-%dT%H:%M%:z",
    ] {
        if let Ok(dt) = chrono::DateTime::parse_from_str(value, fmt) {
            return Ok(dt.timestamp_micros() as u64);
        }
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
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

fn has_compact_timezone_offset(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 5
        && value.contains('T')
        && matches!(bytes[bytes.len() - 5], b'+' | b'-')
        && bytes[bytes.len() - 4..]
            .iter()
            .all(|byte| byte.is_ascii_digit())
}

fn local_datetime_to_usec(dt: NaiveDateTime) -> Result<u64> {
    let local = Local
        .from_local_datetime(&dt)
        .earliest()
        .ok_or_else(|| anyhow!("failed to parse local timestamp"))?;
    Ok(local.timestamp_micros() as u64)
}

fn parse_duration_usec(value: &str) -> Result<u64> {
    parse_duration_usec_mode(value, false)
}

fn parse_duration_usec_allow_zero(value: &str) -> Result<u64> {
    parse_duration_usec_mode(value, true)
}

fn parse_duration_usec_mode(value: &str, allow_zero: bool) -> Result<u64> {
    let mut total = 0_f64;
    let bytes = value.as_bytes();
    let mut i = 0usize;
    let mut seen = false;
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
        seen = true;
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
    if !seen || (!allow_zero && total == 0_f64) {
        return Err(anyhow!("failed to parse duration: {value}"));
    }
    Ok(total as u64)
}

fn duration_unit_multiplier(unit: &str) -> Result<f64> {
    if unit == "M" {
        return Ok(2_629_800_000_000_f64);
    }
    match unit.to_ascii_lowercase().as_str() {
        "us" | "usec" | "usecs" => Ok(1_f64),
        "ms" | "msec" | "msecs" => Ok(1_000_f64),
        "s" | "sec" | "secs" | "second" | "seconds" => Ok(1_000_000_f64),
        "m" | "min" | "mins" | "minute" | "minutes" => Ok(60_000_000_f64),
        "h" | "hr" | "hour" | "hours" => Ok(3_600_000_000_f64),
        "d" | "day" | "days" => Ok(86_400_000_000_f64),
        "w" | "week" | "weeks" => Ok(604_800_000_000_f64),
        "month" | "months" => Ok(2_629_800_000_000_f64),
        "y" | "year" | "years" => Ok(31_557_600_000_000_f64),
        _ => Err(anyhow!("failed to parse duration unit: {unit}")),
    }
}

fn for_each_follow_entry<F>(
    input: &CliInput,
    args: &Args,
    boot_descriptor_override: Option<&str>,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
    post_filters: &CliPostFilters,
    cursor_seek: Option<&CursorSeek>,
    mut visitor: F,
) -> Result<()>
where
    F: FnMut(Entry) -> Result<()>,
{
    let mut journal = open_filtered_journal(input, args, boot_descriptor_override)?;
    for_each_matching_entry_with_direction(
        &mut journal,
        since_usec,
        until_usec,
        false,
        post_filters,
        cursor_seek,
        |entry| {
            visitor(entry)?;
            Ok(true)
        },
    )
}

fn resolve_follow_boot_descriptor(input: &CliInput, args: &Args) -> Result<Option<String>> {
    let Some(boot) = effective_boot_descriptor(args).map(str::trim) else {
        return Ok(None);
    };
    if boot == "all" {
        return Ok(None);
    }

    let mut journal = input.open_journal()?;
    let boot_id = resolve_boot_id(&mut journal, boot)?;
    Ok((!boot_id.is_empty()).then_some(boot_id))
}

fn run_follow(
    input: &CliInput,
    args: &Args,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
    tail: usize,
    post_filters: &CliPostFilters,
    cursor_control: &CursorControl,
    output_options: &OutputOptions,
) -> Result<()> {
    let mut last_seen_cursor: Option<String> = None;
    let mut renderer = OutputRenderer::new(output_options.clone());
    let mut boot_separators = BootSeparatorState::new(
        output_options.mode(),
        effective_quiet(args),
        output_options.suppress_boot_separators(),
    );
    let mut stdout = std::io::stdout().lock();
    let boot_descriptor_override = resolve_follow_boot_descriptor(input, args)?;
    let boot_descriptor_override = boot_descriptor_override.as_deref();

    if args.no_tail {
        for_each_follow_entry(
            input,
            args,
            boot_descriptor_override,
            since_usec,
            until_usec,
            post_filters,
            cursor_control.seek.as_ref(),
            |entry| {
                render_entry(
                    &mut stdout,
                    &mut renderer,
                    &mut boot_separators,
                    post_filters,
                    &entry,
                )?;
                if !entry.cursor.is_empty() {
                    last_seen_cursor = Some(entry.cursor.clone());
                    update_cursor_file(cursor_control, last_seen_cursor.as_deref())?;
                }
                Ok(())
            },
        )?;
    } else {
        let mut selected = VecDeque::new();
        for_each_follow_entry(
            input,
            args,
            boot_descriptor_override,
            since_usec,
            until_usec,
            post_filters,
            cursor_control.seek.as_ref(),
            |entry| {
                if !entry.cursor.is_empty() {
                    last_seen_cursor = Some(entry.cursor.clone());
                }
                if tail > 0 && selected.len() == tail {
                    selected.pop_front();
                }
                if tail > 0 {
                    selected.push_back(entry);
                }
                Ok(())
            },
        )?;
        for entry in &selected {
            render_entry(
                &mut stdout,
                &mut renderer,
                &mut boot_separators,
                post_filters,
                entry,
            )?;
        }
        update_cursor_file(cursor_control, last_seen_cursor.as_deref())?;
    }

    let min_poll_interval = Duration::from_millis(100);
    let max_poll_interval = Duration::from_secs(1);
    let mut poll_interval = min_poll_interval;
    loop {
        thread::sleep(poll_interval);
        let last_cursor = last_seen_cursor.clone();
        let follow_seek = last_cursor.map(|cursor| CursorSeek {
            cursor,
            after: true,
        });
        let cursor_seek = follow_seek.as_ref().or(cursor_control.seek.as_ref());
        let mut emitted = 0usize;
        for_each_follow_entry(
            input,
            args,
            boot_descriptor_override,
            since_usec,
            until_usec,
            post_filters,
            cursor_seek,
            |entry| {
                render_entry(
                    &mut stdout,
                    &mut renderer,
                    &mut boot_separators,
                    post_filters,
                    &entry,
                )?;
                emitted += 1;
                if !entry.cursor.is_empty() {
                    last_seen_cursor = Some(entry.cursor.clone());
                    update_cursor_file(cursor_control, last_seen_cursor.as_deref())?;
                }
                Ok(())
            },
        )?;
        poll_interval = if emitted > 0 {
            min_poll_interval
        } else {
            (poll_interval * 2).min(max_poll_interval)
        };
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

    fn write_unsealed_file(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create journal parent");
        }
        let repo_file = RepoFile::from_path(path)
            .unwrap_or_else(|| panic!("test journal path should parse: {}", path.display()));
        let mut journal_file = JournalFile::<MmapMut>::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
        )
        .expect("create journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=vacuum".as_slice()],
                1_500_000,
                100,
            )
            .expect("write entry");
        journal_file.sync().expect("sync journal");
    }

    fn archived_journal_name_for_test(seqnum: u64, realtime: u64) -> String {
        format!("system@12121212121212121212121212121212-{seqnum:016x}-{realtime:016x}.journal")
    }

    #[test]
    fn vacuum_files_with_directory_deletes_oldest_archived() {
        let source_dir = tempfile::tempdir().expect("create source dir");
        let source = source_dir.path().join("source.journal");
        write_unsealed_file(&source);

        let dir = tempfile::tempdir().expect("create vacuum dir");
        let active = dir.path().join("system.journal");
        std::fs::copy(&source, &active).expect("copy active journal");
        let names = [
            archived_journal_name_for_test(1, 1_700_004_100_000_000),
            archived_journal_name_for_test(2, 1_700_004_100_000_500),
            archived_journal_name_for_test(3, 1_700_004_100_001_000),
        ];
        for name in &names {
            std::fs::copy(&source, dir.path().join(name)).expect("copy archived journal");
        }

        let args = Args::try_parse_from([
            "journalctl",
            "--directory",
            dir.path().to_str().expect("utf8 temp path"),
            "--vacuum-files=2",
            "--quiet",
        ])
        .expect("parse vacuum args");
        run_vacuum(dir.path(), &args).expect("vacuum should pass");

        assert!(active.exists(), "active journal should be protected");
        assert!(
            !dir.path().join(&names[0]).exists(),
            "oldest archived journal should be deleted"
        );
        assert!(
            !dir.path().join(&names[1]).exists(),
            "second archived journal should be deleted"
        );
        assert!(
            dir.path().join(&names[2]).exists(),
            "newest archived journal should be retained"
        );
    }

    #[test]
    fn vacuum_time_zero_is_noop() {
        let source_dir = tempfile::tempdir().expect("create source dir");
        let source = source_dir.path().join("source.journal");
        write_unsealed_file(&source);

        let dir = tempfile::tempdir().expect("create vacuum dir");
        let name = archived_journal_name_for_test(1, 1_700_004_100_000_000);
        std::fs::copy(&source, dir.path().join(&name)).expect("copy archived journal");
        let args = Args::try_parse_from([
            "journalctl",
            "--directory",
            dir.path().to_str().expect("utf8 temp path"),
            "--vacuum-time=0s",
            "--quiet",
        ])
        .expect("parse vacuum args");
        run_vacuum(dir.path(), &args).expect("zero-time vacuum should pass");
        assert!(
            dir.path().join(&name).exists(),
            "archived journal should remain after zero-time vacuum"
        );
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
    fn resolve_file_inputs_deduplicates_repeated_paths() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let path = dir.path().join("repeated.journal");
        fs::write(&path, b"not-a-real-journal").expect("write fixture");

        let resolved = resolve_file_inputs(&[path.clone(), path.clone()]).expect("resolve files");
        assert_eq!(resolved, vec![path]);
    }

    #[test]
    fn directory_input_does_not_open_regular_file_as_file() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let path = dir.path().join("regular.journal");
        fs::write(&path, b"not-a-real-journal").expect("write fixture");

        let err = match CliInput::Directory(path).open_journal() {
            Ok(_) => panic!("regular file opened through --directory"),
            Err(err) => err,
        };
        assert!(
            err.to_string()
                .to_ascii_lowercase()
                .contains("not a directory"),
            "expected not-a-directory error, got: {err}"
        );
    }

    #[test]
    fn lines_limit_parser_preserves_systemd_direction() {
        assert_eq!(parse_lines_limit(None).unwrap(), None);
        assert!(parse_lines_limit(Some("")).is_err());
        assert_eq!(
            parse_lines_limit(Some("10")).unwrap(),
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
    fn oldest_lines_conflict_applies_only_to_show_action() {
        let show_args = Args::parse_from([
            "journalctl",
            "--file=/tmp/x.journal",
            "--lines=+1",
            "--reverse",
        ]);
        assert!(
            enforce_oldest_lines_conflict(&show_args).is_err(),
            "show action must reject --lines=+N with --reverse"
        );

        let list_boots_args = Args::parse_from([
            "journalctl",
            "--file=/tmp/x.journal",
            "--list-boots",
            "--lines=+1",
            "--reverse",
        ]);
        enforce_oldest_lines_conflict(&list_boots_args)
            .expect("non-show action should allow --lines=+N with --reverse");
    }

    #[test]
    fn timestamp_parser_accepts_stock_iso_t_forms() {
        let local_space = parse_timestamp_usec("2023-11-15 00:00").expect("space local");
        let local_t = parse_timestamp_usec("2023-11-15T00:00").expect("T local");
        assert_eq!(local_t, local_space);

        parse_timestamp_usec("2023-11-15T00:00:00Z").expect("UTC RFC3339");
        parse_timestamp_usec("2023-11-15T00:00:00.000001Z").expect("UTC RFC3339 fraction");
        parse_timestamp_usec("2023-11-15T00:00:00+02:00").expect("offset RFC3339");
        parse_timestamp_usec("2023-11-15T00:00+02:00").expect("offset without seconds");
        assert!(
            parse_timestamp_usec("2023-11-15T00:00:00+0200").is_err(),
            "stock rejects compact timezone offsets"
        );
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
                "--lines=10".to_string(),
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
    fn rotate_with_vacuum_is_distinct_unsupported_action() {
        let args = Args::parse_from(["journalctl", "--rotate", "--vacuum-files=2"]);
        let err =
            enforce_portable_unsupported(&args).expect_err("rotate+vacuum should be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("portable mode does not support --rotate with --vacuum-*"),
            "expected rotate-and-vacuum portable message, got: {msg}"
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

    #[test]
    fn boot_all_merge_is_not_rejected_as_conflict() {
        let args = Args::parse_from(preprocess_optional_boot_args([
            "journalctl".to_string(),
            "--file=/tmp/x.journal".to_string(),
            "--boot=all".to_string(),
            "--merge".to_string(),
        ]));
        enforce_boot_merge_conflict(&args).expect("boot=all must not conflict with --merge");
    }
}
