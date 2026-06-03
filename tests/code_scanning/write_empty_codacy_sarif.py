#!/usr/bin/env python3
"""Write an empty SARIF file for the Codacy tools used by this repository."""

from __future__ import annotations

import json
import sys
from pathlib import Path


CODACY_TOOL_NAMES = (
    "Agentlinter",
    "Bandit",
    "ESLint8",
    "Flawfinder",
    "PMD",
    "Prospector",
    "PyLintPython3",
    "lizard",
    "markdownlint",
    "shellcheck",
)


def build_empty_sarif() -> dict[str, object]:
    return {
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [
            {
                "tool": {"driver": {"name": tool, "rules": []}},
                "results": [],
            }
            for tool in CODACY_TOOL_NAMES
        ],
    }


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: write_empty_codacy_sarif.py OUTPUT", file=sys.stderr)
        return 2

    output = Path(argv[1])
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(build_empty_sarif()), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
