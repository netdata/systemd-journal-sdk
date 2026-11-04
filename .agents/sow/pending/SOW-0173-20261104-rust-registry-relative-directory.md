# SOW-0173 - Rust Registry Skips Files Under A Relative Directory

## Status

Status: open

Sub-state: tracked 2026-11-04 from the SOW-0172 same-failure scan; parked for
a user decision (user-visible behavior); not scheduled.

## Requirements

### Purpose

Make `journal_registry::Registry::watch_directory` either work with a
relative directory or refuse it, instead of silently finding no files.

### User Request

None yet. Found by the SOW-0172 same-failure scan; recorded under the
followup rule so it does not disappear.

### Assistant Understanding

Facts:

- `Registry::watch_directory(path)` calls `scan_journal_files(path)`
  (`rust/src/crates/journal-registry/src/registry/mod.rs`,
  `repository/file.rs`). The walk yields relative paths for a relative
  directory, and `File::from_path` returns `None` for them, so every file is
  skipped and the call returns `Ok` with zero files.
- Notify events for that watch carry the same relative paths, and the
  event handlers in `registry/mod.rs` skip them the same way.
- The registry is Rust-only (there is no Go equivalent), so this is not a
  Go/Rust parity gap. In-repo callers (`journal-engine/examples/index.rs`)
  pass absolute paths.

Inferences:

- External callers such as Netdata pass absolute journal directories, so it
  is likely unhit; the failure is silent, which makes it costly when hit.

### Acceptance Criteria

- To be written at activation, after the decision below.

## Analysis

Options (for the user):

1. A: `watch_directory` and `unwatch_directory` make the path absolute once
   (`std::path::absolute`), like the SOW-0172 writer fix. Long-term-best.
2. B: refuse a relative directory with an error. Surgical; callers learn
   immediately.
3. C: leave it and document "absolute paths only".

Recommendation: A, for consistency with SOW-0172.

Risks:

- A: `watched_directories` keys become absolute, so `unwatch_directory`
  must resolve the same way to match.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- See Assistant Understanding.

Evidence reviewed:

- The code paths above.

Affected contracts and surfaces:

- Rust `journal-registry` `Registry` only.

Existing patterns to reuse:

- SOW-0172 (`Log::new_inner` resolves with `std::path::absolute`).

Risk and blast radius:

- Registry directory watching only.

Sensitive data handling plan:

- Synthetic directories only.

Implementation plan:

1. To be written after the decision.

Validation plan:

- Registry test with a relative directory (scan and notify event).

Artifact impact plan:

- AGENTS.md: unaffected.
- Runtime project skills: unaffected.
- Specs: product scope if the registry contract is described there.
- End-user/operator docs: registry rustdoc.
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

### 2026-11-04

- Created from the SOW-0172 same-failure scan.

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
