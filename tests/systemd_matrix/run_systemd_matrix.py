#!/usr/bin/env python3
"""Build systemd-version helpers and run sanitized reader compatibility checks."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess  # nosec B404 - subprocess is required by harnesses.
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
        # nosemgrep
        # subprocess is required by this harness; commands are shell=False vectors.
        proc = subprocess.run(  # nosec B603 - harness uses shell=False command vectors.
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


def parse_digest_stdout(
    stream: BinaryIO,
    digest_state: dict[str, Any],
    stdout_state: dict[str, Any],
) -> None:
    hashing_stdout = HashingReader(stream)
    try:
        digest_state["digest"] = digest_export_stream(hashing_stdout)
    except Exception as exc:  # pragma: no cover - exercised by bad helpers.
        digest_state["error_class"] = type(exc).__name__
        digest_state["error_sha256"] = sha256_bytes(str(exc).encode("utf-8"))
    finally:
        stdout_state["sha256"] = hashing_stdout.hexdigest()
        stdout_state["bytes"] = hashing_stdout.bytes


def drain_digest_stderr(stream: BinaryIO, stderr_state: dict[str, Any]) -> None:
    try:
        stderr_state.update(drain_digest(stream))
    except Exception as exc:  # pragma: no cover - defensive only.
        stderr_state["sha256"] = sha256_bytes(str(exc).encode("utf-8"))
        stderr_state["bytes"] = 0


def start_digest_threads(
    label: str,
    proc: subprocess.Popen[bytes],
    digest_state: dict[str, Any],
    stdout_state: dict[str, Any],
    stderr_state: dict[str, Any],
) -> tuple[threading.Thread, threading.Thread]:
    assert proc.stdout is not None
    assert proc.stderr is not None
    stdout_thread = threading.Thread(
        target=parse_digest_stdout,
        args=(proc.stdout, digest_state, stdout_state),
        name=f"{label}-stdout",
    )
    stderr_thread = threading.Thread(
        target=drain_digest_stderr,
        args=(proc.stderr, stderr_state),
        name=f"{label}-stderr",
    )
    stdout_thread.start()
    stderr_thread.start()
    return stdout_thread, stderr_thread


def wait_for_stdout_or_timeout(
    proc: subprocess.Popen[bytes],
    stdout_thread: threading.Thread,
    deadline: float,
) -> bool:
    timed_out = False
    while proc.poll() is None and stdout_thread.is_alive():
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            timed_out = True
            proc.kill()
            break
        stdout_thread.join(timeout=min(0.1, remaining))
    return timed_out


def wait_for_process_exit(
    proc: subprocess.Popen[bytes],
    deadline: float,
    timed_out: bool,
) -> tuple[int, bool]:
    try:
        remaining = max(0.0, deadline - time.monotonic())
        return proc.wait(timeout=remaining), timed_out
    except subprocess.TimeoutExpired:
        proc.kill()
        return proc.wait(), True


def join_digest_threads(
    stdout_thread: threading.Thread,
    stderr_thread: threading.Thread,
    returncode: int,
    timed_out: bool,
) -> tuple[int, bool]:
    stdout_thread.join(timeout=5)
    stderr_thread.join(timeout=5)
    if stdout_thread.is_alive() or stderr_thread.is_alive():
        return 124, True
    return returncode, timed_out


def wait_digest_process(
    proc: subprocess.Popen[bytes],
    stdout_thread: threading.Thread,
    stderr_thread: threading.Thread,
    digest_state: dict[str, Any],
    timeout: int,
) -> tuple[int, bool]:
    deadline = time.monotonic() + timeout
    timed_out = wait_for_stdout_or_timeout(proc, stdout_thread, deadline)
    if "error_class" in digest_state and proc.poll() is None:
        proc.kill()
    returncode, timed_out = wait_for_process_exit(proc, deadline, timed_out)
    return join_digest_threads(stdout_thread, stderr_thread, returncode, timed_out)


def export_digest_command_result(
    label: str,
    cmd: list[str],
    started: float,
    returncode: int,
    timed_out: bool,
    stdout_state: dict[str, Any],
    stderr_state: dict[str, Any],
    timeout: int,
) -> CommandResult:
    empty_sha = sha256_bytes(b"")
    return CommandResult(
        label=label,
        returncode=returncode if not timed_out else 124,
        elapsed_seconds=time.perf_counter() - started,
        stdout_sha256=str(stdout_state.get("sha256", empty_sha)),
        stdout_bytes=int(stdout_state.get("bytes", 0)),
        stderr_sha256=str(stderr_state.get("sha256", empty_sha)),
        stderr_bytes=int(stderr_state.get("bytes", 0)),
        command_sha256=command_sha(cmd),
        timeout_seconds=timeout,
        timed_out=timed_out,
    )


def stream_export_command_digest(
    label: str,
    cmd: list[str],
    *,
    env: dict[str, str],
    timeout: int,
) -> tuple[dict[str, Any] | None, CommandResult]:
    started = time.perf_counter()
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    proc = subprocess.Popen(  # nosec B603 - harness uses shell=False command vectors.
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
    stdout_thread, stderr_thread = start_digest_threads(label, proc, digest_state, stdout_state, stderr_state)
    returncode, timed_out = wait_digest_process(proc, stdout_thread, stderr_thread, digest_state, timeout)
    command_result = export_digest_command_result(
        label,
        cmd,
        started,
        returncode,
        timed_out,
        stdout_state,
        stderr_state,
        timeout,
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
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    proc = subprocess.run(cmd, cwd=str(ROOT), text=True, capture_output=True, check=False)  # nosec B603 - harness uses shell=False command vectors.
    if proc.returncode != 0:
        raise RuntimeError(f"could not resolve systemd ref {ref!r} from {systemd_src}")
    return proc.stdout.strip()


def is_git_checkout(path: Path) -> bool:
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    proc = subprocess.run(  # nosec B603
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


def meson_helper_insertion(text: str) -> str:
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
        return text.replace(marker, marker + entry)

    simple_marker = "        'sd-journal/test-journal-file.c',\n"
    simple_entry = f"        'sd-journal/{SYSTEMD_HELPER_SOURCE_NAME}',\n"
    if simple_marker in text:
        return text.replace(simple_marker, simple_marker + simple_entry)

    legacy_marker = """        [files('sd-journal/test-format-change-ingester.c'),
         [], [], [], '', 'manual'],
"""
    legacy_entry = f"""        [files('sd-journal/{SYSTEMD_HELPER_SOURCE_NAME}'),
         [], [], [], '', 'manual'],
"""
    if legacy_marker in text:
        return text.replace(legacy_marker, legacy_marker + legacy_entry)
    raise RuntimeError("could not find systemd meson journal test marker")


def patch_meson_helper(source_dir: Path) -> None:
    meson_file = source_dir / "src" / "libsystemd" / "meson.build"
    text = meson_file.read_text(encoding="utf-8")
    if SYSTEMD_HELPER_SOURCE_NAME in text:
        return
    meson_file.write_text(meson_helper_insertion(text), encoding="utf-8")


def fss_root_replacements() -> tuple[tuple[str, str], ...]:
    new_path = """        const char *fss_root = getenv("SYSTEMD_JOURNAL_FSS_ROOT");
        if (!fss_root || !*fss_root)
                fss_root = "/var/log/journal";

        if (asprintf(&path, "%s/" SD_ID128_FORMAT_STR "/fss",
                     fss_root, SD_ID128_FORMAT_VAL(machine)) < 0)
                return -ENOMEM;
"""
    old_path = """        if (asprintf(&path, "/var/log/journal/" SD_ID128_FORMAT_STR "/fss",
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
    old_legacy = """        if (asprintf(&p, "/var/log/journal/" SD_ID128_FORMAT_STR "/fss",
                     SD_ID128_FORMAT_VAL(machine)) < 0)
                return -ENOMEM;
"""
    return ((old_path, new_path), (old_legacy, new_legacy))


def patch_authenticate_fss_root(source_dir: Path) -> None:
    authenticate_file = source_dir / "src" / "libsystemd" / "sd-journal" / "journal-authenticate.c"
    text = authenticate_file.read_text(encoding="utf-8")
    if "SYSTEMD_JOURNAL_FSS_ROOT" in text:
        return

    if "#include <stdlib.h>" not in text:
        text = text.replace("#include <unistd.h>\n", "#include <stdlib.h>\n#include <unistd.h>\n")
    for old, new in fss_root_replacements():
        if old in text:
            authenticate_file.write_text(text.replace(old, new), encoding="utf-8")
            return
    raise RuntimeError("could not find systemd journal FSS path marker")


def patch_filesystems_gperf(source_dir: Path) -> None:
    filesystems_file = source_dir / "src" / "basic" / "filesystems-gperf.gperf"
    if not filesystems_file.exists():
        return
    text = filesystems_file.read_text(encoding="utf-8")
    additions = {
        "bcachefs,": "bcachefs,        {BCACHEFS_SUPER_MAGIC}\n",
        "guest_memfd,": "guest_memfd,     {GUEST_MEMFD_MAGIC}\n",
        "pidfs,": "pidfs,           {PID_FS_MAGIC}\n",
    }
    missing = [line for marker, line in additions.items() if marker not in text]
    if missing:
        filesystems_file.write_text(text.rstrip() + "\n" + "".join(missing), encoding="utf-8")


def patch_systemd_helper(source_dir: Path) -> None:
    helper_dest = source_dir / "src" / "libsystemd" / "sd-journal" / SYSTEMD_HELPER_SOURCE_NAME
    shutil.copyfile(SYSTEMD_HELPER_SOURCE, helper_dest)
    patch_meson_helper(source_dir)
    patch_authenticate_fss_root(source_dir)
    patch_filesystems_gperf(source_dir)


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


def prepare_generation_paths(args: argparse.Namespace, out: Path) -> tuple[Path, Path]:
    journal_path = args.journal or generated_journal_path(out, args.version, args.case)
    journal_path = require_under(journal_path, out, "--journal output")
    key_path = verification_key_path(out, args.version, args.case)
    journal_path.parent.mkdir(parents=True, exist_ok=True)
    journal_path.unlink(missing_ok=True)
    key_path.unlink(missing_ok=True)
    return journal_path, key_path


def corpus_generator_path(build: dict[str, Any] | None) -> Path | None:
    artifacts = build.get("artifacts", {}) if build else {}
    generator_rel = artifacts.get("generator")
    return ROOT / generator_rel if generator_rel else None


def generation_command(args: argparse.Namespace, out: Path, generator: Path, journal_path: Path) -> list[str]:
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
    return cmd


def run_generation_command(
    args: argparse.Namespace,
    out: Path,
    generator: Path | None,
    journal_path: Path,
    key_path: Path,
) -> tuple[str, list[dict[str, Any]], dict[str, Any] | None, dict[str, Any] | None]:
    if not generator or not generator.exists():
        return "failed", [{"code": "GENERATE_FAILED", "reason": "missing-generator"}], None, None

    payload, result = run_json_line(
        "generate deterministic systemd corpus",
        generation_command(args, out, generator, journal_path),
        env=matrix_env(out),
        timeout=args.timeout,
    )
    if result.returncode != 0 or payload is None:
        return "failed", [{"code": "GENERATE_FAILED", "command_sha256": result.command_sha256}], payload, result.as_dict()

    payload, _ = sanitize_generator_payload(payload, key_path)
    return "ok", [], payload, result.as_dict()


def generated_journal_metadata(
    args: argparse.Namespace,
    journal_path: Path,
    status: str,
) -> tuple[dict[str, Any] | None, str, list[dict[str, Any]]]:
    if status == "ok" and journal_path.exists():
        stat = journal_path.stat()
        return {
            "artifact": relative(journal_path),
            "size_bytes": stat.st_size,
            "sha256": sha256_file(journal_path),
            "producer": "systemd-matrix-ingester",
            "final_state": args.final_state,
            "compact": args.compact,
            "sealed": args.sealed,
        }, status, []
    if status == "ok":
        return None, "failed", [{"code": "GENERATE_FAILED", "reason": "missing-output"}]
    return None, status, []


def generation_report(
    args: argparse.Namespace,
    build: dict[str, Any] | None,
    journal: dict[str, Any] | None,
    key_path: Path,
    payload: dict[str, Any] | None,
    command: dict[str, Any] | None,
    status: str,
    discrepancies: list[dict[str, Any]],
) -> dict[str, Any]:
    return {
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


def generate_corpus(args: argparse.Namespace) -> dict[str, Any]:
    out = args.out.resolve()
    build = build_systemd(args) if args.sealed else ensure_build(args)
    slug = version_slug(args.version)
    case = version_slug(args.case)
    reports_dir = out / "reports"
    journal_path, key_path = prepare_generation_paths(args, out)
    status, discrepancies, payload, command = run_generation_command(
        args,
        out,
        corpus_generator_path(build),
        journal_path,
        key_path,
    )
    journal, status, journal_discrepancies = generated_journal_metadata(args, journal_path, status)
    discrepancies.extend(journal_discrepancies)
    report = generation_report(args, build, journal, key_path, payload, command, status, discrepancies)
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
    baseline = select_reader_baseline(readers)
    discrepancies: list[dict[str, Any]] = []
    observations: list[dict[str, Any]] = []
    if baseline is None:
        return None, discrepancies, observations
    for row in readers:
        if row is baseline:
            continue
        row_discrepancies, row_observations = compare_reader_row(row, baseline)
        discrepancies.extend(row_discrepancies)
        observations.extend(row_observations)
    return baseline, discrepancies, observations


def select_reader_baseline(readers: list[dict[str, Any]]) -> dict[str, Any] | None:
    for preferred in ("stock_journalctl_read", "version_journalctl_read"):
        baseline = next((row for row in readers if row.get("role") == preferred), None)
        if baseline is not None:
            return baseline
    return readers[0] if readers else None


def is_version_metadata_drift(row: dict[str, Any], baseline: dict[str, Any]) -> bool:
    return (
        row.get("role") == "version_journalctl_read"
        and baseline.get("role") == "stock_journalctl_read"
        and row.get("counts") == baseline.get("counts")
    )


def digest_mismatch(row: dict[str, Any], baseline: dict[str, Any]) -> dict[str, Any]:
    return {
        "code": "DIGEST_MISMATCH",
        "baseline": baseline.get("role"),
        "reader": row.get("role"),
        "baseline_digest": baseline.get("logical_digest"),
        "reader_digest": row.get("logical_digest"),
    }


def metadata_drift_observation(row: dict[str, Any], baseline: dict[str, Any]) -> dict[str, Any]:
    observation = digest_mismatch(row, baseline)
    observation["code"] = "VERSION_EXPORT_METADATA_DRIFT"
    return observation


def count_mismatch(row: dict[str, Any], baseline: dict[str, Any]) -> dict[str, Any]:
    return {
        "code": "COUNT_MISMATCH",
        "baseline": baseline.get("role"),
        "reader": row.get("role"),
    }


def compare_reader_row(
    row: dict[str, Any],
    baseline: dict[str, Any],
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    discrepancies: list[dict[str, Any]] = []
    observations: list[dict[str, Any]] = []
    if row.get("logical_digest") != baseline.get("logical_digest"):
        if is_version_metadata_drift(row, baseline):
            observations.append(metadata_drift_observation(row, baseline))
        else:
            discrepancies.append(digest_mismatch(row, baseline))
    if row.get("counts") != baseline.get("counts"):
        discrepancies.append(count_mismatch(row, baseline))
    return discrepancies, observations

REQUIRED_MATRIX_ROLES = {
    "stock_journalctl_verify",
    "stock_journalctl_read",
    "rust_sdk_read",
    "go_sdk_read",
    "python_sdk_read",
    "node_sdk_read",
}

ROLE_FAILURE_CODES = {
    "version_journalctl_verify": "VERSION_VERIFY_FAILED",
    "stock_journalctl_verify": "STOCK_VERIFY_FAILED",
    "version_journalctl_read": "VERSION_READ_FAILED",
    "stock_journalctl_read": "STOCK_READ_FAILED",
    "rust_sdk_read": "RUST_READ_FAILED",
    "go_sdk_read": "GO_READ_FAILED",
    "python_sdk_read": "PYTHON_READ_FAILED",
    "node_sdk_read": "NODE_READ_FAILED",
}


def matrix_journal_path(args: argparse.Namespace, out: Path) -> Path:
    journal_path = args.journal or generated_journal_path(out, args.version, args.case)
    journal_path = require_under(journal_path, out, "--journal input")
    if not journal_path.exists():
        raise SystemExit(f"journal input does not exist: {journal_path}")
    return journal_path


def read_verification_key(args: argparse.Namespace, out: Path) -> tuple[str | None, dict[str, Any] | None]:
    key_path = getattr(args, "verify_key_file", None) or verification_key_path(out, args.version, args.case)
    if not key_path.exists():
        return None, None
    return key_path.read_text(encoding="utf-8").strip(), {
        "present": True,
        "artifact": relative(key_path),
        "sha256": sha256_file(key_path),
    }


def version_journalctl_path(args: argparse.Namespace, build: dict[str, Any] | None) -> Path | None:
    explicit_version_journalctl = getattr(args, "version_journalctl", None)
    if explicit_version_journalctl:
        return explicit_version_journalctl.resolve()
    artifacts = build.get("artifacts", {}) if build else {}
    return ROOT / artifacts["journalctl"] if artifacts.get("journalctl") else None


def matrix_tools(sdk_build: dict[str, Any], out: Path) -> dict[str, Any]:
    return {
        "sdk_build": {
            "status": sdk_build.get("status"),
            "report": relative(out / "reports" / "sdk-tools.json"),
        }
    }


def add_version_journalctl_results(
    args: argparse.Namespace,
    env: dict[str, str],
    tools: dict[str, Any],
    results: list[dict[str, Any]],
    discrepancies: list[dict[str, Any]],
    version_journalctl: Path | None,
    journal_path: Path,
    verification_key: str | None,
) -> None:
    if not version_journalctl or not version_journalctl.exists():
        discrepancies.append({"code": "VERSION_JOURNALCTL_UNAVAILABLE"})
        tools["version_journalctl"] = {"available": False}
        return
    tools["version_journalctl"] = journalctl_version(version_journalctl, env, args.timeout)
    results.append(verify_with_journalctl("version_journalctl_verify", version_journalctl, journal_path, env=env, timeout=args.timeout, verification_key=verification_key))
    results.append(read_with_journalctl("version_journalctl_read", version_journalctl, journal_path, env=env, timeout=args.timeout))


def add_stock_journalctl_results(
    args: argparse.Namespace,
    env: dict[str, str],
    tools: dict[str, Any],
    results: list[dict[str, Any]],
    discrepancies: list[dict[str, Any]],
    journal_path: Path,
    verification_key: str | None,
) -> None:
    stock_journalctl = shutil.which("journalctl")
    if not stock_journalctl:
        discrepancies.append({"code": "MISSING_TOOL", "tool": "journalctl"})
        tools["stock_journalctl"] = {"available": False}
        return
    tools["stock_journalctl"] = journalctl_version(stock_journalctl, env, args.timeout)
    results.append(verify_with_journalctl("stock_journalctl_verify", stock_journalctl, journal_path, env=env, timeout=args.timeout, verification_key=verification_key))
    results.append(read_with_journalctl("stock_journalctl_read", stock_journalctl, journal_path, env=env, timeout=args.timeout))


def add_compiled_sdk_readers(
    args: argparse.Namespace,
    env: dict[str, str],
    results: list[dict[str, Any]],
    discrepancies: list[dict[str, Any]],
    journal_path: Path,
    rust_digest: Path,
    go_digest: Path,
) -> None:
    if rust_digest.exists():
        results.append(read_with_sdk("rust_sdk_read", rust_digest, journal_path, env=env, timeout=args.timeout))
    else:
        discrepancies.append({"code": "BUILD_FAILED", "tool": "rust-corpus-digest"})
    if go_digest.exists():
        results.append(read_with_sdk("go_sdk_read", go_digest, journal_path, env=env, timeout=args.timeout))
    else:
        discrepancies.append({"code": "BUILD_FAILED", "tool": "go-corpus-digest"})


def add_python_reader(
    args: argparse.Namespace,
    env: dict[str, str],
    results: list[dict[str, Any]],
    journal_path: Path,
) -> None:
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


def add_node_reader(
    args: argparse.Namespace,
    env: dict[str, str],
    results: list[dict[str, Any]],
    discrepancies: list[dict[str, Any]],
    journal_path: Path,
) -> None:
    node = shutil.which("node")
    if not node:
        discrepancies.append({"code": "MISSING_TOOL", "tool": "node"})
        return
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


def append_failed_result_discrepancies(results: list[dict[str, Any]], discrepancies: list[dict[str, Any]]) -> None:
    for row in results:
        if row.get("status") == "ok":
            continue
        code = ROLE_FAILURE_CODES.get(str(row.get("role")))
        if code:
            discrepancies.append({"code": code, "role": row.get("role")})


def present_required_roles(results: list[dict[str, Any]]) -> set[str]:
    return {
        str(row.get("role"))
        for row in results
        if row.get("role") in REQUIRED_MATRIX_ROLES and row.get("status") == "ok"
    }


def matrix_report(
    args: argparse.Namespace,
    build: dict[str, Any] | None,
    journal_path: Path,
    tools: dict[str, Any],
    verification_key_info: dict[str, Any] | None,
    results: list[dict[str, Any]],
    observations: list[dict[str, Any]],
    discrepancies: list[dict[str, Any]],
    status: str,
    baseline: dict[str, Any] | None,
) -> dict[str, Any]:
    stat = journal_path.stat()
    return {
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


def test_matrix(args: argparse.Namespace) -> dict[str, Any]:
    out = args.out.resolve()
    env = matrix_env(out)
    build = None if getattr(args, "version_journalctl", None) else ensure_build(args)
    rust_digest, go_digest, sdk_build = sdk_tool_paths(out, args.timeout)
    journal_path = matrix_journal_path(args, out)
    verification_key, verification_key_info = read_verification_key(args, out)
    tools = matrix_tools(sdk_build, out)
    results: list[dict[str, Any]] = []
    discrepancies: list[dict[str, Any]] = []

    add_version_journalctl_results(args, env, tools, results, discrepancies, version_journalctl_path(args, build), journal_path, verification_key)
    add_stock_journalctl_results(args, env, tools, results, discrepancies, journal_path, verification_key)
    add_compiled_sdk_readers(args, env, results, discrepancies, journal_path, rust_digest, go_digest)
    add_python_reader(args, env, results, journal_path)
    add_node_reader(args, env, results, discrepancies, journal_path)
    append_failed_result_discrepancies(results, discrepancies)
    baseline, compare_discrepancies, observations = compare_readers(results)
    discrepancies.extend(compare_discrepancies)
    status = "ok" if not discrepancies and present_required_roles(results) == REQUIRED_MATRIX_ROLES else "failed"
    report = matrix_report(args, build, journal_path, tools, verification_key_info, results, observations, discrepancies, status, baseline)
    slug = version_slug(args.version)
    case = version_slug(args.case)
    reports_dir = out / "reports"
    write_json(reports_dir / f"matrix-{slug}-{case}.json", report)
    write_markdown_report(reports_dir / f"matrix-{slug}-{case}.md", report)
    return report


def append_markdown_header(lines: list[str], report: dict[str, Any]) -> None:
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


def append_markdown_discrepancies(lines: list[str], report: dict[str, Any]) -> None:
    discrepancies = report.get("discrepancies") or []
    lines.append("## Discrepancies")
    if discrepancies:
        for item in discrepancies:
            code = item.get("code", "UNKNOWN")
            lines.append(f"- `{code}`: {DISCREPANCY_CODES.get(code, 'see JSON report')}")
    else:
        lines.append("- `OK`: no discrepancy detected")


def append_markdown_observations(lines: list[str], report: dict[str, Any]) -> None:
    observations = report.get("observations") or []
    if observations:
        lines.append("")
        lines.append("## Observations")
        for item in observations:
            code = item.get("code", "UNKNOWN")
            lines.append(f"- `{code}`: {DISCREPANCY_CODES.get(code, 'see JSON report')}")


def markdown_result_row(row: dict[str, Any]) -> str:
    counts = row.get("counts") if isinstance(row.get("counts"), dict) else {}
    digest = str(row.get("logical_digest") or "")
    digest_prefix = f"`{digest[:16]}`" if digest else ""
    return (
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


def append_markdown_results(lines: list[str], report: dict[str, Any]) -> None:
    results = report.get("results")
    if not isinstance(results, list):
        return
    lines.append("")
    lines.append("## Results")
    lines.append("")
    lines.append("| Role | Kind | Status | Entries | Payloads | Digest |")
    lines.append("| --- | --- | --- | ---: | ---: | --- |")
    for row in results:
        lines.append(markdown_result_row(row))


def write_markdown_report(path: Path, report: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines: list[str] = []
    append_markdown_header(lines, report)
    append_markdown_discrepancies(lines, report)
    append_markdown_observations(lines, report)
    append_markdown_results(lines, report)
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
