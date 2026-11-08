# SOW-0173 - Rust Registry Skips Files Under A Relative Directory

## Status

Status: completed

Sub-state: completed 2026-11-05.

## Requirements

### Purpose

Make `journal_registry::Registry::watch_directory` either work with a
relative directory or refuse it, instead of silently finding no files.

### User Request

Found by the SOW-0172 same-failure scan. On 2026-11-05 the user approved the
recommendation (option A, make the directory absolute).

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

- `watch_directory` with a relative directory registers the journal files
  under it, and later notify events (create, remove, rename) update the
  registry, as with the equivalent absolute directory.
- `unwatch_directory` with the same relative string matches the watch and
  removes its files.
- An absolute directory string is used unchanged (no behavior change for
  existing callers); an empty directory is refused with an error instead of
  scanning the working directory.
- Tests fail without the fix and pass with it.

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

Status: ready

Problem / root-cause model:

- See Assistant Understanding.

Evidence reviewed:

- The code paths above.

Affected contracts and surfaces:

- Rust `journal-registry` `Registry::watch_directory` and
  `unwatch_directory` only. No Go surface exists (registry is Rust-only), so
  no parity work is needed.
- `std::path::absolute` is lexical: absolute input is returned unchanged
  (probed: `"/x/y/"` and `"/x/../y"` come back as given), so existing
  absolute callers see identical keys and paths. Empty input fails with
  `InvalidInput`, which maps to `RegistryError::Io`.

Existing patterns to reuse:

- SOW-0172 (`Log::new_inner` resolves with `std::path::absolute`).

Risk and blast radius:

- Registry directory watching only.

Sensitive data handling plan:

- Synthetic directories only.

Implementation plan:

1. In `Registry::watch_directory` and `unwatch_directory`, resolve the input
   once with `std::path::absolute` and use the result for the watched-set key,
   the scan, the notify watch and directory removal. A non-UTF-8 result (only
   possible through a non-UTF-8 working directory) is refused with an I/O
   `InvalidInput` error, because keys are `String`.
2. Rustdoc on both methods states the contract.
3. Tests in the registry crate: relative watch registers files, a create
   event from notify is applied, unwatch with the same relative string
   removes them, and an empty path is refused.

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

- None; decided 2026-11-05.

## Implications And Decisions

1. 2026-11-05, user: option A. The registry resolves a relative directory
   once against the working directory at call time, like the SOW-0172 writer
   fix.

## Plan

See the Pre-Implementation Gate implementation plan.

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

### 2026-11-05

- User chose option A; SOW activated on branch
  `sow-0173/rust-registry-relative-dir` (stacked on SOW-0172).
- Added `absolute_directory` in `registry/mod.rs`, used by
  `watch_directory` and `unwatch_directory`; rustdoc on both.
- Added `journal-registry/tests/relative_directory.rs`. The first run
  failed on the test's own expectation: resolution is lexical, so registered
  paths keep the `..` components (as SOW-0172 accessors do); the test now
  expects that form.
- Probe of unwatch with other directory forms found a pre-existing exact-key
  mismatch; tracked as SOW-0174.

## Validation

Acceptance criteria evidence:

- Relative watch: `relative_directory_registers_files_events_and_unwatch`
  registers the existing `system.journal`, applies notify create, rename and
  remove events, and `unwatch_directory` with the same relative string removes
  what remains.
- Absolute input unchanged: `std::path::absolute` returns `"/x/y/"` and
  `"/x/../y"` as given (probe), so keys and paths for absolute callers are
  identical; all existing workspace tests pass.
- Empty directory: `empty_directory_is_refused` passes. It also passes
  without the fix (`walkdir` already errored on an empty path), so it guards
  the contract rather than proving the fix.

Tests or equivalent validation:

- With `absolute_directory` made a pass-through, the relative test fails at
  the first assertion (zero files registered); with the fix it passes
  (3 consecutive runs).
- `cargo test --workspace`: 685 passed, 0 failed.
- `cargo clippy -p systemd-journal-sdk-registry --all-targets`: no warning in
  changed lines; existing `redundant_field_names` (`registry/mod.rs`
  `update_time_range`) and `items_after_test_module` (`repository/file.rs`)
  predate this SOW.
- `rustfmt --edition 2024 --check` clean on both changed Rust files.

Real-use evidence:

- No CLI or product path exposes `Registry` with a caller directory; the
  integration test drives the real `notify` watcher and filesystem, which
  is the runtime path.

Reviewer findings:

- Self-review of the whole diff. No blocking finding. Non-UTF-8 results
  (only via a non-UTF-8 working directory) are refused with `InvalidInput`,
  because watch keys are `String`; previously such a path could not arise
  from `&str` input.

Same-failure scan:

- All `File::from_path` caller-path sites were covered by SOW-0172 and this
  SOW. Probing `unwatch_directory` with other directory forms found that a
  trailing slash or a directly watched machine-id directory leaves files
  registered after unwatch; pre-existing and independent of relative paths;
  tracked as SOW-0174.

Sensitive data gate:

- Synthetic machine ID and temporary directories only.

Artifact maintenance gate:

- AGENTS.md: unaffected; no workflow or rule change.
- Runtime project skills: unaffected.
- Specs: none describe the registry's directory contract; unaffected.
- End-user/operator docs: rustdoc on `watch_directory` and
  `unwatch_directory`; `docs/` does not cover the registry.
- End-user/operator skills: none exist.
- SOW lifecycle: completed, moved to `done/` with the work.
- SOW-status.md: updated (backlog, Recently Closed, SOW-0174 pending).

Specs update:

- None needed; see the artifact gate.

Project skills update:

- None needed.

End-user/operator docs update:

- Rustdoc only.

End-user/operator skills update:

- None exist.

Lessons:

- See Lessons Extracted.

Follow-up mapping:

- Unwatch exact-key mismatch: tracked by SOW-0174.

## Outcome

`Registry::watch_directory` and `unwatch_directory` accept a relative
directory, resolved once at call time, and keep absolute input unchanged.

## Lessons Extracted

- A guard test that passes without the fix must be labeled as such in the
  Validation section, so it is not read as regression proof.

## Followup

- SOW-0174 (unwatch exact-key mismatch).

## Regression Log

None yet.
