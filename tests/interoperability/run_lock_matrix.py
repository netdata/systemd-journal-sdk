#!/usr/bin/env python3
"""Cross-SDK writer lock matrix.

Validates the cooperative writer lock contract:
  - one active writer prevents every SDK writer from opening the same file;
  - lock failure happens before a contender publishes its ready file;
  - clean close removes the lock file;
  - stale lock files left by crashed writers are cleaned by the next SDK writer.

Runtime artifacts stay under .local/interoperability/.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path

from run_live_matrix import BIN_DIR, LOCAL_DIR, REPO_ROOT, build_tools, run, systemd_version


WRITERS = ("go", "rust", "node", "python")


def writer_cmd(name: str, tools: dict[str, str], path: Path, ready: Path, entries: int, delay_ms: int, *, crash_after: int = 0) -> list[str]:
    delay = f"{delay_ms}ms"
    if name == "go":
        cmd = [tools["go_livewriter"], "--path", str(path), "--ready-file", str(ready), "--entries", str(entries), "--delay", delay]
    elif name == "rust":
        cmd = [tools["rust_livewriter"], "--path", str(path), "--ready-file", str(ready), "--entries", str(entries), "--delay", delay]
    elif name == "node":
        cmd = [
            "node",
            str(REPO_ROOT / "node/internal/testcmd/livewriter.js"),
            "--path",
            str(path),
            "--ready-file",
            str(ready),
            "--entries",
            str(entries),
            "--delay",
            delay,
        ]
    elif name == "python":
        cmd = [
            "python3",
            str(REPO_ROOT / "python/cmd/livewriter.py"),
            "--path",
            str(path),
            "--ready-file",
            str(ready),
            "--entries",
            str(entries),
            "--delay",
            delay,
        ]
    else:
        raise ValueError(name)
    if crash_after > 0:
        cmd.extend(["--crash-after", str(crash_after)])
    return cmd


def wait_for_ready(ready: Path, proc: subprocess.Popen[str], timeout: float) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if ready.exists():
            return
        if proc.poll() is not None:
            stdout, stderr = proc.communicate(timeout=1)
            raise RuntimeError(f"writer exited before ready file\nstdout:\n{stdout[-1000:]}\nstderr:\n{stderr[-1000:]}")
        time.sleep(0.02)
    proc.terminate()
    try:
        stdout, stderr = proc.communicate(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        stdout, stderr = proc.communicate(timeout=5)
    raise RuntimeError(f"writer did not publish ready file\nstdout:\n{stdout[-1000:]}\nstderr:\n{stderr[-1000:]}")


def start_holder(name: str, tools: dict[str, str], path: Path, ready: Path, entries: int, delay_ms: int) -> subprocess.Popen[str]:
    cmd = writer_cmd(name, tools, path, ready, entries, delay_ms)
    return subprocess.Popen(
        cmd,
        cwd=str(REPO_ROOT),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def contender_attempt(name: str, tools: dict[str, str], path: Path, ready: Path) -> dict:
    ready.unlink(missing_ok=True)
    result = run(writer_cmd(name, tools, path, ready, entries=1, delay_ms=0), timeout=20)
    passed = result.returncode != 0 and not ready.exists()
    return {
        "contender": name,
        "exit_code": result.returncode,
        "ready_file_created": ready.exists(),
        "stderr_tail": result.stderr[-500:],
        "status": "PASS" if passed else "FAIL",
    }


def verify_journal(path: Path) -> dict:
    result = run(["journalctl", "--verify", "--file", str(path)], timeout=30)
    return {
        "exit_code": result.returncode,
        "stderr_tail": result.stderr[-500:],
        "status": "PASS" if result.returncode == 0 else "FAIL",
    }


def run_lock_contention(tools: dict[str, str], entries: int, delay_ms: int) -> list[dict]:
    results = []
    for holder in WRITERS:
        path = LOCAL_DIR / f"lock-{holder}.journal"
        ready = LOCAL_DIR / f"lock-{holder}.ready"
        lock_path = Path(str(path) + ".lock")
        path.unlink(missing_ok=True)
        ready.unlink(missing_ok=True)
        lock_path.unlink(missing_ok=True)

        proc = start_holder(holder, tools, path, ready, entries, delay_ms)
        holder_result = {
            "holder": holder,
            "journal_path": str(path),
            "contenders": [],
            "verify": None,
            "lock_removed_after_close": False,
            "status": "PASS",
            "errors": [],
        }
        try:
            wait_for_ready(ready, proc, timeout=15)
            for contender in WRITERS:
                attempt = contender_attempt(contender, tools, path, LOCAL_DIR / f"lock-{holder}-{contender}.ready")
                holder_result["contenders"].append(attempt)
                if attempt["status"] != "PASS":
                    holder_result["status"] = "FAIL"
                    holder_result["errors"].append(f"{contender} acquired or published while {holder} held lock")

            stdout, stderr = proc.communicate(timeout=max(30, entries * delay_ms / 1000 + 10))
            holder_result["holder_exit_code"] = proc.returncode
            holder_result["holder_stdout_tail"] = stdout[-500:]
            holder_result["holder_stderr_tail"] = stderr[-500:]
            if proc.returncode != 0:
                holder_result["status"] = "FAIL"
                holder_result["errors"].append(f"{holder} exited with {proc.returncode}")

            holder_result["lock_removed_after_close"] = not lock_path.exists()
            if lock_path.exists():
                holder_result["status"] = "FAIL"
                holder_result["errors"].append("lock file remained after clean close")

            holder_result["verify"] = verify_journal(path)
            if holder_result["verify"]["status"] != "PASS":
                holder_result["status"] = "FAIL"
                holder_result["errors"].append("journalctl --verify failed after lock test")
        finally:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.communicate(timeout=5)
            ready.unlink(missing_ok=True)
        results.append(holder_result)
    return results


def run_stale_lock_cleanup(tools: dict[str, str]) -> list[dict]:
    results = []
    for holder in WRITERS:
        contender = WRITERS[(WRITERS.index(holder) + 1) % len(WRITERS)]
        path = LOCAL_DIR / f"stale-lock-{holder}.journal"
        ready = LOCAL_DIR / f"stale-lock-{holder}.ready"
        recovery_ready = LOCAL_DIR / f"stale-lock-{holder}-{contender}.ready"
        lock_path = Path(str(path) + ".lock")
        for item in (path, ready, recovery_ready, lock_path):
            item.unlink(missing_ok=True)

        crash = run(writer_cmd(holder, tools, path, ready, entries=3, delay_ms=0, crash_after=1), timeout=20)
        recovery = run(writer_cmd(contender, tools, path, recovery_ready, entries=1, delay_ms=0), timeout=20)
        verify = verify_journal(path) if recovery.returncode == 0 else {"status": "SKIP", "exit_code": None, "stderr_tail": ""}
        passed = crash.returncode == 17 and recovery.returncode == 0 and recovery_ready.exists() and verify["status"] == "PASS" and not lock_path.exists()
        results.append({
            "holder": holder,
            "recovery_writer": contender,
            "crash_exit_code": crash.returncode,
            "recovery_exit_code": recovery.returncode,
            "recovery_ready_file_created": recovery_ready.exists(),
            "lock_removed_after_recovery": not lock_path.exists(),
            "verify": verify,
            "status": "PASS" if passed else "FAIL",
            "crash_stderr_tail": crash.stderr[-500:],
            "recovery_stderr_tail": recovery.stderr[-500:],
        })
    return results


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--entries", type=int, default=200)
    parser.add_argument("--delay-ms", type=int, default=20)
    args = parser.parse_args()

    LOCAL_DIR.mkdir(parents=True, exist_ok=True)
    BIN_DIR.mkdir(parents=True, exist_ok=True)
    tools = build_tools()

    contention = run_lock_contention(tools, args.entries, args.delay_ms)
    stale = run_stale_lock_cleanup(tools)
    total = len(contention) + len(stale)
    passed = sum(1 for item in contention + stale if item["status"] == "PASS")
    failed = total - passed

    payload = {
        "timestamp": datetime.now().isoformat(timespec="seconds"),
        "systemd_version": systemd_version(),
        "writers": list(WRITERS),
        "entries_per_holder": args.entries,
        "holder_delay_ms": args.delay_ms,
        "contention": contention,
        "stale_lock_cleanup": stale,
        "summary": {"total": total, "passed": passed, "failed": failed},
    }
    now = datetime.now()
    timestamp = (
        f"{now.year:04d}{now.month:02d}{now.day:02d}-"
        f"{now.hour:02d}{now.minute:02d}{now.second:02d}"
    )
    result_path = LOCAL_DIR / f"lock-matrix-results-{timestamp}.json"
    result_path.write_text(json.dumps(payload, indent=2) + "\n")

    print("=== LOCK MATRIX ===", flush=True)
    print(f"systemd: {payload['systemd_version']}", flush=True)
    print(f"writers: {', '.join(WRITERS)}", flush=True)
    print(f"total: {total}, passed: {passed}, failed: {failed}", flush=True)
    print(f"results: {result_path}", flush=True)
    for result in contention:
        print(f"  contention {result['holder']}: {result['status']}", flush=True)
        for error in result["errors"]:
            print(f"    {error}", flush=True)
    for result in stale:
        print(f"  stale {result['holder']} -> {result['recovery_writer']}: {result['status']}", flush=True)

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
