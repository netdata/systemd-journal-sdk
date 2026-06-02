#!/usr/bin/env python3
"""Export Codacy cloud issues into `.local/` for offline triage."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any


DEFAULT_API_BASE = "https://app.codacy.com/api/v3"


def _request_json(url: str, token: str, body: dict[str, Any]) -> dict[str, Any]:
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
        with urllib.request.urlopen(req, timeout=60) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", errors="replace")
        raise RuntimeError(
            f"Codacy API request failed with HTTP {error.code}: {detail[:500]}"
        ) from error


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


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--provider", default="gh")
    parser.add_argument("--organization", default="netdata")
    parser.add_argument("--repository", default="systemd-journal-sdk")
    parser.add_argument("--branch", default="master")
    parser.add_argument("--api-base", default=DEFAULT_API_BASE)
    parser.add_argument("--limit", type=int, default=1000)
    parser.add_argument("--output-dir", default=".local/codacy")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if not (1 <= args.limit <= 1000):
        raise SystemExit("--limit must be between 1 and 1000")

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    output_path = output_dir / "codacy-issues.json"

    payload = export_issues(args)
    output_path.write_text(json.dumps(payload, indent=2, sort_keys=True), encoding="utf-8")
    print(f"wrote {payload['count']} Codacy issues to {output_path}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except RuntimeError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
