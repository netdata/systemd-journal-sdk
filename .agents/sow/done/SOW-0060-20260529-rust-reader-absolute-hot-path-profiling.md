# SOW-0060 - Rust Reader Absolute Hot-Path Profiling

## Status

Status: completed

Sub-state: completed. Rust reader profiles identified the remaining costs,
Rust ordered directory reads gained the non-overlap sequential fast path, and
Rust facade data enumeration now provides the accepted row-scoped payload
lifetime contract.

## Requirements

### Purpose

Make the Rust journal reader as efficient as possible for Netdata-fit
production reads while preserving accuracy, journal-format robustness, and the
reader compatibility contracts already established in this repository.

### User Request

The user clarified that Rust and Go readers/writers must be as fast as possible
without sacrificing accuracy or robustness. For reader optimization:

- readers may assume they see a snapshot of journal files as they were when the
  query started;
- whole-file mmap is acceptable if it provides a benefit;
- every condition, branch, calculation, and data access in the hot path needs a
  strong reason tied to correctness, accuracy, robustness, or compatibility;
- Rust must be profiled to find exact reasons it is not yet an absolutely
  optimal journal-file reader.

### Assistant Understanding

Facts:

- SOW-0043 completed the Rust reader compatibility target, including the
  libsystemd-like facade and Netdata `jf` compatibility shape.
- SOW-0044 fixed a live-reader performance regression by using systemd-style
  cached mutable bounds instead of refreshing file state on every slice.
- SOW-0057 measured Rust live whole-file mmap and found that whole-file mmap
  alone did not explain the Go/Rust reader gap on the then-current compact
  payload corpus.
- SOW-0058 changed Rust DATA payload reads to parse only the common 16-byte
  object header before slicing payload data, but its benchmark result was mixed
  and did not conclusively explain the remaining Go/Rust reader gap.
- SOW-0056 made Go reader hot paths substantially faster with mmap-backed
  access, offset storage reuse, by-value ENTRY headers, layout-size caching, and
  specialized compact/regular loops.

Inferences:

- The next useful Rust work is not another benchmark-only run. It must profile
  the Rust reader and attribute CPU cost to exact code paths.
- The production hot-path mode should include snapshot bounds and whole-file
  mmap because the user explicitly accepted those semantics when they improve
  speed.
- If Rust still loses to Go after comparable options, the difference is likely
  in per-entry/per-payload overhead such as guard/lifetime machinery,
  self-referencing reader access, directory merge state, object access
  abstraction, or benchmark callback shape.

Unknowns:

- Exact CPU attribution for Rust `sdk-payloads`, `facade-data`, and
  `core-payloads` after SOW-0058.
- Whether the remaining Rust cost is in SDK layers, core journal object access,
  facade lifetime guarantees, benchmark callback overhead, or directory
  ordering/merge code.

### Acceptance Criteria

- Rust reader profiles are collected for at least:
  - single-file `core-payloads`;
  - single-file `sdk-payloads`;
  - single-file `facade-data`;
  - ordered `open-files` `sdk-payloads`;
  - ordered `open-files` `facade-data`.
- Profiles use production-relevant reader options: compact format,
  compression off, FSS off, snapshot bounds where relevant, and whole-file mmap
  where relevant.
- A fresh apples-to-apples benchmark compares Rust, Go, and systemd using the
  same fixture/settings after SOW-0058.
- The SOW records exact hot functions, approximate cost, and code evidence for
  why each cost exists.
- Candidate Rust optimizations are classified as:
  - keep and implement now;
  - measure with an experiment first;
  - reject because they sacrifice accuracy or robustness;
  - defer to a real follow-up SOW.
- If code changes are made, conformance/regression tests and benchmarks pass.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/specs/product-scope.md`
- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`
- `.agents/sow/done/SOW-0043-20260528-rust-reader-libsystemd-jf-parity.md`
- `.agents/sow/done/SOW-0044-20260528-rust-reader-hot-path-optimization.md`
- `.agents/sow/done/SOW-0056-20260529-go-reader-hot-path-optimization-phase2.md`
- `.agents/sow/done/SOW-0057-20260529-rust-live-whole-file-mmap-reader-option.md`
- `.agents/sow/done/SOW-0058-20260529-rust-data-header-fast-path.md`
- `tests/benchmarks/run_reader_core_benchmarks.py`
- `rust/src/internal/testcmd/reader_core_bench/src/main.rs`
- `rust/src/journal/src/lib.rs`
- `rust/src/crates/journal-core/src/file/reader.rs`
- `rust/src/crates/journal-core/src/file/file.rs`
- `go/journal/reader.go`
- `go/journal/mmap_unix.go`

Current state:

- The benchmark harness already supports Rust `live` and `snapshot` bounds,
  plus `windowed` and `whole-file` mmap strategies.
- The Rust benchmark binary has no built-in CPU profiler flag. Linux `perf` is
  available in the environment and can write profile artifacts under `.local/`.
- The user accepts snapshot-at-query-start semantics for optimized reader
  paths, so the production hot-path baseline no longer needs to be live-follow
  compatible by default for Netdata-style polling queries.

Risks:

- Profiles are sensitive to CPU scheduling, cache state, and fixture size.
  Conclusions must come from repeated runs and stable high-cost symbols, not
  one noisy sample.
- Removing checks or guards without proving their correctness role could break
  corrupt-file handling or mmap safety.
- Optimizing only benchmark code would create false performance evidence and
  must be avoided unless the benchmark code is itself the measured API layer.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Rust is faster than stock systemd on the measured reader paths, but Go is
  still faster in comparable SDK payload reads. Previous isolated experiments
  show that neither whole-file mmap nor DATA-header parsing alone fully explains
  the gap. The likely remaining causes are in Rust per-entry/per-payload
  framework overhead, but this must be proven by profiling before code changes.

Evidence reviewed:

- SOW-0056 records Go hot-path optimizations and Go reader benchmark results.
- SOW-0057 records Rust whole-file mmap measurements and concludes mmap
  strategy alone did not explain the gap.
- SOW-0058 records Rust DATA-header fast-path measurements and concludes the
  change did not conclusively explain the gap.
- Current Rust code shows reader option support for snapshot and whole-file
  mmap, SDK payload visitor APIs, facade data enumeration, and lower-level
  `JournalReader` object access.

Affected contracts and surfaces:

- Rust reader SDK API: `FileReader`, `DirectoryReader`, payload visitor paths.
- Rust libsystemd-compatible facade: current-entry data enumeration and pointer
  lifetime semantics.
- Benchmark harness: reader-core fixture generation, reader command wrappers,
  and report artifacts.
- Product performance contract for Netdata reader integration readiness.

Existing patterns to reuse:

- `.local/benchmarks/` for generated benchmark/profile artifacts.
- `tests/benchmarks/run_reader_core_benchmarks.py` for apples-to-apples
  fixture generation and checksum validation.
- `reader_core_bench` modes for separating core, SDK, facade, single-file, and
  ordered multi-file costs.
- SOW-0056's rule that kept optimizations require profile/benchmark evidence.

Risk and blast radius:

- High for Rust reader performance claims and later Python/Node.js ports.
- Medium compatibility risk if a future optimization bypasses validation,
  guard, decompression, or facade lifetime logic.
- Low repository-boundary risk if all profile artifacts stay under `.local/`.

Sensitive data handling plan:

- Use generated benchmark fixtures only.
- Do not record real production logs, traps, flows, customer data, personal
  data, credentials, bearer tokens, SNMP communities, private endpoints, or
  proprietary incident details.

Implementation plan:

1. Establish a fresh Rust/Go/systemd benchmark baseline using identical
   compact, uncompressed, FSS-off fixtures and production-relevant max sizes.
2. Build the Rust reader benchmark with release symbols suitable for profiling.
3. Collect `perf` profiles for Rust core, SDK, facade, single-file, and
   open-files modes with snapshot/whole-file settings where relevant.
4. Compare profile attribution against Go's known hot-path shape and current
   Rust code.
5. Only after profiling, decide whether to implement a narrow Rust hot-path
   optimization in this SOW or split follow-up work.

Validation plan:

- Run the benchmark harness for Rust, Go, and systemd with checksum validation.
- Run targeted Rust reader benchmarks for profile target modes.
- If code changes are made, run affected Rust package tests and rerun the same
  benchmarks.
- Record exact commands, artifact paths, and profile conclusions.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: no update expected unless a durable profiling
  workflow is added.
- Specs: update only if public reader options/defaults or performance contract
  change.
- End-user/operator docs: no update expected during profiling-only work.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: new active child SOW under SOW-0009 reader performance.
- SOW-status.md: update to list this active profiling SOW.

Open-source reference evidence:

- No new external open-source source was checked for this profiling setup. The
  compatibility baseline remains the systemd source evidence already recorded
  in `product-scope.md`.

Open decisions:

- Resolved by user: snapshot-at-query-start semantics are acceptable for
  optimized reader hot paths when they improve performance.
- Resolved by user: whole-file mmap is acceptable when it provides measured
  benefit.
- Resolved by user: performance optimizations must not sacrifice accuracy or
  robustness.

## Implications And Decisions

1. 2026-05-29 reader performance target
   - Decision: optimized Rust reader paths may target snapshot-at-query-start
     semantics for Netdata-style polling queries.
   - Implication: live-follow compatibility remains required where explicitly
     selected, but it is not the only production benchmark baseline.

2. 2026-05-29 mmap strategy
   - Decision: whole-file mmap is allowed when measurements show a benefit.
   - Implication: Rust may expose or use a high-memory-performance mode without
     treating windowed mmap as the only production path.

3. 2026-05-29 hot-path minimalism
   - Decision: every retained hot-path branch/check/calculation must be tied to
     accuracy, robustness, compatibility, or measured performance benefit.
   - Implication: speculative caches, compatibility wrappers, and repeated
     parsing must be removed or bypassed on hot paths if profiles show they cost
     CPU and do not preserve a required contract.

4. 2026-05-29 reader payload lifetime
   - Decision: SDK reader APIs and libsystemd-like facades must provide a
     stronger-than-libsystemd row-scoped payload lifetime guarantee.
   - Requirement: payload pointers/slices returned while enumerating a current
     row remain valid until the reader advances to another row, seeks, closes,
     refreshes/remaps the file, or explicitly releases the row.
   - Evidence: Netdata `systemd-journal.plugin` passes borrowed journal payloads
     to facets, and facets copies retained values at row finish instead of
     copying every field immediately. Stock libsystemd documents field payloads
     as valid only until the next data/enumeration call, but normal libsystemd
     mmap-cache behavior makes the existing Netdata path work in practice for
     common uncompressed data.
   - Implication: Rust must not invalidate prior same-row payload pointers on
     each field enumeration. Compressed DATA must be decompressed into
     row-scoped storage, not one reusable field-scoped scratch buffer.
   - Rejected alternative: changing facets to copy every field immediately,
     because it would copy fields from rows that filters, search, sampling,
     anchoring, or pagination may later reject, harming facets performance.

## Plan

1. Run fresh Rust/Go/systemd reader benchmarks with standardized settings.
2. Produce Rust `perf` reports for the main core, SDK, facade, and open-files
   payload paths.
3. Map top symbols to source lines and classify exact cost centers.
4. Implement or split only evidence-backed optimizations.
5. Validate correctness and benchmark impact before closing.

## Delegation Plan

Implementer:

- Local implementation by the project manager. No external implementer agents.

Reviewers:

- Read-only reviewers from the approved pool only after a complete local
  profiling/optimization batch is ready.

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

- If `perf` cannot produce usable symbols, record the blocker and use the next
  best local profiler or instrumentation under `.local/`.
- If profiles are noisy, rerun with larger fixture sizes or repeated runs before
  drawing conclusions.
- If an optimization is not clearly beneficial, remove it before close or split
  a measurement-only follow-up.

## Execution Log

### 2026-05-29

- Created active SOW from the user's stricter Rust reader optimization request.
- Ran a fresh Rust/Go/systemd baseline with compact, uncompressed, FSS-off
  generated fixtures:

  ```text
  CARGO_HOME=.local/cargo-home CARGO_TARGET_DIR=.local/cargo-target \
  GOCACHE=$PWD/.local/go-cache GOMODCACHE=$PWD/.local/go-modcache \
  GOPATH=$PWD/.local/go-path \
  python3 tests/benchmarks/run_reader_core_benchmarks.py \
    --languages rust,go,systemd \
    --rows 200000 \
    --directory-rows 200000 \
    --repetitions 5 \
    --warmups 1 \
    --format compact \
    --final-state offline \
    --max-size-bytes 134217728 \
    --directory-max-size-bytes 33554432 \
    --window-size 33554432 \
    --out .local/benchmarks/reader-core-rust-hotpath-profile-baseline \
    --keep-fixtures
  ```

  Artifact: `.local/benchmarks/reader-core-rust-hotpath-profile-baseline/20260529T174259Z/`.

- Collected Linux `perf` profiles for Rust:
  - single-file `core-payloads` snapshot whole-file;
  - single-file `sdk-payloads` snapshot whole-file;
  - single-file `facade-data` snapshot whole-file;
  - open-files `sdk-payloads` live whole-file;
  - open-files `facade-data` live whole-file.

  Artifacts:
  - `.local/benchmarks/reader-core-rust-hotpath-profile-baseline/20260529T174259Z/perf/`
  - `.local/benchmarks/reader-core-rust-hotpath-profile-baseline/20260529T174259Z/perf-after-nonoverlap/`

- Profile finding: low-level Rust `core-payloads` is not the production hot
  path and is slow because it opens a full DATA object per field.
  Evidence:
  - `rust/src/internal/testcmd/reader_core_bench/src/main.rs`: `core-payloads`
    uses `file.data_ref(offset)?`, `data.is_compressed()`, and
    `data.raw_payload()`.
  - `rust/src/crates/journal-core/src/file/file.rs`: `journal_object_ref()`
    validates object type and returns a full `ValueGuard<DataObject>`.
  - Perf report `rust-file-core-payloads-snapshot-whole.no-children.txt`:
    `reader_core_bench::read_core` about 29.69% self,
    `WindowManager<M>::get_slice` about 25.90%, and the full
    `ValueGuard<DataObject>` branch about 10.53%.
  - Classification: reject as a production comparison baseline. Keep the
    benchmark for low-level API cost visibility, but use SDK/facade payload
    paths for production-reader performance decisions.

- Profile finding: Rust single-file SDK payload reads are already competitive
  with or faster than Go on the accepted production path.
  Evidence:
  - `rust/src/journal/src/lib.rs`: `FileReader::visit_entry_payloads()`
    delegates to `visit_entry_payload_offsets()`.
  - `rust/src/crates/journal-core/src/file/file.rs`:
    `visit_data_payload_at_with_context()` parses the common DATA header and
    slices the payload directly for uncompressed DATA objects.
  - Perf report `rust-file-sdk-payloads-snapshot-whole.children.txt`:
    `visit_data_payload_at_with_context()` accounts for about 38.28% of the
    sampled inclusive cost. Most of that is direct slice/header work plus the
    benchmark checksum.
  - Classification: keep. This is the main optimized Rust payload path.

- Profile finding: Rust facade DATA enumeration still has avoidable overhead
  compared to the SDK payload visitor.
  Evidence:
  - `rust/src/journal/src/lib.rs`: `enumerate_entry_payload()` calls
    `data_payload_object_info_at()` and then `raw_data_payload_at()`.
  - `rust/src/crates/journal-core/src/file/file.rs`:
    `data_payload_object_info_at()` parses DATA object header metadata, and
    `raw_data_payload_ref_with_info()` later obtains the guarded payload slice.
  - Perf report `rust-file-facade-data-snapshot-whole.children.txt`:
    `SdJournalEnumerateAvailableData` is about 29.20% inclusive,
    `data_payload_object_info_at()` about 10.42%, and
    `raw_data_payload_at()` about 7.23%.
  - Classification: measure/implement next. A combined uncompressed facade
    DATA enumerator may preserve libsystemd-like pointer lifetime while
    removing one lookup/header pass.

- Experimented with a combined uncompressed facade DATA enumerator and rejected
  it:
  - the safe Rust version was blocked by the facade's returned borrowed-slice
    lifetime plus compressed fallback mutable access;
  - a narrow internal raw-pointer version compiled and passed
    `cargo test -p journal`, but early benchmark output showed worse
    `facade-data` throughput than the existing implementation;
  - the experiment was removed before this SOW continued.

  Evidence:
  - partial aborted benchmark artifact:
    `.local/benchmarks/reader-core-rust-hotpath-profile-final2/20260529T182952Z/`;
  - observed current-code `rust file facade-data snapshot whole-file` runs
    during the rejected experiment were around 1.97M-2.18M rows/s, below the
    existing implementation's final full-run median 2.33M rows/s.

  Classification:
  - reject the combined guarded facade path;
  - do not add unsafe code for this unless a later SOW proves a net win and
    records a specific safety argument.

- Go comparison for the remaining facade gap:
  - Go facade DATA enumeration is a direct wrapper around
    `Reader.EnumerateEntryPayload()`.
  - Go `EnumerateEntryPayload()` calls `readDataPayload()` and returns the
    resulting slice.
  - Go `readDataPayload()` parses the object header and slices payload bytes
    from the mmap buffer without a Rust-style `ValueGuard`/`RefCell` guard
    layer.
  - Rust facade enumeration keeps a libsystemd-like borrowed pointer valid
    until the next reader call by storing a guarded payload reference in
    `JournalReader`.

  Evidence:
  - `go/journal/facade.go`: `SdJournalEnumerateAvailableData()` delegates to
    `j.reader.EnumerateEntryPayload()`.
  - `go/journal/reader.go`: `EnumerateEntryPayload()` calls
    `readDataPayload()`.
  - `go/journal/reader.go`: `readDataPayload()` parses the object header and
    returns a slice from `readSlice()`.
  - `rust/src/journal/src/lib.rs`: `enumerate_entry_payload()` performs the
    facade state machine and stores the current DATA payload through
    `JournalReader::raw_data_payload_at()`.
  - `rust/src/crates/journal-core/src/file/reader.rs`:
    `raw_data_payload_at()` stores `raw_payload_guard`.
  - `rust/src/crates/journal-core/src/file/file.rs`:
    `raw_data_payload_ref_with_info()` uses `with_guarded()`.

  Classification:
  - The remaining single-file facade DATA gap is not a generic Rust reader
    weakness. It is tied to Rust's stronger guard/lifetime implementation for
    the libsystemd-like facade.
  - A no-guard whole-file snapshot facade path might be faster, but it is a
    separate safety/API decision and needs a SOW decision before implementation.

- Profile and code-comparison finding: Rust ordered directory reads were
  missing Go's non-overlapping-file sequential fast path.
  Evidence:
  - Go detects non-overlapping files and uses sequential stepping in
    `go/journal/reader.go` through `directoryFilesNonOverlapping()`,
    `canStepSequential()`, and `stepSequential()`.
  - Rust previously always used the merge-candidate scan in
    `DirectoryReader::step_merged()`.
  - This was a real algorithmic difference for rotated directories whose files
    have ordered, non-overlapping sequence/realtime ranges.
  - Classification: implemented now.

- Implemented Rust `DirectoryReader` non-overlap detection and sequential
  stepping:
  - added `non_overlapping` state;
  - added `directory_files_non_overlapping()`;
  - added `can_step_sequential()`;
  - added `step_sequential()`;
  - added a Rust unit test proving forward/backward traversal uses a
    non-overlapping fixture correctly.

- Rejected experiment: lazy DATA-offset collection on `FileReader::next()`.
  Result:
  - It improved `next`-only traversal because `next()` no longer collected
    DATA offsets.
  - It hurt payload modes because payload enumeration then reopened the ENTRY
    object and recollected offsets, duplicating work.
  - The experiment was reverted. No lazy-offset change remains in the working
    tree.
  Classification:
  - Do not merge as a general behavior change.
  - A separate explicit `next`-only API/mode may be worth measuring later if a
    real consumer needs pure entry counting/traversal without payload access.

- Ran Rust validation after the kept code change:

  ```text
  cd rust
  CARGO_HOME=../.local/cargo-home CARGO_TARGET_DIR=../.local/cargo-target cargo fmt --all
  CARGO_HOME=../.local/cargo-home CARGO_TARGET_DIR=../.local/cargo-target cargo test -p journal
  ```

  Result: `cargo test -p journal` passed, 19 tests passed.

- Ran directory interoperability validation after the kept code change:

  ```text
  python3 tests/interoperability/run_directory_matrix.py --readers rust stock
  ```

  Result: PASS against stock systemd `260 (260.1-2-manjaro)`.

- Re-ran Rust validation after removing the rejected facade experiment:

  ```text
  cd rust
  CARGO_HOME=../.local/cargo-home CARGO_TARGET_DIR=../.local/cargo-target cargo fmt --all
  CARGO_HOME=../.local/cargo-home CARGO_TARGET_DIR=../.local/cargo-target cargo test -p journal
  ```

  Result: `cargo test -p journal` passed, 19 tests passed.

- Re-ran directory interoperability validation after removing the rejected
  facade experiment:

  ```text
  python3 tests/interoperability/run_directory_matrix.py --readers rust stock
  ```

  Result: PASS against stock systemd `260 (260.1-2-manjaro)`.

- Ran the SOW audit:

  ```text
  .agents/sow/audit.sh
  ```

  Result: PASS.

- Ran a fresh full Rust/Go/systemd benchmark on current code after the
  non-overlap optimization:

  ```text
  CARGO_HOME=.local/cargo-home CARGO_TARGET_DIR=.local/cargo-target \
  GOCACHE=$PWD/.local/go-cache GOMODCACHE=$PWD/.local/go-modcache \
  GOPATH=$PWD/.local/go-path \
  python3 tests/benchmarks/run_reader_core_benchmarks.py \
    --languages rust,go,systemd \
    --rows 200000 \
    --directory-rows 200000 \
    --repetitions 5 \
    --warmups 1 \
    --format compact \
    --final-state offline \
    --max-size-bytes 134217728 \
    --directory-max-size-bytes 33554432 \
    --window-size 33554432 \
    --out .local/benchmarks/reader-core-rust-hotpath-profile-final \
    --keep-fixtures
  ```

  Artifact: `.local/benchmarks/reader-core-rust-hotpath-profile-final/20260529T181338Z/`.

- Current-code median rows/s from the final benchmark:

  ```text
  systemd file data                         653,312
  systemd open-files data                   634,883
  rust file sdk-payloads snapshot whole   2,761,053
  go   file sdk-payloads snapshot mmap    2,569,001
  rust file facade-data snapshot whole    2,329,531
  go   file facade-data snapshot mmap     2,467,054
  rust open-files sdk-payloads live whole 2,113,775
  go   open-files sdk-payloads live mmap  2,206,680
  rust open-files facade-data live window 2,115,699
  go   open-files facade-data live mmap   1,836,903
  ```

  Interpretation:
  - Rust is not generally slower than Go.
  - Rust beats Go in single-file SDK payload mode and ordered-directory
    facade DATA mode.
  - Go still wins slightly in single-file facade DATA and open-files SDK
    payload mode.
  - Both Rust and Go are several times faster than stock systemd DATA
    enumeration on this compact uncompressed corpus.

- User decision on reader payload lifetime:
  - selected the SDK-side row-scoped lifetime contract instead of changing
    Netdata facets to copy every field immediately;
  - rejected a mode that only copied at the facet layer because facets filters,
    search, sampling, anchoring, and pagination should not pay copies for
    fields from rows that are later rejected.

- Implemented Rust row-scoped facade payload lifetime:
  - whole-file mmap uncompressed DATA uses row-scoped borrowed mmap pointers;
  - compressed DATA is decompressed into row-scoped owned buffers;
  - windowed mmap uses row-scoped owned buffers because the window manager may
    remap or evict the backing window;
  - end-of-data for the current row no longer clears row payload storage, so
    callers can cache field pointers in the inner enumeration loop and process
    them after that loop returns `None`;
  - advancing, seeking, restarting/releasing current-entry DATA, full-entry
    materialization, and visitor APIs still invalidate the current row.

  Code evidence:
  - `rust/src/journal/src/lib.rs`: `FileReader` now stores `row_payloads` and
    `row_owned_payloads` for current-row lifetime.
  - `rust/src/journal/src/lib.rs`: `enumerate_entry_payload()` keeps row
    storage alive after end-of-data and reads through `read_row_payload()`.
  - `rust/src/crates/journal-core/src/file/file.rs`:
    `raw_data_payload_ptr_with_info_unguarded()` exposes a raw pointer only for
    callers that prove mmap window stability; the Rust SDK uses it only for
    whole-file mmap row-scoped facade enumeration.

- Added regression coverage:
  - uncompressed whole-file facade enumeration still returns the mmap-backed
    payload pointer;
  - compressed multi-field facade enumeration keeps the first returned payload
    valid after the second field and after the end-of-data result for the row.

- Ran Rust validation after the row-lifetime change:

  ```text
  cd rust
  CARGO_HOME=../.local/cargo-home CARGO_TARGET_DIR=../.local/cargo-target cargo fmt --all
  CARGO_HOME=../.local/cargo-home CARGO_TARGET_DIR=../.local/cargo-target cargo test -p journal
  CARGO_HOME=../.local/cargo-home CARGO_TARGET_DIR=../.local/cargo-target cargo test -p journal-core
  ```

  Results:
  - `cargo test -p journal`: PASS, 20 tests passed.
  - `cargo test -p journal-core`: PASS, 61 tests passed.

- Ran directory interoperability validation after the row-lifetime change:

  ```text
  python3 tests/interoperability/run_directory_matrix.py --readers rust stock
  ```

  Result: PASS against stock systemd `260 (260.1-2-manjaro)`.

- Benchmark note after row-lifetime change:
  - A full Rust/Go/systemd benchmark was started, but stopped after Rust file
    modes because it had moved into slow Go `read-at` diagnostic modes that do
    not exercise the changed Rust facade surface.
  - Replaced it with a focused Rust benchmark using the same 200k-row compact,
    uncompressed, FSS-off fixture. Artifact:
    `.local/benchmarks/reader-core-rust-row-lifetime-focused/20260529T193915Z/`.
  - The focused artifact was produced before the final split of borrowed row
    slots from owned compressed buffers, so final facade spot checks were run
    directly with `reader_core_bench` against the same fixture.

- Current row-scoped facade spot-check rows/s after the final split:

  ```text
  rust file facade-data snapshot whole-file:
    2,017,898
    1,507,772
    1,834,801
    2,045,365
    2,031,595

  rust file facade-data live whole-file:
    2,107,463
    1,933,688
    1,998,655
    1,946,396
    2,012,627
  ```

  Interpretation:
  - the row-scoped guarantee has a real cost versus the previous
    current-pointer-only facade benchmark, but whole-file mmap facade reads
    remain around 2.0M rows/s on the accepted compact uncompressed corpus;
  - windowed mmap facade enumeration is slower because row-scoped safety
    requires owned buffers when mmap window stability cannot be proven;
  - SDK visitor payload reads are unchanged and remain the faster production
    path for consumers that can use callback-scoped payloads.

- Updated `.agents/sow/specs/product-scope.md` to record the row-scoped
  current-entry facade data lifetime contract.

- Re-ran the SOW audit after the row-lifetime implementation and spec update:

  ```text
  .agents/sow/audit.sh
  ```

  Result: PASS.

- Ran whole-SOW read-only reviewer batch:
  - `llm-netdata-cloud/glm-5.1`: PRODUCTION GRADE; finding was a missing
    explicit windowed-mmap uncompressed row-lifetime test.
  - `llm-netdata-cloud/qwen3.6-plus`: PRODUCTION GRADE; same missing
    windowed-mmap uncompressed row-lifetime test, plus non-blocking suggestions
    to tighten the unsafe helper comment and clarify the debug assertion.
  - `llm-netdata-cloud/minimax-m2.7-coder`: stopped because it attempted to
    spawn additional agents despite the read-only reviewer prompt forbidding
    recursive external assistants. Partial output also noticed that Go facade
    docs still describe libsystemd-style current-pointer lifetime.
  - `llm-netdata-cloud/kimi-k2.6`: stopped as no-result after it produced no
    review output for an extended period.

- Dispositioned reviewer findings:
  - Added `facade_uncompressed_windowed_data_remains_valid_for_current_row()`
    to cover the windowed mmap + uncompressed DATA owned-buffer fallback.
  - Tightened the hidden unsafe helper documentation to explicitly forbid
    windowed-mmap use.
  - Added a message to the row-payload storage invariant `debug_assert`.
  - Updated `product-scope.md` and SOW-0009 to record that Go, Node.js, and
    Python must be brought to the same strengthened row-scoped facade contract
    before cross-language reader facade parity is claimed.

- Re-ran Rust validation after the reviewer fixes:

  ```text
  cd rust
  CARGO_HOME=../.local/cargo-home CARGO_TARGET_DIR=../.local/cargo-target cargo fmt --all
  CARGO_HOME=../.local/cargo-home CARGO_TARGET_DIR=../.local/cargo-target cargo test -p journal
  CARGO_HOME=../.local/cargo-home CARGO_TARGET_DIR=../.local/cargo-target cargo test -p journal-core
  ```

  Results:
  - `cargo test -p journal`: PASS, 21 tests passed.
  - `cargo test -p journal-core`: PASS, 61 tests passed.

- Ran a second whole-SOW read-only reviewer batch with the same scope:
  - `llm-netdata-cloud/glm-5.1`: PRODUCTION GRADE. Non-blocking notes were to
    consider a helper for clearing row storage and add an optional mixed
    compressed/uncompressed whole-file test.
  - `llm-netdata-cloud/qwen3.6-plus`: PRODUCTION GRADE. Findings were:
    replace the row-payload invariant `debug_assert` with a runtime check, add
    a mixed compressed/uncompressed whole-file test, and consider a test for
    pointer invalidation after advancing to the next row.

- Dispositioned the second reviewer batch:
  - Replaced the row-payload invariant `debug_assert` with
    `.get(index).expect("payload should be stored before returning it")`.
  - Added
    `facade_whole_file_row_handles_mixed_compressed_and_uncompressed_payloads()`
    to cover a whole-file row containing one borrowed uncompressed DATA payload
    and one owned decompressed DATA payload.
  - Rejected a literal pointer-invalidation-after-next test. Evidence: the
    public contract guarantees validity until the next row is fetched, not
    dereferenceable invalidity after that point. Testing invalidation by
    dereferencing stale pointers would be undefined behavior and testing pointer
    inequality after storage reuse would be nondeterministic, so this is better
    specified as a contract boundary than asserted as a memory access test.

- Re-ran Rust validation after the second reviewer fixes:

  ```text
  cd rust
  CARGO_HOME=../.local/cargo-home CARGO_TARGET_DIR=../.local/cargo-target cargo fmt --all
  CARGO_HOME=../.local/cargo-home CARGO_TARGET_DIR=../.local/cargo-target cargo test -p journal
  CARGO_HOME=../.local/cargo-home CARGO_TARGET_DIR=../.local/cargo-target cargo test -p journal-core
  ```

  Results:
  - `cargo test -p journal`: PASS, 22 tests passed.
  - `cargo test -p journal-core`: PASS, 61 tests passed.

- Moved this SOW to `.agents/sow/done/`, updated `SOW-status.md`, and ran the
  project SOW audit:

  ```text
  .agents/sow/audit.sh
  ```

  Result: PASS. The audit reported status/directory consistency for this SOW
  as `completed` in `.agents/sow/done/`.

## Validation

Acceptance criteria evidence:

- Satisfied for profiling and benchmark evidence:
  - baseline benchmark artifact:
    `.local/benchmarks/reader-core-rust-hotpath-profile-baseline/20260529T174259Z/`;
  - final benchmark artifact:
    `.local/benchmarks/reader-core-rust-hotpath-profile-final/20260529T181338Z/`;
  - perf artifacts:
    `.local/benchmarks/reader-core-rust-hotpath-profile-baseline/20260529T174259Z/perf/`;
  - post-optimization perf artifacts:
    `.local/benchmarks/reader-core-rust-hotpath-profile-baseline/20260529T174259Z/perf-after-nonoverlap/`.

Tests or equivalent validation:

- `cargo fmt --all` passed for Rust.
- `cargo test -p journal` passed with 19 tests.
- After the row-scoped facade lifetime change, `cargo test -p journal` passed
  with 20 tests and `cargo test -p journal-core` passed with 61 tests.
- After reviewer fixes, `cargo test -p journal` passed with 21 tests and
  `cargo test -p journal-core` passed with 61 tests.
- After the second reviewer fixes, `cargo test -p journal` passed with 22
  tests and `cargo test -p journal-core` passed with 61 tests.
- `tests/interoperability/run_directory_matrix.py --readers rust stock`
  passed against stock systemd `260 (260.1-2-manjaro)`.
- After the row-scoped facade lifetime change,
  `tests/interoperability/run_directory_matrix.py --readers rust stock`
  passed again against stock systemd `260 (260.1-2-manjaro)`.
- Full Rust/Go/systemd reader benchmark completed with checksum validation.
- A later facade fast-path experiment was rejected and removed; Rust tests and
  the directory matrix were rerun after removal.
- A focused Rust row-lifetime benchmark completed on the accepted 200k-row
  compact uncompressed fixture. Additional direct `reader_core_bench`
  spot-checks were run after the final row-storage split.
- `.agents/sow/audit.sh` passed again after the row-lifetime implementation
  and spec update.
- Final close audit passed after moving this SOW to `.agents/sow/done/`.

Real-use evidence:

- Generated compact journal fixtures were read through Rust, Go, and stock
  systemd readers. No production logs or live host journals were used.

Reviewer findings:

- Whole-SOW read-only review batch completed with two usable reviews:
  - GLM: PRODUCTION GRADE, requested an explicit windowed-mmap uncompressed
    row-lifetime test.
  - Qwen: PRODUCTION GRADE, requested the same test and minor safety-comment /
    invariant-comment tightening.
  - Both findings were implemented and revalidated.
  - Minimax was stopped for violating the no-recursive-assistants rule.
  - Kimi produced no usable review output and was stopped as no-result.
- Second whole-SOW read-only review batch completed with GLM and Qwen:
  - GLM: PRODUCTION GRADE, only non-blocking maintainability/test suggestions.
  - Qwen: PRODUCTION GRADE, requested a runtime invariant check and mixed
    compressed/uncompressed whole-file row test; both were implemented.
  - Qwen also suggested a pointer-invalidation-after-next test. This was
    rejected as unsafe/nondeterministic because the contract boundary is that
    cached row payloads must not be used after advancing; proving stale memory
    is invalid would require behavior the SDK must not expose or rely on.

Same-failure scan:

- Done for the main Rust/Go ordered-directory gap. Go had explicit
  non-overlap sequential stepping and Rust did not. Rust now has the same class
  of optimization.
- Done for Rust facade row-lifetime gaps. Tests now cover whole-file borrowed
  uncompressed DATA, windowed owned uncompressed DATA, compressed owned DATA,
  and mixed whole-file borrowed-plus-owned rows. Cross-language parity is
  intentionally tracked by SOW-0009, not closed in this Rust-only SOW.

Sensitive data gate:

- Passed for the work so far. Artifacts are generated benchmark fixtures and
  profiler outputs under `.local/`; no real customer, credential, host-journal,
  SNMP community, or private endpoint data was used.

Artifact maintenance gate:

- AGENTS.md: no update needed. Existing project rules already cover SOW
  lifecycle, row-scoped compatibility work, and reviewer routing.
- Runtime project skills: no update needed. This SOW did not add a reusable
  project workflow beyond the existing benchmark/profile harness and
  compatibility rules.
- Specs: updated `.agents/sow/specs/product-scope.md` for the row-scoped
  current-entry facade data lifetime contract.
- End-user/operator docs: no update expected.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: this child SOW is marked `completed` and moved to
  `.agents/sow/done/` with the implementation commit.
- SOW-status.md: updated to move this SOW from Current to Done.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` because the Rust facade now
  exposes a stronger row-scoped current-entry payload lifetime contract.

Project skills update:

- No project skill update needed. The existing benchmark harness and project
  compatibility/orchestration skills already describe the durable workflow.

End-user/operator docs update:

- Not updated. No end-user behavior has changed.

End-user/operator skills update:

- Not updated. No output/reference skill is affected.

Lessons:

- `core-payloads` is a diagnostic low-level cost path, not the production Rust
  performance baseline.
- Ordered rotated directories need a non-overlap sequential fast path; merge
  scanning every file on every step is unnecessary when header sequence and
  realtime ranges prove no overlap.
- Avoid optimizing `next()` by moving DATA-offset collection blindly; that
  trades next-only speed for payload-reader regressions.

Follow-up mapping:

- Implemented in this SOW:
  - Rust ordered-directory non-overlap sequential stepping.
  - Rust row-scoped facade payload lifetime for whole-file, windowed,
    compressed, and mixed current rows.
- Rejected in this SOW:
  - the combined guarded facade DATA experiment, because measurements were
    slower;
  - the lazy DATA-offset collection experiment, because it improved next-only
    traversal but regressed payload modes;
  - a literal stale-pointer invalidation test after advancing rows, because it
    would be unsafe or nondeterministic.
- Tracked by SOW-0009 before the reader phase closes:
  - Go, Node.js, and Python row-scoped current-entry facade payload lifetime
    parity with the Rust reference behavior.
- Future SOW only if a real consumer needs it:
  - explicit next-only reader mode that does not precollect DATA offsets.

## Outcome

Completed. The profiling pass found and fixed one real Rust ordered-directory
algorithmic gap and established the accepted Rust row-scoped facade payload
lifetime contract. It also identified the remaining facade DATA cost center;
the first combined-path experiment was slower and was removed instead of
shipping speculative unsafe code.

## Lessons Extracted

- Use SDK payload visitor benchmarks for production payload-reader decisions.
  Low-level full-object core benchmarks are useful, but they intentionally
  include object guard and full DATA validation overhead.
- Keep correctness work and performance work tied to profiles. The lazy-offset
  experiment showed a local win in `next`-only mode but a regression in the
  payload modes that matter for journal consumers.
- Do not assume fewer apparent passes means faster Rust. The rejected facade
  experiment removed one visible pass but added enough guard/branch/unsafe
  structure to lose throughput.

## Followup

- SOW-0009 tracks Go, Node.js, and Python row-scoped current-entry facade
  payload lifetime parity before the reader phase closes.
- A next-only reader mode is not currently scheduled; it should become a new
  SOW only if a real consumer needs traversal without payload access.
