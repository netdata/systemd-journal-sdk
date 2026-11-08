# SOW-0174 - Rust Registry Unwatch Leaves Files When The Directory Key Differs

## Status

Status: open

Sub-state: tracked 2026-11-05 from the SOW-0173 same-failure scan; parked for
a user decision (user-visible behavior); not scheduled.

## Requirements

### Purpose

Make `journal_registry::Registry::unwatch_directory` remove the files its
matching `watch_directory` registered, whatever form the directory string
had.

### User Request

None yet. Found while validating SOW-0173; recorded under the followup rule.

### Assistant Understanding

Facts:

- `unwatch_directory` removes files with `remove_directory(path)`
  (`rust/src/crates/journal-registry/src/registry/mod.rs:69-73`,
  `repository/collection.rs:254`), which drops the repository entry whose
  key equals the given string exactly.
- Repository directory keys come from `File::dir()`
  (`repository/file.rs:437`): the file's parent, or the grandparent when the
  parent is a machine-id directory. That key never has a trailing slash and
  never ends in the machine-id component.
- So the watch string matches the key only for the standard
  `<root>` layout without a trailing slash. Probe on 2026-11-05 (temporary
  test, removed): watching `<tmp>/` (trailing slash), or `<tmp>/<machine-id>`
  directly, registers 1 file, and `unwatch_directory` with the same string
  returns `Ok` and leaves that 1 file registered. Its notify watch is gone,
  so the stale entry is never updated again.
- The behavior predates SOW-0173: `std::path::absolute` returns absolute
  input unchanged, including a trailing slash.
- In-repo callers never call `unwatch_directory` outside tests. The registry
  is Rust-only, so there is no Go parity gap.

Inferences:

- Netdata watches root journal directories without a trailing slash, so it is
  likely unhit; the failure is silent.

### Acceptance Criteria

- To be written at activation, after the decision below.

## Analysis

Options (for the user):

1. A: on unwatch, remove every registered file whose path lies under the
   watched directory (path-prefix by components), instead of an exact key.
   Long-term-best: correct for any layout the watch accepted.
2. B: normalize the watch string (drop trailing separators) and keep the
   exact-key removal. Surgical; fixes the trailing-slash case only, not a
   machine-id directory watched directly.
3. C: document that `watch_directory` takes a journal root without a
   trailing slash.

Recommendation: A.

Risks:

- A: overlapping watches (a root and one of its machine-id subdirectories)
  must not remove files the other watch still covers; the plan must define
  that case.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- See Assistant Understanding.

Evidence reviewed:

- The code paths above and the 2026-11-05 probe.

Affected contracts and surfaces:

- Rust `journal-registry` `Registry::unwatch_directory` only.

Existing patterns to reuse:

- SOW-0173 registry tests (`tests/relative_directory.rs`).

Risk and blast radius:

- Registry unwatch only.

Sensitive data handling plan:

- Synthetic directories only.

Implementation plan:

1. To be written after the decision.

Validation plan:

- Registry tests for trailing-slash and machine-id-directory watches, and
  for overlapping watches under option A.

Artifact impact plan:

- AGENTS.md: unaffected.
- Runtime project skills: unaffected.
- Specs: none describe the registry contract.
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

### 2026-11-05

- Created from the SOW-0173 same-failure scan.

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
