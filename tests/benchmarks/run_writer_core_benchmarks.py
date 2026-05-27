#!/usr/bin/env python3
"""Run writer-core benchmarks.

This harness measures the SDK/systemd append loop separately from dataset
generation, JSON parsing, final close/sync, and journal verification. Each
driver pre-materializes the deterministic 32-field rows before starting its
append timer, then reports append rows/sec from the timed append loop only.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUT = ROOT / ".local" / "benchmarks" / "writer-core"
BIN_DIR = ROOT / ".local" / "benchmarks" / "bin"
LANGUAGES = ("systemd", "rust", "go", "node", "python")
INCOMPATIBLE_COMPRESSED_XZ = 1 << 0
INCOMPATIBLE_COMPRESSED_LZ4 = 1 << 1
INCOMPATIBLE_COMPRESSED_ZSTD = 1 << 3
INCOMPATIBLE_COMPACT = 1 << 4
INCOMPATIBLE_COMPRESSION_MASK = (
    INCOMPATIBLE_COMPRESSED_XZ | INCOMPATIBLE_COMPRESSED_LZ4 | INCOMPATIBLE_COMPRESSED_ZSTD
)


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
    return subprocess.run(
        cmd,
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
        "GOCACHE": str(local / "go-cache"),
        "GOMODCACHE": str(local / "go-mod-cache"),
        "GOPATH": str(local / "go-path"),
        "CARGO_HOME": str(local / "cargo-home"),
        "CARGO_TARGET_DIR": str(local / "cargo-target"),
        "npm_config_cache": str(local / "npm-cache"),
        "PIP_CACHE_DIR": str(local / "pip-cache"),
        "PYTHONPATH": str(ROOT / "python"),
    }


def build_tool(language: str, env: dict[str, str]) -> tuple[list[str], dict[str, Any]]:
    BIN_DIR.mkdir(parents=True, exist_ok=True)
    if language == "python":
        return [sys.executable, str(ROOT / "python" / "cmd" / "writer_core_bench.py")], {}
    if language == "node":
        return ["node", str(ROOT / "node" / "internal" / "testcmd" / "writer-core-bench.js")], {}
    if language == "go":
        output = BIN_DIR / "go-writer-core-bench"
        require_ok(
            run(
                ["go", "build", "-o", str(output), "./internal/testcmd/writer_core_bench"],
                cwd=ROOT / "go",
                env=env,
                timeout=300,
            ),
            "build go writer-core bench",
        )
        return [str(output)], {"build": "go build ./internal/testcmd/writer_core_bench"}
    if language == "rust":
        require_ok(
            run(
                ["cargo", "build", "--release", "-p", "writer_core_bench"],
                cwd=ROOT / "rust",
                env=env,
                timeout=600,
            ),
            "build rust writer-core bench",
        )
        return [str(ROOT / ".local" / "cargo-target" / "release" / "writer_core_bench")], {
            "build": "cargo build --release -p writer_core_bench"
        }
    if language == "systemd":
        result = run(
            [str(ROOT / "tests" / "benchmarks" / "systemd" / "build_writer_core_bench.sh")],
            env=env,
            timeout=1800,
        )
        require_ok(result, "build systemd writer-core bench")
        binary = result.stdout.strip().splitlines()[-1]
        return [binary], {"build_stdout_tail": result.stdout[-1000:]}
    raise ValueError(language)


def bench_command(
    base: list[str],
    *,
    output: Path,
    rows: int,
    journal_format: str,
    final_state: str,
    max_size_bytes: int,
) -> list[str]:
    cmd = [
        *base,
        "--rows",
        str(rows),
        "--output",
        str(output),
        "--format",
        journal_format,
        "--final-state",
        final_state,
        "--max-size-bytes",
        str(max_size_bytes),
    ]
    return cmd


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
                '{"process_wall_seconds":%e,"process_user_seconds":%U,"process_system_seconds":%S,"max_rss_kb":%M}',
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


def parse_driver_result(stdout: str) -> dict[str, Any]:
    for line in reversed(stdout.splitlines()):
        line = line.strip()
        if line.startswith("{") and line.endswith("}"):
            return json.loads(line)
    return {"errors": ["missing JSON result"], "records": 0}


def verify_journal(path: Path) -> dict[str, Any]:
    if shutil.which("journalctl") is None:
        return {"returncode": 127, "stderr_tail": "journalctl not found", "stdout_tail": ""}
    result = run(["journalctl", "--verify", "--file", str(path)], timeout=300)
    return {
        "returncode": result.returncode,
        "stdout_tail": result.stdout[-1000:],
        "stderr_tail": result.stderr[-1000:],
    }


def quick_header_check(path: Path, *, compact: bool) -> dict[str, Any]:
    try:
        data = path.read_bytes()[:16]
        if len(data) < 16 or data[:8] != b"LPKSHHRH":
            return {"status": "FAIL", "error": "invalid or truncated journal header"}
        incompatible = int.from_bytes(data[12:16], "little")
        actual_compact = bool(incompatible & INCOMPATIBLE_COMPACT)
        compression_flags = incompatible & INCOMPATIBLE_COMPRESSION_MASK
        errors = []
        if actual_compact != compact:
            errors.append(f"compact flag mismatch: got {actual_compact}, want {compact}")
        if compression_flags != 0:
            errors.append(f"unexpected compression flags: {compression_flags:#x}")
        return {
            "status": "PASS" if not errors else "FAIL",
            "compact": actual_compact,
            "incompatible_flags": incompatible,
            "compression_flags": compression_flags,
            "error": "; ".join(errors),
        }
    except OSError as err:
        return {"status": "FAIL", "error": str(err)}


def one_measurement(
    language: str,
    base: list[str],
    *,
    output_dir: Path,
    rows: int,
    repetition: int,
    warmup: bool,
    journal_format: str,
    final_state: str,
    max_size_bytes: int,
    env: dict[str, str],
    verify: bool,
    keep_journals: bool,
) -> dict[str, Any]:
    label = "warmup" if warmup else f"rep-{repetition}"
    run_dir = output_dir / language / label
    run_dir.mkdir(parents=True, exist_ok=True)
    output = run_dir / "output.journal"
    output.unlink(missing_ok=True)

    cmd = bench_command(
        base,
        output=output,
        rows=rows,
        journal_format=journal_format,
        final_state=final_state,
        max_size_bytes=max_size_bytes,
    )
    stats_path = run_dir / "time.json"
    result = timed_run(cmd, stats_path, env)
    stats = parse_time_stats(stats_path)
    driver = parse_driver_result(result.stdout)
    journal_path = Path(driver.get("journal_path") or output)
    file_size = journal_path.stat().st_size if journal_path.exists() else 0
    records = int(driver.get("records", 0) or 0)
    errors = list(driver.get("errors", []) or [])
    structure = quick_header_check(journal_path, compact=journal_format == "compact") if journal_path.exists() else {
        "status": "FAIL",
        "error": "journal file missing",
    }
    verification = verify_journal(journal_path) if verify and journal_path.exists() and not warmup else None
    append_seconds = float(driver.get("append_seconds", 0.0) or 0.0)
    append_rate = float(driver.get("append_rows_per_second", 0.0) or 0.0)
    process_wall = float(stats.get("process_wall_seconds", 0.0) or 0.0)

    item = {
        "language": language,
        "kind": "warmup" if warmup else "measurement",
        "repetition": repetition,
        "command": cmd,
        "returncode": result.returncode,
        "stdout_tail": result.stdout[-1000:],
        "stderr_tail": result.stderr[-1000:],
        "driver": driver,
        "process_time": stats,
        "records": records,
        "expected_records": rows,
        "append_seconds": append_seconds,
        "append_rows_per_second": append_rate,
        "process_rows_per_second": records / process_wall if process_wall > 0 else None,
        "journal_path": str(journal_path) if keep_journals else None,
        "journal_size_bytes": file_size,
        "structure": structure,
        "verify": verification,
        "status": "PASS"
        if result.returncode == 0
        and records == rows
        and not errors
        and structure["status"] == "PASS"
        and (verification is None or verification["returncode"] == 0)
        else "FAIL",
    }

    if not keep_journals:
        output.unlink(missing_ok=True)
        if journal_path != output:
            journal_path.unlink(missing_ok=True)
    return item


def summarize(results: list[dict[str, Any]]) -> dict[str, Any]:
    summary: dict[str, Any] = {}
    for language in LANGUAGES:
        rows = [
            r
            for r in results
            if r["language"] == language and r["kind"] == "measurement" and r["status"] == "PASS"
        ]
        if not rows:
            continue
        append_rates = [float(r["append_rows_per_second"]) for r in rows]
        process_rates = [
            float(r["process_rows_per_second"])
            for r in rows
            if r["process_rows_per_second"] is not None
        ]
        sizes = [int(r["journal_size_bytes"]) for r in rows]
        api_modes = sorted({str(r["driver"].get("api_mode", "unknown")) for r in rows})
        data_buckets = sorted({int(r["driver"].get("data_hash_table_buckets", 0) or 0) for r in rows})
        field_buckets = sorted({int(r["driver"].get("field_hash_table_buckets", 0) or 0) for r in rows})
        max_sizes = sorted({int(r["driver"].get("max_size_bytes", 0) or 0) for r in rows})
        summary[language] = {
            "measurements": len(rows),
            "append_rows_per_second_min": min(append_rates),
            "append_rows_per_second_median": statistics.median(append_rates),
            "append_rows_per_second_max": max(append_rates),
            "process_rows_per_second_median": statistics.median(process_rates) if process_rates else None,
            "journal_size_bytes_median": statistics.median(sizes),
            "api_modes": api_modes,
            "data_hash_table_buckets": data_buckets,
            "field_hash_table_buckets": field_buckets,
            "max_size_bytes": max_sizes,
        }
    systemd_rate = summary.get("systemd", {}).get("append_rows_per_second_median")
    if systemd_rate:
        for item in summary.values():
            item["systemd_append_ratio_median"] = item["append_rows_per_second_median"] / systemd_rate
    return summary


def driver_consistency_failures(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    rows = [
        r
        for r in results
        if r["kind"] == "measurement" and r["status"] == "PASS"
    ]
    failures: list[dict[str, Any]] = []
    for field in (
        "data_hash_table_buckets",
        "field_hash_table_buckets",
        "max_size_bytes",
    ):
        values: dict[str, int | None] = {}
        for row in rows:
            raw = row["driver"].get(field)
            values[row["language"]] = int(raw) if raw is not None else None
        if len(set(values.values())) > 1:
            failures.append(
                {
                    "kind": "cross-driver-consistency",
                    "status": "FAIL",
                    "field": field,
                    "values_by_language": values,
                    "error": f"{field} differs across passing drivers",
                }
            )
    return failures


def first_line(cmd: list[str], env: dict[str, str] | None = None) -> str:
    result = run(cmd, env=env, timeout=30)
    text = result.stdout.strip() or result.stderr.strip()
    return text.splitlines()[0] if text else f"exit {result.returncode}"


def cpu_model() -> str | None:
    cpuinfo = Path("/proc/cpuinfo")
    if not cpuinfo.exists():
        return None
    for line in cpuinfo.read_text(errors="replace").splitlines():
        if line.startswith("model name"):
            return line.split(":", 1)[1].strip()
    return None


def cpu_governor() -> str | None:
    path = Path("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
    if path.exists():
        return path.read_text(encoding="utf-8").strip()
    return None


def filesystem_type(path: Path) -> str | None:
    result = run(["stat", "-f", "-c", "%T", str(path)], timeout=30)
    return result.stdout.strip() if result.returncode == 0 else None


def environment_report(env: dict[str, str], output_dir: Path) -> dict[str, Any]:
    output_dir.mkdir(parents=True, exist_ok=True)
    return {
        "timestamp_utc": datetime.now(timezone.utc).isoformat(),
        "platform": platform.platform(),
        "python": sys.version.split()[0],
        "cpu_count": os.cpu_count(),
        "cpu_model": cpu_model(),
        "cpu_governor": cpu_governor(),
        "filesystem": filesystem_type(output_dir),
        "go": first_line(["go", "version"], env),
        "rustc": first_line(["rustc", "--version"], env),
        "cargo": first_line(["cargo", "--version"], env),
        "node": first_line(["node", "--version"], env),
        "journalctl": first_line(["journalctl", "--version"], env),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--languages", nargs="+", choices=LANGUAGES, default=list(LANGUAGES))
    parser.add_argument("--rows", type=int, default=100_000)
    parser.add_argument("--repetitions", type=int, default=3)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--final-state", choices=("online", "offline", "archived"), default="online")
    parser.add_argument("--format", choices=("compact", "regular"), default="compact")
    parser.add_argument("--skip-verify", action="store_true")
    parser.add_argument("--keep-journals", action="store_true")
    parser.add_argument("--max-size-bytes", type=int, default=128 * 1024 * 1024)
    args = parser.parse_args()

    env = build_env()
    run_id = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    profile = f"{args.format}-none-fss-off"
    out = args.output_dir / f"{profile}-{run_id}"
    out.mkdir(parents=True, exist_ok=True)

    tools = {}
    for language in args.languages:
        base, metadata = build_tool(language, env)
        tools[language] = {"command": base, "metadata": metadata}

    results: list[dict[str, Any]] = []
    for language in args.languages:
        base = tools[language]["command"]
        for warmup in range(args.warmups):
            results.append(
                one_measurement(
                    language,
                    base,
                    output_dir=out,
                    rows=args.rows,
                    repetition=warmup + 1,
                    warmup=True,
                    journal_format=args.format,
                    final_state=args.final_state,
                    max_size_bytes=args.max_size_bytes,
                    env=env,
                    verify=False,
                    keep_journals=False,
                )
            )
        for repetition in range(args.repetitions):
            results.append(
                one_measurement(
                    language,
                    base,
                    output_dir=out,
                    rows=args.rows,
                    repetition=repetition + 1,
                    warmup=False,
                    journal_format=args.format,
                    final_state=args.final_state,
                    max_size_bytes=args.max_size_bytes,
                    env=env,
                    verify=not args.skip_verify,
                    keep_journals=args.keep_journals,
                )
            )

    report = {
        "benchmark": "writer-core",
        "profile": profile,
        "parameters": {
            "format": args.format,
            "compression": "none",
            "fss": False,
            "final_state": args.final_state,
            "rows": args.rows,
            "fields_per_row": 32,
            "repetitions": args.repetitions,
            "warmups": args.warmups,
            "languages": args.languages,
            "keep_journals": args.keep_journals,
            "max_size_bytes": args.max_size_bytes,
            "hash_table_sizing": "systemd v260.1 formula: data=max(max_size*4/768/3,2047), field=1023",
            "append_timer_excludes": [
                "row generation",
                "writer creation",
                "final close/sync",
                "journal verification",
            ],
        },
        "environment": environment_report(env, out),
        "tools": tools,
        "results": results,
        "summary": summarize(results),
    }
    measurement_failures = [
        r for r in results if r["kind"] == "measurement" and r["status"] != "PASS"
    ]
    consistency_failures = driver_consistency_failures(results)
    failures = [*measurement_failures, *consistency_failures]
    report["status"] = "PASS" if not failures else "FAIL"
    report["failures"] = failures

    report_path = out / "report.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True), encoding="utf-8")
    print(
        json.dumps(
            {"status": report["status"], "report": str(report_path), "summary": report["summary"]},
            indent=2,
            sort_keys=True,
        )
    )
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
