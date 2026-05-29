# SOW-0056 - Go Reader Hot-Path Optimization Phase 2

## Status

Status: completed

Sub-state: implementation, validation, read-only reviewer pass, and SOW closure
are complete.

## Requirements

### Purpose

Improve Go reader hot-path performance for Netdata-fit reader use cases without
weakening the systemd journal compatibility contract established by the Rust
reference and SOW-0045.

### User Request

After SOW-0045 measured Go reader performance below Rust, the user asked whether
Go can be improved and then approved doing the focused Go optimization work.

### Assistant Understanding

Facts:

- SOW-0045 measured Go single-file `sdk-payloads` live/mmap at about 1.07M
  rows/s and Rust single-file `sdk-payloads` live/windowed at about 2.47M
  rows/s on the same compact, uncompressed, FSS-off 100k-row fixture.
- SOW-0045 measured Go open-files `sdk-payloads` live/mmap at about 697k rows/s
  and Rust open-files `sdk-payloads` live/windowed at about 1.58M rows/s.
- Comparable Go and Rust benchmark cases produced the same row counts, field
  counts, byte counts, and checksums.
- Go is already faster than stock systemd for the measured DATA enumeration
  cases, but Rust remains materially faster.

Inferences:

- The gap is likely in the Go reader hot path rather than compatibility work,
  because comparable counters/checksums match.
- Optimization should target the production SDK reader path first, then the
  benchmark harness only where it adds measurement clarity.
- A change is worth keeping only if profiling and benchmark data show a clear
  benefit without broadening API ambiguity or compatibility risk.

Unknowns:

- Exact CPU/allocation attribution before a new Go profile is collected.
- Whether a DATA metadata cache helps or hurts Go after the Rust cache removal
  showed no benefit.

### Acceptance Criteria

- Go reader single-file and open-files payload/facade hot paths are profiled
  before or during optimization, with evidence recorded.
- Kept optimizations preserve identical benchmark counters/checksums against the
  existing reader-core fixtures.
- Kept optimizations improve at least one production-relevant Go reader hot path
  materially, or the SOW records evidence-backed no-go results and leaves no
  speculative code behind.
- Go reader tests and shared interoperability/live reader checks pass.
- Specs/docs are updated if the public Go reader API or performance contract
  changes.
- External reviewers review the whole SOW batch read-only after local
  implementation and validation.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`
- `.agents/sow/done/SOW-0045-20260528-go-reader-alignment-optimization.md`
- `.agents/sow/specs/product-scope.md`
- `tests/benchmarks/run_reader_core_benchmarks.py`
- `go/internal/testcmd/reader_core_bench/main.go`
- `go/journal/reader.go`
- `go/journal/mmap_unix.go`

Current state:

- Go reader default access mode is Unix mmap-backed live reading.
- Go reader exposes current-entry payload APIs and facade DATA fast paths.
- `go/internal/testcmd/reader_core_bench/main.go` opens readers through an
  interface that includes `VisitEntryPayloads(func([]byte) error) error`,
  which adds benchmark dispatch/callback overhead that may or may not represent
  real consumers.
- `go/journal/reader.go` reads every DATA object by parsing the DATA header via
  one slice and then reading the payload through a second slice.
- Go reader cache state currently stores current-entry DATA offsets and current
  entry header only; it does not cache DATA object metadata by offset.

Risks:

- Hot-path optimizations can accidentally create stale borrowed slices after a
  live remap or change facade pointer lifetime semantics.
- Public API additions can create maintenance burden if they overlap confusingly
  with existing facade and SDK payload APIs.
- Caches can reduce performance if hit accounting is good but maintenance cost
  exceeds direct mmap lookup cost.
- Directory/open-files optimizations can break ordering if they bypass merge
  semantics.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The Go reader is correct and mmap-backed after SOW-0045, but its hot path is
  still slower than Rust for equivalent payload enumeration. Current code
  evidence points to per-entry callback/interface dispatch in the benchmark path
  and extra DATA header/payload access work in `readDataPayload()`. This is a
  working model to verify or reject with profiling before keeping invasive
  changes.

Evidence reviewed:

- SOW-0045 benchmark medians and identical counter/checksum evidence.
- `go/internal/testcmd/reader_core_bench/main.go` uses interface dispatch in
  `openSDKReader()` and callback enumeration in `readSDK()`.
- `go/journal/reader.go` implements current-entry offset caching in
  `currentEntryDataOffsets()`, payload visitation in `VisitEntryPayloads()`,
  and DATA access in `readDataPayload()`.
- `go/journal/mmap_unix.go` returns mapped byte slices from `bytesAt()`.

Affected contracts and surfaces:

- Go reader SDK API.
- Go libsystemd-compatible facade DATA enumeration.
- Go reader benchmark command.
- Shared reader benchmark harness.
- Product-scope performance notes if public contracts change.

Existing patterns to reuse:

- Rust SOW-0052 model: zero-copy uncompressed DATA payloads and explicit
  reusable fallback only where compression requires decoding.
- Go SOW-0045 model: borrowed payload slices are valid only until the next
  reader method call, refresh, or close.
- Existing reader-core benchmark counters/checksum are the regression oracle for
  hot-path changes.

Risk and blast radius:

- Medium to high for Go reader consumers. Reader correctness and borrowed-slice
  lifetime are core contracts. Changes must not affect writer compatibility,
  journal file format, or cross-language data semantics.

Sensitive data handling plan:

- Use generated benchmark fixtures and repository test fixtures only. Do not
  record real customer logs, SNMP communities, credentials, bearer tokens,
  personal data, private endpoints, or proprietary incident details.

Implementation plan:

1. Add profiling support for targeted Go reader benchmark cases using
   repository-local `.local/` artifacts.
2. Add measurement-only benchmark modes if needed to separate benchmark
   interface/callback cost from SDK reader cost.
3. Optimize the Go reader DATA hot path by avoiding redundant header/payload
   access where mmap already exposes the full object safely.
4. Prototype higher-throughput current-entry payload iteration with
   caller-reused state only if profiling shows callback/allocation overhead is
   material.
5. Prototype DATA metadata caching only as a measured experiment; keep it only
   if it improves production-relevant cases.
6. Update Go tests, docs, specs, and benchmark docs if public API or durable
   behavior changes.

Validation plan:

- `go test ./journal ./internal/testcmd/reader_core_bench`
- `go test ./...`
- Reader benchmark before/after with Rust, Go, and systemd for compact,
  uncompressed, FSS-off single-file and open-files cases.
- Go profile artifacts under `.local/benchmarks/`.
- Shared interoperability checks that exercise Go reader behavior:
  mixed-directory matrix, cross-language matrix, and live matrix for Go/Rust
  affected readers/writers.
- `git diff --check`
- `.agents/sow/audit.sh`
- Whole-SOW read-only external reviewer round after local validation.

Artifact impact plan:

- AGENTS.md: no update expected; workflow rules are unchanged.
- Runtime project skills: no update expected unless a durable benchmark/profile
  workflow changes.
- Specs: update `.agents/sow/specs/product-scope.md` only if Go reader public
  API or performance contract changes.
- End-user/operator docs: update Go README/API docs if public API changes.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: close SOW-0056 as completed with implementation, validation,
  review evidence, and commit in one chunk.
- SOW-status.md: update while active and again when closed.

Open-source reference evidence:

- No new external open-source repository inspection is required for this SOW.
  The compatibility authority remains systemd v260.1 evidence already recorded
  in the product-scope spec and reader parity SOWs. This SOW optimizes local Go
  implementation internals without changing the format contract.

Open decisions:

- None blocking. The user approved the Go reader optimization work after the
  proposed focused SOW. Any new product-level API choice beyond additive,
  idiomatic Go hot-path helpers will be brought back for a numbered decision
  before implementation.

## Implications And Decisions

1. 2026-05-29 Go reader optimization phase
   - Decision: proceed with a focused Go reader hot-path optimization SOW after
     SOW-0045.
   - Implication: implementation may add measured internal optimizations and
     additive Go reader helpers, but must not weaken reader compatibility or
     leave speculative optimization code without measured benefit.

## Plan

1. Establish Go reader profile/baseline artifacts for the current SOW.
2. Implement the lowest-risk DATA access hot-path improvement.
3. Measure and keep/revert each optimization based on benchmark evidence.
4. Run compatibility/regression tests.
5. Run whole-SOW read-only reviews and resolve findings.
6. Close, commit, and push the verified chunk.

## Delegation Plan

Implementer:

- Local implementation by the project manager. No external implementer agents.

Reviewers:

- Read-only reviewers from the approved pool after local implementation and
  validation: `llm-netdata-cloud/minimax-m2.7-coder`,
  `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/qwen3.6-plus`, and
  `llm-netdata-cloud/glm-5.1`. Skip `mimo` because the user reported quota
  exhaustion.

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

- Record profile/benchmark no-go results and remove speculative code if an
  optimization does not help. Record reviewer findings and dispositions before
  closure. If audit fails, repair in this repository and rerun before closing.

## Execution Log

### 2026-05-29

- Created active SOW from user approval to improve Go reader performance.
- Added Go reader-core benchmark helper profiling flags:
  `--cpuprofile`, `--memprofile`, and `--loops`. These are measurement-only
  flags; the shared Python harness keeps `--loops=1` and preserves existing
  checksum/counter behavior.
- Profiled Go single-file `sdk-payloads` and `facade-data` mmap cases using
  `.local/benchmarks/go-reader-phase2-profiles/` artifacts.
- Kept measured Go reader hot-path optimizations:
  - parse only the common 16-byte DATA object header in `readDataPayload()`
    before slicing the payload;
  - reuse current-entry DATA offset backing storage across entry transitions
    instead of dropping it on every invalidation;
  - return ENTRY headers by value to avoid per-entry heap pressure;
  - cache immutable per-file compact/regular layout sizes and refresh them
    defensively when live header state is refreshed;
  - specialize regular and compact ENTRY/OFFSET_ARRAY loops outside the item
    loop.
- Added `TestReaderPayloadEnumerationReusesOffsetsAcrossEntries`, covering
  regular and compact journal files, read-at and mmap readers, entries with
  more -> fewer -> more fields, visitor enumeration, libsystemd-style restart
  enumeration, and repeated restart after exhaustion.
- Updated `tests/benchmarks/README.md` with the Go reader profiling helper
  flags and `.local/benchmarks/` profile-output convention.
- Local post-fix benchmark:
  `.local/benchmarks/reader-core-go-phase2-final/20260529T125459Z/summary.json`.
  Key medians:
  - Go single-file `sdk-payloads` live/mmap: 2,744,437 rows/s.
  - Go single-file `facade-data` live/mmap: 2,334,678 rows/s.
  - Go open-files `sdk-payloads` live/mmap: 2,398,712 rows/s.
  - Go open-files `facade-data` live/mmap: 1,990,810 rows/s.
  - Rust single-file `sdk-payloads` live/windowed: 2,076,382 rows/s.
  - Rust open-files `sdk-payloads` live/windowed: 1,857,504 rows/s.
  - Stock systemd single-file DATA: 634,172 rows/s.
  - Stock systemd open-files DATA: 627,911 rows/s.
- Read-only reviewers ran against the whole SOW batch. One reviewer raised a
  false critical concern that Go `binary.LittleEndian.Uint64()` requires an
  exactly 8-byte slice; local validation with regular journal files already
  disproved a panic. The regular ENTRY decode was still changed to slice the
  exact 8-byte DATA offset field for readability and to prevent future
  confusion.
- Re-ran final post-review validation after the exact-width decode cleanup.

## Validation

Acceptance criteria evidence:

- Profiling evidence exists under
  `.local/benchmarks/go-reader-phase2-profiles/`.
- Final benchmark evidence exists under
  `.local/benchmarks/reader-core-go-phase2-final/20260529T125459Z/`.
- Shared benchmark counters/checksums matched across compared cases; the
  benchmark runner completed without mismatch errors.
- Material improvements versus SOW-0045 baseline:
  - single-file `sdk-payloads` live/mmap improved from about 1.07M rows/s to
    2.74M rows/s;
  - single-file `facade-data` live/mmap improved from about 1.09M rows/s to
    2.33M rows/s;
  - open-files `sdk-payloads` live/mmap improved from about 697k rows/s to
    2.40M rows/s.

Tests or equivalent validation:

- `go test ./journal ./internal/testcmd/reader_core_bench` passed.
- `go test ./...` from `go/` passed.
- `git diff --check` passed.
- `.agents/sow/audit.sh` passed before closure.
- `PYTHONPATH=.local/python-deps:python python tests/interoperability/run_mixed_directory_matrix.py --readers go rust stock`
  passed 42/42 on systemd 260 (260.1-2-manjaro).
- `PYTHONPATH=.local/python-deps:python python tests/interoperability/run_matrix.py --entries 200 --writers go rust --readers go rust stock`
  passed 32/32 on systemd 260 (260.1-2-manjaro).
- `PYTHONPATH=.local/python-deps:python python tests/interoperability/run_live_matrix.py --entries 100 --features regular compact --writers go rust --readers go rust stock --poll-readers 1 --libsystemd-readers 1 --writer-delay-ms 1`
  passed 4/4 on systemd 260 (260.1-2-manjaro).

Real-use evidence:

- The live matrix confirmed stock `journalctl --file`, stock libsystemd,
  Rust reader, and Go reader final reads while Go/Rust writers appended regular
  and compact files.
- The mixed-directory matrix confirmed the Go reader/journalctl path still
  handles mixed regular/compact, compressed, sealed/unsealed, and `.journal.zst`
  directory cases after the hot-path changes.

Reviewer findings:

- Minimax reviewed the whole SOW batch and found no correctness bugs; verdict:
  production-grade.
- GLM reviewed the whole SOW batch and found no blocking issues; it noted that
  `Outcome`, `Lessons Extracted`, and `Followup` needed final closure content.
  Disposition: filled before closing.
- Kimi reviewed the whole SOW batch, independently reran Go tests,
  interoperability checks, and `.agents/sow/audit.sh`, and found no blocking
  issues. It requested that the SOW explicitly record the audit pass.
  Disposition: recorded.
- Qwen reviewed the whole SOW batch and flagged the regular ENTRY item slice as
  a critical panic risk. The panic claim was false because Go's
  `binary.LittleEndian.Uint64()` reads `b[0:8]` and requires at least 8 bytes,
  not exactly 8; additionally, the new regular-file test and `go test ./...`
  passed. Disposition: changed the code to slice the exact 8-byte DATA offset
  field anyway, then reran Go tests, `git diff --check`, and all three
  interoperability matrices.

Same-failure scan:

- `rg -n "entryDataOffsets|configureLayout|readDataPayload|dataPayloadOffset|parseEntryHeader|cpuprofile|memprofile|loops" go/journal go/internal/testcmd/reader_core_bench tests/benchmarks/README.md`
  reviewed all touched hot-path/state/profile surfaces.
- Manual diff review checked that benchmark helper changes are measurement-only
  and that reader changes stay inside Go reader internals plus tests.

Sensitive data gate:

- Only generated benchmark fixtures, generated interoperability fixtures, and
  repository test fixtures were used. No customer logs, SNMP communities,
  credentials, bearer tokens, private endpoints, personal data, or proprietary
  incident details were recorded in durable artifacts.

Artifact maintenance gate:

- AGENTS.md: no update needed; project workflow and guardrails did not change.
- Runtime project skills: no update needed; no durable HOW-to workflow changed.
- Specs: no update needed; public API and journal compatibility contracts did
  not change.
- End-user/operator docs: `tests/benchmarks/README.md` updated for the Go
  reader profiling helper flags.
- End-user/operator skills: no output/reference skills are affected.
- SOW lifecycle: SOW is marked `completed` and moved to `done/` together with
  implementation and artifact updates.
- SOW-status.md: updated to move SOW-0056 from current to recently completed.

Specs update:

- Not needed. This SOW changes Go reader implementation internals and benchmark
  helper diagnostics, not product/file/API contracts.

Project skills update:

- Not needed. Existing project orchestration and journal compatibility skills
  already cover this workflow.

End-user/operator docs update:

- `tests/benchmarks/README.md` updated for `--cpuprofile`, `--memprofile`, and
  `--loops` on the Go reader helper.

End-user/operator skills update:

- Not needed; no output/reference skill changed.

Lessons:

- The Go reader hot path had avoidable allocation/header parsing/layout branch
  overhead even after mmap parity. Profiling per focused helper run made those
  costs visible without changing the shared harness semantics.
- The read-at diagnostic mode is much slower than mmap and should remain a
  diagnostic/portability comparison, not the production Unix baseline.

Follow-up mapping:

- No new follow-up SOW is required from this phase. Existing SOW-0055 continues
  to track the unrelated Rust cursor-seek parity follow-up from SOW-0045.

## Outcome

Completed.

Go reader hot-path performance was materially improved without changing public
API or compatibility semantics. The final benchmark evidence recorded for the
compact, uncompressed, FSS-off reader-core harness shows Go live/mmap
single-file `sdk-payloads` at 2.74M rows/s and Go live/mmap open-files
`sdk-payloads` at 2.40M rows/s, both above the same-run Rust windowed payload
paths and more than 3.8x stock systemd DATA enumeration in this harness.

The implementation remains compatibility-verified by Go package tests,
cross-language matrix, mixed-directory matrix, live regular/compact matrix, and
read-only reviewer pass.

## Lessons Extracted

- In Go, avoiding allocation/escape pressure in small hot-path helpers can be
  as important as avoiding large copies. Returning `entryHeader` by value was
  both simpler and faster than returning a pointer that escapes.
- DATA payload reads only need the common object header fields. Parsing the full
  DATA header in the payload hot path was unnecessary work.
- Preserving slice backing storage is safe when the cache key and visible
  length are reset together. Tests should cover more -> fewer -> more field
  transitions because that is the failure shape stale-capacity bugs usually
  take.
- Reviewer findings can be factually wrong. Treat them as evidence to verify,
  not instructions to apply mechanically.

## Followup

No new follow-up SOW is required from this work. Existing SOW-0055 remains the
only known related follow-up and tracks the unrelated Rust cursor-seek parity
issue discovered during SOW-0045.
