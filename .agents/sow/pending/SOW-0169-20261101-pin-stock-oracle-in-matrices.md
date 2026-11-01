# SOW-0169 - Pin The Stock journalctl Oracle In Interoperability Matrices

## Status

Status: open

Sub-state: new, 2026-11-01; raised by a PR #13 review thread; not activated.

## Requirements

### Purpose

Compatibility gates must prove agreement with the systemd release the project
targets, not with whatever `journalctl` the host happens to have.

### User Request

Tracked from a PR #13 review finding: the verifier gate runs the bare host
`journalctl`, so it can certify against a different systemd version than the
pinned oracle. No user request yet; activation needs user scheduling.

### Assistant Understanding

Facts:

- `tests/interoperability/run_verify_matrix.py` runs `["journalctl", ...]` from
  `PATH` for the stock reader (`systemd_version()` and `verify_command()`), and
  records only that binary's `--version` line in its report.
- The same bare invocation appears in `run_matrix.py`, `run_binary_matrix.py`,
  `run_live_matrix.py`, `run_compression_matrix.py`, `run_compact_matrix.py`,
  `run_lock_matrix.py`, `run_directory_matrix.py` and
  `run_mixed_directory_matrix.py` (nine scripts in total, counting
  `run_verify_matrix.py`).
- `run_reader_hardening.py` already requires `--journalctl PATH`, and
  `run_journalctl_query_matrix.py` accepts one and passes it to
  `journalctl_v262_cases.py` as `tools["stock_journalctl"]`; these are the
  patterns to reuse. `run_journalctl_portable_smoke.py` runs only the Go and
  Rust binaries (its `"journalctl"` strings are expected command names).
- The parser-parity helpers (`tests/parser-parity/`) compare against pinned
  systemd source, not a stock binary, so they are out of scope.

Inferences:

- A report from a host with a different systemd version is still useful, but it
  is not evidence for the pinned target.

Unknowns:

- Whether every matrix should require the flag, or default to `PATH` and mark
  the report as unpinned. This is decision 1 at activation.

### Acceptance Criteria

- Every stock-invoking matrix runs the selected oracle for all stock calls,
  including version reporting.
- Reports record the oracle path and version.
- The compat skill's test-tool table states how to pass the oracle.

## Analysis

Sources checked:

- The scripts listed above (`rg '\["journalctl"' tests/interoperability`,
  with each hit checked for whether it launches a stock binary).

Current state:

- Bare host `journalctl` in nine scripts; explicit oracle in two plus the
  shared v262 case module.

Risks:

- Low: test tooling only. Making the flag required changes how the matrices
  are invoked locally and in any CI job that calls them.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- The matrices predate the pinned-oracle rule and resolve `journalctl` from
  `PATH`.

Evidence reviewed:

- See Assistant Understanding.

Affected contracts and surfaces:

- Interoperability test scripts and their reports; the
  `project-journal-compatibility` skill test-tool table. No SDK or CLI code.

Existing patterns to reuse:

- `run_reader_hardening.py` `--journalctl` argument.

Risk and blast radius:

- Test tooling only.

Sensitive data handling plan:

- Reports record binary paths under the repository; no sensitive data.

Implementation plan:

1. Add a shared oracle argument and route every stock call through it.
2. Update the skill table.

Validation plan:

- Run each changed matrix against the v262 oracle build.

Artifact impact plan:

- AGENTS.md: unaffected.
- Runtime project skills: `project-journal-compatibility` test-tool table.
- Specs: unaffected.
- End-user/operator docs: unaffected.
- End-user/operator skills: none exist.
- SOW lifecycle: single SOW.
- SOW-status.md: updated on activation and close.

Open-source reference evidence:

- None needed; this concerns repository test tooling.

Open decisions:

1. Required flag versus `PATH` default with an "unpinned" report marker.

## Implications And Decisions

Pending activation.

## Plan

Pending activation.

## Implementation And Review Plan

Implementation:

- Project manager, at activation.

Reviewers:

- Whole-SOW self-review after local validation; tests are the gate. External
  reviewers are not used.

## Execution Log

### 2026-11-01

- Created from a PR #13 review thread.

## Validation

Pending implementation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
