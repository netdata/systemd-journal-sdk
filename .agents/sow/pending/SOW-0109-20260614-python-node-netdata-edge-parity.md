# SOW-0109 - Python And Node Netdata Edge Parity Closure

## Status

Status: open

Sub-state: created from SOW-0107 final reviewer findings on 2026-06-14. Not
started.

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
  equivalent to Rust where active sampling can reach that path, or the SOW
  records evidence that the path cannot be reached by public Netdata requests
  and is accepted as Explorer-only follow-up.
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

Current state:

- Python delta/data-only stop behavior, Python `candidate_row` callback parity,
  Node commit-realtime-zero handling, facet-only scan sampling, and dead local
  sampling-state construction require first-principles audit.

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
- SOW-status.md: add this SOW as pending now and update on activation/closure.

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
  `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`,
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

## Validation

Acceptance criteria evidence:

- Pending.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending.

Reviewer findings:

- Pending.

Same-failure scan:

- Pending.

Sensitive data gate:

- Pending.

Artifact maintenance gate:

- AGENTS.md: pending.
- Runtime project skills: pending.
- Specs: pending.
- End-user/operator docs: pending.
- End-user/operator skills: pending.
- SOW lifecycle: pending.
- SOW-status.md: pending.

Specs update:

- Pending.

Project skills update:

- Pending.

End-user/operator docs update:

- Pending.

End-user/operator skills update:

- Pending.

Lessons:

- Pending.

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

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
