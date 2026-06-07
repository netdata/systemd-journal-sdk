from __future__ import annotations

import json
from pathlib import Path

import pytest

from tests.code_scanning.export_codacy_issues import parse_codacy_json, validate_https_url
from tests.code_scanning.summarize_findings import (
    build_summary,
    findings_from_codacy_issues,
    findings_from_codacy_security,
    findings_from_sarif,
    path_class,
    path_prefix,
    tool_from_rule,
)
from tests.code_scanning.write_empty_codacy_sarif import (
    CODACY_TOOL_NAMES,
    build_empty_sarif,
)
from tests.code_scanning.summarize_codacy_file_metrics import (
    complexity_classification,
    duplication_classification,
    metric_row,
    path_surface,
    render_markdown,
)


def test_path_grouping_is_sanitized() -> None:
    assert path_class("rust/src/lib.rs") == "rust"
    assert path_class(".agents/sow/current/SOW.md") == ".agents"
    assert path_prefix("rust/src/crates/journal-core/src/file.rs") == "rust/src"


def test_sarif_summary_uses_rule_and_path_only(tmp_path: Path) -> None:
    sarif = {
        "version": "2.1.0",
        "runs": [
            {
                "tool": {
                    "driver": {
                        "name": "sample-tool",
                        "rules": [
                            {
                                "id": "unsafe-rule",
                                "properties": {
                                    "problem.severity": "warning",
                                    "tags": ["security"],
                                },
                            }
                        ],
                    }
                },
                "results": [
                    {
                        "ruleId": "unsafe-rule",
                        "level": "warning",
                        "message": {"text": "do not copy this into reports"},
                        "locations": [
                            {
                                "physicalLocation": {
                                    "artifactLocation": {"uri": "go/journal/writer.go"},
                                    "region": {"startLine": 10},
                                }
                            }
                        ],
                    }
                ],
            }
        ],
    }
    path = tmp_path / "sample.sarif"
    path.write_text(json.dumps(sarif), encoding="utf-8")

    findings = findings_from_sarif(path)
    summary = build_summary(findings, limit=10)

    assert summary["total"] == 1
    assert summary["by_tool"] == [{"tool": "sample-tool", "count": 1}]
    assert summary["by_path_class"] == [{"path_class": "go", "count": 1}]
    assert "do not copy" not in json.dumps(summary)


def test_empty_codacy_sarif_closeout_has_stale_alert_tools_only() -> None:
    sarif = build_empty_sarif()
    runs = sarif["runs"]

    assert sarif["version"] == "2.1.0"
    assert len(runs) == len(CODACY_TOOL_NAMES)
    assert [run["tool"]["driver"]["name"] for run in runs] == list(CODACY_TOOL_NAMES)
    assert all(run["results"] == [] for run in runs)
    assert "ESLint8" in CODACY_TOOL_NAMES
    assert "ESLint9" not in CODACY_TOOL_NAMES


def test_codacy_issue_summary_handles_cloud_shape(tmp_path: Path) -> None:
    export = {
        "data": [
            {
                "filePath": "python/journal/reader.py",
                "severity": "Medium",
                "category": "ErrorProne",
                "toolInfo": {"name": "Ruff"},
                "patternInfo": {"id": "F401"},
            },
            {
                "filePath": "python/journal/writer.py",
                "severity": "Medium",
                "category": "ErrorProne",
                "toolInfo": {"name": "Ruff"},
                "patternInfo": {"id": "F401"},
            },
        ]
    }
    path = tmp_path / "codacy-issues.json"
    path.write_text(json.dumps(export), encoding="utf-8")

    findings = findings_from_codacy_issues(path)
    summary = build_summary(findings, limit=10)

    assert summary["total"] == 2
    assert summary["by_tool"] == [{"tool": "Ruff", "count": 2}]
    assert summary["by_rule"] == [
        {"tool": "Ruff", "rule": "F401", "severity": "Medium", "count": 2}
    ]


def test_codacy_cli_json_parser_ignores_progress_lines() -> None:
    payload = parse_codacy_json('- Fetching issues...\n{"issues": [{"resultDataId": 1}]}')
    assert payload == {"issues": [{"resultDataId": 1}]}


def test_rule_tool_detection_handles_current_and_historical_eslint() -> None:
    assert tool_from_rule("ESLint8_security_detect-object-injection") == "ESLint8"
    assert tool_from_rule("ESLint9_security_detect-object-injection") == "ESLint9"


def test_codacy_api_url_requires_https() -> None:
    validate_https_url("https://app.codacy.com/api/v3")
    with pytest.raises(RuntimeError):
        validate_https_url("http://app.codacy.com/api/v3")


def test_codacy_security_summary_handles_findings_shape(tmp_path: Path) -> None:
    export = {
        "data": [
            {
                "priority": "Critical",
                "securityCategory": "CommandInjection",
                "scanType": "SAST",
                "filePath": "tests/corpus_eval/run_corpus_eval.py",
            }
        ]
    }
    path = tmp_path / "codacy-findings.json"
    path.write_text(json.dumps(export), encoding="utf-8")

    findings = findings_from_codacy_security(path)
    summary = build_summary(findings, limit=10)

    assert summary["total"] == 1
    assert summary["by_tool"] == [{"tool": "SAST", "count": 1}]
    assert summary["by_rule"] == [
        {
            "tool": "SAST",
            "rule": "CommandInjection",
            "severity": "Critical",
            "count": 1,
        }
    ]


def test_codacy_file_metrics_classifies_tests_and_harnesses() -> None:
    assert path_surface("go/internal/testcmd/reader/main.go") == "test_or_harness"
    assert path_surface("go/journal/reader_test.go") == "test_or_harness"
    assert path_surface("rust/src/crates/journal-core/src/file/tests.rs") == "test_or_harness"
    assert path_surface("rust/src/crates/jf/journal_file/src/file_test.rs") == "test_or_harness"


def test_codacy_file_metrics_classifies_rust_and_go_surfaces() -> None:
    assert path_surface("go/journal/netdata.go") == "go_sdk"
    assert path_surface("go/cmd/journalctl/main.go") == "cli"
    assert path_surface("rust/src/crates/journal-core/src/file/file.rs") == "rust_core"
    assert path_surface("rust/src/crates/jf/journal_file/src/file.rs") == "legacy_jf"


def test_codacy_file_metrics_separates_file_size_from_function_complexity() -> None:
    assert (
        complexity_classification("go/journal/netdata.go", complexity=870, max_ccn=12)
        == "real file-size/ownership pressure; functions stay below CCN gate"
    )
    assert (
        complexity_classification("go/journal/netdata.go", complexity=50, max_ccn=13)
        == "actionable function complexity; inspect before accepting"
    )


def test_codacy_file_metrics_flags_legacy_core_duplication_debt() -> None:
    assert (
        duplication_classification(
            "rust/src/crates/jf/journal_file/src/file.rs",
            duplication=686,
        )
        == "real legacy/core overlap; architecture debt, not scanner noise"
    )
    assert (
        duplication_classification("go/journal/explorer.go", duplication=111)
        == "high production duplication; follow-up refactor candidate"
    )


def test_codacy_file_metric_row_is_sanitized_and_joinable() -> None:
    row = metric_row(
        {
            "path": "go/journal/explorer.go",
            "gradeLetter": "B",
            "complexity": 763,
            "duplication": 111,
            "numberOfClones": 4,
            "coverageWithDecimals": 78.46,
            "linesOfCode": 2316,
        },
        max_ccn=12,
    )

    assert row["path"] == "go/journal/explorer.go"
    assert row["surface"] == "go_sdk"
    assert row["codacy_complexity"] == 763
    assert row["local_max_ccn"] == 12
    assert row["duplication"] == 111
    assert "token" not in json.dumps(row).lower()


def test_codacy_file_metric_row_accepts_numeric_strings() -> None:
    row = metric_row(
        {
            "path": "go/journal/explorer.go",
            "gradeLetter": "B",
            "complexity": "763",
            "duplication": "111.0",
            "numberOfClones": "",
            "coverageWithDecimals": 78.46,
            "linesOfCode": "not-a-number",
        },
        max_ccn=12,
    )

    assert row["codacy_complexity"] == 763
    assert row["duplication"] == 111
    assert row["clones"] == 0
    assert row["loc"] == 0


def test_codacy_file_metrics_markdown_renderer_smoke() -> None:
    rows = [
        metric_row(
            {
                "path": "go/journal/explorer.go",
                "gradeLetter": "B",
                "complexity": 763,
                "duplication": 111,
                "numberOfClones": 4,
                "coverageWithDecimals": 78.46,
                "linesOfCode": 2316,
            },
            max_ccn=12,
        )
    ]

    text = render_markdown({"branch": "master", "fetched_at": "now"}, rows)
    assert "# Codacy Rust/Go Metrics Audit" in text
    assert "go/journal/explorer.go" in text
