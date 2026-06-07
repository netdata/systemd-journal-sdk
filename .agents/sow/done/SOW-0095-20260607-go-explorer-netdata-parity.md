# SOW-0095 - Go Explorer And Netdata Function Parity

## Status

Status: completed

Sub-state: Go Explorer API, Go Netdata function API, and Go stdin wrapper are
implemented, locally validated, benchmarked, profiled, reviewed, and ready for
repository push.

## Requirements

### Purpose

Make the Go SDK a first-class peer of the Rust SDK for optimized journal
exploration and Netdata-compatible generic log function execution, without
regressing reader correctness, runtime purity, or performance.

### User Request

Port the Rust Explorer SDK API, the generic Netdata logs function API, and the
Netdata plugin wrapper to Go. The Go code must pass the same relevant quality
gates as Rust and must include the Rust reader performance and Explorer work
from SOW-0083 through SOW-0092 where it affects Go.

### Assistant Understanding

Facts:

- Rust exposes the Explorer API through `rust/src/journal/src/explorer.rs`.
- Rust exposes the generic Netdata log function API through
  `rust/src/journal/src/netdata.rs`.
- Rust exposes the test wrapper CLI through
  `rust/src/internal/testcmd/netdata_function_wrapper/src/main.rs`.
- Go currently has reader, directory reader, unique-field, and libsystemd-like
  facade APIs, but no Explorer API, no generic Netdata function API, and no
  Go equivalent of the Netdata function wrapper.
- SOW-0083 through SOW-0092 are Rust-focused and have not been ported as a
  Go Explorer/Netdata API surface.

Inferences:

- The Go port cannot be a wrapper around the existing Go facade filter path,
  because that path expands full entries before matching filters.
- The Go port must reuse Go's mmap reader and indexed FIELD/DATA paths, and
  add missing Explorer-specific primitives where required.

Unknowns:

- The exact Rust-vs-Go Explorer performance ratio is unknown until the Go port
  exists and the same benchmark cases run against both implementations.

### Acceptance Criteria

- Go exposes an idiomatic Explorer API equivalent to Rust's current public
  Explorer API: query shape, filters, FTS terms/patterns, facets, histogram,
  row limits, direction, anchors, sampling, field modes, strategy selection,
  progress, cancellation, timeouts, result stats, and comparison diagnostics.
- Go exposes an idiomatic generic Netdata log function API equivalent to Rust's
  current `journal::netdata` API: configurable defaults, systemd-journal
  profile behavior, request parsing, directory selection, progress,
  cancellation, timeout, state hook, response shape, and stable content parity.
- Go provides a wrapper CLI equivalent to Rust's `netdata_function_wrapper`
  with the same stdin request contract:
  `--test FUNCTION --dir DIRECTORY --timeout SECONDS < request.json`.
- Go tests cover the same behavior families as Rust Explorer and Netdata
  function tests, including filters, negative filters, FTS, default facets,
  histograms, data-only, tail/no-change, sampling, progress, cancellation,
  timeout, source filtering, and selected-row expansion.
- The shared Netdata function comparator can run SDK-first comparisons against
  the Go wrapper, and the Go wrapper produces stable-content parity with the
  Rust wrapper for the committed fixture/request matrix.
- Go reader performance-sensitive paths used by Explorer avoid full-entry
  expansion except for returned rows, avoid unnecessary decompression, use
  FIELD indexes for column catalogs, and preserve row-level payload lifetime
  guarantees where an API returns borrowed payloads.
- Benchmarks compare Rust and Go on the same Explorer/Netdata function cases.
  Go must be materially comparable to Rust; any slower path requires measured
  evidence, profiling, and explicit recorded disposition before close.
- `go test ./...`, relevant Rust tests, shared comparator tests, code scanning
  local checks available in-repo, `git diff --check`, and
  `.agents/sow/audit.sh` pass before closure.
- Whole-SOW read-only reviewer runs from the approved reviewer pool reach
  `PRODUCTION GRADE`, or all blocking findings are fixed and re-reviewed.

## Analysis

Sources checked:

- `rust/src/journal/src/explorer.rs`
- `rust/src/journal/src/netdata.rs`
- `go/journal/reader.go`
- `go/journal/reader_entry.go`
- `go/journal/reader_filter.go`
- `go/journal/reader_unique.go`
- `go/journal/facade.go`
- `go/internal/testcmd/reader_core_bench/main.go`
- `.agents/sow/done/SOW-0082-20260602-rust-optimized-journal-explorer-api.md`
- `.agents/sow/done/SOW-0083-20260602-index-derived-facet-histogram-optimization.md`
- `.agents/sow/done/SOW-0086-20260604-rust-reader-performance-contract-gap-analysis.md`
- `.agents/sow/done/SOW-0093-20260605-netdata-function-boundary-reader-comparison.md`

Current state:

- Rust Explorer public query fields include realtime bounds, anchor,
  direction, limit, filters, facets, histogram, FTS, field mode, source
  realtime, sampling, row-full stopping, and a debug-only column traversal
  switch.
- Rust Explorer public results include rows, facets, histogram, column fields,
  stats, and optional traversal-vs-index comparison diagnostics.
- Rust Netdata function public API includes configuration defaults, profiles,
  run options, progress, state hooks, JSON request execution, and byte request
  execution.
- Go `ReaderOptions` already supports mmap and snapshot/live bounds.
- Go `Reader` caches immutable layout sizes and current entry headers, and
  caches current entry DATA offsets.
- Go `Step()` and `StepBack()` apply facade filters by calling `GetEntry()`,
  which fully expands and copies the row before matching.
- Go `GetEntry()` allocates maps and owned payload/value copies for every DATA
  object in the row.
- Go `EnumerateEntryPayload()` documents row-level payload lifetime for
  libsystemd-style DATA enumeration.
- Go `VisitEntryPayloads()` documents a weaker visitor-only lifetime.
- Go `QueryUnique()` and `EnumerateFields()` already use FIELD/DATA indexes
  where possible.
- Go `reader_core_bench` has SDK and facade reader modes, but no Explorer or
  Netdata function modes.

Risks:

- Reusing Go facade filters would produce correct-looking results but would
  violate the performance contract by expanding rows before indexed slicing and
  facet/histogram traversal.
- Porting only the public API without matching Rust's control, sampling,
  state, and comparator semantics would create a false parity claim for Netdata
  integration.
- Matching Rust literally in Go can create non-idiomatic or allocation-heavy
  code; the required outcome is behavioral/API parity and performance parity,
  not line-by-line translation.
- The Netdata function response code is large and has many behavior edge cases;
  insufficient fixture parity would make benchmark wins untrustworthy.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Rust has the optimized Explorer and Netdata generic function surface that
  Netdata integration needs; Go does not. Evidence:
  `rust/src/journal/src/explorer.rs:77`,
  `rust/src/journal/src/netdata.rs:156`, and no matching Go files found under
  `go/journal/`.
- Existing Go facade filtering is not a valid Explorer foundation because it
  expands entries with `GetEntry()` before matching. Evidence:
  `go/journal/reader.go:932` and `go/journal/reader_entry.go:135`.
- Existing Go reader primitives are still useful: mmap/snapshot options,
  current-header caching, current DATA-offset caching, FIELD-index unique
  enumeration, and row-level facade DATA enumeration exist. Evidence:
  `go/journal/reader.go:78`, `go/journal/reader.go:125`,
  `go/journal/reader.go:766`, `go/journal/reader_entry.go:111`, and
  `go/journal/reader_unique.go:18`.

Evidence reviewed:

- Rust Explorer API: `rust/src/journal/src/explorer.rs:18`,
  `rust/src/journal/src/explorer.rs:33`,
  `rust/src/journal/src/explorer.rs:46`,
  `rust/src/journal/src/explorer.rs:77`,
  `rust/src/journal/src/explorer.rs:215`,
  `rust/src/journal/src/explorer.rs:277`,
  `rust/src/journal/src/explorer.rs:298`, and
  `rust/src/journal/src/explorer.rs:1198`.
- Rust Netdata API: `rust/src/journal/src/netdata.rs:156`,
  `rust/src/journal/src/netdata.rs:204`,
  `rust/src/journal/src/netdata.rs:261`,
  `rust/src/journal/src/netdata.rs:294`, and
  `rust/src/journal/src/netdata.rs:357`.
- Go reader/facade state: `go/journal/reader.go:78`,
  `go/journal/reader.go:125`, `go/journal/reader.go:803`,
  `go/journal/reader.go:932`, `go/journal/reader_entry.go:19`,
  `go/journal/reader_entry.go:111`, `go/journal/reader_entry.go:135`,
  `go/journal/reader_filter.go:42`, and `go/journal/reader_unique.go:18`.
- Prior requirements: SOW-0082, SOW-0083, SOW-0086 through SOW-0092, and
  SOW-0093.

Affected contracts and surfaces:

- Go SDK public API under `go/journal/`.
- Go reader hot path and internal row traversal helpers.
- Go test commands and internal benchmark tools.
- Shared Netdata function comparator fixtures under `tests/netdata_function/`.
- README/API documentation for Go and project specs describing Explorer and
  Netdata function behavior.

Existing patterns to reuse:

- Rust Explorer and Netdata APIs are the behavioral reference.
- Go reader mmap and snapshot options remain the file access foundation.
- Go FIELD/DATA index walkers in `reader_unique.go` remain the reference for
  indexed field/value enumeration.
- Rust wrapper stdin request contract from SOW-0093 is the CLI contract.
- Shared comparator tests under `tests/netdata_function/` are reused for
  stable-content validation.

Risk and blast radius:

- High performance risk: Explorer hot paths can silently become full-row scans
  if implementation reuses existing facade filters or `GetEntry()`.
- Medium compatibility risk: Netdata function response semantics include
  nuanced plugin-compatible fields, sampling, progress, and no-change behavior.
- Medium API risk: Go API must be idiomatic but still recognizable as the Rust
  API sibling.
- Low writer risk: this SOW is reader/query focused and must not change writer
  behavior except where tests need fixtures.
- Security risk: wrapper must read request payload from stdin in test mode, not
  from a command-line filename.

Sensitive data handling plan:

- Durable artifacts will include only sanitized fixture names, code paths,
  command names, aggregate benchmark numbers, and reviewer summaries.
- No raw journal payloads, customer identifiers, personal data, secrets,
  tokens, SNMP communities, or private endpoints will be written to SOWs,
  specs, docs, skills, or commits.
- Benchmark and comparator reports that may contain raw payload details must
  stay under `.local/` and must not be committed.

Implementation plan:

1. Add Go Explorer types, validation, traversal strategy, index strategy,
   compare strategy, stats, control/progress, and tests modeled on Rust.
2. Add any missing internal Go row traversal primitive required to avoid
   `GetEntry()` in Explorer hot paths and to preserve row-level lifetime for
   borrowed payload APIs.
3. Add Go Netdata function API and systemd-journal profile behavior modeled on
   Rust, including stdin wrapper CLI.
4. Extend Go benchmarks and shared Netdata comparator tooling to run against
   the Go wrapper and comparable Explorer modes.
5. Update Go docs/specs and run validation, benchmarks, code scanning checks,
   and whole-SOW reviewers.

Validation plan:

- `gofmt` on changed Go files.
- `go test ./...`.
- Focused Go Explorer and Netdata function tests.
- Relevant Rust tests to ensure shared fixtures and comparator behavior are not
  regressed.
- Go wrapper build and shared comparator runs against Rust wrapper for committed
  request fixtures.
- Reader/Explorer benchmark comparison for Rust vs Go using the same inputs and
  report schema.
- `git diff --check`.
- `.agents/sow/audit.sh`.
- Whole-SOW external reviewer pass with the approved reviewer pool.

Artifact impact plan:

- AGENTS.md: no expected update unless this SOW discovers a new project-wide
  rule.
- Runtime project skills: update only if a reusable Go parity workflow or
  mandatory validation rule is discovered.
- Specs: expected update for Go Explorer and Netdata function parity.
- End-user/operator docs: expected update for Go README/API docs and wrapper
  help.
- End-user/operator skills: no expected update; none currently exist for this
  surface.
- SOW lifecycle: completion requires code, validation, reviewers, follow-up
  mapping, status change, and move to `done/`.
- SOW-status.md: update both project status ledgers when this SOW changes
  state.

Open-source reference evidence:

- None checked for this SOW creation. The behavior reference is the already
  implemented Rust SDK in this repository and the prior Netdata plugin evidence
  captured in SOW-0081 and SOW-0093.

Open decisions:

- None currently blocking. The user already decided that Go must match Rust in
  API, testing, performance, and quality for this surface.

## Implications And Decisions

- 2026-06-07 routing decision inherited from project instructions:
  implementation is local in this repository; external models are read-only
  reviewers only.
- 2026-06-07 scope decision from user: Go must port the Rust Explorer SDK API,
  generic Netdata logs function API, Netdata plugin wrapper, and Rust SOW-0083
  through SOW-0092 reader/Explorer-relevant work.

## Plan

1. Port Go Explorer API and tests.
2. Port Go Netdata function API and wrapper CLI.
3. Add shared comparator and benchmark coverage for Go.
4. Repair performance gaps found by benchmarks/profiling.
5. Run local validation and whole-SOW reviewer cycle.
6. Close the SOW with explicit status, spec/docs updates, and commit/push.

## Delegation Plan

Implementer:

- Local implementation in this repository, per current routing decision.

Reviewers:

- Read-only whole-SOW reviewers after local implementation and validation:
  `llm-netdata-cloud/glm-5.1`,
  `llm-netdata-cloud/kimi-k2.6`,
  `llm-netdata-cloud/mimo-v2.5-pro`,
  `llm-netdata-cloud/qwen3.6-plus`,
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

- Reviewer findings will be recorded with dispositions in this SOW.
- If a reviewer votes below `PRODUCTION GRADE`, the blocking findings must be
  fixed or explicitly dispositioned with evidence, then the whole-SOW review
  scope must be rerun.
- If benchmarks show Go materially behind Rust, profile before closing and
  either fix the gap or record evidence and ask for a user decision.

## Execution Log

### 2026-06-07

- Created this SOW after confirming the repository was clean and pushed.
- Verified Rust has Explorer and Netdata APIs while Go does not yet have
  equivalent files or wrapper CLI.
- Verified Go's existing facade filter path expands full entries before
  matching and therefore cannot be the Explorer hot path.
- Implemented Go Explorer API in `go/journal/explorer.go`, including
  traversal, index, compare, control/progress/cancellation, FTS, first-value
  field mode, source-realtime handling, compressed DATA avoidance, offset
  classification cache, sampling, and FIELD-index column catalog behavior.
- Added Go Explorer tests in `go/journal/explorer_test.go` for filters,
  facets, histograms, rows, duplicate field first-value behavior, index/compare
  parity, debug column traversal rejection, progress/cancellation, compressed
  DATA avoidance, same-field filter exclusion, FTS/early-stop interaction,
  backward time-bound scanning, and sampling skip/estimate behavior.
- Added Go `reader_core_bench` `explorer-query` mode and Explorer flags in
  `go/internal/testcmd/reader_core_bench/main.go`.
- Found and fixed a Go traversal correctness gap: traversal candidate sets
  were prefiltering exact commit realtime before row scan. Rust traversal uses
  indexed FIELD filters first, then applies slack/source realtime during row
  evaluation. The Go traversal path now keeps time filtering out of traversal
  candidate sets; indexed strategy keeps exact time filtering.
- Verified Rust/Go Explorer benchmark-smoke parity on
  `.local/sow-0093/smoke-journals/system.journal` with the query:
  backward, snapshot, `PRIORITY` facet, `PRIORITY` histogram, limit 5,
  after `1666569601000000`, before `1666584438000000`.
  Evidence: both report `records=1922`, `fields=9431`,
  checksum `4140931603882331884`, facet checksum
  `12899555891149432167`, histogram checksum `11574013657736998499`,
  `rows_examined=1922`, `rows_matched=1920`, `facet_rows_matched=1920`,
  `data_refs_seen=9431`, `data_payloads_loaded=1114`,
  `data_cache_hits=8317`, `data_cache_misses=1114`,
  `payloads_decompressed=0`, and `returned_row_expansions=5`.
- Local validation passed for this chunk:
  `cd go && gofmt -w journal/explorer.go journal/explorer_test.go internal/testcmd/reader_core_bench/main.go && go test ./...`.
- First read-only reviewer batch on the Go Explorer chunk reported a
  non-production-grade result for missing Rust-equivalent Explorer tests,
  stale filtered-traversal performance parity, and small code-quality findings.
  The findings were valid for the reviewed revision.
- Fixed reviewer findings in the Explorer chunk:
  - Ported the missing Rust Explorer behavior families to Go tests, including
    multi-value filter OR plus cross-field AND, cursor-only rows, empty
    results with requested facets, negative FTS terms, duplicate facet
    rejection, first-value duplicate facet/histogram accounting, source
    realtime plus histogram early stop, indexed-strategy rejection and
    same-field exclusion, sampling bucket-count/seqnum/math edge cases,
    realtime anchor exclusivity, and row-full stop behavior.
  - Reworked Go filtered traversal to iterate sorted candidate ENTRY offsets
    directly for non-`all` candidate sets, matching Rust's cursor-level
    filter-skip shape and avoiding full ENTRY-array scans for selective
    filters.
  - Added a regression test proving filtered traversal reaches a single
    matching candidate at the end of a 9,000-entry file without triggering the
    8,192-row cancellation check on unrelated entries.
  - Changed unsupported Explorer strategies to wrap `ErrUnsupported`.
  - Removed the redundant `*&deferred` expression.
  - Removed an unused facet-sorting helper.
  - Matched Rust/Netdata sampling edge behavior by clamping over-scanned
    seqnum estimates to one.
  - Matched Rust/Netdata estimated histogram integer math without allocation by
    using `bits.Mul64`/`bits.Div64`.
  - Removed a misleading timestamp pointer copy and fixed histogram bucket
    indexing to clamp before unsigned subtraction.
- Local revalidation after fixes passed:
  `cd go && gofmt -w journal/explorer.go journal/explorer_test.go internal/testcmd/reader_core_bench/main.go && go test ./...`.
- Rust/Go Explorer benchmark-smoke parity after fixes still matched on
  `.local/sow-0093/smoke-journals/system.journal` for the same query and
  reported the same checksums and Explorer counters listed above.
- Applied second-batch low-risk cleanup before rerunning reviewers:
  - Forward `StopWhenRowsFull` slack comparison now uses the existing
    saturating add helper instead of unchecked `newest + slack`.
  - Added index-strategy cursor-only row coverage so the index path proves it
    can return cursors without expanding returned-row payloads.
  - Added short comments for the private row-payload expansion modes.
  - Removed the duplicate Explorer-local `minU64` helper and reused the
    package `minUint64` helper.
- Local validation after second-batch cleanup passed:
  `cd go && gofmt -w journal/explorer.go journal/explorer_test.go internal/testcmd/reader_core_bench/main.go && go test ./...`.
- Rust/Go Explorer benchmark-smoke parity after second-batch cleanup still
  matched on `.local/sow-0093/smoke-journals/system.journal` for the same
  query and reported the same checksums and Explorer counters listed above.
- Second reviewer batch for the Explorer chunk produced `PRODUCTION GRADE`
  votes from `llm-netdata-cloud/glm-5.1`,
  `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`,
  and `llm-netdata-cloud/minimax-m3-coder`. The captured
  `llm-netdata-cloud/qwen3.6-plus` session ended before a final verdict was
  available, and the `llm-netdata-cloud/deepseek-v4-pro` session became stale
  after code changed and was stopped by exact PID.
- Fixed second-batch reviewer findings before the final rerun:
  - `stopByCommitTime` and `skipByCommitTime` now use saturating addition for
    `before + realtime_slack`, matching Rust.
  - Indexed histogram unset handling now reuses candidate offset slices instead
    of copying all candidate offsets.
  - Added timeout-path coverage for `ExplorerControl`.
- Local validation after final second-batch fixes passed:
  `cd go && gofmt -w journal/explorer.go journal/explorer_test.go internal/testcmd/reader_core_bench/main.go && go test ./...`.
- Rust/Go Explorer benchmark-smoke parity after final second-batch fixes still
  matched on `.local/sow-0093/smoke-journals/system.journal` for the same
  query and reported the same checksums and Explorer counters listed above.
- Fixed an additional sampling parity gap found during final review rerun:
  Go now uses `firstRealtimeUsec` in the sampling overlapping-timeframe model,
  matching Rust's `overlapping_timeframe` and `remaining_time_details`
  structure instead of carrying an unused first-realtime field.
- Local validation after the sampling parity fix passed:
  `cd go && gofmt -w journal/explorer.go journal/explorer_test.go internal/testcmd/reader_core_bench/main.go && go test ./...`.
- Rust/Go Explorer benchmark-smoke parity after the sampling parity fix still
  matched on `.local/sow-0093/smoke-journals/system.journal` for the same
  query and reported the same checksums and Explorer counters listed above.
- Implemented the Go generic Netdata function API in `go/journal/netdata.go`,
  including request parsing, source/file selection, indexed Explorer
  orchestration, cursor-only row selection, final returned-row expansion,
  plugin-compatible display profiles, progress, timeout, cancellation, and
  state hooks.
- Added the Go stdin-based test wrapper CLI in
  `go/internal/testcmd/netdata_function_wrapper/main.go` with the same
  privileged-safe request contract as Rust:
  `--test systemd-journal --dir DIRECTORY --timeout SECONDS < request.json`.
- Added focused Go API tests in `go/journal/netdata_test.go` for info
  responses, filters, FTS, facets, histogram, returned rows, byte-request
  execution, cancellation status, and invalid JSON rejection.
- Mechanically aligned Go `ERRNO` and `MESSAGE_ID` display tables with the
  Rust Netdata function profile tables, including 131 errno names and 133
  message-id names.
- Local Go Netdata validation passed:
  `cd go && gofmt -w journal/netdata.go journal/netdata_test.go internal/testcmd/netdata_function_wrapper/main.go && go test ./journal -run 'TestNetdataFunction' -count=1 && go test ./...`.
- Wrapper build validation passed:
  `cd go && go build -o ../.local/sow-0095/go-netdata-function-wrapper ./internal/testcmd/netdata_function_wrapper`
  and `cd rust && cargo build -q -p netdata_function_wrapper`.
- Shared SDK-first comparator validation passed 10/10 request fixtures with no
  stable-content diffs, comparing
  `.local/sow-0095/go-netdata-function-wrapper` against
  `rust/target/debug/netdata_function_wrapper` on
  `.local/sow-0093/smoke-journals`:
  `info`, `window-error-filter`, `window-last5-data-only-delta`,
  `window-last5-data-only`, `window-last5-default-facets-sampling20`,
  `window-last5-default-facets`, `window-last5-fts-or-negative`,
  `window-last5-priority-source-system`, `window-last5-priority`, and
  `window-last5-tail-no-change`.
- Updated `go/API.md` with the Go Explorer and Netdata function API contract,
  including stdin request handling for privileged test wrappers and
  cancellation/timeout status behavior.
- Found a large-query Go performance regression after comparing the Go wrapper
  against the Rust wrapper on the real `/var/log/journal` window request from
  SOW-0093. The same request selects 200 files and 5,341,590 matched rows with
  131,362,149 DATA references. Initial Go behavior either made no visible
  progress in 30 seconds or completed the main scan but timed out during
  post-processing.
- Profiled the Go wrapper and found the largest avoidable cost was opening
  files through the full reader path in metadata and post-processing helpers.
  The full path loaded global ENTRY arrays even when only header or FIELD/DATA
  index information was needed.
- Fixed the Go wrapper hot path by adding an internal header/index-only reader
  open path, using it for file-order metadata and zero-count facet vocabulary
  collection, replacing `_BOOT_ID` first-realtime discovery with an indexed
  FIELD/DATA walk, compacting the Explorer DATA-offset classification cache,
  using a heap for the retained page window, delaying unset facet accounting to
  result finalization, and hoisting FTS detection out of per-DATA loops.
- Added focused regression tests for header/index-only open and indexed boot
  first-realtime discovery:
  `TestReaderHeaderOnlyOpenSkipsEntryArrayAndKeepsIndexedUnique` and
  `TestNetdataCollectBootFirstRealtimeUsesBootIndex`.
- Local final validation after the performance fixes passed:
  `gofmt -w go/journal/explorer.go go/journal/netdata.go go/journal/netdata_test.go go/journal/reader.go go/journal/reader_test.go`,
  `cd go && go test ./...`,
  `cd rust && cargo test -q -p journal netdata`, and `git diff --check`.
- Wrapper build validation after the performance fixes passed:
  `cd go && go build -o ../.local/sow-0095/go-netdata-function-wrapper-release ./internal/testcmd/netdata_function_wrapper`
  and `cd rust && cargo build -q -p netdata_function_wrapper --release`.
- Final committed-fixture comparator validation after the performance fixes
  passed 10/10 SDK-first Go-wrapper vs Rust-wrapper request fixtures with
  stable-content parity. Evidence reports are under
  `.local/sow-0095/go-rust-comparator-final/`.
- Final large-query comparator benchmark after the performance fixes passed
  3/3 SDK-first Go-wrapper vs Rust-wrapper repetitions with stable-content
  parity on the SOW-0093 `/var/log/journal` request. Measured wall-clock
  seconds:
  run 1 Go `3.488s`, Rust `2.921s`;
  run 2 Go `3.624s`, Rust `2.942s`;
  run 3 Go `3.490s`, Rust `2.924s`;
  average Go `3.534s`, Rust `2.929s`, so Go is `1.21x` slower than Rust on
  this query after the fixes. Evidence:
  `.local/sow-0095/performance/go-vs-rust-default-facets-4g-final-3rep.json`.
- Post-fix profile evidence shows remaining Go cost is in the expected
  Explorer scan path, not in accidental ENTRY-array loading:
  `scanExplorerCombined`, `handleRowValueClass`, `offsetClassCache.lookup`,
  `readEntryDataOffsetsAt`, and offset-cache insertion/growth. Evidence:
  `.local/sow-0095/performance/go-perf-after-fts-hoist.data`.
- Applied post-review low-risk cleanup and coverage:
  - `scanExplorerFacet` now computes FTS need once and reuses it for deferred
    apply selection.
  - `netdataPageHeap.Push` now panics with a precise internal type-contract
    message if called with a non-`uint64` value.
  - `netdataJournalSourceSummary.addPath` now uses the header/index-only
    reader open path because it only reads header metadata.
  - Added focused tests for `offsetClassCache` zero-offset sentinel behavior,
    cache growth retention, delayed unset facet accounting, page-window
    direction retention, and `_BOOT_ID` first-realtime edge cases.
- Current local validation after post-review cleanup passed:
  `cd go && go test ./...`,
  `cd rust && cargo test -q -p journal netdata`,
  `git diff --check`, and
  `tests/netdata_function/run_function_compare.py` with 10/10 SDK-first
  Go-wrapper vs Rust-wrapper request fixtures passing against current wrapper
  builds. Evidence report:
  `.local/sow-0095/go-rust-comparator-current/all.json`.
- Updated specs and Go API docs so the Explorer and Netdata function boundary
  describe Rust and Go as current implementations, and recorded the
  plugin-compatible UID/GID display-name parity nuance: Rust uses platform
  user/group lookup APIs on Unix; Go keeps the no-CGO contract and uses
  pure-Go passwd/group file lookup when available.
- Added focused Go Netdata unit coverage for the behavior families that were
  previously covered mainly by Rust tests and end-to-end comparator fixtures:
  request parsing, time-window normalization, source selection/classification,
  profile display transforms, UID display-cache fallback, realtime adjustment,
  data-only mode, delta mode, tail/no-change mode, progress callbacks, timeout
  partial responses, sampling counters, sampling-disabled data-only behavior,
  and recursive journal-file collection.
- Fixed `go/API.md` timeout wording to match the implemented
  Netdata/plugin-compatible behavior: timeout is a controlled partial table
  response with `status: 200` and a warning message, not a Go error and not a
  `504` response.
- Current local validation after adding the Go Netdata parity tests passed:
  `cd go && go test ./...`,
  `cd rust && cargo test -q -p journal netdata`, and
  `tests/netdata_function/run_function_compare.py` with 10/10 SDK-first
  Go-wrapper vs Rust-wrapper request fixtures passing against current wrapper
  builds. Evidence report:
  `.local/sow-0095/go-rust-comparator-current/all.json`.
- Final full-SOW reviewer rerun completed. The final missing reviewer,
  `llm-netdata-cloud/kimi-k2.6`, returned `PRODUCTION GRADE: YES` after
  rerunning `go test ./...`, `cargo test -q -p journal netdata`,
  wrapper build checks, `.agents/sow/audit.sh`, `git diff --check`, and
  repeated-pattern scans.
- Fixed Kimi's only concrete low finding by removing the duplicate
  `expandRowPayloads()` call from the Netdata function top-level request path;
  `exploreFiles()` already expands the retained rows before returning.
- Revalidated after that cleanup with `cd go && go test ./...`,
  `git diff --check`, wrapper rebuilds for Go and Rust, and
  `tests/netdata_function/run_function_compare.py`; the comparator report
  stayed `ok: true` with 10/10 cases passing.
- `.agents/sow/audit.sh` passed clean before closeout.

## Validation

Acceptance criteria evidence:

- Explorer chunk only:
  - Go now exposes an idiomatic Explorer API equivalent to the Rust Explorer
    surface for query shape, filters, FTS, facets, histogram, row limits,
    direction, anchors, sampling, field modes, strategy selection, progress,
    cancellation, timeouts, result stats, and comparison diagnostics.
  - Go Explorer hot paths avoid `GetEntry()` full-entry expansion except for
    returned-row benchmark modes, use FIELD-index column catalogs, reject the
    debug row-traversal column collector, and avoid core SDK runtime host
    probing.
  - Go `reader_core_bench` now has `explorer-query` mode and Explorer flags
    for Rust/Go parity and performance smoke checks.
- Go Netdata function API/wrapper chunk:
  - Go now exposes the generic Netdata logs function API and a
    plugin-compatible systemd-journal profile in `go/journal/netdata.go`.
  - Go now provides the stdin-based wrapper CLI in
    `go/internal/testcmd/netdata_function_wrapper/main.go`.
  - The wrapper produces stable-content parity with the Rust wrapper for the
    committed 10-request Netdata function comparator matrix.
- The whole-SOW reviewer rerun reached `PRODUCTION GRADE: YES` across the
  approved reviewer pool, and local validation remained clean after the final
  low-risk cleanup.

Tests or equivalent validation:

- Explorer chunk only:
  - `cd go && go test ./...` passed after Go Explorer and benchmark-driver
    changes.
  - `gofmt` was run on changed Go files before each local validation pass.
- Go Netdata function API/wrapper chunk:
  - `cd go && go test ./journal -run 'TestNetdataFunction' -count=1` passed.
  - `cd go && go test ./...` passed.
  - `cd go && go build -o ../.local/sow-0095/go-netdata-function-wrapper ./internal/testcmd/netdata_function_wrapper` passed.
  - `cd rust && cargo build -q -p netdata_function_wrapper` passed.
  - `tests/netdata_function/run_function_compare.py` passed 10/10 SDK-first
    Go-wrapper vs Rust-wrapper request fixtures with `ALL_OK True`.
- Post-review cleanup validation:
  - `cd go && go test ./...` passed.
  - `cd rust && cargo test -q -p journal netdata` passed.
  - `cd go && go build -o ../.local/sow-0095/go-netdata-function-wrapper-current ./internal/testcmd/netdata_function_wrapper` passed.
  - `cd rust && cargo build -q -p netdata_function_wrapper` passed.
  - Current SDK-first comparator validation passed 10/10 Go-wrapper vs
    Rust-wrapper request fixtures with stable-content parity. Evidence:
    `.local/sow-0095/go-rust-comparator-current/all.json`.
  - `git diff --check` passed.
- Post-review test-gap repair validation:
  - `cd go && go test ./...` passed after adding the focused Go Netdata
    request/source/profile/response-mode unit tests.
  - `cd go && go test ./journal -list 'Test(Netdata|Normalize)'` now lists
    14 focused Go Netdata/time-normalization tests covering the behavior
    clusters above.
  - `cd rust && cargo test -q -p journal netdata` passed.
  - Current SDK-first comparator validation still passed 10/10 Go-wrapper vs
    Rust-wrapper request fixtures with stable-content parity.
- Final cleanup validation:
  - `llm-netdata-cloud/kimi-k2.6` reran `cd go && go test ./...`,
    `cd rust && cargo test -q -p journal netdata`, Go wrapper build,
    `.agents/sow/audit.sh`, and `git diff --check`; all passed.
  - After removing the duplicate retained-row expansion call found by Kimi,
    `cd go && go test ./...` passed.
  - `git diff --check` passed.
  - Go and Rust Netdata function wrappers rebuilt successfully.
  - Current SDK-first comparator validation passed 10/10 Go-wrapper vs
    Rust-wrapper request fixtures with stable-content parity. Evidence:
    `.local/sow-0095/go-rust-comparator-current/all.json`.
  - `.agents/sow/audit.sh` passed clean.

Real-use evidence:

- Explorer chunk only: Rust and Go `reader_core_bench` `explorer-query` smoke
  runs on `.local/sow-0093/smoke-journals/system.journal` produced matching
  content and optimizer counters for the tested query:
  `records=1922`, `fields=9431`, checksum `4140931603882331884`, facet
  checksum `12899555891149432167`, histogram checksum
  `11574013657736998499`, `rows_examined=1922`, `rows_matched=1920`,
  `facet_rows_matched=1920`, `data_refs_seen=9431`,
  `data_payloads_loaded=1114`, `data_cache_hits=8317`,
  `data_cache_misses=1114`, `payloads_decompressed=0`, and
  `returned_row_expansions=5`.
- The Go and Rust Netdata function wrappers now also match on the larger real
  `/var/log/journal` request from SOW-0093. The final three-repetition run
  showed stable-content parity on every repetition and average wall time of
  Go `3.534s` versus Rust `2.929s`.

Reviewer findings:

- First batch for the Go Explorer chunk:
  - `llm-netdata-cloud/glm-5.1`: `NOT PRODUCTION GRADE`; blocked on missing
    Rust-equivalent tests.
  - `llm-netdata-cloud/qwen3.6-plus`: `NOT PRODUCTION GRADE`; blocked on
    missing Rust-equivalent tests, `ErrUnsupported` consistency, unused helper,
    and `*&deferred`.
  - `llm-netdata-cloud/minimax-m3-coder`: `NOT PRODUCTION GRADE`; blocked on
    missing Rust-equivalent tests and small code-quality findings.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE` with low-priority
    follow-up suggestions.
  - `llm-netdata-cloud/kimi-k2.6`: `NOT PRODUCTION GRADE`; blocked on filtered
    traversal performance, missing tests, timestamp pointer copy, histogram
    bucket underflow-before-clamp, and `*&deferred`.
  - `llm-netdata-cloud/deepseek-v4-pro`: session result was not available
    after context transition and will be rerun with the full reviewer pool.
- Disposition: all blocking first-batch findings were fixed or verified as
  already Rust-compatible.
- Final Explorer-chunk full-scope reviewer batch:
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/qwen3.6-plus`: replacement run returned
    `PRODUCTION GRADE`; the first final rerun stalled without a verdict and
    was stopped by exact process group after no findings were emitted.
  - `llm-netdata-cloud/minimax-m3-coder`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/deepseek-v4-pro`: replacement run returned
    `PRODUCTION GRADE`; the earlier final rerun session was unavailable after
    context transition.
- Final whole-SOW reviewer rerun after Go Netdata API/wrapper, profiling
  fixes, focused Go Netdata tests, and closeout cleanup:
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE: YES`.
  - `llm-netdata-cloud/deepseek-v4-pro`: `PRODUCTION GRADE: YES`.
  - `llm-netdata-cloud/qwen3.6-plus`: initial final run blocked on procedural
    SOW closeout and test-evidence gaps; rerun returned
    `PRODUCTION GRADE: YES`.
  - `llm-netdata-cloud/mimo-v2.5-pro`: initial final run blocked on procedural
    SOW closeout and Go Netdata unit-test coverage depth; rerun returned
    `PRODUCTION GRADE: YES`.
  - `llm-netdata-cloud/minimax-m3-coder`: rerun returned
    `PRODUCTION GRADE: YES`.
  - `llm-netdata-cloud/kimi-k2.6`: final rerun returned
    `PRODUCTION GRADE: YES`.
- Final whole-SOW reviewer dispositions:
  - Procedural SOW closeout gaps were fixed by populating outcome, lessons,
    follow-up mapping, validation, and artifact gates.
  - Go Netdata test-evidence gaps were fixed with focused
    request/source/profile/progress/timeout/sampling/response-mode tests.
  - Mimo's resource-safety and documentation observations were fixed by
    deferring reader close in `addZeroCountFacetValuesFromFiles`, documenting
    header/index-only open and reader concurrency, and clarifying timeout and
    UID/GID profile docs.
  - Kimi's duplicate retained-row expansion observation was fixed by removing
    the top-level duplicate `expandRowPayloads()` call; local validation and
    comparator parity still passed.
- Final non-blocking reviewer observations:
  - Benchmark-only `/proc/self/status` memory telemetry is isolated to
    `go/internal/testcmd/reader_core_bench/main.go` and mirrors the Rust
    benchmark tool; it is not core SDK runtime code.
  - Indexed strategy time-bounded candidate collection can read more entry
    headers than Rust in some full-file candidate cases; this is a
    performance opportunity, not a correctness blocker, and belongs to the
    remaining benchmark/profile stage if it measures material.
  - Extreme arithmetic overflow differences are theoretical for realistic
    journal timestamps and entry counts; current checks and tests are
    sufficient for this chunk.

Same-failure scan:

- `rg -n "GetEntry\(|debug_collect_column_fields_by_row_traversal|/proc|machine-id|exec\.Command|os/exec|TODO|FIXME|HACK|BUG" go/journal/explorer.go go/journal/explorer_test.go go/journal/netdata.go go/journal/netdata_test.go go/internal/testcmd/netdata_function_wrapper/main.go go/internal/testcmd/reader_core_bench/main.go`
  found no `GetEntry()` calls in `go/journal/explorer.go` and no forbidden
  host probing in core Explorer or Netdata function code.
- Matches are limited to the explicit debug-row-traversal rejection string in
  `go/journal/explorer.go`, intended returned-row `GetEntry()` expansion in
  `go/journal/netdata.go`, existing benchmark `sdk-entry` mode `GetEntry()`
  use in `reader_core_bench`, and benchmark-only `/proc/self/status`
  telemetry in `reader_core_bench`.

Sensitive data gate:

- No raw journal payloads, customer identifiers, secrets, tokens, private
  endpoints, or personal data were written to durable artifacts. Reviewer
  summaries and validation evidence use sanitized file paths, counters,
  checksums, and command names.

Artifact maintenance gate:

- AGENTS.md: no update needed; no new project-wide workflow rule was
  discovered.
- Runtime project skills: no update needed; existing orchestration and journal
  compatibility skills covered the work.
- Specs: updated `.agents/sow/specs/product-scope.md` and
  `.agents/sow/specs/systemd-journal-plugin-facets.md` to describe Rust and
  Go as current Explorer/Netdata function implementations and to record the
  UID/GID display-name parity nuance.
- End-user/operator docs: updated `go/API.md` for Go Explorer, Netdata
  function, stdin request, cancellation, timeout, and UID/GID profile behavior.
- End-user/operator skills: no update needed; none exist for this surface.
- SOW lifecycle: this SOW is completed and moves to `done/` with the
  implementation, validation, docs/spec updates, and SOW lifecycle change in
  one commit.
- SOW-status.md: both status ledgers are updated during the final SOW move.

Specs update:

- Updated `.agents/sow/specs/product-scope.md`.
- Updated `.agents/sow/specs/systemd-journal-plugin-facets.md`.

Project skills update:

- No update needed; no new reusable workflow rule or compatibility rule was
  discovered.

End-user/operator docs update:

- Updated `go/API.md`.

End-user/operator skills update:

- No update needed; none exist for this surface.

Lessons:

- For large parity ports, keeping Rust/Go smoke output checksums and internal
  optimizer counters side by side caught behavioral drift faster than API-only
  review.
- Shell reviewer prompts must avoid unescaped backticks; a final `qwen` rerun
  was required because shell expansion removed a literal identifier from the
  first replacement prompt.
- End-to-end comparator parity was necessary but not sufficient for future
  maintainability; focused Go Netdata unit tests were added for the
  Rust-covered behavior clusters most likely to regress independently.
- Timeout behavior must be documented at the Netdata function contract level,
  not inferred from generic HTTP status semantics. The plugin-compatible
  contract returns a partial table with warning status.

Follow-up mapping:

- Go Netdata function API and wrapper: implemented in this SOW.
- Indexed-strategy time-bounded candidate early-exit: rejected as a required
  follow-up for this SOW. It is a possible future optimization, but the final
  profiled large-query comparison shows Go is materially comparable to Rust
  at `1.21x` slower and remaining cost is in expected scan paths, not in an
  accidental full-entry expansion or correctness issue.
- Go Netdata unit-test isolation gap: addressed in this SOW by adding focused
  request/source/profile/progress/timeout/sampling/response-mode tests.

## Outcome

Implemented the Go Explorer SDK API, generic Netdata logs function API, and
stdin-based Netdata function wrapper. The Go wrapper matches the Rust wrapper
on the committed Netdata function request matrix and the larger SOW-0093
journal-window benchmark, and Go has focused unit coverage for the Rust-covered
Netdata behavior clusters that are most likely to regress independently.

## Lessons Extracted

- Whole-SOW reviewer prompts can validly fail closeout if the SOW still has
  stale lifecycle text. Keep `Outcome`, `Lessons Extracted`, and `Followup`
  populated before the final review run.
- Comparator fixtures prove content parity, but focused unit tests are still
  needed for maintainability where one language has a larger local test set.
- Documentation needs explicit status-shape wording for controlled stops:
  cancellation is a compact function error, while timeout is a partial table
  with a warning.

## Followup

No required follow-up remains for SOW-0095. Netdata component integration
continues under SOW-0047 through SOW-0050, and the broader benchmark/profile
umbrella remains SOW-0009.
