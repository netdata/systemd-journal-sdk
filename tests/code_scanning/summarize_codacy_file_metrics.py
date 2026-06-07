#!/usr/bin/env python3
"""Summarize Codacy file metrics without storing API credentials."""

from __future__ import annotations

import argparse
import collections
import csv
import itertools
import json
import re
from pathlib import Path
from typing import Any


TEST_OR_HARNESS_RE = re.compile(
    r"(^|/)internal/testcmd/|_test\.go$|_tests?\.rs$|(^|/)tests\.rs$|(^|/)tests?/|(^|/)testdata/|(^|/)examples?/"
)


def path_surface(path: str) -> str:
    if TEST_OR_HARNESS_RE.search(path):
        return "test_or_harness"
    if path.startswith("go/journal/"):
        return "go_sdk"
    if path.startswith("go/cmd/"):
        return "cli"
    if path.startswith("go/adapter/"):
        return "adapter"
    if path.startswith("rust/src/crates/jf/"):
        return "legacy_jf"
    if path.startswith("rust/src/crates/journal-core/"):
        return "rust_core"
    if path.startswith("rust/src/journal/"):
        return "rust_public"
    if path.startswith("rust/src/crates/journal-log-writer/"):
        return "rust_log_writer"
    if path.startswith("rust/src/crates/journal-index/"):
        return "rust_index"
    if path.startswith("rust/src/crates/journal-engine/"):
        return "rust_engine"
    if path.startswith("rust/src/cmd/"):
        return "cli"
    if path.startswith("rust/src/adapter/"):
        return "adapter"
    return "other"


def complexity_classification(path: str, complexity: int, max_ccn: int) -> str:
    surface = path_surface(path)
    if complexity <= 20:
        return "low; reasonable"
    if surface == "test_or_harness":
        return "test/harness metric; not production coverage signal"
    if max_ccn <= 12:
        if complexity >= 200:
            return "real file-size/ownership pressure; functions stay below CCN gate"
        return "moderate file-size pressure; functions stay below CCN gate"
    return "actionable function complexity; inspect before accepting"


def duplication_classification(path: str, duplication: int) -> str:
    surface = path_surface(path)
    if duplication <= 1:
        return "low; reasonable"
    if surface == "test_or_harness":
        return "test/harness repetition; not production coverage signal"
    if surface in {"legacy_jf", "rust_core"} and duplication >= 100:
        return "real legacy/core overlap; architecture debt, not scanner noise"
    if duplication >= 100:
        return "high production duplication; follow-up refactor candidate"
    return "small repeated blocks; monitor"


def load_lizard_max_ccn(path: Path) -> dict[str, int]:
    max_by_file: dict[str, int] = collections.defaultdict(int)
    with path.open(newline="", encoding="utf-8") as handle:
        for row in csv.reader(handle):
            if len(row) < 8:
                continue
            try:
                ccn = int(row[1])
            except ValueError:
                continue
            file_path = row[6]
            max_by_file[file_path] = max(max_by_file[file_path], ccn)
    return dict(max_by_file)


def as_int(value: Any) -> int:
    return int(value) if isinstance(value, int | float) else 0


def as_display(value: Any) -> str:
    if value is None:
        return "-"
    if isinstance(value, float):
        return f"{value:.2f}"
    return str(value)


def metric_row(file_metric: dict[str, Any], max_ccn: int) -> dict[str, Any]:
    path = str(file_metric["path"])
    complexity = as_int(file_metric.get("complexity"))
    duplication = as_int(file_metric.get("duplication"))
    return {
        "path": path,
        "surface": path_surface(path),
        "grade": file_metric.get("gradeLetter"),
        "codacy_complexity": complexity,
        "local_max_ccn": max_ccn,
        "duplication": duplication,
        "clones": as_int(file_metric.get("numberOfClones")),
        "coverage": file_metric.get("coverageWithDecimals"),
        "loc": as_int(file_metric.get("linesOfCode")),
        "complexity_classification": complexity_classification(path, complexity, max_ccn),
        "duplication_classification": duplication_classification(path, duplication),
    }


def build_summary(rows: list[dict[str, Any]]) -> dict[str, Any]:
    by_surface: dict[str, dict[str, Any]] = {}
    for surface, group_iter in itertools_groupby_sorted(rows, "surface"):
        group = list(group_iter)
        by_surface[surface] = {
            "files": len(group),
            "complex_files": sum(1 for row in group if row["codacy_complexity"] > 20),
            "duplicated_files": sum(1 for row in group if row["duplication"] > 1),
            "complexity_sum": sum(row["codacy_complexity"] for row in group),
            "duplication_sum": sum(row["duplication"] for row in group),
        }
    return {
        "files": len(rows),
        "complex_files": sum(1 for row in rows if row["codacy_complexity"] > 20),
        "duplicated_files": sum(1 for row in rows if row["duplication"] > 1),
        "by_surface": by_surface,
    }


def itertools_groupby_sorted(rows: list[dict[str, Any]], key: str):
    return itertools.groupby(sorted(rows, key=lambda row: row[key]), key=lambda row: row[key])


def write_markdown(path: Path, source: dict[str, Any], rows: list[dict[str, Any]]) -> None:
    summary = build_summary(rows)
    top_complexity = sorted(rows, key=lambda row: row["codacy_complexity"], reverse=True)[:20]
    top_duplication = sorted(rows, key=lambda row: row["duplication"], reverse=True)[:20]

    lines = [
        "# Codacy Rust/Go Metrics Audit",
        "",
        "## Scope",
        "",
        f"- Codacy branch: `{source.get('branch', 'unknown')}`.",
        f"- Codacy fetched at: `{source.get('fetchedAt', 'unknown')}`.",
        f"- Files analyzed in this report: `{summary['files']}`.",
        "- Raw Codacy API responses stay under `.local/` and are not committed.",
        "",
        "## Interpretation",
        "",
        "- Codacy file complexity is the sum of method/function cyclomatic complexity in a file.",
        "- Local `lizard` max CCN is the highest single-function CCN found in the same tracked file set.",
        "- A high Codacy complexity with max CCN <= 12 means file-size/ownership pressure, not a single dangerous function.",
        "- Test and harness paths are classified separately because they should not drive production coverage decisions.",
        "- Coverage values are Codacy file metrics at fetch time; coverage-report exclusions are validated separately by the coverage scripts and remote Codacy run.",
        "- This is a point-in-time audit snapshot. Regenerate it after substantial Rust/Go refactors or Codacy metric changes.",
        "",
        "## Regeneration",
        "",
        "```bash",
        "tests/code_scanning/export_codacy_file_metrics.js \\",
        "  --output .local/codacy/file-metrics-rust-go.json \\",
        "  --search go/ --search rust/",
        "git ls-files 'go/**/*.go' 'rust/**/*.rs' > .local/codacy/rust-go-source-files.txt",
        "lizard -C 12 --csv -f .local/codacy/rust-go-source-files.txt > .local/codacy/lizard-rust-go.csv",
        "python3 tests/code_scanning/summarize_codacy_file_metrics.py \\",
        "  --metrics .local/codacy/file-metrics-rust-go.json \\",
        "  --lizard-csv .local/codacy/lizard-rust-go.csv \\",
        "  --markdown-output .agents/sow/specs/codacy-rust-go-metrics-audit.md \\",
        "  --json-output .local/codacy/file-metrics-rust-go-summary.json",
        "```",
        "",
        "## Surface Summary",
        "",
        "| Surface | Files | Complex Files | Duplicated Files | Complexity Sum | Duplication Sum |",
        "|---|---:|---:|---:|---:|---:|",
    ]

    for surface, values in sorted(summary["by_surface"].items()):
        lines.append(
            f"| `{surface}` | {values['files']} | {values['complex_files']} | "
            f"{values['duplicated_files']} | {values['complexity_sum']} | {values['duplication_sum']} |"
        )

    lines.extend(
        [
            "",
            "## Top Complexity",
            "",
            "| Path | Surface | Complexity | Max CCN | Duplication | Coverage | Classification |",
            "|---|---|---:|---:|---:|---:|---|",
        ]
    )
    for row in top_complexity:
        lines.append(
            f"| `{row['path']}` | `{row['surface']}` | {row['codacy_complexity']} | "
            f"{row['local_max_ccn']} | {row['duplication']} | {as_display(row['coverage'])} | "
            f"{row['complexity_classification']} |"
        )

    lines.extend(
        [
            "",
            "## Top Duplication",
            "",
            "| Path | Surface | Duplication | Clones | Complexity | Coverage | Classification |",
            "|---|---|---:|---:|---:|---:|---|",
        ]
    )
    for row in top_duplication:
        lines.append(
            f"| `{row['path']}` | `{row['surface']}` | {row['duplication']} | "
            f"{row['clones']} | {row['codacy_complexity']} | {as_display(row['coverage'])} | "
            f"{row['duplication_classification']} |"
        )

    lines.extend(
        [
            "",
            "## File By File",
            "",
            "| Path | Surface | Grade | Complexity | Max CCN | Duplication | Clones | Coverage | LOC | Complexity Classification | Duplication Classification |",
            "|---|---|---:|---:|---:|---:|---:|---:|---:|---|---|",
        ]
    )
    for row in sorted(rows, key=lambda item: item["path"]):
        lines.append(
            f"| `{row['path']}` | `{row['surface']}` | {as_display(row['grade'])} | "
            f"{row['codacy_complexity']} | {row['local_max_ccn']} | {row['duplication']} | "
            f"{row['clones']} | {as_display(row['coverage'])} | {row['loc']} | "
            f"{row['complexity_classification']} | {row['duplication_classification']} |"
        )

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--metrics", required=True)
    parser.add_argument("--lizard-csv", required=True)
    parser.add_argument("--markdown-output", required=True)
    parser.add_argument("--json-output")
    args = parser.parse_args()

    source = json.loads(Path(args.metrics).read_text(encoding="utf-8"))
    max_ccn = load_lizard_max_ccn(Path(args.lizard_csv))
    rows = [
        metric_row(file_metric, max_ccn.get(str(file_metric["path"]), 0))
        for file_metric in source.get("files", [])
        if isinstance(file_metric, dict) and isinstance(file_metric.get("path"), str)
    ]

    write_markdown(Path(args.markdown_output), source, rows)
    if args.json_output:
        output = {"summary": build_summary(rows), "files": rows}
        Path(args.json_output).parent.mkdir(parents=True, exist_ok=True)
        Path(args.json_output).write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
