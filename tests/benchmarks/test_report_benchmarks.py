#!/usr/bin/env python3
"""Unit tests for report_benchmarks.py."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import report_benchmarks as reports  # noqa: E402


class BenchmarkReportTests(unittest.TestCase):
    def test_reader_missing_production_rows_stay_visible(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            run_dir = Path(tmp)
            (run_dir / "summary.json").write_text(
                json.dumps(
                    [
                        {
                            "language": "rust",
                            "surface": "file",
                            "mode": "core-payloads",
                            "bounds": "live",
                            "mmap_strategy": "windowed",
                            "median_read_rows_per_second": 10,
                            "min_read_rows_per_second": 9,
                            "max_read_rows_per_second": 11,
                        }
                    ]
                ),
                encoding="utf-8",
            )
            (run_dir / "manifest.json").write_text(
                json.dumps({"languages": "rust", "format": "compact", "rows": 1}),
                encoding="utf-8",
            )

            run = reports.load_run(run_dir, "run")
            markdown = reports.render_report(
                title="Reader Missing Rows",
                run=run,
                before=None,
                after=None,
                conclusion="not-assessed",
                conclusion_note="",
            )

        self.assertIn("| file | rust | core-payloads | live/windowed | measured | 10 | 9 | 11 | - | - |", markdown)
        self.assertIn("| file | rust | sdk-payloads | live/windowed | missing | - | - | - | - | - |", markdown)
        self.assertIn("| file | rust | facade-data | live/windowed | missing | - | - | - | - | - |", markdown)

    def test_writer_missing_required_metric_fails_cleanly(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            run_dir = Path(tmp)
            (run_dir / "report.json").write_text(
                json.dumps(
                    {
                        "benchmark": "writer-core",
                        "environment": {"timestamp_utc": "2026-05-29T00:00:00+00:00"},
                        "parameters": {"languages": ["rust"]},
                        "summary": {
                            "rust": {
                                "api_modes": ["raw-payload"],
                                "mmap_strategies": ["windowed"],
                            }
                        },
                    }
                ),
                encoding="utf-8",
            )

            run = reports.load_run(run_dir, "run")
            with self.assertRaises(SystemExit) as raised:
                reports.render_report(
                    title="Malformed Writer",
                    run=run,
                    before=None,
                    after=None,
                    conclusion="not-assessed",
                    conclusion_note="",
                )

        self.assertIn("missing append_rows_per_second_median", str(raised.exception))

    def test_reader_duplicate_keys_fail_cleanly(self) -> None:
        row = {
            "language": "rust",
            "surface": "file",
            "mode": "sdk-payloads",
            "bounds": "live",
            "mmap_strategy": "windowed",
            "median_read_rows_per_second": 10,
            "min_read_rows_per_second": 9,
            "max_read_rows_per_second": 11,
        }
        with tempfile.TemporaryDirectory() as tmp:
            run_dir = Path(tmp)
            (run_dir / "summary.json").write_text(json.dumps([row, row]), encoding="utf-8")
            (run_dir / "manifest.json").write_text(
                json.dumps({"languages": "rust", "format": "compact", "rows": 1}),
                encoding="utf-8",
            )

            run = reports.load_run(run_dir, "run")
            with self.assertRaises(SystemExit) as raised:
                reports.render_report(
                    title="Duplicate Reader",
                    run=run,
                    before=None,
                    after=None,
                    conclusion="not-assessed",
                    conclusion_note="",
                )

        self.assertIn("duplicate reader benchmark row key", str(raised.exception))

    def test_invalid_programmatic_conclusion_fails_cleanly(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            run_dir = Path(tmp)
            (run_dir / "summary.json").write_text("[]", encoding="utf-8")
            (run_dir / "manifest.json").write_text("{}", encoding="utf-8")
            run = reports.load_run(run_dir, "run")

            with self.assertRaises(SystemExit) as raised:
                reports.render_report(
                    title="Invalid Conclusion",
                    run=run,
                    before=None,
                    after=None,
                    conclusion="bogus",
                    conclusion_note="",
                )

        self.assertIn("unsupported conclusion label: bogus", str(raised.exception))

    def test_zero_reference_ratios_are_explicit(self) -> None:
        self.assertEqual(reports.fmt_ratio_to(None, 10), "-")
        self.assertEqual(reports.fmt_ratio_to(0, 0), "n/a")
        self.assertEqual(reports.fmt_ratio_to(10, 0), "inf")
        self.assertEqual(reports.fmt_ratio_to(0, 10), "0.000x")

    def test_empty_reader_production_has_placeholder(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            run_dir = Path(tmp)
            (run_dir / "summary.json").write_text("[]", encoding="utf-8")
            (run_dir / "manifest.json").write_text(
                json.dumps({"languages": "rust", "format": "compact", "rows": 0}),
                encoding="utf-8",
            )

            run = reports.load_run(run_dir, "run")
            markdown = reports.render_report(
                title="Empty Reader",
                run=run,
                before=None,
                after=None,
                conclusion="not-assessed",
                conclusion_note="",
            )

        self.assertIn("## Production Comparison\n\n_No matching rows._", markdown)

    def test_null_metadata_sections_do_not_traceback(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            run_dir = Path(tmp)
            (run_dir / "report.json").write_text(
                json.dumps(
                    {
                        "benchmark": "writer-core",
                        "environment": None,
                        "parameters": None,
                        "summary": {
                            "rust": {
                                "api_modes": ["raw-payload"],
                                "mmap_strategies": ["windowed"],
                                "append_rows_per_second_median": 10,
                                "append_rows_per_second_min": 9,
                                "append_rows_per_second_max": 11,
                            }
                        },
                    }
                ),
                encoding="utf-8",
            )

            run = reports.load_run(run_dir, "run")
            markdown = reports.render_report(
                title="Null Metadata",
                run=run,
                before=None,
                after=None,
                conclusion="not-assessed",
                conclusion_note="",
            )

        self.assertIn("| writer-core | rust | raw-payload | windowed | measured | 10 | 9 | 11 | - | 1.000x |", markdown)

    def test_reader_change_reports_unmatched_rows(self) -> None:
        common = {
            "language": "rust",
            "surface": "file",
            "mode": "sdk-payloads",
            "bounds": "live",
            "mmap_strategy": "windowed",
            "median_read_rows_per_second": 10,
            "min_read_rows_per_second": 9,
            "max_read_rows_per_second": 11,
        }
        before_only = {
            "language": "rust",
            "surface": "file",
            "mode": "facade-data",
            "bounds": "live",
            "mmap_strategy": "windowed",
            "median_read_rows_per_second": 8,
            "min_read_rows_per_second": 7,
            "max_read_rows_per_second": 9,
        }
        after_only = {
            "language": "rust",
            "surface": "file",
            "mode": "core-offsets",
            "bounds": "live",
            "mmap_strategy": "windowed",
            "median_read_rows_per_second": 12,
            "min_read_rows_per_second": 11,
            "max_read_rows_per_second": 13,
        }
        after_common = dict(common)
        after_common["median_read_rows_per_second"] = 20
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            before_dir = root / "before"
            after_dir = root / "after"
            before_dir.mkdir()
            after_dir.mkdir()
            for run_dir, rows in (
                (before_dir, [common, before_only]),
                (after_dir, [after_common, after_only]),
            ):
                (run_dir / "summary.json").write_text(json.dumps(rows), encoding="utf-8")
                (run_dir / "manifest.json").write_text(
                    json.dumps({"languages": "rust", "format": "compact", "rows": 1}),
                    encoding="utf-8",
                )

            before = reports.load_run(before_dir, "before")
            after = reports.load_run(after_dir, "after")
            markdown = reports.render_report(
                title="Reader Change",
                run=None,
                before=before,
                after=after,
                conclusion="not-assessed",
                conclusion_note="",
            )

        self.assertIn("### Production Modes", markdown)
        self.assertIn("| file | rust | sdk-payloads | live/windowed | 10 | 20 | +100.0% | 2.000x |", markdown)
        self.assertIn("### Unmatched Rows", markdown)
        self.assertIn("| before only | file | rust | facade-data | live/windowed |", markdown)
        self.assertIn("| after only | file | rust | core-offsets | live/windowed |", markdown)

    def test_open_files_core_payloads_is_diagnostic(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            run_dir = Path(tmp)
            (run_dir / "summary.json").write_text(
                json.dumps(
                    [
                        {
                            "language": "rust",
                            "surface": "open-files",
                            "mode": "core-payloads",
                            "bounds": "live",
                            "mmap_strategy": "windowed",
                            "median_read_rows_per_second": 10,
                            "min_read_rows_per_second": 9,
                            "max_read_rows_per_second": 11,
                        },
                        {
                            "language": "rust",
                            "surface": "open-files",
                            "mode": "sdk-payloads",
                            "bounds": "live",
                            "mmap_strategy": "windowed",
                            "median_read_rows_per_second": 20,
                            "min_read_rows_per_second": 19,
                            "max_read_rows_per_second": 21,
                        },
                    ]
                ),
                encoding="utf-8",
            )
            (run_dir / "manifest.json").write_text(
                json.dumps({"languages": "rust", "format": "compact", "rows": 1}),
                encoding="utf-8",
            )

            run = reports.load_run(run_dir, "run")
            markdown = reports.render_report(
                title="Open Files",
                run=run,
                before=None,
                after=None,
                conclusion="not-assessed",
                conclusion_note="",
            )

        self.assertIn("| open-files | rust | sdk-payloads | live/windowed | measured | 20 | 19 | 21 | - | 1.000x |", markdown)
        self.assertIn("| open-files | rust | core-payloads | live/windowed | 10 | 9 | 11 |", markdown)

    def test_open_files_core_payloads_is_diagnostic_in_change_report(self) -> None:
        before_row = {
            "language": "rust",
            "surface": "open-files",
            "mode": "core-payloads",
            "bounds": "live",
            "mmap_strategy": "windowed",
            "median_read_rows_per_second": 10,
            "min_read_rows_per_second": 9,
            "max_read_rows_per_second": 11,
        }
        after_row = dict(before_row)
        after_row["median_read_rows_per_second"] = 20
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            before_dir = root / "before"
            after_dir = root / "after"
            before_dir.mkdir()
            after_dir.mkdir()
            for run_dir, rows in ((before_dir, [before_row]), (after_dir, [after_row])):
                (run_dir / "summary.json").write_text(json.dumps(rows), encoding="utf-8")
                (run_dir / "manifest.json").write_text(
                    json.dumps({"languages": "rust", "format": "compact", "rows": 1}),
                    encoding="utf-8",
                )

            before = reports.load_run(before_dir, "before")
            after = reports.load_run(after_dir, "after")
            markdown = reports.render_report(
                title="Open Files Change",
                run=None,
                before=before,
                after=after,
                conclusion="not-assessed",
                conclusion_note="",
            )

        change = markdown.split("## Change Comparison", 1)[1]
        production, diagnostics = change.split("### Diagnostic Modes", 1)
        row = "| open-files | rust | core-payloads | live/windowed | 10 | 20 | +100.0% | 2.000x |"
        self.assertNotIn(row, production)
        self.assertIn(row, diagnostics)

    def test_latest_cycle_fails_cleanly(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            run_dir = Path(tmp)
            (run_dir / "latest").symlink_to(run_dir, target_is_directory=True)

            with self.assertRaises(SystemExit) as raised:
                reports.resolve_artifact(run_dir)

        self.assertIn("benchmark artifact path cycle detected", str(raised.exception))

    def test_writer_missing_configured_language_stays_visible(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            run_dir = Path(tmp)
            (run_dir / "report.json").write_text(
                json.dumps(
                    {
                        "benchmark": "writer-core",
                        "parameters": {"languages": ["rust", "go"]},
                        "summary": {
                            "rust": {
                                "api_modes": ["raw-payload"],
                                "mmap_strategies": ["windowed"],
                                "append_rows_per_second_median": 10,
                                "append_rows_per_second_min": 9,
                                "append_rows_per_second_max": 11,
                            }
                        },
                    }
                ),
                encoding="utf-8",
            )

            run = reports.load_run(run_dir, "run")
            markdown = reports.render_report(
                title="Writer Missing Rows",
                run=run,
                before=None,
                after=None,
                conclusion="not-assessed",
                conclusion_note="",
            )

        self.assertIn("| writer-core | go | - | - | missing | - | - | - | - | - |", markdown)

    def test_writer_report_has_no_diagnostic_modes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            run_dir = Path(tmp)
            (run_dir / "report.json").write_text(
                json.dumps(
                    {
                        "benchmark": "writer-core",
                        "parameters": {"languages": ["rust"]},
                        "summary": {
                            "rust": {
                                "api_modes": ["raw-payload"],
                                "mmap_strategies": ["windowed"],
                                "append_rows_per_second_median": 10,
                                "append_rows_per_second_min": 9,
                                "append_rows_per_second_max": 11,
                            }
                        },
                    }
                ),
                encoding="utf-8",
            )

            run = reports.load_run(run_dir, "run")
            markdown = reports.render_report(
                title="Writer Report",
                run=run,
                before=None,
                after=None,
                conclusion="not-assessed",
                conclusion_note="",
            )

        self.assertNotIn("## Diagnostic Modes", markdown)

    def test_main_rejects_run_with_before_after(self) -> None:
        old_argv = sys.argv
        try:
            sys.argv = [
                "report_benchmarks.py",
                "--run",
                "run",
                "--before",
                "before",
                "--after",
                "after",
            ]
            with self.assertRaises(SystemExit) as raised:
                reports.main()
        finally:
            sys.argv = old_argv

        self.assertIn("--run cannot be combined with --before/--after", str(raised.exception))

    def test_writer_change_reports_unmatched_and_config_differences(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            before_dir = root / "before"
            after_dir = root / "after"
            before_dir.mkdir()
            after_dir.mkdir()
            before_report = {
                "benchmark": "writer-core",
                "parameters": {"languages": ["rust"]},
                "summary": {
                    "rust": {
                        "api_modes": ["raw-payload"],
                        "mmap_strategies": ["windowed"],
                        "append_rows_per_second_median": 10,
                        "append_rows_per_second_min": 9,
                        "append_rows_per_second_max": 11,
                    }
                },
            }
            after_report = {
                "benchmark": "writer-core",
                "parameters": {"languages": ["rust", "systemd"]},
                "summary": {
                    "systemd": {
                        "api_modes": ["raw-payload"],
                        "mmap_strategies": ["unknown"],
                        "append_rows_per_second_median": 20,
                        "append_rows_per_second_min": 19,
                        "append_rows_per_second_max": 21,
                    },
                    "rust": {
                        "api_modes": ["structured-field"],
                        "mmap_strategies": ["windowed"],
                        "append_rows_per_second_median": 12,
                        "append_rows_per_second_min": 11,
                        "append_rows_per_second_max": 13,
                    },
                },
            }
            (before_dir / "report.json").write_text(json.dumps(before_report), encoding="utf-8")
            (after_dir / "report.json").write_text(json.dumps(after_report), encoding="utf-8")

            before = reports.load_run(before_dir, "before")
            after = reports.load_run(after_dir, "after")
            markdown = reports.render_report(
                title="Writer Change",
                run=None,
                before=before,
                after=after,
                conclusion="not-assessed",
                conclusion_note="",
            )

        self.assertIn("### Unmatched Rows", markdown)
        self.assertIn("| after only | systemd | raw-payload | unknown |", markdown)
        self.assertIn("### Configuration Differences", markdown)
        self.assertIn("| rust | raw-payload | windowed | structured-field | windowed |", markdown)


if __name__ == "__main__":
    unittest.main()
