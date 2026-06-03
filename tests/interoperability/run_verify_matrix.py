#!/usr/bin/env python3
"""Full verifier parity matrix for stock systemd and all SDK journalctl rewrites.

The runner generates deterministic positive journals and negative corruptions
under `.local/interoperability/verify/`. Stock `journalctl --verify --file`
is the oracle for every generated corruption class.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess  # nosec B404 - subprocess is required by harnesses.
import sys
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Iterable


REPO_ROOT = Path(__file__).resolve().parents[2]
LOCAL_DIR = REPO_ROOT / ".local" / "interoperability"
BIN_DIR = LOCAL_DIR / "bin"
FIXTURE_DIR = LOCAL_DIR / "verify"
PYTHON = os.environ.get("PYTHON", sys.executable)

sys.path.insert(0, str(REPO_ROOT / ".local" / "python-deps"))
sys.path.insert(0, str(REPO_ROOT / "python"))

from journal import Writer  # noqa: E402
from journal.seal import SealOptions  # noqa: E402


MACHINE_ID = "00112233445566778899aabbccddeeff"
BOOT_ID = "11112222333344445555666677778888"
SEQNUM_ID = "88887777666655554444333322221111"
VERIFY_SEED_HEX = "000000000000000000000000"
VERIFY_START_USEC = 1_000_000
VERIFY_INTERVAL_USEC = 1_000_000
VERIFY_KEY = f"{VERIFY_SEED_HEX}/{VERIFY_START_USEC // VERIFY_INTERVAL_USEC:x}-{VERIFY_INTERVAL_USEC:x}"
SEAL_OPTS = SealOptions(
    seed=bytes.fromhex(VERIFY_SEED_HEX),
    interval_usec=VERIFY_INTERVAL_USEC,
    start_usec=VERIFY_START_USEC,
)
LONG_PAYLOAD = bytes((idx % 26) + 0x41 for idx in range(512))
BINARY_PAYLOAD = bytes([0x00, 0x01, 0x02, 0x41, 0x0A, 0x7F, 0x80, 0xFF])

HEADER_SIZE = 272
HEADER_MIN_SIZE = 208
OBJECT_HEADER_SIZE = 16
HASH_ITEM_SIZE = 16
ENTRY_OBJECT_HEADER_SIZE = 64
OFFSET_ARRAY_OBJECT_HEADER_SIZE = 24
REGULAR_OFFSET_ARRAY_ITEM_SIZE = 8
OBJECT_TYPE_DATA = 1
OBJECT_TYPE_ENTRY = 3
OBJECT_TYPE_ENTRY_ARRAY = 6
OBJECT_TYPE_TAG = 7
OBJECT_COMPRESSED_ZSTD = 1 << 2
DATA_OBJECT_HEADER_SIZE = 64
COMPACT_DATA_OBJECT_HEADER_SIZE = 72
INCOMPATIBLE_COMPACT = 1 << 4
MAX_UNCOMPRESSED_DATA_OBJECT_SIZE = 768 * 1024 * 1024


@dataclass(frozen=True)
class FixtureSpec:
    name: str
    compact: bool = False
    compression: str = "none"
    sealed: bool = False

    @property
    def verify_key(self) -> str | None:
        return VERIFY_KEY if self.sealed else None


@dataclass(frozen=True)
class CorruptionSpec:
    name: str
    source: str
    verify_key: str | None = None


POSITIVE_SPECS = [
    FixtureSpec("regular"),
    FixtureSpec("zstd", compression="zstd"),
    FixtureSpec("xz", compression="xz"),
    FixtureSpec("lz4", compression="lz4"),
    FixtureSpec("compact", compact=True),
    FixtureSpec("compact-zstd", compact=True, compression="zstd"),
    FixtureSpec("compact-xz", compact=True, compression="xz"),
    FixtureSpec("compact-lz4", compact=True, compression="lz4"),
    FixtureSpec("sealed", sealed=True),
]

NEGATIVE_SPECS = [
    CorruptionSpec("object_type_unknown", "regular"),
    CorruptionSpec("object_size_too_small", "regular"),
    CorruptionSpec("zstd_decompressed_size_too_large", "zstd"),
    CorruptionSpec("data_hash_bad", "regular"),
    CorruptionSpec("data_hash_bucket_missing", "regular"),
    CorruptionSpec("entry_array_unsorted", "regular"),
    CorruptionSpec("header_n_data_bad", "regular"),
    CorruptionSpec("main_entry_array_missing", "regular"),
    CorruptionSpec("entry_seqnum_zero", "regular"),
    CorruptionSpec("tail_entry_seqnum_bad", "regular"),
    CorruptionSpec("tail_monotonic_bad", "regular"),
    CorruptionSpec("tag_hmac_bad", "sealed", VERIFY_KEY),
]


def run(
    cmd: list[str],
    *,
    cwd: Path = REPO_ROOT,
    timeout: int = 120,
    env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    return subprocess.run(  # nosec B603 - harness uses shell=False command vectors.
        cmd,  # nosemgrep
        cwd=str(cwd),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        env=env,
        check=False,
    )


def require_ok(result: subprocess.CompletedProcess[str], label: str) -> None:
    if result.returncode != 0:
        raise RuntimeError(
            f"{label} failed with exit {result.returncode}\n"
            f"stdout:\n{text_tail(result.stdout)}\n"
            f"stderr:\n{text_tail(result.stderr)}"
        )


def build_env() -> dict[str, str]:
    env = os.environ.copy()
    local = REPO_ROOT / ".local"
    env.setdefault("GOMODCACHE", str(local / "go" / "pkg" / "mod"))
    env.setdefault("GOCACHE", str(local / "go-build"))
    env.setdefault("GOPATH", str(local / "go"))
    env.setdefault("CARGO_HOME", str(local / "cargo-home"))
    env.setdefault("CARGO_TARGET_DIR", str(local / "cargo-target"))
    env.setdefault("npm_config_cache", str(local / "npm-cache"))
    env.setdefault("PIP_CACHE_DIR", str(local / "pip-cache"))
    python_deps = local / "python-deps"
    env["PYTHONPATH"] = (
        f"{REPO_ROOT / 'python'}{os.pathsep}{python_deps}{os.pathsep}{env['PYTHONPATH']}"
        if env.get("PYTHONPATH")
        else f"{REPO_ROOT / 'python'}{os.pathsep}{python_deps}"
    )
    return env


def build_tools() -> dict[str, str]:
    BIN_DIR.mkdir(parents=True, exist_ok=True)
    env = build_env()

    go_journalctl = BIN_DIR / "go-journalctl"
    require_ok(
        run(["go", "build", "-o", str(go_journalctl), "./cmd/journalctl"], cwd=REPO_ROOT / "go", env=env),
        "build go journalctl",
    )
    require_ok(
        run(
            ["cargo", "build", "--manifest-path", str(REPO_ROOT / "rust/Cargo.toml"), "-p", "journalctl"],
            timeout=180,
            env=env,
        ),
        "build rust journalctl",
    )
    rust_src = Path(env["CARGO_TARGET_DIR"]) / "debug" / "journalctl"
    rust_journalctl = BIN_DIR / "rust-journalctl"
    if rust_src.exists():
        shutil.copy2(rust_src, rust_journalctl)

    for path in (go_journalctl, rust_journalctl):
        if not path.exists():
            raise RuntimeError(f"expected binary not found: {path}")

    return {
        "go": str(go_journalctl),
        "rust": str(rust_journalctl),
    }


def systemd_version() -> str:
    result = run(["journalctl", "--version"], timeout=10)
    if result.returncode == 0 and result.stdout.strip():
        return result.stdout.splitlines()[0]
    return "unavailable"


def make_fixtures() -> tuple[dict[str, Path], dict[str, Path]]:
    if FIXTURE_DIR.exists():
        shutil.rmtree(FIXTURE_DIR)
    positive_dir = FIXTURE_DIR / "positive"
    negative_dir = FIXTURE_DIR / "negative"
    positive_dir.mkdir(parents=True)
    negative_dir.mkdir(parents=True)

    positives: dict[str, Path] = {}
    for spec in POSITIVE_SPECS:
        path = positive_dir / f"{spec.name}.journal"
        write_positive(path, spec)
        positives[spec.name] = path

    negatives: dict[str, Path] = {}
    for spec in NEGATIVE_SPECS:
        source = positives[spec.source]
        target = negative_dir / f"{spec.name}.journal"
        data = bytearray(source.read_bytes())
        corrupt(data, spec.name)
        target.write_bytes(data)
        negatives[spec.name] = target

    return positives, negatives


def write_positive(path: Path, spec: FixtureSpec) -> None:
    opts = {
        "machine_id": MACHINE_ID,
        "boot_id": BOOT_ID,
        "seqnum_id": SEQNUM_ID,
        "compact": spec.compact,
        "compression": spec.compression,
        "compression_threshold_bytes": 16,
    }
    if spec.sealed:
        opts["seal"] = SEAL_OPTS
    writer = Writer.create(str(path), opts)
    try:
        for idx in range(5):
            writer.append(
                [
                    {"name": "_MACHINE_ID", "value": MACHINE_ID},
                    {"name": "_BOOT_ID", "value": BOOT_ID},
                    {"name": "TEST_ID", "value": "verify-parity"},
                    {"name": "MESSAGE", "value": f"verify parity {spec.name}"},
                    {"name": "PRIORITY", "value": "6"},
                    {"name": "LIVE_SEQ", "value": f"{idx:06d}"},
                    {"name": "VERIFY_KIND", "value": spec.name},
                    {"name": "SHARED_VALUE", "value": "shared"},
                    {"name": "LONG_PAYLOAD", "value": LONG_PAYLOAD},
                    {"name": "BINARY_PAYLOAD", "value": BINARY_PAYLOAD},
                ],
                {"realtime_usec": 1_500_000 + idx * 1_000, "monotonic_usec": idx + 1},
            )
    finally:
        writer.close()


def corrupt(data: bytearray, name: str) -> None:
    parsed = parse_journal(data)
    first_data = parsed["data_offsets"][0]
    first_entry = parsed["entry_offsets"][0]

    if name == "object_type_unknown":
        data[first_data] = 99
    elif name == "object_size_too_small":
        write_u64(data, first_data + 8, 8)
    elif name == "zstd_decompressed_size_too_large":
        data_offset = first_zstd_data_object(parsed, data)
        payload_offset = COMPACT_DATA_OBJECT_HEADER_SIZE if parsed["compact"] else DATA_OBJECT_HEADER_SIZE
        payload_len = read_u64(data, data_offset + 8) - payload_offset
        oversized = oversized_zstd_frame(MAX_UNCOMPRESSED_DATA_OBJECT_SIZE + 1)
        if payload_len < len(oversized):
            raise RuntimeError("zstd oversized corruption needs a larger compressed DATA object")
        data[data_offset + payload_offset:data_offset + payload_offset + len(oversized)] = oversized
    elif name == "data_hash_bad":
        write_u64(data, first_data + 16, read_u64(data, first_data + 16) + 1)
    elif name == "data_hash_bucket_missing":
        data_hash = read_u64(data, first_data + 16)
        bucket_count = parsed["data_hash_table_size"] // HASH_ITEM_SIZE
        bucket_offset = parsed["data_hash_table_offset"] + (data_hash % bucket_count) * HASH_ITEM_SIZE
        write_u64(data, bucket_offset, 0)
        write_u64(data, bucket_offset + 8, 0)
    elif name == "entry_array_unsorted":
        entry_array = parsed["entry_array_offset"]
        if len(parsed["entry_offsets"]) < 2:
            raise RuntimeError("entry_array_unsorted needs at least two entries")
        write_u64(data, entry_array + OFFSET_ARRAY_OBJECT_HEADER_SIZE, parsed["entry_offsets"][1])
    elif name == "header_n_data_bad":
        write_u64(data, 208, read_u64(data, 208) + 1)
    elif name == "main_entry_array_missing":
        write_u64(data, 176, 0)
    elif name == "entry_seqnum_zero":
        write_u64(data, first_entry + 16, 0)
    elif name == "tail_entry_seqnum_bad":
        write_u64(data, 160, 999)
    elif name == "tail_monotonic_bad":
        write_u64(data, 200, 999999)
    elif name == "tag_hmac_bad":
        tag_offset = parsed["tag_offsets"][0]
        data[tag_offset + 32] ^= 0x01
    else:
        raise ValueError(name)


def parse_journal(data: bytes | bytearray) -> dict:
    if len(data) < HEADER_MIN_SIZE:
        raise RuntimeError("journal too small")
    header_size = read_u64(data, 88)
    tail_object_offset = read_u64(data, 136)
    if header_size < HEADER_MIN_SIZE or tail_object_offset < header_size:
        raise RuntimeError("invalid generated journal header")

    parsed = {
        "data_hash_table_offset": read_u64(data, 104),
        "data_hash_table_size": read_u64(data, 112),
        "entry_array_offset": read_u64(data, 176),
        "data_offsets": [],
        "entry_offsets": [],
        "entry_array_offsets": [],
        "tag_offsets": [],
        "compact": (int.from_bytes(data[12:16], "little") & INCOMPATIBLE_COMPACT) != 0,
    }

    offset = header_size
    while True:
        typ = data[offset]
        size = read_u64(data, offset + 8)
        if typ == OBJECT_TYPE_DATA:
            parsed["data_offsets"].append(offset)
        elif typ == OBJECT_TYPE_ENTRY:
            parsed["entry_offsets"].append(offset)
        elif typ == OBJECT_TYPE_ENTRY_ARRAY:
            parsed["entry_array_offsets"].append(offset)
        elif typ == OBJECT_TYPE_TAG:
            parsed["tag_offsets"].append(offset)
        if offset == tail_object_offset:
            break
        offset += align8(size)

    if not parsed["data_offsets"] or not parsed["entry_offsets"]:
        raise RuntimeError("generated journal did not contain DATA and ENTRY objects")
    return parsed


def first_zstd_data_object(parsed: dict, data: bytes | bytearray) -> int:
    for offset in parsed["data_offsets"]:
        if data[offset + 1] & OBJECT_COMPRESSED_ZSTD:
            return offset
    raise RuntimeError("generated journal did not contain a zstd-compressed DATA object")


def oversized_zstd_frame(content_size: int) -> bytes:
    frame = bytearray()
    frame.extend(b"\x28\xb5\x2f\xfd")
    frame.append(0xE0)  # single segment plus 8-byte frame content size.
    frame.extend(content_size.to_bytes(8, "little"))
    frame.extend(b"\x01\x00\x00")  # final raw block, size 0.
    return bytes(frame)


def verify_command(reader: str, tools: dict[str, str], path: Path, verify_key: str | None) -> list[str]:
    if reader == "stock":
        cmd = ["journalctl", "--verify"]
    elif reader == "go":
        cmd = [tools["go"], "--verify"]
    elif reader == "rust":
        cmd = [tools["rust"], "--verify"]
    elif reader == "node":
        cmd = ["node", str(REPO_ROOT / "node/cmd/journalctl/index.js"), "--verify"]
    elif reader == "python":
        cmd = [PYTHON, str(REPO_ROOT / "python/cmd/journalctl.py"), "--verify"]
    else:
        raise ValueError(reader)
    if verify_key:
        cmd.extend(["--verify-key", verify_key])
    cmd.extend(["--file", str(path)])
    return cmd


def run_check(
    reader: str,
    tools: dict[str, str],
    path: Path,
    *,
    verify_key: str | None,
    should_pass: bool,
    test_name: str,
    env: dict[str, str],
) -> dict:
    cmd = verify_command(reader, tools, path, verify_key)
    result = run(cmd, timeout=45, env=env)
    passed = result.returncode == 0
    status = "PASS" if passed == should_pass else "FAIL"
    error = ""
    if status == "FAIL":
        expected = "pass" if should_pass else "fail"
        error = (
            f"expected {expected}, got exit {result.returncode}; "
            f"stdout={text_tail(result.stdout, 500)!r}; stderr={text_tail(result.stderr, 500)!r}"
        )
    return {
        "test": test_name,
        "reader": reader,
        "path": str(path),
        "command": shell_join(cmd),
        "status": status,
        "error": error,
    }


def read_u64(data: bytes | bytearray, offset: int) -> int:
    return int.from_bytes(data[offset:offset + 8], "little")


def write_u64(data: bytearray, offset: int, value: int) -> None:
    data[offset:offset + 8] = int(value).to_bytes(8, "little")


def align8(value: int) -> int:
    return (value + 7) & ~7


def shell_join(cmd: Iterable[str]) -> str:
    return " ".join(json.dumps(part) if any(ch.isspace() for ch in part) else part for part in cmd)


def text_tail(value: str | bytes, limit: int = 1000) -> str:
    if isinstance(value, bytes):
        value = value.decode("utf-8", errors="replace")
    return value[-limit:]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-build", action="store_true", help="reuse existing .local interoperability binaries")
    args = parser.parse_args()

    env = build_env()
    tools = {
        "go": str(BIN_DIR / "go-journalctl"),
        "rust": str(BIN_DIR / "rust-journalctl"),
    }
    if not args.skip_build:
        tools = build_tools()

    positives, negatives = make_fixtures()
    readers = ["stock", "go", "rust", "node", "python"]
    results = []

    for spec in POSITIVE_SPECS:
        path = positives[spec.name]
        for reader in readers:
            results.append(
                run_check(
                    reader,
                    tools,
                    path,
                    verify_key=spec.verify_key,
                    should_pass=True,
                    test_name=f"positive-{spec.name}",
                    env=env,
                )
            )

    for spec in NEGATIVE_SPECS:
        path = negatives[spec.name]
        for reader in readers:
            results.append(
                run_check(
                    reader,
                    tools,
                    path,
                    verify_key=spec.verify_key,
                    should_pass=False,
                    test_name=f"negative-{spec.name}",
                    env=env,
                )
            )

    failures = [result for result in results if result["status"] != "PASS"]
    output = {
        "generated_at": datetime.now(UTC).isoformat(timespec="seconds").replace("+00:00", "Z"),
        "systemd": systemd_version(),
        "fixture_dir": str(FIXTURE_DIR),
        "positive_count": len(POSITIVE_SPECS),
        "negative_count": len(NEGATIVE_SPECS),
        "results": results,
        "status": "PASS" if not failures else "FAIL",
        "failures": failures,
    }
    print(json.dumps(output, indent=2, sort_keys=True))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
