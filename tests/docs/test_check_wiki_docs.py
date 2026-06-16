#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Unit tests for ``tests/docs/check_wiki_docs.py``.

These tests exercise the new verified-example marker check added to the
wiki validator. They are stdlib-only and do not depend on the real
``docs/`` content: every test creates a temporary ``docs/``-shaped
directory, drops one or more markdown files into it, and invokes the
public helpers directly.

The new check's failure helper is the same ``fail()`` used by the rest
of the validator, which calls ``SystemExit(1)`` after writing a
diagnostic. Tests use ``assertRaises(SystemExit)`` to catch that.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import sys
import unittest
from pathlib import Path
from typing import Iterator

REPO_ROOT = Path(__file__).resolve().parents[2]
VALIDATOR_PATH = REPO_ROOT / "tests" / "docs" / "check_wiki_docs.py"


def _load_validator():
    spec = importlib.util.spec_from_file_location("check_wiki_docs_for_tests", VALIDATOR_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load validator from {VALIDATOR_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


cwd = _load_validator()


@contextlib.contextmanager
def _tmp_docs(file_specs: list[tuple[str, str]]) -> Iterator[Path]:
    """Create a temporary ``docs/`` directory with the given (name, body) files.

    Also writes the ``Home.md`` and ``_Sidebar.md`` stubs that the existing
    ``check_required`` step demands, so the new check can be exercised in
    isolation without triggering the pre-existing link/text checks.
    """
    import tempfile

    with tempfile.TemporaryDirectory() as td:
        root = Path(td) / "docs"
        root.mkdir(parents=True, exist_ok=True)
        (root / "Home.md").write_text("# Home\n", encoding="utf-8")
        (root / "_Sidebar.md").write_text("# Sidebar\n", encoding="utf-8")
        for name, body in file_specs:
            (root / name).write_text(body, encoding="utf-8")
        yield root


def _run_marker_check(docs_dir: Path):
    """Run the new marker checks against a temporary docs dir.

    The validator's ``main()`` also runs the legacy link and forbidden
    text checks. By invoking the new helpers directly with only the
    files in the temporary directory we keep each test focused on the
    new check.
    """
    files = sorted(docs_dir.glob("*.md"))
    seen_ids: dict[str, str] = {}
    cwd.check_verified_example_markers(files, seen_ids=seen_ids)
    cwd.check_verify_example_markers_followed_by_fence(files)
    return seen_ids


def _assert_fails_with(check_callable) -> str:
    """Run ``check_callable`` while capturing ``fail()`` output.

    Returns the captured stderr text. Raises ``AssertionError`` if the
    call did not raise ``SystemExit``.
    """
    buf = io.StringIO()
    try:
        with contextlib.redirect_stderr(buf):
            check_callable()
    except SystemExit:
        return buf.getvalue()
    raise AssertionError("check_callable() did not call fail()")


class MarkedFenceTests(unittest.TestCase):
    def test_verify_marker_before_rust_fence_passes(self):
        body = (
            "Intro text.\n"
            "\n"
            "<!-- verify-example: lang=rust id=demo-rust -->\n"
            "```rust\n"
            "fn main() {}\n"
            "```\n"
        )
        with _tmp_docs([("Demo.md", body)]) as docs:
            seen = _run_marker_check(docs)
        self.assertEqual(set(seen), {"demo-rust"})
        # The recorded label includes the file name and the marker line
        # number; the exact prefix depends on whether the temp dir lives
        # inside the repository, so we only check the trailing fragment.
        self.assertTrue(seen["demo-rust"].endswith("Demo.md:3"))

    def test_verify_marker_before_go_fence_passes(self):
        body = (
            "<!-- verify-example: lang=go id=demo-go -->\n"
            "```go\n"
            "package main\n"
            "```\n"
        )
        with _tmp_docs([("Go.md", body)]) as docs:
            _run_marker_check(docs)

    def test_illustrative_marker_before_rust_fence_passes(self):
        body = (
            "<!-- illustrative-only: not compilable on its own -->\n"
            "```rust\n"
            "fn main() {}\n"
            "```\n"
        )
        with _tmp_docs([("Rust.md", body)]) as docs:
            _run_marker_check(docs)

    def test_unmarked_rust_fence_fails(self):
        body = "```rust\nfn main() {}\n```\n"
        with _tmp_docs([("Bad.md", body)]) as docs:
            msg = _assert_fails_with(lambda: _run_marker_check(docs))
        self.assertIn("Bad.md", msg)
        self.assertIn("rust", msg)

    def test_unmarked_go_fence_fails(self):
        body = "```go\npackage main\n```\n"
        with _tmp_docs([("Bad.md", body)]) as docs:
            msg = _assert_fails_with(lambda: _run_marker_check(docs))
        self.assertIn("go", msg)

    def test_blank_lines_between_marker_and_fence_are_allowed(self):
        body = (
            "<!-- verify-example: lang=rust id=spaced -->\n"
            "\n"
            "\n"
            "```rust\n"
            "fn main() {}\n"
            "```\n"
        )
        with _tmp_docs([("Spaced.md", body)]) as docs:
            _run_marker_check(docs)

    def test_marker_lang_mismatch_fails(self):
        body = (
            "<!-- verify-example: lang=go id=mixed -->\n"
            "```rust\n"
            "fn main() {}\n"
            "```\n"
        )
        with _tmp_docs([("Mixed.md", body)]) as docs:
            msg = _assert_fails_with(lambda: _run_marker_check(docs))
        self.assertIn("does not match fence lang", msg)

    def test_marker_missing_lang_attribute_fails(self):
        body = (
            "<!-- verify-example: id=missing-lang -->\n"
            "```rust\n"
            "fn main() {}\n"
            "```\n"
        )
        with _tmp_docs([("MissingLang.md", body)]) as docs:
            msg = _assert_fails_with(lambda: _run_marker_check(docs))
        self.assertIn("missing required 'lang'", msg)

    def test_marker_missing_id_attribute_fails(self):
        body = (
            "<!-- verify-example: lang=rust -->\n"
            "```rust\n"
            "fn main() {}\n"
            "```\n"
        )
        with _tmp_docs([("MissingId.md", body)]) as docs:
            msg = _assert_fails_with(lambda: _run_marker_check(docs))
        self.assertIn("missing required 'id'", msg)

    def test_marker_id_with_invalid_slug_fails(self):
        body = (
            "<!-- verify-example: lang=rust id=Bad_ID -->\n"
            "```rust\n"
            "fn main() {}\n"
            "```\n"
        )
        with _tmp_docs([("BadId.md", body)]) as docs:
            msg = _assert_fails_with(lambda: _run_marker_check(docs))
        self.assertIn("must match [a-z0-9-]+", msg)

    def test_duplicate_id_across_files_fails(self):
        a = (
            "<!-- verify-example: lang=rust id=duplicated -->\n"
            "```rust\nfn main() {}\n```\n"
        )
        b = (
            "<!-- verify-example: lang=rust id=duplicated -->\n"
            "```rust\nfn main() {}\n```\n"
        )
        with _tmp_docs([("A.md", a), ("B.md", b)]) as docs:
            msg = _assert_fails_with(lambda: _run_marker_check(docs))
        self.assertIn("duplicate verify-example id", msg)
        self.assertIn("duplicated", msg)

    def test_unique_ids_across_files_pass(self):
        a = (
            "<!-- verify-example: lang=rust id=first -->\n"
            "```rust\nfn main() {}\n```\n"
        )
        b = (
            "<!-- verify-example: lang=go id=second -->\n"
            "```go\npackage main\n```\n"
        )
        with _tmp_docs([("A.md", a), ("B.md", b)]) as docs:
            seen = _run_marker_check(docs)
        self.assertEqual(set(seen), {"first", "second"})

    def test_verify_marker_without_any_fence_fails(self):
        body = "Intro\n<!-- verify-example: lang=rust id=lonely -->\nMore text\n"
        with _tmp_docs([("Lonely.md", body)]) as docs:
            msg = _assert_fails_with(lambda: _run_marker_check(docs))
        self.assertIn("not followed by a fenced code block", msg)

    def test_verify_marker_followed_by_non_fence_fails(self):
        body = (
            "<!-- verify-example: lang=rust id=not-a-fence -->\n"
            "ordinary paragraph line\n"
        )
        with _tmp_docs([("NoFence.md", body)]) as docs:
            msg = _assert_fails_with(lambda: _run_marker_check(docs))
        self.assertIn("not followed by a fenced code block", msg)

    def test_verify_marker_followed_by_toml_fence_fails(self):
        body = (
            "<!-- verify-example: lang=rust id=wrong-lang -->\n"
            "```toml\n"
            "x = 1\n"
            "```\n"
        )
        with _tmp_docs([("WrongLang.md", body)]) as docs:
            msg = _assert_fails_with(lambda: _run_marker_check(docs))
        self.assertIn("expected one of rust, go", msg)

    def test_illustrative_marker_does_not_consume_id_uniqueness(self):
        # Two illustrative-only markers with similar text in different docs
        # must not collide, because they do not carry verify-example ids.
        a = (
            "<!-- illustrative-only: pseudo -->\n"
            "```rust\nfn main() {}\n```\n"
        )
        b = (
            "<!-- illustrative-only: pseudo -->\n"
            "```rust\nfn main() {}\n```\n"
        )
        with _tmp_docs([("A.md", a), ("B.md", b)]) as docs:
            _run_marker_check(docs)

    def test_non_supported_languages_are_exempt(self):
        # Only Rust and Go are product documentation languages. Other fences
        # are exempt from verified-example markers.
        body = (
            "```toml\nx = 1\n```\n"
            "```sh\necho hi\n```\n"
            "```json\n{\"a\": 1}\n```\n"
            "```text\nplain\n```\n"
            "```js\nconsole.log(1)\n```\n"
            "```python\nprint(1)\n```\n"
            "```javascript\nconsole.log(1)\n```\n"
        )
        with _tmp_docs([("Exempt.md", body)]) as docs:
            _run_marker_check(docs)

    def test_go_fence_requires_marker(self):
        body = "```go\npackage main\n```\n"
        with _tmp_docs([("Go.md", body)]) as docs:
            msg = _assert_fails_with(lambda: _run_marker_check(docs))
        self.assertIn("go fenced code block is not preceded", msg)

    def test_failure_message_includes_file_and_line(self):
        body = (
            "intro line\n"
            "second line\n"
            "```rust\nfn main() {}\n```\n"
        )
        with _tmp_docs([("LineAt.md", body)]) as docs:
            msg = _assert_fails_with(lambda: _run_marker_check(docs))
        self.assertIn("LineAt.md", msg)
        # Line 3 of the file is the fence opener.
        self.assertIn(":3:", msg)

    def test_illustrative_only_with_no_following_supported_fence_is_fine(self):
        # An illustrative-only marker that is not followed by a fence
        # (or followed by an unsupported fence) must not be flagged; the
        # check only requires that verify-example markers point to supported
        # language fences, and that supported-language fences are marked.
        body = (
            "<!-- illustrative-only: just a textual example -->\n"
            "```text\n"
            "some words\n"
            "```\n"
        )
        with _tmp_docs([("Ill.md", body)]) as docs:
            _run_marker_check(docs)


class FenceAwareTests(unittest.TestCase):
    """Markers and fence-looking lines inside another fenced block are content."""

    def test_marker_inside_markdown_fence_is_ignored(self):
        # This is the Wiki-Publishing.md case: the marker syntax is
        # documented inside a ```markdown block. Those literal lines
        # must not be treated as real markers, must not require a
        # following supported-language fence, and must not count toward id
        # uniqueness.
        body = (
            "## Verified Examples\n"
            "\n"
            "```markdown\n"
            "<!-- verify-example: lang=rust id=read-one-file -->\n"
            "```\n"
            "\n"
            "That was just a literal example of the marker syntax.\n"
        )
        with _tmp_docs([("Wiki-Publishing.md", body)]) as docs:
            seen = _run_marker_check(docs)
        # The literal id is in the markdown fence and must not register.
        self.assertNotIn("read-one-file", seen)

    def test_fence_looking_line_inside_markdown_fence_is_ignored(self):
        # A literal ```rust``` line inside a ```markdown block must
        # not be parsed as a rust fence that needs a marker.
        body = (
            "```markdown\n"
            "Use ```rust for rust examples.\n"
            "```\n"
        )
        with _tmp_docs([("Mention.md", body)]) as docs:
            _run_marker_check(docs)

    def test_marker_inside_fence_is_not_required_to_have_following_fence(self):
        # A verify-example marker literal inside a ```markdown fence
        # must not be reported as a marker that lacks a following
        # supported-language fence.
        body = (
            "```markdown\n"
            "<!-- verify-example: lang=rust id=phantom -->\n"
            "```\n"
        )
        with _tmp_docs([("Phantom.md", body)]) as docs:
            _run_marker_check(docs)

    def test_real_marker_after_fenced_block_still_validates(self):
        # A literal marker inside a markdown fence must not satisfy
        # the marker requirement for a real rust fence that follows
        # the markdown fence.
        body = (
            "```markdown\n"
            "<!-- verify-example: lang=rust id=fake -->\n"
            "```\n"
            "\n"
            "```rust\n"
            "fn main() {}\n"
            "```\n"
        )
        with _tmp_docs([("Mixed.md", body)]) as docs:
            msg = _assert_fails_with(lambda: _run_marker_check(docs))
        self.assertIn("not preceded by", msg)

    def test_nested_fence_aware_iteration_handles_three_backticks(self):
        # Using three backticks consistently, the literal example
        # must not require a marker.
        body = (
            "```markdown\n"
            "```rust\n"
            "fn main() {}\n"
            "```\n"
            "```\n"
        )
        with _tmp_docs([("Three.md", body)]) as docs:
            _run_marker_check(docs)


class MainEntrypointTests(unittest.TestCase):
    """The ``--docs-dir`` hook makes the new check runnable in tests."""

    def test_main_accepts_docs_dir_argument(self):
        body = "```rust\nfn main() {}\n```\n"
        with _tmp_docs([("Real.md", body)]) as docs:
            buf = io.StringIO()
            with contextlib.redirect_stderr(buf):
                with self.assertRaises(SystemExit):
                    cwd.main(["--docs-dir", str(docs)])
        self.assertIn("Real.md", buf.getvalue())

    def test_main_passes_with_clean_docs(self):
        body = (
            "<!-- verify-example: lang=rust id=clean -->\n"
            "```rust\nfn main() {}\n```\n"
        )
        with _tmp_docs([("Clean.md", body)]) as docs:
            with contextlib.redirect_stdout(io.StringIO()):
                rc = cwd.main(["--docs-dir", str(docs)])
        self.assertEqual(rc, 0)


if __name__ == "__main__":
    unittest.main()
