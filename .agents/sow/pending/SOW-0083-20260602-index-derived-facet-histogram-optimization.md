# SOW-0083 - Index-Derived Facet And Histogram Optimization

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: pending; depends on SOW-0082.

## Requirements

### Purpose

Measure and implement optional index-derived facet and histogram strategies on top of the optimized Rust explorer API, using real query shapes and the local large journal corpus to determine when index aggregation beats optimized traversal.

### User Request

The user asked to optimize the Rust implementation further by using indexes for histograms and facet counts after first recreating the legacy behavior in an optimal traversal engine.

### Assistant Understanding

Facts:

- The query always includes filters and timeframe for all outputs.
- Facet and histogram index strategies can avoid row-field traversal but still need time selection.
- Histogram requires timestamp bucketing for matching occurrences.
- Facets require timeframe-aware counts and filter-aware intersections.
- The break-even between optimized traversal and index-derived aggregation depends on facet count, histogram field, filter selectivity, cardinality, field presence, DATA reuse, compression, and locality.

Inferences:

- Both engines must exist before the break-even can be measured.
- The API should expose `auto`, `traversal`, `index`, and `compare` strategy modes or equivalent diagnostics.
- Real-corpus experiments are required because generated 32-field corpora may not capture production locality and cardinality.

Unknowns:

- Break-even thresholds are not known and must be measured.
- Whether index-derived aggregation should be enabled by default depends on measured correctness and performance.

### Acceptance Criteria

- Add index-derived facet counting for exact filters plus timeframe using FIELD/DATA entry posting lists.
- Add index-derived histogram generation for one selected field using FIELD/DATA posting lists and ENTRY timestamps.
- Preserve the SOW-0082 optimized traversal engine as a selectable strategy.
- Add `compare` mode that runs traversal and index strategies, verifies identical logical outputs, and reports timing/counter deltas.
- Benchmark generated query matrices varying facet count, histogram field cardinality, filter selectivity, compression, repeated fields, and DATA reuse.
- Benchmark representative real queries on the local large journal corpus using sanitized reports only.
- Produce a break-even report describing when traversal wins, when index aggregation wins, and recommended planner rules.
- Implement `auto` planner rules only after benchmark evidence supports them.
- Record any cases where index aggregation is rejected as too complex or not worth maintaining.

## Analysis

Sources checked:

- SOW-0082 will provide the optimized traversal baseline.
- systemd source evidence for DATA entry arrays and bisection will be needed during implementation.

Current state:

- Index-derived aggregation is intentionally out of scope until SOW-0082 creates a correct optimized traversal baseline.

Risks:

- Index aggregation can become random-access heavy and slower than selected-field traversal.
- Posting-list intersections can be memory-heavy on broad queries.
- High-cardinality fields can make complete facet/histogram output itself expensive.
- Real-corpus reports can leak sensitive data if not sanitized.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Optimized traversal fixes the immediate legacy API waste. Further gains may come from avoiding candidate-row traversal entirely for facets and histogram, but only if posting-list/range/intersection work is cheaper for the query shape.

Evidence reviewed:

- User discussion on 2026-06-02.
- SOW-0082 is required before implementation.

Affected contracts and surfaces:

- Rust explorer API strategy selection.
- Benchmark reports.
- Future Go port and Netdata integration planning.

Existing patterns to reuse:

- SOW-0082 optimized traversal engine.
- Existing corpus evaluation sanitized reporting.
- systemd DATA entry-array bisection model.

Risk and blast radius:

- Medium API risk if strategy controls become public.
- High performance risk if auto-planner thresholds are guessed instead of measured.

Sensitive data handling plan:

- Real-corpus benchmark reports must use sanitized file IDs, query IDs, feature classes, counts, hashes, timings, and status codes only.
- Raw journals and raw query values stay under `.local/` and are not staged.

Implementation plan:

1. Wait for SOW-0082 completion.
2. Implement index-derived facet/histogram strategies behind explicit strategy controls.
3. Add `compare` mode and correctness parity checks.
4. Run generated and real-corpus benchmark matrices.
5. Implement evidence-based `auto` planner rules if justified.

Validation plan:

- Rust tests for index-derived facets and histogram.
- Compare-mode logical equality tests between traversal and index strategies.
- Generated benchmark matrix.
- Sanitized real-corpus benchmark matrix.
- `git diff --check`.
- `.agents/sow/audit.sh`.
- Whole-SOW reviewer pass.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: update if index/traversal benchmark evidence becomes mandatory for explorer changes.
- Specs: update explorer strategy contract and planner rules.
- End-user/operator docs: update Rust README/API docs.
- End-user/operator skills: no expected update.
- SOW lifecycle: complete only after measured evidence and reviewer pass.
- SOW-status.md: update with pending/current/completed state.

Open-source reference evidence:

- systemd source evidence for DATA entry-array bisection and time movement must be recorded during implementation.

Open decisions:

- Blocked until SOW-0082 completes.

## Implications And Decisions

1. 2026-06-02 sequencing decision
   - Decision: index-derived facets/histogram are a second optimization phase, not the first implementation.
   - Implication: break-even and auto-planner rules must be measured after the optimized traversal baseline exists.

## Plan

1. Implement explicit index strategies.
2. Compare against optimized traversal.
3. Measure break-even on generated and real corpus queries.
4. Add auto-planner only if evidence supports it.

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

- If index-derived aggregation does not beat optimized traversal for meaningful query classes, record the evidence and do not keep unnecessary code.

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
