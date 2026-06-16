# Raw object-graph verification for systemd journal files.

from collections import Counter

from .compress import (
    MAX_UNCOMPRESSED_SIZE,
    decompress_lz4_sync,
    decompress_xz_sync,
    decompress_zst_sync,
)
from .hash import jenkins_hash_64, sip_hash_24
from .header import (
    COMPATIBLE_SEALED,
    COMPATIBLE_SEALED_CONTINUOUS,
    COMPATIBLE_TAIL_ENTRY_BOOT_ID,
    COMPACT_DATA_OBJECT_HEADER_SIZE,
    COMPACT_ENTRY_ITEM_SIZE,
    COMPACT_OFFSET_ARRAY_ITEM_SIZE,
    DATA_OBJECT_HEADER_SIZE,
    ENTRY_OBJECT_HEADER_SIZE,
    FIELD_OBJECT_HEADER_SIZE,
    HASH_ITEM_SIZE,
    HEADER_MIN_SIZE,
    INCOMPATIBLE_COMPACT,
    INCOMPATIBLE_COMPRESSED_LZ4,
    INCOMPATIBLE_COMPRESSED_XZ,
    INCOMPATIBLE_COMPRESSED_ZSTD,
    INCOMPATIBLE_KEYED_HASH,
    JOURNAL_COMPACT_SIZE_MAX,
    OBJECT_COMPRESSED_LZ4,
    OBJECT_COMPRESSED_XZ,
    OBJECT_COMPRESSED_ZSTD,
    OBJECT_HEADER_SIZE,
    OBJECT_TYPE_DATA,
    OBJECT_TYPE_DATA_HASH_TABLE,
    OBJECT_TYPE_ENTRY,
    OBJECT_TYPE_ENTRY_ARRAY,
    OBJECT_TYPE_FIELD,
    OBJECT_TYPE_FIELD_HASH_TABLE,
    OFFSET_ARRAY_OBJECT_HEADER_SIZE,
    REGULAR_ENTRY_ITEM_SIZE,
    REGULAR_OFFSET_ARRAY_ITEM_SIZE,
    parse_file_header,
)
from .seal import OBJECT_TYPE_TAG, TAG_LENGTH


OBJECT_TYPES = {
    OBJECT_TYPE_DATA: "DATA",
    OBJECT_TYPE_FIELD: "FIELD",
    OBJECT_TYPE_ENTRY: "ENTRY",
    OBJECT_TYPE_DATA_HASH_TABLE: "DATA_HASH_TABLE",
    OBJECT_TYPE_FIELD_HASH_TABLE: "FIELD_HASH_TABLE",
    OBJECT_TYPE_ENTRY_ARRAY: "ENTRY_ARRAY",
    OBJECT_TYPE_TAG: "TAG",
}

OBJECT_COMPRESSED_MASK = (
    OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD
)
INCOMPATIBLE_COMPRESSION_MASK = (
    INCOMPATIBLE_COMPRESSED_XZ
    | INCOMPATIBLE_COMPRESSED_LZ4
    | INCOMPATIBLE_COMPRESSED_ZSTD
)
COMPATIBLE_SUPPORTED_MASK = (
    COMPATIBLE_SEALED | COMPATIBLE_TAIL_ENTRY_BOOT_ID | COMPATIBLE_SEALED_CONTINUOUS
)
TAG_OBJECT_SIZE = OBJECT_HEADER_SIZE + 8 + 8 + TAG_LENGTH


class ObjectGraphVerificationError(Exception):
    pass


class _WalkState:
    def __init__(self):
        self.entry_seqnum = 0
        self.entry_seqnum_set = False
        self.entry_monotonic = 0
        self.entry_monotonic_set = False
        self.entry_boot_id = b"\x00" * 16
        self.entry_realtime = 0
        self.entry_realtime_set = False
        self.last_tag_realtime = 0


def verify_object_graph(data):
    """Verify raw object graph invariants that normal readers may tolerate."""
    _GraphVerifier(data).verify()


class _GraphVerifier:
    def __init__(self, data):
        self.data = data
        self.header = None
        self.compact = False
        self.spans = {}
        self.order = []
        self.data_objects = {}
        self.field_objects = {}
        self.entry_objects = {}
        self.entry_arrays = {}
        self.counts = Counter()
        self.main_entry_array_found = False

    def verify(self):
        self._read_header()
        self._walk_objects()
        self._validate_header_counts()
        self._validate_main_entry_array_presence()
        self._validate_tail_metadata()
        self._validate_global_entry_array()
        self._validate_data_hash_table()

    def _read_header(self):
        if len(self.data) < HEADER_MIN_SIZE:
            raise ObjectGraphVerificationError("file too small")
        self.header = parse_file_header(self.data)
        self.compact = bool(self.header["incompatible_flags"] & INCOMPATIBLE_COMPACT)

        header_size = self.header["header_size"]
        arena_size = self.header["arena_size"]
        if header_size < HEADER_MIN_SIZE:
            raise ObjectGraphVerificationError(
                f"invalid header_size {header_size}: smaller than {HEADER_MIN_SIZE}"
            )
        if header_size > len(self.data):
            raise ObjectGraphVerificationError(
                f"invalid header_size {header_size}: exceeds file size {len(self.data)}"
            )
        if header_size % 8 != 0:
            raise ObjectGraphVerificationError(f"header_size {header_size} is not aligned")
        if header_size + arena_size > len(self.data):
            raise ObjectGraphVerificationError(
                f"header_size + arena_size exceeds file size: {header_size} + {arena_size}"
            )
        if self.header["state"] not in (0, 1, 2):
            raise ObjectGraphVerificationError(f"invalid journal state {self.header['state']}")
        if self.header["compatible_flags"] & ~COMPATIBLE_SUPPORTED_MASK:
            raise ObjectGraphVerificationError(
                f"unsupported compatible flags 0x{self.header['compatible_flags']:x}"
            )
        if any(self.data[17:24]):
            raise ObjectGraphVerificationError("reserved header bytes are non-zero")
        if self.compact and len(self.data) > JOURNAL_COMPACT_SIZE_MAX:
            raise ObjectGraphVerificationError("compact journal exceeds 32-bit size limit")

    def _walk_objects(self):
        tail = self.header["tail_object_offset"]
        if tail == 0:
            self._validate_empty_tail()
            return

        self._validate_tail_start(tail)
        state = _WalkState()
        offset = self.header["header_size"]

        while True:
            obj = self._read_walk_object(offset, tail)
            self._record_walk_object(obj)
            self._dispatch_walk_object(obj, state)

            if offset == tail:
                break
            offset = obj["end"]

        self._validate_walk_tail(state, tail)

    def _validate_empty_tail(self):
        if self.header["n_objects"] != 0:
            raise ObjectGraphVerificationError("tail_object_offset is zero with objects recorded")

    def _validate_tail_start(self, tail):
        if tail < self.header["header_size"]:
            raise ObjectGraphVerificationError("tail_object_offset is before header_size")

    def _read_walk_object(self, offset, tail):
        if offset > tail:
            raise ObjectGraphVerificationError(
                f"object walk skipped past tail_object_offset {tail}"
            )
        if offset + OBJECT_HEADER_SIZE > len(self.data):
            raise ObjectGraphVerificationError(
                f"object header at offset {offset} exceeds file bounds"
            )

        typ = self.data[offset]
        flags = self.data[offset + 1]
        size = _u64(self.data, offset + 8)
        aligned_size = _align8(size)
        end = offset + aligned_size

        self._validate_walk_object_header(offset, typ, size, aligned_size, end)
        return {
            "offset": offset,
            "type": typ,
            "flags": flags,
            "size": size,
            "end": end,
        }

    def _validate_walk_object_header(self, offset, typ, size, aligned_size, end):
        if typ == 0 and size == 0:
            raise ObjectGraphVerificationError(f"zero object before tail at offset {offset}")
        if typ not in OBJECT_TYPES:
            raise ObjectGraphVerificationError(f"unknown object type {typ} at offset {offset}")
        if size < OBJECT_HEADER_SIZE:
            raise ObjectGraphVerificationError(
                f"object size {size} too small at offset {offset}"
            )
        if aligned_size < size or aligned_size == 0 or end > len(self.data):
            raise ObjectGraphVerificationError(
                f"object at offset {offset} exceeds file bounds"
            )
        if offset % 8 != 0:
            raise ObjectGraphVerificationError(f"object offset {offset} is not aligned")

    def _record_walk_object(self, obj):
        offset = obj["offset"]
        typ = obj["type"]
        flags = obj["flags"]
        self.spans[offset] = (typ, flags, obj["size"], obj["end"])
        self.order.append(offset)
        self.counts[typ] += 1
        self._validate_object_flags(offset, typ, flags)

    def _validate_object_flags(self, offset, typ, flags):
        if flags & ~OBJECT_COMPRESSED_MASK:
            raise ObjectGraphVerificationError(
                f"object at offset {offset} has unknown flags 0x{flags:x}"
            )
        if _flag_count(flags & OBJECT_COMPRESSED_MASK) > 1:
            raise ObjectGraphVerificationError(
                f"object at offset {offset} has multiple compression flags"
            )
        if typ != OBJECT_TYPE_DATA and flags:
            raise ObjectGraphVerificationError(
                f"{OBJECT_TYPES[typ]} object at offset {offset} has compression flags"
            )
        self._validate_compression_header_flags(offset, flags)

    def _validate_compression_header_flags(self, offset, flags):
        incompatible_flags = self.header["incompatible_flags"]
        if flags & OBJECT_COMPRESSED_XZ and not (incompatible_flags & INCOMPATIBLE_COMPRESSED_XZ):
            raise ObjectGraphVerificationError(
                f"XZ DATA object without matching header flag at offset {offset}"
            )
        if flags & OBJECT_COMPRESSED_LZ4 and not (incompatible_flags & INCOMPATIBLE_COMPRESSED_LZ4):
            raise ObjectGraphVerificationError(
                f"LZ4 DATA object without matching header flag at offset {offset}"
            )
        if flags & OBJECT_COMPRESSED_ZSTD and not (incompatible_flags & INCOMPATIBLE_COMPRESSED_ZSTD):
            raise ObjectGraphVerificationError(
                f"ZSTD DATA object without matching header flag at offset {offset}"
            )

    def _dispatch_walk_object(self, obj, state):
        offset = obj["offset"]
        typ = obj["type"]
        size = obj["size"]
        if typ == OBJECT_TYPE_DATA:
            self._parse_data(offset, obj["flags"], size)
        elif typ == OBJECT_TYPE_FIELD:
            self._parse_field(offset, size)
        elif typ == OBJECT_TYPE_ENTRY:
            self._handle_entry_walk_object(offset, size, state)
        elif typ in (OBJECT_TYPE_DATA_HASH_TABLE, OBJECT_TYPE_FIELD_HASH_TABLE):
            self._parse_hash_table(offset, typ, size)
        elif typ == OBJECT_TYPE_ENTRY_ARRAY:
            self._handle_entry_array_walk_object(offset, size)
        elif typ == OBJECT_TYPE_TAG:
            self._handle_tag_walk_object(offset, size, state)

    def _handle_entry_walk_object(self, offset, size, state):
        seqnum, realtime, monotonic, boot_id = self._parse_entry(offset, size)
        entry = {
            "seqnum": seqnum,
            "realtime": realtime,
            "monotonic": monotonic,
            "boot_id": boot_id,
        }
        self._validate_entry_walk_order(offset, entry, state)
        state.entry_seqnum = seqnum
        state.entry_seqnum_set = True
        state.entry_monotonic = monotonic
        state.entry_boot_id = boot_id
        state.entry_monotonic_set = True
        state.entry_realtime = realtime
        state.entry_realtime_set = True

    def _validate_entry_walk_order(self, offset, entry, state):
        self._validate_entry_tag_order(offset, entry["realtime"], state)
        self._validate_entry_seqnum_order(offset, entry["seqnum"], state)
        self._validate_entry_monotonic_order(offset, entry, state)
        self._validate_entry_realtime_head(offset, entry["realtime"], state)

    def _validate_entry_tag_order(self, offset, realtime, state):
        if self.header["compatible_flags"] & COMPATIBLE_SEALED and self.counts[OBJECT_TYPE_TAG] <= 0:
            raise ObjectGraphVerificationError(f"first entry before first tag at offset {offset}")
        if realtime >= state.last_tag_realtime:
            return
        raise ObjectGraphVerificationError(f"older entry after newer tag at offset {offset}")

    def _validate_entry_seqnum_order(self, offset, seqnum, state):
        if not state.entry_seqnum_set and seqnum != self.header["head_entry_seqnum"]:
            raise ObjectGraphVerificationError(f"head entry seqnum mismatch at offset {offset}")
        if state.entry_seqnum_set and state.entry_seqnum >= seqnum:
            raise ObjectGraphVerificationError(f"entry seqnum out of sync at offset {offset}")

    def _validate_entry_monotonic_order(self, offset, entry, state):
        if (
            state.entry_monotonic_set
            and state.entry_boot_id == entry["boot_id"]
            and state.entry_monotonic > entry["monotonic"]
        ):
            raise ObjectGraphVerificationError(f"entry monotonic out of sync at offset {offset}")

    def _validate_entry_realtime_head(self, offset, realtime, state):
        if not state.entry_realtime_set and realtime != self.header["head_entry_realtime"]:
            raise ObjectGraphVerificationError(f"head entry realtime mismatch at offset {offset}")

    def _handle_entry_array_walk_object(self, offset, size):
        self._parse_entry_array(offset, size)
        if offset != self.header["entry_array_offset"]:
            return
        if self.main_entry_array_found:
            raise ObjectGraphVerificationError("more than one main entry array")
        self.main_entry_array_found = True

    def _handle_tag_walk_object(self, offset, size, state):
        if not (self.header["compatible_flags"] & COMPATIBLE_SEALED):
            raise ObjectGraphVerificationError("TAG object in unsealed file")
        if size != TAG_OBJECT_SIZE:
            raise ObjectGraphVerificationError(f"invalid TAG size at offset {offset}")
        seqnum = _u64(self.data, offset + 16)
        if seqnum != self.counts[OBJECT_TYPE_TAG]:
            raise ObjectGraphVerificationError(f"TAG seqnum mismatch at offset {offset}")
        if state.entry_realtime_set:
            state.last_tag_realtime = state.entry_realtime

    def _validate_walk_tail(self, state, tail):
        if self.order[-1] != tail:
            raise ObjectGraphVerificationError("tail_object_offset does not point to walked tail")
        if state.entry_seqnum_set and state.entry_seqnum != self.header["tail_entry_seqnum"]:
            raise ObjectGraphVerificationError("tail_entry_seqnum mismatch")
        if (
            state.entry_monotonic_set
            and self.header["compatible_flags"] & COMPATIBLE_TAIL_ENTRY_BOOT_ID
            and state.entry_boot_id == self.header["tail_entry_boot_id"]
            and state.entry_monotonic != self.header["tail_entry_monotonic"]
        ):
            raise ObjectGraphVerificationError("tail_entry_monotonic mismatch")
        if state.entry_realtime_set and state.entry_realtime != self.header["tail_entry_realtime"]:
            raise ObjectGraphVerificationError("tail_entry_realtime mismatch")

    def _parse_data(self, offset, flags, size):
        payload_offset = COMPACT_DATA_OBJECT_HEADER_SIZE if self.compact else DATA_OBJECT_HEADER_SIZE
        if size <= payload_offset:
            raise ObjectGraphVerificationError(f"DATA object at offset {offset} has no payload")
        payload = self.data[offset + payload_offset:offset + size]
        stored_hash = _u64(self.data, offset + 16)
        self._validate_data_hash(offset, flags, payload, stored_hash)

        entry_offset, n_entries = self._data_entry_head(offset)
        obj = {
            "hash": stored_hash,
            "next_hash_offset": _u64(self.data, offset + 24),
            "next_field_offset": _u64(self.data, offset + 32),
            "entry_offset": entry_offset,
            "entry_array_offset": _u64(self.data, offset + 48),
            "n_entries": n_entries,
            "tail_entry_array_offset": _u32(self.data, offset + 64) if self.compact else 0,
            "tail_entry_array_n_entries": _u32(self.data, offset + 68) if self.compact else 0,
        }
        self._validate_data_object_links(offset, obj)
        self.data_objects[offset] = obj

    def _validate_data_hash(self, offset, flags, payload, stored_hash):
        hash_payload = self._decompress_payload(flags, payload, offset) if flags else payload
        computed_hash = self._hash(hash_payload)
        if stored_hash != computed_hash:
            raise ObjectGraphVerificationError(
                f"DATA hash mismatch at offset {offset}: {stored_hash:#x} != {computed_hash:#x}"
            )

    def _data_entry_head(self, offset):
        entry_offset = _u64(self.data, offset + 40)
        n_entries = _u64(self.data, offset + 56)
        if (entry_offset == 0) != (n_entries == 0):
            raise ObjectGraphVerificationError(f"DATA object at offset {offset} has bad n_entries")
        return entry_offset, n_entries

    def _validate_data_object_links(self, offset, obj):
        for field in ("next_hash_offset", "next_field_offset", "entry_offset", "entry_array_offset"):
            self._valid_offset(obj[field], f"DATA {offset} {field}")
        if obj["n_entries"] < 2 and obj["entry_array_offset"] != 0:
            raise ObjectGraphVerificationError(f"DATA object at offset {offset} has unexpected entry array")
        if obj["n_entries"] >= 2 and obj["entry_array_offset"] == 0:
            raise ObjectGraphVerificationError(f"DATA object at offset {offset} is missing entry array")

    def _parse_field(self, offset, size):
        if size <= FIELD_OBJECT_HEADER_SIZE:
            raise ObjectGraphVerificationError(f"FIELD object at offset {offset} has no payload")
        payload = self.data[offset + FIELD_OBJECT_HEADER_SIZE:offset + size]
        stored_hash = _u64(self.data, offset + 16)
        computed_hash = self._hash(payload)
        if stored_hash != computed_hash:
            raise ObjectGraphVerificationError(
                f"FIELD hash mismatch at offset {offset}: {stored_hash:#x} != {computed_hash:#x}"
            )
        obj = {
            "hash": stored_hash,
            "next_hash_offset": _u64(self.data, offset + 24),
            "head_data_offset": _u64(self.data, offset + 32),
        }
        self._valid_offset(obj["next_hash_offset"], f"FIELD {offset} next_hash_offset")
        self._valid_offset(obj["head_data_offset"], f"FIELD {offset} head_data_offset")
        self.field_objects[offset] = obj

    def _parse_entry(self, offset, size):
        item_size = COMPACT_ENTRY_ITEM_SIZE if self.compact else REGULAR_ENTRY_ITEM_SIZE
        if size < ENTRY_OBJECT_HEADER_SIZE:
            raise ObjectGraphVerificationError(f"ENTRY object at offset {offset} is too small")
        if (size - ENTRY_OBJECT_HEADER_SIZE) % item_size != 0:
            raise ObjectGraphVerificationError(f"ENTRY object at offset {offset} has unaligned items")
        item_offsets = []
        for item_offset in range(offset + ENTRY_OBJECT_HEADER_SIZE, offset + size, item_size):
            item = _u32(self.data, item_offset) if self.compact else _u64(self.data, item_offset)
            if item == 0:
                raise ObjectGraphVerificationError(f"ENTRY object at offset {offset} has zero item")
            self._valid_offset(item, f"ENTRY {offset} item")
            item_offsets.append(item)
        if not item_offsets:
            raise ObjectGraphVerificationError(f"ENTRY object at offset {offset} has no items")
        seqnum = _u64(self.data, offset + 16)
        realtime = _u64(self.data, offset + 24)
        monotonic = _u64(self.data, offset + 32)
        boot_id = bytes(self.data[offset + 40:offset + 56])
        if seqnum == 0:
            raise ObjectGraphVerificationError(f"ENTRY object at offset {offset} has zero seqnum")
        if realtime == 0:
            raise ObjectGraphVerificationError(f"ENTRY object at offset {offset} has zero realtime")
        self.entry_objects[offset] = {
            "seqnum": seqnum,
            "realtime": realtime,
            "monotonic": monotonic,
            "boot_id": boot_id,
            "items": tuple(item_offsets),
        }
        return seqnum, realtime, monotonic, boot_id

    def _parse_hash_table(self, offset, typ, size):
        if size < OBJECT_HEADER_SIZE + HASH_ITEM_SIZE:
            raise ObjectGraphVerificationError(f"{OBJECT_TYPES[typ]} at offset {offset} is too small")
        if (size - OBJECT_HEADER_SIZE) % HASH_ITEM_SIZE != 0:
            raise ObjectGraphVerificationError(f"{OBJECT_TYPES[typ]} at offset {offset} has unaligned items")
        if typ == OBJECT_TYPE_DATA_HASH_TABLE:
            table_offset = self.header["data_hash_table_offset"]
            table_size = self.header["data_hash_table_size"]
        else:
            table_offset = self.header["field_hash_table_offset"]
            table_size = self.header["field_hash_table_size"]
        if table_offset != offset + OBJECT_HEADER_SIZE:
            raise ObjectGraphVerificationError(f"{OBJECT_TYPES[typ]} header offset mismatch")
        if table_size != size - OBJECT_HEADER_SIZE:
            raise ObjectGraphVerificationError(f"{OBJECT_TYPES[typ]} header size mismatch")
        for item_offset in range(offset + OBJECT_HEADER_SIZE, offset + size, HASH_ITEM_SIZE):
            head = _u64(self.data, item_offset)
            tail = _u64(self.data, item_offset + 8)
            if (head == 0) != (tail == 0):
                raise ObjectGraphVerificationError(f"{OBJECT_TYPES[typ]} bucket head/tail mismatch")
            self._valid_offset(head, f"{OBJECT_TYPES[typ]} bucket head")
            self._valid_offset(tail, f"{OBJECT_TYPES[typ]} bucket tail")

    def _parse_entry_array(self, offset, size):
        item_size = COMPACT_OFFSET_ARRAY_ITEM_SIZE if self.compact else REGULAR_OFFSET_ARRAY_ITEM_SIZE
        if size < OFFSET_ARRAY_OBJECT_HEADER_SIZE + item_size:
            raise ObjectGraphVerificationError(f"ENTRY_ARRAY object at offset {offset} is too small")
        if (size - OFFSET_ARRAY_OBJECT_HEADER_SIZE) % item_size != 0:
            raise ObjectGraphVerificationError(f"ENTRY_ARRAY object at offset {offset} has unaligned items")
        items = []
        for item_offset in range(offset + OFFSET_ARRAY_OBJECT_HEADER_SIZE, offset + size, item_size):
            item = _u32(self.data, item_offset) if self.compact else _u64(self.data, item_offset)
            if item:
                self._valid_offset(item, f"ENTRY_ARRAY {offset} item")
            items.append(item)
        next_offset = _u64(self.data, offset + 16)
        self._valid_offset(next_offset, f"ENTRY_ARRAY {offset} next")
        self.entry_arrays[offset] = {"next": next_offset, "items": tuple(items)}

    def _validate_header_counts(self):
        expected = {
            "n_objects": len(self.order),
            "n_entries": self.counts[OBJECT_TYPE_ENTRY],
            "n_data": self.counts[OBJECT_TYPE_DATA],
            "n_fields": self.counts[OBJECT_TYPE_FIELD],
            "n_tags": self.counts[OBJECT_TYPE_TAG],
            "n_entry_arrays": self.counts[OBJECT_TYPE_ENTRY_ARRAY],
        }
        field_ends = {
            "n_objects": 152,
            "n_entries": 160,
            "n_data": 216,
            "n_fields": 224,
            "n_tags": 232,
            "n_entry_arrays": 240,
        }
        for field, value in expected.items():
            if self._header_has(field_ends[field]) and self.header[field] != value:
                raise ObjectGraphVerificationError(
                    f"header {field} mismatch: got {self.header[field]}, walked {value}"
                )

    def _validate_main_entry_array_presence(self):
        if self.header["entry_array_offset"] and not self.main_entry_array_found:
            raise ObjectGraphVerificationError("missing main entry array")
        if self.header["n_entries"] and not self.header["entry_array_offset"]:
            raise ObjectGraphVerificationError("entry_array_offset is zero with entries recorded")

    def _validate_tail_metadata(self):
        if not self.entry_objects:
            if self.header["n_entries"] != 0:
                raise ObjectGraphVerificationError("entries recorded but no ENTRY objects found")
            return

        entries = sorted(self.entry_objects.items(), key=lambda item: item[1]["seqnum"])
        head_offset, head = entries[0]
        tail_offset, tail = entries[-1]
        self._validate_head_tail_entry_metadata(head, tail)
        self._validate_tail_boot_metadata(tail)
        self._validate_tail_entry_offset(head_offset, tail_offset)

    def _validate_head_tail_entry_metadata(self, head, tail):
        if self.header["head_entry_seqnum"] != head["seqnum"]:
            raise ObjectGraphVerificationError("head_entry_seqnum mismatch")
        if self.header["tail_entry_seqnum"] != tail["seqnum"]:
            raise ObjectGraphVerificationError("tail_entry_seqnum mismatch")
        if self.header["head_entry_realtime"] != head["realtime"]:
            raise ObjectGraphVerificationError("head_entry_realtime mismatch")
        if self.header["tail_entry_realtime"] != tail["realtime"]:
            raise ObjectGraphVerificationError("tail_entry_realtime mismatch")

    def _validate_tail_boot_metadata(self, tail):
        if not (self.header["compatible_flags"] & COMPATIBLE_TAIL_ENTRY_BOOT_ID):
            return
        if self.header["tail_entry_monotonic"] != tail["monotonic"]:
            raise ObjectGraphVerificationError("tail_entry_monotonic mismatch")
        if self.header["tail_entry_boot_id"] != tail["boot_id"]:
            raise ObjectGraphVerificationError("tail_entry_boot_id mismatch")

    def _validate_tail_entry_offset(self, head_offset, tail_offset):
        if self._header_has(272) and self.header["tail_entry_offset"] != tail_offset:
            raise ObjectGraphVerificationError("tail_entry_offset mismatch")
        if head_offset == 0:
            raise ObjectGraphVerificationError("head entry offset is zero")

    def _validate_global_entry_array(self):
        entries = self._walk_entry_array_chain(
            self.header["entry_array_offset"],
            self.header["n_entries"],
            "global entry array",
        )
        if len(entries) != self.header["n_entries"]:
            raise ObjectGraphVerificationError("global entry array count mismatch")
        last = 0
        for idx, entry_offset in enumerate(entries):
            if entry_offset <= last:
                raise ObjectGraphVerificationError("global entry array is not sorted")
            if entry_offset not in self.entry_objects:
                raise ObjectGraphVerificationError("global entry array references missing ENTRY")
            last = entry_offset
            self._validate_entry_data_links(entry_offset, last_entry=idx + 1 == len(entries))

    def _validate_data_hash_table(self):
        table_offset = self.header["data_hash_table_offset"]
        table_size = self.header["data_hash_table_size"]
        if table_offset == 0 or table_size == 0:
            return
        bucket_count = table_size // HASH_ITEM_SIZE
        for bucket_index in range(bucket_count):
            item_offset = table_offset + bucket_index * HASH_ITEM_SIZE
            head = _u64(self.data, item_offset)
            tail = _u64(self.data, item_offset + 8)
            current = head
            last = 0
            seen = set()
            while current:
                if current in seen:
                    raise ObjectGraphVerificationError("data hash chain cycle")
                seen.add(current)
                obj = self.data_objects.get(current)
                if obj is None:
                    raise ObjectGraphVerificationError("data hash chain references missing DATA")
                if obj["hash"] % bucket_count != bucket_index:
                    raise ObjectGraphVerificationError("data hash bucket mismatch")
                self._validate_data_entry_array(current, obj)
                next_offset = obj["next_hash_offset"]
                if next_offset and next_offset <= current:
                    raise ObjectGraphVerificationError("data hash chain points backwards")
                last = current
                current = next_offset
            if last != tail:
                raise ObjectGraphVerificationError("data hash bucket tail mismatch")

    def _validate_entry_data_links(self, entry_offset, *, last_entry):
        entry = self.entry_objects[entry_offset]
        for data_offset in entry["items"]:
            data = self.data_objects.get(data_offset)
            if data is None:
                raise ObjectGraphVerificationError("entry references missing DATA object")
            if not self._data_object_in_hash_table(data_offset, data["hash"]):
                raise ObjectGraphVerificationError("entry DATA object missing from hash table")
            if not self._data_references_entry(data, entry_offset) and not last_entry:
                raise ObjectGraphVerificationError("entry not referenced by linked DATA object")

    def _validate_data_entry_array(self, data_offset, data):
        n_entries = data["n_entries"]
        if n_entries == 0:
            return
        if data["entry_offset"] not in self.entry_objects:
            raise ObjectGraphVerificationError("DATA inline entry is missing")
        last = data["entry_offset"]
        if data["entry_array_offset"] and n_entries < 2:
            raise ObjectGraphVerificationError("DATA entry array present with fewer than two entries")
        entries = self._walk_entry_array_chain(
            data["entry_array_offset"],
            n_entries - 1,
            f"DATA {data_offset} entry array",
        )
        for entry_offset in entries:
            if entry_offset <= last:
                raise ObjectGraphVerificationError("DATA entry array is not sorted")
            last = entry_offset

    def _walk_entry_array_chain(self, start_offset, used_count, label):
        if used_count == 0:
            if start_offset != 0:
                raise ObjectGraphVerificationError(f"{label} has start offset with zero entries")
            return []
        if start_offset == 0:
            raise ObjectGraphVerificationError(f"{label} is missing")
        entries = []
        remaining = used_count
        current = start_offset
        seen = set()
        while remaining > 0:
            array = self._entry_array_chain_node(current, seen, label)
            next_offset = self._entry_array_next_offset(current, array, label)
            capacity = len(array["items"])
            used_here = min(remaining, capacity)
            entries.extend(self._entry_array_used_items(array, used_here, label))
            remaining -= used_here
            if remaining == 0:
                break
            if next_offset == 0:
                raise ObjectGraphVerificationError(f"{label} ended early")
            current = next_offset
        return entries

    def _entry_array_chain_node(self, current, seen, label):
        if current in seen:
            raise ObjectGraphVerificationError(f"{label} has a cycle")
        seen.add(current)
        array = self.entry_arrays.get(current)
        if array is None:
            raise ObjectGraphVerificationError(f"{label} references missing ENTRY_ARRAY")
        return array

    def _entry_array_next_offset(self, current, array, label):
        next_offset = array["next"]
        if next_offset and next_offset <= current:
            raise ObjectGraphVerificationError(f"{label} next pointer is not increasing")
        return next_offset

    def _entry_array_used_items(self, array, used_count, label):
        entries = []
        for idx in range(used_count):
            item = array["items"][idx]
            if item == 0:
                raise ObjectGraphVerificationError(f"{label} has zero used item")
            if item not in self.entry_objects:
                raise ObjectGraphVerificationError(f"{label} references missing ENTRY")
            entries.append(item)
        return entries

    def _data_object_in_hash_table(self, data_offset, data_hash):
        table_offset = self.header["data_hash_table_offset"]
        table_size = self.header["data_hash_table_size"]
        if table_offset == 0 or table_size == 0:
            return False
        bucket_count = table_size // HASH_ITEM_SIZE
        bucket = data_hash % bucket_count
        current = _u64(self.data, table_offset + bucket * HASH_ITEM_SIZE)
        seen = set()
        while current:
            if current in seen:
                raise ObjectGraphVerificationError("data hash chain cycle")
            seen.add(current)
            if current == data_offset:
                return True
            obj = self.data_objects.get(current)
            if obj is None:
                raise ObjectGraphVerificationError("data hash chain references missing DATA")
            current = obj["next_hash_offset"]
        return False

    def _data_references_entry(self, data, entry_offset):
        if data["entry_offset"] == entry_offset:
            return True
        for item in self._walk_entry_array_chain(
            data["entry_array_offset"],
            max(0, data["n_entries"] - 1),
            "DATA entry array lookup",
        ):
            if item == entry_offset:
                return True
        return False

    def _valid_offset(self, offset, label):
        if offset == 0:
            return
        if offset % 8 != 0:
            raise ObjectGraphVerificationError(f"{label} offset {offset} is not aligned")
        if offset < self.header["header_size"] or offset > self.header["tail_object_offset"]:
            raise ObjectGraphVerificationError(f"{label} offset {offset} outside object range")

    def _hash(self, payload):
        if self.header["incompatible_flags"] & INCOMPATIBLE_KEYED_HASH:
            return sip_hash_24(self.header["file_id"], payload)
        return jenkins_hash_64(payload)

    def _decompress_payload(self, flags, payload, offset):
        try:
            if flags & OBJECT_COMPRESSED_ZSTD:
                return decompress_zst_sync(payload, max_output_size=MAX_UNCOMPRESSED_SIZE)
            if flags & OBJECT_COMPRESSED_XZ:
                return decompress_xz_sync(payload, max_output_size=MAX_UNCOMPRESSED_SIZE)
            if flags & OBJECT_COMPRESSED_LZ4:
                return decompress_lz4_sync(payload)
        except Exception as err:
            raise ObjectGraphVerificationError(
                f"DATA decompression failed at offset {offset}: {err}"
            ) from err
        return payload

    def _header_has(self, end):
        return self.header["header_size"] >= end and len(self.data) >= end


def _align8(value):
    return (value + 7) & ~7


def _u32(data, offset):
    if offset + 4 > len(data):
        raise ObjectGraphVerificationError(f"uint32 read at {offset} exceeds file bounds")
    return int.from_bytes(data[offset:offset + 4], "little")


def _u64(data, offset):
    if offset + 8 > len(data):
        raise ObjectGraphVerificationError(f"uint64 read at {offset} exceeds file bounds")
    return int.from_bytes(data[offset:offset + 8], "little")


def _flag_count(value):
    return value.bit_count()
