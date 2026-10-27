"""Approved deterministic archive ordering for clock-rollback histories."""
from __future__ import annotations

import os
import shutil
import subprocess  # nosec B404
import time
from pathlib import Path

from go_fixture_writer import write_journal_file

# All 21 originals were verified by the pinned v262 oracle.
ARCHIVES = [(2, 1, 1600000000000776),
 (2, 2, 1600000000000041),
 (2, 3, 1600000000000988),
 (3, 4, 1600000000000497),
 (2, 5, 1600000000000940),
 (2, 6, 1600000000000991),
 (2, 7, 1600000000000366),
 (3, 8, 1600000000000913),
 (1, 9, 1600000000000516),
 (1, 10, 1600000000000288),
 (1, 11, 1600000000000773),
 (1, 12, 1600000000000633),
 (2, 13, 1600000000000931),
 (3, 14, 1600000000000722),
 (3, 15, 1600000000000923),
 (1, 16, 1600000000000317),
 (1, 17, 1600000000000747),
 (1, 18, 1600000000000920),
 (3, 19, 1600000000000338),
 (2, 20, 1600000000000573),
 (1, 21, 1600000000000362)]


def run_vacuum_cases(binary: Path, workspace: Path) -> list[bool]:
    source = workspace / "vacuum-source"
    source.mkdir()
    rows = []
    for group, sequence, realtime in ARCHIVES:
        name = f"system@{group:032x}-{sequence:016x}-{realtime:016x}.journal"
        write_journal_file(source / name, machine_id="a" * 32, boot_id="b" * 32,
                           seqnum_id=f"{group:032x}", head_seqnum=sequence,
                           entries=[{"realtime_usec": realtime, "monotonic_usec": sequence,
                                     "fields": [("MESSAGE", "archive rollback")]}])
        rows.append((name, group, sequence, realtime))
    # Independent specification: a file is eligible only after every earlier
    # sequence from its history has been removed from the ordering model.
    remaining = rows.copy()
    order = []
    while remaining:
        available = [r for r in remaining if not any(
            other[1] == r[1] and (other[2], other[0]) < (r[2], r[0])
            for other in remaining)]
        chosen = min(available, key=lambda r: (r[3], r[1], r[0]))
        order.append(chosen[0])
        remaining.remove(chosen)
    results = []
    cases = [("count-two", ["--vacuum-files=2"], set(order[-2:])),
             ("count-ten", ["--vacuum-files=10"], set(order[-10:])),
             ("age", ["--vacuum-time=1d"], set()),
             ("size", ["--vacuum-size=1"], set()),
             ("combined", ["--vacuum-files=2", "--vacuum-time=1d"], set())]
    for reverse in (False, True):
        for label, args, expected in cases:
            directory = workspace / f"vacuum-{label}-{reverse}"
            directory.mkdir()
            for name, *_ in (reversed(rows) if reverse else rows):
                shutil.copyfile(source / name, directory / name)
            result = subprocess.run(  # nosec B603 - explicit test binary and private fixture directory.
                [str(binary), "--directory", str(directory), "--quiet", *args],
                capture_output=True, timeout=30, check=False,
                env={**os.environ, "LC_ALL": "C", "TZ": "UTC"})
            observed = {p.name for p in directory.iterdir()}
            passed = result.returncode == 0 and observed == expected
            results.append(passed)
            print("PASS" if passed else "FAIL", "vacuum", label, "reverse", reverse)
            if not passed:
                print("status", result.returncode, "stderr", result.stderr,
                      "expected", sorted(expected), "observed", sorted(observed))
    # An old successor cannot bypass its recent earlier sequence for age-only vacuum.
    recent = int(time.time() * 1_000_000)
    old = recent - 3 * 86_400_000_000
    for reverse in (False, True):
        directory = workspace / f"vacuum-age-sequence-{reverse}"
        directory.mkdir()
        entries = [(1, 1, recent), (1, 2, old), (2, 1, old)]
        keep = set()
        for group, sequence, realtime in (reversed(entries) if reverse else entries):
            name = f"system@{group:032x}-{sequence:016x}-{realtime:016x}.journal"
            write_journal_file(directory / name, machine_id="a" * 32, boot_id="b" * 32,
                               seqnum_id=f"{group:032x}", head_seqnum=sequence,
                               entries=[{"realtime_usec": realtime, "monotonic_usec": sequence,
                                         "fields": [("MESSAGE", "age sequence")]}])
            if group == 1:
                keep.add(name)
        result = subprocess.run(  # nosec B603 - explicit test binary and private fixture directory.
            [str(binary), "--directory", str(directory), "--quiet", "--vacuum-time=1d"],
            capture_output=True, timeout=30, check=False)
        passed = result.returncode == 0 and {p.name for p in directory.iterdir()} == keep
        results.append(passed)
        print("PASS" if passed else "FAIL", "vacuum age sequence", reverse)
    return results
