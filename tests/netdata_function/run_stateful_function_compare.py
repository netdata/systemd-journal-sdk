#!/usr/bin/env python3
"""Run stateful SDK/plugin Netdata function comparisons.

The one-shot comparator proves a single request. This runner proves UI-shaped
state transitions where the next request depends on the previous response.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from compare_function_json import compare
from run_function_compare import run_command

# Netdata treats after=0 as a relative UI window, not as the Unix epoch. Use a
# positive absolute second so old synthetic fixtures stay inside the request.
DEFAULT_AFTER_SECONDS = 1
DEFAULT_BEFORE_SECONDS = 4_102_444_800


@dataclass(frozen=True)
class CommandConfig:
    sdk: Path
    plugin: Path
    function: str
    directory: Path
    timeout_seconds: int
    process_timeout_seconds: int
    save_json_dir: Path | None
    python: Path | None = None
    python_interpreter: Path | None = None


@dataclass
class SequenceState:
    anchors: dict[str, int]
    seen_rows: set[tuple[int, str]]
    collected_rows: list[tuple[int, str]]

    @classmethod
    def empty(cls) -> "SequenceState":
        return cls(anchors={}, seen_rows=set(), collected_rows=[])


def base_request() -> dict[str, Any]:
    return {
        "after": DEFAULT_AFTER_SECONDS,
        "before": DEFAULT_BEFORE_SECONDS,
        "slice": True,
    }


def data_only_request(direction: str, last: int) -> dict[str, Any]:
    request = base_request()
    request.update(
        {
            "last": last,
            "direction": direction,
            "data_only": True,
            "facets": ["PRIORITY"],
            "histogram": "PRIORITY",
        }
    )
    return request


def tail_request(anchor: int, *, delta: bool = False) -> dict[str, Any]:
    request = data_only_request("backward", 20)
    request.update(
        {
            "anchor": anchor,
            "if_modified_since": anchor,
            "tail": True,
        }
    )
    if delta:
        request.update(
            {
                "delta": True,
                "facets": ["PRIORITY", "SYSLOG_IDENTIFIER"],
                "histogram": "PRIORITY",
            }
        )
    return request


def filtered_tail_request(anchor: int) -> dict[str, Any]:
    request = tail_request(anchor)
    request["selections"] = {"PRIORITY": ["3"]}
    return request


def column_index(doc: dict[str, Any], column: str) -> int | None:
    columns = doc.get("columns")
    if not isinstance(columns, dict):
        return None
    meta = columns.get(column)
    if not isinstance(meta, dict):
        return None
    index = meta.get("index")
    return index if isinstance(index, int) else None


def response_rows(doc: dict[str, Any]) -> list[dict[str, Any]]:
    timestamp_index = column_index(doc, "timestamp")
    message_index = column_index(doc, "MESSAGE")
    rows = doc.get("data")
    if timestamp_index is None or not isinstance(rows, list):
        return []
    out: list[dict[str, Any]] = []
    for raw in rows:
        if not isinstance(raw, list) or timestamp_index >= len(raw):
            continue
        timestamp = raw[timestamp_index]
        if not isinstance(timestamp, int):
            continue
        message = ""
        if message_index is not None and message_index < len(raw):
            value = raw[message_index]
            if isinstance(value, str):
                message = value
        out.append({"timestamp": timestamp, "message": message})
    return out


def response_anchor(doc: dict[str, Any], mode: str) -> int:
    rows = response_rows(doc)
    if not rows:
        raise ValueError(f"cannot derive {mode} anchor from response with no rows")
    timestamps = [int(row["timestamp"]) for row in rows]
    if mode == "first":
        return timestamps[0]
    if mode == "last":
        return timestamps[-1]
    if mode == "max":
        return max(timestamps)
    raise ValueError(f"unknown anchor mode: {mode}")


def assert_no_duplicate_rows(
    sequence: str, step: str, state: SequenceState, doc: dict[str, Any]
) -> None:
    for row in response_rows(doc):
        key = (int(row["timestamp"]), str(row["message"]))
        if key in state.seen_rows:
            raise AssertionError(
                f"{sequence}:{step}: duplicate returned row timestamp={key[0]} message={key[1]!r}"
            )
        state.seen_rows.add(key)
        state.collected_rows.append(key)


def assert_tail_rows_newer(sequence: str, step: str, anchor: int, doc: dict[str, Any]) -> None:
    stale = [row for row in response_rows(doc) if int(row["timestamp"]) <= anchor]
    if stale:
        first = stale[0]
        raise AssertionError(
            f"{sequence}:{step}: tail row timestamp={first['timestamp']} is not newer than anchor={anchor}"
        )


def request_digest(request: dict[str, Any]) -> str:
    payload = json.dumps(request, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def run_pair(
    config: CommandConfig,
    sequence: str,
    step: str,
    request: dict[str, Any],
) -> dict[str, Any]:
    payload = json.dumps(request, sort_keys=True).encode("utf-8") + b"\n"
    sdk_run = run_command(
        config.sdk,
        config.function,
        config.directory,
        payload,
        config.timeout_seconds,
        config.process_timeout_seconds,
    )
    plugin_run = run_command(
        config.plugin,
        config.function,
        config.directory,
        payload,
        config.timeout_seconds,
        config.process_timeout_seconds,
    )
    python_run: dict[str, Any] | None = None
    if config.python is not None:
        python_run = run_command(
            config.python,
            config.function,
            config.directory,
            payload,
            config.timeout_seconds,
            config.process_timeout_seconds,
            python_interpreter=config.python_interpreter,
        )
    comparison = {
        "ok": False,
        "reason": "one or both commands did not return JSON",
        "checks": {},
    }
    if isinstance(plugin_run["json"], dict) and isinstance(sdk_run["json"], dict):
        comparison = compare(plugin_run["json"], sdk_run["json"])
    if python_run is not None:
        python_vs_sdk = (
            compare(python_run["json"], sdk_run["json"])
            if isinstance(python_run["json"], dict) and isinstance(sdk_run["json"], dict)
            else {"ok": False, "checks": {}, "reason": "python or sdk missing JSON"}
        )
        python_vs_plugin = (
            compare(python_run["json"], plugin_run["json"])
            if isinstance(python_run["json"], dict) and isinstance(plugin_run["json"], dict)
            else {"ok": False, "checks": {}, "reason": "python or plugin missing JSON"}
        )
        comparison = {
            **comparison,
            "python_vs_sdk": python_vs_sdk,
            "python_vs_plugin": python_vs_plugin,
            "ok": (
                comparison.get("ok", False)
                and python_vs_sdk.get("ok", False)
                and python_vs_plugin.get("ok", False)
            ),
        }
    if config.save_json_dir is not None:
        config.save_json_dir.mkdir(parents=True, exist_ok=True)
        prefix = f"{sequence}-{step}"
        if isinstance(sdk_run["json"], dict):
            (config.save_json_dir / f"{prefix}-sdk.json").write_text(
                json.dumps(sdk_run["json"], indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
        if isinstance(plugin_run["json"], dict):
            (config.save_json_dir / f"{prefix}-plugin.json").write_text(
                json.dumps(plugin_run["json"], indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
        if python_run is not None and isinstance(python_run["json"], dict):
            (config.save_json_dir / f"{prefix}-python.json").write_text(
                json.dumps(python_run["json"], indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
    out: dict[str, Any] = {
        "step": step,
        "request_sha256": request_digest(request),
        "sdk": {key: value for key, value in sdk_run.items() if key != "json"},
        "plugin": {key: value for key, value in plugin_run.items() if key != "json"},
        "comparison": comparison,
        "sdk_json": sdk_run["json"],
        "plugin_json": plugin_run["json"],
    }
    if python_run is not None:
        out["python"] = {key: value for key, value in python_run.items() if key != "json"}
        out["python_json"] = python_run["json"]
    return out


def clean_step_report(step: dict[str, Any]) -> dict[str, Any]:
    return {
        key: value
        for key, value in step.items()
        if key not in {"sdk_json", "plugin_json", "python_json"}
    }


def validate_step_json(sequence: str, step: str, result: dict[str, Any]) -> tuple[dict[str, Any], dict[str, Any]]:
    sdk_json = result.get("sdk_json")
    plugin_json = result.get("plugin_json")
    if not isinstance(sdk_json, dict) or not isinstance(plugin_json, dict):
        raise AssertionError(f"{sequence}:{step}: SDK or plugin did not return JSON object")
    if not result["comparison"].get("ok"):
        raise AssertionError(f"{sequence}:{step}: content comparison failed")
    return sdk_json, plugin_json


def run_paging_sequence(
    config: CommandConfig,
    direction: str,
    *,
    page_size: int,
    expected_rows: int | None,
    max_pages: int,
) -> dict[str, Any]:
    sequence = f"paging-{direction}"
    state = SequenceState.empty()
    steps = []
    anchor: int | None = None
    for page in range(max_pages):
        request = data_only_request(direction, page_size)
        if anchor is not None:
            request["anchor"] = anchor
        step_name = f"page-{page + 1}"
        result = run_pair(config, sequence, step_name, request)
        sdk_json, plugin_json = validate_step_json(sequence, step_name, result)
        assert_no_duplicate_rows(sequence, step_name, state, sdk_json)
        sdk_rows = response_rows(sdk_json)
        plugin_rows = response_rows(plugin_json)
        if len(sdk_rows) != len(plugin_rows):
            raise AssertionError(f"{sequence}:{step_name}: SDK/plugin row counts differ after comparison")
        steps.append(clean_step_report(result))
        if not sdk_rows or len(sdk_rows) < page_size:
            break
        anchor = response_anchor(sdk_json, "last" if direction == "backward" else "first")
    if expected_rows is not None and len(state.collected_rows) != expected_rows:
        raise AssertionError(
            f"{sequence}: collected {len(state.collected_rows)} rows, want {expected_rows}"
        )
    return {
        "sequence": sequence,
        "steps": steps,
        "collected_rows": len(state.collected_rows),
        "unique_rows": len(state.seen_rows),
        "ok": True,
    }


def run_tail_positive_sequence(config: CommandConfig) -> dict[str, Any]:
    sequence = "tail-newer-then-304"
    steps = []
    seed = run_pair(config, sequence, "seed-forward-page", data_only_request("forward", 6))
    sdk_seed, _plugin_seed = validate_step_json(sequence, "seed-forward-page", seed)
    steps.append(clean_step_report(seed))
    anchor = response_anchor(sdk_seed, "max")

    tail = run_pair(config, sequence, "tail-newer", tail_request(anchor))
    steps.append(clean_step_report(tail))
    sdk_tail, _plugin_tail = validate_step_json(sequence, "tail-newer", tail)
    assert_tail_rows_newer(sequence, "tail-newer", anchor, sdk_tail)
    tail_rows = response_rows(sdk_tail)
    if not tail_rows:
        raise AssertionError(f"{sequence}: positive tail step returned no rows")

    no_change_anchor = response_anchor(sdk_tail, "max")
    no_change = run_pair(config, sequence, "tail-no-change", tail_request(no_change_anchor))
    steps.append(clean_step_report(no_change))
    sdk_no_change, _plugin_no_change = validate_step_json(sequence, "tail-no-change", no_change)
    if sdk_no_change.get("status") != 304:
        raise AssertionError(f"{sequence}: no-change status={sdk_no_change.get('status')}, want 304")
    return {"sequence": sequence, "anchor": anchor, "steps": steps, "ok": True}


def run_filtered_tail_no_change_sequence(config: CommandConfig) -> dict[str, Any]:
    sequence = "tail-filtered-no-change"
    steps = []
    seed = run_pair(config, sequence, "seed-forward-page", data_only_request("forward", 6))
    sdk_seed, _plugin_seed = validate_step_json(sequence, "seed-forward-page", seed)
    steps.append(clean_step_report(seed))
    anchor = response_anchor(sdk_seed, "max")

    filtered = run_pair(config, sequence, "tail-filtered-no-change", filtered_tail_request(anchor))
    steps.append(clean_step_report(filtered))
    sdk_filtered, _plugin_filtered = validate_step_json(sequence, "tail-filtered-no-change", filtered)
    if sdk_filtered.get("status") != 200:
        raise AssertionError(
            f"{sequence}: filtered no-change status={sdk_filtered.get('status')}, want 200"
        )
    if response_rows(sdk_filtered):
        raise AssertionError(f"{sequence}: filtered no-change returned rows, want empty data")
    return {"sequence": sequence, "anchor": anchor, "steps": steps, "ok": True}


def run_tail_delta_sequence(config: CommandConfig) -> dict[str, Any]:
    sequence = "tail-delta"
    steps = []
    seed = run_pair(config, sequence, "seed-forward-page", data_only_request("forward", 6))
    sdk_seed, _plugin_seed = validate_step_json(sequence, "seed-forward-page", seed)
    steps.append(clean_step_report(seed))
    anchor = response_anchor(sdk_seed, "max")

    delta = run_pair(config, sequence, "tail-delta", tail_request(anchor, delta=True))
    steps.append(clean_step_report(delta))
    sdk_delta, _plugin_delta = validate_step_json(sequence, "tail-delta", delta)
    assert_tail_rows_newer(sequence, "tail-delta", anchor, sdk_delta)
    for key in ("facets_delta", "histogram_delta", "items_delta"):
        if key not in sdk_delta:
            raise AssertionError(f"{sequence}: SDK response missing {key}")
    return {"sequence": sequence, "anchor": anchor, "steps": steps, "ok": True}


def selected_sequences(names: list[str]) -> list[str]:
    if not names or "all" in names:
        return [
            "paging-backward",
            "paging-forward",
            "tail-newer-then-304",
            "tail-filtered-no-change",
            "tail-delta",
        ]
    return names


def run_sequence(
    name: str,
    config: CommandConfig,
    expected_paging_rows: int | None,
    page_size: int,
    max_pages: int,
) -> dict[str, Any]:
    if name == "paging-backward":
        return run_paging_sequence(
            config,
            "backward",
            page_size=page_size,
            expected_rows=expected_paging_rows,
            max_pages=max_pages,
        )
    if name == "paging-forward":
        return run_paging_sequence(
            config,
            "forward",
            page_size=page_size,
            expected_rows=expected_paging_rows,
            max_pages=max_pages,
        )
    if name == "tail-newer-then-304":
        return run_tail_positive_sequence(config)
    if name == "tail-filtered-no-change":
        return run_filtered_tail_no_change_sequence(config)
    if name == "tail-delta":
        return run_tail_delta_sequence(config)
    raise ValueError(f"unknown sequence: {name}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--plugin", type=Path, required=True)
    parser.add_argument(
        "--python",
        type=Path,
        default=None,
        help=(
            "Optional third peer: path to the Python Netdata function "
            "wrapper script (e.g. python/cmd/netdata_function_wrapper.py). "
            "When provided, the runner invokes it as "
            "`<interpreter> <python> ...` and compares the response "
            "against the SDK and plugin peers."
        ),
    )
    parser.add_argument(
        "--python-interpreter",
        type=Path,
        default=Path(sys.executable),
        help="Python interpreter used to run --python (default: current interpreter).",
    )
    parser.add_argument("--function", default="systemd-journal")
    parser.add_argument("--dir", type=Path, required=True)
    parser.add_argument("--sequence", action="append", default=[])
    parser.add_argument("--expected-paging-rows", type=int)
    parser.add_argument("--page-size", type=int, default=5)
    parser.add_argument("--max-pages", type=int, default=20)
    parser.add_argument("--timeout", type=int, default=0)
    parser.add_argument("--process-timeout", type=int, default=3600)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--save-json-dir", type=Path)
    args = parser.parse_args()

    config = CommandConfig(
        sdk=args.sdk,
        plugin=args.plugin,
        function=args.function,
        directory=args.dir,
        timeout_seconds=args.timeout,
        process_timeout_seconds=args.process_timeout,
        save_json_dir=args.save_json_dir,
        python=args.python,
        python_interpreter=args.python_interpreter,
    )
    report: dict[str, Any] = {
        "function": args.function,
        "directory": str(args.dir),
        "directory_name": args.dir.name,
        "page_size": args.page_size,
        "expected_paging_rows": args.expected_paging_rows,
        "python_peer": (
            {"wrapper": str(args.python), "interpreter": str(args.python_interpreter)}
            if args.python is not None
            else None
        ),
        "sequences": [],
    }
    ok = True
    for name in selected_sequences(args.sequence):
        try:
            sequence_report = run_sequence(
                name,
                config,
                args.expected_paging_rows,
                args.page_size,
                args.max_pages,
            )
        except Exception as err:  # noqa: BLE001 - report the exact failing sequence.
            ok = False
            sequence_report = {"sequence": name, "ok": False, "error": str(err)}
        report["sequences"].append(sequence_report)
    report["ok"] = ok and all(sequence.get("ok") for sequence in report["sequences"])
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
