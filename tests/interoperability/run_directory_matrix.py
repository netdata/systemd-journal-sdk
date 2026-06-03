#!/usr/bin/env python3
"""Directory traversal parity matrix for file-backed journalctl readers.

The runner creates synthetic journal directories under .local/ and compares
repository `--directory` behavior against stock journalctl for the stock
supported `.journal` and `.journal~` directory layout. Whole-file `.journal.zst`
directory discovery is validated separately as a repository extension because
systemd v260.1 directory enumeration only accepts `.journal` and `.journal~`.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess  # nosec B404 - subprocess is required by harnesses.
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


REPO_ROOT = Path(__file__).resolve().parents[2]
LOCAL_DIR = REPO_ROOT / ".local" / "interoperability"
BIN_DIR = LOCAL_DIR / "bin"
FIXTURE_DIR = LOCAL_DIR / "directory"

PYTHON = os.environ.get("PYTHON", "python3")

MACHINE_A = "00112233445566778899aabbccddeeff"
MACHINE_B = "102132435465768798a9babbdcddedef"
MACHINE_C = "112233445566778899aabbccddeeff00"
MACHINE_D_UUID = "12345678-9abc-def0-1234-56789abcdef0"
NAMESPACE_DIR = f"{MACHINE_A}.ns"


@dataclass(frozen=True)
class ReaderSpec:
    name: str


@dataclass(frozen=True)
class FixtureRow:
    seq: str
    group: str
    realtime: int
    monotonic: int
    message: str


READERS = {
    "stock": ReaderSpec("stock"),
    "go": ReaderSpec("go"),
    "rust": ReaderSpec("rust"),
    "node": ReaderSpec("node"),
    "python": ReaderSpec("python"),
}


def run(
    cmd: list[str],
    *,
    cwd: Path = REPO_ROOT,
    timeout: int = 120,
    env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    return subprocess.run(  # nosec B603 - harness uses shell=False command vectors.
        cmd,
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
    stock_dir = FIXTURE_DIR / "stock-parity"
    corrupt_dir = FIXTURE_DIR / "corrupt-skip"
    zst_dir = FIXTURE_DIR / "zst-extension"
    empty_dir = FIXTURE_DIR / "empty"
    stock_dir.mkdir(parents=True)
    corrupt_dir.mkdir(parents=True)
    zst_dir.mkdir(parents=True)
    empty_dir.mkdir(parents=True)

    write_journal(
        stock_dir / "root-active.journal",
        MACHINE_A,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "01010101010101010101010101010101",
        [
            FixtureRow("000000", "root", 1_700_003_000_000_000, 1, "directory root first"),
            FixtureRow("000004", "root", 1_700_003_000_000_004, 2, "directory root second"),
        ],
    )
    write_journal(
        stock_dir / "root-overlap.journal",
        MACHINE_B,
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "02020202020202020202020202020202",
        [FixtureRow("000002", "root", 1_700_003_000_000_002, 1, "directory root overlap")],
    )
    write_journal(
        stock_dir / "archived.journal~",
        MACHINE_C,
        "cccccccccccccccccccccccccccccccc",
        "03030303030303030303030303030303",
        [FixtureRow("000003", "archived", 1_700_003_000_000_003, 1, "directory archived")],
    )

    machine_dir = stock_dir / MACHINE_A
    write_journal(
        machine_dir / "machine.journal",
        MACHINE_A,
        "dddddddddddddddddddddddddddddddd",
        "04040404040404040404040404040404",
        [FixtureRow("000001", "machine", 1_700_003_000_000_001, 1, "directory machine")],
    )

    uuid_dir = stock_dir / MACHINE_D_UUID
    write_journal(
        uuid_dir / "uuid-machine.journal",
        "123456789abcdef0123456789abcdef0",
        "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        "05050505050505050505050505050505",
        [FixtureRow("000005", "machine", 1_700_003_000_000_005, 1, "directory dashed machine")],
    )

    write_journal(
        stock_dir / "not-a-machine-id" / "skipped.journal",
        MACHINE_A,
        "99990000000000000000000000000000",
        "99990101010101010101010101010101",
        [FixtureRow("999000", "skipped", 1_700_003_000_000_900, 1, "invalid subdir")],
    )
    write_journal(
        stock_dir / NAMESPACE_DIR / "namespace.journal",
        MACHINE_A,
        "99990000000000000000000000000001",
        "99990202020202020202020202020202",
        [FixtureRow("999001", "skipped", 1_700_003_000_000_901, 1, "namespace subdir")],
    )
    write_journal(
        machine_dir / "00112233445566778899aabbccddeeff" / "nested.journal",
        MACHINE_A,
        "99990000000000000000000000000002",
        "99990303030303030303030303030303",
        [FixtureRow("999002", "skipped", 1_700_003_000_000_902, 1, "nested subdir")],
    )

    write_journal(
        corrupt_dir / "valid.journal",
        MACHINE_A,
        "abababababababababababababababa0",
        "99990404040404040404040404040404",
        [FixtureRow("corrupt-valid", "corrupt", 1_700_003_000_001_000, 1, "directory corrupt valid")],
    )
    (corrupt_dir / "corrupt.journal").write_bytes(b"not a valid journal")
    unreadable = corrupt_dir / "unreadable.journal"
    unreadable.write_bytes(b"not readable as a journal")
    unreadable.chmod(0)

    plain_zst_source = zst_dir / "sdk-zst-source.journal"
    write_journal(
        plain_zst_source,
        MACHINE_A,
        "abababababababababababababababab",
        "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
        [FixtureRow("zst-000000", "zst", 1_700_003_100_000_000, 1, "directory zst extension")],
    )
    zst_target = zst_dir / "sdk-zst.journal.zst"
    require_ok(run(["zstd", "-q", "-f", "-o", str(zst_target), str(plain_zst_source)]), "compress zst fixture")
    plain_zst_source.unlink()

    return {
        "stock": stock_dir,
        "corrupt": corrupt_dir,
        "zst": zst_dir,
        "empty": empty_dir,
    }


def write_journal(path: Path, machine_id: str, boot_id: str, seqnum_id: str, rows: list[FixtureRow]) -> None:
    sys.path.insert(0, str(REPO_ROOT / "python"))
    from journal import Writer

    path.parent.mkdir(parents=True, exist_ok=True)
    writer = Writer.create(
        str(path),
        {
            "machine_id": machine_id,
            "boot_id": boot_id,
            "seqnum_id": seqnum_id,
        },
    )
    try:
        for row in rows:
            writer.append(
                [
                    {"name": "TEST_ID", "value": "directory-parity"},
                    {"name": "_BOOT_ID", "value": boot_id},
                    {"name": "_MACHINE_ID", "value": machine_id},
                    {"name": "MESSAGE", "value": row.message},
                    {"name": "PRIORITY", "value": "6"},
                    {"name": "DIRECTORY_SEQ", "value": row.seq},
                    {"name": "LIVE_SEQ", "value": row.seq},
                    {"name": "DIRECTORY_GROUP", "value": row.group},
                ],
                {"realtime_usec": row.realtime, "monotonic_usec": row.monotonic},
            )
    finally:
        writer.close()


def reader_command(reader: ReaderSpec, tools: dict[str, str], mode: str, directory: Path, args: list[str]) -> list[str]:
    if reader.name == "stock":
        base = ["journalctl", "--directory", str(directory), "--no-pager", "--quiet"]
    elif reader.name == "go":
        base = [tools["go_journalctl"], "--directory", str(directory)]
    elif reader.name == "rust":
        base = [tools["rust_journalctl"], "--directory", str(directory)]
    elif reader.name == "node":
        base = ["node", str(REPO_ROOT / "node/cmd/journalctl/index.js"), "--directory", str(directory)]
    elif reader.name == "python":
        base = [PYTHON, str(REPO_ROOT / "python/cmd/journalctl.py"), "--directory", str(directory)]
    else:
        raise ValueError(reader.name)

    if mode == "json":
        return [*base, "--output=json", *args]
    if mode == "export":
        return [*base, "--output=export", *args]
    if mode == "text":
        return [*base, *args]
    if mode == "fields":
        return [*base, "--fields"]
    if mode == "boots":
        return [*base, "--list-boots"]
    if mode == "verify":
        if reader.name == "stock":
            return ["journalctl", "--verify", "--directory", str(directory), "--no-pager", "--quiet"]
        return [base[0], *base[1:], "--verify"]
    raise ValueError(mode)


def parse_json_lines(stdout: str) -> list[dict]:
    return [json.loads(line) for line in stdout.splitlines() if line.strip()]


def sequence_values(entries: list[dict]) -> list[str]:
    return [str(entry.get("DIRECTORY_SEQ", "")) for entry in entries]


def export_sequence_values(stdout: str) -> list[str]:
    return [line.split("=", 1)[1] for line in stdout.splitlines() if line.startswith("DIRECTORY_SEQ=")]


def run_json_check(
    reader: ReaderSpec,
    tools: dict[str, str],
    directory: Path,
    test_name: str,
    matches: list[str],
    expected: list[str],
) -> dict:
    cmd = reader_command(reader, tools, "json", directory, matches)
    result = run(cmd)
    record = base_record(reader, test_name, cmd)
    if result.returncode != 0:
        record["error"] = result.stderr[-1000:] or result.stdout[-1000:]
        return record
    try:
        entries = parse_json_lines(result.stdout)
    except Exception as error:
        record["error"] = f"invalid JSON output: {error}"
        return record
    got = sequence_values(entries)
    record["entries_read"] = len(got)
    record["expected"] = len(expected)
    record["sequence"] = got
    if got == expected:
        record["status"] = "PASS"
    else:
        record["error"] = f"sequence mismatch: got {got}, expected {expected}"
    return record


def run_export_check(reader: ReaderSpec, tools: dict[str, str], directory: Path, expected: list[str]) -> dict:
    cmd = reader_command(reader, tools, "export", directory, [])
    result = run(cmd)
    record = base_record(reader, "export-output", cmd)
    if result.returncode != 0:
        record["error"] = result.stderr[-1000:] or result.stdout[-1000:]
        return record
    got = export_sequence_values(result.stdout)
    if got == expected:
        record["status"] = "PASS"
    else:
        record["error"] = f"export sequence mismatch: got {got}, expected {expected}"
    return record


def run_text_check(reader: ReaderSpec, tools: dict[str, str], directory: Path, expected_messages: list[str]) -> dict:
    cmd = reader_command(reader, tools, "text", directory, [])
    result = run(cmd)
    record = base_record(reader, "text-output", cmd)
    if result.returncode != 0:
        record["error"] = result.stderr[-1000:] or result.stdout[-1000:]
        return record
    missing = [message for message in expected_messages if message not in result.stdout]
    if not missing:
        record["status"] = "PASS"
    else:
        record["error"] = f"missing text messages: {missing}"
    return record


def run_fields_check(reader: ReaderSpec, tools: dict[str, str], directory: Path) -> dict:
    required = {"TEST_ID", "MESSAGE", "PRIORITY", "DIRECTORY_SEQ", "LIVE_SEQ", "DIRECTORY_GROUP"}
    cmd = reader_command(reader, tools, "fields", directory, [])
    result = run(cmd)
    record = base_record(reader, "fields", cmd)
    if result.returncode != 0:
        record["error"] = result.stderr[-1000:] or result.stdout[-1000:]
        return record
    fields = {line.strip() for line in result.stdout.splitlines() if line.strip()}
    missing = sorted(required - fields)
    if not missing:
        record["status"] = "PASS"
    else:
        record["error"] = f"missing fields: {missing}"
    return record


def run_boots_check(reader: ReaderSpec, tools: dict[str, str], directory: Path, expected_count: int) -> dict:
    cmd = reader_command(reader, tools, "boots", directory, [])
    result = run(cmd)
    record = base_record(reader, "list-boots", cmd)
    if result.returncode != 0:
        record["error"] = result.stderr[-1000:] or result.stdout[-1000:]
        return record
    lines = [line for line in result.stdout.splitlines() if line.strip()]
    record["entries_read"] = len(lines)
    record["expected"] = expected_count
    if len(lines) == expected_count:
        record["status"] = "PASS"
    else:
        record["error"] = f"boot count mismatch: got {len(lines)}, expected {expected_count}"
    return record


def run_verify_corrupt_check(reader: ReaderSpec, tools: dict[str, str], directory: Path) -> dict:
    cmd = reader_command(reader, tools, "verify", directory, [])
    result = run(cmd)
    record = base_record(reader, "verify-skips-corrupt-directory", cmd)
    if result.returncode == 0:
        record["status"] = "PASS"
    else:
        record["error"] = result.stderr[-500:] or result.stdout[-500:]
    return record


def base_record(reader: ReaderSpec, test_name: str, cmd: list[str]) -> dict:
    return {
        "reader": reader.name,
        "test": test_name,
        "command": shell_join(cmd),
        "status": "FAIL",
    }


def shell_join(cmd: Iterable[str]) -> str:
    return " ".join(json.dumps(part) if any(ch.isspace() for ch in part) else part for part in cmd)


def selected(mapping: dict[str, ReaderSpec], names: list[str] | None) -> list[ReaderSpec]:
    if not names:
        return list(mapping.values())
    missing = [name for name in names if name not in mapping]
    if missing:
        raise SystemExit(f"unknown readers: {', '.join(missing)}")
    return [mapping[name] for name in names]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--readers", nargs="*", choices=sorted(READERS))
    parser.add_argument("--keep-files", action="store_true")
    args = parser.parse_args()

    LOCAL_DIR.mkdir(parents=True, exist_ok=True)
    tools = build_tools()
    fixtures = make_fixtures()
    readers = selected(READERS, args.readers)

    expected_all = ["000001", "000000", "000002", "000003", "000004", "000005"]
    expected_messages = [
        "directory root first",
        "directory machine",
        "directory root overlap",
        "directory archived",
        "directory root second",
        "directory dashed machine",
    ]
    expected_groups = {
        "root-machine-or": ["000001", "000000", "000002", "000004", "000005"],
        "and-root-priority": ["000000", "000002", "000004"],
        "plus-disjunction": ["000000", "000003"],
    }

    checks: list[dict] = []

    stock_check = run_json_check(READERS["stock"], tools, fixtures["stock"], "json-all", [], expected_all)
    if stock_check["status"] != "PASS":
        checks.append(stock_check)
    else:
        for reader in readers:
            checks.append(run_json_check(reader, tools, fixtures["stock"], "json-all", [], expected_all))
            checks.append(
                run_json_check(
                    reader,
                    tools,
                    fixtures["stock"],
                    "root-machine-or",
                    ["DIRECTORY_GROUP=root", "DIRECTORY_GROUP=machine"],
                    expected_groups["root-machine-or"],
                )
            )
            checks.append(
                run_json_check(
                    reader,
                    tools,
                    fixtures["stock"],
                    "and-root-priority",
                    ["DIRECTORY_GROUP=root", "PRIORITY=6"],
                    expected_groups["and-root-priority"],
                )
            )
            checks.append(
                run_json_check(
                    reader,
                    tools,
                    fixtures["stock"],
                    "plus-disjunction",
                    ["DIRECTORY_SEQ=000000", "+", "DIRECTORY_SEQ=000003"],
                    expected_groups["plus-disjunction"],
                )
            )
            checks.append(run_export_check(reader, tools, fixtures["stock"], expected_all))
            checks.append(run_text_check(reader, tools, fixtures["stock"], expected_messages))
            checks.append(run_fields_check(reader, tools, fixtures["stock"]))
            checks.append(run_boots_check(reader, tools, fixtures["stock"], expected_count=5))
            checks.append(
                run_json_check(
                    reader,
                    tools,
                    fixtures["corrupt"],
                    "corrupt-directory-skips-bad-files",
                    [],
                    ["corrupt-valid"],
                )
            )
            checks.append(run_verify_corrupt_check(reader, tools, fixtures["corrupt"]))

    repo_readers = [reader for reader in readers if reader.name != "stock"]
    for reader in repo_readers:
        checks.append(run_json_check(reader, tools, fixtures["zst"], "zst-directory-extension", [], ["zst-000000"]))
        checks.append(run_json_check(reader, tools, fixtures["empty"], "empty-directory", [], []))

    failed = [check for check in checks if check["status"] != "PASS"]
    report = {
        "status": "FAIL" if failed else "PASS",
        "systemd_version": systemd_version(),
        "fixture_directory": str(FIXTURE_DIR),
        "checks": checks,
    }
    print(json.dumps(report, indent=2, sort_keys=True))

    if failed:
        return 1
    if not args.keep_files:
        shutil.rmtree(FIXTURE_DIR, ignore_errors=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
