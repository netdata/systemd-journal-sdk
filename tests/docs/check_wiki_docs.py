#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
DOCS_DIR = REPO_ROOT / "docs"

MARKDOWN_LINK_RE = re.compile(r"\[[^\]]+\]\(([^)]+)\)")
WIKI_LINK_RE = re.compile(r"\[\[([^\]]+)\]\]")
FORBIDDEN_RE = re.compile(r"(/home/[A-Za-z0-9._-]+(?:/|\b)|\bcosta\b)", re.IGNORECASE)


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
        return (source.parent / target).resolve().is_file()
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
    match = FORBIDDEN_RE.search(text)
    if match:
        fail(f"{path.relative_to(REPO_ROOT)} contains forbidden local/private text")


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
