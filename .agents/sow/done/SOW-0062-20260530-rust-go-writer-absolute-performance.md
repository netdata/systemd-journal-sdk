# SOW-0062 - Rust And Go Writer Absolute Performance

## Status

Status: completed

Sub-state: completed Rust-first, Go-second writer profiling and optimization pass.

## Requirements

### Purpose

Make the Rust and Go journal writers exceptionally fast for Netdata ingestion
hot paths. This directly affects Netdata NetFlow, OTEL logs ingestion, and SNMP
traps ingestion, so wasted CPU here scales globally across a large installed
base.

### User Request

The user stated that Rust and Go writer performance is exceptionally important
because Netdata NetFlow, OTEL ingestion, and SNMP traps depend on these
writers. Existing performance above systemd is not enough. The requested order
is:

1. Profile and optimize Rust first.
2. Profile and optimize Go second.
3. Do everything that can be done to make ingestion performance exceptional.

### Assistant Understanding

Facts:

- The current writer-core benchmark is the right local baseline for append-loop
  throughput because it pre-materializes deterministic rows before timing and
  excludes writer creation, final close/sync, journal verification, and row
  generation from the append timer.
- The relevant production-style baseline is compact format, no compression, FSS
  off, raw-payload API, online state, 32 fields per row, fixed max file size,
  and a latency-tolerant `live_publish_every_entries=64` mode.
- The latest inspected artifact for that surface reports Rust at about 53k
  rows/s and Go at about 59k rows/s for 100k rows on one cold repetition.
- The same artifact has important caveats: powersave CPU governor, one
  repetition, no warmup, and no kept journals for byte-comparison evidence.
- The first confirmed Rust-vs-Go differences are:
  - Rust Jenkins lookup3 hashing reads words through a byte-at-a-time
    `PartReader` even for single contiguous payloads.
  - Rust re-walks DATA hash chains on dedup hits while Go returns immediately.

Inferences:

- Rust should be able to close or exceed the Go writer gap if the confirmed
  byte-safe hot-path differences are fixed.
- Go may still have measurable optimization opportunities after Rust is fixed,
  especially around allocations, header/data access helpers, publication
  metadata writes, and benchmark-profiled hot paths.
- Any optimization that changes journal layout, hash semantics, live-reader
  publication semantics, field-name policy, compression behavior, compact
  layout, or FSS behavior is not acceptable unless separately proven and
  explicitly recorded.

Unknowns:

- The exact percentage cost of each Rust hotspot must be measured with a fresh
  profile before and after changes.
- The exact Go bottlenecks after Rust catches up are unknown until Go is
  profiled on the same workload.

### Acceptance Criteria

- Establish a clean Rust and Go writer-core baseline with multiple repetitions,
  warmups, explicit environment metadata, and kept journals when needed for
  identity/verification evidence.
- Collect Rust writer profiles before implementation and after every material
  optimization batch.
- Implement only measured or mechanically proven Rust writer optimizations that
  preserve journal compatibility and benchmark semantics.
- Validate Rust writer compatibility after changes with unit tests,
  writer-core verification, and applicable interoperability/byte-identity
  checks.
- Repeat profiling and optimization for the Go writer after the Rust pass.
- Keep benchmark comparisons standardized through
  `tests/benchmarks/report_benchmarks.py`; do not hand-compose final benchmark
  tables when report artifacts exist.
- Do not make changes outside this repository except normal tool outputs under
  `.local/` and temporary files under `/tmp`.
- Close only after local validation, whole-SOW read-only review, SOW audit, and
  a committed rollback point.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`
- `tests/benchmarks/README.md`
- `tests/benchmarks/run_writer_core_benchmarks.py`
- `.local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-64-mmap-windowed-20260528T200139204303Z/report.json`
- `rust/src/internal/testcmd/writer_core_bench/src/main.rs`
- `rust/src/crates/journal-core/src/file/hash.rs`
- `rust/src/crates/journal-core/src/file/writer.rs`
- `rust/src/crates/journal-core/src/file/file.rs`
- `rust/src/crates/journal-core/src/file/mmap.rs`
- `rust/src/crates/journal-core/src/file/guarded_cell.rs`
- `go/internal/testcmd/writer_core_bench/main.go`
- `go/journal/hash.go`
- `go/journal/writer.go`
- `go/journal/mmap_unix.go`

Current state:

- The 2026-05-28 writer-core artifact reports:
  - Go: `59,423.7` append rows/s.
  - Rust: `53,453.6` append rows/s.
  - systemd: `35,634.1` append rows/s.
  - Node.js: `1,015.2` append rows/s.
  - Python: `983.4` append rows/s.
- That artifact is valid for directional comparison but not sufficient as a
  final optimization baseline because it used one repetition, no warmup,
  powersave governor, and `keep_journals=false`.
- Rust writer-core rows contain 32 fields: 4 fixed fields, 12 low-cardinality
  fields, 8 medium-cardinality fields, and 8 high-cardinality fields.
- For 100k rows, the dataset has about 3.2M total payloads, 816,580 unique
  payloads, and about 2.38M dedup hits.
- Rust single contiguous Jenkins hashing currently calls
  `jenkins_hash64_from_parts([data], data.len())`.
- Rust `PartReader::read_u32_le()` calls `next_byte()` four times per 32-bit
  word.
- Go Jenkins hashing uses direct `binary.LittleEndian.Uint32()` word loads.
- Rust `add_data()` calls `update_data_hash_chain_depth()` on dedup hits and
  on new DATA object insertion.
- Go `addData()` returns immediately on dedup hit and updates hash chain depth
  only during new hash item insertion.
- Rust object access goes through guarded/windowed/typed object machinery.
  This is plausible overhead but must be profiled before changing.
- Go writer maps the whole arena and accesses byte slices with a simple
  bounds-checked `bytesAt()` path on Unix.

Risks:

- Optimizing hashes can silently break file lookup compatibility if Jenkins or
  SipHash output changes even for one tail-length case.
- Removing a chain-depth update is safe only for dedup hits because hits do not
  extend append-only chains. The change must not skip updates after new hash
  item insertion.
- Flattening Rust object access has higher risk because object validation,
  compact layout, guard lifetime, and live mmap behavior are intertwined.
- Benchmarking can mislead if CPU governor, repetitions, warmups, live cadence,
  API mode, max-size, final state, or journal retention differ between runs.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Rust trails Go on the same writer-core append workload despite both being
  faster than systemd. Evidence points to two concrete Rust-only costs that Go
  already avoids: byte-at-a-time Jenkins word loading on contiguous payloads and
  redundant hash-chain-depth walks on DATA dedup hits. A third potential cost,
  guarded/windowed object access, is plausible but must be measured.

Evidence reviewed:

- Prior writer-core artifact under `.local/benchmarks/writer-core/...`.
- Writer benchmark harness documentation and runner implementation.
- Rust and Go writer-core deterministic row generators.
- Rust and Go hash implementations.
- Rust and Go DATA insertion/dedup paths.
- Rust guarded/windowed object access code and Go mapped arena access code.

Affected contracts and surfaces:

- Rust writer public APIs and internal writer hot paths.
- Go writer public APIs and internal writer hot paths.
- Writer-core benchmark artifacts and reports.
- Compatibility with stock `journalctl --verify --file`, stock readers, and
  repository readers.
- Deterministic writer layout and byte-identity evidence for applicable
  uncompressed compact/regular slices.

Existing patterns to reuse:

- `tests/benchmarks/run_writer_core_benchmarks.py`.
- `tests/benchmarks/report_benchmarks.py`.
- Rust hash vector tests in `hash.rs`.
- Go hash tests and writer unit tests.
- Existing writer interoperability matrices.
- Existing `.local/` cache/output conventions.

Risk and blast radius:

- Medium to high because writer hot paths determine file validity and production
  ingestion performance.
- Hash and chain-depth changes are relatively low risk when protected by vector
  tests, unit tests, byte-identity checks, stock verification, and matrix runs.
- Object-access flattening is higher risk and must only be attempted after
  profiling shows it remains material.
- No production systems or host live journals will be touched.

Sensitive data handling plan:

- Use deterministic synthetic benchmark rows only.
- Do not read or write live host journals.
- Do not record real logs, SNMP communities, customer names, customer
  identifiers, credentials, bearer tokens, private endpoints, personal data, or
  proprietary incident details.

Implementation plan:

1. Establish a fresh Rust/Go writer-core baseline and collect Rust profiles.
2. Implement the contiguous Jenkins lookup3 fast path in Rust, protected by
   existing and added vector/tail tests.
3. Remove Rust DATA dedup-hit hash-chain-depth re-walks while keeping new-insert
   depth updates.
4. Rebenchmark and profile Rust; only then decide whether guarded/windowed
   object-access changes are justified.
5. Profile Go on the same benchmark surface after Rust is optimized.
6. Implement measured Go optimizations that do not reduce compatibility.
7. Run final validation, reporter comparisons, and whole-SOW read-only review.

Validation plan:

- Rust unit tests for `journal-core` and writer-related crates.
- Go unit tests for `go/journal`.
- Writer-core benchmark baseline and after-runs with Rust and Go, multiple
  repetitions, warmups, compact/no-compression/FSS-off/raw-payload/online,
  max-size 128 MiB, `live_publish_every_entries=64`.
- `tests/benchmarks/report_benchmarks.py` before/after report.
- `journalctl --verify --file` through the writer-core harness.
- Applicable byte-identity and interoperability checks after behavior changes.
- `.agents/sow/audit.sh` and `git diff --check`.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if this exposes a durable performance
  workflow rule missing from current skills.
- Specs: update only if public performance-related contracts or writer options
  change.
- End-user/operator docs: update only if public writer behavior/options change.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: active child SOW under SOW-0009.
- SOW-status.md: update to list this active SOW.

Open-source reference evidence:

- No new external open-source repositories were checked for this SOW yet. The
  current work compares local Rust/Go implementation behavior against existing
  systemd-compatible tests and benchmark artifacts. If systemd source is checked
  later, record durable upstream evidence instead of workstation paths.

Open decisions:

- Resolved by user: prioritize Rust writer profiling/optimization first and Go
  writer profiling/optimization second.
- Resolved by user: being faster than systemd is insufficient; optimize for the
  fastest compatible Rust and Go writers possible.

## Implications And Decisions

1. 2026-05-30 writer performance priority
   - Decision: Rust and Go writer hot-path performance is a primary production
     requirement for Netdata ingestion.
   - Implication: optimization work is justified even when current writers are
     already faster than systemd.
   - Risk: performance changes must not weaken journal compatibility, live
     reader behavior, byte identity where required, or writer robustness.

2. 2026-05-30 optimization order
   - Decision: profile and optimize Rust first, then Go.
   - Implication: use Go as an internal implementation comparison while making
     Rust as fast as possible, then improve Go from its own measured profile.
   - Risk: do not make Go worse for Rust parity; port only proven wins in the
     appropriate direction.

## Plan

1. Fresh baseline and profile
   - Scope: Rust and Go writer-core benchmark plus Rust profile.
   - Risk: benchmark noise; mitigate with repetitions, warmups, and recorded
     environment.

2. Rust low-risk hot-path fixes
   - Scope: contiguous Jenkins word-at-a-time path and dedup-hit chain-depth
     removal.
   - Risk: hash or metadata regression; mitigate with vector tests,
     writer-core verification, and identity/matrix checks.

3. Rust residual profiling
   - Scope: decide whether guarded/windowed object-access flattening remains
     material.
   - Risk: higher implementation blast radius; only proceed with measured
     evidence.

4. Go profiling and optimization
   - Scope: profile Go after Rust is optimized and fix measured Go writer
     bottlenecks.
   - Risk: avoid speculative changes that make Go less maintainable without
     measurable wins.

5. Final verification and close
   - Scope: standardized benchmark report, compatibility validation, whole-SOW
     review, SOW audit, commit, and push.

## Delegation Plan

Implementer:

- Local implementation. No external implementer agents.

Reviewers:

- Read-only reviewers from the approved pool after the complete SOW is locally
  implemented and validated.

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

- If a benchmark/profile tool is unavailable, record the failure and use the
  strongest available local evidence without changing system configuration.
- If any optimization breaks compatibility, revert the specific optimization
  with a targeted patch and record the rejected approach.
- If reviewer findings identify blocking compatibility or performance issues,
  fix them and rerun the relevant benchmark/validation batch.

## Execution Log

### 2026-05-30

- Created this active child SOW under SOW-0009 after the user elevated Rust and
  Go writer performance to a production-critical priority.
- Established a fresh Rust/Go writer-core baseline:
  `.local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-64-mmap-windowed-20260529T212457592764Z/report.json`.
  - Rust median append throughput: `54,448.8241281762` rows/s.
  - Go median append throughput: `61,304.621379301934` rows/s.
- Collected Rust and Go perf profiles under
  `.local/benchmarks/profiles/sow-0062/`.
- Implemented accepted Rust writer optimizations:
  - `rust/src/crates/journal-core/src/file/hash.rs`: contiguous Jenkins
    lookup3 fast path using word-at-a-time little-endian reads, protected by
    split/tail tests.
  - `rust/src/crates/journal-core/src/file/writer.rs` and
    `rust/src/crates/jf/journal_file/src/writer.rs`: removed redundant
    DATA hash-chain-depth re-walks on dedup hits while preserving new-insert
    depth updates.
  - `rust/src/crates/journal-core/src/file/writer.rs`: compact DATA objects
    now use their stored tail entry-array metadata for appends, with fallback
    to the full chain walk when tail metadata is absent or inconsistent.
  - `rust/src/crates/journal-core/src/file/file.rs`: DATA lookup now checks
    the fixed DATA header first, skips payload reads on hash mismatch, and
    uses direct mmap payload slices for uncompressed matches while preserving
    compressed-DATA decompression behavior.
- Implemented accepted Go writer optimizations:
  - `go/journal/mmap_unix.go`, `go/journal/mmap_other.go`, and
    `go/journal/writer.go`: new-object buffers and small integer/header reads
    use direct mmap slices when available, avoiding extra heap buffers and
    copies on Unix.
  - `go/journal/writer.go`: DATA lookup now checks the DATA header hash before
    reading payload bytes, and compressed payload decoding remains isolated to
    matching-hash candidates.
  - `go/journal/writer.go`: compact DATA objects now use stored tail
    entry-array metadata for appends, with fallback to the full chain walk when
    tail metadata is absent or inconsistent.
- Rejected and reverted measured regressions:
  - Rust direct DATA lookup rewrite variant before the final header-first path:
    lower median than the accepted batch.
  - Rust direct hash-chain tail setter:
    `.local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-64-mmap-windowed-20260529T215746654511Z/report.json`
    reported `86,427.38906361406` rows/s versus the prior `89,147.20468188568`
    rows/s Rust-only run.
  - Go parser-inline pass:
    `.local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-64-20260529T220005643123Z/report.json`
    reported `81,401.90961342136` rows/s versus the prior accepted Go-only
    `84,536.55536997452` rows/s run.
- Final benchmark report:
  `.local/benchmarks/profiles/sow-0062/final-report.md`.
  - Final 10-measurement/2-warmup run:
    `.local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-64-mmap-windowed-20260529T220258446468Z/report.json`.
  - Rust median append throughput: `86,576.92488170974` rows/s, `+59.0%`
    versus fresh baseline.
  - Go median append throughput: `78,868.42568045705` rows/s, `+28.7%`
    versus fresh baseline.
  - Benchmark caveat: workstation CPU governor metadata still reports
    `powersave` in these benchmark artifacts, so absolute numbers are
    conservative and should not be treated as a lab-stable ceiling.

## Validation

Acceptance criteria evidence:

- Fresh baseline, profiles, accepted/rejected optimization evidence, and final
  standardized benchmark report are recorded in the execution log above.
- Rust and Go writer-core final run passed stock `journalctl --verify --file`
  through the benchmark harness for every kept generated journal.
- The final benchmark surface matches the production-oriented baseline:
  compact format, no compression, FSS off, raw-payload API, online final state,
  fixed 128 MiB max file size, and `live_publish_every_entries=64`.

Tests or equivalent validation:

- `CARGO_HOME=$PWD/.local/cargo-home CARGO_TARGET_DIR=$PWD/.local/cargo-target cargo test --manifest-path rust/Cargo.toml -p journal-core`: PASS, 62 unit tests plus doc tests.
- `GOCACHE=$PWD/../.local/go-cache GOMODCACHE=$PWD/../.local/go-modcache GOPATH=$PWD/../.local/go-path go test ./...` from `go/`: PASS.
- `python3 tests/interoperability/run_compact_matrix.py --writers rust go --readers stock rust go node python --entries 256`: PASS, 28/28 checks, systemd `260 (260.1-2-manjaro)`.
- `python3 tests/interoperability/run_compression_matrix.py --writers rust go --readers stock rust go node python --compression zstd xz lz4 --entries 256`: PASS, 108/108 checks, systemd `260 (260.1-2-manjaro)`.
- `python3 tests/interoperability/run_byte_identity.py --final-state all`: exit
  status 1 because Node.js and Python still differ from systemd on
  `field_hash_chain_depth`; Rust and Go both compared byte-for-byte equal to
  systemd for `online`, `offline`, and `archived`.
- Saved byte-identity parse artifact:
  `.local/benchmarks/profiles/sow-0062/byte-identity-skiprun.json`.
  Parsed result for this SOW: `('systemd', 'rust'): True` and
  `('systemd', 'go'): True` in all three final states.
- `git diff --check`: PASS.
- `.agents/sow/audit.sh`: PASS before SOW lifecycle move and PASS after moving
  the completed SOW to `.agents/sow/done/`.

Real-use evidence:

- User reported SNMP traps ingestion improved from about 5.5k traps/s on an
  older SDK version to about 170k traps/s after recent writer API/performance
  work. This SOW continues optimizing the same ingestion-critical writer path.

Reviewer findings:

- Read-only `glm-5.1` whole-SOW review: PRODUCTION GRADE. No blocking
  issues. Non-blocking notes were limited to documenting the Rust
  `read_u32_le_at()` unsafe invariants more explicitly, acknowledging the
  theoretical `remaining as u32` constraint that matches the existing
  systemd-compatible hash path, and noting existing duplicate chain-depth walk
  helper code outside this optimization scope.
- Read-only `minimax-m2.7-coder` whole-SOW review: PRODUCTION GRADE. No
  blocking issues. The reviewer found the Rust unsafe mmap/hash paths and Go
  direct mmap slice paths safe, compact DATA tail metadata correct, compressed
  DATA behavior unchanged, and Rust/Go byte-identity evidence sufficient.
- Read-only `kimi-k2.6` whole-SOW review: PRODUCTION GRADE. No blocking
  issues. The reviewer confirmed functional correctness, journal compatibility,
  safety of the direct mmap paths, compact DATA tail fallback behavior,
  compressed DATA behavior, performance evidence, and SOW evidence quality.
- A `qwen3.6-plus` read-only review process was launched with the same prompt,
  but the process had exited before its final transcript could be retrieved
  after session resume. No qwen findings are used as acceptance evidence.
- Disposition: no code changes required from reviewer feedback. The only
  actionable reviewer comments were non-blocking documentation nits, and the
  current Rust unsafe block already documents the safety invariant locally.

Same-failure scan:

- Rejected regression attempts were searched by benchmark re-run before being
  reverted. Remaining accepted changes are covered by compact, compression, and
  byte-identity validation for Rust and Go writers.

Sensitive data gate:

- Synthetic benchmark and conformance data only. No raw secrets, credentials, bearer
  tokens, SNMP communities, customer identifiers, personal data, private
  endpoints, or proprietary incident details are recorded.

Artifact maintenance gate:

- AGENTS.md: no update expected.
- Runtime project skills: no update expected; the workflow already requires
  benchmark/profile evidence for performance claims.
- Specs: no public contract change; this SOW optimizes existing writer
  behavior.
- End-user/operator docs: no public option or behavior change.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: completed child SOW under SOW-0009.
- SOW-status.md: updated to move this SOW from Current to Done.

Specs update:

- No spec update needed because no public SDK contract, file-format behavior,
  field policy, compression option, FSS behavior, or live publication contract
  changed.

Project skills update:

- No project skill update needed; the current compatibility and orchestration
  skills already covered the required profiling, benchmark, matrix, and
  review workflow.

End-user/operator docs update:

- No end-user/operator docs update needed because this is an internal
  performance optimization with unchanged public API and behavior.

End-user/operator skills update:

- No output/reference skill affected.

Lessons:

- Keep only measured wins. Several plausible direct/inline paths were rejected
  because they lost throughput after benchmark runs.
- Compact DATA tail metadata was already written by the writers; using it for
  append avoids unnecessary chain walks and produces the same compact journal
  layout.
- DATA lookup should check object hash before touching payload bytes; this
  avoids expensive payload copies on hash-bucket collisions while preserving
  compressed-DATA behavior.

Follow-up mapping:

- Node.js and Python byte-identity mismatch on `field_hash_chain_depth` remains
  outside this Rust/Go writer SOW and belongs to the existing Node/Python writer
  parity work tracked under SOW-0053 and SOW-0054.

## Outcome

Completed. Rust and Go writer hot paths were profiled, optimized, benchmarked,
validated, and reviewed. Three independent read-only whole-SOW reviews found no
blocking issues and marked the work production-grade.

## Lessons Extracted

- Performance work must record rejected experiments, not only accepted patches,
  because plausible low-level changes can regress the hot path.
- The benchmark harness should continue using multiple repetitions and warmups;
  one-off runs were directionally useful but less reliable than the final
  10-measurement report.

## Followup

- Existing Node.js and Python writer byte-identity parity work remains tracked
  by SOW-0053 and SOW-0054.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
