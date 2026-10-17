#!/usr/bin/env python3
"""Compare retained reader binaries on the same immutable journal fixtures.

Alternates baseline/candidate order, validates complete semantic counters, and
retains individual measurements plus standard reader benchmark summaries.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import random
import shlex
import statistics
from datetime import datetime, timezone
from pathlib import Path

import run_reader_core_benchmarks as reader
from reader_filter_golden import auxiliary_goldens, filter_goldens, journal_inputs


def cases(language: str) -> list[dict]:
    strategies = (
        ("windowed", "whole-file") if language == "rust" else ("mmap", "read-at")
    )
    out = []
    for strategy in strategies:
        for bounds in ("live", "snapshot"):
            for mode in ("sdk-payloads", "facade-data", "sdk-entry", "facade-next"):
                for direction in ("forward", "backward"):
                    out.append(
                        {
                            "mode": mode,
                            "bounds": bounds,
                            "strategy": strategy,
                            "direction": direction,
                        }
                    )
    if language == "rust":
        for mode in ("core-next", "core-offsets", "core-payloads"):
            for bounds in ("live", "snapshot"):
                for strategy in strategies:
                    for direction in ("forward", "backward"):
                        out.append(dict(mode=mode, bounds=bounds, strategy=strategy, direction=direction))
    for mode, field, repetitions in (
        ("sdk-fields", "HIGH_CARD_00", 10000),
        ("sdk-unique", "LOW_CARD_00", 10000),
        ("sdk-unique-state", "LOW_CARD_00", 10000),
        ("sdk-unique", "HIGH_CARD_00", 3),
        ("sdk-unique-state", "HIGH_CARD_00", 3),
    ):
        out.append(
            {
                "mode": mode,
                "field": field,
                "repetitions": repetitions,
                "bounds": "snapshot",
                "strategy": strategies[0],
                "direction": "forward",
            }
        )
    out.append(
        {
            "mode": "explorer-query",
            "bounds": "snapshot",
            "strategy": strategies[0],
            "direction": "forward",
        }
    )
    return out


def filter_cases(language: str, realtime: int | None) -> list[dict]:
    strategy = "windowed" if language == "rust" else "mmap"
    dense = ["PERF_PROFILE=mixed-cardinality-32-fields"]
    scenarios = [
        ("unfiltered", [], 1, 0, False, None),
        ("dense", dense, 1, 0, False, None),
        ("sparse", ["HIGH_CARD_00=high-00-000000"], 1, 0, False, None),
        ("absent", ["LOW_CARD_00=absent"], 1, 0, False, None),
        ("same-field-or", ["LOW_CARD_00=low-00-00", "LOW_CARD_00=low-00-01"], 1, 0, False, None),
        ("different-field-and", ["LOW_CARD_00=low-00-00", "LOW_CARD_00=low-00-01", "LOW_CARD_01=low-01-01", "LOW_CARD_01=low-01-02"], 1, 0, False, None),
        ("fresh-first", dense, 1000, 1, True, None),
        ("restart-first", dense, 10000, 1, False, None),
    ]
    if realtime is not None:
        scenarios.append(("realtime-first", dense, 1000, 1, False, realtime))
    out = []
    for mode in ("sdk-filter", "facade-filter"):
        for bounds in ("live", "snapshot"):
            for direction in ("forward", "backward"):
                for name, matches, repetitions, limit, rebuild, time in scenarios:
                    out.append(dict(
                        mode=mode, scenario=name, matches=matches,
                        filter_repetitions=repetitions, filter_limit=limit,
                        filter_rebuild=rebuild, filter_realtime=time,
                        bounds=bounds, strategy=strategy, direction=direction,
                    ))
    return out


def case_id(case: dict) -> str:
    return "/".join(
        str(case.get(key, ""))
        for key in ("mode", "field", "bounds", "strategy", "direction", "scenario")
    )


def auxiliary_cases(language: str) -> list[dict]:
    strategies = ("windowed", "whole-file") if language == "rust" else ("mmap", "read-at")
    out = [dict(mode=mode, field="LOW_CARD_00", repetitions=1000 if mode == "indexed-open" else 1,
                snapshot_match="LOW_CARD_00=low-00-00", bounds="snapshot", strategy=strategy,
                direction="forward")
           for mode in ("indexed-open", "indexed-exact", "indexed-field", "indexed-all")
           for strategy in strategies]
    out += [dict(mode=mode, bounds="snapshot", strategy="windowed" if language == "rust" else "auto",
                 direction="forward") for mode in ("verify-index", "verify-file")]
    return out


def sha256(path: Path) -> str:
    if path.is_file():
        with path.open("rb") as stream:
            return hashlib.file_digest(stream, "sha256").hexdigest()
    digest = hashlib.sha256()
    inodes = {}
    for file in sorted(path.rglob("*")):
        if file.is_file():
            stat = file.stat()
            key = (stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns)
            if key not in inodes:
                inodes[key] = sha256(file)
            digest.update(file.relative_to(path).as_posix().encode() + b"\0")
            digest.update(inodes[key].encode() + b"\n")
    return digest.hexdigest()


def compare(pairs: list[tuple[float, float]]) -> dict:
    ratios = [after / before for before, after in pairs]
    rng = random.Random(159)
    sampled = sorted(
        statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(10000)
    )
    low, high = sampled[249], sampled[9749]
    return {
        "paired_median_time_ratio": statistics.median(ratios),
        "paired_bootstrap_95_percent_interval": [low, high],
        "baseline_median_seconds": statistics.median(p[0] for p in pairs),
        "candidate_median_seconds": statistics.median(p[1] for p in pairs),
        "baseline_coefficient_of_variation": statistics.stdev(p[0] for p in pairs)
        / statistics.mean(p[0] for p in pairs),
        "candidate_coefficient_of_variation": statistics.stdev(p[1] for p in pairs)
        / statistics.mean(p[1] for p in pairs),
        "assessment": "regression"
        if low > 1
        else "improvement"
        if high < 1
        else "inconclusive",
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    for phase in ("baseline", "candidate"):
        for language in ("rust", "go"):
            parser.add_argument(f"--{phase}-{language}", type=Path, required=True)
    parser.add_argument("--fixture", type=Path, action="append", required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--surface", choices=("file", "directory", "open-files"), default="file")
    parser.add_argument("--filter-suite", action="store_true", help="Include ordinary filtering on the mixed-cardinality corpus")
    parser.add_argument("--auxiliary-suite", action="store_true", help="Include indexed snapshot lifecycles and verification on a bounded uncompressed corpus")
    parser.add_argument("--auxiliary-golden-source", type=Path, help="Uncompressed original for one compressed/staged auxiliary fixture")
    parser.add_argument("--filter-time-usec", type=int, help="Add repeated filtered realtime-seek cases at this timestamp")
    parser.add_argument("--repetitions", type=int, default=10)
    parser.add_argument("--warmups", type=int, default=2)
    parser.add_argument(
        "--batch",
        type=int,
        default=1,
        help="Child runs per phase; compare paired mean read times",
    )
    parser.add_argument(
        "--cpu", type=int, help="Pin this harness and its children to one allowed CPU"
    )
    parser.add_argument(
        "--case",
        action="append",
        default=[],
        help="Include cases containing any supplied substring",
    )
    args = parser.parse_args()
    if args.repetitions < 3 or args.warmups < 1 or args.batch < 1:
        parser.error("at least three measured repetitions and one warmup are required")
    if args.filter_time_usec is not None and not 0 <= args.filter_time_usec <= (1 << 64) - 1:
        parser.error("--filter-time-usec must fit an unsigned 64-bit timestamp")
    if args.auxiliary_suite and args.surface != "file":
        parser.error("--auxiliary-suite supports file input only")
    if args.auxiliary_golden_source and (not args.auxiliary_suite or len(args.fixture) != 1):
        parser.error("--auxiliary-golden-source requires --auxiliary-suite and one fixture")
    if args.cpu is not None:
        if not hasattr(os, "sched_getaffinity") or args.cpu not in os.sched_getaffinity(
            0
        ):
            parser.error("--cpu must name a CPU available to this process")
        os.sched_setaffinity(0, {args.cpu})
    root = reader.ROOT.resolve()
    out = args.out.resolve()
    if not out.is_relative_to(root / ".local") or out.exists():
        parser.error("--out must be a new directory under the repository's .local/")
    fixtures = [path.resolve(strict=True) for path in args.fixture]
    binaries = {
        (phase, language): getattr(args, f"{phase}_{language}").resolve(strict=True)
        for phase in ("baseline", "candidate")
        for language in ("rust", "go")
    }
    binary_hashes = {key: sha256(path) for key, path in binaries.items()}
    out.mkdir(parents=True)
    (out / "tmp").mkdir()
    env = {"TMPDIR": str(out / "tmp")}
    comparisons = []
    for fixture_index, fixture in enumerate(fixtures):
        fixture_hash = sha256(fixture)
        inputs = [fixture]
        if args.surface == "open-files" and fixture.is_dir():
            inputs = journal_inputs(fixture, args.surface)
            if not inputs:
                raise RuntimeError("open-files fixture contains no journals")
        fixture_out = out / f"fixture-{fixture_index}"
        goldens = filter_goldens(journal_inputs(fixture, args.surface), filter_cases("go", args.filter_time_usec)) if args.filter_suite else {}
        auxiliary_source = args.auxiliary_golden_source.resolve(strict=True) if args.auxiliary_golden_source else fixture
        auxiliary_source_hash = sha256(auxiliary_source) if args.auxiliary_suite else None
        auxiliary = auxiliary_goldens(auxiliary_source, auxiliary_cases("go")) if args.auxiliary_suite else {}
        if auxiliary:
            fixture_out.mkdir(parents=True)
            (fixture_out / "auxiliary-goldens.json").write_text(json.dumps(auxiliary, indent=2) + "\n")
        retained = {"baseline": [], "candidate": []}
        for phase in retained:
            phase_out = fixture_out / phase
            phase_out.mkdir(parents=True)
            (phase_out / "manifest.json").write_text(
                json.dumps(
                    {
                        "created_at": datetime.now(timezone.utc).isoformat(),
                        "benchmark": "reader-core",
                        "binaries": {
                            language: {
                                "path": str(binaries[phase, language]),
                                "sha256": binary_hashes[phase, language],
                            }
                            for language in ("rust", "go")
                        },
                        "parameters": {
                            "repetitions": args.repetitions,
                            "warmups": args.warmups,
                            "batch": args.batch,
                            "cpu": args.cpu,
                            "surface": args.surface,
                            "fixture_sha256": fixture_hash,
                        },
                        "status": "in progress; inspect per-process output on failure",
                    },
                    indent=2,
                )
                + "\n"
            )
        if goldens:
            (fixture_out / "filter-goldens.json").write_text(json.dumps(goldens, indent=2) + "\n")
        for language in ("rust", "go"):
            selected_cases = cases(language)
            if args.filter_suite:
                selected_cases += filter_cases(language, args.filter_time_usec)
            if args.auxiliary_suite:
                selected_cases += auxiliary_cases(language)
            for case in selected_cases:
                if args.surface != "file" and (
                    case["mode"].startswith("core-") or case["mode"] == "explorer-query"
                ):
                    continue
                identity = f"{language}/{case_id(case)}"
                if args.case and not any(
                    fragment in identity for fragment in args.case
                ):
                    continue
                pairs = []
                golden_key = "/".join(str(case[key]) for key in ("mode", "bounds", "direction", "scenario")) if "scenario" in case else ""
                expected = goldens.get(golden_key)
                auxiliary_expected = auxiliary.get(case["mode"])
                if auxiliary_expected:
                    expected = tuple(auxiliary_expected["counts"])
                for iteration in range(-args.warmups, args.repetitions):
                    order = (
                        ("baseline", "candidate")
                        if iteration % 2 == 0
                        else ("candidate", "baseline")
                    )
                    times = {}
                    for phase in order:
                        command = [
                            str(binaries[phase, language]),
                            "--surface",
                            args.surface,
                            "--mode",
                            case["mode"],
                            "--bounds",
                            case["bounds"],
                            "--mmap-strategy",
                            case["strategy"],
                            "--direction",
                            case["direction"],
                        ]
                        for input_path in inputs:
                            command += ["--input", str(input_path)]
                        if "field" in case:
                            command += [
                                "--catalog-field",
                                case["field"],
                                "--catalog-repetitions",
                                str(case["repetitions"]),
                            ]
                        if "snapshot_match" in case:
                            command += ["--match", case["snapshot_match"]]
                        if case["mode"] == "explorer-query":
                            command += [
                                "--explorer-strategy",
                                "index",
                                "--explorer-field-mode",
                                "all-values",
                                "--explorer-filter",
                                "LOW_CARD_00=low-00-00",
                                "--explorer-facet",
                                "LOW_CARD_01",
                                "--explorer-limit",
                                "100",
                            ]
                        if "scenario" in case:
                            for match in case["matches"]:
                                command += ["--match", match]
                            command += ["--filter-repetitions", str(case["filter_repetitions"]), "--filter-limit", str(case["filter_limit"])]
                            if case["filter_rebuild"]:
                                command += ["--filter-rebuild"]
                            if case["filter_realtime"] is not None:
                                command += ["--filter-realtime", str(case["filter_realtime"])]
                        times[phase] = 0.0
                        for sample in range(args.batch):
                            print(shlex.join(command), flush=True)
                            stats_path = (
                                fixture_out
                                / phase
                                / f"process-{len(retained[phase])}.json"
                            )
                            result = reader.timed_run(command, stats_path, env)
                            stats_path.with_suffix(".stdout").write_text(result.stdout)
                            stats_path.with_suffix(".stderr").write_text(result.stderr)
                            stats_path.with_suffix(".command.json").write_text(
                                json.dumps(
                                    {
                                        "command": command,
                                        "returncode": result.returncode,
                                    },
                                    indent=2,
                                )
                                + "\n"
                            )
                            reader.require_ok(result, identity)
                            parsed = reader.parse_json_result(result.stdout)
                            semantic = tuple(
                                parsed.get(key) for key in reader.CHECKSUM_FIELDS
                            )
                            if expected is None:
                                expected = semantic
                            if semantic != expected:
                                raise RuntimeError(
                                    f"semantic mismatch {identity}/{phase}: {semantic} != {expected}"
                                )
                            if auxiliary_expected:
                                extra = parsed.get("extra", {})
                                if any(extra.get(key) != value for key, value in auxiliary_expected["extra"].items()):
                                    raise RuntimeError(f"capture/verification metadata mismatch {identity}/{phase}: {extra}")
                            seconds = float(parsed["read_seconds"])
                            if not math.isfinite(seconds) or seconds <= 0:
                                raise RuntimeError(
                                    f"invalid timing {identity}: {seconds}"
                                )
                            times[phase] += seconds / args.batch
                            if "field" in case:
                                parsed["mode"] += f"[{case['field']}]"
                            if "scenario" in case:
                                parsed["mode"] += f"[{case['scenario']}]"
                            retained[phase].append(
                                {
                                    "warmup": iteration < 0,
                                    "iteration": iteration,
                                    "sample": sample,
                                    "command": command,
                                    "result": parsed,
                                    "process_stats": reader.parse_time_stats(
                                        stats_path
                                    ),
                                }
                            )
                            with (fixture_out / phase / "runs.jsonl").open(
                                "a"
                            ) as stream:
                                stream.write(json.dumps(retained[phase][-1]) + "\n")
                    if iteration >= 0:
                        pairs.append((times["baseline"], times["candidate"]))
                comparisons.append(
                    dict(fixture=fixture_index, case=identity, **compare(pairs))
                )
                print(json.dumps(comparisons[-1]), flush=True)
                (out / "comparisons.json").write_text(
                    json.dumps(comparisons, indent=2) + "\n"
                )
                for phase, runs in retained.items():
                    for name, value in (
                        ("runs.json", runs),
                        ("summary.json", reader.summarize(runs)),
                    ):
                        (fixture_out / phase / name).write_text(
                            json.dumps(value, indent=2) + "\n"
                        )
        if sha256(fixture) != fixture_hash:
            raise RuntimeError(f"fixture changed during measurement: {fixture}")
        if args.auxiliary_suite and sha256(auxiliary_source) != auxiliary_source_hash:
            raise RuntimeError(f"auxiliary golden source changed during measurement: {auxiliary_source}")
        for key, binary in binaries.items():
            if sha256(binary) != binary_hashes[key]:
                raise RuntimeError(f"binary changed during measurement: {binary}")
        for phase, runs in retained.items():
            phase_out = fixture_out / phase
            manifest = {
                "created_at": datetime.now(timezone.utc).isoformat(),
                "benchmark": "reader-core",
                "binaries": {
                    language: {
                        "path": str(binaries[phase, language]),
                        "sha256": binary_hashes[phase, language],
                    }
                    for language in ("rust", "go")
                },
                "parameters": {
                    "repetitions": args.repetitions,
                    "warmups": args.warmups,
                    "batch": args.batch,
                    "cpu": args.cpu,
                    "surface": args.surface,
                    "fixture_sha256": fixture_hash,
                    "auxiliary_golden_source_sha256": auxiliary_source_hash,
                },
                "status": "measured; correctness and performance acceptance require review",
            }
            for name, value in (
                ("runs.json", runs),
                ("summary.json", reader.summarize(runs)),
                ("manifest.json", manifest),
            ):
                (phase_out / name).write_text(json.dumps(value, indent=2) + "\n")
            for direction in ("forward", "backward"):
                directional = [
                    run for run in runs if run["result"]["direction"] == direction
                ]
                if directional:
                    destination = phase_out / direction
                    destination.mkdir()
                    (destination / "summary.json").write_text(
                        json.dumps(reader.summarize(directional), indent=2) + "\n"
                    )
                    manifest["parameters"]["direction"] = direction
                    (destination / "manifest.json").write_text(
                        json.dumps(manifest, indent=2) + "\n"
                    )
        (out / "comparisons.json").write_text(json.dumps(comparisons, indent=2) + "\n")


if __name__ == "__main__":
    main()
