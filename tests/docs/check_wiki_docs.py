#!/usr/bin/env python3
from __future__ import annotations

import argparse
import importlib.util
import os
import re
import sys
from pathlib import Path
from typing import Iterable


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_DOCS_DIR = REPO_ROOT / "docs"
DOCS_DIR = DEFAULT_DOCS_DIR

MARKDOWN_LINK_RE = re.compile(r"\[[^\]]+\]\(([^)]+)\)")
WIKI_LINK_RE = re.compile(r"\[\[([^\]]+)\]\]")
FENCED_CODE_RE = re.compile(r"```.*?```", re.DOTALL)
LOCAL_HOME_PATH_RE = re.compile(r"/home/[A-Za-z0-9._-]+(?:/|\b)")
EXTRA_FORBIDDEN_TERMS = tuple(
    term.strip() for term in os.getenv("DOCS_FORBIDDEN_TERMS", "").split(",") if term.strip()
)
EXTRA_FORBIDDEN_RE = (
    re.compile(
        r"\b(?:%s)\b" % "|".join(re.escape(term) for term in EXTRA_FORBIDDEN_TERMS),
        re.IGNORECASE,
    )
    if EXTRA_FORBIDDEN_TERMS
    else None
)


def fail(message: str) -> None:
    print(f"docs validation failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def markdown_files(docs_dir: Path) -> list[Path]:
    return sorted(docs_dir.glob("*.md"))


def check_required(files: list[Path]) -> None:
    names = {path.name for path in files}
    for required in {"Home.md", "_Sidebar.md"}:
        if required not in names:
            fail(f"missing docs/{required}")


def link_target_exists(source: Path, target: str, page_names: set[str]) -> bool:
    target = target.strip()
    if not target or target.startswith("#"):
        return True
    if "://" in target or target.startswith("mailto:"):
        return True
    target = target.split("#", 1)[0]
    target = target.split("?", 1)[0]
    if not target:
        return True
    if target.endswith(".md"):
        fail(
            f"{source.relative_to(REPO_ROOT)} uses repository Markdown link "
            f"{target!r}; use a GitHub wiki page link instead"
        )
    return target in page_names


def check_markdown_links(path: Path, page_names: set[str]) -> None:
    text = FENCED_CODE_RE.sub("", path.read_text(encoding="utf-8"))
    for match in MARKDOWN_LINK_RE.finditer(text):
        target = match.group(1)
        if not link_target_exists(path, target, page_names):
            fail(f"{path.relative_to(REPO_ROOT)} links to missing target {target!r}")


def check_wiki_links(path: Path, page_names: set[str]) -> None:
    text = FENCED_CODE_RE.sub("", path.read_text(encoding="utf-8"))
    for match in WIKI_LINK_RE.finditer(text):
        raw = match.group(1)
        target = raw.split("|", 1)[0].strip()
        if target not in page_names:
            fail(f"{path.relative_to(REPO_ROOT)} links to missing wiki page {target!r}")


def check_forbidden_text(path: Path) -> None:
    text = path.read_text(encoding="utf-8")
    checks = (
        ("local home path", LOCAL_HOME_PATH_RE),
        ("configured forbidden term", EXTRA_FORBIDDEN_RE),
    )
    for label, pattern in checks:
        if pattern is None:
            continue
        match = pattern.search(text)
        if match:
            line = text.count("\n", 0, match.start()) + 1
            fail(
                f"{path.relative_to(REPO_ROOT)} contains forbidden local/private "
                f"text ({label}) on line {line}"
            )


# ---------------------------------------------------------------------------
# Verified-example marker check
# ---------------------------------------------------------------------------
#
# This check enforces the marker convention documented in
# `docs/Wiki-Publishing.md`: every supported-language fenced code block in
# `docs/*.md` must be immediately preceded (ignoring blank lines) by either a
# `<!-- verify-example: ... -->` marker or an `<!-- illustrative-only: ... -->`
# marker, and verify-example markers must carry valid, unique attributes.
#
# Marker grammar is the same one implemented by `tests/docs/verify_examples.py`
# so the validator and the harness cannot drift apart. The harness module is
# loaded read-only and is only used for its pure grammar constants and
# `parse_marker` function.
#
# The check tracks fenced-block state while scanning: lines that appear
# inside another fenced block are content, not markers or fences. The
# documented marker syntax is itself shown inside ```markdown fences in
# `docs/Wiki-Publishing.md`; those literal lines must be ignored, must not
# require a following supported-language fence, and must not count toward id
# uniqueness.


def _load_grammar():
    """Load pure grammar constants and ``parse_marker`` from the harness.

    The harness module is loaded as a side-effect-free import. It is never
    invoked; only its regex constants and ``parse_marker`` function are used.
    """
    harness_path = Path(__file__).resolve().parent / "verify_examples.py"
    spec = importlib.util.spec_from_file_location("_verify_examples_grammar", harness_path)
    if spec is None or spec.loader is None:
        fail(f"could not load verify_examples.py from {harness_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    grammar = {
        "MARKER_RE": module.MARKER_RE,
        "FENCE_OPEN_RE": module.FENCE_OPEN_RE,
        "ID_SLUG_RE": module.ID_SLUG_RE,
        "SUPPORTED_LANGS": module.SUPPORTED_LANGS,
        "KIND_VERIFY": module.KIND_VERIFY,
        "KIND_ILLUSTRATIVE": module.KIND_ILLUSTRATIVE,
        "parse_marker": module.parse_marker,
    }
    return grammar


_GRAMMAR = _load_grammar()


def _fence_info_lang(info: str) -> str:
    """Return the lower-cased first whitespace-separated token of ``info``.

    Returns an empty string when ``info`` is empty or whitespace-only.
    """
    stripped = (info or "").strip()
    if not stripped:
        return ""
    return stripped.split()[0].lower()


def _iter_fence_aware(lines: list[str], fence_open_re: re.Pattern[str]):
    """Yield (line_index, kind, payload) for marker and fence events.

    ``kind`` is one of ``"marker"`` or ``"fence"``. ``payload`` is either a
    marker tuple ``(kind_str, attrs_dict, reason_str)`` or a fence tuple
    ``(info_string, fence_char, fence_len)``. Lines that appear inside an
    open fenced block are skipped entirely so that literal markers and
    fence-looking lines inside ```markdown blocks are not treated as real
    markers or fences.
    """
    in_fence = False
    fence_char = ""
    fence_len = 0
    for idx, line in enumerate(lines):
        if in_fence:
            if re.match(
                rf"^(\s*)({re.escape(fence_char)}{{{fence_len},}})\s*$",
                line,
            ):
                in_fence = False
            continue
        fence_match = fence_open_re.match(line)
        if fence_match:
            yield idx, "fence", (
                fence_match.group("info") or "",
                fence_match.group("fence")[0],
                len(fence_match.group("fence")),
            )
            in_fence = True
            fence_char = fence_match.group("fence")[0]
            fence_len = len(fence_match.group("fence"))
            continue
        marker = _GRAMMAR["parse_marker"](line)
        if marker is None:
            continue
        kind_str, attrs, reason = marker
        if kind_str not in (_GRAMMAR["KIND_VERIFY"], _GRAMMAR["KIND_ILLUSTRATIVE"]):
            continue
        yield idx, "marker", (kind_str, attrs, reason)


def _find_preceding_marker(
    lines: list[str],
    fence_line: int,
    fence_open_re: re.Pattern[str],
) -> tuple[int, str, dict[str, str]] | None:
    """Find the marker line (ignoring blanks) immediately above ``fence_line``.

    Returns the line index, the marker kind, and the parsed marker payload.
    Returns None when no marker is found before the fence.
    """
    j = fence_line - 1
    while j >= 0 and lines[j].strip() == "":
        j -= 1
    if j < 0:
        return None
    candidate = _GRAMMAR["parse_marker"](lines[j])
    if candidate is None:
        return None
    kind_str, attrs, _reason = candidate
    if kind_str not in (_GRAMMAR["KIND_VERIFY"], _GRAMMAR["KIND_ILLUSTRATIVE"]):
        return None
    return j, kind_str, attrs


def check_verified_example_markers(
    paths: Iterable[Path],
    *,
    seen_ids: dict[str, str],
) -> None:
    """Reject unmarked supported-language fences and invalid markers.

    ``seen_ids`` is a per-run map of already-recorded verify-example ids to
    the file:line of their first occurrence, used to enforce global id
    uniqueness across all of ``docs/``. The caller is expected to start
    with an empty dict and to invoke this function once for every file.
    """
    fence_open_re = _GRAMMAR["FENCE_OPEN_RE"]
    supported_langs = _GRAMMAR["SUPPORTED_LANGS"]

    for path in paths:
        rel = _rel_label(path)
        text = path.read_text(encoding="utf-8")
        lines = text.splitlines()

        for idx, kind, payload in _iter_fence_aware(lines, fence_open_re):
            if kind == "marker":
                continue

            info_text, _fence_char, _fence_len = payload
            lang = _fence_info_lang(info_text)
            if lang not in supported_langs:
                continue
            preceding = _find_preceding_marker(lines, idx, fence_open_re)
            if preceding is None:
                fail(
                    f"{rel}:{idx + 1}: {lang} fenced code block is not preceded by "
                    f"a verify-example or illustrative-only marker"
                )
            _check_supported_fence_marker(rel, lang, preceding, seen_ids)


def _check_supported_fence_marker(
    rel: str,
    lang: str,
    marker: tuple[int, str, dict[str, str]],
    seen_ids: dict[str, str],
) -> None:
    id_slug_re = _GRAMMAR["ID_SLUG_RE"]
    kind_verify = _GRAMMAR["KIND_VERIFY"]
    kind_illustrative = _GRAMMAR["KIND_ILLUSTRATIVE"]
    marker_line, marker_kind, attrs = marker
    if marker_kind == kind_illustrative:
        # Illustrative markers are allowed before any supported language fence.
        return
    if marker_kind != kind_verify:
        return
    for required_key in ("lang", "id"):
        if required_key not in attrs:
            fail(
                f"{rel}:{marker_line + 1}: verify-example marker is missing "
                f"required {required_key!r} attribute"
            )
    if attrs.get("lang") != lang:
        fail(
            f"{rel}:{marker_line + 1}: verify-example lang {attrs.get('lang')!r} "
            f"does not match fence lang {lang!r}"
        )
    ex_id = attrs.get("id", "")
    if not id_slug_re.match(ex_id):
        fail(
            f"{rel}:{marker_line + 1}: verify-example id {ex_id!r} "
            f"must match [a-z0-9-]+"
        )
    if ex_id in seen_ids:
        fail(
            f"{rel}:{marker_line + 1}: duplicate verify-example id {ex_id!r} "
            f"(first seen at {seen_ids[ex_id]})"
        )
    seen_ids[ex_id] = f"{rel}:{marker_line + 1}"


def check_verify_example_markers_followed_by_fence(
    paths: Iterable[Path],
) -> None:
    """Reject verify-example markers without a supported-language fence.

    Markers inside fenced blocks are content and are ignored. The check
    walks each file, ignoring content inside any fenced block, and reports
    a verify-example marker as an error if the next non-blank, non-marker
    line is not a supported-language fence opener.
    """
    fence_open_re = _GRAMMAR["FENCE_OPEN_RE"]
    supported_langs = _GRAMMAR["SUPPORTED_LANGS"]
    kind_verify = _GRAMMAR["KIND_VERIFY"]

    for path in paths:
        rel = _rel_label(path)
        text = path.read_text(encoding="utf-8")
        lines = text.splitlines()

        for idx, kind, payload in _iter_fence_aware(lines, fence_open_re):
            if kind != "marker":
                continue
            marker_kind, _attrs, _reason = payload
            if marker_kind != kind_verify:
                continue
            # Find the next non-blank line below the marker.
            j = idx + 1
            while j < len(lines) and lines[j].strip() == "":
                j += 1
            if j >= len(lines):
                fail(
                    f"{rel}:{idx + 1}: verify-example marker is not followed "
                    f"by any fenced code block"
                )
                continue
            next_fence = fence_open_re.match(lines[j])
            if next_fence is None:
                fail(
                    f"{rel}:{idx + 1}: verify-example marker is not followed by "
                    f"a fenced code block (next non-blank line {j + 1} is not a fence)"
                )
                continue
            next_lang = _fence_info_lang(next_fence.group("info") or "")
            if next_lang not in supported_langs:
                fail(
                    f"{rel}:{idx + 1}: verify-example marker is followed by a "
                    f"{next_lang!r} fence; expected one of {', '.join(supported_langs)}"
                )


def _rel_label(path: Path) -> str:
    """Return a repo-relative label for ``path`` or its absolute path.

    Paths outside the repository (for example, temporary directories used
    by unit tests) are reported by their absolute path so the new check
    remains testable without copying fixtures under the repo.
    """
    try:
        return str(path.relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


def main(argv: list[str] | None = None) -> int:
    global DOCS_DIR
    parser = argparse.ArgumentParser(
        description="Validate consumer wiki markdown files in docs/."
    )
    parser.add_argument(
        "--docs-dir",
        default=str(DEFAULT_DOCS_DIR),
        help="wiki markdown directory to validate (default: docs/)",
    )
    args = parser.parse_args(argv)

    DOCS_DIR = Path(args.docs_dir).resolve()
    if not DOCS_DIR.is_dir():
        fail("missing docs directory")
    files = markdown_files(DOCS_DIR)
    check_required(files)
    page_names = {path.stem for path in files}
    for path in files:
        check_markdown_links(path, page_names)
        check_wiki_links(path, page_names)
        check_forbidden_text(path)
    seen_ids: dict[str, str] = {}
    check_verified_example_markers(files, seen_ids=seen_ids)
    check_verify_example_markers_followed_by_fence(files)
    print(f"validated {len(files)} wiki markdown files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
