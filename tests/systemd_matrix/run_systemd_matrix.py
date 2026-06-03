#!/usr/bin/env python3
"""Build systemd-version helpers and run sanitized reader compatibility checks."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import threading
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, BinaryIO

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from tests.corpus_eval.canonical import SCHEMA_VERSION as DIGEST_SCHEMA
from tests.corpus_eval.canonical import digest_export_stream


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUT = ROOT / ".local" / "systemd-matrix"
DEFAULT_SYSTEMD_SRC = Path(
    os.environ.get("SYSTEMD_SRC", str(Path.home() / "src" / "systemd.git"))
)
DEFAULT_DATASET = ROOT / "tests" / "datasets" / "correctness" / "corpus.jsonl"
SYSTEMD_HELPER_SOURCE = (
    ROOT / "tests" / "datasets" / "ingesters" / "systemd" / "dataset_ingester.c"
)
SYSTEMD_HELPER_NAME = "test-systemd-matrix-ingester"
SYSTEMD_HELPER_SOURCE_NAME = f"{SYSTEMD_HELPER_NAME}.c"
REPORT_SCHEMA = "systemd-journal-sdk-systemd-matrix-v1"
DEFAULT_TIMEOUT = 1800

DISCREPANCY_CODES = {
    "OK": "no discrepancy detected",
    "BUILD_FAILED": "systemd or SDK helper build failed",
    "GENERATE_FAILED": "systemd helper could not generate the journal corpus",
    "MISSING_TOOL": "a required local tool was unavailable",
    "VERSION_VERIFY_FAILED": "version-built journalctl verification failed",
    "STOCK_VERIFY_FAILED": "stock journalctl verification failed",
    "VERSION_READ_FAILED": "version-built journalctl export read failed",
    "STOCK_READ_FAILED": "stock journalctl export read failed",
    "RUST_READ_FAILED": "Rust SDK digest helper failed",
    "GO_READ_FAILED": "Go SDK digest helper failed",
    "PYTHON_READ_FAILED": "Python SDK file-backed journalctl export failed",
    "NODE_READ_FAILED": "Node.js SDK file-backed journalctl export failed",
    "DIGEST_MISMATCH": "reader logical digest differs from the selected baseline",
    "COUNT_MISMATCH": "reader logical counts differ from the selected baseline",
    "VERSION_EXPORT_METADATA_DRIFT": (
        "version-built journalctl export differs from modern stock output while "
        "counts match; this is recorded as a historical-export observation"
    ),
    "VERSION_JOURNALCTL_UNAVAILABLE": "version build did not produce journalctl",
    "VERIFY_KEY_MISSING": "sealed journal verification key was not available",
}


@dataclass(frozen=True)
class CommandResult:
    label: str
    returncode: int
    elapsed_seconds: float
    stdout_sha256: str
    stdout_bytes: int
    stderr_sha256: str
    stderr_bytes: int
    command_sha256: str
    timeout_seconds: int | None = None
    timed_out: bool = False

    def as_dict(self) -> dict[str, Any]:
        data = {
            "label": self.label,
            "returncode": self.returncode,
            "elapsed_seconds": self.elapsed_seconds,
            "stdout_sha256": self.stdout_sha256,
            "stdout_bytes": self.stdout_bytes,
            "stderr_sha256": self.stderr_sha256,
            "stderr_bytes": self.stderr_bytes,
            "command_sha256": self.command_sha256,
            "timed_out": self.timed_out,
        }
        if self.timeout_seconds is not None:
            data["timeout_seconds"] = self.timeout_seconds
        return data


def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def version_slug(version: str) -> str:
    allowed = set("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-")
    slug = "".join(ch if ch in allowed else "_" for ch in version)
    return slug or "version"


def relative(path: Path | str) -> str:
    path = Path(path)
    try:
        return str(path.resolve().relative_to(ROOT))
    except (FileNotFoundError, ValueError):
        try:
            return str(path.relative_to(ROOT))
        except ValueError:
            redacted = sha256_bytes(str(path).encode("utf-8", "surrogateescape"))[:24]
            return f"external-path-sha256:{redacted}"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def command_sha(cmd: list[str]) -> str:
    encoded = b"\0".join(part.encode("utf-8", "surrogateescape") for part in cmd)
    return sha256_bytes(encoded)


def write_json(path: Path, data: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def matrix_env(out: Path) -> dict[str, str]:
    cache = out / "sdk-build"
    env = os.environ.copy()
    env.update(
        {
            "CARGO_HOME": str(cache / "cargo-home"),
            "CARGO_TARGET_DIR": str(cache / "cargo-target"),
            "GOCACHE": str(cache / "go-cache"),
            "GOMODCACHE": str(cache / "go-mod-cache"),
            "GOPATH": str(cache / "go-path"),
            "npm_config_cache": str(cache / "npm-cache"),
            "PIP_CACHE_DIR": str(cache / "pip-cache"),
            "PYTHONPATH": os.pathsep.join(
                [str(ROOT / ".local" / "python-deps"), str(ROOT / "python")]
            ),
        }
    )
    return env


def run_capture(
    label: str,
    cmd: list[str],
    *,
    cwd: Path = ROOT,
    env: dict[str, str] | None = None,
    timeout: int = DEFAULT_TIMEOUT,
) -> tuple[CommandResult, bytes]:
    started = time.perf_counter()
    try:
        proc = subprocess.run(
            cmd,
            cwd=str(cwd),
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
        elapsed = time.perf_counter() - started
        return (
            CommandResult(
                label=label,
                returncode=proc.returncode,
                elapsed_seconds=elapsed,
                stdout_sha256=sha256_bytes(proc.stdout),
                stdout_bytes=len(proc.stdout),
                stderr_sha256=sha256_bytes(proc.stderr),
                stderr_bytes=len(proc.stderr),
                command_sha256=command_sha(cmd),
                timeout_seconds=timeout,
            ),
            proc.stdout,
        )
    except subprocess.TimeoutExpired as err:
        stdout = err.stdout or b""
        stderr = err.stderr or b""
        elapsed = time.perf_counter() - started
        return (
            CommandResult(
                label=label,
                returncode=124,
                elapsed_seconds=elapsed,
                stdout_sha256=sha256_bytes(stdout),
                stdout_bytes=len(stdout),
                stderr_sha256=sha256_bytes(stderr),
                stderr_bytes=len(stderr),
                command_sha256=command_sha(cmd),
                timeout_seconds=timeout,
                timed_out=True,
            ),
            stdout,
        )


def run_json_line(
    label: str,
    cmd: list[str],
    *,
    cwd: Path = ROOT,
    env: dict[str, str] | None = None,
    timeout: int = DEFAULT_TIMEOUT,
) -> tuple[dict[str, Any] | None, CommandResult]:
    result, stdout = run_capture(label, cmd, cwd=cwd, env=env, timeout=timeout)
    if result.returncode != 0:
        return None, result
    lines = [line for line in stdout.splitlines() if line.strip()]
    if len(lines) != 1:
        return None, CommandResult(
            label=result.label,
            returncode=65,
            elapsed_seconds=result.elapsed_seconds,
            stdout_sha256=result.stdout_sha256,
            stdout_bytes=result.stdout_bytes,
            stderr_sha256=result.stderr_sha256,
            stderr_bytes=result.stderr_bytes,
            command_sha256=result.command_sha256,
            timeout_seconds=result.timeout_seconds,
            timed_out=result.timed_out,
        )
    try:
        parsed = json.loads(lines[0])
    except json.JSONDecodeError:
        return None, CommandResult(
            label=result.label,
            returncode=65,
            elapsed_seconds=result.elapsed_seconds,
            stdout_sha256=result.stdout_sha256,
            stdout_bytes=result.stdout_bytes,
            stderr_sha256=result.stderr_sha256,
            stderr_bytes=result.stderr_bytes,
            command_sha256=result.command_sha256,
            timeout_seconds=result.timeout_seconds,
            timed_out=result.timed_out,
        )
    if not isinstance(parsed, dict):
        return None, result
    return parsed, result


def drain_digest(stream: BinaryIO) -> dict[str, Any]:
    digest = hashlib.sha256()
    byte_count = 0
    while True:
        chunk = stream.read(65536)
        if not chunk:
            break
        digest.update(chunk)
        byte_count += len(chunk)
    return {"sha256": digest.hexdigest(), "bytes": byte_count}


class HashingReader:
    """Read wrapper that hashes bytes consumed by the export parser."""

    def __init__(self, stream: BinaryIO):
        self._stream = stream
        self._digest = hashlib.sha256()
        self.bytes = 0

    def _record(self, data: bytes) -> bytes:
        if data:
            self._digest.update(data)
            self.bytes += len(data)
        return data

    def read(self, size: int = -1) -> bytes:
        return self._record(self._stream.read(size))

    def readline(self, size: int = -1) -> bytes:
        return self._record(self._stream.readline(size))

    def hexdigest(self) -> str:
        return self._digest.hexdigest()


def require_under(path: Path, root: Path, label: str) -> Path:
    resolved = path.resolve()
    root = root.resolve()
    if resolved == root or root in resolved.parents:
        return resolved
    raise SystemExit(f"{label} must be under {root}")


def journalctl_version(journalctl: Path | str, env: dict[str, str], timeout: int) -> dict[str, Any]:
    cmd = [str(journalctl), "--version"]
    result, stdout = run_capture("journalctl version", cmd, env=env, timeout=timeout)
    first_line = ""
    if result.returncode == 0:
        first_line = stdout.decode("utf-8", "replace").splitlines()[0:1]
        first_line = first_line[0] if first_line else ""
    return {
        "available": result.returncode == 0,
        "version_line": first_line,
        "command": result.as_dict(),
    }


def stream_export_command_digest(
    label: str,
    cmd: list[str],
    *,
    env: dict[str, str],
    timeout: int,
) -> tuple[dict[str, Any] | None, CommandResult]:
    started = time.perf_counter()
    proc = subprocess.Popen(
        cmd,
        cwd=str(ROOT),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert proc.stdout is not None
    assert proc.stderr is not None
    digest_state: dict[str, Any] = {}
    stderr_state: dict[str, Any] = {}
    stdout_state: dict[str, Any] = {}

    def parse_stdout() -> None:
        hashing_stdout = HashingReader(proc.stdout)
        try:
            digest_state["digest"] = digest_export_stream(hashing_stdout)
        except Exception as exc:  # pragma: no cover - exercised by bad helpers.
            digest_state["error_class"] = type(exc).__name__
            digest_state["error_sha256"] = sha256_bytes(str(exc).encode("utf-8"))
        finally:
            stdout_state["sha256"] = hashing_stdout.hexdigest()
            stdout_state["bytes"] = hashing_stdout.bytes

    def drain_stderr() -> None:
        try:
            stderr_state.update(drain_digest(proc.stderr))
        except Exception as exc:  # pragma: no cover - defensive only.
            stderr_state["sha256"] = sha256_bytes(str(exc).encode("utf-8"))
            stderr_state["bytes"] = 0

    stdout_thread = threading.Thread(target=parse_stdout, name=f"{label}-stdout")
    stderr_thread = threading.Thread(target=drain_stderr, name=f"{label}-stderr")
    stdout_thread.start()
    stderr_thread.start()

    deadline = time.monotonic() + timeout
    timed_out = False
    while proc.poll() is None and stdout_thread.is_alive():
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            timed_out = True
            proc.kill()
            break
        stdout_thread.join(timeout=min(0.1, remaining))

    if "error_class" in digest_state and proc.poll() is None:
        proc.kill()

    try:
        remaining = max(0.0, deadline - time.monotonic())
        returncode = proc.wait(timeout=remaining)
    except subprocess.TimeoutExpired:
        timed_out = True
        proc.kill()
        returncode = proc.wait()

    stdout_thread.join(timeout=5)
    stderr_thread.join(timeout=5)
    if stdout_thread.is_alive() or stderr_thread.is_alive():
        timed_out = True
        returncode = 124

    elapsed = time.perf_counter() - started
    empty_sha = sha256_bytes(b"")
    command_result = CommandResult(
        label=label,
        returncode=returncode if not timed_out else 124,
        elapsed_seconds=elapsed,
        stdout_sha256=str(stdout_state.get("sha256", empty_sha)),
        stdout_bytes=int(stdout_state.get("bytes", 0)),
        stderr_sha256=str(stderr_state.get("sha256", empty_sha)),
        stderr_bytes=int(stderr_state.get("bytes", 0)),
        command_sha256=command_sha(cmd),
        timeout_seconds=timeout,
        timed_out=timed_out,
    )
    if timed_out or returncode != 0 or "error_class" in digest_state:
        return None, command_result
    digest = digest_state.get("digest")
    if not isinstance(digest, dict):
        return None, command_result
    return digest, command_result


def stream_journalctl_digest(
    label: str,
    journalctl: Path | str,
    journal_path: Path,
    *,
    env: dict[str, str],
    timeout: int,
) -> tuple[dict[str, Any] | None, CommandResult]:
    return stream_export_command_digest(
        label,
        [
            str(journalctl),
            "--file",
            str(journal_path),
            "--output=export",
            "--all",
            "--no-pager",
        ],
        env=env,
        timeout=timeout,
    )


def maybe_meson_option(options_text: str, name: str, value: str) -> list[str]:
    if f"option('{name}'" in options_text or f'option("{name}"' in options_text:
        if value == "disabled" and f"option('{name}', type : 'combo', choices : ['auto', 'true', 'false']" in options_text:
            value = "false"
        return [f"-D{name}={value}"]
    return []


def resolve_systemd_ref(systemd_src: Path, version: str, explicit_ref: str | None) -> str:
    ref = explicit_ref or version
    cmd = ["git", "-C", str(systemd_src), "rev-parse", f"{ref}^{{commit}}"]
    proc = subprocess.run(cmd, cwd=str(ROOT), text=True, capture_output=True, check=False)
    if proc.returncode != 0:
        raise RuntimeError(f"could not resolve systemd ref {ref!r} from {systemd_src}")
    return proc.stdout.strip()


def is_git_checkout(path: Path) -> bool:
    proc = subprocess.run(
        ["git", "-C", str(path), "rev-parse", "--show-toplevel"],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        check=False,
    )
    if proc.returncode != 0:
        return False
    return Path(proc.stdout.strip()).resolve() == path.resolve()


def non_git_source_fingerprint(path: Path) -> str:
    """Return a report-only identifier for unpacked release source trees."""
    digest = hashlib.sha256()
    for relative_name in (
        "meson.build",
        "meson_options.txt",
        "NEWS",
        "src/libsystemd/sd-journal/journal-file.c",
        "src/libsystemd/sd-journal/journal-authenticate.c",
    ):
        item = path / relative_name
        if not item.exists():
            continue
        digest.update(relative_name.encode("utf-8"))
        digest.update(b"\0")
        digest.update(sha256_file(item).encode("ascii"))
        digest.update(b"\0")
    return f"non-git-source-sha256:{digest.hexdigest()[:24]}"


def ensure_systemd_source(
    version: str,
    *,
    out: Path,
    systemd_src: Path,
    source_ref: str | None,
    timeout: int,
) -> tuple[Path, str, list[dict[str, Any]]]:
    slug = version_slug(version)
    version_root = out / "builds" / slug
    source_dir = version_root / "source"
    commands: list[dict[str, Any]] = []
    if not is_git_checkout(systemd_src):
        if not (systemd_src / "meson.build").exists():
            raise RuntimeError(f"systemd source tree is not buildable: {systemd_src}")
        if source_dir.exists():
            shutil.rmtree(source_dir)
        source_dir.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(
            systemd_src,
            source_dir,
            symlinks=True,
            ignore=shutil.ignore_patterns(".git", "build", "__pycache__"),
        )
        return source_dir, non_git_source_fingerprint(systemd_src), commands

    commit = resolve_systemd_ref(systemd_src, version, source_ref)
    if not (source_dir / ".git").exists():
        source_dir.parent.mkdir(parents=True, exist_ok=True)
        cmd = ["git", "clone", "--no-checkout", "--local", str(systemd_src), str(source_dir)]
        result, _ = run_capture(
            "clone systemd source",
            cmd,
            env=matrix_env(out),
            timeout=timeout,
        )
        commands.append(result.as_dict())
        if result.returncode != 0:
            raise RuntimeError("systemd source clone failed")
    for label, cmd in (
        (
            "fetch requested systemd commit",
            ["git", "-C", str(source_dir), "fetch", "--tags", str(systemd_src), commit],
        ),
        (
            "checkout requested systemd commit",
            ["git", "-C", str(source_dir), "checkout", "--detach", commit],
        ),
    ):
        result, _ = run_capture(label, cmd, env=matrix_env(out), timeout=timeout)
        commands.append(result.as_dict())
        if result.returncode != 0:
            raise RuntimeError(f"{label} failed")
    return source_dir, commit, commands


def patch_systemd_helper(source_dir: Path) -> None:
    helper_dest = (
        source_dir / "src" / "libsystemd" / "sd-journal" / SYSTEMD_HELPER_SOURCE_NAME
    )
    shutil.copyfile(SYSTEMD_HELPER_SOURCE, helper_dest)

    meson_file = source_dir / "src" / "libsystemd" / "meson.build"
    text = meson_file.read_text(encoding="utf-8")
    if SYSTEMD_HELPER_SOURCE_NAME not in text:
        marker = """        {
                'sources' : files('sd-journal/test-journal-append.c'),
                'type' : 'manual',
        },
"""
        entry = f"""        {{
                'sources' : files('sd-journal/{SYSTEMD_HELPER_SOURCE_NAME}'),
                'type' : 'manual',
        }},
"""
        if marker in text:
            text = text.replace(marker, marker + entry)
        else:
            simple_marker = "        'sd-journal/test-journal-file.c',\n"
            simple_entry = f"        'sd-journal/{SYSTEMD_HELPER_SOURCE_NAME}',\n"
            legacy_marker = """        [files('sd-journal/test-format-change-ingester.c'),
         [], [], [], '', 'manual'],
"""
            legacy_entry = f"""        [files('sd-journal/{SYSTEMD_HELPER_SOURCE_NAME}'),
         [], [], [], '', 'manual'],
"""
            if simple_marker in text:
                text = text.replace(simple_marker, simple_marker + simple_entry)
            elif legacy_marker in text:
                text = text.replace(legacy_marker, legacy_marker + legacy_entry)
            else:
                raise RuntimeError("could not find systemd meson journal test marker")
        meson_file.write_text(text, encoding="utf-8")

    authenticate_file = (
        source_dir / "src" / "libsystemd" / "sd-journal" / "journal-authenticate.c"
    )
    text = authenticate_file.read_text(encoding="utf-8")
    if "SYSTEMD_JOURNAL_FSS_ROOT" not in text:
        # Generation opens the file through systemd internals, which load the
        # FSS state path before appending. Verification still uses --verify-key
        # and must not depend on any host or repository-local FSS state.
        if "#include <stdlib.h>" not in text:
            text = text.replace("#include <unistd.h>\n", "#include <stdlib.h>\n#include <unistd.h>\n")
        new = """        const char *fss_root = getenv("SYSTEMD_JOURNAL_FSS_ROOT");
        if (!fss_root || !*fss_root)
                fss_root = "/var/log/journal";

        if (asprintf(&path, "%s/" SD_ID128_FORMAT_STR "/fss",
                     fss_root, SD_ID128_FORMAT_VAL(machine)) < 0)
                return -ENOMEM;
"""
        old = """        if (asprintf(&path, "/var/log/journal/" SD_ID128_FORMAT_STR "/fss",
                     SD_ID128_FORMAT_VAL(machine)) < 0)
                return -ENOMEM;
"""
        old_legacy = """        if (asprintf(&p, "/var/log/journal/" SD_ID128_FORMAT_STR "/fss",
                     SD_ID128_FORMAT_VAL(machine)) < 0)
                return -ENOMEM;
"""
        new_legacy = """        const char *fss_root = getenv("SYSTEMD_JOURNAL_FSS_ROOT");
        if (!fss_root || !*fss_root)
                fss_root = "/var/log/journal";

        if (asprintf(&p, "%s/" SD_ID128_FORMAT_STR "/fss",
                     fss_root, SD_ID128_FORMAT_VAL(machine)) < 0)
                return -ENOMEM;
"""
        if old in text:
            text = text.replace(old, new)
        elif old_legacy in text:
            text = text.replace(old_legacy, new_legacy)
        else:
            raise RuntimeError("could not find systemd journal FSS path marker")
        authenticate_file.write_text(text, encoding="utf-8")

    filesystems_file = source_dir / "src" / "basic" / "filesystems-gperf.gperf"
    if filesystems_file.exists():
        text = filesystems_file.read_text(encoding="utf-8")
        additions = {
            "bcachefs,": "bcachefs,        {BCACHEFS_SUPER_MAGIC}\n",
            "guest_memfd,": "guest_memfd,     {GUEST_MEMFD_MAGIC}\n",
            "pidfs,": "pidfs,           {PID_FS_MAGIC}\n",
        }
        missing = [line for marker, line in additions.items() if marker not in text]
        if missing:
            filesystems_file.write_text(text.rstrip() + "\n" + "".join(missing), encoding="utf-8")


def build_systemd(args: argparse.Namespace) -> dict[str, Any]:
    out = args.out.resolve()
    env = matrix_env(out)
    slug = version_slug(args.version)
    version_root = out / "builds" / slug
    build_dir = version_root / "build"
    reports_dir = out / "reports"
    commands: list[dict[str, Any]] = []
    status = "ok"
    discrepancies: list[dict[str, Any]] = []
    commit = ""
    source_dir: Path | None = None

    try:
        source_dir, commit, source_commands = ensure_systemd_source(
            args.version,
            out=out,
            systemd_src=args.systemd_src.resolve(),
            source_ref=args.source_ref,
            timeout=args.timeout,
        )
        commands.extend(source_commands)
        patch_systemd_helper(source_dir)
        options_text = (source_dir / "meson_options.txt").read_text(encoding="utf-8")
        meson_opts = []
        meson_opts += maybe_meson_option(options_text, "mode", "release")
        meson_opts += maybe_meson_option(options_text, "tests", "true")
        meson_opts += maybe_meson_option(options_text, "man", "disabled")
        meson_opts += maybe_meson_option(options_text, "html", "disabled")
        meson_opts += maybe_meson_option(options_text, "fuzz-tests", "false")
        meson_opts += maybe_meson_option(options_text, "slow-tests", "false")
        meson_opts += maybe_meson_option(options_text, "link-journalctl-shared", "false")
        if not (build_dir / "build.ninja").exists():
            if build_dir.exists():
                shutil.rmtree(build_dir)
            cmd = ["meson", "setup", str(build_dir), str(source_dir), *meson_opts]
            result, _ = run_capture(
                "meson setup systemd",
                cmd,
                env=env,
                timeout=args.timeout,
            )
            commands.append(result.as_dict())
            if result.returncode != 0:
                raise RuntimeError("meson setup failed")
        else:
            cmd = ["meson", "setup", "--reconfigure", str(build_dir), str(source_dir), *meson_opts]
            result, _ = run_capture(
                "meson reconfigure systemd",
                cmd,
                env=env,
                timeout=args.timeout,
            )
            commands.append(result.as_dict())
            if result.returncode != 0:
                raise RuntimeError("meson reconfigure failed")
        cmd = ["ninja", "-C", str(build_dir), "journalctl", SYSTEMD_HELPER_NAME]
        result, _ = run_capture("ninja systemd matrix targets", cmd, env=env, timeout=args.timeout)
        commands.append(result.as_dict())
        if result.returncode != 0:
            raise RuntimeError("ninja target build failed")
    except Exception as exc:
        status = "failed"
        discrepancies.append(
            {
                "code": "BUILD_FAILED",
                "error_class": type(exc).__name__,
                "error_sha256": sha256_bytes(str(exc).encode("utf-8")),
            }
        )

    journalctl = build_dir / "journalctl"
    generator = build_dir / SYSTEMD_HELPER_NAME
    report = {
        "schema": REPORT_SCHEMA,
        "kind": "build",
        "created_at": utc_now(),
        "version": args.version,
        "source": {
            "upstream": "systemd/systemd",
            "requested_ref": args.source_ref or args.version,
            "commit": commit,
            "local_source_used": args.systemd_src.exists(),
        },
        "artifacts": {
            "root": relative(version_root),
            "source": relative(source_dir) if source_dir else None,
            "build": relative(build_dir),
            "journalctl": relative(journalctl) if journalctl.exists() else None,
            "generator": relative(generator) if generator.exists() else None,
        },
        "commands": commands,
        "discrepancies": discrepancies,
        "status": status,
    }
    write_json(reports_dir / f"build-{slug}.json", report)
    write_markdown_report(reports_dir / f"build-{slug}.md", report)
    return report


def build_metadata(out: Path, version: str) -> dict[str, Any] | None:
    path = out / "reports" / f"build-{version_slug(version)}.json"
    if not path.exists():
        return None
    return load_json(path)


def ensure_build(args: argparse.Namespace) -> dict[str, Any]:
    report = build_metadata(args.out.resolve(), args.version)
    if report and report.get("status") == "ok":
        artifacts = report.get("artifacts", {})
        journalctl = artifacts.get("journalctl")
        generator = artifacts.get("generator")
        if journalctl and generator and (ROOT / journalctl).exists() and (ROOT / generator).exists():
            return report
    return build_systemd(args)


def build_sdk_tools(out: Path, timeout: int) -> dict[str, Any]:
    env = matrix_env(out)
    reports_dir = out / "reports"
    bin_dir = out / "sdk-build" / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    commands: list[dict[str, Any]] = []
    status = "ok"
    discrepancies: list[dict[str, Any]] = []
    steps = [
        (
            "build Rust corpus digest helper",
            ["cargo", "build", "--release", "-p", "corpus_digest"],
            ROOT / "rust",
        ),
        (
            "build Go corpus digest helper",
            [
                "go",
                "build",
                "-o",
                str(bin_dir / "go-corpus-digest"),
                "./internal/testcmd/corpus_digest",
            ],
            ROOT / "go",
        ),
    ]
    for label, cmd, cwd in steps:
        result, _ = run_capture(label, cmd, cwd=cwd, env=env, timeout=timeout)
        commands.append(result.as_dict())
        if result.returncode != 0:
            status = "failed"
            discrepancies.append({"code": "BUILD_FAILED", "tool": label})
            break
    report = {
        "schema": REPORT_SCHEMA,
        "kind": "sdk-tools-build",
        "created_at": utc_now(),
        "status": status,
        "artifacts": {
            "rust_digest": relative(out / "sdk-build" / "cargo-target" / "release" / "corpus_digest"),
            "go_digest": relative(bin_dir / "go-corpus-digest"),
        },
        "commands": commands,
        "discrepancies": discrepancies,
    }
    write_json(reports_dir / "sdk-tools.json", report)
    write_markdown_report(reports_dir / "sdk-tools.md", report)
    return report


def sdk_tool_paths(out: Path, timeout: int) -> tuple[Path, Path, dict[str, Any]]:
    report = build_sdk_tools(out, timeout)
    rust_digest = out / "sdk-build" / "cargo-target" / "release" / "corpus_digest"
    go_digest = out / "sdk-build" / "bin" / "go-corpus-digest"
    return rust_digest, go_digest, report


def generated_journal_path(out: Path, version: str, case: str) -> Path:
    return out / "corpus" / version_slug(version) / f"{version_slug(case)}.journal"


def verification_key_path(out: Path, version: str, case: str) -> Path:
    return out / "secrets" / version_slug(version) / f"{version_slug(case)}.verify-key"


def fss_root_path(out: Path, version: str, case: str) -> Path:
    return out / "fss" / version_slug(version) / version_slug(case)


def sanitize_generator_payload(
    payload: dict[str, Any] | None,
    key_path: Path,
) -> tuple[dict[str, Any] | None, str | None]:
    if payload is None:
        return None, None
    sanitized = dict(payload)
    key = sanitized.pop("verification_key", None)
    if not isinstance(key, str):
        return sanitized, None
    key_path.parent.mkdir(parents=True, exist_ok=True)
    key_path.write_text(key + "\n", encoding="utf-8")
    key_path.chmod(0o600)
    sanitized["verification_key_sha256"] = sha256_bytes(key.encode("utf-8"))
    sanitized["verification_key_file"] = relative(key_path)
    return sanitized, key


def generate_corpus(args: argparse.Namespace) -> dict[str, Any]:
    out = args.out.resolve()
    build = build_systemd(args) if args.sealed else ensure_build(args)
    slug = version_slug(args.version)
    case = version_slug(args.case)
    reports_dir = out / "reports"
    discrepancies: list[dict[str, Any]] = []
    status = "ok"
    journal_path = args.journal or generated_journal_path(out, args.version, args.case)
    journal_path = require_under(journal_path, out, "--journal output")
    key_path = verification_key_path(out, args.version, args.case)
    journal_path.parent.mkdir(parents=True, exist_ok=True)
    journal_path.unlink(missing_ok=True)
    key_path.unlink(missing_ok=True)
    artifacts = build.get("artifacts", {}) if build else {}
    generator_rel = artifacts.get("generator")
    generator = ROOT / generator_rel if generator_rel else None
    payload: dict[str, Any] | None = None
    command: dict[str, Any] | None = None

    if not generator or not generator.exists():
        status = "failed"
        discrepancies.append({"code": "GENERATE_FAILED", "reason": "missing-generator"})
    else:
        cmd = [
            str(generator),
            "--dataset",
            str(args.dataset.resolve()),
            "--output",
            str(journal_path),
            "--final-state",
            args.final_state,
            "--max-size-bytes",
            str(args.max_size_bytes),
        ]
        if args.compact:
            cmd.append("--compact")
        if args.sealed:
            fss_root = fss_root_path(out, args.version, args.case)
            if fss_root.exists():
                shutil.rmtree(fss_root)
            fss_root.mkdir(parents=True, exist_ok=True)
            cmd.extend(["--sealed", "--fss-root", str(fss_root)])
        payload, result = run_json_line(
            "generate deterministic systemd corpus",
            cmd,
            env=matrix_env(out),
            timeout=args.timeout,
        )
        command = result.as_dict()
        if result.returncode != 0 or payload is None:
            status = "failed"
            discrepancies.append({"code": "GENERATE_FAILED", "command_sha256": result.command_sha256})
        else:
            payload, _ = sanitize_generator_payload(payload, key_path)

    journal = None
    if status == "ok" and journal_path.exists():
        stat = journal_path.stat()
        journal = {
            "artifact": relative(journal_path),
            "size_bytes": stat.st_size,
            "sha256": sha256_file(journal_path),
            "producer": "systemd-matrix-ingester",
            "final_state": args.final_state,
            "compact": args.compact,
            "sealed": args.sealed,
        }
    elif status == "ok":
        status = "failed"
        discrepancies.append({"code": "GENERATE_FAILED", "reason": "missing-output"})

    report = {
        "schema": REPORT_SCHEMA,
        "kind": "generate",
        "created_at": utc_now(),
        "version": args.version,
        "source_commit": build.get("source", {}).get("commit") if build else None,
        "case": args.case,
        "dataset": {
            "artifact": relative(args.dataset),
            "sha256": sha256_file(args.dataset.resolve()),
        },
        "journal": journal,
        "generator_result": payload,
        "verification_key": {
            "present": key_path.exists(),
            "artifact": relative(key_path) if key_path.exists() else None,
            "sha256": sha256_file(key_path) if key_path.exists() else None,
        } if args.sealed else None,
        "command": command,
        "status": status,
        "discrepancies": discrepancies,
    }
    write_json(reports_dir / f"generate-{slug}-{case}.json", report)
    write_markdown_report(reports_dir / f"generate-{slug}-{case}.md", report)
    return report


def verify_with_journalctl(
    role: str,
    journalctl: Path | str,
    journal_path: Path,
    *,
    env: dict[str, str],
    timeout: int,
    verification_key: str | None = None,
) -> dict[str, Any]:
    cmd = [str(journalctl), "--verify", "--file", str(journal_path)]
    if verification_key:
        cmd.append(f"--verify-key={verification_key}")
    result, _ = run_capture(role, cmd, env=env, timeout=timeout)
    return {
        "role": role,
        "kind": "verify",
        "status": "ok" if result.returncode == 0 else "failed",
        "command": result.as_dict(),
    }


def read_with_journalctl(
    role: str,
    journalctl: Path | str,
    journal_path: Path,
    *,
    env: dict[str, str],
    timeout: int,
) -> dict[str, Any]:
    digest, result = stream_journalctl_digest(
        role,
        journalctl,
        journal_path,
        env=env,
        timeout=timeout,
    )
    row = {
        "role": role,
        "kind": "reader",
        "status": "ok" if digest is not None and result.returncode == 0 else "failed",
        "schema": DIGEST_SCHEMA,
        "command": result.as_dict(),
    }
    if digest is not None:
        row["logical_digest"] = digest.get("logical_digest")
        row["counts"] = digest.get("counts")
    return row


def read_with_sdk(
    role: str,
    binary: Path,
    journal_path: Path,
    *,
    env: dict[str, str],
    timeout: int,
) -> dict[str, Any]:
    cmd = [str(binary), "--input", str(journal_path), "--bounds", "snapshot"]
    payload, result = run_json_line(role, cmd, env=env, timeout=timeout)
    row = {
        "role": role,
        "kind": "reader",
        "status": "ok" if payload is not None and result.returncode == 0 else "failed",
        "schema": DIGEST_SCHEMA,
        "command": result.as_dict(),
    }
    if payload is not None:
        row["logical_digest"] = payload.get("logical_digest")
        row["counts"] = payload.get("counts")
    return row


def read_with_export_command(
    role: str,
    cmd: list[str],
    *,
    env: dict[str, str],
    timeout: int,
) -> dict[str, Any]:
    digest, result = stream_export_command_digest(role, cmd, env=env, timeout=timeout)
    row = {
        "role": role,
        "kind": "reader",
        "status": "ok" if digest is not None and result.returncode == 0 else "failed",
        "schema": DIGEST_SCHEMA,
        "command": result.as_dict(),
    }
    if digest is not None:
        row["logical_digest"] = digest.get("logical_digest")
        row["counts"] = digest.get("counts")
    return row


def compare_readers(
    results: list[dict[str, Any]],
) -> tuple[dict[str, Any] | None, list[dict[str, Any]], list[dict[str, Any]]]:
    readers = [row for row in results if row.get("kind") == "reader" and row.get("status") == "ok"]
    baseline = None
    for preferred in ("stock_journalctl_read", "version_journalctl_read"):
        baseline = next((row for row in readers if row.get("role") == preferred), None)
        if baseline is not None:
            break
    if baseline is None and readers:
        baseline = readers[0]
    discrepancies: list[dict[str, Any]] = []
    observations: list[dict[str, Any]] = []
    if baseline is None:
        return None, discrepancies, observations
    for row in readers:
        if row is baseline:
            continue
        if row.get("logical_digest") != baseline.get("logical_digest"):
            if (
                row.get("role") == "version_journalctl_read"
                and baseline.get("role") == "stock_journalctl_read"
                and row.get("counts") == baseline.get("counts")
            ):
                observations.append(
                    {
                        "code": "VERSION_EXPORT_METADATA_DRIFT",
                        "baseline": baseline.get("role"),
                        "reader": row.get("role"),
                        "baseline_digest": baseline.get("logical_digest"),
                        "reader_digest": row.get("logical_digest"),
                    }
                )
                continue
            discrepancies.append(
                {
                    "code": "DIGEST_MISMATCH",
                    "baseline": baseline.get("role"),
                    "reader": row.get("role"),
                    "baseline_digest": baseline.get("logical_digest"),
                    "reader_digest": row.get("logical_digest"),
                }
            )
        if row.get("counts") != baseline.get("counts"):
            discrepancies.append(
                {
                    "code": "COUNT_MISMATCH",
                    "baseline": baseline.get("role"),
                    "reader": row.get("role"),
                }
            )
    return baseline, discrepancies, observations


def test_matrix(args: argparse.Namespace) -> dict[str, Any]:
    out = args.out.resolve()
    env = matrix_env(out)
    explicit_version_journalctl = getattr(args, "version_journalctl", None)
    build = None if explicit_version_journalctl else ensure_build(args)
    rust_digest, go_digest, sdk_build = sdk_tool_paths(out, args.timeout)
    slug = version_slug(args.version)
    case = version_slug(args.case)
    reports_dir = out / "reports"
    journal_path = args.journal or generated_journal_path(out, args.version, args.case)
    journal_path = require_under(journal_path, out, "--journal input")
    if not journal_path.exists():
        raise SystemExit(f"journal input does not exist: {journal_path}")
    key_path = getattr(args, "verify_key_file", None) or verification_key_path(out, args.version, args.case)
    verification_key = None
    verification_key_info = None
    if key_path.exists():
        verification_key = key_path.read_text(encoding="utf-8").strip()
        verification_key_info = {
            "present": True,
            "artifact": relative(key_path),
            "sha256": sha256_file(key_path),
        }

    artifacts = build.get("artifacts", {}) if build else {}
    if explicit_version_journalctl:
        version_journalctl = explicit_version_journalctl.resolve()
    else:
        version_journalctl = ROOT / artifacts["journalctl"] if artifacts.get("journalctl") else None
    stock_journalctl = shutil.which("journalctl")
    results: list[dict[str, Any]] = []
    discrepancies: list[dict[str, Any]] = []
    tools: dict[str, Any] = {
        "sdk_build": {
            "status": sdk_build.get("status"),
            "report": relative(out / "reports" / "sdk-tools.json"),
        }
    }

    if version_journalctl and version_journalctl.exists():
        tools["version_journalctl"] = journalctl_version(version_journalctl, env, args.timeout)
        results.append(
            verify_with_journalctl(
                "version_journalctl_verify",
                version_journalctl,
                journal_path,
                env=env,
                timeout=args.timeout,
                verification_key=verification_key,
            )
        )
        results.append(
            read_with_journalctl(
                "version_journalctl_read",
                version_journalctl,
                journal_path,
                env=env,
                timeout=args.timeout,
            )
        )
    else:
        discrepancies.append({"code": "VERSION_JOURNALCTL_UNAVAILABLE"})
        tools["version_journalctl"] = {"available": False}

    if stock_journalctl:
        tools["stock_journalctl"] = journalctl_version(stock_journalctl, env, args.timeout)
        results.append(
            verify_with_journalctl(
                "stock_journalctl_verify",
                stock_journalctl,
                journal_path,
                env=env,
                timeout=args.timeout,
                verification_key=verification_key,
            )
        )
        results.append(
            read_with_journalctl(
                "stock_journalctl_read",
                stock_journalctl,
                journal_path,
                env=env,
                timeout=args.timeout,
            )
        )
    else:
        discrepancies.append({"code": "MISSING_TOOL", "tool": "journalctl"})
        tools["stock_journalctl"] = {"available": False}

    if rust_digest.exists():
        results.append(
            read_with_sdk("rust_sdk_read", rust_digest, journal_path, env=env, timeout=args.timeout)
        )
    else:
        discrepancies.append({"code": "BUILD_FAILED", "tool": "rust-corpus-digest"})
    if go_digest.exists():
        results.append(read_with_sdk("go_sdk_read", go_digest, journal_path, env=env, timeout=args.timeout))
    else:
        discrepancies.append({"code": "BUILD_FAILED", "tool": "go-corpus-digest"})
    results.append(
        read_with_export_command(
            "python_sdk_read",
            [
                sys.executable,
                str(ROOT / "python" / "cmd" / "journalctl.py"),
                "--file",
                str(journal_path),
                "--output=export",
            ],
            env=env,
            timeout=args.timeout,
        )
    )
    node = shutil.which("node")
    if node:
        results.append(
            read_with_export_command(
                "node_sdk_read",
                [
                    node,
                    str(ROOT / "node" / "cmd" / "journalctl" / "index.js"),
                    "--file",
                    str(journal_path),
                    "--output",
                    "export",
                ],
                env=env,
                timeout=args.timeout,
            )
        )
    else:
        discrepancies.append({"code": "MISSING_TOOL", "tool": "node"})

    for row in results:
        if row.get("status") == "ok":
            continue
        role = row.get("role")
        if role == "version_journalctl_verify":
            discrepancies.append({"code": "VERSION_VERIFY_FAILED", "role": role})
        elif role == "stock_journalctl_verify":
            discrepancies.append({"code": "STOCK_VERIFY_FAILED", "role": role})
        elif role == "version_journalctl_read":
            discrepancies.append({"code": "VERSION_READ_FAILED", "role": role})
        elif role == "stock_journalctl_read":
            discrepancies.append({"code": "STOCK_READ_FAILED", "role": role})
        elif role == "rust_sdk_read":
            discrepancies.append({"code": "RUST_READ_FAILED", "role": role})
        elif role == "go_sdk_read":
            discrepancies.append({"code": "GO_READ_FAILED", "role": role})
        elif role == "python_sdk_read":
            discrepancies.append({"code": "PYTHON_READ_FAILED", "role": role})
        elif role == "node_sdk_read":
            discrepancies.append({"code": "NODE_READ_FAILED", "role": role})

    baseline, compare_discrepancies, observations = compare_readers(results)
    discrepancies.extend(compare_discrepancies)
    required_roles = {
        "stock_journalctl_verify",
        "stock_journalctl_read",
        "rust_sdk_read",
        "go_sdk_read",
        "python_sdk_read",
        "node_sdk_read",
    }
    present_ok = {
        str(row.get("role"))
        for row in results
        if row.get("role") in required_roles and row.get("status") == "ok"
    }
    status = "ok" if not discrepancies and present_ok == required_roles else "failed"
    stat = journal_path.stat()
    report = {
        "schema": REPORT_SCHEMA,
        "kind": "reader-matrix",
        "created_at": utc_now(),
        "version": args.version,
        "source_commit": build.get("source", {}).get("commit") if build else None,
        "case": args.case,
        "journal": {
            "artifact": relative(journal_path),
            "size_bytes": stat.st_size,
            "sha256": sha256_file(journal_path),
        },
        "tools": tools,
        "verification_key": verification_key_info,
        "results": results,
        "baseline": {"role": baseline.get("role")} if baseline else None,
        "observations": observations,
        "status": status,
        "discrepancies": discrepancies,
    }
    write_json(reports_dir / f"matrix-{slug}-{case}.json", report)
    write_markdown_report(reports_dir / f"matrix-{slug}-{case}.md", report)
    return report


def write_markdown_report(path: Path, report: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines: list[str] = []
    kind = report.get("kind", "report")
    status = report.get("status", "unknown")
    lines.append(f"# systemd matrix {kind}")
    lines.append("")
    lines.append(f"- Status: `{status}`")
    if report.get("version"):
        lines.append(f"- Version: `{report['version']}`")
    if report.get("source_commit"):
        lines.append(f"- systemd commit: `{report['source_commit']}`")
    if report.get("case"):
        lines.append(f"- Case: `{report['case']}`")
    journal = report.get("journal")
    if isinstance(journal, dict):
        lines.append(f"- Journal artifact: `{journal.get('artifact')}`")
        lines.append(f"- Journal bytes: `{journal.get('size_bytes')}`")
        if journal.get("sha256"):
            lines.append(f"- Journal byte sha256: `{journal.get('sha256')}`")
    if report.get("baseline"):
        lines.append(f"- Baseline reader: `{report['baseline'].get('role')}`")
    lines.append("")
    discrepancies = report.get("discrepancies") or []
    lines.append("## Discrepancies")
    if discrepancies:
        for item in discrepancies:
            code = item.get("code", "UNKNOWN")
            lines.append(f"- `{code}`: {DISCREPANCY_CODES.get(code, 'see JSON report')}")
    else:
        lines.append("- `OK`: no discrepancy detected")
    observations = report.get("observations") or []
    if observations:
        lines.append("")
        lines.append("## Observations")
        for item in observations:
            code = item.get("code", "UNKNOWN")
            lines.append(f"- `{code}`: {DISCREPANCY_CODES.get(code, 'see JSON report')}")
    results = report.get("results")
    if isinstance(results, list):
        lines.append("")
        lines.append("## Results")
        lines.append("")
        lines.append("| Role | Kind | Status | Entries | Payloads | Digest |")
        lines.append("| --- | --- | --- | ---: | ---: | --- |")
        for row in results:
            counts = row.get("counts") if isinstance(row.get("counts"), dict) else {}
            digest = str(row.get("logical_digest") or "")
            digest_prefix = f"`{digest[:16]}`" if digest else ""
            lines.append(
                "| "
                + " | ".join(
                    [
                        f"`{row.get('role')}`",
                        f"`{row.get('kind')}`",
                        f"`{row.get('status')}`",
                        str(counts.get("entries", "")),
                        str(counts.get("payloads", "")),
                        digest_prefix,
                    ]
                )
                + " |"
            )
    lines.append("")
    path.write_text("\n".join(lines), encoding="utf-8")


def summarize_report(args: argparse.Namespace) -> dict[str, Any]:
    report = load_json(args.report.resolve())
    write_markdown_report(args.markdown.resolve(), report)
    return {
        "schema": REPORT_SCHEMA,
        "kind": "summary",
        "status": report.get("status"),
        "source_report": relative(args.report),
        "markdown": relative(args.markdown),
        "discrepancy_codes": [item.get("code") for item in report.get("discrepancies", [])],
    }


def smoke(args: argparse.Namespace) -> dict[str, Any]:
    build = build_systemd(args)
    if build.get("status") != "ok":
        return build
    generate = generate_corpus(args)
    if generate.get("status") != "ok":
        return generate
    matrix = test_matrix(args)
    smoke_report = {
        "schema": REPORT_SCHEMA,
        "kind": "smoke",
        "created_at": utc_now(),
        "version": args.version,
        "case": args.case,
        "status": matrix.get("status"),
        "reports": {
            "build": relative(args.out.resolve() / "reports" / f"build-{version_slug(args.version)}.json"),
            "generate": relative(
                args.out.resolve()
                / "reports"
                / f"generate-{version_slug(args.version)}-{version_slug(args.case)}.json"
            ),
            "matrix": relative(
                args.out.resolve()
                / "reports"
                / f"matrix-{version_slug(args.version)}-{version_slug(args.case)}.json"
            ),
        },
        "discrepancies": matrix.get("discrepancies", []),
        "observations": matrix.get("observations", []),
    }
    report_path = args.out.resolve() / "reports" / f"smoke-{version_slug(args.version)}-{version_slug(args.case)}.json"
    write_json(report_path, smoke_report)
    write_markdown_report(
        args.out.resolve() / "reports" / f"smoke-{version_slug(args.version)}-{version_slug(args.case)}.md",
        smoke_report,
    )
    return smoke_report


def add_common_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--version", default="v260.1", help="systemd tag/ref label")
    parser.add_argument("--source-ref", help="systemd git ref/commit to build; defaults to --version")
    parser.add_argument("--systemd-src", type=Path, default=DEFAULT_SYSTEMD_SRC)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    build = sub.add_parser("build", help="build systemd journalctl and corpus generator")
    add_common_args(build)

    generate = sub.add_parser("generate", help="generate a deterministic systemd journal corpus")
    add_common_args(generate)
    generate.add_argument("--case", default="smoke")
    generate.add_argument("--dataset", type=Path, default=DEFAULT_DATASET)
    generate.add_argument("--journal", type=Path)
    generate.add_argument("--final-state", choices=("online", "offline", "archived"), default="offline")
    generate.add_argument("--compact", action="store_true")
    generate.add_argument("--sealed", action="store_true")
    generate.add_argument("--max-size-bytes", type=int, default=64 * 1024 * 1024)

    test = sub.add_parser("test", help="run stock/version/Rust/Go reader matrix")
    add_common_args(test)
    test.add_argument("--case", default="smoke")
    test.add_argument("--journal", type=Path)
    test.add_argument("--verify-key-file", type=Path)
    test.add_argument("--version-journalctl", type=Path)

    smoke_cmd = sub.add_parser("smoke", help="build, generate, and test one version")
    add_common_args(smoke_cmd)
    smoke_cmd.add_argument("--case", default="smoke")
    smoke_cmd.add_argument("--dataset", type=Path, default=DEFAULT_DATASET)
    smoke_cmd.add_argument("--journal", type=Path)
    smoke_cmd.add_argument("--final-state", choices=("online", "offline", "archived"), default="offline")
    smoke_cmd.add_argument("--compact", action="store_true")
    smoke_cmd.add_argument("--sealed", action="store_true")
    smoke_cmd.add_argument("--max-size-bytes", type=int, default=64 * 1024 * 1024)

    summarize = sub.add_parser("summarize", help="write a sanitized Markdown summary for a JSON report")
    summarize.add_argument("--report", type=Path, required=True)
    summarize.add_argument("--markdown", type=Path, required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.command == "build":
        report = build_systemd(args)
    elif args.command == "generate":
        report = generate_corpus(args)
    elif args.command == "test":
        report = test_matrix(args)
    elif args.command == "smoke":
        report = smoke(args)
    elif args.command == "summarize":
        report = summarize_report(args)
    else:  # pragma: no cover - argparse enforces choices.
        parser.error(f"unsupported command: {args.command}")
    print(
        json.dumps(
            {
                "status": report.get("status"),
                "kind": report.get("kind"),
                "version": report.get("version"),
                "discrepancy_codes": [
                    item.get("code") for item in report.get("discrepancies", [])
                ],
                "observation_codes": [
                    item.get("code") for item in report.get("observations", [])
                ],
            },
            sort_keys=True,
        )
    )
    return 0 if report.get("status") == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(main())
