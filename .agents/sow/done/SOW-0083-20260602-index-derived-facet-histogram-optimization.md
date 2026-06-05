# SOW-0083 - Index-Derived Facet And Histogram Optimization

## Status

Status: completed

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: completed; final whole-SOW reviewer rerun passed.

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

### 2026-06-05

- Confirmed rollback point before SOW-0083 work:
  - `42c15858` contained the completed optimized traversal explorer.
  - `91819b24` activated this SOW from a clean tree and was pushed before the
    index prototype started.
- Added an evidence-first Rust index prototype:
  - strategy controls: `traversal`, `index`, and `compare`;
  - compare mode runs traversal and index and rejects mismatched logical output;
  - index mode is exact only for `ExplorerFieldMode::AllValues`, no FTS, and
    commit-realtime time semantics;
  - index mode rejects default `FirstValue` because FIELD/DATA posting lists do
    not encode row-local field order, so exact first-value semantics would
    require re-reading row field order and would collapse back toward traversal.
- Prototype evidence:
  - corrected benchmark helper so empty facet lists remain empty and
    histogram-only runs are not polluted by an implicit default facet;
  - benchmark artifacts are under `.local/benchmarks/sow-0083-prototype-*`
    and `.local/benchmarks/sow-0083-filtered-*`; these are not staged.
- Initial unfiltered compare-mode validation passed for:
  - generated 200k-row compact file;
  - local NetFlow 200 MiB real corpus file;
  - one facet, three facets, twenty facets, and one histogram query shapes.
- Initial filtered compare-mode validation passed for generated and NetFlow
  query shapes, including a highly selective generated filter and a broad
  NetFlow filter.

Measured rows/s, single release-build run, snapshot/windowed mmap, all-values,
commit realtime:

| corpus | query shape | traversal rows/s | index rows/s | index / traversal |
| --- | ---: | ---: | ---: | ---: |
| generated compact | 1 facet | 703,453 | 9,548,432 | 13.57x |
| generated compact | 3 facets | 687,853 | 3,280,556 | 4.77x |
| generated compact | 20 facets | 479,053 | 476,242 | 0.99x |
| generated compact | 1 histogram | 689,914 | 5,369,988 | 7.78x |
| NetFlow real | 1 facet | 1,626,796 | 9,550,999 | 5.87x |
| NetFlow real | 3 facets | 1,608,797 | 3,396,577 | 2.11x |
| NetFlow real | 20 facets | 951,922 | 430,160 | 0.45x |
| NetFlow real | 1 histogram | 1,548,383 | 3,023,983 | 1.95x |

Filtered rows/s, single release-build run, snapshot/windowed mmap, all-values,
commit realtime:

| corpus | query shape | traversal rows/s | index rows/s | index / traversal |
| --- | ---: | ---: | ---: | ---: |
| generated compact | selective filter + 3 facets | 668,486 | 236,114 | 0.35x |
| generated compact | highly selective filter + 20 facets | 2,508 | 3 | 0.001x |
| NetFlow real | broad filter + 3 facets | 1,385,049 | 1,361,267 | 0.98x |
| NetFlow real | broad filter + 20 facets | 1,186,358 | 484,270 | 0.41x |

Evidence-backed interim conclusion:

- The index strategy has real upside for low facet counts and histogram-only
  queries when there are no selective filters.
- The index strategy is not a universal replacement for optimized traversal.
- Selective filters can make index aggregation catastrophically slower because
  it still walks all posting-list entries for each requested facet value, while
  traversal walks only candidate rows.
- Twenty-facet real queries are slower with index aggregation even without
  filters, because exact unset counting and per-facet row-set maintenance erase
  the benefit of avoiding unrelated fields.
- Do not add an `auto` planner yet. The measured shape is too branchy and a bad
  planner would create severe latency cliffs.
- First reviewer round fixes:
  - `ExplorerStrategy` is now `#[non_exhaustive]` before becoming a documented
    public enum.
  - `ExplorerStrategy::Compare` now returns `ExplorerComparison` timing and
    counter diagnostics in `ExplorerResult::comparison`.
  - `reader_core_bench` now emits compare diagnostics in `explorer_comparison`.
  - Indexed histogram counting now hoists loop-invariant bucket bounds outside
    the posting-list loop.
  - The compare-mode unit test now asserts exact histogram bucket content and
    returned comparison diagnostics.

## Validation

Acceptance criteria evidence:

- Add index-derived facet counting using FIELD/DATA entry posting lists:
  implemented in `FileReader::explore_with_strategy(..., ExplorerStrategy::Index)`,
  `indexed_count_facet_group()`, `indexed_count_facet_entries()`, and
  `JournalFile::field_data_objects_with_offsets()`.
- Add index-derived histogram generation using FIELD/DATA posting lists and
  ENTRY timestamps: implemented in `indexed_count_histogram()` and
  `indexed_count_histogram_entries()`.
- Preserve optimized traversal as selectable strategy: `FileReader::explore()`
  still delegates to `ExplorerStrategy::Traversal`, and
  `ExplorerStrategy::Traversal` remains the default.
- Add compare mode with timing/counter diagnostics:
  `ExplorerStrategy::Compare` runs traversal and index, rejects logical output
  mismatches, and returns `ExplorerComparison` timing/stat diagnostics.
- Benchmark generated and representative real query shapes: recorded above.
  The accepted evidence is directional single-run evidence, not a final
  production planner calibration.
- Produce break-even report: recorded above. Index is retained only as explicit
  opt-in, no `auto` planner is added.
- Record rejected cases: selective filters and many-facet real queries are
  explicitly recorded as cases where traversal wins or index can regress badly.

Tests or equivalent validation:

- `cargo fmt --check -p journal -p journal-core -p reader_core_bench`: passed.
- `cargo test -p journal explorer_`: passed, 19 tests.
- `cargo test -p journal-core -p reader_core_bench --no-run`: passed.
- New and affected tests cover:
  - compare-mode traversal/index logical equality;
  - exact histogram bucket equality for compare mode;
  - returned comparison diagnostics;
  - same-field filter exclusion;
  - indexed strategy rejection for `FirstValue`.

Real-use evidence:

- Generated 200k-row compact benchmark and local NetFlow real-file benchmark
  compare-mode runs passed with no logical mismatches for the query classes
  recorded above.
- Raw benchmark artifacts remain under `.local/benchmarks/` and are not staged.
- No raw journal paths, payloads, field values, IPs, or private endpoints are
  recorded in durable artifacts.

Reviewer findings:

- First whole-SOW reviewer batch:
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`; noted SOW validation
    placeholders needed closeout.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`; noted SOW
    validation placeholders needed closeout.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`; no code blockers.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`; noted known
    performance/memory tradeoffs and SOW closeout gaps.
  - `llm-netdata-cloud/kimi-k2.6`: `NOT PRODUCTION GRADE` due to three
    blockers: public enum future-compat risk, missing compare timing/counter
    diagnostics, and SOW validation placeholders.
- First-round dispositions:
  - Added `#[non_exhaustive]` to `ExplorerStrategy`.
  - Added `ExplorerComparison` timing/stat diagnostics and exposed them through
    `ExplorerResult::comparison`.
  - Emitted compare diagnostics in the benchmark helper.
  - Filled this validation section.
- Final whole-SOW reviewer rerun:
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`; noted one accepted
    low-risk corrupt-file edge case for direct `Index` mode on invalid
    journals, with `Compare` mode catching mismatches.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.

Same-failure scan:

- Searched the changed API and docs for `ExplorerStrategy`,
  `explore_with_strategy`, `ExplorerStrategy::Compare`, and strategy
  descriptions. Updated Rust README, product scope, and Rust reader performance
  spec to describe compare diagnostics and no-auto-planner behavior.
- The known pre-existing clippy warning in `journal-registry` is outside this
  SOW's changed surface and is not introduced by SOW-0083.

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

- AGENTS.md: no update needed. The project-wide performance contract already
  covers explicit evidence before planner/default decisions.
- Runtime project skills: no update needed. Existing journal compatibility and
  orchestration skills already cover performance evidence and whole-SOW review.
- Specs: updated `product-scope.md` and `rust-reader-performance.md` with
  explicit traversal/index/compare strategy contracts.
- End-user/operator docs: updated `rust/README.md` with
  `explore_with_strategy()` usage, limitations, and compare diagnostics.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: moved to `.agents/sow/done/` with `Status: completed` after
  final reviewer rerun and clean local validation.
- SOW-status.md: updated for completed SOW-0083 and pending SOW-0093.

Specs update:

- Updated `.agents/sow/specs/product-scope.md`.
- Updated `.agents/sow/specs/rust-reader-performance.md`.

Project skills update:

- No project skill update needed; this SOW did not change how agents must work
  here.

End-user/operator docs update:

- Updated `rust/README.md`.

End-user/operator skills update:

- No end-user/operator skill exists or is affected.

Lessons:

- Index-derived aggregation is not inherently superior. It is a strong win for
  narrow unfiltered all-values facets and histogram-only queries, but a poor
  fit for selective filters and many-facet real queries.
- A wrong auto planner would create severe latency cliffs. The correct product
  outcome for this SOW is explicit strategy controls plus compare diagnostics,
  not automatic strategy selection.
- Function-boundary Netdata evidence is required before deciding whether any
  strategy should be selected by default for real plugin integration.

Follow-up mapping:

- SOW-0093 tracks the next evidence gate: Netdata function-boundary output
  equality and performance comparison against `systemd-journal.plugin` and
  `netflow.plugin` command-line test entrypoints.

## Outcome

Implemented explicit Rust explorer strategy controls:

- `Traversal`: default and unchanged behavior for `FileReader::explore()`.
- `Index`: opt-in all-values, no-FTS, commit-realtime strategy using FIELD/DATA
  chains and DATA entry posting lists.
- `Compare`: validation strategy that runs traversal and index, rejects
  mismatches, and returns timing/counter diagnostics.

No automatic planner was added. The measured evidence does not justify it.

## Lessons Extracted

- Keep index strategy explicit until real Netdata function-boundary measurements
  prove query-shape planner rules.
- Do not expose public strategy enums without future-compatibility protection.
- Compare/diagnostic APIs must expose enough data to justify performance
  decisions, not only pass/fail correctness.

## Followup

- SOW-0093 - Netdata Function Boundary Reader Comparison.

## Regression Log

None yet.
