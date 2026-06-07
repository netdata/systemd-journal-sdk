#!/usr/bin/env python3
"""Summarize Codacy file metrics without storing API credentials."""

from __future__ import annotations

import collections
import csv
import itertools
import re
from pathlib import Path
from typing import Any


TEST_OR_HARNESS_RE = re.compile(
    r"(^|/)internal/testcmd/"
    r"|_test\.go$"
    r"|_tests?\.rs$"
    r"|(^|/)tests\.rs$"
    r"|(^|/)tests?/"
    r"|(^|/)testdata/"
    r"|(^|/)examples?/"
)

SURFACE_PREFIXES = (
    ("go/journal/", "go_sdk"),
    ("go/cmd/", "cli"),
    ("go/adapter/", "adapter"),
    ("rust/src/crates/jf/", "legacy_jf"),
    ("rust/src/crates/journal-core/", "rust_core"),
    ("rust/src/journal/", "rust_public"),
    ("rust/src/crates/journal-log-writer/", "rust_log_writer"),
    ("rust/src/crates/journal-index/", "rust_index"),
    ("rust/src/crates/journal-engine/", "rust_engine"),
    ("rust/src/cmd/", "cli"),
    ("rust/src/adapter/", "adapter"),
)


def path_surface(path: str) -> str:
    if TEST_OR_HARNESS_RE.search(path):
        return "test_or_harness"
    for prefix, surface in SURFACE_PREFIXES:
        if path.startswith(prefix):
            return surface
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


def source_field(source: dict[str, Any], name: str) -> str:
    value = source.get(name)
    return value if isinstance(value, str) else "unknown"


def markdown_preamble(source: dict[str, str], summary: dict[str, Any]) -> list[str]:
    coverage_note = (
        "- Coverage values are Codacy file metrics at fetch time; coverage-report exclusions are "
        "validated separately by the coverage scripts and remote Codacy run."
    )
    return [
        "# Codacy Rust/Go Metrics Audit",
        "",
        "## Scope",
        "",
        f"- Codacy branch: `{source['branch']}`.",
        f"- Codacy fetched at: `{source['fetched_at']}`.",
        f"- Files analyzed in this report: `{summary['files']}`.",
        "- Raw Codacy API responses stay under `.local/` and are not committed.",
        "",
        "## Interpretation",
        "",
        "- Codacy file complexity is the sum of method/function cyclomatic complexity in a file.",
        "- Local `lizard` max CCN is the highest single-function CCN found in the same tracked file set.",
        "- A high Codacy complexity with max CCN <= 12 means file-size/ownership pressure, not a single dangerous function.",
        "- Test and harness paths are classified separately because they should not drive production coverage decisions.",
        coverage_note,
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
        "python3 - <<'PY' > .agents/sow/specs/codacy-rust-go-metrics-audit.md",
        "import json",
        "from pathlib import Path",
        "from tests.code_scanning.summarize_codacy_file_metrics import (",
        "    load_lizard_max_ccn, metric_row, render_markdown, source_field,",
        ")",
        "source = json.loads(Path('.local/codacy/file-metrics-rust-go.json').read_text(encoding='utf-8'))",
        "max_ccn = load_lizard_max_ccn(Path('.local/codacy/lizard-rust-go.csv'))",
        "rows = [",
        "    metric_row(file_metric, max_ccn.get(str(file_metric['path']), 0))",
        "    for file_metric in source.get('files', [])",
        "    if isinstance(file_metric, dict) and isinstance(file_metric.get('path'), str)",
        "]",
        "print(render_markdown({",
        "    'branch': source_field(source, 'branch'),",
        "    'fetched_at': source_field(source, 'fetchedAt'),",
        "}, rows), end='')",
        "PY",
        "```",
        "",
        "## Surface Summary",
        "",
        "| Surface | Files | Complex Files | Duplicated Files | Complexity Sum | Duplication Sum |",
        "|---|---:|---:|---:|---:|---:|",
    ]


def append_surface_summary(lines: list[str], summary: dict[str, Any]) -> None:
    for surface, values in sorted(summary["by_surface"].items()):
        lines.append(
            f"| `{surface}` | {values['files']} | {values['complex_files']} | "
            f"{values['duplicated_files']} | {values['complexity_sum']} | {values['duplication_sum']} |"
        )


def append_top_complexity(lines: list[str], rows: list[dict[str, Any]]) -> None:
    lines.extend(
        [
            "",
            "## Top Complexity",
            "",
            "| Path | Surface | Complexity | Max CCN | Duplication | Coverage | Classification |",
            "|---|---|---:|---:|---:|---:|---|",
        ]
    )
    for row in rows:
        lines.append(
            f"| `{row['path']}` | `{row['surface']}` | {row['codacy_complexity']} | "
            f"{row['local_max_ccn']} | {row['duplication']} | {as_display(row['coverage'])} | "
            f"{row['complexity_classification']} |"
        )


def append_top_duplication(lines: list[str], rows: list[dict[str, Any]]) -> None:
    lines.extend(
        [
            "",
            "## Top Duplication",
            "",
            "| Path | Surface | Duplication | Clones | Complexity | Coverage | Classification |",
            "|---|---|---:|---:|---:|---:|---|",
        ]
    )
    for row in rows:
        lines.append(
            f"| `{row['path']}` | `{row['surface']}` | {row['duplication']} | "
            f"{row['clones']} | {row['codacy_complexity']} | {as_display(row['coverage'])} | "
            f"{row['duplication_classification']} |"
        )


def append_file_by_file(lines: list[str], rows: list[dict[str, Any]]) -> None:
    file_table_header = (
        "| Path | Surface | Grade | Complexity | Max CCN | Duplication | Clones | "
        "Coverage | LOC | Complexity Classification | Duplication Classification |"
    )
    lines.extend(
        [
            "",
            "## File By File",
            "",
            file_table_header,
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


def render_markdown(source: dict[str, str], rows: list[dict[str, Any]]) -> str:
    summary = build_summary(rows)
    lines = markdown_preamble(source, summary)
    append_surface_summary(lines, summary)
    append_top_complexity(
        lines,
        sorted(rows, key=lambda row: row["codacy_complexity"], reverse=True)[:20],
    )
    append_top_duplication(
        lines,
        sorted(rows, key=lambda row: row["duplication"], reverse=True)[:20],
    )
    append_file_by_file(lines, sorted(rows, key=lambda item: item["path"]))
    return "\n".join(lines) + "\n"
