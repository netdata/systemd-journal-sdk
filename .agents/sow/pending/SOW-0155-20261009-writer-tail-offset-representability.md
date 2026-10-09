# SOW-0155 - Writer Tail Offset Representability

## Status

Status: open

Sub-state: source-analysis lead from PR #9 cubic comment 4234196552; pending
activation. No implementation or product decision is authorized by this record.

## Requirements

### Purpose

Investigate whether regular writers can publish unrepresentable global ENTRY-array
tail hints, and select paired writer protection if the defect is reproduced.

### User Request

The user requested verification of cubic's PR #9 findings. The proposed relaxation
of fixed-header validation was rejected because the wrapped header violates the
pinned systemd format checks. This record preserves the distinct pre-existing
writer-side concern found while verifying that suggestion.

### Assistant Understanding

Facts: both writers narrow global tail offsets to 32 bits. Regular-mode allocators
lack a universal 4-GiB ceiling. A zero hint invokes traversal, while nonzero wrapped
hints are trusted. The upstream fixed-header validator rejects zero/nonzero pairing
mismatches; accepting those headers would not repair other wrapped residues.

Inference: sufficiently large regular files can publish invalid hints or use a
wrong wrapped location. This is source-analysis evidence, not a runtime reproducer.

Unknowns: exact reachable workload, safe pre-mutation rejection boundary, high-level
rotation/retry behavior, and whether any supported format route can preserve a
larger file without publishing an unrepresentable hint.

### Acceptance Criteria

- Reproduce through actual array allocation/publication, including zero and
  nonzero wrap residues. Label forced-offset tests as synthetic.
- Preserve or improve valid format compatibility; do not accept malformed headers
  to hide a writer defect.
- Propose paired Go/Rust behavior, tradeoffs and validation before implementation.
- If authorized, ensure rejected writes preserve a valid file and document the
  exact supported boundary and directory-writer behavior.

## Analysis

Sources checked at SDK commit 993279f:

- Go writer_arrays.go:53,64,69-85,109 and440-446; writer.go:36-38;
  mmap_unix.go:33-37; log.go:871-885.
- Rust journal-core file/writer_entry_arrays.rs:147,152-166,246,271 and623-638.
- systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced:
  src/libsystemd/sd-journal/journal-file.c:618-628,794-799,2167-2175,4048-4055.

These writer casts predate the root-retention validator changes. Current root
validation correctly rejects the resulting malformed metadata; fixing the writer
requires a separate allocation/failure-boundary investigation. This is not deferred
implementation of the accepted aggregate-header repair.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model: narrowing a 64-bit allocated offset to a 32-bit cached
hint can lose address bits; a zero-only fallback does not handle other residues.

Evidence reviewed: source paths above and PR #9 cubic comment 4234196552. A bounded
independent source review agrees the validator relaxation is invalid.

Affected contracts and surfaces: paired low-level regular writers, global array
allocation/publication, high-level Log rotation and public failure semantics.

Existing patterns to reuse: checked compact-offset conversion, terminal writer
failure boundaries, synthetic allocation fixtures and shared conformance matrix.

Risk and blast radius: premature mutation, malformed output, unexpected file-size
limits, repeated rotation and changed retry semantics. Runtime evidence is required
before choosing a remedy.

Sensitive data handling plan: synthetic identities and records only; scratch files,
logs and isolated caches under .local or /tmp; no live host journals or identity probes.

Implementation plan: not authorized. First reproduce and establish representability
limits, then present a paired design and obtain any required user decision.

Validation plan: actual writer-path regression at both wrap residues, rejection
before mutation, regular/compact and both append shapes, reopen/snapshot/stock-reader
checks, relevant performance and portability evidence.

Artifact impact plan: consumer writer guides, product-scope and compatibility skill
must capture any accepted boundary; no AGENTS or dependency change is currently needed.
Both SOW indexes track this unactivated work. No output/reference skill is affected.

Open-source reference evidence: pinned systemd source listed in Analysis.

Open decisions: whether to activate this investigation; after reproduction, whether
and how high-level writers rotate while low-level writers reject unrepresentable
writes. No particular behavior is approved by this pending record.

## Implications And Decisions

The validator fallback suggestion is rejected in SOW-0152. This does not certify
large-file writer behavior. The user retains activation and public behavior decisions.

## Plan

1. Reproduce the source-analysis concern through bounded synthetic writer paths.
2. Establish the clean paired end state and obtain decisions before implementation.
3. Implement, validate and independently review only after authorization.

## Implementation And Review Plan

No implementer is assigned. Independent source investigation is evidence for the
lead only. Future reviewer prompts must follow the repository boundary and current
read-only review policy. A failed reproducer is evidence to reconsider the premise,
not permission to force the proposed limitation.

## Execution Log

### 2026-10-09

- Recorded the source-analysis lead during authorized PR #9 triage. No writer source
  or output format was changed for this concern.

## Validation

Only source analysis was performed. No runtime reproduction, large-file benchmark or
native portability result is claimed. SOW-0152 retains the valid-header rejection
regressions. Durable evidence contains no sensitive data. Relevant documentation and
skills remain unchanged for this unapproved writer contract.

## Outcome

Pending activation; no implementation authorized.

## Lessons Extracted

A recovery fallback does not prove that the state leading to it is format-valid.
Protect representability at its writer boundary rather than weakening validation.

## Followup

Tracked from SOW-0152-20261008-root-history-retention and PR #9 comment 4234196552.

## Regression Log

No runtime regression has been reproduced in this pending investigation.
