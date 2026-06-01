#!/usr/bin/env python3
"""Benchmark the Go SOW-0074 baseline and optimized explorer tools."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
QUERY_DIR = REPO_ROOT / "tests" / "explorer_query" / "queries" / "performance"
WORK_ROOT = REPO_ROOT / ".local" / "explorer-query" / "benchmarks"
DATASET = REPO_ROOT / ".local" / "datasets" / "explorer-query-performance.jsonl"
BIN_ROOT = REPO_ROOT / ".local" / "explorer-query" / "go-bin"


def run(cmd: list[str], stdout: Path | None = None) -> None:
    kwargs = {"cwd": REPO_ROOT, "check": True}
    if stdout is None:
        subprocess.run(cmd, **kwargs)
        return
    stdout.parent.mkdir(parents=True, exist_ok=True)
    with stdout.open("w", encoding="utf-8") as stream:
        subprocess.run(cmd, stdout=stream, **kwargs)


def generate_dataset(rows: int) -> None:
    run(
        [
            "python3",
            "tests/datasets/generate.py",
            "performance",
            "--rows",
            str(rows),
            "--output",
            str(DATASET.relative_to(REPO_ROOT)),
        ]
    )


def build_tools() -> dict[str, Path]:
    BIN_ROOT.mkdir(parents=True, exist_ok=True)
    tools = {
        "dataset_ingester": BIN_ROOT / "dataset_ingester",
        "explorer_query_baseline": BIN_ROOT / "explorer_query_baseline",
        "explorer_query_optimized": BIN_ROOT / "explorer_query_optimized",
    }
    for name, output in tools.items():
        run(
            [
                "go",
                "-C",
                "go",
                "build",
                "-o",
                str(output),
                f"./internal/testcmd/{name}",
            ]
        )
    return tools


def ingest_fixture(work_dir: Path, tools: dict[str, Path], compression: str, compact: bool) -> Path:
    journal = work_dir / "performance.journal"
    cmd = [
        str(tools["dataset_ingester"]),
        "--dataset",
        str(DATASET),
        "--output",
        str(journal),
        "--final-state",
        "offline",
        "--compression",
        compression,
    ]
    if compact:
        cmd.append("--compact")
    run(cmd, work_dir / "ingest.json")
    return journal


def run_tool(tool: Path, journal: Path, query: Path, output: Path) -> None:
    run(
        [
            str(tool),
            "--input",
            str(journal),
            "--query",
            str(query),
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
    parser.add_argument("--rows", type=int, default=200_000)
    parser.add_argument("--compression", choices=["none", "zstd"], default="none")
    parser.add_argument("--compact", action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument("--keep-going", action="store_true")
    args = parser.parse_args()

    variant = f"go-{args.rows}-rows-{args.compression}{'-compact' if args.compact else ''}"
    work_dir = WORK_ROOT / variant
    work_dir.mkdir(parents=True, exist_ok=True)

    generate_dataset(args.rows)
    tools = build_tools()
    journal = ingest_fixture(work_dir, tools, args.compression, args.compact)

    rows = []
    failures = []
    for query in sorted(QUERY_DIR.glob("*.json")):
        baseline_path = work_dir / f"{query.stem}.baseline.json"
        optimized_path = work_dir / f"{query.stem}.optimized.json"
        run_tool(tools["explorer_query_baseline"], journal, query, baseline_path)
        run_tool(tools["explorer_query_optimized"], journal, query, optimized_path)
        baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
        optimized = json.loads(optimized_path.read_text(encoding="utf-8"))
        baseline_seconds = float(baseline["elapsed_seconds"])
        optimized_seconds = float(optimized["elapsed_seconds"])
        equivalent = comparable(baseline) == comparable(optimized)
        if not equivalent:
            failures.append(query.name)
            if not args.keep_going:
                break
        row = {
            "query": query.name,
            "equivalent": equivalent,
            "baseline_seconds": baseline_seconds,
            "optimized_seconds": optimized_seconds,
            "speedup": baseline_seconds / optimized_seconds
            if optimized_seconds > 0
            else None,
            "baseline_counters": baseline["counters"],
            "optimized_counters": optimized["counters"],
        }
        rows.append(row)
        print(json.dumps(row, sort_keys=True))

    summary = {
        "schema": "systemd-journal-sdk-explorer-go-benchmark-v1",
        "variant": variant,
        "rows": args.rows,
        "compression": args.compression,
        "compact": args.compact,
        "journal": str(journal.relative_to(REPO_ROOT)),
        "queries": rows,
        "failures": failures,
    }
    (work_dir / "summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        json.dumps(
            {
                "status": "fail" if failures else "pass",
                "summary": str((work_dir / "summary.json").relative_to(REPO_ROOT)),
            },
            sort_keys=True,
        )
    )
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
