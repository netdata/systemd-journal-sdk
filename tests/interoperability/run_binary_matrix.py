#!/usr/bin/env python3
"""Cross-language binary-field interoperability matrix.

Generates journal files with binary fixture fields from each writer language,
then validates every reader (stock journalctl + every repository implementation)
against each generated file. Runtime artifacts stay under .local/interoperability/.

Binary fixture per writer:
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
FIXTURE_DIR = LOCAL_DIR / "binary"

BINARY_PAYLOAD = bytes([0x00, 0x01, 0x02, 0x41, 0x0a, 0x7f, 0x80, 0xff])
BINARY_MATCH = bytes([0x61, 0x62, 0x63, 0x07, 0x64, 0x65, 0x66])
BINARY_EMPTY = b""


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


def run(cmd: list[str], *, cwd: Path = REPO_ROOT, timeout: int = 120, binary: bool = False) -> subprocess.CompletedProcess:
    return subprocess.run(
        cmd,
        cwd=str(cwd),
        text=None if binary else True,
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

    for src, dst in [
        (REPO_ROOT / "rust/target/debug/livewriter", BIN_DIR / "rust-livewriter"),
        (REPO_ROOT / "rust/target/debug/journalctl", BIN_DIR / "rust-journalctl"),
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


def writer_command(writer: WriterSpec, tools: dict[str, str], target: Path, ready: Path, entries: int) -> list[str]:
    if writer.name == "go":
        return [tools["go_livewriter"], "--path", str(target), "--ready-file", str(ready), "--entries", str(entries), "--delay", "1ms", "--binary-fixture"]
    if writer.name == "rust":
        return [tools["rust_livewriter"], "--dir", str(target), "--ready-file", str(ready), "--entries", str(entries), "--delay", "1ms", "--binary-fixture"]
    if writer.name == "node":
        return ["node", str(REPO_ROOT / "node/internal/testcmd/livewriter.js"), "--path", str(target), "--ready-file", str(ready), "--entries", str(entries), "--delay", "1ms", "--binary-fixture"]
    if writer.name == "python":
        return ["python3", str(REPO_ROOT / "python/cmd/livewriter.py"), "--path", str(target), "--ready-file", str(ready), "--entries", str(entries), "--delay", "1ms", "--binary-fixture"]
    raise ValueError(writer.name)


def generate_journal(writer: WriterSpec, tools: dict[str, str], entries: int) -> dict[str, str]:
    writer_root = FIXTURE_DIR / writer.name
    if writer_root.exists():
        shutil.rmtree(writer_root)
    writer_root.mkdir(parents=True, exist_ok=True)

    ready = FIXTURE_DIR / f"{writer.name}.ready"
    if ready.exists():
        ready.unlink()

    if writer.mode == "directory":
        target = writer_root
    else:
        target = writer_root / f"{writer.name}.journal"

    result = run(writer_command(writer, tools, target, ready, entries), timeout=max(60, entries // 2))
    require_ok(result, f"{writer.name} binary writer")

    wait_for_file(ready, f"{writer.name} ready file")

    if writer.mode == "directory":
        journal_files = sorted(writer_root.rglob("*.journal"))
        if len(journal_files) != 1:
            raise RuntimeError(f"{writer.name} writer expected exactly one journal file, found {len(journal_files)}")
        journal_path = journal_files[0]
    else:
        journal_path = target

    if not journal_path.exists():
        raise RuntimeError(f"{writer.name} binary journal was not created: {journal_path}")

    return {
        "writer": writer.name,
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
        if not line:
            continue
        entries.append(json.loads(line))
    return entries


def json_bytes(value) -> list[int] | None:
    if isinstance(value, list) and all(isinstance(v, int) for v in value):
        return value
    return None


def validate_binary_json_entry(entry: dict) -> list[str]:
    errors = []
    if entry.get("TEST_ID") != "binary-interoperability":
        errors.append(f"TEST_ID={entry.get('TEST_ID')!r}")
    if entry.get("MESSAGE") != "binary interoperability":
        errors.append(f"MESSAGE={entry.get('MESSAGE')!r}")

    bp = json_bytes(entry.get("BINARY_PAYLOAD"))
    if bp is None:
        errors.append(f"BINARY_PAYLOAD not byte array: {entry.get('BINARY_PAYLOAD')!r}")
    elif bp != list(BINARY_PAYLOAD):
        errors.append(f"BINARY_PAYLOAD mismatch: got {bp}, want {list(BINARY_PAYLOAD)}")

    bm = json_bytes(entry.get("BINARY_MATCH"))
    if bm is None:
        errors.append(f"BINARY_MATCH not byte array: {entry.get('BINARY_MATCH')!r}")
    elif bm != list(BINARY_MATCH):
        errors.append(f"BINARY_MATCH mismatch: got {bm}, want {list(BINARY_MATCH)}")

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
    result = run(cmd, timeout=30)
    if result.returncode != 0:
        return {"test": "stock-json", "command": shell_join(cmd), "status": "FAIL", "error": text_tail(result.stderr)}
    try:
        entries = parse_json_lines(result.stdout)
    except Exception as e:
        return {"test": "stock-json", "command": shell_join(cmd), "status": "FAIL", "error": f"JSON parse error: {e}"}
    if len(entries) != 1:
        return {"test": "stock-json", "command": shell_join(cmd), "status": "FAIL", "error": f"expected 1 entry, got {len(entries)}"}
    errors = validate_binary_json_entry(entries[0])
    if errors:
        return {"test": "stock-json", "command": shell_join(cmd), "status": "FAIL", "error": "; ".join(errors)}
    return {"test": "stock-json", "command": shell_join(cmd), "status": "PASS"}


def check_stock_export(journal_path: str) -> dict:
    cmd = ["journalctl", "--file", journal_path, "--output=export", "--quiet", "--no-pager", "TEST_ID=binary-interoperability"]
    result = run(cmd, timeout=30, binary=True)
    if result.returncode != 0:
        return {"test": "stock-export", "command": shell_join(cmd), "status": "FAIL", "error": text_tail(result.stderr)}
    return _validate_export_output_bytes(result.stdout, "stock-export", cmd)


def check_stock_export_binary_match(journal_path: str) -> dict:
    cmd = ["journalctl", "--file", journal_path, "--output=export", "--quiet", "--no-pager", "BINARY_MATCH=abc\x07def"]
    result = run(cmd, timeout=30, binary=True)
    if result.returncode != 0:
        return {"test": "stock-export-match", "command": shell_join(cmd), "status": "FAIL", "error": text_tail(result.stderr)}
    return _validate_export_output_bytes(result.stdout, "stock-export-match", cmd)


def _parse_export_binary_entries(output: bytes) -> dict[str, bytes]:
    """Parse stock export output into field-name to raw value bytes.

    Stock export uses size-prefixed binary format (no '='):
      FIELD_NAME\\n
      <8-byte little-endian size>\\n
      <raw bytes>\\n

    But printable fields use text format:
      FIELD_NAME=value\\n

    Returns dict mapping field name to raw value bytes.
    """
    if isinstance(output, str):
        output = output.encode('latin-1')
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
        line_str = line.decode('latin-1', errors='replace')
        eq_idx = line_str.find('=')
        if eq_idx >= 0:
            name = line_str[:eq_idx]
            value_str = line_str[eq_idx + 1:]
            try:
                value_bytes = value_str.encode('latin-1')
            except Exception:
                value_bytes = value_str.encode('utf-8', errors='replace')
            fields[name] = value_bytes
            continue
        if 0x00 <= line[0] <= 0x09:
            continue
        name = line_str
        if i + 8 > len(output):
            break
        size_bytes = output[i:i + 8]
        size = int.from_bytes(size_bytes, 'little')
        i += 8
        data = output[i:i + size]
        i += size
        if i < len(output) and output[i] == 0x0a:
            i += 1
        fields[name] = data
    return fields


def _validate_export_output_bytes(output: bytes, test_name: str, cmd: list[str]) -> dict:
    fields = _parse_export_binary_entries(output)
    errors = []
    for field, expected_bytes in [("BINARY_PAYLOAD", BINARY_PAYLOAD), ("BINARY_MATCH", BINARY_MATCH)]:
        if field not in fields:
            errors.append(f"{field} missing from export")
            continue
        actual_bytes = fields[field]
        if len(actual_bytes) != len(expected_bytes) or actual_bytes != expected_bytes:
            errors.append(f"{field} mismatch: got {actual_bytes.hex()}, want {expected_bytes.hex()}")
    if "BINARY_EMPTY" not in fields:
        errors.append("BINARY_EMPTY missing from export")
    else:
        actual_bytes = fields["BINARY_EMPTY"]
        if actual_bytes != b"":
            errors.append(f"BINARY_EMPTY should be empty, got: {actual_bytes.hex() if actual_bytes else '(empty)'}")
    if errors:
        return {"test": test_name, "command": shell_join(cmd), "status": "FAIL", "error": "; ".join(errors)}
    return {"test": test_name, "command": shell_join(cmd), "status": "PASS"}


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
            stderr_str = result.stderr.decode('utf-8', errors='replace') if isinstance(result.stderr, bytes) else result.stderr
            errors.append(f"{field}: exit {result.returncode} {stderr_str[-200:]}")
    if errors:
        return {"test": "libsystemd", "status": "FAIL", "error": "; ".join(errors)}
    return {"test": "libsystemd", "status": "PASS"}


def check_reader_json(reader: ReaderSpec, tools: dict[str, str], journal_path: str, writer_name: str) -> dict:
    cmd = _reader_json_cmd(reader, tools, journal_path)
    result = run(cmd, timeout=30)
    if result.returncode != 0:
        return {"writer": writer_name, "reader": reader.name, "test": "json", "command": shell_join(cmd), "status": "FAIL", "error": text_tail(result.stderr)}
    try:
        entries = parse_json_lines(result.stdout)
    except Exception as e:
        return {"writer": writer_name, "reader": reader.name, "test": "json", "command": shell_join(cmd), "status": "FAIL", "error": f"JSON parse error: {e}"}
    if len(entries) != 1:
        return {"writer": writer_name, "reader": reader.name, "test": "json", "command": shell_join(cmd), "status": "FAIL", "error": f"expected 1 entry, got {len(entries)}"}
    field_errors = validate_binary_json_entry(entries[0])
    if field_errors:
        return {"writer": writer_name, "reader": reader.name, "test": "json", "command": shell_join(cmd), "status": "FAIL", "error": "; ".join(field_errors)}
    return {"writer": writer_name, "reader": reader.name, "test": "json", "command": shell_join(cmd), "status": "PASS"}


def check_reader_export(reader: ReaderSpec, tools: dict[str, str], journal_path: str, writer_name: str) -> dict:
    cmd = _reader_export_cmd(reader, tools, journal_path)
    result = run(cmd, timeout=30, binary=True)
    if result.returncode != 0:
        return {"writer": writer_name, "reader": reader.name, "test": "export", "command": shell_join(cmd), "status": "FAIL", "error": text_tail(result.stderr)}
    validation = _validate_export_output_bytes(result.stdout, f"{writer_name}-{reader.name}-export", cmd)
    validation["writer"] = writer_name
    validation["reader"] = reader.name
    return validation


def _reader_json_cmd(reader: ReaderSpec, tools: dict[str, str], journal_path: str) -> list[str]:
    if reader.name == "stock":
        return ["journalctl", "--file", journal_path, "--output=json", "--quiet", "--no-pager", "TEST_ID=binary-interoperability"]
    if reader.name == "go":
        return [tools["go_journalctl"], "--file", journal_path, "--output=json", "TEST_ID=binary-interoperability"]
    if reader.name == "rust":
        return [tools["rust_journalctl"], "--file", journal_path, "--output=json", "TEST_ID=binary-interoperability"]
    if reader.name == "node":
        return ["node", str(REPO_ROOT / "node/cmd/journalctl/index.js"), "--file", journal_path, "--output", "json", "TEST_ID=binary-interoperability"]
    if reader.name == "python":
        return ["python3", str(REPO_ROOT / "python/cmd/journalctl.py"), "--file", journal_path, "--output", "json", "TEST_ID=binary-interoperability"]
    raise ValueError(reader.name)


def _reader_export_cmd(reader: ReaderSpec, tools: dict[str, str], journal_path: str) -> list[str]:
    if reader.name == "stock":
        return ["journalctl", "--file", journal_path, "--output=export", "--quiet", "--no-pager", "TEST_ID=binary-interoperability"]
    if reader.name == "go":
        return [tools["go_journalctl"], "--file", journal_path, "--output=export", "TEST_ID=binary-interoperability"]
    if reader.name == "rust":
        return [tools["rust_journalctl"], "--file", journal_path, "--output=export", "TEST_ID=binary-interoperability"]
    if reader.name == "node":
        return ["node", str(REPO_ROOT / "node/cmd/journalctl/index.js"), "--file", journal_path, "--output", "export", "TEST_ID=binary-interoperability"]
    if reader.name == "python":
        return ["python3", str(REPO_ROOT / "python/cmd/journalctl.py"), "--file", journal_path, "--output=export", "TEST_ID=binary-interoperability"]
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
    parser.add_argument("--keep-files", action="store_true")
    args = parser.parse_args()

    LOCAL_DIR.mkdir(parents=True, exist_ok=True)
    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)

    print("Building tools...")
    tools = build_tools()
    libsystemd_reader = build_libsystemd_reader()

    writer_specs = selected(WRITERS, args.writers)
    reader_specs = selected(READERS, args.readers)

    generated = []
    all_checks: list[dict] = []

    for writer in writer_specs:
        print(f"\n--- Generating {writer.name} binary fixture ---", flush=True)
        try:
            result = generate_journal(writer, tools, 10)
            generated.append(result)
        except Exception as e:
            print(f"ERROR generating {writer.name}: {e}", flush=True)
            all_checks.append({"writer": writer.name, "status": "FAIL", "error": str(e)})
            continue

        journal_path = result["journal_file"]
        print(f"  journal: {journal_path}", flush=True)

        stock_verify = check_stock_verify(journal_path)
        all_checks.append({"writer": writer.name, **stock_verify})
        print(f"  stock-verify: {stock_verify['status']}", flush=True)

        stock_json = check_stock_json(journal_path)
        all_checks.append({"writer": writer.name, **stock_json})
        print(f"  stock-json: {stock_json['status']}", flush=True)

        stock_export = check_stock_export(journal_path)
        all_checks.append({"writer": writer.name, **stock_export})
        print(f"  stock-export: {stock_export['status']}", flush=True)

        stock_export_match = check_stock_export_binary_match(journal_path)
        all_checks.append({"writer": writer.name, **stock_export_match})
        print(f"  stock-export-match: {stock_export_match['status']}", flush=True)

        libsystemd = check_libsystemd(journal_path, libsystemd_reader)
        all_checks.append({"writer": writer.name, **libsystemd})
        print(f"  libsystemd: {libsystemd['status']}", flush=True)

        for reader in reader_specs:
            if reader.name == "stock":
                continue
            reader_json = check_reader_json(reader, tools, journal_path, writer.name)
            all_checks.append(reader_json)
            print(f"  {reader.name}-json: {reader_json['status']}", flush=True)

            reader_export = check_reader_export(reader, tools, journal_path, writer.name)
            all_checks.append(reader_export)
            print(f"  {reader.name}-export: {reader_export['status']}", flush=True)

    passed = sum(1 for c in all_checks if c.get("status") == "PASS")
    failed = len(all_checks) - passed

    payload = {
        "timestamp": datetime.now().isoformat(timespec="seconds"),
        "systemd_version": systemd_version(),
        "writers": [w.name for w in writer_specs],
        "readers": [r.name for r in reader_specs],
        "generated": generated,
        "checks": all_checks,
        "summary": {"total": len(all_checks), "passed": passed, "failed": failed},
        "binary_fixture": {
            "BINARY_PAYLOAD": BINARY_PAYLOAD.hex(),
            "BINARY_MATCH": BINARY_MATCH.hex(),
            "BINARY_EMPTY": "(empty)",
        },
    }

    now = datetime.now()
    timestamp = (
        f"{now.year:04d}{now.month:02d}{now.day:02d}-"
        f"{now.hour:02d}{now.minute:02d}{now.second:02d}"
    )
    result_path = LOCAL_DIR / f"binary-matrix-results-{timestamp}.json"
    result_path.write_text(json.dumps(payload, indent=2) + "\n")

    print("\n=== SUMMARY ===", flush=True)
    print(f"systemd: {payload['systemd_version']}", flush=True)
    print(f"writers: {', '.join([w.name for w in writer_specs])}", flush=True)
    print(f"total: {len(all_checks)}, passed: {passed}, failed: {failed}", flush=True)
    print(f"results: {result_path}", flush=True)

    for check in all_checks:
        if check.get("status") != "PASS":
            print(f"FAIL: {check.get('writer', '?')} {check.get('test', '?')}: {check.get('error', '')}", flush=True)

    if not args.keep_files:
        for f in FIXTURE_DIR.glob("*.ready"):
            f.unlink(missing_ok=True)

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
