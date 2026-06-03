#!/usr/bin/env python3
"""Streaming, metrics-only real-world journal corpus evaluation harness."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess  # nosec B404 - subprocess is required by harnesses.
import sys
import threading
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, BinaryIO

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from tests.corpus_eval.canonical import SCHEMA_VERSION, digest_export_stream


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUT = ROOT / ".local" / "corpus-eval"
BIN_DIR = DEFAULT_OUT / "bin"
JOURNAL_SUFFIXES = (".journal", ".journal.zst")
RESOURCE_TIME_FORMAT = (
    '{"process_wall_seconds":%e,'
    '"process_user_seconds":%U,'
    '"process_system_seconds":%S,'
    '"max_rss_kb":%M,'
    '"minor_page_faults":%R,'
    '"major_page_faults":%F,'
    '"fs_inputs":%I,'
    '"fs_outputs":%O,'
    '"voluntary_context_switches":%w,'
    '"involuntary_context_switches":%c}'
)


@dataclass(frozen=True)
class ToolPaths:
    rust_digest: Path
    rust_regenerate: Path
    rust_writer_core: Path
    go_digest: Path
    go_regenerate: Path
    journalctl: str
    journal_remote: str | None


@dataclass(frozen=True)
class JournalCase:
    path: Path
    root: Path
    file_id: str
    size: int
    mtime_ns: int
    suffix: str
    identity: dict[str, Any]


def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def run_env() -> dict[str, str]:
    local = ROOT / ".local"
    env = os.environ.copy()
    env.update(
        {
            "CARGO_HOME": str(local / "cargo-home"),
            "CARGO_TARGET_DIR": str(local / "cargo-target"),
            "GOCACHE": str(local / "go-cache"),
            "GOMODCACHE": str(local / "go-mod-cache"),
            "GOPATH": str(local / "go-path"),
            "npm_config_cache": str(local / "npm-cache"),
            "PIP_CACHE_DIR": str(local / "pip-cache"),
            "PYTHONPATH": str(ROOT / "python"),
        }
    )
    return env


def command_digest(cmd: list[str]) -> str:
    encoded = b"\0".join(part.encode("utf-8", "surrogateescape") for part in cmd)
    return hashlib.sha256(encoded).hexdigest()


def load_json(path: Path, default: Any) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return default


def write_json(path: Path, data: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def safe_file_id(root: Path, path: Path, stat: os.stat_result) -> str:
    try:
        rel = path.relative_to(root)
    except ValueError:
        rel = Path(path.name)
    seed = {
        "root": hashlib.sha256(str(root.resolve()).encode("utf-8", "surrogateescape")).hexdigest(),
        "relative": str(rel),
        "size": stat.st_size,
        "mtime_ns": stat.st_mtime_ns,
        "ctime_ns": stat.st_ctime_ns,
        "dev": stat.st_dev,
        "ino": stat.st_ino,
    }
    return hashlib.sha256(json.dumps(seed, sort_keys=True).encode("utf-8")).hexdigest()[:24]


def discover_cases(roots: list[Path], *, max_files: int | None = None) -> list[JournalCase]:
    cases: list[JournalCase] = []
    for root in roots:
        root = root.resolve()
        if not root.exists():
            raise SystemExit(f"input root does not exist: {root}")
        if root.is_file():
            candidates = [root]
        else:
            candidates = []
            for dirpath, _, filenames in os.walk(root):
                for filename in filenames:
                    if filename.endswith(JOURNAL_SUFFIXES):
                        candidates.append(Path(dirpath) / filename)
        for path in sorted(candidates):
            if not path.name.endswith(JOURNAL_SUFFIXES):
                continue
            stat = path.stat()
            file_id = safe_file_id(root, path, stat)
            suffix = ".journal.zst" if path.name.endswith(".journal.zst") else ".journal"
            cases.append(
                JournalCase(
                    path=path,
                    root=root,
                    file_id=file_id,
                    size=stat.st_size,
                    mtime_ns=stat.st_mtime_ns,
                    suffix=suffix,
                    identity={
                        "file_id": file_id,
                        "size": stat.st_size,
                        "mtime_ns": stat.st_mtime_ns,
                        "ctime_ns": stat.st_ctime_ns,
                        "suffix": suffix,
                    },
                )
            )
            if max_files is not None and len(cases) >= max_files:
                return cases
    return cases


def summarize_discovery(cases: list[JournalCase]) -> dict[str, Any]:
    total_bytes = sum(case.size for case in cases)
    suffix_counts: dict[str, int] = {}
    largest = 0
    for case in cases:
        suffix_counts[case.suffix] = suffix_counts.get(case.suffix, 0) + 1
        largest = max(largest, case.size)
    return {
        "files": len(cases),
        "total_input_bytes": total_bytes,
        "largest_input_bytes": largest,
        "suffix_counts": suffix_counts,
        "estimated_min_scratch_bytes_per_file": largest * 4 if largest else 0,
        "scratch_estimate_includes_input_snapshot": True,
    }


def snapshot_case(case: JournalCase, work_dir: Path) -> JournalCase:
    """Create a bounded per-file input snapshot for consistent driver compares."""
    snapshot_dir = work_dir / "snapshots"
    snapshot_dir.mkdir(parents=True, exist_ok=True)
    suffix = ".journal.zst" if case.suffix == ".journal.zst" else ".journal"
    snapshot = snapshot_dir / f"{case.file_id}{suffix}"
    try:
        snapshot.unlink()
    except FileNotFoundError:
        pass
    shutil.copyfile(case.path, snapshot)
    stat = snapshot.stat()
    return JournalCase(
        path=snapshot,
        root=snapshot_dir,
        file_id=case.file_id,
        size=stat.st_size,
        mtime_ns=stat.st_mtime_ns,
        suffix=case.suffix,
        identity=case.identity,
    )


def case_keys(case: JournalCase, args: argparse.Namespace) -> list[str]:
    keys = [f"{case.file_id}:reader:{driver}" for driver in args.drivers]
    keys.extend(
        f"{case.file_id}:writer:{driver}:{mode}"
        for driver in args.regenerators
        for mode in args.regeneration_modes
    )
    return keys


def build_tools(env: dict[str, str], out: Path) -> ToolPaths:
    BIN_DIR.mkdir(parents=True, exist_ok=True)
    commands = [
        (
            "build rust corpus tools",
            ["cargo", "build", "--release", "-p", "corpus_digest", "-p", "corpus_regenerate", "-p", "writer_core_bench"],
            ROOT / "rust",
        ),
        (
            "build go corpus digest",
            ["go", "build", "-o", str(BIN_DIR / "go-corpus-digest"), "./internal/testcmd/corpus_digest"],
            ROOT / "go",
        ),
        (
            "build go corpus regenerate",
            ["go", "build", "-o", str(BIN_DIR / "go-corpus-regenerate"), "./internal/testcmd/corpus_regenerate"],
            ROOT / "go",
        ),
    ]
    build_results = []
    for label, cmd, cwd in commands:
        started = time.perf_counter()
        # nosemgrep
        # subprocess is required by this harness; commands are shell=False vectors.
        result = subprocess.run(  # nosec B603 - harness uses shell=False command vectors.
            cmd,
            cwd=str(cwd),
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=1800,
            check=False,
        )
        build_results.append(
            {
                "label": label,
                "returncode": result.returncode,
                "seconds": time.perf_counter() - started,
                "stdout_sha256": hashlib.sha256(result.stdout.encode()).hexdigest(),
                "stderr_sha256": hashlib.sha256(result.stderr.encode()).hexdigest(),
            }
        )
        if result.returncode != 0:
            write_json(out / "build-results.json", build_results)
            raise RuntimeError(f"{label} failed; see {out / 'build-results.json'}")
    write_json(out / "build-results.json", build_results)

    journalctl = shutil.which("journalctl")
    if journalctl is None:
        raise RuntimeError("journalctl is required for the systemd baseline")
    return ToolPaths(
        rust_digest=ROOT / ".local" / "cargo-target" / "release" / "corpus_digest",
        rust_regenerate=ROOT / ".local" / "cargo-target" / "release" / "corpus_regenerate",
        rust_writer_core=ROOT / ".local" / "cargo-target" / "release" / "writer_core_bench",
        go_digest=BIN_DIR / "go-corpus-digest",
        go_regenerate=BIN_DIR / "go-corpus-regenerate",
        journalctl=journalctl,
        journal_remote=shutil.which("systemd-journal-remote"),
    )


def parse_time_stats(path: Path) -> dict[str, Any]:
    data = load_json(path, {})
    if not isinstance(data, dict):
        return {}
    page_size = os.sysconf("SC_PAGE_SIZE")
    fs_block_size = 512
    wall = float(data.get("process_wall_seconds") or 0)
    fs_input_bytes = int(data.get("fs_inputs") or 0) * fs_block_size
    fs_output_bytes = int(data.get("fs_outputs") or 0) * fs_block_size
    major_fault_bytes = int(data.get("major_page_faults") or 0) * page_size
    data.update(
        {
            "fs_input_bytes": fs_input_bytes,
            "fs_output_bytes": fs_output_bytes,
            "major_fault_bytes_estimate": major_fault_bytes,
            "major_fault_bandwidth_confidence": "lower-bound-estimate",
            "avg_fs_read_bytes_per_second": fs_input_bytes / wall if wall > 0 else 0,
            "avg_fs_write_bytes_per_second": fs_output_bytes / wall if wall > 0 else 0,
            "peak_fs_read_bytes_per_second": None,
            "peak_fs_write_bytes_per_second": None,
            "peak_io_source": "not-sampled",
        }
    )
    return data


def timed_command_prefix(stats_path: Path) -> list[str]:
    time_bin = shutil.which("time")
    if not time_bin:
        return []
    stats_path.parent.mkdir(parents=True, exist_ok=True)
    return [time_bin, "-f", RESOURCE_TIME_FORMAT, "-o", str(stats_path)]


def json_from_stdout(stdout: bytes) -> dict[str, Any]:
    lines = [raw_line.strip() for raw_line in stdout.splitlines() if raw_line.strip()]
    if len(lines) != 1:
        raise ValueError("command stdout must contain exactly one JSON line")
    parsed = json.loads(lines[0])
    if not isinstance(parsed, dict):
        raise ValueError("command stdout JSON payload is not an object")
    return parsed


def drain_stream_digest(stream: BinaryIO) -> dict[str, Any]:
    sha = hashlib.sha256()
    byte_count = 0
    while True:
        chunk = stream.read(65536)
        if not chunk:
            break
        sha.update(chunk)
        byte_count += len(chunk)
    return {"sha256": sha.hexdigest(), "bytes": byte_count}


def run_json_driver(
    cmd: list[str],
    *,
    env: dict[str, str],
    stats_path: Path,
    timeout: int,
) -> tuple[dict[str, Any], dict[str, Any]]:
    actual = [*timed_command_prefix(stats_path), *cmd]
    started = time.perf_counter()
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    result = subprocess.run(  # nosec B603 - harness uses shell=False command vectors.
        actual,
        cwd=str(ROOT),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    elapsed = time.perf_counter() - started
    stats = parse_time_stats(stats_path)
    if "process_wall_seconds" not in stats:
        stats["process_wall_seconds"] = elapsed
    if result.returncode != 0:
        raise RuntimeError(
            json.dumps(
                {
                    "returncode": result.returncode,
                    "command_sha256": command_digest(cmd),
                    "stdout_sha256": hashlib.sha256(result.stdout).hexdigest(),
                    "stderr_sha256": hashlib.sha256(result.stderr).hexdigest(),
                },
                sort_keys=True,
            )
        )
    payload = json_from_stdout(result.stdout)
    return payload, stats


def systemd_digest(
    path: Path,
    *,
    tools: ToolPaths,
    env: dict[str, str],
    stats_path: Path,
    timeout: int,
) -> tuple[dict[str, Any], dict[str, Any]]:
    cmd = [
        tools.journalctl,
        "--file",
        str(path),
        "--output=export",
        "--all",
        "--no-pager",
    ]
    actual = [*timed_command_prefix(stats_path), *cmd]
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    proc = subprocess.Popen(  # nosec B603 - harness uses shell=False command vectors.
        actual,
        cwd=str(ROOT),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert proc.stdout is not None
    assert proc.stderr is not None
    digest_state: dict[str, Any] = {}
    stderr_state: dict[str, Any] = {}

    def parse_stdout() -> None:
        try:
            digest_state["digest"] = digest_export_stream(proc.stdout)
        except Exception as exc:
            digest_state["error"] = exc

    def drain_stderr() -> None:
        try:
            stderr_state.update(drain_stream_digest(proc.stderr))
        except Exception as exc:
            stderr_state["error_class"] = type(exc).__name__
            stderr_state["error_sha256"] = hashlib.sha256(str(exc).encode("utf-8")).hexdigest()

    stdout_thread = threading.Thread(target=parse_stdout, name="systemd-export-digest")
    stderr_thread = threading.Thread(target=drain_stderr, name="systemd-export-stderr")
    stdout_thread.start()
    stderr_thread.start()

    deadline = time.monotonic() + timeout
    timed_out = False
    while proc.poll() is None and stdout_thread.is_alive():
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            timed_out = True
            proc.kill()
            break
        stdout_thread.join(timeout=min(0.1, remaining))

    if "error" in digest_state and proc.poll() is None:
        proc.kill()

    try:
        remaining = max(0.0, deadline - time.monotonic())
        returncode = proc.wait(timeout=remaining)
    except subprocess.TimeoutExpired:
        timed_out = True
        proc.kill()
        returncode = proc.wait()

    stdout_thread.join(timeout=5)
    stderr_thread.join(timeout=5)
    stderr_sha = str(stderr_state.get("sha256", hashlib.sha256(b"").hexdigest()))
    if stdout_thread.is_alive() or stderr_thread.is_alive():
        raise TimeoutError(
            json.dumps(
                {
                    "returncode": returncode,
                    "command_sha256": command_digest(cmd),
                    "stderr_sha256": stderr_sha,
                    "reader_thread_alive": stdout_thread.is_alive(),
                    "stderr_thread_alive": stderr_thread.is_alive(),
                },
                sort_keys=True,
            )
        )
    if "error" in digest_state:
        raise digest_state["error"]
    if timed_out:
        raise TimeoutError(
            json.dumps(
                {
                    "returncode": returncode,
                    "command_sha256": command_digest(cmd),
                    "stderr_sha256": stderr_sha,
                },
                sort_keys=True,
            )
        )
    digest = digest_state.get("digest")
    if not isinstance(digest, dict):
        raise RuntimeError(
            json.dumps(
                {
                    "returncode": returncode,
                    "command_sha256": command_digest(cmd),
                    "stderr_sha256": stderr_sha,
                    "reason": "systemd digest did not produce a result",
                },
                sort_keys=True,
            )
        )
    stats = parse_time_stats(stats_path)
    if returncode != 0:
        raise RuntimeError(
            json.dumps(
                {
                    "returncode": returncode,
                    "command_sha256": command_digest(cmd),
                    "stderr_sha256": stderr_sha,
                },
                sort_keys=True,
            )
        )
    digest.update({"driver": "systemd"})
    return digest, stats


def digest_driver_cmd(driver: str, path: Path, tools: ToolPaths) -> list[str]:
    if driver == "rust":
        return [str(tools.rust_digest), "--input", str(path), "--bounds", "snapshot"]
    if driver == "go":
        return [str(tools.go_digest), "--input", str(path), "--bounds", "snapshot"]
    raise ValueError(f"unsupported digest driver: {driver}")


def run_digest_driver(
    driver: str,
    case: JournalCase,
    *,
    tools: ToolPaths,
    env: dict[str, str],
    stats_dir: Path,
    timeout: int,
) -> dict[str, Any]:
    stats_path = stats_dir / f"{case.file_id}-{driver}-digest.json"
    started = time.perf_counter()
    if driver == "systemd":
        result, stats = systemd_digest(
            case.path,
            tools=tools,
            env=env,
            stats_path=stats_path,
            timeout=timeout,
        )
    else:
        result, stats = run_json_driver(
            digest_driver_cmd(driver, case.path, tools),
            env=env,
            stats_path=stats_path,
            timeout=timeout,
        )
    counts = result.get("counts", {})
    payload_bytes = int(counts.get("payload_bytes") or 0) if isinstance(counts, dict) else 0
    entries = int(counts.get("entries") or 0) if isinstance(counts, dict) else 0
    wall = float(stats.get("process_wall_seconds") or result.get("elapsed_seconds") or 0)
    return {
        "kind": "reader",
        "driver": driver,
        "status": "ok",
        "file_id": case.file_id,
        "schema": result.get("schema", SCHEMA_VERSION),
        "logical_digest": result.get("logical_digest"),
        "counts": counts,
        "metrics": {
            "process": stats,
            "entries_per_second": entries / wall if wall > 0 else 0,
            "payload_bytes_per_second": payload_bytes / wall if wall > 0 else 0,
            "input_bytes": case.size,
            "read_io_multiplication": (
                float(stats.get("fs_input_bytes") or 0) / case.size if case.size else None
            ),
        },
        "elapsed_seconds_observed": time.perf_counter() - started,
    }


def mode_parts(mode: str) -> tuple[str, str, bool]:
    if mode == "regular":
        return "regular", "none", False
    if mode == "compact":
        return "compact", "none", False
    if mode == "compact-zstd":
        return "compact", "zstd", False
    if mode == "compact-fss":
        return "compact", "none", True
    raise ValueError(f"unsupported regeneration mode: {mode}")


def regenerate_cmd(
    driver: str,
    case: JournalCase,
    output: Path,
    mode: str,
    tools: ToolPaths,
) -> list[str] | None:
    fmt, compression, fss = mode_parts(mode)
    base = [
        "--input",
        str(case.path),
        "--output",
        str(output),
        "--format",
        fmt,
        "--compression",
        compression,
        "--final-state",
        "offline",
    ]
    if fss:
        base.append("--fss")
    if driver == "rust":
        return [str(tools.rust_regenerate), *base]
    if driver == "go":
        return [str(tools.go_regenerate), *base]
    if driver == "systemd":
        return None
    raise ValueError(f"unsupported regeneration driver: {driver}")


def fss_verify_key(start_usec: int, interval_usec: int) -> str:
    seed_hex = "00" * 12
    return f"{seed_hex}/{start_usec // interval_usec:x}-{interval_usec:x}"


def verify_generated(
    path: Path,
    tools: ToolPaths,
    env: dict[str, str],
    timeout: int,
    *,
    verify_key: str | None = None,
) -> dict[str, Any]:
    cmd = [tools.journalctl, "--verify", "--file", str(path)]
    if verify_key is not None:
        cmd = [tools.journalctl, "--verify", "--verify-key", verify_key, "--file", str(path)]
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    result = subprocess.run(  # nosec B603 - harness uses shell=False command vectors.
        cmd,
        cwd=str(ROOT),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    return {
        "status": "ok" if result.returncode == 0 else "failed",
        "returncode": result.returncode,
        "stdout_sha256": hashlib.sha256(result.stdout).hexdigest(),
        "stderr_sha256": hashlib.sha256(result.stderr).hexdigest(),
        "command_sha256": command_digest(cmd),
        "verify_key_used": verify_key is not None,
    }


def run_regenerator(
    driver: str,
    mode: str,
    case: JournalCase,
    baseline_digest: str,
    *,
    tools: ToolPaths,
    env: dict[str, str],
    work_dir: Path,
    stats_dir: Path,
    keep_outputs: bool,
    timeout: int,
) -> dict[str, Any]:
    output = work_dir / "generated" / f"{case.file_id}-{driver}-{mode}.journal"
    output.parent.mkdir(parents=True, exist_ok=True)
    stats_path = stats_dir / f"{case.file_id}-{driver}-{mode}-regenerate.json"
    cmd = regenerate_cmd(driver, case, output, mode, tools)
    if cmd is None:
        return {
            "kind": "writer",
            "driver": driver,
            "mode": mode,
            "status": "unsupported",
            "file_id": case.file_id,
            "reason": (
                "systemd public regeneration requires journal export plus "
                "systemd-journal-remote; this harness records the limitation "
                "unless an installed remote helper is explicitly enabled later"
            ),
        }

    generated_path = output
    try:
        free_bytes = shutil.disk_usage(output.parent).free
        required_bytes = max(case.size * 2, 64 * 1024 * 1024)
        if free_bytes < required_bytes:
            raise OSError(
                f"insufficient scratch space: required_bytes={required_bytes} free_bytes={free_bytes}"
            )
        writer_result, stats = run_json_driver(cmd, env=env, stats_path=stats_path, timeout=timeout)
        generated_path = Path(str(writer_result.get("generated_path", output)))
        verify_key = None
        if writer_result.get("fss"):
            start_usec = int(writer_result.get("fss_start_usec") or 0)
            interval_usec = int(writer_result.get("fss_interval_usec") or 0)
            if start_usec > 0 and interval_usec > 0:
                verify_key = fss_verify_key(start_usec, interval_usec)
        verify = verify_generated(generated_path, tools, env, timeout, verify_key=verify_key)
        generated_case = JournalCase(
            path=generated_path,
            root=generated_path.parent,
            file_id=f"{case.file_id}-{driver}-{mode}",
            size=int(writer_result.get("generated_bytes") or generated_path.stat().st_size),
            mtime_ns=generated_path.stat().st_mtime_ns,
            suffix=".journal",
            identity={"file_id": f"{case.file_id}-{driver}-{mode}", "suffix": ".journal"},
        )
        reread = run_digest_driver(
            "systemd",
            generated_case,
            tools=tools,
            env=env,
            stats_dir=stats_dir,
            timeout=timeout,
        )
        generated_digest = str(reread.get("logical_digest"))
        generated_bytes = int(writer_result.get("generated_bytes") or generated_case.size)
    except Exception as exc:
        if not keep_outputs:
            try:
                generated_path.unlink()
            except FileNotFoundError:
                pass
        return {
            "kind": "writer",
            "driver": driver,
            "mode": mode,
            "status": "failed",
            "file_id": case.file_id,
            "error_class": type(exc).__name__,
            "error_sha256": hashlib.sha256(str(exc).encode("utf-8")).hexdigest(),
        }
    if not keep_outputs:
        try:
            generated_path.unlink()
        except FileNotFoundError:
            pass
    return {
        "kind": "writer",
        "driver": driver,
        "mode": mode,
        "status": "ok" if generated_digest == baseline_digest and verify["status"] == "ok" else "discrepancy",
        "file_id": case.file_id,
        "writer_result": {
            key: value
            for key, value in writer_result.items()
            if key not in {"generated_path", "fss_start_usec", "fss_interval_usec"}
        },
        "verify": verify,
        "reread": {
            "driver": "systemd",
            "logical_digest": generated_digest,
            "counts": reread.get("counts"),
        },
        "metrics": {
            "process": stats,
            "input_bytes": case.size,
            "generated_bytes": generated_bytes,
            "footprint_ratio": generated_bytes / case.size if case.size else None,
            "write_io_multiplication": (
                float(stats.get("fs_output_bytes") or 0) / case.size if case.size else None
            ),
        },
    }


def generate_smoke_fixture(tools: ToolPaths, env: dict[str, str], out: Path) -> Path:
    fixture = out / "smoke-fixtures" / "system.journal"
    fixture.parent.mkdir(parents=True, exist_ok=True)
    cmd = [
        str(tools.rust_writer_core),
        "--output",
        str(fixture),
        "--rows",
        "16",
        "--format",
        "regular",
        "--final-state",
        "offline",
        "--surface",
        "direct",
        "--api-mode",
        "raw-payload",
    ]
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    result = subprocess.run(  # nosec B603
        cmd,
        cwd=str(ROOT),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=1800,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(
            json.dumps(
                {
                    "returncode": result.returncode,
                    "stdout_sha256": hashlib.sha256(result.stdout).hexdigest(),
                    "stderr_sha256": hashlib.sha256(result.stderr).hexdigest(),
                },
                sort_keys=True,
            )
        )
    return fixture


def report_markdown(report: dict[str, Any]) -> str:
    discovery = report.get("discovery", {})
    lines = [
        "# Journal Corpus Evaluation",
        "",
        f"- Created: `{report.get('created_at')}`",
        f"- Mode: `{report.get('mode')}`",
        f"- Files: `{discovery.get('files', 0)}`",
        f"- Total input bytes: `{discovery.get('total_input_bytes', 0)}`",
        f"- Sensitive data policy: `{report.get('sensitive_data_policy')}`",
        "",
        "## Results",
        "",
        "| kind | driver | mode | status | file_id |",
        "| --- | --- | --- | --- | --- |",
    ]
    for result in report.get("results", []):
        lines.append(
            "| {kind} | {driver} | {mode} | {status} | `{file_id}` |".format(
                kind=result.get("kind", ""),
                driver=result.get("driver", ""),
                mode=result.get("mode", "-"),
                status=result.get("status", ""),
                file_id=result.get("file_id", ""),
            )
        )
    lines.extend(
        [
            "",
            "## Discrepancies",
            "",
        ]
    )
    discrepancies = report.get("discrepancies", [])
    if not discrepancies:
        lines.append("_None recorded._")
    else:
        lines.extend(["| code | file_id | detail |", "| --- | --- | --- |"])
        for item in discrepancies:
            lines.append(
                f"| {item.get('code')} | `{item.get('file_id')}` | {item.get('detail')} |"
            )
    lines.append("")
    return "\n".join(lines)


def run_evaluation(args: argparse.Namespace) -> dict[str, Any]:
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    env = run_env()
    tools: ToolPaths | None = None
    roots = [Path(root) for root in args.root]

    if args.mode == "smoke":
        tools = build_tools(env, out)
        roots = [generate_smoke_fixture(tools, env, out)]

    cases = discover_cases(roots, max_files=args.max_files)
    discovery = summarize_discovery(cases)
    report: dict[str, Any] = {
        "schema": "systemd-journal-sdk-corpus-eval-report-v1",
        "created_at": utc_now(),
        "mode": args.mode,
        "canonical_digest_schema": SCHEMA_VERSION,
        "sensitive_data_policy": "metrics-only: no raw journal field names, values, messages, hostnames, IPs, usernames, or binary payload dumps are written",
        "full_corpus_guard": "full corpus execution requires explicit --allow-full-run and is not used by smoke or dry-run",
        "input_snapshot_policy": "run mode copies one input journal at a time into .local scratch before reader/writer comparisons, then deletes the snapshot",
        "discovery": discovery,
        "inputs": [case.identity for case in cases],
        "results": [],
        "discrepancies": [],
    }

    if args.mode == "dry-run":
        report["dry_run_payload_policy"] = "stat/list only; journal payloads are not opened or read"
        write_json(out / "report.json", report)
        (out / "report.md").write_text(report_markdown(report), encoding="utf-8")
        return report

    if args.mode == "run" and not args.allow_full_run:
        raise SystemExit("run mode requires --allow-full-run; use --mode smoke or --mode dry-run first")

    if tools is None:
        tools = build_tools(env, out)

    state_path = out / "state.json"
    state = load_json(state_path, {"completed": {}})
    completed = state.setdefault("completed", {})
    stats_dir = out / "time"
    work_dir = out / "work"

    for case in cases:
        expected_keys = case_keys(case, args)
        case_complete = all(
            key in completed and completed[key].get("identity") == case.identity
            for key in expected_keys
        )
        if not case_complete:
            for key in expected_keys:
                completed.pop(key, None)

        active_case = case
        snapshot_path: Path | None = None
        try:
            if not case_complete:
                active_case = snapshot_case(case, work_dir)
                snapshot_path = active_case.path

            reader_results: dict[str, dict[str, Any]] = {}
            for driver in args.drivers:
                key = f"{case.file_id}:reader:{driver}"
                if key in completed and completed[key].get("identity") == case.identity:
                    result = completed[key]["result"]
                else:
                    try:
                        result = run_digest_driver(
                            driver,
                            active_case,
                            tools=tools,
                            env=env,
                            stats_dir=stats_dir,
                            timeout=args.timeout,
                        )
                    except Exception as exc:
                        result = {
                            "kind": "reader",
                            "driver": driver,
                            "status": "failed",
                            "file_id": case.file_id,
                            "error_class": type(exc).__name__,
                            "error_sha256": hashlib.sha256(str(exc).encode("utf-8")).hexdigest(),
                        }
                    completed[key] = {"identity": case.identity, "result": result}
                    write_json(state_path, state)
                report["results"].append(result)
                reader_results[driver] = result

            baseline = reader_results.get("systemd")
            if baseline and baseline.get("status") == "ok":
                baseline_digest = str(baseline.get("logical_digest"))
                for driver, result in reader_results.items():
                    if driver == "systemd" or result.get("status") != "ok":
                        continue
                    if result.get("logical_digest") != baseline_digest:
                        report["discrepancies"].append(
                            {
                                "code": "reader_digest_mismatch",
                                "file_id": case.file_id,
                                "detail": f"{driver} logical digest differs from systemd baseline",
                            }
                        )

                for regen_driver in args.regenerators:
                    for regen_mode in args.regeneration_modes:
                        key = f"{case.file_id}:writer:{regen_driver}:{regen_mode}"
                        if key in completed and completed[key].get("identity") == case.identity:
                            regen_result = completed[key]["result"]
                        else:
                            regen_result = run_regenerator(
                                regen_driver,
                                regen_mode,
                                active_case,
                                baseline_digest,
                                tools=tools,
                                env=env,
                                work_dir=work_dir,
                                stats_dir=stats_dir,
                                keep_outputs=args.keep_outputs,
                                timeout=args.timeout,
                            )
                            completed[key] = {"identity": case.identity, "result": regen_result}
                            write_json(state_path, state)
                        report["results"].append(regen_result)
                        if regen_result.get("status") == "discrepancy":
                            report["discrepancies"].append(
                                {
                                    "code": "writer_regeneration_mismatch",
                                    "file_id": case.file_id,
                                    "detail": f"{regen_driver}/{regen_mode} generated output did not match systemd logical digest or stock verify",
                                }
                            )
            else:
                report["discrepancies"].append(
                    {
                        "code": "missing_systemd_baseline",
                        "file_id": case.file_id,
                        "detail": "systemd baseline failed, so SDK parity checks were not conclusive",
                    }
                )
        finally:
            if snapshot_path is not None:
                try:
                    snapshot_path.unlink()
                except FileNotFoundError:
                    pass

    write_json(out / "report.json", report)
    (out / "report.md").write_text(report_markdown(report), encoding="utf-8")
    return report


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("dry-run", "smoke", "run"), default="dry-run")
    parser.add_argument("--root", action="append", default=[], help="journal file or directory root")
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT / "manual")
    parser.add_argument("--drivers", nargs="+", default=["systemd", "rust", "go"], choices=("systemd", "rust", "go"))
    parser.add_argument("--regenerators", nargs="+", default=["rust", "go"], choices=("systemd", "rust", "go"))
    parser.add_argument(
        "--regeneration-modes",
        nargs="+",
        default=["regular", "compact", "compact-zstd", "compact-fss"],
        choices=("regular", "compact", "compact-zstd", "compact-fss"),
    )
    parser.add_argument("--max-files", type=int)
    parser.add_argument("--timeout", type=int, default=1800)
    parser.add_argument("--allow-full-run", action="store_true")
    parser.add_argument("--keep-outputs", action="store_true")
    args = parser.parse_args()
    if args.mode != "smoke" and not args.root:
        parser.error("--root is required unless --mode smoke is used")
    if args.mode == "smoke" and args.root:
        parser.error("--mode smoke generates its own fixture and does not accept --root")
    return args


def display_path(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(ROOT))
    except ValueError:
        return str(path)


def main() -> int:
    args = parse_args()
    try:
        run_evaluation(args)
    except Exception as exc:
        print(f"corpus evaluation failed: {type(exc).__name__}: {exc}", file=sys.stderr)
        return 1
    print(
        json.dumps(
            {
                "status": "ok",
                "report_json": display_path(args.out / "report.json"),
                "report_md": display_path(args.out / "report.md"),
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
