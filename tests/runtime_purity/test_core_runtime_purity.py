import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]

CORE_RUNTIME_FILES = [
    # Keep this list aligned with every production reader/writer/facade module.
    # Optional identity and lock helpers are intentionally excluded; core code
    # must not import them.
    "go/journal/writer.go",
    "go/journal/log.go",
    "go/journal/reader.go",
    "go/journal/facade.go",
    "rust/src/journal/src/lib.rs",
    "rust/src/journal/src/facade.rs",
    "rust/src/crates/journal-core/src/file/file.rs",
    "rust/src/crates/journal-core/src/file/writer.rs",
    "rust/src/crates/journal-core/src/file/reader.rs",
    "rust/src/crates/journal-log-writer/src/log/mod.rs",
    "rust/src/crates/jf/journal_file/src/file.rs",
    "rust/src/crates/jf/journal_file/src/writer.rs",
]

FORBIDDEN_CORE_IMPORTS = {
    "go/journal/writer.go": [
        r"\bcurrentBootID\b",
        r"\breadHostBootID\b",
        r"\breadUUIDFile\b",
    ],
    "go/journal/log.go": [
        r"\bcurrentBootID\b",
        r"\breadHostBootID\b",
        r"\breadUUIDFile\b",
    ],
    "go/journal/reader.go": [
        r"\bcurrentBootID\b",
        r"\breadHostBootID\b",
        r"\breadUUIDFile\b",
    ],
    "go/journal/facade.go": [
        r"\bcurrentBootID\b",
        r"\breadHostBootID\b",
        r"\breadUUIDFile\b",
    ],
    "rust/src/journal/src/lib.rs": [
        r"\bload_boot_id\b",
        r"\bload_machine_id\b",
        r"\bload_boot_id_from_sysctl_boottime\b",
    ],
    "rust/src/journal/src/facade.rs": [
        r"\bload_boot_id\b",
        r"\bload_machine_id\b",
        r"\bload_boot_id_from_sysctl_boottime\b",
    ],
    "rust/src/crates/journal-core/src/file/file.rs": [
        r"\bload_boot_id\b",
        r"\bload_machine_id\b",
        r"\bload_boot_id_from_sysctl_boottime\b",
    ],
    "rust/src/crates/journal-core/src/file/writer.rs": [
        r"\bload_boot_id\b",
        r"\bload_machine_id\b",
        r"\bload_boot_id_from_sysctl_boottime\b",
    ],
    "rust/src/crates/journal-core/src/file/reader.rs": [
        r"\bload_boot_id\b",
        r"\bload_machine_id\b",
        r"\bload_boot_id_from_sysctl_boottime\b",
    ],
    "rust/src/crates/journal-log-writer/src/log/mod.rs": [
        r"\bload_boot_id\b",
        r"\bload_machine_id\b",
        r"\bload_boot_id_from_sysctl_boottime\b",
    ],
    "rust/src/crates/jf/journal_file/src/file.rs": [
        r"\bload_boot_id\b",
        r"\bload_machine_id\b",
        r"\bload_boot_id_from_sysctl_boottime\b",
    ],
    "rust/src/crates/jf/journal_file/src/writer.rs": [
        r"\bload_boot_id\b",
        r"\bload_machine_id\b",
        r"\bload_boot_id_from_sysctl_boottime\b",
    ],
}

FORBIDDEN_PATTERNS = [
    r"/proc",
    r"/host/proc",
    r"/etc/machine-id",
    r"system_profiler",
    r"\bsysctl\b",
    r"\bsubprocess\b",
    r"\bchild_process\b",
    r"\bspawn\s*\(",
    r"\bexecFile\b",
    r"\bCommand::new\s*\(",
    r"\bexec\.Command\s*\(",
    r"\bjournalhost\b",
    r"\bjournal_host\b",
    r"\bjournal-host\b",
    r"\bsystemd-journal-sdk-host\b",
    r"\breadHostBootId\b",
    r"\breadHostBootIdText\b",
    r"\breadHostBootID\b",
    r"\breadUUIDFile\b",
    r"\bload_boot_id\b",
    r"\bload_boot_id_from_sysctl_boottime\b",
    r"\bload_machine_id\b",
    r"\bboot_id_string\b",
    r"\bboot_id_bytes\b",
    r"\bcurrentBootID\b",
    r"\bEnableLock\b",
    r"\benableLock\b",
    r"\benable_lock\b",
    r"\bwith_writer_lock\b",
]


def runtime_source(path: Path) -> str:
    text = path.read_text(encoding="utf-8")
    if path.suffix == ".rs":
        marker = "\n#[cfg(test)]\nmod tests"
        if marker in text:
            text = text.split(marker, 1)[0]
    return text


class CoreRuntimePurityTest(unittest.TestCase):
    def test_core_runtime_has_no_host_probing_or_subprocesses(self):
        failures = []
        for relative in CORE_RUNTIME_FILES:
            path = ROOT / relative
            text = runtime_source(path)
            for pattern in FORBIDDEN_PATTERNS:
                if re.search(pattern, text):
                    failures.append(f"{relative}: forbidden pattern {pattern!r}")
        if failures:
            self.fail("\n".join(failures))

    def test_core_runtime_does_not_import_optional_platform_helpers(self):
        failures = []
        for relative, patterns in FORBIDDEN_CORE_IMPORTS.items():
            path = ROOT / relative
            text = runtime_source(path)
            for pattern in patterns:
                if re.search(pattern, text):
                    failures.append(f"{relative}: forbidden helper import/name {pattern!r}")
        if failures:
            self.fail("\n".join(failures))


if __name__ == "__main__":
    unittest.main()
