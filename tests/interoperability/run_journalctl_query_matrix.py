#!/usr/bin/env python3
"""File-backed journalctl query parity matrix.

Generates repo-local fixtures and compares stock journalctl with the Rust and
Go journalctl rewrites for --since, --until, --boot, --lines, --reverse,
--pager-end, --show-cursor, cursor seeking, field filters, grep filters,
output-control behavior, empty results, and --follow behavior.
Runtime artifacts stay under .local/interoperability/.
"""

from __future__ import annotations

import argparse
import json
import os
import re
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
HOST_UID = str(os.getuid()) if hasattr(os, "getuid") else "0"
COREDUMP_MESSAGE_ID = "fc2e22bc6ee647b6b90729ab34a250b1"
NEW_ID128_RE = re.compile(
    r"^As string:\n"
    r"([0-9a-f]{32})\n\n"
    r"As UUID:\n"
    r"([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\n\n"
    r"As systemd-id128\(1\) macro:\n"
    r"#define XYZ SD_ID128_MAKE\(((?:[0-9a-f]{2},){15}[0-9a-f]{2})\)\n\n"
    r"As Python constant:\n"
    r">>> import uuid\n"
    r">>> XYZ = uuid.UUID\('([0-9a-f]{32})'\)\n$"
)
VACUUM_SEQNUM_ID = "12121212121212121212121212121212"


@dataclass(frozen=True)
class ReaderSpec:
    name: str


@dataclass(frozen=True)
class Row:
    path: str
    boot_id: str
    message: str
    realtime: int
    fields: tuple[tuple[str, str], ...] = ()


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
    Row(
        "multi-boot-file.journal",
        BOOT_A,
        "file-a",
        1_700_004_100_000_000,
        (
            ("SYSLOG_IDENTIFIER", "app-a"),
            ("SYSLOG_PID", "9001"),
            ("PRIORITY", "3"),
            ("SYSLOG_FACILITY", "3"),
            ("_TRANSPORT", "syslog"),
            ("_HOSTNAME", "fixture-host-a"),
            ("_PID", "101"),
            ("_SYSTEMD_UNIT", "alpha.service"),
            ("_SYSTEMD_CGROUP", "/init.scope"),
            ("_SYSTEMD_INVOCATION_ID", "11111111111111111111111111111111"),
            ("UNIT", "manager-alpha.service"),
            ("_UID", "0"),
            ("OBJECT_SYSTEMD_UNIT", "object-alpha.service"),
            ("MESSAGE_ID", COREDUMP_MESSAGE_ID),
            ("COREDUMP_UNIT", "crash-alpha.service"),
            ("_SYSTEMD_SLICE", "app-alpha.slice"),
        ),
    ),
    Row(
        "multi-boot-file.journal",
        BOOT_B,
        "file-alpha-second",
        1_700_004_100_000_500,
        (
            ("SYSLOG_IDENTIFIER", "app-a"),
            ("SYSLOG_PID", "9002"),
            ("PRIORITY", "5"),
            ("SYSLOG_FACILITY", "3"),
            ("_TRANSPORT", "syslog"),
            ("_HOSTNAME", "fixture-host-b"),
            ("_SYSTEMD_UNIT", "alpha.service"),
            ("_SYSTEMD_INVOCATION_ID", "22222222222222222222222222222222"),
        ),
    ),
    Row(
        "multi-boot-file.journal",
        BOOT_B,
        "file-B",
        1_700_004_100_001_000,
        (
            ("SYSLOG_IDENTIFIER", "app-b"),
            ("PRIORITY", "4"),
            ("SYSLOG_FACILITY", "16"),
            ("_TRANSPORT", "syslog"),
            ("_SYSTEMD_USER_UNIT", "user-alpha.service"),
            ("USER_UNIT", "user-manager-alpha.service"),
            ("OBJECT_SYSTEMD_USER_UNIT", "user-object-alpha.service"),
            ("COREDUMP_USER_UNIT", "user-crash-alpha.service"),
            ("_SYSTEMD_USER_SLICE", "user-alpha.slice"),
            ("USER_INVOCATION_ID", "33333333333333333333333333333333"),
            ("_UID", HOST_UID),
        ),
    ),
    Row(
        "multi-boot-file.journal",
        BOOT_C,
        "file-c",
        1_700_004_100_002_000,
        (
            ("SYSLOG_IDENTIFIER", "app-a"),
            ("PRIORITY", "7"),
            ("SYSLOG_FACILITY", "3"),
            ("_TRANSPORT", "kernel"),
            ("_SYSTEMD_UNIT", "beta.service"),
            ("_SYSTEMD_INVOCATION_ID", "44444444444444444444444444444444"),
        ),
    ),
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
    output_special = FIXTURE_DIR / "output-special.journal"
    output_long = FIXTURE_DIR / "output-long.journal"
    no_message = FIXTURE_DIR / "no-message.journal"
    source_realtime = FIXTURE_DIR / "source-realtime.journal"
    pager = FIXTURE_DIR / "pager.journal"
    implicit_boot = FIXTURE_DIR / "implicit-boot.journal"
    follow = FIXTURE_DIR / "follow"
    directory.mkdir(parents=True)
    follow.mkdir(parents=True)

    for row in STATIC_ROWS:
        write_journal(directory / row.path, [row])
    write_journal(single_file, FILE_ROWS)
    write_output_special_journal(output_special)
    write_output_long_journal(output_long)
    write_no_message_journal(no_message)
    write_source_realtime_journal(source_realtime)
    write_pager_journal(pager)
    write_implicit_boot_journal(implicit_boot)
    return {
        "directory": directory,
        "file": single_file,
        "output_special": output_special,
        "output_long": output_long,
        "no_message": no_message,
        "source_realtime": source_realtime,
        "pager": pager,
        "implicit_boot": implicit_boot,
        "follow": follow,
    }


def archived_journal_name(seqnum: int, realtime_usec: int) -> str:
    return f"system@{VACUUM_SEQNUM_ID}-{seqnum:016x}-{realtime_usec:016x}.journal"


def make_vacuum_dir(path: Path, source: Path, include_active: bool = True) -> None:
    path.mkdir(parents=True, exist_ok=True)
    if include_active:
        shutil.copy2(source, path / "system.journal")
    for seqnum, realtime in (
        (1, 1_700_004_100_000_000),
        (2, 1_700_004_100_000_500),
        (3, 1_700_004_100_001_000),
    ):
        shutil.copy2(source, path / archived_journal_name(seqnum, realtime))
    (path / archived_journal_name(4, 1_700_004_100_001_500)).write_bytes(b"")
    (path / archived_journal_name(5, 1_700_004_100_002_000)).mkdir()
    try:
        (path / archived_journal_name(6, 1_700_004_100_002_500)).symlink_to(source)
    except (OSError, NotImplementedError):
        pass
    (path / "unknown.log").write_text("not a journal\n", encoding="utf-8")


def remaining_names(path: Path) -> list[str]:
    return sorted(child.name for child in path.iterdir())


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
                ]
                + list(row.fields),
            }
            for i, row in enumerate(rows, start=1)
        ],
    )


def write_output_special_journal(path: Path) -> None:
    write_journal_file(
        path,
        machine_id=MACHINE_ID,
        boot_id=BOOT_A,
        seqnum_id=VACUUM_SEQNUM_ID,
        entries=[
            {
                "realtime_usec": 1_700_005_000_000_000,
                "monotonic_usec": 1,
                "boot_id": BOOT_A,
                "fields": [
                    ("MESSAGE", b"hello\x00world"),
                    ("TEST_ID", "journalctl-output-special"),
                    ("_BOOT_ID", BOOT_A),
                    ("_MACHINE_ID", MACHINE_ID),
                    ("SYSLOG_IDENTIFIER", "special"),
                    ("LONG_FIELD", "L" * 500),
                    ("BINARY_FIELD", b"a\x00b"),
                ],
            },
            {
                "realtime_usec": 1_700_005_000_000_100,
                "monotonic_usec": 2,
                "boot_id": BOOT_A,
                "fields": [
                    ("MESSAGE", "first line\nsecond line"),
                    ("TEST_ID", "journalctl-output-newline"),
                    ("_BOOT_ID", BOOT_A),
                    ("_MACHINE_ID", MACHINE_ID),
                    ("SYSLOG_IDENTIFIER", "special"),
                ],
            },
            {
                "realtime_usec": 1_700_005_000_000_200,
                "monotonic_usec": 3,
                "boot_id": BOOT_A,
                "fields": [
                    ("MESSAGE", "caf\u00e9"),
                    ("TEST_ID", "journalctl-output-utf8"),
                    ("_BOOT_ID", BOOT_A),
                    ("_MACHINE_ID", MACHINE_ID),
                    ("SYSLOG_IDENTIFIER", "special"),
                    ("UTF8_FIELD", "caf\u00e9"),
                ],
            },
            {
                "realtime_usec": 1_699_185_600_000_000,
                "monotonic_usec": 4,
                "boot_id": BOOT_A,
                "fields": [
                    ("MESSAGE", "single-digit-day"),
                    ("TEST_ID", "journalctl-output-single-digit-day"),
                    ("_BOOT_ID", BOOT_A),
                    ("_MACHINE_ID", MACHINE_ID),
                    ("SYSLOG_IDENTIFIER", "special"),
                ],
            },
        ],
    )


def write_output_long_journal(path: Path) -> None:
    write_journal_file(
        path,
        machine_id=MACHINE_ID,
        boot_id=BOOT_A,
        seqnum_id=VACUUM_SEQNUM_ID,
        entries=[
            {
                "realtime_usec": 1_700_005_000_000_000,
                "monotonic_usec": 1,
                "boot_id": BOOT_A,
                "fields": [
                    ("MESSAGE", "M" * 500),
                    ("TEST_ID", "journalctl-output-long"),
                    ("_BOOT_ID", BOOT_A),
                    ("_MACHINE_ID", MACHINE_ID),
                    ("SYSLOG_IDENTIFIER", "special"),
                    ("HUGE_FIELD", "H" * 5000),
                ],
            }
        ],
    )


def write_no_message_journal(path: Path) -> None:
    write_journal_file(
        path,
        machine_id=MACHINE_ID,
        boot_id=BOOT_A,
        seqnum_id=VACUUM_SEQNUM_ID,
        entries=[
            {
                "realtime_usec": 1_700_004_100_000_000,
                "monotonic_usec": 1,
                "boot_id": BOOT_A,
                "fields": [
                    ("TEST_ID", "journalctl-no-message"),
                    ("_BOOT_ID", BOOT_A),
                    ("_MACHINE_ID", MACHINE_ID),
                    ("SYSLOG_IDENTIFIER", "app"),
                ],
            }
        ],
    )


def write_source_realtime_journal(path: Path) -> None:
    write_journal_file(
        path,
        machine_id=MACHINE_ID,
        boot_id=BOOT_A,
        seqnum_id=VACUUM_SEQNUM_ID,
        entries=[
            {
                "realtime_usec": 1_700_004_100_000_000,
                "monotonic_usec": 1,
                "boot_id": BOOT_A,
                "fields": [
                    ("MESSAGE", "source-realtime-test"),
                    ("TEST_ID", "journalctl-source-realtime"),
                    ("_BOOT_ID", BOOT_A),
                    ("_MACHINE_ID", MACHINE_ID),
                    ("SYSLOG_IDENTIFIER", "app"),
                    ("_SOURCE_REALTIME_TIMESTAMP", "1700004100005000"),
                ],
            }
        ],
    )


def write_pager_journal(path: Path) -> None:
    write_journal_file(
        path,
        machine_id=MACHINE_ID,
        boot_id=BOOT_A,
        seqnum_id=VACUUM_SEQNUM_ID,
        entries=[
            {
                "realtime_usec": 1_700_006_000_000_000 + i,
                "monotonic_usec": i + 1,
                "boot_id": BOOT_A,
                "fields": [
                    ("MESSAGE", f"pager-{i:04d}"),
                    ("TEST_ID", "journalctl-pager-end"),
                    ("_BOOT_ID", BOOT_A),
                    ("_MACHINE_ID", MACHINE_ID),
                ],
            }
            for i in range(1005)
        ],
    )


def write_implicit_boot_journal(path: Path) -> None:
    entries = []
    for idx, (boot_id, message) in enumerate(((BOOT_A, "implicit-old"), (BOOT_C, "implicit-current")), start=1):
        entries.append(
            {
                "realtime_usec": 1_700_006_100_000_000 + idx,
                "monotonic_usec": idx,
                "boot_id": boot_id,
                "fields": [
                    ("MESSAGE", message),
                    ("TEST_ID", "journalctl-implicit-boot"),
                    ("_TRANSPORT", "kernel"),
                    ("_BOOT_ID", boot_id),
                    ("_MACHINE_ID", MACHINE_ID),
                ],
            }
        )
    write_journal_file(
        path,
        machine_id=MACHINE_ID,
        boot_id=BOOT_C,
        seqnum_id=VACUUM_SEQNUM_ID,
        entries=entries,
    )


def row_entry(message: str, realtime: int, monotonic: int, test_id: str, boot_id: str = BOOT_A) -> dict[str, object]:
    return {
        "realtime_usec": realtime,
        "monotonic_usec": monotonic,
        "boot_id": boot_id,
        "fields": [
            ("MESSAGE", message),
            ("TEST_ID", test_id),
            ("_BOOT_ID", boot_id),
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


def raw_reader_command(reader: str, tools: dict[str, str], mode: str, path: Path, args: list[str]) -> list[str]:
    if reader == "stock":
        base = ["journalctl", f"--{mode}", str(path), "--no-pager"]
    elif reader == "go":
        base = [tools["go_journalctl"], f"--{mode}", str(path)]
    elif reader == "rust":
        base = [tools["rust_journalctl"], f"--{mode}", str(path)]
    else:
        raise ValueError(reader)
    return [*base, *args]


def action_command(reader: str, tools: dict[str, str], args: list[str]) -> list[str]:
    if reader == "stock":
        return ["journalctl", "--no-pager", "--quiet", *args]
    if reader == "go":
        return [tools["go_journalctl"], "--quiet", *args]
    if reader == "rust":
        return [tools["rust_journalctl"], "--quiet", *args]
    raise ValueError(reader)


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


def parse_cursors(output: str) -> list[str]:
    cursors = []
    for line in output.splitlines():
        if not line.startswith("{"):
            continue
        obj = json.loads(line)
        cursor = obj.get("__CURSOR")
        if isinstance(cursor, str) and cursor:
            cursors.append(cursor)
    return cursors


def parse_json_output(mode: str, output: str) -> list[object]:
    if mode == "json":
        return [json.loads(line) for line in output.splitlines() if line.strip()]
    if mode == "json-pretty":
        decoder = json.JSONDecoder()
        values: list[object] = []
        index = 0
        while index < len(output):
            while index < len(output) and output[index].isspace():
                index += 1
            if index >= len(output):
                break
            value, index = decoder.raw_decode(output, index)
            values.append(value)
        return values
    if mode == "json-sse":
        values = []
        for block in output.split("\n\n"):
            payload_lines = [
                line[len("data: ") :]
                for line in block.splitlines()
                if line.startswith("data: ")
            ]
            if payload_lines:
                values.append(json.loads("\n".join(payload_lines)))
        return values
    if mode == "json-seq":
        values = []
        for record in output.split("\x1e"):
            record = record.strip()
            if record:
                values.append(json.loads(record))
        return values
    raise ValueError(f"not a JSON output mode: {mode}")


def json_pretty_style_ok(output: str) -> bool:
    saw_key_line = False
    saw_tab_indent = False
    for line in output.splitlines():
        if not line:
            continue
        if line.startswith(" "):
            return False
        stripped = line.lstrip("\t")
        if len(stripped) != len(line):
            saw_tab_indent = True
        if stripped.startswith('"'):
            saw_key_line = True
            if '" : ' not in stripped or '":' in stripped:
                return False
    return saw_key_line and saw_tab_indent


def valid_new_id128_output(output: str) -> bool:
    match = NEW_ID128_RE.match(output)
    if not match:
        return False
    simple, uuid_text, macro_bytes, python_simple = match.groups()
    return (
        uuid_text.replace("-", "") == simple
        and macro_bytes.replace(",", "") == simple
        and python_simple == simple
    )


def run_static_cases(tools: dict[str, str], fixtures: dict[str, Path]) -> list[dict[str, object]]:
    cursor_probe = run(
        reader_command(
            "stock",
            tools,
            "file",
            fixtures["file"],
            ["--boot=all", "TEST_ID=journalctl-query"],
        ),
        timeout=30,
    )
    require_ok(cursor_probe, "stock cursor probe")
    file_cursors = parse_cursors(cursor_probe.stdout)
    if len(file_cursors) != len(FILE_ROWS):
        raise RuntimeError(f"expected {len(FILE_ROWS)} file cursors, got {len(file_cursors)}")

    cases = [
        ("directory-all", "directory", fixtures["directory"], ["TEST_ID=journalctl-query"]),
        ("directory-reverse", "directory", fixtures["directory"], ["--reverse", "TEST_ID=journalctl-query"]),
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
        ("file-system-noop", "file", fixtures["file"], ["--system", "TEST_ID=journalctl-query"]),
        ("file-user-noop", "file", fixtures["file"], ["--user", "TEST_ID=journalctl-query"]),
        ("file-reverse", "file", fixtures["file"], ["--reverse", "TEST_ID=journalctl-query"]),
        ("file-lines-tail", "file", fixtures["file"], ["--lines=2", "TEST_ID=journalctl-query"]),
        ("file-lines-zero", "file", fixtures["file"], ["--lines=0", "TEST_ID=journalctl-query"]),
        ("file-lines-reverse", "file", fixtures["file"], ["--reverse", "--lines=2", "--boot=all", "TEST_ID=journalctl-query"]),
        ("file-lines-oldest", "file", fixtures["file"], ["--lines=+2", "TEST_ID=journalctl-query"]),
        ("file-lines-default", "file", fixtures["file"], ["--lines", "TEST_ID=journalctl-query"]),
        ("file-short-attached-lines", "file", fixtures["file"], ["-n2", "--boot=all", "TEST_ID=journalctl-query"]),
        ("file-short-cluster-reverse-lines", "file", fixtures["file"], ["-rn2", "-ball", "TEST_ID=journalctl-query"]),
        ("file-pager-end-default-lines", "file", fixtures["pager"], ["--pager-end", "TEST_ID=journalctl-pager-end"]),
        ("file-pager-end-implicit-boot", "file", fixtures["implicit_boot"], ["--pager-end", "TEST_ID=journalctl-implicit-boot"]),
        (
            "file-pager-end-merge-suppresses-implicit-boot",
            "file",
            fixtures["implicit_boot"],
            ["--pager-end", "--merge", "TEST_ID=journalctl-implicit-boot"],
        ),
        ("file-show-cursor", "file", fixtures["file"], ["--show-cursor", "TEST_ID=journalctl-query"]),
        ("file-cursor-first", "file", fixtures["file"], ["--cursor", file_cursors[0], "--boot=all", "TEST_ID=journalctl-query"]),
        (
            "file-after-cursor-first",
            "file",
            fixtures["file"],
            ["--after-cursor", file_cursors[0], "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-after-cursor-filtered-first",
            "file",
            fixtures["file"],
            ["--after-cursor", file_cursors[0], "--identifier=app-b", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-after-cursor-partial-seqnum",
            "file",
            fixtures["file"],
            [f"--after-cursor=s={VACUUM_SEQNUM_ID};i=999", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        ("file-boot-latest", "file", fixtures["file"], ["--boot=0", "TEST_ID=journalctl-query"]),
        ("file-this-boot", "file", fixtures["file"], ["--this-boot", "TEST_ID=journalctl-query"]),
        ("file-boot-first", "file", fixtures["file"], ["--boot=1", "TEST_ID=journalctl-query"]),
        (
            "file-since-until",
            "file",
            fixtures["file"],
            ["--since", "@1700004100.000001", "--until", "@1700004100.001", "TEST_ID=journalctl-query"],
        ),
        (
            "file-since-relative-ago",
            "file",
            fixtures["file"],
            ["--since", "1 hour ago", "TEST_ID=journalctl-query"],
        ),
        (
            "file-since-relative-month-ago",
            "file",
            fixtures["file"],
            ["--since", "1 month ago", "TEST_ID=journalctl-query"],
        ),
        (
            "file-since-relative-capital-month-ago",
            "file",
            fixtures["file"],
            ["--since", "1 M ago", "TEST_ID=journalctl-query"],
        ),
        (
            "file-since-relative-year-ago",
            "file",
            fixtures["file"],
            ["--since", "1 year ago", "TEST_ID=journalctl-query"],
        ),
        (
            "file-since-signed-four-digit-hours",
            "file",
            fixtures["file"],
            ["--since=+2025h", "TEST_ID=journalctl-query"],
        ),
        (
            "file-since-signed-four-digit-minutes",
            "file",
            fixtures["file"],
            ["--since=-2025m", "TEST_ID=journalctl-query"],
        ),
        (
            "file-until-signed-four-digit-bare",
            "file",
            fixtures["file"],
            ["--until=+2025", "TEST_ID=journalctl-query"],
        ),
        (
            "file-synchronize-on-exit-zero-noop",
            "file",
            fixtures["file"],
            ["--synchronize-on-exit=0", "TEST_ID=journalctl-query"],
        ),
        (
            "file-synchronize-on-exit-f-noop",
            "file",
            fixtures["file"],
            ["--synchronize-on-exit=f", "TEST_ID=journalctl-query"],
        ),
        (
            "file-synchronize-on-exit-off-noop",
            "file",
            fixtures["file"],
            ["--synchronize-on-exit=off", "TEST_ID=journalctl-query"],
        ),
        ("file-identifier", "file", fixtures["file"], ["--identifier=app-a", "--boot=all", "TEST_ID=journalctl-query"]),
        (
            "file-identifier-or",
            "file",
            fixtures["file"],
            ["--identifier=app-a", "--identifier=app-b", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-exclude-identifier",
            "file",
            fixtures["file"],
            ["--exclude-identifier=app-a", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        ("file-priority-named", "file", fixtures["file"], ["--priority=err", "--boot=all", "TEST_ID=journalctl-query"]),
        (
            "file-priority-range",
            "file",
            fixtures["file"],
            ["--priority=err..warning", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        ("file-facility", "file", fixtures["file"], ["--facility=daemon", "--boot=all", "TEST_ID=journalctl-query"]),
        (
            "file-facility-list",
            "file",
            fixtures["file"],
            ["--facility=daemon,local0", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        ("file-grep-auto", "file", fixtures["file"], ["--grep=file-b", "--boot=all", "TEST_ID=journalctl-query"]),
        ("file-grep-tail-reverse", "file", fixtures["file"], ["--grep=file", "--lines=2", "--boot=all", "TEST_ID=journalctl-query"]),
        (
            "file-interspersed-lines",
            "file",
            fixtures["file"],
            ["TEST_ID=journalctl-query", "--lines=2", "--boot=all"],
        ),
        (
            "file-plus-disjunction",
            "file",
            fixtures["file"],
            [
                "TEST_ID=journalctl-query",
                "SYSLOG_IDENTIFIER=app-b",
                "+",
                "TEST_ID=journalctl-query",
                "MESSAGE=file-c",
                "--boot=all",
            ],
        ),
        (
            "file-grep-case-insensitive",
            "file",
            fixtures["file"],
            ["--grep=FILE-B", "--case-sensitive=false", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-grep-case-sensitive",
            "file",
            fixtures["file"],
            ["--grep=file-B", "--case-sensitive=true", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-grep-case-sensitive-t",
            "file",
            fixtures["file"],
            ["--grep=file-B", "--case-sensitive=t", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-grep-case-insensitive-f",
            "file",
            fixtures["file"],
            ["--grep=FILE-B", "--case-sensitive=f", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        ("file-dmesg", "file", fixtures["file"], ["--dmesg", "--boot=all", "TEST_ID=journalctl-query"]),
        ("file-dmesg-implicit-boot", "file", fixtures["implicit_boot"], ["--dmesg", "TEST_ID=journalctl-implicit-boot"]),
        (
            "file-dmesg-merge-suppresses-implicit-boot",
            "file",
            fixtures["implicit_boot"],
            ["--dmesg", "--merge", "TEST_ID=journalctl-implicit-boot"],
        ),
        ("file-unit-direct", "file", fixtures["file"], ["--unit=alpha.service", "--boot=all", "TEST_ID=journalctl-query"]),
        ("file-unit-short-mangled", "file", fixtures["file"], ["-u", "alpha", "--boot=all", "TEST_ID=journalctl-query"]),
        ("file-unit-manager", "file", fixtures["file"], ["--unit=manager-alpha.service", "--boot=all", "TEST_ID=journalctl-query"]),
        ("file-unit-object", "file", fixtures["file"], ["--unit=object-alpha.service", "--boot=all", "TEST_ID=journalctl-query"]),
        ("file-unit-coredump", "file", fixtures["file"], ["--unit=crash-alpha.service", "--boot=all", "TEST_ID=journalctl-query"]),
        ("file-unit-slice", "file", fixtures["file"], ["--unit=app-alpha.slice", "--boot=all", "TEST_ID=journalctl-query"]),
        ("file-unit-glob", "file", fixtures["file"], ["--unit=*.service", "--boot=all", "TEST_ID=journalctl-query"]),
        (
            "file-user-unit-direct",
            "file",
            fixtures["file"],
            ["--user-unit=user-alpha.service", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-user-unit-manager",
            "file",
            fixtures["file"],
            ["--user-unit=user-manager-alpha.service", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-user-unit-object",
            "file",
            fixtures["file"],
            ["--user-unit=user-object-alpha.service", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-user-unit-coredump",
            "file",
            fixtures["file"],
            ["--user-unit=user-crash-alpha.service", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-user-unit-slice",
            "file",
            fixtures["file"],
            ["--user-unit=user-alpha.slice", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        ("file-user-unit-glob", "file", fixtures["file"], ["--user-unit=user-*", "--boot=all", "TEST_ID=journalctl-query"]),
        (
            "file-user-plus-unit-rewrite",
            "file",
            fixtures["file"],
            ["--user", "--unit=user-alpha.service", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-invocation-explicit",
            "file",
            fixtures["file"],
            ["--invocation=11111111111111111111111111111111", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-invocation-latest-unit",
            "file",
            fixtures["file"],
            ["-I", "--unit=alpha.service", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-invocation-all-unit",
            "file",
            fixtures["file"],
            ["--invocation=all", "--unit=alpha.service", "--boot=all", "TEST_ID=journalctl-query"],
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
            expect_cursor = "--show-cursor" in args
            cursor_ok = not expect_cursor or "-- cursor:" in result.stdout
            ok = result.returncode == 0 and actual == expected and cursor_ok
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected": expected,
                    "actual": actual,
                    "cursor_present": "-- cursor:" in result.stdout,
                    "cursor_required": expect_cursor,
                    "returncode": result.returncode,
                    "stderr": result.stderr[-1000:],
                }
            )

    raw_exact_cases = [
        (
            "file-exclude-identifier-short",
            "file",
            fixtures["file"],
            ["--exclude-identifier=app-a", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-exclude-identifier-lines-short",
            "file",
            fixtures["file"],
            ["--exclude-identifier=app-a", "--lines=2", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-exclude-identifier-reverse-lines-short",
            "file",
            fixtures["file"],
            ["--exclude-identifier=app-a", "--reverse", "--lines=2", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-lines-zero-short",
            "file",
            fixtures["file"],
            ["--lines=0", "TEST_ID=journalctl-query"],
        ),
        (
            "file-cursor-lines-zero-short",
            "file",
            fixtures["file"],
            ["--cursor", file_cursors[0], "--lines=0", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "file-grep-reverse-lines-boot-separators-short",
            "file",
            fixtures["file"],
            ["--grep=file-a", "--reverse", "--lines=2", "--boot=all", "TEST_ID=journalctl-query"],
        ),
    ]
    for case_name, mode, path, args in raw_exact_cases:
        stock = run(raw_reader_command("stock", tools, mode, path, args), timeout=30)
        require_ok(stock, f"stock {case_name}")
        expected = stock.stdout
        for reader in READERS:
            cmd = raw_reader_command(reader, tools, mode, path, args)
            result = run(cmd, timeout=30)
            ok = result.returncode == 0 and result.stdout == expected
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected": expected,
                    "actual": result.stdout,
                    "returncode": result.returncode,
                    "stderr": result.stderr[-1000:],
                }
            )

    directory_file_a = fixtures["directory"] / "a.journal"
    directory_file_b = fixtures["directory"] / "b.journal"
    multi_file_cases = [
        (
            "multi-file-repeat",
            ["--file", str(directory_file_a), "--file", str(directory_file_b), "--output=json", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "multi-file-short-repeat",
            ["-i", str(directory_file_a), "-i", str(directory_file_b), "--output=json", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "multi-file-glob",
            ["--file", str(fixtures["directory"] / "*.journal"), "--output=json", "--boot=all", "TEST_ID=journalctl-query"],
        ),
        (
            "multi-file-duplicate-same-file",
            ["--file", str(fixtures["file"]), "--file", str(fixtures["file"]), "--output=json", "--boot=all", "TEST_ID=journalctl-query"],
        ),
    ]
    for case_name, args in multi_file_cases:
        stock = run(action_command("stock", tools, args), timeout=30)
        require_ok(stock, f"stock {case_name}")
        expected = parse_messages(stock.stdout)
        for reader in READERS:
            cmd = action_command(reader, tools, args)
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

    no_match_args = [
        "--file",
        str(fixtures["directory"] / "does-not-match-*.journal"),
        "--output=json",
        "--boot=all",
        "TEST_ID=journalctl-query",
    ]
    for reader in READERS:
        cmd = action_command(reader, tools, no_match_args)
        result = run(cmd, timeout=30)
        ok = result.returncode != 0
        results.append(
            {
                "test": "multi-file-glob-no-match-preserved",
                "reader": reader,
                "status": "PASS" if ok else "FAIL",
                "command": " ".join(cmd),
                "stdout": result.stdout[-1000:],
                "stderr": result.stderr[-1000:],
                "returncode": result.returncode,
            }
        )
    return results


def run_cursor_file_cases(tools: dict[str, str], fixtures: dict[str, Path]) -> list[dict[str, object]]:
    cursor_probe = run(
        reader_command(
            "stock",
            tools,
            "file",
            fixtures["file"],
            ["--boot=all", "TEST_ID=journalctl-query"],
        ),
        timeout=30,
    )
    require_ok(cursor_probe, "stock cursor-file probe")
    file_cursors = parse_cursors(cursor_probe.stdout)
    if len(file_cursors) != len(FILE_ROWS):
        raise RuntimeError(f"expected {len(FILE_ROWS)} file cursors, got {len(file_cursors)}")

    cases = [
        ("file-cursor-file-existing", file_cursors[0]),
        ("file-cursor-file-missing", None),
    ]
    results: list[dict[str, object]] = []
    for case_name, initial_cursor in cases:
        expected_messages: list[str] | None = None
        expected_cursor: str | None = None
        for reader in READERS:
            cursor_file = FIXTURE_DIR / f"{case_name}-{reader}.cursor"
            if cursor_file.exists():
                cursor_file.unlink()
            if initial_cursor is not None:
                cursor_file.write_text(initial_cursor, encoding="utf-8")
            cmd = reader_command(
                reader,
                tools,
                "file",
                fixtures["file"],
                ["--cursor-file", str(cursor_file), "--boot=all", "TEST_ID=journalctl-query"],
            )
            result = run(cmd, timeout=30)
            actual = parse_messages(result.stdout)
            written_cursor = cursor_file.read_text(encoding="utf-8") if cursor_file.exists() else ""
            if reader == "stock":
                expected_messages = actual
                expected_cursor = written_cursor
                ok = result.returncode == 0 and bool(written_cursor)
            else:
                ok = (
                    result.returncode == 0
                    and actual == expected_messages
                    and written_cursor == expected_cursor
                    and bool(written_cursor)
                )
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected": expected_messages,
                    "actual": actual,
                    "expected_cursor": expected_cursor,
                    "actual_cursor": written_cursor,
                    "returncode": result.returncode,
                    "stderr": result.stderr[-1000:],
                }
            )
    return results


def run_portable_error_cases(tools: dict[str, str], fixtures: dict[str, Path]) -> list[dict[str, object]]:
    cases = [
        (
            "file-path-match-unsupported",
            "file",
            fixtures["file"],
            ["./some/path"],
            "journalctl portable mode does not support path match argument",
        ),
        (
            "directory-rotate-vacuum-unsupported",
            "directory",
            fixtures["directory"],
            ["--rotate", "--vacuum-size=1G"],
            "journalctl portable mode does not support --rotate",
        ),
    ]

    results: list[dict[str, object]] = []
    for case_name, mode, path, args, expected_error in cases:
        for reader in ("go", "rust"):
            cmd = reader_command(reader, tools, mode, path, args)
            result = run(cmd, timeout=30)
            combined = result.stdout + result.stderr
            ok = result.returncode != 0 and expected_error in combined
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected_error": expected_error,
                    "stdout": result.stdout[-1000:],
                    "stderr": result.stderr[-1000:],
                    "returncode": result.returncode,
                }
            )
    action_error_cases = [
        (
            "file-stdin-unsupported",
            ["--file=-", "--output=json"],
            "journalctl portable mode does not support --file=-",
        ),
        (
            "default-source-unsupported",
            ["--output=json", "TEST_ID=journalctl-query"],
            "journalctl portable mode does not support default journal source",
        ),
    ]
    for case_name, args, expected_error in action_error_cases:
        for reader in ("go", "rust"):
            cmd = action_command(reader, tools, args)
            result = run(cmd, timeout=30)
            combined = result.stdout + result.stderr
            ok = result.returncode != 0 and expected_error in combined
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected_error": expected_error,
                    "stdout": result.stdout[-1000:],
                    "stderr": result.stderr[-1000:],
                    "returncode": result.returncode,
                }
            )

    missing_follow_file = fixtures["directory"] / "missing-follow.journal"
    for reader in ("go", "rust"):
        cmd = action_command(reader, tools, ["--follow", "--file", str(missing_follow_file), "--output=json"])
        try:
            result = run(cmd, timeout=5)
            combined = (result.stdout + result.stderr).lower()
            ok = result.returncode != 0 and ("open" in combined or "no such file" in combined)
            stdout = result.stdout[-1000:]
            stderr = result.stderr[-1000:]
            returncode = result.returncode
        except subprocess.TimeoutExpired:
            ok = False
            stdout = ""
            stderr = "timeout waiting for --follow on a missing journal file"
            returncode = -1
        results.append(
            {
                "test": "follow-missing-file-errors",
                "reader": reader,
                "status": "PASS" if ok else "FAIL",
                "command": " ".join(cmd),
                "expected_error": "open failure",
                "stdout": stdout,
                "stderr": stderr,
                "returncode": returncode,
            }
        )

    stock_error_cases = [
        (
            "explicit-empty-boot",
            ["--file", str(fixtures["file"]), "--boot="],
            "failed to parse boot descriptor",
        ),
        (
            "explicit-true-boot",
            ["--file", str(fixtures["file"]), "--boot=true"],
            "failed to parse boot descriptor",
        ),
        (
            "short-explicit-true-boot",
            ["--file", str(fixtures["file"]), "-b=true"],
            "failed to parse boot descriptor",
        ),
        (
            "explicit-empty-lines",
            ["--file", str(fixtures["file"]), "--lines="],
            "failed to parse --lines",
        ),
        (
            "short-invalid-equals-lines",
            ["--file", str(fixtures["file"]), "-n=2"],
            "failed to parse --lines",
        ),
        (
            "negative-lines",
            ["--file", str(fixtures["file"]), "--lines=-2"],
            "failed to parse --lines",
        ),
        (
            "explicit-empty-case-sensitive",
            ["--file", str(fixtures["file"]), "--grep=file", "--case-sensitive="],
            "case-sensitive",
        ),
        (
            "explicit-empty-case-sensitive-no-grep",
            ["--file", str(fixtures["file"]), "--case-sensitive="],
            "case-sensitive",
        ),
        (
            "invalid-output-mode",
            ["--file", str(fixtures["file"]), "--output=jzon"],
            "output",
        ),
        (
            "invalid-cursor-lines-zero",
            ["--file", str(fixtures["file"]), "--cursor", "invalid", "--lines=0"],
            "cursor",
        ),
        (
            "explicit-empty-synchronize-on-exit",
            ["--file", str(fixtures["file"]), "--synchronize-on-exit="],
            "synchronize-on-exit",
        ),
    ]
    for case_name, args, expected_error in stock_error_cases:
        stock = run(action_command("stock", tools, args), timeout=30)
        stock_combined = (stock.stdout + stock.stderr).lower()
        if stock.returncode == 0 or expected_error not in stock_combined:
            raise AssertionError(f"stock {case_name} did not produce {expected_error!r}: {stock}")
        for reader in ("go", "rust"):
            cmd = action_command(reader, tools, args)
            result = run(cmd, timeout=30)
            combined = (result.stdout + result.stderr).lower()
            ok = result.returncode != 0 and expected_error in combined
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected_error": expected_error,
                    "stdout": result.stdout[-1000:],
                    "stderr": result.stderr[-1000:],
                    "returncode": result.returncode,
                }
            )
    return results


def run_utility_action_cases(tools: dict[str, str], fixtures: dict[str, Path]) -> list[dict[str, object]]:
    results: list[dict[str, object]] = []

    for reader in READERS:
        cmd = action_command(reader, tools, ["--new-id128"])
        result = run(cmd, timeout=30)
        ok = result.returncode == 0 and valid_new_id128_output(result.stdout)
        results.append(
            {
                "test": "new-id128",
                "reader": reader,
                "status": "PASS" if ok else "FAIL",
                "command": " ".join(cmd),
                "stdout": result.stdout[-1000:],
                "stderr": result.stderr[-1000:],
                "returncode": result.returncode,
            }
        )

    stock_output_help = run(action_command("stock", tools, ["--output=help"]), timeout=30)
    require_ok(stock_output_help, "stock output-help")
    for reader in READERS:
        cmd = action_command(reader, tools, ["--output=help"])
        result = run(cmd, timeout=30)
        ok = result.returncode == 0 and result.stdout == stock_output_help.stdout
        results.append(
            {
                "test": "output-help",
                "reader": reader,
                "status": "PASS" if ok else "FAIL",
                "command": " ".join(cmd),
                "expected": stock_output_help.stdout,
                "actual": result.stdout,
                "stderr": result.stderr[-1000:],
                "returncode": result.returncode,
            }
        )

    extraneous_action_cases = [
        ("action-extra-new-id128", ["--new-id128", "foo"], "foo"),
        ("action-extra-fields", ["--file", str(fixtures["file"]), "--fields", "TEST_ID=journalctl-query"], "TEST_ID=journalctl-query"),
        (
            "action-extra-field",
            ["--file", str(fixtures["file"]), "--field=MESSAGE", "TEST_ID=journalctl-query"],
            "TEST_ID=journalctl-query",
        ),
        (
            "action-extra-verify",
            ["--file", str(fixtures["file"]), "--verify", "TEST_ID=journalctl-query"],
            "TEST_ID=journalctl-query",
        ),
        (
            "action-extra-disk-usage",
            ["--file", str(fixtures["file"]), "--disk-usage", "TEST_ID=journalctl-query"],
            "TEST_ID=journalctl-query",
        ),
        ("action-extra-sync", ["--sync", "foo"], "foo"),
    ]
    for case_name, args, token in extraneous_action_cases:
        expected_error = f"Extraneous arguments starting with '{token}'"
        stock = run(action_command("stock", tools, args), timeout=30)
        if stock.returncode == 0 or expected_error not in (stock.stdout + stock.stderr):
            raise AssertionError(f"stock {case_name} did not produce {expected_error!r}: {stock}")
        for reader in ("go", "rust"):
            cmd = action_command(reader, tools, args)
            result = run(cmd, timeout=30)
            combined = result.stdout + result.stderr
            ok = result.returncode != 0 and expected_error in combined
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected_error": expected_error,
                    "stdout": result.stdout[-1000:],
                    "stderr": result.stderr[-1000:],
                    "returncode": result.returncode,
                }
            )

    for case_name, mode, path in (
        ("disk-usage-file", "file", fixtures["file"]),
        ("disk-usage-directory", "directory", fixtures["directory"]),
    ):
        stock = run(action_command("stock", tools, [f"--{mode}", str(path), "--disk-usage"]), timeout=30)
        require_ok(stock, f"stock {case_name}")
        expected = stock.stdout
        for reader in READERS:
            cmd = action_command(reader, tools, [f"--{mode}", str(path), "--disk-usage"])
            result = run(cmd, timeout=30)
            ok = result.returncode == 0 and result.stdout == expected
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected": expected,
                    "actual": result.stdout,
                    "stderr": result.stderr[-1000:],
                    "returncode": result.returncode,
                }
            )

    field_set_cases = [
        ("fields-file", ["--file", str(fixtures["file"]), "--fields"]),
    ]
    for case_name, args in field_set_cases:
        stock = run(action_command("stock", tools, args), timeout=30)
        require_ok(stock, f"stock {case_name}")
        expected = sorted(line for line in stock.stdout.splitlines() if line)
        for reader in ("go", "rust"):
            cmd = action_command(reader, tools, args)
            result = run(cmd, timeout=30)
            actual = sorted(line for line in result.stdout.splitlines() if line)
            ok = result.returncode == 0 and actual == expected
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected": expected,
                    "actual": actual,
                    "stderr": result.stderr[-1000:],
                    "returncode": result.returncode,
                }
            )

    exact_action_cases = [
        ("field-message-file", ["--file", str(fixtures["file"]), "--field=MESSAGE"]),
        ("header-file", ["--file", str(fixtures["file"]), "--header"]),
        ("list-boots-file", ["--file", str(fixtures["file"]), "--list-boots"]),
        ("list-boots-file-tail", ["--file", str(fixtures["file"]), "--list-boots", "--lines=2"]),
        ("list-boots-file-head", ["--file", str(fixtures["file"]), "--list-boots", "--lines=+2"]),
        ("list-boots-file-reverse", ["--file", str(fixtures["file"]), "--list-boots", "--reverse"]),
        (
            "list-invocations-alpha",
            ["--file", str(fixtures["file"]), "--list-invocations", "--unit=alpha.service"],
        ),
        (
            "list-invocations-alpha-tail",
            ["--file", str(fixtures["file"]), "--list-invocations", "--unit=alpha.service", "--lines=1"],
        ),
        (
            "list-invocations-alpha-head",
            ["--file", str(fixtures["file"]), "--list-invocations", "--unit=alpha.service", "--lines=+1"],
        ),
    ]
    for case_name, args in exact_action_cases:
        stock = run(action_command("stock", tools, args), timeout=30)
        require_ok(stock, f"stock {case_name}")
        expected = stock.stdout
        for reader in ("go", "rust"):
            cmd = action_command(reader, tools, args)
            result = run(cmd, timeout=30)
            ok = result.returncode == 0 and result.stdout == expected
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected": expected,
                    "actual": result.stdout,
                    "stderr": result.stderr[-1000:],
                    "returncode": result.returncode,
                }
            )

    vacuum_root = fixtures["directory"].parent / "vacuum"
    vacuum_cases = [
        ("vacuum-files-protect-active", ["--vacuum-files=2"], True),
        ("vacuum-time-protect-active", ["--vacuum-time=1s"], True),
        ("vacuum-time-month-protect-active", ["--vacuum-time=1 month"], True),
        ("vacuum-time-capital-month-protect-active", ["--vacuum-time=1M"], True),
        ("vacuum-size-protect-active", ["--vacuum-size=1"], True),
    ]
    for case_name, vacuum_args, include_active in vacuum_cases:
        stock_dir = vacuum_root / case_name / "stock"
        make_vacuum_dir(stock_dir, fixtures["file"], include_active=include_active)
        stock_args = ["--directory", str(stock_dir), *vacuum_args]
        stock = run(action_command("stock", tools, stock_args), timeout=30)
        require_ok(stock, f"stock {case_name}")
        expected = remaining_names(stock_dir)

        for reader in ("go", "rust"):
            reader_dir = vacuum_root / case_name / reader
            make_vacuum_dir(reader_dir, fixtures["file"], include_active=include_active)
            args = ["--directory", str(reader_dir), *vacuum_args]
            cmd = action_command(reader, tools, args)
            result = run(cmd, timeout=30)
            actual = remaining_names(reader_dir)
            ok = result.returncode == 0 and actual == expected
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected": expected,
                    "actual": actual,
                    "stdout": result.stdout[-1000:],
                    "stderr": result.stderr[-1000:],
                    "returncode": result.returncode,
                }
            )

    return results


def run_output_mode_cases(tools: dict[str, str], fixtures: dict[str, Path]) -> list[dict[str, object]]:
    base_args = ["--boot=all", "TEST_ID=journalctl-query"]
    text_cases = [
        ("output-short", ["--output=short", *base_args], "exact"),
        ("output-short-quiet", ["--quiet", "--output=short", *base_args], "exact"),
        ("output-short-no-hostname", ["--no-hostname", "--output=short", *base_args], "exact"),
        ("output-short-full", ["--output=short-full", *base_args], "exact"),
        ("output-short-full-utc", ["--utc", "--output=short-full", *base_args], "exact"),
        ("output-short-iso", ["--output=short-iso", *base_args], "exact"),
        ("output-short-iso-utc", ["--utc", "--output=short-iso", *base_args], "exact"),
        ("output-short-iso-precise", ["--output=short-iso-precise", *base_args], "exact"),
        ("output-short-iso-precise-utc", ["--utc", "--output=short-iso-precise", *base_args], "exact"),
        ("output-short-precise", ["--output=short-precise", *base_args], "exact"),
        ("output-short-monotonic", ["--output=short-monotonic", *base_args], "exact"),
        ("output-short-delta", ["--output=short-delta", *base_args], "exact"),
        ("output-short-unix", ["--output=short-unix", *base_args], "exact"),
        ("output-with-unit", ["--output=with-unit", *base_args], "exact"),
        ("output-cat", ["--output=cat", *base_args], "exact"),
        (
            "output-cat-fields",
            ["--output=cat", "--output-fields=MESSAGE,PRIORITY", *base_args],
            "line-multiset",
        ),
        ("output-verbose-fields", ["--output=verbose", "--output-fields=MESSAGE,PRIORITY", *base_args], "exact"),
        ("output-export-fields", ["--output=export", "--output-fields=MESSAGE,PRIORITY", *base_args], "exact"),
        (
            "output-short-merge-suppresses-boot-separators",
            ["--merge", "--output=short", "--lines=5", "TEST_ID=journalctl-query"],
            "exact",
        ),
    ]
    results: list[dict[str, object]] = []
    for case_name, args, comparison in text_cases:
        stock_cmd = raw_reader_command("stock", tools, "file", fixtures["file"], args)
        stock = run(stock_cmd, timeout=30)
        require_ok(stock, f"stock {case_name}")
        expected = stock.stdout
        expected_cmp: object = sorted(expected.splitlines()) if comparison == "line-multiset" else expected
        for reader in READERS:
            cmd = raw_reader_command(reader, tools, "file", fixtures["file"], args)
            result = run(cmd, timeout=30)
            actual_cmp: object = sorted(result.stdout.splitlines()) if comparison == "line-multiset" else result.stdout
            ok = result.returncode == 0 and actual_cmp == expected_cmp
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected": expected,
                    "actual": result.stdout,
                    "comparison": comparison,
                    "returncode": result.returncode,
                    "stderr": result.stderr[-1000:],
                }
            )

    control_text_cases = [
        (
            "output-short-binary-default",
            fixtures["output_special"],
            ["--output=short", "--boot=all", "TEST_ID=journalctl-output-special"],
            "exact",
        ),
        (
            "output-short-binary-all",
            fixtures["output_special"],
            ["--all", "--output=short", "--boot=all", "TEST_ID=journalctl-output-special"],
            "exact",
        ),
        (
            "output-short-long-no-full",
            fixtures["output_long"],
            ["--no-full", "--output=short", "--boot=all", "TEST_ID=journalctl-output-long"],
            "exact",
        ),
        (
            "output-short-newline-default",
            fixtures["output_special"],
            ["--output=short", "--boot=all", "TEST_ID=journalctl-output-newline"],
            "exact",
        ),
        (
            "output-short-newline-truncate",
            fixtures["output_special"],
            ["--truncate-newline", "--output=short", "--boot=all", "TEST_ID=journalctl-output-newline"],
            "exact",
        ),
        (
            "output-short-single-digit-day",
            fixtures["output_special"],
            ["--output=short", "--boot=all", "TEST_ID=journalctl-output-single-digit-day"],
            "exact",
        ),
        (
            "output-short-precise-single-digit-day",
            fixtures["output_special"],
            ["--output=short-precise", "--boot=all", "TEST_ID=journalctl-output-single-digit-day"],
            "exact",
        ),
        (
            "output-short-no-message",
            fixtures["no_message"],
            ["--output=short", "--boot=all", "TEST_ID=journalctl-no-message"],
            "exact",
        ),
        (
            "output-with-unit-no-message",
            fixtures["no_message"],
            ["--output=with-unit", "--boot=all", "TEST_ID=journalctl-no-message"],
            "exact",
        ),
        (
            "output-short-precise-source-realtime",
            fixtures["source_realtime"],
            ["--output=short-precise", "--boot=all", "TEST_ID=journalctl-source-realtime"],
            "exact",
        ),
        (
            "output-short-monotonic-source-realtime",
            fixtures["source_realtime"],
            ["--output=short-monotonic", "--boot=all", "TEST_ID=journalctl-source-realtime"],
            "exact",
        ),
        (
            "output-short-delta-source-realtime",
            fixtures["source_realtime"],
            ["--output=short-delta", "--boot=all", "TEST_ID=journalctl-source-realtime"],
            "exact",
        ),
        (
            "output-short-unix-source-realtime",
            fixtures["source_realtime"],
            ["--output=short-unix", "--boot=all", "TEST_ID=journalctl-source-realtime"],
            "exact",
        ),
        (
            "output-verbose-binary-default",
            fixtures["output_special"],
            ["--output=verbose", "--boot=all", "TEST_ID=journalctl-output-special"],
            "exact",
        ),
        (
            "output-verbose-binary-all",
            fixtures["output_special"],
            ["--all", "--output=verbose", "--boot=all", "TEST_ID=journalctl-output-special"],
            "exact",
        ),
        (
            "output-verbose-binary-no-full",
            fixtures["output_special"],
            ["--no-full", "--output=verbose", "--boot=all", "TEST_ID=journalctl-output-special"],
            "exact",
        ),
        (
            "output-verbose-long-no-full",
            fixtures["output_long"],
            ["--no-full", "--output=verbose", "--boot=all", "TEST_ID=journalctl-output-long"],
            "exact",
        ),
        (
            "output-cat-binary-raw",
            fixtures["output_special"],
            ["--output=cat", "--boot=all", "TEST_ID=journalctl-output-special"],
            "exact",
        ),
        (
            "output-export-utf8",
            fixtures["output_special"],
            ["--output=export", "--boot=all", "TEST_ID=journalctl-output-utf8"],
            "exact",
        ),
        (
            "output-export-output-fields-boot-id",
            fixtures["file"],
            ["--output=export", "--output-fields=_BOOT_ID", *base_args],
            "exact",
        ),
    ]
    for case_name, path, args, comparison in control_text_cases:
        stock_cmd = raw_reader_command("stock", tools, "file", path, args)
        stock = run(stock_cmd, timeout=30)
        require_ok(stock, f"stock {case_name}")
        expected = stock.stdout
        expected_cmp = sorted(expected.splitlines()) if comparison == "line-multiset" else expected
        for reader in READERS:
            cmd = raw_reader_command(reader, tools, "file", path, args)
            result = run(cmd, timeout=30)
            actual_cmp = sorted(result.stdout.splitlines()) if comparison == "line-multiset" else result.stdout
            ok = result.returncode == 0 and actual_cmp == expected_cmp
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected": expected,
                    "actual": result.stdout,
                    "comparison": comparison,
                    "returncode": result.returncode,
                    "stderr": result.stderr[-1000:],
                }
            )

    json_modes = ["json", "json-pretty", "json-sse", "json-seq"]
    for mode in json_modes:
        for suffix, extra_args in (
            ("", []),
            ("-fields", ["--output-fields=MESSAGE,PRIORITY"]),
            ("-metadata-fields", ["--output-fields=_BOOT_ID"]),
        ):
            case_name = f"output-{mode}{suffix}"
            args = [f"--output={mode}", *extra_args, *base_args]
            stock_cmd = raw_reader_command("stock", tools, "file", fixtures["file"], args)
            stock = run(stock_cmd, timeout=30)
            require_ok(stock, f"stock {case_name}")
            expected = parse_json_output(mode, stock.stdout)
            for reader in READERS:
                cmd = raw_reader_command(reader, tools, "file", fixtures["file"], args)
                result = run(cmd, timeout=30)
                try:
                    actual = parse_json_output(mode, result.stdout)
                    parse_error = ""
                except Exception as err:  # noqa: BLE001 - failure detail belongs in matrix report.
                    actual = []
                    parse_error = str(err)
                ok = result.returncode == 0 and actual == expected and not parse_error
                results.append(
                    {
                        "test": case_name,
                        "reader": reader,
                        "status": "PASS" if ok else "FAIL",
                        "command": " ".join(cmd),
                        "expected": expected,
                        "actual": actual,
                        "parse_error": parse_error,
                        "returncode": result.returncode,
                        "stderr": result.stderr[-1000:],
                    }
                )
    json_pretty_style_cases = [
        (
            "output-json-pretty-style",
            fixtures["file"],
            ["--output=json-pretty", "--lines=1", *base_args],
        ),
        (
            "output-json-pretty-array-style",
            fixtures["output_special"],
            ["--all", "--output=json-pretty", "--boot=all", "TEST_ID=journalctl-output-special"],
        ),
    ]
    for case_name, path, args in json_pretty_style_cases:
        stock_cmd = raw_reader_command("stock", tools, "file", path, args)
        stock = run(stock_cmd, timeout=30)
        require_ok(stock, f"stock {case_name}")
        expected = parse_json_output("json-pretty", stock.stdout)
        expected_style = json_pretty_style_ok(stock.stdout)
        for reader in READERS:
            cmd = raw_reader_command(reader, tools, "file", path, args)
            result = run(cmd, timeout=30)
            try:
                actual = parse_json_output("json-pretty", result.stdout)
                parse_error = ""
            except Exception as err:  # noqa: BLE001 - failure detail belongs in matrix report.
                actual = []
                parse_error = str(err)
            actual_style = json_pretty_style_ok(result.stdout)
            ok = (
                result.returncode == 0
                and actual == expected
                and not parse_error
                and expected_style
                and actual_style == expected_style
            )
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected": expected,
                    "actual": actual,
                    "expected_style": expected_style,
                    "actual_style": actual_style,
                    "parse_error": parse_error,
                    "returncode": result.returncode,
                    "stderr": result.stderr[-1000:],
                }
            )
    json_control_cases = [
        (
            "output-json-binary-default",
            fixtures["output_special"],
            ["--output=json", "--boot=all", "TEST_ID=journalctl-output-special"],
        ),
        (
            "output-json-binary-all",
            fixtures["output_special"],
            ["--all", "--output=json", "--boot=all", "TEST_ID=journalctl-output-special"],
        ),
        (
            "output-json-long-threshold",
            fixtures["output_long"],
            ["--output=json", "--boot=all", "TEST_ID=journalctl-output-long"],
        ),
        (
            "output-json-long-threshold-all",
            fixtures["output_long"],
            ["--all", "--output=json", "--boot=all", "TEST_ID=journalctl-output-long"],
        ),
    ]
    for case_name, path, args in json_control_cases:
        stock_cmd = raw_reader_command("stock", tools, "file", path, args)
        stock = run(stock_cmd, timeout=30)
        require_ok(stock, f"stock {case_name}")
        expected = parse_json_output("json", stock.stdout)
        for reader in READERS:
            cmd = raw_reader_command(reader, tools, "file", path, args)
            result = run(cmd, timeout=30)
            try:
                actual = parse_json_output("json", result.stdout)
                parse_error = ""
            except Exception as err:  # noqa: BLE001 - failure detail belongs in matrix report.
                actual = []
                parse_error = str(err)
            ok = result.returncode == 0 and actual == expected and not parse_error
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected": expected,
                    "actual": actual,
                    "parse_error": parse_error,
                    "returncode": result.returncode,
                    "stderr": result.stderr[-1000:],
                }
            )
    return results


def run_empty_result_cases(tools: dict[str, str], fixtures: dict[str, Path]) -> list[dict[str, object]]:
    cases = [
        ("empty-default", [], "-- No entries --\n"),
        ("empty-quiet", ["--quiet"], ""),
        ("empty-json", ["--output=json"], ""),
        ("empty-cat", ["--output=cat"], ""),
        ("empty-export", ["--output=export"], ""),
        ("empty-verbose", ["--output=verbose"], "-- No entries --\n"),
    ]
    results: list[dict[str, object]] = []
    path = fixtures["file"]
    for case_name, extra_args, _expected_hint in cases:
        args = [*extra_args, "--boot=all", "TEST_ID=journalctl-empty-result"]
        stock_cmd = ["journalctl", "--file", str(path), "--no-pager", *args]
        stock = run(stock_cmd, timeout=30)
        require_ok(stock, f"stock {case_name}")
        expected = stock.stdout
        for reader in READERS:
            if reader == "stock":
                cmd = stock_cmd
            elif reader == "go":
                cmd = [tools["go_journalctl"], "--file", str(path), "--no-pager", *args]
            else:
                cmd = [tools["rust_journalctl"], "--file", str(path), "--no-pager", *args]
            result = run(cmd, timeout=30)
            ok = result.returncode == 0 and result.stdout == expected
            results.append(
                {
                    "test": case_name,
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected": expected,
                    "actual": result.stdout,
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
            "name": "follow-cursor-file-no-tail",
            "test_id": "journalctl-follow-cursor-file",
            "args": ["--follow", "--no-tail", "--boot=all"],
            "initial": [],
            "appends": [("cursor-follow-0", 1_700_004_250_000_000)],
            "expected": ["cursor-follow-0"],
            "cursor_file": True,
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
            "name": "follow-lines-since",
            "test_id": "journalctl-follow-lines-since",
            "args": ["--follow", "--lines=2", "--since", "@1700004300", "--boot=all"],
            "initial": [(f"lines-since-initial-{i}", 1_700_004_300_000_000 + i) for i in range(4)],
            "appends": [("lines-since-new", 1_700_004_300_001_000)],
            "expected": ["lines-since-initial-2", "lines-since-initial-3", "lines-since-new"],
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
        {
            "name": "follow-implicit-boot-no-tail",
            "test_id": "journalctl-follow-implicit-boot",
            "args": ["--follow", "--no-tail"],
            "initial": [("implicit-seed-current", 1_700_004_500_000_000, BOOT_C)],
            "appends": [
                ("implicit-old-append", 1_700_004_500_000_001, BOOT_A),
                ("implicit-current-append", 1_700_004_500_000_002, BOOT_C),
            ],
            "expected": ["implicit-seed-current", "implicit-current-append"],
            "writer_boot_id": BOOT_C,
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
            actual, returncode, stderr, cmd, cursor_written = run_follow_reader(reader, tools, mode, read_path, write_path, case)
            expected = case["expected"]
            ok = returncode == 0 and actual == expected and (not case.get("cursor_file") or cursor_written)
            results.append(
                {
                    "test": case["name"],
                    "reader": reader,
                    "status": "PASS" if ok else "FAIL",
                    "command": " ".join(cmd),
                    "expected": expected,
                    "actual": actual,
                    "cursor_written": cursor_written,
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
) -> tuple[list[str], int, str, list[str], bool]:
    if mode == "directory":
        read_path.mkdir(parents=True, exist_ok=True)
    write_path.parent.mkdir(parents=True, exist_ok=True)
    ready_file = write_path.with_suffix(write_path.suffix + ".ready")
    seq = 1
    initial_entries = []
    for row in case["initial"]:
        message, realtime, *boot_override = row
        boot_id = str(boot_override[0]) if boot_override else BOOT_A
        initial_entries.append(row_entry(message, realtime, seq, str(case["test_id"]), boot_id=boot_id))
        seq += 1
    append_entries = []
    for row in case["appends"]:
        message, realtime, *boot_override = row
        boot_id = str(boot_override[0]) if boot_override else BOOT_A
        append_entries.append(row_entry(message, realtime, seq, str(case["test_id"]), boot_id=boot_id))
        seq += 1
    writer_proc = start_live_journal_writer(
        write_path,
        ready_file=ready_file,
        machine_id=MACHINE_ID,
        boot_id=str(case.get("writer_boot_id", BOOT_A)),
        seqnum_id="34343434343434343434343434343434",
        initial_entries=initial_entries,
        append_entries=append_entries,
    )
    ready_error = wait_for_writer_ready(writer_proc, ready_file)
    if ready_error is not None:
        return [], ready_error[0], ready_error[1], [], False
    cursor_file = write_path.with_suffix(write_path.suffix + ".cursor")
    args = [*case["args"], f"TEST_ID={case['test_id']}"]
    if case.get("cursor_file"):
        if cursor_file.exists():
            cursor_file.unlink()
        args = ["--cursor-file", str(cursor_file), *args]
    cmd = reader_command(
        reader,
        tools,
        mode,
        read_path,
        args,
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
    cursor_written = cursor_file.exists() and bool(cursor_file.read_text(encoding="utf-8").strip())
    if writer_proc.returncode != 0:
        return actual, writer_proc.returncode or 1, (writer_stdout + writer_stderr + stderr)[-1000:], cmd, cursor_written
    return actual, 0 if actual == expected else proc.returncode or 1, stderr, cmd, cursor_written


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
    results.extend(run_cursor_file_cases(tools, fixtures))
    results.extend(run_portable_error_cases(tools, fixtures))
    results.extend(run_utility_action_cases(tools, fixtures))
    results.extend(run_output_mode_cases(tools, fixtures))
    results.extend(run_empty_result_cases(tools, fixtures))
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
