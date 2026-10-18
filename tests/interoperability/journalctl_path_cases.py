"""Native filenames and file-pattern controls for the pinned journalctl oracle."""

from __future__ import annotations

import os
import shutil
import subprocess  # nosec B404
import tempfile
from pathlib import Path

from go_fixture_writer import write_journal_file


def run_file_path_cases(commands: dict[str, str], source: Path,
                        env: dict[str, str]) -> list[dict[str, object]]:
    if os.name != "posix":
        return []
    work = Path(tempfile.mkdtemp(prefix="v262-native-paths-", dir=source.parent))
    patterns = work / "patterns"
    patterns.mkdir()
    names = [b"brace-a.journal", b"brace-b.journal", "unicode-é.journal".encode(),
             b"raw-\xff.journal", b"raw-\xef.journal", "raw-�.journal".encode(),
             b".hidden.journal", b"literal-[a].journal", b"literal-*.journal"]
    for index, name in enumerate(names):
        seed = work / f"seed-{index}.journal"
        write_journal_file(seed, machine_id="00112233445566778899aabbccddeeff",
                           boot_id="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                           seqnum_id="12121212121212121212121212121212",
                           entries=[{"realtime_usec": 1_700_007_000_000_000 + index,
                                     "monotonic_usec": index + 1,
                                     "fields": [("MESSAGE", f"path-{index}")]}])
        shutil.copyfile(seed, patterns / os.fsdecode(name))
    parent = work / os.fsdecode(b"parent-\xff")
    parent.mkdir()
    shutil.copyfile(source, parent / "system.journal")
    decoy = work / "parent-�"
    decoy.mkdir()
    shutil.copyfile(work / "seed-0.journal", decoy / "system.journal")
    raw_directory = work / "raw-names"
    raw_directory.mkdir()
    for name in (b"raw-\xff.journal", b"raw-\xef.journal"):
        shutil.copyfile(patterns / os.fsdecode(name), raw_directory / os.fsdecode(name))

    cases = [
        ("literal-parent", ["--file", str(parent / "system.journal")], "C"),
        ("directory-parent", ["--directory", str(parent)], "C"),
        ("literal-byte-name", ["--file", str(raw_directory / os.fsdecode(b"raw-\xff.journal"))], "C"),
        ("directory-byte-names", ["--directory", str(raw_directory)], "C"),
        ("multiple-byte-names", ["--file", str(raw_directory / os.fsdecode(b"raw-\xff.journal")),
                                 "--file", str(raw_directory / os.fsdecode(b"raw-\xef.journal"))], "C"),
        ("header-byte-name", ["--file", str(raw_directory / os.fsdecode(b"raw-\xff.journal")), "--header"], "C"),
    ]
    for locale in ("C", "C.UTF-8"):
        for pattern in ["brace-{a,b}.journal", "unicode-?.journal", "unicode-??.journal",
                        os.fsdecode(b"raw-[\xff].journal"), "raw-[�].journal", "raw-?.journal",
                        "raw-???.journal", "*.journal", ".*.journal", r"literal-\[a\].journal",
                        r"literal-\*.journal", "missing-*.journal", "{missing,brace-a}.journal",
                        "brace-{a,{b,missing}}.journal"]:
            cases.append(("pattern-" + repr(pattern), ["--file", str(patterns / pattern)], locale))
    results = []
    for name, args, locale in cases:
        observed = {}
        for reader, command in commands.items():
            observed[reader] = subprocess.run(  # nosec B603 - fixed binaries and private fixtures.
                [command, *args, "--no-pager", "--boot=all", "--merge", "-q", "-ocat"],
                env={**env, "LC_ALL": locale}, capture_output=True, timeout=30, check=False,
            )
        expected = observed["stock"]
        if not name.startswith("pattern-") and expected.returncode != 0:
            raise AssertionError(f"native filename control {name} failed: {expected.stderr!r}")
        for reader in ("go", "rust"):
            actual = observed[reader]
            same = (actual.returncode == expected.returncode and actual.stdout == expected.stdout
                    and bool(actual.stderr) == bool(expected.stderr))
            results.append({"test": "v262-path-" + name, "locale": locale, "reader": reader,
                            "status": "PASS" if same else "FAIL", "args": args,
                            "returncode": actual.returncode, "expected_returncode": expected.returncode,
                            "actual": actual.stdout.hex(), "expected": expected.stdout.hex(),
                            "stderr": actual.stderr.decode(errors="replace")[-1000:],
                            "expected_stderr": expected.stderr.decode(errors="replace")[-1000:]})
    return results
