# SOW-0068 - Rust Cross Platform Portability

## Status

Status: open

Sub-state: pending Rust-only portability implementation; child of SOW-0063.

## Requirements

### Purpose

Make the Rust SDK reader, writer, directory writer, and file-backed journalctl
paths portable to Linux, FreeBSD, macOS, and Windows while keeping Rust as the
reference implementation for compatibility and performance.

### User Request

The user requested SDK portability to Linux, FreeBSD, macOS, and Windows, and
approved parallel worktrees for independent SOWs.

### Assistant Understanding

Facts:

- This SOW covers Rust only.
- SOW-0063 recorded that Rust Windows target checking failed in
  `journal-common` because `nix` time APIs were unavailable on the checked
  Windows target.
- SOW-0063 recorded Linux `/proc` assumptions in Rust stale-lock owner
  detection.
- Rust is the reference implementation for cross-language behavior.

Inferences:

- Rust needs platform modules for time, locking, process identity, boot ID,
  directory sync, and mmap/fallback behavior.
- Linux behavior and reader row-scoped lifetime guarantees must not weaken.

Unknowns:

- Which non-Linux runtime environments are available locally for execution.

### Acceptance Criteria

- Linux Rust tests pass for affected crates.
- Windows Rust target checks compile for the SDK crates in scope, or exact
  target/toolchain blockers are recorded.
- FreeBSD and macOS target checks are added or blockers are recorded.
- Rust writer locking preserves one-writer behavior on supported targets.
- Rust reader/writer behavior keeps existing shared conformance and
  interoperability results on Linux.
- Linux Rust performance and facade lifetime guarantees do not regress.
- Specs/docs describe Rust platform behavior.

## Analysis

Sources checked:

- `rust/src/crates/journal-core/src/file/lock.rs`
- `rust/src/crates/journal-core/src/file/file.rs`
- `rust/src/crates/journal-common/src/time.rs`
- `.agents/sow/pending/SOW-0063-20260530-cross-platform-portability.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`

Current state:

- Rust lock stale-owner detection reads Linux `/proc`.
- Rust Windows target check failed in `journal-common` time code.
- Linux behavior is currently the compatibility and performance reference.

Risks:

- Refactoring platform code can accidentally change Linux semantics.
- Rust row-scoped reader data lifetime guarantees must remain intact.
- Windows locking and file sharing require explicit behavior.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Rust portability is blocked by Linux/POSIX assumptions in common time and
  locking helpers. Format-level code is mostly OS-independent, but platform
  integration is not isolated.

Evidence reviewed:

- SOW-0063 target-check and source evidence.
- Project compatibility skill requirements for compatibility, live safety, and
  row-scoped facade data.

Affected contracts and surfaces:

- Rust crates under `rust/src/crates/`.
- Rust SDK reader/writer APIs and libsystemd facade.
- Rust journalctl rewrite.
- Locking, directory sync, mmap/fallback, boot/process identity, and time APIs.

Existing patterns to reuse:

- Rust `cfg` modules.
- Existing lockfile format `systemd-journal-sdk-lock-v1`.
- Existing row-scoped facade lifetime model.
- Existing interoperability and benchmark runners.

Risk and blast radius:

- High because Rust is the reference implementation.

Sensitive data handling plan:

- Use synthetic fixtures only; do not read host live journals or record raw log
  payloads.

Implementation plan:

1. Isolate Rust platform assumptions behind target-specific modules.
2. Preserve Linux code paths and hot behavior.
3. Add target checks and tests.
4. Update docs/specs and SOW validation.

Validation plan:

- Linux cargo tests for affected crates.
- Windows target check with cache/output paths under `.local/`.
- FreeBSD/macOS checks or exact blocker evidence.
- Relevant interoperability and lock tests on Linux.
- Benchmark smoke check if hot paths change.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: no update expected.
- Specs: update cross-platform behavior.
- End-user/operator docs: update Rust docs.
- End-user/operator skills: no update expected.
- SOW lifecycle: child of SOW-0063.
- SOW-status.md: list as pending.

Open-source reference evidence:

- None added; baseline remains systemd/systemd v260.1 from project specs.

Open decisions:

- None. User approved parallel worktree execution.

## Implications And Decisions

1. 2026-05-30: This SOW is assigned to an isolated worktree. It should not edit
   other language implementations except shared specs/docs/tests required by the
   Rust portability contract.

## Plan

1. Isolate Rust platform code.
2. Implement portable lock, time, and identity helpers.
3. Validate and document.

## Delegation Plan

Implementer:

- User-spawned implementation agent in a dedicated worktree.

Reviewers:

- Whole-SOW read-only reviewer pass after implementation and local validation.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- DO NOT MAKE CHANGES OUTSIDE THIS REPOSITORY FOR ANY REASON.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- Append questions or blockers to this SOW under `## Agent Questions -
  YYYY-MM-DD` with evidence, options, and a recommendation, then stop.

## Execution Log

### 2026-05-30

- Created as Rust-only child SOW under SOW-0063 for parallel worktree
  execution.

## Validation

Acceptance criteria evidence:

- Pending.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending.

Reviewer findings:

- Pending.

Same-failure scan:

- Pending.

Sensitive data gate:

- Pending.

Artifact maintenance gate:

- Pending.

Lessons extracted:

- Pending.

Follow-up mapping:

- Parent umbrella: `SOW-0063-20260530-cross-platform-portability.md`.
