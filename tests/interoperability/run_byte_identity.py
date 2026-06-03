#!/usr/bin/env python3
"""Run deterministic ingesters and compare generated journals byte-for-byte."""

from __future__ import annotations

import argparse
import json
import subprocess  # nosec B404 - subprocess is required by harnesses.
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
INGESTER_RUNNER = ROOT / "tests" / "datasets" / "ingesters" / "run_dataset_ingesters.py"
OUT = ROOT / ".local" / "datasets" / "ingesters"
LANGUAGES = ("systemd", "rust", "go", "node", "python")
REFERENCE = "systemd"
INGESTER_TIMEOUT_SECONDS = 300
SEQNUM_ID = "22222222222222222222222222222222"
BYTE_IDENTITY_MAX_SIZE_BYTES = 64 * 1024 * 1024
EXPECTED_DATA_HASH_CHAIN_DEPTH = 3

HEADER_FIELDS = (
    ("signature", 0, 8, "bytes"),
    ("compatible_flags", 8, 12, "u32"),
    ("incompatible_flags", 12, 16, "u32"),
    ("state", 16, 17, "u8"),
    ("reserved", 17, 24, "bytes"),
    ("file_id", 24, 40, "bytes"),
    ("machine_id", 40, 56, "bytes"),
    ("tail_entry_boot_id", 56, 72, "bytes"),
    ("seqnum_id", 72, 88, "bytes"),
    ("header_size", 88, 96, "u64"),
    ("arena_size", 96, 104, "u64"),
    ("data_hash_table_offset", 104, 112, "u64"),
    ("data_hash_table_size", 112, 120, "u64"),
    ("field_hash_table_offset", 120, 128, "u64"),
    ("field_hash_table_size", 128, 136, "u64"),
    ("tail_object_offset", 136, 144, "u64"),
    ("n_objects", 144, 152, "u64"),
    ("n_entries", 152, 160, "u64"),
    ("tail_entry_seqnum", 160, 168, "u64"),
    ("head_entry_seqnum", 168, 176, "u64"),
    ("entry_array_offset", 176, 184, "u64"),
    ("head_entry_realtime", 184, 192, "u64"),
    ("tail_entry_realtime", 192, 200, "u64"),
    ("tail_entry_monotonic", 200, 208, "u64"),
    ("n_data", 208, 216, "u64"),
    ("n_fields", 216, 224, "u64"),
    ("n_tags", 224, 232, "u64"),
    ("n_entry_arrays", 232, 240, "u64"),
    ("data_hash_chain_depth", 240, 248, "u64"),
    ("field_hash_chain_depth", 248, 256, "u64"),
    ("tail_entry_array_offset", 256, 260, "u32"),
    ("tail_entry_array_n_entries", 260, 264, "u32"),
    ("tail_entry_offset", 264, 272, "u64"),
)

OBJECT_TYPES = {
    1: "DATA",
    2: "FIELD",
    3: "ENTRY",
    4: "DATA_HASH_TABLE",
    5: "FIELD_HASH_TABLE",
    6: "ENTRY_ARRAY",
    7: "TAG",
}


@dataclass(frozen=True)
class ObjectSpan:
    offset: int
    end: int
    typ: int
    flags: int
    size: int


def run(cmd: list[str]) -> dict:
    try:
        # nosemgrep
        # subprocess is required by this harness; commands are shell=False vectors.
        proc = subprocess.run(  # nosec B603 - harness uses shell=False command vectors.
            cmd,  # nosemgrep
            cwd=ROOT,
            text=True,
            capture_output=True,
            timeout=INGESTER_TIMEOUT_SECONDS,
        )
        return {
            "cmd": cmd,
            "returncode": proc.returncode,
            "stdout": proc.stdout,
            "stderr": proc.stderr,
        }
    except subprocess.TimeoutExpired as err:
        stdout = err.stdout.decode(errors="replace") if isinstance(err.stdout, bytes) else err.stdout or ""
        stderr = err.stderr.decode(errors="replace") if isinstance(err.stderr, bytes) else err.stderr or ""
        return {
            "cmd": cmd,
            "returncode": 124,
            "stdout": stdout,
            "stderr": f"{stderr}\nTimed out after {INGESTER_TIMEOUT_SECONDS} seconds.",
        }


def align8(value: int) -> int:
    return (value + 7) & ~7


def read_u32(buf: bytes, start: int) -> int:
    return int.from_bytes(buf[start : start + 4], "little")


def read_u64(buf: bytes, start: int) -> int:
    return int.from_bytes(buf[start : start + 8], "little")


def header_field_for(offset: int) -> str | None:
    for name, start, end, _kind in HEADER_FIELDS:
        if start <= offset < end:
            return name
    return None


def header_summary(data: bytes) -> dict:
    summary: dict[str, int | str] = {}
    for name, start, end, kind in HEADER_FIELDS:
        if len(data) < end:
            continue
        raw = data[start:end]
        if kind == "u8":
            summary[name] = raw[0]
        elif kind == "u32":
            summary[name] = int.from_bytes(raw, "little")
        elif kind == "u64":
            summary[name] = int.from_bytes(raw, "little")
        else:
            summary[name] = raw.hex()
    return summary


def object_start_offset(data: bytes) -> int:
    header_size = read_u64(data, 88)
    candidates = [
        read_u64(data, offset)
        for offset in (104, 120, 176)
        if len(data) >= offset + 8
    ]
    candidates = [
        candidate
        for candidate in candidates
        if header_size <= candidate < len(data)
    ]
    return min(candidates, default=align8(header_size))


def object_spans(data: bytes) -> list[ObjectSpan]:
    if len(data) < 96:
        return []
    header_size = read_u64(data, 88)
    if header_size <= 0 or header_size >= len(data):
        return []

    spans: list[ObjectSpan] = []
    offset = align8(object_start_offset(data))
    while offset + 16 <= len(data):
        typ = data[offset]
        flags = data[offset + 1]
        size = read_u64(data, offset + 8)
        if typ == 0 and size == 0:
            break
        if size < 16:
            break
        end = offset + align8(size)
        if end > len(data):
            break
        spans.append(ObjectSpan(offset=offset, end=end, typ=typ, flags=flags, size=size))
        offset = end
    return spans


def object_context(data: bytes, offset: int) -> dict:
    field = header_field_for(offset)
    header_size = read_u64(data, 88) if len(data) >= 96 else 0
    if field is not None or offset < header_size:
        return {"region": "header", "field": field or "unknown"}

    for span in object_spans(data):
        if span.offset <= offset < span.end:
            return {
                "region": "object",
                "object_offset": span.offset,
                "object_type": OBJECT_TYPES.get(span.typ, f"UNKNOWN_{span.typ}"),
                "object_flags": span.flags,
                "object_size": span.size,
                "relative_offset": offset - span.offset,
            }

    return {"region": "padding_or_unparsed", "header_size": header_size}


def probable_source(left_context: dict, right_context: dict) -> str:
    if left_context.get("region") == "eof" or right_context.get("region") == "eof":
        return "file size, allocation, or truncation policy"
    if left_context.get("region") == "header" or right_context.get("region") == "header":
        left_field = left_context.get("field")
        right_field = right_context.get("field")
        field = left_field if left_field == right_field else f"{left_field}/{right_field}"
        return f"header metadata field {field}"
    if left_context.get("region") == "object" or right_context.get("region") == "object":
        left_type = left_context.get("object_type")
        right_type = right_context.get("object_type")
        if left_type != right_type:
            return f"object ordering or allocation changed object type {left_type}/{right_type}"
        return f"object payload or metadata changed within {left_type}"
    return "padding, alignment, or currently unparsed region"


def first_differences(left: bytes, right: bytes, *, limit: int) -> list[dict]:
    differences = []
    max_common = min(len(left), len(right))
    for offset in range(max_common):
        if left[offset] == right[offset]:
            continue
        left_context = object_context(left, offset)
        right_context = object_context(right, offset)
        differences.append(
            {
                "offset": offset,
                "left": left[offset],
                "right": right[offset],
                "left_context": left_context,
                "right_context": right_context,
                "probable_source": probable_source(left_context, right_context),
            }
        )
        if len(differences) >= limit:
            return differences

    if len(left) != len(right):
        left_context = object_context(left, max_common) if len(left) > max_common else {"region": "eof"}
        right_context = object_context(right, max_common) if len(right) > max_common else {"region": "eof"}
        differences.append(
            {
                "offset": max_common,
                "left": None if len(left) == max_common else left[max_common],
                "right": None if len(right) == max_common else right[max_common],
                "left_context": left_context,
                "right_context": right_context,
                "probable_source": probable_source(left_context, right_context),
            }
        )
    return differences


def first_accepted_realtime() -> int:
    dataset = ROOT / "tests" / "datasets" / "correctness" / "corpus.jsonl"
    with dataset.open("r", encoding="utf-8") as f:
        for line in f:
            if not line.strip():
                continue
            record = json.loads(line)
            if record.get("record_type") == "accepted":
                return int(record["realtime_usec"])
    return 0


def archive_path_for(output: Path) -> Path:
    prefix = output.name[:-len(".journal")] if output.name.endswith(".journal") else output.name
    return output.with_name(
        f"{prefix}@{SEQNUM_ID}-0000000000000001-{first_accepted_realtime():016x}.journal"
    )


def journal_path(language: str, final_state: str) -> Path:
    output = (OUT / language if final_state == "online" else OUT / final_state / language) / "correctness.journal"
    return archive_path_for(output) if final_state == "archived" else output


def comparison_pairs(reference: str) -> list[tuple[str, str]]:
    pairs: list[tuple[str, str]] = []
    seen: set[tuple[str, str]] = set()

    def add(left: str, right: str) -> None:
        if left == right:
            return
        key = tuple(sorted((left, right)))
        if key in seen:
            return
        seen.add(key)
        pairs.append((left, right))

    for language in LANGUAGES:
        add(reference, language)
    for i, left in enumerate(LANGUAGES):
        for right in LANGUAGES[i + 1:]:
            add(left, right)
    return pairs


def validate_chain_depth(data: bytes, path: Path) -> dict:
    if len(data) < 272:
        return {"returncode": 1, "stderr": "file too small for header"}
    chain_depth = read_u64(data, 240)
    return {
        "returncode": 0,
        "chain_depth": chain_depth,
        "expected_chain_depth": EXPECTED_DATA_HASH_CHAIN_DEPTH,
        "path": str(path),
        "ok": chain_depth == EXPECTED_DATA_HASH_CHAIN_DEPTH,
    }


def compare_pair(left_name: str, right_name: str, *, final_state: str, limit: int) -> dict:
    left_path = journal_path(left_name, final_state)
    right_path = journal_path(right_name, final_state)
    left = left_path.read_bytes()
    right = right_path.read_bytes()
    equal = left == right
    return {
        "left": left_name,
        "right": right_name,
        "equal": equal,
        "left_size": len(left),
        "right_size": len(right),
        "left_header": header_summary(left),
        "right_header": header_summary(right),
        "differences": [] if equal else first_differences(left, right, limit=limit),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-run", action="store_true", help="compare existing .local ingester outputs")
    parser.add_argument("--diff-limit", type=int, default=16)
    parser.add_argument("--reference", choices=LANGUAGES, default=REFERENCE)
    parser.add_argument(
        "--final-state",
        choices=("all", "online", "offline", "archived"),
        default="online",
        help="journal final state to compare; use all to run online, offline, and archived",
    )
    args = parser.parse_args()

    summary: dict[str, object] = {}
    states = ["online", "offline", "archived"] if args.final_state == "all" else [args.final_state]
    state_summaries: dict[str, object] = {}

    for final_state in states:
        state_summary: dict[str, object] = {}
        if not args.skip_run:
            ingest = run([
                sys.executable,
                str(INGESTER_RUNNER),
                "--both",
                "--final-state",
                final_state,
                "--max-size-bytes",
                str(BYTE_IDENTITY_MAX_SIZE_BYTES),
            ])
            state_summary["ingesters"] = ingest
            if ingest["returncode"] != 0:
                state_summaries[final_state] = state_summary
                summary["states"] = state_summaries
                summary["all_equal"] = False
                print(json.dumps(summary, indent=2, sort_keys=True))
                return 1

        paths = {language: str(journal_path(language, final_state)) for language in LANGUAGES}
        missing = [path for path in paths.values() if not Path(path).exists()]
        if missing:
            state_summary["missing"] = missing
            state_summaries[final_state] = state_summary
            summary["states"] = state_summaries
            summary["all_equal"] = False
            print(json.dumps(summary, indent=2, sort_keys=True))
            return 1

        chain_depths = {}
        for language in LANGUAGES:
            path = journal_path(language, final_state)
            data = path.read_bytes()
            result = validate_chain_depth(data, path)
            chain_depths[language] = result
            if result["returncode"] != 0 or not result["ok"]:
                state_summary[f"chain_depth_{language}"] = result
                state_summaries[final_state] = state_summary
                summary["states"] = state_summaries
                summary["all_equal"] = False
                print(json.dumps(summary, indent=2, sort_keys=True))
                return 1
        state_summary["max_size_bytes"] = BYTE_IDENTITY_MAX_SIZE_BYTES
        state_summary["chain_depths"] = {k: v["chain_depth"] for k, v in chain_depths.items()}

        comparisons = [
            compare_pair(left, right, final_state=final_state, limit=args.diff_limit)
            for left, right in comparison_pairs(args.reference)
        ]
        state_summary["paths"] = paths
        state_summary["comparisons"] = comparisons
        state_summary["all_equal"] = all(item["equal"] for item in comparisons)
        state_summaries[final_state] = state_summary

    if len(states) == 1:
        summary.update(state_summaries[states[0]])  # Preserve the original single-state shape.
    else:
        summary["states"] = state_summaries
    summary["all_equal"] = all(state["all_equal"] for state in state_summaries.values())
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0 if summary["all_equal"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
