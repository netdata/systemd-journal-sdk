"""Timestamp grammar, bounds and calendar rendering for the v262 CLI target."""

from __future__ import annotations

import json
import os
import subprocess  # nosec B404
import struct
from datetime import datetime, timezone
from pathlib import Path

from go_fixture_writer import write_journal_file


def write_timestamp_zone(path, transitions, kinds, leaps=(), footer=None, standard=(), universal=()):
    """Write a tiny valid TZif; all identities and clock rules are synthetic."""
    abbreviations = b""
    records = []
    for offset, dst, name in kinds:
        records.append(struct.pack(">iBB", offset, dst, len(abbreviations)))
        abbreviations += name.encode() + b"\0"
    def block(width):
        fmt = ">i" if width == 4 else ">q"
        return (b"".join(struct.pack(fmt, t) for t, _ in transitions)
                + bytes(k for _, k in transitions) + b"".join(records) + abbreviations
                + b"".join(struct.pack(fmt, t) + struct.pack(">i", c) for t, c in leaps)
                + bytes(standard) + bytes(universal))
    version = b"2" if footer is not None else b"\0"
    header = struct.pack(">4sc15x6I", b"TZif", version, len(universal), len(standard), len(leaps),
                         len(transitions), len(kinds), len(abbreviations))
    data = header + block(4)
    if footer is not None:
        data += header + block(8) + b"\n" + footer.encode() + b"\n"
    path.write_bytes(data)
    return str(path.resolve())


def run_timestamp_cases(tools: dict[str, str], fixtures: dict[str, Path]) -> list[dict[str, object]]:
    path = fixtures["file"].parent / "timestamp-bounds.journal"
    # Place rows on either side of calendar, epoch-fraction, and DST boundaries.
    # Compare complete returned rows/cursors; accepting a spelling is insufficient.
    points = {0, 1, 999999, 1000000, 1000001, 1500000, 3600000000, 5400000000,
              253402214399000000, 253402214399000001}
    for year, month, day, hour, minute, second in [
        (1970,1,1,0,0,0), (2000,1,1,0,0,0), (2012,9,22,16,34,22),
        (2023,3,3,0,0,0), (2023,11,14,0,0,0), (2023,11,14,23,20,0),
        (2023,11,14,23,20,50), (2024,3,10,6,30,0), (2024,3,10,7,30,0),
        (2024,3,31,0,30,0), (2024,3,31,1,30,0), (2024,10,27,0,30,0),
        (2024,10,27,1,30,0), (2024,11,3,5,30,0), (2024,11,3,6,30,0),
        (2068,1,1,0,0,0), (datetime.now(timezone.utc).year,11,14,23,20,50),
    ]:
        base = int(datetime(year,month,day,hour,minute,second,tzinfo=timezone.utc).timestamp())*1_000_000
        points.update(base+d for d in [-1,0,1,123455,123456,123457,999999,1000000] if base+d >= 0)
    for base in [1483228800, 1704110400]:
        for offset in [-10800,-7200,-3600,0,3600,7200,10800]:
            points.update((base+offset+second)*1_000_000 for second in [-2,-1,0,1,2])
    boot = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    write_journal_file(path, machine_id="00112233445566778899aabbccddeeff", boot_id=boot,
                       seqnum_id="12121212121212121212121212121212", entries=[
                           {"realtime_usec": value, "monotonic_usec": i+1,
                            "fields": [("MESSAGE",str(value)),("_BOOT_ID",boot),
                                       ("_SYSTEMD_UNIT","clock.service"),
                                       ("_SYSTEMD_INVOCATION_ID","34343434343434343434343434343434")]}
                           for i,value in enumerate(sorted(value for value in points if 0 < value < (1 << 55)))])
    commands = {"stock": tools.get("stock_journalctl", "journalctl"),
                "go": tools["go_journalctl"], "rust": tools["rust_journalctl"]}
    check = subprocess.run(  # nosec B603 - fixture validation with the selected oracle.
        [commands["stock"], "--file", str(path), "--verify"], capture_output=True, timeout=30, check=False)
    if check.returncode != 0:
        raise AssertionError("stock rejected timestamp fixture: " + check.stderr.decode(errors="replace"))
    spellings = [
        "@0", "@.5", "@1.0000009", "@1h30m", "@1 2", "@infinity", "@18446744073709.551614",
        "2012-09-22 16:34:22", "2012-09-22T16:34:22", "2012-9-22T16:34:22.1234567Z",
        "12-09-22 16:34:22", "12-09-22 16:34", "2012-09-22 16:34", "2012-09-22T16:34",
        "12-09-22", "2012-09-22", "Nov 14 23:20:50", "November 14 23:20:50",
        "00:00:00", "00:00", "today", "yesterday", "tomorrow", "now", "+0", "-0", "0 ago", "0 left",
        "2023-11-14 00:00:00 UTC", "2023-11-14T00:00:00Z", "2023-11-14 00:00:00 +00",
        "2023-11-14 00:00:00 +0000", "2023-11-14 00:00:00 +00:00", "2023-11-14T00:00:00.0000005Z",
        "2023-11-14 00:00:00.9999999 UTC", "2023-11-14 00:00:60 UTC", "2023-11-14 00:00:61 UTC",
        "2023-02-31", "Tue 2023-11-14 00:00:00 UTC", "tuesday 2023-11-14", "2023-11-14 00:00:00 Z",
        "1970-01-01 00:00:00 +24:00", "1969-12-31 23:00:00 -06", "68-01-01", "69-12-31",
        "2023-11-14 00:00:00 GMT", "2023-11-14 00:00:00 Etc/UTC", "2023-11-14 00:00:00 UCT", "2023-11-14 00:00:00 Universal",
        "2023-11-14 00:00:00 Europe/Berlin", "2023-11-14 00:00:00 europe/berlin",
        "2024-01-01 12:00:00 NZ", "2024-01-01 12:00:00 Europe//Berlin",
        "2024-01-01 12:00:00 Europe/./Berlin", "2024-01-01 12:00:00 Europe/Berlin/",
        "", "infinity", "@-0", "@18446744073710", "@9223372036854775808us", "@1secs", "@1msecs",
        "@1H", "@1µs", "@1μs", "@0.1234567min", "@1.2.3", "@1.2 .3", "@1.2s.3", "@1.",
        "2023-11-14T00:00:00. UTC", "2023-13-01", "2023-01-00", "2023-01-01 24:00:00",
        "2023-01-01 00:60:00", "2023-01-01 00:00:62", "Mon 2023-11-14", "9999-12-31",
    ]
    cases = [("grammar-"+str(i), ["--since="+value], "UTC") for i,value in enumerate(spellings)]
    for zone, fold, summer, winter in [
        ("Europe/Berlin", "2024-10-27 02:30:00", "2024-07-01 12:00:00", "2024-01-01 12:00:00"),
        ("America/New_York", "2024-11-03 01:30:00", "2024-07-01 12:00:00", "2024-01-01 12:00:00"),
    ]:
        cases.extend([
            (zone+"-fresh-fold", ["--since="+fold], zone),
            (zone+"-summer-seeded-fold", ["--since="+summer,"--since="+fold], zone),
            (zone+"-winter-seeded-fold", ["--since="+winter,"--since="+fold], zone),
            (zone+"-ordered-bounds", ["--until="+summer,"--since="+fold,"--until=@infinity"], zone),
        ])
    cases.extend([
        ("berlin-gap", ["--since=2024-03-31 02:30:00"], "Europe/Berlin"),
        ("new-york-gap", ["--since=2024-03-10 02:30:00"], "America/New_York"),
        ("forced-standard-summer", ["--since=2024-07-01 12:00:00 CET"], "Europe/Berlin"),
        ("forced-daylight-winter", ["--since=2024-01-01 12:00:00 CEST"], "Europe/Berlin"),
        ("posix-rule-gap",["--since=2024-03-10 02:30:00"],"EST5EDT,M3.2.0/2,M11.1.0/2"),
        ("posix-rule-forced-standard",["--since=2024-07-01 12:00:00 EST"],"EST5EDT,M3.2.0/2,M11.1.0/2"),
        ("posix-rule-forced-daylight",["--since=2024-01-01 12:00:00 EDT"],"EST5EDT,M3.2.0/2,M11.1.0/2"),
    ])
    leap = write_timestamp_zone(path.parent / "timestamp-leap.tzif", [], [(0,0,"UTC")], [(1483228800,1)])
    historical = write_timestamp_zone(path.parent / "timestamp-historical.tzif",
        [(946684800,1),(1585443600,2),(1603587600,1),(1893456000,3)],
        [(0,0,"OLD"),(3600,0,"NEW"),(7200,1,"DAY"),(10800,0,"FUT")])
    footer = write_timestamp_zone(path.parent / "timestamp-footer.tzif", [],
        [(0,0,"OLD"),(3600,1,"DAY")], footer="XST-2XDT,M3.5.0/2,M10.5.0/3")
    for suffix in ["", " UTC"]:
        cases.append(("tzif-leap"+suffix, ["--since=2024-01-01 12:00:00"+suffix], leap))
    for stamp in ["2016-12-31 23:59:59 UTC", "2016-12-31 23:59:60 UTC", "2017-01-01 00:00:00 UTC"]:
        cases.append(("tzif-leap-hit-"+stamp,["--since="+stamp],leap))
    for suffix in ["", " UTC", " FUT", " OLD", " DAY"]:
        cases.append(("tzif-abbreviation"+suffix,["--since=2024-01-01 12:00:00"+suffix],historical))
    for suffix in [" FUT", " NEW", " DAY"]:
        cases.append(("tzif-abbreviation-seeded"+suffix,
                      ["--since=2024-01-01 12:00:00","--since=2024-01-01 12:00:00"+suffix],historical))
    cases.extend([
        ("tzif-utc-preserves-names", ["--since=2024-01-01 12:00:00 UTC","--since=2024-01-01 12:00:00 FUT"],historical),
        ("tzif-forced-dst-updates-names", ["--since=2024-01-01 12:00:00 DAY","--since=2024-01-01 12:00:00 DAY"],historical),
        ("tzif-named-restores-fresh-pair", ["--since=2024-01-01 12:00:00","--since=2024-01-01 12:00:00 Etc/UTC","--since=2024-01-01 12:00:00 FUT"],historical),
        ("tzif-no-transition-footer",["--since=2024-01-01 12:00:00"],footer),
        ("tzif-no-transition-utc",["--since=2024-01-01 12:00:00 UTC"],footer),
    ])
    for rule in ["ABC-99", "ABC-2junk", "<ABC>-99:99:99DEF,M3.2.0,M11.1.0",
                 "XYZ5ABC", "ABC-2DEF", "ABC65536", "ABC0:65536"]:
        cases.append(("tz-fallback-"+rule, ["--since=2023-11-15 00:00:00"], rule))
    results = []
    for name,args,tz in cases:
        env = {**os.environ,"LC_ALL":"C","TZ":tz,"SYSTEMD_COLORS":"0"}
        def execute(reader):
            return subprocess.run(  # nosec B603 - fixed binaries and generated argument vectors.
                [commands[reader],"--file",str(path),"--no-pager","--output=json",*args],
                capture_output=True,env=env,timeout=30,check=False)
        expected=execute("stock")
        def rows(data):
            return [json.loads(line) for line in data.splitlines() if line.strip()]
        for reader in ("go","rust"):
            actual=execute(reader)
            try: same=rows(actual.stdout)==rows(expected.stdout)
            except (ValueError,UnicodeDecodeError):same=False
            ok=actual.returncode==expected.returncode and same and bool(actual.stderr)==bool(expected.stderr)
            results.append({"test":"timestamp-"+name,"reader":reader,"status":"PASS" if ok else "FAIL",
                            "args":args,"tz":tz,"expected_returncode":expected.returncode,"returncode":actual.returncode,
                            "expected_rows":len(expected.stdout.splitlines()),"actual_rows":len(actual.stdout.splitlines()),
                            "expected_stderr":expected.stderr.decode(errors="replace")[-600:],
                            "stderr":actual.stderr.decode(errors="replace")[-600:]})
    # Parsing and rendering must share TZ/TZDIR/TZif calendar semantics.
    zone_dir = path.parent / "timestamp-zones"
    zone_dir.mkdir(exist_ok=True)
    write_timestamp_zone(zone_dir / "ReviewZone", [], [(-18000,0,"REV")])
    default_dir = path.parent / "timestamp-default-rules"
    default_dir.mkdir(exist_ok=True)
    write_timestamp_zone(default_dir / "posixrules",
                         [(1672531200,1),(1701388800,0),(1704067200,1),(1733011200,0)],
                         [(-18000,0,"STD"),(-14400,1,"DST")])
    flagged_dir = path.parent / "timestamp-flagged-rules"
    flagged_dir.mkdir(exist_ok=True)
    write_timestamp_zone(flagged_dir / "posixrules",
                         [(1672531200,1),(1701388800,0),(1704067200,1),(1733011200,0)],
                         [(-18000,0,"STD"),(-14400,1,"DST")],
                         standard=[1,1], universal=[0,1])
    rendering_zones = [
        ("posix", {"TZ":"EST5EDT,M3.2.0,M11.1.0"}),
        ("fixed-posix", {"TZ":"ABC-02"}),
        ("named", {"TZ":"America/New_York"}),
        ("directory", {"TZ":"ReviewZone", "TZDIR":str(zone_dir.resolve())}),
        ("leap", {"TZ":leap}), ("footer", {"TZ":footer}),
        ("historical", {"TZ":historical}),
        ("clamped-offset", {"TZ":"ABC-99"}),
        ("invalid-name", {"TZ":"Missing/Zone"}),
        ("empty-name", {"TZ":"<AB>"}),
        ("empty-setting", {"TZ":""}),
        ("default-dst", {"TZ":"XYZ5ABC"}),
        ("default-rebase", {"TZ":"ABC-2DEF"}),
        ("default-file", {"TZ":"ABC-2DEF", "TZDIR":str(default_dir.resolve())}),
        ("default-flags", {"TZ":"ABC-2DEF,", "TZDIR":str(flagged_dir.resolve())}),
        ("default-missing", {"TZ":"XYZ5ABC", "TZDIR":str(zone_dir.resolve())}),
        ("default-explicit", {"TZ":"XYZ5ABC,M3.2.0,M11.1.0", "TZDIR":str(default_dir.resolve())}),
    ]
    rendering_args = [["--output="+mode, "--no-hostname"] for mode in
                      ("short", "short-full", "short-iso", "short-iso-precise",
                       "short-precise", "short-unix", "verbose")]
    rendering_args += [["--list-boots"], ["--header"], ["--list-invocations", "-uclock.service"],
                       ["--since=2024-03-30 20:30:00", "--output=cat"],
                       ["--since=2024-03-31 00:30:00 UTC",
                        "--since=2024-03-30 20:30:00", "--output=cat"]]
    for name, settings in rendering_zones:
        for args in rendering_args:
            for utc in ([], ["--utc"]):
                env = {**os.environ, "LC_ALL":"C", "SYSTEMD_COLORS":"0",
                       "SYSTEMD_URLIFY":"0", **settings}
                def execute_render(reader):
                    return subprocess.run(  # nosec B603 - fixed binaries and synthetic inputs.
                        [commands[reader], "--file", str(path), "--no-pager", "-q",
                         "--all", "--lines=all", *args, *utc], capture_output=True,
                        env=env, timeout=30, check=False)
                expected = execute_render("stock")
                for reader in ("go", "rust"):
                    actual = execute_render(reader)
                    ok = (actual.returncode == expected.returncode
                          and actual.stdout == expected.stdout
                          and bool(actual.stderr) == bool(expected.stderr))
                    results.append({"test":"timestamp-render-"+name, "reader":reader,
                                    "status":"PASS" if ok else "FAIL", "args":args+utc,
                                    "expected_returncode":expected.returncode,
                                    "returncode":actual.returncode,
                                    "expected_output":expected.stdout.decode(errors="replace")[-600:] if not ok else "",
                                    "output":actual.stdout.decode(errors="replace")[-600:] if not ok else ""})
    return results
