#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Verify marked code examples extracted from wiki markdown pages.

The harness is the machine that keeps documentation examples honest. It
extracts marked fenced code examples from wiki markdown pages, materializes
them as real programs compiled against the LOCAL Rust workspace and Go module,
runs them against synthetic journal fixtures, and fails on any error.

The script is stdlib-only. It honors the runtime purity rules from
`AGENTS.md` and never probes host identity, real journal paths, or
system state. All scratch data lives under `.local/docs-examples/` and is
wiped per run (sources can be kept with `--keep`).

Known limitations
-----------------

(a) Go import detection and Rust ``?``-detection are substring heuristics.
    `detect_go_imports` scans the body for ``journal.``, ``fmt.``, ``time.``,
    and similar prefixes; `_rust_needs_result` scans for the literal ``?``
    character. A string literal, a comment, or a doc URL inside an example
    body that contains one of these substrings can therefore trigger an
    unused-import error from `go build` or wrap the Rust example into a
    `Result`-returning `main()` that never uses ``?``. Example authors must
    keep snippets free of such incidental matches. The build fails loudly
    in both cases (`go build` rejects unused imports; `cargo build` rejects
    a `Result`-returning `main` whose body has no `?`); the harness never
    silently swallows a wrap mismatch.

(b) The harness is a single-instance tool. Concurrent runs from the same
    repository checkout share `.local/docs-examples/` (generated sources,
    fixtures, scratch directories) and will race on the same files. Run
    one verification at a time per checkout.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import textwrap
import time
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
PYTHON_PKG_ROOT = REPO_ROOT / "python"
RUST_JOURNAL_PKG = REPO_ROOT / "rust" / "src" / "journal"
RUST_LOG_WRITER_PKG = REPO_ROOT / "rust" / "src" / "crates" / "journal-log-writer"
GO_MODULE_DIR = REPO_ROOT / "go"
WORK_ROOT = REPO_ROOT / ".local" / "docs-examples"

SUPPORTED_LANGS = ("rust", "go")
ID_SLUG_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
MARKER_RE = re.compile(
    r"<!--\s*(?P<kind>verify-example|illustrative-only)\s*(?P<rest>.*?)\s*-->"
)
FENCE_OPEN_RE = re.compile(r"^(\s*)(?P<fence>`{3,}|~{3,})(?P<info>[^\n]*)$")
FENCE_CLOSE_RE_TEMPLATE = r"^(\s*)(?P<fence>{fence})\s*$"
KIND_VERIFY = "verify-example"
KIND_ILLUSTRATIVE = "illustrative-only"

VERIFY_PREFIXES = (
    "/var/log/journal/example/system.journal",
    "/var/log/journal-sdk/example.journal",
    "/var/log/journal-sdk",
    "/var/log/journal",
    "/tmp/example.journal",
)
VERIFY_REPLACEMENTS = (
    "<FIXTURES>/basic/file/system.journal",
    "<SCRATCH>/example.journal",
    "<SCRATCH>",
    "<FIXTURES>/basic/dir",
    "<SCRATCH>/tmp-example.journal",
)

GO_IMPORT_PREFIXES = (
    ("journal.", "journal", "github.com/netdata/systemd-journal-sdk/go/journal"),
    ("fmt.", "fmt", "fmt"),
    ("bytes.", "bytes", "bytes"),
    ("json.", "json", "encoding/json"),
    ("time.", "time", "time"),
    ("os.", "os", "os"),
)
GO_REQUIRED_IMPORTS = ("fmt", "os")

RUST_HIDDEN_PREFIX = "# "
PRELUDES: dict[tuple[str, str], str] = {
    ("go", "open-reader"): textwrap.dedent(
        """\
        r, err := journal.OpenFile("/var/log/journal/example/system.journal")
        if err != nil {
            return err
        }
        defer r.Close()
        r.SeekHead()
        """
    ),
    ("go", "open-writer"): textwrap.dedent(
        """\
        w, err := journal.Create("/var/log/journal-sdk/example.journal", journal.Options{})
        if err != nil {
            return err
        }
        defer w.Close()
        """
    ),
    ("rust", "netdata-config-imports"): textwrap.dedent(
        """\
        use journal::netdata::{NetdataFunctionConfig, NetdataJournalFunction, SystemdJournalProfile};
        """
    ),
}


# ---------------------------------------------------------------------------
# Errors and small utilities
# ---------------------------------------------------------------------------


class HarnessError(Exception):
    """Fatal harness error. Extraction or generation bugs raise this."""


def die(message: str) -> None:
    """Print a fatal message to stderr and exit with status 1."""
    print(f"verify_examples: {message}", file=sys.stderr)
    raise SystemExit(1)


def info(message: str) -> None:
    print(f"verify_examples: {message}", file=sys.stderr)


def run_subprocess(args: list[str], *, cwd: Path, env: dict[str, str] | None = None,
                   timeout: float | None = None) -> subprocess.CompletedProcess[str]:
    """Run a subprocess, echoing the command line to stderr first."""
    printable = " ".join(_quote_token(t) for t in args)
    print(f"+ {printable}", file=sys.stderr)
    full_env = os.environ.copy()
    if env:
        full_env.update(env)
    return subprocess.run(
        args,
        cwd=str(cwd),
        env=full_env,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
    )


def _quote_token(tok: str) -> str:
    if not tok or any(ch in tok for ch in (" ", "\t", "\n", "\"")):
        return f'"{tok}"'
    return tok


def rmtree(path: Path) -> None:
    if path.is_dir():
        shutil.rmtree(path)


def ensure_clean_dir(path: Path) -> None:
    rmtree(path)
    path.mkdir(parents=True, exist_ok=True)


# ---------------------------------------------------------------------------
# Marker parsing
# ---------------------------------------------------------------------------


def parse_marker(line: str) -> tuple[str, dict[str, str], str] | None:
    """Parse a marker line into (kind, attributes, raw_reason).

    Returns None when the line is not a marker. For ``illustrative-only`` the
    third element is the free-text reason with the leading separator
    (colon or whitespace) stripped.
    """
    match = MARKER_RE.search(line)
    if not match:
        return None
    kind = match.group("kind")
    rest = match.group("rest").strip()
    if kind == KIND_VERIFY:
        attrs: dict[str, str] = {}
        for token in rest.split():
            if "=" not in token:
                continue
            key, _, value = token.partition("=")
            key = key.strip()
            value = value.strip()
            if not key:
                continue
            attrs[key] = value
        return kind, attrs, ""
    if kind == KIND_ILLUSTRATIVE:
        reason = rest
        if reason.startswith(":"):
            reason = reason[1:].lstrip()
        return kind, {}, reason
    return kind, {}, rest


def _find_fenced_block_at(lines: list[str], start: int) -> tuple[str, int] | None:
    """Return (info_string, end_line_index) for a fence that opens at ``start``.

    ``end_line_index`` is the line index of the closing fence (inclusive).
    Returns None when the line at ``start`` is not an opening fence or no
    matching closing fence is found.
    """
    if start < 0 or start >= len(lines):
        return None
    open_match = FENCE_OPEN_RE.match(lines[start])
    if not open_match:
        return None
    fence_char = open_match.group("fence")[0]
    fence_len = len(open_match.group("fence"))
    info = open_match.group("info") or ""
    close_re = re.compile(rf"^(\s*)({re.escape(fence_char)}{{{fence_len},}})\s*$")
    i = start + 1
    while i < len(lines):
        close_match = close_re.match(lines[i])
        if close_match:
            return info, i
        i += 1
    return None


def extract_examples(
    markdown_path: Path,
    seen_ids: dict[str, str] | None = None,
) -> list[dict[str, Any]]:
    """Return the list of marked example dicts for one markdown file.

    Each dict has keys: page, lang, id, mode, fixture, prelude, body, info.
    Non-verified blocks (illustrative, no marker, unsupported lang) are
    skipped; only `verify-example` markers on rust/go blocks are returned.

    ``seen_ids`` is an optional id-first-seen map shared across the
    whole discovery scan. When provided, duplicate verify-example ids
    across files are reported with the file:line of the first occurrence.
    When omitted, the function still detects same-file duplicates using a
    private local set so unit tests of single-file pages keep working.
    """
    text = markdown_path.read_text(encoding="utf-8")
    lines = text.splitlines()
    examples: list[dict[str, Any]] = []
    if seen_ids is None:
        seen_ids = {}
    local_seen: dict[str, int] = {}

    i = 0
    in_fence = False
    fence_char = ""
    fence_len = 0
    while i < len(lines):
        if in_fence:
            close = re.match(rf"^(\s*)({re.escape(fence_char)}{{{fence_len},}})\s*$", lines[i])
            if close:
                in_fence = False
            i += 1
            continue
        open_match = FENCE_OPEN_RE.match(lines[i])
        if open_match:
            in_fence = True
            fence_char = open_match.group("fence")[0]
            fence_len = len(open_match.group("fence"))
            i += 1
            continue
        marker = parse_marker(lines[i])
        if marker is None:
            i += 1
            continue
        kind, attrs, _reason = marker
        if kind != KIND_VERIFY:
            i += 1
            continue
        j = i + 1
        while j < len(lines) and lines[j].strip() == "":
            j += 1
        fence = _find_fenced_block_at(lines, j)
        if fence is None:
            raise HarnessError(
                f"{markdown_path}:{i + 1}: verify marker is not followed by a fenced code block"
            )
        info_text, fence_end = fence
        lang_token = info_text.strip().split()
        if not lang_token:
            raise HarnessError(
                f"{markdown_path}:{j + 1}: verify marker is followed by a fence without an info string"
            )
        lang = lang_token[0].lower()
        if lang not in SUPPORTED_LANGS:
            raise HarnessError(
                f"{markdown_path}:{j + 1}: verify marker on unsupported lang {lang!r} "
                f"(supported: {', '.join(SUPPORTED_LANGS)})"
            )
        for required_key in ("lang", "id"):
            if required_key not in attrs:
                raise HarnessError(
                    f"{markdown_path}:{i + 1}: verify marker missing required {required_key!r} attribute"
                )
        if attrs["lang"] != lang:
            raise HarnessError(
                f"{markdown_path}:{i + 1}: marker lang {attrs['lang']!r} "
                f"does not match fence lang {lang!r}"
            )
        ex_id = attrs["id"]
        if not ID_SLUG_RE.match(ex_id):
            raise HarnessError(
                f"{markdown_path}:{i + 1}: verify marker id {ex_id!r} must match [a-z0-9-]+"
            )
        if ex_id in local_seen:
            raise HarnessError(
                f"{markdown_path}:{i + 1}: duplicate verify marker id {ex_id!r}"
            )
        if ex_id in seen_ids:
            raise HarnessError(
                f"{markdown_path}:{i + 1}: duplicate verify marker id {ex_id!r} "
                f"(first seen at {seen_ids[ex_id]})"
            )
        local_seen[ex_id] = i + 1
        seen_ids[ex_id] = f"{markdown_path}:{i + 1}"
        mode = attrs.get("mode", "run")
        if mode not in ("run", "build"):
            raise HarnessError(
                f"{markdown_path}:{i + 1}: verify marker mode {mode!r} must be 'run' or 'build'"
            )
        fixture = attrs.get("fixture", "basic")
        prelude_name = attrs.get("prelude")
        if prelude_name is not None and (lang, prelude_name) not in PRELUDES:
            raise HarnessError(
                f"{markdown_path}:{i + 1}: unknown prelude {prelude_name!r} for lang {lang!r}"
            )
        body = "\n".join(lines[j + 1:fence_end])
        try:
            page_rel = str(markdown_path.relative_to(REPO_ROOT))
        except ValueError:
            page_rel = str(markdown_path)
        examples.append({
            "page": page_rel,
            "lang": lang,
            "id": ex_id,
            "mode": mode,
            "fixture": fixture,
            "prelude": prelude_name,
            "body": body,
        })
        i = fence_end + 1

    return examples


def discover_examples(docs_dir: Path) -> list[dict[str, Any]]:
    """Discover all marked examples under ``docs_dir``."""
    files = sorted(docs_dir.rglob("*.md"))
    out: list[dict[str, Any]] = []
    seen_ids: dict[str, str] = {}
    for path in files:
        out.extend(extract_examples(path, seen_ids=seen_ids))
    return out


# ---------------------------------------------------------------------------
# Path substitution
# ---------------------------------------------------------------------------


def apply_path_substitution(text: str, scratch_dir: Path, fixture_dir: Path) -> str:
    """Apply the documented literal substitutions to ``text`` in order."""
    scratch_token = "<SCRATCH>"
    fixtures_token = "<FIXTURES>"
    fixtures_path = str(fixture_dir)
    scratch_path = str(scratch_dir)
    replacements = list(VERIFY_REPLACEMENTS)
    values = {
        "<FIXTURES>": fixtures_path,
        "<SCRATCH>": scratch_path,
    }
    out = text
    for prefix, replacement in zip(VERIFY_PREFIXES, replacements):
        final = replacement
        if fixtures_token in final:
            final = final.replace(fixtures_token, fixtures_path)
        if scratch_token in final:
            final = final.replace(scratch_token, scratch_path)
        out = out.replace(prefix, final)
    out = out.replace(fixtures_token, fixtures_path)
    out = out.replace(scratch_token, scratch_path)
    return out


# ---------------------------------------------------------------------------
# Rust wrapping
# ---------------------------------------------------------------------------


def _unhide_rust_lines(body: str) -> str:
    """Strip rustdoc `# ` and `#` hidden-line markers, keeping the content."""
    out_lines: list[str] = []
    for raw in body.splitlines():
        if raw == "#":
            out_lines.append("")
            continue
        if raw.startswith(RUST_HIDDEN_PREFIX):
            out_lines.append(raw[len(RUST_HIDDEN_PREFIX):])
        else:
            out_lines.append(raw)
    while out_lines and out_lines[-1] == "":
        out_lines.pop()
    return "\n".join(out_lines)


def _rust_needs_result(body: str) -> bool:
    """Return True when the body requires a Result-returning main.

    A body needs a Result-returning main when it explicitly uses the
    `?` operator, contains the rustdoc `Ok::<()...>()` turbofish marker,
    or names `Result<...>` in a way that signals the writer expects
    `Box<dyn std::error::Error>` error handling.
    """
    if "Ok::<(), Box<dyn std::error::Error>>" in body:
        return True
    if "Result<(), Box<dyn std::error::Error>>" in body:
        return True
    for line in body.splitlines():
        stripped = line.strip()
        if stripped.startswith("//"):
            continue
        if "?" in stripped:
            return True
    return False


def _rust_ends_with_result_expr(body: str) -> bool:
    for line in reversed([raw for raw in body.splitlines() if raw.strip()]):
        if line.lstrip().startswith("//"):
            continue
        stripped = line.lstrip()
        return stripped.startswith("Ok(") or stripped.startswith("Ok::<")
    return False


def wrap_rust_example(body: str, prelude_body: str) -> str:
    """Wrap a rust example body (after unhiding) into a main function."""
    unhidden_body = _unhide_rust_lines(body)
    unhidden_prelude = _unhide_rust_lines(prelude_body) if prelude_body else ""
    prelude_section = unhidden_prelude
    if prelude_section and not prelude_section.endswith("\n"):
        prelude_section += "\n"
    if _rust_needs_result(unhidden_body):
        body_section = unhidden_body
        if not _rust_ends_with_result_expr(body_section):
            if body_section and not body_section.endswith("\n"):
                body_section += "\n"
            body_section += "Ok(())\n"
        wrapped = (
            "fn main() -> Result<(), Box<dyn std::error::Error>> {\n"
            f"{prelude_section}"
            f"{body_section}"
            "}\n"
        )
    else:
        wrapped = (
            "fn main() {\n"
            f"{prelude_section}"
            f"{unhidden_body}\n"
            "}\n"
        )
    return wrapped


# ---------------------------------------------------------------------------
# Go wrapping
# ---------------------------------------------------------------------------


def detect_go_imports(body: str, prelude_body: str) -> list[str]:
    """Return the deduplicated list of import lines to use for a Go example.

    Each entry is a string suitable for one line inside an `import (...)`
    block: it is either `\"fmt\"` (no alias) or
    `journal "github.com/netdata/systemd-journal-sdk/go/journal"` (aliased).
    The package name (for dedup) is the second element of each
    ``GO_IMPORT_PREFIXES`` tuple.
    """
    haystack = (prelude_body or "") + "\n" + body
    imports: list[str] = []
    seen: set[str] = set()
    for prefix, pkg_name, import_path in GO_IMPORT_PREFIXES:
        if prefix in haystack and pkg_name not in seen:
            if pkg_name == import_path:
                imports.append(f'"{import_path}"')
            else:
                imports.append(f'{pkg_name} "{import_path}"')
            seen.add(pkg_name)
    for required in GO_REQUIRED_IMPORTS:
        if required not in seen:
            imports.append(f'"{required}"')
            seen.add(required)
    return imports


def wrap_go_example(body: str, prelude_body: str) -> str:
    """Wrap a Go example body into package main + run/main scaffold."""
    imports = detect_go_imports(body, prelude_body)
    prelude_section = prelude_body
    if prelude_section and not prelude_section.endswith("\n"):
        prelude_section += "\n"
    body_section = body
    if body_section and not body_section.endswith("\n"):
        body_section += "\n"
    body_section += "return nil\n"
    import_block = "import (\n"
    for imp in imports:
        import_block += f"\t{imp}\n"
    import_block += ")\n\n"
    wrapped = (
        "package main\n\n"
        f"{import_block}"
        "func run() error {\n"
        f"{prelude_section}"
        f"{body_section}"
        "}\n\n"
        "func main() {\n"
        "\tif err := run(); err != nil {\n"
        "\t\tfmt.Fprintln(os.Stderr, err)\n"
        "\t\tos.Exit(1)\n"
        "\t}\n"
        "}\n"
    )
    return wrapped


# ---------------------------------------------------------------------------
# Project generation
# ---------------------------------------------------------------------------


def _read_rust_edition(cargo_path: Path = REPO_ROOT / "rust" / "Cargo.toml") -> str:
    """Return the ``edition`` value from the ``[workspace.package]`` section.

    The generated example package must use the same Rust edition as the
    workspace so member crates compiled via path dependencies are emitted with
    a compatible edition. Hardcoding the edition silently drifts when the
    workspace bumps it. Raises ``HarnessError`` if the file is missing or the
    ``[workspace.package]`` section does not declare an ``edition = "..."``.
    """
    try:
        text = cargo_path.read_text(encoding="utf-8")
    except OSError as exc:
        raise HarnessError(
            f"cannot read rust edition: {cargo_path} is not readable ({exc})"
        )
    in_workspace_package = False
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("["):
            in_workspace_package = (line == "[workspace.package]")
            continue
        if not in_workspace_package:
            continue
        if line.startswith("edition"):
            parts = line.split("=", 1)
            if len(parts) != 2:
                continue
            value = parts[1].strip()
            if value.startswith('"') and value.endswith('"') and len(value) >= 2:
                return value[1:-1]
            if "'" in value and value.startswith("'") and value.endswith("'") and len(value) >= 2:
                return value[1:-1]
    raise HarnessError(
        f"cannot find an 'edition = \"...\"' entry under [workspace.package] in {cargo_path}"
    )


def render_rust_cargo_toml() -> str:
    edition = _read_rust_edition()
    return textwrap.dedent(
        f"""\
        [package]
        name = "docs-examples"
        version = "0.0.0"
        edition = "{edition}"
        publish = false

        [workspace]

        [dependencies]
        journal = {{ package = "systemd-journal-sdk", path = "{RUST_JOURNAL_PKG}" }}
        journal_log_writer = {{ package = "systemd-journal-sdk-log-writer", path = "{RUST_LOG_WRITER_PKG}" }}
        serde_json = "1"
        """
    )


def _read_go_directive(go_mod_path: Path = GO_MODULE_DIR / "go.mod") -> str:
    """Return the version token from the ``go X.Y`` directive in ``go/go.mod``.

    The generated module must match the repository's own minimum Go version so
    examples never compile against a different toolchain than the SDK itself.
    Raises ``HarnessError`` if the file is missing or contains no parseable
    ``go`` directive.
    """
    try:
        text = go_mod_path.read_text(encoding="utf-8")
    except OSError as exc:
        raise HarnessError(
            f"cannot read go directive: {go_mod_path} is not readable ({exc})"
        )
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("//"):
            continue
        if line.startswith("go ") or line == "go":
            parts = line.split(None, 1)
            if len(parts) != 2 or not parts[1]:
                raise HarnessError(
                    f"cannot parse go directive in {go_mod_path}: empty version after 'go'"
                )
            token = parts[1]
            comment = token.find("//")
            if comment != -1:
                token = token[:comment]
            token = token.strip()
            if not token:
                raise HarnessError(
                    f"cannot parse go directive in {go_mod_path}: empty version after 'go'"
                )
            return token
    raise HarnessError(
        f"cannot find a 'go X.Y' directive in {go_mod_path}"
    )


def render_go_go_mod() -> str:
    go_directive = _read_go_directive()
    return textwrap.dedent(
        f"""\
        module docsexamples

        go {go_directive}

        require github.com/netdata/systemd-journal-sdk/go v0.0.0

        replace github.com/netdata/systemd-journal-sdk/go => """ + str(GO_MODULE_DIR) + "\n"
    )


def generate_rust_project(examples: list[dict[str, Any]], rust_dir: Path) -> None:
    ensure_clean_dir(rust_dir)
    (rust_dir / "Cargo.toml").write_text(render_rust_cargo_toml(), encoding="utf-8")
    src_bin = rust_dir / "src" / "bin"
    src_bin.mkdir(parents=True, exist_ok=True)
    for ex in examples:
        prelude_body = ""
        if ex["prelude"]:
            prelude_body = PRELUDES[(ex["lang"], ex["prelude"])]
        prelude_body = apply_path_substitution(prelude_body, Path(ex["_scratch_dir"]), Path(ex["_fixtures_dir"]))
        body = apply_path_substitution(ex["body"], Path(ex["_scratch_dir"]), Path(ex["_fixtures_dir"]))
        wrapped = wrap_rust_example(body, prelude_body)
        (src_bin / f"{ex['id']}.rs").write_text(wrapped, encoding="utf-8")


def generate_go_project(examples: list[dict[str, Any]], go_dir: Path) -> None:
    ensure_clean_dir(go_dir)
    (go_dir / "go.mod").write_text(render_go_go_mod(), encoding="utf-8")
    for ex in examples:
        prelude_body = ""
        if ex["prelude"]:
            prelude_body = PRELUDES[(ex["lang"], ex["prelude"])]
        prelude_body = apply_path_substitution(prelude_body, Path(ex["_scratch_dir"]), Path(ex["_fixtures_dir"]))
        body = apply_path_substitution(ex["body"], Path(ex["_scratch_dir"]), Path(ex["_fixtures_dir"]))
        wrapped = wrap_go_example(body, prelude_body)
        cmd_dir = go_dir / "cmd" / ex["id"]
        cmd_dir.mkdir(parents=True, exist_ok=True)
        (cmd_dir / "main.go").write_text(wrapped, encoding="utf-8")


# ---------------------------------------------------------------------------
# Fixtures (synthetic journal files using the in-repo Python SDK)
# ---------------------------------------------------------------------------


_SYNTHETIC_MACHINE_ID = bytes.fromhex("10000000000000000000000000000000")
_SYNTHETIC_BOOT_ID = bytes.fromhex("20000000000000000000000000000000")
_SYNTHETIC_SEQNUM_ID = bytes.fromhex("30000000000000000000000000000000")
_SYNTHETIC_FILE_ID = bytes.fromhex("40000000000000000000000000000000")
_BASE_REALTIME_USEC = 1_700_000_000_000_000


def _import_python_journal():
    """Import the in-repo Python journal SDK as `journal`."""
    if str(PYTHON_PKG_ROOT) not in sys.path:
        sys.path.insert(0, str(PYTHON_PKG_ROOT))
    import journal  # type: ignore
    return journal


def _build_fixture(path: Path, entry_count: int, ident_cycle: tuple[str, ...],
                   priority_cycle: tuple[int, ...], file_label: str) -> None:
    journal_mod = _import_python_journal()
    path.parent.mkdir(parents=True, exist_ok=True)
    writer = journal_mod.Writer.create(str(path), {
        "file_id": _SYNTHETIC_FILE_ID,
        "machine_id": _SYNTHETIC_MACHINE_ID,
        "boot_id": _SYNTHETIC_BOOT_ID,
        "seqnum_id": _SYNTHETIC_SEQNUM_ID,
    })
    try:
        for i in range(entry_count):
            identifier = ident_cycle[i % len(ident_cycle)]
            priority = priority_cycle[i % len(priority_cycle)]
            writer.append([
                {"name": "MESSAGE", "value": f"synthetic message {i} ({file_label})".encode("utf-8")},
                {"name": "PRIORITY", "value": str(priority).encode("utf-8")},
                {"name": "SYSLOG_IDENTIFIER", "value": identifier.encode("utf-8")},
            ], {
                "realtime_usec": _BASE_REALTIME_USEC + i * 1_000_000,
                "monotonic_usec": i + 1,
            })
    finally:
        writer.close_offline()
    verify_mod = journal_mod.verify_file
    verify_mod(str(path))


def build_basic_fixtures(fixtures_dir: Path) -> None:
    """Build the `basic` fixture set under ``fixtures_dir/basic/``."""
    basic_dir = fixtures_dir / "basic"
    if fixtures_dir.exists():
        rmtree(fixtures_dir)
    file_dir = basic_dir / "file"
    file_dir.mkdir(parents=True, exist_ok=True)
    dir_dir = basic_dir / "dir"
    dir_dir.mkdir(parents=True, exist_ok=True)
    ident_cycle = ("example-plugin", "demo-agent", "web-server")
    priority_cycle = (3, 4, 6)
    _build_fixture(
        file_dir / "system.journal",
        entry_count=30,
        ident_cycle=ident_cycle,
        priority_cycle=priority_cycle,
        file_label="file",
    )
    machine_id_hex = _SYNTHETIC_MACHINE_ID.hex()
    subdir = dir_dir / machine_id_hex
    _build_fixture(
        subdir / "system.journal",
        entry_count=10,
        ident_cycle=ident_cycle,
        priority_cycle=priority_cycle,
        file_label="dir",
    )


# ---------------------------------------------------------------------------
# Build and run
# ---------------------------------------------------------------------------


def _ensure_cargo_env(extra: dict[str, str]) -> dict[str, str]:
    env = dict(extra)
    env.setdefault("CARGO_HOME", str(WORK_ROOT / "caches" / "cargo-home"))
    env.setdefault("CARGO_TARGET_DIR", str(WORK_ROOT / "caches" / "cargo-target"))
    return env


def _ensure_go_env(extra: dict[str, str]) -> dict[str, str]:
    env = dict(extra)
    env.setdefault("GOMODCACHE", str(WORK_ROOT / "caches" / "gomod"))
    env.setdefault("GOCACHE", str(WORK_ROOT / "caches" / "gocache"))
    return env


def _ensure_scratch_dir(scratch_dir: Path) -> None:
    """Recreate the per-example scratch dir empty."""
    rmtree(scratch_dir)
    scratch_dir.mkdir(parents=True, exist_ok=True)


def _run_example(ex: dict[str, Any], *, build_dir: Path, cache_dir: Path,
                 timeout: float) -> dict[str, Any]:
    started = time.monotonic()
    detail: dict[str, Any] = {"phase": "run", "command": "", "stdout": "", "stderr": ""}
    try:
        if ex["lang"] == "rust":
            binary = cache_dir / "cargo-target" / "debug" / ex["id"]
            cmd = [str(binary)]
        elif ex["lang"] == "go":
            binary = build_dir / "bin" / ex["id"]
            cmd = [str(binary)]
        else:
            raise HarnessError(f"unsupported lang {ex['lang']!r}")
        detail["command"] = " ".join(_quote_token(t) for t in cmd)
        _ensure_scratch_dir(Path(ex["_scratch_dir"]))
        result = run_subprocess(cmd, cwd=build_dir, timeout=timeout)
        detail["stdout"] = result.stdout
        detail["stderr"] = result.stderr
        if result.returncode != 0:
            return {
                "status": "fail",
                "duration_ms": int((time.monotonic() - started) * 1000),
                "detail": {
                    "phase": "run",
                    "command": detail["command"],
                    "returncode": result.returncode,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                },
            }
        return {
            "status": "pass",
            "duration_ms": int((time.monotonic() - started) * 1000),
            "detail": {
                "phase": "run",
                "command": detail["command"],
                "stdout": result.stdout,
                "stderr": result.stderr,
            },
        }
    except subprocess.TimeoutExpired as exc:
        return {
            "status": "fail",
            "duration_ms": int((time.monotonic() - started) * 1000),
            "detail": {
                "phase": "run",
                "command": detail["command"] or "<timeout>",
                "returncode": None,
                "stdout": exc.stdout.decode("utf-8", "replace") if isinstance(exc.stdout, bytes) else (exc.stdout or ""),
                "stderr": (exc.stderr.decode("utf-8", "replace") if isinstance(exc.stderr, bytes) else (exc.stderr or "")) + "\n[harness timeout]",
            },
        }


def _build_rust(rust_dir: Path, cache_dir: Path, env: dict[str, str], timeout: float) -> dict[str, Any]:
    started = time.monotonic()
    cargo_target = cache_dir / "cargo-target"
    cargo_home = cache_dir / "cargo-home"
    full_env = _ensure_cargo_env({
        "CARGO_HOME": str(cargo_home),
        "CARGO_TARGET_DIR": str(cargo_target),
    })
    full_env.update({k: v for k, v in env.items() if k not in full_env})
    cmd = ["cargo", "build", "--bins"]
    try:
        result = run_subprocess(cmd, cwd=rust_dir, env=full_env, timeout=timeout)
        return {
            "returncode": result.returncode,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "duration_ms": int((time.monotonic() - started) * 1000),
        }
    except subprocess.TimeoutExpired as exc:
        return {
            "returncode": None,
            "stdout": exc.stdout.decode("utf-8", "replace") if isinstance(exc.stdout, bytes) else (exc.stdout or ""),
            "stderr": (exc.stderr.decode("utf-8", "replace") if isinstance(exc.stderr, bytes) else (exc.stderr or "")) + "\n[harness timeout]",
            "duration_ms": int((time.monotonic() - started) * 1000),
        }


def _build_go(go_dir: Path, cache_dir: Path, env: dict[str, str], timeout: float,
              go_examples: list[dict[str, Any]]) -> dict[str, Any]:
    started = time.monotonic()
    full_env = _ensure_go_env({
        "GOMODCACHE": str(cache_dir / "gomod"),
        "GOCACHE": str(cache_dir / "gocache"),
    })
    full_env.update({k: v for k, v in env.items() if k not in full_env})
    try:
        tidy = run_subprocess(["go", "mod", "tidy"], cwd=go_dir, env=full_env, timeout=timeout)
        if tidy.returncode != 0:
            return {
                "returncode": tidy.returncode,
                "stdout": tidy.stdout,
                "stderr": tidy.stderr,
                "duration_ms": int((time.monotonic() - started) * 1000),
            }
        bin_dir = go_dir / "bin"
        bin_dir.mkdir(parents=True, exist_ok=True)
        aggregated_stdout = ""
        aggregated_stderr = ""
        for ex in go_examples:
            pkg = "./cmd/" + ex["id"]
            out = bin_dir / ex["id"]
            build = run_subprocess(
                ["go", "build", "-o", str(out), pkg],
                cwd=go_dir, env=full_env, timeout=timeout,
            )
            aggregated_stdout += build.stdout
            aggregated_stderr += build.stderr
            if build.returncode != 0:
                return {
                    "returncode": build.returncode,
                    "stdout": aggregated_stdout,
                    "stderr": aggregated_stderr,
                    "duration_ms": int((time.monotonic() - started) * 1000),
                }
        return {
            "returncode": 0,
            "stdout": aggregated_stdout,
            "stderr": aggregated_stderr,
            "duration_ms": int((time.monotonic() - started) * 1000),
        }
    except subprocess.TimeoutExpired as exc:
        return {
            "returncode": None,
            "stdout": exc.stdout.decode("utf-8", "replace") if isinstance(exc.stdout, bytes) else (exc.stdout or ""),
            "stderr": (exc.stderr.decode("utf-8", "replace") if isinstance(exc.stderr, bytes) else (exc.stderr or "")) + "\n[harness timeout]",
            "duration_ms": int((time.monotonic() - started) * 1000),
        }


def _resolve_per_example_paths(examples: list[dict[str, Any]], fixtures_dir: Path,
                              scratch_root: Path) -> None:
    for ex in examples:
        ex["_fixtures_dir"] = str(fixtures_dir)
        ex["_scratch_dir"] = str(scratch_root / ex["id"])


def _run_examples(examples: list[dict[str, Any]], *, rust_dir: Path, go_dir: Path,
                  timeout: float, env: dict[str, str], cache_dir: Path) -> list[dict[str, Any]]:
    rust_examples = [ex for ex in examples if ex["lang"] == "rust"]
    go_examples = [ex for ex in examples if ex["lang"] == "go"]

    records: list[dict[str, Any]] = []
    build_records: list[dict[str, Any]] = []

    if rust_examples:
        info(f"building {len(rust_examples)} rust example(s) into {rust_dir}")
        outcome = _build_rust(rust_dir, cache_dir, env, timeout=timeout * 4)
        build_records.append({
            "phase": "rust-build",
            "returncode": outcome["returncode"],
            "stdout": outcome["stdout"],
            "stderr": outcome["stderr"],
            "duration_ms": outcome["duration_ms"],
        })
        if outcome["returncode"] != 0:
            for ex in rust_examples:
                records.append({
                    "id": ex["id"],
                    "page": ex["page"],
                    "lang": ex["lang"],
                    "mode": ex["mode"],
                    "fixture": ex["fixture"],
                    "status": "fail",
                    "duration_ms": 0,
                    "detail": {
                        "phase": "build",
                        "returncode": outcome["returncode"],
                        "stdout": outcome["stdout"],
                        "stderr": outcome["stderr"],
                    },
                })
        else:
            for ex in rust_examples:
                if ex["mode"] == "build":
                    records.append({
                        "id": ex["id"],
                        "page": ex["page"],
                        "lang": ex["lang"],
                        "mode": ex["mode"],
                        "fixture": ex["fixture"],
                        "status": "pass",
                        "duration_ms": 0,
                        "detail": {"phase": "build", "note": "compile-only"},
                    })
                    continue
                run_record = _run_example(ex, build_dir=rust_dir, cache_dir=cache_dir, timeout=timeout)
                records.append({
                    "id": ex["id"],
                    "page": ex["page"],
                    "lang": ex["lang"],
                    "mode": ex["mode"],
                    "fixture": ex["fixture"],
                    "status": run_record["status"],
                    "duration_ms": run_record["duration_ms"],
                    "detail": run_record["detail"],
                })

    if go_examples:
        info(f"building {len(go_examples)} go example(s) into {go_dir}")
        outcome = _build_go(go_dir, cache_dir, env, timeout=timeout * 4,
                            go_examples=go_examples)
        build_records.append({
            "phase": "go-build",
            "returncode": outcome["returncode"],
            "stdout": outcome["stdout"],
            "stderr": outcome["stderr"],
            "duration_ms": outcome["duration_ms"],
        })
        if outcome["returncode"] != 0:
            for ex in go_examples:
                records.append({
                    "id": ex["id"],
                    "page": ex["page"],
                    "lang": ex["lang"],
                    "mode": ex["mode"],
                    "fixture": ex["fixture"],
                    "status": "fail",
                    "duration_ms": 0,
                    "detail": {
                        "phase": "build",
                        "returncode": outcome["returncode"],
                        "stdout": outcome["stdout"],
                        "stderr": outcome["stderr"],
                    },
                })
        else:
            for ex in go_examples:
                if ex["mode"] == "build":
                    records.append({
                        "id": ex["id"],
                        "page": ex["page"],
                        "lang": ex["lang"],
                        "mode": ex["mode"],
                        "fixture": ex["fixture"],
                        "status": "pass",
                        "duration_ms": 0,
                        "detail": {"phase": "build", "note": "compile-only"},
                    })
                    continue
                run_record = _run_example(ex, build_dir=go_dir, cache_dir=cache_dir, timeout=timeout)
                records.append({
                    "id": ex["id"],
                    "page": ex["page"],
                    "lang": ex["lang"],
                    "mode": ex["mode"],
                    "fixture": ex["fixture"],
                    "status": run_record["status"],
                    "duration_ms": run_record["duration_ms"],
                    "detail": run_record["detail"],
                })

    return records + ([{"_build": r} for r in build_records])


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def _print_summary(records: list[dict[str, Any]]) -> None:
    example_records = [r for r in records if "id" in r]
    if not example_records:
        print("verify_examples: no examples to summarize", file=sys.stderr)
        return
    header = ("ID", "LANG", "MODE", "STATUS", "DURATION_MS", "PAGE")
    rows = [header]
    for rec in sorted(example_records, key=lambda r: r["id"]):
        rows.append((
            rec["id"],
            rec["lang"],
            rec["mode"],
            rec["status"],
            str(rec["duration_ms"]),
            rec["page"],
        ))
    widths = [max(len(row[i]) for row in rows) for i in range(len(header))]
    sep = "  ".join("-" * w for w in widths)
    print("  ".join(cell.ljust(widths[i]) for i, cell in enumerate(rows[0])))
    print(sep)
    for row in rows[1:]:
        print("  ".join(cell.ljust(widths[i]) for i, cell in enumerate(row)))
    failing = [r for r in example_records if r["status"] != "pass"]
    if failing:
        print("", file=sys.stderr)
        for rec in failing:
            print(f"--- failing example: {rec['id']} ({rec['lang']} {rec['mode']}) ---", file=sys.stderr)
            detail = rec["detail"]
            if "returncode" in detail:
                print(f"returncode: {detail['returncode']}", file=sys.stderr)
            if detail.get("stdout"):
                print("stdout:", file=sys.stderr)
                print(textwrap.indent(detail["stdout"], "  "), file=sys.stderr)
            if detail.get("stderr"):
                print("stderr:", file=sys.stderr)
                print(textwrap.indent(detail["stderr"], "  "), file=sys.stderr)


def _filter_examples(examples: list[dict[str, Any]], *, only: str | None, lang: str | None) -> list[dict[str, Any]]:
    out = list(examples)
    if lang is not None:
        out = [ex for ex in out if ex["lang"] == lang]
    if only is not None:
        out = [ex for ex in out if ex["id"] == only]
    return out


def _print_listing(examples: list[dict[str, Any]]) -> None:
    if not examples:
        print("verify_examples: no marked examples discovered", file=sys.stderr)
        return
    for ex in examples:
        print(
            f"{ex['page']}\t{ex['id']}\t{ex['lang']}\t{ex['mode']}\t"
            f"{ex['fixture']}\t{ex['prelude'] or ''}"
        )


def main(argv: list[str] | None = None) -> int:
    try:
        return _main_impl(argv)
    except HarnessError as exc:
        die(str(exc))
    return 0  # unreachable: die() raises SystemExit


def _main_impl(argv: list[str] | None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--docs-dir", default=str(REPO_ROOT / "docs"),
                        help="wiki markdown directory to scan (default: docs/)")
    parser.add_argument("--only", default=None, help="only handle example with this id")
    parser.add_argument("--lang", choices=SUPPORTED_LANGS, default=None,
                        help="filter to one language")
    parser.add_argument("--list", action="store_true",
                        help="list discovered examples without building or running")
    parser.add_argument("--keep", action="store_true",
                        help="skip the final cleanup of generated sources (sources are kept by default; "
                             "only scratch dirs are recreated per run)")
    parser.add_argument("--timeout", type=float, default=60.0,
                        help="per-example timeout in seconds (default: 60)")
    args = parser.parse_args(argv)

    docs_dir = Path(args.docs_dir).resolve()
    if not docs_dir.is_dir():
        die(f"docs dir does not exist: {docs_dir}")

    examples = discover_examples(docs_dir)
    selected = _filter_examples(examples, only=args.only, lang=args.lang)
    if args.list:
        _print_listing(selected)
        print(f"verify_examples: discovered {len(examples)} example(s); selected {len(selected)}", file=sys.stderr)
        return 0

    if not selected:
        print(f"verify_examples: zero verified examples found in {docs_dir}", file=sys.stderr)
        return 0

    fixtures_dir = WORK_ROOT / "fixtures"
    scratch_root = WORK_ROOT / "scratch"
    cache_dir = WORK_ROOT / "caches"
    rust_dir = WORK_ROOT / "rust-src"
    go_dir = WORK_ROOT / "go-src"
    manifest_path = WORK_ROOT / "manifest.json"

    rmtree(scratch_root)
    scratch_root.mkdir(parents=True, exist_ok=True)
    ensure_clean_dir(rust_dir)
    ensure_clean_dir(go_dir)
    cache_dir.mkdir(parents=True, exist_ok=True)

    info(f"building fixtures under {fixtures_dir}")
    build_basic_fixtures(fixtures_dir)

    _resolve_per_example_paths(selected, fixtures_dir, scratch_root)
    rust_selected = [ex for ex in selected if ex["lang"] == "rust"]
    go_selected = [ex for ex in selected if ex["lang"] == "go"]
    if rust_selected:
        generate_rust_project(rust_selected, rust_dir)
    if go_selected:
        generate_go_project(go_selected, go_dir)

    records = _run_examples(
        selected,
        rust_dir=rust_dir,
        go_dir=go_dir,
        timeout=args.timeout,
        env={},
        cache_dir=cache_dir,
    )
    example_records = [r for r in records if "id" in r]
    build_records = [r for r in records if "_build" in r]
    build_records = [r["_build"] for r in build_records]

    failing = [r for r in example_records if r["status"] != "pass"]
    summary = {
        "discovered": len(examples),
        "selected": len(selected),
        "passed": sum(1 for r in example_records if r["status"] == "pass"),
        "failed": len(failing),
        "builds": build_records,
    }
    manifest = {"summary": summary, "examples": example_records}
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True), encoding="utf-8")

    _print_summary(example_records)
    print(f"verify_examples: passed {summary['passed']} of {summary['selected']}; "
          f"failed {summary['failed']}; manifest: {manifest_path.relative_to(REPO_ROOT)}",
          file=sys.stderr)

    if failing:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
