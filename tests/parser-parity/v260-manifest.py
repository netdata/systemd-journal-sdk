#!/usr/bin/env python3
"""Shared v260.1 journalctl parser parity manifest.

Derived from the authoritative option/action list in
`systemd/systemd @ c0a5a2516d28` (`v260.1`)
`src/journal/journalctl.c:420` (long-option table),
`src/shared/output-mode.c:26` (output mode names),
and `src/journal/journalctl.h:7` (JournalctlAction enum).

The manifest lists every official long option, output mode, and action so
Rust and Go parser parity tests can validate against a single shared source
of truth. Tests must fail if either command rejects one of these names with
an "unknown option" error.

Output schema:

```json
{
  "baseline": "systemd v260.1",
  "source_commit": "c0a5a2516d28",
  "long_options": [
    {
      "name": "verify",
      "args": "none",
      "short_alias": null,
      "classification": "file-backed-required",
      "expectation": "recognized-supported"
    }
  ],
  "short_options": [
    {"letter": "h", "long": "help", "args": "none"}
  ],
  "output_modes": ["short", "short-full", ...],
  "actions": ["ACTION_SHOW", ...]
}
```

The `expectation` field is one of:

- `recognized-supported`: parser must accept and behavior must exist.
- `recognized-no-op`: parser must accept and command must succeed (or skip).
- `recognized-unsupported`: parser must accept and command must fail with a
  portable-mode unsupported message that mentions the feature and a reason.
- `parser-required`: parser must accept and command must enforce the related
  interaction rule; outcome depends on the rule.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SPEC = REPO_ROOT / ".agents" / "sow" / "specs" / "journalctl-v260-parity-matrix.md"


def build_long_options():
    """Every official v260.1 long option, with portable expectation."""
    # Tuple format: (long_name, args, classification, expectation, optional_alias_short)
    rows = [
        # Source options
        ("system", "none", "recognized-no-op", "recognized-no-op", None),
        ("user", "none", "recognized-no-op", "recognized-no-op", None),
        ("machine", "string", "recognized-unsupported", "recognized-unsupported", "M"),
        ("merge", "none", "recognized-no-op", "recognized-no-op", "m"),
        ("directory", "path", "file-backed-required", "recognized-supported", "D"),
        ("file", "path", "file-backed-required", "recognized-supported", "i"),
        ("root", "path", "recognized-unsupported", "recognized-unsupported", None),
        ("image", "path", "recognized-unsupported", "recognized-unsupported", None),
        ("image-policy", "policy", "recognized-unsupported", "recognized-unsupported", None),
        ("namespace", "string", "recognized-unsupported", "recognized-unsupported", None),
        # Filtering options
        ("since", "timestamp", "file-backed-required", "recognized-supported", "S"),
        ("until", "timestamp", "file-backed-required", "recognized-supported", "U"),
        ("cursor", "cursor", "file-backed-required", "recognized-supported", "c"),
        ("after-cursor", "cursor", "file-backed-required", "recognized-supported", None),
        ("cursor-file", "path", "file-backed-required", "recognized-supported", None),
        ("boot", "optional descriptor", "file-backed-required", "recognized-supported", "b"),
        ("this-boot", "none", "file-backed-required", "recognized-supported", None),
        ("unit", "unit/glob", "file-backed-required", "recognized-supported", "u"),
        ("user-unit", "unit/glob", "file-backed-required", "recognized-supported", None),
        ("invocation", "descriptor", "file-backed-required", "recognized-supported", None),
        ("identifier", "string", "file-backed-required", "recognized-supported", "t"),
        ("exclude-identifier", "string", "file-backed-required", "recognized-supported", "T"),
        ("priority", "range", "file-backed-required", "recognized-supported", "p"),
        ("facility", "list", "file-backed-required", "recognized-supported", None),
        ("grep", "pattern", "file-backed-required", "recognized-supported", "g"),
        ("case-sensitive", "optional bool", "file-backed-required", "recognized-supported", None),
        ("dmesg", "none", "file-backed-required", "recognized-supported", "k"),
        # Output control options
        ("output", "mode", "file-backed-required", "recognized-supported", "o"),
        ("output-fields", "comma list", "file-backed-required", "recognized-supported", None),
        ("lines", "optional int", "file-backed-required", "recognized-supported", "n"),
        ("reverse", "none", "file-backed-required", "recognized-supported", "r"),
        ("show-cursor", "none", "file-backed-required", "recognized-supported", None),
        ("utc", "none", "file-backed-required", "recognized-supported", None),
        ("catalog", "none", "recognized-no-op", "recognized-no-op", "x"),
        ("no-hostname", "none", "file-backed-required", "recognized-supported", "W"),
        ("no-full", "none", "file-backed-required", "recognized-supported", None),
        ("full", "none", "file-backed-required", "recognized-supported", "l"),
        ("all", "none", "file-backed-required", "recognized-supported", "a"),
        ("follow", "none", "file-backed-required", "recognized-supported", "f"),
        ("no-tail", "none", "file-backed-required", "recognized-supported", None),
        ("truncate-newline", "none", "file-backed-required", "recognized-supported", None),
        ("quiet", "none", "file-backed-required", "recognized-supported", "q"),
        ("synchronize-on-exit", "bool", "recognized-unsupported", "recognized-unsupported", None),
        ("no-pager", "none", "recognized-no-op", "recognized-no-op", None),
        ("pager-end", "none", "file-backed-required", "recognized-supported", "e"),
        # FSS options
        ("verify-key", "key", "file-backed-required", "recognized-supported", None),
        ("interval", "duration", "recognized-no-op", "recognized-no-op", None),
        ("force", "none", "recognized-no-op", "recognized-no-op", None),
        ("setup-keys", "none", "recognized-unsupported", "recognized-unsupported", None),
        # Commands and actions
        ("help", "none", "portable-utility-required", "recognized-supported", "h"),
        ("version", "none", "portable-utility-required", "recognized-supported", None),
        ("new-id128", "none", "portable-utility-required", "recognized-supported", None),
        ("fields", "none", "file-backed-required", "recognized-supported", "N"),
        ("field", "field", "file-backed-required", "recognized-supported", "F"),
        ("list-boots", "none", "file-backed-required", "recognized-supported", None),
        ("list-invocations", "none", "file-backed-required", "recognized-supported", None),
        ("list-namespaces", "none", "recognized-unsupported", "recognized-unsupported", None),
        ("disk-usage", "none", "file-backed-required", "recognized-supported", None),
        ("vacuum-size", "bytes", "file-backed-maintenance-required", "recognized-supported", None),
        ("vacuum-files", "int", "file-backed-maintenance-required", "recognized-supported", None),
        ("vacuum-time", "duration", "file-backed-maintenance-required", "recognized-supported", None),
        ("verify", "none", "file-backed-required", "recognized-supported", None),
        ("sync", "none", "recognized-unsupported", "recognized-unsupported", None),
        ("relinquish-var", "none", "recognized-unsupported", "recognized-unsupported", None),
        ("smart-relinquish-var", "none", "recognized-unsupported", "recognized-unsupported", None),
        ("flush", "none", "recognized-unsupported", "recognized-unsupported", None),
        ("rotate", "none", "recognized-unsupported", "recognized-unsupported", None),
        ("header", "none", "file-backed-required", "recognized-supported", None),
        ("list-catalog", "none", "recognized-unsupported", "recognized-unsupported", None),
        ("dump-catalog", "none", "recognized-unsupported", "recognized-unsupported", None),
        ("update-catalog", "none", "recognized-unsupported", "recognized-unsupported", None),
    ]

    out = []
    for name, args, classification, expectation, short_alias in rows:
        out.append(
            {
                "name": name,
                "args": args,
                "classification": classification,
                "expectation": expectation,
                "short_alias": short_alias,
            }
        )
    return out


def build_short_options():
    """Official short option letters from journalctl.c:520."""
    return [
        {"letter": "h", "long": "help"},
        {"letter": "e", "long": "pager-end"},
        {"letter": "f", "long": "follow"},
        {"letter": "o", "long": "output"},
        {"letter": "a", "long": "all"},
        {"letter": "l", "long": "full"},
        {"letter": "n", "long": "lines"},
        {"letter": "q", "long": "quiet"},
        {"letter": "m", "long": "merge"},
        {"letter": "b", "long": "boot"},
        {"letter": "k", "long": "dmesg"},
        {"letter": "D", "long": "directory"},
        {"letter": "p", "long": "priority"},
        {"letter": "g", "long": "grep"},
        {"letter": "c", "long": "cursor"},
        {"letter": "S", "long": "since"},
        {"letter": "U", "long": "until"},
        {"letter": "t", "long": "identifier"},
        {"letter": "T", "long": "exclude-identifier"},
        {"letter": "u", "long": "unit"},
        {"letter": "I", "long": "invocation"},
        {"letter": "N", "long": "fields"},
        {"letter": "F", "long": "field"},
        {"letter": "x", "long": "catalog"},
        {"letter": "r", "long": "reverse"},
        {"letter": "M", "long": "machine"},
        {"letter": "i", "long": "file"},
        {"letter": "W", "long": "no-hostname"},
    ]


def build_output_modes():
    """Every official v260.1 output mode string."""
    return [
        "short",
        "short-full",
        "short-iso",
        "short-iso-precise",
        "short-precise",
        "short-monotonic",
        "short-delta",
        "short-unix",
        "verbose",
        "export",
        "json",
        "json-pretty",
        "json-sse",
        "json-seq",
        "cat",
        "with-unit",
    ]


def build_actions():
    """Every official v260.1 JournalctlAction enum value."""
    return [
        "ACTION_SHOW",
        "ACTION_NEW_ID128",
        "ACTION_SETUP_KEYS",
        "ACTION_LIST_CATALOG",
        "ACTION_DUMP_CATALOG",
        "ACTION_UPDATE_CATALOG",
        "ACTION_PRINT_HEADER",
        "ACTION_VERIFY",
        "ACTION_DISK_USAGE",
        "ACTION_LIST_BOOTS",
        "ACTION_LIST_FIELDS",
        "ACTION_LIST_FIELD_NAMES",
        "ACTION_LIST_INVOCATIONS",
        "ACTION_LIST_NAMESPACES",
        "ACTION_FLUSH",
        "ACTION_RELINQUISH_VAR",
        "ACTION_SYNC",
        "ACTION_ROTATE",
        "ACTION_VACUUM",
        "ACTION_ROTATE_AND_VACUUM",
    ]


def build_parser_interactions():
    """Parser interaction rules from journalctl.c:1085."""
    return [
        {
            "name": "source-exclusivity",
            "description": (
                "Reject combinations of --directory=, --file=, "
                "-M/--machine=, --root=, --image=."
            ),
        },
        {
            "name": "time-bounds-order",
            "description": "Reject --since= later than --until=.",
        },
        {
            "name": "cursor-source-exclusivity",
            "description": (
                "Reject combinations of --since=, --cursor=, "
                "--cursor-file=, --after-cursor=."
            ),
        },
        {
            "name": "follow-reverse-conflict",
            "description": "Reject --follow with --reverse.",
        },
        {
            "name": "oldest-lines-conflict",
            "description": "Reject --lines=+N with --reverse or --follow.",
        },
        {
            "name": "boot-merge-conflict",
            "description": "Reject --boot or --list-boots with --merge.",
        },
    ]


def main():
    out = {
        "baseline": "systemd v260.1",
        "source_commit": "c0a5a2516d28",
        "long_options": build_long_options(),
        "short_options": build_short_options(),
        "output_modes": build_output_modes(),
        "actions": build_actions(),
        "parser_interactions": build_parser_interactions(),
    }
    json.dump(out, sys.stdout, indent=2, sort_keys=False)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
