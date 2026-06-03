"""Structural inspector for SDK-generated systemd journal files.

The checks here intentionally avoid byte-for-byte comparison. They validate the
object graph invariants that systemd readers depend on when compressor output or
compact offsets make raw byte identity the wrong oracle.
"""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Any


HEADER_MIN_SIZE = 208
OBJECT_HEADER_SIZE = 16
HASH_ITEM_SIZE = 16
DATA_OBJECT_HEADER_SIZE = 64
COMPACT_DATA_OBJECT_HEADER_SIZE = 72
FIELD_OBJECT_HEADER_SIZE = 40
ENTRY_OBJECT_HEADER_SIZE = 64
ENTRY_ARRAY_OBJECT_HEADER_SIZE = 24
REGULAR_ENTRY_ITEM_SIZE = 16
COMPACT_ENTRY_ITEM_SIZE = 4
REGULAR_ENTRY_ARRAY_ITEM_SIZE = 8
COMPACT_ENTRY_ARRAY_ITEM_SIZE = 4
JOURNAL_COMPACT_SIZE_MAX = (1 << 32) - 1
TAG_OBJECT_SIZE = OBJECT_HEADER_SIZE + 8 + 8 + (256 // 8)

OBJECT_TYPE_DATA = 1
OBJECT_TYPE_FIELD = 2
OBJECT_TYPE_ENTRY = 3
OBJECT_TYPE_DATA_HASH_TABLE = 4
OBJECT_TYPE_FIELD_HASH_TABLE = 5
OBJECT_TYPE_ENTRY_ARRAY = 6
OBJECT_TYPE_TAG = 7

OBJECT_TYPES = {
    OBJECT_TYPE_DATA: "DATA",
    OBJECT_TYPE_FIELD: "FIELD",
    OBJECT_TYPE_ENTRY: "ENTRY",
    OBJECT_TYPE_DATA_HASH_TABLE: "DATA_HASH_TABLE",
    OBJECT_TYPE_FIELD_HASH_TABLE: "FIELD_HASH_TABLE",
    OBJECT_TYPE_ENTRY_ARRAY: "ENTRY_ARRAY",
    OBJECT_TYPE_TAG: "TAG",
}

OBJECT_COMPRESSED_XZ = 1 << 0
OBJECT_COMPRESSED_LZ4 = 1 << 1
OBJECT_COMPRESSED_ZSTD = 1 << 2
OBJECT_COMPRESSED_MASK = OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD

INCOMPATIBLE_COMPRESSED_XZ = 1 << 0
INCOMPATIBLE_COMPRESSED_LZ4 = 1 << 1
INCOMPATIBLE_KEYED_HASH = 1 << 2
INCOMPATIBLE_COMPRESSED_ZSTD = 1 << 3
INCOMPATIBLE_COMPACT = 1 << 4
INCOMPATIBLE_COMPRESSION_MASK = (
    INCOMPATIBLE_COMPRESSED_XZ | INCOMPATIBLE_COMPRESSED_LZ4 | INCOMPATIBLE_COMPRESSED_ZSTD
)

COMPATIBLE_TAIL_ENTRY_BOOT_ID = 1 << 1

COMPRESSION_OBJECT_FLAGS = {
    "xz": OBJECT_COMPRESSED_XZ,
    "lz4": OBJECT_COMPRESSED_LZ4,
    "zstd": OBJECT_COMPRESSED_ZSTD,
}

COMPRESSION_HEADER_FLAGS = {
    "xz": INCOMPATIBLE_COMPRESSED_XZ,
    "lz4": INCOMPATIBLE_COMPRESSED_LZ4,
    "zstd": INCOMPATIBLE_COMPRESSED_ZSTD,
}

HEADER_FIELDS = (
    ("compatible_flags", 8, 12, "u32"),
    ("incompatible_flags", 12, 16, "u32"),
    ("state", 16, 17, "u8"),
    ("file_id", 24, 40, "bytes"),
    ("machine_id", 40, 56, "bytes"),
    ("tail_entry_boot_id", 56, 72, "bytes"),
    ("seqnum_id", 72, 88, "bytes"),
    ("header_size", 88, 96, "u64"),
    ("arena_size", 96, 104, "u64"),
    ("data_hash_table_offset", 104, 112, "u64"),
    ("data_hash_table_size", 112, 120, "u64"),
    ("field_hash_table_offset", 120, 128, "u64"),
    ("field_hash_table_size", 128, 136, "u64"),
    ("tail_object_offset", 136, 144, "u64"),
    ("n_objects", 144, 152, "u64"),
    ("n_entries", 152, 160, "u64"),
    ("tail_entry_seqnum", 160, 168, "u64"),
    ("head_entry_seqnum", 168, 176, "u64"),
    ("entry_array_offset", 176, 184, "u64"),
    ("head_entry_realtime", 184, 192, "u64"),
    ("tail_entry_realtime", 192, 200, "u64"),
    ("tail_entry_monotonic", 200, 208, "u64"),
    ("n_data", 208, 216, "u64"),
    ("n_fields", 216, 224, "u64"),
    ("n_tags", 224, 232, "u64"),
    ("n_entry_arrays", 232, 240, "u64"),
    ("data_hash_chain_depth", 240, 248, "u64"),
    ("field_hash_chain_depth", 248, 256, "u64"),
    ("tail_entry_array_offset", 256, 260, "u32"),
    ("tail_entry_array_n_entries", 260, 264, "u32"),
    ("tail_entry_offset", 264, 272, "u64"),
)


@dataclass(frozen=True)
class ObjectSpan:
    offset: int
    end: int
    typ: int
    flags: int
    size: int


@dataclass(frozen=True)
class RawObjectHeader:
    typ: int
    flags: int
    size: int
    aligned_end: int


@dataclass(frozen=True)
class DataObject:
    offset: int
    hash: int
    next_hash_offset: int
    next_field_offset: int
    entry_offset: int
    entry_array_offset: int
    n_entries: int
    compact_tail_entry_array_offset: int
    compact_tail_entry_array_n_entries: int


@dataclass(frozen=True)
class FieldObject:
    offset: int
    hash: int
    next_hash_offset: int
    head_data_offset: int


@dataclass(frozen=True)
class EntryObject:
    offset: int
    seqnum: int
    realtime: int
    monotonic: int
    boot_id: bytes
    item_offsets: tuple[int, ...]


@dataclass(frozen=True)
class EntryArrayObject:
    offset: int
    next_entry_array_offset: int
    capacity: int
    items: tuple[int, ...]


@dataclass(frozen=True)
class HashChainSpec:
    name: str
    object_type: int
    objects: dict[int, Any]
    table_offset: int
    table_size: int
    header_depth_field: str


@dataclass
class EntryArrayWalkState:
    used_items: list[tuple[int, int]]
    remaining: int
    offset: int
    seen: set[int]
    last_array_offset: int = 0
    last_used: int = 0


def inspect_journal_structure(
    journal_path: str | Path,
    *,
    expected_compact: bool | None = None,
    expected_compression: str | None = None,
    test_name: str = "journal-structure",
) -> dict[str, Any]:
    """Return a matrix-compatible PASS/FAIL structural inspection result."""

    try:
        inspector = _JournalStructureInspector(
            Path(journal_path).read_bytes(),
            expected_compact=expected_compact,
            expected_compression=expected_compression,
        )
        result = inspector.inspect()
    except Exception as err:
        return {"test": test_name, "status": "FAIL", "error": str(err)}

    result["test"] = test_name
    result["status"] = "FAIL" if result.pop("errors") else "PASS"
    result["error"] = "; ".join(result.pop("_error_messages", []))
    return result


class _JournalStructureInspector:
    def __init__(
        self,
        data: bytes,
        *,
        expected_compact: bool | None,
        expected_compression: str | None,
    ) -> None:
        self.data = data
        self.expected_compact = expected_compact
        self.expected_compression = expected_compression
        self.errors: list[str] = []
        self.header: dict[str, Any] = {}
        self.spans: list[ObjectSpan] = []
        self.by_offset: dict[int, ObjectSpan] = {}
        self.data_objects: dict[int, DataObject] = {}
        self.field_objects: dict[int, FieldObject] = {}
        self.entry_objects: dict[int, EntryObject] = {}
        self.entry_arrays: dict[int, EntryArrayObject] = {}
        self.referenced_entry_arrays: set[int] = set()
        self.actual_data_hash_chain_depth = 0
        self.actual_field_hash_chain_depth = 0

    def inspect(self) -> dict[str, Any]:
        self._read_header()
        if not self.errors:
            self._walk_objects()
        if not self.errors:
            self._validate_layout()

        compression_counts = Counter()
        for span in self.spans:
            if span.typ == OBJECT_TYPE_DATA:
                if span.flags & OBJECT_COMPRESSED_XZ:
                    compression_counts["xz"] += 1
                if span.flags & OBJECT_COMPRESSED_LZ4:
                    compression_counts["lz4"] += 1
                if span.flags & OBJECT_COMPRESSED_ZSTD:
                    compression_counts["zstd"] += 1

        type_counts = Counter(OBJECT_TYPES.get(span.typ, f"UNKNOWN_{span.typ}") for span in self.spans)
        return {
            "errors": bool(self.errors),
            "_error_messages": self.errors,
            "header_size": self.header.get("header_size", 0),
            "state": self.header.get("state", 0),
            "compatible_flags": self.header.get("compatible_flags", 0),
            "incompatible_flags": self.header.get("incompatible_flags", 0),
            "compact": self._is_compact(),
            "object_count": len(self.spans),
            "object_type_counts": dict(sorted(type_counts.items())),
            "object_type_order": [OBJECT_TYPES.get(span.typ, f"UNKNOWN_{span.typ}") for span in self.spans],
            "tail_object_offset": self.header.get("tail_object_offset", 0),
            "tail_entry_offset": self.header.get("tail_entry_offset", 0),
            "n_entries": self.header.get("n_entries", 0),
            "n_data": self.header.get("n_data", 0),
            "n_fields": self.header.get("n_fields", 0),
            "n_entry_arrays": self.header.get("n_entry_arrays", 0),
            "data_hash_chain_depth": self.header.get("data_hash_chain_depth", 0),
            "field_hash_chain_depth": self.header.get("field_hash_chain_depth", 0),
            "actual_data_hash_chain_depth": self.actual_data_hash_chain_depth,
            "actual_field_hash_chain_depth": self.actual_field_hash_chain_depth,
            "compressed_data_objects": dict(sorted(compression_counts.items())),
            "referenced_entry_arrays": len(self.referenced_entry_arrays),
        }

    def _read_header(self) -> None:
        if not self._has_readable_header_prefix():
            return
        self._read_header_fields()
        self._validate_header_geometry()
        self._validate_header_semantics()

    def _has_readable_header_prefix(self) -> bool:
        if len(self.data) < HEADER_MIN_SIZE:
            self._error(f"journal smaller than minimum header: {len(self.data)} < {HEADER_MIN_SIZE}")
            return False
        if self.data[:8] != b"LPKSHHRH":
            self._error("invalid journal signature")
            return False
        return True

    def _read_header_fields(self) -> None:
        for name, start, end, kind in HEADER_FIELDS:
            if len(self.data) >= end:
                self.header[name] = self._read_header_field(start, end, kind)

    def _read_header_field(self, start: int, end: int, kind: str) -> int | bytes:
        if kind == "u8":
            return self.data[start]
        if kind == "u32":
            return _u32(self.data, start)
        if kind == "u64":
            return _u64(self.data, start)
        return bytes(self.data[start:end])

    def _validate_header_geometry(self) -> None:
        header_size = self.header.get("header_size", 0)
        arena_size = self.header.get("arena_size", 0)
        if header_size < HEADER_MIN_SIZE:
            self._error(f"invalid header_size {header_size}: smaller than {HEADER_MIN_SIZE}")
        if header_size > len(self.data):
            self._error(f"invalid header_size {header_size}: exceeds file size {len(self.data)}")
        if header_size % 8 != 0:
            self._error(f"header_size {header_size} is not 8-byte aligned")
        if self.header.get("state") not in (0, 1, 2):
            self._error(f"invalid journal state {self.header.get('state')}")
        if header_size + arena_size > len(self.data):
            self._error(
                f"header_size + arena_size exceeds file size: {header_size} + {arena_size} > {len(self.data)}"
            )

    def _validate_header_semantics(self) -> None:
        if self.header.get("incompatible_flags", 0) & INCOMPATIBLE_KEYED_HASH == 0:
            self._error("HEADER_INCOMPATIBLE_KEYED_HASH not set")
        if self.expected_compact is not None and self._is_compact() != self.expected_compact:
            self._error(f"compact flag mismatch: got {self._is_compact()}, want {self.expected_compact}")

    def _walk_objects(self) -> None:
        offset, tail = self._object_walk_bounds()
        if offset is None:
            return

        while True:
            header = self._read_object_header(offset, tail)
            if header is None:
                return
            span = ObjectSpan(
                offset=offset,
                end=header.aligned_end,
                typ=header.typ,
                flags=header.flags,
                size=header.size,
            )
            self.spans.append(span)
            self.by_offset[offset] = span
            self._parse_object(span)

            if offset == tail:
                break
            next_offset = header.aligned_end
            if next_offset <= offset:
                self._error(f"object walk did not advance from offset {offset}")
                return
            offset = next_offset

        self._validate_trailing_padding()

    def _object_walk_bounds(self) -> tuple[int | None, int]:
        offset = self.header["header_size"]
        tail = self.header["tail_object_offset"]
        if tail == 0:
            self._error("tail_object_offset is zero")
            return None, tail
        if tail < offset:
            self._error(f"tail_object_offset {tail} is before header_size {offset}")
            return None, tail
        return offset, tail

    def _read_object_header(self, offset: int, tail: int) -> RawObjectHeader | None:
        if offset > tail:
            self._error(f"object walk skipped past tail_object_offset {tail} at {offset}")
            return None
        if offset + OBJECT_HEADER_SIZE > len(self.data):
            self._error(f"object header at offset {offset} exceeds file bounds")
            return None
        header = RawObjectHeader(
            typ=self.data[offset],
            flags=self.data[offset + 1],
            size=_u64(self.data, offset + 8),
            aligned_end=offset + align8(_u64(self.data, offset + 8)),
        )
        return header if self._validate_object_header(offset, header) else None

    def _validate_object_header(self, offset: int, header: RawObjectHeader) -> bool:
        if header.typ == 0 and header.size == 0:
            self._error(f"zero object encountered before tail at offset {offset}")
            return False
        if header.size < OBJECT_HEADER_SIZE:
            self._error(f"object at offset {offset} has invalid size {header.size}")
            return False
        if header.aligned_end > len(self.data):
            self._error(f"object at offset {offset} with aligned size {align8(header.size)} exceeds file bounds")
            return False
        if offset % 8 != 0:
            self._error(f"object offset {offset} is not 8-byte aligned")
        if header.typ not in OBJECT_TYPES:
            self._error(f"unknown object type {header.typ} at offset {offset}")
        return True

    def _validate_trailing_padding(self) -> None:
        padding = self.data[self.spans[-1].end :]
        if any(padding):
            self._error(f"non-zero bytes found after tail object at offset {self.spans[-1].end}")

    def _parse_object(self, span: ObjectSpan) -> None:
        self._validate_object_flags(span)
        if span.typ == OBJECT_TYPE_DATA:
            self._parse_data(span)
            return
        parser = self._object_parser(span.typ)
        if parser is not None:
            parser(span)

    def _validate_object_flags(self, span: ObjectSpan) -> None:
        if span.flags & ~OBJECT_COMPRESSED_MASK:
            self._error(f"object at offset {span.offset} has unknown flags 0x{span.flags:x}")
        if _flag_count(span.flags & OBJECT_COMPRESSED_MASK) > 1:
            self._error(f"object at offset {span.offset} has multiple compression flags")
        if span.typ != OBJECT_TYPE_DATA and span.flags != 0:
            self._error(f"{OBJECT_TYPES.get(span.typ, 'UNKNOWN')} object at offset {span.offset} has flags 0x{span.flags:x}")

    def _object_parser(self, typ: int) -> Any:
        return {
            OBJECT_TYPE_FIELD: self._parse_field,
            OBJECT_TYPE_ENTRY: self._parse_entry,
            OBJECT_TYPE_ENTRY_ARRAY: self._parse_entry_array,
            OBJECT_TYPE_DATA_HASH_TABLE: self._parse_hash_table,
            OBJECT_TYPE_FIELD_HASH_TABLE: self._parse_hash_table,
            OBJECT_TYPE_TAG: self._parse_tag,
        }.get(typ)

    def _parse_hash_table(self, span: ObjectSpan) -> None:
        if span.size < OBJECT_HEADER_SIZE + HASH_ITEM_SIZE:
            self._error(f"{OBJECT_TYPES[span.typ]} at offset {span.offset} is too small")
        elif (span.size - OBJECT_HEADER_SIZE) % HASH_ITEM_SIZE != 0:
            self._error(f"{OBJECT_TYPES[span.typ]} at offset {span.offset} has unaligned hash items")

    def _parse_tag(self, span: ObjectSpan) -> None:
        if span.size != TAG_OBJECT_SIZE:
            self._error(f"TAG object at offset {span.offset} has invalid size {span.size}")

    def _parse_data(self, span: ObjectSpan) -> None:
        payload_offset = COMPACT_DATA_OBJECT_HEADER_SIZE if self._is_compact() else DATA_OBJECT_HEADER_SIZE
        if span.size <= payload_offset:
            self._error(f"DATA object at offset {span.offset} has no payload")
            return
        self.data_objects[span.offset] = DataObject(
            offset=span.offset,
            hash=_u64(self.data, span.offset + 16),
            next_hash_offset=_u64(self.data, span.offset + 24),
            next_field_offset=_u64(self.data, span.offset + 32),
            entry_offset=_u64(self.data, span.offset + 40),
            entry_array_offset=_u64(self.data, span.offset + 48),
            n_entries=_u64(self.data, span.offset + 56),
            compact_tail_entry_array_offset=_u32(self.data, span.offset + 64) if self._is_compact() else 0,
            compact_tail_entry_array_n_entries=_u32(self.data, span.offset + 68) if self._is_compact() else 0,
        )

    def _parse_field(self, span: ObjectSpan) -> None:
        if span.size <= FIELD_OBJECT_HEADER_SIZE:
            self._error(f"FIELD object at offset {span.offset} has no payload")
            return
        self.field_objects[span.offset] = FieldObject(
            offset=span.offset,
            hash=_u64(self.data, span.offset + 16),
            next_hash_offset=_u64(self.data, span.offset + 24),
            head_data_offset=_u64(self.data, span.offset + 32),
        )

    def _parse_entry(self, span: ObjectSpan) -> None:
        item_size = COMPACT_ENTRY_ITEM_SIZE if self._is_compact() else REGULAR_ENTRY_ITEM_SIZE
        if span.size < ENTRY_OBJECT_HEADER_SIZE:
            self._error(f"ENTRY object at offset {span.offset} is too small")
            return
        if (span.size - ENTRY_OBJECT_HEADER_SIZE) % item_size != 0:
            self._error(f"ENTRY object at offset {span.offset} has unaligned {item_size}-byte items")
            return
        item_offsets: list[int] = []
        for item_offset in range(span.offset + ENTRY_OBJECT_HEADER_SIZE, span.offset + span.size, item_size):
            if self._is_compact():
                item_offsets.append(_u32(self.data, item_offset))
            else:
                item_offsets.append(_u64(self.data, item_offset))
        if not item_offsets:
            self._error(f"ENTRY object at offset {span.offset} has no items")
        self.entry_objects[span.offset] = EntryObject(
            offset=span.offset,
            seqnum=_u64(self.data, span.offset + 16),
            realtime=_u64(self.data, span.offset + 24),
            monotonic=_u64(self.data, span.offset + 32),
            boot_id=bytes(self.data[span.offset + 40 : span.offset + 56]),
            item_offsets=tuple(item_offsets),
        )

    def _parse_entry_array(self, span: ObjectSpan) -> None:
        item_size = COMPACT_ENTRY_ARRAY_ITEM_SIZE if self._is_compact() else REGULAR_ENTRY_ARRAY_ITEM_SIZE
        if span.size < ENTRY_ARRAY_OBJECT_HEADER_SIZE + item_size:
            self._error(f"ENTRY_ARRAY object at offset {span.offset} is too small")
            return
        if (span.size - ENTRY_ARRAY_OBJECT_HEADER_SIZE) % item_size != 0:
            self._error(f"ENTRY_ARRAY object at offset {span.offset} has unaligned {item_size}-byte items")
            return
        items: list[int] = []
        for item_offset in range(span.offset + ENTRY_ARRAY_OBJECT_HEADER_SIZE, span.offset + span.size, item_size):
            items.append(_u32(self.data, item_offset) if self._is_compact() else _u64(self.data, item_offset))
        self.entry_arrays[span.offset] = EntryArrayObject(
            offset=span.offset,
            next_entry_array_offset=_u64(self.data, span.offset + 16),
            capacity=len(items),
            items=tuple(items),
        )

    def _validate_layout(self) -> None:
        self._validate_header_counts()
        self._validate_hash_table_header("field", OBJECT_TYPE_FIELD_HASH_TABLE)
        self._validate_hash_table_header("data", OBJECT_TYPE_DATA_HASH_TABLE)
        self._validate_object_order()
        self._validate_compression()
        self._validate_references()
        self._validate_hash_chains("data")
        self._validate_hash_chains("field")
        self._validate_entry_arrays()
        self._validate_tail_metadata()
        self._validate_compact_constraints()

    def _validate_header_counts(self) -> None:
        counts = Counter(span.typ for span in self.spans)
        expected_counts = {
            "n_objects": len(self.spans),
            "n_entries": counts[OBJECT_TYPE_ENTRY],
            "n_data": counts[OBJECT_TYPE_DATA],
            "n_fields": counts[OBJECT_TYPE_FIELD],
            "n_tags": counts[OBJECT_TYPE_TAG],
            "n_entry_arrays": counts[OBJECT_TYPE_ENTRY_ARRAY],
        }
        for field, actual in expected_counts.items():
            if field in self.header and self.header[field] != actual:
                self._error(f"header {field} mismatch: got {self.header[field]}, walked {actual}")

    def _validate_hash_table_header(self, name: str, typ: int) -> None:
        table_offset = self.header[f"{name}_hash_table_offset"]
        table_size = self.header[f"{name}_hash_table_size"]
        if (table_offset == 0) != (table_size == 0):
            self._error(f"{name} hash table offset/size zero mismatch")
            return
        if table_offset == 0:
            return
        if table_size % HASH_ITEM_SIZE != 0:
            self._error(f"{name} hash table size {table_size} is not a multiple of {HASH_ITEM_SIZE}")
        if table_offset <= OBJECT_HEADER_SIZE:
            self._error(f"{name} hash table offset {table_offset} is before hash items")
            return
        object_offset = table_offset - OBJECT_HEADER_SIZE
        span = self._object_at(object_offset, typ, f"{name} hash table object")
        if not span:
            return
        if span.size != OBJECT_HEADER_SIZE + table_size:
            self._error(
                f"{name} hash table size mismatch: object size {span.size}, header table size {table_size}"
            )

    def _validate_object_order(self) -> None:
        if len(self.spans) < 2:
            self._error("journal has fewer than two objects")
            return
        if self.spans[0].typ != OBJECT_TYPE_FIELD_HASH_TABLE or self.spans[1].typ != OBJECT_TYPE_DATA_HASH_TABLE:
            names = [OBJECT_TYPES.get(span.typ, f"UNKNOWN_{span.typ}") for span in self.spans[:2]]
            self._error(f"first objects are {names}, want FIELD_HASH_TABLE then DATA_HASH_TABLE")
        if self.header["tail_object_offset"] != self.spans[-1].offset:
            self._error(
                f"tail_object_offset {self.header['tail_object_offset']} does not point to last walked object {self.spans[-1].offset}"
            )

    def _validate_compression(self) -> None:
        incompatible_flags = self.header["incompatible_flags"]
        header_compression = incompatible_flags & INCOMPATIBLE_COMPRESSION_MASK
        object_compression_counts = self._count_compressed_data_objects(incompatible_flags)
        if self.expected_compression is None:
            return
        if self.expected_compression == "none":
            self._validate_no_compression_expected(header_compression, object_compression_counts)
            return
        self._validate_expected_compression(header_compression, object_compression_counts)

    def _count_compressed_data_objects(self, incompatible_flags: int) -> Counter:
        object_compression_counts = Counter()
        for span in self.spans:
            if span.typ == OBJECT_TYPE_DATA:
                self._record_data_compression(span, object_compression_counts, incompatible_flags)
        return object_compression_counts

    def _record_data_compression(
        self,
        span: ObjectSpan,
        object_compression_counts: Counter,
        incompatible_flags: int,
    ) -> None:
        for name, flag in COMPRESSION_OBJECT_FLAGS.items():
            if not span.flags & flag:
                continue
            object_compression_counts[name] += 1
            if not incompatible_flags & COMPRESSION_HEADER_FLAGS[name]:
                self._error(f"{name} DATA object at offset {span.offset} without matching header flag")

    def _validate_no_compression_expected(self, header_compression: int, object_compression_counts: Counter) -> None:
        if header_compression != 0:
            self._error(f"compression header flags set for uncompressed expectation: 0x{header_compression:x}")
        if object_compression_counts:
            self._error(f"compressed DATA objects found for uncompressed expectation: {dict(object_compression_counts)}")

    def _validate_expected_compression(self, header_compression: int, object_compression_counts: Counter) -> None:
        expected_header = COMPRESSION_HEADER_FLAGS[self.expected_compression]
        expected_object = COMPRESSION_OBJECT_FLAGS[self.expected_compression]
        if header_compression != expected_header:
            self._error(
                f"compression header flag mismatch: got 0x{header_compression:x}, want 0x{expected_header:x}"
            )
        if object_compression_counts[self.expected_compression] == 0:
            self._error(f"no DATA object has expected {self.expected_compression} compression flag")
        for span in self.spans:
            if span.typ == OBJECT_TYPE_DATA and (span.flags & OBJECT_COMPRESSED_MASK) & ~expected_object:
                self._error(f"DATA object at offset {span.offset} has unexpected compression flags 0x{span.flags:x}")

    def _validate_references(self) -> None:
        for obj in self.data_objects.values():
            self._validate_data_references(obj)
        for obj in self.field_objects.values():
            self._validate_field_references(obj)
        for obj in self.entry_objects.values():
            self._validate_entry_references(obj)
        for array in self.entry_arrays.values():
            self._validate_entry_array_reference(array)

    def _validate_data_references(self, obj: DataObject) -> None:
        if (obj.entry_offset == 0) != (obj.n_entries == 0):
            self._error(f"DATA object at offset {obj.offset} has inconsistent entry_offset/n_entries")
        self._valid_offset(obj.next_hash_offset, OBJECT_TYPE_DATA, f"DATA {obj.offset} next_hash_offset")
        self._valid_offset(obj.next_field_offset, OBJECT_TYPE_DATA, f"DATA {obj.offset} next_field_offset")
        self._valid_offset(obj.entry_offset, OBJECT_TYPE_ENTRY, f"DATA {obj.offset} entry_offset")
        self._valid_offset(obj.entry_array_offset, OBJECT_TYPE_ENTRY_ARRAY, f"DATA {obj.offset} entry_array_offset")
        self._validate_data_entry_array_rules(obj)
        if self._is_compact():
            self._valid_offset(
                obj.compact_tail_entry_array_offset,
                OBJECT_TYPE_ENTRY_ARRAY,
                f"DATA {obj.offset} compact tail_entry_array_offset",
            )

    def _validate_data_entry_array_rules(self, obj: DataObject) -> None:
        if obj.n_entries <= 1 and obj.entry_array_offset != 0:
            self._error(f"DATA object at offset {obj.offset} has entry_array_offset with n_entries={obj.n_entries}")
        if obj.n_entries > 1 and obj.entry_array_offset == 0:
            self._error(f"DATA object at offset {obj.offset} has n_entries={obj.n_entries} without entry array")

    def _validate_field_references(self, obj: FieldObject) -> None:
        self._valid_offset(obj.next_hash_offset, OBJECT_TYPE_FIELD, f"FIELD {obj.offset} next_hash_offset")
        self._valid_offset(obj.head_data_offset, OBJECT_TYPE_DATA, f"FIELD {obj.offset} head_data_offset")

    def _validate_entry_references(self, obj: EntryObject) -> None:
        if obj.seqnum == 0:
            self._error(f"ENTRY object at offset {obj.offset} has zero seqnum")
        if obj.realtime == 0:
            self._error(f"ENTRY object at offset {obj.offset} has zero realtime")
        if obj.boot_id == b"\x00" * 16:
            self._error(f"ENTRY object at offset {obj.offset} has zero boot_id")
        for item_offset in obj.item_offsets:
            self._valid_offset(item_offset, OBJECT_TYPE_DATA, f"ENTRY {obj.offset} item offset")
        if list(obj.item_offsets) != sorted(obj.item_offsets):
            self._error(f"ENTRY object at offset {obj.offset} item offsets are not sorted")

    def _validate_entry_array_reference(self, array: EntryArrayObject) -> None:
        next_offset = array.next_entry_array_offset
        if next_offset == 0:
            return
        if next_offset <= array.offset:
            self._error(f"ENTRY_ARRAY at offset {array.offset} has non-increasing next offset {next_offset}")
        self._valid_offset(next_offset, OBJECT_TYPE_ENTRY_ARRAY, f"ENTRY_ARRAY {array.offset} next")

    def _validate_hash_chains(self, name: str) -> None:
        spec = self._hash_chain_spec(name)
        if spec.table_offset == 0 or spec.table_size == 0:
            return
        bucket_count = spec.table_size // HASH_ITEM_SIZE
        referenced: set[int] = set()
        max_depth = 0
        for bucket_index in range(bucket_count):
            depth = self._walk_hash_bucket(spec, bucket_index, bucket_count, referenced)
            max_depth = max(max_depth, depth)
        self._validate_hash_chain_summary(spec, referenced, max_depth)
        if name == "data":
            self.actual_data_hash_chain_depth = max_depth
        else:
            self.actual_field_hash_chain_depth = max_depth

    def _hash_chain_spec(self, name: str) -> HashChainSpec:
        if name == "data":
            return HashChainSpec(
                name=name,
                object_type=OBJECT_TYPE_DATA,
                objects=self.data_objects,
                table_offset=self.header["data_hash_table_offset"],
                table_size=self.header["data_hash_table_size"],
                header_depth_field="data_hash_chain_depth",
            )
        return HashChainSpec(
            name=name,
            object_type=OBJECT_TYPE_FIELD,
            objects=self.field_objects,
            table_offset=self.header["field_hash_table_offset"],
            table_size=self.header["field_hash_table_size"],
            header_depth_field="field_hash_chain_depth",
        )

    def _walk_hash_bucket(
        self,
        spec: HashChainSpec,
        bucket_index: int,
        bucket_count: int,
        referenced: set[int],
    ) -> int:
        head, tail = self._hash_bucket_head_tail(spec, bucket_index)
        if (head == 0) != (tail == 0):
            self._error(f"{spec.name} hash bucket {bucket_index} has mismatched head/tail")
            return 0
        depth, last = self._walk_hash_chain(spec, bucket_index, bucket_count, head, referenced)
        if last and last != tail:
            self._error(f"{spec.name} hash bucket {bucket_index} tail mismatch: got {tail}, walked {last}")
        return depth

    def _hash_bucket_head_tail(self, spec: HashChainSpec, bucket_index: int) -> tuple[int, int]:
        bucket_offset = spec.table_offset + bucket_index * HASH_ITEM_SIZE
        return _u64(self.data, bucket_offset), _u64(self.data, bucket_offset + 8)

    def _walk_hash_chain(
        self,
        spec: HashChainSpec,
        bucket_index: int,
        bucket_count: int,
        head: int,
        referenced: set[int],
    ) -> tuple[int, int]:
        depth = 0
        current = head
        seen: set[int] = set()
        last = 0
        while current:
            step = self._validate_hash_chain_step(spec, bucket_index, bucket_count, current, seen, referenced)
            if step is None:
                break
            last = current
            if step:
                depth += 1
            current = step
        return depth, last

    def _validate_hash_chain_step(
        self,
        spec: HashChainSpec,
        bucket_index: int,
        bucket_count: int,
        current: int,
        seen: set[int],
        referenced: set[int],
    ) -> int | None:
        if current in seen:
            self._error(f"{spec.name} hash bucket {bucket_index} has a cycle at {current}")
            return None
        seen.add(current)
        if not self._object_at(current, spec.object_type, f"{spec.name} hash bucket {bucket_index} object"):
            return None
        obj = spec.objects.get(current)
        if obj is None:
            self._error(f"{spec.name} hash bucket {bucket_index} references unparsable object at {current}")
            return None
        if obj.hash % bucket_count != bucket_index:
            self._error(
                f"{spec.name} hash bucket mismatch for object {current}: hash bucket {obj.hash % bucket_count}, table bucket {bucket_index}"
            )
        referenced.add(current)
        next_offset = obj.next_hash_offset
        if next_offset and next_offset <= current:
            self._error(f"{spec.name} hash chain at {current} points backwards to {next_offset}")
            return None
        return next_offset

    def _validate_hash_chain_summary(
        self,
        spec: HashChainSpec,
        referenced: set[int],
        max_depth: int,
    ) -> None:
        unreferenced = sorted(set(spec.objects) - referenced)
        if unreferenced:
            self._error(f"{spec.name} objects missing from hash table: {unreferenced[:8]}")
        header_depth = self.header.get(spec.header_depth_field, 0)
        if header_depth > max_depth:
            self._error(f"header {spec.header_depth_field} {header_depth} exceeds walked max depth {max_depth}")

    def _validate_entry_arrays(self) -> None:
        global_offsets = self._validate_entry_array_chain(
            self.header["entry_array_offset"],
            self.header["n_entries"],
            "global entry array",
            tail_offset=self.header.get("tail_entry_array_offset", 0),
            tail_n_entries=self.header.get("tail_entry_array_n_entries", 0),
        )
        global_entries = [offset for _array_offset, offset in global_offsets]
        if len(global_entries) != self.header["n_entries"]:
            self._error(f"global entry array item count mismatch: got {len(global_entries)}")
        if global_entries:
            if global_entries[0] not in self.entry_objects:
                self._error(f"global first entry offset {global_entries[0]} is not an ENTRY")
            if global_entries[-1] != self.header.get("tail_entry_offset", 0):
                self._error(
                    f"tail_entry_offset {self.header.get('tail_entry_offset', 0)} does not match global last entry {global_entries[-1]}"
                )

        for data in self.data_objects.values():
            if data.n_entries == 0:
                continue
            self._object_at(data.entry_offset, OBJECT_TYPE_ENTRY, f"DATA {data.offset} inline entry")
            if data.n_entries <= 1:
                continue
            self._validate_entry_array_chain(
                data.entry_array_offset,
                data.n_entries - 1,
                f"DATA {data.offset} entry array",
                tail_offset=data.compact_tail_entry_array_offset if self._is_compact() else None,
                tail_n_entries=data.compact_tail_entry_array_n_entries if self._is_compact() else None,
            )

        unreferenced = sorted(set(self.entry_arrays) - self.referenced_entry_arrays)
        if unreferenced:
            self._error(f"ENTRY_ARRAY objects not referenced by header or DATA objects: {unreferenced[:8]}")

    def _validate_entry_array_chain(
        self,
        start_offset: int,
        n_used: int,
        label: str,
        *,
        tail_offset: int | None = None,
        tail_n_entries: int | None = None,
    ) -> list[tuple[int, int]]:
        if not self._entry_array_start_is_valid(start_offset, n_used, label):
            return []
        state = EntryArrayWalkState([], n_used, start_offset, set())
        self._walk_entry_array_chain(label, state)
        self._validate_entry_array_tail(label, state, tail_offset, tail_n_entries)
        return state.used_items

    def _entry_array_start_is_valid(self, start_offset: int, n_used: int, label: str) -> bool:
        if n_used == 0:
            if start_offset != 0:
                self._error(f"{label} has start offset {start_offset} with zero used entries")
            return False
        if start_offset == 0:
            self._error(f"{label} has zero start offset with {n_used} used entries")
            return False
        return True

    def _walk_entry_array_chain(self, label: str, state: EntryArrayWalkState) -> None:
        while state.offset:
            array = self._entry_array_at_current(label, state)
            if array is None:
                break
            used_here = min(state.remaining, array.capacity)
            self._validate_entry_array_items(label, state.offset, array, used_here, state.used_items)
            state.remaining -= used_here
            state.last_array_offset = state.offset
            state.last_used = used_here
            if self._entry_array_chain_done(label, state, array):
                break
            state.offset = array.next_entry_array_offset

    def _entry_array_at_current(
        self,
        label: str,
        state: EntryArrayWalkState,
    ) -> EntryArrayObject | None:
        if state.offset in state.seen:
            self._error(f"{label} has cycle at ENTRY_ARRAY {state.offset}")
            return None
        state.seen.add(state.offset)
        self.referenced_entry_arrays.add(state.offset)
        array = self.entry_arrays.get(state.offset)
        if array is None:
            self._object_at(state.offset, OBJECT_TYPE_ENTRY_ARRAY, label)
        return array

    def _validate_entry_array_items(
        self,
        label: str,
        offset: int,
        array: EntryArrayObject,
        used_here: int,
        used_items: list[tuple[int, int]],
    ) -> None:
        for index, item in enumerate(array.items):
            if index < used_here:
                self._validate_used_entry_array_item(label, offset, index, item, used_items)
            elif item != 0:
                self._error(f"{label} ENTRY_ARRAY {offset} has non-zero unused item at index {index}")

    def _validate_used_entry_array_item(
        self,
        label: str,
        offset: int,
        index: int,
        item: int,
        used_items: list[tuple[int, int]],
    ) -> None:
        if item == 0:
            self._error(f"{label} ENTRY_ARRAY {offset} has zero item at used index {index}")
            return
        self._object_at(item, OBJECT_TYPE_ENTRY, f"{label} item")
        used_items.append((offset, item))

    def _entry_array_chain_done(
        self,
        label: str,
        state: EntryArrayWalkState,
        array: EntryArrayObject,
    ) -> bool:
        if state.remaining == 0:
            if array.next_entry_array_offset != 0:
                self._error(f"{label} has unused next ENTRY_ARRAY {array.next_entry_array_offset}")
            return True
        if array.next_entry_array_offset == 0:
            self._error(f"{label} ended before {state.remaining} entries were linked")
            return True
        return False

    def _validate_entry_array_tail(
        self,
        label: str,
        state: EntryArrayWalkState,
        tail_offset: int | None,
        tail_n_entries: int | None,
    ) -> None:
        if tail_offset is None or tail_n_entries is None:
            return
        if tail_offset != state.last_array_offset:
            self._error(f"{label} tail array mismatch: header {tail_offset}, walked {state.last_array_offset}")
        if tail_n_entries != state.last_used:
            self._error(f"{label} tail entry count mismatch: header {tail_n_entries}, walked {state.last_used}")

    def _validate_tail_metadata(self) -> None:
        entries = sorted(self.entry_objects.values(), key=lambda item: item.seqnum)
        if not entries:
            if self.header.get("n_entries", 0) != 0:
                self._error("header records entries but no ENTRY objects were parsed")
            return

        head = entries[0]
        tail = entries[-1]
        if self.header.get("head_entry_seqnum") != head.seqnum:
            self._error(f"head_entry_seqnum mismatch: got {self.header.get('head_entry_seqnum')}, walked {head.seqnum}")
        if self.header.get("tail_entry_seqnum") != tail.seqnum:
            self._error(f"tail_entry_seqnum mismatch: got {self.header.get('tail_entry_seqnum')}, walked {tail.seqnum}")
        if self.header.get("head_entry_realtime") != head.realtime:
            self._error(f"head_entry_realtime mismatch: got {self.header.get('head_entry_realtime')}, walked {head.realtime}")
        if self.header.get("tail_entry_realtime") != tail.realtime:
            self._error(f"tail_entry_realtime mismatch: got {self.header.get('tail_entry_realtime')}, walked {tail.realtime}")
        if self.header.get("tail_entry_monotonic") != tail.monotonic:
            self._error(f"tail_entry_monotonic mismatch: got {self.header.get('tail_entry_monotonic')}, walked {tail.monotonic}")
        if self.header.get("tail_entry_offset") != tail.offset:
            self._error(f"tail_entry_offset mismatch: got {self.header.get('tail_entry_offset')}, walked {tail.offset}")
        if self.header.get("compatible_flags", 0) & COMPATIBLE_TAIL_ENTRY_BOOT_ID:
            if self.header.get("tail_entry_boot_id") != tail.boot_id:
                self._error("tail_entry_boot_id does not match tail ENTRY boot_id")

    def _validate_compact_constraints(self) -> None:
        if not self._is_compact():
            return
        if len(self.data) > JOURNAL_COMPACT_SIZE_MAX:
            self._error(f"compact journal file exceeds 32-bit limit: {len(self.data)}")
        for span in self.spans:
            if span.offset > JOURNAL_COMPACT_SIZE_MAX or span.offset + span.size > JOURNAL_COMPACT_SIZE_MAX:
                self._error(f"compact object at offset {span.offset} exceeds 32-bit offset range")
        for entry in self.entry_objects.values():
            for item_offset in entry.item_offsets:
                if item_offset > JOURNAL_COMPACT_SIZE_MAX:
                    self._error(f"compact ENTRY item offset exceeds 32-bit range: {item_offset}")
        for array in self.entry_arrays.values():
            for item_offset in array.items:
                if item_offset > JOURNAL_COMPACT_SIZE_MAX:
                    self._error(f"compact ENTRY_ARRAY item offset exceeds 32-bit range: {item_offset}")

    def _valid_offset(self, offset: int, typ: int, label: str) -> bool:
        if offset == 0:
            return True
        tail = self.header["tail_object_offset"]
        header_size = self.header["header_size"]
        if offset % 8 != 0:
            self._error(f"{label} offset {offset} is not 8-byte aligned")
            return False
        if offset < header_size or offset > tail:
            self._error(f"{label} offset {offset} outside object range {header_size}..{tail}")
            return False
        return self._object_at(offset, typ, label) is not None

    def _object_at(self, offset: int, typ: int, label: str) -> ObjectSpan | None:
        if offset == 0:
            return None
        span = self.by_offset.get(offset)
        if span is None:
            self._error(f"{label} points to missing object at offset {offset}")
            return None
        if span.typ != typ:
            self._error(
                f"{label} points to {OBJECT_TYPES.get(span.typ, f'UNKNOWN_{span.typ}')} at offset {offset}, want {OBJECT_TYPES[typ]}"
            )
            return None
        return span

    def _is_compact(self) -> bool:
        return bool(self.header.get("incompatible_flags", 0) & INCOMPATIBLE_COMPACT)

    def _error(self, message: str) -> None:
        self.errors.append(message)


def align8(value: int) -> int:
    return (value + 7) & ~7


def _u32(data: bytes, offset: int) -> int:
    return int.from_bytes(data[offset : offset + 4], "little")


def _u64(data: bytes, offset: int) -> int:
    return int.from_bytes(data[offset : offset + 8], "little")


def _flag_count(value: int) -> int:
    return value.bit_count()
