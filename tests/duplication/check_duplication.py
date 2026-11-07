#!/usr/bin/env python3
"""Fail when production Go or Rust code gains duplicated blocks.

Runs a pinned jscpd over go/ and rust/, groups production-to-production
clones by file pair, and compares them with the committed baseline. A new
pair, or a pair whose duplicated line count grew, fails the check. Pairs are
compared instead of line ranges so unrelated edits that shift code do not
fail it. Test and harness duplication is reported, never enforced.
"""

from __future__ import annotations

import argparse
import json
import re
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BASELINE = Path(__file__).resolve().parent / "baseline.json"
JSCPD_VERSION = "5.3.1"
MIN_TOKENS = 70
LANGUAGES = {"go": "go", "rust": "rust"}
IGNORE = "**/target/**,**/testdata/**,**/.local/**,**/node_modules/**"

TEST_PATTERNS = (
    re.compile(r"_test\.go$"),
    re.compile(r"(^|/)tests?/"),
    re.compile(r"(^|/)tests?\.rs$"),
    re.compile(r"(^|/)internal/testcmd/"),
    re.compile(r"(^|/)benches/"),
    re.compile(r"(^|/)examples/"),
)


def is_test_path(path: str) -> bool:
    return any(pattern.search(path) for pattern in TEST_PATTERNS)


def pair_key(first: str, second: str) -> str:
    low, high = sorted((first, second))
    return f"{low} | {high}"


def group_clones(clones: list[dict]) -> tuple[dict[str, int], int]:
    """Returns production pair line counts and total test-involved lines."""
    production: dict[str, int] = {}
    test_lines = 0
    for clone in clones:
        first, second, lines = clone["first"], clone["second"], clone["lines"]
        if is_test_path(first) or is_test_path(second):
            test_lines += lines
            continue
        key = pair_key(first, second)
        production[key] = production.get(key, 0) + lines
    return production, test_lines


def compare(baseline: dict[str, int], current: dict[str, int]) -> tuple[list[str], list[str]]:
    """Returns (failures, improvements) as human-readable lines."""
    failures = []
    improvements = []
    for key, lines in sorted(current.items()):
        allowed = baseline.get(key)
        if allowed is None:
            failures.append(f"new duplication ({lines} lines): {key}")
        elif lines > allowed:
            failures.append(f"duplication grew {allowed} -> {lines} lines: {key}")
        elif lines < allowed:
            improvements.append(f"duplication shrank {allowed} -> {lines} lines: {key}")
    for key, allowed in sorted(baseline.items()):
        if key not in current:
            improvements.append(f"duplication removed ({allowed} lines): {key}")
    return failures, improvements


def run_jscpd(command: list[str]) -> list[dict]:
    version = subprocess.run(
        [*command, "--version"], check=True, capture_output=True, text=True
    ).stdout.strip().removeprefix("jscpd ").strip()
    if version != JSCPD_VERSION:
        raise SystemExit(
            f"jscpd {JSCPD_VERSION} is required (results differ between versions), found {version!r}"
        )
    clones: list[dict] = []
    with tempfile.TemporaryDirectory(prefix="dup-scan-") as tmp:
        for prefix, fmt in LANGUAGES.items():
            out = Path(tmp) / prefix
            subprocess.run(
                [
                    *command,
                    "--silent",
                    "--min-tokens",
                    str(MIN_TOKENS),
                    "--format",
                    fmt,
                    "--ignore",
                    IGNORE,
                    "--reporters",
                    "json",
                    "--output",
                    str(out),
                    str(ROOT / prefix),
                ],
                check=True,
                capture_output=True,
                cwd=ROOT,
            )
            report = json.loads((out / "jscpd-report.json").read_text())
            for duplicate in report["duplicates"]:
                clones.append(
                    {
                        "first": normalize(prefix, duplicate["firstFile"]["name"]),
                        "second": normalize(prefix, duplicate["secondFile"]["name"]),
                        "lines": duplicate["lines"],
                    }
                )
    return clones


def normalize(prefix: str, name: str) -> str:
    path = Path(name)
    if path.is_absolute():
        return path.relative_to(ROOT).as_posix()
    if path.parts and path.parts[0] == prefix:
        return path.as_posix()
    return f"{prefix}/{path.as_posix()}"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--jscpd",
        default="jscpd",
        help="jscpd command, for example 'npm exec --yes --package jscpd@5.3.1 -- jscpd'",
    )
    parser.add_argument(
        "--update", action="store_true", help="rewrite the baseline from the current tree"
    )
    args = parser.parse_args()

    production, test_lines = group_clones(run_jscpd(shlex.split(args.jscpd)))
    total = sum(production.values())

    if args.update:
        BASELINE.write_text(
            json.dumps(
                {
                    "jscpd": JSCPD_VERSION,
                    "min_tokens": MIN_TOKENS,
                    "production_pairs": dict(sorted(production.items())),
                },
                indent=2,
            )
            + "\n"
        )
        print(f"baseline updated: {len(production)} pairs, {total} production lines")
        return 0

    baseline = json.loads(BASELINE.read_text())
    if baseline.get("jscpd") != JSCPD_VERSION or baseline.get("min_tokens") != MIN_TOKENS:
        raise SystemExit("baseline was produced with different jscpd settings; rerun with --update")
    failures, improvements = compare(baseline["production_pairs"], production)
    print(f"production duplication: {total} lines in {len(production)} file pairs")
    print(f"test and harness duplication (not enforced): {test_lines} lines")
    for line in improvements:
        print(f"  improved: {line}")
    if improvements:
        print("  run with --update to lock in the improvement")
    if failures:
        print("FAIL: production code gained duplicated blocks.")
        print("Move the shared behavior to one owner instead of copying it (AGENTS.md).")
        for line in failures:
            print(f"  {line}")
        return 1
    print("OK: no new production duplication")
    return 0


if __name__ == "__main__":
    sys.exit(main())
