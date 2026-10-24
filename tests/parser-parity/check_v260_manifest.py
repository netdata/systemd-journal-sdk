#!/usr/bin/env python3
"""Cross-check the committed manifest against the systemd v262 source.

Reads `.agents/sow/specs/journalctl-v260-parity-matrix.md` and
`tests/parser-parity/v260-manifest.json` and verifies that every official
v262 long option, short option, output mode, and JournalctlAction enum
value is represented in both.

The systemd source reference is read-only. The script is part of the
self-contained SOW-0121 parser parity workflow.
"""

from __future__ import annotations

import json
import os
import re
import subprocess  # nosec B404
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
MANIFEST = REPO_ROOT / "tests" / "parser-parity" / "v260-manifest.json"
SPEC = REPO_ROOT / ".agents" / "sow" / "specs" / "journalctl-v260-parity-matrix.md"
SOURCE_COMMIT = "8cc40e0c5e9234bf45084751ac53b1fbfe70b492"
SYSTEMD_ROOT = Path(os.environ.get("SYSTEMD_SOURCE", "~/src/systemd.git")).expanduser()


def git_show(path: str) -> str:
    """Read a file from the systemd v262 tag in the read-only mirror."""
    result = subprocess.run(  # nosec B603 B607
        ["git", "show", f"{SOURCE_COMMIT}:{path}"],
        cwd=str(SYSTEMD_ROOT),
        capture_output=True,
        text=True,
        check=True,
    )
    return result.stdout


def official_options_source() -> str:
    """Read the regular namespace and expand only its referenced common macros."""
    source = git_show("src/journal/journalctl.c")
    start = source.index('OptionParser opts = { argc, argv, .namespace = "journalctl" };')
    source = source[start:source.index("char **args =", start)]
    common = git_show("src/shared/options.h")
    for macro in set(re.findall(r"\bOPTION_COMMON_[A-Z_]+\b", source)):
        match = re.search(r"^#define " + macro + r"\s+(.*?)(?=^#define|\Z)", common,
                          re.MULTILINE | re.DOTALL)
        if not match:
            raise RuntimeError("missing common option macro: " + macro)
        source = source.replace(macro, match.group(1).replace("\\\n", " "))
    return re.sub(r"/\*.*?\*/", "", source, flags=re.DOTALL)


def official_long_options() -> set[str]:
    source = official_options_source()
    patterns = [r'\bOPTION\(\s*\'[^\']\'\s*,\s*"([^"]+)"',
                r'\bOPTION_FULL\([^,]+,\s*\'[^\']\'\s*,\s*"([^"]+)"',
                r'\bOPTION_LONG\(\s*"([^"]+)"',
                r'\bOPTION_LONG_FLAGS\([^,]+,\s*"([^"]+)"']
    return {name for pattern in patterns for name in re.findall(pattern, source)}


def official_short_options() -> set[str]:
    source = official_options_source()
    patterns = [r"\bOPTION(?:_SHORT)?\(\s*'([^'])'",
                r"\bOPTION_FULL\([^,]+,\s*'([^'])'"]
    return {letter for pattern in patterns for letter in re.findall(pattern, source)}


def official_output_modes() -> set[str]:
    """Extract `output_mode_table` entries from output-mode.c:26."""
    source = git_show("src/shared/output-mode.c")
    out: set[str] = set()
    for match in re.finditer(r'\[OUTPUT_([A-Z_]+)\]\s*=\s*"([^"]+)"', source):
        out.add(match.group(2))
    return out


def official_actions() -> set[str]:
    """Extract ACTION_* enum members from journalctl.h:7."""
    source = git_show("src/journal/journalctl.h")
    out: set[str] = set()
    # Match indented `ACTION_NAME,` rows inside the JournalctlAction enum block.
    # Names may include digits (e.g. ACTION_NEW_ID128).
    for match in re.finditer(r"^\s*(ACTION_[A-Z0-9_]+)\s*,", source, re.MULTILINE):
        out.add(match.group(1))
    return out


def main():
    with MANIFEST.open() as fh:
        manifest = json.load(fh)

    long_official = official_long_options()
    long_manifest = {opt["name"] for opt in manifest["long_options"]}

    short_official = official_short_options()
    short_manifest = {opt["letter"] for opt in manifest["short_options"]}

    mode_official = official_output_modes()
    mode_manifest = set(manifest["output_modes"])

    action_official = official_actions()
    action_manifest = set(manifest["actions"])

    print(f"Official long options ({len(long_official)}):")
    for name in sorted(long_official):
        marker = "OK" if name in long_manifest else "MISSING"
        print(f"  [{marker}] {name}")

    print(f"Official short option letters ({len(short_official)}):")
    for ch in sorted(short_official):
        marker = "OK" if ch in short_manifest else "MISSING"
        print(f"  [{marker}] -{ch}")

    print(f"Official output modes ({len(mode_official)}):")
    for name in sorted(mode_official):
        marker = "OK" if name in mode_manifest else "MISSING"
        print(f"  [{marker}] {name}")

    print(f"Official actions ({len(action_official)}):")
    for name in sorted(action_official):
        marker = "OK" if name in action_manifest else "MISSING"
        print(f"  [{marker}] {name}")

    missing_long = long_official - long_manifest
    extra_long = long_manifest - long_official
    missing_short = short_official - short_manifest
    extra_short = short_manifest - short_official
    missing_modes = mode_official - mode_manifest
    extra_modes = mode_manifest - mode_official
    missing_actions = action_official - action_manifest
    extra_actions = action_manifest - action_official

    failures = []
    if missing_long:
        failures.append(f"manifest missing long options: {sorted(missing_long)}")
    if extra_long:
        failures.append(f"manifest has non-official long options: {sorted(extra_long)}")
    if missing_short:
        failures.append(f"manifest missing short letters: {sorted(missing_short)}")
    if extra_short:
        failures.append(f"manifest has non-official short letters: {sorted(extra_short)}")
    if missing_modes:
        failures.append(f"manifest missing output modes: {sorted(missing_modes)}")
    if extra_modes:
        failures.append(f"manifest has non-official output modes: {sorted(extra_modes)}")
    if missing_actions:
        failures.append(f"manifest missing actions: {sorted(missing_actions)}")
    if extra_actions:
        failures.append(f"manifest has non-official actions: {sorted(extra_actions)}")

    if failures:
        print("FAIL")
        for line in failures:
            print(f"  - {line}")
        sys.exit(1)
    print("OK: manifest matches systemd v262 official surface")


if __name__ == "__main__":
    main()
