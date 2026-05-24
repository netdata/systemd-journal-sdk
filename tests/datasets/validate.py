#!/usr/bin/env python3
"""Validate the deterministic ingestion dataset."""

from __future__ import annotations

import hashlib
import json
import re
import shutil
import subprocess
import sys
import base64
from pathlib import Path
from typing import Iterable

try:
    import jsonschema
except ImportError:  # pragma: no cover - report a clear validation dependency error.
    jsonschema = None


ROOT = Path(__file__).resolve().parent
REPO_ROOT = ROOT.parents[1]
MANIFEST_PATH = ROOT / "ingestion-manifest.json"
SCHEMA_PATH = ROOT / "schema.schema.json"
CORRECTNESS_PATH = ROOT / "correctness" / "corpus.jsonl"
REJECTIONS_PATH = ROOT / "rejections" / "corpus.jsonl"
PERFORMANCE_MANIFEST_PATH = ROOT / "performance" / "manifest.json"
VALIDATION_DIR = REPO_ROOT / ".local" / "datasets" / "validation"

FIELD_RE = re.compile(r"^[A-Z_][A-Z0-9_]{0,63}$")


def load_json(path: Path) -> object:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def load_jsonl(path: Path) -> list[dict[str, object]]:
    records = []
    with path.open("r", encoding="utf-8") as handle:
        for line_no, line in enumerate(handle, 1):
            line = line.rstrip("\n")
            if not line:
                raise ValueError(f"{path}:{line_no}: empty JSONL line")
            value = json.loads(line)
            if not isinstance(value, dict):
                raise ValueError(f"{path}:{line_no}: JSONL record is not an object")
            records.append(value)
    return records


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def all_coverage(records: Iterable[dict[str, object]]) -> set[str]:
    tags: set[str] = set()
    for record in records:
        coverage = record.get("coverage")
        if not isinstance(coverage, list):
            raise ValueError(f"record missing coverage list: {record.get('entry_id') or record.get('case_id')}")
        tags.update(str(item) for item in coverage)
    return tags


def validate_value(value: object) -> None:
    if not isinstance(value, dict):
        raise ValueError("field value must be an object")
    kind = value.get("kind")
    if kind == "utf8":
        if not isinstance(value.get("text"), str):
            raise ValueError("utf8 value requires text")
    elif kind == "bytes":
        raw = value.get("base64")
        size = value.get("size")
        if not isinstance(raw, str) or not isinstance(size, int):
            raise ValueError("bytes value requires base64 and size")
        decoded = base64.b64decode(raw.encode("ascii"))
        if len(decoded) != size:
            raise ValueError("bytes value size does not match decoded base64")
    elif kind == "repeat":
        byte = value.get("byte")
        size = value.get("size")
        if not isinstance(byte, int) or not 0 <= byte <= 255 or not isinstance(size, int) or size < 0:
            raise ValueError("repeat value requires byte 0..255 and non-negative size")
    else:
        raise ValueError(f"unsupported value kind: {kind}")


def validate_correctness(records: list[dict[str, object]], manifest: dict[str, object]) -> None:
    if not records:
        raise ValueError("correctness corpus is empty")
    expected_count = manifest["corpora"]["correctness"]["record_count"]
    if len(records) != expected_count:
        raise ValueError(f"correctness count mismatch: {len(records)} != {expected_count}")

    for expected_index, record in enumerate(records):
        if record.get("record_type") != "accepted":
            raise ValueError(f"correctness record {expected_index} is not accepted")
        if record.get("entry_index") != expected_index:
            raise ValueError(f"correctness record index mismatch at {expected_index}")
        if record.get("expected_outcome") != "accept":
            raise ValueError(f"correctness record {expected_index} does not expect accept")
        fields = record.get("fields")
        if not isinstance(fields, list) or not fields:
            raise ValueError(f"correctness record {expected_index} has no fields")
        for item in fields:
            if not isinstance(item, dict):
                raise ValueError(f"correctness record {expected_index} field is not an object")
            name = item.get("name")
            if not isinstance(name, str) or not FIELD_RE.match(name):
                raise ValueError(f"correctness record {expected_index} has invalid accepted field name: {name!r}")
            validate_value(item.get("value"))

    missing = set(manifest["coverage"]["correctness_required"]) - all_coverage(records)
    if missing:
        raise ValueError(f"correctness corpus missing coverage tags: {sorted(missing)}")


def validate_rejections(records: list[dict[str, object]], manifest: dict[str, object]) -> None:
    expected_count = manifest["corpora"]["rejections"]["record_count"]
    if len(records) != expected_count:
        raise ValueError(f"rejection count mismatch: {len(records)} != {expected_count}")
    for expected_index, record in enumerate(records):
        if record.get("record_type") != "rejected":
            raise ValueError(f"rejection record {expected_index} is not rejected")
        if record.get("case_index") != expected_index:
            raise ValueError(f"rejection record index mismatch at {expected_index}")
        if record.get("expected_outcome") != "reject":
            raise ValueError(f"rejection record {expected_index} does not expect reject")
        if not record.get("expected_error"):
            raise ValueError(f"rejection record {expected_index} has no expected_error")
    missing = set(manifest["coverage"]["rejections_required"]) - all_coverage(records)
    if missing:
        raise ValueError(f"rejection corpus missing coverage tags: {sorted(missing)}")


def validate_hashes(manifest: dict[str, object], performance_manifest: dict[str, object]) -> None:
    correctness_hash = sha256_file(CORRECTNESS_PATH)
    rejection_hash = sha256_file(REJECTIONS_PATH)
    if correctness_hash != manifest["corpora"]["correctness"]["sha256"]:
        raise ValueError("correctness corpus sha256 mismatch")
    if rejection_hash != manifest["corpora"]["rejections"]["sha256"]:
        raise ValueError("rejection corpus sha256 mismatch")
    if performance_manifest["stream_sha256"] != manifest["corpora"]["performance"]["sha256"]:
        raise ValueError("performance stream sha256 mismatch between manifests")


def validate_json_schema(
    schema: dict[str, object],
    manifest: dict[str, object],
    correctness: list[dict[str, object]],
    rejections: list[dict[str, object]],
) -> None:
    if jsonschema is None:
        raise RuntimeError("python jsonschema package is required to validate tests/datasets/schema.schema.json")

    jsonschema.Draft7Validator.check_schema(schema)
    jsonschema.Draft7Validator(schema).validate(manifest)

    definitions = schema["definitions"]
    accepted_schema = {
        "$schema": schema["$schema"],
        "definitions": definitions,
        **definitions["accepted_record"],
    }
    rejected_schema = {
        "$schema": schema["$schema"],
        "definitions": definitions,
        **definitions["rejected_record"],
    }
    accepted_validator = jsonschema.Draft7Validator(accepted_schema)
    rejected_validator = jsonschema.Draft7Validator(rejected_schema)

    for index, record in enumerate(correctness):
        errors = sorted(accepted_validator.iter_errors(record), key=lambda error: list(error.path))
        if errors:
            first = errors[0]
            raise ValueError(f"correctness record {index} schema error at {list(first.path)}: {first.message}")

    for index, record in enumerate(rejections):
        errors = sorted(rejected_validator.iter_errors(record), key=lambda error: list(error.path))
        if errors:
            first = errors[0]
            raise ValueError(f"rejection record {index} schema error at {list(first.path)}: {first.message}")


def run_generator_twice() -> None:
    if VALIDATION_DIR.exists():
        shutil.rmtree(VALIDATION_DIR)
    run1 = VALIDATION_DIR / "run1"
    run2 = VALIDATION_DIR / "run2"
    run1.mkdir(parents=True, exist_ok=True)
    run2.mkdir(parents=True, exist_ok=True)

    for target in (run1, run2):
        subprocess.run(
            [sys.executable, str(ROOT / "generate.py"), "committed", "--output-root", str(target / "tests" / "datasets")],
            cwd=REPO_ROOT,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

    for rel in (
        Path("tests/datasets/correctness/corpus.jsonl"),
        Path("tests/datasets/rejections/corpus.jsonl"),
        Path("tests/datasets/performance/manifest.json"),
        Path("tests/datasets/ingestion-manifest.json"),
    ):
        left = sha256_file(run1 / rel)
        right = sha256_file(run2 / rel)
        committed = sha256_file(ROOT / rel.relative_to("tests/datasets"))
        if left != right or left != committed:
            raise ValueError(f"deterministic regeneration mismatch for {rel}")


def validate_performance_hash(performance_manifest: dict[str, object]) -> None:
    rows = int(performance_manifest["record_count"])
    result = subprocess.run(
        [sys.executable, str(ROOT / "generate.py"), "performance-hash", "--rows", str(rows)],
        cwd=REPO_ROOT,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    payload = json.loads(result.stdout)
    if payload["record_count"] != rows:
        raise ValueError("performance hash row count mismatch")
    if payload["sha256"] != performance_manifest["stream_sha256"]:
        raise ValueError("performance stream hash mismatch")


def main() -> int:
    schema = load_json(SCHEMA_PATH)
    manifest = load_json(MANIFEST_PATH)
    performance_manifest = load_json(PERFORMANCE_MANIFEST_PATH)
    if not isinstance(schema, dict) or not isinstance(manifest, dict) or not isinstance(performance_manifest, dict):
        raise ValueError("schema and manifests must be JSON objects")

    correctness = load_jsonl(CORRECTNESS_PATH)
    rejections = load_jsonl(REJECTIONS_PATH)
    validate_json_schema(schema, manifest, correctness, rejections)
    validate_correctness(correctness, manifest)
    validate_rejections(rejections, manifest)
    validate_hashes(manifest, performance_manifest)
    run_generator_twice()
    validate_performance_hash(performance_manifest)

    print(
        json.dumps(
            {
                "status": "PASS",
                "correctness_records": len(correctness),
                "rejection_records": len(rejections),
                "performance_records": performance_manifest["record_count"],
                "performance_sha256": performance_manifest["stream_sha256"],
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
