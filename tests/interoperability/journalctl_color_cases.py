"""Terminal color controls against the pinned native journalctl."""

from __future__ import annotations

import collections
import json
import os
import re
import subprocess  # nosec B404
import tempfile
from pathlib import Path

from go_fixture_writer import write_journal_file


ANSI = re.compile(rb"\x1b\[[0-9;:]*m")
JSON_TOKEN = re.compile(rb'"(?:\\.|[^"\\])*"|(?:\x1b\[[0-9;:]*m)*(?:-?\d+|null|true|false)(?:\x1b\[[0-9;:]*m)*')


def normalized(data: bytes, mode: str):
    if mode == "cat-projection":
        # Native output-fields is an unordered set; this control uses one-line values.
        return sorted(data.splitlines(keepends=True))
    if not mode.startswith("json"):
        return data
    if mode == "json-sse":
        bodies = [v.removeprefix(b"data: ") for v in data.split(b"\n\n") if v]
    elif mode == "json-seq":
        bodies = [v for v in data.split(b"\x1e") if v.strip()]
    elif mode == "json-pretty":
        text = ANSI.sub(b"", data).decode()
        decoder, offset, parsed = json.JSONDecoder(), 0, []
        while offset < len(text):
            if text[offset:].isspace():
                break
            item, count = decoder.raw_decode(text[offset:])
            parsed.append(item)
            offset += count
            while offset < len(text) and text[offset].isspace():
                offset += 1
        return parsed, collections.Counter(JSON_TOKEN.findall(data))
    else:
        bodies = data.splitlines()
    return ([json.loads(ANSI.sub(b"", value)) for value in bodies],
            collections.Counter(JSON_TOKEN.findall(b"\n".join(bodies))))


def run_color_cases(commands: dict[str, str], directory: Path,
                    env: dict[str, str]) -> list[dict[str, object]]:
    source = directory / "v262-colors.journal"
    boot = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    entries = []
    values = [b"alpha hit beta"] * 8 + [
        b"hit\nsecond", b"head\nlong second hit tail", b"\talpha hit", b"\x1b[31malpha hit\x1b[0m",
        b"\x1b[2J alpha hit", b"hit\x00tail", b"invalid\xff hit", b"", b"hit\n", b"hit\n\n",
        b"x" * 160 + b" hit tail", b"one\ntwo\nthree hit\nfour", b"alpha hit\r\nsecond",
    ]
    for index, value in enumerate(values):
        fields = [("MESSAGE", value), ("ROW", str(index)), ("PRIORITY", str(index % 8)),
                  ("_BOOT_ID", boot), ("BINARY", b"\xff\0"), ("NULL_VALUE", b"x" * 4100)]
        if index == 6:
            fields.append(("_TRANSPORT", "audit"))
        entries.append({"realtime_usec": 1_700_005_000_000_000 + index,
                        "monotonic_usec": index + 1, "fields": fields})
    # First MESSAGE supplies grep offsets, while short formats display the last.
    entries.append({"realtime_usec": 1_700_005_000_000_100, "monotonic_usec": 101,
                    "fields": [("MESSAGE", "hit first"), ("MESSAGE", "last message"),
                               ("PRIORITY", "3"), ("ROW", "duplicate"), ("_BOOT_ID", boot)]})
    entries.append({"realtime_usec": 1_700_005_000_000_101, "monotonic_usec": 102,
                    "fields": [("MESSAGE", "/var/log/journal-sdk/example.conf: hit configuration"),
                               ("CONFIG_FILE", "/var/log/journal-sdk/example.conf"), ("DOCUMENTATION", "https://example.org/manual"),
                               ("PRIORITY", "4"), ("ROW", "links"), ("_BOOT_ID", boot)]})
    entries.append({"realtime_usec": 1_700_005_000_000_102, "monotonic_usec": 103,
                    "fields": [("MESSAGE", "example.conf: hit configuration"), ("CONFIG_FILE", "example.conf"),
                               ("PRIORITY", "4"), ("ROW", "relative-links"), ("_BOOT_ID", boot)]})
    write_journal_file(source, machine_id="00112233445566778899aabbccddeeff",
                       boot_id=boot, seqnum_id="12121212121212121212121212121212", entries=entries)
    boots = directory / "v262-color-boots.journal"
    # No catalog message IDs: implicit host catalog lookup is outside portable mode.
    write_journal_file(boots, machine_id="00112233445566778899aabbccddeeff", boot_id=boot,
                       seqnum_id="12121212121212121212121212121212",
                       entries=[{"realtime_usec": 1_700_005_000_000_000 + index,
                                 "monotonic_usec": 1, "boot_id": ch * 32,
                                 "fields": [("MESSAGE", "boot-" + ch), ("PRIORITY", str(priority)),
                                            ("_BOOT_ID", ch * 32)]}
                                for index, (ch, priority) in enumerate([("a", 3), ("b", 7)])])
    modes = ["short", "short-full", "short-iso", "short-iso-precise", "short-precise",
             "short-monotonic", "short-delta", "short-unix", "with-unit", "cat", "verbose",
             "json", "json-pretty", "json-seq", "json-sse", "export"]
    cases = []
    for mode in modes:
        for extra in ([], ["--grep=hit"], ["--grep=hit", "-n3", "-r"], ["--all", "--grep=hit"]):
            cases.append((mode + "-" + repr(extra), mode, ["-q", "-o", mode, *extra], {"SYSTEMD_COLORS": "16"}))
    for policy in ["256", "24bit", "true", "false", "auto-16", "invalid"]:
        cases.append(("policy-" + policy, "short", ["-q", "-oshort", "ROW=4", "ROW=7"],
                      {"SYSTEMD_COLORS": policy}))
    cases += [("plain-verbose-multiline", "verbose", ["-q", "-overbose", "ROW=8", "ROW=13", "--all"], {"SYSTEMD_COLORS": "0"}),
              ("plain-reverse-delta", "short-delta", ["-q", "-oshort-delta", "--grep=hit", "-n3", "-r"], {"SYSTEMD_COLORS": "0"})]
    for mode in ("short", "verbose"):
        for link in ("0", "1"):
            for grep in ([], ["--grep=hit"], ["--grep=example"]):
                cases.append((f"links-{mode}-{link}-{grep}", mode, ["-q", "-o", mode, "ROW=links", *grep],
                              {"SYSTEMD_COLORS": "16", "SYSTEMD_URLIFY": link}))
    for mode in ("short", "cat", "verbose", "json"):
        for policy in ("16", "true", "auto-16"):
            cases.append(("no-color-" + mode + "-" + policy, mode, ["-q", "-o", mode, "ROW=4"],
                          {"SYSTEMD_COLORS": policy, "NO_COLOR": ""}))
    cases += [("linux-underline", "verbose", ["-q", "-overbose", "ROW=0"], {"SYSTEMD_COLORS": "16", "TERM": "linux"}),
              ("cat-fields", "cat-projection", ["-q", "-ocat", "--output-fields=ROW,MESSAGE", "--grep=hit", "ROW=4"], {"SYSTEMD_COLORS": "16"}),
              ("truncated", "short", ["-q", "-oshort", "--truncate-newline", "--grep=hit"], {"SYSTEMD_COLORS": "16"}),
              ("no-entries", "short", ["-oshort", "ROW=missing"], {"SYSTEMD_COLORS": "16"})]
    for action in ("--list-boots", "--list-invocations"):
        # The shared multi-boot fixture has invocation IDs as well.
        if not (directory / "multi-boot-file.journal").is_file():
            continue
        for mode in ("short", "json", "json-pretty", "json-seq", "json-sse"):
            cases.append((action + "-" + mode, mode, [action, "-o", mode], {"SYSTEMD_COLORS": "16"}))
    cases += [("boots-short", "short", ["-oshort"], {"SYSTEMD_COLORS": "16"}),
              ("boots-reverse", "short", ["-oshort", "--grep=boot", "-r", "-n3"], {"SYSTEMD_COLORS": "16"}),
              ("--list-boots-linux", "short", ["--list-boots"], {"SYSTEMD_COLORS": "16", "TERM": "linux"}),
              ("links-utf8", "short", ["-q", "-oshort", "ROW=links"], {"SYSTEMD_COLORS": "16", "LC_ALL": "C.UTF-8"}),
              ("links-plain", "short", ["-q", "-oshort", "ROW=links", "--grep=hit"], {"SYSTEMD_COLORS": "0", "SYSTEMD_URLIFY": "1"})]
    logical_cwd = None
    if os.name == "posix":
        work = Path(tempfile.mkdtemp(prefix="v262-color-cwd-", dir=directory))
        real, logical_cwd = work / "real", work / "alias"
        real.mkdir()
        logical_cwd.symlink_to(real, target_is_directory=True)
        cases.append(("links-logical-cwd", "short", ["-q", "-oshort", "ROW=relative-links"],
                      {"SYSTEMD_COLORS": "16", "PWD": str(logical_cwd) + "//"}))
    clean_env = {key: value for key, value in env.items() if key not in ("NO_COLOR", "COLORTERM", "SYSTEMD_URLIFY")}
    results = []
    for name, mode, args, changes in cases:
        path = (directory / "multi-boot-file.journal" if name.startswith("--list-")
                else boots if name.startswith("boots-") else source)
        observed = {}
        for reader, command in commands.items():
            observed[reader] = subprocess.run(  # nosec B603 - fixed executables and synthetic input.
                [command, "--file", str(path), "--no-pager", *args],
                env={**clean_env, "TERM": "xterm", **changes}, capture_output=True, timeout=30, check=False,
                cwd=logical_cwd if name == "links-logical-cwd" else None,
            )
        expected = observed["stock"]
        for reader in ("go", "rust"):
            actual = observed[reader]
            try:
                same_output = normalized(actual.stdout, mode) == normalized(expected.stdout, mode)
            except (ValueError, UnicodeError):
                same_output = False
            same = (actual.returncode == expected.returncode and same_output
                    and bool(actual.stderr) == bool(expected.stderr))
            results.append({"test": "v262-color-" + name, "reader": reader, "args": args,
                            "status": "PASS" if same else "FAIL", "returncode": actual.returncode,
                            "expected_returncode": expected.returncode,
                            "actual": actual.stdout.hex(), "expected": expected.stdout.hex(),
                            "stderr": actual.stderr.decode(errors="replace")[-1000:],
                            "expected_stderr": expected.stderr.decode(errors="replace")[-1000:]})
    if os.name == "posix":
        results.extend(run_terminal_color_cases(commands, source, clean_env))
    return results


def run_terminal_color_cases(commands: dict[str, str], fixture: Path,
                             env: dict[str, str]) -> list[dict[str, object]]:
    import errno
    import pty
    import termios

    results = []
    for both, mode, term in [(True, "auto-16", "xterm"), (False, "auto-16", "xterm"),
                             (True, "auto-16", ""), (True, "auto-16", None),
                             (True, "auto-16", "dumb"), (False, "true", "dumb")]:
        terminal_env = {**env, "SYSTEMD_COLORS": mode, "SYSTEMD_URLIFY": "0"}
        if term is None:
            terminal_env.pop("TERM", None)
        else:
            terminal_env["TERM"] = term
        observed = {}
        for reader, command in commands.items():
            master, slave = pty.openpty()
            output = bytearray()
            with os.fdopen(master, "rb", buffering=0) as terminal:
                try:
                    attributes = termios.tcgetattr(slave)
                    attributes[1] &= ~termios.ONLCR
                    termios.tcsetattr(slave, termios.TCSANOW, attributes)
                    # One bounded message fits before draining; no follow child is terminated.
                    result = subprocess.run(  # nosec B603 - fixed binaries and synthetic input.
                        [command, "--file", str(fixture), "--no-pager", "-q", "-ocat", "ROW=4"],
                        env=terminal_env, stdout=slave, stderr=slave if both else subprocess.PIPE,
                        timeout=30, check=False,
                    )
                finally:
                    os.close(slave)
                while True:
                    try:
                        chunk = terminal.read(4096)
                    except OSError as error:
                        if error.errno == errno.EIO:
                            break
                        raise
                    if not chunk:
                        break
                    output.extend(chunk)
            observed[reader] = (result.returncode, bytes(output), result.stderr or b"")
        expected = observed["stock"]
        if expected[0] != 0 or expected[2] or not expected[1]:
            raise AssertionError("native terminal color control failed")
        for reader in ("go", "rust"):
            actual = observed[reader]
            results.append({"test": f"v262-color-terminal-{both}-{mode}-{term!r}", "reader": reader,
                            "status": "PASS" if actual == expected else "FAIL",
                            "returncode": actual[0], "expected_returncode": expected[0],
                            "actual": actual[1].hex(), "expected": expected[1].hex(),
                            "stderr": actual[2].decode(errors="replace"),
                            "expected_stderr": expected[2].decode(errors="replace")})
    return results
