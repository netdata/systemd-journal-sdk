#!/usr/bin/env python3
"""Run writer ingestion benchmarks.

The first supported profile is the production baseline requested for SOW-0009:
compact journal files, DATA compression disabled, FSS disabled, one writer
process, and one final writer sync/close. Runtime artifacts stay under
`.local/benchmarks/`.
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
DATASETS = ROOT / "tests" / "datasets"
PERF_MANIFEST = DATASETS / "performance" / "manifest.json"
DEFAULT_CORPUS = ROOT / ".local" / "datasets" / "performance-corpus.jsonl"
DEFAULT_OUT = ROOT / ".local" / "benchmarks" / "writers"
BIN_DIR = ROOT / ".local" / "benchmarks" / "bin"
SEQNUM_ID = "22222222222222222222222222222222"
DEFAULT_MAX_SIZE_BYTES = 128 * 1024 * 1024
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


def sha256_file(path: Path) -> tuple[str, int]:
    digest = hashlib.sha256()
    count = 0
    with path.open("rb") as handle:
        for line in handle:
            digest.update(line)
            count += 1
    return digest.hexdigest(), count


def ensure_performance_corpus(path: Path, rows: int, regenerate: bool) -> dict[str, Any]:
    manifest = json.loads(PERF_MANIFEST.read_text(encoding="utf-8"))
    expected_hash = str(manifest["stream_sha256"]) if rows == int(manifest["record_count"]) else None
    if regenerate or not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        require_ok(
            run(
                [
                    sys.executable,
                    str(DATASETS / "generate.py"),
                    "performance",
                    "--output",
                    str(path),
                    "--rows",
                    str(rows),
                ],
                timeout=600,
            ),
            "generate performance corpus",
        )

    actual_hash, actual_rows = sha256_file(path)
    if actual_rows != rows or (expected_hash is not None and actual_hash != expected_hash):
        path.unlink(missing_ok=True)
        require_ok(
            run(
                [
                    sys.executable,
                    str(DATASETS / "generate.py"),
                    "performance",
                    "--output",
                    str(path),
                    "--rows",
                    str(rows),
                ],
                timeout=600,
            ),
            "regenerate performance corpus",
        )
        actual_hash, actual_rows = sha256_file(path)

    return {
        "path": str(path),
        "rows": actual_rows,
        "sha256": actual_hash,
        "manifest_sha256": expected_hash,
        "fields_per_row": manifest["fields_per_row"],
        "cardinality_profile": manifest["cardinality_profile"],
    }


def build_tool(language: str, env: dict[str, str]) -> tuple[list[str], dict[str, Any]]:
    BIN_DIR.mkdir(parents=True, exist_ok=True)
    if language == "python":
        return [sys.executable, str(ROOT / "python" / "cmd" / "dataset_ingester.py")], {}
    if language == "node":
        return ["node", str(ROOT / "node" / "cmd" / "dataset_ingester.js")], {}
    if language == "go":
        output = BIN_DIR / "go-dataset-ingester"
        require_ok(
            run(
                ["go", "build", "-o", str(output), "./internal/testcmd/dataset_ingester"],
                cwd=ROOT / "go",
                env=env,
                timeout=300,
            ),
            "build go dataset ingester",
        )
        return [str(output)], {"build": "go build"}
    if language == "rust":
        require_ok(
            run(
                ["cargo", "build", "--release", "-p", "dataset_ingester"],
                cwd=ROOT / "rust",
                env=env,
                timeout=600,
            ),
            "build rust dataset ingester",
        )
        return [str(ROOT / ".local" / "cargo-target" / "release" / "dataset_ingester")], {
            "build": "cargo build --release -p dataset_ingester"
        }
    if language == "systemd":
        result = run([str(DATASETS / "ingesters" / "systemd" / "build.sh")], env=env, timeout=1800)
        require_ok(result, "build systemd dataset ingester")
        binary = result.stdout.strip().splitlines()[-1]
        return [binary], {"build_stdout_tail": result.stdout[-1000:]}
    raise ValueError(language)


def ingester_command(
    base: list[str],
    *,
    dataset: Path,
    output: Path,
    final_state: str,
    compact: bool,
    language: str,
    max_size_bytes: int,
) -> list[str]:
    cmd = [
        *base,
        "--dataset",
        str(dataset),
        "--output",
        str(output),
        "--final-state",
        final_state,
    ]
    if compact:
        cmd.append("--compact")
    if language == "systemd":
        cmd += ["--max-size-bytes", str(max_size_bytes)]
    return cmd


def final_journal_path(output: Path, final_state: str, first_realtime: int) -> Path:
    if final_state != "archived":
        return output
    prefix = output.name[: -len(".journal")] if output.name.endswith(".journal") else output.name
    return output.with_name(f"{prefix}@{SEQNUM_ID}-0000000000000001-{first_realtime:016x}.journal")


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
                '{"wall_seconds":%e,"user_seconds":%U,"system_seconds":%S,"max_rss_kb":%M}',
                "-o",
                str(stats_path),
                *cmd,
            ],
            env=env,
            timeout=1800,
        )

    started = time.perf_counter()
    result = run(cmd, env=env, timeout=1800)
    stats_path.write_text(json.dumps({"wall_seconds": time.perf_counter() - started}), encoding="utf-8")
    return result


def parse_ingester_result(stdout: str) -> dict[str, Any]:
    for line in reversed(stdout.splitlines()):
        line = line.strip()
        if line.startswith("{") and line.endswith("}"):
            return json.loads(line)
    return {"errors": ["missing JSON result"], "records": 0}


def verify_journal(path: Path) -> dict[str, Any]:
    if shutil.which("journalctl") is None:
        return {"returncode": 127, "stderr": "journalctl not found", "stdout": ""}
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
    args: argparse.Namespace,
    dataset: Path,
    output_dir: Path,
    repetition: int,
    warmup: bool,
    env: dict[str, str],
) -> dict[str, Any]:
    label = "warmup" if warmup else f"rep-{repetition}"
    run_dir, output, actual = prepare_measurement_paths(
        output_dir,
        language,
        label,
        args.final_state,
    )
    cmd = ingester_command(
        base,
        dataset=dataset,
        output=output,
        final_state=args.final_state,
        compact=not args.regular,
        language=language,
        max_size_bytes=args.max_size_bytes,
    )
    stats_path = run_dir / "time.json"
    result = timed_run(cmd, stats_path, env)
    stats = parse_time_stats(stats_path)
    ingester = parse_ingester_result(result.stdout)
    journal_path = actual if actual.exists() else output
    file_size = journal_path.stat().st_size if journal_path.exists() else 0
    wall = float(stats.get("wall_seconds", 0.0) or 0.0)
    records = int(ingester.get("records", 0) or 0)
    structure = quick_header_check(journal_path, compact=not args.regular) if journal_path.exists() else {
        "status": "FAIL",
        "error": "journal file missing",
    }
    verification = verify_journal(journal_path) if should_verify(args, warmup, journal_path) else None

    item = writer_measurement_item(
        {
            "language": language,
            "repetition": repetition,
            "warmup": warmup,
            "command": cmd,
            "result": result,
            "ingester": ingester,
            "stats": stats,
            "records": records,
            "expected_records": args.rows,
            "wall": wall,
            "journal_path": journal_path,
            "file_size": file_size,
            "keep_journals": args.keep_journals,
            "structure": structure,
            "verification": verification,
        }
    )

    if not args.keep_journals:
        cleanup_journal_paths(output, actual)
    return item


def should_verify(args: argparse.Namespace, warmup: bool, journal_path: Path) -> bool:
    return not args.skip_verify and not warmup and journal_path.exists()


def prepare_measurement_paths(
    output_dir: Path,
    language: str,
    label: str,
    final_state: str,
) -> tuple[Path, Path, Path]:
    run_dir = output_dir / language / label
    run_dir.mkdir(parents=True, exist_ok=True)
    output = run_dir / "output.journal"
    actual = final_journal_path(output, final_state, 1_700_000_000_000_000)
    cleanup_journal_paths(output, actual)
    return run_dir, output, actual


def writer_measurement_item(data: dict[str, Any]) -> dict[str, Any]:
    return {
        "language": data["language"],
        "kind": "warmup" if data["warmup"] else "measurement",
        "repetition": data["repetition"],
        "command": data["command"],
        "returncode": data["result"].returncode,
        "stdout_tail": data["result"].stdout[-1000:],
        "stderr_tail": data["result"].stderr[-1000:],
        "ingester": data["ingester"],
        "time": data["stats"],
        "records": data["records"],
        "expected_records": data["expected_records"],
        "rows_per_second": writer_rows_per_second(data),
        "bytes_per_second": writer_bytes_per_second(data),
        "journal_path": str(data["journal_path"]) if data["keep_journals"] else None,
        "journal_size_bytes": data["file_size"],
        "structure": data["structure"],
        "verify": data["verification"],
        "status": "PASS"
        if writer_measurement_passed(data)
        else "FAIL",
    }


def writer_rows_per_second(data: dict[str, Any]) -> float | None:
    return data["records"] / data["wall"] if data["wall"] > 0 else None


def writer_bytes_per_second(data: dict[str, Any]) -> float | None:
    return data["file_size"] / data["wall"] if data["wall"] > 0 else None


def writer_measurement_passed(data: dict[str, Any]) -> bool:
    verification = data["verification"]
    return (
        data["result"].returncode == 0
        and data["records"] == data["expected_records"]
        and data["structure"]["status"] == "PASS"
        and (verification is None or verification["returncode"] == 0)
    )


def cleanup_journal_paths(*paths: Path) -> None:
    for path in set(paths):
        path.unlink(missing_ok=True)


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
        rates = [float(r["rows_per_second"]) for r in rows if r["rows_per_second"] is not None]
        sizes = [int(r["journal_size_bytes"]) for r in rows]
        summary[language] = {
            "measurements": len(rows),
            "rows_per_second_min": min(rates),
            "rows_per_second_median": statistics.median(rates),
            "rows_per_second_max": max(rates),
            "journal_size_bytes_median": statistics.median(sizes),
        }
    systemd_rate = summary.get("systemd", {}).get("rows_per_second_median")
    if systemd_rate:
        for item in summary.values():
            item["systemd_ratio_median"] = item["rows_per_second_median"] / systemd_rate
    return summary


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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--languages", nargs="+", choices=LANGUAGES, default=list(LANGUAGES))
    parser.add_argument("--rows", type=int, default=100_000)
    parser.add_argument("--repetitions", type=int, default=3)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--dataset", type=Path, default=DEFAULT_CORPUS)
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--final-state", choices=("online", "offline", "archived"), default="online")
    parser.add_argument("--regular", action="store_true", help="benchmark regular format instead of compact")
    parser.add_argument("--regenerate-dataset", action="store_true")
    parser.add_argument("--skip-verify", action="store_true")
    parser.add_argument("--keep-journals", action="store_true")
    parser.add_argument("--max-size-bytes", type=int, default=DEFAULT_MAX_SIZE_BYTES)
    return parser.parse_args()


def compact_timestamp_id() -> str:
    now = datetime.now(timezone.utc)
    return (
        f"{now.year:04d}{now.month:02d}{now.day:02d}T"
        f"{now.hour:02d}{now.minute:02d}{now.second:02d}Z"
    )


def writer_profile(args: argparse.Namespace) -> str:
    return "regular-none-fss-off" if args.regular else "compact-none-fss-off"


def build_all_tools(args: argparse.Namespace, env: dict[str, str]) -> dict[str, dict[str, Any]]:
    tools = {}
    for language in args.languages:
        base, metadata = build_tool(language, env)
        tools[language] = {"command": base, "metadata": metadata}
    return tools


def run_writer_measurements(
    args: argparse.Namespace,
    tools: dict[str, dict[str, Any]],
    dataset: Path,
    output_dir: Path,
    env: dict[str, str],
) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []
    for language in args.languages:
        base = tools[language]["command"]
        results.extend(run_language_measurements(args, language, base, dataset, output_dir, env))
    return results


def run_language_measurements(
    args: argparse.Namespace,
    language: str,
    base: list[str],
    dataset: Path,
    output_dir: Path,
    env: dict[str, str],
) -> list[dict[str, Any]]:
    results = []
    for warmup in range(args.warmups):
        results.append(
            one_measurement(language, base, args, dataset, output_dir, warmup + 1, True, env)
        )
    for repetition in range(args.repetitions):
        results.append(
            one_measurement(language, base, args, dataset, output_dir, repetition + 1, False, env)
        )
    return results


def writer_report(
    args: argparse.Namespace,
    profile: str,
    dataset: dict[str, Any],
    output_dir: Path,
    env: dict[str, str],
    tools: dict[str, dict[str, Any]],
    results: list[dict[str, Any]],
) -> dict[str, Any]:
    report = {
        "benchmark": "writer-ingestion",
        "profile": profile,
        "parameters": {
            "compact": not args.regular,
            "compression": "none",
            "fss": False,
            "final_state": args.final_state,
            "sync_policy": "one writer sync plus close at end of ingestion",
            "rows": args.rows,
            "repetitions": args.repetitions,
            "warmups": args.warmups,
            "languages": args.languages,
            "keep_journals": args.keep_journals,
        },
        "dataset": dataset,
        "environment": environment_report(env, output_dir),
        "tools": tools,
        "results": results,
        "summary": summarize(results),
    }
    failures = [r for r in results if r["kind"] == "measurement" and r["status"] != "PASS"]
    report["status"] = "PASS" if not failures else "FAIL"
    report["failures"] = failures
    return report


def write_report(output_dir: Path, report: dict[str, Any]) -> Path:
    report_path = output_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True), encoding="utf-8")
    return report_path


def print_report_summary(report: dict[str, Any], report_path: Path) -> None:
    print(
        json.dumps(
            {"status": report["status"], "report": str(report_path), "summary": report["summary"]},
            indent=2,
            sort_keys=True,
        )
    )


def main() -> int:
    args = parse_args()
    env = build_env()
    profile = writer_profile(args)
    out = args.output_dir / f"{profile}-{compact_timestamp_id()}"
    out.mkdir(parents=True, exist_ok=True)

    dataset = ensure_performance_corpus(args.dataset, args.rows, args.regenerate_dataset)
    dataset_path = Path(str(dataset["path"]))
    tools = build_all_tools(args, env)
    results = run_writer_measurements(args, tools, dataset_path, out, env)
    report = writer_report(args, profile, dataset, out, env, tools, results)
    report_path = write_report(out, report)
    print_report_summary(report, report_path)
    failures = report["failures"]
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
