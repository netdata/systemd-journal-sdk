//! Subprocess tests use Cargo's integration-test binary path instead of a
//! workspace-target fallback.

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
