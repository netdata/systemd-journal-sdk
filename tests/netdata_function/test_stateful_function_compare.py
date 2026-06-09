#!/usr/bin/env python3
"""Unit tests for stateful Netdata function comparison helpers."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from run_stateful_function_compare import (  # noqa: E402
    SequenceState,
    assert_no_duplicate_rows,
    assert_tail_rows_newer,
    data_only_request,
    filtered_tail_request,
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


if __name__ == "__main__":
    unittest.main()
