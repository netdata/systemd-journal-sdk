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

    # Root level
    for field in ("manifest_version", "test_suite"):
        if field not in data:
            errors.append(f"root: missing required field '{field}'")

    suite = data.get("test_suite", {})
    if not isinstance(suite, dict):
        errors.append("test_suite: expected object")
        return errors

    # Test suite level
    for field in ("suite_name", "systemd_baseline", "test_cases"):
        if field not in suite:
            errors.append(f"test_suite: missing required field '{field}'")

    baseline = suite.get("systemd_baseline", {})
    if isinstance(baseline, dict):
        for field in ("repo", "commit"):
            if field not in baseline:
                errors.append(f"test_suite.systemd_baseline: missing required field '{field}'")

    # Test cases
    test_cases = suite.get("test_cases", [])
    if not isinstance(test_cases, list):
        errors.append("test_suite.test_cases: expected array")
        return errors

    valid_categories = {
        "file-format", "entry-parse", "entry-write", "matching",
        "enumeration", "verification", "sealing", "compression",
        "corruption-resilience", "cursor-navigation", "stream",
        "import-export", "journalctl-cli", "live-concurrency",
    }
    valid_result_formats = {
        "entry-list", "cursor-list", "field-list", "export",
        "count", "boolean", "error",
    }
    valid_fixture_types = {"file", "inline", "generated", "external"}

    for i, tc in enumerate(test_cases):
        prefix = f"test_suite.test_cases[{i}]"
        if not isinstance(tc, dict):
            errors.append(f"{prefix}: expected object")
            continue

        # Required test case fields
        for field in ("test_name", "category", "fixtures", "adapter_cmd", "expected"):
            if field not in tc:
                errors.append(f"{prefix}: missing required field '{field}'")

        tc_name = tc.get("test_name", f"<unnamed index {i}>")

        # Category validation
        category = tc.get("category")
        if category and category not in valid_categories:
            errors.append(f"{prefix} ({tc_name}): invalid category '{category}'")

        # Fixtures validation
        fixtures = tc.get("fixtures", {})
        if not isinstance(fixtures, dict):
            errors.append(f"{prefix} ({tc_name}): fixtures must be object")
        else:
            for fname, ref in fixtures.items():
                if not isinstance(ref, dict):
                    errors.append(f"{prefix} ({tc_name}).fixtures.{fname}: expected object")
                    continue
                if "type" not in ref:
                    errors.append(f"{prefix} ({tc_name}).fixtures.{fname}: missing 'type'")
                elif ref["type"] not in valid_fixture_types:
                    errors.append(f"{prefix} ({tc_name}).fixtures.{fname}: invalid type '{ref['type']}'")
                if "path" not in ref:
                    errors.append(f"{prefix} ({tc_name}).fixtures.{fname}: missing 'path'")

                ftype = ref.get("type")
                fpath = ref.get("path")

                # Check type:file fixture existence (file or directory)
                if ftype == "file" and fpath:
                    full = FIXTURES_BASE / fpath
                    if not full.exists():
                        errors.append(f"{prefix} ({tc_name}).fixtures.{fname}: not found: {fpath}")

                # Check type:generated fixtures have committed source inputs
                if ftype == "generated":
                    source_path = ref.get("source_path")
                    if not source_path:
                        errors.append(
                            f"{prefix} ({tc_name}).fixtures.{fname}: "
                            f"type:generated must have 'source_path' pointing to committed source inputs, or 'daemon-required' if a live daemon is needed"
                        )
                    elif source_path != "daemon-required":
                        full_source = FIXTURES_BASE / source_path
                        if not full_source.exists():
                            errors.append(
                                f"{prefix} ({tc_name}).fixtures.{fname}: "
                                f"source_path not found: {source_path}"
                            )

        # Expected validation
        expected = tc.get("expected", {})
        if not isinstance(expected, dict):
            errors.append(f"{prefix} ({tc_name}): expected must be object")
        else:
            if "result_format" not in expected:
                errors.append(f"{prefix} ({tc_name}).expected: missing required field 'result_format'")
            elif expected["result_format"] not in valid_result_formats:
                errors.append(f"{prefix} ({tc_name}).expected: invalid result_format '{expected['result_format']}'")
            if "entries_match" not in expected:
                errors.append(f"{prefix} ({tc_name}).expected: missing required field 'entries_match'")

            # error_contains must be non-empty for error results
            if expected.get("result_format") == "error":
                error_contains = expected.get("error_contains")
                if error_contains is None:
                    errors.append(
                        f"{prefix} ({tc_name}).expected: "
                        "missing required field 'error_contains' for result_format: error"
                    )
                elif error_contains == "":
                    errors.append(
                        f"{prefix} ({tc_name}).expected: error_contains must not be empty for result_format: error"
                    )

        # adapter_cmd validation
        adapter_cmd = tc.get("adapter_cmd")
        if adapter_cmd is not None and not isinstance(adapter_cmd, list):
            errors.append(f"{prefix} ({tc_name}): adapter_cmd must be array")

    return errors


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
