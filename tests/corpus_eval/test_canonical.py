#!/usr/bin/env python3
from __future__ import annotations

import struct
import unittest

from tests.corpus_eval.canonical import digest_entries, digest_export_bytes
from tests.corpus_eval.run_corpus_eval import json_from_stdout


class CanonicalDigestTests(unittest.TestCase):
    def test_payload_order_is_canonical_but_entry_order_is_not(self) -> None:
        a = digest_entries(
            [
                (
                    {
                        "__REALTIME_TIMESTAMP": 10,
                        "__MONOTONIC_TIMESTAMP": 20,
                        "__SEQNUM": 1,
                        "__BOOT_ID": "0123456789abcdef0123456789abcdef",
                    },
                    [b"B=2", b"A=1"],
                )
            ]
        )
        b = digest_entries(
            [
                (
                    {
                        "__REALTIME_TIMESTAMP": b"10",
                        "__MONOTONIC_TIMESTAMP": b"20",
                        "__SEQNUM": b"1",
                        "__BOOT_ID": b"0123456789abcdef0123456789abcdef",
                    },
                    [b"A=1", b"B=2"],
                )
            ]
        )
        c = digest_entries(
            [
                (
                    {
                        "__REALTIME_TIMESTAMP": 11,
                        "__MONOTONIC_TIMESTAMP": 20,
                        "__SEQNUM": 1,
                        "__BOOT_ID": "0123456789abcdef0123456789abcdef",
                    },
                    [b"A=1", b"B=2"],
                )
            ]
        )
        self.assertEqual(a["logical_digest"], b["logical_digest"])
        self.assertNotEqual(a["logical_digest"], c["logical_digest"])

    def test_repeated_fields_and_binary_payloads_are_counted_without_names(self) -> None:
        result = digest_entries(
            [
                (
                    {"__REALTIME_TIMESTAMP": 1},
                    [b"MESSAGE=hello", b"MESSAGE=again", b"BINARY=\x00\x01"],
                )
            ]
        )
        counts = result["counts"]
        self.assertEqual(counts["entries"], 1)
        self.assertEqual(counts["payloads"], 3)
        self.assertEqual(counts["binary_payloads"], 1)
        self.assertEqual(counts["entries_with_repeated_field_names"], 1)
        self.assertEqual(counts["repeated_field_name_occurrences"], 1)
        self.assertNotIn("MESSAGE", str(result))

    def test_payloads_without_separator_are_counted_once(self) -> None:
        result = digest_entries(
            [
                (
                    {"__REALTIME_TIMESTAMP": 1},
                    [b"=empty-name", b"NO_SEPARATOR", b"FIELD=value"],
                )
            ]
        )
        counts = result["counts"]
        self.assertEqual(counts["payloads"], 3)
        self.assertEqual(counts["payloads_without_separator"], 2)

    def test_systemd_export_text_and_binary_fields(self) -> None:
        binary_field = b"BINARY\n" + struct.pack("<Q", 5) + b"a\x00b\nc" + b"\n"
        data = (
            b"__REALTIME_TIMESTAMP=100\n"
            b"__MONOTONIC_TIMESTAMP=200\n"
            b"__SEQNUM=7\n"
            b"_BOOT_ID=0123456789abcdef0123456789abcdef\n"
            b"MESSAGE=hello\n"
            + binary_field
            + b"\n"
        )
        result = digest_export_bytes(data)
        counts = result["counts"]
        self.assertEqual(counts["entries"], 1)
        self.assertEqual(counts["payloads"], 2)
        self.assertEqual(counts["binary_payloads"], 1)
        self.assertEqual(counts["payloads_without_separator"], 0)

    def test_boot_id_payload_is_metadata_not_payload(self) -> None:
        result = digest_entries(
            [
                (
                    {"__BOOT_ID": "0123456789abcdef0123456789abcdef"},
                    [
                        b"_BOOT_ID=0123456789abcdef0123456789abcdef",
                        b"MESSAGE=hello",
                    ],
                )
            ]
        )
        counts = result["counts"]
        self.assertEqual(counts["payloads"], 1)
        self.assertEqual(counts["payload_bytes"], len(b"MESSAGE=hello"))

    def test_truncated_export_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            digest_export_bytes(b"BINARY\n" + struct.pack("<Q", 10) + b"short")

    def test_helper_stdout_must_be_one_json_object_line(self) -> None:
        self.assertEqual(json_from_stdout(b'{"status":"ok"}\n'), {"status": "ok"})
        with self.assertRaises(ValueError):
            json_from_stdout(b'{"progress":1}\n{"status":"ok"}\n')
        with self.assertRaises(ValueError):
            json_from_stdout(b"helper log line\n")


if __name__ == "__main__":
    unittest.main()
