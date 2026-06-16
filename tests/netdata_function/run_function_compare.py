#!/usr/bin/env python3
"""Run SDK/plugin Netdata function comparisons with sanitized timing reports."""

from __future__ import annotations

import argparse
import hashlib
import json
# Harness runs explicit SDK/plugin binaries supplied by the operator.
import subprocess  # nosec B404
import time
from pathlib import Path
from typing import Any

from compare_function_json import compare


def parse_stdout_json(stdout: bytes) -> tuple[Any | None, str | None, int]:
    text = stdout.decode("utf-8")
    start = min(
        [idx for idx in (text.find("{"), text.find("[")) if idx >= 0],
        default=-1,
    )
    if start < 0:
        return None, "no JSON object or array found in stdout", len(stdout)
    try:
        return json.loads(text[start:]), None, len(text[:start].encode("utf-8"))
    except Exception as err:  # noqa: BLE001 - report parse failure class.
        return None, str(err), len(text[:start].encode("utf-8"))


def build_command(
    binary: Path,
    function: str,
    directory: Path,
    timeout_seconds: int,
) -> list[str]:
    return [
        str(binary),
        "--test",
        function,
        "--dir",
        str(directory),
        "--timeout",
        str(timeout_seconds),
    ]


def run_command(
    binary: Path,
    function: str,
    directory: Path,
    request_payload: bytes,
    timeout_seconds: int,
    process_timeout_seconds: int,
) -> dict[str, Any]:
    command = build_command(
        binary,
        function,
        directory,
        timeout_seconds,
    )
    started = time.perf_counter()
    timed_out = False
    try:
        # Command is an argv list; shell is never used.
        # nosemgrep: python.lang.security.audit.dangerous-subprocess-use-tainted-env-args.dangerous-subprocess-use-tainted-env-args
        completed = subprocess.run(  # nosec B603
            command,
            check=False,
            input=request_payload,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=process_timeout_seconds,
        )
        stdout = completed.stdout
        stderr = completed.stderr
        exit_code = completed.returncode
    except subprocess.TimeoutExpired as err:
        timed_out = True
        stdout = err.stdout or b""
        stderr = err.stderr or b""
        exit_code = -1
    elapsed = time.perf_counter() - started
    parsed = None
    parse_error = None
    json_prefix_bytes = 0
    if stdout:
        parsed, parse_error, json_prefix_bytes = parse_stdout_json(stdout)
    return {
        "command_hash": hashlib.sha256("\0".join(command).encode()).hexdigest(),
        "exit_code": exit_code,
        "timed_out": timed_out,
        "process_timeout_seconds": process_timeout_seconds,
        "wall_seconds": elapsed,
        "stdin_bytes": len(request_payload),
        "stdout_bytes": len(stdout),
        "stderr_bytes": len(stderr),
        "json_prefix_bytes": json_prefix_bytes,
        "json": parsed,
        "json_parse_error": parse_error,
    }


def run_case(
    sdk: Path,
    plugin: Path,
    function: str,
    directory: Path,
    request: Path,
    repetitions: int,
    timeout_seconds: int,
    process_timeout_seconds: int,
    save_json_dir: Path | None,
) -> dict[str, Any]:
    runs = []
    request_payload = request.read_bytes()
    for repetition in range(repetitions):
        sdk_run = run_command(
            sdk,
            function,
            directory,
            request_payload,
            timeout_seconds,
            process_timeout_seconds,
        )
        plugin_run = run_command(
            plugin,
            function,
            directory,
            request_payload,
            timeout_seconds,
            process_timeout_seconds,
        )
        comparison = {
            "ok": False,
            "checks": {},
            "reason": "one or both commands did not return JSON",
        }
        if isinstance(plugin_run["json"], dict) and isinstance(sdk_run["json"], dict):
            comparison = compare(plugin_run["json"], sdk_run["json"])
        if save_json_dir is not None:
            save_json_dir.mkdir(parents=True, exist_ok=True)
            case_name = request.stem
            if isinstance(sdk_run["json"], dict):
                (save_json_dir / f"{case_name}-run{repetition + 1}-sdk.json").write_text(
                    json.dumps(sdk_run["json"], indent=2, sort_keys=True) + "\n",
                    encoding="utf-8",
                )
            if isinstance(plugin_run["json"], dict):
                (save_json_dir / f"{case_name}-run{repetition + 1}-plugin.json").write_text(
                    json.dumps(plugin_run["json"], indent=2, sort_keys=True) + "\n",
                    encoding="utf-8",
                )
        plugin_report = {k: v for k, v in plugin_run.items() if k != "json"}
        sdk_report = {k: v for k, v in sdk_run.items() if k != "json"}
        run_entry: dict[str, Any] = {
            "plugin": plugin_report,
            "sdk": sdk_report,
            "comparison": comparison,
        }
        runs.append(run_entry)
    return {
        "request": str(request),
        "request_sha256": hashlib.sha256(request_payload).hexdigest(),
        "runs": runs,
        "ok": all(run["comparison"].get("ok") for run in runs),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--plugin", type=Path, required=True)
    parser.add_argument("--function", default="systemd-journal")
    parser.add_argument("--dir", type=Path, required=True)
    parser.add_argument("--request", type=Path, action="append", required=True)
    parser.add_argument("--repetitions", type=int, default=1)
    parser.add_argument("--timeout", type=int, default=0)
    parser.add_argument(
        "--process-timeout",
        type=int,
        default=3600,
        help="Subprocess wall-clock timeout in seconds; independent from the function --timeout value.",
    )
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--save-json-dir", type=Path)
    args = parser.parse_args()

    report = {
        "function": args.function,
        "directory": str(args.dir),
        "directory_name": args.dir.name,
        "repetitions": args.repetitions,
        "cases": [
            run_case(
                args.sdk,
                args.plugin,
                args.function,
                args.dir,
                request,
                args.repetitions,
                args.timeout,
                args.process_timeout,
                args.save_json_dir,
            )
            for request in args.request
        ],
    }
    report["ok"] = all(case["ok"] for case in report["cases"])
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
