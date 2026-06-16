#!/usr/bin/env python3
"""Run reader-core benchmarks.

The harness generates journal fixtures outside the read timer, then measures
reader loops only. It separates single-file hot paths from explicit multi-file
ordered reads so Netdata-style single-file scanning and SDK directory merging do
not hide each other's costs.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import statistics
import subprocess  # nosec B404
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUT = ROOT / ".local" / "benchmarks" / "reader-core"
BIN_DIR = ROOT / ".local" / "benchmarks" / "bin"
DEFAULT_MAX_SIZE_BYTES = 128 * 1024 * 1024
DEFAULT_DIRECTORY_MAX_SIZE_BYTES = 32 * 1024 * 1024
DEFAULT_WINDOW_SIZE = 32 * 1024 * 1024

SINGLE_FILE_CASES = [
    ("rust", "file", "core-next", "live", "windowed"),
    ("rust", "file", "core-offsets", "live", "windowed"),
    ("rust", "file", "core-payloads", "live", "windowed"),
    ("rust", "file", "core-payloads", "live", "whole-file"),
    ("rust", "file", "core-payloads", "snapshot", "windowed"),
    ("rust", "file", "core-payloads", "snapshot", "whole-file"),
    ("rust", "file", "sdk-entry", "live", "windowed"),
    ("rust", "file", "sdk-entry", "live", "whole-file"),
    ("rust", "file", "sdk-entry", "snapshot", "windowed"),
    ("rust", "file", "sdk-entry", "snapshot", "whole-file"),
    ("rust", "file", "sdk-payloads", "live", "windowed"),
    ("rust", "file", "sdk-payloads", "live", "whole-file"),
    ("rust", "file", "sdk-payloads", "snapshot", "windowed"),
    ("rust", "file", "sdk-payloads", "snapshot", "whole-file"),
    ("rust", "file", "facade-data", "live", "windowed"),
    ("rust", "file", "facade-data", "live", "whole-file"),
    ("rust", "file", "facade-data", "snapshot", "windowed"),
    ("rust", "file", "facade-data", "snapshot", "whole-file"),
    ("go", "file", "sdk-entry", "live", "read-at"),
    ("go", "file", "sdk-payloads", "live", "read-at"),
    ("go", "file", "facade-data", "live", "read-at"),
    ("go", "file", "sdk-entry", "live", "auto"),
    ("go", "file", "sdk-payloads", "live", "auto"),
    ("go", "file", "facade-data", "live", "auto"),
    ("go", "file", "sdk-entry", "live", "mmap"),
    ("go", "file", "sdk-payloads", "live", "mmap"),
    ("go", "file", "facade-data", "live", "mmap"),
    ("go", "file", "sdk-entry", "snapshot", "mmap"),
    ("go", "file", "sdk-payloads", "snapshot", "mmap"),
    ("go", "file", "facade-data", "snapshot", "mmap"),
    ("systemd", "file", "next", "", ""),
    ("systemd", "file", "data", "", ""),
]

OPEN_FILES_CASES = [
    ("rust", "open-files", "sdk-entry", "live", "windowed"),
    ("rust", "open-files", "sdk-entry", "live", "whole-file"),
    ("rust", "open-files", "sdk-entry", "snapshot", "windowed"),
    ("rust", "open-files", "sdk-payloads", "live", "windowed"),
    ("rust", "open-files", "sdk-payloads", "live", "whole-file"),
    ("rust", "open-files", "sdk-payloads", "snapshot", "windowed"),
    ("rust", "open-files", "facade-data", "live", "windowed"),
    ("rust", "open-files", "facade-data", "live", "whole-file"),
    ("rust", "open-files", "facade-data", "snapshot", "windowed"),
    ("go", "open-files", "sdk-entry", "live", "read-at"),
    ("go", "open-files", "sdk-payloads", "live", "read-at"),
    ("go", "open-files", "facade-data", "live", "read-at"),
    ("go", "open-files", "sdk-entry", "live", "auto"),
    ("go", "open-files", "sdk-payloads", "live", "auto"),
    ("go", "open-files", "facade-data", "live", "auto"),
    ("go", "open-files", "sdk-entry", "live", "mmap"),
    ("go", "open-files", "sdk-payloads", "live", "mmap"),
    ("go", "open-files", "facade-data", "live", "mmap"),
    ("systemd", "open-files", "data", "", ""),
]

COMPARABLE_RUST_PAYLOAD_MODES = {
    "core-payloads",
    "sdk-entry",
    "sdk-payloads",
    "facade-data",
}

COMPARABLE_GO_PAYLOAD_MODES = {
    "sdk-entry",
    "sdk-payloads",
    "facade-data",
}


def run(
    cmd: list[str],
    *,
    cwd: Path = ROOT,
    env: dict[str, str] | None = None,
    timeout: int | None = None,
) -> subprocess.CompletedProcess[str]:
    merged = os.environ.copy()
    if env:
        merged.update(env)
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    return subprocess.run(  # nosec B603
        cmd,  # nosemgrep
        cwd=str(cwd),
        env=merged,
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
    local = ROOT / ".local"
    return {
        "CARGO_HOME": str(local / "cargo-home"),
        "CARGO_TARGET_DIR": str(local / "cargo-target"),
        "GOCACHE": str(local / "go-cache"),
        "GOMODCACHE": str(local / "go-mod-cache"),
        "GOPATH": str(local / "go-path"),
    }


def parse_json_result(stdout: str) -> dict[str, Any]:
    for line in reversed(stdout.splitlines()):
        line = line.strip()
        if line.startswith("{") and line.endswith("}"):
            return json.loads(line)
    raise RuntimeError(f"missing JSON result in stdout:\n{stdout[-2000:]}")


def parse_time_stats(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {"raw": path.read_text(encoding="utf-8")}


def timed_run(cmd: list[str], stats_path: Path, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    time_bin = shutil.which("time")
    if time_bin:
        stats_path.parent.mkdir(parents=True, exist_ok=True)
        return run(
            [
                time_bin,
                "-f",
                (
                    '{"process_wall_seconds":%e,"process_user_seconds":%U,'
                    '"process_system_seconds":%S,"max_rss_kb":%M,'
                    '"minor_page_faults":%R,"major_page_faults":%F,'
                    '"voluntary_context_switches":%w,'
                    '"involuntary_context_switches":%c}'
                ),
                "-o",
                str(stats_path),
                *cmd,
            ],
            env=env,
            timeout=1800,
        )
    started = time.perf_counter()
    result = run(cmd, env=env, timeout=1800)
    stats_path.write_text(
        json.dumps({"process_wall_seconds": time.perf_counter() - started}),
        encoding="utf-8",
    )
    return result


def build_tools(env: dict[str, str]) -> dict[str, list[str]]:
    BIN_DIR.mkdir(parents=True, exist_ok=True)
    require_ok(
        run(
            ["cargo", "build", "--release", "-p", "writer_core_bench", "-p", "reader_core_bench"],
            cwd=ROOT / "rust",
            env=env,
            timeout=900,
        ),
        "build rust reader/writer core benches",
    )
    systemd_build = run(
        [str(ROOT / "tests" / "benchmarks" / "systemd" / "build_reader_core_bench.sh")],
        env=env,
        timeout=300,
    )
    require_ok(systemd_build, "build systemd reader-core bench")
    require_ok(
        run(
            ["go", "build", "-o", str(BIN_DIR / "go-reader-core-bench"), "./internal/testcmd/reader_core_bench"],
            cwd=ROOT / "go",
            env=env,
            timeout=300,
        ),
        "build go reader-core bench",
    )
    systemd_binary = systemd_build.stdout.strip().splitlines()[-1]
    return {
        "rust_writer": [str(ROOT / ".local" / "cargo-target" / "release" / "writer_core_bench")],
        "rust_reader": [str(ROOT / ".local" / "cargo-target" / "release" / "reader_core_bench")],
        "go_reader": [str(BIN_DIR / "go-reader-core-bench")],
        "systemd_reader": [systemd_binary],
    }


def generate_direct_fixture(
    tools: dict[str, list[str]],
    env: dict[str, str],
    fixture_dir: Path,
    rows: int,
    journal_format: str,
    final_state: str,
    max_size_bytes: int,
) -> dict[str, Any]:
    path = fixture_dir / "single" / "system.journal"
    path.parent.mkdir(parents=True, exist_ok=True)
    result = run(
        [
            *tools["rust_writer"],
            "--rows",
            str(rows),
            "--output",
            str(path),
            "--format",
            journal_format,
            "--final-state",
            final_state,
            "--surface",
            "direct",
            "--max-size-bytes",
            str(max_size_bytes),
            "--api-mode",
            "raw-payload",
            "--live-publish-every-entries",
            "0",
        ],
        env=env,
        timeout=1800,
    )
    require_ok(result, "generate direct reader fixture")
    info = parse_json_result(result.stdout)
    return {"path": str(Path(info["journal_path"])), "writer_result": info}


def generate_directory_fixture(
    tools: dict[str, list[str]],
    env: dict[str, str],
    fixture_dir: Path,
    rows: int,
    journal_format: str,
    max_size_bytes: int,
) -> dict[str, Any]:
    path = fixture_dir / "directory"
    result = run(
        [
            *tools["rust_writer"],
            "--rows",
            str(rows),
            "--output",
            str(path),
            "--format",
            journal_format,
            "--final-state",
            "archived",
            "--surface",
            "directory",
            "--max-size-bytes",
            str(max_size_bytes),
            "--rotation-max-size-bytes",
            str(max_size_bytes),
            "--api-mode",
            "raw-payload",
            "--live-publish-every-entries",
            "0",
        ],
        env=env,
        timeout=1800,
    )
    require_ok(result, "generate directory reader fixture")
    info = parse_json_result(result.stdout)
    files = [str(Path(raw)) for raw in info.get("journal_files", [])]
    if not files:
        raise RuntimeError("directory fixture generated no journal files")
    return {
        "path": str(path),
        "journal_directory": str(Path(info["journal_directory"])),
        "files": files,
        "writer_result": info,
    }


def case_command(
    tools: dict[str, list[str]],
    language: str,
    surface: str,
    mode: str,
    inputs: list[str],
    direction: str,
    window_size: int,
    bounds: str,
    mmap_strategy: str,
) -> list[str]:
    if language == "rust":
        cmd = [
            *tools["rust_reader"],
            "--surface",
            surface,
            "--mode",
            mode,
            "--direction",
            direction,
            "--window-size",
            str(window_size),
            "--bounds",
            bounds,
            "--mmap-strategy",
            mmap_strategy,
        ]
    elif language == "systemd":
        cmd = [
            *tools["systemd_reader"],
            "--surface",
            surface,
            "--mode",
            mode,
            "--direction",
            direction,
        ]
    elif language == "go":
        cmd = [
            *tools["go_reader"],
            "--surface",
            surface,
            "--mode",
            mode,
            "--direction",
            direction,
            "--window-size",
            str(window_size),
            "--bounds",
            bounds,
            "--mmap-strategy",
            mmap_strategy,
        ]
    else:
        raise ValueError(language)

    for path in inputs:
        cmd.extend(["--input", path])
    return cmd


def validate_equivalent_checksums(runs: list[dict[str, Any]]) -> None:
    references = systemd_checksum_references(runs)
    errors = []
    for item in runs:
        if item.get("warmup"):
            continue
        result = item["result"]
        if not is_comparable_checksum_result(result):
            continue
        errors.extend(checksum_mismatches(result, references))
    if errors:
        raise RuntimeError(f"reader checksum mismatch: {json.dumps(errors, indent=2)}")


def systemd_checksum_references(
    runs: list[dict[str, Any]],
) -> dict[tuple[str, str], dict[str, Any]]:
    references: dict[tuple[str, str], dict[str, Any]] = {}
    for item in runs:
        if item.get("warmup"):
            continue
        result = item["result"]
        if result["language"] != "systemd" or result["mode"] != "data":
            continue
        key = (result["surface"], result["direction"])
        existing = references.get(key)
        if existing is not None and checksum_fields_differ(existing, result):
            raise RuntimeError(f"systemd checksum changed across runs for {key}")
        references[key] = result
    return references


def checksum_fields_differ(left: dict[str, Any], right: dict[str, Any]) -> bool:
    return any(left[field] != right[field] for field in CHECKSUM_FIELDS)


def is_comparable_checksum_result(result: dict[str, Any]) -> bool:
    language = result["language"]
    mode = result["mode"]
    return (
        (language == "rust" and mode in COMPARABLE_RUST_PAYLOAD_MODES)
        or (language == "go" and mode in COMPARABLE_GO_PAYLOAD_MODES)
    )


CHECKSUM_FIELDS = ("records", "fields", "bytes", "checksum")


def checksum_mismatches(
    result: dict[str, Any],
    references: dict[tuple[str, str], dict[str, Any]],
) -> list[dict[str, Any]]:
    key = (result["surface"], result["direction"])
    reference = references.get(key)
    if reference is None:
        return []
    return [
        checksum_mismatch(result, reference, field)
        for field in CHECKSUM_FIELDS
        if result[field] != reference[field]
    ]


def checksum_mismatch(
    result: dict[str, Any],
    reference: dict[str, Any],
    field: str,
) -> dict[str, Any]:
    return {
        "surface": result["surface"],
        "direction": result["direction"],
        "language": result["language"],
        "mode": result["mode"],
        "bounds": result.get("bounds", ""),
        "mmap_strategy": result.get("mmap_strategy", ""),
        "field": field,
        "sdk": result[field],
        "systemd": reference[field],
    }


def summarize(runs: list[dict[str, Any]]) -> list[dict[str, Any]]:
    groups: dict[tuple[str, str, str, str, str, str], list[dict[str, Any]]] = {}
    for item in runs:
        if item.get("warmup"):
            continue
        result = item["result"]
        key = (
            result["language"],
            result["surface"],
            result["mode"],
            result["direction"],
            result.get("bounds", ""),
            result.get("mmap_strategy", ""),
        )
        groups.setdefault(key, []).append(result)

    out = []
    for key, items in sorted(groups.items()):
        rates = [float(item["read_rows_per_second"]) for item in items]
        seconds = [float(item["read_seconds"]) for item in items]
        fields = [float(item["read_fields_per_second"]) for item in items]
        out.append(
            {
                "language": key[0],
                "surface": key[1],
                "mode": key[2],
                "direction": key[3],
                "bounds": key[4],
                "mmap_strategy": key[5],
                "runs": len(items),
                "records": items[-1]["records"],
                "fields": items[-1]["fields"],
                "bytes": items[-1]["bytes"],
                "median_read_rows_per_second": statistics.median(rates),
                "min_read_rows_per_second": min(rates),
                "max_read_rows_per_second": max(rates),
                "median_read_fields_per_second": statistics.median(fields),
                "median_read_seconds": statistics.median(seconds),
            }
        )
    return out


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--rows", type=int, default=100_000)
    parser.add_argument("--directory-rows", type=int, default=100_000)
    parser.add_argument("--repetitions", type=int, default=3)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--format", choices=("compact", "regular"), default="compact")
    parser.add_argument("--final-state", choices=("online", "offline", "archived"), default="online")
    parser.add_argument("--max-size-bytes", type=int, default=DEFAULT_MAX_SIZE_BYTES)
    parser.add_argument("--directory-max-size-bytes", type=int, default=DEFAULT_DIRECTORY_MAX_SIZE_BYTES)
    parser.add_argument("--window-size", type=int, default=DEFAULT_WINDOW_SIZE)
    parser.add_argument("--direction", choices=("forward", "backward"), default="forward")
    parser.add_argument("--languages", default="", help="Comma-separated language filter, e.g. rust,go,systemd")
    parser.add_argument("--skip-open-files", action="store_true")
    parser.add_argument("--keep-fixtures", action="store_true")
    return parser.parse_args()


def timestamp_id() -> str:
    now = datetime.now(timezone.utc)
    return (
        f"{now.year:04d}{now.month:02d}{now.day:02d}T"
        f"{now.hour:02d}{now.minute:02d}{now.second:02d}Z"
    )


def prepare_run_directories(args: argparse.Namespace, timestamp: str) -> tuple[Path, Path]:
    run_dir = args.out / timestamp
    fixture_dir = run_dir / "fixtures"
    run_dir.mkdir(parents=True, exist_ok=True)
    return run_dir, fixture_dir


def prepare_reader_fixtures(
    args: argparse.Namespace,
    tools: dict[str, Any],
    env: dict[str, str],
    fixture_dir: Path,
) -> tuple[dict[str, Any], dict[str, Any] | None]:
    direct_fixture = generate_direct_fixture(
        tools,
        env,
        fixture_dir,
        args.rows,
        args.format,
        args.final_state,
        args.max_size_bytes,
    )
    if args.skip_open_files:
        return direct_fixture, None
    directory_fixture = generate_directory_fixture(
        tools,
        env,
        fixture_dir,
        args.directory_rows,
        args.format,
        args.directory_max_size_bytes,
    )
    return direct_fixture, directory_fixture


def selected_reader_cases(
    args: argparse.Namespace,
    directory_fixture: dict[str, Any] | None,
) -> list[tuple[str, str, str, str, str]]:
    cases = list(SINGLE_FILE_CASES)
    if directory_fixture is not None:
        cases.extend(OPEN_FILES_CASES)
    if args.languages:
        wanted_languages = {item.strip() for item in args.languages.split(",") if item.strip()}
        cases = [case for case in cases if case[0] in wanted_languages]
    return cases


def run_reader_cases(
    args: argparse.Namespace,
    tools: dict[str, Any],
    env: dict[str, str],
    run_dir: Path,
    direct_fixture: dict[str, Any],
    directory_fixture: dict[str, Any] | None,
) -> list[dict[str, Any]]:
    runs: list[dict[str, Any]] = []
    cases = selected_reader_cases(args, directory_fixture)

    total_iterations = args.warmups + args.repetitions
    for language, surface, mode, bounds, mmap_strategy in cases:
        inputs = reader_case_inputs(surface, direct_fixture, directory_fixture)
        for iteration in range(total_iterations):
            runs.append(
                run_reader_iteration(
                    args,
                    tools,
                    env,
                    run_dir,
                    language,
                    surface,
                    mode,
                    bounds,
                    mmap_strategy,
                    inputs,
                    iteration,
                )
            )
    return runs


def reader_case_inputs(
    surface: str,
    direct_fixture: dict[str, Any],
    directory_fixture: dict[str, Any] | None,
) -> list[str]:
    if surface == "file":
        return [direct_fixture["path"]]
    if directory_fixture is None:
        raise RuntimeError("open-files reader case requires directory fixture")
    return list(directory_fixture["files"])


def run_reader_iteration(
    args: argparse.Namespace,
    tools: dict[str, Any],
    env: dict[str, str],
    run_dir: Path,
    language: str,
    surface: str,
    mode: str,
    bounds: str,
    mmap_strategy: str,
    inputs: list[str],
    iteration: int,
) -> dict[str, Any]:
    warmup = iteration < args.warmups
    stats_path = run_dir / "time" / f"{language}-{surface}-{mode}-{iteration}.json"
    cmd = case_command(
        tools,
        language,
        surface,
        mode,
        inputs,
        args.direction,
        args.window_size,
        bounds,
        mmap_strategy,
    )
    result = timed_run(cmd, stats_path, env)
    require_ok(result, f"reader bench {language}/{surface}/{mode} iteration {iteration}")
    parsed = parse_json_result(result.stdout)
    print_reader_progress(language, surface, mode, bounds, mmap_strategy, iteration, warmup, parsed)
    return {
        "warmup": warmup,
        "iteration": iteration,
        "command": cmd,
        "result": parsed,
        "process_stats": parse_time_stats(stats_path),
        "stderr_tail": result.stderr[-2000:],
    }


def print_reader_progress(
    language: str,
    surface: str,
    mode: str,
    bounds: str,
    mmap_strategy: str,
    iteration: int,
    warmup: bool,
    parsed: dict[str, Any],
) -> None:
    print(
        json.dumps(
            {
                "warmup": warmup,
                "language": language,
                "surface": surface,
                "mode": mode,
                "bounds": bounds,
                "mmap_strategy": mmap_strategy,
                "iteration": iteration,
                "records": parsed.get("records"),
                "read_rows_per_second": parsed.get("read_rows_per_second"),
            }
        ),
        flush=True,
    )


def reader_manifest(
    args: argparse.Namespace,
    timestamp: str,
    direct_fixture: dict[str, Any],
    directory_fixture: dict[str, Any] | None,
    summary: list[dict[str, Any]],
) -> dict[str, Any]:
    return {
        "created_at": timestamp,
        "host": os.uname().nodename,
        "format": args.format,
        "final_state": args.final_state,
        "rows": args.rows,
        "directory_rows": args.directory_rows,
        "max_size_bytes": args.max_size_bytes,
        "directory_max_size_bytes": args.directory_max_size_bytes,
        "window_size": args.window_size,
        "direction": args.direction,
        "languages": args.languages,
        "direct_fixture": direct_fixture,
        "directory_fixture": directory_fixture,
        "timer_excludes": ["fixture generation", "tool builds", "process startup", "external verification"],
        "summary": summary,
    }


def write_reader_artifacts(
    args: argparse.Namespace,
    timestamp: str,
    run_dir: Path,
    direct_fixture: dict[str, Any],
    directory_fixture: dict[str, Any] | None,
    runs: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    summary = summarize(runs)
    manifest = reader_manifest(args, timestamp, direct_fixture, directory_fixture, summary)
    (run_dir / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    with (run_dir / "runs.jsonl").open("w", encoding="utf-8") as f:
        for item in runs:
            f.write(json.dumps(item) + "\n")
    (run_dir / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    return summary


def update_latest_link(args: argparse.Namespace, run_dir: Path) -> None:
    latest = args.out / "latest"
    if latest.is_symlink() or latest.exists():
        if latest.is_dir() and not latest.is_symlink():
            shutil.rmtree(latest)
        else:
            latest.unlink()
    latest.symlink_to(run_dir.resolve(), target_is_directory=True)


def main() -> int:
    args = parse_args()
    env = build_env()
    timestamp = timestamp_id()
    run_dir, fixture_dir = prepare_run_directories(args, timestamp)
    tools = build_tools(env)
    direct_fixture, directory_fixture = prepare_reader_fixtures(args, tools, env, fixture_dir)
    runs = run_reader_cases(args, tools, env, run_dir, direct_fixture, directory_fixture)
    validate_equivalent_checksums(runs)
    summary = write_reader_artifacts(
        args,
        timestamp,
        run_dir,
        direct_fixture,
        directory_fixture,
        runs,
    )
    update_latest_link(args, run_dir)
    if not args.keep_fixtures:
        shutil.rmtree(fixture_dir, ignore_errors=True)

    print(json.dumps({"run_dir": str(run_dir), "summary": summary}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
