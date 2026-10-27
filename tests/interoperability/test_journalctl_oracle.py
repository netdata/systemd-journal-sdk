"""Oracle selection checks without building tools or accessing host journals."""
import subprocess  # nosec B404 - CompletedProcess test values only; execution is mocked.
import unittest
from pathlib import Path
from unittest.mock import patch

import run_journalctl_query_matrix as matrix


class OracleSelectionTests(unittest.TestCase):
    def test_version_uses_selected_binary_and_required_features(self):
        output = "systemd 262 (262)\n+PCRE2 +ZSTD +XZ +LZ4\n"
        with patch.object(matrix, "run", return_value=subprocess.CompletedProcess([], 0, output, "")) as run:
            self.assertEqual(matrix.systemd_version("/selected/journalctl"), "systemd 262 (262)")
            run.assert_called_once_with(["/selected/journalctl", "--version"], timeout=10)

    def test_rejects_other_version_or_missing_feature(self):
        for output in ["systemd 260\n+PCRE2 +ZSTD +XZ +LZ4", "systemd 262\n-PCRE2 +ZSTD +XZ +LZ4"]:
            with self.subTest(output=output), patch.object(matrix, "run", return_value=subprocess.CompletedProcess([], 0, output, "")):
                with self.assertRaises(RuntimeError):
                    matrix.systemd_version("/selected/journalctl")

    def test_all_command_builders_require_explicit_oracle(self):
        tools = {"stock_journalctl": "/selected/journalctl"}
        commands = [matrix.reader_command("stock", tools, "file", Path("fixture"), []),
                    matrix.raw_reader_command("stock", tools, "file", Path("fixture"), []),
                    matrix.action_command("stock", tools, ["--version"])]
        self.assertTrue(all(command[0] == tools["stock_journalctl"] for command in commands))
        with self.assertRaises(KeyError):
            matrix.reader_command("stock", {}, "file", Path("fixture"), [])

    def test_empty_cases_use_explicit_oracle(self):
        calls = []
        def run(command, **kwargs):
            calls.append(command)
            return subprocess.CompletedProcess(command, 0, "", "")
        tools = {"stock_journalctl": "/selected/journalctl", "go_journalctl": "/selected/go", "rust_journalctl": "/selected/rust"}
        with patch.object(matrix, "run", side_effect=run):
            matrix.run_empty_result_cases(tools, {"file": Path("fixture")})
        self.assertTrue(calls)
        self.assertTrue(all(command[0] in tools.values() for command in calls))


if __name__ == "__main__":
    unittest.main()
