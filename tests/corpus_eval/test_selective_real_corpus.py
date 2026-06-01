from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from tests.corpus_eval.run_corpus_eval import discover_cases
from tests.corpus_eval.run_selective_real_corpus import (
    classify_probe,
    mark_distribution_features,
    parse_header,
    select_probes,
)


def write_minimal_journal(
    path: Path,
    *,
    state: int,
    compatible_flags: int = 0,
    incompatible_flags: int = 1 << 2,
    n_entries: int = 1,
    n_data: int = 1,
    n_fields: int = 1,
    n_tags: int = 0,
) -> None:
    raw = bytearray(4096)
    raw[0:8] = b"LPKSHHRH"
    raw[8:12] = compatible_flags.to_bytes(4, "little")
    raw[12:16] = incompatible_flags.to_bytes(4, "little")
    raw[16] = state
    raw[88:96] = (272).to_bytes(8, "little")
    raw[96:104] = (4096 - 272).to_bytes(8, "little")
    raw[136:144] = (0).to_bytes(8, "little")
    raw[144:152] = (0).to_bytes(8, "little")
    raw[152:160] = n_entries.to_bytes(8, "little")
    raw[208:216] = n_data.to_bytes(8, "little")
    raw[216:224] = n_fields.to_bytes(8, "little")
    raw[224:232] = n_tags.to_bytes(8, "little")
    path.write_bytes(raw)


class SelectiveRealCorpusTests(unittest.TestCase):
    def test_parse_header_records_sanitized_feature_fields(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "system.journal"
            write_minimal_journal(
                path,
                state=2,
                compatible_flags=1,
                incompatible_flags=(1 << 2) | (1 << 4),
                n_entries=7,
                n_data=11,
                n_fields=3,
                n_tags=2,
            )
            header = parse_header(path)
        self.assertEqual(header["state"], "archived")
        self.assertIn("compact", header["incompatible_flags"])
        self.assertIn("sealed", header["compatible_flags"])
        self.assertEqual(header["n_entries"], 7)
        self.assertEqual(header["n_data"], 11)

    def test_classification_and_selection_are_feature_based(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archived = root / "archived.journal"
            active = root / "active.journal"
            historical = root / "historical.journal"
            write_minimal_journal(archived, state=2, incompatible_flags=(1 << 2) | (1 << 4), n_data=20)
            write_minimal_journal(active, state=1, incompatible_flags=(1 << 2) | (1 << 3), n_fields=30)
            write_minimal_journal(historical, state=0, incompatible_flags=0, n_data=40)
            cases = discover_cases([root])
            probes = [classify_probe(case, set(), 1) for case in cases]
            mark_distribution_features(probes, large_min_bytes=1)
            selected, missing = select_probes(probes, max_selected=3)

        selected_features = {reason for probe in selected for reason in probe.selection_reasons}
        self.assertIn("historical-unkeyed", selected_features)
        self.assertIn("compact", selected_features)
        self.assertIn("compressed-data", selected_features)
        self.assertIn("previous-bug-exposure", missing)


if __name__ == "__main__":
    unittest.main()
