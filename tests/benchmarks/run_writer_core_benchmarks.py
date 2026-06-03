#!/usr/bin/env python3
"""Run writer-core benchmarks.

This harness measures the SDK/systemd append loop separately from dataset
generation, JSON parsing, final close/sync, and journal verification. Each
driver pre-materializes the deterministic 32-field rows before starting its
append timer, then reports append rows/sec from the timed append loop only.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import statistics
import subprocess  # nosec B404 - subprocess is required by harnesses.
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
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    return subprocess.run(  # nosec B603 - harness uses shell=False command vectors.
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
    language: str,
    output: Path,
    rows: int,
    journal_format: str,
    final_state: str,
    max_size_bytes: int,
    api_mode: str,
    rust_trusted_unique_payloads: bool,
    live_publish_every_entries: int,
    rust_mmap_strategy: str,
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
    if language != "systemd":
        cmd.extend(["--live-publish-every-entries", str(live_publish_every_entries)])
        cmd.extend(["--api-mode", api_mode])
    if language == "rust":
        cmd.extend(["--mmap-strategy", rust_mmap_strategy])
        if rust_trusted_unique_payloads:
            cmd.append("--trusted-unique-payloads")
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


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def rust_api_mode_byte_identity(
    base: list[str],
    args: argparse.Namespace,
    output_dir: Path,
    live_publish_every_entries: int,
    env: dict[str, str],
) -> dict[str, Any]:
    compare_dir = output_dir / "rust-api-mode-byte-identity"
    compare_dir.mkdir(parents=True, exist_ok=True)
    measurements: list[dict[str, Any]] = []

    for mode in ("raw-payload", "structured-field"):
        measurements.append(
            rust_api_mode_measurement(
                base,
                args,
                compare_dir,
                mode,
                live_publish_every_entries,
                env,
            )
        )

    status, errors = rust_api_identity_status(measurements)
    cleanup_rust_api_identity_outputs(compare_dir, measurements, args.keep_journals)
    return {
        "kind": "rust-api-mode-byte-identity",
        "status": status,
        "rows": args.rows,
        "fields_per_row": 32,
        "format": args.format,
        "final_state": args.final_state,
        "max_size_bytes": args.max_size_bytes,
        "trusted_unique_payloads": args.rust_trusted_unique_payloads,
        "live_publish_every_entries": live_publish_every_entries,
        "mmap_strategy": args.rust_mmap_strategy,
        "measurements": measurements,
        "errors": errors,
    }


def rust_api_mode_measurement(
    base: list[str],
    args: argparse.Namespace,
    compare_dir: Path,
    mode: str,
    live_publish_every_entries: int,
    env: dict[str, str],
) -> dict[str, Any]:
    output = compare_dir / f"{mode}.journal"
    output.unlink(missing_ok=True)
    cmd = bench_command(
        base,
        language="rust",
        output=output,
        rows=args.rows,
        journal_format=args.format,
        final_state=args.final_state,
        max_size_bytes=args.max_size_bytes,
        api_mode=mode,
        rust_trusted_unique_payloads=args.rust_trusted_unique_payloads,
        live_publish_every_entries=live_publish_every_entries,
        rust_mmap_strategy=args.rust_mmap_strategy,
    )
    result = run(cmd, env=env, timeout=1800)
    driver = parse_driver_result(result.stdout)
    journal_path = Path(driver.get("journal_path") or output)
    return rust_api_mode_result(args, mode, cmd, result, driver, journal_path)


def rust_api_mode_result(
    args: argparse.Namespace,
    mode: str,
    cmd: list[str],
    result: subprocess.CompletedProcess[str],
    driver: dict[str, Any],
    journal_path: Path,
) -> dict[str, Any]:
    exists = journal_path.exists()
    structure = (
        quick_header_check(journal_path, compact=args.format == "compact")
        if exists
        else {"status": "FAIL", "error": "journal file missing"}
    )
    verification = verify_journal(journal_path) if not args.skip_verify and exists else None
    records = int(driver.get("records", 0) or 0)
    errors = list(driver.get("errors", []) or [])
    return {
        "api_mode": mode,
        "command": cmd,
        "returncode": result.returncode,
        "stdout_tail": result.stdout[-1000:],
        "stderr_tail": result.stderr[-1000:],
        "driver": driver,
        "records": records,
        "expected_records": args.rows,
        "journal_path": str(journal_path) if args.keep_journals else None,
        "journal_size_bytes": journal_path.stat().st_size if exists else 0,
        "sha256": sha256_file(journal_path) if exists else None,
        "structure": structure,
        "verify": verification,
        "status": rust_api_mode_status(result, records, args.rows, errors, structure, verification),
    }


def rust_api_mode_status(
    result: subprocess.CompletedProcess[str],
    records: int,
    expected_records: int,
    errors: list[Any],
    structure: dict[str, Any],
    verification: dict[str, Any] | None,
) -> str:
    passed = (
        result.returncode == 0
        and records == expected_records
        and not errors
        and structure["status"] == "PASS"
        and (verification is None or verification["returncode"] == 0)
    )
    return "PASS" if passed else "FAIL"


def rust_api_identity_status(measurements: list[dict[str, Any]]) -> tuple[str, list[str]]:
    raw = next(item for item in measurements if item["api_mode"] == "raw-payload")
    structured = next(item for item in measurements if item["api_mode"] == "structured-field")
    status = "PASS"
    errors: list[str] = []
    if any(item["status"] != "PASS" for item in measurements):
        status = "FAIL"
        errors.append("one or more Rust API mode comparison runs failed")
    if raw["journal_size_bytes"] != structured["journal_size_bytes"]:
        status = "FAIL"
        errors.append("raw-payload and structured-field output sizes differ")
    if raw["sha256"] != structured["sha256"]:
        status = "FAIL"
        errors.append("raw-payload and structured-field output hashes differ")
    return status, errors


def cleanup_rust_api_identity_outputs(
    compare_dir: Path,
    measurements: list[dict[str, Any]],
    keep_journals: bool,
) -> None:
    if keep_journals:
        return
    for item in measurements:
        path = Path(item["driver"].get("journal_path") or compare_dir / f"{item['api_mode']}.journal")
        path.unlink(missing_ok=True)


def one_measurement(
    language: str,
    base: list[str],
    args: argparse.Namespace,
    output_dir: Path,
    repetition: int,
    warmup: bool,
    live_publish_every_entries: int,
    env: dict[str, str],
) -> dict[str, Any]:
    label = "warmup" if warmup else f"rep-{repetition}"
    run_dir = output_dir / language / label
    run_dir.mkdir(parents=True, exist_ok=True)
    output = run_dir / "output.journal"
    output.unlink(missing_ok=True)

    cmd = bench_command(
        base,
        language=language,
        output=output,
        rows=args.rows,
        journal_format=args.format,
        final_state=args.final_state,
        max_size_bytes=args.max_size_bytes,
        api_mode=args.api_mode,
        rust_trusted_unique_payloads=args.rust_trusted_unique_payloads,
        live_publish_every_entries=live_publish_every_entries,
        rust_mmap_strategy=args.rust_mmap_strategy,
    )
    stats_path = run_dir / "time.json"
    result = timed_run(cmd, stats_path, env)
    stats = parse_time_stats(stats_path)
    driver = parse_driver_result(result.stdout)
    journal_path = Path(driver.get("journal_path") or output)
    file_size = journal_path.stat().st_size if journal_path.exists() else 0
    records = int(driver.get("records", 0) or 0)
    errors = list(driver.get("errors", []) or [])
    structure = quick_header_check(journal_path, compact=args.format == "compact") if journal_path.exists() else {
        "status": "FAIL",
        "error": "journal file missing",
    }
    verification = verify_journal(journal_path) if should_verify(args, warmup, journal_path) else None
    append_seconds = float(driver.get("append_seconds", 0.0) or 0.0)
    append_rate = float(driver.get("append_rows_per_second", 0.0) or 0.0)
    process_wall = float(stats.get("process_wall_seconds", 0.0) or 0.0)

    item = writer_core_measurement_item(
        {
            "language": language,
            "repetition": repetition,
            "warmup": warmup,
            "command": cmd,
            "result": result,
            "driver": driver,
            "stats": stats,
            "records": records,
            "expected_records": args.rows,
            "append_seconds": append_seconds,
            "append_rate": append_rate,
            "process_wall": process_wall,
            "journal_path": journal_path,
            "file_size": file_size,
            "keep_journals": args.keep_journals,
            "structure": structure,
            "verification": verification,
            "errors": errors,
        }
    )

    cleanup_writer_core_output(output, journal_path, args.keep_journals)
    return item


def should_verify(args: argparse.Namespace, warmup: bool, journal_path: Path) -> bool:
    return not args.skip_verify and not warmup and journal_path.exists()


def writer_core_measurement_item(data: dict[str, Any]) -> dict[str, Any]:
    return {
        "language": data["language"],
        "kind": "warmup" if data["warmup"] else "measurement",
        "repetition": data["repetition"],
        "command": data["command"],
        "returncode": data["result"].returncode,
        "stdout_tail": data["result"].stdout[-1000:],
        "stderr_tail": data["result"].stderr[-1000:],
        "driver": data["driver"],
        "process_time": data["stats"],
        "records": data["records"],
        "expected_records": data["expected_records"],
        "append_seconds": data["append_seconds"],
        "append_rows_per_second": data["append_rate"],
        "process_rows_per_second": process_rows_per_second(data),
        "journal_path": str(data["journal_path"]) if data["keep_journals"] else None,
        "journal_size_bytes": data["file_size"],
        "structure": data["structure"],
        "verify": data["verification"],
        "status": "PASS"
        if writer_core_measurement_passed(data)
        else "FAIL",
    }


def process_rows_per_second(data: dict[str, Any]) -> float | None:
    process_wall = data["process_wall"]
    return data["records"] / process_wall if process_wall > 0 else None


def writer_core_measurement_passed(data: dict[str, Any]) -> bool:
    verification = data["verification"]
    return (
        data["result"].returncode == 0
        and data["records"] == data["expected_records"]
        and not data["errors"]
        and data["structure"]["status"] == "PASS"
        and (verification is None or verification["returncode"] == 0)
    )


def cleanup_writer_core_output(output: Path, journal_path: Path, keep_journals: bool) -> None:
    if keep_journals:
        return
    output.unlink(missing_ok=True)
    if journal_path != output:
        journal_path.unlink(missing_ok=True)


def summarize(results: list[dict[str, Any]]) -> dict[str, Any]:
    summary: dict[str, Any] = {}
    for language in LANGUAGES:
        rows = passing_measurements(results, language)
        if not rows:
            continue
        summary[language] = summarize_language(rows)
    systemd_rate = summary.get("systemd", {}).get("append_rows_per_second_median")
    if systemd_rate:
        add_systemd_ratios(summary, systemd_rate)
    return summary


def passing_measurements(results: list[dict[str, Any]], language: str) -> list[dict[str, Any]]:
    return [
        row
        for row in results
        if row["language"] == language and row["kind"] == "measurement" and row["status"] == "PASS"
    ]


def summarize_language(rows: list[dict[str, Any]]) -> dict[str, Any]:
    append_rates = [float(row["append_rows_per_second"]) for row in rows]
    process_rates = [
        float(row["process_rows_per_second"])
        for row in rows
        if row["process_rows_per_second"] is not None
    ]
    sizes = [int(row["journal_size_bytes"]) for row in rows]
    summary = {
        "measurements": len(rows),
        "append_rows_per_second_min": min(append_rates),
        "append_rows_per_second_median": statistics.median(append_rates),
        "append_rows_per_second_max": max(append_rates),
        "process_rows_per_second_median": statistics.median(process_rates) if process_rates else None,
        "journal_size_bytes_median": statistics.median(sizes),
    }
    summary.update(driver_field_summary(rows))
    return summary


def driver_field_summary(rows: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "api_modes": sorted_driver_strings(rows, "api_mode"),
        "live_publication": sorted_driver_strings(rows, "live_publication"),
        "live_publish_every_entries": sorted_driver_ints(rows, "live_publish_every_entries", -1),
        "mmap_strategies": sorted_driver_strings(rows, "mmap_strategy"),
        "data_hash_table_buckets": sorted_driver_ints(rows, "data_hash_table_buckets", 0),
        "field_hash_table_buckets": sorted_driver_ints(rows, "field_hash_table_buckets", 0),
        "max_size_bytes": sorted_driver_ints(rows, "max_size_bytes", 0),
    }


def sorted_driver_strings(rows: list[dict[str, Any]], field: str) -> list[str]:
    return sorted({str(row["driver"].get(field, "unknown")) for row in rows})


def sorted_driver_ints(rows: list[dict[str, Any]], field: str, default: int) -> list[int]:
    return sorted({int(row["driver"].get(field, default) or 0) for row in rows})


def add_systemd_ratios(summary: dict[str, Any], systemd_rate: float) -> None:
    for item in summary.values():
        item["systemd_append_ratio_median"] = item["append_rows_per_second_median"] / systemd_rate


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


def resolve_live_publish_every_entries(
    mode: str,
    interval: int,
    explicit: int | None,
) -> int:
    if explicit is not None:
        if explicit < 0:
            raise ValueError("--live-publish-every-entries must be non-negative")
        return explicit
    if mode == "immediate":
        return 1
    if mode == "disabled":
        return 0
    if mode == "every-n":
        if interval <= 0:
            raise ValueError("--rust-live-publication-interval must be positive")
        return interval
    raise ValueError(f"invalid live publication mode: {mode}")


def parse_args() -> argparse.Namespace:
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
    parser.add_argument(
        "--api-mode",
        "--rust-api-mode",
        dest="api_mode",
        choices=("raw-payload", "structured-field"),
        default="raw-payload",
        help="Append API shape for SDK writers. systemd always uses raw full-payload iovecs.",
    )
    parser.add_argument("--rust-trusted-unique-payloads", action="store_true")
    parser.add_argument(
        "--live-publish-every-entries",
        dest="live_publish_every_entries",
        type=int,
        help="Explicit live-reader publication cadence for SDK writers; 1 is default, 0 disables explicit publication.",
    )
    parser.add_argument(
        "--rust-live-publish-every-entries",
        dest="live_publish_every_entries",
        type=int,
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--rust-live-publication",
        choices=("immediate", "disabled", "every-n"),
        default="immediate",
        help=argparse.SUPPRESS,
    )
    parser.add_argument("--rust-live-publication-interval", type=int, default=64, help=argparse.SUPPRESS)
    parser.add_argument(
        "--rust-mmap-strategy",
        choices=("windowed", "whole-file"),
        default="windowed",
    )
    parser.add_argument(
        "--rust-compare-api-modes",
        action="store_true",
        help="After benchmark runs, write the same Rust corpus through raw and structured APIs and require byte identity.",
    )
    return parser.parse_args()


def timestamp_id() -> str:
    now = datetime.now(timezone.utc)
    return (
        f"{now.year:04d}{now.month:02d}{now.day:02d}T"
        f"{now.hour:02d}{now.minute:02d}{now.second:02d}{now.microsecond:06d}Z"
    )


def writer_core_profile(args: argparse.Namespace, live_publish_every_entries: int) -> str:
    profile_parts = [
        f"{args.format}-none-fss-off",
        f"api-{args.api_mode}",
        f"live-every-{live_publish_every_entries}",
    ]
    if "rust" in args.languages:
        if args.rust_trusted_unique_payloads:
            profile_parts.append("trusted-unique")
        profile_parts.append(f"mmap-{args.rust_mmap_strategy}")
    return "-".join(profile_parts)


def build_all_tools(args: argparse.Namespace, env: dict[str, str]) -> dict[str, dict[str, Any]]:
    tools = {}
    for language in args.languages:
        base, metadata = build_tool(language, env)
        tools[language] = {"command": base, "metadata": metadata}
    return tools


def run_writer_core_measurements(
    args: argparse.Namespace,
    tools: dict[str, dict[str, Any]],
    output_dir: Path,
    live_publish_every_entries: int,
    env: dict[str, str],
) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []
    for language in args.languages:
        base = tools[language]["command"]
        results.extend(
            run_writer_core_language(args, language, base, output_dir, live_publish_every_entries, env)
        )
    return results


def run_writer_core_language(
    args: argparse.Namespace,
    language: str,
    base: list[str],
    output_dir: Path,
    live_publish_every_entries: int,
    env: dict[str, str],
) -> list[dict[str, Any]]:
    results = []
    for warmup in range(args.warmups):
        results.append(
            one_measurement(
                language,
                base,
                args,
                output_dir,
                warmup + 1,
                True,
                live_publish_every_entries,
                env,
            )
        )
    for repetition in range(args.repetitions):
        results.append(
            one_measurement(
                language,
                base,
                args,
                output_dir,
                repetition + 1,
                False,
                live_publish_every_entries,
                env,
            )
        )
    return results


def run_rust_api_mode_compare(
    args: argparse.Namespace,
    tools: dict[str, dict[str, Any]],
    output_dir: Path,
    live_publish_every_entries: int,
    env: dict[str, str],
) -> dict[str, Any] | None:
    if not args.rust_compare_api_modes:
        return None
    if "rust" not in args.languages:
        return {
            "kind": "rust-api-mode-byte-identity",
            "status": "FAIL",
            "errors": ["--rust-compare-api-modes requires --languages to include rust"],
        }
    return rust_api_mode_byte_identity(
        tools["rust"]["command"],
        args,
        output_dir=output_dir,
        live_publish_every_entries=live_publish_every_entries,
        env=env,
    )


def writer_core_report(
    args: argparse.Namespace,
    profile: str,
    output_dir: Path,
    env: dict[str, str],
    tools: dict[str, dict[str, Any]],
    results: list[dict[str, Any]],
    live_publish_every_entries: int,
    rust_api_mode_compare: dict[str, Any] | None,
) -> dict[str, Any]:
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
            "api_mode": args.api_mode,
            "rust_api_mode": args.api_mode,
            "rust_trusted_unique_payloads": args.rust_trusted_unique_payloads,
            "live_publish_every_entries": live_publish_every_entries,
            "rust_mmap_strategy": args.rust_mmap_strategy,
            "rust_compare_api_modes": args.rust_compare_api_modes,
            "hash_table_sizing": "systemd v260.1 formula: data=max(max_size*4/768/3,2047), field=1023",
            "append_timer_excludes": [
                "row generation",
                "writer creation",
                "final close/sync",
                "journal verification",
            ],
        },
        "environment": environment_report(env, output_dir),
        "tools": tools,
        "results": results,
        "rust_api_mode_byte_identity": rust_api_mode_compare,
        "summary": summarize(results),
    }
    failures = writer_core_failures(results, rust_api_mode_compare)
    report["status"] = "PASS" if not failures else "FAIL"
    report["failures"] = failures
    return report


def writer_core_failures(
    results: list[dict[str, Any]],
    rust_api_mode_compare: dict[str, Any] | None,
) -> list[dict[str, Any]]:
    measurement_failures = [
        row for row in results if row["kind"] == "measurement" and row["status"] != "PASS"
    ]
    compare_failures = (
        [rust_api_mode_compare]
        if rust_api_mode_compare is not None and rust_api_mode_compare["status"] != "PASS"
        else []
    )
    return [*measurement_failures, *driver_consistency_failures(results), *compare_failures]


def write_report(output_dir: Path, report: dict[str, Any]) -> Path:
    report_path = output_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True), encoding="utf-8")
    return report_path


def print_report_summary(report: dict[str, Any], report_path: Path) -> None:
    print(
        json.dumps(
            {"status": report["status"], "report": _display_report_path(report_path)},
            indent=2,
            sort_keys=True,
        )
    )


def _display_report_path(report_path: Path) -> str:
    try:
        return str(report_path.relative_to(ROOT))
    except ValueError:
        return str(report_path)


def main() -> int:
    args = parse_args()
    live_publish_every_entries = resolve_live_publish_every_entries(
        args.rust_live_publication,
        args.rust_live_publication_interval,
        args.live_publish_every_entries,
    )
    env = build_env()
    profile = writer_core_profile(args, live_publish_every_entries)
    out = args.output_dir / f"{profile}-{timestamp_id()}"
    out.mkdir(parents=True, exist_ok=True)

    tools = build_all_tools(args, env)
    results = run_writer_core_measurements(args, tools, out, live_publish_every_entries, env)
    rust_api_mode_compare = run_rust_api_mode_compare(
        args,
        tools,
        out,
        live_publish_every_entries,
        env,
    )
    report = writer_core_report(
        args,
        profile,
        out,
        env,
        tools,
        results,
        live_publish_every_entries,
        rust_api_mode_compare,
    )
    report_path = write_report(out, report)
    print_report_summary(report, report_path)
    failures = report["failures"]
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
