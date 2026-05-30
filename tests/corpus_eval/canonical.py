#!/usr/bin/env python3
"""Binary-safe canonical journal entry digesting for corpus evaluation.

This module intentionally never exposes raw field names or values in result
objects. Callers may stream sensitive journals through it and persist only the
returned counts and digests.
"""

from __future__ import annotations

import hashlib
import io
import struct
from dataclasses import dataclass, field
from typing import BinaryIO, Iterable


SCHEMA_VERSION = "systemd-journal-sdk-corpus-logical-v1"
SCHEMA_MAGIC = (SCHEMA_VERSION + "\0").encode("ascii")
METADATA_ORDER = (
    "__REALTIME_TIMESTAMP",
    "__MONOTONIC_TIMESTAMP",
    "__SEQNUM",
    "__BOOT_ID",
)


def _u64(value: int) -> bytes:
    if value < 0:
        raise ValueError("negative value cannot be encoded as u64")
    return struct.pack(">Q", value)


def _to_bytes(value: bytes | str | int) -> bytes:
    if isinstance(value, bytes):
        return value
    if isinstance(value, str):
        return value.encode("utf-8")
    return str(value).encode("ascii")


def split_payload_name(payload: bytes) -> bytes | None:
    try:
        offset = payload.index(b"=")
    except ValueError:
        return None
    if offset == 0:
        return None
    return payload[:offset]


def payload_has_binary_bytes(payload: bytes) -> bool:
    return any(byte < 32 and byte != 9 for byte in payload)


@dataclass
class DigestCounts:
    entries: int = 0
    payloads: int = 0
    payload_bytes: int = 0
    binary_payloads: int = 0
    payloads_without_separator: int = 0
    entries_with_repeated_field_names: int = 0
    repeated_field_name_occurrences: int = 0
    largest_payload_bytes: int = 0

    def as_dict(self) -> dict[str, int]:
        return {
            "entries": self.entries,
            "payloads": self.payloads,
            "payload_bytes": self.payload_bytes,
            "binary_payloads": self.binary_payloads,
            "payloads_without_separator": self.payloads_without_separator,
            "entries_with_repeated_field_names": self.entries_with_repeated_field_names,
            "repeated_field_name_occurrences": self.repeated_field_name_occurrences,
            "largest_payload_bytes": self.largest_payload_bytes,
        }


@dataclass
class CanonicalDigest:
    """Incremental digest for a logical stream of journal entries."""

    _sha: hashlib._Hash = field(default_factory=hashlib.sha256)
    _started: bool = False
    counts: DigestCounts = field(default_factory=DigestCounts)

    def _ensure_started(self) -> None:
        if not self._started:
            self._sha.update(SCHEMA_MAGIC)
            self._started = True

    def _update_bytes(self, tag: bytes, value: bytes) -> None:
        self._sha.update(tag)
        self._sha.update(_u64(len(value)))
        self._sha.update(value)

    def _update_named_bytes(self, tag: bytes, name: bytes, value: bytes) -> None:
        self._sha.update(tag)
        self._sha.update(_u64(len(name)))
        self._sha.update(name)
        self._sha.update(_u64(len(value)))
        self._sha.update(value)

    def add_entry(
        self,
        metadata: dict[str, bytes | str | int] | None,
        payloads: Iterable[bytes],
    ) -> None:
        self._ensure_started()
        entry_index = self.counts.entries
        payload_list = [bytes(payload) for payload in payloads]
        metadata = {} if metadata is None else metadata

        self._sha.update(b"E")
        self._sha.update(_u64(entry_index))
        for key in METADATA_ORDER:
            if key in metadata:
                self._update_named_bytes(b"M", key.encode("ascii"), _to_bytes(metadata[key]))

        repeated_names: set[bytes] = set()
        seen_names: set[bytes] = set()
        repeated_occurrences = 0
        for payload in payload_list:
            self.counts.payloads += 1
            self.counts.payload_bytes += len(payload)
            self.counts.largest_payload_bytes = max(
                self.counts.largest_payload_bytes, len(payload)
            )
            if payload_has_binary_bytes(payload):
                self.counts.binary_payloads += 1
            name = split_payload_name(payload)
            if name is None:
                self.counts.payloads_without_separator += 1
                continue
            if name in seen_names:
                repeated_names.add(name)
                repeated_occurrences += 1
            else:
                seen_names.add(name)

        if repeated_names:
            self.counts.entries_with_repeated_field_names += 1
            self.counts.repeated_field_name_occurrences += repeated_occurrences

        for payload in sorted(payload_list):
            self._update_bytes(b"P", payload)
        self._sha.update(b"e")
        self.counts.entries += 1

    def result(self) -> dict[str, object]:
        self._ensure_started()
        return {
            "schema": SCHEMA_VERSION,
            "logical_digest": self._sha.hexdigest(),
            "counts": self.counts.as_dict(),
        }


def iter_export_entries(stream: BinaryIO):
    """Yield `(metadata, payloads)` entries from systemd Journal Export Format."""

    metadata: dict[str, bytes] = {}
    payloads: list[bytes] = []
    while True:
        line = stream.readline()
        if line == b"":
            if metadata or payloads:
                yield metadata, payloads
            return
        if line == b"\n":
            if metadata or payloads:
                yield metadata, payloads
            metadata = {}
            payloads = []
            continue
        if not line.endswith(b"\n"):
            raise ValueError("truncated journal export field line")
        line = line[:-1]
        if b"=" in line:
            name, value = line.split(b"=", 1)
        else:
            name = line
            size_raw = stream.read(8)
            if len(size_raw) != 8:
                raise ValueError("truncated binary journal export field size")
            size = struct.unpack("<Q", size_raw)[0]
            value = stream.read(size)
            if len(value) != size:
                raise ValueError("truncated binary journal export field value")
            trailer = stream.read(1)
            if trailer != b"\n":
                raise ValueError("truncated binary journal export field trailer")

        if name == b"_BOOT_ID":
            metadata["__BOOT_ID"] = value
            continue
        if name.startswith(b"__"):
            try:
                key = name.decode("ascii")
            except UnicodeDecodeError:
                continue
            if key in METADATA_ORDER:
                metadata[key] = value
            continue
        payloads.append(name + b"=" + value)


def digest_entries(
    entries: Iterable[tuple[dict[str, bytes | str | int], Iterable[bytes]]],
) -> dict[str, object]:
    digest = CanonicalDigest()
    for metadata, payloads in entries:
        digest.add_entry(metadata, payloads)
    return digest.result()


def digest_export_stream(stream: BinaryIO) -> dict[str, object]:
    return digest_entries(iter_export_entries(stream))


def digest_export_bytes(data: bytes) -> dict[str, object]:
    return digest_export_stream(io.BytesIO(data))
