#!/usr/bin/env python3
"""Live cross-language interoperability matrix.

Starts one writer per language and polls multiple readers while the writer is
actively appending. Validates:
  - at least one reader observation happens while the writer is still active;
  - observed sequences are ordered prefixes of LIVE_SEQ;
  - final reader snapshots include all expected entries in order;
  - stock journalctl --verify --file passes for generated files.

Runtime artifacts stay under .local/interoperability/.

For directory-mode writers, the runner discovers the active `.journal` file
after the writer publishes the ready file, then passes that file to each reader.
Directory traversal behavior is tracked separately from live file compatibility.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
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
    mode: str  # "file" or "directory"


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

def run(cmd: list[str], *, cwd: Path = REPO_ROOT, timeout: int = 120) -> subprocess.CompletedProcess[str]:
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


def shell_join(cmd: Iterable[str]) -> str:
    return " ".join(json.dumps(part) if any(ch.isspace() for ch in part) else part for part in cmd)


# ----------------------------------------------------------------------
# Build helpers
# ----------------------------------------------------------------------

def build_tools() -> dict[str, str]:
    BIN_DIR.mkdir(parents=True, exist_ok=True)

    require_ok(
        run(["go", "build", "-o", str(BIN_DIR / "go-livewriter"), "./internal/testcmd/livewriter"], cwd=REPO_ROOT / "go"),
        "build go livewriter",
    )
    require_ok(
        run(["go", "build", "-o", str(BIN_DIR / "go-journalctl"), "./cmd/journalctl"], cwd=REPO_ROOT / "go"),
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
        if not (BIN_DIR / name).exists():
            raise RuntimeError(f"expected binary not found: {BIN_DIR / name}")

    return {
        "go_livewriter": str(BIN_DIR / "go-livewriter"),
        "go_journalctl": str(BIN_DIR / "go-journalctl"),
        "rust_livewriter": str(BIN_DIR / "rust-livewriter"),
        "rust_journalctl": str(BIN_DIR / "rust-journalctl"),
    }


# ----------------------------------------------------------------------
# Writer command construction
# ----------------------------------------------------------------------

def writer_cmd(writer: WriterSpec, tools: dict[str, str], target: Path, ready: Path, entries: int, delay_ms: int = 1) -> list[str]:
    delay = f"{delay_ms}ms"
    if writer.name == "go":
        return [tools["go_livewriter"], "--path", str(target), "--ready-file", str(ready), "--entries", str(entries), "--delay", delay]
    if writer.name == "rust":
        return [tools["rust_livewriter"], "--dir", str(target), "--ready-file", str(ready), "--entries", str(entries), "--delay", delay]
    if writer.name == "node":
        return ["node", str(REPO_ROOT / "node/internal/testcmd/livewriter.js"), "--path", str(target), "--ready-file", str(ready), "--entries", str(entries), "--delay", f"{delay_ms}ms"]
    if writer.name == "python":
        return ["python3", str(REPO_ROOT / "python/cmd/livewriter.py"), "--path", str(target), "--ready-file", str(ready), "--entries", str(entries), "--delay", f"{delay_ms}ms"]
    raise ValueError(writer.name)


# ----------------------------------------------------------------------
# Reader command construction
# ----------------------------------------------------------------------

def reader_cmd(reader: ReaderSpec, tools: dict[str, str], journal_path: str, matches: list[str]) -> list[str]:
    """Build a file-backed reader command."""
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


# ----------------------------------------------------------------------
# JSON helpers
# ----------------------------------------------------------------------

def parse_json_lines(stdout: str, source: str) -> list[dict]:
    entries = []
    for line in stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            entries.append(json.loads(line))
        except json.JSONDecodeError as error:
            raise RuntimeError(f"{source}: invalid JSON line {line!r}: {error}") from error
    return entries


# ----------------------------------------------------------------------
# Live matrix runner for one writer
# ----------------------------------------------------------------------


def run_one_live(
    writer_spec: WriterSpec,
    tools: dict[str, str],
    reader_specs: list[ReaderSpec],
    entries: int,
    num_poll_readers: int,
    poll_sec: float,
    writer_delay_ms: int,
) -> dict:
    """Run live matrix for one writer language."""

    # Clean workspace
    writer_root = LOCAL_DIR / "live" / writer_spec.name
    if writer_root.exists():
        shutil.rmtree(writer_root)
    writer_root.mkdir(parents=True, exist_ok=True)

    ready_file = LOCAL_DIR / f"{writer_spec.name}.ready"
    if ready_file.exists():
        ready_file.unlink()

    # Target path
    if writer_spec.mode == "directory":
        target = writer_root
    else:
        target = writer_root / f"{writer_spec.name}.journal"

    cmd = writer_cmd(writer_spec, tools, target, ready_file, entries, writer_delay_ms)

    # Start writer
    writer_proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    # Wait for ready signal
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        if writer_proc.poll() is not None:
            stdout, stderr = writer_proc.communicate(timeout=1)
            raise RuntimeError(
                f"writer {writer_spec.name} exited early with {writer_proc.returncode}; "
                f"stderr={stderr[-500:]}"
            )
        if ready_file.exists():
            if writer_spec.mode == "file":
                if target.exists() and target.stat().st_size > 0:
                    break
            else:
                journals = list(writer_root.rglob("*.journal"))
                if journals and journals[0].stat().st_size > 0:
                    break
        time.sleep(0.02)
    else:
        writer_proc.terminate()
        raise RuntimeError(f"writer {writer_spec.name} ready timeout after 30s")

    if writer_spec.mode == "file":
        journal_file = target
    else:
        journal_files = sorted(writer_root.rglob("*.journal"))
        if len(journal_files) != 1:
            writer_proc.terminate()
            raise RuntimeError(
                f"writer {writer_spec.name} expected exactly one active journal file, "
                f"found {len(journal_files)}"
            )
        journal_file = journal_files[0]
    journal_path = str(journal_file)

    results_active = []
    results_final = []
    lock = threading.Lock()
    stop_poll = threading.Event()
    writer_finished = threading.Event()

    def do_poll(reader_name: str, cmd: list[str]) -> dict:
        best_seq = []
        best_count = 0
        last_error = ""
        while not stop_poll.is_set():
            try:
                active_at_start = not writer_finished.is_set()
                res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=8)
                active_at_end = not writer_finished.is_set()
                if active_at_start and active_at_end and res.returncode == 0:
                    parsed = parse_json_lines(res.stdout, reader_name)
                    if parsed:
                        seqs = [str(e.get("LIVE_SEQ", "")) for e in parsed]
                        if len(seqs) > best_count:
                            best_count = len(seqs)
                            best_seq = seqs
                            last_error = ""
                elif res.returncode != 0:
                    last_error = f"exit {res.returncode}: {res.stderr[-200:]}"
            except subprocess.TimeoutExpired:
                last_error = "reader poll timed out"
            except Exception as error:
                last_error = str(error)
            time.sleep(poll_sec)
        return {
            "reader": reader_name,
            "while_active": bool(best_seq),
            "seq_observed": best_seq,
            "entries_count": best_count,
            "command": shell_join(cmd),
            "error": "" if best_seq else last_error,
        }

    def do_final(reader_name: str, cmd: list[str]) -> dict:
        try:
            res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=30)
            if res.returncode == 0:
                parsed = parse_json_lines(res.stdout, reader_name)
                seqs = [str(e.get("LIVE_SEQ", "")) for e in parsed]
                return {
                    "reader": reader_name,
                    "while_active": False,
                    "seq_observed": seqs,
                    "entries_count": len(seqs),
                    "command": shell_join(cmd),
                    "error": "",
                }
            else:
                return {
                    "reader": reader_name,
                    "while_active": False,
                    "seq_observed": [],
                    "entries_count": 0,
                    "command": shell_join(cmd),
                    "error": f"exit {res.returncode}: {res.stderr[-200:]}",
                }
        except Exception as e:
            return {
                "reader": reader_name,
                "while_active": False,
                "seq_observed": [],
                "entries_count": 0,
                "command": shell_join(cmd),
                "error": str(e),
            }

    eligible_readers = []
    for reader_spec in reader_specs:
        for idx in range(num_poll_readers):
            eligible_readers.append((f"{reader_spec.name}-{idx}", reader_spec, idx))

    poll_futures = []
    with ThreadPoolExecutor(max_workers=len(eligible_readers) + 4) as executor:
        for fname, rspec, idx in eligible_readers:
            rcmd = reader_cmd(rspec, tools, journal_path, ["PRIORITY=6"])
            poll_futures.append(executor.submit(do_poll, fname, rcmd))

        # Wait for writer to finish
        try:
            writer_stdout, writer_stderr = writer_proc.communicate(timeout=90)
        except subprocess.TimeoutExpired:
            writer_proc.terminate()
            writer_proc.wait(timeout=5)
            raise RuntimeError(f"writer {writer_spec.name} did not finish within 90s")
        finally:
            writer_finished.set()
            stop_poll.set()

        exit_code = writer_proc.returncode

        for f in as_completed(poll_futures):
            try:
                result = f.result(timeout=10)
                with lock:
                    results_active.append(result)
            except Exception:
                pass

        for fname, rspec, idx in eligible_readers:
            rcmd = reader_cmd(rspec, tools, journal_path, ["PRIORITY=6"])
            try:
                res = do_final(fname, rcmd)
                with lock:
                    results_final.append(res)
            except Exception as e:
                with lock:
                    results_final.append({
                        "reader": fname, "while_active": False,
                        "seq_observed": [], "entries_count": 0,
                        "command": shell_join(rcmd), "error": str(e),
                    })

    vcmd = ["journalctl", "--verify", "--file", journal_path]
    vp = subprocess.run(vcmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=30)
    verify = {"command": shell_join(vcmd), "returncode": vp.returncode, "stderr": vp.stderr[-300:]}

    return {
        "writer": writer_spec.name,
        "journal_path": journal_path,
        "journal_mode": writer_spec.mode,
        "entries": entries,
        "exit_code": exit_code,
        "active_polls": results_active,
        "final_reads": results_final,
        "verify": verify,
        "writer_stderr": writer_stderr[-500:] if writer_stderr else "",
    }


def assess(result: dict, entries: int) -> tuple[str, list[str]]:
    errors = []
    expected = [f"{i:06d}" for i in range(entries)]
    if result["exit_code"] != 0:
        errors.append(f"writer exit {result['exit_code']}")

    active_with_entries = [o for o in result["active_polls"] if o.get("entries_count", 0) > 0]
    if not active_with_entries:
        errors.append("no reader observed entries while writer was actively writing")
    if len(active_with_entries) != len(result["active_polls"]):
        missing = [
            o["reader"]
            for o in result["active_polls"]
            if o.get("entries_count", 0) == 0
        ]
        errors.append(f"active readers with no live entries: {', '.join(missing)}")
    for obs in active_with_entries:
        observed = obs.get("seq_observed", [])
        if observed != expected[:len(observed)]:
            errors.append(
                f"{obs['reader']}: active sequence is not an ordered prefix, "
                f"got {observed[:3]}... len={len(observed)}"
            )

    for obs in result["final_reads"]:
        if obs.get("error"):
            errors.append(f"{obs['reader']}: {obs['error']}")
            continue
        if obs.get("entries_count", 0) != entries:
            errors.append(
                f"{obs['reader']}: expected {entries} entries, got {obs.get('entries_count', 0)} "
                f"(seq={obs.get('seq_observed', [])[:3]}...)"
            )
        else:
            observed = obs.get("seq_observed", [])
            if observed != expected:
                errors.append(
                    f"{obs['reader']}: sequence mismatch, got {observed[:3]}... "
                    f"len={len(observed)}, expected len={entries}"
                )

    if result.get("verify") and result["verify"].get("returncode") != 0:
        errors.append(f"verify failed: {result['verify'].get('stderr', '')}")

    return "PASS" if not errors else "FAIL", errors


def selected(mapping: dict[str, object], names: list[str] | None):
    if not names:
        return list(mapping.values())
    missing = [name for name in names if name not in mapping]
    if missing:
        raise SystemExit(f"unknown names: {', '.join(missing)}")
    return [mapping[name] for name in names]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--entries", type=int, default=30)
    parser.add_argument("--writers", nargs="*", choices=sorted(WRITERS))
    parser.add_argument("--readers", nargs="*", choices=sorted(READERS))
    parser.add_argument("--poll-readers", type=int, default=2,
                        help="number of polling reader tasks per language (default: 2)")
    parser.add_argument("--poll-interval", type=float, default=0.1,
                        help="seconds between poll attempts (default: 0.1)")
    parser.add_argument("--writer-delay-ms", type=int, default=20,
                        help="delay between writer appends in milliseconds (default: 20)")
    parser.add_argument("--keep-files", action="store_true")
    args = parser.parse_args()

    if args.entries < 5:
        raise SystemExit("--entries must be at least 5")
    if args.writer_delay_ms < 0:
        raise SystemExit("--writer-delay-ms must be non-negative")

    LOCAL_DIR.mkdir(parents=True, exist_ok=True)
    tools = build_tools()
    writer_specs = selected(WRITERS, args.writers)
    reader_specs = selected(READERS, args.readers)

    results = []
    all_passed = True

    for ws in writer_specs:
        print(f"\n--- {ws.name} writer ---", flush=True)
        try:
            result = run_one_live(
                ws,
                tools,
                reader_specs,
                args.entries,
                args.poll_readers,
                args.poll_interval,
                args.writer_delay_ms,
            )
        except Exception as e:
            print(f"ERROR: {e}", flush=True)
            results.append({"writer": ws.name, "error": str(e), "status": "FAIL"})
            all_passed = False
            continue

        status, errors = assess(result, args.entries)
        result["status"] = status
        result["errors"] = errors
        results.append(result)

        active_with_entries = [o for o in result["active_polls"] if o.get("entries_count", 0) > 0]
        print(f"  exit={result['exit_code']}", flush=True)
        print(f"  active polls with entries: {len(active_with_entries)}/{len(result['active_polls'])}", flush=True)
        for o in active_with_entries[:3]:
            print(f"    {o['reader']}: {o['entries_count']} entries, seq={o['seq_observed'][:3]}...", flush=True)
        final_full = [o for o in result["final_reads"] if o.get("entries_count", 0) == args.entries]
        print(f"  final reads with all {args.entries} entries: {len(final_full)}/{len(result['final_reads'])}", flush=True)
        if result.get("verify"):
            print(f"  verify: rc={result['verify']['returncode']}", flush=True)
        if status == "FAIL":
            all_passed = False
            for err in errors:
                print(f"  FAIL: {err}", flush=True)
        else:
            print(f"  status: {status}", flush=True)

    total = len(results)
    passed = sum(1 for r in results if r.get("status") == "PASS")
    failed = total - passed

    payload = {
        "timestamp": datetime.now().isoformat(timespec="seconds"),
        "systemd_version": systemd_version(),
        "entries_per_writer": args.entries,
        "writers": [ws.name for ws in writer_specs],
        "readers": [reader.name for reader in reader_specs],
        "poll_readers_per_lang": args.poll_readers,
        "poll_interval_sec": args.poll_interval,
        "writer_delay_ms": args.writer_delay_ms,
        "results": results,
        "summary": {"total": total, "passed": passed, "failed": failed},
    }

    result_path = LOCAL_DIR / f"live-matrix-results-{datetime.now().strftime('%Y%m%d-%H%M%S')}.json"
    result_path.write_text(json.dumps(payload, indent=2) + "\n")

    print(f"\n=== SUMMARY ===", flush=True)
    print(f"systemd: {payload['systemd_version']}", flush=True)
    print(f"writers: {', '.join([ws.name for ws in writer_specs])}", flush=True)
    print(f"entries per writer: {args.entries}", flush=True)
    print(f"total: {total}, passed: {passed}, failed: {failed}", flush=True)
    print(f"results: {result_path}", flush=True)

    if not args.keep_files:
        for f in LOCAL_DIR.glob("*.ready"):
            f.unlink(missing_ok=True)

    return 0 if all_passed else 1


if __name__ == "__main__":
    sys.exit(main())
