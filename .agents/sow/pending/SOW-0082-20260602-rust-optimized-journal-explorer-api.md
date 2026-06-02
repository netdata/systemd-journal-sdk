# SOW-0082 - Rust Optimized Journal Explorer API

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: pending; depends on SOW-0081.

## Requirements

### Purpose

Implement a new Rust SDK API that matches the useful legacy `systemd-journal.plugin` plus facets query behavior while fixing the three traversal API performance problems: unnecessary compressed-DATA decompression, unnecessary field traversal after required fields are satisfied, and repeated processing of deduplicated DATA objects.

### User Request

The user asked to recreate the `systemd-journal.plugin` implementation in Rust, fixing the three known problems, and benchmark it against the real Netdata `systemd-journal.plugin`.

### Assistant Understanding

Facts:

- The API should be close to the legacy traversal shape, but optimized.
- The API must support opt-in histogram, opt-in selected facet counters, indexed filters/slicing, seek/fetch Top-N rows with direction and anchor, and optional FTS.
- Top-N rows should include all fields, but only for returned rows.
- Filters and timeframe apply to all output aspects.
- This is a new Rust API; prior SOW-0074 explorer API is intentionally removed.

Inferences:

- The first production-grade API should optimize traversal before attempting index-derived facet/histogram aggregation.
- The implementation should use FIELD linkage and DATA object identity to avoid touching irrelevant compressed DATA and to avoid reprocessing repeated `FIELD=value` DATA objects.
- Index-derived facet/histogram aggregation belongs to SOW-0083 after this optimized traversal baseline exists.

Unknowns:

- Exact API type names and result structs should be finalized after SOW-0081 records the behavior.
- Early-stop behavior must account for repeated fields and may need per-field or per-query semantics.

### Acceptance Criteria

- Rust exposes a new SDK API for the specified query model without restoring the killed SOW-0074 API.
- Query inputs include timeframe, filters, direction, anchor, row limit, optional selected facets, optional histogram field, and optional FTS.
- Filters use journal DATA/FIELD indexes for exact slicing.
- Returned Top-N rows expand all fields only after row selection.
- Facets and histogram traverse candidate rows only when requested.
- Traversal touches only fields required for facets, histogram, FTS, and returned rows.
- Compressed DATA outside required fields is not decompressed.
- Reusable DATA objects are classified and value-processed once per query where correctness permits.
- Traversal stops once all required fields for the row are satisfied where repeated-field semantics permit it.
- Correctness is validated against the SOW-0081 specification and the legacy Netdata plugin/facets behavior.
- Benchmarks compare the Rust implementation against the real Netdata `systemd-journal.plugin` for representative generated and real-corpus queries.
- Counters report rows examined, DATA refs inspected, DATA objects classified, payloads decompressed, DATA-cache hits/misses, early-stop opportunities, FTS scans, facet/histogram updates, and returned-row expansions.
- Existing Rust reader/facade behavior remains backward compatible.

## Analysis

Sources checked:

- SOW-0081 will provide the authoritative behavior spec.
- Netdata source evidence from SOW-0081 is a dependency.

Current state:

- Prior SOW-0074 explorer implementation is being reverted.
- Existing Rust reader/facade APIs remain the baseline file-format reader surface.

Risks:

- Repeating SOW-0074's mistake by accepting a partial benchmark instead of the full query behavior.
- Early-stop can break repeated-field correctness if field multiplicity is not specified.
- Benchmarking against the real plugin requires careful isolation to avoid comparing different query semantics.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- The legacy libsystemd-style traversal shape forces callers to enumerate every current-row payload. This causes unnecessary decompression, unnecessary field traversal, and repeated processing of deduplicated DATA objects.
- The SDK needs a Rust API that preserves the legacy result semantics while changing the internal work model.

Evidence reviewed:

- User request on 2026-06-02.
- SOW-0081 is required before implementation begins.

Affected contracts and surfaces:

- Rust public SDK reader/query API.
- Rust reader internals.
- Benchmarks and real-corpus validation.
- Future Netdata reader integration SOWs.

Existing patterns to reuse:

- Rust reader/facade lifetime guarantees.
- FIELD/DATA traversal helpers.
- Existing corpus and benchmark harness patterns.

Risk and blast radius:

- Medium to high reader API risk; implementation must be additive and must not regress existing facade APIs.
- High performance risk if counters and benchmarks do not model the actual query shape.

Sensitive data handling plan:

- Generated fixtures are preferred for correctness tests.
- Real-corpus benchmark reports must contain only sanitized IDs, counts, hashes, timings, and status codes.
- No raw journal payloads or private paths are committed.

Implementation plan:

1. Wait for SOW-0081 completion.
2. Finalize Rust API shape from the specification.
3. Implement optimized traversal using FIELD linkage and DATA object identity.
4. Add correctness tests against generated fixtures and legacy-equivalent outputs.
5. Benchmark against the real Netdata plugin on generated and sanitized real-corpus queries.

Validation plan:

- Rust tests for the new API.
- Compatibility tests proving existing Rust reader/facade behavior is unchanged.
- Generated query fixtures covering filters, timeframe, facets, histogram, Top-N, FTS, compressed irrelevant DATA, repeated fields, and DATA reuse.
- Real Netdata plugin comparison benchmark.
- `git diff --check`.
- `.agents/sow/audit.sh`.
- Whole-SOW reviewer pass.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: update if new mandatory explorer validation rules are established.
- Specs: update Rust explorer/query API contract.
- End-user/operator docs: update Rust README/API docs.
- End-user/operator skills: no expected update.
- SOW lifecycle: complete only after implementation, validation, review, and status update.
- SOW-status.md: update with pending/current/completed state.

Open-source reference evidence:

- SOW-0081 must provide Netdata source evidence.
- systemd source evidence may be needed for FIELD/DATA/index behavior.

Open decisions:

- Blocked until SOW-0081 completes and records whether any Netdata-specific policy should remain outside the generic SDK API.

## Implications And Decisions

1. 2026-06-02 replacement decision
   - Decision: implement the optimized legacy-like engine in Rust first.
   - Implication: Go/Node/Python ports and index-derived optimization wait until the Rust API proves the right behavior and baseline performance.

## Plan

1. Consume SOW-0081 specification.
2. Design and implement Rust API.
3. Validate behavior and benchmark against Netdata plugin.
4. Record results and follow-up work for SOW-0083.

## Delegation Plan

Implementer:

- Local implementation unless the user explicitly re-enables external implementers.

Reviewers:

- Reviewer pool after complete implementation and local validation: minimax, kimi, qwen, glm, and mimo.

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

- If benchmark comparison exposes a semantic mismatch, fix semantics before optimizing further.

## Execution Log

### 2026-06-02

- Created pending replacement SOW after SOW-0074 was killed.

## Validation

Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

Pending.

## Regression Log

None yet.
