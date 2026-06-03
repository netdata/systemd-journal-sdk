"""Runtime, discovery, and build helpers for corpus evaluation."""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import os
import shutil
import subprocess  # nosec B404
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUT = ROOT / ".local" / "corpus-eval"
BIN_DIR = DEFAULT_OUT / "bin"
JOURNAL_SUFFIXES = (".journal", ".journal.zst")


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


@dataclass
class EvaluationRuntime:
    env: dict[str, str]
    tools: ToolPaths
    state_path: Path
    state: dict[str, Any]
    completed: dict[str, Any]
    stats_dir: Path
    work_dir: Path


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
        candidates = [root] if root.is_file() else root_journal_candidates(root)
        for path in sorted(candidates):
            if not path.name.endswith(JOURNAL_SUFFIXES):
                continue
            cases.append(journal_case(root, path))
            if max_files is not None and len(cases) >= max_files:
                return cases
    return cases


def root_journal_candidates(root: Path) -> list[Path]:
    candidates = []
    for dirpath, _, filenames in os.walk(root):
        for filename in filenames:
            if filename.endswith(JOURNAL_SUFFIXES):
                candidates.append(Path(dirpath) / filename)
    return candidates


def journal_case(root: Path, path: Path) -> JournalCase:
    stat = path.stat()
    file_id = safe_file_id(root, path, stat)
    suffix = ".journal.zst" if path.name.endswith(".journal.zst") else ".journal"
    return JournalCase(
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
    with contextlib.suppress(FileNotFoundError):
        snapshot.unlink()
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
    build_results = [run_build_command(label, cmd, cwd, env) for label, cmd, cwd in commands]
    write_json(out / "build-results.json", build_results)
    failed = next((row for row in build_results if row["returncode"] != 0), None)
    if failed:
        raise RuntimeError(f"{failed['label']} failed; see {out / 'build-results.json'}")

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


def run_build_command(label: str, cmd: list[str], cwd: Path, env: dict[str, str]) -> dict[str, Any]:
    started = time.perf_counter()
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    result = subprocess.run(  # nosec B603
        cmd,
        cwd=str(cwd),
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=1800,
        check=False,
    )
    return {
        "label": label,
        "returncode": result.returncode,
        "seconds": time.perf_counter() - started,
        "stdout_sha256": hashlib.sha256(result.stdout.encode()).hexdigest(),
        "stderr_sha256": hashlib.sha256(result.stderr.encode()).hexdigest(),
    }
