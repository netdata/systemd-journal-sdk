#!/usr/bin/env python3
"""Unit tests for Netdata function anchor regression helpers."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from run_anchor_regression import (  # noqa: E402
    edge_anchor,
    validate_collected_messages,
    validate_ordered_scalar_anchor,
)


def row(timestamp: int, message: str = "") -> dict[str, object]:
    return {"timestamp": timestamp, "message": message or f"row-{timestamp}"}


class AnchorRegressionHelpersTest(unittest.TestCase):
    def test_backward_anchor_allows_gaps_but_requires_strict_progress(self) -> None:
        report = validate_ordered_scalar_anchor(
            "backward",
            100,
            [row(120), row(100)],
            [row(80), row(60)],
        )

        self.assertTrue(report["ok"])
        self.assertTrue(report["non_overlapping"])
        self.assertTrue(report["anchor_progressed"])
        self.assertEqual(report["page2_edge_anchor"], 60)

    def test_backward_anchor_rejects_overlap_at_previous_anchor(self) -> None:
        report = validate_ordered_scalar_anchor(
            "backward",
            100,
            [row(120), row(100)],
            [row(100), row(90)],
        )

        self.assertFalse(report["ok"])
        self.assertFalse(report["non_overlapping"])
        self.assertTrue(report["anchor_progressed"])

    def test_forward_anchor_allows_gaps_but_requires_strict_progress(self) -> None:
        report = validate_ordered_scalar_anchor(
            "forward",
            100,
            [row(100), row(120)],
            [row(130), row(170)],
        )

        self.assertTrue(report["ok"])
        self.assertEqual(report["page2_edge_anchor"], 130)

    def test_forward_anchor_rejects_overlap_at_previous_anchor(self) -> None:
        report = validate_ordered_scalar_anchor(
            "forward",
            100,
            [row(100), row(120)],
            [row(100), row(130)],
        )

        self.assertFalse(report["ok"])
        self.assertFalse(report["non_overlapping"])
        self.assertFalse(report["anchor_progressed"])

    def test_ordered_scalar_rejects_internally_unordered_pages(self) -> None:
        report = validate_ordered_scalar_anchor(
            "backward",
            100,
            [row(120), row(130)],
            [row(80), row(70)],
        )

        self.assertFalse(report["ok"])
        self.assertFalse(report["page1_ordered"])

    def test_empty_second_page_is_valid_after_boundary_group_is_complete(self) -> None:
        report = validate_ordered_scalar_anchor(
            "backward",
            100,
            [row(100, "source-a"), row(100, "source-b"), row(100, "source-c")],
            [],
        )

        self.assertTrue(report["ok"])
        self.assertTrue(report["non_overlapping"])
        self.assertTrue(report["anchor_progressed"])

    def test_edge_anchor_matches_ui_anchor_side(self) -> None:
        rows = [row(130), row(120), row(110)]

        self.assertEqual(edge_anchor(rows, "backward"), 110)
        self.assertEqual(edge_anchor(rows, "forward"), 130)
        self.assertIsNone(edge_anchor([], "backward"))

    def test_collected_messages_reject_missing_and_duplicate_rows(self) -> None:
        report = validate_collected_messages(
            ["source-a", "source-b", "source-c"],
            [row(100, "source-a"), row(90, "source-b")],
            [row(80, "source-b")],
        )

        self.assertFalse(report["ok"])
        self.assertEqual(report["missing_messages"], ["source-c"])
        self.assertEqual(report["duplicate_messages"], ["source-b"])


if __name__ == "__main__":
    unittest.main()
