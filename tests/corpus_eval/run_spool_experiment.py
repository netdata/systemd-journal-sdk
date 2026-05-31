#!/usr/bin/env python3
"""Focused raw-reader and spool-writer corpus experiment."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from tests.corpus_eval.run_corpus_eval import (
    BIN_DIR,
    ROOT,
    discover_cases,
    parse_time_stats,
    run_env,
    timed_command_prefix,
    write_json,
)

SCHEMA = "systemd-journal-sdk-spool-experiment-v1"


def utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def run_json(
    cmd: list[str],
    *,
    cwd: Path,
    env: dict[str, str],
    stats_path: Path,
    timeout: int,
) -> tuple[Any, dict[str, Any]]:
    actual = [*timed_command_prefix(stats_path), *cmd]
    started = time.perf_counter()
    result = subprocess.run(
        actual,
        cwd=str(cwd),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    elapsed = time.perf_counter() - started
    stats = parse_time_stats(stats_path)
    stats.setdefault("process_wall_seconds", elapsed)
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
    parsed = json.loads(result.stdout)
    return parsed, stats


def run_quiet(
    cmd: list[str],
    *,
    cwd: Path,
    env: dict[str, str],
    stats_path: Path,
    timeout: int,
) -> dict[str, Any]:
    actual = [*timed_command_prefix(stats_path), *cmd]
    started = time.perf_counter()
    result = subprocess.run(
        actual,
        cwd=str(cwd),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    elapsed = time.perf_counter() - started
    stats = parse_time_stats(stats_path)
    stats.setdefault("process_wall_seconds", elapsed)
    return {
        "returncode": result.returncode,
        "stats": stats,
        "stdout_sha256": hashlib.sha256(result.stdout).hexdigest(),
        "stderr_sha256": hashlib.sha256(result.stderr).hexdigest(),
    }


def build_tools(env: dict[str, str], out: Path) -> dict[str, Path | str]:
    BIN_DIR.mkdir(parents=True, exist_ok=True)
    commands = [
        (
            "build rust experiment tools",
            [
                "cargo",
                "build",
                "--release",
                "-p",
                "corpus_experiment",
                "-p",
                "corpus_digest",
            ],
            ROOT / "rust",
        ),
        (
            "build go experiment tool",
            [
                "go",
                "build",
                "-o",
                str(BIN_DIR / "go-corpus-experiment"),
                "./internal/testcmd/corpus_experiment",
            ],
            ROOT / "go",
        ),
        (
            "build go digest tool",
            [
                "go",
                "build",
                "-o",
                str(BIN_DIR / "go-corpus-digest"),
                "./internal/testcmd/corpus_digest",
            ],
            ROOT / "go",
        ),
    ]
    results = []
    for label, cmd, cwd in commands:
        started = time.perf_counter()
        result = subprocess.run(
            cmd,
            cwd=str(cwd),
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=1800,
            check=False,
        )
        results.append(
            {
                "label": label,
                "returncode": result.returncode,
                "seconds": time.perf_counter() - started,
                "stdout_sha256": hashlib.sha256(result.stdout).hexdigest(),
                "stderr_sha256": hashlib.sha256(result.stderr).hexdigest(),
            }
        )
        if result.returncode != 0:
            write_json(out / "build-results.json", results)
            raise RuntimeError(f"{label} failed; see {out / 'build-results.json'}")
    write_json(out / "build-results.json", results)
    journalctl = shutil.which("journalctl")
    if journalctl is None:
        raise RuntimeError("journalctl is required for generated-file verification")
    return {
        "rust_experiment": ROOT / ".local" / "cargo-target" / "release" / "corpus_experiment",
        "rust_digest": ROOT / ".local" / "cargo-target" / "release" / "corpus_digest",
        "go_experiment": BIN_DIR / "go-corpus-experiment",
        "go_digest": BIN_DIR / "go-corpus-digest",
        "journalctl": journalctl,
    }


def raw_read(
    driver: str,
    path: Path,
    *,
    tools: dict[str, Path | str],
    env: dict[str, str],
    stats_dir: Path,
    timeout: int,
) -> dict[str, Any]:
    exe = tools[f"{driver}_experiment"]
    parsed, stats = run_json(
        [
            str(exe),
            "raw-read",
            "--input",
            str(path),
            "--output",
            "json",
        ],
        cwd=ROOT,
        env=env,
        stats_path=stats_dir / f"{driver}-raw-read.json",
        timeout=timeout,
    )
    if not isinstance(parsed, list) or len(parsed) != 1:
        raise RuntimeError(f"{driver} raw-read returned unexpected JSON")
    row = parsed[0]
    row["process_stats"] = stats
    return row


def digest(
    driver: str,
    path: Path,
    *,
    tools: dict[str, Path | str],
    env: dict[str, str],
    stats_dir: Path,
    timeout: int,
) -> dict[str, Any]:
    exe = tools[f"{driver}_digest"]
    parsed, stats = run_json(
        [str(exe), "--input", str(path)],
        cwd=ROOT,
        env=env,
        stats_path=stats_dir / f"{driver}-digest-{path.stem}.json",
        timeout=timeout,
    )
    parsed["process_stats"] = stats
    return parsed


def dump_spool(
    driver: str,
    input_path: Path,
    spool_path: Path,
    *,
    tools: dict[str, Path | str],
    env: dict[str, str],
    stats_dir: Path,
    timeout: int,
) -> dict[str, Any]:
    exe = tools[f"{driver}_experiment"]
    result = run_quiet(
        [
            str(exe),
            "dump-spool",
            "--input",
            str(input_path),
            "--output",
            str(spool_path),
        ],
        cwd=ROOT,
        env=env,
        stats_path=stats_dir / f"{driver}-dump-spool.json",
        timeout=timeout,
    )
    payload = spool_path.read_bytes()
    return {
        "driver": driver,
        "status": "ok" if result["returncode"] == 0 else "failed",
        "bytes": len(payload),
        "sha256": hashlib.sha256(payload).hexdigest(),
        "process_stats": result["stats"],
        "returncode": result["returncode"],
        "stdout_sha256": result["stdout_sha256"],
        "stderr_sha256": result["stderr_sha256"],
    }


def write_spool(
    driver: str,
    spool_path: Path,
    output_path: Path,
    *,
    tools: dict[str, Path | str],
    env: dict[str, str],
    stats_dir: Path,
    timeout: int,
    fmt: str,
    compression: str,
    fss: bool,
    final_state: str,
    live_publish_every_entries: int,
    max_size_bytes: int,
) -> dict[str, Any]:
    exe = tools[f"{driver}_experiment"]
    cmd = [
        str(exe),
        "write-spool",
        "--input",
        str(spool_path),
        "--output",
        str(output_path),
        "--format",
        fmt,
        "--compression",
        compression,
        "--final-state",
        final_state,
        "--live-publish-every-entries",
        str(live_publish_every_entries),
        "--max-size-bytes",
        str(max_size_bytes),
    ]
    if fss:
        cmd.append("--fss")
    parsed, stats = run_json(
        cmd,
        cwd=ROOT,
        env=env,
        stats_path=stats_dir / f"{driver}-write-spool.json",
        timeout=timeout,
    )
    parsed["process_stats"] = stats
    return parsed


def verify_generated(
    path: Path,
    *,
    tools: dict[str, Path | str],
    env: dict[str, str],
    stats_dir: Path,
    timeout: int,
) -> dict[str, Any]:
    return run_quiet(
        [str(tools["journalctl"]), "--verify", "--file", str(path)],
        cwd=ROOT,
        env=env,
        stats_path=stats_dir / f"verify-{path.stem}.json",
        timeout=timeout,
    )


def compare_raw(rows: dict[str, dict[str, Any]]) -> dict[str, Any]:
    rust = rows.get("rust", {})
    go = rows.get("go", {})
    count_keys = [
        "entries",
        "payloads",
        "payload_bytes",
        "binary_payloads",
        "payloads_without_equals",
        "largest_payload_bytes",
    ]
    mismatches = [
        key for key in count_keys if rust.get(key) != go.get(key)
    ]
    hash_match = rust.get("hash") == go.get("hash")
    return {
        "hash_match": hash_match,
        "count_mismatches": mismatches,
        "ok": hash_match and not mismatches,
    }


def run_case(
    case: Any,
    *,
    tools: dict[str, Path | str],
    env: dict[str, str],
    out: Path,
    args: argparse.Namespace,
) -> dict[str, Any]:
    case_dir = out / "work" / case.file_id
    stats_dir = out / "stats" / case.file_id
    case_dir.mkdir(parents=True, exist_ok=True)
    stats_dir.mkdir(parents=True, exist_ok=True)

    raw_original = {
        driver: raw_read(
            driver,
            case.path,
            tools=tools,
            env=env,
            stats_dir=stats_dir,
            timeout=args.timeout,
        )
        for driver in ("rust", "go")
    }
    spool = {
        driver: dump_spool(
            driver,
            case.path,
            case_dir / f"{driver}.spool",
            tools=tools,
            env=env,
            stats_dir=stats_dir,
            timeout=args.timeout,
        )
        for driver in ("rust", "go")
    }
    original_digest = {
        driver: digest(
            driver,
            case.path,
            tools=tools,
            env=env,
            stats_dir=stats_dir,
            timeout=args.timeout,
        )
        for driver in ("rust", "go")
    }
    writers: dict[str, Any] = {}
    for driver in ("rust", "go"):
        generated = case_dir / f"{driver}-{args.format}-{args.compression}.journal"
        writers[driver] = {
            "write": write_spool(
                driver,
                case_dir / "rust.spool",
                generated,
                tools=tools,
                env=env,
                stats_dir=stats_dir,
                timeout=args.timeout,
                fmt=args.format,
                compression=args.compression,
                fss=args.fss,
                final_state=args.final_state,
                live_publish_every_entries=args.live_publish_every_entries,
                max_size_bytes=args.max_size_bytes,
            ),
            "verify": verify_generated(
                generated,
                tools=tools,
                env=env,
                stats_dir=stats_dir,
                timeout=args.timeout,
            ),
            "raw_read": {
                reader: raw_read(
                    reader,
                    generated,
                    tools=tools,
                    env=env,
                    stats_dir=stats_dir / f"{driver}-generated",
                    timeout=args.timeout,
                )
                for reader in ("rust", "go")
            },
            "digest": {
                reader: digest(
                    reader,
                    generated,
                    tools=tools,
                    env=env,
                    stats_dir=stats_dir / f"{driver}-generated",
                    timeout=args.timeout,
                )
                for reader in ("rust", "go")
            },
        }
        if not args.keep_artifacts:
            generated.unlink(missing_ok=True)

    if not args.keep_artifacts:
        for path in case_dir.glob("*.spool"):
            path.unlink(missing_ok=True)

    spool_byte_identical = spool["rust"]["sha256"] == spool["go"]["sha256"]
    result = {
        "file_id": case.file_id,
        "input_bytes": case.size,
        "raw_original": raw_original,
        "raw_original_agreement": compare_raw(raw_original),
        "spool": spool,
        "spool_byte_identical": spool_byte_identical,
        "original_digest": original_digest,
        "writers": writers,
    }
    discrepancies = []
    if not result["raw_original_agreement"]["ok"]:
        discrepancies.append("rust_go_original_raw_read_mismatch")
    if not spool_byte_identical:
        discrepancies.append("rust_go_spool_dump_mismatch")
    for driver, writer in writers.items():
        if writer["verify"]["returncode"] != 0:
            discrepancies.append(f"{driver}_generated_stock_verify_failed")
        generated_digest = writer["digest"]["rust"]
        if (
            generated_digest.get("logical_digest")
            != original_digest["rust"].get("logical_digest")
        ):
            discrepancies.append(f"{driver}_generated_logical_digest_mismatch")
    result["discrepancies"] = discrepancies
    return result


def write_markdown(report: dict[str, Any], path: Path) -> None:
    lines = [
        "# Spool Experiment Report",
        "",
        f"- Schema: `{report['schema']}`",
        f"- Generated: `{report['generated_at']}`",
        f"- Files: `{len(report['cases'])}`",
        f"- Discrepancies: `{len(report['discrepancies'])}`",
        "",
        "## Raw Reader",
        "",
        "| file_id | input MiB | rust entries/s | go entries/s | hash match | counts ok |",
        "|---|---:|---:|---:|---|---|",
    ]
    for case in report["cases"]:
        rust = case["raw_original"]["rust"]
        go = case["raw_original"]["go"]
        agree = case["raw_original_agreement"]
        lines.append(
            "| {file_id} | {mib:.2f} | {rust_eps:,.0f} | {go_eps:,.0f} | {hash_match} | {counts_ok} |".format(
                file_id=case["file_id"],
                mib=case["input_bytes"] / 1024 / 1024,
                rust_eps=float(rust.get("entries_per_second") or 0),
                go_eps=float(go.get("entries_per_second") or 0),
                hash_match="yes" if agree["hash_match"] else "no",
                counts_ok="yes" if not agree["count_mismatches"] else "no",
            )
        )
    lines.extend(
        [
            "",
            "## Writer",
            "",
            "| file_id | writer | append entries/s | total entries/s | generated MiB | stock verify | logical digest |",
            "|---|---|---:|---:|---:|---|---|",
        ]
    )
    for case in report["cases"]:
        original_digest = case["original_digest"]["rust"].get("logical_digest")
        for driver, writer in case["writers"].items():
            write = writer["write"]
            generated_digest = writer["digest"]["rust"].get("logical_digest")
            lines.append(
                "| {file_id} | {driver} | {append_eps:,.0f} | {total_eps:,.0f} | {mib:.2f} | {verify} | {digest} |".format(
                    file_id=case["file_id"],
                    driver=driver,
                    append_eps=float(write.get("append_entries_per_second") or 0),
                    total_eps=float(write.get("total_entries_per_second") or 0),
                    mib=float(write.get("generated_bytes") or 0) / 1024 / 1024,
                    verify="ok" if writer["verify"]["returncode"] == 0 else "failed",
                    digest="ok" if generated_digest == original_digest else "mismatch",
                )
            )
    lines.extend(["", "## Discrepancies", ""])
    if report["discrepancies"]:
        lines.extend(f"- `{item}`" for item in report["discrepancies"])
    else:
        lines.append("- none")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--input", type=Path)
    source.add_argument("--root", type=Path)
    parser.add_argument("--max-files", type=int, default=1)
    parser.add_argument("--out", type=Path, default=ROOT / ".local" / "corpus-eval" / "spool-experiment")
    parser.add_argument("--timeout", type=int, default=1800)
    parser.add_argument("--format", choices=("regular", "compact"), default="compact")
    parser.add_argument("--compression", choices=("none", "zstd", "xz", "lz4"), default="none")
    parser.add_argument("--fss", action="store_true")
    parser.add_argument("--final-state", choices=("online", "offline"), default="offline")
    parser.add_argument("--live-publish-every-entries", type=int, default=64)
    parser.add_argument("--max-size-bytes", type=int, default=128 * 1024 * 1024)
    parser.add_argument("--keep-artifacts", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    env = run_env()
    args.out.mkdir(parents=True, exist_ok=True)
    roots = [args.input] if args.input else [args.root]
    cases = discover_cases([root for root in roots if root is not None], max_files=args.max_files)
    if not cases:
        raise SystemExit("no journal files found")
    tools = build_tools(env, args.out)
    case_reports = [
        run_case(case, tools=tools, env=env, out=args.out, args=args)
        for case in cases
    ]
    discrepancies = [
        f"{case['file_id']}:{item}"
        for case in case_reports
        for item in case["discrepancies"]
    ]
    report = {
        "schema": SCHEMA,
        "generated_at": utc_now(),
        "options": {
            "format": args.format,
            "compression": args.compression,
            "fss": args.fss,
            "final_state": args.final_state,
            "live_publish_every_entries": args.live_publish_every_entries,
            "max_size_bytes": args.max_size_bytes,
            "keep_artifacts": args.keep_artifacts,
        },
        "cases": case_reports,
        "discrepancies": discrepancies,
    }
    report_json = args.out / "report.json"
    report_md = args.out / "report.md"
    write_json(report_json, report)
    write_markdown(report, report_md)
    print(json.dumps({"report_json": str(report_json), "report_md": str(report_md), "discrepancies": len(discrepancies)}, sort_keys=True))
    return 0 if not discrepancies else 1


if __name__ == "__main__":
    raise SystemExit(main())
