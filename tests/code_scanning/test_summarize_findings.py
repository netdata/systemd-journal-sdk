from __future__ import annotations

import json

from tests.code_scanning.summarize_findings import (
    build_summary,
    findings_from_codacy_issues,
    findings_from_sarif,
    path_class,
    path_prefix,
)


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
