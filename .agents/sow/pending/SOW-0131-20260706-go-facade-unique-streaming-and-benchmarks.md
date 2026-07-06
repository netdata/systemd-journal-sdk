# SOW-0131 - Go Facade Unique Streaming And Benchmarks

## Status

Status: open

Sub-state: created 2026-07-06 from SOW-0129 reviewer findings; not yet
implemented.

## Requirements

### Purpose

Bring Go sd-journal-style stateful unique enumeration to the same lazy
FIELD/DATA-chain behavior implemented for Rust in SOW-0129, and add benchmark
evidence for high-cardinality unique enumeration so future release work has
measured performance data instead of only functional tests.

### User Request

The user requested SOW-0127 through SOW-0130 be fixed and reviewed. During the
read-only review pass for those SOWs, reviewers identified an untracked Go
facade parity gap and missing benchmark evidence in SOW-0129.

### Assistant Understanding

Facts:

- Rust `SdJournalQueryUniqueState` now streams stateful unique enumeration over
  FIELD/DATA chains without pre-materializing all returned payloads.
- Go `SdJournalQueryUniqueState` still calls `VisitUnique`, stores every
  `FIELD=value` payload in `sdJournal.uniqueItems`, and returns cloned payloads
  from `SdJournalEnumerateAvailableUnique`
  (`go/journal/facade.go`).
- SOW-0129 listed benchmark evidence as an acceptance target, but the completed
  work only added functional tests.

Inferences:

- Go consumers that use the Go facade for facets-style unique enumeration still
  pay the eager allocation cost Rust just removed.
- A shared benchmark or comparable Rust/Go benchmark data is needed to keep
  future performance claims honest.

Unknowns:

- Whether Go has enough existing low-level iterator state to implement the
  stateful facade change surgically, or whether it needs a small reader helper
  first.

### Acceptance Criteria

- Go `SdJournalQueryUniqueState` plus `SdJournalEnumerateAvailableUnique` no
  longer pre-materializes all unique payloads.
- Go stateful unique enumeration uses FIELD/DATA chains and preserves existing
  owned `FIELD=value` return shape.
- Directory readers deduplicate unique values across files without hiding
  duplicates in tests.
- Benchmark or benchmark-style evidence records before/after allocation and time
  behavior for high-cardinality unique enumeration, at least for Rust and Go
  current implementations.
- SOW-0129 benchmark deferral is resolved by this SOW's outcome.

## Analysis

Sources checked:

- `go/journal/facade.go`
- `go/journal/reader_unique.go`
- `go/journal/directory_reader.go`
- `tests/benchmarks/`
- `rust/src/journal/src/facade.rs`

Current state:

- Rust is fixed; Go still materializes stateful unique payloads eagerly.
- Benchmark harnesses exist under `tests/benchmarks/`, but no high-cardinality
  unique-enumeration benchmark was added for SOW-0129.

Risks:

- Implementing Go streaming without preserving restart behavior can break
  libsystemd-style callers.
- Benchmarks can become noisy if they use live host journals; use generated
  repository-local fixtures only.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- SOW-0129 fixed Rust only, while the Go facade exposes the same stateful unique
  API shape and still uses the old materializing implementation.

Evidence reviewed:

- Review findings from the SOW-0127 through SOW-0130 read-only reviewer batch.
- `go/journal/facade.go` stateful unique implementation.
- Rust SOW-0129 implementation and tests.

Affected contracts and surfaces:

- Go facade API internals and tests.
- Performance benchmark artifacts under `tests/benchmarks/` or internal test
  commands.
- SOW-0129 follow-up mapping and future release notes.

Existing patterns to reuse:

- Rust SOW-0129 FIELD/DATA stateful iterator pattern.
- Go `Reader.VisitUnique` and `DirectoryReader.VisitUnique` FIELD/DATA paths.
- Existing benchmark scripts under `tests/benchmarks/`.

Risk and blast radius:

- Medium implementation risk in Go reader state handling; low public API risk
  if owned return shape and function signatures remain unchanged.

Sensitive data handling plan:

- Use generated synthetic journal files only. Do not read live host journals or
  production data.

Implementation plan:

1. Add Go reader/directory stateful unique helpers or equivalent iterator state.
2. Rework Go facade stateful unique enumeration to stream and preserve restart.
3. Add duplicate-sensitive cross-file tests.
4. Add benchmark evidence for high-cardinality unique enumeration.

Validation plan:

- `go test ./...` with repository-local Go caches.
- Rust workspace tests if shared docs/specs change.
- Benchmark command output recorded in this SOW.
- External read-only reviewer pass before completion.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: no expected update.
- Specs: update only if public behavior or benchmark policy changes.
- End-user/operator docs: update only if public Go facade guidance changes.
- End-user/operator skills: no expected update.
- SOW lifecycle: this SOW tracks the SOW-0129 benchmark/parity follow-up.
- SOW-status.md: update while open and on close.

Open-source reference evidence:

- None checked yet; this is an internal parity/performance follow-up.

Open decisions:

- No user decision currently blocks implementation. The conservative design is
  to preserve the existing Go public signatures and owned payload return shape.

## Implications And Decisions

- This SOW tracks SOW-0129 reviewer findings instead of reopening SOW-0129,
  because SOW-0129's shipped Rust behavior remains valid and the missing Go
  parity/benchmark work is a separate implementation chunk.

## Plan

1. Implement Go streaming unique state.
2. Add duplicate-sensitive tests.
3. Add benchmark evidence.
4. Validate and review.

## Delegation Plan

Implementer:

- Local implementation by the primary agent unless the user changes routing.

Reviewers:

- Read-only reviewers from the approved pool before completion.

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

- Record blockers and missing evidence before changing scope.

## Execution Log

### 2026-07-06

- Created from read-only reviewer findings during SOW-0127 through SOW-0130
  review.

## Validation

Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

Pending.
