#!/usr/bin/env python3
"""Run the Rust SOW-0074 explorer query comparison."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
QUERY_ROOT = REPO_ROOT / "tests" / "explorer_query" / "queries"
WORK_ROOT = REPO_ROOT / ".local" / "explorer-query"


def run(cmd: list[str], stdout: Path | None = None) -> None:
    kwargs = {"cwd": REPO_ROOT, "check": True}
    if stdout is not None:
        stdout.parent.mkdir(parents=True, exist_ok=True)
        with stdout.open("w", encoding="utf-8") as stream:
            subprocess.run(cmd, stdout=stream, **kwargs)
        return
    subprocess.run(cmd, **kwargs)


def run_ingester(journal_path: Path, compression: str, compact: bool, output: Path) -> None:
    cmd = [
        "cargo",
        "run",
        "-q",
        "-p",
        "dataset_ingester",
        "--manifest-path",
        "rust/Cargo.toml",
        "--",
        "--dataset",
        "tests/datasets/correctness/corpus.jsonl",
        "--output",
        str(journal_path.relative_to(REPO_ROOT)),
        "--final-state",
        "offline",
        "--compression",
        compression,
    ]
    if compact:
        cmd.append("--compact")
    run(cmd, output)


def ensure_file_fixture(
    work_dir: Path, journal_path: Path, compression: str, compact: bool
) -> Path:
    work_dir.mkdir(parents=True, exist_ok=True)
    run_ingester(journal_path, compression, compact, work_dir / "ingest.json")
    return journal_path


def ensure_directory_fixture(work_dir: Path) -> Path:
    directory = work_dir / "journals"
    directory.mkdir(parents=True, exist_ok=True)
    run_ingester(
        directory / "system.journal",
        "none",
        False,
        work_dir / "ingest-regular.json",
    )
    run_ingester(
        directory / "user.journal",
        "zstd",
        True,
        work_dir / "ingest-compact-zstd.json",
    )
    return directory


def run_tool(package: str, input_path: Path, query: Path, output: Path, surface: str) -> None:
    run(
        [
            "cargo",
            "run",
            "-q",
            "-p",
            package,
            "--manifest-path",
            "rust/Cargo.toml",
            "--",
            "--input",
            str(input_path.relative_to(REPO_ROOT)),
            "--query",
            str(query.relative_to(REPO_ROOT)),
            "--surface",
            surface,
        ],
        output,
    )


def comparable(report: dict) -> dict:
    return {
        "rows": report["rows"],
        "facets": report["facets"],
        "unique_values": report["unique_values"],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--suite", choices=["smoke", "full"], default="smoke")
    parser.add_argument("--surface", choices=["file", "directory"], default="file")
    parser.add_argument("--compression", choices=["none", "zstd"], default="none")
    parser.add_argument("--compact", action="store_true")
    parser.add_argument("--keep-going", action="store_true")
    args = parser.parse_args()

    variant = f"{args.suite}-{args.surface}"
    if args.surface == "file":
        variant += f"-{args.compression}{'-compact' if args.compact else ''}"
    else:
        variant += "-mixed"
    work_dir = WORK_ROOT / variant
    journal_path = work_dir / "correctness.journal"
    query_dir = QUERY_ROOT / args.suite
    input_path = (
        ensure_file_fixture(work_dir, journal_path, args.compression, args.compact)
        if args.surface == "file"
        else ensure_directory_fixture(work_dir)
    )
    failures: list[str] = []
    for query in sorted(query_dir.glob("*.json")):
        baseline_path = work_dir / f"{query.stem}.baseline.json"
        optimized_path = work_dir / f"{query.stem}.optimized.json"
        run_tool("explorer_query_baseline", input_path, query, baseline_path, args.surface)
        run_tool("explorer_query_optimized", input_path, query, optimized_path, args.surface)
        baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
        optimized = json.loads(optimized_path.read_text(encoding="utf-8"))
        failure_reasons = []
        if comparable(baseline) != comparable(optimized):
            failure_reasons.append("logical-output-mismatch")
        has_compressed_payloads = args.compression != "none" or args.surface == "directory"
        if query.stem == "compressed-irrelevant-skip" and has_compressed_payloads:
            if optimized["counters"].get("payloads_decompressed", 0) != 0:
                failure_reasons.append("irrelevant-compressed-payload-decompressed")
        if query.stem == "compressed-selected-facet" and has_compressed_payloads:
            if optimized["counters"].get("payloads_decompressed", 0) == 0:
                failure_reasons.append("selected-compressed-facet-not-decompressed")
        if query.stem in {"topn-no-filter", "topn-no-facet", "filter-equal-facet"}:
            if optimized["counters"].get("candidate_data_refs_visited", 0) != 0:
                failure_reasons.append("fast-path-visited-candidate-data-refs")
        if failure_reasons:
            failures.append(query.name)
            if not args.keep_going:
                break
        print(
            json.dumps(
                {
                    "query": query.name,
                    "status": "fail" if failure_reasons else "pass",
                    "failure_reasons": failure_reasons,
                    "baseline_counters": baseline["counters"],
                    "optimized_counters": optimized["counters"],
                },
                sort_keys=True,
            )
        )

    if failures:
        print(
            json.dumps(
                {"status": "fail", "variant": variant, "failures": failures},
                sort_keys=True,
            )
        )
        return 1
    print(json.dumps({"status": "pass", "variant": variant}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
