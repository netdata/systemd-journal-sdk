#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Unit tests for ``tests/docs/verify_examples.py``.

These tests exercise only the pure functions of the harness: marker parsing,
path substitution, Rust wrapping, Go wrapping, Go import detection, and
prelude application. They MUST NOT require cargo/go toolchains or build
anything.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import subprocess
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
HARNESS_PATH = REPO_ROOT / "tests" / "docs" / "verify_examples.py"


def _load_harness():
    spec = importlib.util.spec_from_file_location("verify_examples_for_tests", HARNESS_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load harness from {HARNESS_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


ve = _load_harness()


class MarkerParsingTests(unittest.TestCase):
    def test_verify_marker_basic(self):
        kind, attrs, reason = ve.parse_marker(
            "<!-- verify-example: lang=rust id=read-one-file -->"
        )
        self.assertEqual(kind, "verify-example")
        self.assertEqual(attrs, {"lang": "rust", "id": "read-one-file"})
        self.assertEqual(reason, "")

    def test_verify_marker_with_mode_fixture_prelude(self):
        kind, attrs, _ = ve.parse_marker(
            "<!-- verify-example: lang=rust id=x-y-z mode=build fixture=basic prelude=netdata-config-imports -->"
        )
        self.assertEqual(kind, "verify-example")
        self.assertEqual(attrs["lang"], "rust")
        self.assertEqual(attrs["id"], "x-y-z")
        self.assertEqual(attrs["mode"], "build")
        self.assertEqual(attrs["fixture"], "basic")
        self.assertEqual(attrs["prelude"], "netdata-config-imports")

    def test_illustrative_marker(self):
        kind, attrs, reason = ve.parse_marker(
            "<!-- illustrative-only: not yet compilable -->"
        )
        self.assertEqual(kind, "illustrative-only")
        self.assertEqual(attrs, {})
        self.assertEqual(reason, "not yet compilable")

    def test_non_marker_line(self):
        self.assertIsNone(ve.parse_marker("ordinary markdown line"))

    def test_extract_examples_finds_all_required_blocks(self):
        page = REPO_ROOT / "tests" / "docs" / "testdata" / "Sample-Page.md"
        examples = ve.extract_examples(page)
        ids = sorted(ex["id"] for ex in examples)
        self.assertEqual(ids, [
            "go-open-writer",
            "go-read-one-file",
            "go-write-three-entries",
            "rust-netdata-config-build",
            "rust-read-one-file",
        ])
        langs = {ex["lang"] for ex in examples}
        self.assertEqual(langs, {"rust", "go"})

    def test_illustrative_block_is_ignored(self):
        page = REPO_ROOT / "tests" / "docs" / "testdata" / "Sample-Page.md"
        examples = ve.extract_examples(page)
        for ex in examples:
            self.assertNotEqual(ex["id"], "illustrative-block")

    def test_marker_inside_fenced_block_is_ignored(self):
        body = (
            "## Verified Examples\n"
            "\n"
            "```markdown\n"
            "<!-- verify-example: lang=rust id=read-one-file -->\n"
            "```\n"
            "\n"
            "That was just a literal example of the marker syntax.\n"
        )
        with self._tmp_md("inside-fence.md", body) as path:
            examples = ve.extract_examples(path)
            self.assertEqual(examples, [])

    def test_duplicate_id_is_a_harness_error(self):
        with self._tmp_md("dup.md", self._duplicate_page()) as path:
            with self.assertRaises(ve.HarnessError):
                ve.extract_examples(path)

    def test_duplicate_id_across_files_is_a_harness_error(self):
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as td:
            root = Path(td)
            page_a = (
                "<!-- verify-example: lang=rust id=shared-id -->\n"
                "```rust\nfn main() {}\n```\n"
            )
            page_b = (
                "<!-- verify-example: lang=rust id=shared-id -->\n"
                "```rust\nfn main() {}\n```\n"
            )
            (root / "A.md").write_text(page_a, encoding="utf-8")
            (root / "B.md").write_text(page_b, encoding="utf-8")
            with self.assertRaises(ve.HarnessError) as ctx:
                ve.discover_examples(root)
            message = str(ctx.exception)
            self.assertIn("shared-id", message)
            self.assertIn("first seen at", message)
            self.assertIn("A.md", message)

    def test_marker_without_fence_is_a_harness_error(self):
        body = "<!-- verify-example: lang=rust id=lone -->\nordinary text\n"
        with self._tmp_md("nofence.md", body) as path:
            with self.assertRaises(ve.HarnessError):
                ve.extract_examples(path)

    def test_marker_on_unsupported_lang_is_a_harness_error(self):
        body = (
            "<!-- verify-example: lang=python id=py -->\n"
            "```python\nprint(1)\n```\n"
        )
        with self._tmp_md("py.md", body) as path:
            with self.assertRaises(ve.HarnessError):
                ve.extract_examples(path)

    def test_marker_lang_must_match_fence_lang(self):
        body = (
            "<!-- verify-example: lang=go id=mix -->\n"
            "```rust\nfn main() {}\n```\n"
        )
        with self._tmp_md("mix.md", body) as path:
            with self.assertRaises(ve.HarnessError):
                ve.extract_examples(path)

    def test_unknown_prelude_is_a_harness_error(self):
        body = (
            "<!-- verify-example: lang=go id=bad-prelude prelude=nope -->\n"
            "```go\npackage main\nfunc main() {}\n```\n"
        )
        with self._tmp_md("bp.md", body) as path:
            with self.assertRaises(ve.HarnessError):
                ve.extract_examples(path)

    def test_bad_id_slug_is_a_harness_error(self):
        body = (
            "<!-- verify-example: lang=rust id=BAD -->\n"
            "```rust\nfn main() {}\n```\n"
        )
        with self._tmp_md("bid.md", body) as path:
            with self.assertRaises(ve.HarnessError):
                ve.extract_examples(page=path) if False else ve.extract_examples(path)

    @staticmethod
    def _duplicate_page() -> str:
        return (
            "<!-- verify-example: lang=rust id=dup -->\n"
            "```rust\nfn main() {}\n```\n"
            "\n"
            "<!-- verify-example: lang=rust id=dup -->\n"
            "```rust\nfn main() {}\n```\n"
        )

    @staticmethod
    def _tmp_md(name: str, body: str):
        from tempfile import TemporaryDirectory
        from contextlib import contextmanager

        @contextmanager
        def _ctx():
            with TemporaryDirectory() as td:
                p = Path(td) / name
                p.write_text(body, encoding="utf-8")
                yield p

        return _ctx()


class PathSubstitutionTests(unittest.TestCase):
    def test_applies_replacements_in_order(self):
        text = (
            "see /var/log/journal/example/system.journal and "
            "/var/log/journal-sdk/example.journal plus /var/log/journal-sdk and "
            "/var/log/journal and /tmp/example.journal"
        )
        out = ve.apply_path_substitution(
            text,
            scratch_dir=Path("/tmp/scratch"),
            fixture_dir=Path("/tmp/fixtures"),
        )
        self.assertIn("/tmp/fixtures/basic/file/system.journal", out)
        self.assertIn("/tmp/scratch/example.journal", out)
        self.assertIn("/tmp/scratch", out)
        self.assertIn("/tmp/fixtures/basic/dir", out)
        self.assertIn("/tmp/scratch/tmp-example.journal", out)
        self.assertNotIn("/var/log/journal", out)
        self.assertNotIn("/tmp/example.journal", out)

    def test_replacement_uses_passed_paths(self):
        out = ve.apply_path_substitution(
            "/var/log/journal",
            scratch_dir=Path("/x/scratch"),
            fixture_dir=Path("/x/fixtures"),
        )
        self.assertEqual(out, "/x/fixtures/basic/dir")

    def test_verify_prefixes_are_longest_first(self):
        """No earlier ``VERIFY_PREFIXES`` entry may be a prefix of a later one.

        ``apply_path_substitution`` rewrites the text once per prefix in tuple
        order. If a shorter prefix appears before a longer one that starts
        with it (for example ``/var/log/journal`` before
        ``/var/log/journal/example/system.journal``), the shorter
        substitution runs first and rewrites the longer path's leading
        segment, so the longer-prefix rule never matches and the more
        specific replacement (e.g. ``<FIXTURES>/basic/file/system.journal``)
        silently shadows. This test fails the moment someone reorders the
        tuple.
        """
        prefixes = ve.VERIFY_PREFIXES
        self.assertGreater(len(prefixes), 1)
        for i in range(len(prefixes)):
            for j in range(i + 1, len(prefixes)):
                self.assertFalse(
                    prefixes[j].startswith(prefixes[i]),
                    msg=(
                        f"VERIFY_PREFIXES[{i}]={prefixes[i]!r} is a proper prefix of "
                        f"VERIFY_PREFIXES[{j}]={prefixes[j]!r}; reorder so the longer "
                        f"path comes first or apply_path_substitution will shadow it."
                    ),
                )


class RustWrappingTests(unittest.TestCase):
    def test_run_mode_with_question_mark_wraps_result(self):
        body = "let mut r = open_reader()?;\nr.seek_head();"
        wrapped = ve.wrap_rust_example(body, "")
        self.assertIn("fn main() -> Result<(), Box<dyn std::error::Error>> {", wrapped)
        self.assertIn("let mut r = open_reader()?;", wrapped)
        self.assertIn("r.seek_head();", wrapped)
        self.assertIn("Ok(())", wrapped)
        self.assertTrue(wrapped.rstrip().endswith("}"))

    def test_run_mode_with_explicit_result_return_appends_ok(self):
        body = "fn go() -> Result<(), Box<dyn std::error::Error>> { Ok(()) }\ngo()"
        wrapped = ve.wrap_rust_example(body, "")
        self.assertIn("fn main() -> Result<(), Box<dyn std::error::Error>> {", wrapped)
        # User's inner Ok(()) is preserved; the harness also appends Ok(())
        # at the end of main() because the body's last line is `go()`,
        # not an `Ok(...)-style` expression per the spec.
        self.assertGreaterEqual(wrapped.count("Ok(())"), 1)
        # The appended Ok(()) must appear after the user's last statement.
        self.assertGreater(wrapped.rindex("Ok(())"), wrapped.rindex("go()"))

    def test_run_mode_without_question_mark_wraps_simple_main(self):
        body = 'println!("hi");'
        wrapped = ve.wrap_rust_example(body, "")
        self.assertIn("fn main() {", wrapped)
        self.assertIn('println!("hi");', wrapped)
        self.assertNotIn("Result", wrapped)

    def test_hidden_lines_are_unhidden(self):
        body = "use std::io;\n# let x = 1;\nprintln!(\"{}\", x);"
        wrapped = ve.wrap_rust_example(body, "")
        self.assertIn("use std::io;", wrapped)
        self.assertIn("let x = 1;", wrapped)
        self.assertIn("println!(\"{}\", x);", wrapped)
        self.assertNotIn("# let x = 1;", wrapped)

    def test_hidden_lone_hash_becomes_empty(self):
        body = "let x = 1;\n#\nprintln!(\"{}\", x);"
        wrapped = ve.wrap_rust_example(body, "")
        self.assertIn("let x = 1;", wrapped)
        self.assertIn("println!(\"{}\", x);", wrapped)
        self.assertNotIn("#\n", wrapped)

    def test_prelude_is_prepended_inside_main(self):
        body = "do_work();"
        prelude = "use some::thing;\n"
        wrapped = ve.wrap_rust_example(body, prelude)
        self.assertLess(wrapped.index("use some::thing;"), wrapped.index("do_work();"))

    def test_ends_with_simple_Ok_unit_does_not_append(self):
        body = "let mut r = open()?;\nOk(())"
        wrapped = ve.wrap_rust_example(body, "")
        self.assertIn("fn main() -> Result<(), Box<dyn std::error::Error>> {", wrapped)
        self.assertIn("Ok(())", wrapped)
        # Body already ends with Ok(()); harness must NOT append another one.
        self.assertEqual(wrapped.count("Ok(())"), 1)
        self.assertTrue(wrapped.rstrip().endswith("Ok(())}"))

    def test_ends_with_turbofish_Ok_does_not_append(self):
        body = (
            "use journal::FileReader;\n"
            "let mut r = FileReader::open(\"/x\")?;\n"
            "Ok::<(), Box<dyn std::error::Error>>(())"
        )
        wrapped = ve.wrap_rust_example(body, "")
        self.assertIn("fn main() -> Result<(), Box<dyn std::error::Error>> {", wrapped)
        # The rustdoc-style turbofish Ok must be preserved verbatim.
        self.assertIn("Ok::<(), Box<dyn std::error::Error>>(())", wrapped)
        # Harness must not append a second trailing Ok(()).
        self.assertEqual(wrapped.count("Ok::<(), Box<dyn std::error::Error>>(())"), 1)
        self.assertNotIn("Ok(())", wrapped)
        self.assertTrue(
            wrapped.rstrip().endswith("Ok::<(), Box<dyn std::error::Error>>(())}")
        )

    def test_question_mark_without_Ok_appends_Ok_unit(self):
        body = "let mut r = open_reader()?;\nr.seek_head();"
        wrapped = ve.wrap_rust_example(body, "")
        self.assertIn("fn main() -> Result<(), Box<dyn std::error::Error>> {", wrapped)
        self.assertIn("let mut r = open_reader()?;", wrapped)
        self.assertIn("r.seek_head();", wrapped)
        # Body has ? but no Ok ending; harness must append Ok(()).
        self.assertIn("Ok(())", wrapped)
        self.assertEqual(wrapped.count("Ok(())"), 1)
        # The appended Ok(()) must be the body's last line, right before the
        # closing brace of main().
        self.assertTrue(wrapped.rstrip().endswith("Ok(())\n}"))


class GoWrappingTests(unittest.TestCase):
    def test_basic_body_uses_fmt_and_os(self):
        body = "fmt.Println(\"x\")"
        wrapped = ve.wrap_go_example(body, "")
        self.assertIn("package main", wrapped)
        self.assertIn('\t"fmt"\n', wrapped)
        self.assertIn('\t"os"\n', wrapped)
        self.assertIn("func run() error {", wrapped)
        self.assertIn("fmt.Println(\"x\")", wrapped)
        self.assertIn("return nil", wrapped)
        self.assertIn("func main() {", wrapped)
        self.assertIn("os.Exit(1)", wrapped)

    def test_import_detection_uses_journal_when_journal_dot_present(self):
        body = "r, _ := journal.OpenFile(\"/p\")\nr.Next()"
        wrapped = ve.wrap_go_example(body, "")
        self.assertIn('journal "github.com/netdata/systemd-journal-sdk/go/journal"', wrapped)
        self.assertIn('\t"os"\n', wrapped)
        self.assertIn('\t"fmt"\n', wrapped)

    def test_import_detection_omits_unused_prefixes(self):
        body = "fmt.Println(\"a\")"
        wrapped = ve.wrap_go_example(body, "")
        self.assertNotIn('"time"', wrapped)
        self.assertNotIn('"bytes"', wrapped)
        self.assertNotIn('"encoding/json"', wrapped)
        self.assertNotIn("journal ", wrapped)

    def test_prelude_is_prepended_inside_run(self):
        body = "do_work()"
        prelude = "r, err := journal.OpenFile(\"/p\")\nif err != nil { return err }\n"
        wrapped = ve.wrap_go_example(body, prelude)
        self.assertLess(wrapped.index("r, err :="), wrapped.index("do_work()"))
        self.assertIn("return nil", wrapped)
        self.assertIn("func main() {", wrapped)

    def test_detect_go_imports_includes_required(self):
        imports = ve.detect_go_imports("fmt.Println(\"x\")", "")
        self.assertIn('"fmt"', imports)
        self.assertIn('"os"', imports)

    def test_detect_go_imports_returns_unique(self):
        imports = ve.detect_go_imports("fmt.Fprintf(os.Stderr, \"\")", "")
        self.assertEqual(imports.count('"fmt"'), 1)
        self.assertEqual(imports.count('"os"'), 1)

    def test_run_body_always_ends_with_return_nil(self):
        wrapped = ve.wrap_go_example("do_work()", "")
        self.assertIn("return nil", wrapped)
        # Confirm the run() function returns nil before main()
        run_idx = wrapped.index("func run() error {")
        main_idx = wrapped.index("func main() {")
        self.assertGreater(main_idx, run_idx)
        # Find the end-of-run body, it should contain return nil before main.
        run_section = wrapped[run_idx:main_idx]
        self.assertIn("return nil", run_section)


class PreludeRegistryTests(unittest.TestCase):
    def test_open_reader_prelude_in_journal_openfile(self):
        prelude = ve.PRELUDES[("go", "open-reader")]
        self.assertIn("journal.OpenFile(\"/var/log/journal/example/system.journal\")", prelude)
        self.assertIn("r.Close()", prelude)
        self.assertIn("r.SeekHead()", prelude)

    def test_open_writer_prelude_in_journal_create(self):
        prelude = ve.PRELUDES[("go", "open-writer")]
        self.assertIn("journal.Create(\"/var/log/journal-sdk/example.journal\"", prelude)
        self.assertIn("defer w.Close()", prelude)

    def test_netdata_config_imports_prelude(self):
        prelude = ve.PRELUDES[("rust", "netdata-config-imports")]
        self.assertIn("NetdataFunctionConfig", prelude)
        self.assertIn("NetdataJournalFunction", prelude)
        self.assertIn("SystemdJournalProfile", prelude)

    def test_preludes_get_path_substitution(self):
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as td:
            scratch = Path(td) / "scratch"
            fixtures = Path(td) / "fixtures"
            scratch.mkdir()
            fixtures.mkdir()
            raw = "r, err := journal.OpenFile(\"/var/log/journal/example/system.journal\")"
            out = ve.apply_path_substitution(raw, scratch_dir=scratch, fixture_dir=fixtures)
            self.assertIn(str(fixtures / "basic" / "file" / "system.journal"), out)


class GoDirectiveTests(unittest.TestCase):
    """The generated module's ``go`` directive must mirror ``go/go.mod``.

    Hardcoding the version in the harness silently diverges from the
    repository's own toolchain target whenever Go is bumped in
    ``go/go.mod``. The harness parses the live directive instead and the
    test guarantees both the parser and the rendered module agree with
    the file on disk.
    """

    def _expected_directive(self) -> str:
        text = (REPO_ROOT / "go" / "go.mod").read_text(encoding="utf-8")
        for raw in text.splitlines():
            line = raw.strip()
            if not line or line.startswith("//"):
                continue
            if line.startswith("go ") or line == "go":
                parts = line.split(None, 1)
                if len(parts) == 2 and parts[1]:
                    token = parts[1]
                    comment = token.find("//")
                    if comment != -1:
                        token = token[:comment]
                    return token.strip()
        self.fail("go/go.mod has no go directive (test fixture broken)")
        return ""  # unreachable

    def test_read_go_directive_matches_go_mod(self):
        expected = self._expected_directive()
        self.assertEqual(ve._read_go_directive(), expected)

    def test_render_go_go_mod_uses_directive_from_go_mod(self):
        expected = self._expected_directive()
        rendered = ve.render_go_go_mod()
        self.assertIn(f"\ngo {expected}\n", rendered)
        # Cross-check: the hardcoded literal must not have crept back in
        # unless go/go.mod actually says go 1.26.
        if expected != "1.26":
            self.assertNotIn("\ngo 1.26\n", rendered)

    def test_read_go_directive_missing_file_raises(self):
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as td:
            missing = Path(td) / "absent.mod"
            with self.assertRaises(ve.HarnessError) as ctx:
                ve._read_go_directive(missing)
            self.assertIn("not readable", str(ctx.exception))

    def test_read_go_directive_no_directive_raises(self):
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as td:
            empty = Path(td) / "go.mod"
            empty.write_text("module example\n", encoding="utf-8")
            with self.assertRaises(ve.HarnessError) as ctx:
                ve._read_go_directive(empty)
            self.assertIn("cannot find", str(ctx.exception))


class MainErrorReportingTests(unittest.TestCase):
    """``main()`` must convert ``HarnessError`` into a clean one-line failure.

    Before this fix, extraction errors propagated out of ``main()`` as a raw
    Python traceback. The contract is now: ``HarnessError`` is caught at the
    top of ``main()`` and reported through the existing ``die()`` helper,
    which prints a single ``verify_examples: <message>`` line to stderr and
    exits with status 1.
    """

    def test_main_reports_harness_error_via_die_no_traceback(self):
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as td:
            root = Path(td)
            body = (
                "<!-- verify-example: lang=rust id=lone -->\n"
                "ordinary text, not a fence\n"
            )
            (root / "Bad.md").write_text(body, encoding="utf-8")
            stderr_buf = io.StringIO()
            stdout_buf = io.StringIO()
            with contextlib.redirect_stderr(stderr_buf):
                with contextlib.redirect_stdout(stdout_buf):
                    with self.assertRaises(SystemExit) as exit_ctx:
                        ve.main(["--docs-dir", str(root)])
            self.assertEqual(exit_ctx.exception.code, 1)
            combined = stdout_buf.getvalue() + stderr_buf.getvalue()
            self.assertIn("verify_examples:", combined)
            self.assertIn("not followed by a fenced code block", combined)
            self.assertNotIn("Traceback (most recent call last)", combined)

    def test_main_reports_cross_file_duplicate_id_cleanly(self):
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as td:
            root = Path(td)
            page_a = (
                "<!-- verify-example: lang=rust id=shared -->\n"
                "```rust\nfn main() {}\n```\n"
            )
            page_b = (
                "<!-- verify-example: lang=rust id=shared -->\n"
                "```rust\nfn main() {}\n```\n"
            )
            (root / "A.md").write_text(page_a, encoding="utf-8")
            (root / "B.md").write_text(page_b, encoding="utf-8")
            stderr_buf = io.StringIO()
            stdout_buf = io.StringIO()
            with contextlib.redirect_stderr(stderr_buf):
                with contextlib.redirect_stdout(stdout_buf):
                    with self.assertRaises(SystemExit) as exit_ctx:
                        ve.main(["--docs-dir", str(root)])
            self.assertEqual(exit_ctx.exception.code, 1)
            combined = stdout_buf.getvalue() + stderr_buf.getvalue()
            self.assertIn("first seen at", combined)
            self.assertIn("A.md", combined)
            self.assertNotIn("Traceback (most recent call last)", combined)


class BuildTimeoutTests(unittest.TestCase):
    """Build helpers must catch ``subprocess.TimeoutExpired`` cleanly.

    Before this fix, a build that exceeded the configured timeout surfaced
    as an uncaught traceback out of ``_build_rust`` / ``_build_go``. The
    contract is now: a timeout returns a dict whose shape matches a
    completed run (with ``returncode`` set to ``None``), whose ``stderr``
    ends with a ``[harness timeout]`` marker, and whose ``duration_ms`` is
    populated. The mirror of this contract for per-example runs lives in
    ``_run_example``.
    """

    def test_build_rust_returns_timeout_outcome(self):
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as td:
            tmp = Path(td)

            def raise_timeout(*args, **kwargs):
                raise subprocess.TimeoutExpired(cmd=["cargo", "build"], timeout=1.0)

            original = ve.run_subprocess
            ve.run_subprocess = raise_timeout
            try:
                outcome = ve._build_rust(
                    rust_dir=tmp / "rust-src",
                    cache_dir=tmp / "caches",
                    env={},
                    timeout=1.0,
                )
            finally:
                ve.run_subprocess = original
            self.assertIsNone(outcome["returncode"])
            self.assertIn("[harness timeout]", outcome["stderr"])
            self.assertIsInstance(outcome["duration_ms"], int)
            self.assertGreaterEqual(outcome["duration_ms"], 0)
            self.assertEqual(outcome["stdout"], "")

    def test_build_go_returns_timeout_outcome(self):
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as td:
            tmp = Path(td)

            def raise_timeout(*args, **kwargs):
                raise subprocess.TimeoutExpired(cmd=["go", "build"], timeout=1.0)

            original = ve.run_subprocess
            ve.run_subprocess = raise_timeout
            try:
                outcome = ve._build_go(
                    go_dir=tmp / "go-src",
                    cache_dir=tmp / "caches",
                    env={},
                    timeout=1.0,
                    go_examples=[{"id": "x"}],
                )
            finally:
                ve.run_subprocess = original
            self.assertIsNone(outcome["returncode"])
            self.assertIn("[harness timeout]", outcome["stderr"])
            self.assertIsInstance(outcome["duration_ms"], int)
            self.assertGreaterEqual(outcome["duration_ms"], 0)
            self.assertEqual(outcome["stdout"], "")


class RustEditionTests(unittest.TestCase):
    """The generated example package must use the workspace's ``edition``.

    The workspace declares its edition under ``[workspace.package]`` in
    ``rust/Cargo.toml``. Hardcoding the example's edition in the harness
    silently diverges when the workspace bumps it; ``_read_rust_edition``
    scans the live manifest so the rendered ``Cargo.toml`` always agrees.
    """

    def _expected_edition(self) -> str:
        text = (REPO_ROOT / "rust" / "Cargo.toml").read_text(encoding="utf-8")
        in_section = False
        for raw in text.splitlines():
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            if line.startswith("["):
                in_section = (line == "[workspace.package]")
                continue
            if not in_section:
                continue
            if line.startswith("edition"):
                _, _, value = line.partition("=")
                value = value.strip()
                if value.startswith('"') and value.endswith('"'):
                    return value[1:-1]
        self.fail("rust/Cargo.toml has no edition under [workspace.package] (test fixture broken)")
        return ""  # unreachable

    def test_read_rust_edition_matches_workspace(self):
        expected = self._expected_edition()
        self.assertEqual(ve._read_rust_edition(), expected)

    def test_render_rust_cargo_toml_uses_workspace_edition(self):
        expected = self._expected_edition()
        rendered = ve.render_rust_cargo_toml()
        self.assertIn(f'\nedition = "{expected}"\n', rendered)
        # Cross-check: the hardcoded literal must not have crept back in
        # unless the workspace actually declares edition 2021.
        if expected != "2021":
            self.assertNotIn('\nedition = "2021"\n', rendered)

    def test_read_rust_edition_missing_file_raises(self):
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as td:
            missing = Path(td) / "Cargo.toml"
            with self.assertRaises(ve.HarnessError) as ctx:
                ve._read_rust_edition(missing)
            self.assertIn("not readable", str(ctx.exception))

    def test_read_rust_edition_no_edition_raises(self):
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as td:
            cargo = Path(td) / "Cargo.toml"
            cargo.write_text(
                "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"0.0.0\"\n",
                encoding="utf-8",
            )
            with self.assertRaises(ve.HarnessError) as ctx:
                ve._read_rust_edition(cargo)
            self.assertIn("edition", str(ctx.exception))
            self.assertIn("workspace.package", str(ctx.exception))

    def test_read_rust_edition_ignores_edition_outside_workspace_package(self):
        """An ``edition`` under a non-workspace section must not be picked up.

        Workspace members commonly re-declare ``[package]`` with their own
        ``edition`` for older crates; the parser must restrict itself to
        the ``[workspace.package]`` table the SDK uses as the source of
        truth.
        """
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as td:
            cargo = Path(td) / "Cargo.toml"
            cargo.write_text(
                "[workspace]\nmembers = [\"x\"]\n\n"
                "[workspace.package]\nversion = \"0.0.0\"\nedition = \"2024\"\n\n"
                "[package]\nedition = \"2021\"\n",
                encoding="utf-8",
            )
            self.assertEqual(ve._read_rust_edition(cargo), "2024")


if __name__ == "__main__":
    unittest.main()
