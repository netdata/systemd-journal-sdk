"""Shared runtime helpers for the systemd-version matrix harness."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess  # nosec B404 - subprocess is required by harnesses.
import threading
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, BinaryIO

from tests.corpus_eval.canonical import digest_export_stream


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUT = ROOT / ".local" / "systemd-matrix"
DEFAULT_TIMEOUT = 1800


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


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


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
        return None, json_parse_error_result(result)
    try:
        parsed = json.loads(lines[0])
    except json.JSONDecodeError:
        return None, json_parse_error_result(result)
    if not isinstance(parsed, dict):
        return None, result
    return parsed, result


def json_parse_error_result(result: CommandResult) -> CommandResult:
    return CommandResult(
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
