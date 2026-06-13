# SOW-0107 - Python Explorer Sampling Engine And Facet-Sort Parity

## Status

Status: open

Sub-state: pending; discovered during SOW-0105 round-2 review fixes. Affects
both the Python and Node Explorer ports. Not a regression (the behavior was
never validated as present); tracked here per the project rule to file pending
SOWs for problems found in already-shipped SDK code.

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

Status: blocked

Blocked on: this is a pending follow-up. Activate after SOW-0106 (the docs
SOW) or whenever the user prioritizes it. Refresh the gate at activation with
a fresh read of the Rust engine and a built high-row fixture.

Problem / root-cause model:

- The Python and Node Explorer ports shipped the sampling data structures and
  stats plumbing but not the decision engine; the gates could not catch it
  because no fixture exceeds the budget.

Evidence reviewed:

- Listed in Analysis; verified 2026-06-13.

Affected contracts and surfaces:

- `python/journal/explorer.py`, `python/journal/netdata.py`,
  `node/src/lib/explorer.js`, `node/src/lib/netdata.js`,
  `tests/netdata_function/` fixtures and runners, per-language tests.

Existing patterns to reuse:

- Rust `ExplorerSamplingState` as the authority; the SOW-0104/0105 comparator
  and frozen-fixture protocol; value-pinning test style.

Risk and blast radius:

- Python- and Node-only additive traversal logic; no Rust/Go changes; the
  shared matrices and the existing gates guard against regressions.

Sensitive data handling plan:

- Synthetic high-row fixtures only; no host journal data in durable artifacts.

Implementation plan:

1. Build a high-row synthetic fixture and prove the gap (Rust samples,
   Python/Node do not) with the comparator.
2. Port `ExplorerSamplingState` to the Python explorer; thread it; validate.
3. Port to the Node explorer; thread `query.sampling` in
   `_requestToExplorerQuery` (restore the reserved params); validate.
4. Add per-language sampling-stat tests; run both comparator gates.

Validation plan:

- Four-peer comparator on the high-row fixture; per-language tests; both
  parity gates re-run; `.agents/sow/audit.sh`.

Artifact impact plan:

- Specs: note the sampling contract if a spec covers explorer semantics.
- End-user/operator docs: Production-Profiles/Explorer pages if sampling
  behavior is documented.
- SOW-status.md: add to Pending now.

Open-source reference evidence:

- Rust in-repo source is the reference; no external repos.

Open decisions:

- Port location (explorer traversal vs netdata layer) — recommend explorer
  traversal to mirror Rust.

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

- The five-model `llm-netdata-cloud` pool, read-only, whole-SOW batches.

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

## Validation

Acceptance criteria evidence:

- Pending implementation.

Tests or equivalent validation:

- Pending implementation.

Real-use evidence:

- Pending implementation.

Reviewer findings:

- Pending implementation.

Same-failure scan:

- Pending implementation.

Sensitive data gate:

- This SOW contains no raw sensitive data.

Artifact maintenance gate:

- Pending close.

Specs update:

- Pending implementation.

Project skills update:

- Pending implementation.

End-user/operator docs update:

- Pending implementation.

End-user/operator skills update:

- Pending implementation.

Lessons:

- A comparator gate validates only what its fixtures exercise: a feature that
  only activates above a threshold (sampling budget) needs a fixture above
  that threshold or it silently passes unimplemented. Future feature ports
  must include a triggering fixture, not only a present-but-inert one.

Follow-up mapping:

- Pending implementation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
