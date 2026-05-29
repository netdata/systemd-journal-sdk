# SOW-0052 - Rust Reader Last-Mile Optimization

## Status

Status: completed

Sub-state: completed and ready for Python reader/writer port follow-up.

## Requirements

### Purpose

Make the Rust reader the fastest and cleanest reference reader this SDK can
provide while preserving libsystemd/Netdata `jf` compatibility. This is the
reference implementation that Python and Node.js reader ports will follow.

### User Request

The user asked to optimize the Rust reader as much as practical first, then
port the Rust reader and writer behavior to Python, and finally to Node.js.

### Assistant Understanding

Facts:

- Rust reader parity with libsystemd/Netdata `jf` is complete in SOW-0043.
- Rust reader hot-path optimization already fixed live-bound refresh overhead
  in SOW-0044.
- The user now wants one more Rust-first optimization pass before Python and
  Node.js are aligned to Rust.
- Python and Node.js reader/writer work must follow the Rust contract, not a
  temporary or less optimized contract.

Inferences:

- This SOW should focus on measurable Rust reader bottlenecks that remain after
  SOW-0043 and SOW-0044.
- The optimization goal is not speculative complexity. Changes should be kept
  only when benchmarks or profiles prove they help without weakening
  compatibility or API clarity.

Unknowns:

- Whether meaningful Rust reader bottlenecks remain after SOW-0044.
- Whether the best remaining gains are in the native SDK path, facade path,
  directory merge path, compression path, or query/filter path.

### Acceptance Criteria

- Current Rust reader benchmarks are re-established for single-file and
  ordered directory reads.
- Profiling identifies the remaining Rust reader hot paths before changes.
- Rust reader optimizations are implemented only when supported by measured
  evidence.
- libsystemd-compatible facade semantics from SOW-0043 remain unchanged:
  uncompressed current-entry data is mmap-backed, compressed current-entry data
  uses one reusable decompression buffer, and pointer lifetime is reader-owned.
- Shared reader conformance, directory, mixed-directory, and live-reader
  compatibility tests still pass for affected Rust surfaces.
- Benchmarks are rerun and recorded after optimization, including comparison to
  stock libsystemd where the harness supports it.
- Documentation/specs are updated if public reader behavior or performance
  claims change.
- Read-only external reviewers review the whole SOW after local validation.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/done/SOW-0043-20260528-rust-reader-libsystemd-jf-parity.md`
- `.agents/sow/done/SOW-0044-20260528-rust-reader-hot-path-optimization.md`
- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`
- `.agents/sow/specs/product-scope.md`
- `rust/src/journal/src/lib.rs`
- `tests/benchmarks/run_reader_core_benchmarks.py`

Current state:

- SOW-0043 records Rust single-file `facade-data` live/windowed at about
  1.17M rows/s versus stock libsystemd data enumeration at about 645k rows/s on
  the compact 100k-row benchmark.
- SOW-0044 records Rust `sdk-payloads` live/windowed at about 1.34M rows/s
  versus stock libsystemd data enumeration at about 660k rows/s, with live
  bounds no longer refreshing on every read slice.
- Rust is already ahead of stock libsystemd on the measured single-file and
  open-file benchmark slices, but this does not prove no remaining hot-path
  gains exist.

Risks:

- Over-optimizing Rust could make the reference implementation harder to port
  cleanly to Python and Node.js.
- Changes to facade lifetime handling can silently break libsystemd-like users.
- Changes to directory ordering or live bounds can break live-reader
  compatibility while improving a synthetic benchmark.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The Rust reader is currently production-fast on the measured benchmark
  slices, but the user wants a final Rust-first pass before dynamic-language
  ports. The root-cause model is not a known bug; this is a profiling-driven
  search for remaining avoidable syscalls, allocations, decompression churn,
  entry/object lookups, facade copies, and directory-merge overhead.

Evidence reviewed:

- `.agents/sow/done/SOW-0043-20260528-rust-reader-libsystemd-jf-parity.md`
  records the libsystemd/`jf` facade contract and latest facade benchmark.
- `.agents/sow/done/SOW-0044-20260528-rust-reader-hot-path-optimization.md`
  records the prior live-bound optimization and validation gates.
- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`
  requires separate single-file and ordered directory reader benchmarks.
- `.agents/sow/specs/product-scope.md` records current reader API and
  compatibility contracts.

Affected contracts and surfaces:

- Rust native reader API.
- Rust libsystemd-compatible facade API.
- Rust ordered directory reader.
- Reader benchmark harnesses and benchmark reports.
- Product scope specs and Rust README if public reader behavior is clarified.
- Follow-on Python and Node.js port SOWs.

Existing patterns to reuse:

- `tests/benchmarks/run_reader_core_benchmarks.py`.
- Existing Rust `ReadMode::Live` and snapshot/windowed/whole-file options.
- Existing Rust facade tests around `entry_data_restart()` and
  `enumerate_entry_payload()`.
- Existing mixed-directory and live interoperability harnesses.

Risk and blast radius:

- Medium. The work touches the reference reader hot path used by later ports.
  Incorrect changes could regress compatibility, pointer lifetime, ordered
  directory reads, or live reading.

Sensitive data handling plan:

- Use generated fixtures and repository benchmark artifacts only.
- Do not record real logs, SNMP communities, credentials, bearer tokens,
  customer data, personal data, private endpoints, or production incident
  details.

Implementation plan:

1. Re-run baseline Rust reader benchmarks for single-file and ordered directory
   slices with the current code.
2. Profile representative Rust reader modes: native payload iteration, facade
   data enumeration, ordered directory read, and query/filter path if available
   in the benchmark harness.
3. Inspect hot paths and apply only evidence-backed optimizations.
4. Add focused regression tests for any lifetime, allocation, or ordering
   behavior changed.
5. Re-run Rust tests, compatibility matrices relevant to reader behavior, and
   benchmarks.
6. Run whole-SOW read-only reviewer passes after local validation.

Validation plan:

- `cargo test --manifest-path Cargo.toml --workspace`
- Reader benchmark reruns with recorded command lines and result directories.
- Rust reader/facade targeted tests for any touched behavior.
- Directory, mixed-directory, or live-reader matrix runs when touched code
  affects those surfaces.
- `git diff --check`
- `.agents/sow/audit.sh`
- Whole-SOW read-only reviewer passes.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: no update expected unless a durable profiling rule is
  discovered.
- Specs: update `.agents/sow/specs/product-scope.md` if reader contracts or
  published performance status change.
- End-user/operator docs: update `rust/README.md` only if public reader
  behavior changes or needs clarification.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: this SOW is current/in-progress and should close before Python
  work starts.
- SOW-status.md: update for activation and later close.

Open-source reference evidence:

- No new external open-source repository evidence was needed to create this
  profiling SOW. Existing systemd/libsystemd evidence is inherited from
  SOW-0043 and SOW-0044. If this SOW changes compatibility-sensitive behavior,
  systemd v260.1 source evidence will be cited with durable upstream identity.

Open decisions:

- None. The user already selected the priority order.

## Implications And Decisions

1. 2026-05-29 Rust-first optimization order
   - Decision: optimize Rust reader before Python and Node.js reader/writer
     ports.
   - Implication: Python and Node.js should inherit the final Rust reference
     behavior, reducing duplicated rework.
   - Risk: Rust optimization may find little gain; if so, the correct outcome
     is measured evidence and a clean handoff to Python, not speculative
     complexity.

## Plan

1. Establish baseline and profiles for current Rust reader modes.
2. Implement measured Rust reader optimizations in cohesive batches.
3. Validate tests, compatibility, and benchmarks.
4. Review the complete SOW with read-only reviewers.
5. Commit and push the verified chunk.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current user routing.

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

- If profiling shows no meaningful Rust bottleneck, record the evidence and
  close with no code changes except benchmark/SOW/spec documentation updates.
- If a reviewer finds a production-grade blocker, fix and rerun the same whole
  SOW review scope.

## Execution Log

### 2026-05-29

- Created from the user's priority change.
- Established a 200k-row compact/offline reader benchmark baseline:
  - Command: `python3 tests/benchmarks/run_reader_core_benchmarks.py --rows 200000 --directory-rows 200000 --repetitions 5 --warmups 1 --format compact --final-state offline --max-size-bytes 134217728 --directory-max-size-bytes 134217728 --keep-fixtures`
  - Result directory: `.local/benchmarks/reader-core/20260529T030137Z`
  - Baseline medians: Rust single-file `sdk-payloads` live/windowed
    1,104,750 rows/s; Rust single-file `facade-data` live/windowed
    1,055,677 rows/s; Rust open-files `sdk-payloads` live/windowed
    1,092,742 rows/s; Rust open-files `facade-data` live/windowed
    1,052,428 rows/s; stock systemd single-file data enumeration
    603,714 rows/s; stock systemd open-files data enumeration
    619,788 rows/s.
- Profiled the baseline hot path:
  - `perf record -F 999 -g --call-graph dwarf -o .local/benchmarks/reader-core/20260529T030137Z/profiles/perf-sdk-payloads-live.data -- .local/cargo-target/release/reader_core_bench ...`
  - `perf report` showed `JournalFile::journal_object_ref`, DATA object
    access, and `WindowManager::get_slice` as the dominant payload scan costs.
- Implemented the Rust reader optimization batch:
  - Added temporary guarded mutable access for closure-only reads in
    `GuardedCell::with_mut`.
  - Added DATA payload visitor helpers that parse DATA headers and visit
    payload bytes without materializing `DataObject` guards for the native SDK
    payload visitor path.
  - Cached current-entry DATA offsets and per-entry DATA payload read context
    in `FileReader` so `next()` plus `visit_entry_payloads()` does not parse
    the ENTRY object twice.
  - Added mmap-backed raw payload guards for uncompressed facade
    `enumerate_entry_payload()` while preserving compressed fallback to the
    reusable decompression buffer.
  - Added active-window slice reuse after DATA header reads to avoid a second
    full window lookup when the active mmap window already contains the DATA
    object.
  - Kept the helper types doc-hidden and removed them from the high-level
    `journal` crate re-export to avoid creating an accidental public SDK API
    promise.
- Added focused Rust regression tests:
  - `visit_data_payload_at_returns_compact_uncompressed_payload`
  - `visit_data_payload_at_decompresses_payload`
  - Existing facade tests continue to cover mmap-backed uncompressed DATA and
    compressed DATA fallback behavior.
- Reran the final 200k-row compact/offline benchmark:
  - Command: `python3 tests/benchmarks/run_reader_core_benchmarks.py --rows 200000 --directory-rows 200000 --repetitions 5 --warmups 1 --format compact --final-state offline --max-size-bytes 134217728 --directory-max-size-bytes 134217728 --keep-fixtures`
  - Result directory: `.local/benchmarks/reader-core/20260529T032943Z`
  - Final medians:
    - Rust single-file `sdk-payloads` live/windowed: 2,436,956 rows/s,
      77,982,593 fields/s.
    - Rust single-file `facade-data` live/windowed: 2,242,801 rows/s,
      71,769,641 fields/s.
    - Rust open-files `sdk-payloads` live/windowed: 1,740,104 rows/s,
      57,423,436 fields/s.
    - Rust open-files `facade-data` live/windowed: 1,698,669 rows/s,
      56,056,084 fields/s.
    - Stock systemd single-file data enumeration: 537,075 rows/s,
      17,186,413 fields/s.
    - Stock systemd open-files data enumeration: 612,792 rows/s,
      20,222,129 fields/s.

## Validation

Acceptance criteria evidence:

- Current Rust reader benchmarks were re-established for single-file and
  ordered directory reads in `.local/benchmarks/reader-core/20260529T030137Z`.
- Profiling identified DATA object access and mmap window lookup as the
  remaining payload hot-path costs.
- Implemented optimizations are directly tied to those costs: fewer ENTRY/DATA
  object materializations, fewer duplicated ENTRY scans, mmap-backed facade
  uncompressed DATA, and active-window reuse after DATA header reads.
- Final benchmark evidence is recorded in
  `.local/benchmarks/reader-core/20260529T032943Z`.
- Final single-file live/windowed Rust `sdk-payloads` is 2,436,956 rows/s
  versus stock systemd data enumeration at 537,075 rows/s on the same run.
- Final single-file live/windowed Rust `facade-data` is 2,242,801 rows/s
  versus stock systemd data enumeration at 537,075 rows/s on the same run.
- Final open-files live/windowed Rust `sdk-payloads` is 1,740,104 rows/s
  versus stock systemd open-files data enumeration at 612,792 rows/s.
- Final open-files live/windowed Rust `facade-data` is 1,698,669 rows/s
  versus stock systemd open-files data enumeration at 612,792 rows/s.

Tests or equivalent validation:

- `cargo test --manifest-path Cargo.toml -p journal-core visit_data_payload_at`
  - PASS.
- `cargo test --manifest-path Cargo.toml -p journal jf_facade`
  - PASS.
- `cargo test --manifest-path Cargo.toml -p journal facade_uncompressed_data_uses_mmap_payload`
  - PASS.
- `cargo test --manifest-path Cargo.toml -p journal file_reader_seek_clears_cached_entry_payload_offsets`
  - PASS.
- `cargo test --manifest-path Cargo.toml --workspace`
  - PASS.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_directory_matrix.py`
  - PASS on systemd 260 (260.1-2-manjaro).
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_mixed_directory_matrix.py`
  - PASS, 72/72, latest result file
    `.local/interoperability/mixed-directory-matrix-results-20260529-070401.json`.
  - The first run with system Python failed before executing checks because
    `lz4.block` was missing; this is the documented local dependency setup
    issue, not a reader compatibility failure.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_live_matrix.py`
  - PASS, 36/36, latest result file
    `.local/interoperability/live-feature-matrix-results-20260529-070549.json`.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_journalctl_query_matrix.py`
  - PASS on systemd 260 (260.1-2-manjaro).
- `git diff --check`
  - PASS.
- `.agents/sow/audit.sh`
  - PASS after moving this SOW to `.agents/sow/done/`, with status/directory
    consistency clean and no sensitive-data findings.

Real-use evidence:

- Stock `journalctl` and stock libsystemd readers were exercised by the
  directory, mixed-directory, live feature, and journalctl query matrices.
- Live feature coverage included regular, zstd, xz, lz4, compact,
  compact-zstd, compact-xz, compact-lz4, and sealed files, with Go, Rust,
  Node.js, and Python writers.

Reviewer findings:

- First whole-SOW review pass:
  - GLM: PRODUCTION GRADE. Non-blocking notes covered conservative cached
    live context, doc-hidden helper visibility, and compressed fallback
    behavior; no code blocker.
  - Minimax: PRODUCTION GRADE. Non-blocking notes covered the
    `GuardedCell::with_mut` comment, doc-hidden helper visibility, and one
    false `PAGE_SIZE` observation. The comment was clarified; `PAGE_SIZE` was
    not changed because it is existing code and is used.
  - Qwen replacement review: PRODUCTION GRADE. Non-blocking notes covered the
    conservative cached context and panic-safety invariant; no code blocker.
  - Kimi: PRODUCTION GRADE. Non-blocking findings identified stale cached DATA
    offsets after seek for direct `FileReader` callers, missing defensive
    validation in `raw_data_payload_ref_with_info`, public helper method
    visibility, and pending audit evidence.
- Disposition after first pass:
  - Added `reset_cached_entry_data_state()`, made seek operations clear cached
    entry DATA state unconditionally, marked `enumerate_entry_payload()` active
    while enumeration is in progress, and added
    `file_reader_seek_clears_cached_entry_payload_offsets`.
  - Added runtime validation in `raw_data_payload_ref_with_info()` before
    slicing from caller-supplied `DataPayloadObjectInfo`.
  - Marked cross-crate `JournalReader` payload helper methods `#[doc(hidden)]`
    because they are implementation helpers, not intended high-level SDK API.
- Second whole-SOW review pass after fixes:
  - Qwen: code review PRODUCTION GRADE; procedural verdict NOT PRODUCTION
    GRADE only because this SOW still said reviewer findings and audit evidence
    were pending at review time. This SOW update and final audit close that
    procedural gap.
  - Minimax: PRODUCTION GRADE. No blocking findings.
  - Kimi: PRODUCTION GRADE. No blocking findings.
  - GLM rerun did not return usable final output before the process exited; the
    first GLM pass is recorded above and three usable second-pass reviewers
    completed after the final code fixes.

Same-failure scan:

- Searched the changed reader paths for duplicate object-guard and payload
  enumeration patterns while implementing the facade guard release fix.
- Existing facade stateful tests caught the first guard-lifetime regression
  (`previous object is still in use`); the fix releases object guards before
  the next facade metadata read.
- Searched for cached entry payload offset invalidation paths after Kimi's
  finding; seek-head, seek-tail, seek-realtime, entry-data restart, exhaustion,
  and end-of-file paths now clear or refresh cached state explicitly.

Sensitive data gate:

- Planning and validation text contain no raw sensitive data. Benchmark and
  interoperability fixtures are synthetic and repository-local.

Artifact maintenance gate:

- AGENTS.md: no change needed; workflow and repository-boundary rules are
  unchanged.
- Runtime project skills: no change needed; no new durable workflow rule was
  introduced.
- Specs: no spec update needed; public reader behavior and compatibility
  contracts are unchanged. The new hot-path helpers are doc-hidden and not
  re-exported by the high-level `journal` crate; cross-crate `JournalReader`
  helpers needed by the facade implementation are also marked `#[doc(hidden)]`.
- End-user/operator docs: no change needed; this SOW changes internal Rust
  performance, not user-facing reader semantics.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: completed in this closeout and moved to `.agents/sow/done/`.
- SOW-status.md: updated to move SOW-0052 from Current to Recently Closed Or
  Completed.

Specs update:

- No spec update required; behavior is unchanged and the performance evidence
  is recorded in this SOW and SOW-status.

Project skills update:

- No project skill update required.

End-user/operator docs update:

- No end-user/operator docs update required.

End-user/operator skills update:

- No end-user/operator skill update required.

Lessons:

- The fastest Rust reader path should avoid materializing full entry objects
  when the caller only needs `FIELD=VALUE` payload bytes.
- Facade pointer-lifetime tests are critical: mmap-backed payload pointers must
  remain valid until the next reader operation, but the previous guard must be
  released before the next metadata read.
- Whole-file mmap is not a universal win; in the final benchmark it helps
  single-file snapshot `sdk-payloads`, but live/windowed remains the production
  compatibility baseline.

Follow-up mapping:

- Python reader/writer port work remains tracked by SOW-0053.
- Node.js reader/writer port work remains tracked by SOW-0054.
- Go reader alignment remains tracked by SOW-0045.

## Outcome

Completed.

Rust reader last-mile optimization is complete. The final compact 200k-row
benchmark measured Rust single-file `sdk-payloads` live/windowed at 2,436,956
rows/s and Rust single-file `facade-data` live/windowed at 2,242,801 rows/s,
versus stock systemd data enumeration at 537,075 rows/s on the same run. Open
files measured 1,740,104 rows/s for `sdk-payloads` and 1,698,669 rows/s for
`facade-data`, versus stock systemd open-files enumeration at 612,792 rows/s.

The kept implementation changes are limited to reader hot paths: cached
current-entry DATA offsets, direct DATA payload visitors, active mmap window
reuse, mmap-backed uncompressed facade payloads, reusable compressed fallback,
and explicit cached-state invalidation. Rust reader semantics and
libsystemd/Netdata `jf` facade behavior are unchanged.

## Lessons Extracted

- Direct `FIELD=VALUE` payload scans should not materialize full ENTRY and DATA
  object wrappers when callers only need bytes.
- Any reader optimization that returns mmap-backed data must keep pointer
  lifetime tests close to the change. The facade contract is "valid until the
  next reader operation", not "valid forever".
- Cached entry state must be invalidated at every cursor movement boundary,
  including direct `FileReader` uses outside the facade.
- `#[doc(hidden)]` is the right compromise for cross-crate Rust helpers that
  must be public to sibling crates but are not intended as SDK API.
- Whole-SOW reviews are useful when they review a complete optimization batch;
  the second pass found no code blocker after the defensive fixes.

## Followup

- SOW-0053 remains the next implementation SOW: port the finalized Rust reader
  and writer behavior to Python.
- SOW-0054 follows SOW-0053: port the finalized Rust reader and writer behavior
  to Node.js.
- SOW-0045 still tracks Go reader alignment and optimization after the current
  Rust/Python/Node priority chain.

## Regression Log

None yet.
