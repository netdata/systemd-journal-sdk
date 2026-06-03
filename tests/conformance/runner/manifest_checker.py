#!/usr/bin/env python3
"""
Manifest checker / harness validator.

Validates conformance manifests against manifest-schema.json and provides
dry-run / stub-adapter modes for harness validation.

Usage:
    python3 manifest_checker.py validate <manifest.json>
    python3 manifest_checker.py list <manifest.json>
    python3 manifest_checker.py dry-run <manifest.json> [--adapter-cmd <cmd>]
    python3 manifest_checker.py stub <manifest.json> [--test-name <name>]
    python3 manifest_checker.py validate-files <manifest.json>
"""

import json
import sys
import argparse
from pathlib import Path

try:
    from jsonschema import validate, ValidationError as JsonSchemaValidationError
    HAS_JSONSCHEMA = True
except ImportError:
    HAS_JSONSCHEMA = False

REPO_ROOT = Path(__file__).parent.parent.parent.parent
FIXTURES_BASE = REPO_ROOT

VALID_CATEGORIES = {
    "file-format", "entry-parse", "entry-write", "matching",
    "enumeration", "verification", "sealing", "compression",
    "corruption-resilience", "cursor-navigation", "stream",
    "import-export", "journalctl-cli", "live-concurrency",
}
VALID_RESULT_FORMATS = {
    "entry-list", "cursor-list", "field-list", "export",
    "count", "boolean", "error",
}
VALID_FIXTURE_TYPES = {"file", "inline", "generated", "external"}


def load_schema():
    schema_path = Path(__file__).parent.parent / "manifest-schema.json"
    with open(schema_path) as f:
        return json.load(f)


def stdlib_validate_manifest(data, schema):
    """Validate the full manifest using stdlib only (no jsonschema dependency).

    Checks:
    - Root required fields: manifest_version, test_suite
    - Test suite required fields: suite_name, systemd_baseline, test_cases
    - Every test case required field: test_name, category, fixtures, adapter_cmd, expected
    - expected.result_format is present and valid
    - fixture refs have type and path
    - type:file fixtures exist on disk (file or directory)
    - type:generated fixtures have source_path pointing to committed inputs
    - error_contains is non-empty for result_format: error
    """
    errors = []
    validate_manifest_root(errors, data)
    suite = data.get("test_suite", {})
    if not isinstance(suite, dict):
        errors.append("test_suite: expected object")
        return errors
    validate_suite_fields(errors, suite)
    test_cases = suite.get("test_cases", [])
    if not isinstance(test_cases, list):
        errors.append("test_suite.test_cases: expected array")
        return errors
    for index, test_case in enumerate(test_cases):
        validate_test_case(errors, index, test_case)
    return errors


def validate_manifest_root(errors, data):
    for field in ("manifest_version", "test_suite"):
        if field not in data:
            errors.append(f"root: missing required field '{field}'")


def validate_suite_fields(errors, suite):
    for field in ("suite_name", "systemd_baseline", "test_cases"):
        if field not in suite:
            errors.append(f"test_suite: missing required field '{field}'")

    baseline = suite.get("systemd_baseline", {})
    if isinstance(baseline, dict):
        for field in ("repo", "commit"):
            if field not in baseline:
                errors.append(f"test_suite.systemd_baseline: missing required field '{field}'")


def validate_test_case(errors, index, test_case):
    prefix = f"test_suite.test_cases[{index}]"
    if not isinstance(test_case, dict):
        errors.append(f"{prefix}: expected object")
        return

    for field in ("test_name", "category", "fixtures", "adapter_cmd", "expected"):
        if field not in test_case:
            errors.append(f"{prefix}: missing required field '{field}'")

    test_name = test_case.get("test_name", f"<unnamed index {index}>")
    validate_test_case_category(errors, prefix, test_name, test_case)
    validate_test_case_fixtures(errors, prefix, test_name, test_case.get("fixtures", {}))
    validate_expected(errors, prefix, test_name, test_case.get("expected", {}))
    adapter_cmd = test_case.get("adapter_cmd")
    if adapter_cmd is not None and not isinstance(adapter_cmd, list):
        errors.append(f"{prefix} ({test_name}): adapter_cmd must be array")


def validate_test_case_category(errors, prefix, test_name, test_case):
    category = test_case.get("category")
    if category and category not in VALID_CATEGORIES:
        errors.append(f"{prefix} ({test_name}): invalid category '{category}'")


def validate_test_case_fixtures(errors, prefix, test_name, fixtures):
    if not isinstance(fixtures, dict):
        errors.append(f"{prefix} ({test_name}): fixtures must be object")
        return
    for name, ref in fixtures.items():
        validate_fixture_ref(errors, prefix, test_name, name, ref)


def validate_fixture_ref(errors, prefix, test_name, name, ref):
    ref_prefix = f"{prefix} ({test_name}).fixtures.{name}"
    if not isinstance(ref, dict):
        errors.append(f"{ref_prefix}: expected object")
        return
    validate_fixture_type_and_path(errors, ref_prefix, ref)
    validate_file_fixture_exists(errors, ref_prefix, ref)
    validate_generated_fixture_source(errors, ref_prefix, ref)


def validate_fixture_type_and_path(errors, ref_prefix, ref):
    if "type" not in ref:
        errors.append(f"{ref_prefix}: missing 'type'")
    elif ref["type"] not in VALID_FIXTURE_TYPES:
        errors.append(f"{ref_prefix}: invalid type '{ref['type']}'")
    if "path" not in ref:
        errors.append(f"{ref_prefix}: missing 'path'")


def validate_file_fixture_exists(errors, ref_prefix, ref):
    if ref.get("type") != "file" or not ref.get("path"):
        return
    path = FIXTURES_BASE / ref.get("path")
    if not path.exists():
        errors.append(f"{ref_prefix}: not found: {ref.get('path')}")


def validate_generated_fixture_source(errors, ref_prefix, ref):
    if ref.get("type") != "generated":
        return
    source_path = ref.get("source_path")
    if not source_path:
        errors.append(
            f"{ref_prefix}: type:generated must have 'source_path' pointing to committed source inputs, or 'daemon-required' if a live daemon is needed"
        )
        return
    if source_path != "daemon-required" and not (FIXTURES_BASE / source_path).exists():
        errors.append(f"{ref_prefix}: source_path not found: {source_path}")


def validate_expected(errors, prefix, test_name, expected):
    expected_prefix = f"{prefix} ({test_name}).expected"
    if not isinstance(expected, dict):
        errors.append(f"{prefix} ({test_name}): expected must be object")
        return
    validate_expected_result_format(errors, expected_prefix, expected)
    if "entries_match" not in expected:
        errors.append(f"{expected_prefix}: missing required field 'entries_match'")
    validate_error_expected(errors, expected_prefix, expected)


def validate_expected_result_format(errors, expected_prefix, expected):
    if "result_format" not in expected:
        errors.append(f"{expected_prefix}: missing required field 'result_format'")
    elif expected["result_format"] not in VALID_RESULT_FORMATS:
        errors.append(f"{expected_prefix}: invalid result_format '{expected['result_format']}'")


def validate_error_expected(errors, expected_prefix, expected):
    if expected.get("result_format") != "error":
        return
    error_contains = expected.get("error_contains")
    if error_contains is None:
        errors.append(
            f"{expected_prefix}: missing required field 'error_contains' for result_format: error"
        )
    elif error_contains == "":
        errors.append(f"{expected_prefix}: error_contains must not be empty for result_format: error")


def validate_manifest(manifest_path):
    """Validate a manifest against the schema.

    Uses jsonschema if available, falls back to stdlib validation.
    Fails on malformed manifests and missing required fields.
    """
    with open(manifest_path) as f:
        data = json.load(f)
    schema = load_schema()

    if HAS_JSONSCHEMA:
        try:
            validate(instance=data, schema=schema)
            # Also run stdlib checks for additional validation rules
            return stdlib_validate_manifest(data, schema)
        except JsonSchemaValidationError as e:
            return [str(e)]
    else:
        return stdlib_validate_manifest(data, schema)


def list_tests(manifest_path):
    with open(manifest_path) as f:
        data = json.load(f)
    suite = data.get("test_suite", {})
    print(f"Suite: {suite.get('suite_name')} v{suite.get('suite_version')}")
    print(f"Manifest version: {data.get('manifest_version')}")
    baseline = suite.get("systemd_baseline", {})
    print(f"Baseline: {baseline.get('repo')} @ {baseline.get('commit')} ({baseline.get('tag')})")
    print(f"\n{len(suite.get('test_cases', []))} test cases:\n")
    for tc in suite.get("test_cases", []):
        print(f"  [{tc.get('category'):<25}] {tc.get('test_name'):<40} tags={tc.get('tags', [])}")


def check_fixtures(test_case, fixtures_base=FIXTURES_BASE):
    """Check that referenced fixtures exist or are generateable."""
    fixtures = test_case.get("fixtures", {})
    results = []
    missing = []
    for name, ref in fixtures.items():
        ftype = ref.get("type")
        fpath = ref.get("path")
        if ftype == "file":
            full = fixtures_base / fpath
            if full.exists():
                kind = "dir" if full.is_dir() else "file"
                results.append(("OK", name, f"exists ({kind}): {fpath}"))
            else:
                results.append(("MISSING", name, f"not found: {fpath}"))
                missing.append(name)
        elif ftype == "generated":
            source = ref.get("source_path", "none")
            results.append(("GENERATED", name, f"generated_by: {ref.get('generated_by', 'unknown')}, source: {source}"))
        elif ftype == "external":
            results.append(("EXTERNAL", name, f"external reference: {fpath}"))
        else:
            results.append(("UNKNOWN", name, f"type={ftype}"))
    return results, missing


def validate_fixture_files(manifest_path, fixtures_base=FIXTURES_BASE):
    """Validate all type:file fixtures exist. Returns (errors, missing_files)."""
    with open(manifest_path) as f:
        data = json.load(f)
    errors = []
    missing = []
    for tc in data.get("test_suite", {}).get("test_cases", []):
        for name, ref in tc.get("fixtures", {}).items():
            if ref.get("type") == "file":
                fpath = ref.get("path")
                full = fixtures_base / fpath
                if not full.exists():
                    errors.append(f"test_case '{tc.get('test_name')}': fixture '{name}' missing: {fpath}")
                    missing.append(fpath)
    return errors, missing


def dry_run(manifest_path, adapter_cmd=None, test_name=None):
    """Simulate running each test case against a stub adapter."""
    with open(manifest_path) as f:
        data = json.load(f)

    suite = data.get("test_suite", {})
    baseline = suite.get("systemd_baseline", {})

    print(f"[DRY RUN] Simulating harness against {baseline.get('repo')} @ {baseline.get('commit')}")
    print(f"[DRY RUN] Adapter command: {adapter_cmd or 'stub (no-op)'}")
    print()

    results = []
    for tc in suite.get("test_cases", []):
        tn = tc.get("test_name")
        if test_name and tn != test_name:
            continue

        category = tc.get("category")
        expected = tc.get("expected", {})
        result_format = expected.get("result_format")
        tags = tc.get("tags", [])

        print(f"[DRY RUN] Would run: {tn}")
        print(f"         category : {category}")
        print(f"         format   : {result_format}")
        print(f"         tags     : {tags}")

        fixture_status, _fixture_missing = check_fixtures(tc)
        for status, name, msg in fixture_status:
            print(f"         fixture  [{status}] {name}: {msg}")

        note = expected.get("note", "")
        if note:
            print(f"         note     : {note[:80]}{'...' if len(note) > 80 else ''}")

        stub_result = {
            "test_name": tn,
            "status": "PASS",
            "result_format": result_format,
            "actual": expected.get("entries_match"),
            "expected": expected.get("entries_match"),
            "duration_ms": 0,
            "note": f"[STUB] dry-run only -- adapter: {adapter_cmd}"
        }
        results.append(stub_result)
        print(f"         -> STUB RESULT: {json.dumps(stub_result)}")
        print()

    print(f"\n[DRY RUN] Summary: {len(results)} test cases")
    return results


def main():
    parser = argparse.ArgumentParser(description="Manifest checker / harness validator")
    sub = parser.add_subparsers(dest="command", required=True)

    val = sub.add_parser("validate", help="Validate a manifest against the schema")
    val.add_argument("manifest", help="Path to manifest JSON file")

    lst = sub.add_parser("list", help="List test cases in a manifest")
    lst.add_argument("manifest", help="Path to manifest JSON file")

    dr = sub.add_parser("dry-run", help="Dry-run simulation of harness against manifest")
    dr.add_argument("manifest", help="Path to manifest JSON file")
    dr.add_argument("--adapter-cmd", default=None, help="Adapter command to simulate")
    dr.add_argument("--test-name", default=None, help="Run only this test name")

    stb = sub.add_parser("stub", help="Run manifest against stub adapter (returns PASS)")
    stb.add_argument("manifest", help="Path to manifest JSON file")
    stb.add_argument("--test-name", default=None, help="Run only this test name")

    vff = sub.add_parser("validate-files", help="Validate all type:file fixtures exist")
    vff.add_argument("manifest", help="Path to manifest JSON file")

    args = parser.parse_args()

    if args.command == "validate":
        errors = validate_manifest(args.manifest)
        if errors:
            print("Schema validation FAILED:")
            for e in errors:
                print(f"  ERROR: {e}")
            sys.exit(1)
        else:
            print(f"OK: {args.manifest} is valid")
            sys.exit(0)
    elif args.command == "list":
        list_tests(args.manifest)
    elif args.command == "dry-run":
        dry_run(args.manifest, args.adapter_cmd, args.test_name)
    elif args.command == "stub":
        dry_run(args.manifest, "stub", args.test_name)
    elif args.command == "validate-files":
        errors, missing = validate_fixture_files(args.manifest)
        if errors:
            print("Fixture validation FAILED:")
            for e in errors:
                print(f"  ERROR: {e}")
            sys.exit(1)
        else:
            print(f"OK: All type:file fixtures exist ({len(missing)} missing)")
            sys.exit(0)


if __name__ == "__main__":
    main()
