#!/usr/bin/env python3
"""Run deterministic ingesters and compare generated journals byte-for-byte."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
INGESTER_RUNNER = ROOT / "tests" / "datasets" / "ingesters" / "run_dataset_ingesters.py"
OUT = ROOT / ".local" / "datasets" / "ingesters"
LANGUAGES = ("systemd", "rust", "go", "node", "python")
REFERENCE = "systemd"
INGESTER_TIMEOUT_SECONDS = 300

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
        proc = subprocess.run(
            cmd,
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


def journal_path(language: str) -> Path:
    return OUT / language / "correctness.journal"


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
    for language in ("node", "python", "rust"):
        add("go", language)
    return pairs


def compare_pair(left_name: str, right_name: str, *, limit: int) -> dict:
    left_path = journal_path(left_name)
    right_path = journal_path(right_name)
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
    args = parser.parse_args()

    summary: dict[str, object] = {}
    if not args.skip_run:
        ingest = run([sys.executable, str(INGESTER_RUNNER), "--both"])
        summary["ingesters"] = ingest
        if ingest["returncode"] != 0:
            print(json.dumps(summary, indent=2, sort_keys=True))
            return 1

    paths = {language: str(journal_path(language)) for language in LANGUAGES}
    missing = [path for path in paths.values() if not Path(path).exists()]
    if missing:
        summary["missing"] = missing
        print(json.dumps(summary, indent=2, sort_keys=True))
        return 1

    comparisons = [
        compare_pair(left, right, limit=args.diff_limit)
        for left, right in comparison_pairs(args.reference)
    ]
    summary["paths"] = paths
    summary["comparisons"] = comparisons
    summary["all_equal"] = all(item["equal"] for item in comparisons)
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0 if summary["all_equal"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
