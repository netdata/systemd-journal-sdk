# SOW-0067 - Go Cross Platform Portability

## Status

Status: open

Sub-state: pending Go-only portability implementation; child of SOW-0063.

## Requirements

### Purpose

Make the Go SDK reader, writer, directory writer, and file-backed journalctl
paths portable to Linux, FreeBSD, macOS, and Windows without weakening Linux
performance or journal compatibility.

### User Request

The user requested SDK portability to Linux, FreeBSD, macOS, and Windows, and
approved parallel worktrees for independent SOWs.

### Assistant Understanding

Facts:

- This SOW covers Go only.
- SOW-0063 recorded that Go currently fails Windows compilation because
  `syscall.Flock`, `syscall.LOCK_EX`, `syscall.LOCK_NB`, and
  `syscall.LOCK_UN` are used in writer paths.
- SOW-0063 recorded Linux `/proc` assumptions in Go stale-lock owner detection.
- Go already has a non-Unix mmap/read-write fallback, but common writer code
  fails before that fallback is useful on Windows.

Inferences:

- The correct implementation shape is build-tagged platform helpers for locks,
  process identity, directory sync, and any target-specific file behavior.
- Linux hot paths must stay as close to current behavior as possible because
  Go writer/reader performance is a Netdata gate.

Unknowns:

- Which non-Linux runtime environments are available locally for execution.

### Acceptance Criteria

- `go test ./...` passes on Linux from `go/`.
- `GOOS=windows GOARCH=amd64 go test ./...` compiles Go packages from `go/`.
- FreeBSD and macOS checks are added or exact local blockers are recorded.
- Go writer locking preserves one-writer behavior on supported targets.
- Non-Linux generated files have a reproducible path for Linux stock
  `journalctl --verify --file` validation.
- Linux Go performance does not regress unless measured and explicitly accepted.
- Specs/docs describe Go platform behavior.

## Analysis

Sources checked:

- `go/journal/writer.go`
- `go/journal/lock.go`
- `go/journal/mmap_other.go`
- `.agents/sow/pending/SOW-0063-20260530-cross-platform-portability.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`

Current state:

- Go uses POSIX file locking from shared writer code.
- Go stale-lock owner detection reads Linux `/proc`.
- Non-Unix mmap fallback exists but is blocked by compile failures elsewhere.

Risks:

- Weak locking can corrupt journal files under accidental multiple writers.
- Abstractions in hot paths can reduce ingestion performance.
- Windows sharing, delete, and rename semantics differ from POSIX.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Go portability is blocked by POSIX and Linux assumptions in file locking and
  stale-owner detection, not by the journal format model itself.

Evidence reviewed:

- SOW-0063 build-failure and source evidence.
- Project compatibility skill one-writer/multiple-reader and stock-validation
  requirements.

Affected contracts and surfaces:

- Go SDK imports/builds.
- Go writer, directory writer, reader, and journalctl rewrite.
- Lockfile behavior and retention/rotation paths.
- Linux performance benchmark expectations.

Existing patterns to reuse:

- Go build tags and existing `mmap_other.go`.
- Existing lockfile format `systemd-journal-sdk-lock-v1`.
- Existing interoperability and lock matrix runners.

Risk and blast radius:

- Medium-high for Go users; high if Linux performance regresses.

Sensitive data handling plan:

- Use synthetic fixtures only; do not read host live journals or record raw log
  payloads.

Implementation plan:

1. Split Go platform assumptions into build-tagged helpers.
2. Implement platform locks and owner/boot/directory helpers.
3. Run Linux tests and cross-target checks.
4. Update docs/specs and SOW validation.

Validation plan:

- Linux Go tests.
- Windows cross-compilation with caches under `.local/`.
- FreeBSD/macOS checks or blocker evidence.
- Relevant lock/interoperability tests.
- Benchmark smoke check if hot paths change.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: no update expected.
- Specs: update cross-platform behavior.
- End-user/operator docs: update Go docs.
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
   Go portability contract.

## Plan

1. Isolate Go platform code.
2. Implement portable lock and identity helpers.
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

- Created as Go-only child SOW under SOW-0063 for parallel worktree execution.

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
