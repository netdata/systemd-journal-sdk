#!/usr/bin/env python3
"""Mixed-format directory reader interoperability matrix.

Generates a synthetic fixture tree containing regular and compact journal files,
uncompressed and DATA-compressed files, sealed/unsealed files, and a
repository-extension directory for mixed whole-file `.journal.zst` files. It
compares file-backed `journalctl --directory` behavior across stock journalctl
and all repository rewrites for stock-supported names; stock systemd v260.1
directory enumeration accepts only `.journal` and `.journal~` names.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Iterable


REPO_ROOT = Path(__file__).resolve().parents[2]
LOCAL_DIR = REPO_ROOT / ".local" / "interoperability"
BIN_DIR = LOCAL_DIR / "bin"
FIXTURE_DIR = LOCAL_DIR / "mixed-directory"
PYTHON = os.environ.get("PYTHON", sys.executable)

sys.path.insert(0, str(REPO_ROOT / "python"))
from journal import Writer  # noqa: E402
from journal.header import (  # noqa: E402
    HEADER_MIN_SIZE,
    INCOMPATIBLE_COMPRESSED_XZ,
    INCOMPATIBLE_COMPRESSED_LZ4,
    INCOMPATIBLE_KEYED_HASH,
    INCOMPATIBLE_COMPRESSED_ZSTD,
    INCOMPATIBLE_COMPACT,
    COMPATIBLE_SEALED,
    OBJECT_TYPE_DATA,
    OBJECT_HEADER_SIZE,
    OBJECT_COMPRESSED_XZ,
    OBJECT_COMPRESSED_LZ4,
    OBJECT_COMPRESSED_ZSTD,
)
from journal.seal import SealOptions  # noqa: E402


MACHINE_ID = "00112233445566778899aabbccddeeff"
BASE_REALTIME = 1_500_000
PAYLOAD = bytes((idx % 26) + 0x41 for idx in range(256))
SEAL_OPTS = SealOptions(seed=bytes(12), interval_usec=1_000_000, start_usec=1_000_000)
VERIFY_KEY = "000000000000000000000000/1-f4240"
WRONG_VERIFY_KEY = "000000000000000000000001/1-f4240"


@dataclass(frozen=True)
class ReaderSpec:
    name: str


@dataclass(frozen=True)
class FixtureSpec:
    seq: str
    kind: str
    relative_path: str
    compact: bool = False
    compression: str = "none"
    sealed: bool = False
    whole_file_zst: bool = False
    archived: bool = False

    @property
    def message(self) -> str:
        return f"mixed directory {self.kind}"

    @property
    def realtime(self) -> int:
        return BASE_REALTIME + int(self.seq) * 1_000

    @property
    def monotonic(self) -> int:
        return int(self.seq) + 1

    @property
    def boot_id(self) -> str:
        return f"1000000000000000000000000000{int(self.seq):04x}"

    @property
    def seqnum_id(self) -> str:
        return f"2000000000000000000000000000{int(self.seq):04x}"


READERS = {
    "stock": ReaderSpec("stock"),
    "go": ReaderSpec("go"),
    "rust": ReaderSpec("rust"),
    "node": ReaderSpec("node"),
    "python": ReaderSpec("python"),
}

STOCK_SPECS = [
    FixtureSpec("000000", "regular-none", "regular-none.journal"),
    FixtureSpec("000001", "compact-none", "compact-none.journal", compact=True),
    FixtureSpec("000002", "regular-zstd", f"{MACHINE_ID}/regular-zstd.journal", compression="zstd"),
    FixtureSpec("000003", "compact-xz-archive", "compact-xz.journal~", compact=True, compression="xz", archived=True),
    FixtureSpec("000004", "regular-lz4-archive", f"{MACHINE_ID}/regular-lz4.journal~", compression="lz4", archived=True),
    FixtureSpec("000005", "sealed-regular", "sealed-regular.journal", sealed=True),
    FixtureSpec("000006", "sealed-compact", "sealed-compact.journal~", compact=True, sealed=True, archived=True),
    FixtureSpec("000007", "sealed-compact-zstd", f"{MACHINE_ID}/sealed-compact-zstd.journal", compact=True, compression="zstd", sealed=True),
]

ZST_SPECS = [
    FixtureSpec("000008", "zst-regular", "zst-regular.journal.zst", whole_file_zst=True),
    FixtureSpec("000009", "zst-compact", "zst-compact.journal.zst", compact=True, whole_file_zst=True),
    FixtureSpec("000010", "zst-sealed", "zst-sealed.journal.zst", sealed=True, whole_file_zst=True),
    FixtureSpec(
        "000011",
        "zst-sealed-compact-archive",
        f"{MACHINE_ID}/zst-sealed-compact.journal~.zst",
        compact=True,
        sealed=True,
        whole_file_zst=True,
        archived=True,
    ),
]


def run(
    cmd: list[str],
    *,
    cwd: Path = REPO_ROOT,
    timeout: int = 120,
    binary: bool = False,
    env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess:
    return subprocess.run(
        cmd,
        cwd=str(cwd),
        env=env,
        text=None if binary else True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )


def require_ok(result: subprocess.CompletedProcess, label: str) -> None:
    if result.returncode != 0:
        raise RuntimeError(
            f"{label} failed with exit {result.returncode}\n"
            f"stdout:\n{text_tail(result.stdout, 2000)}\n"
            f"stderr:\n{text_tail(result.stderr, 2000)}"
        )


def build_env() -> dict[str, str]:
    env = os.environ.copy()
    local = REPO_ROOT / ".local"
    env.setdefault("GOMODCACHE", str(local / "go" / "pkg" / "mod"))
    env.setdefault("GOCACHE", str(local / "go-build"))
    env.setdefault("GOPATH", str(local / "go"))
    env.setdefault("CARGO_HOME", str(local / "cargo-home"))
    env.setdefault("CARGO_TARGET_DIR", str(local / "cargo-target"))
    env.setdefault("npm_config_cache", str(local / "npm-cache"))
    env.setdefault("PIP_CACHE_DIR", str(local / "pip-cache"))
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

    stock_dir = FIXTURE_DIR / "stock-supported"
    unsealed_dir = FIXTURE_DIR / "unsealed-only"
    zst_dir = FIXTURE_DIR / "zst-extension"
    stock_dir.mkdir(parents=True)
    unsealed_dir.mkdir(parents=True)
    zst_dir.mkdir(parents=True)

    for spec in STOCK_SPECS:
        path = stock_dir / spec.relative_path
        write_journal(path, spec)
        inspect = inspect_journal_features(path)
        feature_errors = validate_feature_flags(spec, inspect)
        if feature_errors:
            raise RuntimeError(f"{path}: {'; '.join(feature_errors)}")
        if not spec.sealed:
            copy_path = unsealed_dir / spec.relative_path
            copy_path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, copy_path)

    for spec in ZST_SPECS:
        target = zst_dir / spec.relative_path
        source = target.with_suffix("")
        write_journal(source, spec)
        inspect = inspect_journal_features(source)
        feature_errors = validate_feature_flags(spec, inspect)
        if feature_errors:
            raise RuntimeError(f"{source}: {'; '.join(feature_errors)}")
        target.parent.mkdir(parents=True, exist_ok=True)
        require_ok(run(["zstd", "-q", "-f", "-o", str(target), str(source)]), f"compress {source.name}")
        source.unlink()
        inspect_zst = inspect_journal_features(target)
        feature_errors = validate_feature_flags(spec, inspect_zst)
        if feature_errors:
            raise RuntimeError(f"{target}: {'; '.join(feature_errors)}")

    return {"stock": stock_dir, "unsealed": unsealed_dir, "zst": zst_dir}


def write_journal(path: Path, spec: FixtureSpec) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    opts = {
        "machine_id": MACHINE_ID,
        "boot_id": spec.boot_id,
        "seqnum_id": spec.seqnum_id,
        "compact": spec.compact,
        "compression": spec.compression,
        "compression_threshold_bytes": 16,
    }
    if spec.sealed:
        opts["seal"] = SEAL_OPTS
    writer = Writer.create(str(path), opts)
    try:
        writer.append(
            [
                {"name": "TEST_ID", "value": "mixed-directory"},
                {"name": "_BOOT_ID", "value": spec.boot_id},
                {"name": "_MACHINE_ID", "value": MACHINE_ID},
                {"name": "MESSAGE", "value": spec.message},
                {"name": "PRIORITY", "value": "6"},
                {"name": "MIXED_SEQ", "value": spec.seq},
                {"name": "LIVE_SEQ", "value": spec.seq},
                {"name": "MIXED_KIND", "value": spec.kind},
                {"name": "MIXED_COMPACT", "value": "1" if spec.compact else "0"},
                {"name": "MIXED_COMPRESSION", "value": spec.compression},
                {"name": "MIXED_SEALED", "value": "1" if spec.sealed else "0"},
                {"name": "MIXED_WHOLE_FILE_ZST", "value": "1" if spec.whole_file_zst else "0"},
                {"name": "MIXED_PAYLOAD", "value": PAYLOAD},
            ],
            {"realtime_usec": spec.realtime, "monotonic_usec": spec.monotonic},
        )
    finally:
        writer.close()


def inspect_journal_features(path: Path) -> dict:
    data = read_journal_bytes(path)
    if len(data) < HEADER_MIN_SIZE:
        return {"status": "FAIL", "error": "journal smaller than header"}

    compatible_flags = int.from_bytes(data[8:12], "little")
    incompatible_flags = int.from_bytes(data[12:16], "little")
    header_size = int.from_bytes(data[88:96], "little")
    if header_size < HEADER_MIN_SIZE or header_size > len(data):
        return {"status": "FAIL", "error": f"invalid header_size {header_size}"}

    compression_objects = {"zstd": 0, "xz": 0, "lz4": 0}
    offset = header_size
    while offset + OBJECT_HEADER_SIZE <= len(data):
        obj_type = data[offset]
        obj_flags = data[offset + 1]
        obj_size = int.from_bytes(data[offset + 8:offset + 16], "little")
        if obj_size < OBJECT_HEADER_SIZE or offset + obj_size > len(data):
            break
        if obj_type == OBJECT_TYPE_DATA:
            if obj_flags & OBJECT_COMPRESSED_ZSTD:
                compression_objects["zstd"] += 1
            if obj_flags & OBJECT_COMPRESSED_XZ:
                compression_objects["xz"] += 1
            if obj_flags & OBJECT_COMPRESSED_LZ4:
                compression_objects["lz4"] += 1
        offset = (offset + obj_size + 7) & ~7

    return {
        "status": "PASS",
        "compatible_flags": compatible_flags,
        "incompatible_flags": incompatible_flags,
        "header_size": header_size,
        "compression_objects": compression_objects,
    }


def read_journal_bytes(path: Path) -> bytes:
    if path.name.endswith(".zst"):
        result = run(["zstd", "-q", "-dc", str(path)], binary=True)
        require_ok(result, f"decompress {path.name}")
        return result.stdout
    return path.read_bytes()


def validate_feature_flags(spec: FixtureSpec, inspect: dict) -> list[str]:
    if inspect.get("status") != "PASS":
        return [inspect.get("error", "inspection failed")]

    incompatible = inspect["incompatible_flags"]
    compatible = inspect["compatible_flags"]
    compressed = inspect["compression_objects"]
    errors = []
    if not (incompatible & INCOMPATIBLE_KEYED_HASH):
        errors.append("KEYED_HASH flag missing")
    if spec.compact != bool(incompatible & INCOMPATIBLE_COMPACT):
        errors.append(f"compact flag mismatch: got {bool(incompatible & INCOMPATIBLE_COMPACT)}")
    if spec.sealed != bool(compatible & COMPATIBLE_SEALED):
        errors.append(f"sealed flag mismatch: got {bool(compatible & COMPATIBLE_SEALED)}")

    expected_compression_flags = {
        "none": 0,
        "zstd": INCOMPATIBLE_COMPRESSED_ZSTD,
        "xz": INCOMPATIBLE_COMPRESSED_XZ,
        "lz4": INCOMPATIBLE_COMPRESSED_LZ4,
    }
    expected_flag = expected_compression_flags[spec.compression]
    compression_mask = INCOMPATIBLE_COMPRESSED_ZSTD | INCOMPATIBLE_COMPRESSED_XZ | INCOMPATIBLE_COMPRESSED_LZ4
    if (incompatible & compression_mask) != expected_flag:
        errors.append(f"compression header flag mismatch: got 0x{incompatible & compression_mask:x}, want 0x{expected_flag:x}")
    if spec.compression != "none" and compressed[spec.compression] == 0:
        errors.append(f"no {spec.compression} DATA object found")
    return errors


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
            base = ["journalctl", "--verify", "--directory", str(directory), "--no-pager", "--quiet"]
        else:
            base = [*base, "--verify"]
        return [*base, *args]
    raise ValueError(mode)


def parse_json_lines(stdout: str) -> list[dict]:
    return [json.loads(line) for line in stdout.splitlines() if line.strip()]


def expected_sequence(specs: list[FixtureSpec]) -> list[str]:
    return [spec.seq for spec in sorted(specs, key=lambda spec: spec.realtime)]


def expected_by_seq(specs: list[FixtureSpec]) -> dict[str, FixtureSpec]:
    return {spec.seq: spec for spec in specs}


def sequence_values(entries: list[dict]) -> list[str]:
    return [str(entry.get("MIXED_SEQ", "")) for entry in entries]


def export_sequence_values(stdout: bytes) -> list[str]:
    values = []
    for line in stdout.splitlines():
        if line.startswith(b"MIXED_SEQ="):
            values.append(line.split(b"=", 1)[1].decode("latin-1"))
    return values


def run_json_check(
    reader: ReaderSpec,
    tools: dict[str, str],
    directory: Path,
    test_name: str,
    specs: list[FixtureSpec],
    matches: list[str],
    expected: list[str] | None = None,
) -> dict:
    expected = expected if expected is not None else expected_sequence(specs)
    cmd = reader_command(reader, tools, "json", directory, matches)
    result = run(cmd)
    record = base_record(reader, test_name, cmd)
    if result.returncode != 0:
        record["error"] = text_tail(result.stderr) or text_tail(result.stdout)
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
    if got != expected:
        record["error"] = f"sequence mismatch: got {got}, expected {expected}"
        return record

    spec_by_seq = expected_by_seq(specs)
    field_errors = []
    for entry in entries:
        seq = str(entry.get("MIXED_SEQ", ""))
        spec = spec_by_seq.get(seq)
        if spec is None:
            field_errors.append(f"unexpected MIXED_SEQ={seq!r}")
            continue
        field_errors.extend(validate_json_entry(entry, spec))
    if field_errors:
        record["error"] = "; ".join(field_errors)
        return record
    record["status"] = "PASS"
    return record


def validate_json_entry(entry: dict, spec: FixtureSpec) -> list[str]:
    errors = []
    expected_fields = {
        "TEST_ID": "mixed-directory",
        "_BOOT_ID": spec.boot_id,
        "_MACHINE_ID": MACHINE_ID,
        "MESSAGE": spec.message,
        "PRIORITY": "6",
        "LIVE_SEQ": spec.seq,
        "MIXED_KIND": spec.kind,
        "MIXED_COMPACT": "1" if spec.compact else "0",
        "MIXED_COMPRESSION": spec.compression,
        "MIXED_SEALED": "1" if spec.sealed else "0",
        "MIXED_WHOLE_FILE_ZST": "1" if spec.whole_file_zst else "0",
        "MIXED_PAYLOAD": PAYLOAD.decode("ascii"),
    }
    for field, expected in expected_fields.items():
        if entry.get(field) != expected:
            got = entry.get(field)
            if isinstance(got, str) and len(got) > 80:
                got = got[:80] + "..."
            errors.append(f"{spec.seq}:{field}={got!r}, want {expected!r}")
    return errors


def run_export_check(reader: ReaderSpec, tools: dict[str, str], directory: Path, specs: list[FixtureSpec]) -> dict:
    expected = expected_sequence(specs)
    cmd = reader_command(reader, tools, "export", directory, [])
    result = run(cmd, binary=True)
    record = base_record(reader, "export-output", cmd)
    if result.returncode != 0:
        record["error"] = text_tail(result.stderr) or text_tail(result.stdout)
        return record
    got = export_sequence_values(result.stdout)
    if got == expected:
        record["status"] = "PASS"
    else:
        record["error"] = f"export sequence mismatch: got {got}, expected {expected}"
    return record


def run_text_check(reader: ReaderSpec, tools: dict[str, str], directory: Path, specs: list[FixtureSpec]) -> dict:
    cmd = reader_command(reader, tools, "text", directory, [])
    result = run(cmd)
    record = base_record(reader, "text-output", cmd)
    if result.returncode != 0:
        record["error"] = text_tail(result.stderr) or text_tail(result.stdout)
        return record
    missing = [spec.message for spec in specs if spec.message not in result.stdout]
    if not missing:
        record["status"] = "PASS"
    else:
        record["error"] = f"missing text messages: {missing}"
    return record


def run_fields_check(reader: ReaderSpec, tools: dict[str, str], directory: Path) -> dict:
    required = {
        "TEST_ID",
        "MESSAGE",
        "PRIORITY",
        "MIXED_SEQ",
        "MIXED_KIND",
        "MIXED_COMPACT",
        "MIXED_COMPRESSION",
        "MIXED_SEALED",
        "MIXED_WHOLE_FILE_ZST",
        "MIXED_PAYLOAD",
    }
    cmd = reader_command(reader, tools, "fields", directory, [])
    result = run(cmd)
    record = base_record(reader, "fields", cmd)
    if result.returncode != 0:
        record["error"] = text_tail(result.stderr) or text_tail(result.stdout)
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
        record["error"] = text_tail(result.stderr) or text_tail(result.stdout)
        return record
    lines = [line for line in result.stdout.splitlines() if line.strip()]
    record["entries_read"] = len(lines)
    record["expected"] = expected_count
    if len(lines) == expected_count:
        record["status"] = "PASS"
    else:
        record["error"] = f"boot count mismatch: got {len(lines)}, expected {expected_count}"
    return record


def run_verify_check(
    reader: ReaderSpec,
    tools: dict[str, str],
    directory: Path,
    test_name: str,
    args: list[str],
    should_pass: bool,
) -> dict:
    cmd = reader_command(reader, tools, "verify", directory, args)
    result = run(cmd)
    record = base_record(reader, test_name, cmd)
    passed = result.returncode == 0
    if passed == should_pass:
        record["status"] = "PASS"
    else:
        expectation = "pass" if should_pass else "fail"
        record["error"] = (
            f"expected verify to {expectation}, exit={result.returncode}, "
            f"stderr={text_tail(result.stderr)}, stdout={text_tail(result.stdout)}"
        )
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


def text_tail(value: str | bytes, limit: int = 500) -> str:
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")[-limit:]
    return value[-limit:]


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
    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)
    if shutil.which("zstd") is None:
        raise SystemExit("zstd CLI is required for whole-file .journal.zst fixture generation")

    print("Building tools...", flush=True)
    tools = build_tools()
    readers = selected(READERS, args.readers)

    print("Generating mixed-format fixtures...", flush=True)
    fixtures = make_fixtures()

    all_checks: list[dict] = []
    stock_sequence = expected_sequence(STOCK_SPECS)
    zst_sequence = expected_sequence(ZST_SPECS)

    for reader in readers:
        checks = [
            run_json_check(reader, tools, fixtures["stock"], "json-all", STOCK_SPECS, [], stock_sequence),
            run_json_check(
                reader,
                tools,
                fixtures["stock"],
                "regular-or-compact-none",
                STOCK_SPECS,
                ["MIXED_KIND=regular-none", "MIXED_KIND=compact-none"],
                ["000000", "000001"],
            ),
            run_json_check(
                reader,
                tools,
                fixtures["stock"],
                "compact-and-unsealed",
                STOCK_SPECS,
                ["MIXED_COMPACT=1", "MIXED_SEALED=0"],
                ["000001", "000003"],
            ),
            run_json_check(
                reader,
                tools,
                fixtures["stock"],
                "plus-disjunction",
                STOCK_SPECS,
                ["MIXED_SEQ=000000", "+", "MIXED_SEQ=000006"],
                ["000000", "000006"],
            ),
            run_export_check(reader, tools, fixtures["stock"], STOCK_SPECS),
            run_text_check(reader, tools, fixtures["stock"], STOCK_SPECS),
            run_fields_check(reader, tools, fixtures["stock"]),
            run_boots_check(reader, tools, fixtures["stock"], len(STOCK_SPECS)),
            run_verify_check(reader, tools, fixtures["unsealed"], "verify-unsealed-without-key", [], True),
            run_verify_check(reader, tools, fixtures["stock"], "verify-sealed-without-key-fails", [], False),
            run_verify_check(reader, tools, fixtures["stock"], "verify-sealed-with-key", ["--verify-key", VERIFY_KEY], True),
            run_verify_check(reader, tools, fixtures["stock"], "verify-sealed-wrong-key-fails", ["--verify-key", WRONG_VERIFY_KEY], False),
        ]
        all_checks.extend(checks)
        for check in checks:
            print(f"{reader.name} {check['test']}: {check['status']}", flush=True)

        if reader.name != "stock":
            zst_checks = [
                run_json_check(reader, tools, fixtures["zst"], "zst-json-all", ZST_SPECS, [], zst_sequence),
                run_verify_check(reader, tools, fixtures["zst"], "zst-verify-without-key-fails", [], False),
                run_verify_check(reader, tools, fixtures["zst"], "zst-verify-with-key", ["--verify-key", VERIFY_KEY], True),
            ]
            all_checks.extend(zst_checks)
            for check in zst_checks:
                print(f"{reader.name} {check['test']}: {check['status']}", flush=True)

    passed = sum(1 for check in all_checks if check.get("status") == "PASS")
    failed = len(all_checks) - passed

    payload = {
        "timestamp": datetime.now().isoformat(timespec="seconds"),
        "systemd_version": systemd_version(),
        "fixture_directory": str(FIXTURE_DIR),
        "verification_key": VERIFY_KEY,
        "readers": [reader.name for reader in readers],
        "stock_sequence": stock_sequence,
        "zst_sequence": zst_sequence,
        "checks": all_checks,
        "summary": {"total": len(all_checks), "passed": passed, "failed": failed},
    }
    result_path = LOCAL_DIR / f"mixed-directory-matrix-results-{datetime.now().strftime('%Y%m%d-%H%M%S')}.json"
    result_path.write_text(json.dumps(payload, indent=2) + "\n")

    print("\n=== SUMMARY ===", flush=True)
    print(f"systemd: {payload['systemd_version']}", flush=True)
    print(f"readers: {', '.join(payload['readers'])}", flush=True)
    print(f"total: {len(all_checks)}, passed: {passed}, failed: {failed}", flush=True)
    print(f"results: {result_path}", flush=True)

    for check in all_checks:
        if check.get("status") != "PASS":
            print(f"FAIL: {check.get('reader', '?')} {check.get('test', '?')}: {check.get('error', '')}", flush=True)

    if not args.keep_files:
        shutil.rmtree(FIXTURE_DIR, ignore_errors=True)

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
