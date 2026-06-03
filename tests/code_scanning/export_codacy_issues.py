#!/usr/bin/env python3
"""Export Codacy cloud issues into `.local/` for offline triage.

The default local path uses the authenticated `codacy` CLI. The API-token path
is retained for GitHub Actions and non-interactive environments.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import subprocess  # nosec B404 - subprocess is required by harnesses.
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any


DEFAULT_API_BASE = "https://app.codacy.com/api/v3"
DEFAULT_LANGUAGES = ("C", "Go", "Javascript", "Markdown", "Python", "Rust", "Shell")
DEFAULT_CLI_TIMEOUT_SECONDS = 300


def parse_codacy_json(raw: str) -> dict[str, Any]:
    """Parse Codacy CLI JSON after stripping progress lines."""
    for marker in ("{", "["):
        index = raw.find(marker)
        if index >= 0:
            parsed = json.loads(raw[index:])
            if isinstance(parsed, dict):
                return parsed
            return {"data": parsed}
    raise RuntimeError("Codacy CLI output did not include JSON")


def run_codacy(args: list[str], timeout: int = DEFAULT_CLI_TIMEOUT_SECONDS) -> dict[str, Any]:
    command = ["codacy", *args, "-o", "json"]
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    completed = subprocess.run(  # nosec B603 - harness uses shell=False command vectors.
        command,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=timeout,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"Codacy CLI failed with exit code {completed.returncode}: "
            f"{completed.stdout[:500]}"
        )
    return parse_codacy_json(completed.stdout)


def _request_json(url: str, token: str, body: dict[str, Any]) -> dict[str, Any]:
    _validate_https_url(url)
    data = json.dumps(body).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=data,
        method="POST",
        headers={
            "Content-Type": "application/json",
            "Accept": "application/json",
            "api-token": token,
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as response:  # nosec B310 - _validate_https_url() rejects non-HTTPS schemes.
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", errors="replace")
        raise RuntimeError(
            f"Codacy API request failed with HTTP {error.code}: {detail[:500]}"
        ) from error


def _validate_https_url(url: str) -> None:
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme != "https":
        raise RuntimeError(f"Codacy API URL must use https, got {parsed.scheme or '<empty>'}")
    if not parsed.netloc:
        raise RuntimeError("Codacy API URL must include a host")


def _next_cursor(payload: dict[str, Any]) -> str | None:
    pagination = payload.get("pagination")
    if isinstance(pagination, dict):
        for key in ("cursor", "nextCursor", "next"):
            value = pagination.get(key)
            if isinstance(value, str) and value:
                return value

    for key in ("cursor", "nextCursor", "next"):
        value = payload.get(key)
        if isinstance(value, str) and value:
            return value

    return None


def export_issues(args: argparse.Namespace) -> dict[str, Any]:
    token = os.environ.get("CODACY_API_TOKEN")
    if not token:
        raise RuntimeError("CODACY_API_TOKEN is not set")

    endpoint = (
        f"{args.api_base.rstrip('/')}/analysis/organizations/"
        f"{urllib.parse.quote(args.provider)}/"
        f"{urllib.parse.quote(args.organization)}/repositories/"
        f"{urllib.parse.quote(args.repository)}/issues/search"
    )

    items: list[dict[str, Any]] = []
    cursor: str | None = None
    page = 0

    while True:
        page += 1
        query = {"limit": str(args.limit)}
        if args.branch:
            query["branch"] = args.branch
        if cursor:
            query["cursor"] = cursor

        url = f"{endpoint}?{urllib.parse.urlencode(query)}"
        payload = _request_json(url, token, {})
        page_items = payload.get("data", payload.get("items", []))
        if not isinstance(page_items, list):
            raise RuntimeError("Codacy API response did not include an issue list")

        items.extend(page_items)
        cursor = _next_cursor(payload)
        if not cursor:
            total = payload.get("total", payload.get("totalCount", len(items)))
            return {
                "provider": args.provider,
                "organization": args.organization,
                "repository": args.repository,
                "branch": args.branch,
                "fetched_at": dt.datetime.now(dt.UTC).isoformat(),
                "api_base": args.api_base,
                "pages": page,
                "total_reported": total,
                "count": len(items),
                "data": items,
            }


def export_issues_with_cli(args: argparse.Namespace) -> dict[str, Any]:
    """Export quality issues using language partitions to avoid CLI caps."""
    overview = run_codacy(
        [
            "issues",
            args.provider,
            args.organization,
            args.repository,
            "--branch",
            args.branch,
            "--overview",
        ],
        timeout=args.cli_timeout,
    )

    deduped: dict[str, dict[str, Any]] = {}
    partitions: list[dict[str, Any]] = []
    for language in codacy_overview_languages(overview):
        issues = fetch_language_issues(args, language)
        partitions.append({"language": language, "count": len(issues)})
        if len(issues) >= args.limit:
            raise RuntimeError(
                f"Codacy CLI language partition {language} reached the {args.limit} limit; "
                "add a narrower partition before trusting the export"
            )
        for issue in issues:
            if not isinstance(issue, dict):
                continue
            deduped[codacy_issue_key(issue)] = issue

    return {
        "provider": args.provider,
        "organization": args.organization,
        "repository": args.repository,
        "branch": args.branch,
        "fetched_at": dt.datetime.now(dt.UTC).isoformat(),
        "source": "codacy-cli",
        "overview": overview.get("overview", {}),
        "partitions": partitions,
        "count": len(deduped),
        "data": list(deduped.values()),
    }


def codacy_overview_languages(overview: dict[str, Any]) -> list[str]:
    overview_payload = overview.get("overview")
    overview_languages = (
        overview_payload.get("languages", [])
        if isinstance(overview_payload, dict)
        else []
    )
    languages = [
        item["name"]
        for item in overview_languages
        if isinstance(item, dict) and isinstance(item.get("name"), str)
    ]
    return languages or list(DEFAULT_LANGUAGES)


def fetch_language_issues(args: argparse.Namespace, language: str) -> list[Any]:
    payload = run_codacy(
        [
            "issues",
            args.provider,
            args.organization,
            args.repository,
            "--branch",
            args.branch,
            "--languages",
            language,
            "--limit",
            str(args.limit),
        ],
        timeout=args.cli_timeout,
    )
    issues = payload.get("issues", [])
    if not isinstance(issues, list):
        raise RuntimeError(f"Codacy CLI response for {language} did not include issues")
    return issues


def codacy_issue_key(issue: dict[str, Any]) -> str:
    result_id = issue.get("resultDataId")
    if result_id:
        return str(result_id)
    pattern = issue.get("patternInfo")
    pattern_id = pattern.get("id") if isinstance(pattern, dict) else None
    return str(
        (
            issue.get("filePath"),
            issue.get("lineNumber"),
            issue.get("message"),
            pattern_id,
        )
    )


def export_findings_with_cli(args: argparse.Namespace) -> dict[str, Any]:
    payload = run_codacy(
        [
            "findings",
            args.provider,
            args.organization,
            args.repository,
            "--statuses",
            args.finding_statuses,
            "--limit",
            str(args.limit),
        ],
        timeout=args.cli_timeout,
    )
    findings = payload.get("findings", [])
    if not isinstance(findings, list):
        raise RuntimeError("Codacy CLI findings response did not include findings")
    if len(findings) >= args.limit:
        raise RuntimeError(
            f"Codacy CLI findings export reached the {args.limit} limit; "
            "add narrower severity/status partitions before trusting the export"
        )
    return {
        "provider": args.provider,
        "organization": args.organization,
        "repository": args.repository,
        "branch": args.branch,
        "fetched_at": dt.datetime.now(dt.UTC).isoformat(),
        "source": "codacy-cli",
        "statuses": args.finding_statuses,
        "count": len(findings),
        "data": findings,
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--provider", default="gh")
    parser.add_argument("--organization", default="netdata")
    parser.add_argument("--repository", default="systemd-journal-sdk")
    parser.add_argument("--branch", default="master")
    parser.add_argument("--api-base", default=DEFAULT_API_BASE)
    parser.add_argument("--limit", type=int, default=1000)
    parser.add_argument("--cli-timeout", type=int, default=DEFAULT_CLI_TIMEOUT_SECONDS)
    parser.add_argument("--output-dir", default=".local/codacy")
    parser.add_argument("--source", choices=("cli", "api"), default="cli")
    parser.add_argument("--skip-findings", action="store_true")
    parser.add_argument(
        "--finding-statuses",
        default="Overdue,OnTrack,DueSoon",
        help="comma-separated Codacy security finding statuses for CLI export",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if not (1 <= args.limit <= 1000):
        raise SystemExit("--limit must be between 1 and 1000")
    if args.cli_timeout <= 0:
        raise SystemExit("--cli-timeout must be positive")

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    output_path = output_dir / "codacy-issues.json"

    payload = export_issues_with_cli(args) if args.source == "cli" else export_issues(args)
    output_path.write_text(json.dumps(payload, indent=2, sort_keys=True), encoding="utf-8")
    print(f"wrote {payload['count']} Codacy issues to {output_path}")
    if args.source == "cli" and not args.skip_findings:
        findings_payload = export_findings_with_cli(args)
        findings_path = output_dir / "codacy-findings.json"
        findings_path.write_text(
            json.dumps(findings_payload, indent=2, sort_keys=True),
            encoding="utf-8",
        )
        print(f"wrote {findings_payload['count']} Codacy findings to {findings_path}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except RuntimeError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
