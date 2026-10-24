mod colors;
mod file_glob;
mod invocation;
use invocation::{collect_invocations_from_input, resolve_invocation_filter};
mod output;
mod output_links;
mod parser;
mod pcre;
mod terminal;
mod timestamp;
mod timestamp_zone;
mod unit_name;
mod unit_pattern;
mod vacuum_metadata;
mod vacuum_order;

use anyhow::{Result, anyhow};
use clap::builder::TypedValueParser;
use clap::{Parser, ValueEnum};
use journal::{
    Entry, FacadeError, FileHeader, FileReader, OutputMode, SdJournal, SdJournalAddConjunction,
    SdJournalAddDisjunction, SdJournalAddMatch, SdJournalEnumerateFields, SdJournalFlushMatches,
    SdJournalGetCursor, SdJournalGetEntry, SdJournalNext, SdJournalOpenDirectoryPath,
    SdJournalOpenFilePaths, SdJournalPrevious, SdJournalSeekCursor, SdJournalSeekHead,
    SdJournalSeekRealtimeUsec, SdJournalSeekTail, SdJournalSetOutputMode, SdJournalTestCursor,
    SdJournalVisitUniqueValues, parse_match_bytes, parse_match_string, verify_file,
    verify_file_with_key,
};
use output::{OutputOptions, OutputRenderer};
use regex::Regex;
use std::cell::{Cell, OnceCell};
use std::collections::{HashSet, VecDeque};
use std::ffi::{OsStr, OsString};
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

const OFFICIAL_OPTION_SURFACE_HELP: &str = r#"Official systemd v262 option reference:
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

type UnitSpecs = (Vec<Vec<u8>>, Vec<Vec<u8>>);

#[derive(Parser, Debug)]
#[command(name = "journalctl", args_override_self = true)]
#[command(about = "Portable file-backed journalctl")]
#[command(after_help = OFFICIAL_OPTION_SURFACE_HELP)]
struct Args {
    #[arg(skip)]
    action: Option<&'static str>,
    #[arg(skip)]
    unit_specs: OnceCell<std::result::Result<UnitSpecs, String>>,
    #[arg(skip)]
    unit_matches: OnceCell<std::result::Result<UnitSpecs, String>>,
    #[arg(skip)]
    resolved_invocation: OnceCell<std::result::Result<Option<String>, String>>,
    #[arg(skip)]
    since_usec: Option<u64>,
    #[arg(skip)]
    until_usec: Option<u64>,

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
    matches: Vec<OsString>,

    // Parser-recognized v262 options that the Rust CLI does not yet
    // dispatch behavior for. They exist so the parser accepts every
    // official long option; downstream validation rejects the ones that
    // are intentionally unsupported in portable mode with a portable
    // unsupported message. This keeps the parser in lock-step with the
    // shared v262 manifest under tests/parser-parity/.
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
    namespace: Option<OsString>,

    #[arg(short = 'c', long = "cursor")]
    cursor: Option<String>,
    #[arg(long = "after-cursor")]
    after_cursor: Option<String>,
    #[arg(long = "cursor-file", value_parser = clap::builder::OsStringValueParser::new().map(PathBuf::from))]
    cursor_file: Option<PathBuf>,
    #[arg(long = "this-boot")]
    this_boot: bool,
    #[arg(short = 'u', long = "unit")]
    unit: Vec<OsString>,
    #[arg(long = "user-unit")]
    user_unit: Vec<OsString>,
    #[arg(long = "invocation")]
    invocation: Option<String>,
    #[arg(short = 'I')]
    invocation_latest: bool,
    #[arg(short = 't', long = "identifier")]
    identifier: Vec<OsString>,
    #[arg(short = 'T', long = "exclude-identifier")]
    exclude_identifier: Vec<OsString>,
    #[arg(short = 'p', long = "priority")]
    priority: Vec<String>,
    #[arg(long = "facility")]
    facility: Vec<String>,
    #[arg(short = 'g', long = "grep")]
    grep: Option<OsString>,
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
    output_fields: Vec<OsString>,

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
        if !err.is::<SilentNoData>() {
            eprintln!("Error: {err}");
        }
        exit(1);
    }
}

fn portable_unsupported(feature: &str, reason: &str) -> anyhow::Error {
    anyhow!("journalctl portable mode does not support {feature}: {reason}")
}

fn run() -> Result<()> {
    let Some(args) = parser::parse_cli(std::env::args_os().collect())? else {
        return Ok(());
    };
    let full_width = args.full;
    // Enforce the v262 parser-required interaction rules. These run
    // before any dispatch so the user sees an explicit conflict error.
    enforce_source_exclusivity(&args)?;
    if let (Some(s), Some(u)) = (args.since_usec, args.until_usec) {
        if s > u {
            return Err(anyhow!("--since= must be before --until=."));
        }
    }
    enforce_cursor_source_exclusivity(&args)?;
    enforce_follow_reverse_conflict(&args)?;
    enforce_oldest_lines_conflict(&args)?;
    enforce_case_sensitive_value(&args)?;
    enforce_boot_descriptor_value(&args)?;

    enforce_action_argument_restriction(&args)?;
    enforce_field_enumeration_scope(&args)?;
    enforce_boot_merge_conflict(&args)?;
    let post_filters = CliPostFilters::from_args(&args)?;
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

    if args.verify || args.verify_only {
        return run_verify_input(&input, args.verify_key.as_deref(), effective_quiet(&args));
    }
    if args.list_boots {
        return run_list_boots(&input, &args);
    }

    let since_usec = args.since_usec;
    let until_usec = args.until_usec;
    let cursor_control = CursorControl::from_args(&args)?;
    let output_options = OutputOptions::from_args(&args, full_width);
    post_filters.capture_highlight.set(
        (output_options.colors().enabled() || output_options.urlify())
            && (matches!(args.output, OutputModeArg::Cat)
                || output_uses_exclude_identifier(args.output)),
    );

    if args.follow && is_show_action(&args) {
        let (tail, all) = match parse_lines_limit(args.lines.as_deref())? {
            Some(LinesLimit::All) => (0, true),
            Some(LinesLimit::Tail(n) | LinesLimit::Head(n)) => (n, false),
            None => match args.tail {
                Some(n) => (n, false),
                None if args.since.is_none() => (10, false),
                None if args.pager_end => (1000, false),
                None => (0, true),
            },
        };
        return run_follow(
            &input,
            &args,
            since_usec,
            until_usec,
            tail,
            all || args.no_tail,
            &post_filters,
            &cursor_control,
            &output_options,
        );
    }

    let result = (|| -> Result<()> {
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
            if !output::valid_field_name(field) {
                return Err(anyhow!("Invalid field name: {field}"));
            }
            let limit = match parse_lines_limit(args.lines.as_deref())? {
                Some(LinesLimit::Head(n) | LinesLimit::Tail(n)) => Some(n),
                _ => None,
            };
            if limit == Some(0) {
                return Ok(());
            }
            let mut count = 0usize;
            let mut stopped_at_limit = false;
            let mut stdout = std::io::stdout().lock();
            let result = SdJournalVisitUniqueValues(&mut journal, field, |value| {
                if limit == Some(count) {
                    stopped_at_limit = true;
                    return Err(FacadeError::Other("unique-limit".into()));
                }
                let value = value.split(|&b| b == 0).next().unwrap_or_default();
                stdout
                    .write_all(value)
                    .map_err(|err| FacadeError::Other(err.to_string()))?;
                stdout
                    .write_all(b"\n")
                    .map_err(|err| FacadeError::Other(err.to_string()))?;
                count += 1;
                Ok(())
            });
            if let Err(err) = result {
                if !stopped_at_limit {
                    return Err(anyhow!("field: {err}"));
                }
            }
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
                LinesLimit::Tail(n)
                    if cursor_control.seek.is_some() && !args.reverse && !grep_tail_reverse =>
                {
                    show_head_or_all_with_reverse(
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
                    )
                }
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
                    reverse || grep_tail_reverse,
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
    })();
    if result.is_ok() && is_show_action(&args) && args.grep.is_some() && !post_filters.shown.get() {
        return Err(SilentNoData.into());
    }
    result
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
            Self::Directory(path) => {
                SdJournalOpenDirectoryPath(path, 0).map_err(|err| anyhow!("open: {err}"))
            }
            Self::Files(paths) => {
                SdJournalOpenFilePaths(paths, 0).map_err(|err| anyhow!("open: {err}"))
            }
        }
    }

    fn journal_files(&self, context: &str) -> Result<(Vec<PathBuf>, bool)> {
        match self {
            Self::Files(paths) => Ok((paths.clone(), false)),
            Self::Directory(path) => collect_journal_files_for_verify(path)
                .map(|files| (files, true))
                .map_err(|err| anyhow!("{}: read directory {}: {}", context, path.display(), err)),
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
        let matches = file_glob::expand(value)?;
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

/// Parse `--lines=[+]N` per the v262 grammar.
/// Bare `--lines` is normalized to the official default tail count, 10.
/// Explicit empty `--lines=` is invalid, matching systemd v262.
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
    let n =
        parse_native_i32(stripped).ok_or_else(|| anyhow!("Failed to parse --lines='{value}'."))?;
    if n < 0 {
        return Err(anyhow!("Failed to parse --lines='{value}'."));
    }
    let n = n as usize;
    if oldest {
        Ok(Some(LinesLimit::Head(n)))
    } else {
        Ok(Some(LinesLimit::Tail(n)))
    }
}

fn grep_tail_implies_reverse(args: &Args) -> Result<bool> {
    if args.grep.is_none() || args.follow {
        return Ok(false);
    }
    let Some(limit) = parse_lines_limit(args.lines.as_deref())? else {
        return Ok(args.pager_end);
    };
    Ok(matches!(limit, LinesLimit::Tail(_)))
}

fn parse_native_number(value: &str) -> Option<(u64, bool)> {
    let mut value = value.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let mut base = 0;
    if value.starts_with("0b") || value.starts_with("0B") {
        base = 2;
    } else if value.starts_with("0o") || value.starts_with("0O") {
        base = 8;
    }
    if base != 0 {
        value = value[2..].trim_start_matches(|c: char| c.is_ascii_whitespace());
    }
    let negative = value.starts_with('-');
    value = value
        .strip_prefix('+')
        .or_else(|| value.strip_prefix('-'))
        .unwrap_or(value);
    if base == 0 {
        base = 10;
        if value.starts_with("0x") || value.starts_with("0X") {
            base = 16;
            value = &value[2..];
        } else if value.starts_with('0') {
            base = 8;
        }
    }
    if value.starts_with('+') || value.starts_with('-') {
        return None;
    }
    Some((u64::from_str_radix(value, base).ok()?, negative))
}

fn parse_native_i32(value: &str) -> Option<i32> {
    let (number, negative) = parse_native_number(value)?;
    let number = i64::try_from(number).ok()?;
    i32::try_from(if negative { -number } else { number }).ok()
}

fn parse_native_u64(value: &str) -> Option<u64> {
    let (number, negative) = parse_native_number(value)?;
    (!negative || number == 0).then_some(number)
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
    if let Some(action) = args.action {
        return action == "show";
    }
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
        args.matches[0].to_string_lossy()
    ))
}

fn action_rejects_arguments(args: &Args) -> bool {
    if let Some(action) = args.action {
        return !matches!(action, "show" | "list-catalog" | "dump-catalog");
    }
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
    if args.action == Some("rotate-and-vacuum") || args.rotate && has_vacuum_flags(args) {
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

fn open_filtered_journal(
    input: &CliInput,
    args: &Args,
    boot_descriptor_override: Option<&str>,
) -> Result<SdJournal> {
    let mut journal = input.open_journal()?;
    if let Some(invocation_id) = resolve_invocation_filter(input, args)? {
        add_invocation_matches(&mut journal, &invocation_id)?;
        apply_non_unit_matches(&mut journal, args)?;
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
    run_verify_input(&input, verify_key, false)
}

fn run_verify_input(input: &CliInput, verify_key: Option<&str>, quiet: bool) -> Result<()> {
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
                Ok(()) => {
                    if !quiet {
                        eprintln!("PASS: {}", file.display());
                    }
                }
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
        if !quiet {
            eprintln!("PASS: {}", file.display());
        }
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
        .map(|name| name.as_encoded_bytes())
        .is_some_and(|name| {
            name.ends_with(b".journal")
                || name.ends_with(b".journal~")
                || name.ends_with(b".journal.zst")
                || name.ends_with(b".journal~.zst")
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
    name: OsString,
    usage: u64,
    seqnum_id: [u8; 16],
    seqnum: u64,
    realtime: u64,
    have_seqno: bool,
}

fn has_vacuum_flags(args: &Args) -> bool {
    if let Some(action) = args.action {
        return matches!(action, "vacuum" | "rotate-and-vacuum");
    }
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
        Some(value) => parse_native_u64(value)
            .ok_or_else(|| anyhow!("failed to parse --vacuum-files value: {value}"))?,
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
    fn parse(value: &str) -> Option<u64> {
        const UNITS: [&str; 8] = ["E", "P", "T", "G", "M", "K", "B", ""];
        const FACTORS: [u64; 8] = [1 << 60, 1 << 50, 1 << 40, 1 << 30, 1 << 20, 1 << 10, 1, 1];
        let mut remaining = value;
        let mut first_unit = 0;
        let mut total = 0_u64;
        loop {
            remaining = remaining.trim_start_matches([' ', '\t', '\n', '\r']);
            if remaining.starts_with('-') {
                return None;
            }
            let number = remaining.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
            let negative = number.starts_with('-');
            let number = number.strip_prefix(['+', '-']).unwrap_or(number);
            let end = number.bytes().take_while(u8::is_ascii_digit).count();
            let mut whole = number[..end].parse::<u64>().ok()?;
            if negative {
                whole = whole.wrapping_neg();
            }
            remaining = &number[end..];
            let mut fraction = 0.0;
            if let Some(rest) = remaining.strip_prefix('.') {
                let end = rest.bytes().take_while(u8::is_ascii_digit).count();
                if end > 0 {
                    fraction = rest[..end].parse::<u64>().ok()? as f64;
                    for _ in 0..end {
                        fraction /= 10.0;
                    }
                }
                remaining = &rest[end..];
            }
            remaining = remaining.trim_start_matches([' ', '\t', '\n', '\r']);
            let unit = (first_unit..UNITS.len()).find(|&i| remaining.starts_with(UNITS[i]))?;
            let factor = FACTORS[unit];
            let ceiling = whole.checked_add(u64::from(fraction > 0.0))?;
            if ceiling > u64::MAX / factor {
                return None;
            }
            total = total.checked_add(whole * factor + (fraction * factor as f64) as u64)?;
            remaining = &remaining[UNITS[unit].len()..];
            if remaining.is_empty() {
                return Some(total);
            }
            first_unit = unit + 1;
        }
    }
    parse(value).ok_or_else(|| anyhow!("failed to parse --vacuum-size value: {value}"))
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
        let native_name = entry.file_name();
        let name = native_name.to_string_lossy();
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

    let retention_limit = if opts.max_retention_usec > 0 {
        current_realtime_usec().saturating_sub(opts.max_retention_usec)
    } else {
        0
    };
    patch_vacuum_creation_times(&mut candidates, &opts, retention_limit, sum, active_files);
    let candidate_count = candidates.len();
    let order = vacuum_order::VacuumOrder::new(&mut candidates);

    for (idx, candidate) in order.enumerate() {
        let left = active_files.saturating_add((candidate_count - idx) as u64);
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
                        candidate.name.to_string_lossy(),
                        format_journal_bytes(candidate.usage)
                    );
                }
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) if !quiet => {
                eprintln!(
                    "Failed to delete archived journal {}/{}: {err}",
                    dir.display(),
                    candidate.name.to_string_lossy()
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
                name: path.file_name().unwrap_or_default().to_os_string(),
                path,
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
                name: path.file_name().unwrap_or_default().to_os_string(),
                path,
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
        let flags = libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK;
        #[cfg(target_os = "linux")]
        let flags = flags | libc::O_NOATIME;
        opts.custom_flags(flags);
        #[cfg(target_os = "linux")]
        match opts.open(path) {
            Ok(file) => return Ok(file),
            Err(_) => {
                // Native retries when preserving atime is unavailable to the caller.
                opts.custom_flags(flags & !libc::O_NOATIME);
            }
        }
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
    candidate.realtime = vacuum_metadata::oldest_realtime(metadata, candidate.realtime);
}

fn patch_vacuum_creation_times(
    candidates: &mut [VacuumCandidate],
    opts: &VacuumOptions,
    retention_limit: u64,
    usage: u64,
    active_files: u64,
) {
    let Some(first) = candidates.first() else {
        return;
    };
    if retention_limit == 0
        && (opts.max_use == 0 || usage <= opts.max_use)
        && (opts.max_files == 0
            || active_files.saturating_add(candidates.len() as u64) <= opts.max_files)
    {
        return;
    }
    let sequence_only = first.have_seqno
        && candidates
            .iter()
            .all(|candidate| candidate.have_seqno && candidate.seqnum_id == first.seqnum_id);
    for candidate in candidates {
        // A shared sequence ID fixes order; only unresolved age can need I/O.
        if sequence_only && (retention_limit == 0 || candidate.realtime < retention_limit) {
            continue;
        }
        candidate.realtime =
            vacuum_metadata::creation_realtime(&candidate.path, candidate.realtime);
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
    {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(b"File path: ")?;
        stdout.write_all(path.as_os_str().as_encoded_bytes())?;
        stdout.write_all(b"\n")?;
    }
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
    let rows = select_boot_rows(&boots, args)?;
    if rows.is_empty() {
        return Err(no_data_error(effective_quiet(args), "No boot found."));
    }
    if is_json_output(args.output) {
        let rows: Vec<_> = rows.iter().map(|row| serde_json::json!({
            "index": row.index, "boot_id": row.boot_id,
            "first_entry": table_timestamp(row.first_entry), "last_entry": table_timestamp(row.last_entry),
        })).collect();
        return print_json_table(args.output, &rows);
    }
    if !args.quiet {
        print!("{}", colors::Colors::detect().table_header("BOOT ID"));
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
    let rows = select_invocation_rows(&invocations, args)?;
    if rows.is_empty() {
        return Err(no_data_error(
            effective_quiet(args),
            "No invocation ID found.",
        ));
    }
    if is_json_output(args.output) {
        let rows: Vec<_> = rows.iter().map(|(index, row)| serde_json::json!({
            "index": index, "invocation_id": row.id,
            "first_entry": table_timestamp(row.first_usec), "last_entry": table_timestamp(row.last_usec),
        })).collect();
        return print_json_table(args.output, &rows);
    }
    if !args.quiet {
        print!("{}", colors::Colors::detect().table_header("INVOCATION ID"));
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

fn apply_matches(journal: &mut SdJournal, matches: &[OsString]) -> Result<()> {
    for item in matches {
        if item.as_encoded_bytes() == b"+" {
            SdJournalAddDisjunction(journal).map_err(|err| anyhow!("add disjunction: {err}"))?;
            continue;
        }
        let data = parse_match_bytes(item.as_encoded_bytes())
            .map_err(|err| anyhow!("invalid match {}: {err}", item.to_string_lossy()))?;
        SdJournalAddMatch(journal, &data).map_err(|err| anyhow!("add match: {err}"))?;
    }
    Ok(())
}

fn validate_path_match_arguments(matches: &[OsString]) -> Result<()> {
    for (i, arg) in matches.iter().enumerate() {
        if arg == "+" && (i == 0 || i + 1 == matches.len() || matches[i - 1] == "+") {
            return Err(anyhow!("\"+\" can only be used between terms."));
        }
    }

    for item in matches {
        if item != "+" && !item.as_encoded_bytes().contains(&b'=') {
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

fn add_unit_match_group(journal: &mut SdJournal, pairs: &[(&str, &[u8])]) -> Result<()> {
    for (field, value) in pairs {
        let mut payload = format!("{field}=").into_bytes();
        payload.extend_from_slice(value);
        SdJournalAddMatch(journal, &payload)?;
    }
    Ok(())
}

fn add_journalctl_unit_matches(journal: &mut SdJournal, args: &Args) -> Result<()> {
    if args.unit.is_empty() && args.user_unit.is_empty() {
        return Ok(());
    }
    // Follow retains the initial wildcard expansion, like native journalctl.
    let (system_units, user_units) = args
        .unit_matches
        .get_or_init(|| {
            (|| -> Result<UnitSpecs> {
                let (system, user) = effective_unit_specs(args)?;
                let expanded_system = expand_unit_specs(journal, system, SYSTEM_UNIT_FIELDS_FULL)?;
                let expanded_user = expand_unit_specs(journal, user, USER_UNIT_FIELDS_FULL)?;
                if (!system.is_empty() || !user.is_empty())
                    && expanded_system.is_empty()
                    && expanded_user.is_empty()
                {
                    return Err(anyhow!("Failed to add filter for units: No data available"));
                }
                Ok((expanded_system, expanded_user))
            })()
            .map_err(|err| err.to_string())
        })
        .as_ref()
        .map_err(|err| anyhow!("{err}"))?;
    if system_units.is_empty() && user_units.is_empty() {
        return Ok(());
    }

    for unit in system_units {
        add_system_unit_match_groups(journal, unit, true)?;
    }

    let uid = current_uid_string();
    for unit in user_units {
        add_user_unit_match_groups(journal, unit, uid.as_deref(), true)?;
    }

    add_unit_conjunction(journal)
}

fn add_system_unit_match_groups(journal: &mut SdJournal, unit: &[u8], full: bool) -> Result<()> {
    add_unit_match_group(journal, &[("_SYSTEMD_UNIT", unit)])?;
    add_unit_disjunction(journal)?;

    add_unit_match_group(
        journal,
        &[
            ("_SYSTEMD_CGROUP", b"/init.scope".as_slice()),
            ("UNIT", unit),
        ],
    )?;
    add_unit_disjunction(journal)?;

    add_unit_match_group(
        journal,
        &[("_UID", b"0".as_slice()), ("OBJECT_SYSTEMD_UNIT", unit)],
    )?;
    add_unit_disjunction(journal)?;

    if !full {
        return Ok(());
    }
    add_unit_match_group(
        journal,
        &[
            ("MESSAGE_ID", COREDUMP_MESSAGE_ID.as_bytes()),
            ("COREDUMP_UNIT", unit),
        ],
    )?;

    if unit.ends_with(b".slice") {
        add_unit_disjunction(journal)?;
        add_unit_match_group(journal, &[("_SYSTEMD_SLICE", unit)])?;
    }

    add_unit_disjunction(journal)
}

fn add_user_unit_match_groups(
    journal: &mut SdJournal,
    unit: &[u8],
    uid: Option<&str>,
    full: bool,
) -> Result<()> {
    add_user_unit_match_group(journal, &[("_SYSTEMD_USER_UNIT", unit)], uid, false)?;
    add_unit_disjunction(journal)?;

    add_user_unit_match_group(journal, &[("USER_UNIT", unit)], uid, false)?;
    add_unit_disjunction(journal)?;

    add_user_unit_match_group(journal, &[("OBJECT_SYSTEMD_USER_UNIT", unit)], uid, true)?;
    add_unit_disjunction(journal)?;

    if !full {
        return Ok(());
    }
    add_user_unit_match_group(journal, &[("COREDUMP_USER_UNIT", unit)], uid, true)?;

    if unit.ends_with(b".slice") {
        add_unit_disjunction(journal)?;
        add_user_unit_match_group(journal, &[("_SYSTEMD_USER_SLICE", unit)], uid, false)?;
    }

    add_unit_disjunction(journal)
}

fn add_user_unit_match_group(
    journal: &mut SdJournal,
    pairs: &[(&str, &[u8])],
    uid: Option<&str>,
    include_root_uid: bool,
) -> Result<()> {
    add_unit_match_group(journal, pairs)?;
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
    specs: &[Vec<u8>],
    fields: &[&str],
) -> Result<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut patterns = Vec::new();

    for spec in specs {
        let unit = spec.clone();
        if unit.iter().any(|c| b"*?[".contains(c)) {
            patterns.push(unit);
        } else if seen.insert(unit.clone()) {
            out.push(unit);
        }
    }

    if patterns.is_empty() {
        return Ok(out);
    }

    let mut matcher = unit_pattern::Matcher::new(&patterns)?;
    for field in fields {
        SdJournalVisitUniqueValues(journal, field, |value| {
            let value = value.split(|c| *c == 0).next().unwrap_or_default();
            if matcher.matches(value) {
                let value = value.to_vec();
                if seen.insert(value.clone()) {
                    out.push(value);
                }
            }
            Ok(())
        })
        .map_err(|err| anyhow!("query possible units for {field}: {err}"))?;
    }

    Ok(out)
}

#[cfg(any(
    test,
    not(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))
))]
fn is_glob_pattern(value: &str) -> bool {
    value.contains(['*', '?', '['])
}

#[cfg(any(
    test,
    not(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))
))]
fn glob_pattern_to_regex(pattern: &str) -> String {
    let mut out = String::from("(?s)^");
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            '[' => {
                let mut end = i + 1;
                if end < chars.len() && matches!(chars[end], '!' | '^') {
                    end += 1;
                }
                if end < chars.len() && chars[end] == ']' {
                    end += 1;
                }
                while end < chars.len() && chars[end] != ']' {
                    if chars[end] == '['
                        && end + 1 < chars.len()
                        && matches!(chars[end + 1], ':' | '.')
                    {
                        let mut k = end + 2;
                        while k + 1 < chars.len()
                            && !(chars[k] == chars[end + 1] && chars[k + 1] == ']')
                        {
                            k += 1;
                        }
                        if k + 1 < chars.len() {
                            end = k + 2;
                            continue;
                        }
                    }
                    end += 1;
                }
                if end == chars.len() {
                    out.push_str("\\[");
                } else {
                    out.push('[');
                    let mut start = i + 1;
                    if matches!(chars[start], '!' | '^') {
                        out.push('^');
                        start += 1;
                    }
                    let mut k = start;
                    while k < end {
                        if chars[k] == '[' && k + 1 < end && matches!(chars[k + 1], ':' | '.') {
                            let marker = chars[k + 1];
                            let mut n = k + 2;
                            while n + 1 < end && !(chars[n] == marker && chars[n + 1] == ']') {
                                n += 1;
                            }
                            if n + 1 < end {
                                if marker == ':' {
                                    out.extend(&chars[k..n + 2]);
                                } else {
                                    if n != k + 3 {
                                        return "[".into();
                                    }
                                    if r"\[]^-".contains(chars[k + 2]) {
                                        out.push('\\');
                                    }
                                    out.push(chars[k + 2]);
                                }
                                k = n + 2;
                                continue;
                            }
                        }
                        let c = chars[k];
                        if matches!(c, '\\' | ']' | '[') {
                            out.push('\\');
                        }
                        out.push(c);
                        k += 1;
                    }
                    out.push(']');
                    i = end;
                }
            }
            c => out.push_str(&regex::escape(&c.to_string())),
        }
        i += 1;
    }
    out.push('$');
    out
}

fn apply_cli_matches(journal: &mut SdJournal, args: &Args) -> Result<()> {
    add_journalctl_unit_matches(journal, args)?;
    apply_non_unit_matches(journal, args)
}

fn apply_non_unit_matches(journal: &mut SdJournal, args: &Args) -> Result<()> {
    if args.dmesg {
        add_field_matches(journal, "_TRANSPORT", ["kernel"])?;
    }
    if !args.identifier.is_empty() {
        for value in &args.identifier {
            let mut data = b"SYSLOG_IDENTIFIER=".to_vec();
            data.extend_from_slice(value.as_encoded_bytes());
            SdJournalAddMatch(journal, &data)
                .map_err(|err| anyhow!("add identifier match: {err}"))?;
        }
        SdJournalAddConjunction(journal)
            .map_err(|err| anyhow!("add identifier conjunction: {err}"))?;
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

fn raw_unit_specs(args: &Args) -> (Vec<OsString>, Vec<OsString>) {
    let mut system = args.unit.clone();
    let mut user = args.user_unit.clone();
    if args.user && !args.system {
        for value in system.drain(..) {
            if !user.contains(&value) {
                user.push(value);
            }
        }
    }
    (system, user)
}

fn effective_unit_specs(args: &Args) -> Result<&UnitSpecs> {
    args.unit_specs
        .get_or_init(|| {
            let (system, user) = raw_unit_specs(args);
            let normalize = |values: Vec<OsString>| -> Result<Vec<Vec<u8>>> {
                values
                    .iter()
                    .map(|value| {
                        let (name, escaped) = unit_name::normalize(value.as_encoded_bytes())?;
                        if let Some(escaped) = escaped.filter(|_| !args.quiet) {
                            let mut diagnostic = b"Invalid unit name \"".to_vec();
                            diagnostic.extend_from_slice(value.as_encoded_bytes());
                            diagnostic.extend_from_slice(b"\" escaped as \"");
                            diagnostic.extend_from_slice(escaped.as_bytes());
                            diagnostic
                                .extend_from_slice(b"\" (maybe you should use systemd-escape?).\n");
                            let _ = std::io::stderr().lock().write_all(&diagnostic);
                        }
                        Ok(name.into_bytes())
                    })
                    .collect()
            };
            (|| Ok((normalize(system)?, normalize(user)?)))()
                .map_err(|e: anyhow::Error| e.to_string())
        })
        .as_ref()
        .map_err(|e| anyhow!("{e}"))
}

#[derive(Debug)]
struct CliPostFilters {
    grep: Option<pcre::Pattern>,
    shown: Cell<bool>,
    exclude_identifiers: std::collections::HashSet<Vec<u8>>,
    capture_highlight: Cell<bool>,
    highlight: Cell<colors::MatchRange>,
    buffered_highlights: std::cell::RefCell<VecDeque<colors::MatchRange>>,
}

impl CliPostFilters {
    fn from_args(args: &Args) -> Result<Self> {
        let exclude_identifiers = if output_uses_exclude_identifier(args.output) {
            args.exclude_identifier
                .iter()
                .map(|value| value.as_encoded_bytes().to_vec())
                .collect()
        } else {
            std::collections::HashSet::new()
        };
        Ok(Self {
            grep: compile_grep_filter(args.grep.as_deref(), args.case_sensitive.as_deref())?,
            exclude_identifiers,
            shown: Cell::new(false),
            capture_highlight: Cell::new(false),
            highlight: Cell::new(colors::MatchRange::default()),
            buffered_highlights: std::cell::RefCell::new(VecDeque::new()),
        })
    }

    fn matches(&self, entry: &Entry) -> Result<bool> {
        let Some(pattern) = &self.grep else {
            return Ok(true);
        };
        let value = entry
            .field_values
            .get("MESSAGE")
            .and_then(|v| v.first().map(Vec::as_slice))
            .or_else(|| entry.fields.get("MESSAGE").map(Vec::as_slice));
        match value {
            Some(value) if self.capture_highlight.get() => {
                let mut highlight = colors::MatchRange::default();
                let matched = pattern.match_with_highlight(value, &mut highlight)?;
                self.highlight.set(highlight);
                Ok(matched)
            }
            Some(value) => pattern.matches(value),
            None => Ok(false),
        }
    }

    fn buffer_highlight(&self) {
        if self.capture_highlight.get() && self.grep.is_some() {
            self.buffered_highlights
                .borrow_mut()
                .push_back(self.highlight.get());
        }
    }

    fn restore_highlight(&self) {
        if self.capture_highlight.get() {
            if let Some(highlight) = self.buffered_highlights.borrow_mut().pop_front() {
                self.highlight.set(highlight);
            }
        }
    }

    fn excludes_entry(&self, entry: &Entry) -> bool {
        if self.exclude_identifiers.is_empty() {
            return false;
        }
        output::last_value_for_name(entry, "SYSLOG_IDENTIFIER").is_some_and(|value| {
            self.exclude_identifiers
                .contains(value.split(|&b| b == 0).next().unwrap_or_default())
        })
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

fn compile_grep_filter(
    pattern: Option<&OsStr>,
    case_sensitive: Option<&str>,
) -> Result<Option<pcre::Pattern>> {
    let Some(pattern) = pattern else {
        return Ok(None);
    };
    let bytes = pattern.as_encoded_bytes();
    let sensitive = match case_sensitive {
        Some(value) => parse_bool_option("--case-sensitive", value)?,
        None => pcre::Pattern::compile(b"[[:upper:]]", true)?.matches(bytes)?,
    };
    pcre::Pattern::compile(bytes, sensitive).map(Some)
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
    let level = match value {
        "emerg" => Some(0),
        "alert" => Some(1),
        "crit" => Some(2),
        "err" => Some(3),
        "warning" => Some(4),
        "notice" => Some(5),
        "info" => Some(6),
        "debug" => Some(7),
        _ => parse_native_i32(value)
            .filter(|value| (0..=7).contains(value))
            .map(|value| value as u8),
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
    if let Some(number) = parse_native_i32(value) {
        if (0..=127).contains(&number) {
            return Ok(number as u8);
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
    colors: colors::Colors,
}

impl BootSeparatorState {
    fn new(mode: OutputModeArg, quiet: bool, suppressed: bool) -> Self {
        Self {
            enabled: !quiet && !suppressed && output_emits_boot_separators(mode),
            previous_boot: None,
            colors: colors::Colors::default(),
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
            stdout.write_all(self.colors.boot(&hex::encode(entry.boot_id)).as_bytes())?;
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

fn render_matching_entry<W: Write>(
    stdout: &mut W,
    renderer: &mut OutputRenderer,
    boots: &mut BootSeparatorState,
    post_filters: &CliPostFilters,
    entry: &Entry,
) -> Result<bool> {
    boots.colors = renderer.colors();
    boots.before_entry(stdout, entry)?;
    if !post_filters.matches(entry)? {
        return Ok(false);
    }
    render_entry(stdout, renderer, post_filters, entry)?;
    Ok(true)
}

fn render_entry<W: Write>(
    stdout: &mut W,
    renderer: &mut OutputRenderer,
    post_filters: &CliPostFilters,
    entry: &Entry,
) -> Result<bool> {
    if post_filters.excludes_entry(entry) {
        post_filters.shown.set(true);
        return Ok(false);
    }
    if renderer.skips_entry(entry) {
        post_filters.shown.set(true);
        return Ok(false);
    }
    if post_filters.capture_highlight.get() {
        renderer.set_highlight(post_filters.highlight.get());
    }
    let output = renderer.render(entry)?;
    stdout.write_all(&output)?;
    post_filters.shown.set(true);
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
    if limit == Some(0) {
        let cursor = initial_cursor(
            journal,
            since_usec,
            until_usec,
            cursor_control,
            reverse,
            show_cursor,
        )?;
        print_no_entries(&mut stdout, 0, quiet)?;
        return finish_cursor_output(
            &mut stdout,
            show_cursor,
            cursor_control.update_file.as_deref(),
            cursor.as_deref(),
        );
    }
    let mut renderer = OutputRenderer::new(output_options.clone());
    let mut boot_separators = BootSeparatorState::new(
        output_options.mode(),
        quiet,
        output_options.suppress_boot_separators(),
    );
    let mut shown = 0usize;
    let mut last_cursor = String::new();
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
                show_cursor || cursor_control.update_file.is_some(),
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
            show_cursor || cursor_control.update_file.is_some(),
        )?;
        for event in events {
            match event {
                ReverseOutputEvent::Boot(boot_id) => {
                    stdout.write_all(
                        output_options
                            .colors()
                            .boot(&hex::encode(boot_id))
                            .as_bytes(),
                    )?;
                }
                ReverseOutputEvent::Entry(entry) => {
                    post_filters.restore_highlight();
                    render_entry(&mut stdout, &mut renderer, post_filters, &entry)?;
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
    for_each_entry_with_checkpoint(
        journal,
        since_usec,
        until_usec,
        reverse,
        cursor_control.seek.as_ref(),
        (show_cursor || cursor_control.update_file.is_some()).then_some(&mut last_cursor),
        |entry| {
            if !render_matching_entry(
                &mut stdout,
                &mut renderer,
                &mut boot_separators,
                post_filters,
                &entry,
            )? {
                return Ok(true);
            }
            shown += 1;
            Ok(limit.is_none_or(|limit| shown < limit))
        },
    )?;
    print_no_entries(&mut stdout, shown, quiet)?;
    finish_cursor_output(
        &mut stdout,
        show_cursor,
        cursor_control.update_file.as_deref(),
        Some(last_cursor.as_str()),
    )?;
    Ok(())
}

fn initial_cursor(
    journal: &mut SdJournal,
    since: Option<u64>,
    until: Option<u64>,
    control: &CursorControl,
    from_tail: bool,
    show_cursor: bool,
) -> Result<Option<String>> {
    if !show_cursor && control.update_file.is_none() && control.seek.is_none() {
        return Ok(None);
    }
    let mut cursor = String::new();
    if let Some(seek) = control.seek.as_ref() {
        seek_cursor_start_tracked(journal, seek, from_tail, Some(&mut cursor))?;
        // A successful --after-cursor advance replaces the seeded cursor.
        if let Ok(current) = SdJournalGetCursor(journal) {
            cursor = current;
        }
        return Ok((!cursor.is_empty()).then_some(cursor));
    }
    if from_tail {
        if let Some(until) = until {
            SdJournalSeekRealtimeUsec(journal, until)
        } else {
            SdJournalSeekTail(journal)
        }
    } else if let Some(since) = since {
        SdJournalSeekRealtimeUsec(journal, since)
    } else {
        SdJournalSeekHead(journal)
    }
    .map_err(|err| anyhow!("seek: {err}"))?;
    let n = if from_tail {
        SdJournalPrevious(journal)
    } else {
        SdJournalNext(journal)
    }
    .map_err(|err| anyhow!("iterate: {err}"))?;
    if n == 0 {
        return Ok(None);
    }
    Ok(Some(
        SdJournalGetCursor(journal).map_err(|err| anyhow!("get cursor: {err}"))?,
    ))
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
        let cursor = initial_cursor(
            journal,
            since_usec,
            until_usec,
            cursor_control,
            true,
            show_cursor,
        )?;
        let mut stdout = std::io::stdout().lock();
        print_no_entries(&mut stdout, 0, quiet)?;
        finish_cursor_output(
            &mut stdout,
            show_cursor,
            cursor_control.update_file.as_deref(),
            cursor.as_deref(),
        )?;
        return Ok(());
    }

    let mut last_cursor = String::new();
    let capture_cursor = show_cursor || cursor_control.update_file.is_some();
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
            show_cursor || cursor_control.update_file.is_some(),
        )?;
        let mut stdout = std::io::stdout().lock();
        let mut renderer = OutputRenderer::new(output_options.clone());
        for event in events {
            match event {
                ReverseOutputEvent::Boot(boot_id) => {
                    stdout.write_all(
                        output_options
                            .colors()
                            .boot(&hex::encode(boot_id))
                            .as_bytes(),
                    )?;
                }
                ReverseOutputEvent::Entry(entry) => {
                    post_filters.restore_highlight();
                    render_entry(&mut stdout, &mut renderer, post_filters, &entry)?;
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
        for_each_entry_with_direction(journal, since_usec, until_usec, true, None, |entry| {
            selected.push_front(entry);
            Ok(selected.len() < limit)
        })?;
    } else {
        for_each_entry_with_checkpoint(
            journal,
            since_usec,
            until_usec,
            false,
            cursor_control.seek.as_ref(),
            capture_cursor.then_some(&mut last_cursor),
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
    let mut shown = 0;
    for entry in &selected {
        if capture_cursor && cursor_control.seek.is_none() {
            last_cursor.clone_from(&entry.cursor);
        }
        if render_matching_entry(
            &mut stdout,
            &mut renderer,
            &mut boot_separators,
            post_filters,
            entry,
        )? {
            shown += 1;
        }
    }
    if capture_cursor
        && cursor_control.seek.is_none()
        && until_usec.is_some()
        && selected.len() < limit
    {
        if !last_cursor.is_empty() {
            SdJournalSeekCursor(journal, &last_cursor)
                .map_err(|err| anyhow!("seek cursor: {err}"))?;
        } else {
            SdJournalSeekRealtimeUsec(journal, until_usec.unwrap())
                .map_err(|err| anyhow!("seek realtime: {err}"))?;
        }
        if SdJournalNext(journal).map_err(|err| anyhow!("next: {err}"))? > 0 {
            last_cursor =
                SdJournalGetCursor(journal).map_err(|err| anyhow!("get cursor: {err}"))?;
        }
    }
    print_no_entries(&mut stdout, shown, quiet)?;
    finish_cursor_output(
        &mut stdout,
        show_cursor,
        cursor_control.update_file.as_deref(),
        Some(last_cursor.as_str()),
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
    capture_cursor: bool,
    stdout: &mut W,
    renderer: &mut OutputRenderer,
) -> Result<(usize, Option<String>)> {
    let mut boot_context = BootSeparatorState::new(
        output_options.mode(),
        quiet,
        output_options.suppress_boot_separators(),
    );
    let mut count = 0usize;
    let mut last_cursor: Option<String> = None;

    let mut visit = |entry: Entry| -> Result<bool> {
        if capture_cursor && !entry.cursor.is_empty() {
            last_cursor
                .get_or_insert_default()
                .clone_from(&entry.cursor);
        }
        if since_usec.is_some_and(|since| entry.realtime < since) {
            return Ok(false);
        }
        if boot_context.enabled {
            if boot_context
                .previous_boot
                .is_some_and(|previous| previous != entry.boot_id)
            {
                stdout.write_all(
                    output_options
                        .colors()
                        .boot(&hex::encode(entry.boot_id))
                        .as_bytes(),
                )?;
            }
            boot_context.previous_boot = Some(entry.boot_id);
        }
        if since_usec.is_none_or(|since| entry.realtime >= since)
            && until_usec.is_none_or(|until| entry.realtime <= until)
            && post_filters.matches(&entry)?
        {
            render_entry(stdout, renderer, post_filters, &entry)?;
            count += 1;
        }
        Ok(true)
    };

    if let Some(cursor_seek) = cursor_seek {
        let mut initial = String::new();
        if let Some(entry) = seek_cursor_start_tracked(
            journal,
            cursor_seek,
            true,
            capture_cursor.then_some(&mut initial),
        )? {
            if !visit(entry)? {
                return Ok((count, last_cursor));
            }
        } else {
            return Ok((count, (!initial.is_empty()).then_some(initial)));
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
    capture_cursor: bool,
) -> Result<(Vec<ReverseOutputEvent>, usize, Option<String>)> {
    let mut boot_context = BootSeparatorState::new(
        output_options.mode(),
        quiet,
        output_options.suppress_boot_separators(),
    );
    let mut events = Vec::new();
    let mut count = 0usize;
    let mut last_cursor: Option<String> = None;

    let mut visit = |entry: Entry| -> Result<bool> {
        if capture_cursor && !entry.cursor.is_empty() {
            last_cursor
                .get_or_insert_default()
                .clone_from(&entry.cursor);
        }
        if since_usec.is_some_and(|since| entry.realtime < since) {
            return Ok(false);
        }
        if boot_context.enabled {
            if boot_context
                .previous_boot
                .is_some_and(|previous| previous != entry.boot_id)
            {
                events.push(ReverseOutputEvent::Boot(entry.boot_id));
            }
            boot_context.previous_boot = Some(entry.boot_id);
        }
        if since_usec.is_none_or(|since| entry.realtime >= since)
            && until_usec.is_none_or(|until| entry.realtime <= until)
            && post_filters.matches(&entry)?
        {
            events.push(ReverseOutputEvent::Entry(entry));
            post_filters.buffer_highlight();
            count += 1;
            if limit > 0 && count >= limit {
                return Ok(false);
            }
        }
        Ok(true)
    };

    if let Some(cursor_seek) = cursor_seek {
        let mut initial = String::new();
        if let Some(entry) = seek_cursor_start_tracked(
            journal,
            cursor_seek,
            true,
            capture_cursor.then_some(&mut initial),
        )? {
            if !visit(entry)? {
                return Ok((events, count, last_cursor));
            }
        } else {
            return Ok((events, count, (!initial.is_empty()).then_some(initial)));
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
    Ok((events, count, last_cursor))
}

fn for_each_entry_with_direction<F>(
    journal: &mut SdJournal,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
    reverse: bool,
    cursor_seek: Option<&CursorSeek>,
    visitor: F,
) -> Result<()>
where
    F: FnMut(Entry) -> Result<bool>,
{
    for_each_entry_with_checkpoint(
        journal,
        since_usec,
        until_usec,
        reverse,
        cursor_seek,
        None,
        visitor,
    )
}

fn for_each_entry_with_checkpoint<F>(
    journal: &mut SdJournal,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
    reverse: bool,
    cursor_seek: Option<&CursorSeek>,
    mut checkpoint: Option<&mut String>,
    mut visitor: F,
) -> Result<()>
where
    F: FnMut(Entry) -> Result<bool>,
{
    if reverse {
        if let Some(cursor_seek) = cursor_seek {
            if let Some(entry) =
                seek_cursor_start_tracked(journal, cursor_seek, true, checkpoint.as_deref_mut())?
            {
                if let Some(cursor) = checkpoint.as_deref_mut() {
                    cursor.clone_from(&entry.cursor);
                }
                if since_usec.is_none_or(|since| entry.realtime >= since)
                    && until_usec.is_none_or(|until| entry.realtime <= until)
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
                    if let Some(cursor) = checkpoint.as_deref_mut() {
                        cursor.clone_from(&entry.cursor);
                    }
                    if since_usec.is_some_and(|since| entry.realtime < since) {
                        break;
                    }
                    if since_usec.is_none_or(|since| entry.realtime >= since)
                        && until_usec.is_none_or(|until| entry.realtime <= until)
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
        if let Some(entry) =
            seek_cursor_start_tracked(journal, cursor_seek, false, checkpoint.as_deref_mut())?
        {
            if let Some(cursor) = checkpoint.as_deref_mut() {
                cursor.clone_from(&entry.cursor);
            }
            if since_usec.is_none_or(|since| entry.realtime >= since)
                && until_usec.is_none_or(|until| entry.realtime <= until)
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
                if let Some(cursor) = checkpoint.as_deref_mut() {
                    cursor.clone_from(&entry.cursor);
                }
                if until_usec.is_some_and(|until| entry.realtime > until) {
                    break;
                }
                if since_usec.is_none_or(|since| entry.realtime >= since)
                    && until_usec.is_none_or(|until| entry.realtime <= until)
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

fn seek_cursor_start_tracked(
    journal: &mut SdJournal,
    cursor_seek: &CursorSeek,
    reverse: bool,
    checkpoint: Option<&mut String>,
) -> Result<Option<Entry>> {
    SdJournalSeekCursor(journal, &cursor_seek.cursor)
        .map_err(|err| anyhow!("seek cursor: {err}"))?;
    if let Some(checkpoint) = checkpoint {
        match SdJournalGetCursor(journal) {
            Ok(cursor) => *checkpoint = cursor,
            Err(FacadeError::NoEntry | FacadeError::EndOfEntries) => {}
            Err(err) => return Err(anyhow!("get cursor: {err}")),
        }
    }
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
    let (boot_id, rest) = if descriptor
        .as_bytes()
        .get(..32)
        .is_some_and(|id| id.iter().all(u8::is_ascii_hexdigit))
    {
        let id = &descriptor[..32];
        (
            if id.bytes().all(|b| b == b'0') {
                String::new()
            } else {
                id.to_ascii_lowercase()
            },
            &descriptor[32..],
        )
    } else {
        (String::new(), descriptor)
    };
    if descriptor.len() >= 32
        && !rest.is_empty()
        && !rest.starts_with('+')
        && !rest.starts_with('-')
    {
        return Err(anyhow!("failed to parse boot descriptor: {descriptor}"));
    }
    let offset = if rest.is_empty() {
        0
    } else {
        parse_native_i32(rest)
            .ok_or_else(|| anyhow!("failed to parse boot descriptor: {descriptor}"))?
            as isize
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

fn parse_duration_usec_allow_zero(value: &str) -> Result<u64> {
    timestamp::duration(value)
}

fn for_each_follow_entry<F>(
    input: &CliInput,
    args: &Args,
    boot_descriptor_override: Option<&str>,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
    cursor_seek: Option<&CursorSeek>,
    mut visitor: F,
) -> Result<()>
where
    F: FnMut(Entry) -> Result<()>,
{
    let mut journal = open_filtered_journal(input, args, boot_descriptor_override)?;
    for_each_entry_with_direction(
        &mut journal,
        since_usec,
        until_usec,
        false,
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
    all: bool,
    post_filters: &CliPostFilters,
    cursor_control: &CursorControl,
    output_options: &OutputOptions,
) -> Result<()> {
    let mut last_seen_cursor: Option<String> = None;
    let mut last_visited_cursor: Option<String> = None;
    let mut renderer = OutputRenderer::new(output_options.clone());
    let mut boot_separators = BootSeparatorState::new(
        output_options.mode(),
        effective_quiet(args),
        output_options.suppress_boot_separators(),
    );
    let mut stdout = std::io::stdout().lock();
    let boot_descriptor_override = resolve_follow_boot_descriptor(input, args)?;
    let boot_descriptor_override = boot_descriptor_override.as_deref();

    if cursor_control.seek.is_some() || all {
        for_each_follow_entry(
            input,
            args,
            boot_descriptor_override,
            since_usec,
            until_usec,
            cursor_control.seek.as_ref(),
            |entry| {
                let matched = render_matching_entry(
                    &mut stdout,
                    &mut renderer,
                    &mut boot_separators,
                    post_filters,
                    &entry,
                )?;
                if matched && !entry.cursor.is_empty() {
                    last_seen_cursor = Some(entry.cursor.clone());
                    update_cursor_file(cursor_control, last_seen_cursor.as_deref())?;
                }
                if !entry.cursor.is_empty() {
                    last_visited_cursor = Some(entry.cursor);
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
            cursor_control.seek.as_ref(),
            |entry| {
                if tail == 0 {
                    if !entry.cursor.is_empty() {
                        last_visited_cursor = Some(entry.cursor);
                    }
                    return Ok(());
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
        if let Some(entry) = selected.back().filter(|entry| !entry.cursor.is_empty()) {
            last_visited_cursor = Some(entry.cursor.clone());
        }
        for entry in &selected {
            if render_matching_entry(
                &mut stdout,
                &mut renderer,
                &mut boot_separators,
                post_filters,
                entry,
            )? && !entry.cursor.is_empty()
            {
                last_seen_cursor = Some(entry.cursor.clone());
            }
        }
        update_cursor_file(cursor_control, last_seen_cursor.as_deref())?;
    }

    if cursor_control.update_file.is_some() && last_visited_cursor != last_seen_cursor {
        update_cursor_file(cursor_control, last_visited_cursor.as_deref())?;
        last_seen_cursor.clone_from(&last_visited_cursor);
    }
    let min_poll_interval = Duration::from_millis(100);
    let max_poll_interval = Duration::from_secs(1);
    let mut poll_interval = min_poll_interval;
    loop {
        thread::sleep(poll_interval);
        let last_cursor = last_visited_cursor.clone();
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
            cursor_seek,
            |entry| {
                let matched = render_matching_entry(
                    &mut stdout,
                    &mut renderer,
                    &mut boot_separators,
                    post_filters,
                    &entry,
                )?;
                if matched {
                    emitted += 1;
                }
                if matched && !entry.cursor.is_empty() {
                    last_seen_cursor = Some(entry.cursor.clone());
                    update_cursor_file(cursor_control, last_seen_cursor.as_deref())?;
                }
                if !entry.cursor.is_empty() {
                    last_visited_cursor = Some(entry.cursor);
                }
                Ok(())
            },
        )?;
        if cursor_control.update_file.is_some() && last_visited_cursor != last_seen_cursor {
            update_cursor_file(cursor_control, last_visited_cursor.as_deref())?;
            last_seen_cursor.clone_from(&last_visited_cursor);
        }
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
    // Every official systemd v262 long option must be recognized by the
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

    #[test]
    fn every_official_long_option_is_parsed() {
        for (opt, takes_value) in OFFICIAL_LONG_OPTIONS {
            let flag = match (*opt, *takes_value) {
                ("output", true) => "--output=short".to_string(),
                ("synchronize-on-exit", true) => "--synchronize-on-exit=false".to_string(),
                ("since" | "until", true) => format!("--{opt}=@0"),
                ("priority" | "facility", true) => format!("--{opt}=3"),
                ("invocation", true) => "--invocation=all".to_string(),
                ("interval" | "vacuum-time", true) => format!("--{opt}=1s"),
                ("vacuum-files", true) => "--vacuum-files=1".to_string(),
                ("vacuum-size", true) => "--vacuum-size=1M".to_string(),
                (_, true) => format!("--{opt}=placeholder"),
                (_, false) => format!("--{opt}"),
            };
            let argv: Vec<String> = vec!["journalctl".to_string(), flag];
            let parsed = parser::parse_cli(argv.iter().map(OsString::from).collect());
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
            let parsed = parser::parse_cli(argv.iter().map(OsString::from).collect());
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
        let show_args = cli_args(&[
            "journalctl",
            "--file=/tmp/x.journal",
            "--lines=+1",
            "--reverse",
        ]);
        assert!(
            enforce_oldest_lines_conflict(&show_args).is_err(),
            "show action must reject --lines=+N with --reverse"
        );

        let list_boots_args = cli_args(&[
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
        let local_space = timestamp::Parser::default()
            .parse("2023-11-15 00:00")
            .expect("space local");
        let local_t = timestamp::Parser::default()
            .parse("2023-11-15T00:00")
            .expect("T local");
        assert_eq!(local_t, local_space);

        timestamp::Parser::default()
            .parse("2023-11-15T00:00:00Z")
            .expect("UTC RFC3339");
        timestamp::Parser::default()
            .parse("2023-11-15T00:00:00.000001Z")
            .expect("UTC RFC3339 fraction");
        timestamp::Parser::default()
            .parse("2023-11-15T00:00:00+02:00")
            .expect("offset RFC3339");
        timestamp::Parser::default()
            .parse("2023-11-15T00:00+02:00")
            .expect("offset without seconds");
        assert!(
            timestamp::Parser::default()
                .parse("2023-11-15T00:00:00+0200")
                .is_err(),
            "stock rejects compact timezone offsets"
        );
    }

    fn cli_args(argv: &[&str]) -> Args {
        parser::parse_cli(argv.iter().map(OsString::from).collect())
            .expect("parse")
            .expect("nonterminal")
    }

    #[test]
    fn ordered_optional_and_required_values() {
        let args = cli_args(&["journalctl", "--lines", "TEST_ID=journalctl-query"]);
        assert_eq!(args.lines.as_deref(), Some("10"));
        assert_eq!(args.matches, vec!["TEST_ID=journalctl-query"]);
        let args = cli_args(&["journalctl", "--lines", "+2", "TEST_ID=journalctl-query"]);
        assert_eq!(args.lines.as_deref(), Some("+2"));
        let args = cli_args(&["journalctl", "--grep", "--help", "-rn2"]);
        assert_eq!(args.grep.as_deref(), Some(OsStr::new("--help")));
        assert_eq!(args.lines.as_deref(), Some("2"));
        assert!(args.reverse);
        assert!(
            parser::parse_cli(
                ["journalctl", "--since=bad", "--help"]
                    .map(OsString::from)
                    .to_vec()
            )
            .is_err()
        );
    }

    #[test]
    fn rotate_with_vacuum_is_distinct_unsupported_action() {
        let args = cli_args(&["journalctl", "--rotate", "--vacuum-files=2"]);
        let err =
            enforce_portable_unsupported(&args).expect_err("rotate+vacuum should be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("portable mode does not support --rotate with --vacuum-*"),
            "expected rotate-and-vacuum portable message, got: {msg}"
        );
    }

    #[test]
    fn boot_all_merge_is_not_rejected_as_conflict() {
        let args = cli_args(&[
            "journalctl",
            "--file=/tmp/x.journal",
            "--boot=all",
            "--merge",
        ]);
        enforce_boot_merge_conflict(&args).expect("boot=all must not conflict with --merge");
    }
}

#[derive(Debug)]
struct SilentNoData;
impl std::fmt::Display for SilentNoData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no matching entries")
    }
}
impl std::error::Error for SilentNoData {}

fn no_data_error(quiet: bool, message: &str) -> anyhow::Error {
    if quiet {
        SilentNoData.into()
    } else {
        anyhow!("{message}")
    }
}

fn enforce_field_enumeration_scope(args: &Args) -> Result<()> {
    if !args.fields && args.field.is_none() {
        return Ok(());
    }
    let boot = args.this_boot || args.boot.as_deref().is_some_and(|b| b != "all");
    let invocation =
        args.invocation_latest || args.invocation.as_deref().is_some_and(|b| b != "all");
    if boot
        || invocation
        || args.dmesg
        || !args.unit.is_empty()
        || !args.user_unit.is_empty()
        || !args.identifier.is_empty()
        || !args.exclude_identifier.is_empty()
        || !args.priority.is_empty()
        || !args.facility.is_empty()
        || args.since.is_some()
        || args.until.is_some()
        || args.cursor.is_some()
        || args.after_cursor.is_some()
        || args.cursor_file.is_some()
        || args.grep.is_some()
    {
        return Err(anyhow!(
            "-F/--field= and -N/--fields cannot be combined with options that limit the journal."
        ));
    }
    Ok(())
}

fn is_json_output(mode: OutputModeArg) -> bool {
    matches!(
        mode,
        OutputModeArg::Json
            | OutputModeArg::JsonPretty
            | OutputModeArg::JsonSse
            | OutputModeArg::JsonSeq
    )
}

fn table_timestamp(value: u64) -> Option<u64> {
    (value != u64::MAX).then_some(value)
}

fn print_json_table(mode: OutputModeArg, rows: &[serde_json::Value]) -> Result<()> {
    let mut text = if matches!(mode, OutputModeArg::JsonPretty) {
        serde_json::to_string_pretty(rows)?
    } else {
        serde_json::to_string(rows)?
    };
    if colors::Colors::detect().enabled() {
        text = String::from_utf8(colors::json(text.as_bytes()))?;
    }
    match mode {
        OutputModeArg::JsonSse => println!("data: {text}\n"),
        OutputModeArg::JsonSeq => println!("\x1e{text}"),
        _ => println!("{text}"),
    }
    Ok(())
}
