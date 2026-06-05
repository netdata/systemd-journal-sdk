# SOW-0082 - Rust Optimized Journal Explorer API

## Status

Status: completed

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: regression repaired, reviewed, and re-closed. SOW-0093 real
function-boundary testing found that Netdata-shaped Explorer queries scanned
candidate rows once for main/histogram work and again for facet work, and that
the SDK wrapper used about 4 GiB maximum RSS on a 4 GiB journal-window query.
The repaired normal path now uses one candidate-row traversal and final-row-only
payload expansion; final validation measured 83,876 KiB maximum RSS for the
same request.

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
- The implementation should use candidate-row DATA object identity to avoid
  reprocessing repeated `FIELD=value` DATA objects. A prior FIELD-chain
  preclassification design helped low-facet queries but regressed high-facet
  queries and was rejected.
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
- Compressed DATA outside required fields is not decompressed on offset-cache
  hits or after early-stop skips the remaining row. A lazy first encounter may
  need to decode one DATA object to discover whether it is relevant because the
  journal DATA object does not store the field name separately from the payload.
- Reusable DATA objects are classified and value-processed once per traversal
  pass where correctness permits.
- Traversal stops once all required fields for the row are satisfied under the
  explorer's default first-value field semantics. Exact duplicate-value
  accounting remains available only through an explicit slower `AllValues`
  mode.
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

Status: ready

Problem / root-cause model:

- The legacy libsystemd-style traversal shape forces callers to enumerate every current-row payload. This causes unnecessary decompression, unnecessary field traversal, and repeated processing of deduplicated DATA objects.
- The SDK needs a Rust API that preserves the legacy result semantics while changing the internal work model.

Evidence reviewed:

- User request on 2026-06-02.
- Completed SOW-0081 and
  `.agents/sow/specs/systemd-journal-plugin-facets.md`.

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
3. Implement optimized traversal using lazy DATA-offset classification from
   candidate rows and DATA object identity.
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

- `ktsaou/netdata @ b695fa41f8ef` via SOW-0081:
  `src/collectors/systemd-journal.plugin/systemd-journal.c`,
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h`,
  `src/collectors/systemd-journal.plugin/systemd-journal-function.h`,
  `src/collectors/systemd-journal.plugin/systemd-journal-sampling.h`,
  `src/collectors/systemd-journal.plugin/systemd-internals.h`,
  `src/collectors/systemd-journal.plugin/systemd-journal-files.c`,
  `src/collectors/systemd-journal.plugin/systemd-main.c`,
  `src/libnetdata/facets/facets.c`,
  `src/libnetdata/facets/facets.h`,
  `src/libnetdata/facets/logs_query_status.h`, and
  `src/libnetdata/facets/README.md`.
- `netdata/netdata @ 5d611c4ce8c2` via SOW-0081 for byte-identical core
  facets files.
- systemd source evidence may be needed for FIELD/DATA/index behavior.

Open decisions:

- SOW-0081 records the working split: generic SDK explorer semantics belong in
  this API; Netdata-specific JSON envelope, synthetic fields, default facet
  policy, severity labels, source taxonomy, pluginsd progress, fstat counters,
  and GET hash-ID compatibility belong in an adapter layer.

## Implications And Decisions

1. 2026-06-02 replacement decision
   - Decision: implement the optimized legacy-like engine in Rust first.
   - Implication: Go/Node/Python ports and index-derived optimization wait until the Rust API proves the right behavior and baseline performance.

2. 2026-06-05 multi-facet design correction
   - Decision: reject the global FIELD-chain preclassification path for this
     API and use lazy candidate-row DATA-offset classification.
   - Evidence: 20-facet benchmark on the generated 128 MiB compact file showed
     the implemented explorer slower than the old full traversal:
     `.local/benchmarks/sow-0082-20-facets/report-low-medium.json`.
     The old traversal measured 455,783 rows/s, explorer `AllValues` measured
     136,254 rows/s, and explorer `FirstValue` measured 225,240 rows/s.
   - Implication: the hot path must not pre-walk all values of requested
     fields before knowing the candidate rows. The offset cache is populated
     while traversing candidate rows and only from DATA objects encountered in
     those rows.
   - Design rules:
     - The cache key is the DATA object offset; no payload-byte hashing is used
       in the row hot path.
     - Cache values are compact classifications: irrelevant, FTS-only match, or
       a FACET_VALUE index.
     - FACET and FACET_VALUE hot state must be compact arrays. Cold labels,
       output maps, and owned value bytes are built only on first
       classification or finalization.
     - FACET tracks the last row id where the field was seen. A row starts with
       `fields_missing_from_row = required field identities`, and duplicate
       values for the same field in the same row do not decrement it again.
     - FACET_VALUE owns counters and optional histogram buckets. Histogram is
       attached to FACET_VALUE, not processed through a separate per-hit target
       loop.
     - The row loop uses local scalar state. It allocates only for cache growth
       or for decompressing/copying a newly classified compressed/selected DATA
       object.
     - If FTS or source-time filtering can reject a row after fields were seen,
       counter application is deferred through reusable scratch storage. The
       facet-only fast path applies counters immediately.
     - `AllValues` scans the whole row for repeated-field correctness.
       `FirstValue` may break the inner DATA-ref loop when every required field
       identity has been seen for the row.

3. 2026-06-05 lazy-classification benchmark finding
   - Resolution: the user accepted default first-value row semantics in
     Decision 4, so this `AllValues` tradeoff is no longer the production
     explorer default.
   - Evidence: `.local/benchmarks/sow-0082-lazy-offset/report.json`.
   - Generated 128 MiB compact file:
     - 1 facet: old full traversal 1,127,998 rows/s; lazy `AllValues`
       909,145 rows/s; lazy `FirstValue` 6,953,460 rows/s.
     - 3 facets: old full traversal 975,336 rows/s; lazy `AllValues`
       926,802 rows/s; lazy `FirstValue` 6,181,033 rows/s.
     - 20 facets: old full traversal 506,203 rows/s; lazy `AllValues`
       689,883 rows/s; lazy `FirstValue` 2,742,325 rows/s.
   - Root cause: `AllValues` cannot stop early because journal entries may
     contain multiple values for the same selected field. With pure lazy
     classification, the row loop must inspect every DATA ref and classify every
     first-seen irrelevant high-cardinality DATA object. On the generated file
     that means 816,580 cache misses and DATA payload reads even for a one-facet
     query.
   - Compatibility implication: if an irrelevant DATA object is compressed,
     pure lazy classification may need to decompress it once to discover its
     field name. Therefore pure lazy classification cannot fully satisfy
     "compressed DATA outside required fields is not decompressed" for
     `AllValues`; only early stop or an indexed positive-offset strategy can
     satisfy that requirement.
   - Current local code state: lazy classification is retained. The production
     default uses first-value early stop; explicit `AllValues` remains the
     slower exact duplicate-value mode.

4. 2026-06-05 first-value explorer semantics decision
   - User decision: use first-value row semantics as the explorer default.
   - Context: journal ENTRY objects can reference multiple DATA objects for the
     same field in one row. This is valid in the file format, but uncommon for
     log-explorer facet and histogram behavior.
   - Contract: for selected facet/histogram/source fields, one row contributes
     at most the first encountered value for that field. Later duplicate values
     for the same field in the same row are ignored by the default explorer
     mode.
   - Early-stop guard: duplicate values for a field must not reduce
     `fields_missing_from_row`. Each FACET identity tracks the last processed
     row id, and only the first value for that field in that row can satisfy the
     missing-field condition. This prevents premature inner-loop stop when a
     row contains `FIELD=a`, `FIELD=b`, and another required field appears
     later.
   - Implication: default explorer accounting is accurate for row-level
     first-value semantics, not exact duplicate-value enumeration.
   - Performance implication: the row loop may stop as soon as all requested
     field identities are seen. This avoids inspecting unrelated trailing DATA
     references and avoids decompressing compressed DATA skipped by early stop.
   - Compatibility escape hatch: `ExplorerFieldMode::AllValues` remains
     available for callers that need exact duplicate-value accounting and
     accept the slower full-row scan.

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

### 2026-06-05

- Moved to current after SOW-0081 completed and published the
  `systemd-journal.plugin` plus facets behavior specification.
- Added the Rust `FileReader::explore()` API with `ExplorerQuery`,
  `ExplorerFilter`, `ExplorerAnchor`, `ExplorerFieldMode`, `ExplorerResult`,
  `ExplorerRow`, `ExplorerHistogram`, and `ExplorerStats`.
- Implemented exact positive filter slicing through existing Rust reader
  matches. Repeated values for one field are OR alternatives; different fields
  are ANDed by the existing match builder.
- Implemented lazy per-pass DATA classification keyed by DATA object offset.
  The cache stores compact offset classes for irrelevant DATA, FTS-only matches,
  and selected FACET_VALUE objects. Required DATA labels are copied only on
  first classification and are reused for final output materialization.
- Added grouped facet traversal: facets that share the same effective filter
  set are counted in one pass, so multi-facet queries do not multiply row scans
  when no same-field filter exclusion forces separate semantics.
- Added `ExplorerAnchor::Auto` as the default anchor policy. Forward scans use
  the lower time bound or file head; backward scans use the upper time bound or
  file tail. This fixed a backward-query regression found during plugin-shaped
  benchmarking.
- Rewrote backward commit-time stop logic as `commit < after - slack` and added
  a regression test for realistic backward time-bounded scans stopping at the
  safe lower-bound slack window.
- Added `ExplorerFieldMode::FirstValue` as the default explorer accounting
  mode after the user accepted row-level first-value semantics. Added
  `ExplorerFieldMode::AllValues` as the explicit slower mode for exact
  duplicate-value accounting.
- Replaced the precomputed FIELD-chain classifier with lazy candidate-row
  DATA-offset classification. The default first-value path uses per-field row
  ids to prevent duplicate values for one field from satisfying another
  required field identity.
- Renamed the early-stop completion count to `required_identity_count` and
  added a regression test for a single DATA object satisfying multiple
  requested roles, removing ambiguity found during final review.
- Added reviewer-driven hardening tests for same-field filter exclusion,
  explicit `FirstValue` semantics, and empty result shape.
- Extended `reader_core_bench` with `sdk-facet-scan` and `explorer-query`
  modes plus explorer facets, filters, histogram, FTS, timeframe, field-mode,
  and source-realtime options. The benchmark CLI defaults to first-value
  explorer semantics, matching `ExplorerQuery::default()`.
- Updated Rust README, product scope, and Rust reader performance specs.

Performance evidence:

- Current generated-fixture report:
  `.local/benchmarks/sow-0082-first-value-default/report.json`.
- Current NetFlow real-field report:
  `.local/benchmarks/sow-0082-first-value-default-netflow-real-fields/report.json`.
- Historical all-values-default and FIELD-chain reports retained for audit but
  superseded:
  `.local/benchmarks/sow-0082-current-v5/report.json` and
  `.local/benchmarks/sow-0082-lazy-offset/report.json`.
- The current benchmark used large files only:
  - `generated-compact-128m`: 100,000 rows, 128 MiB generated compact journal,
    generated fields.
  - `netdata-flow-snapshot-80m`: 44,532 rows, 80 MiB NetFlow-shaped snapshot,
    real NetFlow fields from the file.
  - `netdata-flow-live-200m`: 114,552 rows, 200 MiB NetFlow-shaped live file
    snapshot label, real NetFlow fields from the file.
- `sdk-facet-scan` is the old API baseline: full payload traversal plus facet
  counting in the harness.
- `explorer-query` now uses the default
  `ExplorerFieldMode::FirstValue` semantics unless the caller passes
  `--explorer-field-mode all-values`.

Generated compact fixture:

| Case | Old traversal | Explorer default first-value | Explicit all-values |
| --- | ---: | ---: | ---: |
| `generated-compact-128m`, 1 facet | 942,825 rows/s | 5,336,262 rows/s (5.66x) | 849,768 rows/s (0.90x) |
| `generated-compact-128m`, 3 facets | 974,879 rows/s | 6,056,332 rows/s (6.21x) | 795,548 rows/s (0.82x) |
| `generated-compact-128m`, 20 facets | 472,415 rows/s | 2,394,752 rows/s (5.07x) | 661,608 rows/s (1.40x) |

Generated fixture counters:

- 1 facet default first-value: 500,000 DATA refs seen, 20 DATA cache misses,
  400,000 refs skipped, 100,000 early stops, 100,000 facet updates.
- 3 facets default first-value: 600,000 DATA refs seen, 36 DATA cache misses,
  300,000 refs skipped, 100,000 early stops, 300,000 facet updates.
- 20 facets default first-value: 2,400,000 DATA refs seen, 16,580 DATA cache
  misses, 400,000 refs skipped, 100,000 early stops, 2,000,000 facet updates.

Real NetFlow-field fixture:

| Case | Old traversal | Explorer default first-value | Explicit all-values |
| --- | ---: | ---: | ---: |
| `netdata-flow-snapshot-80m`, 1 facet | 758,456 rows/s | 4,030,636 rows/s (5.31x) | 1,911,440 rows/s (2.52x) |
| `netdata-flow-snapshot-80m`, 3 facets | 706,449 rows/s | 2,390,752 rows/s (3.38x) | 1,702,606 rows/s (2.41x) |
| `netdata-flow-snapshot-80m`, 20 facets | 406,986 rows/s | 1,139,555 rows/s (2.80x) | 1,119,107 rows/s (2.75x) |
| `netdata-flow-live-200m`, 1 facet | 740,362 rows/s | 4,120,961 rows/s (5.57x) | 1,901,906 rows/s (2.57x) |
| `netdata-flow-live-200m`, 3 facets | 666,454 rows/s | 2,317,189 rows/s (3.48x) | 1,793,750 rows/s (2.69x) |
| `netdata-flow-live-200m`, 20 facets | 385,837 rows/s | 1,175,828 rows/s (3.05x) | 1,080,887 rows/s (2.80x) |

Benchmark interpretation:

- First-value default is the production explorer hot path and is consistently
  faster than old full traversal in generated and real-field NetFlow cases.
- Explicit all-values remains available and is intentionally slower on low
  facet counts because it must scan the full row for duplicate-value
  correctness.
- The first NetFlow benchmark in
  `.local/benchmarks/sow-0082-first-value-default/report.json` reused generated
  field names and mainly measured unset facets for NetFlow files; it is kept as
  a diagnostic but not used for real-field conclusions.

Real Netdata plugin comparison:

- Direct plugin execution would probe the workstation live journal because
  Netdata registers `/run/log/journal` and `/var/log/journal` before
  host-prefix paths. To satisfy the project no-live-journal rule, the benchmark
  ran `systemd-journal.plugin` inside `bwrap` with `/var/log/journal` bound to a
  repo-local fixture tree and `/run/log/journal` as an empty tmpfs.
- Plugin smoke reports:
  `.local/benchmarks/sow-0082-netdata-plugin-smoke/summary-window.json` and
  `.local/benchmarks/sow-0082-netdata-plugin-smoke/summary-flow.json`.
- Comparable SDK report:
  `.local/benchmarks/sow-0082-plugin-comparable-first-value-default/report.json`.
- Note: the Netdata plugin rows/sec values are reused from the earlier plugin
  smoke reports because the plugin binary and fixture were unchanged; the SDK
  side was rerun after first-value became the public default.

| Case | Real Netdata plugin full query | SDK explorer comparable logical rows | SDK speedup |
| --- | ---: | ---: | ---: |
| `generated-compact-128m` | 150,031 rows/s | 3,436,136 rows/s | 22.90x |
| `netdata-flow-snapshot-80m` | 294,325 rows/s | 1,507,888 rows/s | 5.12x |

Notes:

- The comparable SDK query used a plugin-like shape: one facet, matching
  histogram field, backward direction, 200 returned rows, and an explicit full
  timeframe.
- The SDK explorer currently performs two optimized passes for that shape: one
  main pass for histogram/returned rows and one facet pass for faceted-search
  count semantics. The table therefore reports logical file rows per second,
  while `ExplorerStats.rows_examined` records the internal pass count.
- Default first-value mode early-stopped 200,000 row scans on the generated
  fixture and 89,064 row scans on the NetFlow snapshot across the two-pass
  plugin-shaped query.
- SOW-0083 remains the follow-up for index-derived facet and histogram
  strategies that may reduce or remove the remaining traversal passes.

### 2026-06-02

- Created pending replacement SOW after SOW-0074 was killed.

## Validation

Local validation passed:

- `cargo fmt --manifest-path rust/Cargo.toml -p journal -p reader_core_bench`.
- `cargo test --manifest-path rust/Cargo.toml -p journal explorer --target-dir .local/cargo-target`: 16 tests passed.
- `cargo build --release --manifest-path rust/Cargo.toml -p reader_core_bench --target-dir .local/cargo-target`.
- `cargo test --manifest-path rust/Cargo.toml -p journal --target-dir .local/cargo-target`: 45 tests passed.
- `cargo test --manifest-path rust/Cargo.toml -p reader_core_bench --target-dir .local/cargo-target`: 0 tests, build passed.

Correctness coverage added:

- OR values within one filter field and AND across fields.
- Unrelated compressed DATA is skipped for facet-only queries.
- Reusable DATA objects are cached by DATA offset.
- Facets sharing the same effective filter set are grouped into one row pass.
- Same-field filter exclusion counts values outside the selected value set while
  preserving other active filters.
- Default `ExplorerFieldMode::FirstValue` counts one value per selected field
  and records early stops under the row-level first-value explorer contract.
- `ExplorerFieldMode::FirstValue` tracks required field identities, not DATA
  offsets, so repeated values for one selected field cannot stop scanning before
  another required selected field appears.
- `ExplorerFieldMode::FirstValue` applies all roles for one DATA object before
  stopping when the same DATA object satisfies multiple requested identities.
- Default `ExplorerFieldMode::FirstValue` does not double-count duplicate
  values for selected facet or histogram fields; explicit
  `ExplorerFieldMode::AllValues` counts them.
- Duplicate facet fields are rejected because the result map is keyed by field
  name and cannot represent duplicate output columns safely.
- Empty result sets keep requested facet keys with empty value maps.
- Facet-only time-bounded queries with `use_source_realtime = false` do not
  count rows that were reached only because of the commit-time slack window and
  later rejected by the exact timestamp range.
- Backward queries with the default `ExplorerAnchor::Auto` scan from tail.
- Backward time-bounded queries stop at the safe lower-bound slack window.
- Histogram and FTS are opt-in.
- FTS disables first-value early stop, so later row fields can still determine
  row eligibility.

- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed; verdict was clean.

Pending validation:

- Whole-SOW external read-only reviewer rerun after the first-value default,
  lazy offset classification, duplicate-value regression tests, final benchmark
  update, local validation rerun, and spec-wording fixes.

Reviewer findings:

- First whole-SOW read-only reviewer pass:
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`; noted non-blocking
    concerns around owned explorer cache wording, optional FTS complexity,
    and small hot-path micro-optimizations.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`; noted non-blocking
    concerns around duplicate DATA compressed checks and additional
    filter-exclusion tests.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`; noted
    non-blocking concerns only.
  - `llm-netdata-cloud/kimi-k2.6`: `NOT PRODUCTION GRADE`; requested clearer
    backward stop logic plus a regression test, and spec correction because the
    explorer cache owns required values instead of exposing mmap-borrowed
    result data.
  - `llm-netdata-cloud/mimo-v2.5-pro`: first session output was unavailable
    after process cleanup; rerun required.
- Dispositions:
  - Backward stop logic: fixed by rewriting the condition as
    `commit_realtime < after_realtime_usec - realtime_slack_usec` with
    saturating subtraction. The specific `after < slack` example is not a safe
    early-stop case because the slack window intentionally extends to timestamp
    zero, but the rewritten condition removes the ambiguity and matches the
    intended threshold model.
  - Regression test: added
    `explorer_backward_time_bound_stops_after_slack_window`, which proves a
    backward scan examines only the rows inside the safe lower-bound slack
    window.
  - Spec wording: updated product and Rust reader performance specs to describe
    the actual per-pass owned cache for required returned values.
  - Non-blocking same-field filter-exclusion, `FirstValue`, and empty-result
    test-depth notes were addressed with focused tests.
  - Duplicate facet fields are rejected explicitly because the public result
    shape cannot represent duplicate facet columns.
  - A later reviewer found that `ExplorerFieldMode::FirstValue` originally
    tracked DATA offsets rather than required field identities. That was a real
    blocker because one repeated field could prevent another selected field from
    being counted. The implementation now tracks `SourceRealtime`, `Histogram`,
    and individual facet identities separately, and regression tests cover the
    case.
  - An earlier read-only reviewer batch then produced five `PRODUCTION GRADE`
    votes. Kimi and mimo independently identified a non-blocking but real
    hot-path waste: `ExplorerFieldMode::AllValues` maintained row-level
    tracking state that only `FirstValue` needs for missing-field early stop.
    Public facet fields still need row-id marking for unset-value accounting,
    but the first-value missing-field counter is now active only in
    `FirstValue` mode.
  - A later final reviewer pass raised a `NOT PRODUCTION GRADE` concern that
    the early-stop condition compared role bits with a field count. The code
    was already counting requested identities rather than distinct field names,
    but the name was misleading. The count is now named
    `required_identity_count`, and
    `explorer_first_value_stops_after_same_data_satisfies_multiple_roles`
    proves a single DATA object can satisfy multiple roles before early stop.
  - Non-blocking FTS, empty-pattern, query-wide cache sharing, and
    micro-optimization notes remain mapped to later optimization work unless
    SOW-0083 evidence changes their priority.
- 2026-06-05 reviewer pass after the first-value default change:
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`; found non-blocking
    `AllValues` missing-field tracking waste and related test-gap notes.
  - `llm-netdata-cloud/glm-5.1`: `NOT PRODUCTION GRADE`; found a real
    facet-pass correctness bug where `ScanApply::Immediate` could apply facet
    counters before exact timestamp-range rejection when
    `use_source_realtime = false`, time bounds were set, and no FTS pattern was
    present.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`; verified the
    first-value duplicate-field contract and did not find blockers.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`; independently found
    the same timestamp-range overcount as a medium finding.
  - `llm-netdata-cloud/kimi-k2.6`: interrupted as stale after it produced no
    final vote for more than 13 minutes and the timestamp-range blocker had
    already been confirmed by other reviewers.
- Disposition for the 2026-06-05 reviewer pass:
  - The timestamp-range blocker was fixed by deferring facet counter
    application whenever any exact post-scan rejection is possible: time bounds
    or FTS. The focused regression
    `explorer_facet_time_bounds_do_not_count_slack_rows_without_source_realtime`
    proves slack-window rows are not counted after exact timestamp rejection.
  - The first-value early-stop and FTS interaction now has focused coverage in
    `explorer_fts_disables_first_value_early_stop`.
  - The `AllValues` hot-path note was addressed for the missing-field counter:
    `fields_missing_from_row` is initialized and decremented only in
    `FirstValue`. Row-id marking remains for public facet fields because
    `finish_facet_row()` uses it to account unset facet values.
  - Local validation was rerun after the fixes: 16 explorer tests, 45 total
    `journal` tests, the `reader_core_bench` build test, `git diff --check`,
    and `.agents/sow/audit.sh` all passed.
- Final whole-SOW reviewer rerun after the timestamp, FTS, and first-value
  identity fixes:
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`. Found only low-priority
    symmetry/edge-case test gaps: forward time-bound stop, source-realtime
    time-bound facet defer, empty FTS patterns, and empty filter values.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`. Found only
    low-priority combined-path test gaps such as backward FTS and AllValues
    with irrelevant compressed DATA.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`. Found no blockers after
    the deferred facet-counter fix.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`. Verified the
    first-value duplicate-field contract and found no blockers.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`. Found no blockers
    after rerunning directory-independent Rust explorer review.
- Final reviewer disposition:
  - No final reviewer found a correctness, safety, compatibility, or scope
    blocker.
  - The remaining findings are low-priority test-depth improvements, not
    contract defects. They are covered by existing symmetric code paths or by
    later SOW-0083 optimization/coverage work.
  - The user clarified that the key first-value requirement is to avoid
    premature inner-loop stop when a row contains multiple values for one
    selected field and another selected field appears later. The implementation
    satisfies this by tracking the last row id per required field identity:
    duplicate same-field values do not decrement `fields_missing_from_row`, do
    not update facets or histograms in default `FirstValue` mode, and do not
    stop the DATA loop until every required identity has appeared.
- Final closeout validation after moving the SOW to `done/`:
  - `git diff --check`: passed.
  - `.agents/sow/audit.sh`: passed; SOW status and directory consistency are
    clean.
  - `cargo test --manifest-path rust/Cargo.toml -p journal explorer
    --target-dir .local/cargo-target`: passed; 16 focused explorer tests.
  - `cargo test --manifest-path rust/Cargo.toml -p journal --target-dir
    .local/cargo-target`: passed; 45 total `journal` tests.
  - `cargo fmt --manifest-path rust/Cargo.toml -p journal -p
    reader_core_bench -- --check`: passed.
  - `cargo build --release --manifest-path rust/Cargo.toml -p
    reader_core_bench --target-dir .local/cargo-target`: passed.

Same-failure scan:

- `git status --short` and `git diff --stat` show changes scoped to this SOW:
  Rust explorer API, Rust benchmark helper, Rust README, product/performance
  specs, SOW status, and this SOW. No Go, Python, Node.js, writer, or Netdata
  source files were changed.
- Backward-direction explorer behavior now has a focused regression test.
- Multi-facet pass multiplication now has a focused regression test.
- `FirstValue` field-identity behavior and duplicate facet rejection now have
  focused regression tests.
- `FirstValue` multi-role same-DATA early-stop behavior now has a focused
  regression test.

Sensitive data gate:

- Durable artifacts contain only sanitized benchmark labels, aggregate counts,
  benchmark rates, and repository-local report paths.
- Raw journal payloads, credentials, bearer tokens, SNMP communities, customer
  names, personal data, non-private customer-identifying IPs, private endpoints,
  and proprietary incident details were not written to durable artifacts.
- `.local/` benchmark reports are scratch artifacts and are not committed.

Artifact maintenance gate:

- AGENTS.md: no update needed. Existing project performance and runtime-purity
  contracts already cover this work.
- Runtime project skills: no update needed. The implementation does not change
  reusable agent workflow.
- Specs: updated `.agents/sow/specs/product-scope.md` and
  `.agents/sow/specs/rust-reader-performance.md`.
- End-user/operator docs: updated `rust/README.md` for the new public Rust API.
- End-user/operator skills: no output/reference skills were affected.
- SOW lifecycle: SOW is `Status: completed` in `.agents/sow/done/` after the
  regression repair reviewer pass and final closeout.
- SOW-status.md: updated `.agents/sow/SOW-status.md`.

Specs update:

- Updated `.agents/sow/specs/product-scope.md`.
- Updated `.agents/sow/specs/rust-reader-performance.md`.

Project skills update:

- No project skill update needed. The new durable rule is Rust API behavior and
  performance contract content, not a reusable operator workflow.

End-user/operator docs update:

- Updated `rust/README.md` with `FileReader::explore()`,
  `ExplorerAnchor::Auto`, grouped facet traversal, and field-mode guidance.

End-user/operator skills update:

- No output/reference skills were affected.

## Outcome

Completed.

Rust now exposes the additive `FileReader::explore()` API for the optimized
single-file explorer query model. The implementation uses indexed filters,
lazy DATA-offset classification, grouped facet passes, first-value row-level
field identity tracking, compressed-DATA avoidance for irrelevant fields, and
full-row expansion only for returned rows. Default `ExplorerFieldMode::FirstValue`
counts at most one value per selected field per row, while explicit
`ExplorerFieldMode::AllValues` preserves exact duplicate-value accounting at
the expected slower cost.

Final benchmark evidence shows the default explorer path is materially faster
than the old full traversal and the isolated real Netdata plugin comparison
for the measured generated and real NetFlow files. The exact benchmark reports
remain under `.local/benchmarks/`; durable artifacts record only sanitized
rates and aggregate counters.

## Lessons Extracted

- Benchmarking backward direction exposed an API default bug that forward-only
  tests did not catch. Explorer validation must always include both directions.
- Multi-facet performance needs explicit tests and counters. A one-facet
  benchmark would not catch accidental pass multiplication.
- Real plugin comparison is practical without touching the live host journal
  when `bwrap` is available and the mounted journal directories are
  repo-local.

## Followup

- SOW-0083 remains required for index-derived facet and histogram optimization.
- Directory-level explorer APIs are not implemented in this SOW and remain part
  of later Netdata reader integration planning.

## Regression Log

### Regression - 2026-06-06

Status: completed after repair.

What broke:

- The Explorer implementation violates the SOW-0082 purpose for Netdata-shaped
  queries by performing two candidate-row traversal passes: one main pass for
  returned rows plus histogram and one facet pass for facet counters.
- The same query shape also showed unacceptable memory use at the function
  boundary. The SDK wrapper reached about 4 GiB maximum RSS while producing a
  response comparable to the installed plugin, which reached about 750 MiB
  maximum RSS on the same request before the plugin's high-cardinality default
  facet issue was corrected externally.

Evidence:

- Code path:
  - `rust/src/journal/src/explorer.rs:609` starts the main pass.
  - `rust/src/journal/src/explorer.rs:616` starts separate facet pass groups.
  - `rust/src/journal/src/explorer.rs:869` increments `rows_examined` for each
    row-scan operation.
- SOW-0093 4 GiB real-corpus request:
  - request path:
    `.local/sow-0093/big-default-facets/request-default-facets-4g.json`;
  - report path:
    `.local/sow-0093/big-default-facets/sdk-vs-plugin-default-facets-4g-report.json`;
  - matched rows: 5,341,590;
  - SDK `rows_examined`: 10,693,088;
  - SDK `facet_rows_matched`: 5,341,590;
  - SDK `rows_matched`: 5,341,590.
- Same request RSS measurement with stdout redirected to `/dev/null`:
  - installed `systemd-journal.plugin`: 14.40 seconds wall time,
    767,904 KiB maximum RSS;
  - SDK wrapper: 12.23 seconds wall time, 4,287,352 KiB maximum RSS.
- Output-size analysis found that `_STREAM_ID` dominated the 242 MiB response
  because it was still a default facet in the installed plugin at the time of
  the measurement. That plugin default-facet issue is external to this
  repository, but it does not make 4 GiB SDK RSS acceptable.

Why previous validation missed it:

- SOW-0082 recorded the two-pass behavior as an implementation note instead of
  treating it as a violation of the single optimized traversal contract.
- Prior benchmarks reported logical rows per second and did not fail on
  `rows_examined > unique matched candidate rows`.
- Prior memory checks did not compare maximum RSS at the Netdata function
  boundary.

Repair contract:

- Apply filters first using journal indexes.
- Traverse the resulting candidate rows once per file for the normal Explorer
  query shape.
- During that one pass, build requested facet counters, requested histogram
  buckets, and selected row identifiers or cursors for returned rows.
- Expand full returned-row data only for rows selected for return. This may be
  a second targeted lookup over the selected row cursors, not a second full
  candidate-row traversal.
- Preserve special multi-pass behavior only when an explicit query semantic
  truly requires different effective filter sets per facet group, and record
  that behavior in counters so it cannot be mistaken for the normal hot path.
- Reduce function-boundary memory so large facet responses do not require
  multi-gigabyte RSS. The hot path must avoid holding avoidable duplicated JSON
  object trees, repeated owned labels, or per-row temporary allocations beyond
  row-scoped scratch. If response streaming requires a separate SOW, this
  regression must at least identify and eliminate avoidable Explorer-side
  memory amplification before re-closing.

Validation required before re-closing:

- Add focused tests proving a query with rows, histogram, and facets but no
  facet-exclusion semantics performs one candidate-row traversal, with
  `rows_examined` matching the unique scanned candidate-row count.
- Re-run Rust Explorer tests and the Netdata wrapper tests.
- Re-run the SOW-0093 4 GiB comparison after the installed plugin default
  high-cardinality facet fix is present, and record wall time, output bytes,
  maximum RSS, `rows_examined`, `rows_matched`, and `facet_rows_matched`.
- Compare memory against the installed plugin for the same request with stdout
  redirected to `/dev/null`.
- Update specs/docs if public counter semantics or Explorer strategy behavior
  changes.

Repair implemented:

- Added a normal-path combined traversal in
  `rust/src/journal/src/explorer.rs` so queries with returned rows, histogram,
  and facets run one candidate-row pass when no facet-specific filter exclusion
  is required.
- Kept the existing multi-pass facet behavior only for queries that require
  different effective filters per facet group.
- Added crate-internal cursor-only row collection for the Netdata function
  boundary. Public Explorer calls still return expanded row payloads.
- Changed the Netdata directory wrapper to keep only the final global row
  cursors and then expand payloads only for those final rows.
- Added a focused regression test proving crate-internal cursor-only row
  collection stores row identity without expanding payloads and can expand the
  selected row later through its cursor.
- Removed redundant filter reconfiguration and final directory sorting found by
  the reviewer pass after the first local repair.
- Removed `_CAP_EFFECTIVE` and `_STREAM_ID` from the SDK's systemd-journal
  default facet list to match the installed plugin default-facet correction.
  `_CAP_EFFECTIVE` remains a default view column, matching the separate display
  concern.

Repair validation:

- `cd rust && cargo fmt --check && cargo test -q -p journal --lib && cargo build --release -q -p netdata_function_wrapper`
  passed with 53 Rust journal tests.
- `python3 -m py_compile tests/netdata_function/run_function_compare.py tests/netdata_function/compare_function_json.py`
  passed.
- `git diff --check` passed.
- `.agents/sow/audit.sh` passed with a clean verdict.
- SOW-0093 4 GiB real-corpus request after repair:
  `.local/sow-0093/big-default-facets/request-default-facets-4g.json`.
- SDK wrapper direct warm-cache measurement:
  `.local/sow-0082/regression/sdk-default-after-cursor-final-expand.json`
  and `.local/sow-0082/regression/sdk-default-after-cursor-final-expand.time`;
  wall time 3.18 seconds; maximum RSS 85,312 KiB; matched rows 5,341,590;
  returned rows 200; `rows_examined` 5,346,544; `rows_matched` 5,341,590;
  `facet_rows_matched` 5,341,590; `returned_row_expansions` 200.
- SDK wrapper direct final cold-I/O measurement after reviewer cleanup:
  `.local/sow-0082/regression/sdk-default-final.json` and
  `.local/sow-0082/regression/sdk-default-final.time`; wall time 36.84 seconds;
  maximum RSS 83,876 KiB; major page faults 236,921; file-system inputs
  7,475,264; matched rows 5,341,590; returned rows 200; `rows_examined`
  5,346,545; `rows_matched` 5,341,590; `facet_rows_matched` 5,341,590;
  `returned_row_expansions` 200.
- Semantic comparison against installed `systemd-journal.plugin`:
  `.local/sow-0082/regression/sdk-vs-plugin-default-after-cursor-final-expand.json`;
  comparison `ok: true`; status, item counts, rows, facets, and histogram totals
  matched. The comparator measured plugin wall time 12.21 seconds and SDK wall
  time 3.17 seconds.
- Final semantic comparison after reviewer cleanup:
  `.local/sow-0082/regression/sdk-vs-plugin-default-final.json`; status, item
  counts, rows, facets, and histogram totals all matched. The comparator
  measured plugin wall time 16.38 seconds and SDK wall time 3.28 seconds.
- Installed `systemd-journal.plugin` RSS measurement after its default-facet
  correction:
  `.local/sow-0082/regression/plugin-default-after-default-facet-fix.time`;
  wall time 14.88 seconds; maximum RSS 120,408 KiB.

Reviewer results for the regression repair:

- `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`; noted only
  non-blocking documentation/coverage observations.
- `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`; noted
  non-blocking `Compare`/`FirstValue` indexed-strategy limitations that are
  already part of SOW-0083 constraints.
- `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`; noted no blocking
  correctness, memory, or API issues.
- `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`; recommended a
  focused cursor-only unit test, which was added.
- `minimax-coding-plan/MiniMax-M3`: `PRODUCTION GRADE`; found two cleanup
  candidates, the redundant combined-path filter reconfiguration and redundant
  final directory sort, both fixed before final validation.

Follow-up mapping:

- No unresolved blocking follow-ups remain for this regression.
- The broader SOW-0093 Netdata function-boundary comparison can resume from the
  final SOW-0082 repair commit.
