use anyhow::{Result, anyhow};
use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use clap::{Parser, ValueEnum};
use journal::{
    Entry, FacadeError, FileReader, OutputMode, SdJournal, SdJournalAddConjunction,
    SdJournalAddDisjunction, SdJournalAddMatch, SdJournalEnumerateFields, SdJournalGetEntry,
    SdJournalListBoots, SdJournalNext, SdJournalOpen, SdJournalProcessOutput, SdJournalSeekHead,
    SdJournalSeekRealtimeUsec, SdJournalSetOutputMode, parse_match_string, verify_file,
    verify_file_with_key,
};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::thread;
use std::time::Duration;

// HEADER_COMPATIBLE_SEALED from systemd journal-def.h
const COMPATIBLE_SEALED: u32 = 1;

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
    let args = Args::parse_from(preprocess_optional_boot_args(std::env::args()));
    if args.sync || args.flush || args.rotate || args.relinquish_var {
        return Err(anyhow!("{}", FacadeError::Unsupported));
    }

    let path = args
        .file
        .as_ref()
        .or(args.directory.as_ref())
        .ok_or_else(|| anyhow!("use --file or --directory"))?;

    if args.verify || args.verify_only || args.verify_key.is_some() {
        return run_verify(path, args.verify_key.as_deref());
    }

    let since_usec = parse_optional_timestamp(args.since.as_deref())?;
    let until_usec = parse_optional_timestamp(args.until.as_deref())?;
    if let (Some(since), Some(until)) = (since_usec, until_usec) {
        if since > until {
            return Err(anyhow!("--since= must be before --until=."));
        }
    }

    if args.follow {
        let tail = args.tail.unwrap_or(10);
        return run_follow(path, &args, since_usec, until_usec, tail);
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

    if let Some(n) = args.tail {
        show_tail(&mut journal, n, since_usec, until_usec)
    } else {
        show_head_or_all(&mut journal, args.head, since_usec, until_usec)
    }
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

fn open_filtered_journal(path: &Path, args: &Args) -> Result<SdJournal> {
    let mut journal =
        SdJournalOpen(&path.to_string_lossy(), 0).map_err(|err| anyhow!("open: {err}"))?;
    if let Some(boot) = args.boot.as_deref() {
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

fn show_head_or_all(
    journal: &mut SdJournal,
    limit: Option<usize>,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
) -> Result<()> {
    let mut shown = 0usize;
    for entry in matching_entries(journal, since_usec, until_usec)? {
        if limit.is_some_and(|limit| shown >= limit) {
            break;
        }
        write_entry(journal, &entry)?;
        shown += 1;
    }
    Ok(())
}

fn show_tail(
    journal: &mut SdJournal,
    limit: usize,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
) -> Result<()> {
    let entries = matching_entries(journal, since_usec, until_usec)?;
    let outputs = entries
        .iter()
        .map(|entry| SdJournalProcessOutput(journal, entry).map_err(|err| anyhow!("output: {err}")))
        .collect::<Result<Vec<_>>>()?;
    let start = outputs.len().saturating_sub(limit);
    let mut stdout = std::io::stdout().lock();
    for entry in &outputs[start..] {
        stdout.write_all(entry)?;
    }
    Ok(())
}

fn write_entry(journal: &mut SdJournal, entry: &Entry) -> Result<()> {
    let output = SdJournalProcessOutput(journal, entry).map_err(|err| anyhow!("output: {err}"))?;
    std::io::stdout().lock().write_all(&output)?;
    Ok(())
}

fn matching_entries(
    journal: &mut SdJournal,
    since_usec: Option<u64>,
    until_usec: Option<u64>,
) -> Result<Vec<Entry>> {
    if let Some(since) = since_usec {
        SdJournalSeekRealtimeUsec(journal, since).map_err(|err| anyhow!("seek realtime: {err}"))?;
    } else {
        SdJournalSeekHead(journal).map_err(|err| anyhow!("seek head: {err}"))?;
    }
    let mut out = Vec::new();
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
                {
                    out.push(entry);
                }
            }
        }
    }
    Ok(out)
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
) -> Vec<(String, Vec<u8>)> {
    let Ok(mut journal) = open_filtered_journal(path, args) else {
        return Vec::new();
    };
    let Ok(entries) = matching_entries(&mut journal, since_usec, until_usec) else {
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
) -> Result<()> {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    let initial = scan_follow_snapshot(path, args, since_usec, until_usec);
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
        let snapshot = scan_follow_snapshot(path, args, since_usec, until_usec);
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
}
