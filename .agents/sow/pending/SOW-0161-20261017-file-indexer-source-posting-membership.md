# SOW-0161 - File Indexer Source Posting Membership

## Status

Status: open

Sub-state: inherited source-analysis lead; not activated or implemented.

## Requirements

### Purpose

Determine the correct membership and captured-tail contract for source-timestamp
postings used by native metadata indexes, preserving Go/Rust behavior and speed.

### User Request

Follow-up tracking from the requested SDK compatibility and performance work.
This record does not authorize an additional implementation workstream.

### Assistant Understanding

Facts:

- Rust FileIndexer collects source postings without dereferencing their ENTRY
  targets, then fills missing timestamps from global ENTRY offsets.
- The algorithm predates SOW-0159 and is present in prerequisite commit 4c49205.
- Strict bidirectional native membership certification belongs to verify_index.

Inferences:

- Replacing the source DATA inline target with that DATA object's own offset
  should admit an extra histogram record. This is a source prediction, not an
  executed reproduction or an established new regression.

Unknowns:

- Actual baseline/current results, equivalent Go surfaces and live-publication
  treatment must be established before selecting a repair.

### Acceptance Criteria

- Reproduce or reject the source prediction on baseline/current regular and
  compact fixtures, including an actively growing captured-tail case.
- Establish the affected native-index contract in both languages and obtain
  approval for any behavior/performance change before implementation.

## Analysis

Sources checked:

- rust/src/crates/journal-index/src/file_indexer.rs: collect_source_field_info,
  build_histogram and collect_data_entry_indices.
- rust/src/crates/journal-core/src/file/offset_array.rs: collect_offsets.
- .agents/sow/specs/product-scope.md: strict verification contract.
- SOW-0159: required-chain error propagation and documented value tolerance.

Current state:

- With the existing 14-row source fixture, source inspection predicts that
  inline DATA target plus entries 2..14 enter the source map; global fallback
  then adds entry 1, yielding 15 records. No traversal error is generated.
- Source postings beyond the captured tail are another condition to examine.
  Rejecting them mechanically could alter live-file behavior.

Risks:

- Full row parsing, an extra membership container or repeated searches could
  hurt indexing performance. Additional certification must not be introduced
  casually into this native metadata path.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- A native posting target may be accepted as a source-map key without proof
  that the global ENTRY population contains it. Reproduction remains required.

Evidence reviewed:

- The source and inherited algorithm listed above; no runtime reproduction.

Affected contracts and surfaces:

- Optional Rust FileIndexer/engine histogram and cache results; investigate Go
  aggregation equivalents. Core strict verification remains the certification API.

Existing patterns to reuse:

- Existing source timestamp fixtures, global offset pass, entry-offset map,
  engine batch isolation and versioned cache keys.

Risk and blast radius:

- Membership policy, live snapshots, repeated values and indexing costs.
  This lead does not change SOW-0159's decoder or bounded-chain repair.

Sensitive data handling plan:

- Synthetic journal identities and payloads only; generated evidence stays
  ignored under .local/. Durable records contain no raw private data.

Implementation plan:

1. Reproduce baseline/current behavior without changing runtime code.
2. Inspect both language surfaces and native publication semantics.
3. Present behavior and cost alternatives if a repair is justified, then update
   this gate with the approved plan before implementation.

Validation plan:

- Regular/compact, healthy/invalid targets, captured-tail growth and required
  Go/Rust parity; representative cost evidence appropriate to the chosen change.

Artifact impact plan:

- AGENTS.md: existing parity/performance rules apply; no change identified.
- Runtime project skills: update if validation exposes a reusable missed pattern.
- Specs: record an approved membership and captured-tail contract if changed.
- End-user/operator docs: document any changed index/cache behavior.
- End-user/operator skills: none currently exist.
- SOW lifecycle: pending only; no new prerequisite for SDK then CLI delivery.
- SOW-status.md: add this investigation lead to both status indexes.

Open-source reference evidence:

- No external repository was needed for this local source-analysis lead.

Open decisions:

- No implementation choice is approved. First establish actual behavior and
  scope; user design decisions follow concrete evidence.

## Implications And Decisions

1. SOW-0159 fixes an encountered absent-map facet panic on its existing cold
   error branch. Source-map certification is a separate inherited question,
   rather than another suppressed traversal error. Track it here without
   introducing additional indexing passes into the current repair.

## Plan

1. Investigate only after explicit activation; retain the approved current order.

## Implementation And Review Plan

Implementation:

- Not started. The project manager owns any subsequently approved changes.

Reviewers:

- No review has run for this pending investigation. Follow the repository's
  standing review authorization and system-wide skill at a meaningful boundary.

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

- Preserve reproductions and qualify uncertainty; do not change live-file
  behavior or allocate a verification framework without an approved plan.

## Execution Log

### 2026-10-17

- Recorded the source-analysis lead discovered during SOW-0159's consumer audit.

## Validation

- Source trace only. Runtime, same-failure and parity validation are not yet run.
- Artifact maintenance: this pending SOW and both status indexes are added;
  product contracts, skills and runtime code remain unchanged by this record.
- Follow-up mapping: source-map membership and captured-tail investigation
  belongs here; encountered native traversal errors remain SOW-0159.

## Outcome

Investigation is pending; no behavior change or successful repair is claimed.

## Lessons Extracted

Chain completion and cross-index membership are distinct guarantees.

## Followup

No additional follow-up is identified before investigation.

## Regression Log

No completed outcome has been reopened; origin and contract remain to establish.
