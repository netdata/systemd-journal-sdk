#!/usr/bin/env python3
# SPDX-License-Identifier: MIT-0
"""Netdata-compatible journal function CLI for the Python SDK.

This wrapper is the exact CLI contract used by the comparator
harness (`tests/netdata_function/run_function_compare.py` and
`run_stateful_function_compare.py`). It mirrors the Rust and Go
wrappers byte-for-byte on the wire:

- Required flags: --test <name> (must equal "systemd-journal"),
  --dir <journal-directory>.
- Optional flags: --timeout <seconds> (0 => disabled),
  --progress-jsonl <path>, --cancel-immediately <bool>,
  --cancel-after-progress <N>.
- Stdin: the full JSON request payload as bytes.
- Stdout: a single JSON object followed by a newline, the response
  envelope produced by the plugin-compatible
  `NetdataJournalFunction`. On error, stdout is empty and a
  human-readable message goes to stderr with exit code 1.
- Progress JSONL: when --progress-jsonl is set, one JSON line per
  progress callback is written. The line shape is exactly:

      {"current_file": int, "total_files": int, "matched_files": int,
       "skipped_files": int, "elapsed_seconds": float, "stats": {...}}

The wrapper runs against a caller-supplied directory only. It never
reads from /proc, /etc/machine-id, or any host journal.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from pathlib import Path
from typing import Any, Callable, Optional

HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
PYTHON_JOURNAL = os.path.join(REPO_ROOT, "python")
if PYTHON_JOURNAL not in sys.path:
    sys.path.insert(0, PYTHON_JOURNAL)


FUNCTION_NAME = "systemd-journal"


class ProgressRecorder:
    """Drive the progress / cancellation flags the same way as the
    Rust and Go wrappers. ``cancelled`` is checked by the SDK via
    ``NetdataFunctionRunOptions.cancellation_callback``; on every
    progress report we may flip it when the user passed
    ``--cancel-after-progress N``.
    """

    def __init__(
        self,
        progress_path: Optional[Path],
        cancel_immediately: bool,
        cancel_after_progress: int,
    ) -> None:
        self._cancelled = bool(cancel_immediately)
        self._reports = 0
        self._cancel_after_progress = int(cancel_after_progress)
        self._file = None
        self._write_error: Optional[str] = None
        if progress_path is not None:
            self._file = open(progress_path, "w", encoding="utf-8")

    def close(self) -> None:
        if self._file is not None:
            try:
                self._file.close()
            except Exception:
                pass

    def handle(self, progress: Any) -> None:
        if self._write_error is not None:
            return
        self._reports += 1
        if self._file is not None:
            line = {
                "current_file": int(progress.current_file),
                "total_files": int(progress.total_files),
                "matched_files": int(progress.matched_files),
                "skipped_files": int(progress.skipped_files),
                "elapsed_seconds": float(progress.elapsed),
                "stats": _stats_to_jsonable(progress.stats),
            }
            try:
                self._file.write(json.dumps(line, sort_keys=True))
                self._file.write("\n")
            except Exception as err:
                self._write_error = f"failed to write progress JSON: {err}"
                self._cancelled = True
        if (
            self._cancel_after_progress > 0
            and self._reports >= self._cancel_after_progress
        ):
            self._cancelled = True

    def is_cancelled(self) -> bool:
        return self._cancelled

    def take_write_error(self) -> Optional[str]:
        return self._write_error


def _stats_to_jsonable(stats: Any) -> dict:
    if stats is None:
        return {}
    if hasattr(stats, "__dict__"):
        return {k: v for k, v in vars(stats).items() if not k.startswith("_")}
    if isinstance(stats, dict):
        return dict(stats)
    return {}


def _validate_function_name(name: str) -> None:
    if name != FUNCTION_NAME:
        raise SystemExit(f"unsupported function '{name}'")


def _make_options(recorder: ProgressRecorder, timeout_seconds: int):
    from journal import NetdataFunctionRunOptions

    options = NetdataFunctionRunOptions.from_timeout_seconds(int(timeout_seconds))
    if recorder._file is not None or recorder._cancel_after_progress > 0:
        options.progress_callback = recorder.handle
    if (
        recorder._cancelled
        or recorder._cancel_after_progress > 0
    ):
        options.cancellation_callback = recorder.is_cancelled
    return options


def _read_request_stdin() -> bytes:
    return sys.stdin.buffer.read()


def _main(argv: Optional[list] = None) -> int:
    parser = argparse.ArgumentParser(
        prog="netdata_function_wrapper",
        description=(
            "Run a Netdata-compatible journal function through the "
            "Python systemd-journal SDK."
        ),
    )
    parser.add_argument("--test", required=True, help="function name (must be 'systemd-journal')")
    parser.add_argument("--dir", required=True, help="journal directory to scan")
    parser.add_argument("--timeout", type=int, default=0, help="timeout in seconds; 0 disables it")
    parser.add_argument(
        "--progress-jsonl", default=None, help="optional path to write progress JSONL"
    )
    parser.add_argument(
        "--cancel-immediately",
        type=lambda v: v.lower() in ("1", "true", "yes", "on"),
        default=False,
        help="cancel the request before scanning starts",
    )
    parser.add_argument(
        "--cancel-after-progress",
        type=int,
        default=0,
        help="cancel after N progress callbacks (0 disables it)",
    )
    args = parser.parse_args(argv)

    _validate_function_name(args.test)

    progress_path = Path(args.progress_jsonl) if args.progress_jsonl else None
    recorder = ProgressRecorder(
        progress_path=progress_path,
        cancel_immediately=bool(args.cancel_immediately),
        cancel_after_progress=int(args.cancel_after_progress),
    )
    try:
        request_bytes = _read_request_stdin()

        from journal import NetdataJournalFunction  # noqa: E402

        options = _make_options(recorder, int(args.timeout))
        response = NetdataJournalFunction.systemd_journal_plugin_compatible() \
            .run_directory_request_bytes_with_options(args.dir, request_bytes, options)
        write_error = recorder.take_write_error()
        if write_error is not None:
            sys.stderr.write(f"{write_error}\n")
            return 1
        sys.stdout.write(json.dumps(response, sort_keys=True))
        sys.stdout.write("\n")
        sys.stdout.flush()
        return 0
    except SystemExit:
        raise
    except Exception as err:  # noqa: BLE001 - report failure with exit code 1.
        sys.stderr.write(f"{err}\n")
        return 1
    finally:
        recorder.close()


if __name__ == "__main__":
    raise SystemExit(_main())
