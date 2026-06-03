#!/usr/bin/env python3
"""Run live one-writer/multiple-reader journal compatibility checks."""

import argparse
import json
import os
import selectors
import subprocess  # nosec B404 - subprocess is required by harnesses.
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path


def parse_args():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--journal", required=True)
    parser.add_argument("--ready-file", required=True)
    parser.add_argument("--expected-entries", type=int, required=True)
    parser.add_argument("--match", default="PRIORITY=6")
    parser.add_argument("--sequence-field", default="LIVE_SEQ")
    parser.add_argument("--poll-journalctl-readers", type=int, default=2)
    parser.add_argument("--follow-journalctl-readers", type=int, default=1)
    parser.add_argument("--libsystemd-readers", type=int, default=1)
    parser.add_argument("--libsystemd-reader-bin")
    parser.add_argument("--reader-timeout-sec", type=float, default=20.0)
    parser.add_argument("--writer-timeout-sec", type=float, default=20.0)
    parser.add_argument("--allowed-writer-exit-code", type=int, default=0)
    parser.add_argument("--skip-verify", action="store_true")
    parser.add_argument("writer_cmd", nargs=argparse.REMAINDER)
    args = parser.parse_args()

    if args.writer_cmd and args.writer_cmd[0] == "--":
        args.writer_cmd = args.writer_cmd[1:]
    if not args.writer_cmd:
        parser.error("writer command is required after --")
    if args.expected_entries <= 0:
        parser.error("--expected-entries must be positive")
    if not args.sequence_field or "=" in args.sequence_field:
        parser.error("--sequence-field must be a journal field name")
    if args.libsystemd_readers > 0 and not args.libsystemd_reader_bin:
        parser.error("--libsystemd-reader-bin is required when libsystemd readers are enabled")

    return args


def systemd_version():
    try:
        # nosemgrep
        # subprocess is required by this harness; commands are shell=False vectors.
        result = subprocess.run(  # nosec B603 - harness uses shell=False command vectors.
            ["journalctl", "--version"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=5,
        )
    except Exception as e:
        return f"unknown: {e}"

    return result.stdout.splitlines()[0] if result.stdout else "unknown"


def terminate_process(proc):
    if proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=2)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=2)


def parse_json_lines(raw, source):
    rows = []
    for line in raw.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError as e:
            raise RuntimeError(f"{source}: invalid JSON line {line!r}: {e}") from e
        rows.append(row)
    return rows


def row_sequence(row, sequence_field, source):
    value = row.get(sequence_field)
    if value is None:
        keys = ",".join(sorted(row.keys()))
        raise RuntimeError(f"{source}: missing {sequence_field} field; keys={keys}")
    if not isinstance(value, str) or not value.isdigit():
        raise RuntimeError(f"{source}: invalid {sequence_field} value {value!r}")
    return int(value)


def validate_sequence_rows(rows, sequence_field, source):
    for index, row in enumerate(rows):
        sequence = row_sequence(row, sequence_field, source)
        if sequence != index:
            raise RuntimeError(
                f"{source}: out-of-order {sequence_field} at row {index}: "
                f"got {sequence}"
            )


def wait_for_ready(ready_file, journal, writer, timeout_sec):
    deadline = time.monotonic() + timeout_sec
    ready = Path(ready_file)
    journal_path = Path(journal)

    while time.monotonic() < deadline:
        if writer.poll() is not None:
            raise RuntimeError(f"writer exited before ready file was created: exit={writer.returncode}")
        if ready.exists() and journal_path.exists() and journal_path.stat().st_size > 0:
            return
        time.sleep(0.01)

    raise RuntimeError(f"writer did not create ready file within {timeout_sec}s: {ready_file}")


def journalctl_poll_reader(reader_id, args, stop_event, writer_done):
    max_count = 0
    runs = 0
    transient_retries = 0
    deadline = time.monotonic() + args.reader_timeout_sec
    command = [
        "journalctl",
        "--file",
        args.journal,
        "--output=json",
        "--quiet",
        "--no-pager",
        args.match,
    ]

    while time.monotonic() < deadline:
        active_at_start = not writer_done.is_set()
        # nosemgrep
        # subprocess is required by this harness; commands are shell=False vectors.
        result = subprocess.run(  # nosec B603
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=5,
        )
        runs += 1
        if result.returncode != 0:
            stderr = result.stderr.decode(errors="replace")
            if active_at_start and "No data available" in stderr:
                transient_retries += 1
                time.sleep(0.02)
                continue
            raise RuntimeError(
                f"journalctl poll reader {reader_id} failed with {result.returncode}: "
                f"{stderr}"
            )
        if result.stderr:
            raise RuntimeError(
                f"journalctl poll reader {reader_id} wrote stderr: "
                f"{result.stderr.decode(errors='replace')}"
            )

        rows = parse_json_lines(result.stdout.decode(errors="replace"), f"journalctl poll {reader_id}")
        try:
            validate_sequence_rows(rows, args.sequence_field, f"journalctl poll {reader_id}")
        except RuntimeError:
            if active_at_start:
                transient_retries += 1
                time.sleep(0.02)
                continue
            raise
        max_count = max(max_count, len(rows))
        if writer_done.is_set() and max_count >= args.expected_entries:
            return {
                "reader": "journalctl-poll",
                "id": reader_id,
                "runs": runs,
                "max_entries": max_count,
                "transient_retries": transient_retries,
            }
        if stop_event.is_set():
            break
        time.sleep(0.02)

    raise RuntimeError(
        f"journalctl poll reader {reader_id} observed {max_count} entries, "
        f"expected {args.expected_entries}"
    )


def journalctl_follow_reader(reader_id, args, stop_event, writer_done):
    command = journalctl_follow_command(args)
    deadline = time.monotonic() + args.reader_timeout_sec
    transient_retries = 0
    last_count = 0

    while time.monotonic() < deadline:
        active_at_start = not writer_done.is_set()
        attempt = run_journalctl_follow_attempt(
            command,
            reader_id,
            args,
            stop_event,
            writer_done,
            deadline,
            active_at_start,
        )

        if attempt["complete"]:
            return {
                "reader": "journalctl-follow",
                "id": reader_id,
                "entries": attempt["count"],
                "exit": attempt["exit"],
                "transient_retries": transient_retries,
            }

        last_count = max(last_count, attempt["count"])
        stderr_text = attempt["stderr"].decode(errors="replace")
        if should_retry_follow_attempt(active_at_start, attempt, stderr_text):
            transient_retries += 1
            time.sleep(0.02)
            continue
        if attempt["stderr"]:
            raise RuntimeError(
                f"journalctl follow reader {reader_id} wrote stderr: "
                f"{stderr_text}"
            )
        break

    raise RuntimeError(
        f"journalctl follow reader {reader_id} observed {last_count} entries, "
        f"expected {args.expected_entries}"
    )


def journalctl_follow_command(args):
    return [
        "journalctl",
        "--file",
        args.journal,
        "--follow",
        "--no-tail",
        # journalctl enables current-boot filtering by default in follow mode.
        # Synthetic journals use synthetic boot IDs, so clear that implicit filter.
        "--boot=all",
        "--output=json",
        "--quiet",
        "--no-pager",
        args.match,
    ]


def run_journalctl_follow_attempt(
    command,
    reader_id,
    args,
    stop_event,
    writer_done,
    deadline,
    active_at_start,
):
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    proc = subprocess.Popen(  # nosec B603 - harness uses shell=False command vectors.
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    selector = selectors.DefaultSelector()
    selector.register(proc.stdout, selectors.EVENT_READ, "stdout")
    selector.register(proc.stderr, selectors.EVENT_READ, "stderr")
    state = {"stdout": b"", "stderr": b"", "count": 0, "transient_error": False}

    try:
        while time.monotonic() < deadline:
            for key, _ in selector.select(timeout=0.1):
                process_follow_event(key, selector, state, reader_id, args, active_at_start)
            if follow_attempt_done(proc, selector, state, args, stop_event, writer_done):
                break
    finally:
        exit_code = proc.poll()
        terminate_process(proc)

    return {
        "count": state["count"],
        "stderr": state["stderr"],
        "transient_error": state["transient_error"],
        "complete": state["count"] >= args.expected_entries,
        "exit": exit_code,
    }


def process_follow_event(key, selector, state, reader_id, args, active_at_start):
    chunk = os.read(key.fileobj.fileno(), 4096)
    if not chunk:
        unregister_follow_stream(selector, key, state)
        return
    if key.data == "stderr":
        state["stderr"] += chunk
        return
    state["stdout"] += chunk
    consume_follow_stdout_lines(state, reader_id, args, active_at_start)


def unregister_follow_stream(selector, key, state):
    try:
        selector.unregister(key.fileobj)
    except Exception as unregister_error:
        state["stderr"] += f"\nunregister failed: {unregister_error}".encode()


def consume_follow_stdout_lines(state, reader_id, args, active_at_start):
    while b"\n" in state["stdout"]:
        line, state["stdout"] = state["stdout"].split(b"\n", 1)
        if not line.strip():
            continue
        try:
            validate_follow_line(line, reader_id, args, state["count"])
        except RuntimeError:
            if active_at_start:
                state["transient_error"] = True
                return
            raise
        state["count"] += 1


def validate_follow_line(line, reader_id, args, expected_sequence):
    source = f"journalctl follow {reader_id}"
    rows = parse_json_lines(line.decode(errors="replace"), source)
    if len(rows) != 1:
        raise RuntimeError(f"{source}: expected one JSON row")
    sequence = row_sequence(rows[0], args.sequence_field, source)
    if sequence != expected_sequence:
        raise RuntimeError(
            f"{source}: out-of-order {args.sequence_field}: "
            f"got {sequence}, expected {expected_sequence}"
        )


def follow_attempt_done(proc, selector, state, args, stop_event, writer_done):
    if state["transient_error"]:
        return True
    if state["count"] >= args.expected_entries:
        return True
    if proc.poll() is not None and len(selector.get_map()) == 0:
        return True
    return stop_event.is_set() and writer_done.is_set() and state["count"] >= args.expected_entries


def should_retry_follow_attempt(active_at_start, attempt, stderr_text):
    return active_at_start and (
        attempt["transient_error"] or ("No data available" in stderr_text and attempt["count"] == 0)
    )


def libsystemd_reader(reader_id, args, writer_done):
    command = [
        args.libsystemd_reader_bin,
        "--path",
        args.journal,
        "--match",
        args.match,
        "--sequence-field",
        args.sequence_field,
        "--expected",
        str(args.expected_entries),
        "--timeout-sec",
        str(args.reader_timeout_sec),
    ]
    deadline = time.monotonic() + args.reader_timeout_sec
    transient_retries = 0

    while time.monotonic() < deadline:
        active_at_start = not writer_done.is_set()
        remaining = max(1.0, deadline - time.monotonic())
        # nosemgrep
        # subprocess is required by this harness; commands are shell=False vectors.
        result = subprocess.run(  # nosec B603
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=remaining + 5,
        )
        if result.returncode != 0:
            if active_at_start and "No data available" in result.stderr:
                transient_retries += 1
                time.sleep(0.02)
                continue
            raise RuntimeError(
                f"libsystemd reader {reader_id} failed with {result.returncode}: {result.stderr}"
            )
        if result.stderr:
            raise RuntimeError(f"libsystemd reader {reader_id} wrote stderr: {result.stderr}")

        rows = parse_json_lines(result.stdout, f"libsystemd reader {reader_id}")
        entries = rows[0].get("entries", 0) if rows else 0
        return {
            "reader": "libsystemd",
            "id": reader_id,
            "entries": entries,
            "transient_retries": transient_retries,
            "evidence": rows,
        }

    raise RuntimeError(
        f"libsystemd reader {reader_id} did not open a readable journal within "
        f"{args.reader_timeout_sec}s"
    )


def verify_journal(path):
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    result = subprocess.run(  # nosec B603
        ["journalctl", "--verify", "--file", path],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=20,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"journalctl --verify failed with {result.returncode}:\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return {"stdout": result.stdout, "stderr": result.stderr}


def main():
    args = parse_args()
    stop_event = threading.Event()
    writer_done = threading.Event()
    readers = []
    summary = {
        "systemd_version": systemd_version(),
        "journal": args.journal,
        "expected_entries": args.expected_entries,
        "match": args.match,
        "sequence_field": args.sequence_field,
        "readers": [],
    }

    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    writer = subprocess.Popen(  # nosec B603 - harness uses shell=False command vectors.
        args.writer_cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    try:
        wait_for_ready(args.ready_file, args.journal, writer, args.writer_timeout_sec)
        with ThreadPoolExecutor(
            max_workers=args.poll_journalctl_readers +
            args.follow_journalctl_readers +
            args.libsystemd_readers
        ) as executor:
            for i in range(args.poll_journalctl_readers):
                readers.append(executor.submit(journalctl_poll_reader, i, args, stop_event, writer_done))
            for i in range(args.follow_journalctl_readers):
                readers.append(executor.submit(journalctl_follow_reader, i, args, stop_event, writer_done))
            for i in range(args.libsystemd_readers):
                readers.append(executor.submit(libsystemd_reader, i, args, writer_done))

            try:
                writer_stdout, writer_stderr = writer.communicate(timeout=args.writer_timeout_sec)
            except subprocess.TimeoutExpired as e:
                terminate_process(writer)
                raise RuntimeError(f"writer timed out after {args.writer_timeout_sec}s") from e
            finally:
                writer_done.set()

            summary["writer"] = {
                "exit": writer.returncode,
                "stdout": writer_stdout,
                "stderr": writer_stderr,
            }
            if writer.returncode != args.allowed_writer_exit_code:
                raise RuntimeError(
                    f"writer exited with {writer.returncode}, "
                    f"expected {args.allowed_writer_exit_code}; stderr={writer_stderr}"
                )

            for future in as_completed(readers, timeout=args.reader_timeout_sec + 10):
                summary["readers"].append(future.result())

        stop_event.set()
        if not args.skip_verify:
            summary["verify"] = verify_journal(args.journal)

        print(json.dumps(summary, sort_keys=True))
        return 0
    except Exception as e:
        stop_event.set()
        terminate_process(writer)
        print(json.dumps({"status": "ERROR", "error": str(e), "summary": summary}, sort_keys=True), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
