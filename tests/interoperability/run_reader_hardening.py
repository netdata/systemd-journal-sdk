#!/usr/bin/env python3
"""Shared public-SDK chain, complete-stream and crash-prefix checks.

All journals and subprocess artifacts stay in a new repository-local directory.
Potentially looping SDK operations run in bounded child processes.
"""

from __future__ import annotations

import argparse
import ctypes
import ctypes.util
import json
import os
import shlex
import subprocess  # nosec B404
from pathlib import Path

from go_fixture_writer import write_journal_file
from run_verify_matrix import oversized_zstd_frame, parse_journal, read_u64, write_u64

ROOT = Path(__file__).resolve().parents[2]
IDENTITY = "00112233445566778899aabbccddeeff"


def posting_prefix_known(
    data: bytes, at: int, last: int, extent: int, compact: bool
) -> bool:
    """Check whether the fixture exposes every posting up to the recovered tail."""
    previous = read_u64(data, at + 40)
    remaining = read_u64(data, at + 56)
    if not remaining or previous > last:
        return True
    remaining -= 1
    array = read_u64(data, at + 48)
    width = 4 if compact else 8
    seen = set()
    while remaining:
        if not array or array + 24 > extent:
            return previous == last
        if array in seen:
            raise RuntimeError("cyclic source fixture posting arrays")
        seen.add(array)
        wanted = min(remaining, (read_u64(data, array + 8) - 24) // width)
        available = min(wanted, (extent - array - 24) // width)
        for index in range(available):
            slot = array + 24 + index * width
            value = int.from_bytes(data[slot : slot + width], "little")
            if not value:
                return previous == last
            if value <= previous:
                raise RuntimeError("unordered source fixture postings")
            if value > last:
                return True
            previous = value
        if available < wanted:
            return previous == last
        remaining -= wanted
        array = read_u64(data, array + 16)
    return True


class Zstd:
    def __init__(self) -> None:
        library = ctypes.util.find_library("zstd")
        if library is None:
            raise RuntimeError("an existing libzstd is required for fixture encoding")
        self.lib = ctypes.CDLL(library)
        for name, args, result in (
            ("ZSTD_createCCtx", [], ctypes.c_void_p),
            ("ZSTD_freeCCtx", [ctypes.c_void_p], ctypes.c_size_t),
            (
                "ZSTD_CCtx_setParameter",
                [ctypes.c_void_p, ctypes.c_int, ctypes.c_int],
                ctypes.c_size_t,
            ),
            ("ZSTD_compressBound", [ctypes.c_size_t], ctypes.c_size_t),
            (
                "ZSTD_compress2",
                [
                    ctypes.c_void_p,
                    ctypes.c_void_p,
                    ctypes.c_size_t,
                    ctypes.c_void_p,
                    ctypes.c_size_t,
                ],
                ctypes.c_size_t,
            ),
            ("ZSTD_isError", [ctypes.c_size_t], ctypes.c_uint),
        ):
            function = getattr(self.lib, name)
            function.argtypes, function.restype = args, result

    def checked(self, value: int) -> int:
        if self.lib.ZSTD_isError(value):
            raise RuntimeError("libzstd fixture encoding failed")
        return value

    def frame(
        self, payload: bytes, *, known: bool = True, checksum: bool = False
    ) -> bytes:
        context = self.lib.ZSTD_createCCtx()
        if not context:
            raise RuntimeError("libzstd fixture context allocation failed")
        try:
            self.checked(self.lib.ZSTD_CCtx_setParameter(context, 200, int(known)))
            self.checked(self.lib.ZSTD_CCtx_setParameter(context, 201, int(checksum)))
            output = ctypes.create_string_buffer(
                self.lib.ZSTD_compressBound(len(payload))
            )
            size = self.checked(
                self.lib.ZSTD_compress2(
                    context, output, len(output), payload, len(payload)
                )
            )
            return output.raw[:size]
        finally:
            self.checked(self.lib.ZSTD_freeCCtx(context))


def skippable(length: int = 0) -> bytes:
    return b"\x50\x2a\x4d\x18" + length.to_bytes(4, "little") + bytes(length)


def expected_rows(entries: list[dict]) -> list[dict]:
    return [
        {
            "realtime": entry["realtime_usec"],
            "payloads": sorted(
                (
                    name.encode()
                    + b"="
                    + (value.encode() if isinstance(value, str) else value)
                ).hex()
                for name, value in entry["fields"]
            ),
        }
        for entry in entries
    ]


def fixture(
    path: Path, count: int, compact: bool, env: dict
) -> tuple[bytes, dict, list[dict]]:
    entries = [
        {
            "realtime_usec": 1000 + i * 1000,
            "monotonic_usec": 100 + i,
            "fields": [
                ("MESSAGE", b"a" * 1000 + b"\x00\xff\n" + str(i).encode()),
                ("COMMON", "all"),
                ("TAG", "first"),
                ("TAG", "second"),
            ]
            + ([("SPARSE", "some")] if i in (0, count // 2, count - 1) else []),
        }
        for i in range(count)
    ]
    write_journal_file(
        path,
        machine_id=IDENTITY,
        boot_id=IDENTITY,
        seqnum_id=IDENTITY,
        file_id=IDENTITY,
        entries=entries,
        compact=compact,
        env=env,
    )
    data = path.read_bytes()
    return data, parse_journal(data), expected_rows(entries)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--go-adapter", type=Path, required=True)
    parser.add_argument("--rust-adapter", type=Path, required=True)
    parser.add_argument("--journalctl", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument(
        "--only", choices=("chains", "streams", "recovery"), action="append"
    )
    args = parser.parse_args()
    out = args.out.resolve()
    if not out.is_relative_to(ROOT / ".local") or out.exists():
        parser.error("--out must be a new directory inside the repository's .local/")
    adapters = {
        name: path.resolve(strict=True)
        for name, path in (("go", args.go_adapter), ("rust", args.rust_adapter))
    }
    oracle = args.journalctl.resolve(strict=True)
    out.mkdir(parents=True)
    (out / "tmp").mkdir()
    env = os.environ.copy()
    env.update(TMPDIR=str(out / "tmp"), PYTHONDONTWRITEBYTECODE="1")
    selected = set(args.only or ("chains", "streams", "recovery"))
    observations = []

    def probe(
        path: Path,
        operation: str,
        *,
        expected: list | None = None,
        error: bool = False,
        recovery: bool = False,
        declared: int = 0,
        recovered_count: int | None = None,
        **options,
    ) -> None:
        request = dict(path=str(path), operation=operation, snapshot=True, **options)
        for language, binary in adapters.items():
            command = [str(binary), "inspect-reader"]
            print(f"+ {shlex.join(command)}: {path.name}/{operation}", flush=True)
            process = subprocess.run(
                command,
                input=json.dumps(request),
                text=True,
                capture_output=True,
                env=env,
                timeout=8,
                check=False,
            )  # nosec B603
            if process.returncode:
                raise RuntimeError(
                    f"{language}/{path.name}/{operation}: exit {process.returncode}: {process.stderr[-2000:]}"
                )
            result = json.loads(process.stdout)
            if result["error"] != error:
                evidence = out / f"unexpected-result-{language}-{len(observations)}.json"
                evidence.write_text(json.dumps(result, indent=2) + "\n")
                raise RuntimeError(
                    f"{language}/{path.name}/{operation}: expected error={error}, "
                    f"got {result['error']}; inspect {evidence}"
                )
            if expected is not None:
                actual = (
                    result["unique"]
                    if operation.removeprefix("facade-").startswith("unique")
                    else result["rows"]
                )
                if actual != expected:
                    raise RuntimeError(
                        f"{language}/{path.name}/{operation}: incomplete or changed content: {len(actual)} != {len(expected)}"
                    )
            if recovery:
                statuses = result["recovery_statuses"]
                if len(statuses) != 1 or statuses[0]["status"]["reason"] == 0:
                    raise RuntimeError(
                        f"{language}/{path.name}: missing recovery status"
                    )
                status = statuses[0]["status"]
                if status["declared_entries"] != declared or status[
                    "recovered_entries"
                ] != (
                    recovered_count if recovered_count is not None else len(expected)
                ):
                    raise RuntimeError(
                        f"{language}/{path.name}: incorrect recovery population: {status}"
                    )
            observations.append(
                {
                    "language": language,
                    "fixture": path.name,
                    "operation": operation,
                    "expected_error": error,
                    "rows": len(result["rows"]),
                    "recovery_statuses": result["recovery_statuses"],
                }
            )

    def stock_verify(path: Path, expected_error: bool = False) -> None:
        command = [str(oracle), "--verify", "--file", str(path)]
        print(f"+ {shlex.join(command)}", flush=True)
        result = subprocess.run(
            command, text=True, capture_output=True, env=env, timeout=15, check=False
        )  # nosec B603
        if (result.returncode != 0) != expected_error:
            raise RuntimeError(
                f"stock verification disagreement: {path.name}: {result.stderr[-2000:]}"
            )

    zstd = Zstd() if "streams" in selected else None
    for compact in (False, True):
        folder = out / ("compact" if compact else "regular")
        folder.mkdir()
        base = folder / "base.journal"
        data, parsed, rows = fixture(base, 4, compact, env)
        stock_verify(base)
        probe(base, "forward", expected=rows)
        payload_header = 72 if compact else 64
        messages = [
            offset
            for offset in parsed["data_offsets"]
            if data[offset + payload_header : offset + payload_header + 8]
            == b"MESSAGE="
        ]
        if "chains" in selected:
            field_size = read_u64(data, 88)
            tail = read_u64(data, 136)
            fields = []
            at = field_size
            while at <= tail:
                size = read_u64(data, at + 8)
                if data[at] == 2:
                    fields.append(at)
                at += (size + 7) & ~7
            message_field = next(
                at
                for at in fields
                if data[at + 40 : at + read_u64(data, at + 8)] == b"MESSAGE"
            )
            for name in (
                "field-self",
                "data-self",
                "field-data-multi",
                "data-hash-self",
                "array-self",
                "ascending",
            ):
                mutated = bytearray(data)
                operation, options, error = "unique", {"field": "MESSAGE"}, True
                if name == "field-self":
                    write_u64(mutated, message_field + 24, message_field)
                    operation, options = "fields", {}
                elif name == "data-self":
                    head = read_u64(data, message_field + 32)
                    write_u64(mutated, head + 32, head)
                elif name == "field-data-multi":
                    write_u64(mutated, messages[0] + 32, messages[-1])
                elif name == "data-hash-self":
                    first = parsed["data_offsets"][0]
                    write_u64(mutated, first + 24, first)
                    start, size = (
                        parsed["data_hash_table_offset"],
                        parsed["data_hash_table_size"],
                    )
                    mutated[start : start + size] = (
                        first.to_bytes(8, "little") * 2
                    ) * (size // 16)
                    operation, options = "explorer", {"field": "MESSAGE"}
                elif name == "array-self":
                    write_u64(
                        mutated,
                        parsed["entry_array_offset"] + 16,
                        parsed["entry_array_offset"],
                    )
                    write_u64(mutated, 152, 1_000_000)
                    operation, options = "forward", {}
                else:
                    write_u64(mutated, message_field + 32, messages[0])
                    for index, at in enumerate(messages):
                        write_u64(
                            mutated,
                            at + 32,
                            messages[index + 1] if index + 1 < len(messages) else 0,
                        )
                    error = False
                path = folder / f"{name}.journal"
                path.write_bytes(mutated)
                values = [
                    next(
                        bytes.fromhex(payload)
                        for payload in row["payloads"]
                        if bytes.fromhex(payload).startswith(b"MESSAGE=")
                    )
                    for row in rows
                ]
                probe(
                    path,
                    operation,
                    error=error,
                    expected=[value[8:].hex() for value in values]
                    if not error
                    else None,
                    **options,
                )
                if operation == "unique":
                    probe(
                        path,
                        "unique-state",
                        error=error,
                        expected=[value.hex() for value in values]
                        if not error
                        else None,
                        **options,
                    )
                if operation != "explorer":
                    probe(
                        path,
                        f"facade-{operation}",
                        error=error,
                        expected=[value[8:].hex() for value in values]
                        if not error
                        else None,
                        **options,
                    )
                if operation in ("fields", "unique"):
                    directory = folder / f"{name}-directory"
                    directory.mkdir()
                    (directory / "system.journal").write_bytes(mutated)
                    probe(
                        directory,
                        operation,
                        error=error,
                        directory=True,
                        expected=[value[8:].hex() for value in values]
                        if not error
                        else None,
                        **options,
                    )
                    if operation == "unique":
                        probe(
                            directory,
                            "unique-state",
                            error=error,
                            directory=True,
                            expected=[value.hex() for value in values]
                            if not error
                            else None,
                            **options,
                        )
                        probe(
                            path,
                            "facade-unique-state",
                            error=error,
                            expected=[value.hex() for value in values]
                            if not error
                            else None,
                            **options,
                        )
            common = next(
                at
                for at in parsed["data_offsets"]
                if data[at + payload_header : at + read_u64(data, at + 8)]
                == b"COMMON=all"
            )
            array = read_u64(data, common + 48)
            for name, pointer in (
                ("posting-inline", common + 40),
                ("posting-head", common + 48),
                ("posting-next", array + 16),
            ):
                mutated = bytearray(data)
                write_u64(mutated, pointer, 0)
                # Force a second posting array only in the missing-next case.
                if name == "posting-next":
                    write_u64(mutated, common + 56, 1_000_000)
                path = folder / f"{name}.journal"
                path.write_bytes(mutated)
                for operation in (
                    "explorer-filter",
                    "explorer-facet",
                    "explorer-histogram",
                ):
                    probe(path, operation, field="COMMON", error=True)
                for operation in (
                    "forward", "reverse", "facade-forward", "facade-reverse"
                ):
                    probe(
                        path, operation, match_hex=b"COMMON=all".hex(), error=True
                    )
            mutated = bytearray(data)
            write_u64(mutated, parsed["entry_array_offset"] + 16, 0)
            write_u64(mutated, 152, 1_000_000)
            path = folder / "missing-global-next.journal"
            path.write_bytes(mutated)
            for operation in ("forward", "reverse", "facade-forward", "facade-reverse"):
                probe(
                    path,
                    operation,
                    expected=list(reversed(rows)) if "reverse" in operation else rows,
                    recovery=True,
                    declared=1_000_000,
                )
            probe(path, "verify", error=True)
        if "streams" in selected:
            at = messages[0]
            start, end = at + payload_header, at + read_u64(data, at + 8)
            payload = data[start:end]
            half = len(payload) // 2
            frame = zstd.frame(payload, checksum=True)
            bad = bytearray(zstd.frame(payload[half:], checksum=True))
            bad[-1] ^= 1
            streams = {
                "known": (frame, False),
                "unknown": (zstd.frame(payload, known=False), False),
                "concatenated": (
                    zstd.frame(payload[:half]) + zstd.frame(payload[half:]),
                    False,
                ),
                "mixed-skippable": (
                    skippable(7)
                    + zstd.frame(payload[:half], known=False)
                    + skippable()
                    + zstd.frame(payload[half:]),
                    False,
                ),
                "empty-first": (zstd.frame(b"") + frame, False),
                "bad-later-checksum": (zstd.frame(payload[:half]) + bytes(bad), True),
                "truncated-later-frame": (
                    zstd.frame(payload[:half]) + zstd.frame(payload[half:])[:-3],
                    True,
                ),
                "oversized": (oversized_zstd_frame(768 * 1024 * 1024 + 1), True),
                "trailing-garbage": (frame + b"garbage", True),
            }
            for name, (stream, error) in streams.items():
                padding = end - start - len(stream) - 8
                if padding < 0:
                    raise RuntimeError("fixture lacks padding capacity")
                mutated = bytearray(data)
                mutated[at + 1] = 4
                mutated[12:16] = (int.from_bytes(data[12:16], "little") | 8).to_bytes(
                    4, "little"
                )
                mutated[start:end] = stream + skippable(padding)
                path = folder / f"zstd-{name}.journal"
                path.write_bytes(mutated)
                probe(path, "forward", expected=None if error else rows, error=error)
                probe(path, "verify", error=error)
                stock_verify(path, error)
            split = len(data) // 2
            whole = (
                zstd.frame(data[:split], known=False)
                + skippable(9)
                + zstd.frame(data[split:])
            )
            path = folder / "whole.journal.zst"
            path.write_bytes(whole)
            probe(path, "forward", expected=rows)
            path = folder / "truncated-whole.journal.zst"
            path.write_bytes(whole[:-3])
            probe(path, "forward", error=True)
        if "recovery" in selected:
            source = folder / "recovery-base.journal"
            original, graph, expected = fixture(source, 1005, compact, env)
            clear_directory = folder / "clear-matches-directory"
            clear_directory.mkdir()
            (clear_directory / "system.journal").write_bytes(original)
            for path, is_directory in ((source, False), (clear_directory, True)):
                for operation in ("flush", "facade-flush"):
                    probe(path, operation, expected=expected, directory=is_directory,
                          match_hex=b"SPARSE=some".hex())
            item = 4 if compact else 8
            last_array = graph["entry_array_offset"]
            while read_u64(original, last_array + 16):
                last_array = read_u64(original, last_array + 16)
            for name, cut in (
                ("lost-entry", graph["entry_offsets"][-1]),
                ("lost-array", last_array),
                ("partial-array", last_array + 24 + 3 * item),
            ):
                prefix = []
                remaining, array = 1005, graph["entry_array_offset"]
                while remaining and array + 24 <= cut:
                    capacity = (read_u64(original, array + 8) - 24) // item
                    used = min(remaining, capacity)
                    for i in range(used):
                        slot = array + 24 + i * item
                        entry = int.from_bytes(original[slot : slot + item], "little")
                        if (
                            slot + item > cut
                            or entry + read_u64(original, entry + 8) > cut
                        ):
                            break
                        prefix.append(expected[len(prefix)])
                    else:
                        remaining -= used
                        array = read_u64(original, array + 16)
                        continue
                    break
                path = folder / f"{name}.journal"
                path.write_bytes(original[:cut])
                probe(path, "forward", expected=prefix, recovery=True, declared=1005)
                probe(
                    path,
                    "facade-forward",
                    expected=prefix,
                    recovery=True,
                    declared=1005,
                )
                probe(
                    path,
                    "reverse",
                    expected=list(reversed(prefix)),
                    recovery=True,
                    declared=1005,
                )
                probe(
                    path,
                    "facade-reverse",
                    expected=list(reversed(prefix)),
                    recovery=True,
                    declared=1005,
                )
                probe(
                    path,
                    "forward",
                    expected=prefix,
                    recovery=True,
                    declared=1005,
                    match_hex=b"COMMON=all".hex(),
                )
                sparse_rows = [
                    row for row in prefix if b"SPARSE=some".hex() in row["payloads"]
                ]
                sparse_data = next(
                    at
                    for at in graph["data_offsets"]
                    if original[at + payload_header : at + read_u64(original, at + 8)]
                    == b"SPARSE=some"
                )
                sparse_complete = posting_prefix_known(
                    original,
                    sparse_data,
                    graph["entry_offsets"][len(prefix) - 1],
                    cut,
                    compact,
                )
                for operation in (
                    "forward", "reverse", "facade-forward", "facade-reverse"
                ):
                    backwards = operation.endswith("reverse")
                    probe(
                        path,
                        operation,
                        expected=(list(reversed(sparse_rows)) if backwards else sparse_rows)
                        if sparse_complete else [],
                        error=not sparse_complete,
                        recovery=True,
                        declared=1005,
                        recovered_count=len(prefix),
                        match_hex=b"SPARSE=some".hex(),
                    )
                    probe(
                        path,
                        operation,
                        expected=list(reversed(prefix)) if backwards else prefix,
                        recovery=True,
                        declared=1005,
                        recovered_count=len(prefix),
                        match_hex=b"COMMON=all".hex(),
                    )
                probe(path, "forward", expected=[], match_hex=b"ABSENT=value".hex())
                probe(
                    path,
                    "seek-forward",
                    expected=[row for row in prefix if row["realtime"] >= 2000],
                    recovery=True,
                    declared=1005,
                    recovered_count=len(prefix),
                    realtime=2000,
                )
                directory = folder / f"{name}-directory"
                directory.mkdir()
                (directory / "system.journal").write_bytes(original[:cut])
                probe(
                    directory,
                    "forward",
                    expected=prefix,
                    recovery=True,
                    declared=1005,
                    directory=True,
                )
                for operation in ("forward", "reverse", "facade-forward", "facade-reverse"):
                    backwards = operation.endswith("reverse")
                    probe(
                        directory,
                        operation,
                        expected=(list(reversed(sparse_rows)) if backwards else sparse_rows)
                        if sparse_complete else [],
                        error=not sparse_complete,
                        recovery=True,
                        declared=1005,
                        recovered_count=len(prefix),
                        match_hex=b"SPARSE=some".hex(),
                        directory=True,
                    )
                probe(path, "verify", error=True)
    report = {
        "oracle": subprocess.run(  # nosec B603 - explicit oracle and argument vector.
            [str(oracle), "--version"],
            capture_output=True,
            text=True,
            env=env,
            check=True,
        ).stdout,
        "observations": observations,
        "status": "PASS",
    }
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"status": "PASS", "observations": len(observations)}), flush=True)


if __name__ == "__main__":
    main()
