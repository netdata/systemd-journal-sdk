# SOW-0124 - Netdata Query-Wide Anchor Regression

## Status

Status: in-progress

Sub-state: surgical SDK repair analysis under reviewer verification; shared external fixture/query tests prove the same-anchor boundary loss while production logic remains intentionally unchanged.

## Requirements

### Purpose

Restore Netdata logs UI paging correctness. The Netdata function must expose anchors that act as one ordered query-wide cursor across all selected journal files and streams, so incremental retrieval can fetch more rows without duplicates, missed rows, or per-file paging artifacts.

### User Request

Create an SOW to fix the regression where anchors are not already multi-file, query-level anchors. The user clarified that the UI assumes anchors are query-wide and ordered: a later anchor is bigger than an earlier anchor.

### Assistant Understanding

Facts:

- The Netdata UI stores one scalar `anchorAfter` and `anchorBefore` for the whole table/query, not per-file anchors.
- The UI derives anchors from the first and last rendered rows of the current merged table.
- The UI sends one scalar `anchor` on load-more and tail polling.
- The UI relies on backend `items.after` and `items.before` for whether more data exists; it does not infer this from `row_count == 200`.
- The Rust and Go SDK Netdata wrappers currently parse one scalar timestamp anchor and apply it inside per-file Explorer queries before after-the-fact merging.
- Rust and Go directory readers already have stronger multi-file merge keys than visible row timestamp.
- User performance finding: row-by-row k-way multi-file traversal is significantly slower than per-file query plus merge for large journal sets, because many source readers must be considered repeatedly while advancing the global stream.

Inferences:

- The current SDK Netdata implementation is compatible with simple timestamp-window paging in many cases, but it does not implement the stronger UI contract: one ordered anchor over the complete multi-file query stream.
- A correct implementation should separate the anchor time from displayed/event time. The anchor should be based on internal journal order, while visible `_SOURCE_REALTIME_TIMESTAMP` adjustment remains output policy.
- Same-anchor boundary groups across files need explicit semantics so the next page cannot miss rows with the same internal anchor value.
- The fix should preserve the current per-file batched query and merge shape for performance, while making the batch merge contract query-wide and internal-anchor based.

Unknowns:

- Whether a completed SOW explicitly claimed multi-file query-wide anchor correctness. Implementation investigation must search SOW-0082, SOW-0093, and SOW-0101 before coding; if a completed SOW made that exact claim, follow the project regression-reopen procedure instead of treating this as a fresh SOW.
- Whether the final wire-compatible anchor can remain only the internal realtime timestamp, or whether a later additive cursor token is needed for pathological low-level files.

### Acceptance Criteria

- Rust Netdata function pagination treats selected journal files as one logical query stream for data rows and anchors, with tests covering overlapping file ranges.
- Go Netdata function pagination matches the Rust behavior and tests.
- The backend returns rows in globally ordered anchor order for forward/backward paging and tail polling.
- The anchor used for paging is internal journal order time, not adjusted visible source/event time.
- Same-anchor rows across files are handled without skipped or duplicated rows across page boundaries.
- Anchors are ordered scalar values: they do not need to be continuous, but each next page must be strictly outside the previous scalar anchor and must not overlap an earlier page.
- `items.after` and `items.before` remain correct for the UI's more-to-load logic.
- Existing Netdata request shape remains backward compatible unless the user explicitly approves an additive API.
- The implementation does not replace per-file batched retrieval with row-by-row k-way traversal.

## Analysis

Sources checked:

- `rust/src/journal/src/netdata.rs`
- `go/journal/netdata.go`
- `rust/src/journal/src/directory.rs`
- `go/journal/directory_reader.go`
- `rust/src/journal/src/explorer.rs`
- `go/journal/explorer.go`
- `netdata/cloud-frontend @ b0f9c41cfc36`
  - `src/domains/functions/useFetch/index.js`
  - `src/domains/functions/components/table/index.js`
  - `src/domains/functions/useFetch/normalizers/table/index.js`
  - `src/domains/functions/atom.js`

Current state:

- Rust parses a scalar `anchor` request value into `ExplorerAnchor::Realtime`: `rust/src/journal/src/netdata.rs:3547`.
- Go parses the same scalar shape: `go/journal/netdata.go:3071`.
- Rust applies anchor filtering inside row evaluation as `realtime_usec > anchor` for forward and `<= anchor` for backward: `rust/src/journal/src/explorer.rs:3062`.
- Go mirrors that per-row anchor predicate: `go/journal/explorer.go:2627`.
- Rust combines per-file results after each file and sorts by `row.realtime_usec`: `rust/src/journal/src/netdata.rs:2853`, `rust/src/journal/src/netdata.rs:2187`.
- Go combines per-file results after each file and sorts by `Row.RealtimeUsec`: `go/journal/netdata.go:1102`, `go/journal/netdata.go:1135`.
- Rust directory reader k-way merges files with a stronger key: seqnum id/seqnum, boot id/monotonic, realtime, xor hash: `rust/src/journal/src/directory.rs:282`.
- Go directory reader has the same key shape: `go/journal/directory_reader.go:858`.
- The UI sends one scalar anchor on table load-more: `netdata/cloud-frontend @ b0f9c41cfc36 src/domains/functions/components/table/index.js:227`.
- The UI derives `anchorBefore` and `anchorAfter` from the last and first rendered rows: `netdata/cloud-frontend @ b0f9c41cfc36 src/domains/functions/useFetch/normalizers/table/index.js:254`.
- The UI prepends forward pages and appends backward pages: `netdata/cloud-frontend @ b0f9c41cfc36 src/domains/functions/useFetch/normalizers/table/index.js:240`.
- The UI uses `items.after/items.before`, not `== 200`, to detect whether more pages exist: `netdata/cloud-frontend @ b0f9c41cfc36 src/domains/functions/useFetch/normalizers/table/index.js:293`.

Risks:

- Duplicate rows: per-file anchor filtering can include a row that is before the global query anchor when another file provided the displayed boundary row.
- Missing rows: if a page boundary lands on a timestamp shared across files, per-file limiting and final truncation can drop rows that should have been returned before the next anchor becomes exclusive.
- UI breakage: the UI stores only one scalar anchor, so per-file continuation cannot be represented by the current frontend contract.
- Performance regression: a naive fix that scans all rows from all files or advances a k-way multi-file stream row-by-row would violate the journal-native performance contract and the user performance finding.
- API churn: adding a compound cursor token may be cleaner, but the existing UI and function API expect a scalar anchor today.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The SDK Netdata wrappers treat `anchor` as a scalar timestamp but apply it inside each file's Explorer query and then merge limited per-file results.
- The UI contract is stronger: the first/last row of the merged query result is the cursor for the whole query.
- The existing Rust/Go Netdata wrappers already use the performance-friendly per-file batch retrieval shape, but the merge uses displayed row timestamp and per-file anchor semantics instead of a query-wide internal-anchor contract.

Evidence reviewed:

- SDK Rust and Go Netdata wrappers listed in `## Analysis`.
- SDK Rust and Go directory merge-key evidence listed in `## Analysis`.
- UI evidence from `netdata/cloud-frontend @ b0f9c41cfc36` listed in `## Analysis`.
- SOW provenance check:
  - SOW-0082 covered Rust Explorer direction/anchor support, not Netdata multi-file query-wide boundary anchors.
  - SOW-0093 covered tail-anchor and ordinary page-anchor strictness, with single-file paging tests.
  - SOW-0101 covered stateful SDK/plugin paging, tailing, and delta behavior; its anchor tests derive anchors from previous responses but do not record a multi-file same-anchor boundary contract.
- No live host journals were probed.

Affected contracts and surfaces:

- Rust Netdata function request handling and row retrieval.
- Go Netdata function request handling and row retrieval.
- Netdata function response `items.before`, `items.after`, `pagination`, `anchor`, and row ordering.
- Tests for multi-file overlapping journals, source realtime display adjustment, forward/backward paging, and tail polling.
- Specs documenting Netdata journal function paging.

Existing patterns to reuse:

- Rust `DirectoryReader::compare_entry_keys()` for global ordering semantics, without adopting row-by-row k-way traversal.
- Go `DirectoryReader.compareEntryKeys()` for global ordering semantics, without adopting row-by-row k-way traversal.
- Existing per-file Explorer query and after-the-fact merge shape, because it is the performance-preferred architecture for many sources.
- Existing Netdata stateful pagination tests from SOW-0101.
- Existing Explorer control callbacks and selected-file metadata flow.

Risk and blast radius:

- High UI regression risk if row order or anchors change unexpectedly.
- Medium API compatibility risk if a compound token is introduced too early.
- High performance risk if the fix changes the retrieval architecture to row-by-row k-way traversal across many large source files.
- Low sensitive-data risk if tests use synthetic journals only.

Sensitive data handling plan:

- Use synthetic fixtures and generated journal files for validation.
- Durable SOW/spec/test evidence must not include raw customer names, community member names, private endpoints, bearer tokens, SNMP communities, or customer-identifying IP addresses.
- External UI evidence is cited by repository, commit, and relative path only.

Implementation plan:

1. Add shared external fixture/query tests that create multiple synthetic journal files with rows sharing the same internal realtime timestamp, request fewer rows than the boundary group size, and assert the response does not truncate that query-wide anchor group.
2. Add shared external fixture/query tests that page with the UI-style scalar anchor from the first response and assert the complete multi-file group is not lost.
3. Define the anchor contract in specs: scalar internal timestamp anchor remains the wire-compatible contract; visible/event timestamp is output only.
4. Keep per-file batched retrieval: each selected file may return up to the requested page size after/before the scalar internal anchor, then the SDK performs a global merge and truncation/extension by internal anchor order.
5. Change the retained-row ordering and page-edge anchor derivation to use the internal journal anchor timestamp, not adjusted visible row timestamp.
6. Add boundary-group handling without row-by-row k-way traversal: after the initial global top-N merge identifies boundary internal timestamp `T`, include all already discovered rows at `T` and, if needed, query only files whose internal range can contain `T` for additional `T` rows.
7. Implement Go parity.
8. Add synthetic overlapping-file tests for forward page, tail poll, source realtime display adjustment, ordered-scalar non-overlap, and many-source pruning if the initial red tests do not already cover those affected contracts.
9. Verify `items.after/items.before` semantics with variable row counts.

Validation plan:

- Shared external fixture/query tests for multi-file query-level anchors, reusable against the installed plugin and any SDK wrapper.
- Focused Rust Netdata tests for multi-file query-level anchors if implementation needs lower-level diagnostics beyond the shared runner.
- Focused Go Netdata tests for the same scenarios if implementation needs lower-level diagnostics beyond the shared runner.
- Existing Rust and Go Netdata pagination/tail tests.
- Same-failure scan for per-file anchor application and post-merge timestamp truncation.
- SOW audit and `git diff --check`.
- External reviewer gate after implementation because this touches UI-facing paging semantics.

Artifact impact plan:

- AGENTS.md: likely unaffected unless implementation changes project workflow.
- Runtime project skills: likely unaffected unless a new recurring anchor-validation workflow is discovered.
- Specs: update `.agents/sow/specs/systemd-journal-plugin-facets.md` with query-wide anchor semantics.
- End-user/operator docs: likely unaffected unless public SDK docs currently describe anchor semantics.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: this SOW remains pending/open until implementation starts; if a completed SOW is identified as the original claim, lifecycle changes may move to reopened regression handling.
- SOW-status.md: update to list this pending regression SOW.

Open-source reference evidence:

- No third-party open-source reference was checked for this SOW creation. The regression is between this SDK and Netdata UI integration contract, and the required UI evidence came from `netdata/cloud-frontend @ b0f9c41cfc36`.

Open decisions:

1. Boundary group policy.
   - Option A, long-term-best: if the requested limit lands inside a same-internal-anchor group across files, return the whole boundary group, so row count may be greater than requested.
   - Option B, surgical: enforce the requested limit exactly and add a compound cursor token before fixing boundary groups.
   - Decision: Option A, long-term-best. The user agreed on 2026-06-25 that query-wide anchors are the key requirement and that returning more or fewer than the requested number of rows is acceptable when a page boundary lands inside a same-internal-anchor group across files.
2. Wire anchor shape.
   - Option A, surgical: keep existing scalar `anchor` and define it as internal journal order time for Netdata paging.
   - Option B, long-term-best: add an opaque page token with file/order tie-breakers while keeping scalar `anchor` backward compatible.
   - Decision: Option A for this regression SOW. Keep the existing scalar `anchor` request shape and define it as internal journal order time for Netdata paging; track an additive opaque token only if implementation evidence proves the scalar contract cannot satisfy the UI behavior.
3. Retrieval architecture.
   - Decision: keep per-file batched query plus global merge. Do not implement row-by-row k-way multi-file traversal for this regression.
   - Reason: the user has measured k-way multi-file queries as significantly slower than per-file query and merge for many large sources.

## Implications And Decisions

1. User decision recorded 2026-06-25: the key requirement is multi-file, query-level anchors.
2. User decision recorded 2026-06-25: the UI's current scalar query-wide anchor contract is valid and the SDK behavior that does not satisfy it is a regression.
3. User decision recorded 2026-06-25: the implementation must not switch to row-by-row k-way multi-file traversal; preserve per-file query plus merge for performance.
4. User decision recorded 2026-06-25: add test-first regression evidence before production implementation. The first implementation chunk is shared external fixture/query tests plus SOW evidence only.
5. User decision recorded 2026-06-25: same-internal-anchor boundary groups may return more than the requested `last` count so a scalar query-wide anchor cannot skip rows.
6. User decision recorded 2026-06-25: do not fix `systemd-journal.plugin` in this SOW. The SDK is the target; the installed plugin is current-behavior evidence, not the correctness oracle for the same-anchor boundary case.
7. User decision recorded 2026-06-25: anchor values must be ordered scalar values. They do not need to be continuous, but pages must be ordered and non-overlapping.

## Plan

1. Add red shared external multi-file same-anchor boundary tests without production logic changes.
2. Run the focused tests and record the expected failures as regression proof.
3. Write the Netdata query-wide internal anchor contract into the spec.
4. Implement Rust first using per-file batched retrieval plus internal-anchor global merge.
5. Port the same behavior and tests to Go.
6. Run local validation and reviewer gate.

## Surgical SDK Repair Analysis - 2026-06-25

### Diagnosis

Facts:

- Rust and Go Netdata wrappers install a realtime-adjust callback before
  Explorer range filtering:
  - Rust: `rust/src/journal/src/netdata.rs:591-593`.
  - Go: `go/journal/netdata.go:1048` configures the same callback path.
- Explorer applies that adjusted timestamp before checking request time bounds:
  - Rust: `rust/src/journal/src/explorer.rs:1608-1614`.
  - Go: `go/journal/explorer.go:2122-2128`.
- The combined Netdata result then sorts rows, makes duplicate timestamps
  unique, and truncates to `last`:
  - Rust: `rust/src/journal/src/netdata.rs:2187-2198`.
  - Go: `go/journal/netdata.go:1135-1147`.
- Same-timestamp rows can be valid across files. The user clarified that
  journald increments the internal timestamp inside one file, but collisions can
  happen across files and therefore must be handled at the query level.
- Remaining-file pruning already keeps equality conservative:
  - Rust backward pruning stops only when the next file's last timestamp is
    strictly lower than the retained boundary after slack:
    `rust/src/journal/src/netdata.rs:2993-2999`.
  - Go mirrors this:
    `go/journal/netdata.go:2315-2324`.

Root-cause model:

- The early realtime-adjust callback turns equal timestamps into unique values
  before `timestamp_in_range()` and `row_within_anchor()` run. In the synthetic
  boundary fixture, rows after the first can be shifted below the lower bound
  and are rejected before the merge can see the full query-wide boundary group.
- The final `sort_and_limit()` path also mutates duplicate timestamps before
  truncating. If multiple files share the boundary timestamp, truncating after
  mutation can split a query-wide same-anchor group and make later rows
  unreachable through the scalar anchor.

### Recommended Surgical Plan

Classification: surgical.

This plan intentionally avoids a request/response schema change, compound
cursor token, core reader rewrite, or row-by-row k-way traversal.

1. Stop using the Netdata realtime-adjust callback for row selection.
   - Rust: remove `NetdataRealtimeAdjuster` from `explore_selected_files()` /
     `explore_single_file()` control setup.
   - Go: remove `netdataRealtimeAdjuster` from `exploreSelectedFiles()` /
     `configureNetdataExplorerControl()`.
   - Reason: display-time de-duplication must not run before time-window and
     anchor predicates.
2. Make backward realtime-anchor paging exclusive for every non-tail row query,
   not only `data_only` queries.
   - Rust: in `NetdataRequest::to_explorer_query()`, treat any non-tail
     backward realtime anchor as an exclusive upper bound by applying
     `before_realtime_bound_excluding_anchor()` and clearing `query.anchor` to
     `ExplorerAnchor::Auto`.
   - Go: mirror the same rule in `netdataRequest.applyExplorerBounds()`.
   - Reason: after page 1 returns the complete boundary group at timestamp `T`,
     page 2 with `anchor=T` must not re-fetch `T` in non-`data_only`
     backward queries. Forward paging is already strict (`> anchor`) and tail
     already uses `anchor + 1`.
   - Keep `NetdataPageWindow` strict-anchor accounting aligned; its backward
     `anchor_start_usec` path already rejects `realtime_usec >= anchor`.
3. Change combined row retention to truncate by raw pre-display timestamp
   boundary, not by mutated display timestamp.
   - Sort rows by raw `row.realtime_usec` / `Row.RealtimeUsec` in the requested
     direction, with deterministic tie-breakers by file path and cursor for
     stable cross-language output.
   - If `rows.len() > limit`, compute the raw boundary timestamp at
     `limit - 1`.
   - Backward pages retain all rows with raw timestamp greater than or equal to
     that boundary; forward pages retain all rows with raw timestamp less than
     or equal to that boundary.
   - This allows returning more than `last` only when required to include the
     query-wide boundary group.
   - Guard `limit == 0` in Rust and `limit < 0` / `limit == 0` in Go before
     computing `limit - 1`.
4. Preserve duplicate timestamp equality across files.
   - Replace the current unconditional `make_row_timestamps_unique()` call with
     a scoped variant that adjusts duplicate timestamps only inside a
     same-file duplicate run.
   - Do not adjust equal timestamps across different files; those are the exact
     internal timestamp collisions the scalar query-wide anchor must represent.
   - This preserves the existing single-file duplicate-display behavior as much
     as possible while making cross-file anchors ordered and non-overlapping.
5. Keep per-file batched retrieval and conservative file pruning.
   - Do not change Explorer index/filter/facet logic.
   - Do not change directory reader ordering.
   - Do not change `remaining_files_cannot_affect_data_page()` equality
     behavior; equality must continue to force reading the next file.
6. Keep the existing scalar `anchor` API.
   - No new request key.
   - No new response cursor/token.
   - No public SDK API surface should change; this is an internal Netdata
     wrapper behavior fix.
7. Test Rust and Go with the shared external runner.
   - `query-wide-noncollision` must keep passing.
   - `same-anchor-boundary` must pass for Rust and Go.
   - Add and pass a non-`data_only` backward same-anchor boundary scenario.
   - Add and pass a forward same-anchor boundary scenario.
   - The installed plugin may remain an allowed failure for
     `same-anchor-boundary` because it is not being fixed in this SOW.

### Expected Code Touch Points

- Rust:
  - `rust/src/journal/src/netdata.rs`
    - remove early Netdata realtime adjuster wiring;
    - make non-tail backward realtime anchors exclusive for both `data_only`
      and non-`data_only` query rows;
    - update `CombinedResult::merge()` / `sort_and_limit()`;
    - update or replace `make_row_timestamps_unique()`;
    - update focused unit tests near existing Netdata pagination and duplicate
      timestamp tests.
- Go:
  - `go/journal/netdata.go`
    - mirror the Rust wrapper changes;
    - make non-tail backward realtime anchors exclusive for both `data_only`
      and non-`data_only` query rows;
    - update `netdataCombinedResult.merge()` / `sortAndLimit()`;
    - update or replace `makeRowTimestampsUnique()`.
  - `go/journal/netdata_test.go`
    - mirror focused Rust test coverage.
- Shared tests:
  - `tests/netdata_function/run_anchor_regression.py` should be the acceptance
    gate for the multi-source query behavior.

### Risk Controls

- API risk: low. The plan keeps the existing scalar `anchor` request shape and
  the existing response columns.
- Performance risk: low. Per-file batched retrieval is preserved; the only
  extra work is retaining all rows at the final boundary timestamp already
  returned by per-file batches.
- Compatibility risk: medium-low. Cross-file equal timestamps will remain equal
  instead of being display-adjusted. This is intentional for the query-wide
  anchor contract, but reviewer verification should check source-realtime
  duplicate edge cases.
- Visible timestamp behavior changes for cross-file equal timestamps: those
  rows intentionally remain equal in the output because the timestamp column is
  the scalar anchor surface. Same-file duplicate display adjustment remains
  scoped where applicable.
- Regression risk to single-file duplicate display behavior: bounded by the
  scoped same-file duplicate adjustment and existing duplicate timestamp unit
  tests.
- Security risk: low. The change does not add input parsing, subprocesses,
  host probing, or new file access.

### Validation Plan For Implementation

- Rust focused tests for:
  - combined sort/limit includes the whole raw boundary group;
  - cross-file duplicate timestamps are not adjusted;
  - same-file duplicate timestamps still follow the existing direction-specific
    adjustment behavior where applicable.
  - non-tail backward realtime anchors are converted to an exclusive upper
    bound for both `data_only` and non-`data_only` requests.
- Go focused tests mirroring Rust.
- Shared external runner:
  - Rust and Go pass `query-wide-noncollision`.
  - Rust and Go pass `same-anchor-boundary`.
  - Rust and Go pass non-`data_only` backward same-anchor boundary.
  - Rust and Go pass forward same-anchor boundary.
  - Plugin is only used as current-behavior evidence and may be `--allow-fail`
    on `same-anchor-boundary`.
- Validate `items.after` / `items.before` on at least one non-`data_only`
  boundary-group scenario so variable returned row counts do not produce a
  false "no more rows" signal.
- Existing Netdata helper tests:
  - `python3 -m unittest tests.netdata_function.test_anchor_regression`
  - `python3 -m unittest tests.netdata_function.test_stateful_function_compare tests.netdata_function.test_compare_function_json`
- Focused Rust/Go Netdata test suites.
- Full `cargo test` / `go test ./...` if focused implementation touches only
  these files cleanly; record any inability to run full suites.
- `git diff --check`, SOW audit, and sensitive-data/name scan before closing.

### Reviewer Gate - Plan Verification

Round 1 read-only verification:

- glm: `READY TO IMPLEMENT: NO`.
  - Blocking finding: the plan fixed `data_only` backward anchors but left a
    reachable non-`data_only` backward anchor path using inclusive
    `row_within_anchor <= anchor`, so returning a whole boundary group on page 1
    could duplicate that group on page 2.
- qwen: `READY TO IMPLEMENT: NO`.
  - Same blocking finding: backward `row_within_anchor()` must be made
    exclusive through the Netdata request-to-query path, and forward
    same-anchor coverage should be added.
- deepseek: `READY TO IMPLEMENT: YES`.
  - Non-blocking notes: guard limit edge cases, keep deterministic tie-breakers,
    and validate `items.after` / `items.before`.
- kimi: `READY TO IMPLEMENT: YES`.
  - Confirmed diagnosis and same-file scoped uniqueness feasibility.
- mimo: `READY TO IMPLEMENT: YES`.
  - Confirmed page-window exclusive accounting does not need changes.
- minimax: no valid vote; the run timed out after 30 minutes. The partial
  transcript contained analysis consistent with the diagnosis and an apparent
  `READY TO IMPLEMENT: YES`, but the timed-out run is not counted as a
  completed vote.

Disposition:

- The plan now explicitly extends backward realtime-anchor exclusion to all
  non-tail backward row queries, not only `data_only`.
- The validation plan now includes non-`data_only` backward same-anchor and
  forward same-anchor shared scenarios.
- A second reviewer round must use the same whole scope plus these fix notes
  before implementation starts.

Round 2 read-only verification after plan update:

- glm: `READY TO IMPLEMENT: YES`.
- minimax: `READY TO IMPLEMENT: YES`.
- kimi: `READY TO IMPLEMENT: YES`.
- mimo: `READY TO IMPLEMENT: YES`.
- deepseek: `READY TO IMPLEMENT: YES`.
- qwen: `READY TO IMPLEMENT: YES`.

Round 2 non-blocking implementation notes:

- Raw-boundary retention must run before any display-time timestamp mutation.
- Rust and Go must use matching deterministic tie-breakers for equal raw
  timestamps, including file path and cursor.
- Implementation must guard `limit == 0` in Rust and `limit <= 0` in Go before
  computing `limit - 1`.
- Remove the Netdata realtime-adjust callback wiring decisively. If the helper
  structs remain only for historical unit tests, keep that explicit and avoid
  dead production state.
- The spec update must record that non-tail backward realtime anchors are now
  exclusive for all row queries, not only `data_only`.
- The new shared scenarios must be real committed fixture/request pairs for
  non-`data_only` backward same-anchor and forward same-anchor paging.
- `items.after` / `items.before` validation must check for no false "no more
  rows" signal after boundary expansion.

Plan gate result:

- Round 2 reviewer consensus is READY TO IMPLEMENT.

## Delegation Plan

Implementer:

- Per project routing, implementation should be delegated to `llm-netdata-cloud/minimax-m3-coder` after the pre-implementation decisions are recorded.

Reviewers:

- Run the read-only reviewer pool after local implementation and validation: glm, kimi, mimo, qwen, deepseek, and the non-implementer pool model required by current project routing.

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

- Implementer failure, reviewer failure, audit failure, or model unavailability must be recorded in this SOW with exact command/model evidence and disposition.

## Execution Log

### 2026-06-25

- Created SOW from user request after local SDK and read-only UI evidence review.
- Spawned one read-only explorer agent to inspect `netdata/cloud-frontend`; it confirmed one scalar query-level anchor and ordered-edge semantics.
- Recorded user performance decision that row-by-row k-way multi-file traversal is not acceptable for this regression; revised plan to preserve per-file batched retrieval plus merge.
- Moved SOW-0124 to `current/` and changed status to `in-progress` for the test-first regression reproduction phase.
- Checked SOW-0082, SOW-0093, and SOW-0101 for exact prior multi-file query-wide same-anchor boundary claims. Prior work covered anchor support, tail strictness, and stateful single-file paging, but did not record the missing multi-file boundary class.
- Recorded user decisions for test-first reproduction, scalar anchor compatibility, whole-boundary-group returns, and per-file batched retrieval plus merge.
- Added initial Rust and Go red tests for a three-file same-internal-anchor boundary group with `last:2`, plus UI-style second-page anchor reuse. Per user direction, these language-local tests were then removed in favor of shared external fixture/query coverage under `tests/netdata_function/`.
- Added a shared same-anchor fixture spec and request pair:
  - `tests/netdata_function/fixtures/same-anchor-boundary.json`
  - `tests/netdata_function/requests/same-anchor-boundary-page1.json`
  - `tests/netdata_function/requests/same-anchor-boundary-page2-anchor.json`
- Ran scratch external fixtures before production logic changes. Rust and Go SDK wrappers both returned only `source-c` on the same-anchor collision page 1 and zero rows on page 2. The installed plugin returned `source-a`/`source-b` on page 1 and zero rows on page 2. All three peers lose at least one same-anchor row.
- Ran a separate non-collision three-source fixture to isolate query-wide multi-source paging from boundary collisions. Installed plugin, Rust SDK wrapper, and Go SDK wrapper all returned `source-c`/`source-b` on page 1 and `source-a` on page 2, so the basic multi-source scalar timestamp-anchor case passed in that fixture.
- Ran hygiene validation for the test-first chunk: `git diff --check` passed, `.agents/sow/audit.sh` passed, and a durable-artifact scan for the user's personal name returned no matches.
- Checked installed Netdata `systemd-journal.plugin` against the same scratch fixture. It also loses the third same-anchor row: first request returned `source-a` and `source-b`, and the second request with `anchor=1700000000000000` returned zero rows, so `source-c` was unreachable.
- User conclusion recorded: the plugin also has this bug. Therefore `systemd-journal.plugin` is useful as compatibility evidence for current behavior, but not as the correctness oracle for the stronger query-wide anchor requirement.
- Added ordered-scalar anchor checks to the shared external runner. The invariant requires ordered rows inside each page, strict non-overlap after the previous scalar anchor, and scalar-anchor progression when a next page has rows. The scalar may skip timestamp values; continuity is not required.
- Changed the non-collision positive-control fixture to use non-continuous scalar anchor values: `source-c` at `1700000000001000`, `source-b` at `1700000000000100`, and `source-a` at `1700000000000000`.
- Added Python unit tests for the shared runner's ordered-scalar validation logic, including backward and forward non-overlap, gap tolerance, page ordering, empty second page after a complete boundary group, edge-anchor derivation, and missing/duplicate message detection.

## Validation

Acceptance criteria evidence:

- In progress. External scratch fixture evidence separates the two cases:
  - Multi-source, non-collision query-wide paging with non-continuous scalar values passed for installed plugin, Rust SDK wrapper, and Go SDK wrapper.
  - Multi-source, same-anchor boundary collision failed for installed plugin, Rust SDK wrapper, and Go SDK wrapper.

Tests or equivalent validation:

- Temporary Rust/Go unit red tests were removed after the user requested external fixture/query tests independent of implementation language.
- External same-anchor collision scratch fixture:
  - Installed plugin page 1: `source-a`, `source-b`; page 2 with `anchor=1700000000000000`: no rows.
  - Rust SDK wrapper page 1: `source-c`; page 2 with `anchor=1700000000000000`: no rows.
  - Go SDK wrapper page 1: `source-c`; page 2 with `anchor=1700000000000000`: no rows.
- External non-collision multi-source scratch fixture after the ordered-scalar update:
  - Installed plugin page 1: `source-c @ 1700000000001000`, `source-b @ 1700000000000100`; page 2 with anchor from page 1: `source-a @ 1700000000000000`.
  - Rust SDK wrapper page 1: `source-c @ 1700000000001000`, `source-b @ 1700000000000100`; page 2 with anchor from page 1: `source-a @ 1700000000000000`.
  - Go SDK wrapper page 1: `source-c @ 1700000000001000`, `source-b @ 1700000000000100`; page 2 with anchor from page 1: `source-a @ 1700000000000000`.
  - Ordered-scalar validation passed for all peers: page rows were ordered, page 2 was strictly below anchor `1700000000000100`, and the next edge anchor progressed to `1700000000000000`.
- Shared runner helper tests:
  - `python3 -m unittest tests.netdata_function.test_anchor_regression`: passed, 8 tests.
- Existing Netdata comparator helper tests:
  - `python3 -m unittest tests.netdata_function.test_stateful_function_compare tests.netdata_function.test_compare_function_json`: passed, 70 tests.
- Syntax check:
  - `python3 -m py_compile tests/netdata_function/run_anchor_regression.py tests/netdata_function/test_anchor_regression.py`: passed.
- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed.

Real-use evidence:

- Read-only UI inspection at `netdata/cloud-frontend @ b0f9c41cfc36` confirms the frontend expects one scalar ordered query-level anchor.

Reviewer findings:

- One read-only explorer agent confirmed the UI anchor assumptions during SOW creation. Formal implementation reviewers have not run because implementation has not started.

Same-failure scan:

- Initial scan found Rust and Go Netdata wrappers both apply scalar anchors per file and sort/limit after merging by row timestamp. A full same-failure scan remains part of implementation.
- SOW provenance scan found existing single-file paging/tail anchor tests but no synthetic multi-file same-internal-anchor boundary test.
- The external collision fixture reproduced lost rows in installed plugin, Rust SDK wrapper, and Go SDK wrapper. The plugin is therefore a compatibility reference for current behavior, not a correctness oracle for same-anchor boundary collisions.
- The external non-collision fixture did not reproduce a basic query-wide multi-source paging failure in Rust or Go; the shared harness preserves that as a positive control while using the collision fixture as the red regression case.

Sensitive data gate:

- Durable evidence contains only repository paths, commit id, line references, and behavior summaries. No raw secrets, bearer tokens, SNMP communities, customer names, personal data, private endpoints, or customer-identifying IP addresses were recorded.
- A scan for the user's personal-name variants across the changed SOW, status, README, runner, helper tests, and shared fixture/request files returned no matches.

Artifact maintenance gate:

- AGENTS.md: no update needed for SOW creation.
- Runtime project skills: no update needed for SOW creation.
- Specs: update required when the production behavior is fixed; this test-first chunk records intended behavior in the active SOW and red tests but does not yet claim shipped spec reality.
- End-user/operator docs: no update needed until implementation determines public docs impact.
- End-user/operator skills: no update needed.
- SOW lifecycle: moved from `pending/open` to `current/in-progress` for test-first reproduction.
- SOW-status.md: updated with this SOW in current work.

Specs update:

- Required during implementation after the fix changes shipped behavior: `.agents/sow/specs/systemd-journal-plugin-facets.md`.

Project skills update:

- No project-skill update identified during SOW creation.

End-user/operator docs update:

- No docs update identified during SOW creation.

End-user/operator skills update:

- No end-user/operator skill impact identified during SOW creation.

Lessons:

- UI integration assumptions must be checked before treating a Netdata wire field as only a local per-file optimization.

Follow-up mapping:

- Boundary group policy and wire anchor shape decisions are tracked in this SOW before implementation.
- Retrieval architecture is decided: per-file batched query plus global internal-anchor merge, not row-by-row k-way traversal.

## Outcome

Not started.

## Lessons Extracted

Not extracted yet; implementation has not run.

## Followup

- Implement and validate the query-wide anchor fix after the red tests prove the regression.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
