#!/usr/bin/env python3
"""File-backed journalctl query parity matrix.

Generates repo-local fixtures and compares stock journalctl with the Rust and
Go journalctl rewrites for --since, --until, --boot, and --follow behavior.
Runtime artifacts stay under .local/interoperability/.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess  # nosec B404
import time
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

from go_fixture_writer import start_live_journal_writer, write_journal_file


REPO_ROOT = Path(__file__).resolve().parents[2]
LOCAL_DIR = REPO_ROOT / ".local" / "interoperability"
BIN_DIR = LOCAL_DIR / "bin"
FIXTURE_DIR = LOCAL_DIR / "journalctl-query"

MACHINE_ID = "00112233445566778899aabbccddeeff"
BOOT_A = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
BOOT_B = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
BOOT_C = "cccccccccccccccccccccccccccccccc"


@dataclass(frozen=True)
class ReaderSpec:
    name: str


@dataclass(frozen=True)
class Row:
    path: str
    boot_id: str
    message: str
    realtime: int


READERS = {
    "stock": ReaderSpec("stock"),
    "go": ReaderSpec("go"),
    "rust": ReaderSpec("rust"),
}


STATIC_ROWS = [
    Row("a.journal", BOOT_A, "query-a", 1_700_004_000_000_000),
    Row("b.journal", BOOT_B, "query-b", 1_700_004_000_001_000),
    Row("c.journal", BOOT_C, "query-c", 1_700_004_000_002_000),
]

FILE_ROWS = [
    Row("multi-boot-file.journal", BOOT_A, "file-a", 1_700_004_100_000_000),
    Row("multi-boot-file.journal", BOOT_B, "file-b", 1_700_004_100_001_000),
    Row("multi-boot-file.journal", BOOT_C, "file-c", 1_700_004_100_002_000),
]


def local_timestamp(usec: int) -> str:
    return datetime.fromtimestamp(usec / 1_000_000).isoformat(sep=" ", timespec="microseconds")


def run(
    cmd: list[str],
    *,
    cwd: Path = REPO_ROOT,
    timeout: int = 120,
    env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    return subprocess.run(  # nosec B603
        cmd,  # nosemgrep
        cwd=str(cwd),
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )


def require_ok(result: subprocess.CompletedProcess[str], label: str) -> None:
    if result.returncode != 0:
        raise RuntimeError(
            f"{label} failed with exit {result.returncode}\n"
            f"stdout:\n{result.stdout[-2000:]}\n"
            f"stderr:\n{result.stderr[-2000:]}"
        )


def build_env() -> dict[str, str]:
    env = os.environ.copy()
    local = REPO_ROOT / ".local"
    env.setdefault("GOMODCACHE", str(local / "go" / "pkg" / "mod"))
    env.setdefault("GOCACHE", str(local / "go-build"))
    env.setdefault("GOPATH", str(local / "go"))
    env.setdefault("CARGO_HOME", str(local / "cargo-home"))
    env.setdefault("CARGO_TARGET_DIR", str(local / "cargo-target"))
    return env


def build_tools() -> dict[str, str]:
    BIN_DIR.mkdir(parents=True, exist_ok=True)
    env = build_env()

    go_journalctl = BIN_DIR / "go-journalctl"
    require_ok(
        run(["go", "build", "-o", str(go_journalctl), "./cmd/journalctl"], cwd=REPO_ROOT / "go", env=env),
        "build go journalctl",
    )
    require_ok(
        run(
            ["cargo", "build", "--manifest-path", str(REPO_ROOT / "rust/Cargo.toml"), "-p", "journalctl"],
            timeout=180,
            env=env,
        ),
        "build rust journalctl",
    )

    rust_src = Path(env["CARGO_TARGET_DIR"]) / "debug" / "journalctl"
    rust_journalctl = BIN_DIR / "rust-journalctl"
    if rust_src.exists():
        shutil.copy2(rust_src, rust_journalctl)

    for path in (go_journalctl, rust_journalctl):
        if not path.exists():
            raise RuntimeError(f"expected binary not found: {path}")

    return {
        "go_journalctl": str(go_journalctl),
        "rust_journalctl": str(rust_journalctl),
    }


def systemd_version() -> str:
    result = run(["journalctl", "--version"], timeout=10)
    if result.returncode == 0 and result.stdout.strip():
        return result.stdout.splitlines()[0]
    return "unavailable"


def make_fixtures() -> dict[str, Path]:
    if FIXTURE_DIR.exists():
        shutil.rmtree(FIXTURE_DIR)
    directory = FIXTURE_DIR / "directory"
    single_file = FIXTURE_DIR / "multi-boot-file.journal"
    follow = FIXTURE_DIR / "follow"
    directory.mkdir(parents=True)
    follow.mkdir(parents=True)

    for row in STATIC_ROWS:
        write_journal(directory / row.path, [row])
    write_journal(single_file, FILE_ROWS)
    return {"directory": directory, "file": single_file, "follow": follow}


def write_journal(path: Path, rows: list[Row]) -> None:
    first = rows[0]
    write_journal_file(
        path,
        machine_id=MACHINE_ID,
        boot_id=first.boot_id,
        seqnum_id="12121212121212121212121212121212",
        entries=[
            {
                "realtime_usec": row.realtime,
                "monotonic_usec": i,
                "boot_id": row.boot_id,
                "fields": [
                    ("MESSAGE", row.message),
                    ("TEST_ID", "journalctl-query"),
                    ("_BOOT_ID", row.boot_id),
                    ("_MACHINE_ID", MACHINE_ID),
                ],
            }
            for i, row in enumerate(rows, start=1)
        ],
    )


def row_entry(message: str, realtime: int, monotonic: int, test_id: str) -> dict[str, object]:
    return {
        "realtime_usec": realtime,
        "monotonic_usec": monotonic,
        "boot_id": BOOT_A,
        "fields": [
            ("MESSAGE", message),
            ("TEST_ID", test_id),
            ("_BOOT_ID", BOOT_A),
            ("_MACHINE_ID", MACHINE_ID),
        ],
    }


def reader_command(reader: str, tools: dict[str, str], mode: str, path: Path, args: list[str]) -> list[str]:
    if reader == "stock":
        base = ["journalctl", f"--{mode}", str(path), "--output=json", "--no-pager", "--quiet"]
    elif reader == "go":
        base = [tools["go_journalctl"], f"--{mode}", str(path), "--output=json"]
    elif reader == "rust":
        base = [tools["rust_journalctl"], f"--{mode}", str(path), "--output=json"]
    else:
        raise ValueError(reader)
    return [*base, *args]


def parse_messages(output: str) -> list[str]:
    messages = []
    for line in output.splitlines():
        if not line.startswith("{"):
            continue
        obj = json.loads(line)
        message = obj.get("MESSAGE")
        if isinstance(message, list):
            messages.extend(str(v) for v in message)
        elif message is not None:
            messages.append(str(message))
    return messages


def run_static_cases(tools: dict[str, str], fixtures: dict[str, Path]) -> list[dict[str, object]]:
    cases = [
        ("directory-all", "directory", fixtures["directory"], ["TEST_ID=journalctl-query"]),
        ("directory-since", "directory", fixtures["directory"], ["--since", "@1700004000.000001", "TEST_ID=journalctl-query"]),
        (
            "directory-since-local-fraction",
            "directory",
            fixtures["directory"],
            ["--since", local_timestamp(1_700_004_000_000_001), "TEST_ID=journalctl-query"],
        ),
        ("directory-until", "directory", fixtures["directory"], ["--until", "@1700004000.001", "TEST_ID=journalctl-query"]),
        (
            "directory-since-until",
            "directory",
            fixtures["directory"],
            ["--since", "@1700004000.000001", "--until", "@1700004000.001", "TEST_ID=journalctl-query"],
        ),
        ("directory-boot-all", "directory", fixtures["directory"], ["--boot=all", "TEST_ID=journalctl-query"]),
        ("directory-boot-implicit-latest", "directory", fixtures["directory"], ["--boot", "TEST_ID=journalctl-query"]),
        ("directory-boot-latest", "directory", fixtures["directory"], ["--boot=0", "TEST_ID=journalctl-query"]),
        ("directory-boot-previous", "directory", fixtures["directory"], ["--boot=-1", "TEST_ID=journalctl-query"]),
        ("directory-boot-first", "directory", fixtures["directory"], ["--boot=1", "TEST_ID=journalctl-query"]),
        ("directory-boot-id", "directory", fixtures["directory"], [f"--boot={BOOT_A}", "TEST_ID=journalctl-query"]),
        ("directory-boot-id-offset", "directory", fixtures["directory"], [f"--boot={BOOT_B}+1", "TEST_ID=journalctl-query"]),
        (
            "directory-boot-since-until",
            "directory",
            fixtures["directory"],
            ["--boot=-1", "--since", "@1700004000.000001", "--until", "@1700004000.001", "TEST_ID=journalctl-query"],
        ),
        ("file-all", "file", fixtures["file"], ["TEST_ID=journalctl-query"]),
        ("file-boot-latest", "file", fixtures["file"], ["--boot=0", "TEST_ID=journalctl-query"]),
        ("file-boot-first", "file", fixtures["file"], ["--boot=1", "TEST_ID=journalctl-query"]),
        (
            "file-since-until",
            "file",
            fixtures["file"],
            ["--since", "@1700004100.000001", "--until", "@1700004100.001", "TEST_ID=journalctl-query"],
        ),
    ]

    results: list[dict[str, object]] = []
    for case_name, mode, path, args in cases:
        stock = run(reader_command("stock", tools, mode, path, args), timeout=30)
        require_ok(stock, f"stock {case_name}")
        expected = parse_messages(stock.stdout)
        for reader in READERS:
            cmd = reader_command(reader, tools, mode, path, args)
            result = run(cmd, timeout=30)
            actual = parse_messages(result.stdout)
            ok = result.returncode == 0 and actual == expected
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected": expected,
                    "actual": actual,
                    "returncode": result.returncode,
                    "stderr": result.stderr[-1000:],
                }
            )
    return results


def run_follow_cases(tools: dict[str, str], fixtures: dict[str, Path]) -> list[dict[str, object]]:
    results = []
    cases = [
        {
            "name": "follow-live-append-no-tail",
            "test_id": "journalctl-follow",
            "args": ["--follow", "--no-tail", "--boot=all"],
            "initial": [],
            "appends": [(f"follow-{i}", 1_700_004_200_000_000 + i) for i in range(3)],
            "expected": ["follow-0", "follow-1", "follow-2"],
        },
        {
            "name": "follow-default-tail",
            "test_id": "journalctl-follow-tail",
            "args": ["--follow", "--boot=all"],
            "initial": [(f"tail-initial-{i:02d}", 1_700_004_300_000_000 + i) for i in range(12)],
            "appends": [(f"tail-new-{i}", 1_700_004_300_001_000 + i) for i in range(2)],
            "expected": [f"tail-initial-{i:02d}" for i in range(2, 12)] + ["tail-new-0", "tail-new-1"],
        },
        {
            "name": "follow-since-boot-latest",
            "test_id": "journalctl-follow-since",
            "args": ["--follow", "--no-tail", "--boot=0", "--since", "@1700004200.000001"],
            "initial": [("since-before", 1_700_004_200_000_000)],
            "appends": [("since-after-0", 1_700_004_200_000_001), ("since-after-1", 1_700_004_200_000_002)],
            "expected": ["since-after-0", "since-after-1"],
        },
        {
            "name": "follow-directory-no-tail",
            "mode": "directory",
            "test_id": "journalctl-follow-directory",
            "args": ["--follow", "--no-tail", "--boot=all"],
            "initial": [],
            "appends": [(f"dir-follow-{i}", 1_700_004_400_000_000 + i) for i in range(2)],
            "expected": ["dir-follow-0", "dir-follow-1"],
        },
    ]
    for case in cases:
        mode = str(case.get("mode", "file"))
        for reader in READERS:
            case_root = fixtures["follow"] / case["name"] / reader
            if mode == "directory":
                read_path = case_root
                write_path = case_root / "active.journal"
            else:
                read_path = fixtures["follow"] / case["name"] / f"{reader}.journal"
                write_path = read_path
            actual, returncode, stderr, cmd = run_follow_reader(reader, tools, mode, read_path, write_path, case)
            expected = case["expected"]
            ok = returncode == 0 and actual == expected
            results.append(
                {
                    "test": case["name"],
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected": expected,
                    "actual": actual,
                    "returncode": returncode,
                    "stderr": stderr[-1000:],
                }
            )
    return results


def run_follow_reader(
    reader: str,
    tools: dict[str, str],
    mode: str,
    read_path: Path,
    write_path: Path,
    case: dict[str, object],
) -> tuple[list[str], int, str, list[str]]:
    if mode == "directory":
        read_path.mkdir(parents=True, exist_ok=True)
    write_path.parent.mkdir(parents=True, exist_ok=True)
    ready_file = write_path.with_suffix(write_path.suffix + ".ready")
    seq = 1
    initial_entries = []
    for message, realtime in case["initial"]:
        initial_entries.append(row_entry(message, realtime, seq, str(case["test_id"])))
        seq += 1
    append_entries = []
    for message, realtime in case["appends"]:
        append_entries.append(row_entry(message, realtime, seq, str(case["test_id"])))
        seq += 1
    writer_proc = start_live_journal_writer(
        write_path,
        ready_file=ready_file,
        machine_id=MACHINE_ID,
        boot_id=BOOT_A,
        seqnum_id="34343434343434343434343434343434",
        initial_entries=initial_entries,
        append_entries=append_entries,
    )
    ready_error = wait_for_writer_ready(writer_proc, ready_file)
    if ready_error is not None:
        return [], ready_error[0], ready_error[1], []
    cmd = reader_command(
        reader,
        tools,
        mode,
        read_path,
        [*case["args"], f"TEST_ID={case['test_id']}"],
    )
    proc: subprocess.Popen[str] | None = None
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    proc = subprocess.Popen(  # nosec B603
        cmd,  # nosemgrep
        cwd=str(REPO_ROOT),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    try:
        writer_stdout, writer_stderr = communicate_or_stop(writer_proc, timeout=10, stop_timeout=2)
        time.sleep(0.7)
        proc.terminate()
        stdout, stderr = communicate_or_kill(proc, timeout=5)
    finally:
        stop_process_if_running(writer_proc)
        stop_process_if_running(proc)

    actual = parse_messages(stdout)
    expected = case["expected"]
    if writer_proc.returncode != 0:
        return actual, writer_proc.returncode or 1, (writer_stdout + writer_stderr + stderr)[-1000:], cmd
    return actual, 0 if actual == expected else proc.returncode or 1, stderr, cmd


def wait_for_writer_ready(writer_proc: subprocess.Popen[str], ready_file: Path) -> tuple[int, str] | None:
    deadline = time.monotonic() + 5.0
    while not ready_file.exists():
        if writer_proc.poll() is not None:
            stdout, stderr = writer_proc.communicate(timeout=1)
            return writer_proc.returncode or 1, (stdout + stderr)[-1000:]
        if time.monotonic() > deadline:
            stdout, stderr = terminate_or_kill(writer_proc, timeout=2)
            return 1, ("go live fixture writer did not become ready\n" + stdout + stderr)[-1000:]
        time.sleep(0.05)
    return None


def communicate_or_kill(proc: subprocess.Popen[str], timeout: float) -> tuple[str, str]:
    try:
        return proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        proc.kill()
        return proc.communicate(timeout=timeout)


def terminate_or_kill(proc: subprocess.Popen[str], timeout: float) -> tuple[str, str]:
    proc.terminate()
    return communicate_or_kill(proc, timeout)


def communicate_or_stop(proc: subprocess.Popen[str], *, timeout: float, stop_timeout: float) -> tuple[str, str]:
    try:
        return proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        return terminate_or_kill(proc, stop_timeout)


def stop_process_if_running(proc: subprocess.Popen[str] | None) -> None:
    if proc is not None and proc.poll() is None:
        terminate_or_kill(proc, timeout=2)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-follow", action="store_true", help="skip live follow checks")
    args = parser.parse_args()

    tools = build_tools()
    fixtures = make_fixtures()
    results = run_static_cases(tools, fixtures)
    if not args.skip_follow:
        results.extend(run_follow_cases(tools, fixtures))

    failures = [r for r in results if r["status"] != "PASS"]
    report = {
        "status": "PASS" if not failures else "FAIL",
        "systemd": systemd_version(),
        "fixture_dir": str(FIXTURE_DIR),
        "results": results,
        "failures": failures,
    }
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
