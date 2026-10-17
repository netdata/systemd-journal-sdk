"""Independent, untimed filter expectations for uncompressed benchmark journals."""

from __future__ import annotations

import re
import struct
from pathlib import Path


MASK64 = (1 << 64) - 1
JOURNAL_SUFFIXES = (".journal", ".journal~", ".journal.zst", ".journal~.zst")


def journal_inputs(path: Path, surface: str) -> list[Path]:
    if path.is_file():
        return [path]
    if surface == "open-files":
        return sorted(p for p in path.rglob("*") if p.is_file() and p.name.endswith(JOURNAL_SUFFIXES))
    if surface != "directory":
        raise ValueError("file benchmarks require a journal file")
    paths = [p for p in path.iterdir() if p.is_file() and p.name.endswith(JOURNAL_SUFFIXES)]
    for child in path.iterdir():
        if child.is_dir() and re.fullmatch(r"[0-9a-fA-F]{32}|[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}", child.name):
            paths.extend(p for p in child.iterdir() if p.is_file() and p.name.endswith(JOURNAL_SUFFIXES))
    return sorted(paths)


def _rows(path: Path, payload_bits: dict[bytes, int]) -> list[tuple]:
    data = path.read_bytes()
    if data[:8] != b"LPKSHHRH" or len(data) < 208:
        raise ValueError(f"filter goldens require native uncompressed journals: {path}")
    compact = bool(struct.unpack_from("<I", data, 12)[0] & 16)
    width, item_width = (4, 4) if compact else (8, 16)
    integer = struct.Struct("<I" if compact else "<Q").unpack_from
    u64 = struct.Struct("<Q").unpack_from
    seqnum_id = data[72:88]
    remaining, array = u64(data, 152)[0], u64(data, 176)[0]
    arrays, payloads, rows = set(), {}, []
    while remaining:
        if not array or array in arrays or data[array] != 6:
            raise ValueError(f"invalid benchmark ENTRY_ARRAY chain: {path}")
        arrays.add(array)
        count = min(remaining, (u64(data, array + 8)[0] - 24) // width)
        if count <= 0:
            raise ValueError(f"empty required benchmark ENTRY_ARRAY: {path}")
        for slot in range(count):
            entry = integer(data, array + 24 + slot * width)[0]
            if not entry or data[entry] != 3:
                raise ValueError(f"invalid benchmark ENTRY: {path}")
            bits = 0
            for at in range(entry + 64, entry + u64(data, entry + 8)[0], item_width):
                target = integer(data, at)[0]
                bit = payloads.get(target)
                if bit is None:
                    if not target or data[target] != 1 or data[target + 1] != 0:
                        raise ValueError(f"filter goldens require uncompressed DATA: {path}")
                    payload = data[target + (72 if compact else 64):target + u64(data, target + 8)[0]]
                    bit = payload_bits.get(payload, 0)
                    payloads[target] = bit
                bits |= bit
            rows.append((u64(data, entry + 24)[0], bits, seqnum_id, u64(data, entry + 16)[0], data[entry + 40:entry + 56], u64(data, entry + 32)[0]))
        remaining -= count
        array = u64(data, array + 16)[0]
    return rows


def filter_goldens(paths: list[Path], cases: list[dict]) -> dict[str, tuple[int, int, int, int]]:
    payload_bits = {payload: 1 << index for index, payload in enumerate(sorted({match.encode() for case in cases for match in case["matches"]}))}
    rows = [row for path in paths for row in _rows(path, payload_bits)]
    if not rows:
        raise ValueError("filter benchmarks require a nonempty mixed-cardinality corpus")
    if len(paths) > 1:
        rows.sort(key=lambda row: row[0])
        if any(left[0] == right[0] for left, right in zip(rows, rows[1:])):
            raise ValueError("multi-file filter goldens require distinct realtime values; validate tied merge keys separately")
        seqnums, monotonic = {}, {}
        for _, _, seqnum_id, seqnum, boot_id, usec in rows:
            if seqnum <= seqnums.get(seqnum_id, -1) or usec < monotonic.get(boot_id, -1):
                raise ValueError("multi-file filter goldens require sequence/boot order consistent with realtime; validate other merge orders separately")
            seqnums[seqnum_id], monotonic[boot_id] = seqnum, usec
    profile_bit = payload_bits[b"PERF_PROFILE=mixed-cardinality-32-fields"]
    if any(not row[1] & profile_bit for row in rows):
        raise ValueError("--filter-suite requires the mixed-cardinality-32-fields corpus")
    goldens = {}
    for case in cases:
        groups = {}
        for match in case["matches"]:
            payload = match.encode()
            field = payload.split(b"=", 1)[0]
            groups[field] = groups.get(field, 0) | payload_bits[payload]
        selected = [row[0] for row in rows if all(row[1] & group for group in groups.values())]
        if case["direction"] == "backward":
            selected.reverse()
        realtime = case["filter_realtime"]
        if realtime is not None:
            selected = [time for time in selected if (time >= realtime if case["direction"] == "forward" else time <= realtime)]
        if case["filter_limit"]:
            selected = selected[:case["filter_limit"]]
        if case["scenario"] != "absent" and not selected:
            raise ValueError(f"filter scenario {case['scenario']} has no results; choose an applicable corpus/time anchor")
        checksum = 0
        for _ in range(case["filter_repetitions"]):
            for time in selected:
                checksum = (((checksum << 7) | (checksum >> 57)) & MASK64) ^ time
        key = "/".join(str(case[key]) for key in ("mode", "bounds", "direction", "scenario"))
        goldens[key] = (len(selected) * case["filter_repetitions"], 0, 0, checksum)
    return goldens


def auxiliary_goldens(path: Path, cases: list[dict]) -> dict[str, dict]:
    """Derive snapshot output from native rows, independently of SDK indexes."""
    matches = {case["snapshot_match"].encode() for case in cases if "snapshot_match" in case}
    if len(matches) != 1:
        raise ValueError("auxiliary goldens require one shared snapshot declaration")
    match = matches.pop()
    data = path.read_bytes()
    if data[:8] != b"LPKSHHRH" or len(data) < 208:
        raise ValueError("auxiliary goldens require a native journal")
    compact = bool(struct.unpack_from("<I", data, 12)[0] & 16)
    width, item_width = (4, 4) if compact else (8, 16)
    integer = struct.Struct("<I" if compact else "<Q").unpack_from
    u64 = struct.Struct("<Q").unpack_from
    population, array = u64(data, 152)[0], u64(data, 176)[0]
    remaining, seen = population, set()
    digest_modes = any(case["mode"].endswith("-digest") for case in cases)
    totals = {name: [0, 0, 0, 0] for name in ("all", "selected", "all-digest", "selected-digest")}

    def rotate(value: int, amount: int) -> int:
        return ((value << amount) | (value >> (64 - amount))) & MASK64

    def add(name: str, realtime: int, payloads: list[memoryview], digest: bool) -> None:
        counters = totals[name]
        counters[0] += 1
        counters[3] = rotate(counters[3], 7) ^ realtime
        for payload in payloads:
            counters[1] += 1
            counters[2] += len(payload)
            checksum = rotate(counters[3], 5) ^ len(payload)
            if digest:
                for byte in payload:
                    checksum = ((checksum ^ byte) * 1099511628211) & MASK64
            elif payload:
                checksum ^= payload[0] << 8
                checksum ^= payload[-1]
            counters[3] = checksum

    while remaining:
        if not array or array in seen or data[array] != 6:
            raise ValueError("invalid auxiliary ENTRY_ARRAY chain")
        seen.add(array)
        count = min(remaining, (u64(data, array + 8)[0] - 24) // width)
        if count <= 0:
            raise ValueError("empty required auxiliary ENTRY_ARRAY")
        for slot in range(count):
            entry = integer(data, array + 24 + slot * width)[0]
            if not entry or data[entry] != 3:
                raise ValueError("invalid auxiliary ENTRY")
            payloads = []
            for at in range(entry + 64, entry + u64(data, entry + 8)[0], item_width):
                target = integer(data, at)[0]
                if not target or data[target] != 1 or data[target + 1] != 0:
                    raise ValueError("auxiliary goldens require uncompressed DATA")
                payloads.append(memoryview(data)[target + (72 if compact else 64):target + u64(data, target + 8)[0]])
            realtime = u64(data, entry + 24)[0]
            selected = any(payload == match for payload in payloads)
            for name in ("all", "selected") if selected else ("all",):
                add(name, realtime, payloads, False)
                if digest_modes:
                    add(name + "-digest", realtime, payloads, True)
        remaining -= count
        array = u64(data, array + 16)[0]
    if totals["all"][0] != population or not totals["selected"][0]:
        raise ValueError("auxiliary corpus must contain the declared snapshot value")
    goldens = {}
    for case in cases:
        mode, repetitions = case["mode"], case.get("repetitions", 1)
        if mode == "indexed-open" or mode.startswith("verify-"):
            marker = population if mode == "indexed-open" else 1
            checksum = 0
            for _ in range(repetitions):
                checksum = rotate(checksum, 7) ^ marker
            counts = (repetitions, 0, 0, checksum)
        else:
            if repetitions != 1:
                raise ValueError("snapshot visitor goldens require one lifecycle per child")
            name = "all" if mode.startswith("indexed-all") else "selected"
            counts = tuple(totals[name + ("-digest" if mode.endswith("-digest") else "")])
        extra = {"completed_verifications": repetitions, "reader_options": "public SDK defaults"} if mode.startswith("verify-") else {
            "completed_opens": repetitions, "captured_entries": population,
            "captured_value_entries": totals["selected"][0], "captured_value_present": True,
        }
        goldens[mode] = dict(counts=counts, extra=extra)
    return goldens
