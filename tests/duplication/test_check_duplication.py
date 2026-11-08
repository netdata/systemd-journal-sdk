from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from check_duplication import compare, group_clones, is_test_path, normalize  # noqa: E402


class ClassificationTest(unittest.TestCase):
    def test_test_and_harness_paths(self) -> None:
        for path in (
            "go/journal/reader_test.go",
            "rust/src/crates/journal-registry/tests/overlapping_watches.rs",
            "rust/src/journal/src/tests.rs",
            "rust/src/crates/journal-core/src/file/file_tests.rs",
            "rust/src/crates/journal-core/src/file/writer_test.rs",
            "go/internal/testcmd/reader_core_bench/main.go",
            "rust/src/internal/testcmd/livewriter/src/main.rs",
            "rust/src/crates/journal-engine/examples/index.rs",
        ):
            self.assertTrue(is_test_path(path), path)

    def test_production_paths(self) -> None:
        for path in (
            "go/journal/directory_reader.go",
            "go/cmd/journalctl/main.go",
            "rust/src/journal/src/netdata.rs",
            "rust/src/crates/journal-core/src/file/latests.rs",
        ):
            self.assertFalse(is_test_path(path), path)

    def test_normalize_prefixes_language_root(self) -> None:
        self.assertEqual(normalize("go", "journal/log.go"), "go/journal/log.go")
        self.assertEqual(normalize("go", "go/journal/log.go"), "go/journal/log.go")


class GroupingTest(unittest.TestCase):
    def test_pairs_are_order_independent_and_summed(self) -> None:
        production, test_lines = group_clones(
            [
                {"first": "go/a.go", "second": "go/b.go", "lines": 10},
                {"first": "go/b.go", "second": "go/a.go", "lines": 5},
                {"first": "go/a.go", "second": "go/a_test.go", "lines": 7},
            ]
        )
        self.assertEqual(production, {"go/a.go | go/b.go": 15})
        self.assertEqual(test_lines, 7)


class CompareTest(unittest.TestCase):
    def test_new_pair_fails(self) -> None:
        failures, _ = compare({}, {"go/a.go | go/b.go": 12})
        self.assertEqual(len(failures), 1)
        self.assertIn("new duplication", failures[0])

    def test_growth_fails(self) -> None:
        failures, _ = compare({"go/a.go | go/b.go": 10}, {"go/a.go | go/b.go": 11})
        self.assertIn("grew 10 -> 11", failures[0])

    def test_unchanged_passes(self) -> None:
        self.assertEqual(compare({"k": 10}, {"k": 10}), ([], []))

    def test_shrink_and_removal_are_improvements(self) -> None:
        failures, improvements = compare({"a": 10, "b": 4}, {"a": 6})
        self.assertEqual(failures, [])
        self.assertEqual(len(improvements), 2)


if __name__ == "__main__":
    unittest.main()
