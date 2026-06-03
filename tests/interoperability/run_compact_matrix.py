#!/usr/bin/env python3
"""Cross-language compact journal interoperability matrix.

Generates compact-format journal files from each writer language, verifies the
compact incompatible header flag and compact object layout, then validates
stock systemd and repository readers against each generated file. Runtime
artifacts stay under .local/interoperability/.

Compact fixture per writer:
  TEST_ID=binary-interoperability
  MESSAGE=binary interoperability
  PRIORITY=6
  LIVE_SEQ=000000
  BINARY_PAYLOAD=\\x00\\x01\\x02A\\n\\x7f\\x80\\xff
  BINARY_MATCH=abc\\x07def
  BINARY_EMPTY= (empty value)
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess  # nosec B404 - subprocess is required by harnesses.
import sys
import time
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Iterable

from journal_structure import inspect_journal_structure


REPO_ROOT = Path(__file__).resolve().parents[2]
LOCAL_DIR = REPO_ROOT / ".local" / "interoperability"
BIN_DIR = LOCAL_DIR / "bin"
FIXTURE_DIR = LOCAL_DIR / "compact"

BINARY_PAYLOAD = bytes([0x00, 0x01, 0x02, 0x41, 0x0A, 0x7F, 0x80, 0xFF])
BINARY_MATCH = bytes([0x61, 0x62, 0x63, 0x07, 0x64, 0x65, 0x66])


@dataclass(frozen=True)
class WriterSpec:
    name: str
    mode: str


@dataclass(frozen=True)
class ReaderSpec:
    name: str


WRITERS = {
    "go": WriterSpec("go", "file"),
    "rust": WriterSpec("rust", "directory"),
    "node": WriterSpec("node", "file"),
    "python": WriterSpec("python", "file"),
}

READERS = {
    "stock": ReaderSpec("stock"),
    "go": ReaderSpec("go"),
    "rust": ReaderSpec("rust"),
    "node": ReaderSpec("node"),
    "python": ReaderSpec("python"),
}


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
    python_deps = local / "python-deps"
    env["PYTHONPATH"] = (
        f"{python_deps}{os.pathsep}{env['PYTHONPATH']}"
        if env.get("PYTHONPATH")
        else str(python_deps)
    )
    return env


def run(
    cmd: list[str],
    *,
    cwd: Path = REPO_ROOT,
    timeout: int = 120,
    binary: bool = False,
    env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess:
    if env is None:
        env = build_env()
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    return subprocess.run(  # nosec B603 - harness uses shell=False command vectors.
        cmd,  # nosemgrep
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
            f"stdout:\n{text_tail(result.stdout)}\n"
            f"stderr:\n{text_tail(result.stderr)}"
        )


def build_tools() -> dict[str, str]:
    BIN_DIR.mkdir(parents=True, exist_ok=True)
    env = build_env()

    go_livewriter = BIN_DIR / "go-livewriter"
    go_journalctl = BIN_DIR / "go-journalctl"
    require_ok(
        run(
            ["go", "build", "-o", str(go_livewriter), "./internal/testcmd/livewriter"],
            cwd=REPO_ROOT / "go",
            env=env,
        ),
        "build go livewriter",
    )
    require_ok(
        run(
            ["go", "build", "-o", str(go_journalctl), "./cmd/journalctl"],
            cwd=REPO_ROOT / "go",
            env=env,
        ),
        "build go journalctl",
    )
    require_ok(
        run(
            ["cargo", "build", "--manifest-path", str(REPO_ROOT / "rust/Cargo.toml"), "-p", "livewriter"],
            timeout=180,
            env=env,
        ),
        "build rust livewriter",
    )
    require_ok(
        run(
            ["cargo", "build", "--manifest-path", str(REPO_ROOT / "rust/Cargo.toml"), "-p", "journalctl"],
            timeout=180,
            env=env,
        ),
        "build rust journalctl",
    )

    cargo_target = Path(env["CARGO_TARGET_DIR"])
    for src, dst in [
        (cargo_target / "debug" / "livewriter", BIN_DIR / "rust-livewriter"),
        (cargo_target / "debug" / "journalctl", BIN_DIR / "rust-journalctl"),
    ]:
        if src.exists():
            shutil.copy2(src, dst)

    for name in ["go-livewriter", "go-journalctl", "rust-livewriter", "rust-journalctl"]:
        path = BIN_DIR / name
        if not path.exists():
            raise RuntimeError(f"expected binary not found: {path}")

    return {
        "go_livewriter": str(BIN_DIR / "go-livewriter"),
        "go_journalctl": str(BIN_DIR / "go-journalctl"),
        "rust_livewriter": str(BIN_DIR / "rust-livewriter"),
        "rust_journalctl": str(BIN_DIR / "rust-journalctl"),
    }


def build_libsystemd_reader() -> str:
    src = REPO_ROOT / "tests" / "conformance" / "binary" / "libsystemd_binary_field_reader.c"
    dst = BIN_DIR / "libsystemd_binary_field_reader"
    require_ok(
        run(["gcc", "-o", str(dst), str(src), "-Wl,--no-as-needed", "-lsystemd", "-lm", "-lpthread"]),
        "build libsystemd_binary_field_reader",
    )
    return str(dst)


def writer_command(
    writer: WriterSpec,
    tools: dict[str, str],
    target: Path,
    ready: Path,
    entries: int,
    compression: str,
    compression_threshold_bytes: int,
) -> list[str]:
    common = [
        "--ready-file", str(ready),
        "--entries", str(entries),
        "--delay", "1ms",
        "--binary-fixture",
        "--compact",
    ]
    if writer.name == "go":
        return [
            tools["go_livewriter"],
            "--path", str(target),
            "--compression", compression,
            "--compress-threshold", str(compression_threshold_bytes),
            *common,
        ]
    if writer.name == "rust":
        return [
            tools["rust_livewriter"],
            "--dir", str(target),
            "--compression", compression,
            "--compression-threshold-bytes", str(compression_threshold_bytes),
            *common,
        ]
    if writer.name == "node":
        return [
            "node",
            str(REPO_ROOT / "node/internal/testcmd/livewriter.js"),
            "--path", str(target),
            "--compression", compression,
            "--compression-threshold-bytes", str(compression_threshold_bytes),
            *common,
        ]
    if writer.name == "python":
        return [
            "python3",
            str(REPO_ROOT / "python/cmd/livewriter.py"),
            "--path", str(target),
            "--compression", compression,
            "--compression-threshold-bytes", str(compression_threshold_bytes),
            *common,
        ]
    raise ValueError(writer.name)


def generate_journal(
    writer: WriterSpec,
    tools: dict[str, str],
    entries: int,
    compression: str,
    compression_threshold_bytes: int,
) -> dict[str, str]:
    writer_root = FIXTURE_DIR / compression / writer.name
    if writer_root.exists():
        shutil.rmtree(writer_root)
    writer_root.mkdir(parents=True, exist_ok=True)

    ready = FIXTURE_DIR / f"{compression}-{writer.name}.ready"
    ready.unlink(missing_ok=True)

    target = writer_root if writer.mode == "directory" else writer_root / f"{writer.name}.journal"
    result = run(
        writer_command(writer, tools, target, ready, entries, compression, compression_threshold_bytes),
        timeout=max(60, entries // 2),
        env=build_env(),
    )
    require_ok(result, f"{writer.name} compact writer")
    wait_for_file(ready, f"{writer.name} ready file")

    if writer.mode == "directory":
        journal_files = sorted(writer_root.rglob("*.journal"))
        if len(journal_files) != 1:
            raise RuntimeError(f"{writer.name} writer expected exactly one journal file, found {len(journal_files)}")
        journal_path = journal_files[0]
    else:
        journal_path = target

    if not journal_path.exists():
        raise RuntimeError(f"{writer.name} compact journal was not created: {journal_path}")

    return {
        "writer": writer.name,
        "compression": compression,
        "journal_file": str(journal_path),
        "journal_directory": str(writer_root),
    }


def inspect_compact(journal_path: str, compression: str) -> dict:
    return inspect_journal_structure(
        journal_path,
        expected_compact=True,
        expected_compression=compression,
        test_name="compact-structure",
    )


def wait_for_file(path: Path, label: str, timeout: float = 10.0) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if path.exists():
            return
        time.sleep(0.05)
    raise RuntimeError(f"timed out waiting for {label}: {path}")


def shell_join(cmd: Iterable[str]) -> str:
    def display_arg(part: str) -> str:
        if any(ch.isspace() or ord(ch) < 32 or ord(ch) == 127 for ch in part):
            return json.dumps(part)
        return part

    return " ".join(display_arg(part) for part in cmd)


def text_tail(value: str | bytes, limit: int = 500) -> str:
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")[-limit:]
    return value[-limit:]


def parse_json_lines(stdout: str | bytes) -> list[dict]:
    if isinstance(stdout, bytes):
        stdout = stdout.decode("utf-8", errors="replace")
    entries = []
    for line in stdout.splitlines():
        line = line.strip()
        if line:
            entries.append(json.loads(line))
    return entries


def json_bytes(value) -> list[int] | None:
    if isinstance(value, list) and all(isinstance(v, int) for v in value):
        return value
    return None


def validate_binary_json_entry(entry: dict) -> list[str]:
    errors = []
    expected_strings = {
        "TEST_ID": "binary-interoperability",
        "MESSAGE": "binary interoperability",
        "PRIORITY": "6",
        "LIVE_SEQ": "000000",
    }
    for field, expected in expected_strings.items():
        if entry.get(field) != expected:
            errors.append(f"{field}={entry.get(field)!r}")

    for field, expected in [("BINARY_PAYLOAD", BINARY_PAYLOAD), ("BINARY_MATCH", BINARY_MATCH)]:
        value = json_bytes(entry.get(field))
        if value is None:
            errors.append(f"{field} not byte array: {entry.get(field)!r}")
        elif value != list(expected):
            errors.append(f"{field} mismatch: got {value}, want {list(expected)}")

    if "BINARY_EMPTY" not in entry:
        errors.append("BINARY_EMPTY missing")
    elif entry["BINARY_EMPTY"] != "":
        errors.append(f"BINARY_EMPTY={entry['BINARY_EMPTY']!r}")
    return errors


def check_stock_verify(journal_path: str) -> dict:
    cmd = ["journalctl", "--verify", "--file", journal_path]
    result = run(cmd, timeout=30)
    return {
        "test": "stock-verify",
        "command": shell_join(cmd),
        "status": "PASS" if result.returncode == 0 else "FAIL",
        "error": text_tail(result.stderr) if result.returncode != 0 else "",
    }


def check_stock_json(journal_path: str) -> dict:
    cmd = ["journalctl", "--file", journal_path, "--output=json", "--quiet", "--no-pager", "TEST_ID=binary-interoperability"]
    return _check_json_command(cmd, "stock", "stock-json")


def check_stock_export(journal_path: str) -> dict:
    cmd = ["journalctl", "--file", journal_path, "--output=export", "--quiet", "--no-pager", "TEST_ID=binary-interoperability"]
    result = run(cmd, timeout=30, binary=True)
    if result.returncode != 0:
        return {"test": "stock-export", "command": shell_join(cmd), "status": "FAIL", "error": text_tail(result.stderr)}
    return _validate_export_output_bytes(result.stdout, "stock-export", cmd)


def check_stock_export_match(journal_path: str) -> dict:
    cmd = ["journalctl", "--file", journal_path, "--output=export", "--quiet", "--no-pager", "BINARY_MATCH=abc\x07def"]
    result = run(cmd, timeout=30, binary=True)
    if result.returncode != 0:
        return {"test": "stock-export-match", "command": shell_join(cmd), "status": "FAIL", "error": text_tail(result.stderr)}
    return _validate_export_output_bytes(result.stdout, "stock-export-match", cmd)


def check_libsystemd(journal_path: str, libsystemd_reader: str) -> dict:
    errors = []
    for field, expected_hex, match_arg in [
        ("BINARY_PAYLOAD", BINARY_PAYLOAD.hex(), "TEST_ID=binary-interoperability"),
        ("BINARY_MATCH", BINARY_MATCH.hex(), "TEST_ID=binary-interoperability"),
        ("BINARY_EMPTY", "", "TEST_ID=binary-interoperability"),
    ]:
        cmd = [libsystemd_reader, journal_path, field, expected_hex, match_arg]
        result = run(cmd, timeout=30)
        if result.returncode != 0:
            errors.append(f"{field}: exit {result.returncode} {text_tail(result.stderr, 200)}")
    if errors:
        return {"test": "libsystemd", "status": "FAIL", "error": "; ".join(errors)}
    return {"test": "libsystemd", "status": "PASS"}


def check_reader_json(reader: ReaderSpec, tools: dict[str, str], journal_path: str, writer_name: str) -> dict:
    cmd = _reader_json_cmd(reader, tools, journal_path)
    result = _check_json_command(cmd, reader.name, "json")
    result["writer"] = writer_name
    result["reader"] = reader.name
    return result


def check_reader_export(reader: ReaderSpec, tools: dict[str, str], journal_path: str, writer_name: str) -> dict:
    cmd = _reader_export_cmd(reader, tools, journal_path)
    result = run(cmd, timeout=30, binary=True)
    if result.returncode != 0:
        return {
            "writer": writer_name,
            "reader": reader.name,
            "test": "export",
            "command": shell_join(cmd),
            "status": "FAIL",
            "error": text_tail(result.stderr),
        }
    validation = _validate_export_output_bytes(result.stdout, "export", cmd)
    validation["writer"] = writer_name
    validation["reader"] = reader.name
    return validation


def _check_json_command(cmd: list[str], reader_name: str, test_name: str) -> dict:
    result = run(cmd, timeout=30)
    if result.returncode != 0:
        return {"reader": reader_name, "test": test_name, "command": shell_join(cmd), "status": "FAIL", "error": text_tail(result.stderr)}
    try:
        entries = parse_json_lines(result.stdout)
    except Exception as err:
        return {"reader": reader_name, "test": test_name, "command": shell_join(cmd), "status": "FAIL", "error": f"JSON parse error: {err}"}
    if len(entries) != 1:
        return {"reader": reader_name, "test": test_name, "command": shell_join(cmd), "status": "FAIL", "error": f"expected 1 entry, got {len(entries)}"}
    field_errors = validate_binary_json_entry(entries[0])
    if field_errors:
        return {"reader": reader_name, "test": test_name, "command": shell_join(cmd), "status": "FAIL", "error": "; ".join(field_errors)}
    return {"reader": reader_name, "test": test_name, "command": shell_join(cmd), "status": "PASS"}


def _parse_export_entries(output: bytes) -> dict[str, bytes]:
    fields: dict[str, bytes] = {}
    i = 0
    while i < len(output):
        if output[i] == 0x0A:
            i += 1
            continue
        line_start = i
        while i < len(output) and output[i] != 0x0A:
            i += 1
        line = output[line_start:i]
        i += 1
        if not line:
            continue
        eq_idx = line.find(b"=")
        if eq_idx >= 0:
            fields[line[:eq_idx].decode("latin-1", errors="replace")] = line[eq_idx + 1:]
            continue
        name = line.decode("latin-1", errors="replace")
        if i + 8 > len(output):
            break
        size = int.from_bytes(output[i:i + 8], "little")
        i += 8
        fields[name] = output[i:i + size]
        i += size
        if i < len(output) and output[i] == 0x0A:
            i += 1
    return fields


def _validate_export_output_bytes(output: bytes, test_name: str, cmd: list[str]) -> dict:
    fields = _parse_export_entries(output)
    errors = []
    expected = {
        "TEST_ID": b"binary-interoperability",
        "MESSAGE": b"binary interoperability",
        "PRIORITY": b"6",
        "LIVE_SEQ": b"000000",
        "BINARY_PAYLOAD": BINARY_PAYLOAD,
        "BINARY_MATCH": BINARY_MATCH,
        "BINARY_EMPTY": b"",
    }
    for field, expected_bytes in expected.items():
        actual = fields.get(field)
        if actual != expected_bytes:
            got = actual.hex() if isinstance(actual, bytes) else repr(actual)
            errors.append(f"{field} mismatch: got {got}, want {expected_bytes.hex()}")
    if errors:
        return {"test": test_name, "command": shell_join(cmd), "status": "FAIL", "error": "; ".join(errors)}
    return {"test": test_name, "command": shell_join(cmd), "status": "PASS"}


def _reader_json_cmd(reader: ReaderSpec, tools: dict[str, str], journal_path: str) -> list[str]:
    match_arg = "TEST_ID=binary-interoperability"
    if reader.name == "stock":
        return ["journalctl", "--file", journal_path, "--output=json", "--quiet", "--no-pager", match_arg]
    if reader.name == "go":
        return [tools["go_journalctl"], "--file", journal_path, "--output=json", match_arg]
    if reader.name == "rust":
        return [tools["rust_journalctl"], "--file", journal_path, "--output=json", match_arg]
    if reader.name == "node":
        return ["node", str(REPO_ROOT / "node/cmd/journalctl/index.js"), "--file", journal_path, "--output", "json", match_arg]
    if reader.name == "python":
        return ["python3", str(REPO_ROOT / "python/cmd/journalctl.py"), "--file", journal_path, "--output", "json", match_arg]
    raise ValueError(reader.name)


def _reader_export_cmd(reader: ReaderSpec, tools: dict[str, str], journal_path: str) -> list[str]:
    match_arg = "TEST_ID=binary-interoperability"
    if reader.name == "stock":
        return ["journalctl", "--file", journal_path, "--output=export", "--quiet", "--no-pager", match_arg]
    if reader.name == "go":
        return [tools["go_journalctl"], "--file", journal_path, "--output=export", match_arg]
    if reader.name == "rust":
        return [tools["rust_journalctl"], "--file", journal_path, "--output=export", match_arg]
    if reader.name == "node":
        return ["node", str(REPO_ROOT / "node/cmd/journalctl/index.js"), "--file", journal_path, "--output", "export", match_arg]
    if reader.name == "python":
        return ["python3", str(REPO_ROOT / "python/cmd/journalctl.py"), "--file", journal_path, "--output=export", match_arg]
    raise ValueError(reader.name)


def selected(mapping: dict[str, object], names: list[str] | None):
    if not names:
        return list(mapping.values())
    missing = [name for name in names if name not in mapping]
    if missing:
        raise SystemExit(f"unknown names: {', '.join(missing)}")
    return [mapping[name] for name in names]


def systemd_version() -> str:
    result = run(["journalctl", "--version"], timeout=10)
    if result.returncode == 0 and result.stdout.strip():
        return result.stdout.splitlines()[0]
    return "unavailable"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--writers", nargs="*", choices=sorted(WRITERS))
    parser.add_argument("--readers", nargs="*", choices=sorted(READERS))
    parser.add_argument("--entries", type=int, default=10)
    parser.add_argument("--compression", choices=("none", "zstd", "xz", "lz4"), default="none")
    # Intentionally below the SDK default of 512 to exercise compression with small compact fixtures.
    parser.add_argument("--compression-threshold-bytes", type=int, default=16)
    parser.add_argument("--keep-files", action="store_true")
    args = parser.parse_args()

    LOCAL_DIR.mkdir(parents=True, exist_ok=True)
    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)

    print("Building tools...", flush=True)
    tools = build_tools()
    libsystemd_reader = build_libsystemd_reader()

    writer_specs = selected(WRITERS, args.writers)
    reader_specs = selected(READERS, args.readers)

    generated = []
    all_checks: list[dict] = []

    for writer in writer_specs:
        print(f"\n--- Generating {writer.name} compact fixture ({args.compression}) ---", flush=True)
        try:
            result = generate_journal(
                writer,
                tools,
                args.entries,
                args.compression,
                args.compression_threshold_bytes,
            )
            generated.append(result)
        except Exception as err:
            print(f"ERROR generating {writer.name}: {err}", flush=True)
            all_checks.append({"writer": writer.name, "compression": args.compression, "status": "FAIL", "error": str(err)})
            continue

        journal_path = result["journal_file"]
        print(f"  journal: {journal_path}", flush=True)

        checks = [
            inspect_compact(journal_path, args.compression),
            check_stock_verify(journal_path),
            check_stock_json(journal_path),
            check_stock_export(journal_path),
            check_stock_export_match(journal_path),
            check_libsystemd(journal_path, libsystemd_reader),
        ]
        for check in checks:
            all_checks.append({"writer": writer.name, "compression": args.compression, **check})
            print(f"  {check['test']}: {check['status']}", flush=True)

        for reader in reader_specs:
            if reader.name == "stock":
                continue
            reader_json = check_reader_json(reader, tools, journal_path, writer.name)
            all_checks.append(reader_json)
            print(f"  {reader.name}-json: {reader_json['status']}", flush=True)

            reader_export = check_reader_export(reader, tools, journal_path, writer.name)
            all_checks.append(reader_export)
            print(f"  {reader.name}-export: {reader_export['status']}", flush=True)

    passed = sum(1 for check in all_checks if check.get("status") == "PASS")
    failed = len(all_checks) - passed

    payload = {
        "timestamp": datetime.now().isoformat(timespec="seconds"),
        "systemd_version": systemd_version(),
        "writers": [writer.name for writer in writer_specs],
        "readers": [reader.name for reader in reader_specs],
        "compression": args.compression,
        "compression_threshold_bytes": args.compression_threshold_bytes,
        "generated": generated,
        "checks": all_checks,
        "summary": {"total": len(all_checks), "passed": passed, "failed": failed},
    }

    now = datetime.now()
    timestamp = (
        f"{now.year:04d}{now.month:02d}{now.day:02d}-"
        f"{now.hour:02d}{now.minute:02d}{now.second:02d}"
    )
    result_path = LOCAL_DIR / f"compact-matrix-{args.compression}-results-{timestamp}.json"
    result_path.write_text(json.dumps(payload, indent=2) + "\n")

    print("\n=== SUMMARY ===", flush=True)
    print(f"systemd: {payload['systemd_version']}", flush=True)
    print(f"writers: {', '.join(payload['writers'])}", flush=True)
    print(f"compression: {payload['compression']}", flush=True)
    print(f"total: {len(all_checks)}, passed: {passed}, failed: {failed}", flush=True)
    print(f"results: {result_path}", flush=True)

    for check in all_checks:
        if check.get("status") != "PASS":
            print(f"FAIL: {check.get('writer', '?')} {check.get('test', '?')}: {check.get('error', '')}", flush=True)

    if not args.keep_files:
        for ready in FIXTURE_DIR.glob("*.ready"):
            ready.unlink(missing_ok=True)

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
