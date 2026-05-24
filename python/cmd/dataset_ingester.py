#!/usr/bin/env python3
"""Deterministic dataset ingester for the Python journal writer."""

from __future__ import annotations

import argparse
import base64
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "python"))

from journal import Writer  # noqa: E402

BOOT_ID = bytes.fromhex("0123456789abcdef0123456789abcdef")
MACHINE_ID = bytes.fromhex("fedcba9876543210fedcba9876543210")
SEQNUM_ID = bytes.fromhex("22222222222222222222222222222222")
FILE_ID = bytes.fromhex("33333333333333333333333333333333")
OVERSIZED_LIMIT = 4 * 1024 * 1024


def materialize_value(value: dict) -> bytes:
    kind = value.get("kind")
    if kind == "utf8":
        return value["text"].encode("utf-8")
    if kind == "bytes":
        data = base64.b64decode(value["base64"])
        expected = value.get("size")
        if expected is not None and len(data) != expected:
            raise ValueError(f"bytes size mismatch: expected {expected}, got {len(data)}")
        return data
    if kind == "repeat":
        return bytes([value["byte"]]) * value["size"]
    raise ValueError(f"unknown value kind: {kind!r}")


def expected_rejection(input_data: dict) -> str | None:
    if "raw_payload" in input_data:
        raw = input_data["raw_payload"]
        if "=" not in raw:
            return "EINVAL"
        name, _value = raw.split("=", 1)
        if not valid_field_name(name):
            return "EINVAL"
        return None

    name = input_data.get("field_name")
    if name is None or not valid_field_name(name):
        return "EINVAL"
    value = input_data.get("value")
    if value is None:
        return "EINVAL"
    if isinstance(value, dict) and value.get("kind") == "repeat" and value.get("size", 0) > OVERSIZED_LIMIT:
        return "E2BIG"
    return None


def valid_field_name(name: str) -> bool:
    if not name or len(name.encode("utf-8")) > 64:
        return False
    first = name[0]
    if "0" <= first <= "9":
        return False
    return all(ch == "_" or "A" <= ch <= "Z" or "0" <= ch <= "9" for ch in name)


def make_writer(path: Path) -> Writer:
    path.parent.mkdir(parents=True, exist_ok=True)
    return Writer.create(
        str(path),
        {
            "boot_id": BOOT_ID,
            "machine_id": MACHINE_ID,
            "seqnum_id": SEQNUM_ID,
            "file_id": FILE_ID,
            "head_seqnum": 1,
            "compression": "none",
            "compression_threshold_bytes": 64,
        },
    )


def ingest_accepted(dataset: Path, output: Path) -> dict:
    writer = make_writer(output)
    written = 0
    errors: list[str] = []
    try:
        with dataset.open("r", encoding="utf-8") as f:
            for line_no, line in enumerate(f, 1):
                if not line.strip():
                    continue
                record = json.loads(line)
                if record.get("record_type") != "accepted":
                    continue
                fields = [
                    {"name": item["name"], "value": materialize_value(item["value"])}
                    for item in record["fields"]
                ]
                try:
                    writer.append(
                        fields,
                        {
                            "realtime_usec": record["realtime_usec"],
                            "monotonic_usec": record["monotonic_usec"],
                            "boot_id": bytes.fromhex(record.get("boot_id", BOOT_ID.hex())),
                        },
                    )
                    written += 1
                except Exception as err:  # pragma: no cover - command line diagnostic
                    errors.append(f"line {line_no}: append failed: {err}")
        writer.sync()
    finally:
        writer.close()

    return {"records": written, "errors": errors}


def ingest_rejections(dataset: Path, output: Path) -> dict:
    writer: Writer | None = None
    handled = 0
    errors: list[str] = []

    with dataset.open("r", encoding="utf-8") as f:
        for line_no, line in enumerate(f, 1):
            if not line.strip():
                continue
            record = json.loads(line)
            if record.get("record_type") != "rejected":
                continue
            case_id = record["case_id"]
            expected = record["expected_error"]
            input_data = record.get("input", {})
            precheck = expected_rejection(input_data)
            if precheck is not None:
                if precheck == expected:
                    handled += 1
                else:
                    errors.append(f"line {line_no} {case_id}: got {precheck}, expected {expected}")
                continue

            if writer is None:
                writer = make_writer(output)
            name = input_data["field_name"]
            value = materialize_value(input_data["value"])
            try:
                writer.append([{"name": name, "value": value}], {"boot_id": BOOT_ID})
                errors.append(f"line {line_no} {case_id}: unexpectedly accepted")
            except Exception:
                if expected == "EINVAL":
                    handled += 1
                else:
                    errors.append(f"line {line_no} {case_id}: rejected as EINVAL, expected {expected}")

    if writer is not None:
        writer.close()
    return {"records": handled, "errors": errors}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--rejection-mode", action="store_true")
    args = parser.parse_args()

    result = (
        ingest_rejections(args.dataset, args.output)
        if args.rejection_mode
        else ingest_accepted(args.dataset, args.output)
    )
    print(json.dumps(result, sort_keys=True))
    return 0 if not result["errors"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
