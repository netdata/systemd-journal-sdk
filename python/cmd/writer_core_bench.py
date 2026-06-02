#!/usr/bin/env python3
"""Measure the Python writer append loop without JSON ingestion overhead."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

from journal.writer import COMPRESSION_NONE, Writer
from journal.directory_writer import LOG_IDENTITY_STRICT, Log


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


def live_publication_name(every_entries: int) -> str:
    if every_entries == 0:
        return "disabled"
    if every_entries == 1:
        return "immediate"
    return f"every-n:{every_entries}"


def field_with_payload(name: str, value: bytes) -> tuple[dict[str, bytes | str], bytes]:
    return {"name": name, "value": value}, name.encode("ascii") + b"=" + value


def make_rows(rows: int) -> list[dict[str, list[dict[str, bytes | str]] | list[bytes]]]:
    fixed = [
        field_with_payload("TEST_ID", b"deterministic-ingestion-performance"),
        field_with_payload("PERF_PROFILE", b"mixed-cardinality-32-fields"),
        field_with_payload("HOST_CLASS", b"synthetic-edge"),
        field_with_payload("SOURCE_KIND", b"journal-sdk-benchmark"),
    ]
    low_values = [
        [f"low-{offset:02d}-{value:02d}".encode() for value in range(16)]
        for offset in range(12)
    ]
    medium_values = [
        [f"medium-{offset:02d}-{value:04d}".encode() for value in range(2048)]
        for offset in range(8)
    ]

    all_rows: list[dict[str, list[dict[str, bytes | str]] | list[bytes]]] = []
    for row in range(rows):
        fields = list(fixed)
        for offset in range(12):
            fields.append(field_with_payload(
                f"LOW_CARD_{offset:02d}",
                low_values[offset][row % 16],
            ))
        for offset in range(8):
            fields.append(field_with_payload(
                f"MED_CARD_{offset:02d}",
                medium_values[offset][row % 2048],
            ))
        for offset in range(8):
            fields.append(field_with_payload(
                f"HIGH_CARD_{offset:02d}",
                f"high-{offset:02d}-{row:06d}".encode(),
            ))
        all_rows.append({
            "fields": [field for field, _payload in fields],
            "payloads": [payload for _field, payload in fields],
        })
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
        "surface": args.surface,
        "append_seconds": 0.0,
        "append_rows_per_second": 0.0,
        "close_seconds": 0.0,
        "total_writer_seconds": 0.0,
        "precompute_seconds": 0.0,
        "journal_size_bytes": 0,
        "journal_path": "",
        "journal_directory": "",
        "journal_files": [],
        "format": args.format,
        "compression": "none",
        "fss": False,
        "api_mode": args.api_mode,
        "data_hash_table_buckets": data_hash_buckets_for_max_size(args.max_size_bytes),
        "field_hash_table_buckets": FIELD_HASH_BUCKETS,
        "max_size_bytes": args.max_size_bytes,
        "rotation_max_size_bytes": args.rotation_max_size_bytes,
        "live_publication": live_publication_name(args.live_publish_every_entries),
        "live_publish_every_entries": args.live_publish_every_entries,
        "append_timer_excludes": [
            "row generation",
            "writer creation",
            "final close/sync",
            "journal verification",
        ],
        "final_state": args.final_state,
        "errors": [],
    }


def collect_journal_files(root: Path) -> tuple[list[str], int]:
    files: list[str] = []
    total = 0
    for path in root.rglob("*.journal"):
        if path.is_file():
            files.append(str(path))
            total += path.stat().st_size
    return files, total


def run_directory(result: dict[str, object], args: argparse.Namespace, rows) -> None:
    import shutil

    shutil.rmtree(args.output, ignore_errors=True)
    log = Log(str(args.output), {
        "source": "system",
        "machine_id": MACHINE_ID,
        "boot_id": BOOT_ID,
        "seqnum_id": SEQNUM_ID,
        "head_seqnum": 1,
        "identity_mode": LOG_IDENTITY_STRICT,
        "compression": COMPRESSION_NONE,
        "compression_threshold_bytes": 512,
        "compact": args.format == "compact",
        "live_publish_every_entries": args.live_publish_every_entries,
        "rotation_policy": {"max_file_size": args.rotation_max_size_bytes},
    })
    append_start = time.perf_counter()
    for index, row in enumerate(rows):
        opts = {
            "realtime_usec": BASE_REALTIME_USEC + index * 500,
            "monotonic_usec": BASE_MONOTONIC_USEC + index * 50,
            "boot_id": BOOT_ID,
        }
        if args.api_mode == "raw-payload":
            log.append_raw(row["payloads"], opts)
        else:
            log.append(row["fields"], opts)
        result["records"] = int(result["records"]) + 1
    result["append_seconds"] = time.perf_counter() - append_start
    if result["append_seconds"]:
        result["append_rows_per_second"] = int(result["records"]) / float(result["append_seconds"])

    close_start = time.perf_counter()
    log.close()
    result["close_seconds"] = time.perf_counter() - close_start
    result["total_writer_seconds"] = float(result["append_seconds"]) + float(result["close_seconds"])
    result["journal_directory"] = log.journal_directory()
    result["journal_path"] = result["journal_directory"]
    files, total = collect_journal_files(args.output)
    result["journal_files"] = files
    result["journal_size_bytes"] = total


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--rows", type=int, default=100_000)
    parser.add_argument("--format", choices=("compact", "regular"), default="compact")
    parser.add_argument("--final-state", choices=("online", "offline", "archived"), default="online")
    parser.add_argument("--surface", choices=("direct", "directory"), default="direct")
    parser.add_argument("--max-size-bytes", type=int, default=DEFAULT_MAX_SIZE_BYTES)
    parser.add_argument("--rotation-max-size-bytes", type=int, default=DEFAULT_MAX_SIZE_BYTES)
    parser.add_argument("--live-publish-every-entries", type=int, default=1)
    parser.add_argument("--api-mode", choices=("raw-payload", "structured-field"), default="raw-payload")
    args = parser.parse_args()

    result = result_template(args)
    compact = args.format == "compact"

    precompute_start = time.perf_counter()
    rows = make_rows(args.rows)
    result["precompute_seconds"] = time.perf_counter() - precompute_start

    if args.surface == "directory":
        try:
            run_directory(result, args, rows)
        except Exception as err:
            result["errors"].append(str(err))  # type: ignore[index]
        print(json.dumps(result, sort_keys=True))
        return 0 if not result["errors"] and result["records"] == args.rows else 1

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
        "live_publish_every_entries": args.live_publish_every_entries,
        "compact": compact,
    })

    try:
        append_start = time.perf_counter()
        for index, row in enumerate(rows):
            opts = {
                "realtime_usec": BASE_REALTIME_USEC + index * 500,
                "monotonic_usec": BASE_MONOTONIC_USEC + index * 50,
                "boot_id": BOOT_ID,
            }
            if args.api_mode == "raw-payload":
                writer.append_raw(row["payloads"], opts)
            else:
                writer.append(row["fields"], opts)
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
