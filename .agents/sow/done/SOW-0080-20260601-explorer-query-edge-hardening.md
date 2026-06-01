# SOW-0080 - Explorer Query Edge Hardening

## Status

Status: completed

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: implemented, reviewed, validated, and closed.

## Requirements

### Purpose

Harden the Rust and Go optimized explorer/query API after the initial production-grade SOW-0074 delivery, so edge-case semantics, diagnostics, directory ordering, and feature coverage are precise enough for Netdata-style log exploration and future SDK consumers.

### User Request

Close the non-blocking edge findings found during SOW-0074 review without blocking the delivered Rust/Go explorer API.

### Assistant Understanding

Facts:

- SOW-0074 delivered Rust and Go explorer query APIs, filtered unique-value APIs, DATA-reference visitors, isolated comparison tools, query suites, and benchmark runners.
- All five SOW-0074 reviewers voted `PRODUCTION GRADE`.
- Reviewers identified edge hardening items that are not blockers for the current API but should be tracked explicitly.

Inferences:

- These items are best handled together because they affect explorer diagnostics, feature coverage, and exact parity semantics rather than the core API shape.
- The work should remain Rust and Go scoped unless a finding proves shared docs/specs or test harnesses need a broader update.

Unknowns:

- Whether Go reusable compressed-payload buffers provide measurable benefit; this requires profiling or benchmark evidence before implementation.
- Whether directory tie-break ordering can be aligned exactly with existing directory readers without changing public result ordering in currently passing tests.

### Acceptance Criteria

- Rust and Go directory explorer row ordering either matches the existing directory reader comparator for timestamp/sequence ties, or the SOW records measured evidence and a user decision for a deliberate deviation.
- Explorer query validation covers explicit xz, lz4, compact+xz/lz4 where supported, and FSS/sealed files where supported by existing fixture generators.
- `DataRefsReported` / `data_refs_reported` is removed, documented as reserved, or wired to a real and consistently incremented counter path in both Rust and Go.
- Compressed payload counter semantics distinguish selected payload decompression from planning-time collision-verification decompression, or the public docs explicitly define the existing counter scope.
- Empty FTS counter semantics are consistent between the isolated baseline and optimized tools, or docs state clearly which counters are diagnostic-only and not part of result equality.
- The Go compressed-payload materialization buffer allocation profile is measured. If reusable buffering is materially faster or lower-allocation, implement it; otherwise record the evidence and leave the simpler path.
- Existing SOW-0074 smoke suites and benchmark runners still pass after the hardening changes.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0074-20260531-rust-go-optimized-log-explorer-query-api.md` - SOW-0074 reviewer findings and dispositions.
- `rust/src/journal/src/explorer.rs` - Rust explorer counters, query executor, and directory merge behavior.
- `go/journal/explorer.go` - Go explorer counters, query executor, and directory merge behavior.
- `rust/src/journal/src/lib.rs` and `go/journal/reader.go` - existing directory reader ordering comparators.
- `tests/explorer_query/` - current explorer query smoke and benchmark harnesses.

Current state:

- SOW-0074 behavior is production-grade for the delivered feature slice.
- zstd, compact, compact+zstd, and mixed-directory fixtures are covered by the current smoke suites.
- xz, lz4, and FSS explorer fixtures are not yet explicit coverage in the explorer query smoke suite.
- The current directory explorer merge uses a simpler key than the existing directory reader comparator.
- The current explorer report schema includes a DATA-reference counter that is reserved or unused in the initial implementation.

Risks:

- Changing directory ordering can alter externally visible row order for tie cases; tests must include tie fixtures.
- Tightening counters can break benchmark report consumers if fields are removed without documentation or schema-version handling.
- Adding compression/FSS fixtures can make routine smoke runs slower; the suite should preserve a small default and a fuller matrix path.
- Optimizing Go compressed buffer reuse without measurement could add complexity without benefit.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- SOW-0074 optimized the core query path first. Reviewers found remaining edge items around diagnostics, rare directory tie ordering, and feature-variant coverage. These do not invalidate the main API, but leaving them undocumented would create ambiguity for future agents and consumers.

Evidence reviewed:

- SOW-0074 reviewer findings recorded in `.agents/sow/current/SOW-0074-20260531-rust-go-optimized-log-explorer-query-api.md`.
- Existing directory reader comparator code in Rust and Go.
- Existing explorer smoke suite coverage under `tests/explorer_query/`.

Affected contracts and surfaces:

- Rust explorer query API diagnostics and directory query ordering.
- Go explorer query API diagnostics and directory query ordering.
- Explorer query smoke and benchmark report schemas.
- Product specs and Rust/Go docs if public counter semantics change.
- Project compatibility skill if new mandatory validation patterns are added.

Existing patterns to reuse:

- Existing directory reader comparator semantics.
- SOW-0074 isolated baseline/optimized comparison tool model.
- Existing explorer smoke runner `--suite full` and compression/compact flags.
- Existing mixed-directory and verify/FSS fixture generation patterns.

Risk and blast radius:

- Medium correctness risk for directory ordering ties.
- Low API risk if counters are documented or schema-compatible.
- Medium performance risk if additional diagnostics accidentally add hot-path cost.
- Low security and sensitive-data risk because generated fixtures and sanitized reports are sufficient.

Sensitive data handling plan:

- Use generated fixtures and sanitized benchmark/report summaries only. Do not commit raw real-world journal payloads, hostnames, customer identifiers, secrets, credentials, bearer tokens, SNMP communities, private endpoints, or personal data.

Implementation plan:

1. Add targeted Rust and Go tests for directory tie ordering and compare against existing directory reader semantics.
2. Decide and implement the `DataRefsReported` counter disposition consistently in Rust and Go.
3. Clarify decompression counter semantics and update code/docs/tests as needed.
4. Extend explorer smoke coverage for xz, lz4, and FSS/sealed fixtures where current fixture support allows it.
5. Measure Go compressed-payload allocation behavior; implement reusable buffering only if evidence supports it.
6. Update specs/docs/skills only where public semantics or required validation patterns change.

Validation plan:

- Rust explorer tests and full explorer smoke suite.
- Go explorer tests and full explorer smoke suite.
- Added xz/lz4/FSS explorer query coverage where supported.
- Directory tie-ordering test comparing explorer directory query to existing directory reader ordering.
- Benchmark runner or allocation profile evidence for the Go compressed-buffer decision.
- `git diff --check`.
- `.agents/sow/audit.sh`.
- Whole-SOW read-only reviewer pass with all reviewer findings dispositioned.

Artifact impact plan:

- AGENTS.md: no expected change; this is SOW-specific hardening.
- Runtime project skills: update `.agents/skills/project-journal-compatibility/SKILL.md` only if new validation requirements become durable workflow rules.
- Specs: update `.agents/sow/specs/` if public counter or ordering semantics change.
- End-user/operator docs: update Rust/Go docs if public counters or query behavior change.
- End-user/operator skills: no expected impact.
- SOW lifecycle: keep this SOW in pending until activated; close only after implementation, validation, review, and follow-up mapping.
- SOW-status.md: record open/current/completed state changes.

Open-source reference evidence:

- No external open-source repositories were needed for SOW creation. This SOW hardens behavior introduced inside this repository. If implementation changes journal ordering semantics, systemd source may be consulted and recorded at implementation time.

Open decisions:

- None blocking. The default direction is to align with existing Rust/Go directory reader behavior and keep report schema changes explicit.

## Implications And Decisions

- SOW-0074 remains complete; this SOW tracks non-blocking hardening rather than reopening the delivered API.
- Diagnostics are part of the public benchmark/reporting surface. Ambiguous counters must be documented, renamed, removed, or implemented consistently.

## Plan

1. Ordering and counter semantics.
   - Scope: Rust/Go explorer directory ordering and counter definitions.
   - Risk: visible ordering or report schema change.
   - Dependencies: SOW-0074 implementation.
2. Feature-variant smoke coverage.
   - Scope: xz/lz4/FSS explorer fixtures and runner options.
   - Risk: slower full-suite runs.
   - Dependencies: existing fixture generators.
3. Go compressed materialization measurement.
   - Scope: allocation/profile evidence and optional buffer reuse.
   - Risk: unnecessary complexity if not measured first.
   - Dependencies: Go explorer implementation.

## Delegation Plan

Implementer:

- Current project routing is local implementation by the project manager unless the user explicitly changes routing.

Reviewers:

- Run whole-SOW read-only reviews after implementation and local validation using:
  - `llm-netdata-cloud/minimax-m2.7-coder`
  - `llm-netdata-cloud/kimi-k2.6`
  - `llm-netdata-cloud/qwen3.6-plus`
  - `llm-netdata-cloud/glm-5.1`
  - `llm-netdata-cloud/mimo-v2.5-pro`

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

- If a reviewer finds a blocking correctness issue in SOW-0074 behavior, reopen SOW-0074 as a regression instead of solving it here.
- If fixture generation cannot produce an xz/lz4/FSS combination, record the exact blocker and create a narrower follow-up only if needed.
- If Go buffer reuse does not show a measurable benefit, record the benchmark and leave the simpler implementation.

## Execution Log

### 2026-06-01

- Created from SOW-0074 whole-SOW reviewer non-blocking findings.
- Activated for local implementation.
- Implemented Rust and Go directory explorer ordering through the existing
  directory-reader comparator, with per-file limits disabled before the final
  directory merge and global limit applied after merge sorting.
- Removed unused `data_refs_reported` / `DataRefsReported` diagnostics from
  Rust and Go optimized report schemas.
- Documented `payloads_decompressed` / `PayloadsDecompressed` as selected
  payload materialization evidence, not planning-time DATA hash-collision
  verification evidence.
- Added empty-FTS consistency coverage to the shared full query suite and
  aligned the baseline tools with the optimized tool's zero-scan empty-needle
  behavior.
- Extended Rust and Go dataset ingesters and smoke runners with deterministic
  sealed fixture generation and explicit xz/lz4/compact+compression options.
- Measured and implemented Go explorer compressed-payload reuse for zstd/lz4
  materialization. The final benchmark recorded:
  - generic `readDataPayload`: `10496 ns/op`, `33625 B/op`, `20 allocs/op`;
  - explorer `materializePayload`: `2556 ns/op`, `0 B/op`, `0 allocs/op`.
- Recorded the Go reader concurrency and internal materialized-payload lifetime
  contract after reviewer feedback. `Reader` and `DirectoryReader` are not
  safe for concurrent method calls; internal materialized payload slices are
  ephemeral and must be consumed or cloned before the next materialization.

## Validation

Acceptance criteria evidence:

- Directory explorer ordering:
  - Rust test `directory_explorer_uses_directory_reader_tie_ordering` passes.
  - Go test `TestDirectoryExplorerUsesDirectoryReaderTieOrdering` passes.
  - Rust implementation now carries private per-row `DirectoryEntryKey` values
    through file query execution for directory sorting.
  - Go implementation now carries private per-row `directoryEntryKey` values
    through file query execution for directory sorting; no public `ExplorerRow`
    shape change is used for the sort key.
- Compression/FSS validation:
  - Rust full smoke variants passed for `none`, `zstd`, `xz`, `lz4`,
    `compact+zstd`, `compact+xz`, `compact+lz4`, `sealed`, and mixed directory.
  - Go full smoke variants passed for `none`, `zstd`, `xz`, `lz4`,
    `compact+zstd`, `compact+xz`, `compact+lz4`, `sealed`, and mixed directory.
- DATA-ref counter disposition:
  - Removed unused Rust `data_refs_reported` counter and optimized report field.
  - Removed unused Go `DataRefsReported` counter and optimized report field.
- Decompression counter semantics:
  - Documented in Rust and Go public docs/specs that decompression counters
    cover selected materialized payloads, not internal filter-planning hash
    collision checks.
- Empty FTS:
  - Added `tests/explorer_query/queries/full/22-fts-empty.json`.
  - Rust and Go smoke runners assert zero `fts_payloads_scanned` for baseline
    and optimized reports on empty FTS.
- Go compressed allocation:
  - Final benchmark: `.local/explorer-query/sow-0080-smoke-logs/go-materialization-bench-final.log`.
  - Reusable explorer decompression reduced zstd materialization from
    `10496 ns/op`, `33625 B/op`, `20 allocs/op` to `2556 ns/op`, `0 B/op`,
    `0 allocs/op` on the local Linux host.
- SOW-0074 smoke and benchmark runners:
  - Rust and Go full smoke suites pass.
  - Rust and Go 1000-row benchmark runners pass with equivalent outputs.

Tests or equivalent validation:

- `cargo test -p journal explorer --manifest-path rust/Cargo.toml`: passed,
  8 explorer tests.
- `cargo check -p dataset_ingester -p explorer_query_baseline -p explorer_query_optimized --manifest-path rust/Cargo.toml`: passed.
- `cargo test -p journal --manifest-path rust/Cargo.toml`: passed, 33 tests.
- `go test ./internal/testcmd/dataset_ingester ./internal/testcmd/explorer_query_baseline ./internal/testcmd/explorer_query_optimized`: passed.
- `go test ./journal`: passed.
- `go test ./...`: passed.
- `python3 -m py_compile tests/explorer_query/run_rust_smoke.py tests/explorer_query/run_go_smoke.py`: passed.
- Rust full smoke:
  - `python3 tests/explorer_query/run_rust_smoke.py --suite full --keep-going`: passed.
  - `--compression zstd`: passed.
  - `--compression xz`: passed.
  - `--compression lz4`: passed.
  - `--compact --compression zstd`: passed.
  - `--compact --compression xz`: passed.
  - `--compact --compression lz4`: passed.
  - `--sealed`: passed.
  - `--surface directory`: passed.
- Go full smoke:
  - `python3 tests/explorer_query/run_go_smoke.py --suite full --keep-going`: passed.
  - `--compression zstd`: passed.
  - `--compression xz`: passed.
  - `--compression lz4`: passed.
  - `--compact --compression zstd`: passed.
  - `--compact --compression xz`: passed.
  - `--compact --compression lz4`: passed.
  - `--sealed`: passed.
  - `--surface directory`: passed.
- `python3 tests/explorer_query/run_rust_benchmarks.py --rows 1000 --keep-going`: passed; summary `.local/explorer-query/benchmarks/rust-1000-rows-none-compact/summary.json`.
- `python3 tests/explorer_query/run_go_benchmarks.py --rows 1000 --keep-going`: passed; summary `.local/explorer-query/benchmarks/go-1000-rows-none-compact/summary.json`.
- `go test ./journal -run '^$' -bench BenchmarkExplorerCompressedPayloadMaterialization -benchmem`: passed with final allocation evidence recorded above.

Real-use evidence:

- This SOW uses generated deterministic fixtures only. No live host journal or
  real-world corpus payloads were used. That is intentional for SOW-0080
  because it hardens API edge behavior and feature-variant query coverage.

Reviewer findings:

- `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  - Non-blocking: Go `materializePayload` now returns ephemeral uncompressed
    mmap/read slices and compressed scratch slices; future internal callers
    must clone if they retain data. Disposition: accepted; added an internal
    comment on `materializePayload`.
  - Non-blocking: Go xz materialization is not allocation-optimized like zstd
    and lz4. Disposition: accepted; xz remains correct and covered by smoke
    suites, and this SOW measured/optimized the zstd/lz4 hot path.
  - Non-blocking: `explorerDecompressScratch` retains the largest seen buffer.
    Disposition: accepted as a deliberate performance tradeoff; the buffer is
    reader-scoped and released when the reader is closed.
  - Non-blocking: Rust and Go differ internally for uncompressed payload
    allocation. Disposition: accepted; APIs remain equivalent and the Go path
    is intentionally zero-copy on internal hot paths.
- `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  - Non-blocking: xz uses a fresh reader per materialization and was not
    benchmarked separately. Disposition: accepted; correctness coverage exists
    and no xz production hot-path claim is made.
  - Non-blocking: ephemeral scratch-slice ownership should be documented.
    Disposition: accepted; added the internal `materializePayload` comment.
  - Non-blocking: native xz/lz4 unit tests and backward-direction directory
    ordering tests could add narrower coverage. Disposition: accepted as
    future optional hardening; full smoke coverage and comparator reuse tests
    satisfy this SOW.
- `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  - No blocking findings. Non-blocking observations were limited to xz
    materialization asymmetry and unit-level coverage opportunities already
    dispositioned above.
- `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
  - Non-blocking: Go reusable decompression buffers are not safe for concurrent
    use of one `Reader`. Disposition: accepted; documented in `go/API.md` that
    `Reader` and `DirectoryReader` are not safe for concurrent method calls.
  - Non-blocking: stale `.local/` generated reports can still mention the
    removed `data_refs_reported` field. Disposition: accepted; `.local/`
    outputs are scratch artifacts and not committed source.
- `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
  - Blocking process finding: `tests/explorer_query/queries/full/22-fts-empty.json`
    was untracked, so clean checkouts would not exercise empty-FTS assertions.
    Disposition: fixed by staging and committing the fixture with this SOW.
  - Non-blocking: redundant Go DATA header revalidation, ephemeral scratch
    ownership, xz allocation behavior, unstable equal-key directory sort, and
    SOW close hygiene. Disposition: accepted; equal-key ordering matches the
    existing reader comparator behavior, and process hygiene is resolved by
    closing this SOW.

Same-failure scan:

- `rg -n "DataRefsReported|data_refs_reported" rust go tests` finds no
  remaining implementation/report fields after removal.
- Directory ordering was checked against the existing Rust
  `DirectoryReader::compare_entry_keys` and Go
  `DirectoryReader.compareEntryKeys` comparator paths.
- Empty FTS behavior was checked through both Rust and Go full smoke suites.

Sensitive data gate:

- SOW creation contains only sanitized project paths and technical descriptions. No raw secrets, credentials, bearer tokens, SNMP communities, community member names, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details are included.

Artifact maintenance gate:

- AGENTS.md: no change. Existing runtime-purity and SOW rules still apply.
- Runtime project skills: updated `.agents/skills/project-journal-compatibility/SKILL.md`
  so future Rust/Go explorer/query changes must cover zstd/xz/lz4, compact
  plus compression, sealed/FSS, and mixed-directory fixtures.
- Specs: updated `.agents/sow/specs/product-scope.md` with explorer
  decompression counter scope.
- End-user/operator docs: updated `rust/README.md`, `go/API.md`, and
  `tests/explorer_query/README.md`.
- End-user/operator skills: no output/reference skill exists or is affected.
- SOW lifecycle: completed and moved to `.agents/sow/done/`.
- SOW-status.md: updated to list this SOW as completed.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` to define explorer
  decompression counter scope.

Project skills update:

- Updated `.agents/skills/project-journal-compatibility/SKILL.md` with durable
  explorer/query validation expectations for xz/lz4, compact plus compression,
  sealed/FSS, and mixed-directory fixtures.

End-user/operator docs update:

- Updated `rust/README.md`, `go/API.md`, and `tests/explorer_query/README.md`.

End-user/operator skills update:

- No output/reference skill exists or is affected.

Lessons:

- Non-blocking production-grade review findings should become explicit SOWs when they affect diagnostics or edge compatibility.
- Directory-query correctness needs a private sort-key path. Public row result
  structs should not grow hidden sort-key fields when private result wrappers
  can preserve API shape.
- Explorer diagnostics must define what is and is not counted. Planning-time
  collision verification and selected payload materialization are different
  cost centers.

Follow-up mapping:

- This SOW is the follow-up mapping for SOW-0074. No additional follow-up is
  currently required from local validation.

## Outcome

SOW-0080 is complete. Rust and Go explorer query APIs now use existing
directory reader ordering for directory result merges, have consistent and
documented diagnostics, cover empty FTS behavior, and validate xz/lz4,
compact+compression, sealed/FSS, and mixed-directory fixtures. Go compressed
payload materialization now reuses zstd/lz4 decompression state on the explorer
hot path with measured zero-allocation behavior for the benchmarked zstd case.

## Lessons Extracted

- Internal zero-copy or scratch-backed payload helpers must carry explicit
  lifetime comments, even when all current callers are private and safe.
- Public Go reader docs must state the concurrency contract. The implementation
  has mutable cursor, enumeration, mmap, and decompression state, so callers
  need separate readers or serialized access for concurrent queries.
- SOW validation is not complete until new fixture files are tracked in git;
  local smoke success with untracked fixtures is not reproducible evidence.

## Followup

None required for this SOW. Optional future hardening could add xz-specific
allocation benchmarks and narrower language-native xz/lz4 unit tests if those
formats become hot in production explorer workloads.
