#!/usr/bin/env python3
"""Replay a small v262-qualified CLI contract on native non-systemd platforms."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess  # nosec B404
import tempfile
from pathlib import Path

import go_fixture_writer
from journalctl_v262_cases import write_v262_fixture
from journalctl_unit_cases import unit_invocation_cases
from journalctl_timestamp_cases import write_timestamp_zone
from go_fixture_writer import write_journal_file
from journalctl_vacuum_cases import run_vacuum_cases


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--go", type=Path)
    parser.add_argument("--rust", type=Path)
    args = parser.parse_args()
    commands = [(name, binary) for name, binary in [("go", args.go), ("rust", args.rust)] if binary]
    if not commands:
        parser.error("provide --go, --rust, or both")
    root = Path(__file__).resolve().parents[2]
    scratch = root / ".local" / "interoperability"
    scratch.mkdir(parents=True, exist_ok=True)
    workspace = Path(tempfile.mkdtemp(prefix="portable-smoke-", dir=scratch))
    go_fixture_writer.BUILDER_DIR = workspace / "builder"
    go_fixture_writer.SPEC_DIR = workspace / "specs"
    fixture = write_v262_fixture(workspace / "fixture.journal")
    paths = Path(tempfile.mkdtemp(prefix="portable-paths-", dir=fixture.parent))
    unit_directory = paths / "units"
    unit_directory.mkdir()
    unit_cases = {name: (path, options, mode)
                  for name, path, options, mode in unit_invocation_cases(unit_directory)}
    # Expected results are qualified against the pinned v262 oracle matrix.
    def invocation(index, digit, first, last):
        return {"index": index, "invocation_id": digit * 32,
                "first_entry": 1_700_007_000_000_000 + first,
                "last_entry": 1_700_007_000_000_000 + last}
    unit_expected = {
        "unit-name-8---unit": (0, b"3\n5\n6\n8\n9\n10\n", False),
        "unit-name-8---user-unit": (0, b"3\n5\n6\n8\n9\n10\n", False),
        "unit-name-3---unit": (0, b"3\n5\n6\n8\n9\n10\n", False),
        "unit-name-3---user-unit": (0, b"3\n5\n6\n8\n9\n10\n", False),
        "unit-name-9---unit": (0, b"3\n5\n6\n8\n9\n10\n", False),
        "unit-name-10---unit": (0, b"3\n5\n6\n8\n9\n10\n", False),
        "unit-name-14---unit": (0, b"14\n", False),
        "unit-name-20---unit": (0, b"20\n", False),
        "unit-name-26---unit": (0, b"26\n", False),
        "unit-empty---unit": (1, b"", True),
        "invocation-no-id[]": (1, b"", False),
        "invocation-bounds[]": (0, [invocation(0, "1", 0, 2)], False),
        "invocation-precedence[]": (0, [invocation(-1, "1", 0, 0), invocation(0, "3", 1, 1)], False),
        "invocation-overlap['-n1']": (0, [invocation(0, "3", 3, 5)], False),
        "invocation-select-bounds0": (0, b"0\n1\n2\n", False),
        "invocation-select-precedence" + "2" * 32: (0, b"0\n", False),
        "invocation-select-overlap-1": (0, b"0\n2\n", False),
        "invocation-other-filter-['-tabsent']": (0, b"", False),
    }
    for option in ("--unit", "--user-unit"):
        unit_expected["unit-bracket-invocations-" + option] = (
            0, [invocation(-1, "1", 0, 0), invocation(0, "2", 1, 1)], False)
        unit_expected["unit-bracket-select-" + option] = (0, b"1\n", False)
    unicode_file = write_v262_fixture(paths / "file-é.journal", [b"unicode-file"])
    write_v262_fixture(paths / ".hidden.journal", [b"hidden-file"])
    brackets = paths / "brackets"
    brackets.mkdir()
    literal_bracket = write_v262_fixture(brackets / "literal-[x.journal", [b"literal-bracket"])
    colored = fixture.parent / "portable-colors.journal"
    write_journal_file(colored, machine_id="00112233445566778899aabbccddeeff",
                       boot_id="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", seqnum_id="12121212121212121212121212121212",
                       entries=[{"realtime_usec": 1_700_005_000_000_000, "monotonic_usec": 1,
                                 "fields": [("MESSAGE", "alpha hit beta"), ("PRIORITY", "3")]}])
    zone_dir = workspace / "zones"
    zone_dir.mkdir()
    write_timestamp_zone(zone_dir / "ReviewZone", [], [(-18000,0,"REV")])
    leap_zone = write_timestamp_zone(zone_dir / "leap", [], [(0,0,"UTC")], [(1483228800,1)])
    write_timestamp_zone(zone_dir / "posixrules", [(1672531200,1),(1701388800,0)],
                         [(-18000,0,"STD"),(-14400,1,"DST")])
    time_cases = [
        ({"TZ":"EST5EDT,M3.2.0,M11.1.0"}, b"2023-11-14T18:36:40-05:00"),
        ({"TZ":"ReviewZone", "TZDIR":str(zone_dir.resolve())}, b"2023-11-14T18:36:40-05:00"),
        ({"TZ":leap_zone}, b"2023-11-14T23:36:39+00:00"),
        ({"TZ":"XYZ5ABC", "TZDIR":str(zone_dir.resolve())}, b"2023-11-14T19:36:40-04:00"),
        ({"TZ":"XYZ5ABC", "TZDIR":str(workspace.resolve())}, b"2023-11-14T18:36:40-05:00"),
    ]
    env = {**os.environ, "TZ": "UTC", "LC_ALL": "C", "SYSTEMD_COLORS": "0"}
    failures = 0
    for language, binary in commands:
        def execute(options, path=fixture, changes=None):
            return subprocess.run(  # nosec B603 - explicit binary and synthetic fixture.
                [str(binary.resolve()), "--file", str(path), "--no-pager", *options],
                capture_output=True, env={**env, **(changes or {})}, timeout=30, check=False)
        cases = [
            (["--grep=(?=alpha)alpha", "-ocat"], 0, b"alpha alpha\n", False),
            ([r"--grep=(alpha) \1", "-ocat"], 0, b"alpha alpha\n", False),
            ([r"--grep=nul\x00tail", "-ocat"], 0, b"nul\x00tail\n", False),
            ([r"--grep=\xff", "-ocat"], 0, b"invalid\xffbyte\n", False),
            (["--grep=only-second-match", "-ocat"], 1, b"", False),
            (["--grep=unmatched-pattern", "-ocat"], 1, b"", False),
            (["--grep=(", "-ocat"], 1, b"", True),
            (["-F", "ROW", "--since=@0"], 1, b"", True),
            (["--unit=missing-*", "-ocat"], 1, b"", True),
            (["--user-unit=missing-*", "-ocat"], 1, b"", True),
            (["--unit=absent.service", "-ocat"], 0, b"", False),
            (["--since=bad", "--help"], 1, b"", True),
            (["--grep=--help", "--help"], 0, None, False),
        ]
        for options, code, output, diagnostic in cases:
            result = execute(options)
            ok = (result.returncode == code and (output is None or result.stdout == output)
                  and bool(result.stderr) == diagnostic)
            print(language, "PASS" if ok else "FAIL", options)
            failures += not ok
        result = execute(["--version"])
        ok = result.returncode == 0 and b"v262" in result.stdout and b"PCRE2" in result.stdout
        failures += not ok
        print(language, "PASS" if ok else "FAIL", "version")
        result = execute(["--introspect-cli"])
        try:
            info = json.loads(result.stdout)
            ok = result.returncode == 0 and info["commands"][0]["names"] == ["journalctl"]
        except (ValueError, KeyError, IndexError):
            ok = False
        failures += not ok
        print(language, "PASS" if ok else "FAIL", "introspection")
        result = execute(["-ojson"])
        rows = [json.loads(line) for line in result.stdout.splitlines()]
        cursor = rows[1]["__CURSOR"] if result.returncode == 0 and len(rows) == 7 else None
        for option, expected in [("--cursor",b"first\nsecond\n"),("--after-cursor",b"nul\x00tail\n")]:
            result = execute([option, cursor or "invalid", "-n1", "-ocat"])
            ok = cursor is not None and result.returncode == 0 and result.stdout == expected and not result.stderr
            failures += not ok
            print(language, "PASS" if ok else "FAIL", option)
        for path, expected in [(unicode_file, b"unicode-file\n"),
                               (paths / "file-{é,missing}.journal", b"unicode-file\n"),
                               (paths / "file-*.journal", b"unicode-file\n"),
                               (paths / "file-[!a]*.journal", b"unicode-file\n"),
                               (literal_bracket, b"literal-bracket\n"),
                               (paths / "*.journal", b"unicode-file\n"),
                               (paths / ".*.journal", b"hidden-file\n")]:
            result = execute(["-ocat"], path)
            ok = result.returncode == 0 and result.stdout == expected and not result.stderr
            failures += not ok
            print(language, "PASS" if ok else "FAIL", "file selection", path.name)
        result = execute(["-ocat", "--grep=hit"], colored, {"SYSTEMD_COLORS": "16"})
        expected = b"\x1b[0;1;31malpha \x1b[0;1;39mhit\x1b[0;1;31m beta\x1b[0m\n"
        ok = result.returncode == 0 and result.stdout == expected and not result.stderr
        failures += not ok
        print(language, "PASS" if ok else "FAIL", "colored grep")
        result = execute(["-ojson"], colored, {"SYSTEMD_COLORS": "16"})
        try:
            row = json.loads(re.sub(rb"\x1b\[[0-9;:]*m", b"", result.stdout))
            ok = (result.returncode == 0 and not result.stderr and row["MESSAGE"] == "alpha hit beta"
                  and b'"\x1b[0;32mMESSAGE\x1b[0m"' in result.stdout)
        except (ValueError, KeyError):
            ok = False
        failures += not ok
        print(language, "PASS" if ok else "FAIL", "colored JSON")
        for settings, timestamp in time_cases:
            result = execute(["-q", "-oshort-iso", "--no-hostname"], colored, settings)
            ok = (result.returncode == 0 and not result.stderr
                  and result.stdout == timestamp + b" unknown: alpha hit beta\n")
            failures += not ok
            print(language, "PASS" if ok else "FAIL", "calendar timezone", settings)
            if not ok:
                print(f"  observed: {result.stdout!r} {result.stderr!r}")
        for name, (code, expected, diagnostic) in unit_expected.items():
            path, options, _ = unit_cases[name]
            result = execute(["-q", *options], path)
            try:
                output = json.loads(result.stdout) if isinstance(expected, list) else result.stdout
                ok = result.returncode == code and output == expected and bool(result.stderr) == diagnostic
            except ValueError:
                ok = False
            failures += not ok
            print(language, "PASS" if ok else "FAIL", name)
            if not ok:
                print(f"  expected: status={code} stdout={expected!r} diagnostic={diagnostic}")
                print(f"  observed: status={result.returncode} stdout={result.stdout!r} stderr={result.stderr!r}")
    for language, binary in commands:
        vacuum_workspace = workspace / (language + "-vacuum")
        vacuum_workspace.mkdir()
        failures += sum(not passed for passed in run_vacuum_cases(binary, vacuum_workspace))
    total = (38 + len(unit_expected) + len(time_cases)) * len(commands)
    print(f"portable CLI smoke: {total-failures}/{total} passed")
    return int(failures != 0)


if __name__ == "__main__":
    raise SystemExit(main())
