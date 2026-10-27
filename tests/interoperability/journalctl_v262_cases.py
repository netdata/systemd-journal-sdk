"""Semantic v262 controls shared by both journalctl implementations."""

from __future__ import annotations

import json
import os
import re
import subprocess  # nosec B404
from pathlib import Path

from go_fixture_writer import write_journal_file
from journalctl_path_cases import run_file_path_cases
from journalctl_color_cases import run_color_cases
from journalctl_unit_cases import unit_invocation_cases


def write_v262_fixture(binary: Path, values: list[bytes | None] | None = None) -> Path:
    boot = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    repeated_message = values is None
    if values is None:
        values = [b"alpha alpha", b"first\nsecond", b"nul\x00tail", b"invalid\xffbyte", b"", None, "ÉABC".encode()]
    entries = []
    for i, value in enumerate(values):
        fields = [("ROW", str(i)), ("_BOOT_ID", boot)]
        if value is not None:
            fields.insert(0, ("MESSAGE", value))
        if i == 0 and repeated_message:
            fields.append(("MESSAGE", b"only-second-match"))
        entries.append({"realtime_usec": 1_700_005_000_000_000 + i,
                        "monotonic_usec": i + 1, "fields": fields})
    write_journal_file(binary, machine_id="00112233445566778899aabbccddeeff",
                       boot_id=boot, seqnum_id="12121212121212121212121212121212", entries=entries)

    return binary


def run_v262_cases(tools: dict[str, str], fixtures: dict[str, Path]) -> list[dict[str, object]]:
    """Compare status, complete bytes/JSON, and error classes with a pinned oracle."""
    env = {**os.environ, "LC_ALL": "C", "TZ": "UTC", "SYSTEMD_COLORS": "0", "COLUMNS": "80"}
    commands = {"stock": tools["stock_journalctl"],
                "go": tools["go_journalctl"], "rust": tools["rust_journalctl"]}

    def execute(reader: str, path: Path, args: list[str]):
        checkpoint = path.parent / ("v262-checkpoint-" + reader)
        has_checkpoint = "--cursor-file=@CHECKPOINT@" in args
        if has_checkpoint:
            checkpoint.write_bytes(b"")
            args = ["--cursor-file=" + str(checkpoint) if arg == "--cursor-file=@CHECKPOINT@" else arg
                    for arg in args]
        result = subprocess.run(  # nosec B603 - fixed binaries and argument vectors.
            [commands[reader], "--file", str(path), "--no-pager", *args],
            env=env, capture_output=True, timeout=30, check=False,
        )
        if has_checkpoint:
            result.checkpoint = checkpoint.read_bytes()
        return result

    binary = write_v262_fixture(fixtures["file"].parent / "v262-bytes.journal")
    shortened = write_v262_fixture(fixtures["file"].parent / "v262-shortened.journal", [
        b"a" * 40 + b"\n" + b"b" * 40, b"a" * 140 + b"TAIL012345",
        ("界" * 50 + "TAIL012345").encode(), b"one\ntwo\nthree\nfour",
        ("e\u0301" * 70 + "TAIL012345").encode(),
        b"trailing newline\n",
        b"two trailing newlines\n\n", b"nul\0tail\nNEXT",
    ])
    boots = fixtures["file"].parent / "v262-grep-boots.journal"
    write_journal_file(boots, machine_id="00112233445566778899aabbccddeeff",
                       boot_id="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", seqnum_id="12121212121212121212121212121212",
                       entries=[{"realtime_usec": 1_700_005_000_000_000 + i,
                                 "monotonic_usec": 1, "boot_id": ch * 32,
                                 "fields": [("MESSAGE", message), ("_BOOT_ID", ch * 32)]}
                                for i, (ch, message) in enumerate(zip("abc", ["hit-a", "miss", "hit-c"]))])
    labels = fixtures["file"].parent / "v262-labels.journal"
    prepared_values = [b"\x1b[31mred\x1b[0m", b"a\tb", b"line\r\r", b"a\r\nb",
                       b"a\rb", b"\x1b]8;;https://example.org\x07link\x1b]8;;\x1b\\",
                       b"\x1b[31", b"\x1b[2J", b"a\t\nsecond", b"one\r\nnext\r", b"nul\0tail"]
    prepared = write_v262_fixture(fixtures["file"].parent / "v262-prepared.journal", prepared_values)
    duplicate_binary = fixtures["file"].parent / "v262-duplicate-binary.journal"
    write_journal_file(duplicate_binary, machine_id="00112233445566778899aabbccddeeff",
                       boot_id="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", seqnum_id="12121212121212121212121212121212",
                       entries=[{"realtime_usec": 1_700_005_000_000_000 + i, "monotonic_usec": i + 1,
                                 "fields": [("MESSAGE", value) for value in values]}
                                for i, values in enumerate([[b"\xff", b"second"], [b"\xff", b"\xfe"],
                                                            [b"first", b"\xff"], [b"\xff", b"middle", b"last"]])])
    label_fields = [
        [("_EXE", "/bin/example")],
        [("SYSLOG_IDENTIFIER", ""), ("_COMM", "worker")],
        [("SYSLOG_IDENTIFIER", "first"), ("SYSLOG_IDENTIFIER", "second")],
        [("_SYSTEMD_UNIT", "alpha.service"), ("_SYSTEMD_USER_UNIT", "beta.service")],
        [("SYSLOG_IDENTIFIER", b"bad\xff"), ("_COMM", "worker")],
        [("SYSLOG_IDENTIFIER", "x" * 300), ("_COMM", "worker")],
    ]
    write_journal_file(labels, machine_id="00112233445566778899aabbccddeeff",
                       boot_id="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", seqnum_id="12121212121212121212121212121212",
                       entries=[{"realtime_usec": 1_700_005_000_000_000 + i, "monotonic_usec": i + 1,
                                 "fields": [("MESSAGE", "line"), ("ROW", str(i)), *fields]}
                                for i, fields in enumerate(label_fields)])

    source_cases = [
        ("duplicate", [b"1700000000000000", b"1700000001000000"]),
        ("u64-max", [b"18446744073709551615"]),
        ("limit-valid", [str((1 << 55) - 1).encode()]),
        ("limit-invalid", [str(1 << 55).encode()]),
        ("zero", [b"0"]), ("invalid-last", [b"1700000000000000", b"bad"]),
        ("nul", [b"1700000000000000\0trailer"]),
        ("hex", [b"0x60a24181e4000"]), ("padding", [b" +1700000000000000"]),
    ]
    source = fixtures["file"].parent / "v262-source-realtime.journal"
    write_journal_file(source, machine_id="00112233445566778899aabbccddeeff",
                       boot_id="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", seqnum_id="12121212121212121212121212121212",
                       entries=[{"realtime_usec": 1_700_005_000_000_000 + i,
                                 "monotonic_usec": 1_000_001 + i,
                                 "fields": [("MESSAGE", "source-" + label), ("ROW", label),
                                            *[("_SOURCE_REALTIME_TIMESTAMP", value) for value in values]]}
                                for i, (label, values) in enumerate(source_cases)])
    verified = execute("stock", source, ["--verify"])
    if verified.returncode != 0:
        raise AssertionError("stock source-realtime fixture verification failed: "
                             + verified.stderr.decode(errors="replace"))

    # mode is exact bytes, parsed JSON (key order is unspecified), or an error class.
    cases: list[tuple[str, Path, list[str], str]] = []
    cases.extend(unit_invocation_cases(fixtures["file"].parent))
    path = fixtures["file"]
    unit_glob = path.parent / "v262-unit-glob.journal"
    write_journal_file(unit_glob, machine_id="00112233445566778899aabbccddeeff",
                       boot_id="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", seqnum_id="12121212121212121212121212121212",
                       entries=[{"realtime_usec": 1_700_005_000_000_000, "monotonic_usec": 1,
                                 "fields": [("MESSAGE", "unit-glob-marker"), ("_SYSTEMD_UNIT", "alpha.service"),
                                            ("__JOURNALCTL_NEVER_MATCH", "unit-glob")]}])
    for name, args in [
        ("system-empty", ["--unit=no-such-*"]),
        ("user-empty", ["--user-unit=no-such-*"]),
        ("both-empty", ["--unit=no-such-*", "--user-unit=no-such-*"]),
        ("zero-lines", ["--unit=no-such-*", "-n0"]),
    ]:
        cases.append(("unit-glob-" + name, unit_glob, args,
                      "error:failed to add filter for units: no data available"))
    for name, args in [
        ("system-exact-missing", ["--unit=absent.service"]),
        ("user-exact-missing", ["--user-unit=absent.service"]),
        ("mixed-system-exact", ["--unit=absent.service", "--unit=no-such-*"]),
        ("system-exact-user-empty", ["--unit=alpha.service", "--user-unit=no-such-*"]),
        ("user-exact-system-empty", ["--user-unit=absent.service", "--unit=no-such-*"]),
    ]:
        cases.append(("unit-glob-" + name, unit_glob, [*args, "-ocat"], "bytes"))
    for label, _ in source_cases:
        for mode in ("short-iso-precise", "short-monotonic", "verbose"):
            cases.append((f"source-realtime-{label}-{mode}", source,
                          ["--utc", "-q", "ROW=" + label, "-o", mode], "bytes"))
    for i in range(len(prepared_values)):
        for extra in ([], ["--all"], ["--truncate-newline"]):
            cases.append((f"short-prepare-{i}-{extra}", prepared,
                          ["--utc", "-q", f"ROW={i}", *extra], "bytes"))
    for mode in ("json", "json-pretty", "json-sse", "json-seq"):
        cases.append(("duplicate-binary-" + mode, duplicate_binary, ["-o", mode], mode))
    for value, valid in [("1G512M", True), ("1.5G512M", True), ("10.M", True),
                         ("+1K", True), ("1 K 2 B", True), ("1K1K", False),
                         ("1M1G", False), ("1KiB", False), ("1g", False),
                         ("1KB", False), ("-0", False), (".5K", False),
                         ("18446744073709551615", True), ("18446744073709551616", False),
                         ("16E", False), ("15E", True), ("1B2", True)]:
        cases.append(("vacuum-size-native-" + value, path, ["--vacuum-size=" + value, "--help"],
                      "terminal" if valid else "bytes"))
    for grep in ("hit", "unmatched"):
        for extra in ([], ["-r"], ["-n1"], ["-n+1"], ["-n0"], ["-n+0"],
                      ["--until=@1700005000.000001"], ["-r", "--since=@1700005000.000001"]):
            cases.append((f"cursor-position-{grep}-{extra}", boots,
                          ["--grep=" + grep, "-ocat", "--show-cursor", *extra], "bytes"))
    for extra in (["-n0"], ["-n+0"]):
        cases.append(("cursor-zero-position-" + str(extra), path,
                      ["-ocat", "--show-cursor", *extra], "bytes"))
    for extra in ([], ["-r"], ["-n0"], ["-n+0"], ["-n10", "--until=@1700005000.000001"]):
        for grep in ([], ["--grep=hit"], ["--grep=unmatched"]):
            cases.append((f"cursor-file-position-{extra}-{grep}", boots,
                          ["-ocat", "--cursor-file=@CHECKPOINT@", *grep, *extra], "checkpoint"))
    for i in range(len(label_fields)):
        for extra in ([], ["--all"]):
            cases.append((f"short-label-{i}-{extra}", labels,
                          ["--utc", "-q", "-owith-unit" if i == 3 else "-oshort", f"ROW={i}", *extra], "bytes"))
    for excluded in ("first", "second"):
        cases.append((f"exclude-last-identifier-{excluded}", labels,
                      ["--utc", "-q", "-oshort", "ROW=2", "-T" + excluded], "bytes"))
    for i in range(8):
        cases.append((f"shortened-line-{i}", shortened,
                      ["--no-full", "--utc", "--no-hostname", "-q", f"ROW={i}"], "bytes"))
    for extra in ([], ["-r"], ["-q"], ["--merge"], ["--grep=hit-a"]):
        cases.append((f"grep-boot-events-{extra}", boots,
                      ["--boot=all", "--grep=hit", "--utc", "-oshort", *extra], "bytes"))
    for value in ("ERROR", "panic", "critical", "error", "warn", "err", "warning", "0x3"):
        cases.append((f"priority-native-{value}", path, [f"--priority={value}", "--help"],
                      "terminal" if value in ("err", "warning", "0x3") else "bytes"))
    for value in ("24", "127", "128", "0x10"):
        cases.append((f"facility-native-{value}", path, [f"--facility={value}", "--help"],
                      "bytes" if value == "128" else "terminal"))
    for value in ("daemon, cron", " daemon", "daemon ", "daemon, 9", "daemon, help",
                  "daemon,,cron", "daemon,help"):
        cases.append((f"facility-spacing-{value!r}", path, [f"--facility={value}", "--help"],
                      "terminal" if value in ("daemon, 9", "daemon,,cron", "daemon,help") else "bytes"))
    for option, value, valid in [
        ("lines", "0x2", True), ("lines", "0b10", True), ("lines", "0o10", True),
        ("lines", " 2", True), ("lines", "2 ", False), ("lines", "1_0", False),
        ("boot", "0x1", True), ("boot", "2147483648", False),
        ("boot", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1", False),
        ("boot", "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa", False),
        ("invocation", "2147483648", False), ("vacuum-files", "0x1", True),
        ("vacuum-files", "-0", True),
    ]:
        cases.append((f"integer-{option}-{value!r}", path, [f"--{option}={value}", "--help"],
                      "terminal" if valid else "bytes"))
    cases.extend([
        ("short-repeated-message", binary, ["--grep=alpha", "--utc", "-oshort"], "bytes"),
        ("short-full-trailing-newline", shortened, ["ROW=5", "--utc", "-q"], "bytes"),
        ("short-full-trailing-blank", shortened, ["ROW=6", "--utc", "-q"], "bytes"),
        ("short-all-nul-lines", shortened, ["ROW=7", "--utc", "-q", "--all"], "bytes"),
        ("lines-octal-count", fixtures["pager"], ["-n010", "-ocat"], "bytes"),
        ("pager-explicit-all", fixtures["pager"], ["-e", "-nall", "-ocat"], "bytes"),
        ("pager-grep-direction", fixtures["pager"], ["-e", "--grep=pager-100[234]", "-ocat"], "bytes"),
        ("cursor-file-empty-path", path, ["--cursor-file=", "-ocat"], "bytes"),
        ("cursor-file-empty-before-help", path, ["--cursor-file=", "--help"], "terminal"),
        ("verify-quiet", path, ["--verify", "-q"], "bytes"),
        ("verify-implied-quiet", path, ["--verify", "-ocat"], "bytes"),
    ])
    for args in (["-F", "PRIORITY", "-n1"], ["-F", "PRIORITY", "-n0"],
                 ["-N", "-n0"], ["-F", "PRIORITY", "-N"],
                 ["-N", "-F", "PRIORITY"], ["--sync", "-N"],
                 ["--no-tail", "-n1", "-o", "cat"],
                 ["-qq", "-o", "json", "-o", "cat"],
                 ["--output-fields=PRIORITY", "--output-fields=MESSAGE", "-ojson"],
                 ["--output-fields=", "-ocat"],
                 ["--output-fields=MESSAGE", "--output-fields=", "-ocat"],
                 ["--system", "--user", "--unit=alpha.service", "-ocat"],
                 ["-p3", "-p6", "-ocat"], ["--grep", "-n1", "-ocat"]):
        cases.append(("options-" + str(len(cases)), path, list(args), "json" if "-ojson" in args else "fields" if "-N" in args and ("-F" not in args or args.index("-N") > args.index("-F")) else "bytes"))
    for mode in ("json", "json-pretty", "json-sse", "json-seq"):
        for action in ("--list-boots", "--list-invocations"):
            extra = ["--unit=alpha.service"] if action == "--list-invocations" else []
            cases.append((f"table-{action}-{mode}", path, [action, *extra, "-o", mode], mode))
    for action in ("--list-boots", "--list-invocations"):
        extra = ["--unit=alpha.service"] if action == "--list-invocations" else []
        for output in ([], ["-q"], ["-ojson"]):
            cases.append((f"table-zero-{action}-{output}", path,
                          [action, *extra, "-n0", *output], "bytes"))
    for option, value in (("case-sensitive", "true"), ("synchronize-on-exit", "false")):
        for padded in (" " + value, value + " "):
            cases.append((f"boolean-space-{option}-{padded!r}", path,
                          [f"--{option}={padded}", "--help"], "bytes"))
    for field, limit in (("", []), ("", ["-n0"]), (" MESSAGE ", []), ("__CURSOR", [])):
        cases.append((f"invalid-field-{field!r}-{limit}", path,
                      ["-F", field, *limit], "bytes"))
    for args, error in [
        (["-F", "PRIORITY", "-S", "@0"], "cannot be combined"),
        (["-N", "-U", "@infinity"], "cannot be combined"),
        (["-N", "--grep="], "cannot be combined"),
        (["-N", "--cursor="], "cannot be combined"),
        (["-S", "", "--help"], "Failed to parse timestamp"),
        (["-pBAD", "--help"], "Unknown log level"),
        (["-oBAD", "-ocat"], "Unknown output format"),
        (["--case-sensitive=TRUE", "--case-sensitive="], "Bad --case-sensitive"),
        (["--quiet=true"], "doesn't allow an argument"),
        (["+", "MESSAGE=file-a"], "can only be used between terms"),
        (["MESSAGE=file-a", "+"], "can only be used between terms"),
        (["--grep=(", "-N"], "cannot be combined"),
        (["--grep=(", "--list-boots"], "pattern"),
    ]:
        cases.append(("error-" + str(len(cases)), path, args, "error:" + error))
    for args in (["--help", "-pBAD"], ["-ohelp", "--bad-option"],
                 ["--facility=help", "--bad-option"], ["--grep=(", "--grep=file", "-ocat"],
                 ["-N", "--boot=all"], ["-N", "--invocation=all"],
                 ["--since=2023-11-14 00:00:00 UTC", "-ocat"]):
        # Help text has implementation-specific descriptions; status and later-input
        # rejection are the contract, while the shared output-mode/facility lists are exact.
        mode = "terminal" if args[0] == "--help" else "fields" if "-N" in args else "bytes"
        cases.append(("order-" + str(len(cases)), path, list(args), mode))
    for pattern in (r"(?=alpha)alpha", r"(alpha) \1", r"nul\x00tail", r"\xff", "Éabc", "",
                    "only-second-match", "unmatched-pattern", "(*UTF)."):
        cases.append(("grep-" + str(len(cases)), binary, ["--grep=" + pattern, "-ocat"], "bytes"))
    if os.name != "nt":
        for option in ("root", "image"):
            cases.append(("raw-source-presence-" + option, path,
                          ["--" + option + "=\udcff", "-ocat"], "bytes"))
        cases.append(("grep-raw-argv", binary, ["--grep=\udcff", "-ocat"], "bytes"))
        cases.append(("match-raw-argv", binary, ["MESSAGE=invalid\udcffbyte", "-ocat"], "bytes"))
        cases.append(("match-raw-disjunction", binary,
                      ["ROW=0", "+", "MESSAGE=invalid\udcffbyte", "-ocat"], "bytes"))
        for args in (["--identifier=bad\udcff", "-ocat"],
                     ["--exclude-identifier=bad\udcff", "-oshort", "ROW=4"],
                     ["--output-fields=MES\udcff", "-ojson", "ROW=4"],
                     ["--output-fields=MES\udcff", "-ocat", "ROW=4"]):
            cases.append(("opaque-raw-" + str(len(cases)), labels, args,
                          "json" if "-ojson" in args else "bytes"))
    for output_mode in ("json", "json-pretty", "json-sse", "json-seq"):
        cases.append(("introspection-"+output_mode, path, ["-o",output_mode,"--introspect-cli","--bad-option"], "introspection:"+output_mode))
    cases.extend([
        ("quiet-sticky", path, ["-ojson","-oshort","--grep=unmatched-pattern"], "bytes"),
        ("binary-cat", binary, ["-ocat"], "bytes"),
        ("binary-cat-truncate", binary, ["-ocat", "--truncate-newline"], "bytes"),
        ("grep-no-lines", binary, ["--grep=alpha", "-n0", "-ocat"], "bytes"),
        ("grep-head-stops-at-limit", binary, ["--grep=(*UTF).", "-n+3", "-ocat"], "bytes"),
        ("grep-head-zero-skips-match", binary,
         ["--grep=(*UTF).", "-n+0", "--since=@1700005000.000003", "-ocat"], "bytes"),
        ("grep-no-match-short", binary, ["--grep=unmatched-pattern"], "bytes"),
        ("grep-render-exclusion", path, ["--grep=file", "-Tapp-a", "-Tapp-b", "-nall"], "bytes"),
        ("projection-spaces-json", binary, ["-ojson", "--output-fields= MESSAGE ", "-n1"], "json"),
        ("projection-spaces-cat", binary, ["-ocat", "--output-fields= MESSAGE "], "bytes"),
        ("projection-invalid-no-rows", binary, ["-ocat", "--output-fields= MESSAGE ", "-n0"], "bytes"),
        ("lines-oldest-zero", path, ["-n+0"], "bytes"),
        ("lines-negative-zero", path, ["-n-0"], "bytes"),
        ("cursor-empty", path, ["--cursor=", "-n1"], "bytes"),
        ("after-cursor-empty", path, ["--after-cursor=", "-n1"], "bytes"),
        ("directory-empty-source-conflict", path, ["--directory=", "--new-id128"], "bytes"),
        ("machine-empty-source-conflict", path, ["--machine=", "--new-id128"], "bytes"),
        ("root-empty-clear", path, ["--root=", "-n0"], "bytes"),
        ("image-empty-clear", path, ["--image=", "-n0"], "bytes"),
        ("namespace-empty-clear", path, ["--namespace=", "-n0"], "bytes"),
        ("vacuum-action-replaced", path, ["--vacuum-size=1", "--new-id128"], "utility-status"),
        ("vacuum-zero-action-replaced", path, ["--vacuum-time=0", "--new-id128"], "utility-status"),
    ])
    probe = execute("stock", path, ["-ojson"])
    if probe.returncode != 0:
        raise AssertionError("stock cursor probe failed: " + probe.stderr.decode(errors="replace"))
    rows = [json.loads(row) for row in probe.stdout.splitlines()]
    for reverse, row in ((False, rows[-1]), (True, rows[0])):
        for extra in ([], ["-n0"]):
            cases.append((f"cursor-after-terminal-{reverse}-{extra}", path,
                          ["--after-cursor=" + row["__CURSOR"], "--show-cursor", "-ocat",
                           *(["-r"] if reverse else []), *extra], "bytes"))
    for cursor_option in ("--cursor", "--after-cursor"):
        for reverse in (False, True):
            extra = ["--reverse"] if reverse else []
            cases.append((f"{cursor_option}-one-{reverse}", path,
                          [cursor_option, rows[1]["__CURSOR"], "-n1", "-ojson", *extra], "json"))
    directional = fixtures["file"].parent / "v262-directional.journal"
    write_journal_file(directional, machine_id="00112233445566778899aabbccddeeff",
                       boot_id="a" * 32, seqnum_id="12" * 16,
                       entries=[{"realtime_usec": 1700005000000000 + i // 2,
                                 "monotonic_usec": i % 2 + 1, "boot_id": "abc"[i // 2] * 32,
                                 "fields": [("MESSAGE", "keep" if i % 2 else "other"),
                                            ("ROW", str(i)), ("_BOOT_ID", "abc"[i // 2] * 32)]}
                                for i in range(6)])
    directional_probe = execute("stock", directional, ["-ojson"])
    if directional_probe.returncode:
        raise AssertionError("directional cursor oracle fixture failed")
    cursor_rows = [json.loads(line) for line in directional_probe.stdout.splitlines()]
    anchors = []
    for row in cursor_rows:
        parts = dict(part.split("=", 1) for part in row["__CURSOR"].split(";"))
        anchors.extend([row["__CURSOR"], "t=" + parts["t"],
                        "s=" + parts["s"] + ";i=" + parts["i"],
                        "b=" + parts["b"] + ";m=" + parts["m"]])
    anchors.extend(["s=" + "f" * 32 + ";i=3;m=0", "b=" + "f" * 32 + ";m=0",
                    "s=" + "12" * 16 + ";i=ffffffff", "b=" + "b" * 32 + ";m=0",
                    "b=" + "b" * 32 + ";m=ffffffff",
                    "s=" + "12" * 16 + ";i=1;t=ffffffff"])
    for index, anchor in enumerate(anchors):
        for reverse in (False, True):
            for option in ("--cursor", "--after-cursor"):
                for filtered in (False, True):
                    cases.append((f"directional-{index}-{reverse}-{option}-{filtered}", directional,
                                  [option + "=" + anchor, "-ojson", *(["-r"] if reverse else []),
                                   *(["MESSAGE=keep"] if filtered else [])], "json"))
    results = []
    for name, fixture, args, mode in cases:
        expected = execute("stock", fixture, args)
        if mode.startswith("error:"):
            needle = mode.split(":", 1)[1].lower()
            if expected.returncode == 0 or needle not in expected.stderr.decode(errors="replace").lower():
                raise AssertionError(f"stock {name} did not produce {needle!r}: {expected}")
        for reader in ("go", "rust"):
            actual = execute(reader, fixture, args)
            if mode.startswith("error:"):
                same = (mode.split(":", 1)[1].lower() in actual.stderr.decode(errors="replace").lower()
                        and actual.stdout == expected.stdout)
            elif mode.startswith("introspection:"):
                frame = mode.split(":",1)[1]
                a = parse_json(frame, actual.stdout)
                e = parse_json(frame, expected.stdout)
                if isinstance(a,list) and isinstance(e,list):
                    a = a[0] if len(a)==1 else None
                    e = e[0] if len(e)==1 else None
                def surface(value):
                    if not isinstance(value,dict) or value.get("mediaType") != "application/vnd.io.systemd.cli-introspection-0":
                        return None
                    commands = value.get("commands",[])
                    if len(commands)!=1 or commands[0].get("names") != ["journalctl"]:
                        return None
                    command = commands[0]
                    if not command.get("project") or not command.get("version") or not isinstance(command.get("features"),list):
                        return None
                    return [{k:opt[k] for k in ("names","argument","metavar","group") if k in opt}
                            for opt in command.get("options",[])]
                same = surface(a) is not None and surface(a) == surface(e)
            elif mode == "diagnostic-bytes":
                same = actual.stdout == expected.stdout and actual.stderr == expected.stderr
            elif mode == "fields":
                same = sorted(actual.stdout.splitlines()) == sorted(expected.stdout.splitlines())
            elif mode == "terminal":
                same = bool(actual.stdout) and not actual.stderr
            elif mode == "utility-status":
                same = bool(actual.stdout) == bool(expected.stdout)
            elif mode == "checkpoint":
                same = actual.stdout == expected.stdout and actual.checkpoint == expected.checkpoint
            elif mode.startswith("json"):
                same = parse_json(mode, actual.stdout) == parse_json(mode, expected.stdout)
                if mode == "json-pretty":
                    separators = rb'"(?:\\.|[^"\\])*"([ \t]*:[ \t]*)'
                    same = same and re.findall(separators, actual.stdout) == re.findall(separators, expected.stdout)
            else:
                same = actual.stdout == expected.stdout
            # A nonzero grep result must preserve empty stderr; actual match errors
            # must remain errors rather than being mistaken for a no-match result.
            stderr_ok = bool(actual.stderr) == bool(expected.stderr)
            ok = actual.returncode == expected.returncode and same and stderr_ok
            results.append({"test": "v262-" + name, "reader": reader,
                            "status": "PASS" if ok else "FAIL", "args": args,
                            "expected_returncode": expected.returncode, "returncode": actual.returncode,
                            "expected": expected.stdout.hex(), "actual": actual.stdout.hex(),
                            "expected_stderr": expected.stderr.decode(errors="replace")[-1000:],
                            "stderr": actual.stderr.decode(errors="replace")[-1000:]})
            if mode == "checkpoint":
                results[-1].update(checkpoint=actual.checkpoint.hex(), expected_checkpoint=expected.checkpoint.hex())
    if os.name != "nt":
        args = ["--namespace=\udcff", "-ocat"]
        native = execute("stock", path, args)
        if native.returncode != 0:
            raise AssertionError("native namespace control with explicit file changed")
        for reader in ("go", "rust"):
            actual = execute(reader, path, args)
            diagnostic = actual.stderr.decode(errors="replace")
            ok = actual.returncode == 1 and not actual.stdout and "portable mode does not support --namespace" in diagnostic
            results.append({"test": "v262-raw-namespace-portable-qualification", "reader": reader,
                            "status": "PASS" if ok else "FAIL", "args": args,
                            "contract": "explicit portable unsupported; native ignores namespace with --file",
                            "native_returncode": native.returncode, "expected_returncode": 1,
                            "returncode": actual.returncode, "stderr": diagnostic})
    if os.name == "posix":
        results.extend(run_terminal_width_cases(commands, shortened, env))
        results.extend(run_file_path_cases(commands, path, env))
    results.extend(run_color_cases(commands, path.parent, env))
    return results


def run_terminal_width_cases(commands: dict[str, str], fixture: Path,
                             env: dict[str, str]) -> list[dict[str, object]]:
    """A real stdout terminal must supply the width when COLUMNS is unusable."""
    import errno
    import fcntl
    import pty
    import struct
    import termios

    results = []
    for columns in (None, "invalid", "100", "65536"):
        terminal_env = {**env}
        if columns is None:
            terminal_env.pop("COLUMNS", None)
        else:
            terminal_env["COLUMNS"] = columns
        observed = {}
        for reader, command in commands.items():
            master, slave = pty.openpty()
            output = bytearray()
            with os.fdopen(master, "rb", buffering=0) as terminal:
                try:
                    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 120, 0, 0))
                    attrs = termios.tcgetattr(slave)
                    attrs[1] &= ~termios.ONLCR
                    termios.tcsetattr(slave, termios.TCSANOW, attrs)
                    # One bounded row fits in the PTY buffer before it is drained.
                    result = subprocess.run(  # nosec B603 - fixed binaries and fixture arguments.
                        [command, "--file", str(fixture), "--no-pager", "--utc", "--no-hostname",
                         "--no-full", "-q", "ROW=1"], env=terminal_env,
                        stdout=slave, stderr=subprocess.PIPE, timeout=30, check=False,
                    )
                finally:
                    os.close(slave)
                while True:
                    try:
                        chunk = terminal.read(4096)
                    except OSError as err:
                        if err.errno == errno.EIO:
                            break
                        raise
                    if not chunk:
                        break
                    output.extend(chunk)
            observed[reader] = (result.returncode, bytes(output), result.stderr)
        expected = observed["stock"]
        if expected[0] != 0 or expected[2] or not expected[1]:
            raise AssertionError(f"stock terminal-width control failed: {expected!r}")
        for reader in ("go", "rust"):
            actual = observed[reader]
            results.append({"test": "v262-tty-columns-" + str(columns), "reader": reader,
                            "status": "PASS" if actual == expected else "FAIL",
                            "returncode": actual[0], "expected_returncode": expected[0],
                            "actual": actual[1].hex(), "expected": expected[1].hex(),
                            "stderr": actual[2].decode(errors="replace"),
                            "expected_stderr": expected[2].decode(errors="replace")})
    return results


def parse_json(mode: str, data: bytes):
    try:
        if mode == "json-sse":
            return [json.loads(line[6:]) for line in data.splitlines() if line.startswith(b"data: ")]
        if mode == "json-seq":
            return [json.loads(part) for part in data.split(b"\x1e") if part.strip()]
        if mode == "json-pretty":
            decoder = json.JSONDecoder()
            text, values = data.decode().lstrip(), []
            while text:
                value, end = decoder.raw_decode(text)
                values.append(value)
                text = text[end:].lstrip()
            return values
        return [json.loads(line) for line in data.splitlines() if line.strip()]
    except (ValueError, UnicodeDecodeError):
        return {"invalid_json": data.hex()}
