# SOW-0101 - Netdata Function Stateful Equivalence

## Status

Status: completed

Sub-state: completed after stateful SDK/plugin equivalence validation and
reviewer pass.

## Requirements

### Purpose

Prove that the SDK Netdata function wrapper can replace Netdata's
`systemd-journal.plugin` function contract for stateful UI behavior, not only
for single one-shot request payloads. The specific purpose is to prevent
pagination, tailing, and delta-response regressions from reaching Netdata
integration again.

### User Request

Close the remaining test gaps versus Netdata functions and test the behavior
carefully.

### Assistant Understanding

Facts:

- The committed single-request comparator compares stable function content
  between the SDK wrapper and an external Netdata `systemd-journal.plugin`.
- The committed request fixtures do not include an `anchor` request and use
  only backward direction.
- Rust and Go SDK-internal tests now cover paging anchors, tail no-change, and
  tail delta output, but those tests do not execute the installed Netdata
  plugin side by side.
- The UI tail bug was a stateful multi-call contract failure: repeated tail
  calls could return previously seen rows instead of `304`.

Inferences:

- The missing test class is a stateful boundary harness that runs the SDK
  wrapper and `systemd-journal.plugin` in lockstep, derives anchors from
  previous responses, and compares each step.

Unknowns:

- Whether the installed local Netdata plugin has all recent `--test` behavior
  available on this workstation at validation time. This can be resolved by
  running the harness.

### Acceptance Criteria

- Add a committed stateful Netdata function comparison harness for SDK-wrapper
  versus external `systemd-journal.plugin`.
- The harness must run SDK first, then plugin, for every step.
- The harness must derive anchors from previous JSON responses instead of using
  only static request files.
- The harness must compare normalized function content after every step using
  the existing comparator.
- The committed stateful suite must cover:
  - backward paging without duplicates or missing rows;
  - forward paging without duplicates or missing rows;
  - tail poll that returns only rows newer than the anchor and then `304`;
  - tail poll with filters where newer rows exist but filters match no rows
    returns plugin-compatible `200` with empty data;
  - data-only delta response comparing `facets_delta`, `histogram_delta`, and
    `items_delta`.
- Local validation must run Python unit tests for the comparator/harness and a
  real side-by-side smoke against the installed plugin when available.

## Analysis

Sources checked:

- `tests/netdata_function/README.md`
- `tests/netdata_function/requests/*.json`
- `tests/netdata_function/run_function_compare.py`
- `tests/netdata_function/compare_function_json.py`
- `rust/src/journal/src/netdata.rs`
- `go/journal/netdata_test.go`
- `.agents/sow/done/SOW-0093-20260605-netdata-function-boundary-reader-comparison.md`

Current state:

- The existing comparator checks stable top-level fields, columns, rows,
  facets, histograms, counters, and 304 envelopes.
- Existing committed request fixtures include tail and delta one-shot cases but
  no committed request with `anchor` and no committed forward-direction
  request.
- SDK-internal Rust and Go tests cover the known stateful contracts, but they
  do not prove plugin/SKD boundary equivalence.
- Installed `systemd-journal.plugin` evidence corrected one SDK-internal
  assumption: when selected files are newer than `if_modified_since` but
  filters match no returned rows, the plugin returns `200` with empty data. The
  pre-scan "no selected files are newer" path remains the `304` path.
- Installed plugin tail+delta evidence showed that histogram bucket shape is
  based on the original request timeframe. The anchor constrains returned rows
  and delta counts, but must not shrink the histogram timeframe.

Risks:

- Without a stateful side-by-side boundary test, the SDK can pass one-shot
  equivalence while still breaking UI paging or tailing.
- Test reports must not commit raw JSON from real journal data because it may
  contain sensitive content.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The test suite validated stateless function output but not multi-call state
  transitions. The UI bug existed in how an anchor was interpreted on the next
  request, so a single static request file was insufficient evidence.

Evidence reviewed:

- `tests/netdata_function/README.md` documents semantic comparison but only for
  command/request pairs.
- `tests/netdata_function/requests/` has no committed `anchor` fixture and no
  forward-direction fixture.
- `rust/src/journal/src/netdata.rs` and `go/journal/netdata_test.go` include
  SDK-internal paging/tail/delta contract tests.

Affected contracts and surfaces:

- `tests/netdata_function` comparator and validation harnesses.
- SDK Netdata function replacement confidence for Netdata UI logs behavior.
- Rust and Go Netdata function wrapper tail/no-change status behavior.
- Rust and Go Netdata function wrapper tail+delta anchor accounting and
  histogram bucket shape.
- SOW status ledgers and test documentation.

Existing patterns to reuse:

- Reuse `run_function_compare.py` command execution shape.
- Reuse `compare_function_json.py` for normalized content comparisons.
- Keep raw generated JSON under `.local/` only.
- Keep request JSON flowing through stdin in `--test` mode.

Risk and blast radius:

- Low production-code blast radius if this SOW only adds tests/harnesses.
- Medium integration-risk reduction because the new harness targets the exact
  UI contract class that previously regressed.
- Security risk is limited by stdin request payloads and sanitized reports.

Sensitive data handling plan:

- Durable files will include only harness code, synthetic request definitions,
  and sanitized descriptions.
- Raw plugin/SDK JSON outputs from real journals stay under `.local/` and are
  not committed.
- No bearer tokens, cookies, SNMP communities, personal data, customer names,
  private endpoints, or journal payloads will be copied into the SOW or docs.

Implementation plan:

1. Add a stateful comparison runner under `tests/netdata_function/` that can
   execute named sequence specs and derive anchors from prior step output.
2. Add unit tests using small synthetic JSON fixtures to prove anchor
   derivation, duplicate detection, and per-step comparison behavior.
3. Update README and SOW status ledgers.
4. Run local unit tests and a real side-by-side smoke against the installed
   plugin and SDK wrapper when binaries are available.

Validation plan:

- `python -m unittest tests.netdata_function.test_compare_function_json`
- New Python unit tests for the stateful runner.
- Build or locate the Rust SDK wrapper.
- Run at least one real stateful side-by-side sequence against a repository
  fixture or local sanitized journal directory and the installed Netdata
  `systemd-journal.plugin`, with reports under `.local/`.
- `git diff --check`
- `.agents/sow/audit.sh`
- Read-only reviewer pool after local validation.

Artifact impact plan:

- AGENTS.md: no expected update; this is a test harness gap, not a project-wide
  workflow change.
- Runtime project skills: no expected update unless reviewers identify a
  recurring workflow rule.
- Specs: no expected update; the Netdata function contract was already
  introduced by earlier SOWs, this SOW adds validation coverage.
- End-user/operator docs: update `tests/netdata_function/README.md`.
- End-user/operator skills: no expected update.
- SOW lifecycle: create this SOW in `current/`, close only after validation and
  reviewer disposition.
- SOW-status.md: update current/completed state.

Open-source reference evidence:

- None needed for implementation. The target is local SDK versus installed
  Netdata plugin behavior through the established command contract.

Open decisions:

- None. The user requested closing the gap and the implementation is
  test-only/harness-only unless validation exposes a product-code regression.

## Implications And Decisions

- No new user decision is required before implementation. If the harness finds
  a real SDK/plugin content mismatch, the mismatch will be recorded and the
  relevant fix will be handled in this SOW only if it is directly inside the
  SDK Netdata function contract.

## Plan

1. Add stateful sequence runner and tests.
2. Run local unit validation.
3. Run real SDK-vs-plugin smoke.
4. Fix any harness or SDK issues exposed by the stateful comparison.
5. Run reviewers, audit, update SOW, and close.

## Delegation Plan

Implementer:

- Local implementation by the project manager, following current repository
  routing.

Reviewers:

- `llm-netdata-cloud/glm-5.1`
- `llm-netdata-cloud/kimi-k2.6`
- `llm-netdata-cloud/mimo-v2.5-pro`
- `llm-netdata-cloud/qwen3.6-plus`
- `llm-netdata-cloud/minimax-m3-coder`
- `llm-netdata-cloud/deepseek-v4-pro`

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

- Validation failures are recorded in this SOW and fixed before close.
- Reviewer findings are recorded with dispositions and re-reviewed until no
  production-grade blockers remain.
- Audit failures block close.

## Execution Log

### 2026-06-09

- Created SOW after confirming committed side-by-side request fixtures do not
  cover anchors or forward paging.
- Added `tests/netdata_function/run_stateful_function_compare.py` and Python
  unit coverage for anchor derivation, duplicate detection, tail row checks,
  and sequence selection.
- Added stateful SDK-vs-plugin sequences for backward paging, forward paging,
  positive tail then `304`, filtered tail with empty `200`, and tail delta.
- Fixed SDK/plugin content gaps found by the harness and existing one-shot
  fixtures:
  - `ND_JOURNAL_PROCESS` now matches plugin fallback order when `_PID` is
    absent (`identifier[-]`) or present but empty (`identifier`).
  - Tail requests keep the pre-scan `304` path for "no newer selected files"
    but no longer convert "newer file exists, filters match no rows" into a
    post-scan `304`.
  - Tail+delta keeps the `anchor+1` row scan for facet correctness, adds the
    anchor boundary to delta page counters, and renders histograms against the
    original request timeframe.
  - `info` source summaries now render sub-second coverage as `off`, matching
    the plugin.
  - Explicitly requested facets are reported even when they have no values, and
    requested empty histograms are rendered with the plugin bucket shape.

## Validation

Acceptance criteria evidence:

- Stateful harness added and committed under `tests/netdata_function/`.
- Harness runs SDK first, then plugin, and derives anchors from previous SDK
  responses only after semantic SDK/plugin comparison succeeds.
- Harness compares every step with the existing normalized comparator.
- Final stateful side-by-side report under
  `.local/netdata_function_stateful/20260609T155751Z-sow0101-final/stateful/report.json`
  passed:
  - backward paging: 4 steps, 12 collected rows, 12 unique rows;
  - forward paging: 4 steps, 12 collected rows, 12 unique rows;
  - tail newer then no-change `304`: 3 steps;
  - filtered tail with newer files and no matching rows: 2 steps, `200` empty
    data matched plugin;
  - tail delta: 2 steps, `facets_delta`, `histogram_delta`, and `items_delta`
    matched plugin.
- Existing one-shot SDK-vs-plugin fixture suite passed all 10 committed request
  fixtures under
  `.local/netdata_function_stateful/20260609T155751Z-sow0101-final/one-shot/`.

Tests or equivalent validation:

- `python -m unittest tests.netdata_function.test_compare_function_json tests.netdata_function.test_stateful_function_compare` - passed, 32 tests.
- `cargo test --manifest-path rust/Cargo.toml -p systemd-journal-sdk` -
  passed, 114 tests.
- `go test ./...` from `go/` with repository-local Go caches - passed.
- `cargo build --manifest-path rust/Cargo.toml -p netdata_function_wrapper` -
  passed after Rust changes.
- `git diff --check` - passed.
- `.agents/sow/audit.sh` - passed.

Real-use evidence:

- Real side-by-side comparison was run against installed
  `/usr/libexec/netdata/plugins.d/systemd-journal.plugin` and the rebuilt SDK
  wrapper on a synthetic journal directory generated by `livewriter`.
- The stateful harness validated actual plugin behavior for multi-call paging,
  tail, and delta sequences, not only static JSON fixtures.
- The one-shot comparator validated the existing request corpus after the
  stateful fixes.

Reviewer findings:

- `llm-netdata-cloud/glm-5.1` - `PRODUCTION GRADE`. Non-blocking notes:
  `reportable_facet_fields_bytes` now names a pass-through requested facet
  list; stateful validation depends on the installed plugin binary; root
  `SOW-status.md` remains a convenience index.
- `llm-netdata-cloud/qwen3.6-plus` - `PRODUCTION GRADE`. Non-blocking notes:
  empty response anchor helpers intentionally fail on no returned rows; one-shot
  fixtures still cover static data-only requests; stateful harness covers
  forward paging.
- `llm-netdata-cloud/minimax-m3-coder` - `PRODUCTION GRADE`. Non-blocking
  notes: a few redundant harness assertions and helper inlining opportunities
  exist but are not defects.
- `llm-netdata-cloud/deepseek-v4-pro` - `PRODUCTION GRADE`. Non-blocking note:
  add an isolated boundary unit test for sub-second source coverage rendering.
  Disposition: added Rust and Go tests proving `<1s` renders `off` and exactly
  `1s` renders `1s`; full Rust/Go/Python validation was rerun.
- `llm-netdata-cloud/mimo-v2.5-pro` - initial `NOT PRODUCTION GRADE` for
  lifecycle hygiene only: the new harness files were still untracked and the
  SOW still said reviewer findings were pending during review. Disposition:
  final staged-state rerun after moving the SOW to `done/` and explicitly
  staging the new harness files returned `PRODUCTION GRADE`; no code defect was
  reported.
- `llm-netdata-cloud/kimi-k2.6` - unavailable due quota during this review
  window; no code signal.

Same-failure scan:

- Same-failure scan covered both the new stateful sequence suite and all
  existing one-shot Netdata function boundary fixtures.
- The scan found additional related empty-output parity gaps in `info`,
  requested facets, and empty histograms; those were fixed in this SOW.

Sensitive data gate:

- Raw SDK/plugin JSON outputs stayed under `.local/`.
- Durable SOW/docs mention only sanitized relative report locations and summary
  counts.
- No secrets, cookies, personal data, customer identifiers, or raw real journal
  payloads were copied into durable artifacts.

Artifact maintenance gate:

- AGENTS.md: no update needed; no project-wide workflow rule changed.
- Runtime project skills: no update needed; existing SOW and compatibility
  skills were sufficient.
- Specs: no separate spec update needed; this SOW repairs implementation/test
  parity for the previously specified Netdata function wrapper contract.
- End-user/operator docs: `tests/netdata_function/README.md` updated for
  stateful comparisons and filtered-tail empty-`200` behavior.
- End-user/operator skills: no update needed; no exported skill behavior
  changed.
- SOW lifecycle: SOW moved to `done/` during close after validation and reviewer
  disposition.
- SOW-status.md: current and root status ledgers updated for completion.

Specs update:

- No separate spec update required; no new public product contract was added.
  The work makes the SDK match the already intended
  `systemd-journal.plugin`-compatible Netdata function contract.

Project skills update:

- No project skill update required.

End-user/operator docs update:

- `tests/netdata_function/README.md` updated.

End-user/operator skills update:

- No end-user/operator skill update required.

Lessons:

- Stateful function contracts cannot be proven by single static requests.
- Empty-result responses are content-bearing in Netdata functions: requested
  facet groups, histogram shape, and info strings must still match.

Follow-up mapping:

- No follow-up is required for this SOW. Reviewer notes were either addressed in
  this SOW or are non-blocking maintainability observations without a concrete
  behavior gap.

## Outcome

Completed. The SDK Netdata function wrapper now has stateful side-by-side
coverage against the installed Netdata `systemd-journal.plugin` for anchors,
forward/backward paging, tail `304`, filtered tail empty `200`, and delta
facets/histograms. Rust and Go wrapper behavior was repaired where the harness
found plugin-content gaps.

## Lessons Extracted

- Use stateful SDK-vs-plugin harnesses for UI contracts where anchors,
  tailing, or delta state affect the next request.
- Run the older one-shot fixture suite after stateful fixes; it can expose
  adjacent parity gaps that are invisible in tail-focused tests.

## Followup

None.

## Regression Log

None yet.
