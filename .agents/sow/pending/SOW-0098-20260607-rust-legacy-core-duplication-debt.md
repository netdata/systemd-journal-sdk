# SOW-0098 - Rust Legacy Core Duplication Debt

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: opened from SOW-0096 Codacy file metrics audit; blocked on user
priority decision before implementation.

## Requirements

### Purpose

Reduce Rust duplication between the legacy `jf` compatibility implementation
and the current `journal-core` implementation only where doing so preserves the
historical-reader compatibility contract, performance contract, and Netdata
integration path.

### User Request

The user asked for Codacy file-by-file Rust/Go complexity and duplication
analysis. SOW-0096 found that the largest Rust production duplication is real
overlap between legacy `jf` code and `journal-core`.

### Assistant Understanding

Facts:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md` classifies Rust
  duplication using Codacy file metrics plus local Lizard max function CCN.
- Top Rust production duplication files include:
  - `rust/src/crates/jf/journal_file/src/file.rs`: duplication `686`.
  - `rust/src/crates/journal-core/src/file/offset_array.rs`: duplication `662`.
  - `rust/src/crates/jf/journal_file/src/offset_array.rs`: duplication `600`.
  - `rust/src/crates/journal-core/src/file/file.rs`: duplication `491`.
  - `rust/src/crates/jf/journal_file/src/journal_file.rs`: duplication `427`.
- The `jf` crate is historically important because it is the compatibility
  layer for a libsystemd-like reader API and historical journal support.

Inferences:

- The Rust duplication is not scanner noise. It represents real architectural
  overlap that was tolerated while preserving battle-tested `jf` behavior and
  building the current core SDK.
- Any deduplication must be compatibility-first and benchmark-backed. Removing
  duplicated code by redirecting legacy paths to newer primitives may be
  correct, but only if all historical fixtures and facade behavior stay intact.

Unknowns:

- Which duplicated primitives can be safely shared without changing `jf`
  semantics.
- Whether sharing lower-level primitives affects row-lifetime guarantees,
  rolling mmap behavior, or historical compatibility.

### Acceptance Criteria

- User-approved Rust duplication target list and migration direction are
  recorded before implementation.
- Compatibility tests covering `jf`, `journal-core`, facade APIs, historical
  fixtures, and real-corpus representative files pass.
- Reader performance benchmarks prove no regression in the hot paths touched.
- Codacy file metrics are rechecked after push and compared against SOW-0096
  baseline.

## Analysis

Sources checked:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md`.
- SOW-0086 through SOW-0092 Rust reader performance work.
- SOW-0027 Netdata reader API and `jf` facade status.

Current state:

- The largest production duplication is Rust legacy/core overlap.
- The current Rust reader performance work recently optimized `journal-core`
  hot paths; any deduplication must not undo those guarantees.

Risks:

- Consolidating `jf` and `journal-core` too aggressively can break historical
  journal compatibility.
- Moving legacy code onto current primitives can accidentally change error
  behavior, mmap lifetime guarantees, or compressed DATA handling.
- Metric-driven deduplication without performance proof can make the SDK worse.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- Codacy duplication is high because legacy `jf` and current `journal-core`
  contain overlapping implementations of journal file/object/offset-array
  mechanics.

Evidence reviewed:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md`: Rust top duplication
  table and file-by-file classifications.

Affected contracts and surfaces:

- Rust `journal-core` reader/writer behavior.
- Rust `jf` compatibility layer.
- Libsystemd-like facade behavior.
- Historical journal file compatibility.
- Reader hot-path performance contract.

Existing patterns to reuse:

- Rust row-view and mmap lifetime architecture from SOW-0086 through SOW-0092.
- Existing shared `journal-common` crate for truly common primitives.

Risk and blast radius:

- High for reader compatibility and performance.
- Medium for public APIs if deduplication is limited to internal helpers.

Sensitive data handling plan:

- Do not commit raw Codacy API exports or real-corpus payloads. Durable
  artifacts may include file paths, numeric metrics, sanitized counts, and
  benchmark summaries only.

Implementation plan:

1. Ask the user to approve whether Rust duplication reduction should happen
   before Netdata integration.
2. Analyze duplicated code clusters and identify shareable primitives versus
   intentionally divergent compatibility logic.
3. Refactor one cluster at a time with historical fixture and benchmark proof.

Validation plan:

- Rust tests for affected crates.
- Shared conformance and interoperability tests for reader/facade paths.
- Historical fixture validation from existing SOW harnesses.
- Reader benchmark comparison against SOW-0092/SOW-0093 baselines where hot
  paths are affected.
- Codacy file metrics export after push.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: no expected update unless a new duplication-review
  rule is established.
- Specs: update if Rust public/internal compatibility contracts change.
- End-user/operator docs: update only if APIs change.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: complete this SOW after implementation, review, validation,
  and remote Codacy evidence.
- SOW-status.md: update on activation and close.

Open-source reference evidence:

- None checked yet. This SOW is pending and blocked on user priority.

Open decisions:

1. Whether to reduce Rust legacy/core duplication before Netdata integration.
2. Whether `jf` should remain structurally separate until Netdata vendored code
   removal is complete.

## Implications And Decisions

Pending user decision.

## Plan

1. Decide priority and safety constraints.
2. Identify duplicated clusters and accepted sharing boundaries.
3. Refactor one cluster, validate compatibility/performance, and recheck
   Codacy metrics.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Run the approved reviewer pool after the complete SOW implementation and
  local validation.

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

- If compatibility or performance risk exceeds metric value, record evidence
  and ask the user before continuing.

## Execution Log

Pending.

## Validation

Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

Pending.
