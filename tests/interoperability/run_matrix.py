#!/usr/bin/env python3
"""Closed-file interoperability matrix for the pure journal SDKs.

The runner generates synthetic journals with each repository writer and reads
each generated file with stock journalctl plus every repository journalctl
implementation. Runtime artifacts stay under the repository-level .local/.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Iterable


REPO_ROOT = Path(__file__).resolve().parents[2]
LOCAL_DIR = REPO_ROOT / ".local" / "interoperability"
BIN_DIR = LOCAL_DIR / "bin"


@dataclass(frozen=True)
class WriterSpec:
    name: str
    syslog_identifier: str
    mode: str


@dataclass(frozen=True)
class ReaderSpec:
    name: str


WRITERS = {
    "go": WriterSpec("go", "go-live-writer", "file"),
    "rust": WriterSpec("rust", "rust-live-writer", "directory"),
    "node": WriterSpec("node", "node-live-writer", "file"),
    "python": WriterSpec("python", "python-live-writer", "file"),
}

READERS = {
    "stock": ReaderSpec("stock"),
    "go": ReaderSpec("go"),
    "rust": ReaderSpec("rust"),
    "node": ReaderSpec("node"),
    "python": ReaderSpec("python"),
}


def run(cmd: list[str], *, cwd: Path = REPO_ROOT, timeout: int = 60) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        cwd=str(cwd),
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
            f"stdout:\n{result.stdout[-1000:]}\n"
            f"stderr:\n{result.stderr[-1000:]}"
        )


def systemd_version() -> str:
    result = run(["journalctl", "--version"], timeout=10)
    if result.returncode == 0 and result.stdout.strip():
        return result.stdout.splitlines()[0]
    return "unavailable"


def build_tools() -> dict[str, str]:
    BIN_DIR.mkdir(parents=True, exist_ok=True)

    go_livewriter = BIN_DIR / "go-livewriter"
    go_journalctl = BIN_DIR / "go-journalctl"
    require_ok(
        run(["go", "build", "-o", str(go_livewriter), "./internal/testcmd/livewriter"], cwd=REPO_ROOT / "go"),
        "build go livewriter",
    )
    require_ok(
        run(["go", "build", "-o", str(go_journalctl), "./cmd/journalctl"], cwd=REPO_ROOT / "go"),
        "build go journalctl",
    )

    require_ok(
        run(["cargo", "build", "--manifest-path", str(REPO_ROOT / "rust/Cargo.toml"), "-p", "livewriter"], timeout=180),
        "build rust livewriter",
    )
    require_ok(
        run(["cargo", "build", "--manifest-path", str(REPO_ROOT / "rust/Cargo.toml"), "-p", "journalctl"], timeout=180),
        "build rust journalctl",
    )

    rust_livewriter = REPO_ROOT / "rust/target/debug/livewriter"
    rust_journalctl = REPO_ROOT / "rust/target/debug/journalctl"
    for path in (go_livewriter, go_journalctl, rust_livewriter, rust_journalctl):
        if not path.exists():
            raise RuntimeError(f"expected tool not found after build: {path}")

    return {
        "go_livewriter": str(go_livewriter),
        "go_journalctl": str(go_journalctl),
        "rust_livewriter": str(rust_livewriter),
        "rust_journalctl": str(rust_journalctl),
    }


def writer_command(writer: WriterSpec, tools: dict[str, str], target: Path, ready: Path, entries: int) -> list[str]:
    if writer.name == "go":
        return [tools["go_livewriter"], "--path", str(target), "--ready-file", str(ready), "--entries", str(entries), "--delay", "1ms"]
    if writer.name == "rust":
        return [tools["rust_livewriter"], "--dir", str(target), "--ready-file", str(ready), "--entries", str(entries), "--delay", "1ms"]
    if writer.name == "node":
        return ["node", str(REPO_ROOT / "node/internal/testcmd/livewriter.js"), "--path", str(target), "--ready-file", str(ready), "--entries", str(entries), "--delay", "1ms"]
    if writer.name == "python":
        return ["python3", str(REPO_ROOT / "python/cmd/livewriter.py"), "--path", str(target), "--ready-file", str(ready), "--entries", str(entries), "--delay", "1ms"]
    raise ValueError(writer.name)


def generate_journal(writer: WriterSpec, tools: dict[str, str], entries: int) -> dict[str, str]:
    writer_root = LOCAL_DIR / "journals" / writer.name
    if writer_root.exists():
        shutil.rmtree(writer_root)
    writer_root.mkdir(parents=True, exist_ok=True)

    ready = LOCAL_DIR / f"{writer.name}.ready"
    if ready.exists():
        ready.unlink()

    if writer.mode == "directory":
        target = writer_root
    else:
        target = writer_root / f"{writer.name}.journal"

    result = run(writer_command(writer, tools, target, ready, entries), timeout=max(60, entries // 2))
    require_ok(result, f"{writer.name} writer")

    wait_for_file(ready, f"{writer.name} ready file")

    if writer.mode == "directory":
        journal_files = sorted(writer_root.rglob("*.journal"))
        if len(journal_files) != 1:
            raise RuntimeError(f"{writer.name} writer expected exactly one journal file, found {len(journal_files)}")
        journal_path = journal_files[0]
        journal_directory = writer_root
    else:
        journal_path = target
        journal_directory = writer_root

    if not journal_path.exists():
        raise RuntimeError(f"{writer.name} journal was not created: {journal_path}")

    return {
        "writer": writer.name,
        "syslog_identifier": writer.syslog_identifier,
        "journal_file": str(journal_path),
        "journal_directory": str(journal_directory),
    }


def wait_for_file(path: Path, label: str, timeout: float = 5.0) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if path.exists():
            return
        time.sleep(0.05)
    raise RuntimeError(f"timed out waiting for {label}: {path}")


def reader_command(reader: ReaderSpec, tools: dict[str, str], journal_path: str, matches: list[str]) -> list[str]:
    if reader.name == "stock":
        return ["journalctl", "--file", journal_path, "--output=json", "--quiet", "--no-pager", *matches]
    if reader.name == "go":
        return [tools["go_journalctl"], "--file", journal_path, "--output=json", *matches]
    if reader.name == "rust":
        return [tools["rust_journalctl"], "--file", journal_path, "--output=json", *matches]
    if reader.name == "node":
        return ["node", str(REPO_ROOT / "node/cmd/journalctl/index.js"), "--file", journal_path, "--output=json", *matches]
    if reader.name == "python":
        return ["python3", str(REPO_ROOT / "python/cmd/journalctl.py"), "--file", journal_path, "--output=json", *matches]
    raise ValueError(reader.name)


def parse_json_lines(stdout: str) -> list[dict]:
    entries = []
    for line in stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        entries.append(json.loads(line))
    return entries


def sequence_values(entries: list[dict]) -> list[str]:
    return [str(entry.get("LIVE_SEQ", "")) for entry in entries]


def validate_sequence_values(entries: list[dict], expected: list[str]) -> tuple[bool, str]:
    seq = sequence_values(entries)
    if seq != expected:
        return False, f"sequence mismatch: got {seq[:5]}...{seq[-5:] if seq else []}, expected {expected}"
    return True, "ordered"


def read_check(
    reader: ReaderSpec,
    tools: dict[str, str],
    writer_result: dict[str, str],
    matches: list[str],
    expected_count: int,
    test_name: str,
    expected_sequences: list[str] | None = None,
) -> dict:
    cmd = reader_command(reader, tools, writer_result["journal_file"], matches)
    result = run(cmd, timeout=60)
    record = {
        "writer": writer_result["writer"],
        "reader": reader.name,
        "test": test_name,
        "command": shell_join(cmd),
        "status": "FAIL",
    }
    if result.returncode != 0:
        record["error"] = result.stderr[-1000:] or result.stdout[-1000:]
        return record
    try:
        entries = parse_json_lines(result.stdout)
    except Exception as error:
        record["error"] = f"invalid JSON output: {error}"
        return record

    if len(entries) != expected_count:
        record["entries_read"] = len(entries)
        record["expected"] = expected_count
        record["error"] = "entry count mismatch"
        return record

    if expected_sequences is None:
        expected_sequences = [f"{i:06d}" for i in range(expected_count)]
    ok, note = validate_sequence_values(entries, expected_sequences)
    record["entries_read"] = len(entries)
    record["expected"] = expected_count
    record["sequence"] = note
    if ok:
        record["status"] = "PASS"
    else:
        record["error"] = note
    return record


def verify_check(writer_result: dict[str, str]) -> dict:
    cmd = ["journalctl", "--verify", "--file", writer_result["journal_file"]]
    result = run(cmd, timeout=30)
    return {
        "writer": writer_result["writer"],
        "reader": "stock",
        "test": "verify",
        "command": shell_join(cmd),
        "status": "PASS" if result.returncode == 0 else "FAIL",
        "error": "" if result.returncode == 0 else (result.stderr[-1000:] or result.stdout[-1000:]),
    }


def shell_join(cmd: Iterable[str]) -> str:
    return " ".join(json.dumps(part) if any(ch.isspace() for ch in part) else part for part in cmd)


def selected(mapping: dict[str, object], names: list[str] | None) -> list:
    if not names:
        return list(mapping.values())
    missing = [name for name in names if name not in mapping]
    if missing:
        raise SystemExit(f"unknown names: {', '.join(missing)}")
    return [mapping[name] for name in names]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--entries", type=int, default=50)
    parser.add_argument("--writers", nargs="*", choices=sorted(WRITERS))
    parser.add_argument("--readers", nargs="*", choices=sorted(READERS))
    parser.add_argument("--keep-files", action="store_true")
    args = parser.parse_args()

    if args.entries < 2:
        raise SystemExit("--entries must be at least 2")

    LOCAL_DIR.mkdir(parents=True, exist_ok=True)
    tools = build_tools()
    writer_specs = selected(WRITERS, args.writers)
    reader_specs = selected(READERS, args.readers)

    generated = [generate_journal(writer, tools, args.entries) for writer in writer_specs]
    checks: list[dict] = []

    for writer_result in generated:
        checks.append(verify_check(writer_result))
        for reader in reader_specs:
            checks.append(read_check(reader, tools, writer_result, ["PRIORITY=6"], args.entries, "priority-read"))
            checks.append(read_check(reader, tools, writer_result, ["PRIORITY=1"], 0, "negative-priority"))
            checks.append(
                read_check(
                    reader,
                    tools,
                    writer_result,
                    ["MESSAGE=live-000000", "MESSAGE=live-000001"],
                    2,
                    "same-field-or",
                    expected_sequences=["000000", "000001"],
                )
            )
            checks.append(
                read_check(
                    reader,
                    tools,
                    writer_result,
                    ["MESSAGE=live-000000", "+", "MESSAGE=live-000001"],
                    2,
                    "plus-disjunction",
                    expected_sequences=["000000", "000001"],
                )
            )
            checks.append(
                read_check(
                    reader,
                    tools,
                    writer_result,
                    ["PRIORITY=6", "MESSAGE=live-000000"],
                    1,
                    "cross-field-and",
                    expected_sequences=["000000"],
                )
            )

    passed = sum(1 for check in checks if check["status"] == "PASS")
    failed = len(checks) - passed
    payload = {
        "timestamp": datetime.now().isoformat(timespec="seconds"),
        "systemd_version": systemd_version(),
        "entries_per_writer": args.entries,
        "writers": [writer.name for writer in writer_specs],
        "readers": [reader.name for reader in reader_specs],
        "generated": generated,
        "checks": checks,
        "summary": {"total": len(checks), "passed": passed, "failed": failed},
    }

    result_path = LOCAL_DIR / f"matrix-results-{datetime.now().strftime('%Y%m%d-%H%M%S')}.json"
    result_path.write_text(json.dumps(payload, indent=2) + "\n")

    print(f"systemd: {payload['systemd_version']}")
    print(f"entries per writer: {args.entries}")
    print(f"writers: {', '.join(payload['writers'])}")
    print(f"readers: {', '.join(payload['readers'])}")
    print(f"checks: {len(checks)} total, {passed} passed, {failed} failed")
    for check in checks:
        status = check["status"]
        detail = f"{check['writer']} -> {check['reader']} {check['test']}"
        if status == "PASS":
            print(f"PASS {detail}")
        else:
            print(f"FAIL {detail}: {check.get('error', '')}")
    print(f"results: {result_path}")

    if not args.keep_files:
        for ready_file in LOCAL_DIR.glob("*.ready"):
            ready_file.unlink(missing_ok=True)

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
