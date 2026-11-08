//! Subprocess tests use Cargo's integration-test binary path instead of a
//! workspace-target fallback.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run_cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_journalctl"))
        .args(args)
        .output()
        .expect("spawn Cargo-built journalctl binary")
}

#[test]
fn unrecognized_option_is_rejected() {
    let out = run_cli(&["--not-an-official-option"]);
    assert!(
        !out.status.success(),
        "expected non-zero exit for unknown option"
    );
    let combined =
        String::from_utf8_lossy(&out.stderr).into_owned() + &String::from_utf8_lossy(&out.stdout);
    assert!(
        combined.contains("unexpected argument") || combined.contains("unrecognized"),
        "expected unknown-option error, got: {combined}"
    );
}

#[test]
fn portable_unsupported_message_format() {
    // Every daemon-only action must fail with the portable-mode error class.
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
    // The false value is a no-op, not an unsupported daemon operation.
    let out = run_cli(&[
        "--file=/tmp/systemd-journal-sdk-nonexistent.journal",
        "--synchronize-on-exit=false",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
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
        stdout.contains("v262") && stdout.contains("CLI compatibility"),
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

/// `--verify --directory` checks exactly the files a directory read sees:
/// top-level journals plus one level of machine-id subdirectories.
#[test]
fn verify_directory_uses_systemd_layout() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../fixtures/systemd/test-data/no-rtc/system.journal.zst");
    let root = tempfile::tempdir().expect("create temp dir");
    let plain_id = "00112233445566778899aabbccddeeff";
    let dashed_id = "00112233-4455-6677-8899-aabbccddeeff";
    for dir in [
        plain_id,
        dashed_id,
        "not-an-id",
        &format!("{plain_id}/nested"),
    ] {
        std::fs::create_dir_all(root.path().join(dir)).expect("create layout directory");
    }
    let mut want = Vec::new();
    for rel in [
        PathBuf::from("system.journal.zst"),
        PathBuf::from("system@archived.journal~.zst"),
        // Joined per component: the CLI prints native separators on Windows.
        Path::new(plain_id).join("user-1000.journal.zst"),
        Path::new(dashed_id).join("system.journal.zst"),
    ] {
        let path = root.path().join(rel);
        std::fs::copy(&fixture, &path).expect("copy fixture");
        want.push(path.display().to_string());
    }
    for rel in [
        "not-an-id/ignored.journal.zst".to_string(),
        format!("{plain_id}/nested/too-deep.journal.zst"),
    ] {
        std::fs::copy(&fixture, root.path().join(rel)).expect("copy ignored fixture");
    }
    std::fs::write(root.path().join("notes.log"), b"not a journal").expect("write notes");

    let out = run_cli(&["--verify", "--directory", root.path().to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "verify failed: {stderr}");
    let mut got: Vec<String> = stderr
        .lines()
        .map(|line| {
            line.strip_prefix("PASS: ")
                .unwrap_or_else(|| panic!("unexpected stderr line {line:?}"))
                .to_string()
        })
        .collect();
    got.sort();
    want.sort();
    assert_eq!(got, want);
}

/// Export output carries `__SEQNUM_ID`, the seqnum ID component of the cursor.
#[test]
fn export_seqnum_id_matches_cursor() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../fixtures/systemd/test-data/no-rtc/system.journal.zst");
    let out = run_cli(&[
        "--file",
        fixture.to_str().unwrap(),
        "-o",
        "export",
        "-n",
        "3",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    let mut checked = 0;
    for entry in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let field = |name: &str| {
            entry
                .lines()
                .find_map(|line| line.strip_prefix(&format!("{name}=")))
                .map(str::to_string)
        };
        let cursor = field("__CURSOR").expect("cursor");
        let from_cursor = cursor
            .split(';')
            .find_map(|part| part.strip_prefix("s="))
            .expect("cursor seqnum id");
        assert_eq!(field("__SEQNUM_ID").as_deref(), Some(from_cursor));
        checked += 1;
    }
    assert_eq!(checked, 3);
}

/// Export and JSON render as systemd v262 does (goldens in tests/output).
#[test]
fn entry_output_matches_systemd() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../tests/output");
    let file = dir.join("entry-output.journal.zst");
    let fields = "--output-fields=MESSAGE,MULTI,BIN,__CURSOR,_BOOT_ID,AFTER,LAST";
    for (golden, args, json) in [
        ("export.golden", vec!["-o", "export"], false),
        ("export-fields.golden", vec!["-o", "export", fields], false),
        ("json.golden", vec!["-o", "json"], true),
        ("json-all.golden", vec!["-o", "json", "--all"], true),
        ("json-fields.golden", vec!["-o", "json", fields], true),
        ("json-pretty.golden", vec!["-o", "json-pretty"], true),
    ] {
        let mut cli_args = vec!["--file", file.to_str().unwrap()];
        cli_args.extend(&args);
        let out = run_cli(&cli_args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let want = std::fs::read(dir.join(golden)).expect("read golden");
        if !json {
            assert!(out.stdout == want, "{args:?}: output differs from {golden}");
            continue;
        }
        // systemd orders keys by hash table iteration, so compare values.
        let decode = |data: &[u8]| -> Vec<serde_json::Value> {
            serde_json::Deserializer::from_slice(data)
                .into_iter::<serde_json::Value>()
                .map(|value| value.expect("decode JSON"))
                .collect()
        };
        assert_eq!(
            decode(&out.stdout),
            decode(&want),
            "{args:?}: JSON differs from {golden}"
        );
    }
}
