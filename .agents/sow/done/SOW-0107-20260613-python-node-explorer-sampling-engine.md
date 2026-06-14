# SOW-0107 - Python And Node Explorer Engine Parity Gaps

## Status

Status: completed

Sub-state: completed on 2026-06-14. Python and Node Explorer/Netdata parity
gaps in this SOW are implemented, locally validated, externally reviewed, and
ready for the close commit. Follow-up parity edge cases discovered during final
review are tracked by SOW-0109.

## Requirements

### Purpose

Bring the Python and Node.js Explorer/Netdata sampling behavior to true parity
with the Rust reference. Rust implements budget-based row sampling with
estimation in `ExplorerSamplingState`; the Python and Node Explorer traversals
do not implement the sampling decision engine, so a request whose matched-row
count exceeds the sampling budget returns full unsampled results in Python and
Node, while Rust samples and estimates.

### User Request

Derived from the 2026-06-11 program goal: bring Python and Node.js to 100%
parity with Rust. This SOW closes the one sampling gap that the SOW-0104 and
SOW-0105 comparator gates could not catch because no fixture exceeds the
sampling budget.

### Assistant Understanding

Facts (verified 2026-06-13 by code reading):

- Rust implements the sampling decision engine: `ExplorerSamplingState`
  (`rust/src/journal/src/explorer.rs:456-590`) with `for_query`, `begin_file`,
  and `should_sample`, budget-split slot accounting
  (`enable_after_samples = budget/2`, per-file and per-slot thresholds), and
  the `rows_unsampled`/`rows_estimated`/`sampling_unsampled`/
  `sampling_estimated` accounting it drives. The query carries
  `sampling: Option<ExplorerSampling>` populated from `matched_files` and the
  file header (`to_explorer_query` / `file_query`).
- Python: `python/journal/explorer.py` has `ExplorerSampling.budget` and the
  stats fields, but NO `ExplorerSamplingState` decision logic in the
  traversal. `python/journal/netdata.py` builds and fills the
  `ExplorerSampling` (`_fill_sampling_from_header`) and aggregates the stats,
  but the explorer never consumes `query.sampling` to skip/estimate rows.
- Node: `node/src/lib/explorer.js` has the `ExplorerSampling` class and stats
  fields but no decision engine; `node/src/lib/netdata.js`
  `_requestToExplorerQuery` does not even thread `query.sampling`
  (the `_matchedFiles`/`_reader` params are reserved for this, with a code
  comment pointing here).
- Both Python (SOW-0104) and Node (SOW-0105) passed their three-peer
  comparator gates including the low-budget sampling fixture
  (`tests/netdata_function/requests/window-last5-default-facets-sampling20.json`)
  ONLY because that fixture's matched-row count is below the budget (20), so
  no peer samples. The gap has zero observable impact on the validated
  corpus.

Inferences:

- A validating test must use a fixture whose matched-row count exceeds the
  sampling budget so Rust samples and Python/Node currently do not — that
  fixture does not exist yet and must be created (synthetic, high-row).
- Porting the engine is non-trivial: the slot accounting, per-file enable
  thresholds, and the estimation that fills `rows_estimated`/
  `sampling_estimated` must match Rust exactly for the comparator's strict
  sampling-stat checks.

Unknowns:

- Whether the engine should be ported into the shared explorer traversal in
  each language or wrapped at the netdata layer; Rust does it in the explorer
  via `ExplorerSamplingState` threaded through `ExplorerControl`
  (`set_sampling_state`). The faithful port mirrors Rust.

### Acceptance Criteria

- A high-row synthetic comparator fixture (matched rows > budget) is added and
  passes node-vs-sdk, node-vs-plugin, python-vs-sdk, and python-vs-plugin with
  strict `items.unsampled`, `items.estimated`, sampled facet counts, and
  histogram `[unsampled]`/`[estimated]` buckets.
- Python and Node Explorer traversals implement the `ExplorerSamplingState`
  decision/estimation engine semantically equal to Rust.
- `query.sampling` is threaded into the per-file query in both languages
  (Node: restore the `matchedFiles`/`reader` usage in
  `_requestToExplorerQuery`).
- Focused per-language tests pin the sampling stats against Rust-derived
  expectations.
- Whole-SOW reviewer batch returns production-grade.

## Analysis

Sources checked:

- `rust/src/journal/src/explorer.rs:456-590` (ExplorerSamplingState),
  `:1678-1700` (sampling_state_for_combined), `:486-520`
  (begin_file/slot math).
- `python/journal/explorer.py`, `python/journal/netdata.py`
  (`_fill_sampling_from_header`, stats aggregation).
- `node/src/lib/explorer.js`, `node/src/lib/netdata.js`.

Risks:

- The estimation math must match Rust exactly or the strict sampling-stat
  comparator checks fail; this is the delicate part.
- Performance: sampling is a hot-loop decision; the port must not add
  per-row overhead beyond Rust's.

## Pre-Implementation Gate

Status: ready

Activated by user instruction on 2026-06-14. The original "after SOW-0106"
blocker is superseded by the user's priority decision to fix missing parity
features now; SOW-0106 remains blocked behind parity/docs order.

Problem / root-cause model:

- The Python and Node Explorer ports shipped sampling data structures and stats
  plumbing but not Rust's row-level `ExplorerSamplingState` decision engine.
  The current wrappers use a post-pass `_apply_sampling_budget()` approximation
  that mutates counters after a full traversal, so it cannot skip field
  expansion, cannot stop-and-estimate during traversal, and cannot match Rust's
  per-file/per-slot budget math under triggering loads.
- Python still misses two smaller Rust Netdata-layer behaviors: full-text search
  query parsing/application on the Netdata path, and Rust facet-option sorting
  for `PRIORITY` versus generic fields.
- Node and Python Index strategy surfaces need Compare-mode validation on
  filtered queries. Node also still has the known O(N^2)
  `entryOffsets.indexOf()` loop in `_indexedCollectRows`.

Evidence reviewed:

- Rust reference:
  - `rust/src/journal/src/explorer.rs:456-590`: `ExplorerSamplingState`
    fields and `for_query` / `begin_file` / `decide` budget logic.
  - `rust/src/journal/src/explorer.rs:1900-1990`: combined traversal calls
    the sampling decision before scanning row DATA, so sampled-out rows avoid
    field expansion.
  - `rust/src/journal/src/explorer.rs:1765-1845`: sampling decisions record
    unsampled and estimated rows and histogram special buckets.
  - `rust/src/journal/src/netdata.rs:3258-3295`: `sort_facet_options` and
    `parse_fts_query_patterns`.
- Python gaps:
  - `python/journal/explorer.py:1428-1588`: traversal loops do not consult
    `query.sampling`.
  - `python/journal/netdata.py:1973-2018`: `NetdataRequest.to_explorer_query`
    builds `ExplorerSampling` but does not add FTS terms/patterns from
    `request.query`.
  - `python/journal/netdata.py:3772-3783` and `python/journal/netdata.py:4189`:
    wrapper-level post-pass sampling approximation.
  - `python/journal/netdata.py:2943-2966`: facet option output exists but lacks
    Rust's `PRIORITY` numeric ordering rule.
- Node gaps:
  - `node/src/lib/explorer.js:1280-1445`: traversal loops do not consult
    `query.sampling`.
  - `node/src/lib/netdata.js:2383-2445`: `_requestToExplorerQuery` explicitly
    reserves `_matchedFiles` / `_reader` for sampling and still returns without
    setting `query.sampling`.
  - `node/src/lib/netdata.js:2070-2100` and `node/src/lib/netdata.js:2339-2341`:
    wrapper-level post-pass sampling approximation.
  - `node/src/lib/explorer.js:1557`: `_indexedCollectRows` still calls
    `reader.entryOffsets.indexOf(entryOffset)` inside the candidate loop.

Affected contracts and surfaces:

- `python/journal/explorer.py`, `python/journal/netdata.py`,
  `node/src/lib/explorer.js`, `node/src/lib/netdata.js`,
  `tests/netdata_function/` fixtures and runners, per-language tests.

Existing patterns to reuse:

- Rust `ExplorerSamplingState` as the authority; port the decision engine into
  explorer traversal, not the Netdata wrapper, because Rust decides before DATA
  scanning and uses `ExplorerControl` for shared multi-file sampling state.
- The SOW-0104/0105 comparator and frozen-fixture protocol; value-pinning test
  style.
- Existing Python/Node FTS helper and Index strategy tests; extend them with
  triggering fixtures rather than relying on empty windows.

Risk and blast radius:

- Python- and Node-only additive traversal logic; no Rust/Go changes; the
  shared matrices and the existing gates guard against regressions.

Sensitive data handling plan:

- Synthetic high-row fixtures only; no host journal data in durable artifacts.

Implementation plan:

1. Build high-row synthetic coverage that exceeds the sampling budget and
   proves the current Python/Node post-pass behavior diverges from Rust.
2. Port `ExplorerSamplingState` to Python Explorer traversal, thread Netdata
   sampling into the per-file query, remove the post-pass approximation where
   Rust no longer needs it, and validate.
3. Port the same engine to Node Explorer traversal, thread `query.sampling` in
   `_requestToExplorerQuery`, remove the post-pass approximation where Rust no
   longer needs it, and validate.
4. Fix Python FTS parsing/application on the Netdata path and add a non-empty
   triggering fixture.
5. Fix Python facet-option sorting to match Rust, with a value-pinning
   `PRIORITY` order test.
6. Validate Python/Node Index strategy with Compare-mode filtered queries and
   remove Node's O(N^2) row collection loop.
7. Run focused per-language tests, comparator gates, docs/spec checks, reviewer
   batch, and SOW audit.

Validation plan:

- Focused Python and Node tests for sampling decisions, FTS triggering,
  `PRIORITY` facet sort, and Index Compare filtered equality.
- Four-peer comparator on high-row sampling and non-empty FTS fixtures, with
  strict sampling-stat and content checks.
- Existing Python and Node package tests.
- Relevant `tests/netdata_function/` one-shot/stateful comparator gates.
- `git diff --check` and `.agents/sow/audit.sh`.
- Whole-SOW reviewer batch after local validation; iterate until production
  grade.

Artifact impact plan:

- Specs: note the sampling contract if a spec covers explorer semantics.
- End-user/operator docs: Production-Profiles/Explorer pages if sampling
  behavior is documented.
- SOW-status.md: move SOW-0107 from Pending to Current now and update again on
  close.

Open-source reference evidence:

- Rust in-repo source is the reference; no external repos.

Open decisions:

- Resolved by activation: port location is explorer traversal, matching Rust.
  The wrapper post-pass is not an acceptable parity implementation because it
  cannot avoid row field work or stop-and-estimate during traversal.

## Implications And Decisions

1. 2026-06-13 discovery and disposition
   - Decision: track the sampling-engine gap here rather than block SOW-0105,
     because the gap has zero observable impact on every validated fixture and
     gate, exists symmetrically in the already-shipped Python SDK, and is
     larger feature work requiring faithful estimation-math porting.
   - Implication: SOW-0104 and SOW-0105 must NOT claim sampling decision-engine
     parity; their validation records disposition sampling as tracked here.
   - Risk: a future consumer issuing a request whose matched rows exceed the
     sampling budget will get unsampled results from Python/Node where Rust
     samples; documented until this SOW closes.

## Plan

1. Prove the gap with a high-row fixture.
2. Port and thread the engine in Python, then Node.
3. Validate with four-peer comparator and per-language tests.
4. Review, audit, close.

## Delegation Plan

Implementer:

- Pool implementer per the active routing at activation
  (`llm-netdata-cloud/...`); Rust/Go remain frozen.

Reviewers:

- The six-model `llm-netdata-cloud` pool from `AGENTS.md`, read-only,
  whole-SOW batches.

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

- As in SOW-0105: record stalls, retry, and rotate implementer models on
  repeated failure.

## Additional Scope - Python FTS application + Node/Python Index strategy (2026-06-13)

SOW-0105 round-4 review (glm) found two more Rust behaviors of the same
"invisible to the gates" class:

1. FULL-TEXT SEARCH NOT APPLIED ON THE NETDATA PATH (Python). The `query`
   parameter is parsed into the response echo but never converted to FTS
   patterns and applied as a filter, so a logs query with text search returns
   UNFILTERED results — a correctness bug on the production path. Rust parses
   it (`parse_fts_query_patterns`, netdata.rs:3279) and threads
   `fts_terms`/`fts_patterns`/`fts_negative_patterns` into the per-file query
   (`to_explorer_query`, netdata.rs:1602-1604). The Node side was fixed in
   SOW-0105 (`_parseFtsQueryPatterns` + query threading + a synthetic
   triggering fixture). The shared comparator FTS fixture
   (`window-last5-fts-or-negative.json`) hides this because its Oct-2022
   window is empty on the validation host (zero rows for every peer). Port the
   FTS parsing/application to `python/journal/netdata.py` with a synthetic
   triggering fixture, then add a non-empty FTS comparator fixture so all four
   peers exercise it.

2. INDEX EXPLORER STRATEGY (Node and Python). The Index strategy is a
   secondary public Explorer surface (the Netdata path uses Traversal), is not
   exercised by any comparator fixture, and has two concerns flagged in
   review: a potential field-filter handling divergence, and an O(N^2)
   `entryOffsets.indexOf` per candidate in row collection
   (`node/src/lib/explorer.js` `_indexedCollectRows`). Validate Index against
   Traversal with `ExplorerStrategy.Compare` equality on filtered queries in
   both languages, fix the O(N^2) collection, and add Compare-mode tests.
   Rust documents Index as exact only for all-values/commit-realtime/no-FTS
   shapes; the validation must respect that contract.

## Additional Scope - Python facet-option sort (2026-06-13)

SOW-0105 round-3 review (kimi) found a second Rust behavior unported in the
Python netdata layer: `sort_facet_options` (`rust/src/journal/src/netdata.rs:3258`)
sorts facet options (PRIORITY numerically ascending; other fields by count
descending then id ascending). The Node port was fixed in SOW-0105
(`_sortFacetOptions`), but Python still sorts all facets by count and uses a
locale-sensitive id tiebreak. The three-peer comparator normalizes facet
options to an unordered map (`normalized_facet_options` keys by id), so the
gap is invisible to the gates but produces a user-visible option-order
divergence (e.g. PRIORITY shown by frequency instead of severity). Port
`sort_facet_options` to `python/journal/netdata.py` with a value-pinning test
on PRIORITY option order. This is small and independent from the sampling
engine work; it may be done first.

## Execution Log

### 2026-06-13

- Created from the SOW-0105 round-2 discovery that the Explorer sampling
  decision engine is unported in both Python and Node.
- Expanded 2026-06-13 with the Python facet-option-sort gap found in
  SOW-0105 round-3 review (Node side fixed in SOW-0105).

### 2026-06-14

- Activated by user priority after SOW-0108 regression repair closed.
- Ported Rust-compatible `ExplorerSamplingState` into Python and Node Explorer
  traversal:
  - Python: `python/journal/explorer.py` adds shared sampling state through
    `ExplorerControl.set_sampling_state()`, decides before row DATA scanning,
    records `[unsampled]` and `[estimated]` histogram buckets, and stops with
    estimate when the Rust conditions trigger.
  - Node: `node/src/lib/explorer.js` mirrors the same state machine with
    `ExplorerControl.setSamplingState()` and BigInt accounting.
- Threaded Netdata sampling into the per-file Explorer queries:
  - Python: `python/journal/netdata.py` creates one shared
    `_ExplorerSamplingState` for the multi-file request and passes it through
    every per-file control object.
  - Node: `node/src/lib/netdata.js` computes file header sampling metadata in
    `_requestToExplorerQuery()` and shares one `_ExplorerSamplingState` across
    per-file controls.
- Removed the obsolete wrapper-level post-pass sampling helpers from Python
  and Node. Sampling now occurs only at the Explorer row decision point, before
  field expansion, matching Rust.
- Fixed Python Netdata FTS parity by parsing `query` into positive terms,
  positive OR patterns, and negative patterns and passing them into
  `ExplorerQuery`.
- Fixed Python `PRIORITY` facet-option ordering to match Rust: numeric
  ascending by priority value instead of generic count-descending ordering.
- Fixed Python/Node Index Compare filtered-query parity tests; Node
  `_indexedCollectRows` now builds an offset-to-index map once instead of
  calling `entryOffsets.indexOf()` for every candidate.
- Added a high-row sampling request fixture:
  `tests/netdata_function/requests/window-high-row-default-facets-sampling20.json`.
- Initial local evidence used a 600-row synthetic journal and was rejected
  during review because it did not actually trigger `unsampled` or `estimated`
  counters. Rebuilt the evidence around a deterministic 5,000-row, one-file
  synthetic journal covering five seconds so the sampling budget is exceeded
  and the engine actually triggers.
- Fixed two issues exposed by the triggering fixture:
  - Python and Node now pass the current ENTRY seqnum into the sampling state
    instead of `0`, so the seqnum-based progress estimator activates like Rust.
  - Python and Node item counters now use Rust's sampling fallback accounting:
    `items.evaluated` and `items.matched` are based on `rows_examined` when
    unsampled or estimated rows exist, then add `unsampled` and `estimated`.
- Added exact high-row Netdata sampling tests in Python and Node. They pin the
  same Rust/plugin-derived counters used by the comparator:
  `items.evaluated = items.matched = 4604`, `items.unsampled = 34`,
  `items.estimated = 4554`, `items.after = 11`, and `_sampling.sampled = 11`,
  `_sampling.unsampled = 35`, `_sampling.estimated = 4554`.
- Reviewer round 1 found four real edge gaps; all were fixed before close:
  - Python `PRIORITY` facet sort now mirrors Rust's `Option<u8>` ordering for
    non-numeric and out-of-range values (`None` before numeric values).
  - Python and Node seqnum-based sampling estimates now clamp progress values
    above `1.0` to `1.0`, matching Rust `bounded_positive_proportion()`.
  - Python indexed row collection now builds an offset-to-index map instead of
    calling the linear `_index_for_entry_offset()` helper for each candidate.
  - Python and Node Netdata item payloads now use Rust-style page counters for
    anchored sampled queries instead of the fallback `rows_examined` source
    count whenever page-window accounting is available.
- Added focused guards for the round-1 fixes:
  - Python and Node anchored high-row sampling item counters in both backward
    and forward directions.
  - Python and Node seqnum-estimate progress clamping above `1.0`.
  - Python `PRIORITY` sort for non-`u8` values.
  - Python indexed row collection not using the linear reader offset lookup.

## Validation

Acceptance criteria evidence:

- High-row four-peer comparator passed after rebuilding the Rust wrapper from
  current source:
  - command: `python3 tests/netdata_function/run_function_compare.py --sdk rust/target/release/netdata_function_wrapper --plugin /usr/libexec/netdata/plugins.d/systemd-journal.plugin --python python/cmd/netdata_function_wrapper.py --python-interpreter python3 --node node/cmd/netdata_function_wrapper.js --node-interpreter node --dir .local/sow-0107/high-row-fixture-trigger --request tests/netdata_function/requests/window-high-row-default-facets-sampling20.json --out .local/sow-0107/reports/high-row-sampling-trigger-after-review-fixes.json --save-json-dir .local/sow-0107/json-trigger-after-review-fixes --timeout 0 --process-timeout 300`
  - report: `.local/sow-0107/reports/high-row-sampling-trigger-after-review-fixes.json`
  - result: `ok: true`; rows, facets, histogram, histogram schema,
    diagnostic items, and top-level content checks all true for SDK/plugin,
    Python/plugin, Node/plugin, Python/SDK, and Node/SDK comparisons.
  - exact sampling counters matched across Rust SDK, installed
    `systemd-journal.plugin`, Python, and Node:
    - `items`: `after=11`, `before=0`, `estimated=4554`,
      `evaluated=4604`, `matched=4604`, `max_to_return=5`, `returned=5`,
      `unsampled=34`.
    - `_sampling`: `sampled=11`, `unsampled=35`, `estimated=4554`;
      Python/Node/Rust SDK also include `enabled=true`.
- Focused tests prove the new behaviors:
  - Python sampling decision, filtered Index Compare, FTS application, and
    priority ordering.
  - Node sampling decision, filtered Index Compare, and no linear
    `entryOffsets.indexOf()` in indexed row collection.
  - Python/Node Netdata high-row sampling tests pin the same exact unanchored
    sampling counters as the four-peer comparator.
  - Python/Node Netdata anchored high-row sampling tests pin Rust SDK item
    counters in both backward and forward directions, covering the reviewer
    round-1 page-counter finding.
  - Python/Node Explorer tests pin seqnum-estimate progress clamping above
    `1.0`, covering the reviewer round-1 bounded-proportion finding.

Tests or equivalent validation:

- `PYTHONPATH=python python3 python/test_explorer.py` - passed, 26 tests.
- `PYTHONPATH=python python3 -m unittest python.test_netdata.Sampling.test_sampling_high_row_window_matches_rust_accounting python.test_netdata.Sampling.test_sampling_high_row_anchor_items_match_rust_accounting python.test_netdata.PythonNetdataParityGaps` - passed, 5 tests.
- `PYTHONPATH=.local/python-deps:python python3 python/test_all.py` - passed.
- `node -e "import('./node/test/chunks/explorer.js').then(m=>m.run())"` - passed.
- `node -e "import('./node/test/chunks/netdata-chunk2c.js').then(m=>m.run())"` - passed.
- `node node/test/all.js` - passed.
- `python3 tests/netdata_function/test_compare_function_json.py` - passed, 51 tests.
- `python3 tests/netdata_function/test_stateful_function_compare.py` - passed, 19 tests.
- `git diff --check` - passed.
- Initial `python3 python/test_all.py` without `PYTHONPATH=.local/python-deps`
  failed with `ModuleNotFoundError: No module named 'lz4'`; rerun with the
  repository-local dependency path passed. This is an environment dependency,
  not a code failure.

Real-use evidence:

- The high-row comparator used the installed Netdata
  `/usr/libexec/netdata/plugins.d/systemd-journal.plugin` against a repository
  synthetic journal directory. No live host journal writes or live journal
  probing were used.
- The first high-row evidence attempt was rejected because the fixture was
  inert (`unsampled=0`, `estimated=0`). The final evidence uses the corrected
  5,000-row fixture above and proves nonzero sampling/estimation.

Reviewer findings:

- One early Mimo reviewer run returned `PRODUCTION GRADE` before the high-row
  fixture was corrected. That verdict is superseded and is not counted for
  closeout evidence.
- One stale GLM review attempt identified that the first high-row evidence was
  not exercising the intended sampling path. The run was stopped before a final
  verdict because the implementation evidence was invalid. Disposition: fixed
  by the 5,000-row fixture, seqnum passthrough repair, Rust-compatible item
  counter repair, exact Python/Node unit tests, and the final four-peer report.
- Round-1 whole-SOW reviewer batch completed with all six models returning
  `PRODUCTION GRADE`, but several non-blocking findings were accepted as real
  SOW-quality issues and fixed before closure:
  - GLM: Python `PRIORITY` facet sort diverged from Rust for non-numeric and
    `>255` values; fixed with `_parse_priority_for_sort()` and a non-`u8`
    value-pinning test.
  - GLM: Python/Node seqnum-based sampling estimates returned no seqnum
    estimate when progress exceeded `1.0`; fixed by clamping to `1.0` and
    adding direct Explorer tests.
  - Mimo/Kimi: Python indexed row collection still used the linear
    `_index_for_entry_offset()` helper per candidate; fixed with an
    offset-to-index map and a monkey-patch guard test.
  - Kimi: Python/Node sampled anchored requests used fallback item counts
    instead of Rust page-window counters; fixed with `_NetdataPageWindow` /
    `NetdataPageWindow` and anchored high-row tests in both directions.
- Final post-fix whole-SOW reviewer batch:
  - GLM rerun: `PRODUCTION GRADE`. Non-blocking observations were pre-existing
    parity gaps outside SOW-0107 scope: Python delta/data-only
    `stop_when_rows_full` override, Python missing `candidate_row` callback
    parity, non-u8 `PRIORITY` tie ordering, and Python plain-integer counter
    arithmetic. Real parity gaps are tracked by SOW-0109.
  - Kimi rerun: `PRODUCTION GRADE`; no blocking findings. Non-blocking
    observations: Node commit-realtime-zero skip divergence is pre-existing
    and Python tests require the repository-local dependency path for `lz4`.
    The parity divergence is tracked by SOW-0109.
  - Mimo: `PRODUCTION GRADE`; no blocking findings.
  - Qwen: `PRODUCTION GRADE`; no blocking findings. The only action item was
    this closeout status/move.
  - Minimax: `PRODUCTION GRADE`; no blocking findings. It noted a minor
    non-blocking double construction of histogram state.
  - DeepSeek: `PRODUCTION GRADE`; no blocking findings. It noted low-severity
    non-blocking follow-ups: facet-only scan sampling integration,
    dead local sampling-state construction, stale status-ledger hygiene, and
    spec wording precision.

Same-failure scan:

- Removed the only obsolete Python/Node wrapper-level sampling post-pass helper
  implementations found by `rg "_apply_sampling_budget|_applySamplingBudget"`.
- Added filtered Index Compare tests in both languages so future filter/index
  divergences fail.
- Added a Node test that monkey-patches `entryOffsets.indexOf` to throw, so
  the O(N^2) indexed row-collection shape cannot return unnoticed.
- Added exact high-row sampling tests in both Python and Node so future inert
  fixtures or item-counter double-counting cannot pass with loose assertions.
- Added a Python test that monkey-patches `_index_for_entry_offset` to throw,
  so the equivalent Python indexed row-collection shape cannot regress.
- Added Python/Node anchored high-row sampling tests so page-counter fallback
  drift cannot return unnoticed.

Sensitive data gate:

- This SOW contains no raw sensitive data.

Artifact maintenance gate:

- `AGENTS.md` unchanged; workflow guardrails did not change.
- Specs updated:
  - `.agents/sow/specs/product-scope.md` now describes Explorer and strategy
    parity across Rust, Go, Node.js, and Python.
  - `.agents/sow/specs/systemd-journal-plugin-facets.md` now describes Netdata
    function validation/progress/cancellation parity across all four languages.
- Project skills unchanged; no reusable workflow changed.
- End-user docs unchanged here; SOW-0106 remains the tracked docs expansion for
  Python/Node examples and wiki pages after parity closes.
- SOW lifecycle updated in root and canonical status ledgers on activation and
  close. SOW-0107 moves to `.agents/sow/done/` with `Status: completed`.
  Follow-up parity edge cases are tracked by SOW-0109.

Specs update:

- Completed as listed in the artifact maintenance gate.

Project skills update:

- Not needed; this SOW changed SDK behavior, not the repository workflow.

End-user/operator docs update:

- Not changed in this SOW. SOW-0106 is the existing pending docs SOW for
  Python/Node consumer documentation and verified examples.

End-user/operator skills update:

- Not needed; no output/reference skills are consumed outside normal repo work.

Lessons:

- A comparator gate validates only what its fixtures exercise: a feature that
  only activates above a threshold (sampling budget) needs a fixture above
  that threshold or it silently passes unimplemented. Future feature ports
  must include a triggering fixture, not only a present-but-inert one.
- Sampling acceptance evidence is invalid unless `unsampled` and/or
  `estimated` counters are nonzero for the fixture under test.

Follow-up mapping:

- SOW-0109 tracks the final reviewer-discovered Python/Node parity edge cases
  that are outside SOW-0107 scope but should be fixed before resuming docs:
  Python delta/data-only stop behavior, Python `candidate_row` callback parity,
  Node commit-realtime-zero skip divergence, facet-only scan sampling
  integration, and cleanup of dead local sampling-state construction.
- SOW-0106 remains pending for Python/Node docs after parity closure.

## Outcome

Completed. Python and Node now match the Rust Explorer/Netdata behavior for
the scoped SOW-0107 parity gaps: sampling decision/estimation, Python FTS,
Python `PRIORITY` sorting, Python/Node Index Compare validation, and O(1)
indexed row collection. Local validation passed, four-peer high-row comparator
evidence matched exact counters, all six final reviewers returned
`PRODUCTION GRADE`, and follow-up parity edge cases are tracked by SOW-0109.

## Lessons Extracted

- Threshold-gated behavior must have threshold-triggering fixtures. The old
  low-row sampling fixture exercised request parsing but not the sampling
  decision engine.
- Comparator runs must use current local binaries. The first high-row run used
  an older Rust wrapper and failed on stale histogram metadata; rebuilding the
  Rust wrapper from current source made the same comparison pass.

## Followup

- SOW-0109 - Python And Node Netdata Edge Parity Closure.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
