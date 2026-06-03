#!/usr/bin/env python3
"""Live cross-language interoperability matrix.

Starts one writer per language and polls multiple readers while the writer is
actively appending. Validates:
  - at least one reader observation happens while the writer is still active;
  - observed sequences are ordered prefixes of LIVE_SEQ;
  - stock libsystemd can follow the active file to the expected entry count;
  - final reader snapshots include all expected entries in order;
  - stock journalctl --verify --file passes for generated files, with
    --verify-key for sealed files;
  - generated files structurally match the selected compression/compact mode.

Runtime artifacts stay under .local/interoperability/.

For directory-mode writers, the runner discovers the active `.journal` file
after the writer publishes the ready file, then passes that file to each reader.
Directory traversal behavior is tracked separately from live file compatibility.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess  # nosec B404 - subprocess is required by harnesses.
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Iterable

from journal_structure import inspect_journal_structure


REPO_ROOT = Path(__file__).resolve().parents[2]
LOCAL_DIR = REPO_ROOT / ".local" / "interoperability"
BIN_DIR = LOCAL_DIR / "bin"
LIVE_SEAL_START_USEC = 1_700_001_000_000_000
LIVE_SEAL_INTERVAL_USEC = 1_000_000
LIVE_SEAL_SEED_HEX = "000000000000000000000000"
LIVE_SEAL_VERIFY_KEY = (
    f"{LIVE_SEAL_SEED_HEX}/"
    f"{LIVE_SEAL_START_USEC // LIVE_SEAL_INTERVAL_USEC:x}-{LIVE_SEAL_INTERVAL_USEC:x}"
)


@dataclass(frozen=True)
class WriterSpec:
    name: str
    syslog_identifier: str
    mode: str  # "file" or "directory"


@dataclass(frozen=True)
class ReaderSpec:
    name: str


@dataclass(frozen=True)
class FeatureSpec:
    name: str
    compression: str = "none"
    compact: bool = False
    sealed: bool = False
    fixture: str = "regular"
    force_file_writers: frozenset[str] = frozenset()

    @property
    def verify_key(self) -> str | None:
        return LIVE_SEAL_VERIFY_KEY if self.sealed else None


@dataclass(frozen=True)
class LiveRunOptions:
    entries: int
    num_poll_readers: int
    num_libsystemd_readers: int
    poll_sec: float
    writer_delay_ms: int


@dataclass(frozen=True)
class LiveWorkspace:
    writer_root: Path
    ready_file: Path
    target: Path
    mode: str
    cmd: list[str]
    env: dict[str, str]


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

FEATURES = {
    "regular": FeatureSpec("regular"),
    "zstd": FeatureSpec("zstd", compression="zstd", fixture="zstd"),
    "xz": FeatureSpec("xz", compression="xz", fixture="xz"),
    "lz4": FeatureSpec("lz4", compression="lz4", fixture="lz4"),
    "compact": FeatureSpec("compact", compact=True, fixture="binary"),
    "compact-zstd": FeatureSpec("compact-zstd", compression="zstd", compact=True, fixture="binary"),
    "compact-xz": FeatureSpec("compact-xz", compression="xz", compact=True, fixture="binary"),
    "compact-lz4": FeatureSpec("compact-lz4", compression="lz4", compact=True, fixture="binary"),
    "sealed": FeatureSpec("sealed", sealed=True, force_file_writers=frozenset({"rust"})),
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
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        env=env,
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

    require_ok(
        run(
            ["go", "build", "-o", str(BIN_DIR / "go-livewriter"), "./internal/testcmd/livewriter"],
            cwd=REPO_ROOT / "go",
            env=env,
        ),
        "build go livewriter",
    )
    require_ok(
        run(
            ["go", "build", "-o", str(BIN_DIR / "go-journalctl"), "./cmd/journalctl"],
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
    require_ok(
        run(
            [
                "cc",
                str(REPO_ROOT / "tests/conformance/live/libsystemd_live_reader.c"),
                "-o",
                str(BIN_DIR / "libsystemd_live_reader"),
                "-lsystemd",
            ],
            env=env,
        ),
        "build libsystemd live reader",
    )

    cargo_target = Path(env["CARGO_TARGET_DIR"])
    for src, dst in [
        (cargo_target / "debug/livewriter", BIN_DIR / "rust-livewriter"),
        (cargo_target / "debug/journalctl", BIN_DIR / "rust-journalctl"),
    ]:
        if not src.exists():
            raise RuntimeError(f"expected Rust build output not found: {src}")
        shutil.copy2(src, dst)

    for name in [
        "go-livewriter",
        "go-journalctl",
        "rust-livewriter",
        "rust-journalctl",
        "libsystemd_live_reader",
    ]:
        if not (BIN_DIR / name).exists():
            raise RuntimeError(f"expected binary not found: {BIN_DIR / name}")

    return {
        "go_livewriter": str(BIN_DIR / "go-livewriter"),
        "go_journalctl": str(BIN_DIR / "go-journalctl"),
        "rust_livewriter": str(BIN_DIR / "rust-livewriter"),
        "rust_journalctl": str(BIN_DIR / "rust-journalctl"),
        "libsystemd_live_reader": str(BIN_DIR / "libsystemd_live_reader"),
    }


# ----------------------------------------------------------------------
# Writer command construction
# ----------------------------------------------------------------------

def writer_mode(writer: WriterSpec, feature: FeatureSpec) -> str:
    if writer.name in feature.force_file_writers:
        return "file"
    return writer.mode


def feature_args(feature: FeatureSpec) -> list[str]:
    args: list[str] = []
    if feature.compact:
        args.append("--compact")
    if feature.compression != "none":
        # Force compression of the 256-byte test payloads; the default
        # threshold would leave them uncompressed in this live matrix.
        args.extend(["--compression", feature.compression, "--compress-threshold", "16"])
    if feature.fixture == "binary":
        args.append("--binary-fixture")
    elif feature.fixture in {"zstd", "xz", "lz4"}:
        args.append(f"--{feature.fixture}-fixture")
    if feature.sealed:
        args.extend([
            "--seal",
            "--seal-start-usec",
            str(LIVE_SEAL_START_USEC),
            "--seal-interval-usec",
            str(LIVE_SEAL_INTERVAL_USEC),
        ])
    return args


def writer_cmd(
    writer: WriterSpec,
    feature: FeatureSpec,
    tools: dict[str, str],
    target: Path,
    ready: Path,
    entries: int,
    delay_ms: int = 1,
) -> list[str]:
    delay = f"{delay_ms}ms"
    mode = writer_mode(writer, feature)
    extras = feature_args(feature)
    if writer.name == "go":
        return [
            tools["go_livewriter"], "--path", str(target), "--ready-file", str(ready),
            "--entries", str(entries), "--delay", delay, *extras,
        ]
    if writer.name == "rust":
        path_flag = "--dir" if mode == "directory" else "--path"
        return [
            tools["rust_livewriter"], path_flag, str(target), "--ready-file", str(ready),
            "--entries", str(entries), "--delay", delay, *extras,
        ]
    if writer.name == "node":
        return [
            "node", str(REPO_ROOT / "node/internal/testcmd/livewriter.js"),
            "--path", str(target), "--ready-file", str(ready),
            "--entries", str(entries), "--delay", f"{delay_ms}ms", *extras,
        ]
    if writer.name == "python":
        return [
            "python3", str(REPO_ROOT / "python/cmd/livewriter.py"),
            "--path", str(target), "--ready-file", str(ready),
            "--entries", str(entries), "--delay", f"{delay_ms}ms", *extras,
        ]
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


def prepare_live_workspace(
    writer_spec: WriterSpec,
    feature_spec: FeatureSpec,
    tools: dict[str, str],
    options: LiveRunOptions,
) -> LiveWorkspace:
    writer_root = LOCAL_DIR / "live" / feature_spec.name / writer_spec.name
    if writer_root.exists():
        shutil.rmtree(writer_root)
    writer_root.mkdir(parents=True, exist_ok=True)
    ready_file = LOCAL_DIR / f"{feature_spec.name}-{writer_spec.name}.ready"
    ready_file.unlink(missing_ok=True)
    mode = writer_mode(writer_spec, feature_spec)
    target = writer_root if mode == "directory" else writer_root / f"{writer_spec.name}-{feature_spec.name}.journal"
    cmd = writer_cmd(writer_spec, feature_spec, tools, target, ready_file, options.entries, options.writer_delay_ms)
    return LiveWorkspace(writer_root, ready_file, target, mode, cmd, build_env())


def start_writer(cmd: list[str], env: dict[str, str]) -> subprocess.Popen[str]:
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    return subprocess.Popen(  # nosec B603 - harness uses shell=False command vectors.
        cmd,  # nosemgrep
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
    )


def wait_for_writer_ready(
    writer_proc: subprocess.Popen[str],
    writer_spec: WriterSpec,
    feature_spec: FeatureSpec,
    workspace: LiveWorkspace,
) -> None:
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        if writer_proc.poll() is not None:
            _stdout, stderr = writer_proc.communicate(timeout=1)
            raise RuntimeError(
                f"writer {writer_spec.name}/{feature_spec.name} exited early with {writer_proc.returncode}; "
                f"stderr={stderr[-500:]}"
            )
        if workspace.ready_file.exists() and workspace_target_ready(workspace):
            return
        time.sleep(0.02)
    writer_proc.terminate()
    raise RuntimeError(f"writer {writer_spec.name}/{feature_spec.name} ready timeout after 30s")


def workspace_target_ready(workspace: LiveWorkspace) -> bool:
    if workspace.mode == "file":
        return workspace.target.exists() and workspace.target.stat().st_size > 0
    journals = list(workspace.writer_root.rglob("*.journal"))
    return bool(journals and journals[0].stat().st_size > 0)


def discover_journal_file(
    writer_proc: subprocess.Popen[str],
    writer_spec: WriterSpec,
    workspace: LiveWorkspace,
) -> Path:
    if workspace.mode == "file":
        return workspace.target
    journal_files = sorted(workspace.writer_root.rglob("*.journal"))
    if len(journal_files) != 1:
        writer_proc.terminate()
        raise RuntimeError(
            f"writer {writer_spec.name} expected exactly one active journal file, "
            f"found {len(journal_files)}"
        )
    return journal_files[0]


def poll_reader(
    reader_name: str,
    cmd: list[str],
    env: dict[str, str],
    stop_poll: threading.Event,
    writer_finished: threading.Event,
    poll_sec: float,
) -> dict:
    best_seq: list[str] = []
    best_count = 0
    last_error = ""
    while not stop_poll.is_set():
        try:
            best_seq, best_count, last_error = poll_reader_once(
                reader_name,
                cmd,
                env,
                writer_finished,
                best_seq,
                best_count,
                last_error,
            )
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


def poll_reader_once(
    reader_name: str,
    cmd: list[str],
    env: dict[str, str],
    writer_finished: threading.Event,
    best_seq: list[str],
    best_count: int,
    last_error: str,
) -> tuple[list[str], int, str]:
    active_at_start = not writer_finished.is_set()
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    res = subprocess.run(  # nosec B603 - harness uses shell=False command vectors.
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=8,
        env=env,
    )
    active_at_end = not writer_finished.is_set()
    if active_at_start and active_at_end and res.returncode == 0:
        parsed = parse_json_lines(res.stdout, reader_name)
        seqs = [str(e.get("LIVE_SEQ", "")) for e in parsed]
        if len(seqs) > best_count:
            return seqs, len(seqs), ""
    elif res.returncode != 0:
        last_error = f"exit {res.returncode}: {res.stderr[-200:]}"
    return best_seq, best_count, last_error


def final_reader(
    reader_name: str,
    reader_spec: ReaderSpec,
    tools: dict[str, str],
    journal_path: str,
    env: dict[str, str],
) -> dict:
    cmd = reader_cmd(reader_spec, tools, journal_path, ["PRIORITY=6"])
    try:
        res = run(cmd, timeout=30, env=env)
        return final_reader_result(reader_name, cmd, res)
    except Exception as e:
        return reader_error_result(reader_name, cmd, str(e))


def final_reader_result(reader_name: str, cmd: list[str], res: subprocess.CompletedProcess[str]) -> dict:
    if res.returncode != 0:
        return reader_error_result(reader_name, cmd, f"exit {res.returncode}: {res.stderr[-200:]}")
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


def reader_error_result(reader_name: str, cmd: list[str], error: str) -> dict:
    return {
        "reader": reader_name,
        "while_active": False,
        "seq_observed": [],
        "entries_count": 0,
        "command": shell_join(cmd),
        "error": error,
    }


def libsystemd_reader(
    reader_name: str,
    cmd: list[str],
    env: dict[str, str],
    timeout: int,
    writer_finished: threading.Event,
) -> dict:
    active_at_start = not writer_finished.is_set()
    try:
        # nosemgrep
        # subprocess is required by this harness; commands are shell=False vectors.
        res = subprocess.run(  # nosec B603 - harness uses shell=False command vectors.
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout,
            env=env,
        )
        return libsystemd_reader_result(reader_name, cmd, res, active_at_start)
    except Exception as error:
        return libsystemd_error_result(reader_name, cmd, active_at_start, str(error))


def libsystemd_reader_result(
    reader_name: str,
    cmd: list[str],
    res: subprocess.CompletedProcess[str],
    active_at_start: bool,
) -> dict:
    parsed = parse_json_lines(res.stdout, reader_name) if res.returncode == 0 else []
    entries_count = int(parsed[-1].get("entries", 0)) if parsed else 0
    waits = int(parsed[-1].get("waits", 0)) if parsed else 0
    error = "" if res.returncode == 0 else f"exit {res.returncode}: {res.stderr[-300:]}"
    return {
        "reader": reader_name,
        "started_while_active": active_at_start,
        "entries_count": entries_count,
        "waits": waits,
        "command": shell_join(cmd),
        "error": error,
    }


def libsystemd_error_result(reader_name: str, cmd: list[str], active_at_start: bool, error: str) -> dict:
    return {
        "reader": reader_name,
        "started_while_active": active_at_start,
        "entries_count": 0,
        "waits": 0,
        "command": shell_join(cmd),
        "error": error,
    }


def eligible_poll_readers(reader_specs: list[ReaderSpec], num_poll_readers: int) -> list[tuple[str, ReaderSpec, int]]:
    return [
        (f"{reader_spec.name}-{idx}", reader_spec, idx)
        for reader_spec in reader_specs
        for idx in range(num_poll_readers)
    ]


def libsystemd_live_cmd(tools: dict[str, str], journal_path: str, entries: int, timeout: int) -> list[str]:
    return [
        tools["libsystemd_live_reader"],
        "--path",
        journal_path,
        "--expected",
        str(entries),
        "--match",
        "PRIORITY=6",
        "--sequence-field",
        "LIVE_SEQ",
        "--timeout-sec",
        str(timeout),
    ]


def reader_timeout_seconds(options: LiveRunOptions) -> int:
    return max(15, int((options.entries * max(options.writer_delay_ms, 1)) / 1000) + 15)


def start_poll_futures(
    executor: ThreadPoolExecutor,
    readers: list[tuple[str, ReaderSpec, int]],
    tools: dict[str, str],
    journal_path: str,
    workspace: LiveWorkspace,
    writer_finished: threading.Event,
    stop_poll: threading.Event,
    options: LiveRunOptions,
):
    return [
        executor.submit(
            poll_reader,
            reader_name,
            reader_cmd(reader_spec, tools, journal_path, ["PRIORITY=6"]),
            workspace.env,
            stop_poll,
            writer_finished,
            options.poll_sec,
        )
        for reader_name, reader_spec, _idx in readers
    ]


def start_libsystemd_futures(
    executor: ThreadPoolExecutor,
    tools: dict[str, str],
    journal_path: str,
    workspace: LiveWorkspace,
    writer_finished: threading.Event,
    options: LiveRunOptions,
    timeout: int,
):
    return [
        executor.submit(
            libsystemd_reader,
            f"libsystemd-{idx}",
            libsystemd_live_cmd(tools, journal_path, options.entries, timeout),
            workspace.env,
            timeout + 5,
            writer_finished,
        )
        for idx in range(options.num_libsystemd_readers)
    ]


def wait_for_writer_exit(
    writer_proc: subprocess.Popen[str],
    writer_spec: WriterSpec,
    writer_finished: threading.Event,
    stop_poll: threading.Event,
) -> tuple[int, str]:
    try:
        _writer_stdout, writer_stderr = writer_proc.communicate(timeout=90)
    except subprocess.TimeoutExpired as error:
        writer_proc.terminate()
        writer_proc.wait(timeout=5)
        raise RuntimeError(f"writer {writer_spec.name} did not finish within 90s") from error
    finally:
        writer_finished.set()
        stop_poll.set()
    return writer_proc.returncode, writer_stderr


def poll_future_error(error: Exception) -> dict:
    return {
        "reader": "poll-unknown",
        "started_while_active": True,
        "status": "FAIL",
        "error": str(error),
    }


def libsystemd_future_error(error: Exception) -> dict:
    return {
        "reader": "libsystemd-unknown",
        "started_while_active": False,
        "entries_count": 0,
        "waits": 0,
        "command": "",
        "error": str(error),
    }


def collect_poll_futures(poll_futures: list, timeout: int = 10) -> list[dict]:
    results = []
    for future in as_completed(poll_futures):
        try:
            results.append(future.result(timeout=timeout))
        except Exception as error:
            results.append(poll_future_error(error))
    return results


def collect_libsystemd_futures(libsystemd_futures: list, timeout: int) -> list[dict]:
    results = []
    for future in as_completed(libsystemd_futures):
        try:
            results.append(future.result(timeout=timeout))
        except Exception as error:
            results.append(libsystemd_future_error(error))
    return results


def collect_final_reads(
    readers: list[tuple[str, ReaderSpec, int]],
    tools: dict[str, str],
    journal_path: str,
    env: dict[str, str],
) -> list[dict]:
    results = []
    for reader_name, reader_spec, _idx in readers:
        results.append(final_reader(reader_name, reader_spec, tools, journal_path, env))
    return results


def verify_journal(journal_path: str, feature_spec: FeatureSpec, env: dict[str, str]) -> dict:
    cmd = ["journalctl", "--verify"]
    if feature_spec.verify_key:
        cmd.extend(["--verify-key", feature_spec.verify_key])
    cmd.extend(["--file", journal_path])
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    result = subprocess.run(  # nosec B603 - harness uses shell=False command vectors.
        cmd,  # nosemgrep
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=30,
        env=env,
    )
    return {"command": shell_join(cmd), "returncode": result.returncode, "stderr": result.stderr[-300:]}


def expected_structure_compression(feature_spec: FeatureSpec) -> str | None:
    return feature_spec.compression if feature_spec.compression != "none" else None


def inspect_live_structure(journal_path: str, feature_spec: FeatureSpec, writer_spec: WriterSpec) -> dict:
    return inspect_journal_structure(
        journal_path,
        expected_compact=feature_spec.compact,
        expected_compression=expected_structure_compression(feature_spec),
        test_name=f"live-{feature_spec.name}-{writer_spec.name}-structure",
    )


def live_result(
    writer_spec: WriterSpec,
    feature_spec: FeatureSpec,
    workspace: LiveWorkspace,
    journal_path: str,
    options: LiveRunOptions,
    exit_code: int,
    writer_stderr: str,
    active_polls: list[dict],
    libsystemd_live: list[dict],
    final_reads: list[dict],
) -> dict:
    return {
        "writer": writer_spec.name,
        "feature": feature_spec.name,
        "journal_path": journal_path,
        "journal_mode": workspace.mode,
        "entries": options.entries,
        "exit_code": exit_code,
        "writer_command": shell_join(workspace.cmd),
        "active_polls": active_polls,
        "libsystemd_live": libsystemd_live,
        "final_reads": final_reads,
        "verify": verify_journal(journal_path, feature_spec, workspace.env),
        "structure": inspect_live_structure(journal_path, feature_spec, writer_spec),
        "verify_key": feature_spec.verify_key or "",
        "writer_stderr": writer_stderr[-500:] if writer_stderr else "",
    }


def run_one_live(
    writer_spec: WriterSpec,
    feature_spec: FeatureSpec,
    tools: dict[str, str],
    reader_specs: list[ReaderSpec],
    entries: int,
    num_poll_readers: int,
    num_libsystemd_readers: int,
    poll_sec: float,
    writer_delay_ms: int,
) -> dict:
    """Run live matrix for one writer language."""
    options = LiveRunOptions(entries, num_poll_readers, num_libsystemd_readers, poll_sec, writer_delay_ms)
    workspace = prepare_live_workspace(writer_spec, feature_spec, tools, options)
    writer_proc = start_writer(workspace.cmd, workspace.env)
    wait_for_writer_ready(writer_proc, writer_spec, feature_spec, workspace)
    journal_file = discover_journal_file(writer_proc, writer_spec, workspace)
    journal_path = str(journal_file)
    stop_poll = threading.Event()
    writer_finished = threading.Event()
    eligible_readers = eligible_poll_readers(reader_specs, num_poll_readers)

    reader_timeout = reader_timeout_seconds(options)
    with ThreadPoolExecutor(max_workers=len(eligible_readers) + num_libsystemd_readers + 4) as executor:
        poll_futures = start_poll_futures(
            executor, eligible_readers, tools, journal_path, workspace, writer_finished, stop_poll, options
        )
        libsystemd_futures = start_libsystemd_futures(
            executor, tools, journal_path, workspace, writer_finished, options, reader_timeout
        )
        exit_code, writer_stderr = wait_for_writer_exit(writer_proc, writer_spec, writer_finished, stop_poll)
        active_polls = collect_poll_futures(poll_futures)
        libsystemd_live = collect_libsystemd_futures(libsystemd_futures, reader_timeout + 10)
        final_reads = collect_final_reads(eligible_readers, tools, journal_path, workspace.env)

    return live_result(
        writer_spec,
        feature_spec,
        workspace,
        journal_path,
        options,
        exit_code,
        writer_stderr,
        active_polls,
        libsystemd_live,
        final_reads,
    )


def expected_live_sequences(entries: int) -> list[str]:
    return [f"{i:06d}" for i in range(entries)]


def active_observations(result: dict) -> list[dict]:
    return [obs for obs in result["active_polls"] if obs.get("entries_count", 0) > 0]


def assess_writer_exit(result: dict) -> list[str]:
    if result["exit_code"] != 0:
        return [f"writer exit {result['exit_code']}"]
    return []


def assess_active_polls(result: dict, expected: list[str]) -> list[str]:
    errors = []
    active_with_entries = active_observations(result)
    if not active_with_entries:
        errors.append("no reader observed entries while writer was actively writing")
    expected_reader_groups = {reader_group(o["reader"]) for o in result["active_polls"]}
    observed_reader_groups = {reader_group(o["reader"]) for o in active_with_entries}
    missing_reader_groups = sorted(expected_reader_groups - observed_reader_groups)
    if missing_reader_groups:
        errors.append(f"active reader groups with no live entries: {', '.join(missing_reader_groups)}")
    for obs in active_with_entries:
        observed = obs.get("seq_observed", [])
        if observed != expected[:len(observed)]:
            errors.append(
                f"{obs['reader']}: active sequence is not an ordered prefix, "
                f"got {observed[:3]}... len={len(observed)}"
            )
    return errors


def assess_libsystemd_live(result: dict, entries: int) -> list[str]:
    errors = []
    if not result.get("libsystemd_live"):
        errors.append("no stock libsystemd live reader was run")
    for obs in result.get("libsystemd_live", []):
        if not obs.get("started_while_active"):
            errors.append(f"{obs['reader']}: did not start while writer was active")
        if obs.get("error"):
            errors.append(f"{obs['reader']}: {obs['error']}")
            continue
        if obs.get("entries_count", 0) != entries:
            errors.append(
                f"{obs['reader']}: expected {entries} live entries, got {obs.get('entries_count', 0)}"
            )
        if obs.get("waits", 0) <= 0:
            errors.append(f"{obs['reader']}: did not wait for appended entries")
    return errors


def assess_final_reads(result: dict, entries: int, expected: list[str]) -> list[str]:
    errors = []
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
    return errors


def assess_verify(result: dict) -> list[str]:
    errors = []
    if result.get("verify") and result["verify"].get("returncode") != 0:
        errors.append(f"verify failed: {result['verify'].get('stderr', '')}")
    if result.get("feature") == "sealed":
        verify_command = result.get("verify", {}).get("command", "")
        if not result.get("verify_key"):
            errors.append("sealed feature missing verify key")
        if "--verify-key" not in verify_command:
            errors.append("sealed feature was not verified with --verify-key")
    return errors


def assess_structure(result: dict) -> list[str]:
    if result.get("structure") and result["structure"].get("status") != "PASS":
        return [f"structure failed: {result['structure'].get('error', '')}"]
    return []


def assess(result: dict, entries: int) -> tuple[str, list[str]]:
    expected = expected_live_sequences(entries)
    errors = []
    errors.extend(assess_writer_exit(result))
    errors.extend(assess_active_polls(result, expected))
    errors.extend(assess_libsystemd_live(result, entries))
    errors.extend(assess_final_reads(result, entries, expected))
    errors.extend(assess_verify(result))
    errors.extend(assess_structure(result))

    return "PASS" if not errors else "FAIL", errors


def reader_group(reader_name: str) -> str:
    base, sep, suffix = reader_name.rpartition("-")
    if sep and suffix.isdigit():
        return base
    return reader_name


def selected(mapping: dict[str, object], names: list[str] | None):
    if not names:
        return list(mapping.values())
    missing = [name for name in names if name not in mapping]
    if missing:
        raise SystemExit(f"unknown names: {', '.join(missing)}")
    return [mapping[name] for name in names]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--entries", type=int, default=30)
    parser.add_argument("--features", nargs="*", choices=sorted(FEATURES))
    parser.add_argument("--writers", nargs="*", choices=sorted(WRITERS))
    parser.add_argument("--readers", nargs="*", choices=sorted(READERS))
    parser.add_argument("--poll-readers", type=int, default=2,
                        help="number of polling reader tasks per language (default: 2)")
    parser.add_argument("--libsystemd-readers", type=int, default=1,
                        help="number of stock libsystemd live readers per writer/feature (default: 1)")
    parser.add_argument("--poll-interval", type=float, default=0.1,
                        help="seconds between poll attempts (default: 0.1)")
    parser.add_argument("--writer-delay-ms", type=int, default=20,
                        help="delay between writer appends in milliseconds (default: 20)")
    parser.add_argument("--keep-files", action="store_true")
    return parser.parse_args()


def validate_args(args: argparse.Namespace) -> None:
    if args.entries < 5:
        raise SystemExit("--entries must be at least 5")
    if args.writer_delay_ms < 0:
        raise SystemExit("--writer-delay-ms must be non-negative")
    if args.poll_readers < 1:
        raise SystemExit("--poll-readers must be at least 1")
    if args.libsystemd_readers < 1:
        raise SystemExit("--libsystemd-readers must be at least 1")


def failed_live_case(writer_spec: WriterSpec, feature_spec: FeatureSpec, error: Exception) -> dict:
    return {
        "writer": writer_spec.name,
        "feature": feature_spec.name,
        "error": str(error),
        "status": "FAIL",
    }


def run_live_case(
    writer_spec: WriterSpec,
    feature_spec: FeatureSpec,
    tools: dict[str, str],
    reader_specs: list[ReaderSpec],
    args: argparse.Namespace,
) -> dict:
    try:
        result = run_one_live(
            writer_spec,
            feature_spec,
            tools,
            reader_specs,
            args.entries,
            args.poll_readers,
            args.libsystemd_readers,
            args.poll_interval,
            args.writer_delay_ms,
        )
    except Exception as error:
        print(f"ERROR: {error}", flush=True)
        return failed_live_case(writer_spec, feature_spec, error)
    status, errors = assess(result, args.entries)
    result["status"] = status
    result["errors"] = errors
    return result


def print_live_result(result: dict, entries: int) -> None:
    if result.get("error") and result.get("status") == "FAIL":
        return
    active_with_entries = active_observations(result)
    print(f"  exit={result['exit_code']}", flush=True)
    print(f"  active polls with entries: {len(active_with_entries)}/{len(result['active_polls'])}", flush=True)
    for obs in active_with_entries[:3]:
        print(f"    {obs['reader']}: {obs['entries_count']} entries, seq={obs['seq_observed'][:3]}...", flush=True)
    print_libsystemd_summary(result, entries)
    print_final_read_summary(result, entries)
    print_optional_checks(result)


def print_libsystemd_summary(result: dict, entries: int) -> None:
    complete = [
        obs for obs in result.get("libsystemd_live", [])
        if obs.get("entries_count", 0) == entries and not obs.get("error")
    ]
    print(
        f"  stock libsystemd live readers with all {entries} entries: "
        f"{len(complete)}/{len(result.get('libsystemd_live', []))}",
        flush=True,
    )


def print_final_read_summary(result: dict, entries: int) -> None:
    complete = [obs for obs in result["final_reads"] if obs.get("entries_count", 0) == entries]
    print(f"  final reads with all {entries} entries: {len(complete)}/{len(result['final_reads'])}", flush=True)


def print_optional_checks(result: dict) -> None:
    if result.get("verify"):
        print(f"  verify: rc={result['verify']['returncode']}", flush=True)
    if result.get("structure"):
        print(f"  structure: {result['structure'].get('status')}", flush=True)
    if result.get("status") == "FAIL":
        for error in result.get("errors", []):
            print(f"  FAIL: {error}", flush=True)
    else:
        print(f"  status: {result['status']}", flush=True)


def run_live_cases(
    feature_specs: list[FeatureSpec],
    writer_specs: list[WriterSpec],
    reader_specs: list[ReaderSpec],
    tools: dict[str, str],
    args: argparse.Namespace,
) -> list[dict]:
    results = []
    for feature in feature_specs:
        print(f"\n=== feature: {feature.name} ===", flush=True)
        for writer in writer_specs:
            print(f"\n--- {writer.name} writer ---", flush=True)
            result = run_live_case(writer, feature, tools, reader_specs, args)
            print_live_result(result, args.entries)
            results.append(result)
    return results


def result_counts(results: list[dict]) -> tuple[int, int, int]:
    total = len(results)
    passed = sum(1 for result in results if result.get("status") == "PASS")
    return total, passed, total - passed


def live_payload(
    args: argparse.Namespace,
    feature_specs: list[FeatureSpec],
    writer_specs: list[WriterSpec],
    reader_specs: list[ReaderSpec],
    results: list[dict],
) -> dict:
    total, passed, failed = result_counts(results)
    return {
        "timestamp": datetime.now().isoformat(timespec="seconds"),
        "systemd_version": systemd_version(),
        "entries_per_writer": args.entries,
        "features": [feature.name for feature in feature_specs],
        "sealed_verify_key": LIVE_SEAL_VERIFY_KEY,
        "writers": [writer.name for writer in writer_specs],
        "readers": [reader.name for reader in reader_specs],
        "poll_readers_per_lang": args.poll_readers,
        "libsystemd_readers_per_writer": args.libsystemd_readers,
        "poll_interval_sec": args.poll_interval,
        "writer_delay_ms": args.writer_delay_ms,
        "results": results,
        "summary": {"total": total, "passed": passed, "failed": failed},
    }


def timestamped_result_path() -> Path:
    now = datetime.now()
    timestamp = (
        f"{now.year:04d}{now.month:02d}{now.day:02d}-"
        f"{now.hour:02d}{now.minute:02d}{now.second:02d}"
    )
    return LOCAL_DIR / f"live-feature-matrix-results-{timestamp}.json"


def write_payload(payload: dict) -> Path:
    result_path = timestamped_result_path()
    result_path.write_text(json.dumps(payload, indent=2) + "\n")
    return result_path


def print_summary(payload: dict, result_path: Path) -> None:
    print("\n=== SUMMARY ===", flush=True)
    print(f"systemd: {payload['systemd_version']}", flush=True)
    print(f"features: {', '.join(payload['features'])}", flush=True)
    print(f"writers: {', '.join(payload['writers'])}", flush=True)
    print(f"entries per writer: {payload['entries_per_writer']}", flush=True)
    summary = payload["summary"]
    print(f"total: {summary['total']}, passed: {summary['passed']}, failed: {summary['failed']}", flush=True)
    print(f"results: {result_path}", flush=True)


def cleanup_ready_files(keep_files: bool) -> None:
    if keep_files:
        return
    for ready_file in LOCAL_DIR.glob("*.ready"):
        ready_file.unlink(missing_ok=True)


def main() -> int:
    args = parse_args()
    validate_args(args)
    LOCAL_DIR.mkdir(parents=True, exist_ok=True)
    tools = build_tools()
    feature_specs = selected(FEATURES, args.features)
    writer_specs = selected(WRITERS, args.writers)
    reader_specs = selected(READERS, args.readers)

    results = run_live_cases(feature_specs, writer_specs, reader_specs, tools, args)
    payload = live_payload(args, feature_specs, writer_specs, reader_specs, results)
    result_path = write_payload(payload)
    print_summary(payload, result_path)
    cleanup_ready_files(args.keep_files)

    return 0 if payload["summary"]["failed"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
