"""Result assessment and reporting helpers for the live matrix harness."""

from __future__ import annotations


def expected_live_sequences(entries: int) -> list[str]:
    return [f"{i:06d}" for i in range(entries)]


def active_observations(result: dict) -> list[dict]:
    return [obs for obs in result["active_polls"] if obs.get("entries_count", 0) > 0]


def reader_group(reader_name: str) -> str:
    base, sep, suffix = reader_name.rpartition("-")
    if sep and suffix.isdigit():
        return base
    return reader_name


def assess_writer_exit(result: dict) -> list[str]:
    if result["exit_code"] != 0:
        return [f"writer exit {result['exit_code']}"]
    return []


def assess_active_polls(result: dict, expected: list[str]) -> list[str]:
    errors = []
    active_with_entries = active_observations(result)
    if not active_with_entries:
        errors.append("no reader observed entries while writer was actively writing")
    expected_reader_groups = {reader_group(o["reader"]) for o in result["active_polls"]}
    observed_reader_groups = {reader_group(o["reader"]) for o in active_with_entries}
    missing_reader_groups = sorted(expected_reader_groups - observed_reader_groups)
    if missing_reader_groups:
        errors.append(f"active reader groups with no live entries: {', '.join(missing_reader_groups)}")
    for obs in active_with_entries:
        observed = obs.get("seq_observed", [])
        if observed != expected[:len(observed)]:
            errors.append(
                f"{obs['reader']}: active sequence is not an ordered prefix, "
                f"got {observed[:3]}... len={len(observed)}"
            )
    return errors


def assess_libsystemd_live(result: dict, entries: int) -> list[str]:
    errors = []
    if not result.get("libsystemd_live"):
        errors.append("no stock libsystemd live reader was run")
    for obs in result.get("libsystemd_live", []):
        if not obs.get("started_while_active"):
            errors.append(f"{obs['reader']}: did not start while writer was active")
        if obs.get("error"):
            errors.append(f"{obs['reader']}: {obs['error']}")
            continue
        if obs.get("entries_count", 0) != entries:
            errors.append(
                f"{obs['reader']}: expected {entries} live entries, got {obs.get('entries_count', 0)}"
            )
        if obs.get("waits", 0) <= 0:
            errors.append(f"{obs['reader']}: did not wait for appended entries")
    return errors


def assess_final_reads(result: dict, entries: int, expected: list[str]) -> list[str]:
    errors = []
    for obs in result["final_reads"]:
        if obs.get("error"):
            errors.append(f"{obs['reader']}: {obs['error']}")
            continue
        if obs.get("entries_count", 0) != entries:
            errors.append(
                f"{obs['reader']}: expected {entries} entries, got {obs.get('entries_count', 0)} "
                f"(seq={obs.get('seq_observed', [])[:3]}...)"
            )
        else:
            observed = obs.get("seq_observed", [])
            if observed != expected:
                errors.append(
                    f"{obs['reader']}: sequence mismatch, got {observed[:3]}... "
                    f"len={len(observed)}, expected len={entries}"
                )
    return errors


def assess_verify(result: dict) -> list[str]:
    errors = []
    if result.get("verify") and result["verify"].get("returncode") != 0:
        errors.append(f"verify failed: {result['verify'].get('stderr', '')}")
    if result.get("feature") == "sealed":
        verify_command = result.get("verify", {}).get("command", "")
        if not result.get("verify_key"):
            errors.append("sealed feature missing verify key")
        if "--verify-key" not in verify_command:
            errors.append("sealed feature was not verified with --verify-key")
    return errors


def assess_structure(result: dict) -> list[str]:
    if result.get("structure") and result["structure"].get("status") != "PASS":
        return [f"structure failed: {result['structure'].get('error', '')}"]
    return []


def assess(result: dict, entries: int) -> tuple[str, list[str]]:
    expected = expected_live_sequences(entries)
    errors = []
    errors.extend(assess_writer_exit(result))
    errors.extend(assess_active_polls(result, expected))
    errors.extend(assess_libsystemd_live(result, entries))
    errors.extend(assess_final_reads(result, entries, expected))
    errors.extend(assess_verify(result))
    errors.extend(assess_structure(result))

    return "PASS" if not errors else "FAIL", errors


def print_live_result(result: dict, entries: int) -> None:
    if result.get("error") and result.get("status") == "FAIL":
        return
    active_with_entries = active_observations(result)
    print(f"  exit={result['exit_code']}", flush=True)
    print(f"  active polls with entries: {len(active_with_entries)}/{len(result['active_polls'])}", flush=True)
    for obs in active_with_entries[:3]:
        print(f"    {obs['reader']}: {obs['entries_count']} entries, seq={obs['seq_observed'][:3]}...", flush=True)
    print_libsystemd_summary(result, entries)
    print_final_read_summary(result, entries)
    print_optional_checks(result)


def print_libsystemd_summary(result: dict, entries: int) -> None:
    complete = [
        obs for obs in result.get("libsystemd_live", [])
        if obs.get("entries_count", 0) == entries and not obs.get("error")
    ]
    print(
        f"  stock libsystemd live readers with all {entries} entries: "
        f"{len(complete)}/{len(result.get('libsystemd_live', []))}",
        flush=True,
    )


def print_final_read_summary(result: dict, entries: int) -> None:
    complete = [obs for obs in result["final_reads"] if obs.get("entries_count", 0) == entries]
    print(f"  final reads with all {entries} entries: {len(complete)}/{len(result['final_reads'])}", flush=True)


def print_optional_checks(result: dict) -> None:
    if result.get("verify"):
        print(f"  verify: rc={result['verify']['returncode']}", flush=True)
    if result.get("structure"):
        print(f"  structure: {result['structure'].get('status')}", flush=True)
    if result.get("status") == "FAIL":
        for error in result.get("errors", []):
            print(f"  FAIL: {error}", flush=True)
    else:
        print(f"  status: {result['status']}", flush=True)
