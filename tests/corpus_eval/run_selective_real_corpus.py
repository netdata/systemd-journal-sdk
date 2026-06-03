#!/usr/bin/env python3
"""Selective real-corpus journal verification with sanitized reporting.

This runner is intentionally smaller than the full SOW-0064 corpus harness. It
discovers real journal files, selects a bounded feature-based sample, writes raw
path manifests only under `.local/`, and persists only sanitized IDs, classes,
counts, hashes, timings, and discrepancy codes.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from tests.corpus_eval.run_corpus_eval import (
    ROOT,
    JournalCase,
    build_tools,
    case_keys,
    discover_cases,
    run_digest_driver,
    run_env,
    run_regenerator,
    snapshot_case,
    summarize_discovery,
    write_json,
)


REPORT_SCHEMA = "systemd-journal-sdk-selective-real-corpus-v1"
DEFAULT_OUT = ROOT / ".local" / "sow-0076" / "selective-real-corpus"
DEFAULT_REPORT_JSON = ROOT / "tests" / "corpus_eval" / "reports" / "selective-real-corpus-report.json"
DEFAULT_REPORT_MD = ROOT / "tests" / "corpus_eval" / "reports" / "selective-real-corpus-report.md"
JOURNAL_MAGIC = b"LPKSHHRH"

STATE_NAMES = {
    0: "offline",
    1: "online",
    2: "archived",
}

INCOMPATIBLE_FLAGS = {
    "compressed-xz": 1 << 0,
    "compressed-lz4": 1 << 1,
    "keyed-hash": 1 << 2,
    "compressed-zstd": 1 << 3,
    "compact": 1 << 4,
}

COMPATIBLE_FLAGS = {
    "sealed": 1 << 0,
    "tail-entry-boot-id": 1 << 1,
    "sealed-continuous": 1 << 2,
}

OBJECT_FLAGS = {
    "compressed-xz": 1 << 0,
    "compressed-lz4": 1 << 1,
    "compressed-zstd": 1 << 2,
}

FEATURE_POLICY: list[tuple[str, str]] = [
    ("previous-bug-exposure", "files whose sanitized IDs match prior SOW-0064 targeted discrepancy runs"),
    ("historical-unkeyed", "files without HEADER_INCOMPATIBLE_KEYED_HASH"),
    ("fss-sealed", "files with sealed/FSS-compatible header flags or TAG objects"),
    ("compact", "files using HEADER_INCOMPATIBLE_COMPACT"),
    ("compressed-data", "files with compressed DATA support or compressed DATA objects"),
    ("active-open-snapshot", "online-state files that must be snapshotted before driver comparison"),
    ("archived", "archived-state files"),
    ("multi-boot", "files where stock journalctl --file --list-boots reports more than one boot"),
    ("high-cardinality", "files with high DATA object counts"),
    ("high-field-count", "files with high FIELD object counts"),
    ("large-file", "largest files within the bounded selective sample"),
]


@dataclass
class Probe:
    case: JournalCase
    features: set[str] = field(default_factory=set)
    header: dict[str, Any] = field(default_factory=dict)
    object_scan: dict[str, Any] = field(default_factory=dict)
    boot_count: int | None = None
    byte_sha256: str | None = None
    selection_reasons: list[str] = field(default_factory=list)
    probe_status: str = "ok"

    def score(self, feature: str) -> int:
        if feature == "high-cardinality":
            return int(self.header.get("n_data") or 0)
        if feature == "high-field-count":
            return int(self.header.get("n_fields") or 0)
        if feature == "multi-boot":
            return int(self.boot_count or 0)
        return int(self.case.size)

    def sanitized(self) -> dict[str, Any]:
        return {
            "file_id": self.case.file_id,
            "size": self.case.size,
            "suffix": self.case.suffix,
            "byte_sha256": self.byte_sha256,
            "feature_classes": sorted(self.features),
            "selection_reasons": self.selection_reasons,
            "probe_status": self.probe_status,
            "header": sanitize_header(self.header),
            "object_scan": self.object_scan,
            "boot_count": self.boot_count,
        }


def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def read_u32(data: bytes, offset: int) -> int:
    if len(data) < offset + 4:
        return 0
    return int.from_bytes(data[offset : offset + 4], "little")


def read_u64(data: bytes, offset: int) -> int:
    if len(data) < offset + 8:
        return 0
    return int.from_bytes(data[offset : offset + 8], "little")


def parse_header(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        raw = handle.read(272)
    if len(raw) < 208 or raw[:8] != JOURNAL_MAGIC:
        raise ValueError("not a readable systemd journal header")
    compatible = read_u32(raw, 8)
    incompatible = read_u32(raw, 12)
    state = raw[16]
    header_size = read_u64(raw, 88)
    header: dict[str, Any] = {
        "state": STATE_NAMES.get(state, f"unknown-{state}"),
        "compatible_flags": [
            name for name, bit in COMPATIBLE_FLAGS.items() if compatible & bit
        ],
        "incompatible_flags": [
            name for name, bit in INCOMPATIBLE_FLAGS.items() if incompatible & bit
        ],
        "header_size": header_size,
        "arena_size": read_u64(raw, 96),
        "tail_object_offset": read_u64(raw, 136),
        "n_objects": read_u64(raw, 144),
        "n_entries": read_u64(raw, 152),
    }
    if header_size >= 216 and len(raw) >= 216:
        header["n_data"] = read_u64(raw, 208)
    if header_size >= 224 and len(raw) >= 224:
        header["n_fields"] = read_u64(raw, 216)
    if header_size >= 232 and len(raw) >= 232:
        header["n_tags"] = read_u64(raw, 224)
    if header_size >= 248 and len(raw) >= 248:
        header["data_hash_chain_depth"] = read_u64(raw, 240)
    if header_size >= 256 and len(raw) >= 256:
        header["field_hash_chain_depth"] = read_u64(raw, 248)
    return header


def sanitize_header(header: dict[str, Any]) -> dict[str, Any]:
    allowed = {
        "state",
        "compatible_flags",
        "incompatible_flags",
        "header_size",
        "arena_size",
        "n_objects",
        "n_entries",
        "n_data",
        "n_fields",
        "n_tags",
        "data_hash_chain_depth",
        "field_hash_chain_depth",
    }
    return {key: header[key] for key in allowed if key in header}


def align8(value: int) -> int:
    return (value + 7) & ~7


def scan_objects(path: Path, header: dict[str, Any], limit: int) -> dict[str, Any]:
    tail = int(header.get("tail_object_offset") or 0)
    offset = int(header.get("header_size") or 0)
    if offset <= 0 or tail <= 0:
        return {"status": "not-scanned", "reason": "missing-object-range"}
    found_compression: set[str] = set()
    found_tag = False
    scanned = 0
    file_size = path.stat().st_size
    try:
        with path.open("rb") as handle:
            while offset <= tail and offset + 16 <= file_size and scanned < limit:
                handle.seek(offset)
                raw = handle.read(16)
                if len(raw) != 16:
                    break
                object_type = raw[0]
                object_flags = raw[1]
                object_size = read_u64(raw, 8)
                if object_size < 16:
                    break
                if object_type == 1:
                    for name, bit in OBJECT_FLAGS.items():
                        if object_flags & bit:
                            found_compression.add(name)
                elif object_type == 7:
                    found_tag = True
                scanned += 1
                next_offset = offset + align8(object_size)
                if next_offset <= offset:
                    break
                offset = next_offset
    except OSError as exc:
        return {"status": "failed", "error_class": type(exc).__name__}
    return {
        "status": "ok",
        "objects_scanned": scanned,
        "compressed_data_flags": sorted(found_compression),
        "tag_object_seen": found_tag,
        "scan_limit": limit,
    }


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_known_bug_ids() -> set[str]:
    ids: set[str] = set()
    roots = [
        ROOT / ".local" / "corpus-eval" / "single-real-after-seqnum-20260530T213000Z",
        ROOT / ".local" / "corpus-eval" / "debug-go-zstd-after-fix-20260530T214517Z",
        ROOT / ".local" / "corpus-eval" / "spool-experiment-single-large",
    ]
    for root in roots:
        report = root / "report.json"
        if not report.exists():
            continue
        try:
            data = json.loads(report.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        for item in data.get("inputs", []):
            file_id = item.get("file_id")
            if isinstance(file_id, str):
                ids.add(file_id)
        for item in data.get("cases", []):
            file_id = item.get("file_id")
            if isinstance(file_id, str):
                ids.add(file_id)
    return ids


def classify_probe(case: JournalCase, known_bug_ids: set[str], object_scan_limit: int) -> Probe:
    probe = Probe(case=case)
    if case.suffix == ".journal.zst":
        probe.features.add("whole-file-zst")
        probe.probe_status = "header-skipped-whole-file-zst"
        return probe
    try:
        probe.header = parse_header(case.path)
    except Exception as exc:
        probe.probe_status = f"header-failed:{type(exc).__name__}"
        return probe

    incompatible = set(probe.header.get("incompatible_flags", []))
    compatible = set(probe.header.get("compatible_flags", []))
    state = str(probe.header.get("state") or "")
    if state == "online":
        probe.features.add("active-open-snapshot")
    elif state == "archived":
        probe.features.add("archived")
    elif state == "offline":
        probe.features.add("offline-closed")
    if "keyed-hash" not in incompatible:
        probe.features.add("historical-unkeyed")
    if "compact" in incompatible:
        probe.features.add("compact")
    if incompatible.intersection({"compressed-xz", "compressed-lz4", "compressed-zstd"}):
        probe.features.add("compressed-data")
    if compatible.intersection({"sealed", "sealed-continuous"}) or int(probe.header.get("n_tags") or 0) > 0:
        probe.features.add("fss-sealed")
    if case.file_id in known_bug_ids:
        probe.features.add("previous-bug-exposure")

    probe.object_scan = scan_objects(case.path, probe.header, object_scan_limit)
    if probe.object_scan.get("compressed_data_flags"):
        probe.features.add("compressed-data")
    if probe.object_scan.get("tag_object_seen"):
        probe.features.add("fss-sealed")
    return probe


def mark_distribution_features(probes: list[Probe], large_min_bytes: int) -> None:
    readable = [probe for probe in probes if probe.header]
    if not readable:
        return
    largest = max(readable, key=lambda probe: probe.case.size)
    if largest.case.size >= large_min_bytes:
        largest.features.add("large-file")
    high_data = max(readable, key=lambda probe: int(probe.header.get("n_data") or 0))
    if int(high_data.header.get("n_data") or 0) > 0:
        high_data.features.add("high-cardinality")
    high_fields = max(readable, key=lambda probe: int(probe.header.get("n_fields") or 0))
    if int(high_fields.header.get("n_fields") or 0) > 0:
        high_fields.features.add("high-field-count")


def probe_boot_count(path: Path, timeout: int) -> int | None:
    journalctl = shutil.which("journalctl")
    if journalctl is None:
        return None
    cmd = [journalctl, "--file", str(path), "--list-boots", "--no-pager"]
    try:
        proc = subprocess.run(
            cmd,
            cwd=str(ROOT),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    if proc.returncode != 0:
        return None
    return len([line for line in proc.stdout.splitlines() if line.strip()])


def mark_multi_boot(probes: list[Probe], limit: int, timeout: int) -> None:
    candidates = sorted(
        [probe for probe in probes if probe.header],
        key=lambda probe: int(probe.header.get("n_entries") or 0),
        reverse=True,
    )
    for probe in candidates[:limit]:
        count = probe_boot_count(probe.case.path, timeout)
        probe.boot_count = count
        if count is not None and count > 1:
            probe.features.add("multi-boot")
            return


def select_probes(probes: list[Probe], max_selected: int) -> tuple[list[Probe], dict[str, str]]:
    selected: list[Probe] = []
    selected_ids: set[str] = set()
    missing: dict[str, str] = {}
    by_feature = [name for name, _ in FEATURE_POLICY]
    for feature in by_feature:
        all_candidates = [probe for probe in probes if feature in probe.features]
        candidates = [probe for probe in all_candidates if probe.case.file_id not in selected_ids]
        if not all_candidates:
            missing[feature] = "not-present-in-discovered-corpus-or-not-detected"
            continue
        if not candidates:
            missing[feature] = "covered-by-an-earlier-selected-file"
            continue
        candidates.sort(key=lambda probe: (probe.score(feature), probe.case.size), reverse=True)
        picked = candidates[0]
        picked.selection_reasons.append(feature)
        selected.append(picked)
        selected_ids.add(picked.case.file_id)
        if len(selected) >= max_selected:
            break
    return selected, missing


def write_path_manifest(path: Path, selected: list[Probe]) -> None:
    raw = {
        "schema": f"{REPORT_SCHEMA}-raw-path-manifest",
        "created_at": utc_now(),
        "do_not_commit": True,
        "sensitive_data_policy": "raw local paths are kept only under .local for reruns",
        "files": [
            {
                "file_id": probe.case.file_id,
                "path": str(probe.case.path),
                "root": str(probe.case.root),
                "selection_reasons": probe.selection_reasons,
            }
            for probe in selected
        ],
    }
    write_json(path, raw)


def run_selected_verification(
    selected: list[Probe],
    *,
    out: Path,
    drivers: list[str],
    regenerators: list[str],
    regeneration_modes: list[str],
    timeout: int,
    keep_outputs: bool,
) -> tuple[list[dict[str, Any]], list[dict[str, Any]], dict[str, Any]]:
    env = run_env()
    tools = build_tools(env, out)
    state_path = out / "state.json"
    state = {"completed": {}}
    completed: dict[str, Any] = state["completed"]
    stats_dir = out / "time"
    work_dir = out / "work"
    results: list[dict[str, Any]] = []
    discrepancies: list[dict[str, Any]] = []

    class Args:
        pass

    args = Args()
    args.drivers = drivers
    args.regenerators = regenerators
    args.regeneration_modes = regeneration_modes

    for probe in selected:
        case = probe.case
        expected_keys = case_keys(case, args)
        for key in expected_keys:
            completed.pop(key, None)
        active_case = case
        snapshot_path: Path | None = None
        try:
            active_case = snapshot_case(case, work_dir)
            snapshot_path = active_case.path
            reader_results: dict[str, dict[str, Any]] = {}
            for driver in drivers:
                try:
                    result = run_digest_driver(
                        driver,
                        active_case,
                        tools=tools,
                        env=env,
                        stats_dir=stats_dir,
                        timeout=timeout,
                    )
                except Exception as exc:
                    result = {
                        "kind": "reader",
                        "driver": driver,
                        "status": "failed",
                        "file_id": case.file_id,
                        "error_class": type(exc).__name__,
                        "error_sha256": hashlib.sha256(str(exc).encode("utf-8")).hexdigest(),
                    }
                completed[f"{case.file_id}:reader:{driver}"] = {"identity": case.identity, "result": result}
                results.append(result)
                reader_results[driver] = result
                write_json(state_path, state)

            baseline = reader_results.get("systemd")
            if not baseline or baseline.get("status") != "ok":
                discrepancies.append(
                    {
                        "code": "missing_systemd_baseline",
                        "file_id": case.file_id,
                        "detail": "systemd baseline failed, SDK parity and regeneration checks were inconclusive",
                    }
                )
                continue
            baseline_digest = str(baseline.get("logical_digest"))
            for driver, result in reader_results.items():
                if driver == "systemd" or result.get("status") != "ok":
                    continue
                if result.get("logical_digest") != baseline_digest:
                    discrepancies.append(
                        {
                            "code": "reader_digest_mismatch",
                            "file_id": case.file_id,
                            "detail": f"{driver} logical digest differs from systemd baseline",
                        }
                    )
            for regen_driver in regenerators:
                for mode in regeneration_modes:
                    result = run_regenerator(
                        regen_driver,
                        mode,
                        active_case,
                        baseline_digest,
                        tools=tools,
                        env=env,
                        work_dir=work_dir,
                        stats_dir=stats_dir,
                        keep_outputs=keep_outputs,
                        timeout=timeout,
                    )
                    completed[f"{case.file_id}:writer:{regen_driver}:{mode}"] = {
                        "identity": case.identity,
                        "result": result,
                    }
                    results.append(result)
                    write_json(state_path, state)
                    if result.get("status") == "discrepancy":
                        discrepancies.append(
                            {
                                "code": "writer_regeneration_mismatch",
                                "file_id": case.file_id,
                                "detail": f"{regen_driver}/{mode} generated output did not match systemd digest or stock verify",
                            }
                        )
                    elif result.get("status") == "failed":
                        discrepancies.append(
                            {
                                "code": "writer_regeneration_failed",
                                "file_id": case.file_id,
                                "detail": f"{regen_driver}/{mode} failed with {result.get('error_class')}",
                            }
                        )
        finally:
            if snapshot_path is not None:
                snapshot_path.unlink(missing_ok=True)

    metadata = {
        "drivers": drivers,
        "regenerators": regenerators,
        "regeneration_modes": regeneration_modes,
        "state_path": str(state_path.relative_to(ROOT)),
    }
    return results, discrepancies, metadata


def build_report(args: argparse.Namespace) -> dict[str, Any]:
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    roots = [Path(root) for root in args.root]
    known_bug_ids = load_known_bug_ids()
    started = time.perf_counter()
    cases = discover_cases(roots, max_files=args.max_discovered)
    probes = [
        classify_probe(case, known_bug_ids, args.object_scan_limit)
        for case in cases
    ]
    mark_distribution_features(probes, args.large_min_bytes)
    mark_multi_boot(probes, args.boot_probe_limit, args.boot_probe_timeout)
    selected, missing = select_probes(probes, args.max_selected)
    for probe in selected:
        probe.byte_sha256 = sha256_file(probe.case.path)
    path_manifest = out / "path-manifest.json"
    write_path_manifest(path_manifest, selected)

    results: list[dict[str, Any]] = []
    discrepancies: list[dict[str, Any]] = []
    verification: dict[str, Any] = {
        "status": "not-run",
        "reason": "selection-only mode; pass --run-verification to compare systemd/Rust/Go and regenerate outputs",
    }
    if args.run_verification:
        results, discrepancies, verification = run_selected_verification(
            selected,
            out=out,
            drivers=args.drivers,
            regenerators=args.regenerators,
            regeneration_modes=args.regeneration_modes,
            timeout=args.timeout,
            keep_outputs=args.keep_outputs,
        )
        verification["status"] = "ok" if not discrepancies else "discrepancies"

    report = {
        "schema": REPORT_SCHEMA,
        "created_at": utc_now(),
        "purpose": "repeatable feature-selected real-corpus verification without a full corpus run",
        "runtime_seconds": time.perf_counter() - started,
        "sensitive_data_policy": "committed artifacts contain sanitized IDs, classes, sizes, hashes, counts, timings, resource metrics, status codes, and discrepancy codes only",
        "raw_path_manifest": {
            "location": str(path_manifest.relative_to(ROOT)),
            "committed": False,
            "contains_raw_paths": True,
        },
        "python_node_scope": {
            "included": False,
            "reason": "not included in this selective run because the existing real-corpus harness is Rust/Go/systemd focused and Python/Node are mapped to SOW-0065 parity closure unless a small language-specific follow-up is requested",
        },
        "selection_policy": [
            {"feature_class": feature, "reason": reason}
            for feature, reason in FEATURE_POLICY
        ],
        "discovery": summarize_discovery(cases),
        "selected_count": len(selected),
        "selected": [probe.sanitized() for probe in selected],
        "missing_feature_classes": missing,
        "verification": verification,
        "results": results,
        "discrepancies": discrepancies,
        "rerun_recipe": {
            "selection_and_verification": (
                "python tests/corpus_eval/run_selective_real_corpus.py "
                "--root <journal-root> [--root <journal-root>] --run-verification"
            ),
            "selection_only": (
                "python tests/corpus_eval/run_selective_real_corpus.py "
                "--root <journal-root> [--root <journal-root>]"
            ),
        },
    }
    return report


def write_markdown(report: dict[str, Any], path: Path) -> None:
    lines = [
        "# Selective Real-Corpus Verification Report",
        "",
        f"- Schema: `{report['schema']}`",
        f"- Created: `{report['created_at']}`",
        f"- Selected files: `{report['selected_count']}`",
        f"- Verification status: `{report['verification'].get('status')}`",
        f"- Discrepancies: `{len(report['discrepancies'])}`",
        f"- Raw path manifest: `{report['raw_path_manifest']['location']}` (not committed)",
        "",
        "## Selection Policy",
        "",
    ]
    for item in report["selection_policy"]:
        lines.append(f"- `{item['feature_class']}`: {item['reason']}")
    lines.extend(
        [
            "",
            "## Selected Files",
            "",
            "| file_id | size MiB | feature classes | reasons | entries | payload-ish DATA objects |",
            "|---|---:|---|---|---:|---:|",
        ]
    )
    for item in report["selected"]:
        header = item.get("header", {})
        lines.append(
            "| {file_id} | {mib:.2f} | {features} | {reasons} | {entries} | {data} |".format(
                file_id=f"`{item['file_id']}`",
                mib=float(item.get("size") or 0) / 1024 / 1024,
                features=", ".join(f"`{feature}`" for feature in item.get("feature_classes", [])),
                reasons=", ".join(f"`{reason}`" for reason in item.get("selection_reasons", [])),
                entries=int(header.get("n_entries") or 0),
                data=int(header.get("n_data") or 0),
            )
        )
    lines.extend(["", "## Missing Feature Classes", ""])
    missing = report.get("missing_feature_classes", {})
    if missing:
        for feature, reason in missing.items():
            lines.append(f"- `{feature}`: {reason}")
    else:
        lines.append("- none")
    lines.extend(["", "## Verification Results", ""])
    results = report.get("results", [])
    if results:
        lines.extend(
            [
                "| kind | driver | mode | status | file_id |",
                "|---|---|---|---|---|",
            ]
        )
        for result in results:
            lines.append(
                "| {kind} | {driver} | {mode} | {status} | `{file_id}` |".format(
                    kind=result.get("kind", ""),
                    driver=result.get("driver", ""),
                    mode=result.get("mode", "-"),
                    status=result.get("status", ""),
                    file_id=result.get("file_id", ""),
                )
            )
    else:
        lines.append("- verification not run")
    lines.extend(["", "## Discrepancies", ""])
    if report.get("discrepancies"):
        for item in report["discrepancies"]:
            lines.append(f"- `{item.get('code')}` on `{item.get('file_id')}`: {item.get('detail')}")
    else:
        lines.append("- none")
    lines.extend(
        [
            "",
            "## Rerun Recipe",
            "",
            "```bash",
            report["rerun_recipe"]["selection_and_verification"],
            "```",
            "",
        ]
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines), encoding="utf-8")


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", action="append", required=True, help="journal file or directory root")
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--report-json", type=Path, default=DEFAULT_REPORT_JSON)
    parser.add_argument("--report-md", type=Path, default=DEFAULT_REPORT_MD)
    parser.add_argument("--max-discovered", type=int)
    parser.add_argument("--max-selected", type=int, default=8)
    parser.add_argument("--large-min-bytes", type=int, default=64 * 1024 * 1024)
    parser.add_argument("--object-scan-limit", type=int, default=20000)
    parser.add_argument("--boot-probe-limit", type=int, default=64)
    parser.add_argument("--boot-probe-timeout", type=int, default=30)
    parser.add_argument("--run-verification", action="store_true")
    parser.add_argument("--drivers", nargs="+", default=["systemd", "rust", "go"], choices=("systemd", "rust", "go"))
    parser.add_argument("--regenerators", nargs="+", default=["rust", "go"], choices=("rust", "go"))
    parser.add_argument(
        "--regeneration-modes",
        nargs="+",
        default=["regular", "compact", "compact-zstd", "compact-fss"],
        choices=("regular", "compact", "compact-zstd", "compact-fss"),
    )
    parser.add_argument("--timeout", type=int, default=1800)
    parser.add_argument("--keep-outputs", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        report = build_report(args)
        write_json(args.report_json, report)
        write_markdown(report, args.report_md)
    except Exception as exc:
        print(f"selective real corpus verification failed: {type(exc).__name__}: {exc}", file=sys.stderr)
        return 1
    print(
        json.dumps(
            {
                "status": report["verification"].get("status"),
                "selected_count": report["selected_count"],
                "discrepancies": len(report["discrepancies"]),
                "report_json": str(args.report_json),
                "report_md": str(args.report_md),
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
