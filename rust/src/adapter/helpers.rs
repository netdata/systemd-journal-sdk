use super::*;

impl AdapterResult {
    pub(super) fn with_evidence(mut self, evidence: serde_json::Value) -> Self {
        self.evidence = Some(evidence);
        self
    }
}

pub(super) fn expected_error_matches(actual: &serde_json::Value, tc: &TestCase) -> bool {
    let Some(expected) = &tc.expected.error_contains else {
        return true;
    };
    actual.as_str().is_some_and(|actual| {
        actual
            .to_ascii_lowercase()
            .contains(&expected.to_ascii_lowercase())
    })
}

pub(super) fn entries_match(entries: &[HashMap<String, String>], tc: &TestCase) -> bool {
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

pub(super) fn fields_present_in_entries(
    entries: &[HashMap<String, String>],
    tc: &TestCase,
) -> bool {
    tc.expected
        .fields_present
        .iter()
        .all(|field| entries.iter().any(|entry| entry.contains_key(field)))
}

pub(super) fn fields_present_in_strings(values: &[String], tc: &TestCase) -> bool {
    tc.expected
        .fields_present
        .iter()
        .all(|field| values.contains(field))
}

pub(super) fn fields_present_in_values(values: &[serde_json::Value], tc: &TestCase) -> bool {
    tc.expected.fields_present.iter().all(|field| {
        values.iter().all(|value| {
            value
                .as_object()
                .is_some_and(|object| object.contains_key(field))
        })
    })
}

pub(super) fn json_entries_match(values: &[serde_json::Value], tc: &TestCase) -> bool {
    let Some(serde_json::Value::Array(expected)) = &tc.expected.entries_match else {
        return true;
    };
    for expected_entry in expected {
        let Some(expected_object) = expected_entry.as_object() else {
            continue;
        };
        let matched = values.iter().any(|value| {
            value.as_object().is_some_and(|actual_object| {
                expected_object
                    .iter()
                    .all(|(key, expected_value)| actual_object.get(key) == Some(expected_value))
            })
        });
        if !matched {
            return false;
        }
    }
    true
}

pub(super) fn boot_indices_match(actual: &[serde_json::Value], tc: &TestCase) -> bool {
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

pub(super) fn export_fields_present(exports: &[String], tc: &TestCase) -> bool {
    let Some(export) = exports.first() else {
        return tc.expected.fields_present.is_empty();
    };
    tc.expected.fields_present.iter().all(|field| {
        export
            .lines()
            .any(|line| line == field || line.starts_with(&format!("{field}=")))
    })
}

pub(super) fn read_some_entries(
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

pub(super) fn parse_export_text(data: &str) -> Vec<HashMap<String, String>> {
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

pub(super) fn boots_to_json(boots: Vec<journal::BootInfo>) -> Vec<serde_json::Value> {
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

pub(super) fn fixture_path(tc: &TestCase, key: &str) -> Option<PathBuf> {
    let fixture = tc.fixtures.get(key)?;
    if fixture.fixture_type != "file" && fixture.fixture_type != "directory" {
        return None;
    }
    Some(repo_root().join(&fixture.path))
}

pub(super) fn repo_root() -> PathBuf {
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
