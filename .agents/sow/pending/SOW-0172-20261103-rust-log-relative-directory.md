# SOW-0172 - Rust Log Fails Late On A Relative Directory

## Status

Status: open

Sub-state: tracked 2026-11-03 from SOW-0156 real-use validation; parked for a
user decision (user-visible behavior); not scheduled.

## Requirements

### Purpose

Make the Rust and Go directory writers treat a relative directory the same
way, and never fail after creating directories.

### User Request

None yet. Found while validating SOW-0156 chunk 2; recorded under the
followup rule so it does not disappear.

### Assistant Understanding

Facts:

- Go `NewLog(".local/x", ...)` with a relative directory writes and rotates
  normally (SOW-0156 fill run, 22 files).
- Rust `Log::new` with the same relative directory succeeds and creates the
  machine directory, then the first `write_entry_with_timestamps` fails with
  `WriterError::FileCreation("failed to create journal file in <dir>")`.
- Cause: `journal-registry` `repository::File::from_path` returns `None` for
  any non-absolute path (`rust/src/crates/journal-registry/src/repository/file.rs`
  `from_path`), and `journal-log-writer` `chain.rs`
  `create_strict_systemd_active_file` maps that to the generic error.
- Not documented in `docs/` or the READMEs.

Inferences:

- Every caller so far passes absolute paths, so nobody has hit it; it is a
  parity gap and a confusing error, not data loss.

### Acceptance Criteria

- To be written at activation, after the decision below.

## Analysis

Options (for the user):

1. A: Rust `Log::new` makes the directory absolute once (current directory
   join, no symlink resolution), so relative paths behave as in Go.
   Long-term-best: parity, no new error surface.
2. B: Rust `Log::new` refuses a relative directory up front with a clear
   error, before creating anything; Go keeps accepting. Surgical, but leaves a
   documented parity exception.
3. C: Both languages refuse relative directories. Breaks existing Go callers.

Recommendation: A.

Risks:

- A: a later `chdir` in the caller no longer changes where files go (the same
  as Go today, which resolves at open time through the OS).

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- See Assistant Understanding.

Evidence reviewed:

- The code paths above and the SOW-0156 fill run logs (`.local/`, not kept).

Affected contracts and surfaces:

- Rust `journal-log-writer` `Log::new` (and the root-retention constructor);
  Go unchanged under A and B.

Existing patterns to reuse:

- None needed beyond `std::path::absolute`.

Risk and blast radius:

- Directory writer construction only.

Sensitive data handling plan:

- Synthetic directories only.

Implementation plan:

1. To be written after the decision.

Validation plan:

- Paired Go/Rust tests with a relative directory.

Artifact impact plan:

- AGENTS.md: unaffected.
- Runtime project skills: unaffected.
- Specs: directory writer contract in product scope.
- End-user/operator docs: `docs/Writer-APIs.md` if behavior is documented.
- End-user/operator skills: none exist.
- SOW lifecycle: standard.
- SOW-status.md: updated on activation and close.

Open-source reference evidence:

- None.

Open decisions:

1. A, B or C above.

## Implications And Decisions

None yet.

## Plan

Pending the decision.

## Implementation And Review Plan

Implementation:

- Project manager, at activation.

Reviewers:

- Whole-SOW self-review after local validation; tests are the gate. External
  reviewers are not used.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- Do not make changes outside this repository for any reason.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- Validation or audit failures are recorded in the Execution Log with
  evidence; the SOW does not advance until they are fixed or a user decision
  is recorded.

## Execution Log

### 2026-11-03

- Created from SOW-0156 real-use validation.

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
