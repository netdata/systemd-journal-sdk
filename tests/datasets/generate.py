#!/usr/bin/env python3
"""Generate deterministic ingestion datasets for journal writer tests."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
from pathlib import Path
from typing import Iterable, Iterator


DATASET_VERSION = "1.0.0"
SCHEMA_VERSION = "1.0.0"
SEED = 0x5D17A5EED
BASE_REALTIME_USEC = 1_700_000_000_000_000
BASE_MONOTONIC_USEC = 50_000_000
BOOT_ID = "0123456789abcdef0123456789abcdef"
MACHINE_ID = "fedcba9876543210fedcba9876543210"
INVOCATION_ID = "11112222333344445555666677778888"
SYSTEMD_COMMIT = "c0a5a2516d28601fb3afc1a77d7b42fcfe38fced"
DEFAULT_PERFORMANCE_ROWS = 200_000

ROOT = Path(__file__).resolve().parent

CORRECTNESS_REQUIRED = [
    "new-field-objects",
    "reused-field-objects",
    "new-data-objects",
    "reused-data-objects",
    "duplicate-fields-in-entry",
    "duplicate-values-across-entries",
    "binary-field-values",
    "embedded-nul-value",
    "zero-length-value",
    "large-value",
    "compression-threshold-values",
    "high-cardinality-fields",
    "low-cardinality-fields",
    "hash-collision-pressure",
    "hash-collision-chain",
    "entry-array-growth",
    "data-entry-array-growth",
    "sorted-entry-item-ordering",
]

REJECTION_REQUIRED = [
    "empty-field-name",
    "lowercase-field-name",
    "field-name-starts-with-digit",
    "field-name-too-long",
    "field-name-invalid-character",
    "missing-equals",
    "empty-full-data-payload",
    "null-field-payload",
    "value-exceeds-limit",
]


def canonical_json(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(canonical_json(value) + "\n", encoding="utf-8")


def utf8(value: str) -> dict[str, object]:
    return {"kind": "utf8", "text": value}


def raw_bytes(value: bytes) -> dict[str, object]:
    return {
        "kind": "bytes",
        "base64": base64.b64encode(value).decode("ascii"),
        "size": len(value),
    }


def repeat(byte: int, size: int, preview: str | None = None) -> dict[str, object]:
    if not 0 <= byte <= 255:
        raise ValueError("repeat byte must be in range 0..255")
    if size < 0:
        raise ValueError("repeat size must be non-negative")
    if preview is None:
        preview = chr(byte) * min(size, 16)
    return {"kind": "repeat", "byte": byte, "size": size, "preview": preview}


def field(
    name: str,
    value: dict[str, object],
    cardinality: str = "medium",
    note: str | None = None,
) -> dict[str, object]:
    item: dict[str, object] = {
        "name": name,
        "value": value,
        "cardinality": cardinality,
    }
    if note:
        item["note"] = note
    return item


def entry(
    index: int,
    fields: list[dict[str, object]],
    coverage: list[str],
    note: str | None = None,
) -> dict[str, object]:
    names: dict[str, int] = {}
    for item in fields:
        name = str(item["name"])
        names[name] = names.get(name, 0) + 1

    record: dict[str, object] = {
        "record_type": "accepted",
        "entry_id": f"acc-{index:06d}",
        "entry_index": index,
        "realtime_usec": BASE_REALTIME_USEC + index * 1_000,
        "monotonic_usec": BASE_MONOTONIC_USEC + index * 100,
        "boot_id": BOOT_ID,
        "machine_id": MACHINE_ID,
        "invocation_id": INVOCATION_ID,
        "fields": fields,
        "duplicate_field_names": sorted(name for name, count in names.items() if count > 1),
        "coverage": sorted(set(coverage)),
        "expected_outcome": "accept",
    }
    if note:
        record["note"] = note
    return record


def base_fields(index: int) -> list[dict[str, object]]:
    return [
        field("MESSAGE", utf8(f"deterministic correctness {index:06d}"), "high"),
        field("PRIORITY", utf8(str(index % 8)), "low"),
        field("TEST_ID", utf8("deterministic-ingestion-correctness"), "fixed"),
        field("LIVE_SEQ", utf8(f"{index:06d}"), "high"),
        field("LOW_CARDINALITY", utf8(["alpha", "beta", "gamma", "delta"][index % 4]), "low"),
        field("HIGH_CARDINALITY", utf8(f"hc-{index:06d}"), "high"),
        field("REUSED_DATA", utf8("shared-data-object"), "fixed"),
    ]


def correctness_records() -> list[dict[str, object]]:
    records: list[dict[str, object]] = []

    records.append(
        entry(
            0,
            base_fields(0)
            + [
                field("FIRST_UNIQUE_FIELD", utf8("first-value"), "single"),
                field("DATA_UNIQUE_000", utf8("unique-data-000"), "high"),
            ],
            [
                "new-field-objects",
                "new-data-objects",
                "reused-data-objects",
                "low-cardinality-fields",
                "high-cardinality-fields",
            ],
            "Initial entry creates common FIELD and DATA objects.",
        )
    )

    records.append(
        entry(
            1,
            base_fields(1)
            + [
                field("FIRST_UNIQUE_FIELD", utf8("second-value"), "single"),
                field("DATA_UNIQUE_001", utf8("unique-data-001"), "high"),
            ],
            [
                "reused-field-objects",
                "new-data-objects",
                "reused-data-objects",
                "duplicate-values-across-entries",
            ],
            "Reuses field names while introducing new DATA objects.",
        )
    )

    records.append(
        entry(
            2,
            base_fields(2)
            + [
                field("DUPLICATE_FIELD", utf8("first"), "medium"),
                field("DUPLICATE_FIELD", utf8("second"), "medium"),
                field("DUPLICATE_FIELD", utf8("first"), "medium"),
            ],
            ["duplicate-fields-in-entry", "reused-data-objects"],
            "Three same-name fields in one entry, including a repeated same DATA value.",
        )
    )

    records.append(
        entry(
            3,
            base_fields(3)
            + [
                field("BINARY_PAYLOAD", raw_bytes(bytes([0, 1, 2, 65, 10, 127, 128, 255])), "medium"),
                field("BINARY_WITH_NUL", raw_bytes(b"left\x00middle\x00right"), "medium"),
                field("EMPTY_BINARY", raw_bytes(b""), "low"),
                field("EMPTY_TEXT", utf8(""), "low"),
            ],
            ["binary-field-values", "embedded-nul-value", "zero-length-value"],
            "Binary payloads, embedded NUL bytes, and zero-length values.",
        )
    )

    records.append(
        entry(
            4,
            base_fields(4)
            + [
                field("LARGE_REPEAT_VALUE", repeat(ord("L"), 65_536), "medium"),
                field("NEAR_THRESHOLD_511", repeat(ord("A"), 511), "medium"),
                field("NEAR_THRESHOLD_512", repeat(ord("B"), 512), "medium"),
                field("NEAR_THRESHOLD_513", repeat(ord("C"), 513), "medium"),
                field("COMPRESSIBLE_4096", repeat(ord("Z"), 4_096), "medium"),
            ],
            ["large-value", "compression-threshold-values"],
            "Repeat descriptors keep the committed corpus compact while forcing ingesters to materialize large values.",
        )
    )

    records.append(
        entry(
            5,
            [
                field("ZZZ_ORDER_PROBE", utf8("input-first"), "medium"),
                field("AAA_ORDER_PROBE", utf8("input-second"), "medium"),
                field("MMM_ORDER_PROBE", utf8("input-third"), "medium"),
                *base_fields(5),
            ],
            ["sorted-entry-item-ordering"],
            "Input order is intentionally not lexicographic; later byte-identity tests compare systemd entry-item ordering.",
        )
    )

    pressure_fields = base_fields(6)
    for i in range(64):
        pressure_fields.append(field(f"HASH_PRESSURE_{i:02d}", utf8(f"bucket-pressure-{i:02d}"), "high"))
    records.append(
        entry(
            6,
            pressure_fields,
            ["hash-collision-pressure", "new-field-objects", "new-data-objects"],
            "Many similarly shaped DATA objects pressure writer hash tables without relying on fabricated hash collisions.",
        )
    )

    collision_entries = [
        ("AA", "cv-0299"),
        ("AC", "cv-0163"),
        ("AZ", "cv-0168"),
        ("BB", "cv-0245"),
    ]
    collision_fields = base_fields(7)[:]
    for fn, val in collision_entries:
        collision_fields.append(field(fn, utf8(val), "high"))
    records.append(
        entry(
            7,
            collision_fields,
            ["hash-collision-chain", "new-field-objects", "new-data-objects"],
            "Deterministic DATA hash-bucket collision chain: bucket 85984, 4 unique colliding payloads, next_hash_offset chain traversal, data_hash_chain_depth publication.",
        )
    )

    dup_collision_fields = base_fields(8)[:]
    dup_collision_fields.append(field("AA", utf8("cv-0299"), "high"))
    dup_collision_fields.append(field("AC", utf8("cv-0163"), "high"))
    dup_collision_fields.append(field("AZ", utf8("cv-0168"), "high"))
    dup_collision_fields.append(field("BB", utf8("cv-0245"), "high"))
    dup_collision_fields.append(field("DUPLICATE_AA", utf8("cv-0299"), "high"))
    records.append(
        entry(
            8,
            dup_collision_fields,
            ["hash-collision-chain", "reused-field-objects", "reused-data-objects"],
            "Duplicate of AA=cv-0299 after chain is established: lookup traversal updates data_hash_chain_depth to the chain depth.",
        )
    )

    next_index = 9
    for i in range(80):
        records.append(
            entry(
                next_index,
                base_fields(next_index)
                + [
                    field(f"FIELD_GROWTH_{i:03d}", utf8(f"field-growth-value-{i:03d}"), "high"),
                    field("DATA_ARRAY_SHARED", utf8("shared-entry-array-target"), "fixed"),
                ],
                [
                    "new-field-objects",
                    "reused-field-objects",
                    "entry-array-growth",
                    "data-entry-array-growth",
                    "high-cardinality-fields",
                ],
            )
        )
        next_index += 1

    for i in range(260):
        records.append(
            entry(
                next_index,
                base_fields(next_index)
                + [
                    field("DATA_ARRAY_SHARED", utf8("shared-entry-array-target"), "fixed"),
                    field("DUPLICATE_VALUE_LOW", utf8(f"low-{i % 5}"), "low"),
                    field("ROW_VARIANT", utf8(f"variant-{i:03d}"), "high"),
                ],
                [
                    "reused-field-objects",
                    "reused-data-objects",
                    "duplicate-values-across-entries",
                    "entry-array-growth",
                    "data-entry-array-growth",
                    "low-cardinality-fields",
                    "high-cardinality-fields",
                ],
            )
        )
        next_index += 1

    return records


def rejection_records() -> list[dict[str, object]]:
    cases = [
        (
            "rej-empty-field-name",
            {"raw_payload": "=value"},
            ["empty-field-name"],
            "EINVAL",
            "Empty field name before '='.",
        ),
        (
            "rej-lowercase-field-name",
            {"field_name": "lowercase", "value": utf8("value")},
            ["lowercase-field-name"],
            "EINVAL",
            "systemd journal field names must be uppercase, digits, or underscores.",
        ),
        (
            "rej-digit-prefix",
            {"field_name": "1INVALID", "value": utf8("value")},
            ["field-name-starts-with-digit"],
            "EINVAL",
            "Field names must not start with a digit.",
        ),
        (
            "rej-name-too-long",
            {"field_name": "A" * 65, "value": utf8("value")},
            ["field-name-too-long"],
            "EINVAL",
            "Field names longer than 64 bytes are rejected.",
        ),
        (
            "rej-invalid-character",
            {"field_name": "BAD-NAME", "value": utf8("value")},
            ["field-name-invalid-character"],
            "EINVAL",
            "Hyphen is not valid in a journal field name.",
        ),
        (
            "rej-missing-equals",
            {"raw_payload": "NO_EQUALS_PAYLOAD"},
            ["missing-equals"],
            "EINVAL",
            "A DATA payload must contain '=' separating field name and value.",
        ),
        (
            "rej-empty-payload",
            {"raw_payload": ""},
            ["empty-full-data-payload"],
            "EINVAL",
            "An empty DATA payload has no field name and no '='.",
        ),
        (
            "rej-null-field-payload",
            {"field_name": "NULL_PAYLOAD", "value": None},
            ["null-field-payload"],
            "EINVAL",
            "Null is not a byte payload; adapters must reject or make this unrepresentable.",
        ),
        (
            "rej-value-exceeds-limit",
            {"field_name": "TOO_LARGE", "value": repeat(ord("X"), 4 * 1024 * 1024 + 1)},
            ["value-exceeds-limit"],
            "E2BIG",
            "Synthetic oversized payload for SDK limit parity; not part of byte-identical accepted corpus.",
        ),
    ]

    records = []
    for index, (case_id, input_value, coverage, error, note) in enumerate(cases):
        records.append(
            {
                "record_type": "rejected",
                "case_id": case_id,
                "case_index": index,
                "input": input_value,
                "coverage": coverage,
                "expected_outcome": "reject",
                "expected_error": error,
                "note": note,
            }
        )
    return records


PERF_FIXED_FIELDS = [
    "TEST_ID",
    "PERF_PROFILE",
    "HOST_CLASS",
    "SOURCE_KIND",
]
PERF_LOW_FIELDS = [f"LOW_CARD_{i:02d}" for i in range(12)]
PERF_MEDIUM_FIELDS = [f"MED_CARD_{i:02d}" for i in range(8)]
PERF_HIGH_FIELDS = [f"HIGH_CARD_{i:02d}" for i in range(8)]
PERF_FIELD_NAMES = PERF_FIXED_FIELDS + PERF_LOW_FIELDS + PERF_MEDIUM_FIELDS + PERF_HIGH_FIELDS


def performance_record(index: int) -> dict[str, object]:
    fields: list[dict[str, object]] = [
        field("TEST_ID", utf8("deterministic-ingestion-performance"), "fixed"),
        field("PERF_PROFILE", utf8("mixed-cardinality-32-fields"), "fixed"),
        field("HOST_CLASS", utf8("synthetic-edge"), "fixed"),
        field("SOURCE_KIND", utf8("journal-sdk-benchmark"), "fixed"),
    ]

    for offset, name in enumerate(PERF_LOW_FIELDS):
        fields.append(field(name, utf8(f"low-{offset:02d}-{index % 16:02d}"), "low"))

    for offset, name in enumerate(PERF_MEDIUM_FIELDS):
        fields.append(field(name, utf8(f"medium-{offset:02d}-{index % 2048:04d}"), "medium"))

    for offset, name in enumerate(PERF_HIGH_FIELDS):
        fields.append(field(name, utf8(f"high-{offset:02d}-{index:06d}"), "high"))

    return {
        "record_type": "accepted",
        "entry_id": f"perf-{index:06d}",
        "entry_index": index,
        "realtime_usec": BASE_REALTIME_USEC + index * 500,
        "monotonic_usec": BASE_MONOTONIC_USEC + index * 50,
        "boot_id": BOOT_ID,
        "machine_id": MACHINE_ID,
        "invocation_id": INVOCATION_ID,
        "fields": fields,
        "duplicate_field_names": [],
        "coverage": [
            "performance-200k-rows",
            "performance-32-fields",
            "performance-mixed-cardinality",
        ],
        "expected_outcome": "accept",
    }


def performance_records(rows: int = DEFAULT_PERFORMANCE_ROWS) -> Iterator[dict[str, object]]:
    for index in range(rows):
        yield performance_record(index)


def write_jsonl(path: Path, records: Iterable[dict[str, object]]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    digest = hashlib.sha256()
    with path.open("w", encoding="utf-8", newline="\n") as handle:
        for record in records:
            line = canonical_json(record) + "\n"
            handle.write(line)
            digest.update(line.encode("utf-8"))
    return digest.hexdigest()


def stream_hash(records: Iterable[dict[str, object]]) -> tuple[str, int]:
    digest = hashlib.sha256()
    count = 0
    for record in records:
        digest.update((canonical_json(record) + "\n").encode("utf-8"))
        count += 1
    return digest.hexdigest(), count


def build_performance_manifest(rows: int = DEFAULT_PERFORMANCE_ROWS) -> dict[str, object]:
    digest, count = stream_hash(performance_records(rows))
    return {
        "dataset_version": DATASET_VERSION,
        "corpus_type": "performance",
        "record_count": count,
        "fields_per_row": {"target": 32, "min": 32, "max": 32},
        "cardinality_profile": {
            "fixed_fields": len(PERF_FIXED_FIELDS),
            "low_cardinality_fields": len(PERF_LOW_FIELDS),
            "medium_cardinality_fields": len(PERF_MEDIUM_FIELDS),
            "high_cardinality_fields": len(PERF_HIGH_FIELDS),
        },
        "field_names": PERF_FIELD_NAMES,
        "stream_sha256": digest,
        "generator": "tests/datasets/generate.py performance",
        "generation_parameters": {
            "seed": SEED,
            "rows": rows,
            "base_realtime_usec": BASE_REALTIME_USEC,
            "base_monotonic_usec": BASE_MONOTONIC_USEC,
        },
        "materialized_output": ".local/datasets/performance-corpus.jsonl",
        "note": "The 200k-row JSONL performance corpus is generated on demand and is not committed.",
    }


def build_ingestion_manifest(
    correctness_hash: str,
    rejection_hash: str,
    correctness_count: int,
    rejection_count: int,
    performance_manifest: dict[str, object],
) -> dict[str, object]:
    return {
        "dataset_version": DATASET_VERSION,
        "schema_version": SCHEMA_VERSION,
        "systemd_baseline": {
            "repo": "systemd/systemd",
            "tag": "v260.1",
            "commit": SYSTEMD_COMMIT,
        },
        "determinism": {
            "seed": SEED,
            "base_realtime_usec": BASE_REALTIME_USEC,
            "base_monotonic_usec": BASE_MONOTONIC_USEC,
            "boot_id": BOOT_ID,
            "machine_id": MACHINE_ID,
            "invocation_id": INVOCATION_ID,
            "json": "UTF-8, sorted object keys, compact separators, one JSON record per line",
        },
        "corpora": {
            "correctness": {
                "path": "tests/datasets/correctness/corpus.jsonl",
                "record_count": correctness_count,
                "sha256": correctness_hash,
                "generated_by": "python3 tests/datasets/generate.py committed",
            },
            "rejections": {
                "path": "tests/datasets/rejections/corpus.jsonl",
                "record_count": rejection_count,
                "sha256": rejection_hash,
                "generated_by": "python3 tests/datasets/generate.py committed",
            },
            "performance": {
                "path": "tests/datasets/performance/manifest.json",
                "record_count": int(performance_manifest["record_count"]),
                "sha256": str(performance_manifest["stream_sha256"]),
                "generated_by": "python3 tests/datasets/generate.py performance --output .local/datasets/performance-corpus.jsonl",
            },
        },
        "coverage": {
            "correctness_required": CORRECTNESS_REQUIRED,
            "rejections_required": REJECTION_REQUIRED,
        },
    }


def generate_committed(output_root: Path = ROOT) -> None:
    correctness_path = output_root / "correctness" / "corpus.jsonl"
    rejections_path = output_root / "rejections" / "corpus.jsonl"
    performance_manifest_path = output_root / "performance" / "manifest.json"
    ingestion_manifest_path = output_root / "ingestion-manifest.json"

    accepted = correctness_records()
    rejected = rejection_records()
    correctness_hash = write_jsonl(correctness_path, accepted)
    rejection_hash = write_jsonl(rejections_path, rejected)
    performance_manifest = build_performance_manifest()
    write_json(performance_manifest_path, performance_manifest)
    manifest = build_ingestion_manifest(
        correctness_hash,
        rejection_hash,
        len(accepted),
        len(rejected),
        performance_manifest,
    )
    write_json(ingestion_manifest_path, manifest)


def generate_performance(output: Path, rows: int) -> None:
    write_jsonl(output, performance_records(rows))


def print_performance_hash(rows: int) -> None:
    digest, count = stream_hash(performance_records(rows))
    print(canonical_json({"record_count": count, "sha256": digest}))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    committed = subparsers.add_parser("committed", help="Regenerate committed correctness, rejection, and manifest files.")
    committed.add_argument("--output-root", type=Path, default=ROOT)

    perf = subparsers.add_parser("performance", help="Generate the large performance corpus as JSONL.")
    perf.add_argument("--output", type=Path, required=True)
    perf.add_argument("--rows", type=int, default=DEFAULT_PERFORMANCE_ROWS)

    perf_hash = subparsers.add_parser("performance-hash", help="Hash the large performance corpus without writing it.")
    perf_hash.add_argument("--rows", type=int, default=DEFAULT_PERFORMANCE_ROWS)

    args = parser.parse_args()
    if args.command == "committed":
        generate_committed(args.output_root)
    elif args.command == "performance":
        generate_performance(args.output, args.rows)
    elif args.command == "performance-hash":
        print_performance_hash(args.rows)
    else:
        parser.error(f"unsupported command: {args.command}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
