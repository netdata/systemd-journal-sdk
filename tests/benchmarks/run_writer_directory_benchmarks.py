#!/usr/bin/env python3
"""Run high-level directory writer rotation benchmarks.

This harness measures the SDK Log append loop with active-file rotation enabled.
Rows are pre-materialized by each language driver before timing. Final close,
stock verification, and stock directory readback are outside the append timer.
"""

from __future__ import annotations

import argparse
import json
import statistics
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from run_writer_core_benchmarks import (
    DEFAULT_OUT,
    build_env,
    build_tool,
    environment_report,
    parse_driver_result,
    parse_time_stats,
    quick_header_check,
    run,
    timed_run,
)


ROOT = Path(__file__).resolve().parents[2]
LANGUAGES = ("systemd", "rust", "go", "node", "python")


def bench_command(
    language: str,
    base: list[str],
    *,
    output: Path,
    rows: int,
    journal_format: str,
    max_size_bytes: int,
    rotation_max_size_bytes: int,
    api_mode: str,
    live_publish_every_entries: int,
) -> list[str]:
    cmd = [
        *base,
        "--surface",
        "directory",
        "--rows",
        str(rows),
        "--output",
        str(output),
        "--format",
        journal_format,
        "--final-state",
        "archived",
        "--max-size-bytes",
        str(max_size_bytes),
        "--rotation-max-size-bytes",
        str(rotation_max_size_bytes),
    ]
    if language != "systemd":
        cmd.extend(["--api-mode", api_mode])
        cmd.extend(["--live-publish-every-entries", str(live_publish_every_entries)])
    else:
        cmd.extend(["--api-mode", "raw-payload"])
        cmd.extend(["--live-publish-every-entries", "1"])
    return cmd


def verify_file(path: Path) -> dict[str, Any]:
    result = run(["journalctl", "--verify", "--file", str(path)], timeout=300)
    return {
        "path": str(path),
        "returncode": result.returncode,
        "stdout_tail": result.stdout[-1000:],
        "stderr_tail": result.stderr[-1000:],
    }


def count_directory_rows(directory: Path) -> dict[str, Any]:
    result = run(
        ["journalctl", "--directory", str(directory), "--output=json", "--no-pager"],
        timeout=600,
    )
    if result.returncode != 0:
        return {
            "returncode": result.returncode,
            "rows": 0,
            "stdout_tail": result.stdout[-1000:],
            "stderr_tail": result.stderr[-1000:],
        }
    rows = sum(1 for line in result.stdout.splitlines() if line.strip())
    return {
        "returncode": 0,
        "rows": rows,
        "stdout_tail": result.stdout[-1000:],
        "stderr_tail": result.stderr[-1000:],
    }


def one_measurement(
    language: str,
    base: list[str],
    *,
    output_dir: Path,
    rows: int,
    repetition: int,
    warmup: bool,
    journal_format: str,
    max_size_bytes: int,
    rotation_max_size_bytes: int,
    api_mode: str,
    live_publish_every_entries: int,
    env: dict[str, str],
    verify: bool,
    keep_journals: bool,
) -> dict[str, Any]:
    label = "warmup" if warmup else f"rep-{repetition}"
    run_dir = output_dir / language / label
    run_dir.mkdir(parents=True, exist_ok=True)
    output = run_dir / "journal-dir"

    cmd = bench_command(
        language,
        base,
        output=output,
        rows=rows,
        journal_format=journal_format,
        max_size_bytes=max_size_bytes,
        rotation_max_size_bytes=rotation_max_size_bytes,
        api_mode=api_mode,
        live_publish_every_entries=live_publish_every_entries,
    )
    stats_path = run_dir / "time.json"
    result = timed_run(cmd, stats_path, env)
    stats = parse_time_stats(stats_path)
    driver = parse_driver_result(result.stdout)
    records = int(driver.get("records", 0) or 0)
    errors = list(driver.get("errors", []) or [])
    directory = Path(driver.get("journal_directory") or output)
    files = [Path(path) for path in driver.get("journal_files", [])]
    if not files and directory.exists():
        files = sorted(path for path in directory.rglob("*.journal") if path.is_file())
    file_checks = [
        quick_header_check(path, compact=journal_format == "compact")
        if path.exists()
        else {"status": "FAIL", "error": "journal file missing"}
        for path in files
    ]
    verifications = [verify_file(path) for path in files] if verify and not warmup else []
    row_count = count_directory_rows(directory) if verify and not warmup else None
    append_seconds = float(driver.get("append_seconds", 0.0) or 0.0)
    process_wall = float(stats.get("process_wall_seconds", 0.0) or 0.0)
    status = (
        "PASS"
        if result.returncode == 0
        and records == rows
        and not errors
        and files
        and all(check["status"] == "PASS" for check in file_checks)
        and all(item["returncode"] == 0 for item in verifications)
        and (row_count is None or (row_count["returncode"] == 0 and row_count["rows"] == rows))
        else "FAIL"
    )
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
        "append_rows_per_second": float(driver.get("append_rows_per_second", 0.0) or 0.0),
        "process_rows_per_second": records / process_wall if process_wall > 0 else None,
        "journal_directory": str(directory) if keep_journals else None,
        "journal_files": [str(path) for path in files] if keep_journals else None,
        "journal_file_count": len(files),
        "journal_size_bytes": int(driver.get("journal_size_bytes", 0) or 0),
        "structure": file_checks,
        "verify": verifications,
        "stock_directory_read": row_count,
        "status": status,
    }
    if not keep_journals:
        import shutil

        shutil.rmtree(output, ignore_errors=True)
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
        counts = [int(r["journal_file_count"]) for r in rows]
        summary[language] = {
            "measurements": len(rows),
            "append_rows_per_second_min": min(append_rates),
            "append_rows_per_second_median": statistics.median(append_rates),
            "append_rows_per_second_max": max(append_rates),
            "process_rows_per_second_median": statistics.median(process_rates) if process_rates else None,
            "journal_size_bytes_median": statistics.median(sizes),
            "journal_file_count_median": statistics.median(counts),
        }
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
        "rotation_max_size_bytes",
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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--languages", nargs="+", choices=LANGUAGES, default=list(LANGUAGES))
    parser.add_argument("--rows", type=int, default=200_000)
    parser.add_argument("--repetitions", type=int, default=1)
    parser.add_argument("--warmups", type=int, default=0)
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUT.parent / "writer-directory")
    parser.add_argument("--format", choices=("compact", "regular"), default="compact")
    parser.add_argument("--skip-verify", action="store_true")
    parser.add_argument("--keep-journals", action="store_true")
    parser.add_argument("--max-size-bytes", type=int, default=128 * 1024 * 1024)
    parser.add_argument("--rotation-max-size-bytes", type=int, default=128 * 1024 * 1024)
    parser.add_argument("--api-mode", choices=("raw-payload", "structured-field"), default="raw-payload")
    parser.add_argument("--live-publish-every-entries", type=int, default=1)
    args = parser.parse_args()
    if args.max_size_bytes != args.rotation_max_size_bytes:
        parser.error(
            "--max-size-bytes and --rotation-max-size-bytes must match for comparable "
            "writer-directory benchmarks"
        )

    env = build_env()
    now = datetime.now(timezone.utc)
    run_id = (
        f"{now.year:04d}{now.month:02d}{now.day:02d}T"
        f"{now.hour:02d}{now.minute:02d}{now.second:02d}{now.microsecond:06d}Z"
    )
    profile = (
        f"{args.format}-none-fss-off-directory-api-{args.api_mode}"
        f"-live-every-{args.live_publish_every_entries}"
        f"-rotate-{args.rotation_max_size_bytes}"
    )
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
                    max_size_bytes=args.max_size_bytes,
                    rotation_max_size_bytes=args.rotation_max_size_bytes,
                    api_mode=args.api_mode,
                    live_publish_every_entries=args.live_publish_every_entries,
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
                    max_size_bytes=args.max_size_bytes,
                    rotation_max_size_bytes=args.rotation_max_size_bytes,
                    api_mode=args.api_mode,
                    live_publish_every_entries=args.live_publish_every_entries,
                    env=env,
                    verify=not args.skip_verify,
                    keep_journals=args.keep_journals,
                )
            )

    failures = [r for r in results if r["kind"] == "measurement" and r["status"] != "PASS"]
    failures.extend(driver_consistency_failures(results))
    report = {
        "benchmark": "writer-directory",
        "profile": profile,
        "parameters": {
            "format": args.format,
            "compression": "none",
            "fss": False,
            "surface": "directory",
            "rows": args.rows,
            "fields_per_row": 32,
            "repetitions": args.repetitions,
            "warmups": args.warmups,
            "languages": args.languages,
            "keep_journals": args.keep_journals,
            "max_size_bytes": args.max_size_bytes,
            "rotation_max_size_bytes": args.rotation_max_size_bytes,
            "api_mode": args.api_mode,
            "live_publish_every_entries": args.live_publish_every_entries,
            "append_timer_excludes": [
                "row generation",
                "writer creation",
                "final close/sync",
                "journal verification",
                "stock directory readback",
            ],
        },
        "environment": environment_report(env, out),
        "tools": tools,
        "results": results,
        "summary": summarize(results),
        "status": "PASS" if not failures else "FAIL",
        "failures": failures,
    }
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
