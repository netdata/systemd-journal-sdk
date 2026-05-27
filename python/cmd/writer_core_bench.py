#!/usr/bin/env python3
"""Measure the Python writer append loop without JSON ingestion overhead."""

from __future__ import annotations

import argparse
import json
import os
import time
from pathlib import Path

from journal.writer import COMPRESSION_NONE, Writer


BASE_REALTIME_USEC = 1_700_000_000_000_000
BASE_MONOTONIC_USEC = 50_000_000
SEQNUM_ID_HEX = "22222222222222222222222222222222"
FIELDS_PER_ROW = 32
DEFAULT_MAX_SIZE_BYTES = 128 * 1024 * 1024
FIELD_HASH_BUCKETS = 1023

BOOT_ID = bytes.fromhex("0123456789abcdef0123456789abcdef")
MACHINE_ID = bytes.fromhex("fedcba9876543210fedcba9876543210")
SEQNUM_ID = bytes.fromhex(SEQNUM_ID_HEX)
FILE_ID = bytes.fromhex("33333333333333333333333333333333")


def data_hash_buckets_for_max_size(max_size_bytes: int) -> int:
    # Keep this driver aligned with header.py and systemd's max_size * 4 / 768 / 3 formula.
    return max(max_size_bytes // 576, 2047)


def make_rows(rows: int) -> list[list[dict[str, bytes | str]]]:
    fixed = [
        {"name": "TEST_ID", "value": b"deterministic-ingestion-performance"},
        {"name": "PERF_PROFILE", "value": b"mixed-cardinality-32-fields"},
        {"name": "HOST_CLASS", "value": b"synthetic-edge"},
        {"name": "SOURCE_KIND", "value": b"journal-sdk-benchmark"},
    ]
    low_values = [
        [f"low-{offset:02d}-{value:02d}".encode() for value in range(16)]
        for offset in range(12)
    ]
    medium_values = [
        [f"medium-{offset:02d}-{value:04d}".encode() for value in range(2048)]
        for offset in range(8)
    ]

    all_rows: list[list[dict[str, bytes | str]]] = []
    for row in range(rows):
        fields = list(fixed)
        for offset in range(12):
            fields.append({
                "name": f"LOW_CARD_{offset:02d}",
                "value": low_values[offset][row % 16],
            })
        for offset in range(8):
            fields.append({
                "name": f"MED_CARD_{offset:02d}",
                "value": medium_values[offset][row % 2048],
            })
        for offset in range(8):
            fields.append({
                "name": f"HIGH_CARD_{offset:02d}",
                "value": f"high-{offset:02d}-{row:06d}".encode(),
            })
        all_rows.append(fields)
    return all_rows


def archive_path_for(output: Path) -> Path:
    prefix = output.with_suffix("") if output.suffix == ".journal" else output
    return prefix.with_name(
        f"{prefix.name}@{SEQNUM_ID_HEX}-0000000000000001-{BASE_REALTIME_USEC:016x}.journal"
    )


def close_writer(writer: Writer, output: Path, final_state: str) -> Path:
    if final_state == "online":
        writer.close()
        return output
    if final_state == "offline":
        writer.close_offline()
        return output
    if final_state == "archived":
        archive_path = archive_path_for(output)
        archive_path.unlink(missing_ok=True)
        writer.archive_to(str(archive_path))
        return archive_path
    raise ValueError(f"invalid final state: {final_state}")


def result_template(args: argparse.Namespace) -> dict[str, object]:
    return {
        "records": 0,
        "fields_per_row": FIELDS_PER_ROW,
        "append_seconds": 0.0,
        "append_rows_per_second": 0.0,
        "close_seconds": 0.0,
        "total_writer_seconds": 0.0,
        "precompute_seconds": 0.0,
        "journal_size_bytes": 0,
        "journal_path": "",
        "format": args.format,
        "compression": "none",
        "fss": False,
        "api_mode": "field-api",
        "data_hash_table_buckets": data_hash_buckets_for_max_size(args.max_size_bytes),
        "field_hash_table_buckets": FIELD_HASH_BUCKETS,
        "max_size_bytes": args.max_size_bytes,
        "append_timer_excludes": [
            "row generation",
            "writer creation",
            "final close/sync",
            "journal verification",
        ],
        "final_state": args.final_state,
        "errors": [],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--rows", type=int, default=100_000)
    parser.add_argument("--format", choices=("compact", "regular"), default="compact")
    parser.add_argument("--final-state", choices=("online", "offline", "archived"), default="online")
    parser.add_argument("--max-size-bytes", type=int, default=DEFAULT_MAX_SIZE_BYTES)
    args = parser.parse_args()

    result = result_template(args)
    compact = args.format == "compact"

    precompute_start = time.perf_counter()
    rows = make_rows(args.rows)
    result["precompute_seconds"] = time.perf_counter() - precompute_start

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.unlink(missing_ok=True)
    writer = Writer.create(str(args.output), {
        "machine_id": MACHINE_ID,
        "boot_id": BOOT_ID,
        "seqnum_id": SEQNUM_ID,
        "file_id": FILE_ID,
        "head_seqnum": 1,
        "compression": COMPRESSION_NONE,
        "compression_threshold_bytes": 512,
        "data_hash_table_buckets": result["data_hash_table_buckets"],
        "field_hash_table_buckets": result["field_hash_table_buckets"],
        "compact": compact,
    })

    try:
        append_start = time.perf_counter()
        for index, fields in enumerate(rows):
            writer.append(fields, {
                "realtime_usec": BASE_REALTIME_USEC + index * 500,
                "monotonic_usec": BASE_MONOTONIC_USEC + index * 50,
                "boot_id": BOOT_ID,
            })
            result["records"] = int(result["records"]) + 1
        result["append_seconds"] = time.perf_counter() - append_start
        if result["append_seconds"]:
            result["append_rows_per_second"] = int(result["records"]) / float(result["append_seconds"])

        close_start = time.perf_counter()
        journal_path = close_writer(writer, args.output, args.final_state)
        result["close_seconds"] = time.perf_counter() - close_start
        result["total_writer_seconds"] = float(result["append_seconds"]) + float(result["close_seconds"])
        result["journal_path"] = str(journal_path)
        result["journal_size_bytes"] = journal_path.stat().st_size
    except Exception as err:
        result["errors"].append(str(err))  # type: ignore[index]
        try:
            writer.close()
        except Exception:
            pass

    print(json.dumps(result, sort_keys=True))
    return 0 if not result["errors"] and result["records"] == args.rows else 1


if __name__ == "__main__":
    raise SystemExit(main())
