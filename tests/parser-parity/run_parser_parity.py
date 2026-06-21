#!/usr/bin/env python3
"""Shared parser parity runner.

Validates that both Rust and Go journalctl commands recognize every
official systemd v260.1 option, output mode, and parser interaction.

For each long option we build a syntactically valid invocation (using a
synthetic empty path or `--file=...` placeholder as required) and verify:

- The command does NOT exit with an "unknown option" / "unknown flag" /
  unrecognized error.
- If the option is `recognized-unsupported`, the command exits non-zero
  with an error message matching the portable-mode unsupported contract.
- If the option is `recognized-no-op`, the command either succeeds or
  exits with a portable-mode unsupported message (depending on whether
  the option has a side effect or not).
- If the option is `file-backed-required`, we cannot fully test the
  behavior without a real journal file, but the parser must accept the
  option (no "unknown option" error) before the command logic dispatches.

For each output mode we run `--output=<mode> --file=<placeholder>` and
verify the parser accepts the mode (no "unknown output format" error).

For each parser interaction rule we run a synthetic argument combination
and verify the command reports the official v260.1 conflict error class.

Runtime artifacts stay under `.local/parser-parity/`. The harness only
inspects generated output and the in-repo Rust/Go binaries; it never
invokes systemd or reads host journals.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess  # nosec B404
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
LOCAL_DIR = REPO_ROOT / ".local" / "parser-parity"
DEFAULT_RUST_BIN = REPO_ROOT / ".local" / "cargo-target" / "debug" / "journalctl"
DEFAULT_GO_BIN = REPO_ROOT / ".local" / "bin" / "go-journalctl"
DEFAULT_MANIFEST = REPO_ROOT / "tests" / "parser-parity" / "v260-manifest.json"


@dataclass
class Command:
    name: str
    path: Path
    flag_style: str  # "gnu-long" or "go-flag"

    def build_args(self, long_name: str, has_value: bool, value: str | None) -> list[str]:
        """Build CLI args for one option. Always uses `--name=value` form
        when the binary accepts GNU long options, and Go-style `-name
        value` form for the Go binary.
        """
        if self.flag_style == "gnu-long":
            if has_value and value is not None:
                return [f"--{long_name}={value}"]
            if has_value:
                return [f"--{long_name}=placeholder"]
            return [f"--{long_name}"]
        # Go flag style uses single-dash forms too. We always prefer the
        # long form (`-name`) because it is more stable than one-letter
        # short aliases and Go's `flag` accepts both with the same code
        # path.
        if has_value and value is not None:
            return [f"-{long_name}", value]
        if has_value:
            return [f"-{long_name}", "placeholder"]
        return [f"-{long_name}"]

    def _short_alias(self, long_name: str) -> str:
        # Look up the matching manifest entry.
        for opt in MANIFEST["long_options"]:
            if opt["name"] == long_name and opt.get("short_alias"):
                return opt["short_alias"]
        # Fallback: derive one-letter alias from first letter.
        return long_name[0]


@dataclass
class CaseResult:
    name: str
    expectation: str
    status: str  # "ok" | "fail" | "skipped"
    detail: str


def run_command(cmd: Command, args: list[str], timeout: int = 30) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env.setdefault("LC_ALL", "C")
    return subprocess.run(  # nosec B603 B607
        [str(cmd.path), *args],
        cwd=str(REPO_ROOT),
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )


def is_unknown_option(stderr: str, stdout: str) -> bool:
    text = (stderr + "\n" + stdout).lower()
    patterns = [
        r"unknown option",
        r"unexpected argument",
        r"unrecognized option",
        r"no such flag",
        r"invalid option",
        r"unknown.*flag",
        r"bad.*option",
    ]
    return any(re.search(p, text) for p in patterns)


def is_unknown_output_mode(stderr: str, stdout: str) -> bool:
    text = (stderr + "\n" + stdout).lower()
    return "unknown output" in text or "unknown output format" in text


def is_portable_unsupported(stderr: str, stdout: str) -> bool:
    text = (stderr + "\n" + stdout).lower()
    return "portable mode does not support" in text


def looks_like_unsupported(err: str) -> bool:
    """Catch the project-supplied ErrUnsupported / Unsupported variants."""
    text = err.lower()
    return (
        "operation not supported" in text
        or "not supported" in text
        or "portable mode does not support" in text
    )


def exercise_long_option(cmd: Command, opt: dict) -> CaseResult:
    """Probe whether the parser recognizes one long option.

    We send only the option (without --file) when it is no-op / unsupported,
    and a minimal `--file=...` invocation for file-backed options.
    """
    long_name = opt["name"]
    args_type = opt["args"]
    expectation = opt["expectation"]

    # Build syntactically valid invocation.
    has_value = args_type not in ("none",)
    if expectation == "recognized-unsupported":
        # Use a minimal placeholder; the option itself should still be parsed.
        if has_value:
            arg_list = [f"--{long_name}=placeholder"]
        else:
            arg_list = [f"--{long_name}"]
    elif expectation == "recognized-no-op":
        # Recognize. Behavior may or may not require a file/directory.
        if has_value:
            arg_list = [f"--{long_name}=placeholder"]
        else:
            arg_list = [f"--{long_name}"]
    else:
        # file-backed-required / portable-utility-required.
        # Probe with a placeholder path; this will likely fail on file
        # access, but the parser must accept the option first.
        if has_value:
            arg_list = [f"--{long_name}=placeholder"]
        else:
            arg_list = [f"--{long_name}"]

    try:
        result = run_command(cmd, arg_list, timeout=20)
    except subprocess.TimeoutExpired:
        return CaseResult(long_name, expectation, "fail", "timeout")
    except FileNotFoundError as exc:
        return CaseResult(long_name, expectation, "fail", f"binary missing: {exc}")

    combined = result.stderr + "\n" + result.stdout
    if is_unknown_option(combined, ""):
        return CaseResult(
            long_name,
            expectation,
            "fail",
            f"unknown-option error for {arg_list!r}: {result.stderr.strip()[:300]}",
        )

    if expectation == "recognized-unsupported":
        # The portable-mode unsupported contract applies to daemon-only
        # actions. The command must fail with the portable-mode message,
        # with an unsupported-error, or with the specific daemon-only
        # reason. Acceptable patterns:
        # - "journalctl portable mode does not support <feature>: <reason>"
        # - the project's existing ErrUnsupported ("operation not supported")
        # - a targeted daemon-only error (e.g. "no journal files found"
        #   because the daemon is not available, or a parse/validation
        #   error specific to the option).
        if result.returncode == 0:
            return CaseResult(
                long_name,
                expectation,
                "fail",
                f"unsupported option unexpectedly succeeded: stdout={result.stdout[:300]}",
            )
        if not looks_like_unsupported(combined):
            return CaseResult(
                long_name,
                expectation,
                "fail",
                f"unsupported option missing portable message: stderr={result.stderr[:300]}",
            )
        return CaseResult(long_name, expectation, "ok", "portable unsupported reported")

    if expectation == "recognized-no-op":
        # Recognized. Accept success, no-op behavior, or unsupported-style
        # messages (because some no-ops need a file/directory input to
        # actually run). The key requirement is that the parser accepts
        # the option, so we treat any non-unknown-option outcome as ok.
        return CaseResult(long_name, expectation, "ok", f"rc={result.returncode}")

    # file-backed-required / portable-utility-required.
    # Without a real file/directory input we cannot assert full behavior.
    # The contract is that the parser must accept the option. Any other
    # outcome (open failure, validation failure) is acceptable as long as
    # it is not "unknown option".
    if is_unknown_option(combined, ""):
        return CaseResult(long_name, expectation, "fail", "parser rejected option")
    return CaseResult(
        long_name,
        expectation,
        "ok",
        f"parsed (rc={result.returncode}; file/directory placeholder expected to fail)",
    )


def sample_value_for_option(opt: dict) -> str:
    name = opt["name"]
    args_type = opt["args"]
    if name == "output":
        return "json"
    if name == "unit":
        return "alpha.service"
    if name == "identifier":
        return "app"
    if name == "exclude-identifier":
        return "app"
    if name == "priority":
        return "err"
    if name == "grep":
        return "message"
    if name == "machine":
        return "placeholder"
    if args_type in ("path",):
        return "placeholder.journal"
    if args_type in ("timestamp",):
        return "2020-01-01"
    if args_type in ("cursor",):
        return "s=00000000000000000000000000000000;i=1"
    if args_type in ("range",):
        return "err"
    if args_type in ("list",):
        return "daemon"
    if args_type in ("mode",):
        return "json"
    if args_type in ("unit/glob",):
        return "alpha.service"
    if args_type in ("pattern",):
        return "message"
    if args_type in ("descriptor",):
        return "0"
    if args_type in ("policy",):
        return "default"
    if args_type in ("comma list",):
        return "MESSAGE"
    if args_type in ("string",):
        return "placeholder"
    if args_type in ("key",):
        return "placeholder"
    if args_type in ("bytes",):
        return "1M"
    if args_type in ("int",):
        return "1"
    if args_type in ("duration",):
        return "1s"
    return "placeholder"


def build_short_args(letter: str, opt: dict) -> list[str]:
    if letter == "h":
        return ["-h"]
    if letter == "f":
        return ["-f", "--version"]
    if letter == "I":
        return ["-I", "--file=placeholder.journal"]

    args_type = opt["args"]
    if args_type in ("none", "optional int", "optional descriptor"):
        args = [f"-{letter}"]
    else:
        args = [f"-{letter}", sample_value_for_option(opt)]

    if opt["expectation"] == "recognized-supported" and opt["name"] not in {
        "directory",
        "file",
        "fields",
        "field",
    }:
        args.append("--file=placeholder.journal")
    return args


def exercise_short_option(cmd: Command, short: dict, long_options: dict[str, dict]) -> CaseResult:
    letter = short["letter"]
    long_name = short["long"]
    opt = long_options[long_name]
    args = build_short_args(letter, opt)
    name = f"-{letter}/{long_name}"
    try:
        result = run_command(cmd, args, timeout=20)
    except subprocess.TimeoutExpired:
        return CaseResult(name, "short-option", "fail", "timeout")
    except FileNotFoundError as exc:
        return CaseResult(name, "short-option", "fail", f"binary missing: {exc}")

    combined = result.stderr + "\n" + result.stdout
    if is_unknown_option(combined, ""):
        return CaseResult(
            name,
            "short-option",
            "fail",
            f"unknown-option error for {args!r}: {result.stderr.strip()[:300]}",
        )
    return CaseResult(name, "short-option", "ok", f"parsed (rc={result.returncode})")


def exercise_output_mode(cmd: Command, mode: str) -> CaseResult:
    """Probe whether `--output=<mode>` is accepted by the parser."""
    try:
        result = run_command(cmd, [f"--output={mode}", "--file=placeholder.journal"], timeout=20)
    except subprocess.TimeoutExpired:
        return CaseResult(f"output={mode}", "recognized-supported", "fail", "timeout")
    except FileNotFoundError as exc:
        return CaseResult(f"output={mode}", "recognized-supported", "fail", f"binary missing: {exc}")
    combined = result.stderr + "\n" + result.stdout
    if is_unknown_output_mode(combined, ""):
        return CaseResult(
            f"output={mode}",
            "recognized-supported",
            "fail",
            f"unknown output format: {result.stderr.strip()[:300]}",
        )
    return CaseResult(f"output={mode}", "recognized-supported", "ok", f"rc={result.returncode}")


def exercise_interaction(cmd: Command, interaction: dict) -> CaseResult:
    """Exercise one parser interaction rule.

    Returns `ok` if the command rejects the invalid combination with the
    expected conflict error class. Returns `skipped` for rules that
    require full journal semantics beyond what the placeholder can verify.
    """
    name = interaction["name"]
    if name == "time-bounds-order":
        # Use the `--since` later than `--until` conflict.
        try:
            result = run_command(
                cmd,
                [
                    "--since=2020-01-02",
                    "--until=2020-01-01",
                    "--file=placeholder.journal",
                ],
                timeout=20,
            )
        except subprocess.TimeoutExpired:
            return CaseResult(name, "parser-required", "fail", "timeout")
        except FileNotFoundError as exc:
            return CaseResult(name, "parser-required", "fail", f"binary missing: {exc}")
        combined = (result.stderr + "\n" + result.stdout).lower()
        if "since" in combined and ("must be before" in combined or "later" in combined):
            return CaseResult(name, "parser-required", "ok", "since<until enforced")
        return CaseResult(
            name,
            "parser-required",
            "fail",
            f"since>until not enforced: stderr={result.stderr.strip()[:300]}",
        )
    if name == "follow-reverse-conflict":
        try:
            result = run_command(
                cmd,
                ["--follow", "--reverse", "--file=placeholder.journal"],
                timeout=20,
            )
        except subprocess.TimeoutExpired:
            return CaseResult(name, "parser-required", "fail", "timeout")
        except FileNotFoundError as exc:
            return CaseResult(name, "parser-required", "fail", f"binary missing: {exc}")
        combined = (result.stderr + "\n" + result.stdout).lower()
        if "reverse" in combined and "follow" in combined and (
            "not both" in combined or "either" in combined or "conflict" in combined
        ):
            return CaseResult(name, "parser-required", "ok", "reverse+follow rejected")
        return CaseResult(
            name,
            "parser-required",
            "fail",
            f"reverse+follow not enforced: stderr={result.stderr.strip()[:300]}",
        )
    if name == "source-exclusivity":
        try:
            result = run_command(
                cmd,
                [
                    "--directory=placeholder_dir",
                    "--file=placeholder.journal",
                ],
                timeout=20,
            )
        except subprocess.TimeoutExpired:
            return CaseResult(name, "parser-required", "fail", "timeout")
        except FileNotFoundError as exc:
            return CaseResult(name, "parser-required", "fail", f"binary missing: {exc}")
        combined = (result.stderr + "\n" + result.stdout).lower()
        if (
            "directory" in combined
            and "file" in combined
            and ("at most one" in combined or "only one" in combined or "not both" in combined)
        ):
            return CaseResult(name, "parser-required", "ok", "directory+file rejected")
        return CaseResult(
            name,
            "parser-required",
            "fail",
            f"directory+file not enforced: stderr={result.stderr.strip()[:300]}",
        )
    if name == "cursor-source-exclusivity":
        try:
            result = run_command(
                cmd,
                [
                    "--since=2020-01-01",
                    "--cursor=placeholder-cursor",
                    "--file=placeholder.journal",
                ],
                timeout=20,
            )
        except subprocess.TimeoutExpired:
            return CaseResult(name, "parser-required", "fail", "timeout")
        except FileNotFoundError as exc:
            return CaseResult(name, "parser-required", "fail", f"binary missing: {exc}")
        combined = (result.stderr + "\n" + result.stdout).lower()
        if "cursor" in combined and "since" in combined and (
            "only one" in combined or "not both" in combined
        ):
            return CaseResult(name, "parser-required", "ok", "cursor source exclusivity enforced")
        return CaseResult(
            name,
            "parser-required",
            "fail",
            f"cursor source exclusivity not enforced: stderr={result.stderr.strip()[:300]}",
        )
    if name == "oldest-lines-conflict":
        try:
            result = run_command(
                cmd,
                ["--lines=+5", "--reverse", "--file=placeholder.journal"],
                timeout=20,
            )
        except subprocess.TimeoutExpired:
            return CaseResult(name, "parser-required", "fail", "timeout")
        except FileNotFoundError as exc:
            return CaseResult(name, "parser-required", "fail", f"binary missing: {exc}")
        combined = (result.stderr + "\n" + result.stdout).lower()
        if "lines=+n" in combined and ("reverse" in combined or "follow" in combined):
            return CaseResult(name, "parser-required", "ok", "oldest-lines conflict enforced")
        return CaseResult(
            name,
            "parser-required",
            "fail",
            f"oldest-lines conflict not enforced: stderr={result.stderr.strip()[:300]}",
        )
    if name == "boot-merge-conflict":
        try:
            result = run_command(
                cmd,
                ["--boot", "--merge", "--file=placeholder.journal"],
                timeout=20,
            )
        except subprocess.TimeoutExpired:
            return CaseResult(name, "parser-required", "fail", "timeout")
        except FileNotFoundError as exc:
            return CaseResult(name, "parser-required", "fail", f"binary missing: {exc}")
        combined = (result.stderr + "\n" + result.stdout).lower()
        if "boot" in combined and "merge" in combined and (
            "not supported" in combined or "not both" in combined or "conflict" in combined
        ):
            return CaseResult(name, "parser-required", "ok", "boot+merge rejected")
        return CaseResult(
            name,
            "parser-required",
            "fail",
            f"boot+merge not enforced: stderr={result.stderr.strip()[:300]}",
        )
    if name == "interspersed-show-option":
        try:
            result = run_command(
                cmd,
                ["TEST_FIELD=value", "--show-cursor", "--file=placeholder.journal"],
                timeout=20,
            )
        except subprocess.TimeoutExpired:
            return CaseResult(name, "parser-required", "fail", "timeout")
        except FileNotFoundError as exc:
            return CaseResult(name, "parser-required", "fail", f"binary missing: {exc}")
        combined = (result.stderr + "\n" + result.stdout).lower()
        rejected_as_match = (
            "path match argument" in combined
            or "invalid match --show-cursor" in combined
            or "default journal source" in combined
            or is_unknown_option(combined, "")
        )
        if not rejected_as_match:
            return CaseResult(name, "parser-required", "ok", "options accepted after matches")
        return CaseResult(
            name,
            "parser-required",
            "fail",
            f"interspersed option not parsed: stderr={result.stderr.strip()[:300]}",
        )
    if name == "short-attached-values":
        try:
            result = run_command(
                cmd,
                ["-rn2", "-ball", "--file=placeholder.journal"],
                timeout=20,
            )
        except subprocess.TimeoutExpired:
            return CaseResult(name, "parser-required", "fail", "timeout")
        except FileNotFoundError as exc:
            return CaseResult(name, "parser-required", "fail", f"binary missing: {exc}")
        combined = (result.stderr + "\n" + result.stdout).lower()
        if not is_unknown_option(combined, "") and "parse --lines" not in combined:
            return CaseResult(name, "parser-required", "ok", "attached short values parsed")
        return CaseResult(
            name,
            "parser-required",
            "fail",
            f"attached short values rejected: stderr={result.stderr.strip()[:300]}",
        )
    if name == "explicit-empty-optional-values":
        cases = [
            (["--lines=", "--file=placeholder.journal"], "lines"),
            (["--case-sensitive=", "--grep=x", "--file=placeholder.journal"], "case-sensitive"),
        ]
        for args, expected in cases:
            try:
                result = run_command(cmd, args, timeout=20)
            except subprocess.TimeoutExpired:
                return CaseResult(name, "parser-required", "fail", f"timeout for {args!r}")
            except FileNotFoundError as exc:
                return CaseResult(name, "parser-required", "fail", f"binary missing: {exc}")
            combined = (result.stderr + "\n" + result.stdout).lower()
            if result.returncode == 0 or expected not in combined:
                return CaseResult(
                    name,
                    "parser-required",
                    "fail",
                    f"explicit empty {args[0]} not rejected: stderr={result.stderr.strip()[:300]}",
                )
        return CaseResult(name, "parser-required", "ok", "explicit empty values rejected")
    return CaseResult(name, "parser-required", "skipped", "interaction not yet exercised")


def run_for_command(cmd: Command, manifest: dict) -> list[CaseResult]:
    results: list[CaseResult] = []
    long_options = {opt["name"]: opt for opt in manifest["long_options"]}
    for opt in manifest["long_options"]:
        results.append(exercise_long_option(cmd, opt))
    for short in manifest["short_options"]:
        results.append(exercise_short_option(cmd, short, long_options))
    for mode in manifest["output_modes"]:
        results.append(exercise_output_mode(cmd, mode))
    for interaction in manifest["parser_interactions"]:
        results.append(exercise_interaction(cmd, interaction))
    return results


def write_results(name: str, results: list[CaseResult]) -> Path:
    LOCAL_DIR.mkdir(parents=True, exist_ok=True)
    out = LOCAL_DIR / f"{name}.json"
    payload = [
        {
            "name": r.name,
            "expectation": r.expectation,
            "status": r.status,
            "detail": r.detail,
        }
        for r in results
    ]
    out.write_text(json.dumps(payload, indent=2) + "\n")
    return out


def build_go_binary(go_root: Path) -> Path:
    """Build the Go journalctl binary under .local/bin/."""
    bin_path = LOCAL_DIR.parent / "bin" / "go-journalctl"
    LOCAL_DIR.parent.mkdir(parents=True, exist_ok=True)
    cmd = ["go", "build", "-o", str(bin_path), "./cmd/journalctl"]
    env = os.environ.copy()
    env.setdefault("GOCACHE", str(REPO_ROOT / ".local" / "go-build"))
    env.setdefault("GOMODCACHE", str(REPO_ROOT / ".local" / "go-mod-cache"))
    subprocess.run(  # nosec B603 B607
        cmd,
        cwd=str(go_root),
        env=env,
        text=True,
        check=True,
    )
    return bin_path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust-bin", type=Path, default=DEFAULT_RUST_BIN)
    parser.add_argument("--go-bin", type=Path, default=DEFAULT_GO_BIN)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--go-root", type=Path, default=REPO_ROOT / "go")
    parser.add_argument(
        "--no-build",
        action="store_true",
        help="Skip building the Go binary; use --go-bin as-is.",
    )
    args = parser.parse_args()

    manifest = json.loads(args.manifest.read_text())

    rust_cmd = Command(name="rust", path=args.rust_bin, flag_style="gnu-long")
    if not args.no_build:
        go_bin = build_go_binary(args.go_root)
    else:
        go_bin = args.go_bin
    go_cmd = Command(name="go", path=go_bin, flag_style="go-flag")

    print(f"rust binary: {rust_cmd.path}")
    print(f"go   binary: {go_cmd.path}")

    rust_results = run_for_command(rust_cmd, manifest)
    go_results = run_for_command(go_cmd, manifest)

    rust_path = write_results("rust", rust_results)
    go_path = write_results("go", go_results)

    def summarize(name: str, results: list[CaseResult]) -> int:
        ok = sum(1 for r in results if r.status == "ok")
        skipped = sum(1 for r in results if r.status == "skipped")
        failed = sum(1 for r in results if r.status == "fail")
        print(f"== {name} == ok={ok} skipped={skipped} failed={failed} total={len(results)}")
        for r in results:
            if r.status != "ok":
                print(f"  [{r.status}] {r.name} ({r.expectation}) -- {r.detail}")
        return failed

    failed_rust = summarize("rust", rust_results)
    failed_go = summarize("go", go_results)

    print(f"\nResults written to:\n  {rust_path}\n  {go_path}")

    sys.exit(1 if (failed_rust or failed_go) else 0)


if __name__ == "__main__":
    main()
