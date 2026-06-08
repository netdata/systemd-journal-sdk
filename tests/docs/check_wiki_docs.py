#!/usr/bin/env python3
from __future__ import annotations

import os
import re
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
DOCS_DIR = REPO_ROOT / "docs"

MARKDOWN_LINK_RE = re.compile(r"\[[^\]]+\]\(([^)]+)\)")
WIKI_LINK_RE = re.compile(r"\[\[([^\]]+)\]\]")
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


def markdown_files() -> list[Path]:
    return sorted(DOCS_DIR.glob("*.md"))


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
        resolved = (source.parent / target).resolve()
        try:
            resolved.relative_to(DOCS_DIR)
        except ValueError:
            return False
        return resolved.is_file()
    return target in page_names


def check_markdown_links(path: Path, page_names: set[str]) -> None:
    text = path.read_text(encoding="utf-8")
    for match in MARKDOWN_LINK_RE.finditer(text):
        target = match.group(1)
        if not link_target_exists(path, target, page_names):
            fail(f"{path.relative_to(REPO_ROOT)} links to missing target {target!r}")


def check_wiki_links(path: Path, page_names: set[str]) -> None:
    text = path.read_text(encoding="utf-8")
    for match in WIKI_LINK_RE.finditer(text):
        raw = match.group(1)
        target = raw.split("|")[-1].strip()
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


def main() -> int:
    if not DOCS_DIR.is_dir():
        fail("missing docs directory")
    files = markdown_files()
    check_required(files)
    page_names = {path.stem for path in files}
    for path in files:
        check_markdown_links(path, page_names)
        check_wiki_links(path, page_names)
        check_forbidden_text(path)
    print(f"validated {len(files)} wiki markdown files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
