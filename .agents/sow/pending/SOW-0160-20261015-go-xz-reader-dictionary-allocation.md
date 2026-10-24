# SOW-0160 - Go XZ Reader Dictionary Allocation

## Status

Status: open

Sub-state: measured performance opportunity tracked from SOW-0108. Analysis and
implementation are not activated. Preserve the approved SOW-0108, SOW-0159,
SOW-0121 execution order; this item is not a prerequisite for that chain.

## Requirements

### Purpose

Assess whether Go XZ DATA decompression can reduce repeated dictionary
allocation while preserving format compatibility, reader isolation and speed.

### User Request

The user requires SDK/journalctl changes to maintain or improve performance.
This separate opportunity was discovered while measuring the approved reader
lifetime repair; no decoder redesign was requested or authorized.

### Assistant Understanding

Facts:

- `go/journal/writer_compression.go:173` creates a fresh `xz.NewReader` for
  each XZ DATA expansion, then bounds decompressed output with `readAllLimited`.
- SOW-0108's immutable 2,048-row compressed corpus exposes approximately
  8.6 GB/high-cardinality query and 17.2 GB/singleton query of cumulative
  allocation in affected XZ cases. These are per-operation allocation totals,
  not peak resident memory. Baseline and repaired implementations share this
  decoder path and its cost; this is not a new repair regression.
- Existing pending/current SOWs do not own this specific optimization.

Inferences:

- Decoder construction/dictionary allocation is a plausible reusable-work
  opportunity. Reuse may retain excessive memory or cross reader lifetimes;
  lower allocation alone would not establish a better implementation.

Unknowns:

- Whether the compatible dependency API permits bounded reuse, and whether
  reuse, a dependency improvement or retaining current behavior is best.
- The real-use frequency of XZ journals and the acceptable retained-memory
  tradeoff. Investigate measurable costs before requesting a design choice.

### Acceptance Criteria

- Reproduce allocation sources with synthetic fixtures and explicit timer
  boundaries; report cumulative allocation and retained memory separately.
- Present measured alternatives and a user-approved design before coding.
- Any implementation preserves Go/Rust observable behavior, pure SDK runtime,
  complete compressed-stream handling and compiler/dependency constraints.
- Preserve or improve affected latency, allocation and memory paths, including
  repeated readers, concurrent independent readers and non-XZ controls.

## Analysis

Sources checked:

- `go/journal/writer_compression.go:173` and indexed DATA expansion in
  `go/journal/explorer.go`.
- SOW-0108's 2026-10-15 regression evidence, immutable codec corpus and retained
  before/candidate allocation observations under ignored task scratch.
- Pending/current SOW inventory; SOW-0159 owns regression prevention, while
  SOW-0122 owns separate CLI invocation/follow scalability.

Current state:

- No decoder source change is included in SOW-0108. Its lifetime/header repair
  must satisfy its own performance gate independently of this opportunity.

Risks:

- Pooling could exchange allocation for retained memory, contention or complex
  ownership. Stream completeness and malformed-input behavior must remain
  consistent. A dependency change could affect compiler floors or portability.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- Repeated independent XZ DATA decoding constructs new decoder state. Existing
  allocation measurements justify investigation, but do not select a design.

Evidence reviewed:

- The SDK construction call and matched allocation controls described above.
  A complete dependency/API and lifetime analysis is required on activation.

Affected contracts and surfaces:

- Go compressed readers and all callers of XZ DATA expansion; preserve Rust
  parity and cross-language fixture behavior. Writers are affected only if a
  shared dependency or helper actually changes.

Existing patterns to reuse:

- Immutable codec fixtures, matching indexed/traversal checks, per-DATA output
  limits, complete-stream controls and frozen paired benchmark helpers.

Risk and blast radius:

- Decoder ownership, memory retention, concurrency, dependency/compiler floors,
  and allocation/latency regressions in XZ and shared decompression callers.

Sensitive data handling plan:

- Use synthetic fields and identities. Keep raw profiles/fixtures in ignored
  repository-local scratch; commit only aggregate findings and source pointers.

Implementation plan:

1. After activation, profile constructor/dictionary/output costs and inspect
   compatible reuse/dependency alternatives without changing production code.
2. Present measured options and obtain the user's design choice. Implement
   only the selected minimal-complete design, or reject the opportunity with
   evidence if every option has worse costs or incompatible behavior.

Validation plan:

- Cover regular/compact layouts, historical XZ fixtures, multiple blocks and
  streams, read errors, concurrent readers and repeated opens/closes. Measure
  time, allocations and retained memory against current Go and the Rust sibling;
  run non-XZ controls and relevant interoperability checks.

Artifact impact plan:

- AGENTS.md: no workflow change is proposed.
- Runtime project skills: update compression/performance guidance if new reuse
  or validation rules become necessary.
- Specs: update only actual operational guarantees changed by an approved design.
- End-user/operator docs: explain any changed options or memory guarantees;
  an internal improvement may require no usage changes.
- End-user/operator skills: none currently exist in this repository.
- SOW lifecycle: keep open/pending until explicitly activated; no current-chain
  scope is transferred here.
- SOW-status.md: list this opportunity in both canonical and root indexes.

Open-source reference evidence:

- This tracking record cites project source and retained project measurements;
  no external repository checkout is used as evidence. On activation, record
  dependency identity and checked revision for any external source analysis.

Open decisions:

- Activation and the decoder design are not authorized. No immediate user
  answer is required while the requested SDK/journalctl chain remains active.

## Implications And Decisions

1. Track independently instead of expanding the approved lifetime/header repair.
   No pooling, decoder replacement, new dependency or public option is approved.

## Plan

1. Retain the measured opportunity and source pointers in this pending SOW.
2. Await activation, then complete analysis and present design options before code.

## Implementation And Review Plan

Implementation:

- The project manager owns any future analysis and implementation sequentially.
  No runtime implementation is part of creating this tracking record.

Reviewers:

- No external review is authorized. Follow the system-wide external-reviewers
  skill after complete local validation if this SOW is later activated.

Repository boundary block for every external-reviewer prompt:

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

- Retain unfavorable measurements and failed controls. Do not accept a lower
  allocation count that increases latency, retained memory or compatibility gaps.

## Execution Log

### 2026-10-15

- Created from the project SOW template as SOW-0108's concrete follow-up mapping.
  No runtime code, dependency or current execution order changes.

## Validation

Acceptance criteria evidence:

- This is an open tracking record; optimization acceptance is not claimed.

Tests or equivalent validation:

- Source/ownership search and existing matched codec measurements establish
  the lead. New implementation tests are neither required nor run for this record.

Real-use evidence:

- Existing Explorer XZ queries reproduce the cumulative allocation cost with
  synthetic on-disk journal files; consumer prevalence remains to be measured.

Reviewer findings:

- Read-only ownership analysis found no existing specific follow-up SOW.
  No final implementation review has occurred.

Same-failure scan:

- The shared decoder serves multiple SDK callers; their actual blast radius
  must be enumerated before selecting or implementing a design.

Sensitive data gate:

- This record contains only synthetic aggregate measurements and source paths.

Artifact maintenance gate:

- AGENTS.md: unchanged; no workflow rule changes.
- Runtime project skills: unchanged for this tracking-only record.
- Specs: unchanged; no product behavior or guarantee changes.
- End-user/operator docs: unchanged; no new user behavior exists.
- End-user/operator skills: none currently exist.
- SOW lifecycle: open under pending; not an implementation prerequisite.
- SOW-status.md: canonical and root indexes include this opportunity.

Specs update:

- No implementation or shipped contract changes exist yet.

Project skills update:

- Existing compression/performance workflow already governs later analysis.

End-user/operator docs update:

- No usage or memory policy is changed by tracking an opportunity.

End-user/operator skills update:

- No output/reference skills are present or affected.

Lessons:

- Separate cumulative allocated bytes from peak or retained memory, and do not
  mix an unchanged decoder opportunity into a lifetime regression's acceptance.

Follow-up mapping:

- SOW-0108's XZ dictionary-allocation opportunity is tracked here. Required
  SDK/CLI regression-prevention work stays in SOW-0159/SOW-0121.

## Outcome

Open; no optimization has been authorized or implemented.

## Lessons Extracted

The allocation-accounting distinction above is retained for future analysis.

## Followup

This SOW is the concrete tracking destination; it creates no further workstream.

## Regression Log

No completed outcome exists; this record tracks an unchanged baseline opportunity.
