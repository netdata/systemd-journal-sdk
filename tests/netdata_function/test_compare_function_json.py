#!/usr/bin/env python3
"""Unit tests for Netdata function content comparison."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from compare_function_json import compare  # noqa: E402


def function_doc(
    *,
    status: int = 200,
    message: str = "hello",
    columns: dict | None = None,
    facet_id: str = "PRIORITY",
    facet_name: str = "PRIORITY",
    priority_count: int = 1,
    include_histogram: bool = True,
    histogram_value: int = 1,
    evaluated: int = 10,
    include_empty_artifact: bool = False,
) -> dict:
    options = [
        {
            "id": "DwMLdkCaS0P",
            "name": "info",
            "count": priority_count,
            "order": 1,
        }
    ]
    if include_empty_artifact:
        options.append(
            {
                "id": "CzGfAU2z3TC",
                "name": "[unavailable field]",
                "count": 0,
                "order": 2,
            }
        )
    histogram = histogram_doc(histogram_value) if include_histogram else None
    return {
        "status": status,
        "type": "table",
        "columns": columns
        if columns is not None
        else {
            "timestamp": {"index": 0, "name": "Time"},
            "MESSAGE": {"index": 1, "name": "Message"},
        },
        "data": [[1000, message]],
        "facets": [
            {
                "id": facet_id,
                "name": facet_name,
                "order": 1,
                "options": options,
            }
        ],
        "histogram": histogram,
        "items": {
            "matched": 1,
            "returned": 1,
            "max_to_return": 200,
            "after": 0,
            "before": 0,
            "unsampled": 0,
            "estimated": 0,
            "evaluated": evaluated,
        },
        "expires": 123,
    }


def histogram_dimension_stats(histogram_value: int) -> dict:
    return {
        "min": [histogram_value],
        "max": [histogram_value],
        "avg": [histogram_value],
        "arp": [0],
        "con": [100],
    }


def histogram_doc(histogram_value: int) -> dict:
    return {
        "id": "PRIORITY",
        "name": "PRIORITY",
        "chart": {
            "summary": {
                "nodes": [],
                "contexts": [],
                "instances": [],
                "dimensions": [],
                "labels": [],
                "alerts": [],
            },
            "totals": {"nodes": {"sl": 1, "qr": 1}},
            "result": {
                "labels": ["time", "info"],
                "point": {"value": 0, "arp": 1, "pa": 2},
                "data": [[1000, [histogram_value, 0, 0]]],
            },
            "db": histogram_db_doc(histogram_value),
            "view": histogram_view_doc(histogram_value),
            "agents": [],
        },
    }


def histogram_db_doc(histogram_value: int) -> dict:
    return {
        "tiers": 1,
        "update_every": 5,
        "units": "events",
        "dimensions": {
            "ids": ["DwMLdkCaS0P"],
            "names": ["info"],
            "units": ["events"],
            "sts": histogram_dimension_stats(histogram_value),
        },
        "per_tier": [
            {
                "tier": 0,
                "queries": 1,
                "points": 1,
                "update_every": 5,
            }
        ],
    }


def histogram_view_doc(histogram_value: int) -> dict:
    return {
        "title": "Events Distribution by PRIORITY",
        "update_every": 5,
        "after": 1,
        "before": 2,
        "units": "events",
        "chart_type": "stackedBar",
        "dimensions": {
            "grouped_by": ["dimension"],
            "ids": ["DwMLdkCaS0P"],
            "names": ["info"],
            "colors": [None],
            "units": ["events"],
            "sts": histogram_dimension_stats(histogram_value),
        },
        "min": histogram_value,
        "max": histogram_value,
    }


class CompareFunctionJsonTest(unittest.TestCase):
    def test_equal_content_passes(self) -> None:
        report = compare(function_doc(), function_doc())
        self.assertTrue(report["ok"])
        self.assertTrue(all(report["content_checks"].values()))

    def test_row_content_mismatch_fails(self) -> None:
        report = compare(function_doc(message="left"), function_doc(message="right"))
        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["rows"])
        self.assertIn("MESSAGE", report["diffs"]["rows"])

    def test_known_plugin_message_corruption_is_reported_not_content(self) -> None:
        left = function_doc(
            message="=pkexec /path/to/java --token=redacted _CMDLINE=pkexec /path/to/java"
        )
        right = function_doc(
            message="user: Executing command [USER=root] [COMMAND=/path/to/java]"
        )
        left["data"][0][1] = left["data"][0][1].ljust(96, "x")
        right["data"][0][1] = right["data"][0][1].ljust(96, "y")

        report = compare(left, right)

        self.assertTrue(report["ok"])
        self.assertTrue(report["checks"]["rows"])
        self.assertEqual(
            report["non_content"]["known_plugin_message_corruption_rows"], [0]
        )

    def test_similar_message_mismatch_without_plugin_shape_fails(self) -> None:
        left = function_doc(message="=not the plugin corruption shape")
        right = function_doc(message="different valid message")

        report = compare(left, right)

        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["rows"])

    def test_column_catalog_mismatch_fails(self) -> None:
        report = compare(
            function_doc(),
            function_doc(columns={"timestamp": {"index": 0, "name": "Time"}}),
        )
        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["columns"])
        self.assertIn("MESSAGE", report["diffs"]["columns"])

    def test_facet_count_mismatch_fails(self) -> None:
        report = compare(function_doc(priority_count=1), function_doc(priority_count=2))
        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["facets"])
        self.assertIn("count", report["diffs"]["facets"])

    def test_selected_field_facet_quirk_is_reported_not_content(self) -> None:
        left = function_doc(priority_count=1)
        right = function_doc(priority_count=2)
        request = {
            "facets": ["PRIORITY", "SYSLOG_IDENTIFIER"],
            "selections": {"PRIORITY": ["3"]},
        }
        left["_request"] = dict(request)
        right["_request"] = dict(request)

        report = compare(left, right)

        self.assertTrue(report["ok"])
        self.assertTrue(report["checks"]["facets"])
        self.assertEqual(report["non_content"]["selected_field_facet_quirks"], ["PRIORITY"])

    def test_selected_field_facet_quirk_keeps_facet_metadata_as_content(self) -> None:
        left = function_doc(priority_count=1)
        right = function_doc(priority_count=2, facet_name="Severity")
        request = {
            "facets": ["PRIORITY", "SYSLOG_IDENTIFIER"],
            "selections": {"PRIORITY": ["3"]},
        }
        left["_request"] = dict(request)
        right["_request"] = dict(request)

        report = compare(left, right)

        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["facets"])
        self.assertIn("name", report["diffs"]["facets"])

    def test_unselected_facet_count_mismatch_still_fails(self) -> None:
        left = function_doc(priority_count=1)
        right = function_doc(priority_count=2)
        request = {
            "facets": ["PRIORITY", "SYSLOG_IDENTIFIER"],
            "selections": {"SYSLOG_IDENTIFIER": ["systemd"]},
        }
        left["_request"] = dict(request)
        right["_request"] = dict(request)

        report = compare(left, right)

        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["facets"])

    def test_facet_identity_mismatch_fails(self) -> None:
        report = compare(function_doc(), function_doc(facet_name="Severity"))
        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["facets"])
        self.assertIn("name", report["diffs"]["facets"])

    def test_histogram_mismatch_fails(self) -> None:
        report = compare(function_doc(histogram_value=1), function_doc(histogram_value=2))
        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["histogram"])
        self.assertIn("info", report["diffs"]["histogram"])

    def test_missing_histogram_mismatch_fails(self) -> None:
        report = compare(function_doc(), function_doc(include_histogram=False))
        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["histogram"])

    def test_missing_histogram_chart_view_dimensions_fails_schema(self) -> None:
        broken = function_doc()
        del broken["histogram"]["chart"]["view"]["dimensions"]

        report = compare(broken, function_doc())

        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["histogram_schema"])
        self.assertIn(
            "histogram.chart.view.dimensions",
            report["diffs"]["histogram_schema"]["left"],
        )

    def test_missing_histogram_chart_sts_member_fails_schema(self) -> None:
        broken = function_doc()
        del broken["histogram"]["chart"]["view"]["dimensions"]["sts"]["con"]

        report = compare(broken, function_doc())

        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["histogram_schema"])
        self.assertIn(
            "histogram.chart.view.dimensions.sts.con",
            report["diffs"]["histogram_schema"]["left"],
        )

    def test_stable_top_level_mismatch_fails(self) -> None:
        report = compare(function_doc(status=200), function_doc(status=500))
        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["top_level"])
        self.assertIn("status", report["diffs"]["top_level"])

    def test_info_default_window_echo_timestamps_are_volatile(self) -> None:
        left = {
            "status": 200,
            "type": "table",
            "_request": {"info": True, "after": 100, "before": 200},
        }
        right = {
            "status": 200,
            "type": "table",
            "_request": {"info": True, "after": 101, "before": 201},
        }

        report = compare(left, right)

        self.assertTrue(report["ok"])
        self.assertTrue(report["checks"]["top_level"])

    def test_evaluated_is_diagnostic_not_content(self) -> None:
        report = compare(function_doc(evaluated=10), function_doc(evaluated=11))
        self.assertTrue(report["ok"])
        self.assertFalse(report["checks"]["diagnostic_items"])
        self.assertEqual(report["diffs"]["items"], None)

    def test_empty_unavailable_facet_artifact_is_reported_not_content(self) -> None:
        report = compare(function_doc(include_empty_artifact=True), function_doc())
        self.assertTrue(report["ok"])
        artifacts = report["non_content"]["left_empty_unavailable_facet_artifacts"]
        self.assertIn("PRIORITY", artifacts)
        self.assertEqual(artifacts["PRIORITY"][0]["id"], "CzGfAU2z3TC")

    def test_data_only_delta_sections_are_compared_semantically(self) -> None:
        left = function_doc()
        left["_request"] = {"data_only": True}
        left["facets_delta"] = left.pop("facets")
        left["histogram_delta"] = left.pop("histogram")
        left["items_delta"] = left.pop("items")

        right = function_doc()
        right["_request"] = {"data_only": True}
        right["facets_delta"] = right.pop("facets")
        right["histogram_delta"] = right.pop("histogram")
        right["items_delta"] = right.pop("items")
        right["facets_delta"][0]["order"] = 99
        right["facets_delta"][0]["options"][0]["order"] = 42

        report = compare(left, right)
        self.assertTrue(report["ok"])
        self.assertTrue(report["checks"]["facets"])
        self.assertTrue(report["checks"]["histogram"])
        self.assertTrue(report["checks"]["items"])

    def test_data_only_ignores_plugin_only_all_null_columns(self) -> None:
        left = function_doc(
            columns={
                "timestamp": {"index": 0, "name": "Time"},
                "MESSAGE": {"index": 1, "name": "Message"},
            },
        )
        left["_request"] = {"data_only": True}
        right = function_doc(
            columns={
                "timestamp": {"index": 0, "name": "Time"},
                "MESSAGE": {"index": 1, "name": "Message"},
                "CODE_FILE": {"index": 2, "name": "CODE_FILE"},
            },
        )
        right["_request"] = {"data_only": True}
        right["data"] = [[1000, "hello", None]]

        report = compare(left, right)
        self.assertTrue(report["ok"])
        ignored = report["non_content"]["data_only_ignored_all_null_columns"]
        self.assertIn("CODE_FILE", ignored["right"])

    def test_data_only_missing_non_null_column_fails(self) -> None:
        left = function_doc(
            columns={
                "timestamp": {"index": 0, "name": "Time"},
                "MESSAGE": {"index": 1, "name": "Message"},
            },
        )
        left["_request"] = {"data_only": True}
        right = function_doc(
            columns={
                "timestamp": {"index": 0, "name": "Time"},
                "MESSAGE": {"index": 1, "name": "Message"},
                "CODE_FILE": {"index": 2, "name": "CODE_FILE"},
            },
        )
        right["_request"] = {"data_only": True}
        right["data"] = [[1000, "hello", "main.c"]]

        report = compare(left, right)
        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["columns"])
        self.assertFalse(report["checks"]["rows"])
        self.assertIn("CODE_FILE", report["diffs"]["rows"])

    def test_matching_function_error_envelopes_pass(self) -> None:
        error = {"status": 304, "errorMessage": "No new data since the previous call."}
        report = compare(error, dict(error))
        self.assertTrue(report["ok"])
        self.assertTrue(report["checks"]["function_error"])

    def test_function_error_envelope_mismatch_fails(self) -> None:
        report = compare(
            {"status": 304, "errorMessage": "No new data since the previous call."},
            {"status": 499, "errorMessage": "Request cancelled."},
        )
        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["function_error"])
        self.assertIn("errorMessage", report["diffs"]["function_error"])

    def test_test_mode_journal_file_path_roots_are_non_content(self) -> None:
        left = function_doc(
            columns={
                "timestamp": {"index": 0, "name": "Time"},
                "ND_JOURNAL_FILE": {"index": 1, "name": "ND_JOURNAL_FILE"},
            },
        )
        left["data"] = [[1000, "/proc/self/fd/3/system.journal"]]
        right = function_doc(
            columns={
                "timestamp": {"index": 0, "name": "Time"},
                "ND_JOURNAL_FILE": {"index": 1, "name": "ND_JOURNAL_FILE"},
            },
        )
        right["data"] = [[1000, ".local/sow-0093/smoke-journals/system.journal"]]

        report = compare(left, right)

        self.assertTrue(report["ok"])

    def test_test_mode_journal_file_basename_remains_content(self) -> None:
        left = function_doc(
            columns={
                "timestamp": {"index": 0, "name": "Time"},
                "ND_JOURNAL_FILE": {"index": 1, "name": "ND_JOURNAL_FILE"},
            },
        )
        left["data"] = [[1000, "/proc/self/fd/3/system.journal"]]
        right = function_doc(
            columns={
                "timestamp": {"index": 0, "name": "Time"},
                "ND_JOURNAL_FILE": {"index": 1, "name": "ND_JOURNAL_FILE"},
            },
        )
        right["data"] = [[1000, ".local/sow-0093/smoke-journals/other.journal"]]

        report = compare(left, right)

        self.assertFalse(report["ok"])
        self.assertIn("ND_JOURNAL_FILE", report["diffs"]["rows"])


def _doc_with_source_option_info(info: str) -> dict:
    """Return a function doc with the source-option `info` string set.

    The base `function_doc()` already emits an `info` facet option
    because the option `name` is `info`; this helper replaces its
    `info` field with the caller-supplied value so tests can pin the
    exact string.
    """

    doc = function_doc()
    doc["facets"][0]["options"][0]["info"] = info
    return doc


class SourceOptionInfoSkewToleranceTest(unittest.TestCase):
    """Live-journal race: the source-option `info` string embeds
    `covering <duration>, last entry at <iso>` from the live tail. A
    slower peer can see a tail seconds newer than a faster peer. The
    comparator tolerates a bounded skew on
    ONLY those two components. File counts and total sizes stay
    strict. `off`/`unknown` literals compare exactly.
    """

    def test_equal_info_strings_pass(self) -> None:
        info = "3 files, total size 5MiB, covering 1s, last entry at 2023-11-14T22:13:22Z"
        report = compare(_doc_with_source_option_info(info), _doc_with_source_option_info(info))

        self.assertTrue(report["ok"])
        self.assertTrue(report["checks"]["facets"])
        self.assertEqual(report["non_content"]["source_option_info_skew_tolerances"], [])

    def test_last_entry_skew_within_bound_is_tolerated_and_surfaced(self) -> None:
        left = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 1s, last entry at 2023-11-14T22:13:22Z"
        )
        right = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 1s, last entry at 2023-11-14T22:13:25Z"
        )

        report = compare(left, right)

        self.assertTrue(report["ok"])
        self.assertTrue(report["checks"]["facets"])
        skews = report["non_content"]["source_option_info_skew_tolerances"]
        self.assertEqual(len(skews), 1)
        self.assertEqual(skews[0]["facet_id"], "PRIORITY")
        self.assertEqual(skews[0]["skew_bound_seconds"], 300)
        self.assertEqual(skews[0]["fields"]["last_entry"]["delta_seconds"], 3)
        self.assertEqual(skews[0]["fields"]["last_entry"]["bound_seconds"], 300)
        self.assertEqual(skews[0]["fields"]["last_entry"]["left_seconds"], 1_700_000_002)
        self.assertEqual(skews[0]["fields"]["last_entry"]["right_seconds"], 1_700_000_005)

    def test_covering_duration_skew_within_bound_is_tolerated(self) -> None:
        # 1s vs 100s delta = 99s, well within 300s.
        left = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 1s, last entry at 2023-11-14T22:13:22Z"
        )
        right = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 100s, last entry at 2023-11-14T22:13:22Z"
        )

        report = compare(left, right)

        self.assertTrue(report["ok"])
        self.assertTrue(report["checks"]["facets"])
        skews = report["non_content"]["source_option_info_skew_tolerances"]
        self.assertEqual(len(skews), 1)
        self.assertIn("covering", skews[0]["fields"])
        self.assertEqual(skews[0]["fields"]["covering"]["delta_seconds"], 99)

    def test_both_fields_within_bound_are_tolerated_together(self) -> None:
        left = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 1s, last entry at 2023-11-14T22:13:22Z"
        )
        right = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 6s, last entry at 2023-11-14T22:13:25Z"
        )

        report = compare(left, right)

        self.assertTrue(report["ok"])
        skews = report["non_content"]["source_option_info_skew_tolerances"]
        self.assertEqual(len(skews), 1)
        self.assertIn("covering", skews[0]["fields"])
        self.assertIn("last_entry", skews[0]["fields"])

    def test_exact_bound_is_tolerated(self) -> None:
        # Exactly 300s delta must be tolerated (the rule is |delta| <= 300).
        left = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 1s, last entry at 2023-11-14T22:13:22Z"
        )
        right = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 1s, last entry at 2023-11-14T22:18:22Z"
        )

        report = compare(left, right)

        self.assertTrue(report["ok"])
        self.assertTrue(report["checks"]["facets"])
        skews = report["non_content"]["source_option_info_skew_tolerances"]
        self.assertEqual(skews[0]["fields"]["last_entry"]["delta_seconds"], 300)

    def test_last_entry_skew_beyond_bound_is_rejected(self) -> None:
        left = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 1s, last entry at 2023-11-14T22:13:22Z"
        )
        right = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 1s, last entry at 2023-11-14T22:18:23Z"
        )

        report = compare(left, right)

        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["facets"])
        self.assertEqual(report["non_content"]["source_option_info_skew_tolerances"], [])

    def test_covering_duration_skew_beyond_bound_is_rejected(self) -> None:
        # 1s vs 600s = 599s beyond the 300s bound.
        left = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 1s, last entry at 2023-11-14T22:13:22Z"
        )
        right = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 600s, last entry at 2023-11-14T22:13:22Z"
        )

        report = compare(left, right)

        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["facets"])

    def test_off_versus_duration_is_rejected(self) -> None:
        # off is a literal; a duration value must not be considered equal.
        left = _doc_with_source_option_info(
            "2 files, total size 2KiB, covering off, last entry at unknown"
        )
        right = _doc_with_source_option_info(
            "2 files, total size 2KiB, covering 1s, last entry at 2023-11-14T22:13:22Z"
        )

        report = compare(left, right)

        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["facets"])

    def test_unknown_versus_timestamp_is_rejected(self) -> None:
        left = _doc_with_source_option_info(
            "2 files, total size 2KiB, covering off, last entry at unknown"
        )
        right = _doc_with_source_option_info(
            "2 files, total size 2KiB, covering off, last entry at 2023-11-14T22:13:22Z"
        )

        report = compare(left, right)

        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["facets"])

    def test_off_equals_off_even_without_skew(self) -> None:
        info = "2 files, total size 2KiB, covering off, last entry at unknown"
        report = compare(_doc_with_source_option_info(info), _doc_with_source_option_info(info))

        self.assertTrue(report["ok"])
        self.assertEqual(report["non_content"]["source_option_info_skew_tolerances"], [])

    def test_files_count_mismatch_is_rejected(self) -> None:
        # Strict: file count must match exactly.
        left = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 1s, last entry at 2023-11-14T22:13:22Z"
        )
        right = _doc_with_source_option_info(
            "2 files, total size 5MiB, covering 1s, last entry at 2023-11-14T22:13:22Z"
        )

        report = compare(left, right)

        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["facets"])

    def test_total_size_mismatch_is_rejected(self) -> None:
        left = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 1s, last entry at 2023-11-14T22:13:22Z"
        )
        right = _doc_with_source_option_info(
            "3 files, total size 7MiB, covering 1s, last entry at 2023-11-14T22:13:22Z"
        )

        report = compare(left, right)

        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["facets"])

    def test_non_matching_shape_falls_back_to_exact(self) -> None:
        # Free-form strings that don't match the shape are compared exactly.
        left = _doc_with_source_option_info("hello world")
        right_equal = _doc_with_source_option_info("hello world")
        right_diff = _doc_with_source_option_info("hello earth")

        equal_report = compare(left, right_equal)
        self.assertTrue(equal_report["ok"])
        self.assertEqual(equal_report["non_content"]["source_option_info_skew_tolerances"], [])

        diff_report = compare(left, right_diff)
        self.assertFalse(diff_report["ok"])
        self.assertFalse(diff_report["checks"]["facets"])

    def test_tolerance_is_symmetric_across_peer_pairs(self) -> None:
        # The same tolerance logic runs in both directions because the
        # comparison is shape-based; a left/right swap must agree on
        # the structural outcome (skew accepted/rejected and the
        # bound/delta), even though the per-side left/right timestamps
        # swap in the diagnostic.
        a = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 1s, last entry at 2023-11-14T22:13:22Z"
        )
        b = _doc_with_source_option_info(
            "3 files, total size 5MiB, covering 1s, last entry at 2023-11-14T22:13:25Z"
        )
        forward = compare(a, b)
        reverse = compare(b, a)
        self.assertTrue(forward["ok"])
        self.assertTrue(reverse["ok"])
        for direction in (forward, reverse):
            skews = direction["non_content"]["source_option_info_skew_tolerances"]
            self.assertEqual(len(skews), 1)
            field = skews[0]["fields"]["last_entry"]
            self.assertEqual(field["delta_seconds"], 3)
            self.assertEqual(field["bound_seconds"], 300)
            self.assertEqual(skews[0]["skew_bound_seconds"], 300)


def _info_response_doc(
    *,
    required_params_info: str,
    extra_top_level: dict | None = None,
) -> dict:
    """Return a full info-response document (the shape the
    ``compare()`` runner consumes) with a single
    ``required_params[0].options[0]`` source option whose ``info``
    string is the caller-supplied value.

    ``extra_top_level`` lets the test override or add top-level
    fields (e.g. to assert a non-info top-level difference still
    fails).
    """

    doc = {
        "_request": {"info": True, "after": 0, "before": 0},
        "accepted_params": ["info", "__logs_sources", "after", "before"],
        "has_history": True,
        "help": "Netdata-compatible journal log function backed by the systemd journal SDK",
        "pagination": {
            "column": "timestamp",
            "enabled": True,
            "key": "anchor",
            "units": "timestamp_usec",
        },
        "required_params": [
            {
                "help": "Select the logs source to query",
                "id": "__logs_sources",
                "name": "Journal Sources",
                "options": [
                    {
                        "id": "all",
                        "info": required_params_info,
                        "name": "all",
                        "pill": "144.32GiB",
                    }
                ],
                "type": "multiselect",
            }
        ],
        "show_ids": True,
        "status": 200,
        "type": "table",
        "v": 3,
    }
    if extra_top_level:
        doc.update(extra_top_level)
    return doc


class RequiredParamsSourceInfoSkewToleranceTest(unittest.TestCase):
    """The same live-journal race applies to the source-option
    ``info`` strings exposed under the top-level ``required_params``
    envelope of an info response. The skew tolerance must be wired
    into the top-level comparison path (not just the facets path)
    so a slow peer does not produce a false-positive top-level
    mismatch."""

    def test_seven_second_skew_in_required_params_is_tolerated(self) -> None:
        left = _info_response_doc(
            required_params_info=(
                "7337 files, total size 144.32GiB, "
                "covering 2y 6mo 24d 11h 24m 25s, "
                "last entry at 2026-06-11T22:05:49Z"
            )
        )
        right = _info_response_doc(
            required_params_info=(
                "7337 files, total size 144.32GiB, "
                "covering 2y 6mo 24d 11h 24m 32s, "
                "last entry at 2026-06-11T22:05:56Z"
            )
        )

        report = compare(left, right)

        self.assertTrue(report["ok"])
        self.assertTrue(report["checks"]["top_level"])
        skews = report["non_content"][
            "required_params_source_info_skew_tolerances"
        ]
        self.assertEqual(len(skews), 1)
        self.assertEqual(skews[0]["source"], "required_params")
        self.assertEqual(skews[0]["option_id"], "all")
        self.assertEqual(
            skews[0]["path"], "$.required_params[0].options[0]"
        )
        self.assertEqual(skews[0]["skew_bound_seconds"], 300)
        self.assertIn("covering", skews[0]["fields"])
        self.assertIn("last_entry", skews[0]["fields"])
        self.assertEqual(skews[0]["fields"]["last_entry"]["delta_seconds"], 7)
        self.assertEqual(skews[0]["fields"]["covering"]["delta_seconds"], 7)
        # The facets-path diagnostics are not impacted by this test.
        self.assertEqual(
            report["non_content"]["source_option_info_skew_tolerances"], []
        )

    def test_skew_beyond_three_hundred_seconds_is_rejected(self) -> None:
        left = _info_response_doc(
            required_params_info=(
                "7337 files, total size 144.32GiB, "
                "covering 2y 6mo 24d 11h 24m 25s, "
                "last entry at 2026-06-11T22:05:49Z"
            )
        )
        right = _info_response_doc(
            required_params_info=(
                "7337 files, total size 144.32GiB, "
                "covering 2y 6mo 24d 11h 34m 25s, "
                "last entry at 2026-06-11T22:15:49Z"
            )
        )

        report = compare(left, right)

        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["top_level"])
        # Diff wording matches the original: names the option path
        # and shows both `info` values with the value-differs phrasing.
        diff = report["diffs"]["top_level"]
        self.assertIsNotNone(diff)
        self.assertIn("$.required_params[0].options[0].info", diff)
        self.assertIn("value differs", diff)
        self.assertIn(left["required_params"][0]["options"][0]["info"], diff)
        self.assertIn(right["required_params"][0]["options"][0]["info"], diff)
        # No tolerance was applied on the rejected pair.
        self.assertEqual(
            report["non_content"][
                "required_params_source_info_skew_tolerances"
            ],
            [],
        )

    def test_non_info_top_level_difference_still_fails(self) -> None:
        # Same info string on both sides so skew is irrelevant; the
        # accepted_params lists differ. This guards against an
        # over-permissive strip that would hide real top-level
        # diffs.
        info = (
            "7337 files, total size 144.32GiB, "
            "covering 2y 6mo 24d 11h 24m 25s, "
            "last entry at 2026-06-11T22:05:49Z"
        )
        left = _info_response_doc(
            required_params_info=info,
            extra_top_level={"accepted_params": ["info", "after", "before"]},
        )
        right = _info_response_doc(
            required_params_info=info,
            extra_top_level={
                "accepted_params": ["info", "after", "before", "facets"]
            },
        )

        report = compare(left, right)

        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["top_level"])
        self.assertIn("accepted_params", report["diffs"]["top_level"])
        # No skew tolerance should be reported when the only
        # difference is unrelated to the source-option `info` strings.
        self.assertEqual(
            report["non_content"][
                "required_params_source_info_skew_tolerances"
            ],
            [],
        )


def _request_window_echo_doc(
    *,
    after: int,
    before: int,
    info: bool = False,
    extra_request: dict | None = None,
) -> dict:
    """A minimal document with the `_request` echo shape. Mirrors
    the real wire format closely enough to exercise the top-level
    comparison path through the new tolerance."""

    request: dict = {
        "data_only": True,
        "after": after,
        "before": before,
    }
    if info:
        request["info"] = True
    if extra_request:
        request.update(extra_request)
    return {
        "status": 200,
        "type": "table",
        "_request": request,
        "columns": {},
        "data": [],
        "facets": [],
        "histogram": None,
        "items": {
            "matched": 0,
            "returned": 0,
            "max_to_return": 200,
            "after": 0,
            "before": 0,
            "unsampled": 0,
            "estimated": 0,
        },
    }


class RequestWindowSkewToleranceTest(unittest.TestCase):
    """SOW-0104 fix-10: the ``_request.after`` / ``_request.before``
    echoes embed parse-time ``unix_now_seconds()`` by reference
    design (Rust L1418 -> L3624-3690). Two peers invoked seconds
    apart legitimately produce different echoes; a slow third
    peer must not surface as a false-positive content mismatch.
    The comparator tolerates a bounded skew (<=300s) on those two
    fields ONLY. Other ``_request`` fields stay strict. Mirrors
    the fix-4 source-info tolerance precedent (same 300s bound,
    same diagnostics style)."""

    def test_equal_window_echoes_pass_without_tolerance(self) -> None:
        report = compare(
            _request_window_echo_doc(after=1_700_000_000, before=1_700_000_010),
            _request_window_echo_doc(after=1_700_000_000, before=1_700_000_010),
        )

        self.assertTrue(report["ok"])
        self.assertTrue(report["checks"]["top_level"])
        self.assertEqual(report["non_content"]["request_window_skew_tolerances"], [])

    def test_seven_second_skew_on_after_is_tolerated_and_surfaced(self) -> None:
        report = compare(
            _request_window_echo_doc(after=1_781_225_642, before=1_781_229_642),
            _request_window_echo_doc(after=1_781_225_649, before=1_781_229_649),
        )

        self.assertTrue(report["ok"])
        self.assertTrue(report["checks"]["top_level"])
        skews = report["non_content"]["request_window_skew_tolerances"]
        self.assertEqual(len(skews), 2)
        after_entry = next(s for s in skews if s["field"] == "after")
        before_entry = next(s for s in skews if s["field"] == "before")
        self.assertEqual(after_entry["delta_seconds"], 7)
        self.assertEqual(before_entry["delta_seconds"], 7)
        self.assertEqual(after_entry["bound_seconds"], 300)
        self.assertEqual(after_entry["left_seconds"], 1_781_225_642)
        self.assertEqual(after_entry["right_seconds"], 1_781_225_649)
        self.assertEqual(before_entry["left_seconds"], 1_781_229_642)
        self.assertEqual(before_entry["right_seconds"], 1_781_229_649)

    def test_only_after_skew_within_bound_is_tolerated(self) -> None:
        report = compare(
            _request_window_echo_doc(after=1_781_225_642, before=1_781_229_642),
            _request_window_echo_doc(after=1_781_225_700, before=1_781_229_642),
        )

        self.assertTrue(report["ok"])
        skews = report["non_content"]["request_window_skew_tolerances"]
        self.assertEqual(len(skews), 1)
        self.assertEqual(skews[0]["field"], "after")
        self.assertEqual(skews[0]["delta_seconds"], 58)

    def test_exact_bound_is_tolerated(self) -> None:
        report = compare(
            _request_window_echo_doc(after=1_700_000_000, before=1_700_000_010),
            _request_window_echo_doc(after=1_700_000_300, before=1_700_000_310),
        )

        self.assertTrue(report["ok"])
        skews = report["non_content"]["request_window_skew_tolerances"]
        self.assertEqual(len(skews), 2)
        for entry in skews:
            self.assertEqual(entry["delta_seconds"], 300)

    def test_skew_beyond_bound_is_rejected(self) -> None:
        report = compare(
            _request_window_echo_doc(after=1_700_000_000, before=1_700_000_010),
            _request_window_echo_doc(after=1_700_000_301, before=1_700_000_311),
        )

        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["top_level"])
        # No tolerance was applied on the rejected pair.
        self.assertEqual(report["non_content"]["request_window_skew_tolerances"], [])
        # The diff surfaces the exact field path and both values.
        diff = report["diffs"]["top_level"]
        self.assertIsNotNone(diff)
        self.assertIn("$._request.after", diff)
        self.assertIn("value differs", diff)

    def test_only_before_skew_beyond_bound_rejects(self) -> None:
        report = compare(
            _request_window_echo_doc(after=1_700_000_000, before=1_700_000_010),
            _request_window_echo_doc(after=1_700_000_000, before=1_700_000_311),
        )

        self.assertFalse(report["ok"])
        skews = report["non_content"]["request_window_skew_tolerances"]
        self.assertEqual(len(skews), 0)

    def test_other_request_field_mismatch_still_fails(self) -> None:
        # The tolerance is scoped to after/before ONLY. A
        # mismatch on another _request field must still fail.
        report = compare(
            _request_window_echo_doc(
                after=1_700_000_000, before=1_700_000_010,
                extra_request={"last": 5},
            ),
            _request_window_echo_doc(
                after=1_700_000_000, before=1_700_000_010,
                extra_request={"last": 6},
            ),
        )

        self.assertFalse(report["ok"])
        self.assertFalse(report["checks"]["top_level"])
        self.assertEqual(report["non_content"]["request_window_skew_tolerances"], [])

    def test_tolerance_is_symmetric_across_peer_pairs(self) -> None:
        # The same tolerance logic runs in both directions because
        # the comparison is shape-based; a left/right swap must
        # agree on the structural outcome (skew accepted/rejected
        # and the bound/delta), even though the per-side
        # left/right values swap in the diagnostic.
        a = _request_window_echo_doc(after=1_781_225_642, before=1_781_229_642)
        b = _request_window_echo_doc(after=1_781_225_649, before=1_781_229_649)
        forward = compare(a, b)
        reverse = compare(b, a)
        self.assertTrue(forward["ok"])
        self.assertTrue(reverse["ok"])
        for direction in (forward, reverse):
            skews = direction["non_content"]["request_window_skew_tolerances"]
            self.assertEqual(len(skews), 2)
            for entry in skews:
                self.assertEqual(entry["delta_seconds"], 7)
                self.assertEqual(entry["bound_seconds"], 300)

    def test_request_without_window_fields_skips_tolerance(self) -> None:
        # An info response (after/before popped by normalized_top_level)
        # produces empty window fields. The tolerance must skip them.
        report = compare(
            _request_window_echo_doc(after=0, before=0, info=True),
            _request_window_echo_doc(after=0, before=0, info=True),
        )

        self.assertTrue(report["ok"])
        self.assertEqual(report["non_content"]["request_window_skew_tolerances"], [])


if __name__ == "__main__":
    unittest.main()
