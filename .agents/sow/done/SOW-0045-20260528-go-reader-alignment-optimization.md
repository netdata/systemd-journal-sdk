# SOW-0045 - Go Reader Alignment Optimization

## Status

Status: completed

Sub-state: completed on 2026-05-29. Go reader now has Rust-aligned payload
APIs where practical, mmap-backed Unix access, live/snapshot bounds,
byte-preserving RAW field access, facade DATA fast paths, Go reader benchmark
coverage, and recorded benchmark evidence.

## Requirements

### Purpose

Align the Go reader with the optimized Rust reader reference and make it fit for
Netdata reader use cases where Go is relevant.

### User Request

After Rust reader parity and optimization, the user wants Go fixed and
optimized next. The user also noted that the Go writer uses mmap while the Go
reader does not, and that this likely affects performance.

### Assistant Understanding

Facts:

- Go reader currently uses `ReadAt()`-style reads rather than mmap-backed object
  access.
- Go reader must support ordered directory reading and shared reader API
  behavior.
- Go reader performance must be measured separately for single-file and
  directory readers.

Inferences:

- Go reader should be compared against optimized Rust, not the current
  pre-optimization Rust reader.

Unknowns:

- Whether Go mmap reader access should be default, optional, or avoided after
  measurement.

### Acceptance Criteria

- Go reader behavior matches the Rust reader contract from SOW-0043.
- Go reader performance is measured for single-file and ordered directory
  reading.
- Go reader mmap versus `ReadAt()` behavior is measured and decided with
  evidence.
- Go reader passes shared reader conformance and mixed-directory tests.
- Go reader performance gaps versus Rust are profiled and either fixed or
  explicitly recorded.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `go/journal/reader.go`
- `.agents/sow/specs/product-scope.md`

Current state:

- Go reader is functionally capable but likely not optimized for hot-path
  reader use.

Risks:

- Mmap can improve read hot paths but adds resize/SIGBUS/platform complexity.
- Optimizing Go before Rust reader closure could copy the wrong behavior.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Go reader should follow the optimized Rust reader reference from SOW-0052 and
  the Python/Node.js parity lessons from SOW-0053/SOW-0054.
- The current Go reader is functionally capable but still uses `ReadAt()` and
  allocates/copies heavily in the entry hot path. It also lacks the current Rust
  reader payload API layer used for high-throughput scans and facade DATA
  enumeration.
- The current reader-core benchmark harness does not include Go reader cases, so
  Go has no trustworthy apples-to-apples reader performance number yet.

Evidence reviewed:

- Current Go reader implementation and product scope spec.
- `.agents/sow/done/SOW-0052-20260529-rust-reader-last-mile-optimization.md`
  for optimized Rust reader contract and benchmark baseline.
- `.agents/sow/done/SOW-0053-20260529-python-reader-writer-rust-port.md` and
  `.agents/sow/done/SOW-0054-20260529-node-reader-writer-rust-port.md` for
  cross-language parity API shape.
- `tests/benchmarks/run_reader_core_benchmarks.py` currently lists Rust, Python,
  Node.js, and systemd reader cases but no Go reader cases.
- `go/journal/reader.go` currently materializes full entries with repeated
  `ReadAt()` calls and per-field copies in `readEntryAt()`.

Affected contracts and surfaces:

- Go reader API, directory reader, libsystemd-compatible facade, journalctl
  rewrite, and Netdata integration readiness.

Existing patterns to reuse:

- Optimized Rust reader behavior from SOW-0052.
- Go writer mmap support where applicable.
- Shared reader fixtures.
- Python and Node.js current-entry payload APIs and facade fast paths where Go
  cannot expose Rust borrowed slice lifetimes exactly.

Risk and blast radius:

- High for Go reader consumers once integrated.

Sensitive data handling plan:

- Use generated or public fixtures only. Do not record real customer logs,
  SNMP communities, credentials, bearer tokens, personal data, private
  endpoints, or production incident details.

Implementation plan:

1. Compare Go reader against Rust reader contract.
2. Add missing API/behavior parity.
3. Benchmark `ReadAt()` and mmap strategies.
4. Optimize based on profiles.
5. Validate conformance and performance.

Validation plan:

- Go tests.
- Shared reader conformance and mixed-directory tests.
- Benchmark/profiler artifacts.
- Read-only reviewer passes.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if Go reader workflow changes.
- Specs: update Go reader feature/performance status.
- End-user/operator docs: update Go reader docs.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: activate after Rust reader optimization.
- SOW-status.md: update when activated and closed.

Open-source reference evidence:

- systemd/libsystemd evidence comes through SOW-0043 unless new evidence is
  required.

Open decisions:

- Go mmap default/option decision requires measurement. Default should remain
  conservative unless benchmark and validation evidence justify changing it.

## Implications And Decisions

- 2026-05-28: user agreed Go reader follows Rust reader parity and
  optimization.
- 2026-05-29: Go reader default is mmap-backed live reading on Unix. `ReadAt`
  remains an explicit diagnostic/constrained-environment option because the
  benchmark measured `ReadAt` around 17k rows/s versus mmap around 1.07M
  rows/s for single-file payload scans.
- 2026-05-29: Go current-entry payload enumeration intentionally returns
  borrowed reader-owned slices for the zero-copy hot path, matching Rust's
  borrowed `&[u8]` model. `CollectEntryPayloads()`, `GetEntryPayload()`,
  `GetRaw()`, and `GetRawValues()` provide owned copies when callers need
  ownership.
- 2026-05-29: Go `SdJournalSeekCursor()` keeps the systemd/libsystemd
  no-existence-proof behavior already accepted for Python and Node.js. A
  reviewer found that current Rust still returns an error for a syntactically
  valid but nonexistent cursor; this is tracked as SOW-0055 so Rust can be
  realigned to systemd instead of regressing Go.

## Plan

1. Align Go reader API and internals with the finalized Rust/Python/Node.js
   current-entry payload model.
2. Add mmap-backed reader access and live/snapshot bounds.
3. Add Go reader benchmark coverage and compare mmap versus `ReadAt`.
4. Validate shared conformance, mixed-directory, live, and benchmark paths.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Read-only reviewers from the approved pool.

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

- Record mmap findings, performance gaps, reviewer findings, and audit failures
  in this SOW.

## Execution Log

### 2026-05-28

- Created from the agreed SOW restructuring.

### 2026-05-29

- Activated after the user requested Rust parity in Go reader code plus
  benchmark evidence.
- Implemented `ReaderOptions`, `ReaderAccessMode`, `ReaderBounds`,
  `OpenFileWithOptions`, `OpenDirectoryWithOptions`, and
  `OpenFilesWithOptions`.
- Added Unix read-only mmap access for the Go reader and a non-Unix whole-file
  buffer fallback behind the same API.
- Added active-file live refresh at tail/end and snapshot bounds for polling
  sessions that should not see appends during a scan.
- Added byte-preserving RAW field-name representation through `RawField`,
  `Entry.RawFields`, `Entry.RawFieldValues`, `Entry.Raw()`,
  `Entry.RawValues()`, `Reader.GetRaw()`, and `Reader.GetRawValues()`.
- Added current-entry payload APIs: `VisitEntryPayloads()`,
  `CollectEntryPayloads()`, `GetEntryPayload()`, `EntryDataRestart()`, and
  `EnumerateEntryPayload()`.
- Updated the libsystemd-compatible facade so `SdJournalGetData()` and
  restart/enumerate DATA paths use current-entry payload access instead of full
  entry materialization.
- Added ordered directory fast path for strict non-overlapping seqnum and
  realtime file ranges, with generic merge retained for overlapping files,
  filters, realtime seeks, and direction-change cases.
- Added `go/internal/testcmd/reader_core_bench` and integrated Go reader cases
  into `tests/benchmarks/run_reader_core_benchmarks.py`.
- Added regression tests for RAW byte field payload APIs, live versus snapshot
  bounds, and non-overlapping directory fast-path forward/backward ordering.
- Updated Go API docs, Go README, benchmark README, and product scope spec.
- First read-only review round found one useful documentation gap around
  borrowed mmap-backed slices. Added code comments and public docs while
  preserving the zero-copy hot path.
- Second read-only review round raised a Go/Rust `SdJournalSeekCursor()`
  divergence. Verified upstream systemd behavior in
  `systemd/systemd @ cf3156842209`,
  `src/libsystemd/sd-journal/sd-journal.c:1263-1363`: systemd stores the
  requested location and returns success without proving the cursor exists.
  Go behavior is correct for the libsystemd-compatible facade; Rust divergence
  is tracked in SOW-0055.

## Validation

Acceptance criteria evidence:

- Go reader behavior matches the accepted cross-language reader contract for
  mmap/live/snapshot access, current-entry payload scans, byte-preserving RAW
  field names, directory ordering, and libsystemd-compatible facade DATA
  enumeration.
- Mmap versus `ReadAt` was measured in
  `.local/benchmarks/reader-core-go-parity-final/20260529T112718Z`.
- Final benchmark median rows/s for compact 100k-row fixture:
  - Go single-file `sdk-payloads` live/mmap: 1,067,776.
  - Go single-file `facade-data` live/mmap: 1,091,091.
  - Go single-file `sdk-entry` live/mmap: 69,450.
  - Go single-file `sdk-payloads` live/`ReadAt`: 17,425.
  - Go open-files `sdk-payloads` live/mmap: 697,265.
  - Go open-files `facade-data` live/mmap: 606,245.
  - Rust single-file `sdk-payloads` live/windowed: 2,471,020.
  - Rust open-files `sdk-payloads` live/windowed: 1,578,940.
  - systemd single-file DATA enumeration: 564,858.
  - systemd open-files DATA enumeration: 531,905.
- Go mmap payload/facade paths are faster than stock systemd for the measured
  single-file and open-files DATA enumeration baselines. Rust remains faster,
  especially on the payload hot path; this is recorded as the current residual
  performance gap.

Tests or equivalent validation:

- `go test ./journal ./internal/testcmd/reader_core_bench` passed.
- `go test ./...` passed.
- `python -m py_compile tests/benchmarks/run_reader_core_benchmarks.py` passed.
- `PYTHONPATH=.local/python-deps:python python tests/interoperability/run_mixed_directory_matrix.py --readers go rust stock` passed 42/42.
- `PYTHONPATH=.local/python-deps:python python tests/interoperability/run_matrix.py --entries 200 --writers go rust --readers go rust stock` passed 32/32.
- `PYTHONPATH=.local/python-deps:python python tests/interoperability/run_live_matrix.py --entries 100 --features regular compact --writers go rust --readers go rust stock --poll-readers 1 --libsystemd-readers 1 --writer-delay-ms 1` passed 4/4.
- `python tests/benchmarks/run_reader_core_benchmarks.py --rows 100000 --directory-rows 100000 --repetitions 3 --warmups 1 --languages rust,go,systemd --out .local/benchmarks/reader-core-go-parity-final` passed checksum validation.
- `git diff --check` passed.

Real-use evidence:

- Go reader benchmark command builds and is exercised by the shared reader-core
  harness.
- Mixed-directory and live matrices exercised Go readers against repository
  writer output and stock reader paths.

Reviewer findings:

- Initial whole-SOW read-only review:
  - Minimax reported several findings; most were false positives after code
    verification. The useful outcome was extra scrutiny of mmap fallback,
    directory fast path, and `SdJournalSeekCursor()`.
  - Qwen reported the borrowed mmap-backed slice lifetime gap for
    `EnumerateEntryPayload()` / facade data enumeration. Disposition: accepted
    as a documentation/API contract gap, fixed with comments and docs while
    preserving zero-copy behavior.
  - GLM independently reported the same borrowed-slice documentation gap and
    verified the core reader paths. Disposition: fixed.
  - Kimi stalled without a final review and its verified reviewer PIDs were
    stopped.
- Second whole-SOW read-only review after fixes:
  - GLM reported one high-severity `SdJournalSeekCursor()` concern because Go
    now differs from current Rust. Disposition: rejected as a Go blocker after
    direct upstream systemd verification; created SOW-0055 to correct Rust
    parity with systemd.
  - GLM reported a medium residual risk that the non-overlapping directory fast
    path is computed at open time. Disposition: accepted as a residual
    low-practical-risk invariant for standard systemd/SDK rotation, where only
    the last active file can grow; generic merge remains in use when the
    initial ranges overlap, filters are active, realtime seek is pending, or
    direction changes from a current entry.
  - Qwen repeatedly ran Go tests from the repository root where no Go module
    exists and exited without a usable final review. Local `go test ./...` from
    `go/` passed.
  - Minimax became silent for several minutes without final output and its
    verified reviewer PIDs were stopped.

Same-failure scan:

- Searched and verified current-entry payload ownership paths:
  `VisitEntryPayloads()` and `EnumerateEntryPayload()` are documented borrowed
  paths; `CollectEntryPayloads()`, `GetEntryPayload()`, `GetRaw()`,
  `GetRawValues()`, `GetEntry()`, export, and JSON paths clone/own data.
- Searched cursor behavior across Go, Rust, Python, Node.js, and upstream
  systemd. Go/Python/Node match systemd no-existence-proof behavior; current
  Rust exact-search behavior is tracked by SOW-0055.
- Added explicit fast-path regression coverage for non-overlapping directory
  ordering in both directions.

Sensitive data gate:

- Durable artifacts contain generated-fixture paths, benchmark rates, and
  public upstream source references only. No raw secrets, credentials, SNMP
  communities, bearer tokens, customer names, personal data, non-private
  customer-identifying IPs, private endpoints, or proprietary incident details
  were recorded.

Artifact maintenance gate:

- AGENTS.md: no workflow or project-wide guardrail change.
- Runtime project skills: no workflow change requiring skill update.
- Specs: updated `.agents/sow/specs/product-scope.md`.
- End-user/operator docs: updated `go/README.md`, `go/API.md`, and
  `tests/benchmarks/README.md`.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: activated SOW-0045; created follow-up SOW-0055 for Rust cursor
  parity discovered during review.
- SOW-status.md: updated for SOW-0045 activation and SOW-0055 pending tracking.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` with Go mmap default and
  current-entry payload hot-path contract.

Project skills update:

- No project skill update needed; no new reusable workflow rule was introduced.

End-user/operator docs update:

- Updated `go/README.md`, `go/API.md`, and `tests/benchmarks/README.md`.

End-user/operator skills update:

- No output/reference skill affected.

Lessons:

- Go `ReadAt` reader performance is not production-viable for Netdata-style
  hot paths; mmap must be the Unix default.
- Go can match Rust's zero-copy payload model only by documenting borrowed slice
  lifetime explicitly.
- Reviewers may flag current Rust as the reference even when later SOWs already
  verified different upstream systemd behavior; resolve these with source
  evidence, then track true Rust drift separately.

Follow-up mapping:

- SOW-0055 tracks the discovered Rust `seek_cursor()` divergence from upstream
  systemd no-existence-proof behavior.
- The remaining Go-vs-Rust throughput gap is recorded as a measured residual:
  Go beats systemd in the measured payload/facade paths but remains below Rust.
  No separate follow-up is opened until the user prioritizes deeper Go reader
  micro-optimization beyond the current systemd-beating baseline.

## Outcome

Completed. Go reader behavior and benchmark coverage are now aligned with the
accepted cross-language reader contract for this phase. The measured Go mmap
payload/facade paths beat stock systemd on the compact 100k-row reader-core
baseline, while Rust remains faster and is recorded as the current upper-bound
implementation.

## Lessons Extracted

- The fastest Go reader path must avoid full entry materialization and use mmap
  plus current-entry payload iteration.
- Go's type system cannot express Rust borrowed-slice lifetimes, so public docs
  and comments must clearly distinguish borrowed zero-copy APIs from owned-copy
  APIs.
- Rust remains the performance reference, but direct upstream systemd evidence
  overrides Rust when facade semantics differ from libsystemd.

## Followup

- SOW-0055 tracks Rust `seek_cursor()` alignment with upstream systemd
  no-existence-proof behavior.

## Regression Log

None yet.
