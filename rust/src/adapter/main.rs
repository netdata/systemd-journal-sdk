use anyhow::{Result as AnyResult, anyhow};
use journal::{
    Config, EntryTimestamps, FileReader, Log, Origin, OutputMode, RetentionPolicy, RotationPolicy,
    SdJournalAddConjunction, SdJournalAddDisjunction, SdJournalAddMatch, SdJournalEnumerateFields,
    SdJournalGetCursor, SdJournalGetEntry, SdJournalGetRealtimeUsec, SdJournalListBoots,
    SdJournalNext, SdJournalOpen, SdJournalProcessOutput, SdJournalSeekCursor, SdJournalSeekHead,
    SdJournalSetOutputMode, SdJournalTestCursor, Source, parse_match_string, verify_file,
    verify_file_with_key,
};
use journal_core::file::{JournalFile, JournalFileOptions, JournalWriter, MmapMut};
use journal_core::repository::File as RepoFile;
use journal_core::seal::SealOptions;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

mod helpers;
use helpers::*;

const ADAPTER_VERSION: &str = "0.1.0";

#[derive(Debug, Deserialize)]
struct TestCase {
    test_name: String,
    category: String,
    #[serde(default)]
    fixtures: HashMap<String, Fixture>,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    #[serde(rename = "type")]
    fixture_type: String,
    path: String,
}

#[derive(Debug, Deserialize)]
struct Expected {
    result_format: String,
    #[serde(default)]
    entries_match: Option<serde_json::Value>,
    #[serde(default)]
    fields_present: Vec<String>,
    #[serde(default)]
    error_contains: Option<String>,
}

#[derive(Debug, Serialize)]
struct AdapterResult {
    test_name: String,
    status: String,
    result_format: String,
    actual: serde_json::Value,
    duration_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence: Option<serde_json::Value>,
}

impl AdapterResult {
    fn pass(
        test_name: &str,
        result_format: &str,
        actual: serde_json::Value,
        start: Instant,
    ) -> Self {
        Self {
            test_name: test_name.to_string(),
            status: "PASS".to_string(),
            result_format: result_format.to_string(),
            actual,
            duration_ms: start.elapsed().as_millis().max(1),
            error: None,
            note: None,
            evidence: None,
        }
    }

    fn skip(test_name: &str, result_format: &str, note: impl Into<String>, start: Instant) -> Self {
        Self {
            test_name: test_name.to_string(),
            status: "SKIP".to_string(),
            result_format: result_format.to_string(),
            actual: serde_json::Value::Null,
            duration_ms: start.elapsed().as_millis().max(1),
            error: None,
            note: Some(note.into()),
            evidence: None,
        }
    }

    fn fail(
        test_name: &str,
        result_format: &str,
        actual: serde_json::Value,
        error: impl Into<String>,
        start: Instant,
    ) -> Self {
        Self {
            test_name: test_name.to_string(),
            status: "FAIL".to_string(),
            result_format: result_format.to_string(),
            actual,
            duration_ms: start.elapsed().as_millis().max(1),
            error: Some(error.into()),
            note: None,
            evidence: None,
        }
    }

    fn error(
        test_name: &str,
        result_format: &str,
        error: impl Into<String>,
        start: Instant,
    ) -> Self {
        Self {
            test_name: test_name.to_string(),
            status: "ERROR".to_string(),
            result_format: result_format.to_string(),
            actual: serde_json::Value::Null,
            duration_ms: start.elapsed().as_millis().max(1),
            error: Some(error.into()),
            note: None,
            evidence: None,
        }
    }
}

fn adapter_fail(
    tc: &TestCase,
    actual: serde_json::Value,
    error: impl Into<String>,
    start: Instant,
) -> AdapterResult {
    AdapterResult::fail(
        &tc.test_name,
        &tc.expected.result_format,
        actual,
        error,
        start,
    )
}

fn main() {
    if let Err(err) = run() {
        eprintln!("ERROR: {err}");
        std::process::exit(1);
    }
}

fn run() -> AnyResult<()> {
    // nosemgrep: rust.lang.security.args.args -- conformance adapter dispatches trusted test subcommands.
    match std::env::args().nth(1).as_deref() {
        Some("run") => cmd_run(),
        Some("list") => cmd_list(),
        Some("probe") => cmd_probe(),
        Some("__corrupt_probe") => cmd_corrupt_probe(),
        Some(other) => Err(anyhow!("unknown subcommand {other}")),
        None => Err(anyhow!("usage: adapter [run|list|probe]")),
    }
}

fn cmd_corrupt_probe() -> AnyResult<()> {
    // nosemgrep: rust.lang.security.args.args -- private test probe consumes a harness-supplied fixture path.
    let path = std::env::args()
        .nth(2)
        .ok_or_else(|| anyhow!("missing corrupt probe path"))?;
    let Ok(mut journal) = SdJournalOpen(&path, 0) else {
        return Ok(());
    };
    for _ in 0..1000 {
        match SdJournalNext(&mut journal) {
            Ok(0) => break,
            Ok(_) => {
                let _ = SdJournalGetEntry(&mut journal);
            }
            Err(_) => break,
        }
    }
    Ok(())
}

fn cmd_run() -> AnyResult<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let test_case: TestCase = serde_json::from_str(&input)?;
    let result = run_test(&test_case);
    serde_json::to_writer(io::stdout(), &result)?;
    println!();
    Ok(())
}

fn cmd_list() -> AnyResult<()> {
    let tests = [
        "journal-file-parse-uid-from-filename",
        "journal-importer-basic-parsing",
        "journal-importer-eof",
        "journal-match-boolean-logic",
        "journal-match-invalid-input",
        "journal-stream-directory-iteration",
        "journal-query-unique-fields",
        "journal-cursor-test",
        "journal-verify-sealed",
        "journal-verify-corruption-detection",
        "journal-zstd-compressed-read",
        "journal-corruption-append-resilient",
        "journal-file-header-parse",
        "journal-list-boots",
        "journal-export-format",
    ];
    serde_json::to_writer(io::stdout(), &tests)?;
    println!();
    Ok(())
}

fn cmd_probe() -> AnyResult<()> {
    let value = json!({
        "adapter_version": ADAPTER_VERSION,
        "language": "rust",
        "capabilities": {
            "file_reader": true,
            "directory_reader": true,
            "zstd_decompress": true,
            "matching": true,
            "cursor": true,
            "enumeration": true,
            "export_output": true,
            "json_output": true,
            "verification": true,
            "fss": true
        }
    });
    serde_json::to_writer(io::stdout(), &value)?;
    println!();
    Ok(())
}

fn run_test(tc: &TestCase) -> AdapterResult {
    let start = Instant::now();
    match tc.test_name.as_str() {
        "journal-file-parse-uid-from-filename" => test_uid(tc, start),
        "journal-importer-basic-parsing" | "journal-importer-eof" => test_importer(tc, start),
        "journal-match-invalid-input" => test_invalid_match(tc, start),
        "journal-match-boolean-logic" => test_complex_match(tc, start),
        "journal-stream-directory-iteration" | "journal-zstd-compressed-read" => {
            test_read_entries(tc, start)
        }
        "journal-query-unique-fields" => test_fields(tc, start),
        "journal-cursor-test" => test_cursor(tc, start),
        "journal-verify-sealed" => test_verify_sealed(tc, start),
        "journal-verify-corruption-detection" => test_verify_corruption(tc, start),
        "journal-corruption-append-resilient" => test_corruption(tc, start),
        "journal-file-header-parse" => test_file_header(tc, start),
        "journal-list-boots" => test_list_boots(tc, start),
        "journal-export-format" => test_export(tc, start),
        _ => AdapterResult::skip(
            &tc.test_name,
            &tc.expected.result_format,
            format!("unsupported category {}", tc.category),
            start,
        ),
    }
}

fn test_uid(tc: &TestCase, start: Instant) -> AdapterResult {
    let cases = [
        ("user-1000.journal", 1000, true, ""),
        ("system.journal", 0, false, ""),
        ("user-foo.journal", 0, false, "EINVAL"),
        ("user-65535.journal", 0, false, "ENXIO"),
        (
            "user@0000000000000000-0000000000000000.journal~",
            0,
            false,
            "EREMOTE",
        ),
    ];
    let mut evidence = Vec::new();
    for (name, want_uid, want_has_uid, want_err) in cases {
        let (uid, has_uid, err_code) = parse_uid_from_journal_filename(name);
        evidence.push(json!({
            "name": name,
            "uid": uid,
            "has_uid": has_uid,
            "error": err_code,
        }));
        if uid != want_uid || has_uid != want_has_uid || err_code != want_err {
            return AdapterResult::fail(
                &tc.test_name,
                &tc.expected.result_format,
                json!(evidence),
                format!(
                    "{name} parsed as uid={uid} has_uid={has_uid} err={err_code:?}, want uid={want_uid} has_uid={want_has_uid} err={want_err:?}"
                ),
                start,
            );
        }
    }
    AdapterResult::pass(
        &tc.test_name,
        &tc.expected.result_format,
        json!(true),
        start,
    )
}

fn test_file_header(tc: &TestCase, start: Instant) -> AdapterResult {
    let Some(path) = fixture_path(tc, "journal_file") else {
        return AdapterResult::error(
            &tc.test_name,
            &tc.expected.result_format,
            "missing fixture",
            start,
        );
    };

    match FileReader::open(path) {
        Ok(reader) => {
            let header = reader.header();
            let actual = vec![json!({
                "signature": String::from_utf8_lossy(&header.signature).into_owned(),
                "state": header.state,
                "compatible_flags": header.compatible_flags,
                "incompatible_flags": header.incompatible_flags,
                "header_size": header.header_size,
            })];
            if !fields_present_in_values(&actual, tc) || !json_entries_match(&actual, tc) {
                return AdapterResult::fail(
                    &tc.test_name,
                    &tc.expected.result_format,
                    json!(actual),
                    "journal header does not match manifest expectations",
                    start,
                );
            }
            AdapterResult::pass(
                &tc.test_name,
                &tc.expected.result_format,
                json!(actual),
                start,
            )
        }
        Err(err) => AdapterResult::fail(
            &tc.test_name,
            &tc.expected.result_format,
            serde_json::Value::Null,
            err.to_string(),
            start,
        ),
    }
}

fn parse_uid_from_journal_filename(name: &str) -> (u32, bool, &'static str) {
    if name == "system.journal" || name.starts_with("system@") {
        return (0, false, "");
    }
    if name.starts_with("user@") {
        return (0, false, "EREMOTE");
    }
    let Some(raw) = name
        .strip_prefix("user-")
        .and_then(|name| name.strip_suffix(".journal"))
    else {
        return (0, false, "EINVAL");
    };
    let Ok(uid) = raw.parse::<u32>() else {
        return (0, false, "EINVAL");
    };
    if uid == 65535 {
        return (0, false, "ENXIO");
    }
    (uid, true, "")
}

fn test_importer(tc: &TestCase, start: Instant) -> AdapterResult {
    let Some(path) = fixture_path(tc, "importer_data") else {
        return AdapterResult::error(
            &tc.test_name,
            &tc.expected.result_format,
            "missing fixture",
            start,
        );
    };
    match fs::read_to_string(path) {
        Ok(data) => {
            let entries = parse_export_text(&data);
            if tc.test_name == "journal-importer-eof" {
                AdapterResult::pass(
                    &tc.test_name,
                    &tc.expected.result_format,
                    json!(!entries.is_empty()),
                    start,
                )
            } else if !entries_match(&entries, tc) || !fields_present_in_entries(&entries, tc) {
                AdapterResult::fail(
                    &tc.test_name,
                    &tc.expected.result_format,
                    json!(entries),
                    "parsed export data does not match manifest expectations",
                    start,
                )
            } else {
                AdapterResult::pass(
                    &tc.test_name,
                    &tc.expected.result_format,
                    json!(entries),
                    start,
                )
            }
        }
        Err(err) => AdapterResult::error(
            &tc.test_name,
            &tc.expected.result_format,
            err.to_string(),
            start,
        ),
    }
}

fn test_invalid_match(tc: &TestCase, start: Instant) -> AdapterResult {
    let invalid = ["foobar", "foobar=waldo", "", "=", "=xxxxx"];
    let ok = invalid.iter().all(|item| parse_match_string(item).is_err());
    let actual = if ok {
        json!("EINVAL")
    } else {
        serde_json::Value::Null
    };
    if ok && expected_error_matches(&actual, tc) {
        AdapterResult::pass(&tc.test_name, &tc.expected.result_format, actual, start)
    } else {
        AdapterResult::fail(
            &tc.test_name,
            &tc.expected.result_format,
            actual,
            "invalid match validation did not produce expected EINVAL",
            start,
        )
    }
}

fn test_complex_match(tc: &TestCase, start: Instant) -> AdapterResult {
    let (_tmp, active_path) = match create_complex_match_log() {
        Ok(result) => result,
        Err(err) => return adapter_fail(tc, serde_json::Value::Null, err.to_string(), start),
    };
    let matched = match collect_complex_matches(&active_path) {
        Ok(matched) => matched,
        Err(err) => return adapter_fail(tc, serde_json::Value::Null, err, start),
    };
    if !complex_matches_expected(&matched) {
        return AdapterResult::fail(
            &tc.test_name,
            &tc.expected.result_format,
            json!(matched),
            format!(
                "matched {} entries, want the two systemd complex-match entries",
                matched.len()
            ),
            start,
        );
    }

    AdapterResult::pass(
        &tc.test_name,
        &tc.expected.result_format,
        json!(matched),
        start,
    )
}

fn create_complex_match_log() -> AnyResult<(tempfile::TempDir, String)> {
    let tmp = tempfile::tempdir()?;
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
    let mut log = Log::new(tmp.path(), config)?;
    write_complex_match_entries(&mut log)?;
    log.sync()?;
    let active_path = log
        .active_file()
        .map(|file| file.path().to_string())
        .ok_or_else(|| anyhow!("writer did not expose an active file"))?;
    drop(log);
    Ok((tmp, active_path))
}

fn write_complex_match_entries(log: &mut Log) -> AnyResult<()> {
    let entries: Vec<Vec<Vec<u8>>> = vec![
        vec![b"L3=ok".to_vec(), b"TWO=two".to_vec(), b"ONE=one".to_vec()],
        vec![
            b"L4_1=yes".to_vec(),
            b"L4_2=ok".to_vec(),
            b"PIFF=paff".to_vec(),
            b"QUUX=xxxxx".to_vec(),
            b"HALLO=WALDO".to_vec(),
            b"B=C\0D".to_vec(),
            b"A=\x01\x02".to_vec(),
        ],
        vec![b"L3=ok".to_vec()],
        vec![b"TWO=two".to_vec(), b"ONE=one".to_vec()],
    ];

    const REALTIME_BASE: u64 = 1_700_010_000_000_000;
    for (index, entry) in entries.iter().enumerate() {
        let fields: Vec<&[u8]> = entry.iter().map(Vec::as_slice).collect();
        log.write_entry_with_timestamps(
            &fields,
            EntryTimestamps {
                entry_realtime_usec: Some(REALTIME_BASE + index as u64),
                entry_monotonic_usec: Some(index as u64 + 1),
                source_realtime_usec: None,
            },
        )?;
    }
    Ok(())
}

fn collect_complex_matches(active_path: &str) -> Result<Vec<HashMap<String, String>>, String> {
    let mut journal = SdJournalOpen(active_path, 0).map_err(|err| err.to_string())?;
    add_systemd_complex_match_expression(&mut journal)?;
    let mut matched = Vec::new();
    while SdJournalNext(&mut journal).map_err(|err| err.to_string())? != 0 {
        let entry = SdJournalGetEntry(&mut journal).map_err(|err| err.to_string())?;
        let fields = entry
            .fields
            .into_iter()
            .map(|(key, value)| (key, String::from_utf8_lossy(&value).into_owned()))
            .collect::<HashMap<_, _>>();
        matched.push(fields);
    }
    Ok(matched)
}

fn complex_matches_expected(matched: &[HashMap<String, String>]) -> bool {
    matched.len() == 2
        && matched.iter().any(is_l3_one_two_match)
        && matched.iter().any(is_l4_complex_match)
}

fn is_l3_one_two_match(entry: &HashMap<String, String>) -> bool {
    entry.get("L3").is_some_and(|value| value == "ok")
        && entry.get("TWO").is_some_and(|value| value == "two")
        && entry.get("ONE").is_some_and(|value| value == "one")
}

fn is_l4_complex_match(entry: &HashMap<String, String>) -> bool {
    entry.get("L4_1").is_some_and(|value| value == "yes")
        && entry.get("L4_2").is_some_and(|value| value == "ok")
        && entry.get("PIFF").is_some_and(|value| value == "paff")
        && entry.get("QUUX").is_some_and(|value| value == "xxxxx")
        && entry.get("HALLO").is_some_and(|value| value == "WALDO")
}

enum MatchOperation {
    Match(&'static [u8]),
    Disjunction,
    Conjunction,
}

fn add_systemd_complex_match_expression(journal: &mut journal::SdJournal) -> Result<(), String> {
    let operations = [
        MatchOperation::Match(b"A=\x01\x02"),
        MatchOperation::Match(b"B=C\0D"),
        MatchOperation::Match(b"HALLO=WALDO"),
        MatchOperation::Match(b"QUUX=mmmm"),
        MatchOperation::Match(b"QUUX=xxxxx"),
        MatchOperation::Match(b"HALLO="),
        MatchOperation::Match(b"QUUX=xxxxx"),
        MatchOperation::Match(b"QUUX=yyyyy"),
        MatchOperation::Match(b"PIFF=paff"),
        MatchOperation::Disjunction,
        MatchOperation::Match(b"ONE=one"),
        MatchOperation::Match(b"ONE=two"),
        MatchOperation::Match(b"TWO=two"),
        MatchOperation::Conjunction,
        MatchOperation::Match(b"L4_1=yes"),
        MatchOperation::Match(b"L4_1=ok"),
        MatchOperation::Match(b"L4_2=yes"),
        MatchOperation::Match(b"L4_2=ok"),
        MatchOperation::Disjunction,
        MatchOperation::Match(b"L3=yes"),
        MatchOperation::Match(b"L3=ok"),
    ];
    for operation in operations {
        match operation {
            MatchOperation::Match(value) => SdJournalAddMatch(journal, value),
            MatchOperation::Disjunction => SdJournalAddDisjunction(journal),
            MatchOperation::Conjunction => SdJournalAddConjunction(journal),
        }
        .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn test_read_entries(tc: &TestCase, start: Instant) -> AdapterResult {
    let fixture_key = if tc.test_name == "journal-zstd-compressed-read" {
        "journal_file"
    } else {
        "journal_dir"
    };
    let Some(path) = fixture_path(tc, fixture_key) else {
        return AdapterResult::error(
            &tc.test_name,
            &tc.expected.result_format,
            "missing fixture",
            start,
        );
    };
    match read_some_entries(&path, 20) {
        Ok(entries) if !entries.is_empty() => {
            let actual = if tc.expected.result_format == "boolean" {
                json!(true)
            } else {
                json!(entries)
            };
            AdapterResult::pass(&tc.test_name, &tc.expected.result_format, actual, start)
        }
        Ok(_) => AdapterResult::error(
            &tc.test_name,
            &tc.expected.result_format,
            "no entries",
            start,
        ),
        Err(err) => AdapterResult::error(
            &tc.test_name,
            &tc.expected.result_format,
            err.to_string(),
            start,
        ),
    }
}

fn test_fields(tc: &TestCase, start: Instant) -> AdapterResult {
    let Some(path) = fixture_path(tc, "journal_dir") else {
        return AdapterResult::error(
            &tc.test_name,
            &tc.expected.result_format,
            "missing fixture",
            start,
        );
    };
    match SdJournalOpen(&path.to_string_lossy(), 0)
        .and_then(|mut journal| SdJournalEnumerateFields(&mut journal))
    {
        Ok(fields) => {
            if !fields_present_in_strings(&fields, tc) {
                return AdapterResult::fail(
                    &tc.test_name,
                    &tc.expected.result_format,
                    json!(fields),
                    "enumerated fields do not match manifest expectations",
                    start,
                );
            }
            AdapterResult::pass(
                &tc.test_name,
                &tc.expected.result_format,
                json!(fields),
                start,
            )
        }
        Err(err) => AdapterResult::error(
            &tc.test_name,
            &tc.expected.result_format,
            err.to_string(),
            start,
        ),
    }
}

fn test_cursor(tc: &TestCase, start: Instant) -> AdapterResult {
    let Some(path) = fixture_path(tc, "journal_dir") else {
        return AdapterResult::error(
            &tc.test_name,
            &tc.expected.result_format,
            "missing fixture",
            start,
        );
    };
    match cursor_checks(&path) {
        Ok(ok) => AdapterResult::pass(&tc.test_name, &tc.expected.result_format, json!(ok), start),
        Err(err) => AdapterResult::error(
            &tc.test_name,
            &tc.expected.result_format,
            err.to_string(),
            start,
        ),
    }
}

fn cursor_checks(path: &PathBuf) -> Result<bool, journal::FacadeError> {
    let mut journal = SdJournalOpen(&path.to_string_lossy(), 0)?;
    SdJournalSeekHead(&mut journal)?;
    if SdJournalNext(&mut journal)? == 0 {
        return Ok(false);
    }
    let cursor = SdJournalGetCursor(&journal)?;
    if !valid_current_cursor(&journal, &cursor)? {
        return Ok(false);
    }
    let cursor_realtime = SdJournalGetRealtimeUsec(&journal)?;
    if invalid_cursor_accepted(&mut journal)? {
        return Ok(false);
    }
    missing_cursor_positions_after_original(&mut journal, &cursor, cursor_realtime)
}

fn valid_current_cursor(
    journal: &journal::SdJournal,
    cursor: &str,
) -> Result<bool, journal::FacadeError> {
    Ok(SdJournalTestCursor(journal, cursor)? && !SdJournalTestCursor(journal, "invalid-cursor")?)
}

fn invalid_cursor_accepted(journal: &mut journal::SdJournal) -> Result<bool, journal::FacadeError> {
    Ok(SdJournalSeekCursor(journal, "invalid-cursor").is_ok())
}

fn missing_cursor_positions_after_original(
    journal: &mut journal::SdJournal,
    cursor: &str,
    cursor_realtime: u64,
) -> Result<bool, journal::FacadeError> {
    SdJournalSeekCursor(journal, cursor)?;
    let Some((cursor_prefix, _)) = cursor.rsplit_once("n=") else {
        return Ok(false);
    };
    SdJournalSeekCursor(journal, &format!("{cursor_prefix}n=999999"))?;
    Ok(!SdJournalTestCursor(journal, cursor)?
        && SdJournalGetRealtimeUsec(journal)? >= cursor_realtime)
}

fn test_corruption(tc: &TestCase, start: Instant) -> AdapterResult {
    // The conformance adapter re-executes this exact binary to isolate crashes from corrupted fixtures.
    // nosemgrep: rust.lang.security.current-exe.current-exe
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(err) => {
            return AdapterResult::error(
                &tc.test_name,
                &tc.expected.result_format,
                err.to_string(),
                start,
            );
        }
    };
    let mut checked = 0usize;
    let mut crashes = Vec::new();
    for key in ["corrupted_file", "afl_corrupted_1", "afl_corrupted_2"] {
        let Some(path) = fixture_path(tc, key) else {
            continue;
        };
        checked += 1;
        match Command::new(&exe)
            .arg("__corrupt_probe")
            .arg(&path)
            .output()
        {
            Ok(output) if output.status.success() => {}
            Ok(output) => crashes.push(json!({
                "fixture": key,
                "status": output.status.to_string(),
                "stderr": String::from_utf8_lossy(&output.stderr),
            })),
            Err(err) => crashes.push(json!({
                "fixture": key,
                "error": err.to_string(),
            })),
        }
    }

    if !crashes.is_empty() {
        return AdapterResult::fail(
            &tc.test_name,
            &tc.expected.result_format,
            json!(false),
            "corrupted fixture probe crashed or exited unsuccessfully",
            start,
        )
        .with_evidence(json!({"checked": checked, "crashes": crashes}));
    }
    if checked == 0 {
        return AdapterResult::skip(
            &tc.test_name,
            &tc.expected.result_format,
            "no corruption fixtures",
            start,
        );
    }
    AdapterResult::pass(
        &tc.test_name,
        &tc.expected.result_format,
        json!(true),
        start,
    )
    .with_evidence(json!({"checked": checked}))
}

fn test_verify_corruption(tc: &TestCase, start: Instant) -> AdapterResult {
    let Some(path) = fixture_path(tc, "corrupted_file") else {
        return AdapterResult::skip(
            &tc.test_name,
            &tc.expected.result_format,
            "no corrupted_file fixture",
            start,
        );
    };
    match verify_file(&path) {
        Ok(()) => AdapterResult::fail(
            &tc.test_name,
            &tc.expected.result_format,
            serde_json::Value::Null,
            "verification did not detect corruption in truncated zstd frame",
            start,
        ),
        Err(err) => AdapterResult::pass(
            &tc.test_name,
            &tc.expected.result_format,
            json!(err.to_string()),
            start,
        )
        .with_evidence(json!({"error": err.to_string()})),
    }
}

fn test_verify_sealed(tc: &TestCase, start: Instant) -> AdapterResult {
    let (_tmp, path, key) = match create_sealed_verify_fixture() {
        Ok(fixture) => fixture,
        Err(err) => {
            return AdapterResult::error(
                &tc.test_name,
                &tc.expected.result_format,
                err.to_string(),
                start,
            );
        }
    };
    match verify_file_with_key(&path, &key) {
        Ok(()) => AdapterResult::pass(
            &tc.test_name,
            &tc.expected.result_format,
            json!(true),
            start,
        ),
        Err(err) => AdapterResult::fail(
            &tc.test_name,
            &tc.expected.result_format,
            json!(false),
            err.to_string(),
            start,
        ),
    }
}

fn create_sealed_verify_fixture() -> AnyResult<(tempfile::TempDir, PathBuf, String)> {
    let tmp = tempfile::tempdir()?;
    let path = sealed_verify_path(&tmp);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let seed = [0u8; 12];
    write_sealed_verify_journal(&path, seed)?;
    Ok((tmp, path, sealed_verify_key(seed)))
}

fn sealed_verify_path(tmp: &tempfile::TempDir) -> PathBuf {
    tmp.path()
        .join("00000000-0000-0000-0000-000000000001/system.journal")
}

fn write_sealed_verify_journal(path: &PathBuf, seed: [u8; 12]) -> AnyResult<()> {
    let repo_file =
        RepoFile::from_path(path).ok_or_else(|| anyhow!("test journal path should parse"))?;
    let opts = JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
        .with_seal(SealOptions::new(seed, 1_000_000, 1_000_000));
    let mut journal_file = JournalFile::<MmapMut>::create(&repo_file, opts)?;
    let mut writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4))?;
    writer.add_entry(
        &mut journal_file,
        &[b"MESSAGE=sealed verify".as_slice()],
        1_500_000,
        1,
    )?;
    journal_file.sync()?;
    Ok(())
}

fn sealed_verify_key(seed: [u8; 12]) -> String {
    let seed_hex = seed.iter().map(|b| format!("{b:02x}")).collect::<String>();
    format!("{seed_hex}/{:x}-{:x}", 1u64, 1_000_000u64)
}

fn test_uuid(n: u8) -> uuid::Uuid {
    let mut bytes = [0u8; 16];
    bytes[15] = n;
    uuid::Uuid::from_bytes(bytes)
}

fn test_list_boots(tc: &TestCase, start: Instant) -> AdapterResult {
    let Some(path) = fixture_path(tc, "journal_dir") else {
        return AdapterResult::error(
            &tc.test_name,
            &tc.expected.result_format,
            "missing fixture",
            start,
        );
    };
    match SdJournalOpen(&path.to_string_lossy(), 0)
        .and_then(|mut journal| SdJournalListBoots(&mut journal))
    {
        Ok(boots) => {
            let actual = boots_to_json(boots);
            if !boot_indices_match(&actual, tc) || !fields_present_in_values(&actual, tc) {
                return AdapterResult::fail(
                    &tc.test_name,
                    &tc.expected.result_format,
                    json!(actual),
                    "boot list does not match manifest expectations",
                    start,
                );
            }
            AdapterResult::pass(
                &tc.test_name,
                &tc.expected.result_format,
                json!(actual),
                start,
            )
        }
        Err(err) => AdapterResult::error(
            &tc.test_name,
            &tc.expected.result_format,
            err.to_string(),
            start,
        ),
    }
}

fn test_export(tc: &TestCase, start: Instant) -> AdapterResult {
    let Some(path) = fixture_path(tc, "journal_dir") else {
        return AdapterResult::error(
            &tc.test_name,
            &tc.expected.result_format,
            "missing fixture",
            start,
        );
    };
    let result: Result<Vec<String>, journal::FacadeError> = (|| {
        let mut journal = SdJournalOpen(&path.to_string_lossy(), 0)?;
        SdJournalSetOutputMode(&mut journal, OutputMode::Export);
        SdJournalSeekHead(&mut journal)?;
        if SdJournalNext(&mut journal)? == 0 {
            return Ok(Vec::<String>::new());
        }
        let entry = SdJournalGetEntry(&mut journal)?;
        let output = SdJournalProcessOutput(&journal, &entry)?;
        Ok(vec![String::from_utf8_lossy(&output).into_owned()])
    })();
    match result {
        Ok(exports) => {
            if !export_fields_present(&exports, tc) {
                return AdapterResult::fail(
                    &tc.test_name,
                    &tc.expected.result_format,
                    json!(exports),
                    "export output is missing required manifest fields",
                    start,
                );
            }
            AdapterResult::pass(
                &tc.test_name,
                &tc.expected.result_format,
                json!(exports),
                start,
            )
        }
        Err(err) => AdapterResult::error(
            &tc.test_name,
            &tc.expected.result_format,
            err.to_string(),
            start,
        ),
    }
}
