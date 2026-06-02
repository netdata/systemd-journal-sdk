from __future__ import annotations

import json

import pytest

from tests.code_scanning.summarize_findings import (
    build_summary,
    findings_from_codacy_issues,
    findings_from_codacy_security,
    findings_from_sarif,
    path_class,
    path_prefix,
)
from tests.code_scanning.export_codacy_issues import parse_codacy_json, _validate_https_url


def test_path_grouping_is_sanitized() -> None:
    assert path_class("rust/src/lib.rs") == "rust"
    assert path_class(".agents/sow/current/SOW.md") == ".agents"
    assert path_prefix("rust/src/crates/journal-core/src/file.rs") == "rust/src"


def test_sarif_summary_uses_rule_and_path_only(tmp_path) -> None:
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


def test_codacy_issue_summary_handles_cloud_shape(tmp_path) -> None:
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


def test_codacy_api_url_requires_https() -> None:
    _validate_https_url("https://app.codacy.com/api/v3")
    with pytest.raises(RuntimeError):
        _validate_https_url("http://app.codacy.com/api/v3")


def test_codacy_security_summary_handles_findings_shape(tmp_path) -> None:
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
