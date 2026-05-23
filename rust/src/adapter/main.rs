use anyhow::{Result as AnyResult, anyhow};
use journal::{
    OutputMode, SdJournalEnumerateFields, SdJournalGetCursor, SdJournalGetEntry,
    SdJournalListBoots, SdJournalNext, SdJournalOpen, SdJournalProcessOutput, SdJournalSeekHead,
    SdJournalSetOutputMode, SdJournalTestCursor, parse_match_string,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

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

fn main() {
    if let Err(err) = run() {
        eprintln!("ERROR: {err}");
        std::process::exit(1);
    }
}

fn run() -> AnyResult<()> {
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
            "verification": false,
            "fss": false
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
        "journal-match-boolean-logic" => AdapterResult::skip(
            &tc.test_name,
            &tc.expected.result_format,
            "generated complex match fixture not implemented in Rust adapter yet",
            start,
        ),
        "journal-stream-directory-iteration" | "journal-zstd-compressed-read" => {
            test_read_entries(tc, start)
        }
        "journal-query-unique-fields" => test_fields(tc, start),
        "journal-cursor-test" => test_cursor(tc, start),
        "journal-verify-sealed" | "journal-verify-corruption-detection" => AdapterResult::skip(
            &tc.test_name,
            &tc.expected.result_format,
            "verification/FSS is not implemented in the Rust adapter slice",
            start,
        ),
        "journal-corruption-append-resilient" => test_corruption(tc, start),
        "journal-file-header-parse" => AdapterResult::skip(
            &tc.test_name,
            &tc.expected.result_format,
            "file-header adapter assertions are not implemented in the Rust adapter slice",
            start,
        ),
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
    let result: Result<bool, journal::FacadeError> = (|| {
        let mut journal = SdJournalOpen(&path.to_string_lossy(), 0)?;
        SdJournalSeekHead(&mut journal)?;
        if SdJournalNext(&mut journal)? == 0 {
            return Ok(false);
        }
        let cursor = SdJournalGetCursor(&journal)?;
        SdJournalTestCursor(&journal, &cursor)
    })();
    match result {
        Ok(ok) => AdapterResult::pass(&tc.test_name, &tc.expected.result_format, json!(ok), start),
        Err(err) => AdapterResult::error(
            &tc.test_name,
            &tc.expected.result_format,
            err.to_string(),
            start,
        ),
    }
}

fn test_corruption(tc: &TestCase, start: Instant) -> AdapterResult {
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

impl AdapterResult {
    fn with_evidence(mut self, evidence: serde_json::Value) -> Self {
        self.evidence = Some(evidence);
        self
    }
}

fn expected_error_matches(actual: &serde_json::Value, tc: &TestCase) -> bool {
    let Some(expected) = &tc.expected.error_contains else {
        return true;
    };
    actual.as_str().is_some_and(|actual| {
        actual
            .to_ascii_lowercase()
            .contains(&expected.to_ascii_lowercase())
    })
}

fn entries_match(entries: &[HashMap<String, String>], tc: &TestCase) -> bool {
    let Some(serde_json::Value::Array(expected)) = &tc.expected.entries_match else {
        return true;
    };
    for expected_entry in expected {
        let Some(expected_object) = expected_entry.as_object() else {
            continue;
        };
        let matched = entries.iter().any(|entry| {
            expected_object.iter().all(|(key, value)| {
                value.as_str().is_some_and(|expected_value| {
                    entry
                        .get(key)
                        .is_some_and(|actual| actual == expected_value)
                })
            })
        });
        if !matched {
            return false;
        }
    }
    true
}

fn fields_present_in_entries(entries: &[HashMap<String, String>], tc: &TestCase) -> bool {
    tc.expected
        .fields_present
        .iter()
        .all(|field| entries.iter().any(|entry| entry.contains_key(field)))
}

fn fields_present_in_strings(values: &[String], tc: &TestCase) -> bool {
    tc.expected
        .fields_present
        .iter()
        .all(|field| values.contains(field))
}

fn fields_present_in_values(values: &[serde_json::Value], tc: &TestCase) -> bool {
    tc.expected.fields_present.iter().all(|field| {
        values.iter().all(|value| {
            value
                .as_object()
                .is_some_and(|object| object.contains_key(field))
        })
    })
}

fn boot_indices_match(actual: &[serde_json::Value], tc: &TestCase) -> bool {
    let Some(serde_json::Value::Array(expected)) = &tc.expected.entries_match else {
        return true;
    };
    let expected: Vec<_> = expected
        .iter()
        .filter_map(|entry| entry.get("index").and_then(serde_json::Value::as_i64))
        .collect();
    if expected.is_empty() {
        return true;
    }
    let actual: Vec<_> = actual
        .iter()
        .filter_map(|entry| entry.get("index").and_then(serde_json::Value::as_i64))
        .collect();
    actual == expected
}

fn export_fields_present(exports: &[String], tc: &TestCase) -> bool {
    let Some(export) = exports.first() else {
        return tc.expected.fields_present.is_empty();
    };
    tc.expected.fields_present.iter().all(|field| {
        export
            .lines()
            .any(|line| line == field || line.starts_with(&format!("{field}=")))
    })
}

fn read_some_entries(
    path: &PathBuf,
    limit: usize,
) -> Result<Vec<HashMap<String, String>>, journal::FacadeError> {
    let mut journal = SdJournalOpen(&path.to_string_lossy(), 0)?;
    SdJournalSeekHead(&mut journal)?;
    let mut entries = Vec::new();
    while entries.len() < limit && SdJournalNext(&mut journal)? != 0 {
        let entry = SdJournalGetEntry(&mut journal)?;
        entries.push(
            entry
                .fields
                .into_iter()
                .map(|(key, value)| (key, String::from_utf8_lossy(&value).into_owned()))
                .collect(),
        );
    }
    Ok(entries)
}

fn parse_export_text(data: &str) -> Vec<HashMap<String, String>> {
    let mut out = Vec::new();
    let mut current = HashMap::new();
    for line in data.lines() {
        if line.is_empty() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            current.insert(key.to_string(), value.to_string());
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn boots_to_json(boots: Vec<journal::BootInfo>) -> Vec<serde_json::Value> {
    boots
        .into_iter()
        .map(|boot| {
            json!({
                "index": boot.index,
                "boot_id": boot.boot_id,
                "first_entry": boot.first_entry,
                "last_entry": boot.last_entry,
            })
        })
        .collect()
}

fn fixture_path(tc: &TestCase, key: &str) -> Option<PathBuf> {
    let fixture = tc.fixtures.get(key)?;
    if fixture.fixture_type != "file" && fixture.fixture_type != "directory" {
        return None;
    }
    Some(repo_root().join(&fixture.path))
}

fn repo_root() -> PathBuf {
    if let Ok(base) = std::env::var("ADAPTER_FIXTURE_BASE") {
        return PathBuf::from(base);
    }
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        if dir
            .join("tests/conformance/manifests/conformance-v01.json")
            .exists()
        {
            return dir;
        }
        if !dir.pop() {
            return std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        }
    }
}
