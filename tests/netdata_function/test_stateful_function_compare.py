#!/usr/bin/env python3
"""Unit tests for stateful Netdata function comparison helpers."""

from __future__ import annotations

import json
import shutil
import subprocess  # nosec B404
import sys
import tempfile
import time
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from run_stateful_function_compare import (  # noqa: E402
    STATIC_FIXTURE_LOWER_BOUND_SECONDS_AGO,
    STATIC_FIXTURE_MACHINE_DIR,
    STATIC_FIXTURE_ROW_COUNT,
    STATIC_FIXTURE_UPPER_BOUND_SECONDS_AGO,
    SequenceState,
    assert_no_duplicate_rows,
    assert_tail_rows_newer,
    data_only_request,
    filtered_tail_request,
    make_static_fixture,
    response_anchor,
    response_rows,
    tail_request,
)


def doc(*timestamps: int) -> dict:
    return {
        "status": 200,
        "_request": {"data_only": True},
        "columns": {
            "timestamp": {"index": 0, "name": "Time"},
            "MESSAGE": {"index": 1, "name": "Message"},
        },
        "data": [[timestamp, f"row-{timestamp}"] for timestamp in timestamps],
    }


class StatefulFunctionCompareTest(unittest.TestCase):
    def test_response_rows_uses_column_catalog(self) -> None:
        self.assertEqual(
            response_rows(doc(10, 9)),
            [
                {"timestamp": 10, "message": "row-10"},
                {"timestamp": 9, "message": "row-9"},
            ],
        )

    def test_anchor_derivation_modes(self) -> None:
        response = doc(11, 10, 9)
        self.assertEqual(response_anchor(response, "first"), 11)
        self.assertEqual(response_anchor(response, "last"), 9)
        self.assertEqual(response_anchor(response, "max"), 11)

    def test_duplicate_rows_are_rejected_across_pages(self) -> None:
        state = SequenceState.empty()
        assert_no_duplicate_rows("paging-backward", "page-1", state, doc(10, 9))
        with self.assertRaisesRegex(AssertionError, "duplicate returned row"):
            assert_no_duplicate_rows("paging-backward", "page-2", state, doc(9, 8))

    def test_tail_rows_must_be_newer_than_anchor(self) -> None:
        assert_tail_rows_newer("tail", "positive", 10, doc(12, 11))
        with self.assertRaisesRegex(AssertionError, "not newer than anchor"):
            assert_tail_rows_newer("tail", "stale", 10, doc(11, 10))

    def test_tail_request_sets_stop_anchor_contract_fields(self) -> None:
        request = tail_request(1_700_001_000_000_005)
        self.assertTrue(request["tail"])
        self.assertEqual(request["anchor"], 1_700_001_000_000_005)
        self.assertEqual(request["if_modified_since"], 1_700_001_000_000_005)
        self.assertEqual(request["direction"], "backward")

    def test_filtered_tail_request_uses_no_match_priority_filter(self) -> None:
        request = filtered_tail_request(1_700_001_000_000_005)
        self.assertEqual(request["selections"], {"PRIORITY": ["3"]})

    def test_paging_request_keeps_forward_direction(self) -> None:
        request = data_only_request("forward", 5)
        self.assertEqual(request["direction"], "forward")
        self.assertEqual(request["last"], 5)


def _read_fixture_journal_bounds(fixture_dir: Path) -> tuple[int, int]:
    """Return min/max realtime usec from a fixture written by
    ``make_static_fixture``."""

    timestamps = [int(entry["__REALTIME_TIMESTAMP"]) for entry in _read_fixture_entries(fixture_dir)]
    if not timestamps:
        raise AssertionError(f"fixture {fixture_dir} contains no entries")
    return min(timestamps), max(timestamps)


def _read_fixture_priorities(fixture_dir: Path) -> list[str]:
    """Return the PRIORITY field value (decoded string) for each
    entry in fixture order. The directory must already contain a
    system.journal file written by ``make_static_fixture``."""

    return [str(entry.get("PRIORITY", "")) for entry in _read_fixture_entries(fixture_dir)]


def _read_fixture_entries(fixture_dir: Path) -> list[dict[str, object]]:
    journalctl = shutil.which("journalctl")
    if journalctl is None:
        raise unittest.SkipTest("journalctl is required to inspect generated fixture")
    result = subprocess.run(  # nosec B603
        [journalctl, "--directory", str(fixture_dir), "--output=json", "--no-pager"],
        check=False,
        capture_output=True,
        text=True,
        timeout=30,
    )
    if result.returncode != 0:
        raise AssertionError(
            f"journalctl failed for fixture {fixture_dir}: {result.stderr[-1000:]}"
        )
    return [json.loads(line) for line in result.stdout.splitlines() if line.strip()]


class StaticFixtureBuilderTest(unittest.TestCase):
    """SOW-0104 fix-10: the three-peer stateful gate freezes a
    fresh-data synthetic fixture so a slow third peer is not
    divergent because of live tail movement. The
    ``make_static_fixture`` builder writes the fixture and the
    runner exposes it through ``--make-static-fixture <dir>``.
    These tests pin the builder's contract."""

    def test_writes_default_row_count_inside_fresh_window(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            now = int(time.time())
            report = make_static_fixture(
                Path(tmp) / "fixture",
                now_seconds=now,
            )
            self.assertEqual(report["row_count"], STATIC_FIXTURE_ROW_COUNT)
            self.assertEqual(report["now_seconds"], now)
            # First entry: now - lower_bound; last entry: at most
            # now - upper_bound (within one step).
            self.assertEqual(
                report["first_entry_realtime_usec"],
                (now - STATIC_FIXTURE_LOWER_BOUND_SECONDS_AGO) * 1_000_000,
            )
            self.assertLessEqual(
                report["last_entry_realtime_usec"],
                (now - STATIC_FIXTURE_UPPER_BOUND_SECONDS_AGO) * 1_000_000,
            )
            # The journal file lives under the machine-id directory
            # the runner expects.
            self.assertTrue(
                Path(report["journal_path"]).is_file(),
                msg=f"journal file not present: {report['journal_path']}",
            )
            self.assertIn(
                STATIC_FIXTURE_MACHINE_DIR,
                report["journal_path"],
            )
            self.assertTrue(report["journal_path"].endswith("system.journal"))

    def test_custom_row_count_respected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            report = make_static_fixture(
                Path(tmp) / "fixture",
                row_count=10,
                now_seconds=1_700_000_000,
            )
            self.assertEqual(report["row_count"], 10)
            # 10 rows across 2400 seconds (3000-600) -> step 266
            # seconds (2400 // (10 - 1)).
            self.assertEqual(report["step_seconds"], 266)
            self.assertEqual(
                report["first_entry_realtime_usec"],
                (1_700_000_000 - STATIC_FIXTURE_LOWER_BOUND_SECONDS_AGO) * 1_000_000,
            )
            self.assertEqual(
                report["last_entry_realtime_usec"],
                report["first_entry_realtime_usec"]
                + (10 - 1) * 266 * 1_000_000,
            )

    def test_refuses_to_overwrite_non_empty_directory(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "fixture"
            target.mkdir()
            (target / "stale.txt").write_text("stale")
            with self.assertRaisesRegex(
                FileExistsError, "refusing to overwrite non-empty"
            ):
                make_static_fixture(target, now_seconds=int(time.time()))

    def test_journal_contains_exactly_row_count_entries(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            fixture_dir = Path(tmp) / "fixture"
            report = make_static_fixture(
                fixture_dir, row_count=12, now_seconds=int(time.time())
            )
            first, last = _read_fixture_journal_bounds(fixture_dir)
            self.assertEqual(last - first, (12 - 1) * (2400 // (12 - 1)) * 1_000_000)
            # All entries are within the requested fresh-data window.
            now = int(report["now_seconds"])
            self.assertGreaterEqual(
                first,
                (now - STATIC_FIXTURE_LOWER_BOUND_SECONDS_AGO) * 1_000_000,
            )
            self.assertLessEqual(
                last,
                (now - STATIC_FIXTURE_UPPER_BOUND_SECONDS_AGO) * 1_000_000,
            )

    def test_fixture_rows_contain_no_priority_3(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            fixture_dir = Path(tmp) / "fixture"
            make_static_fixture(
                fixture_dir, row_count=30, now_seconds=int(time.time())
            )
            priorities = _read_fixture_priorities(fixture_dir)
            self.assertEqual(len(priorities), 30)
            self.assertNotIn("3", priorities)
            self.assertTrue(all(p in ("5", "6", "7") for p in priorities))

    def test_validates_arguments(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(ValueError, "row_count must be positive"):
                make_static_fixture(
                    Path(tmp) / "fixture", row_count=0, now_seconds=1
                )
            with self.assertRaisesRegex(
                ValueError, "lower_bound_seconds_ago must be greater than upper"
            ):
                make_static_fixture(
                    Path(tmp) / "fixture",
                    lower_bound_seconds_ago=600,
                    upper_bound_seconds_ago=600,
                    now_seconds=1,
                )
            with self.assertRaisesRegex(
                ValueError, "does not fit in span"
            ):
                make_static_fixture(
                    Path(tmp) / "fixture",
                    row_count=10_000,
                    lower_bound_seconds_ago=3000,
                    upper_bound_seconds_ago=600,
                    now_seconds=1,
                )


class StaticFixtureCliTest(unittest.TestCase):
    """SOW-0104 fix-10: the runner exposes
    ``--make-static-fixture <dir>`` which generates a fixture and
    writes a JSON report. The default behavior (no flag) is
    unchanged: the runner expects --dir to point at an existing
    directory and runs the sequences against it."""

    def _runner_python(self) -> str:
        # The repo-pinned venv. Tests in this project are expected
        # to use it (recorded in the SOW as the canonical path).
        return str(
            Path(__file__).resolve().parent.parent.parent
            / ".local"
            / "python-venv"
            / "bin"
            / "python3"
        )

    def _run(self, *args: str) -> subprocess.CompletedProcess:
        cmd = [
            self._runner_python(),
            str(
                Path(__file__).resolve().parent
                / "run_stateful_function_compare.py"
            ),
            *args,
        ]
        return subprocess.run(  # nosec B603
            cmd,
            check=False,
            capture_output=True,
            text=True,
        )

    def test_static_fixture_flag_does_not_require_sdk_or_plugin(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "fixture"
            result = self._run(
                "--make-static-fixture",
                str(target),
            )
            self.assertEqual(
                result.returncode,
                0,
                msg=(
                    "static-fixture mode must NOT require --sdk/--plugin/--out/--dir; "
                    "stdout=%s stderr=%s" % (result.stdout, result.stderr)
                ),
            )
            self.assertTrue(
                (target / STATIC_FIXTURE_MACHINE_DIR / "system.journal").is_file(),
                msg=(
                    "expected journal under "
                    f"{target}/{STATIC_FIXTURE_MACHINE_DIR}/system.journal"
                ),
            )
            report_path = target / "fixture-report.json"
            self.assertTrue(report_path.is_file())
            report = json.loads(report_path.read_text())
            self.assertTrue(report["generated"])
            self.assertNotIn("ok", report)
            self.assertEqual(
                report["static_fixture"]["row_count"], STATIC_FIXTURE_ROW_COUNT,
            )

    def test_static_fixture_flag_writes_custom_out_report(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "fixture"
            out = Path(tmp) / "out.json"
            result = self._run(
                "--make-static-fixture",
                str(target),
                "--out",
                str(out),
            )
            self.assertEqual(result.returncode, 0, msg=result.stderr)
            self.assertTrue(out.is_file())
            report = json.loads(out.read_text())
            self.assertTrue(report["generated"])
            self.assertNotIn("ok", report)
            self.assertEqual(
                report["make_static_fixture"], str(target),
            )

    def test_static_fixture_flag_rejects_non_empty_target(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "fixture"
            target.mkdir()
            (target / "stale.txt").write_text("stale")
            result = self._run(
                "--make-static-fixture",
                str(target),
            )
            self.assertNotEqual(result.returncode, 0)
            report_path = target / "fixture-report.json"
            self.assertTrue(report_path.is_file())
            report = json.loads(report_path.read_text())
            self.assertFalse(report["ok"])
            self.assertIn("refusing to overwrite", report["static_fixture_error"])

    def test_static_fixture_flag_honors_custom_row_count(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "fixture"
            result = self._run(
                "--make-static-fixture",
                str(target),
                "--static-fixture-row-count",
                "5",
            )
            self.assertEqual(result.returncode, 0, msg=result.stderr)
            report = json.loads(
                (target / "fixture-report.json").read_text()
            )
            self.assertEqual(report["static_fixture"]["row_count"], 5)

    def test_generate_only_report_has_generated_no_ok(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "fixture"
            result = self._run(
                "--make-static-fixture",
                str(target),
            )
            self.assertEqual(result.returncode, 0, msg=result.stderr)
            report = json.loads(
                (target / "fixture-report.json").read_text()
            )
            self.assertTrue(report["generated"])
            self.assertNotIn("ok", report)
            self.assertIn("static_fixture", report)

    def test_default_mode_without_static_fixture_still_requires_sdk(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "fixture"
            result = self._run(
                "--dir",
                str(target),
                "--out",
                str(Path(tmp) / "out.json"),
            )
            self.assertNotEqual(result.returncode, 0)
            # The error message must surface the missing --sdk flag
            # so the operator knows to add it.
            self.assertIn("--sdk", result.stderr)


if __name__ == "__main__":
    unittest.main()
