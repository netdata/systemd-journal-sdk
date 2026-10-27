"""Unit naming and invocation traversal controls for the v262 oracle matrix."""

from __future__ import annotations

import os
from pathlib import Path

from go_fixture_writer import write_journal_file


def unit_invocation_cases(directory: Path) -> list[tuple[str, Path, list[str], str]]:
    boot = "a" * 32
    iid = "1" * 32
    uid = str(os.getuid()) if hasattr(os, "getuid") else "0"
    cases = []

    def fixture(name, rows):
        path = directory / ("v262-unit-" + name + ".journal")
        write_journal_file(path, machine_id="00112233445566778899aabbccddeeff",
                           boot_id=boot, seqnum_id="12121212121212121212121212121212",
                           entries=[{"realtime_usec": 1_700_007_000_000_000 + (0 if name == "same-time" else i),
                                     "monotonic_usec": i + 1,
                                     "fields": [("MESSAGE", str(i)), ("_BOOT_ID", boot), *fields]}
                                    for i, fields in enumerate(rows)])
        return path

    names = [
        (".service", ".service.service"), ("@instance.service", "@instance.service.service"),
        ("a" * 256 + ".service", "a" * 256 + ".service.service"),
        ("[[.a.]]lpha.*", "alpha.service"), ("[[]", "["),
        ("alpha", "alpha.service"), ("alpha.*", "alpha.service"),
        ("app-*", "app-alpha.slice"), ("[[:alpha:]]lpha.service", "alpha.service"),
        ("[!b]lpha.service", "alpha.service"), ("[a-c]lpha.service", "alpha.service"),
        ("alpha[", "alpha["), (r"alpha\x20name", r"alpha\x20name.service"),
        ("/dev/sda", "dev-sda.device"), ("//sys//devices/./example", "sys-devices-example.device"),
        ("/mnt/data", "mnt-data.mount"), ("/", "-.mount"),
        ("/../mnt//data/.", "mnt-data.mount"), ("/.dot-dir", r"\x2edot\x2ddir.mount"),
        ("/mnt/part-one", r"mnt-part\x2done.mount"),
        ("/mnt/../data", "-mnt-..-data.service"),
        ("alpha name", r"alpha\x20name.service"), (" alpha ", r"\x20alpha\x20.service"),
        ("café", r"caf\xc3\xa9.service"), ("/café", r"caf\xc3\xa9.mount"),
        ("@instance", "@instance.service"), ("alpha@.service", "alpha@.service"),
        ("/waldo" + "a" * 246, "waldo" + "a" * 227 + "_7736d9ed33c2ec55.mount"),
    ]
    if os.name != "nt":
        names += [("bad-\udcff", r"bad-\xff.service"), ("/bad-\udcff", r"bad\x2d\xff.mount")]
    path = fixture("names", [[("_SYSTEMD_UNIT", target), ("_SYSTEMD_USER_UNIT", target),
                               ("_UID", uid), ("_SYSTEMD_INVOCATION_ID", iid)] for _, target in names])
    for i, (argument, _) in enumerate(names):
        for option in ("--unit", "--user-unit"):
            cases.append((f"unit-name-{i}-{option}", path, [option + "=" + argument, "-ocat"], "bytes"))
    for argument in ["alpha name", "café", *(["bad-\udcff"] if os.name != "nt" else [])]:
        for quiet in ([], ["-q"]):
            cases.append(("unit-diagnostic-" + argument + str(quiet), path,
                          ["--unit=" + argument, "--utc", "--no-hostname", *quiet], "diagnostic-bytes"))
    for option in ("--unit", "--user-unit"):
        cases.append(("unit-empty-" + option, path, [option + "=", "-ocat"], "error:invalid argument"))
        cases.append(("unit-empty-invocation-" + option, path,
                      [option + "=", "--list-invocations", "-ojson"], "error:invalid argument"))
    cases.append(("unit-converted-user-dedup", path,
                  ["--user", "-ualpha", "-ualpha", "--list-invocations", "-ojson"], "json"))
    cases.append(("unit-explicit-user-duplicates", path,
                  ["--user-unit=alpha", "--user-unit=alpha", "--list-invocations", "-ojson"],
                  "error:with multiple units"))
    for scope, field, extra in [
        ("system-coredump", "COREDUMP_UNIT", [("MESSAGE_ID", "fc2e22bc6ee647b6b90729ab34a250b1")]),
        ("system-slice", "_SYSTEMD_SLICE", []),
        ("user-coredump", "COREDUMP_USER_UNIT", [("_UID", uid)]),
        ("user-slice", "_SYSTEMD_USER_SLICE", [("_UID", uid)]),
    ]:
        unit = "crash-alpha.service" if "coredump" in scope else "app-alpha.slice"
        path = fixture(scope, [[(field, unit), ("_SYSTEMD_INVOCATION_ID", iid), *extra]])
        option = "--user-unit=" if scope.startswith("user") else "--unit="
        cases.append(("unit-full-show-" + scope, path, [option + unit, "-ocat"], "bytes"))
        for pattern in (unit, "crash-*" if "coredump" in scope else "*.slice"):
            cases.append(("invocation-base-" + scope + pattern, path,
                          [option + pattern, "--list-invocations", "-ojson"],
                          "error:no matching unit" if "*" in pattern else "bytes"))

    bracket_invocations = fixture("bracket-invocations", [
        [("_SYSTEMD_UNIT", "alpha.service"), ("_SYSTEMD_USER_UNIT", "alpha.service"),
         ("_UID", uid), ("_SYSTEMD_INVOCATION_ID", digit * 32)] for digit in "12"
    ])
    for option in ("--unit", "--user-unit"):
        cases.append(("unit-bracket-invocations-" + option, bracket_invocations,
                      [option + "=[[:alpha:]]lpha.service", "--list-invocations", "-ojson"], "json"))
        cases.append(("unit-bracket-select-" + option, bracket_invocations,
                      [option + "=[[.a.]]lpha.service", "--invocation=0", "-ocat"], "bytes"))

    raw_unit = fixture("raw-value", [[("_SYSTEMD_UNIT", b"bad-\xff.service")]])
    cases.append(("unit-wildcard-raw-value", raw_unit, ["--unit=bad-*", "-ocat"], "bytes"))
    for name, rows in [
        ("no-id", [[("_SYSTEMD_UNIT", "alpha.service")] for _ in range(16)]),
        ("same-time", [[("_SYSTEMD_UNIT", "alpha.service"), ("_SYSTEMD_INVOCATION_ID", c * 32)]
                       for c in "312"]),
        ("bounds", [[("_SYSTEMD_INVOCATION_ID", iid)],
                    [("_SYSTEMD_UNIT", "alpha.service"), ("_SYSTEMD_INVOCATION_ID", iid),
                     ("SYSLOG_IDENTIFIER", "selected"), ("PRIORITY", "6"), ("SYSLOG_FACILITY", "3")],
                    [("_SYSTEMD_INVOCATION_ID", iid)]]),
        ("precedence", [[("_SYSTEMD_UNIT", "alpha.service"), ("_SYSTEMD_INVOCATION_ID", iid),
                         ("OBJECT_SYSTEMD_INVOCATION_ID", "2" * 32)],
                        [("_SYSTEMD_UNIT", "alpha.service"), ("_SYSTEMD_INVOCATION_ID", "3" * 32)]]),
        ("overlap", [[("_SYSTEMD_UNIT", "alpha.service"), ("_SYSTEMD_INVOCATION_ID", value * 32)]
                     for value in "121343"]),
        ("nested", [[("_SYSTEMD_UNIT", "alpha.service"), ("_SYSTEMD_INVOCATION_ID", value * 32)]
                    for value in "12321"]),
        ("broken-before-unit", [[("_SYSTEMD_UNIT", "other.service"), ("_SYSTEMD_INVOCATION_ID", "2" * 32)],
                                [("_SYSTEMD_UNIT", "alpha.service")],
                                [("_SYSTEMD_UNIT", "alpha.service"), ("_SYSTEMD_INVOCATION_ID", "A" * 32)]]),
        ("broken-after-unit", [[("_SYSTEMD_UNIT", "alpha.service"), ("_SYSTEMD_INVOCATION_ID", "A" * 32)],
                               [("_SYSTEMD_UNIT", "alpha.service")],
                               [("_SYSTEMD_UNIT", "other.service"), ("_SYSTEMD_INVOCATION_ID", "2" * 32)]]),
        ("invalid", [[("_SYSTEMD_UNIT", "alpha.service"), ("_SYSTEMD_INVOCATION_ID", value),
                      ("OBJECT_SYSTEMD_INVOCATION_ID", iid)] for value in
                     ["0" * 32, "not-an-id", iid + "\0", iid + "\0tail", " " + iid,
                      "12345678-1234-1234-1234-123456789012", "A" * 32]]),
    ]:
        path = fixture(name, rows)
        for args in ([], ["-n1"], ["-n+1"], ["-n2"], ["-n+2"], ["-n0"], ["-r"]):
            cases.append(("invocation-" + name + str(args), path,
                          ["-ualpha", "--list-invocations", "-ojson", *args], "json"))
        for descriptor in ("0", "1", "-1", iid, "2" * 32, "2" * 32 + "+1", "3" * 32 + "-1"):
            cases.append(("invocation-select-" + name + descriptor, path,
                          ["-ualpha", "--invocation=" + descriptor, "-ocat"], "bytes"))
        if name == "bounds":
            for filters in (["-tselected"], ["-tabsent"], ["-p6"], ["-p3"],
                            ["--facility=3"], ["--facility=1"], ["--dmesg"]):
                cases.append(("invocation-other-filter-" + str(filters), path,
                              ["--invocation=" + iid, "--boot=all", "-ocat", *filters], "bytes"))
    for action, error in [("--list-invocations", "invalid argument"), ("-I", "no journal boot entry")]:
        cases.append(("invocation-error-order-" + action, path,
                      [action, "--unit=", "--boot=" + "9" * 32], "error:" + error))
    return cases
