# SOW-0083 - Index-Derived Facet And Histogram Optimization

## Status

Status: in-progress

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: activated from clean rollback point `42c15858`; evidence-first
prototype before any public API commitment.

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

Status: ready

Problem / root-cause model:

- Optimized traversal fixes the immediate legacy API waste. Further gains may come from avoiding candidate-row traversal entirely for facets and histogram, but only if posting-list/range/intersection work is cheaper for the query shape.
- The current working hypothesis is that index-derived aggregation may not
  provide useful speedups in most real cases and may not justify its
  complexity. The SOW must prove or reject that hypothesis with measurements,
  not assumption.

Evidence reviewed:

- User discussion on 2026-06-02.
- SOW-0082 is required before implementation.
- SOW-0082 completed in commit `42c15858`, which is also pushed to
  `origin/master`.
- User direction on 2026-06-05: try SOW-0083, but ensure all prior work is
  committed and keep rollback possible because the complexity may not pay off.

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

1. Start from the clean pushed rollback point `42c15858`.
2. Build the smallest internal/prototype index-derived implementation needed
   to measure facet and histogram break-even behavior.
3. Keep public API changes behind explicit strategy controls only after the
   prototype proves correctness and useful speed.
4. Add `compare` mode and correctness parity checks before trusting benchmark
   deltas.
5. Run generated and real-corpus benchmark matrices.
6. Implement evidence-based `auto` planner rules only if justified.
7. If index-derived aggregation is usually slower or too complex, remove the
   prototype code, record the evidence, and close the SOW as rejected/retained
   traversal.

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

- None blocking. SOW-0082 is complete. Public API and default planner changes
  are gated on measured evidence inside this SOW.

## Implications And Decisions

1. 2026-06-02 sequencing decision
   - Decision: index-derived facets/histogram are a second optimization phase, not the first implementation.
   - Implication: break-even and auto-planner rules must be measured after the optimized traversal baseline exists.

2. 2026-06-05 evidence-first complexity decision
   - Decision: try the index-derived path, but treat the user's skepticism as
     an explicit acceptance gate. The implementation must be removable if it
     does not provide useful speedups for real query shapes.
   - Evidence: the user stated that SOW-0083 may not provide useful speedup in
     most cases and may not be worth the complexity.
   - Implication: do not commit to public API, auto-planner behavior, or
     retained complex internals until compare-mode correctness and benchmark
     evidence justify them.

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

Acceptance criteria evidence:

- Pending implementation and measurements.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending generated and real-corpus benchmark matrices.

Reviewer findings:

- Pending whole-SOW reviewer pass after local implementation and validation.

Same-failure scan:

- Pending.

Sensitive data gate:

- Durable artifacts must contain only sanitized file IDs, query IDs, feature
  classes, counts, hashes, timings, and status codes.
- Raw journal paths, raw query values, raw payloads, credentials, bearer
  tokens, SNMP communities, customer names, personal data, non-private
  customer-identifying IPs, private endpoints, and proprietary incident details
  must not be written to committed SOWs, specs, docs, skills, or code comments.
- Real-corpus raw data and raw benchmark working files stay under `.local/` and
  are not staged.

Artifact maintenance gate:

- AGENTS.md: pending closeout decision.
- Runtime project skills: pending closeout decision.
- Specs: pending explorer strategy and planner update if retained.
- End-user/operator docs: pending Rust README/API update if retained.
- End-user/operator skills: pending closeout decision.
- SOW lifecycle: active in `.agents/sow/current/`; close only after
  implementation, validation, reviewer disposition, and follow-up mapping.
- SOW-status.md: updated when SOW state changes.

Specs update:

- Pending.

Project skills update:

- Pending.

End-user/operator docs update:

- Pending.

End-user/operator skills update:

- Pending.

Lessons:

- Pending evidence from break-even experiments.

Follow-up mapping:

- Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

Pending.

## Regression Log

None yet.
