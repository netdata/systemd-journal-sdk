#!/usr/bin/env python3
"""Summarize static-analysis exports without copying source snippets."""

from __future__ import annotations

import argparse
import collections
import json
import sys
from pathlib import Path
from typing import Any, Iterable


def path_class(path: str | None) -> str:
    if not path:
        return "unknown"
    normalized = normalize_path(path)
    if normalized.startswith(".github/"):
        return ".github"
    if normalized.startswith(".agents/"):
        return ".agents"
    for prefix in (
        "rust/",
        "go/",
        "experiments/",
        "tests/",
        "fixtures/",
        "documentation/",
        "benchmarks/",
        "cli/",
    ):
        if normalized.startswith(prefix):
            return prefix.rstrip("/")
    if "/" not in normalized:
        return "root"
    return normalized.split("/", 1)[0]


def path_prefix(path: str | None, depth: int = 2) -> str:
    if not path:
        return "unknown"
    normalized = normalize_path(path)
    parts = [part for part in normalized.split("/") if part]
    if len(parts) <= depth:
        return normalized
    return "/".join(parts[:depth])


def normalize_path(path: str) -> str:
    normalized = path.replace("\\", "/")
    while normalized.startswith("./"):
        normalized = normalized[2:]
    return normalized


def _as_list(payload: Any) -> list[Any]:
    if isinstance(payload, list):
        return payload
    if isinstance(payload, dict):
        for key in ("data", "items", "results", "findings", "issues"):
            value = payload.get(key)
            if isinstance(value, list):
                return value
    return []


def _first_string(*values: Any, default: str = "unknown") -> str:
    for value in values:
        if isinstance(value, str) and value:
            return value
    return default


def _nested_string(payload: dict[str, Any], *path: str) -> str | None:
    current: Any = payload
    for key in path:
        if not isinstance(current, dict):
            return None
        current = current.get(key)
    return current if isinstance(current, str) and current else None


def tool_from_rule(rule_id: str | None) -> str | None:
    if not rule_id:
        return None
    known_prefixes = (
        "Agentlinter",
        "Bandit",
        "ESLint8",
        "ESLint9",
        "Lizard",
        "PMD",
        "Prospector",
        "PyLintPython3",
        "Semgrep",
        "Trivy",
        "cppcheck",
        "flawfinder",
        "markdownlint",
        "shellcheck",
    )
    for prefix in known_prefixes:
        if rule_id == prefix or rule_id.startswith(f"{prefix}_"):
            return prefix
    return rule_id.split("_", 1)[0] if "_" in rule_id else None


def findings_from_sarif(path: Path) -> list[dict[str, str]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    findings: list[dict[str, str]] = []
    for run in payload.get("runs", []):
        if not isinstance(run, dict):
            continue
        tool = sarif_tool_name(run)
        rules = sarif_rules(run)
        for result in run.get("results", []):
            if not isinstance(result, dict):
                continue
            findings.append(sarif_result_finding(path.name, tool, rules, result))
    return findings


def sarif_tool_name(run: dict[str, Any]) -> str:
    return _nested_string(run, "tool", "driver", "name") or "sarif"


def sarif_rules(run: dict[str, Any]) -> dict[str, dict[str, Any]]:
    driver = run.get("tool", {}).get("driver", {})
    rules = driver.get("rules", []) if isinstance(driver, dict) else []
    return {
        rule["id"]: rule
        for rule in rules
        if isinstance(rule, dict) and isinstance(rule.get("id"), str)
    }


def sarif_result_uri(result: dict[str, Any]) -> str | None:
    locations = result.get("locations") or []
    if locations and isinstance(locations[0], dict):
        return _nested_string(
            locations[0],
            "physicalLocation",
            "artifactLocation",
            "uri",
        )
    return None


def sarif_rule_properties(
    rules: dict[str, dict[str, Any]],
    rule_id: str,
) -> dict[str, Any]:
    rule = rules.get(rule_id, {})
    properties = rule.get("properties", {})
    return properties if isinstance(properties, dict) else {}


def sarif_result_category(properties: dict[str, Any]) -> str:
    tags = properties.get("tags")
    first_tag = tags[0] if isinstance(tags, list) and tags else None
    return _first_string(first_tag, default="unknown")


def sarif_result_finding(
    source: str,
    tool: str,
    rules: dict[str, dict[str, Any]],
    result: dict[str, Any],
) -> dict[str, str]:
    rule_id = _first_string(result.get("ruleId"))
    properties = sarif_rule_properties(rules, rule_id)
    uri = sarif_result_uri(result)
    return {
        "source": source,
        "tool": tool,
        "rule": rule_id,
        "severity": _first_string(
            result.get("level"),
            properties.get("problem.severity"),
            properties.get("security-severity"),
        ),
        "category": sarif_result_category(properties),
        "path_class": path_class(uri),
        "path_prefix": path_prefix(uri),
    }


def findings_from_codacy_issues(path: Path) -> list[dict[str, str]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    findings: list[dict[str, str]] = []
    for item in _as_list(payload):
        if not isinstance(item, dict):
            continue
        pattern = item.get("patternInfo")
        tool = item.get("toolInfo")
        if not isinstance(pattern, dict):
            pattern = {}
        if not isinstance(tool, dict):
            tool = {}

        file_path = _first_string(
            item.get("filePath"),
            item.get("filename"),
            item.get("path"),
            _nested_string(item, "location", "path"),
            default="unknown",
        )
        rule_id = _first_string(
            pattern.get("id"),
            pattern.get("patternId"),
            item.get("patternId"),
            item.get("ruleId"),
        )
        findings.append(
            {
                "source": path.name,
                "tool": _first_string(
                    tool.get("name"),
                    item.get("toolName"),
                    tool_from_rule(rule_id),
                ),
                "rule": rule_id,
                "severity": _first_string(
                    item.get("severity"),
                    pattern.get("severityLevel"),
                    pattern.get("level"),
                ),
                "category": _first_string(
                    item.get("category"),
                    pattern.get("category"),
                    pattern.get("categoryName"),
                ),
                "path_class": path_class(file_path),
                "path_prefix": path_prefix(file_path),
            }
        )
    return findings


def findings_from_codacy_security(path: Path) -> list[dict[str, str]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    findings: list[dict[str, str]] = []
    for item in _as_list(payload):
        if not isinstance(item, dict):
            continue
        file_path = _first_string(
            item.get("filePath"),
            item.get("filename"),
            item.get("path"),
            _nested_string(item, "location", "path"),
            default="unknown",
        )
        findings.append(
            {
                "source": path.name,
                "tool": _first_string(item.get("scanType"), item.get("toolName")),
                "rule": _first_string(
                    item.get("securityCategory"),
                    item.get("category"),
                    item.get("title"),
                ),
                "severity": _first_string(item.get("priority"), item.get("severity")),
                "category": _first_string(item.get("securityCategory"), item.get("category")),
                "path_class": path_class(file_path),
                "path_prefix": path_prefix(file_path),
            }
        )
    return findings


def top_counts(
    findings: Iterable[dict[str, str]],
    keys: tuple[str, ...],
    limit: int,
) -> list[dict[str, Any]]:
    counter: collections.Counter[tuple[str, ...]] = collections.Counter()
    for finding in findings:
        counter[tuple(finding.get(key, "unknown") for key in keys)] += 1
    rows = []
    for values, count in counter.most_common(limit):
        row = {key: value for key, value in zip(keys, values, strict=True)}
        row["count"] = count
        rows.append(row)
    return rows


def build_summary(findings: list[dict[str, str]], limit: int) -> dict[str, Any]:
    return {
        "total": len(findings),
        "by_source": top_counts(findings, ("source",), limit),
        "by_severity": top_counts(findings, ("severity",), limit),
        "by_path_class": top_counts(findings, ("path_class",), limit),
        "by_tool": top_counts(findings, ("tool",), limit),
        "by_rule": top_counts(findings, ("tool", "rule", "severity"), limit),
        "by_path_prefix": top_counts(findings, ("path_prefix",), limit),
    }


def markdown_table(title: str, rows: list[dict[str, Any]], columns: list[str]) -> str:
    output = [f"### {title}", "", "| " + " | ".join(columns) + " |"]
    output.append("| " + " | ".join("---" for _ in columns) + " |")
    for row in rows:
        output.append("| " + " | ".join(str(row.get(column, "")) for column in columns) + " |")
    if not rows:
        output.append("| " + " | ".join("0" if column == "count" else "" for column in columns) + " |")
    output.append("")
    return "\n".join(output)


def summary_markdown(summary: dict[str, Any]) -> str:
    sections = ["## Static Analysis Summary", "", f"Total findings: {summary['total']}", ""]
    sections.append(markdown_table("By Source", summary["by_source"], ["source", "count"]))
    sections.append(markdown_table("By Severity", summary["by_severity"], ["severity", "count"]))
    sections.append(
        markdown_table("By Path Class", summary["by_path_class"], ["path_class", "count"])
    )
    sections.append(markdown_table("By Tool", summary["by_tool"], ["tool", "count"]))
    sections.append(
        markdown_table(
            "Top Rules",
            summary["by_rule"],
            ["tool", "rule", "severity", "count"],
        )
    )
    sections.append(
        markdown_table(
            "Top Path Prefixes",
            summary["by_path_prefix"],
            ["path_prefix", "count"],
        )
    )
    return "\n".join(sections).rstrip() + "\n"


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sarif", action="append", default=[], help="SARIF file to summarize")
    parser.add_argument(
        "--codacy-issues",
        action="append",
        default=[],
        help="Codacy cloud issue export JSON to summarize",
    )
    parser.add_argument(
        "--codacy-findings",
        action="append",
        default=[],
        help="Codacy security finding export JSON to summarize",
    )
    parser.add_argument("--json-output", required=True)
    parser.add_argument("--markdown-output", required=True)
    parser.add_argument("--limit", type=int, default=25)
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    findings: list[dict[str, str]] = []

    for sarif in args.sarif:
        findings.extend(findings_from_sarif(Path(sarif)))
    for codacy_issues in args.codacy_issues:
        findings.extend(findings_from_codacy_issues(Path(codacy_issues)))
    for codacy_findings in args.codacy_findings:
        findings.extend(findings_from_codacy_security(Path(codacy_findings)))

    summary = build_summary(findings, args.limit)
    json_path = Path(args.json_output)
    markdown_path = Path(args.markdown_output)
    json_path.parent.mkdir(parents=True, exist_ok=True)
    markdown_path.parent.mkdir(parents=True, exist_ok=True)
    json_path.write_text(json.dumps(summary, indent=2, sort_keys=True), encoding="utf-8")
    markdown_path.write_text(summary_markdown(summary), encoding="utf-8")
    print(f"wrote static-analysis summary for {summary['total']} findings")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
