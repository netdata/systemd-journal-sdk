#!/usr/bin/env python3
"""Run Netdata function anchor regression scenarios against peer binaries."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import tempfile
from pathlib import Path
from typing import Any

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
INTEROPERABILITY_DIR = REPO_ROOT / "tests" / "interoperability"
if str(INTEROPERABILITY_DIR) not in sys.path:
    sys.path.insert(0, str(INTEROPERABILITY_DIR))

from go_fixture_writer import write_journal_file  # noqa: E402
from run_function_compare import run_command  # noqa: E402

FIXTURE_DIR = SCRIPT_DIR / "fixtures"
REQUEST_DIR = SCRIPT_DIR / "requests"

SCENARIOS = {
    "query-wide-noncollision": {
        "fixture": FIXTURE_DIR / "query-wide-noncollision.json",
        "page1": REQUEST_DIR / "query-wide-noncollision-page1.json",
        "page2": REQUEST_DIR / "query-wide-noncollision-page2-anchor.json",
    },
    "same-anchor-boundary": {
        "fixture": FIXTURE_DIR / "same-anchor-boundary.json",
        "page1": REQUEST_DIR / "same-anchor-boundary-page1.json",
        "page2": REQUEST_DIR / "same-anchor-boundary-page2-anchor.json",
    },
    "same-anchor-boundary-non-data-only": {
        "fixture": FIXTURE_DIR / "same-anchor-boundary.json",
        "page1": REQUEST_DIR / "same-anchor-boundary-non-data-only-page1.json",
        "page2": REQUEST_DIR / "same-anchor-boundary-non-data-only-page2-anchor.json",
    },
    "same-anchor-boundary-forward": {
        "fixture": FIXTURE_DIR / "same-anchor-boundary.json",
        "page1": REQUEST_DIR / "same-anchor-boundary-forward-page1.json",
        "page2": REQUEST_DIR / "same-anchor-boundary-forward-page2-anchor.json",
    },
}


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def request_digest(request: dict[str, Any]) -> str:
    payload = json.dumps(request, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def parse_peer(raw: str) -> tuple[str, Path]:
    if "=" not in raw:
        raise argparse.ArgumentTypeError("peer must use NAME=/path/to/binary")
    name, value = raw.split("=", 1)
    name = name.strip()
    if not name:
        raise argparse.ArgumentTypeError("peer name must not be empty")
    path = Path(value).expanduser()
    if not path.is_absolute():
        path = (Path.cwd() / path).resolve()
    return name, path


def write_fixture(spec: dict[str, Any], target: Path) -> None:
    target.mkdir(parents=True, exist_ok=True)
    default_realtime = int(spec.get("realtime_usec", 0))
    for index, file_spec in enumerate(spec["files"]):
        realtime_usec = int(file_spec.get("realtime_usec", default_realtime))
        if realtime_usec <= 0:
            raise ValueError(f"fixture file {index} has no realtime_usec")
        journal_path = target / file_spec["machine_dir"] / file_spec["journal_file"]
        write_journal_file(
            journal_path.resolve(),
            machine_id=file_spec["machine_id"],
            boot_id=file_spec["boot_id"],
            seqnum_id=file_spec["seqnum_id"],
            file_id=file_spec["file_id"],
            entries=[
                {
                    "realtime_usec": realtime_usec,
                    "monotonic_usec": int(file_spec["monotonic_usec"]),
                    "fields": [
                        ("MESSAGE", file_spec["message"]),
                        ("PRIORITY", str(file_spec.get("priority", "6"))),
                        ("_COMM", str(file_spec.get("comm", "anchor-regression"))),
                        (
                            "SYSLOG_IDENTIFIER",
                            str(file_spec.get("syslog_identifier", "anchor-regression")),
                        ),
                    ],
                }
            ],
        )


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


def validate_collected_messages(
    expected: list[str],
    page1_rows: list[dict[str, Any]],
    page2_rows: list[dict[str, Any]],
) -> dict[str, Any]:
    collected = [str(row["message"]) for row in page1_rows + page2_rows]
    duplicate_messages = sorted(
        message for message in set(collected) if collected.count(message) > 1
    )
    missing = sorted(set(expected) - set(collected))
    extra = sorted(set(collected) - set(expected))
    return {
        "ok": not duplicate_messages and not missing and not extra,
        "expected_messages": expected,
        "collected_messages": collected,
        "missing_messages": missing,
        "extra_messages": extra,
        "duplicate_messages": duplicate_messages,
    }


def timestamps_ordered(rows: list[dict[str, Any]], direction: str) -> bool:
    timestamps = [int(row["timestamp"]) for row in rows]
    if direction == "forward":
        return all(left <= right for left, right in zip(timestamps, timestamps[1:]))
    return all(left >= right for left, right in zip(timestamps, timestamps[1:]))


def edge_anchor(rows: list[dict[str, Any]], direction: str) -> int | None:
    if not rows:
        return None
    if direction == "forward":
        return int(rows[0]["timestamp"])
    return int(rows[-1]["timestamp"])


def validate_ordered_scalar_anchor(
    direction: str,
    anchor: int | None,
    page1_rows: list[dict[str, Any]],
    page2_rows: list[dict[str, Any]],
) -> dict[str, Any]:
    page1_ordered = timestamps_ordered(page1_rows, direction)
    page2_ordered = timestamps_ordered(page2_rows, direction)
    page2_anchor = edge_anchor(page2_rows, direction)
    if anchor is None:
        non_overlapping = not page2_rows
        anchor_progressed = page2_anchor is None
    elif direction == "forward":
        non_overlapping = all(int(row["timestamp"]) > anchor for row in page2_rows)
        anchor_progressed = page2_anchor is None or page2_anchor > anchor
    else:
        non_overlapping = all(int(row["timestamp"]) < anchor for row in page2_rows)
        anchor_progressed = page2_anchor is None or page2_anchor < anchor
    return {
        "ok": page1_ordered and page2_ordered and non_overlapping and anchor_progressed,
        "direction": direction,
        "anchor": anchor,
        "page2_edge_anchor": page2_anchor,
        "page1_ordered": page1_ordered,
        "page2_ordered": page2_ordered,
        "non_overlapping": non_overlapping,
        "anchor_progressed": anchor_progressed,
        "page1_timestamps": [int(row["timestamp"]) for row in page1_rows],
        "page2_timestamps": [int(row["timestamp"]) for row in page2_rows],
    }


def run_peer_page(
    peer_name: str,
    binary: Path,
    fixture_dir: Path,
    request: dict[str, Any],
    *,
    function: str,
    timeout_seconds: int,
    process_timeout_seconds: int,
) -> dict[str, Any]:
    payload = json.dumps(request, sort_keys=True).encode("utf-8") + b"\n"
    result = run_command(
        binary,
        function,
        fixture_dir,
        payload,
        timeout_seconds,
        process_timeout_seconds,
    )
    rows = []
    if isinstance(result.get("json"), dict):
        rows = response_rows(result["json"])
    return {
        "peer": peer_name,
        "request_sha256": request_digest(request),
        "run": {key: value for key, value in result.items() if key != "json"},
        "rows": rows,
        "status": result["json"].get("status") if isinstance(result.get("json"), dict) else None,
    }


def run_peer_scenario(
    peer_name: str,
    binary: Path,
    scenario_name: str,
    paths: dict[str, Path],
    fixture_dir: Path,
    *,
    function: str,
    timeout_seconds: int,
    process_timeout_seconds: int,
) -> dict[str, Any]:
    spec = load_json(paths["fixture"])
    page1 = load_json(paths["page1"])
    page2 = load_json(paths["page2"])
    page1_result = run_peer_page(
        peer_name,
        binary,
        fixture_dir,
        page1,
        function=function,
        timeout_seconds=timeout_seconds,
        process_timeout_seconds=process_timeout_seconds,
    )
    page1_rows = page1_result["rows"]
    derived_anchor = page1_rows[-1]["timestamp"] if page1_rows else None
    page2_anchor = page2.get("anchor")
    anchor_matches_request = derived_anchor == page2_anchor
    page2_result = run_peer_page(
        peer_name,
        binary,
        fixture_dir,
        page2,
        function=function,
        timeout_seconds=timeout_seconds,
        process_timeout_seconds=process_timeout_seconds,
    )
    direction = str(page1.get("direction", "backward"))
    validation = validate_collected_messages(
        [str(message) for message in spec["expected_messages"]],
        page1_rows,
        page2_result["rows"],
    )
    ordered_scalar = validate_ordered_scalar_anchor(
        direction,
        derived_anchor,
        page1_rows,
        page2_result["rows"],
    )
    ok = bool(validation["ok"] and anchor_matches_request and ordered_scalar["ok"])
    return {
        "scenario": scenario_name,
        "fixture": str(paths["fixture"]),
        "fixture_directory": str(fixture_dir),
        "page1": page1_result,
        "page2": page2_result,
        "derived_page2_anchor": derived_anchor,
        "committed_page2_anchor": page2_anchor,
        "anchor_matches_request": anchor_matches_request,
        "validation": validation,
        "ordered_scalar_anchor": ordered_scalar,
        "ok": ok,
    }


def selected_scenarios(names: list[str]) -> dict[str, dict[str, Path]]:
    if not names or "all" in names:
        return SCENARIOS
    selected = {}
    for name in names:
        if name not in SCENARIOS:
            raise ValueError(f"unknown scenario {name!r}")
        selected[name] = SCENARIOS[name]
    return selected


def build_arg_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--peer",
        action="append",
        type=parse_peer,
        required=True,
        help="Peer binary in NAME=/path/to/binary form. The binary must support the Netdata --test CLI shape.",
    )
    parser.add_argument("--scenario", action="append", default=[])
    parser.add_argument("--function", default="systemd-journal")
    parser.add_argument("--timeout", type=int, default=0)
    parser.add_argument("--process-timeout", type=int, default=3600)
    parser.add_argument(
        "--work-dir",
        type=Path,
        default=REPO_ROOT / ".local" / "netdata-function-anchor-regression",
    )
    parser.add_argument("--out", type=Path)
    parser.add_argument(
        "--allow-fail",
        action="append",
        default=[],
        help="Peer name allowed to fail. Useful for documenting current systemd-journal.plugin behavior.",
    )
    return parser


def write_report(path: Path, report: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main() -> int:
    parser = build_arg_parser()
    args = parser.parse_args()
    scenarios = selected_scenarios(args.scenario)
    args.work_dir.mkdir(parents=True, exist_ok=True)
    report: dict[str, Any] = {
        "function": args.function,
        "scenarios": [],
        "allowed_failures": sorted(args.allow_fail),
    }
    effective_ok = True
    for scenario_name, paths in scenarios.items():
        spec = load_json(paths["fixture"])
        fixture_dir = Path(
            tempfile.mkdtemp(prefix=f"{scenario_name}-", dir=args.work_dir)
        )
        write_fixture(spec, fixture_dir)
        scenario_report = {
            "scenario": scenario_name,
            "description": spec.get("description", ""),
            "fixture_directory": str(fixture_dir),
            "peers": [],
        }
        for peer_name, binary in args.peer:
            peer_report = run_peer_scenario(
                peer_name,
                binary,
                scenario_name,
                paths,
                fixture_dir,
                function=args.function,
                timeout_seconds=args.timeout,
                process_timeout_seconds=args.process_timeout,
            )
            peer_allowed_failure = peer_name in args.allow_fail
            peer_report["allowed_failure"] = peer_allowed_failure
            if not peer_report["ok"] and not peer_allowed_failure:
                effective_ok = False
            scenario_report["peers"].append(peer_report)
        scenario_report["ok"] = all(
            peer["ok"] or peer["allowed_failure"] for peer in scenario_report["peers"]
        )
        report["scenarios"].append(scenario_report)
    report["ok"] = effective_ok and all(
        bool(scenario["ok"]) for scenario in report["scenarios"]
    )
    if args.out is not None:
        write_report(args.out, report)
    else:
        print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
