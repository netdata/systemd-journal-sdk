# SOW-0109 - Python And Node Netdata Edge Parity Closure

## Status

Status: completed

Sub-state: implemented, locally validated, externally reviewed, and closed on
2026-06-14.

## Requirements

### Purpose

Close the remaining Python and Node.js Netdata/Explorer edge parity gaps against
the Rust reference before returning to consumer documentation and stable
integration work. The goal is not broad redesign; it is precise parity for
reviewer-discovered edge cases that were outside SOW-0107 scope.

### User Request

After closing SOW-0107, continue work on the missing parity features.

### Assistant Understanding

Facts:

- SOW-0107 final reviewers returned `PRODUCTION GRADE` for the scoped
  sampling/FTS/sort/index work.
- Final reviewers also found pre-existing Python/Node parity edge cases outside
  SOW-0107 scope.
- These gaps affect public Netdata/Explorer behavior under specific edge
  request shapes and should be fixed before documentation claims complete
  parity.

Inferences:

- This work should be small and focused if it stays anchored to Rust behavior.
- The right implementation authority is the Rust source plus focused
  cross-language value-pinning tests.

Unknowns:

- Whether each reported edge case is a true behavioral divergence after a fresh
  code-level audit. The first implementation step must prove or reject each
  item with evidence before changing code.

### Acceptance Criteria

- Python `data_only && delta` Netdata requests match Rust
  `stop_when_rows_full` behavior.
- Python Explorer control supports Rust-equivalent `candidate_row` callback
  behavior where needed by the Netdata page-window path, or the SOW records
  evidence that current behavior is already equivalent for every public
  request shape.
- Node commit-realtime-zero handling matches Rust/Python behavior, or the SOW
  records evidence that the skip is intentional and harmless for supported
  journal files.
- Python and Node facet-only scan paths either implement sampling integration
  equivalent to Rust, or the SOW records evidence that the current path already
  matches Rust and is not a parity gap.
- Dead local sampling-state construction in Python/Node combined paths is
  removed or explicitly retained with measured/evidence-backed reason.
- Focused tests pin each accepted behavior against Rust-derived expectations.
- Relevant Python/Node package tests and Netdata function comparator gates pass.
- Whole-SOW reviewer batch returns production-grade.

## Analysis

Sources checked:

- `.local/sow-0107/reviews/final3-glm.txt`
- `.local/sow-0107/reviews/final3-kimi.txt`
- `.local/sow-0107/reviews/final2-deepseek.txt`
- SOW-0107 validation and follow-up mapping.

Current state after first-principles audit:

- Confirmed gap: Python `data_only && delta` did not clear
  `stop_when_rows_full`; Rust clears it in per-file query construction
  (`rust/src/journal/src/netdata.rs:1627-1629`), and Python now mirrors that at
  `python/journal/netdata.py:2225-2229`.
- Confirmed gap: Python and Node Explorer control did not expose the Rust
  `candidate_row` callback used by the Netdata page-window sampling path
  (`rust/src/journal/src/netdata.rs:582-590`,
  `rust/src/journal/src/explorer.rs:1700-1727`). Python now wires it through
  `python/journal/explorer.py:362-433`,
  `python/journal/explorer.py:1861-1871`, and
  `python/journal/netdata.py:1464-1472`, `python/journal/netdata.py:3963-3970`.
  Node now wires it through `node/src/lib/explorer.js:294-354`,
  `node/src/lib/explorer.js:1691-1698`, and
  `node/src/lib/netdata.js:1230-1236`, `node/src/lib/netdata.js:2491-2497`.
- Confirmed gap: Node skipped `commitRealtime === 0n` in Explorer traversal and
  index paths. Rust and Python do not perform that skip. Node now lets normal
  stop/skip/time-range logic handle zero commit realtime at
  `node/src/lib/explorer.js:1555-1561`,
  `node/src/lib/explorer.js:1617-1624`, and the matching indexed paths.
- Rejected as non-gap: Python/Node facet-only scan sampling integration. Rust
  facet-only traversal has no sampling integration either
  (`rust/src/journal/src/explorer.rs:1983-2036`), so changing Python/Node there
  would diverge from Rust instead of improving parity.
- Rejected as non-gap: dead local sampling-state construction. Rust constructs
  the local sampling state before preferring shared control sampling
  (`rust/src/journal/src/explorer.rs:1680-1697`), so the Python/Node shape is
  parity-equivalent. Any cleanup would need a Rust-first performance SOW; this
  SOW makes no follow-up claim for it.

Risks:

- These are edge behaviors, but they sit on the Netdata function path and can
  affect paging, sampling, and delta semantics. Fixes must be precise and must
  not disturb the SOW-0107 high-row comparator evidence.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- SOW-0107 implemented the known parity gaps and its final review found
  additional edge divergences that were not part of that SOW's scoped
  acceptance criteria.
- The likely root cause is incremental porting: Python and Node implemented the
  main Netdata traversal paths first, while less common delta, candidate-row,
  commit-realtime-zero, and facet-only sampling paths were not fully compared
  to Rust.

Evidence reviewed:

- GLM final SOW-0107 rerun:
  - `python/journal/netdata.py:2204` was cited for missing Python
    `data_only && delta` `stop_when_rows_full` override.
  - `python/journal/explorer.py:1851-1857` and
    `python/journal/explorer.py:353-423` were cited for missing Python
    `candidate_row` callback parity.
- Kimi final SOW-0107 rerun:
  - Node `_scanExplorerMain` and `_scanExplorerCombined` were cited for
    skipping `commitRealtime === 0n` before Rust/Python-compatible time-stop
    checks.
- DeepSeek final SOW-0107 review:
  - `python/journal/explorer.py:1914-1959` and
    `node/src/lib/explorer.js:1746-1785` were cited for facet-only scan paths
    lacking sampling integration.
  - `python/journal/explorer.py:1843-1848` and
    `node/src/lib/explorer.js:1678-1684` were cited for dead local
    `_ExplorerSamplingState` construction when shared control sampling is used.

Affected contracts and surfaces:

- Python and Node Explorer APIs.
- Python and Node Netdata function APIs.
- Netdata function paging, delta, sampling, and edge timestamp behavior.
- Python/Node tests and possibly shared Netdata function comparator fixtures.

Existing patterns to reuse:

- Rust Explorer and Netdata implementations as the reference.
- SOW-0107 high-row sampling fixture and anchored counter tests.
- Existing Python/Node `ExplorerControl` and Netdata page-window structures.
- Existing comparator harness under `tests/netdata_function/`.

Risk and blast radius:

- Medium for Netdata function behavior if callback or stop semantics are
  wrong.
- Low for core journal reader/writer behavior; this SOW should not touch core
  file-format parsing, writing, compression, sealing, or locking.
- Performance-sensitive: fixes must not add avoidable per-row allocations or
  expensive callback work when unused.

Sensitive data handling plan:

- Use synthetic fixtures only. Do not use live host journals or copy raw
  private journal content into durable artifacts.

Implementation plan:

1. Audit each reviewer finding against Rust, Python, and Node source and record
   whether it is a real parity gap, already equivalent, or intentionally out of
   scope.
2. Add focused tests that fail on each accepted real gap.
3. Implement Python delta/data-only stop parity if confirmed.
4. Implement Python `candidate_row` callback parity if confirmed.
5. Implement or reject Node commit-realtime-zero parity with tests and Rust
   evidence.
6. Implement or explicitly scope facet-only scan sampling behavior with tests.
7. Remove or justify dead local sampling-state construction.
8. Run focused tests, full Python/Node suites, relevant comparators,
   `git diff --check`, audit, and full reviewer batch.

Validation plan:

- Focused Python and Node tests for every accepted parity gap.
- Existing SOW-0107 high-row tests and comparator rerun to prove no regression.
- Relevant Netdata function one-shot/stateful comparators.
- `PYTHONPATH=.local/python-deps:python python3 python/test_all.py`.
- `node node/test/all.js`.
- `git diff --check`.
- `.agents/sow/audit.sh`.
- Whole-SOW reviewer batch from the project reviewer pool.

Artifact impact plan:

- AGENTS.md: no expected change.
- Runtime project skills: no expected change unless a new recurring parity
  review rule is discovered.
- Specs: update Netdata/Explorer specs if confirmed behavior changes the
  documented public contract.
- End-user/operator docs: likely none; SOW-0106 remains the docs SOW after
  parity closes.
- End-user/operator skills: none expected.
- SOW lifecycle: close SOW-0109 before resuming SOW-0105/SOW-0106 if it becomes
  active.
- SOW-status.md: updated on activation and closure.

Open-source reference evidence:

- No external open-source repository evidence checked yet; Rust in this repo is
  the local reference for this parity SOW.

Open decisions:

- None currently. The active decision is to prove each reviewer-reported edge
  case before implementation.

## Implications And Decisions

- 2026-06-14: Created as follow-up from SOW-0107 final review so newly found
  parity edge cases are tracked instead of deferred informally.

## Plan

1. Prove or reject each reviewer-reported parity gap.
2. Add failing focused tests for confirmed gaps.
3. Implement minimal parity fixes.
4. Validate against Rust-derived expectations and existing comparator gates.
5. Run external reviewers and close.

## Delegation Plan

Implementer:

- Current routing is local implementation by the project manager unless the
  user changes the routing decision.

Reviewers:

- Full project reviewer pool after local validation:
  `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/kimi-k2.6`,
  `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.7-plus`,
  `llm-netdata-cloud/minimax-m3-coder`, and
  `llm-netdata-cloud/deepseek-v4-pro`.

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

- Record reviewer timeouts or missing votes explicitly. Do not count them as
  production-grade votes.

## Execution Log

### 2026-06-14

- Created from SOW-0107 final reviewer findings.
- Activated after SOW-0107 was completed and committed.
- Audited each finding against the Rust reference. Confirmed three real gaps:
  Python data-only-delta stop, Python/Node candidate-row callback, and Node
  zero commit-realtime skipping.
- Rejected two reviewer items as non-gaps because Rust has the same behavior:
  facet-only scans do not integrate sampling, and local sampling state is still
  constructed before shared control sampling is preferred.
- Implemented Python `candidate_row` control support and read-only Netdata
  page-window candidate checks.
- Implemented Node `candidateRow` control support and read-only Netdata
  page-window candidate checks.
- Removed Node Explorer skips for `commitRealtime === 0n` so zero commit
  realtime follows the same stop/skip/range logic as Rust and Python.
- Updated the Netdata function facets spec to state that sampling is disabled
  for data-only without delta, while data-only delta remains an analysis path.

## Validation

Acceptance criteria evidence:

- Python `data_only && delta` stop parity:
  `python/journal/netdata.py:2225-2229` matches Rust's per-file override at
  `rust/src/journal/src/netdata.rs:1627-1629`, pinned by
  `python/test_netdata.py:3710-3741`.
- Python Explorer `candidate_row` parity:
  `python/journal/explorer.py:362-433` and
  `python/journal/explorer.py:1861-1871`, pinned by
  `python/test_explorer.py:451-468`.
- Node Explorer `candidateRow` parity:
  `node/src/lib/explorer.js:294-354` and
  `node/src/lib/explorer.js:1691-1698`, pinned by
  `node/test/chunks/explorer.js:472-487`.
- Node zero commit-realtime parity:
  removed zero skips from Node traversal/index paths including
  `node/src/lib/explorer.js:1555-1561` and
  `node/src/lib/explorer.js:1617-1624`, pinned by
  `node/test/chunks/explorer.js:811-824`.
- Facet-only sampling integration:
  not changed because Rust does not integrate sampling in the facet-only scan
  path (`rust/src/journal/src/explorer.rs:1983-2036`).
- Local sampling-state construction:
  retained because Rust also constructs local sampling state before preferring
  shared control sampling (`rust/src/journal/src/explorer.rs:1680-1697`).

Tests or equivalent validation:

- `python3 -m py_compile python/journal/explorer.py python/journal/netdata.py python/test_explorer.py python/test_netdata.py` - passed.
- `PYTHONPATH=python python3 python/test_explorer.py` - passed, 27 tests.
- `PYTHONPATH=python python3 -m unittest python.test_netdata.HistogramBoundsAreOriginalRequestBounds.test_data_only_delta_disables_stop_when_rows_full` - passed.
- `node -e "import('./node/test/chunks/explorer.js').then(m=>m.run())"` - passed.
- `node -e "import('./node/test/chunks/netdata-chunk2c.js').then(m=>m.run())"` - passed.
- `PYTHONPATH=.local/python-deps:python python3 python/test_all.py` - passed.
- `node node/test/all.js` - passed.
- `python3 tests/netdata_function/test_compare_function_json.py` - passed, 51 tests.
- `python3 tests/netdata_function/test_stateful_function_compare.py` - passed, 19 tests.
- `git diff --check` - passed.

Real-use evidence:

- The Netdata function comparator suites passed after the changes. These are
  synthetic but exercise the same SDK wrapper request/response contract used by
  Netdata integration tests.

Reviewer findings:

- `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/qwen3.7-plus`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/minimax-m3-coder`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/deepseek-v4-pro`: `PRODUCTION GRADE`.
- Obsolete reviewer run: an initial `qwen3.6-plus` invocation produced no
  usable vote and was not counted because the active project reviewer pool uses
  `qwen3.7-plus`. The SOW reviewer list was corrected before closure.

Reviewer observations and dispositions:

- Node exported `_combinedSamplingDecision` for focused tests. Accepted as
  non-blocking because the file already exports underscore-prefixed internal
  helpers for tests, and this is not documented as a consumer API.
- Candidate-row callback edge coverage is focused at the Explorer control
  boundary and covered by Netdata comparator suites. Accepted as sufficient for
  this parity SOW because the comparator gates exercise the public wrapper
  contract.
- Python/Node lack Rust's `delta_scan_can_stop` early-termination optimization
  in a matched-row callback path. Rejected as a SOW-0109 blocker: reviewers
  identified it as a pre-existing performance-only gap, not a correctness or
  parity-contract failure in the fixed edge behavior, and the current release
  target is Rust/Go integration.

Same-failure scan:

- `rg -n "commitRealtime === 0n|setCandidateRow|candidateToKeep|_combinedSamplingDecision|candidate_row|candidate_to_keep|stop_when_rows_full =" node/src/lib/explorer.js node/src/lib/netdata.js python/journal/explorer.py python/journal/netdata.py` - passed. The remaining hits are the intended implementations; no Node zero-commit skip remains.
- `rg -n "NetdataRequest\\.parse|to_explorer_query|_requestToExplorerQuery|stop_when_rows_full|stopWhenRowsFull|setCandidateRow|combinedSampling" python/test_netdata.py node/test/chunks/netdata-chunk2c.js node/test/chunks/explorer.js python/test_explorer.py` - used to place matching focused tests.

Sensitive data gate:

- Passed. Only synthetic fixtures and repository-local tests were used; no live
  host journals or private log content were copied into durable artifacts.

Artifact maintenance gate:

- AGENTS.md: no update needed; no project-wide workflow changed.
- Runtime project skills: no update needed; no recurring work procedure changed.
- Specs: updated `.agents/sow/specs/systemd-journal-plugin-facets.md` to clarify
  that data-only without delta skips sampling while data-only delta is an
  analysis path.
- End-user/operator docs: no update yet; SOW-0106 owns Python/Node consumer docs
  after parity closes.
- End-user/operator skills: no update needed; no exported operator skill changed.
- SOW lifecycle: SOW-0109 moved from pending to current during activation and
  moved to done after reviewer approval.
- SOW-status.md: updated root and canonical status ledgers for SOW-0109
  activation.

Specs update:

- Updated `.agents/sow/specs/systemd-journal-plugin-facets.md:205-216` and
  `.agents/sow/specs/systemd-journal-plugin-facets.md:686-692`.

Project skills update:

- No update needed; no workflow rule changed.

End-user/operator docs update:

- No update in this SOW. SOW-0106 remains the docs SOW once parity is complete.

End-user/operator skills update:

- No update needed.

Lessons:

- Review findings that look like "dead" code must be checked against Rust before
  cleanup. In this case, local sampling-state construction is parity-equivalent
  to Rust and should not be removed in Python/Node without a Rust-first
  performance SOW.

Follow-up mapping:

- No follow-up SOW is required for the accepted SOW-0109 scope.
- The matched-row delta early-stop observation is explicitly not a release
  blocker and is not opened as a separate SOW now. It affects Python/Node
  performance only, while v0.7.0 is being prepared for Rust/Go Netdata
  integration.

## Outcome

SOW-0109 completed.

Python and Node now match the Rust reference for the remaining scoped Netdata
edge parity gaps discovered after SOW-0107:

- Python `data_only && delta` requests keep analysis enabled by clearing
  `stop_when_rows_full`.
- Python and Node expose Rust-equivalent `candidate_row` / `candidateRow`
  control callbacks and wire them to the Netdata page-window candidate check.
- Node no longer drops Explorer rows solely because `commitRealtime` is zero.
- Two apparent gaps were rejected with Rust evidence: facet-only scans do not
  integrate sampling, and local sampling-state construction remains
  parity-equivalent to Rust.

All focused tests, full Python/Node suites, Netdata function comparators,
`git diff --check`, `.agents/sow/audit.sh`, and all six reviewer votes passed.

## Lessons Extracted

- Small reviewer-discovered edge cases must be checked against Rust before
  cleanup. Code that appears dead or missing in Python/Node may be intentional
  parity with Rust.
- Reviewer pool version drift can invalidate evidence. The active reviewer
  model names must be checked against `AGENTS.md` before counting votes.

## Followup

None.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
