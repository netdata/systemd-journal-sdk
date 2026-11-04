# SOW-0172 - Rust Log Fails Late On A Relative Directory

## Status

Status: completed

Sub-state: completed 2026-11-04 with option A, chosen by the user.

## Requirements

### Purpose

Make the Rust and Go directory writers treat a relative directory the same
way, and never fail after creating directories.

### User Request

Found while validating SOW-0156 chunk 2. On 2026-11-04 the user chose option
A (Rust makes the directory absolute) and asked for this SOW before SOW-0160.

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

- `helpers.rs` `create_chain` calls `path.canonicalize()` and discards the
  result, so the chain keeps the relative path. The original intent was an
  absolute chain path.
- Root mode keeps the configured directory as given. `prune_root` skips any
  file whose `File::from_path` is `None` (`root_retention.rs` `prune_root`),
  so with a relative root, retention pruning silently does nothing.
- Go `validateNewLogConfig` refuses an empty directory; `std::path::absolute`
  refuses an empty path too.

Inferences:

- Every caller so far passes absolute paths, so nobody has hit it; it is a
  parity gap and a confusing error, not data loss.

### Acceptance Criteria

- Rust `Log` with a relative directory writes, rotates and reopens like Go,
  in both default and root-retention modes, and root pruning works.
- Rust `Log` with an empty directory fails at construction with
  `InvalidPath`, before creating anything (Go refuses it too).
- Existing absolute-path behavior is unchanged.

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

Status: ready

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

1. `Log::new_inner` resolves the directory once with `std::path::absolute`
   (lexical: no symlink resolution, `..` kept, as the OS resolves it in Go),
   mapping failure to `WriterError::InvalidPath`, and passes the absolute path
   to startup, the chain and root retention.
2. Consequence: `configured_directory()`, `journal_directory()` and
   `active_path()` return absolute paths in Rust, where Go returns the path as
   given. Both name the same files; document it in the Rust accessor docs.
3. Tests: relative-directory write, rotate and reopen in default and root
   mode (with pruning), and the empty-directory refusal, in Rust; a Go
   relative-directory test for parity if none exists.

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

- None. The accessor consequence follows from option A.

## Implications And Decisions

1. 2026-11-04, user: option A.

## Plan

See the implementation plan in the gate.

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

### 2026-11-04

- Activated after the user chose option A.
- `Log::new_inner` resolves the directory with `std::path::absolute` before
  startup, so the chain, active files, root inventory, archiving and pruning
  all see absolute paths. An empty directory now fails with `InvalidPath`;
  before, Rust created `<machine-id>/` in the working directory and then
  failed on the first write.
- Tests: Rust `relative_directory_writes_rotates_and_reopens`,
  `empty_directory_is_refused_before_creating_anything` (log_writer) and
  `root_retention_with_relative_root_prunes` (root_retention); Go
  `TestLogRelativeDirectoryWritesRotatesAndReopens` and
  `TestLogRefusesEmptyDirectory`. The relative paths go through `..` from the
  current directory, so no test changes the process working directory.
- Doc wording first said Go resolves at construction; corrected before close
  (Go keeps the path and the OS resolves it per operation).

## Validation

Acceptance criteria evidence:

- Relative directory, default mode: the Rust test writes 3 entries with a
  2-entry rotation policy, reopens and writes 1 more; 4 entries across at
  least 2 files, all accessors absolute. The Go twin passes the same shape.
- Relative directory, root mode: `root_retention_with_relative_root_prunes`
  deletes the oldest of two archived fixtures under a 2-file count policy.
- Empty directory: Rust returns `InvalidPath` at construction; Go returns an
  error (existing `validateNewLogConfig` check).
- Absolute paths unchanged: `std::path::absolute` returns an absolute input
  normalized only by `.` removal; all existing Rust and Go tests pass.

Tests or equivalent validation:

- With the fix removed, all three Rust tests fail: the relative writer and
  root tests fail on the first write, the empty-path test because `Log::new`
  succeeds. With the fix, they pass.
- `go vet ./journal` clean; `go test ./... -count=1` passes.
- `cargo test --workspace` passes.
- `cargo clippy -p systemd-journal-sdk-log-writer --all-targets` reports no
  warning in the changed files. With `-D warnings` it stops on existing
  `manual_is_multiple_of` and similar lints in `journal-common`,
  `journal-core` and `journal-registry`, under both 1.91 and stable; those
  predate this SOW and are not touched here.
- `python3 tests/docs/check_wiki_docs.py`: 16 pages valid.

Real-use evidence:

- The SOW-0156 Rust fill tool failed with a relative directory; the new
  Rust test runs the same `Log::new` path with a relative directory and
  rotation. The unfixed empty-path run created a stray empty `<machine-id>/`
  directory in the crate directory (removed), which confirms the old
  late-failure behavior.

Reviewer findings:

- Self-review of the whole diff. One finding: the first doc and spec
  wording misdescribed Go as resolving at construction; fixed. The discarded
  `canonicalize()` result in `helpers.rs` `create_chain` stays as an
  existence check; with an absolute input it is harmless.

Same-failure scan:

- `File::from_path` call sites in all Rust crates: outside tests and the
  log writer, only `journal-registry` (`scan_journal_files`, `Registry` event
  handlers) takes caller paths. `Registry::watch_directory` with a relative
  directory silently finds no files: tracked as SOW-0173 for a decision.
- Go has no `from_path` equivalent restriction.

Sensitive data gate:

- Synthetic UUIDs and temporary directories only; no secrets, identities or
  host data in any changed file.

Artifact maintenance gate:

- AGENTS.md: unaffected; no workflow or rule change.
- Runtime project skills: unaffected; no new compatibility or docs rule.
- Specs: `.agents/sow/specs/product-scope.md` records the relative and empty
  directory contract for both writers.
- End-user/operator docs: `docs/Writer-APIs.md` Directory Writer section;
  rustdoc on `Log::configured_directory`.
- End-user/operator skills: none exist.
- SOW lifecycle: completed, moved to `done/` in the same commit as the work.
- SOW-status.md: updated (backlog, Recently Closed, SOW-0173 pending).

Specs update:

- `product-scope.md` directory writer bullets.

Project skills update:

- None needed; see the artifact gate.

End-user/operator docs update:

- `docs/Writer-APIs.md`.

End-user/operator skills update:

- None exist.

Lessons:

- See Lessons Extracted.

Follow-up mapping:

- Registry relative directory: tracked by SOW-0173.

## Outcome

Rust `Log` now accepts a relative directory as Go does, in default and root
retention modes, and refuses an empty directory up front. Rust path accessors
return absolute paths; Go returns them as given (documented).

## Lessons Extracted

- A discarded `canonicalize()` result hid the intended absolute-path
  invariant for years; `File::from_path`'s silent `None` turns a path-shape
  issue into a late, vague error or, in pruning and scanning, into silent
  no-ops. Path invariants belong at the public constructor.

## Followup

- SOW-0173 (pending): Rust `Registry::watch_directory` skips every file
  under a relative directory; needs a user decision.

## Regression Log

None yet.
