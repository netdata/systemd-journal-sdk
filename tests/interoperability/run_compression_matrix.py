#!/usr/bin/env python3
"""Cross-language zstd/xz/lz4-compressed DATA interoperability matrix.

Generates journal files with DATA-object compression enabled from each
writer language, verifies that at least one DATA object is actually compressed,
then validates stock systemd and repository readers against each generated file.
Runtime artifacts stay under .local/interoperability/.

Compression fixture per writer:
  TEST_ID={compression}-interoperability
  MESSAGE={compression} interoperability
  PRIORITY=6
  LIVE_SEQ=000000
  COMPRESSED_PAYLOAD=<256 printable bytes>
  COMPRESSED_MATCH=<first 32 bytes of COMPRESSED_PAYLOAD>
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
FIXTURE_DIR = LOCAL_DIR / "compression"

COMPRESSED_PAYLOAD = bytes((i % 26) + 0x41 for i in range(256))
COMPRESSED_MATCH = COMPRESSED_PAYLOAD[:32]
COMPRESSED_PAYLOAD_TEXT = COMPRESSED_PAYLOAD.decode("ascii")
COMPRESSED_MATCH_TEXT = COMPRESSED_MATCH.decode("ascii")

COMPRESSION_FAMILIES = ["zstd", "xz", "lz4"]
DEFAULT_COMPRESSION_FAMILIES = ["zstd"]


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
        stdout = text_tail(result.stdout)
        stderr = text_tail(result.stderr)
        raise RuntimeError(f"{label} failed with exit {result.returncode}\nstdout:\n{stdout}\nstderr:\n{stderr}")


def systemd_version() -> str:
    result = run(["journalctl", "--version"], timeout=10)
    if result.returncode == 0 and result.stdout.strip():
        return result.stdout.splitlines()[0]
    return "unavailable"


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
    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)
    src = REPO_ROOT / "tests" / "conformance" / "binary" / "libsystemd_binary_field_reader.c"
    dst = BIN_DIR / "libsystemd_binary_field_reader"
    require_ok(
        run(["gcc", "-o", str(dst), str(src), "-Wl,--no-as-needed", "-lsystemd", "-lm", "-lpthread"]),
        "build libsystemd_binary_field_reader",
    )
    return str(dst)


def writer_command(writer: WriterSpec, tools: dict[str, str], target: Path, ready: Path, entries: int, compression: str) -> list[str]:
    common = [
        "--ready-file", str(ready),
        "--entries", str(entries),
        "--delay", "1ms",
        "--compression", compression,
        "--compress-threshold", "16",
        f"--{compression}-fixture",
    ]
    if writer.name == "go":
        return [tools["go_livewriter"], "--path", str(target), *common]
    if writer.name == "rust":
        return [tools["rust_livewriter"], "--dir", str(target), *common]
    if writer.name == "node":
        return ["node", str(REPO_ROOT / "node/internal/testcmd/livewriter.js"), "--path", str(target), *common]
    if writer.name == "python":
        return ["python3", str(REPO_ROOT / "python/cmd/livewriter.py"), "--path", str(target), *common]
    raise ValueError(writer.name)


def generate_journal(writer: WriterSpec, tools: dict[str, str], entries: int, compression: str) -> dict[str, str]:
    writer_root = FIXTURE_DIR / f"{writer.name}-{compression}"
    if writer_root.exists():
        shutil.rmtree(writer_root)
    writer_root.mkdir(parents=True, exist_ok=True)

    ready = FIXTURE_DIR / f"{writer.name}-{compression}.ready"
    ready.unlink(missing_ok=True)

    target = writer_root if writer.mode == "directory" else writer_root / f"{writer.name}-{compression}.journal"
    result = run(writer_command(writer, tools, target, ready, entries, compression), timeout=max(60, entries // 2))
    require_ok(result, f"{writer.name} {compression} compressed writer")
    wait_for_file(ready, f"{writer.name} {compression} ready file")

    if writer.mode == "directory":
        journal_files = sorted(writer_root.rglob("*.journal"))
        if len(journal_files) != 1:
            raise RuntimeError(f"{writer.name} writer expected exactly one journal file, found {len(journal_files)}")
        journal_path = journal_files[0]
    else:
        journal_path = target

    if not journal_path.exists():
        raise RuntimeError(f"{writer.name} compressed journal was not created: {journal_path}")

    return {
        "writer": writer.name,
        "compression": compression,
        "journal_file": str(journal_path),
        "journal_directory": str(writer_root),
    }


def wait_for_file(path: Path, label: str, timeout: float = 10.0) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if path.exists():
            return
        time.sleep(0.05)
    raise RuntimeError(f"timed out waiting for {label}: {path}")


def inspect_compression(journal_path: str, compression: str) -> dict:
    return inspect_journal_structure(
        journal_path,
        expected_compact=False,
        expected_compression=compression,
        test_name="compression-structure",
    )


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


def validate_json_entry(entry: dict, compression: str) -> list[str]:
    errors = []
    expected = {
        "TEST_ID": f"{compression}-interoperability",
        "MESSAGE": f"{compression} interoperability",
        "PRIORITY": "6",
        "LIVE_SEQ": "000000",
        "COMPRESSED_PAYLOAD": COMPRESSED_PAYLOAD_TEXT,
        "COMPRESSED_MATCH": COMPRESSED_MATCH_TEXT,
    }
    for key, value in expected.items():
        if entry.get(key) != value:
            got = entry.get(key)
            if isinstance(got, str) and len(got) > 80:
                got = got[:80] + "..."
            errors.append(f"{key}={got!r}")
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


def check_stock_json(journal_path: str, compression: str) -> dict:
    cmd = ["journalctl", "--file", journal_path, "--output=json", "--quiet", "--no-pager", f"TEST_ID={compression}-interoperability"]
    return _check_json_command(cmd, "stock", "stock-json", compression)


def check_stock_export(journal_path: str, compression: str) -> dict:
    cmd = ["journalctl", "--file", journal_path, "--output=export", "--quiet", "--no-pager", f"TEST_ID={compression}-interoperability"]
    result = run(cmd, timeout=30, binary=True)
    if result.returncode != 0:
        return {"test": "stock-export", "command": shell_join(cmd), "status": "FAIL", "error": text_tail(result.stderr)}
    return _validate_export_output(result.stdout, "stock-export", cmd, compression)


def check_stock_export_match(journal_path: str, compression: str) -> dict:
    cmd = ["journalctl", "--file", journal_path, "--output=export", "--quiet", "--no-pager", f"COMPRESSED_MATCH={COMPRESSED_MATCH_TEXT}"]
    result = run(cmd, timeout=30, binary=True)
    if result.returncode != 0:
        return {"test": "stock-export-match", "command": shell_join(cmd), "status": "FAIL", "error": text_tail(result.stderr)}
    return _validate_export_output(result.stdout, "stock-export-match", cmd, compression)


def check_libsystemd(journal_path: str, libsystemd_reader: str, compression: str) -> dict:
    errors = []
    for field, expected_hex in [
        ("COMPRESSED_PAYLOAD", COMPRESSED_PAYLOAD.hex()),
        ("COMPRESSED_MATCH", COMPRESSED_MATCH.hex()),
    ]:
        cmd = [libsystemd_reader, journal_path, field, expected_hex, f"TEST_ID={compression}-interoperability"]
        result = run(cmd, timeout=30)
        if result.returncode != 0:
            errors.append(f"{field}: exit {result.returncode} {text_tail(result.stderr, 200)}")
    if errors:
        return {"test": "libsystemd", "status": "FAIL", "error": "; ".join(errors)}
    return {"test": "libsystemd", "status": "PASS"}


def check_reader_json(reader: ReaderSpec, tools: dict[str, str], journal_path: str, writer_name: str, compression: str) -> dict:
    cmd = _reader_json_cmd(reader, tools, journal_path, compression)
    result = _check_json_command(cmd, reader.name, "json", compression)
    result["writer"] = writer_name
    result["reader"] = reader.name
    return result


def check_reader_export(reader: ReaderSpec, tools: dict[str, str], journal_path: str, writer_name: str, compression: str) -> dict:
    cmd = _reader_export_cmd(reader, tools, journal_path, compression)
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
    validation = _validate_export_output(result.stdout, "export", cmd, compression)
    validation["writer"] = writer_name
    validation["reader"] = reader.name
    return validation


def check_reader_export_match(reader: ReaderSpec, tools: dict[str, str], journal_path: str, writer_name: str, compression: str) -> dict:
    cmd = _reader_export_match_cmd(reader, tools, journal_path, compression)
    result = run(cmd, timeout=30, binary=True)
    if result.returncode != 0:
        return {
            "writer": writer_name,
            "reader": reader.name,
            "test": "export-match",
            "command": shell_join(cmd),
            "status": "FAIL",
            "error": text_tail(result.stderr),
        }
    validation = _validate_export_output(result.stdout, "export-match", cmd, compression)
    validation["writer"] = writer_name
    validation["reader"] = reader.name
    return validation


def _check_json_command(cmd: list[str], reader_name: str, test_name: str, compression: str) -> dict:
    result = run(cmd, timeout=30)
    if result.returncode != 0:
        return {"reader": reader_name, "test": test_name, "command": shell_join(cmd), "status": "FAIL", "error": text_tail(result.stderr)}
    try:
        entries = parse_json_lines(result.stdout)
    except Exception as e:
        return {"reader": reader_name, "test": test_name, "command": shell_join(cmd), "status": "FAIL", "error": f"JSON parse error: {e}"}
    if len(entries) != 1:
        return {"reader": reader_name, "test": test_name, "command": shell_join(cmd), "status": "FAIL", "error": f"expected 1 entry, got {len(entries)}"}
    field_errors = validate_json_entry(entries[0], compression)
    if field_errors:
        return {"reader": reader_name, "test": test_name, "command": shell_join(cmd), "status": "FAIL", "error": "; ".join(field_errors)}
    return {"reader": reader_name, "test": test_name, "command": shell_join(cmd), "status": "PASS"}


def _parse_export_entries(output: bytes) -> dict[str, bytes]:
    fields: dict[str, bytes] = {}
    i = 0
    while i < len(output):
        if output[i] == 0x0a:
            i += 1
            continue
        line_start = i
        while i < len(output) and output[i] != 0x0a:
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
        if i < len(output) and output[i] == 0x0a:
            i += 1
    return fields


def _validate_export_output(output: bytes, test_name: str, cmd: list[str], compression: str) -> dict:
    fields = _parse_export_entries(output)
    errors = []
    for field, expected in [
        ("TEST_ID", f"{compression}-interoperability".encode()),
        ("MESSAGE", f"{compression} interoperability".encode()),
        ("COMPRESSED_PAYLOAD", COMPRESSED_PAYLOAD),
        ("COMPRESSED_MATCH", COMPRESSED_MATCH),
    ]:
        actual = fields.get(field)
        if actual != expected:
            got = actual.hex() if isinstance(actual, bytes) else repr(actual)
            errors.append(f"{field} mismatch: got {got}, want {expected.hex()}")
    if errors:
        return {"test": test_name, "command": shell_join(cmd), "status": "FAIL", "error": "; ".join(errors)}
    return {"test": test_name, "command": shell_join(cmd), "status": "PASS"}


def _reader_json_cmd(reader: ReaderSpec, tools: dict[str, str], journal_path: str, compression: str) -> list[str]:
    match_arg = f"TEST_ID={compression}-interoperability"
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


def _reader_export_cmd(reader: ReaderSpec, tools: dict[str, str], journal_path: str, compression: str) -> list[str]:
    match_arg = f"TEST_ID={compression}-interoperability"
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


def _reader_export_match_cmd(reader: ReaderSpec, tools: dict[str, str], journal_path: str, compression: str) -> list[str]:
    match_arg = f"COMPRESSED_MATCH={COMPRESSED_MATCH_TEXT}"
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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--writers", nargs="*", choices=sorted(WRITERS))
    parser.add_argument("--readers", nargs="*", choices=sorted(READERS))
    parser.add_argument("--compression", nargs="*", choices=COMPRESSION_FAMILIES, default=DEFAULT_COMPRESSION_FAMILIES)
    parser.add_argument("--entries", type=int, default=10)
    parser.add_argument("--keep-files", action="store_true")
    args = parser.parse_args()

    LOCAL_DIR.mkdir(parents=True, exist_ok=True)
    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)

    print("Building tools...")
    tools = build_tools()
    libsystemd_reader = build_libsystemd_reader()

    writer_specs = selected(WRITERS, args.writers)
    reader_specs = selected(READERS, args.readers)
    compression_families = args.compression

    generated = []
    all_checks: list[dict] = []

    for compression in compression_families:
        for writer in writer_specs:
            print(f"\n--- Generating {writer.name} {compression} fixture ---", flush=True)
            try:
                result = generate_journal(writer, tools, args.entries, compression)
                generated.append(result)
            except Exception as e:
                print(f"ERROR generating {writer.name} {compression}: {e}", flush=True)
                all_checks.append({"writer": writer.name, "compression": compression, "status": "FAIL", "error": str(e)})
                continue

            journal_path = result["journal_file"]
            print(f"  journal: {journal_path}", flush=True)

            for check in [
                inspect_compression(journal_path, compression),
                check_stock_verify(journal_path),
                check_stock_json(journal_path, compression),
                check_stock_export(journal_path, compression),
                check_stock_export_match(journal_path, compression),
                check_libsystemd(journal_path, libsystemd_reader, compression),
            ]:
                all_checks.append({"writer": writer.name, "compression": compression, **check})
                print(f"  {check['test']}: {check['status']}", flush=True)

            for reader in reader_specs:
                if reader.name == "stock":
                    continue
                for check in [
                    check_reader_json(reader, tools, journal_path, writer.name, compression),
                    check_reader_export(reader, tools, journal_path, writer.name, compression),
                    check_reader_export_match(reader, tools, journal_path, writer.name, compression),
                ]:
                    all_checks.append(check)
                    print(f"  {reader.name}-{check['test']}: {check['status']}", flush=True)

    passed = sum(1 for c in all_checks if c.get("status") == "PASS")
    failed = len(all_checks) - passed

    payload = {
        "timestamp": datetime.now().isoformat(timespec="seconds"),
        "systemd_version": systemd_version(),
        "writers": [w.name for w in writer_specs],
        "readers": [r.name for r in reader_specs],
        "compression_families": compression_families,
        "generated": generated,
        "checks": all_checks,
        "summary": {"total": len(all_checks), "passed": passed, "failed": failed},
        "compression_fixture": {
            "COMPRESSED_PAYLOAD": COMPRESSED_PAYLOAD.hex(),
            "COMPRESSED_MATCH": COMPRESSED_MATCH.hex(),
        },
    }

    now = datetime.now()
    timestamp = (
        f"{now.year:04d}{now.month:02d}{now.day:02d}-"
        f"{now.hour:02d}{now.minute:02d}{now.second:02d}"
    )
    result_path = LOCAL_DIR / f"compression-matrix-results-{timestamp}.json"
    result_path.write_text(json.dumps(payload, indent=2) + "\n")

    print("\n=== SUMMARY ===", flush=True)
    print(f"systemd: {payload['systemd_version']}", flush=True)
    print(f"writers: {', '.join([w.name for w in writer_specs])}", flush=True)
    print(f"compression: {', '.join(compression_families)}", flush=True)
    print(f"total: {len(all_checks)}, passed: {passed}, failed: {failed}", flush=True)
    print(f"results: {result_path}", flush=True)

    for check in all_checks:
        if check.get("status") != "PASS":
            print(f"FAIL: {check.get('writer', '?')} {check.get('reader', '')} {check.get('test', '?')}: {check.get('error', '')}", flush=True)

    if not args.keep_files:
        for f in FIXTURE_DIR.glob("*.ready"):
            f.unlink(missing_ok=True)

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
