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
        "histogram": (
            {
                "id": "PRIORITY",
                "name": "PRIORITY",
                "chart": {
                    "result": {
                        "labels": ["time", "info"],
                        "data": [[1000, [histogram_value, 0, 0]]],
                    }
                },
            }
            if include_histogram
            else None
        ),
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


if __name__ == "__main__":
    unittest.main()
