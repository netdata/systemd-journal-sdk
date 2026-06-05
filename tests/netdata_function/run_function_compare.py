#!/usr/bin/env python3
"""Run SDK/plugin Netdata function comparisons with sanitized timing reports."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
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


def run_command(
    binary: Path,
    function: str,
    directory: Path,
    request: Path,
    timeout_seconds: int,
) -> dict[str, Any]:
    command = [
        str(binary),
        "--test",
        function,
        "--dir",
        str(directory),
        "--request",
        str(request),
        "--timeout",
        str(timeout_seconds),
    ]
    started = time.perf_counter()
    completed = subprocess.run(
        command,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    elapsed = time.perf_counter() - started
    stdout = completed.stdout
    stderr = completed.stderr
    parsed = None
    parse_error = None
    json_prefix_bytes = 0
    if completed.returncode == 0:
        parsed, parse_error, json_prefix_bytes = parse_stdout_json(stdout)
    return {
        "command_hash": hashlib.sha256("\0".join(command).encode()).hexdigest(),
        "exit_code": completed.returncode,
        "wall_seconds": elapsed,
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
) -> dict[str, Any]:
    runs = []
    for _ in range(repetitions):
        plugin_run = run_command(plugin, function, directory, request, timeout_seconds)
        sdk_run = run_command(sdk, function, directory, request, timeout_seconds)
        comparison = {
            "ok": False,
            "checks": {},
            "reason": "one or both commands did not return JSON",
        }
        if isinstance(plugin_run["json"], dict) and isinstance(sdk_run["json"], dict):
            comparison = compare(plugin_run["json"], sdk_run["json"])
        plugin_report = {k: v for k, v in plugin_run.items() if k != "json"}
        sdk_report = {k: v for k, v in sdk_run.items() if k != "json"}
        runs.append(
            {
                "plugin": plugin_report,
                "sdk": sdk_report,
                "comparison": comparison,
            }
        )
    return {
        "request": str(request),
        "request_sha256": hashlib.sha256(request.read_bytes()).hexdigest(),
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
    parser.add_argument("--out", type=Path, required=True)
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
