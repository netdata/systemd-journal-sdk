# SOW-0124 - Netdata Query-Wide Anchor Regression

## Status

Status: completed

Sub-state: implementation, validation, reviewer gate, and SOW close completed; ready for done-directory move and commit.

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
- Working theory before implementation: a fully separated internal paging cursor would be cleaner than deriving paging from a returned timestamp column.
- Accepted regression scope after user decision 2026-06-25: keep the current scalar timestamp-anchor wire shape and make it query-wide, ordered, and non-overlapping.
- Same-anchor boundary groups across files need explicit semantics so the next page cannot miss rows with the same scalar anchor value.
- The fix should preserve the current per-file batched query and merge shape for performance, while making the batch merge contract query-wide for the existing scalar timestamp anchor.

Unknowns:

- Whether a completed SOW explicitly claimed multi-file query-wide anchor correctness. Implementation investigation must search SOW-0082, SOW-0093, and SOW-0101 before coding; if a completed SOW made that exact claim, follow the project regression-reopen procedure instead of treating this as a fresh SOW.
- Whether a later additive cursor token is needed for exact page-size continuation through very large same-scalar-anchor boundary groups.

### Acceptance Criteria

- Rust Netdata function pagination treats selected journal files as one logical query stream for data rows and anchors, with tests covering overlapping file ranges.
- Go Netdata function pagination matches the Rust behavior and tests.
- The backend returns rows in globally ordered anchor order for forward/backward paging and tail polling.
- The anchor used for paging remains the current ordered scalar timestamp surface and is applied query-wide, not per-file.
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
- The existing Rust/Go Netdata wrappers already use the performance-friendly per-file batch retrieval shape, but the merge used per-file anchor semantics instead of a query-wide scalar timestamp-anchor contract.

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

1. Add shared external fixture/query tests that create multiple synthetic journal files with rows sharing the same scalar timestamp, request fewer rows than the boundary group size, and assert the response does not truncate that query-wide anchor group.
2. Add shared external fixture/query tests that page with the UI-style scalar anchor from the first response and assert the complete multi-file group is not lost.
3. Define the accepted 1A anchor contract in specs: the existing scalar `timestamp` anchor remains the wire-compatible contract, is treated as an ordered scalar by consumers, and is applied query-wide.
4. Keep per-file batched retrieval: each selected file may return up to the requested page size after/before the scalar timestamp anchor, then the SDK performs a global merge and truncation/extension by scalar timestamp order.
5. Change retained-row ordering and page-edge anchor behavior to preserve equal cross-file scalar timestamps until after boundary-group retention.
6. Add boundary-group handling without row-by-row k-way traversal: after the initial global top-N merge identifies boundary timestamp `T`, include all already discovered rows at `T`.
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
- SOW lifecycle: move to `done/completed` when implementation, validation, and reviewer gates pass.
- SOW-status.md: update on close.

Open-source reference evidence:

- No third-party open-source reference was checked for this SOW creation. The regression is between this SDK and Netdata UI integration contract, and the required UI evidence came from `netdata/cloud-frontend @ b0f9c41cfc36`.

Open decisions:

1. Boundary group policy.
   - Option A, long-term-best: if the requested limit lands inside a same-scalar-anchor group across files, return the whole boundary group, so row count may be greater than requested.
   - Option B, surgical: enforce the requested limit exactly and add a compound cursor token before fixing boundary groups.
   - Decision: Option A, long-term-best. The user agreed on 2026-06-25 that query-wide anchors are the key requirement and that returning more or fewer than the requested number of rows is acceptable when a page boundary lands inside a same-scalar-anchor group across files.
2. Wire anchor shape.
   - Option A, surgical: keep the existing scalar `anchor` and existing `pagination.column = "timestamp"` response shape, and repair query-wide ordered scalar behavior for that current timestamp anchor surface.
   - Option B, long-term-best: add an opaque page token or hidden internal-anchor column with file/order tie-breakers while keeping scalar `anchor` backward compatible.
   - Decision: Option A for this regression SOW. The user confirmed on 2026-06-25 that this SOW should remain surgical and should not introduce a new API/response cursor. The implementation repairs the current scalar timestamp-anchor contract. A separate long-term design may split visible/effective event timestamp from an internal paging cursor.
3. Retrieval architecture.
   - Decision: keep per-file batched query plus global merge. Do not implement row-by-row k-way multi-file traversal for this regression.
   - Reason: the user has measured k-way multi-file queries as significantly slower than per-file query and merge for many large sources.

## Implications And Decisions

1. User decision recorded 2026-06-25: the key requirement is multi-file, query-level anchors.
2. User decision recorded 2026-06-25: the UI's current scalar query-wide anchor contract is valid and the SDK behavior that does not satisfy it is a regression.
3. User decision recorded 2026-06-25: the implementation must not switch to row-by-row k-way multi-file traversal; preserve per-file query plus merge for performance.
4. User decision recorded 2026-06-25: add test-first regression evidence before production implementation. The first implementation chunk is shared external fixture/query tests plus SOW evidence only.
5. User decision recorded 2026-06-25: same-scalar-anchor boundary groups may return more than the requested `last` count so a scalar query-wide anchor cannot skip rows.
6. User decision recorded 2026-06-25: do not fix `systemd-journal.plugin` in this SOW. The SDK is the target; the installed plugin is current-behavior evidence, not the correctness oracle for the same-anchor boundary case.
7. User decision recorded 2026-06-25: anchor values must be ordered scalar values. They do not need to be continuous, but pages must be ordered and non-overlapping.
8. User decision recorded 2026-06-25: this regression fix keeps the current scalar `anchor` and `pagination.column = "timestamp"` wire shape. It does not add a hidden internal cursor or opaque page token in this SOW.

## Future Reference - Option 1B

Option 1B was discussed on 2026-06-25 and is intentionally not part of this
regression SOW.

Purpose:

- Solve same-timestamp boundary collisions by continuation, not by returning
  the whole boundary group.
- Decouple the paging cursor from the current scalar `timestamp` response
  column.
- Allow the backend to return exactly `last` rows while still resuming without
  skips or duplicates inside an equal-timestamp group.

Likely shape:

- Add an opaque page token or a hidden internal-anchor column.
- Include enough tie-breaker state to resume inside a timestamp group, such as
  timestamp plus file identity and cursor/entry-order information.
- Keep the existing scalar `anchor` backward compatible during migration.

Implications:

- This is an API/consumer contract change. Consumers would need to store and
  send the new token or hidden cursor instead of deriving continuation only
  from the scalar `timestamp` column.
- It requires UI and SDK contract work, migration tests, and backward
  compatibility behavior for consumers that still send only scalar `anchor`.
- It is only necessary if the ordered-scalar 1A contract is not sufficient, for
  example if exact page size is required, same-anchor boundary groups can become
  operationally too large to return whole, or future backends need a cursor that
  is independent of displayed/effective event time.

Decision:

- Stay with 1A for SOW-0124. Do not introduce a new cursor/token/API change in
  this regression fix.
- Track 1B here for future design discussion only.

## Plan

1. Add red shared external multi-file same-anchor boundary tests without production logic changes.
2. Run the focused tests and record the expected failures as regression proof.
3. Write the Netdata query-wide scalar timestamp-anchor contract into the spec.
4. Implement Rust first using per-file batched retrieval plus scalar timestamp-anchor global merge.
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
3. Change combined row retention to truncate by pre-deduplication row timestamp
   boundary, not by a timestamp already mutated for duplicate display.
   - Sort rows by pre-deduplication `row.realtime_usec` / `Row.RealtimeUsec` in
     the requested direction, with deterministic tie-breakers by file path and
     cursor for stable cross-language output.
   - If `rows.len() > limit`, compute the pre-deduplication boundary timestamp at
     `limit - 1`.
   - Backward pages retain all rows with timestamp greater than or equal to that
     boundary; forward pages retain all rows with timestamp less than or equal
     to that boundary.
   - This allows returning more than `last` only when required to include the
     query-wide boundary group.
   - Guard `limit == 0` in Rust and `limit < 0` / `limit == 0` in Go before
     computing `limit - 1`.
4. Preserve duplicate timestamp equality across files.
   - Replace the current unconditional `make_row_timestamps_unique()` call with
     a scoped variant that adjusts duplicate timestamps only inside a
     same-file duplicate run.
   - Do not adjust equal timestamps across different files; those are the exact
     scalar timestamp collisions the query-wide anchor must represent.
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
  - combined sort/limit includes the whole pre-deduplication boundary group;
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
- Added initial Rust and Go red tests for a three-file same-scalar-anchor boundary group with `last:2`, plus UI-style second-page anchor reuse. Per user direction, these language-local tests were then removed in favor of shared external fixture/query coverage under `tests/netdata_function/`.
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
- Round 1 reviewer verification recorded: glm and qwen returned `READY TO IMPLEMENT: NO`; deepseek, kimi, mimo returned `READY TO IMPLEMENT: YES`; minimax timed out before producing a final vote. Blocking finding: non-tail backward realtime anchors must be exclusive for both `data_only` and non-`data_only` row queries, and forward same-anchor coverage must be added.
- Round 2 reviewer verification after plan update: glm, minimax, kimi, mimo, deepseek, and qwen returned `READY TO IMPLEMENT: YES`. Plan gate result is READY TO IMPLEMENT.
- Implemented the round-2-approved surgical plan in Rust and Go:
  - Removed the early `NetdataRealtimeAdjuster` / `netdataRealtimeAdjuster`
    production wiring from `explore_selected_files` /
    `exploreSelectedFiles` and from the per-file Explorer control setup.
    Removed the `set_realtime_adjust_callback` plumbing from the Rust and
    Go Explorer control surfaces, the `ExplorerControl::adjust_realtime`
    helper, and the `acceptedEffectiveRealtime` call sites that mutated
    row timestamps before range/anchor filtering.
  - Removed the dead `NetdataRealtimeAdjuster` struct (Rust) and
    `netdataRealtimeAdjuster` struct (Go), including the historical unit
    tests that exercised them, to avoid dead production state.
  - Made non-tail backward realtime anchors exclusive for every non-tail
    backward row query by dropping the `data_only` precondition on the
    Rust `backward_page_anchor` branch in `NetdataRequest::to_explorer_query()`
    and on the Go `backwardPageAnchor` branch in `toExplorerQuery()`. Both
    paths now convert the realtime anchor to an exclusive upper bound
    (`anchor - 1` microsecond) and clear the Explorer anchor slot.
  - Changed combined row retention in Rust `CombinedResult::sort_and_limit()`
    and Go `netdataCombinedResult.sortAndLimit()` to:
    - sort by pre-deduplication `row.realtime_usec` / `Row.RealtimeUsec` in the
      requested direction with deterministic tie-breakers by file path and cursor;
    - truncate by the pre-deduplication boundary timestamp at `limit - 1`, retaining all
      rows at the boundary timestamp for backward (`>=`) and forward
      (`<=`) pages;
    - guard `limit == 0` in Rust and `limit <= 0` in Go before computing
      the boundary index.
	  - Replaced the cross-file duplicate adjustment in
	    `make_row_timestamps_unique` / `makeRowTimestampsUnique` with a
	    scoped `make_row_timestamps_unique_same_file` /
	    `makeRowTimestampsUniqueSameFile` variant that only adjusts
	    duplicate timestamps inside a same-file duplicate run, preserving
	    cross-file equal timestamps as the scalar query-wide anchor surface.
	  - Fixed page-window `shifts` accounting so an equal-timestamp boundary
	    peer that remains in the expanded boundary group is not counted as a
	    shifted-out row. Backward pages increment `shifts` only when the new
	    row is strictly newer than the retained oldest row; forward pages
	    increment only when the new row is strictly older than the retained
	    newest row.
	  - Added shared external fixture/request pairs for the new scenarios:
    - `tests/netdata_function/requests/same-anchor-boundary-non-data-only-page1.json`
    - `tests/netdata_function/requests/same-anchor-boundary-non-data-only-page2-anchor.json`
    - `tests/netdata_function/requests/same-anchor-boundary-forward-page1.json`
    - `tests/netdata_function/requests/same-anchor-boundary-forward-page2-anchor.json`
  - Wired the new scenarios into `tests/netdata_function/run_anchor_regression.py`
    as `same-anchor-boundary-non-data-only` and
    `same-anchor-boundary-forward`.
  - Added focused Rust unit tests in
    `rust/src/journal/src/netdata.rs::tests`:
    - `sort_and_limit_retains_full_raw_boundary_group_when_last_cuts_through_it`
    - `sort_and_limit_retains_full_raw_boundary_group_for_forward_query`
    - `sort_and_limit_keeps_cross_file_equal_timestamps_for_anchor_continuity`
	    - `sort_and_limit_keeps_same_file_duplicate_display_adjustment`
	    - `sort_and_limit_clears_rows_when_limit_is_zero`
	    - `backward_realtime_anchor_is_exclusive_for_non_data_only_requests`
	    - `backward_realtime_anchor_remains_exclusive_for_data_only_requests`
	    - `netdata_function_boundary_expansion_does_not_report_more_rows`
	  - Added focused Go unit tests in `go/journal/netdata_test.go`:
    - `TestNetdataSortAndLimitRetainsFullRawBoundaryGroupWhenLastCutsThroughIt`
    - `TestNetdataSortAndLimitRetainsFullRawBoundaryGroupForForwardQuery`
    - `TestNetdataSortAndLimitKeepsCrossFileEqualTimestampsForAnchorContinuity`
    - `TestNetdataSortAndLimitKeepsSameFileDuplicateDisplayAdjustment`
    - `TestNetdataSortAndLimitClearsRowsWhenLimitIsZero`
	    - `TestNetdataBackwardRealtimeAnchorIsExclusiveForNonDataOnlyRequests`
	    - `TestNetdataBackwardRealtimeAnchorRemainsExclusiveForDataOnlyRequests`
	    - `TestNetdataDuplicateRowTimestampsMatchPluginDirectionAdjustment`
	    - `TestNetdataFunctionBoundaryExpansionDoesNotReportMoreRows`
  - Updated `duplicate_row_timestamps_match_plugin_direction_adjustment`
    in Rust and the related Go test to call the new
    `_same_file` helper, preserving the existing single-file duplicate
    display adjustment behavior.
  - Updated `.agents/sow/specs/systemd-journal-plugin-facets.md` with a new
    `## Query-Wide Anchor Contract (SDK Netdata Boundary)` section that
    records the shipped post-fix behavior: scalar timestamp anchor,
    forward strict and backward exclusive for both `data_only` and
    non-`data_only` non-tail queries, full boundary-group retention
    when `last` cuts through it, pre-deduplication boundary truncation with same-file
    duplicate display adjustment scoped to one file, no new request or
    response token, and per-file batched retrieval plus global merge.

## Validation

Acceptance criteria evidence:

- Rust Netdata function pagination treats selected journal files as one
  logical query stream for data rows and anchors, with tests covering
  overlapping file ranges and same-scalar-anchor boundary groups.
  - Focused unit tests
    `sort_and_limit_retains_full_raw_boundary_group_when_last_cuts_through_it`,
    `sort_and_limit_retains_full_raw_boundary_group_for_forward_query`,
    `sort_and_limit_keeps_cross_file_equal_timestamps_for_anchor_continuity`,
    `sort_and_limit_keeps_same_file_duplicate_display_adjustment`,
    `backward_realtime_anchor_is_exclusive_for_non_data_only_requests`, and
    `backward_realtime_anchor_remains_exclusive_for_data_only_requests`
    passed.
  - Existing `netdata_function_pages_with_anchor_without_duplicate_or_missing_rows`,
    `netdata_function_tail_polls_return_only_rows_after_anchor_then_304`,
    `netdata_function_tail_delta_reports_exact_incremental_facets_and_histogram`,
    `netdata_function_tail_anchor_with_newer_filtered_out_rows_returns_empty_200`,
    and `duplicate_row_timestamps_match_plugin_direction_adjustment`
    passed unchanged.
- Go Netdata function pagination matches the Rust behavior and tests.
  - Focused Go unit tests mirroring the Rust suite passed:
    `TestNetdataSortAndLimitRetainsFullRawBoundaryGroupWhenLastCutsThroughIt`,
    `TestNetdataSortAndLimitRetainsFullRawBoundaryGroupForForwardQuery`,
    `TestNetdataSortAndLimitKeepsCrossFileEqualTimestampsForAnchorContinuity`,
    `TestNetdataSortAndLimitKeepsSameFileDuplicateDisplayAdjustment`,
    `TestNetdataSortAndLimitClearsRowsWhenLimitIsZero`,
    `TestNetdataBackwardRealtimeAnchorIsExclusiveForNonDataOnlyRequests`,
    `TestNetdataBackwardRealtimeAnchorRemainsExclusiveForDataOnlyRequests`,
    and `TestNetdataDuplicateRowTimestampsMatchPluginDirectionAdjustment`.
  - Existing `TestNetdataFunctionPagesWithAnchorWithoutDuplicateOrMissingRows`,
    `TestNetdataFunctionTailPollsReturnOnlyRowsAfterAnchorThen304`,
    `TestNetdataFunctionTailDeltaReportsExactIncrementalFacetsAndHistogram`,
    `TestNetdataTailAnchorWithNewerFilteredOutRowsReturnsEmpty200`,
    and `TestNetdataDataOnlyDeltaTailSamplingAndNoChangeModes` passed
    unchanged.
- The backend returns rows in globally ordered anchor order for
  forward/backward paging and tail polling; ordered scalar checks now
  pass for the four shared scenarios against both Rust and Go wrappers.
- The anchor used for paging remains the current ordered scalar timestamp
  surface and is applied query-wide. The early realtime-adjust callback wiring
  was removed, and `acceptedEffectiveRealtime` no longer mutates the effective
  realtime before range/anchor filtering.
- Same-anchor rows across files are handled without skipped or duplicated
  rows across page boundaries: full boundary groups are retained even
  when `last` cuts through them, and the cross-file duplicate
  display-adjustment call was removed.
- Anchors are ordered scalar values: page 2 with the previous page's
  edge anchor is strictly outside that anchor in the requested direction,
  and the next edge anchor progresses in the requested direction. The
  scalar may skip values.
- `items.after` / `items.before` remain correct for the UI's more-to-load
  logic. The new Rust
  `netdata_function_boundary_expansion_does_not_report_more_rows` and Go
  `TestNetdataFunctionBoundaryExpansionDoesNotReportMoreRows` tests validate
  that a non-`data_only` query returning the full equal-timestamp boundary group
  reports `items.after = 0` and `items.before = 0`.
- Existing Netdata request shape remains backward compatible: no new
  request key, no new response cursor token. Only the internal
  anchor-filtering, sort/limit, and cross-file duplicate handling changed.
- The implementation does not replace per-file batched retrieval with
  row-by-row k-way traversal: per-file `explore_selected_files` /
  `exploreSelectedFiles` and the after-the-fact global merge are preserved.

Tests or equivalent validation:

- Rust targeted blocker test:
  - `cargo test --lib -p systemd-journal-sdk netdata_function_boundary_expansion_does_not_report_more_rows --release`:
    passed, 1 test.
- Rust full library suite:
  - `cargo test --lib -p systemd-journal-sdk --release`: passed, 129 tests.
- Go targeted blocker test:
  - `go test ./journal -run TestNetdataFunctionBoundaryExpansionDoesNotReportMoreRows -count=1 -timeout=300s`:
    ok `github.com/netdata/systemd-journal-sdk/go/journal`.
- Go full module suite:
  - `go test ./... -count=1 -timeout=300s`: ok.
- Shared external runner against Rust and Go wrappers:
  - `python3 tests/netdata_function/run_anchor_regression.py \
      --peer rust=.../netdata_function_wrapper \
      --peer go=.../netdata_function_wrapper_go \
      --scenario all --work-dir .local/sow-0124/anchor-regression-final-after-shift-fix \
      --out .local/sow-0124/anchor-regression-final-after-shift-fix-report.json`:
    overall ok, four scenarios ok:
    - `query-wide-noncollision`: rust ok, go ok.
    - `same-anchor-boundary`: rust ok, go ok.
    - `same-anchor-boundary-non-data-only`: rust ok, go ok.
    - `same-anchor-boundary-forward`: rust ok, go ok.
  - Per-page evidence for `same-anchor-boundary-non-data-only`:
    - rust page 1: `source-c`, `source-b`, `source-a` (all three at the
      boundary timestamp); page 2 with `anchor=1700000000000000`: empty.
    - go page 1: same three rows; page 2: empty.
  - Per-page evidence for `same-anchor-boundary-forward`:
    - rust page 1: `source-c`, `source-b`, `source-a`; page 2 with
      `anchor=1700000000000000`: empty.
    - go page 1: same three rows; page 2: empty.
  - Installed Netdata `systemd-journal.plugin` was not re-tested in this
    SOW. SOW-0124 explicitly does not fix the plugin; the runner accepts
    `--allow-fail peer` for the plugin peer on `same-anchor-boundary*`.
- Existing Netdata helper tests:
  - `python3 -m unittest tests.netdata_function.test_anchor_regression
    tests.netdata_function.test_stateful_function_compare
    tests.netdata_function.test_compare_function_json`:
    passed, 78 tests.
- Syntax check:
  - `python3 -m py_compile tests/netdata_function/run_anchor_regression.py
    tests/netdata_function/test_anchor_regression.py`: passed.
- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed.

Real-use evidence:

- Read-only UI inspection at `netdata/cloud-frontend @ b0f9c41cfc36` confirms the frontend expects one scalar ordered query-level anchor.

Reviewer findings:

- One read-only explorer agent confirmed the UI anchor assumptions during SOW creation.
- Implementation review round 1 after local validation:
  - deepseek: `READY TO COMPLETE: YES`; non-blocking cleanup notes.
  - kimi: `READY TO COMPLETE: YES`; non-blocking cleanup notes.
  - qwen: `READY TO COMPLETE: YES`; non-blocking cleanup notes.
  - mimo: `READY TO COMPLETE: YES`; missed the counter issue found by glm.
  - glm: `READY TO COMPLETE: NO`; blocker was `items.after` over-reporting
    after non-`data_only` boundary expansion because equal-timestamp boundary
    peers incremented the page-window `shifts` counter.
- Blocker disposition:
  - Fixed Rust and Go page-window `shifts` accounting so equal-timestamp peers
    that remain in the expanded boundary group are not counted as shifted-out
    rows.
  - Added Rust and Go non-`data_only` boundary-expansion tests asserting three
    returned rows for `last = 2` and `items.after = 0`, `items.before = 0` for
    both backward and forward directions.
- Implementation review round 2 after the blocker fix, same full review scope:
  - glm: `READY TO COMPLETE: YES`.
  - kimi: `READY TO COMPLETE: YES`.
  - mimo: `READY TO COMPLETE: YES`.
  - deepseek: `READY TO COMPLETE: YES`.
  - qwen: `READY TO COMPLETE: YES`.
- Non-blocking reviewer notes:
  - `accepted_effective_realtime` / `acceptedEffectiveRealtime` still accept an
    unused `control` parameter after removing the realtime-adjust callback. This
    was kept to minimize blast radius and can be cleaned up in a later hygiene
    change.
  - Option 1B remains future reference only and is not a current follow-up
    commitment.

Same-failure scan:

- Initial scan found Rust and Go Netdata wrappers both applied scalar anchors per file and sort/limit after merging by row timestamp.
- SOW provenance scan found existing single-file paging/tail anchor tests but no synthetic multi-file same-scalar-anchor boundary test.
- The external collision fixture reproduced lost rows in installed plugin, Rust SDK wrapper, and Go SDK wrapper. The plugin is therefore a compatibility reference for current behavior, not a correctness oracle for same-anchor boundary collisions.
- The external non-collision fixture did not reproduce a basic query-wide multi-source paging failure in Rust or Go; the shared harness preserves that as a positive control while using the collision fixture as the red regression case.

Sensitive data gate:

- Durable evidence contains only repository paths, commit id, line references, and behavior summaries. No raw secrets, bearer tokens, SNMP communities, customer names, personal data, private endpoints, or customer-identifying IP addresses were recorded.
- A scan for the user's personal-name variants across the changed SOW, status, README, runner, helper tests, and shared fixture/request files returned no matches.

Artifact maintenance gate:

- AGENTS.md: no update needed; project-wide workflow did not change.
- Runtime project skills: no update needed; existing journal-compatibility and
  orchestration skills covered the work.
- Specs: updated `.agents/sow/specs/systemd-journal-plugin-facets.md` with the
  shipped 1A scalar query-wide anchor contract and the future-only 1B note.
- End-user/operator docs: no update needed; no public consumer doc page
  currently describes the SDK Netdata anchor surface.
- End-user/operator skills: no update needed.
- SOW lifecycle: remains `current/in-progress` until close; ready to move to
  `done/completed` with the implementation commit.
- SOW-status.md: already updated with this SOW in current work; update again on
  close.

Specs update:

- Completed: `.agents/sow/specs/systemd-journal-plugin-facets.md` records the
  shipped SDK Netdata query-wide scalar timestamp-anchor contract.

Project skills update:

- No project-skill update needed for the implementation. The existing
  `project-journal-compatibility` skill already records the anchor as a
  journal-native surface and the directive that anchoring and indexing
  must follow journal-native structures.

End-user/operator docs update:

- Spec update is the durable artifact that records the shipped behavior.
  No public consumer doc page currently describes the anchor surface at
  the SDK boundary.

End-user/operator skills update:

- No end-user/operator skill impact identified.

Lessons:

- UI integration assumptions must be checked before treating a Netdata wire field as only a local per-file optimization.
- Display-time timestamp mutation must not run before Explorer range and anchor filtering; the early Netdata realtime-adjust callback was the root cause of the same-anchor boundary loss because it turned equal timestamps into unique values before the predicates ran.
- A scalar query-wide anchor cannot represent a compound cursor without dropping or duplicating rows. Returning the full same-anchor boundary group when `last` cuts through it is the only way to honor the UI's one-anchor assumption without changing the wire contract.

Follow-up mapping:

- Boundary group policy and wire anchor shape decisions are implemented and recorded in this SOW and in the updated spec.
- Retrieval architecture is implemented and verified: per-file batched query plus global scalar timestamp-anchor merge, not row-by-row k-way traversal.
- `systemd-journal.plugin` was intentionally not modified. Its same-anchor boundary behavior remains a known regression in the C plugin; a future SOW may add a separate plugin fix or a documented compatibility exception.
- Option 1B is recorded for future reference only. It is not accepted as a
  follow-up commitment in this SOW; if needed later, it requires a separate user
  decision and SOW because it changes the API/consumer contract.

## Outcome

Implemented. The SDK Netdata wrappers in Rust and Go now honor the
Netdata UI's one-anchor, query-wide, ordered-scalar contract:

- Forward paging is strict (`> anchor`).
- Non-tail backward paging is exclusive (`anchor - 1` upper bound) for
  every row query, including `data_only = true` and
  `data_only = false`.
- Tail paging remains exclusive on `anchor + 1`.
- Combined per-file results sort by pre-deduplication timestamp with
  deterministic file-path and cursor tie-breakers, then retain the full
  boundary group at `limit - 1`.
- Cross-file duplicate scalar timestamps are preserved; same-file duplicate
  timestamps still receive the existing direction-specific
  increment/decrement display adjustment scoped to one file only.
- `items.after` / `items.before` continue to indicate whether more rows
  are available, including after a boundary-group expansion. Equal timestamp
  boundary peers retained by expansion are not counted as shifted-out rows.
- The scalar `anchor` request shape and the existing response columns
  are preserved. No new request key, response cursor, or token.
- Per-file batched retrieval and global merge are preserved; the
  implementation does not switch to row-by-row k-way multi-file
  traversal.

## Lessons Extracted

- The early Netdata realtime-adjust callback was wired into both Rust
  and Go Explorer control surfaces. Removing it requires removing the
  ExplorerControl field, the setter, the internal helper, and the
  call site in `acceptedEffectiveRealtime`. Leaving any of those layers
  in place produces a dead production state.
- The combined retention change had to happen as sort by pre-deduplication
  timestamp, then retain the boundary group, then run a scoped
  same-file duplicate display adjustment. Running the duplicate
  adjustment first would have split the boundary group before the
  merge could keep it intact.
- The boundary retention needs the same forward/backward treatment as
  the existing Netdata query semantics: backward pages keep every row
  with `realtime_usec >= boundary` and forward pages keep every row
  with `realtime_usec <= boundary`. Getting the inequality wrong
  silently drops the last or first boundary row.

## Followup

- No active follow-up item is required for SOW-0124.
- `systemd-journal.plugin` is intentionally not modified. A separate plugin
  fix is outside this SOW and would require a new user decision and SOW.
- Option 1B is recorded as future reference only. It is not accepted as a
  current follow-up commitment; adding an opaque cursor/token would require a
  new user decision and SOW because it changes the API/consumer contract.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
