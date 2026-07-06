# SOW-0131 - Directory Unique Cache And Go Parity

## Status

Status: completed

Sub-state: completed 2026-07-06 after implementation, validation, repeat
read-only review, and clean SOW audit.

## Requirements

### Purpose

Implement exact directory-wide unique-value enumeration that avoids repeated
per-query in-memory recomputation for Netdata-style repeated queries, while
preserving journal-native per-file FIELD/DATA traversal. Bring Go
sd-journal-style stateful unique enumeration to parity with Rust and add
benchmark evidence for high-cardinality unique enumeration.

### User Request

The user accepted the recommendation to do directory-wide cached exact unique
enumeration now rather than leave a follow-up. The user also clarified the
project policy for this work: known needed work is done now; unneeded work is
never done; no deferred technical debt is acceptable.

### Assistant Understanding

Facts:

- Rust `SdJournalQueryUniqueState` now streams stateful unique enumeration over
  FIELD/DATA chains without pre-materializing all returned payloads.
- Rust `DirectoryReader::enumerate_unique_payload()` deduplicates exact values
  across files with an in-memory `HashSet<Vec<u8>>` for each stateful query.
- Go `SdJournalQueryUniqueState` still calls `VisitUnique`, stores every
  `FIELD=value` payload in `sdJournal.uniqueItems`, and returns cloned payloads
  from `SdJournalEnumerateAvailableUnique` (`go/journal/facade.go`).
- Directory-wide exact uniqueness has no single native on-disk journal index;
  each journal file has its own FIELD/DATA chain.
- SOW-0129 listed benchmark evidence as an acceptance target, but only
  functional tests were completed.

Inferences:

- Repeated Netdata-style directory queries should not recompute and rehash the
  same directory-wide unique set every time when file membership and file
  metadata are unchanged.
- A cacheable exact directory-wide unique index can preserve correctness, avoid
  row scans, and make repeated queries cheaper.

Unknowns:

- Whether the existing Rust `journal-engine` cache is the right home for a
  durable index, or whether the first production-grade step should be a
  directory-reader-owned in-process cache with benchmark evidence.
- Whether Go has enough existing low-level iterator state to implement the
  stateful facade change surgically, or whether it needs reader/directory
  helpers first.

### Acceptance Criteria

- Rust directory unique enumeration uses an exact directory-wide cached unique
  index for repeated queries when the same directory file set and requested
  field are unchanged.
- The cached index is built only from per-file FIELD/DATA chains, not row scans.
- Rust stateful facade enumeration can stream from the cached directory index
  without recomputing the directory-wide dedupe set on every restart/query.
- Go `SdJournalQueryUniqueState` plus `SdJournalEnumerateAvailableUnique` no
  longer pre-materializes all unique payloads through the facade before the
  caller enumerates.
- Go stateful unique enumeration uses FIELD/DATA chains, preserves the existing
  owned `FIELD=value` return shape, and deduplicates exact values across files.
- Tests prove cross-file duplicate suppression, restart behavior, compressed
  DATA behavior where supported, and repeated-query cache reuse.
- Benchmark or benchmark-style evidence records allocation and time behavior
  for high-cardinality unique enumeration before/after the implementation, for
  Rust and Go where the benchmark surface exists.
- SOW-0129 benchmark deferral is resolved by this SOW's outcome.

## Analysis

Sources checked:

- `go/journal/facade.go`
- `go/journal/reader_unique.go`
- `go/journal/directory_reader.go`
- `tests/benchmarks/`
- `rust/src/journal/src/facade.rs`
- `rust/src/journal/src/directory.rs`
- `rust/src/crates/journal-core/src/file/reader.rs`
- `rust/src/crates/journal-core/src/file/file_iterators.rs`

Current state:

- Rust per-file FIELD/DATA streaming is fixed, but Rust directory state still
  rebuilds the cross-file dedupe set for each stateful unique query.
- Go still materializes stateful unique payloads eagerly.
- Benchmark harnesses exist under `tests/benchmarks/`, but no high-cardinality
  unique-enumeration benchmark was added for SOW-0129.

Risks:

- Implementing cached directory unique enumeration incorrectly can return stale
  values after file rotation, retention, or append publication.
- Implementing Go streaming without preserving restart behavior can break
  libsystemd-style callers.
- Benchmarks can become noisy if they use live host journals; use generated
  repository-local fixtures only.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- SOW-0129 fixed Rust per-file/facade streaming but left directory-wide exact
  unique dedupe as per-query in-memory state. Go still exposes the same
  stateful unique API shape and uses the old materializing implementation.
- The project requirement is now binary: do needed work now, and do not create
  speculative later work.

Evidence reviewed:

- User decision on 2026-07-06 that directory-wide exact unique caching is
  needed now, not deferred.
- Review findings from the SOW-0127 through SOW-0130 read-only reviewer batch.
- `go/journal/facade.go` stateful unique implementation.
- Rust SOW-0129 implementation and tests.
- `rust/src/journal/src/directory.rs` `unique_seen` state.
- `rust/src/crates/journal-core/src/file/reader.rs` FIELD/DATA iterator path.

Affected contracts and surfaces:

- Rust directory reader stateful unique internals and tests.
- Go facade API internals and tests.
- Performance benchmark artifacts under `tests/benchmarks/` or internal test
  commands.
- SOW-0129 follow-up mapping and release notes.

Existing patterns to reuse:

- Rust SOW-0129 FIELD/DATA stateful iterator pattern.
- Go `Reader.VisitUnique` and `DirectoryReader.VisitUnique` FIELD/DATA paths.
- Existing benchmark scripts under `tests/benchmarks/`.
- Rust cache-key discipline from SOW-0128 for any serialized or
  consumer-visible cache semantics.

Risk and blast radius:

- Medium implementation risk in Rust directory cache invalidation and Go reader
  state handling; low public API risk if owned return shape and function
  signatures remain unchanged.

Sensitive data handling plan:

- Use generated synthetic journal files only. Do not read live host journals or
  production data.

Implementation plan:

1. Implement exact Rust directory-wide unique cache keyed by requested field and
   stable directory file metadata.
2. Ensure Rust cached unique values are built only through per-file FIELD/DATA
   chains and are invalidated when directory file metadata changes.
3. Add Go reader/directory stateful unique helpers or equivalent iterator state.
4. Rework Go facade stateful unique enumeration to stream and preserve restart.
5. Add duplicate-sensitive cross-file tests and cache-reuse tests.
6. Add benchmark evidence for high-cardinality unique enumeration.

Validation plan:

- `go test ./...` with repository-local Go caches.
- Rust workspace tests with repository-local Cargo caches.
- Benchmark command output recorded in this SOW.
- External read-only reviewer pass before completion.

Artifact impact plan:

- `AGENTS.md`: no expected update.
- Runtime project skills: no expected update.
- Specs: update if cache behavior or public performance contract changes.
- End-user/operator docs: update if public Rust/Go facade or directory-reader
  behavior guidance changes.
- End-user/operator skills: no expected update.
- SOW lifecycle: this SOW tracks the SOW-0129 benchmark/parity follow-up and
  the user-approved directory-wide cache work.
- `SOW-status.md`: update while in progress and on close.

Open-source reference evidence:

- None checked yet; this is an internal parity/performance follow-up.

Open decisions:

- No user decision currently blocks implementation. The user explicitly
  approved doing the directory-wide exact unique cache now.

## Implications And Decisions

- 2026-07-06: User decision recorded. Work that is needed must be done now;
  work that is not needed must be rejected as never. This SOW therefore owns
  the directory-wide exact unique cache and Go parity instead of leaving them
  as deferred follow-ups.
- Implementation routing: the user explicitly rejected a separate implementer
  for this program. The primary agent is both implementer and orchestrator.

## Plan

1. Implement Rust directory-wide exact unique cache.
2. Implement Go streaming unique state and exact directory dedupe.
3. Add duplicate-sensitive and cache-reuse tests.
4. Add benchmark evidence.
5. Validate and review.

## Delegation Plan

Implementer:

- Local implementation by the primary agent per user routing decision for this
  program.

Reviewers:

- Read-only reviewers from the approved pool before completion.

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

- Record blockers and missing evidence before changing scope.

## Execution Log

### 2026-07-06

- Created from read-only reviewer findings during SOW-0127 through SOW-0130
  review.
- Activated after the user selected the directory-wide exact unique cache as
  now-required work and rejected deferred technical debt.
- Committed SOW-0127 through SOW-0130 before activating this work
  (`512f66c`) so this SOW has a clean implementation boundary.
- Implemented Rust `DirectoryReader` exact bounded unique cache:
  per-open-reader cache key is requested field plus per-file journal header
  signature; values are built only by walking per-file FIELD/DATA chains.
- Implemented Go reader stateful FIELD/DATA unique enumeration and Go
  `DirectoryReader` exact bounded unique cache; the facade now delegates
  `SdJournalQueryUniqueState`, `SdJournalRestartUnique`, and
  `SdJournalEnumerateAvailableUnique` to reader state instead of prebuilding
  `sdJournal.uniqueItems`.
- Protected active stateful unique enumerations from bounded-cache eviction in
  Rust and Go, and made Go report a missing active cache entry as an error
  rather than silently ending enumeration.
- Added duplicate-sensitive cache reuse tests, compressed Go stateful unique
  coverage, active-iterator cache-pressure tests, Rust high-cardinality
  benchmark-style evidence, and Go `-benchmem` benchmarks.
- Updated `docs/Rust-API.md`, `docs/Go-API.md`, `go/README.md`, and
  `.agents/sow/specs/product-scope.md` to describe bounded exact directory
  unique caching.
- Ran first read-only reviewer batch with Claude, GLM, Kimi, Qwen, MiniMax,
  DeepSeek, and Mimo. Mimo produced only a tool header and no review content.
  Claude, GLM, Kimi, and DeepSeek found blocking Go live-append freshness
  issues; Qwen and MiniMax considered the pre-fix implementation production
  grade.
- Fixed the blocking findings:
  - Go directory cache now refreshes each opened file header before cache-key
    lookup, so cache hits cannot use stale per-file metadata.
  - Go unique refresh now reuses the existing entry-offset refresh path, so
    `header`, `fileSize`, and `entryOffsets` stay consistent before later row
    iteration.
  - Rust and Go now insert rebuilt directory cache entries under the verified
    pre-build key, not a third freshly-read key that could race with a live
    append.
  - Rust and Go now have live-append regression tests proving cache
    invalidation and, for Go, preserving row iteration after a unique refresh.
  - Go high-cardinality cold benchmark now resets both cache map and LRU order.
- Tightened docs/spec wording to state the exact 8-entry LRU, per-entry full
  unique-set memory model, and already-open file-set invalidation scope.

Reviewer disposition after first batch:

- NOW: Go stale cache on live append, Go unique refresh / entry-offset
  invariant, Rust verified-key race, live-append tests, benchmark-order reset,
  and docs/spec memory/invalidation precision.
- NEVER / NOT NEEDED for this SOW:
  - Byte cap on unique cache entries. Exact unique enumeration requires holding
    the full unique set for a cached field; the 8-entry LRU is the intended
    bound and is now documented.
  - Restoring the single-file directory fast path. `FileReader` remains the
    streaming single-file API; `DirectoryReader` now consistently provides the
    repeated-query cache behavior this SOW owns.
  - Replacing whole-directory cache entries with per-file sub-caches. The
    shipped contract is exact reuse while already-open file header signatures
    are unchanged; per-file sub-caches are a different granularity and not
    required for this SOW's accepted contract.
  - Changing Go facade iterator reset behavior. The checkpoint implementation
    already cleared unique facade state in `resetIterators()`, so this SOW did
    not introduce the reviewer-claimed behavioral change.
  - Go string cache keys and the package-private test counter. They are not
    public API, not a correctness issue, and benchmark evidence is acceptable.

## Validation

- `cargo fmt --all` from `rust/` - passed.
- `gofmt -w` on changed Go files - passed.
- Focused Rust tests:
  - `cargo test --manifest-path rust/Cargo.toml -p systemd-journal-sdk directory_reader_query_unique_deduplicates_indexed_values_across_files -- --nocapture` - passed.
  - `cargo test --manifest-path rust/Cargo.toml -p systemd-journal-sdk directory_reader_unique_cache_high_cardinality_reuses_index -- --nocapture` - passed.
    Evidence output: `cold_build=207.411µs cached_restart=64.658µs`.
  - `cargo test --manifest-path rust/Cargo.toml -p systemd-journal-sdk directory_reader_unique_state_survives_cache_pressure -- --nocapture` - passed.
  - `cargo test --manifest-path rust/Cargo.toml -p systemd-journal-sdk directory_reader_unique -- --nocapture` - passed after the live-append fixes. Evidence output:
    `cold_build=339.831µs cached_restart=121.837µs`.
  - `cargo test --manifest-path rust/Cargo.toml -p systemd-journal-sdk jf_facade_unique_state_handles_compressed_payloads_and_restart -- --nocapture` - passed.
- Focused Go tests:
  - `go test ./journal -run 'TestDirectoryReader|TestSdJournalStatefulUniqueHandlesCompressedPayloadsAndRestart|TestSdJournalJfFacadeStatefulReaderOperations' -count=1 -v` - passed.
  - `go test ./journal -run TestDirectoryReaderUniqueStateSurvivesCachePressure -count=1 -v` - passed.
  - `go test ./journal -run 'TestDirectoryReaderUniqueCacheInvalidatesAfterLiveAppend|TestReaderUniqueRefreshPreservesLiveEntryIteration' -count=1 -v` - passed.
- Go benchmark evidence:
  - `go test ./journal -run '^$' -bench 'BenchmarkDirectoryReaderUniqueHighCardinality' -benchmem -count=3` - passed.
  - Latest cold build results after fixes: `172121 ns/op, 140933 B/op, 2618
    allocs/op`; `184840 ns/op, 140934 B/op, 2619 allocs/op`; `177215 ns/op,
    140934 B/op, 2619 allocs/op`.
  - Latest cached restart results after fixes: `16363 ns/op, 12000 B/op, 500
    allocs/op`; `16370 ns/op, 12000 B/op, 500 allocs/op`; `16067 ns/op,
    12000 B/op, 500 allocs/op`.
- Docs validation:
  - `python3 tests/docs/check_wiki_docs.py` - passed.
  - `python3 tests/docs/verify_examples.py` - passed 31/31 examples.
- Full suites:
  - `cargo test --manifest-path rust/Cargo.toml --workspace` - passed after
    reviewer fixes.
  - `go test ./...` from `go/` - passed after reviewer fixes.
- `git diff --check` - passed after reviewer fixes.

Acceptance criteria evidence:

- Rust and Go directory unique enumeration build exact values through per-file
  FIELD/DATA chains and deduplicate across files.
- Rust and Go reuse an exact 8-entry per-open-reader LRU cache for repeated
  queries and stateful restarts while the already-open file header signatures
  are unchanged.
- Rust and Go invalidate the cache after live appends to already-open files.
- Go stateful facade unique enumeration delegates to reader state instead of
  prebuilding `sdJournal.uniqueItems`, while preserving the public facade
  function signatures and owned `FIELD=value` payload shape.
- Go live unique refresh preserves later entry iteration by sharing the
  existing entry-offset refresh path.
- Benchmark evidence records cold build versus cached restart time and
  allocation behavior for high-cardinality unique enumeration.

Tests or equivalent validation:

- Formatting, focused tests, full Rust workspace tests, full Go tests, docs
  checks, verified examples, benchmarks, and `git diff --check` are listed
  above.

Real-use evidence:

- Validation used generated synthetic journal files and append/reopen flows
  through the repository's public Go and Rust writer/reader APIs. No live host
  journals or production data were read.

Reviewer findings:

- Round 1:
  - Claude, GLM, Kimi, and DeepSeek found Go live-append cache freshness
    problems. Claude and DeepSeek also identified the Go unique refresh /
    entry-offset invariant risk. Claude identified the Rust verified-key race.
  - Qwen and MiniMax considered the pre-fix implementation production-grade
    but had low-severity notes.
  - Mimo produced no review content.
- Round 1 disposition:
  - Fixed now: Go live-append freshness, Go entry-offset invariant, Rust/Go
    verified-key insertion, live-append tests, benchmark LRU reset, and
    docs/spec precision.
  - Rejected as not needed for this SOW: byte cap, single-file directory fast
    path restoration, per-file sub-cache redesign, Go string-key rewrite, and
    Go test-counter build gating.
- Round 2:
  - Claude, GLM, Qwen, DeepSeek, and MiniMax reported production-grade code.
  - Kimi reported production-grade implementation but found this SOW was
    missing the required `Sensitive data gate:` validation heading. This SOW
    update fixes that process gap.
  - Mimo timed out with no review content in both rounds.
- Public API contract finding:
  - Rust public signatures are unchanged.
  - Go adds stateful unique methods to public reader types and extends only
    the unexported facade reader interface. This is additive and does not
    require a Netdata integration change. The public facade function
    signatures are unchanged.

Same-failure scan:

- `rg -n "func \\(dr \\*DirectoryReader\\) ensureUniqueCache|refreshUniqueHeaders\\(|uniqueCacheKey\\(|finalKey :=|func \\(r \\*Reader\\) refreshUniqueHeader" go/journal/directory_reader.go go/journal/reader_unique.go` showed Go refreshes headers before cache-key lookup and inserts under the verified key.
- `rg -n "fn ensure_unique_cache|unique_cache_key\\(|final_key|fn enforce_unique_cache_capacity|DIRECTORY_UNIQUE_CACHE_CAPACITY" rust/src/journal/src/directory.rs` showed the Rust 8-entry cache, verified-key insertion, and active-state capacity guard.
- `rg -n "PRODUCTION GRADE|NOT PRODUCTION GRADE|BLOCKER|HIGH|MEDIUM|Sensitive data gate|MUST FIX" .local/reviewer-logs/*-sow-0131-review-2.txt` showed second-round reviewers found no code blockers; the remaining Kimi finding was this SOW validation heading.

Sensitive data gate:

- Durable artifacts contain no raw secrets, credentials, bearer tokens, SNMP
  communities, private keys, connection strings, customer names, community
  member names, customer identifiers, personal data, customer-identifying
  public IP addresses, private endpoints, account IDs, or proprietary incident
  details.
- Tests, benchmarks, and examples used generated synthetic journal fixtures.
- Reviewer logs are under `.local/` and are not durable commit artifacts.

Artifact maintenance gate:

- `AGENTS.md`: no update needed; workflow and guardrails did not change.
- Runtime project skills: no update needed; implementation used existing SOW,
  orchestration, docs, and journal compatibility rules.
- Specs: `.agents/sow/specs/product-scope.md` updated for exact directory
  unique cache behavior.
- End-user/operator docs: `docs/Rust-API.md`, `docs/Go-API.md`, and
  `go/README.md` updated.
- End-user/operator skills: none exist for this changed surface; no update
  needed.
- SOW lifecycle: this SOW owns and closes the SOW-0129 benchmark/parity gap
  plus the user-approved directory unique cache work. No follow-up SOW is
  required for this scope.
- `SOW-status.md`: root and `.agents/sow/SOW-status.md` updated for the active
  SOW state and will be updated again on close.

Specs update:

- `.agents/sow/specs/product-scope.md` now records the current exact 8-entry
  per-open-reader LRU cache contract, invalidation scope, and memory model.

Project skills update:

- No runtime project skill update was needed; no HOW-to-work rule changed.

End-user/operator docs update:

- `docs/Rust-API.md`, `docs/Go-API.md`, and `go/README.md` now describe the
  exact cache behavior and memory model.

End-user/operator skills update:

- No output/reference skills are affected by this SDK/docs change.

Lessons:

- Header freshness fixes must preserve every reader invariant that depends on
  the header. In Go, unique refresh has to share the entry-offset refresh path
  instead of updating `header` and `fileSize` alone.
- Cache entries for live files must be inserted under the verified key for the
  state they were built against, not under a later freshly-read key.
- Reviewer-reported API contract changes must be checked against the exact
  checkpoint; the claimed Go facade reset behavior change was not real because
  the checkpoint already cleared unique facade state on iterator reset.

Follow-up mapping:

- Implemented now: SOW-0129 benchmark evidence, Rust directory unique cache,
  Go stateful unique parity, live-append invalidation tests, active-state
  eviction protection, and docs/spec updates.
- Rejected as not needed for this SOW: byte cap, single-file directory fast
  path restoration, per-file sub-cache redesign, Go string-key rewrite, and
  Go test-counter build gating.
- No valid deferred work remains.

## Outcome

Completed. Rust and Go now provide exact 8-entry per-open-reader directory
unique caches built from per-file FIELD/DATA chains; Go stateful unique facade
enumeration delegates to reader state; live append invalidation and Go
entry-iteration invariants are covered by tests; docs/specs record the cache
contract and memory model.

## Lessons Extracted

Recorded in `Lessons` above.

## Followup

No follow-up SOW is required for this scope.
