# SOW-0042 - Writer Final Certification

## Status

Status: completed

Sub-state: completed on 2026-05-29. Rust and Go writer performance are
certified for the accepted production baseline. Node.js and Python writer
correctness is certified for the same baseline, while their writer performance
is explicitly limited and tracked by SOW-0051.

## Requirements

### Purpose

Certify writer correctness and performance across Rust, Go, Python, and Node.js
after all writer feature and parity SOWs are complete.

### User Request

The user wants writer benchmarks first, with compact format, compression
disabled, and FSS disabled as the baseline. The baseline must reflect
production settings and compare against systemd C and Netdata's current
vendored Rust behavior where applicable.

### Assistant Understanding

Facts:

- Writer performance should be measured independently from reader performance.
- The writer baseline must use fixed, explicit settings such as 128 MiB
  max-size when measuring single-file behavior.
- Directory rotation benchmarks should be separate from single-file benchmarks.
- The user-reported SNMP traps result on `v0.3.0` is strong integration
  evidence but not a substitute for controlled SDK benchmarks.

Inferences:

- This SOW should run after SOW-0037, SOW-0040, and SOW-0041 so writer API and
  behavior no longer move under the benchmark.

Unknowns:

- No unresolved unknowns for SOW-0042 after the 2026-05-29 user decision.

### Acceptance Criteria

- Writer benchmarks cover Rust, Go, Python, Node.js, and systemd C.
- Baseline writer mode is compact format, compression disabled, FSS disabled,
  explicit max-size, explicit live publication cadence, and one writer.
- Reports separately cover single-file and directory-rotation writer behavior.
- Reports include rows/sec, bytes/sec, output size, CPU time, wall time, memory
  allocation behavior where available, syscall/file-access behavior where
  available, sync/flush cadence, and validation status.
- Benchmarks compare raw full-payload append and structured field append where
  public APIs expose both.
- Generated outputs pass shared conformance and stock systemd verification
  where the selected field-name policy is systemd-friendly.
- Performance issues are profiled before optimization.
- Any residual performance gap is either fixed or explicitly accepted by the
  user with evidence.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`
- `.agents/sow/specs/product-scope.md`
- `tests/benchmarks/README.md`
- `tests/benchmarks/run_writer_core_benchmarks.py`
- `tests/benchmarks/systemd/writer_core_bench.c`
- `rust/src/internal/testcmd/writer_core_bench/src/main.rs`
- `go/internal/testcmd/writer_core_bench/main.go`
- `node/internal/testcmd/writer-core-bench.js`
- `python/cmd/writer_core_bench.py`
- `tests/datasets/performance/manifest.json`

Current state:

- SOW-0037, SOW-0040, and SOW-0041 are completed.
- The existing `writer-core` harness is a valid single-file append-loop
  benchmark surface. It pre-materializes deterministic rows before timing,
  creates the writer before timing, stops the append timer before final
  close/sync, records process timing separately, and verifies measurement
  outputs with stock `journalctl --verify --file`.
- The current `writer-core` harness records compact/no-compression/no-FSS
  mode, fixed `max_size_bytes`, hash-table bucket sizing, field count, live
  publication cadence, final state, and timer exclusions.
- The current performance dataset definition has 200,000 rows and exactly 32
  fields per row:
  4 fixed fields, 12 low-cardinality fields with 16 values each, 8
  medium-cardinality fields with 2,048 values each, and 8 high-cardinality
  fields with one value per row. For 200,000 rows this creates 32 field names
  and 1,616,580 unique full `KEY=value` payloads.
- A first attempted single-file run with 200,000 rows and
  `max_size_bytes=134,217,728` was invalid for systemd C: the helper stopped at
  104,628 rows when the journal reached exactly 134,217,728 bytes, while stock
  `journalctl --verify --file` still passed. Therefore the 200,000-row dataset
  cannot be used as a single-file 128 MiB systemd baseline. It remains valid
  for directory-rotation benchmarks where crossing 128 MiB is expected.
- The single-file benchmark gap is API-shape parity. Rust and systemd can be
  timed through raw full-payload append. Go, Node.js, and Python drivers
  currently time only their structured field APIs even though the public SDKs
  now expose raw full-payload append too.
- The directory-rotation benchmark gap is larger. No dedicated benchmark
  surface currently measures high-level `Log` directory writing and rotation
  across Rust, Go, Node.js, Python, and a systemd C file-writer reference.

Risks:

- Benchmark settings that do not match production would invalidate results.
- Optimizing writers after this SOW closes would require re-running the full
  certification matrix.
- Comparing raw payload paths against structured field paths without labelling
  them would invalidate cross-language conclusions.
- Directory writer performance can be dominated by rotation, archive rename,
  directory sync, lock handling, retention checks, or metadata injection; it
  must be measured separately from single-file direct writing.

## Pre-Implementation Gate

Status: completed

Problem / root-cause model:

- Writer performance can now be measured because the writer API and file-format
  contract are stable enough across Rust, Go, Python, and Node.js. The remaining
  risk is benchmark invalidity: if the harness silently changes max-size,
  format, live publication, API shape, timing boundary, or directory behavior,
  the resulting numbers will not represent production-relevant writer
  performance.

Evidence reviewed:

- SOW-0009 umbrella performance requirements.
- User-provided SNMP traps benchmark context.
- SOW-0037 writer closure status.
- SOW-0040 Python writer parity status.
- SOW-0041 Node.js writer parity status.
- Existing writer-core benchmark harness and per-language drivers listed in
  Analysis.
- Performance dataset manifest listed in Analysis.

Affected contracts and surfaces:

- Writer performance claims, public docs, Netdata integration readiness, and
  release notes.
- Direct-file writer benchmark commands and reports.
- High-level directory writer benchmark commands and reports.
- Raw full-payload and structured field append APIs.
- `live_publish_every_entries` performance contract.
- Directory rotation contract and active-file max-size handling.

Existing patterns to reuse:

- SOW-0014 deterministic dataset.
- SOW-0015 ingesters.
- Existing `.local/benchmarks/` result convention.
- Shared conformance and interoperability suites.
- Existing `writer-core` report JSON shape and stock `journalctl --verify`
  validation.

Risk and blast radius:

- High for Netdata integration readiness.
- High for future public benchmark claims. Invalid settings must be treated as
  failed validation, not as useful performance evidence.

Sensitive data handling plan:

- Use generated or sanitized datasets only. Do not record real logs, SNMP
  communities, customer data, credentials, bearer tokens, personal data, private
  endpoints, or production incident details.

Implementation plan:

1. Activate this SOW and update SOW status before benchmark/code work.
2. Repair the single-file benchmark harness so Go, Node.js, and Python can time
   both raw full-payload and structured field append paths where public APIs
   expose both. Keep systemd C labelled as raw full-payload only.
3. Add a dedicated directory-rotation benchmark surface for Rust, Go, Node.js,
   Python, and a systemd C reference that rotates journal files using the same
   deterministic rows and explicit max-file-size. The systemd reference is a
   controlled `JournalFile` file-writer loop, not the journald daemon, because
   daemon lifecycle behavior is outside this project scope.
4. Run the single-file writer baseline with:
   - rows: 100,000, or a smaller explicit row count if systemd C cannot fit
     100,000 rows under the 128 MiB cap;
   - fields per row: 32;
   - format: compact;
   - compression: none;
   - FSS: false;
   - final state: online for the primary performance table;
   - one writer;
   - explicit `max_size_bytes`: 134,217,728 bytes (128 MiB);
   - hash-table sizing: data buckets from the systemd v260.1 formula
     `max(max_size * 4 / 768 / 3, 2047)`, field buckets `1023`;
   - API modes: raw full-payload and structured field where available;
   - live publication cadences: `1` stock-compatible, `64` latency-tolerant
     compromise, and `0` disabled for poll/snapshot consumers.
5. Run the directory-rotation writer baseline with:
   - the 200,000-row dataset, the same format, compression, FSS, one-writer,
     and 128 MiB active max-file-size settings;
   - high-level directory writer APIs for SDKs;
   - retention disabled because retention deletion cost is outside the accepted
     SOW-0042 writer baseline;
   - lifecycle output recording created, rotated, deleted, active, and archived
     file counts.
6. Validate generated compatible outputs with stock systemd verification and
   shared interoperability checks.
7. If a production-relevant gap appears, profile before changing code. Keep
   optimization batches large enough for meaningful review.
8. Publish benchmark reports under `.local/benchmarks/` and summarize the
   durable conclusions in specs/docs/SOW artifacts without committing large
   generated journal files.

Validation plan:

- Benchmark command logs.
- Conformance/interoperability tests.
- Stock systemd verification for compatible outputs.
- Read-only reviewer passes.
- `git diff --check`.
- `.agents/sow/audit.sh` before closure.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update if benchmark process becomes durable.
- Specs: update writer performance and certification status.
- End-user/operator docs: update benchmark/API docs if public.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: close only with final report and validation.
- SOW-status.md: update when activated and closed.

Open-source reference evidence:

- systemd source evidence is through the repository-local v260.1 benchmark
  helper builder and will be recorded with the checked commit when the systemd
  helper is built.

Open decisions:

- Resolved 2026-05-29: Node.js and Python writer performance remains about
  0.9k rows/s on the production-sized directory corpus while Rust and Go are
  about 45k-47k rows/s and systemd C is about 31k-33k rows/s. The user selected
  option A: close this SOW with Rust and Go writer performance-certified,
  record Node.js and Python as correctness-certified but performance-limited,
  and track their writer optimization in SOW-0051.

## Implications And Decisions

- 2026-05-28: user agreed writer performance should be separate from reader
  performance and happen after writer feature/parity work.
- 2026-05-29: user selected option A for the residual Node.js/Python writer
  performance gap. Rust and Go writer performance are certified for the
  accepted production baseline. Node.js and Python writer correctness is
  certified for the same baseline, but writer performance is explicitly
  limited and tracked by
  `.agents/sow/pending/SOW-0051-20260529-node-python-writer-performance.md`.
- 2026-05-29: user changed external-review cadence. Future work should finish
  a complete SOW locally first, then run external reviewers against the entire
  SOW as one meaningful batch. Do not run external reviewers after small local
  edits unless the user explicitly asks for early review or a blocking
  design/security/compatibility decision needs independent read-only input.

## Plan

1. Wait for writer closure SOWs.
2. Run writer baseline.
3. Optimize based on profiles.
4. Certify and document.

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

- Record benchmark invalidation, profiler findings, reviewer findings, and
  residual performance risks before close.

## Execution Log

### 2026-05-28

- Created from the agreed SOW restructuring.
- Activated after SOW-0037, SOW-0040, and SOW-0041 completed.
- Inspected the existing writer-core harness and identified two benchmark gaps:
  all-language raw/structured API-mode coverage and missing directory-rotation
  writer benchmarks.
- Repaired the single-file writer-core benchmark drivers so Go, Node.js, and
  Python accept the same `--api-mode raw-payload|structured-field` switch as
  Rust. Smoke runs with 10 rows passed for raw and structured API modes.
- Attempted a 200,000-row single-file raw-payload run with 128 MiB max-size and
  live cadence 1. Marked the run invalid because systemd C correctly stopped
  at 104,628 rows when the file reached 128 MiB. SDK measurements from that
  failed report are useful only as diagnostic context, not as accepted
  benchmark evidence.
- Accepted single-file baseline run, raw full-payload API mode:
  `.local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-1-mmap-windowed-20260528T190348609579Z/report.json`.
  Settings: 100,000 rows, 32 fields per row, compact, no compression, FSS off,
  online final state, one writer, 128 MiB max-size, live cadence 1. Result:
  PASS. Append-loop rows/sec: systemd 35,358; Rust 47,248; Go 49,937; Node.js
  1,020; Python 950.
- Accepted single-file baseline run, structured field API mode:
  `.local/benchmarks/writer-core/compact-none-fss-off-api-structured-field-live-every-1-mmap-windowed-20260528T190753967032Z/report.json`.
  Settings matched the raw baseline except SDK `api_mode=structured-field`;
  systemd remains raw-payload by definition. Result: PASS. Append-loop
  rows/sec: systemd 37,653; Rust 50,066; Go 52,051; Node.js 992; Python 966.
- Added `tests/benchmarks/run_writer_directory_benchmarks.py` and extended the
  Rust, Go, Node.js, and Python writer-core drivers with `--surface directory`
  for high-level `Log` rotation benchmarking.
- Accepted SDK directory-rotation baseline run:
  `.local/benchmarks/writer-directory/compact-none-fss-off-directory-api-raw-payload-live-every-1-rotate-134217728-20260528T191947837005Z/report.json`.
  Settings: 200,000 rows, 32 caller fields per row, compact, no compression,
  FSS off, high-level directory writer, raw full-payload API mode, 128 MiB
  active rotation max-size, live cadence 1, retention disabled. Result: PASS.
  Every SDK produced two journal files, stock `journalctl --verify --file`
  passed for every generated file, and stock `journalctl --directory` read back
  200,000 rows. Append-loop rows/sec: Rust 48,617; Go 50,805; Node.js 955;
  Python 914.
- Added a systemd C directory reference mode to
  `tests/benchmarks/systemd/writer_core_bench.c`. The helper rotates and
  retries when the systemd `JournalFile` API refuses an append at the active
  file cap; this keeps the reference inside file-backed scope without invoking
  the journald daemon.
- Superseded all-language directory reports from
  `20260528T194046038146Z` and `20260528T195000300535Z` after reviewer review
  exposed that the systemd C reference helper was opening already-archived
  filenames and then calling `journal_file_archive()`, producing double-`@`
  filenames. Stock readers accepted them, but the naming was not the native
  systemd directory pattern.
- Accepted all-language directory-rotation baseline run after the systemd
  reference naming fix:
  `.local/benchmarks/writer-directory/compact-none-fss-off-directory-api-raw-payload-live-every-1-rotate-134217728-20260528T202753909806Z/report.json`.
  Settings matched the SDK-only directory run and added the systemd C reference.
  Result: PASS. Every implementation produced two journal files, stock
  `journalctl --verify --file` passed for every generated file, and stock
  `journalctl --directory` read back 200,000 rows. Append-loop rows/sec:
  systemd 31,480; Rust 45,242; Go 46,105; Node.js 924; Python 879.
- Accepted all-language directory-rotation structured-field run after the
  systemd reference naming fix:
  `.local/benchmarks/writer-directory/compact-none-fss-off-directory-api-structured-field-live-every-1-rotate-134217728-20260528T203710888471Z/report.json`.
  Settings matched the raw directory baseline except SDK
  `api_mode=structured-field`; systemd remains raw-payload by definition.
  Result: PASS. Every implementation produced two journal files, stock
  `journalctl --verify --file` passed for every generated file, and stock
  `journalctl --directory` read back 200,000 rows. Append-loop rows/sec:
  systemd 32,612; Rust 47,324; Go 44,816; Node.js 920; Python 864.
- Accepted single-file raw-payload live-cadence run, cadence 64:
  `.local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-64-mmap-windowed-20260528T200139204303Z/report.json`.
  Settings matched the accepted 100,000-row raw baseline except SDK
  `live_publish_every_entries=64`; systemd remains stock default. Result:
  PASS. Append-loop rows/sec: systemd 35,634; Rust 53,454; Go 59,424;
  Node.js 1,015; Python 983.
- Accepted single-file raw-payload live-cadence run, cadence 0:
  `.local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-0-mmap-windowed-20260528T200540908271Z/report.json`.
  Settings matched the accepted 100,000-row raw baseline except SDK
  `live_publish_every_entries=0`; systemd remains stock default. Result:
  PASS. Append-loop rows/sec: systemd 37,486; Rust 54,106; Go 57,482;
  Node.js 1,038; Python 980.
- Read-only reviewer round 1 completed with minimax, kimi, qwen, and glm. Qwen
  and glm returned `NOT PRODUCTION GRADE` because the SOW validation section
  was still pending and because benchmark-maintainability issues needed
  disposition. Minimax and kimi returned `PRODUCTION GRADE` while also noting
  closure/process findings.
- Fixed accepted reviewer findings in the benchmark harness: removed a dead
  Python directory benchmark config key, made the directory harness pass the
  explicit language to `bench_command()` instead of inferring systemd from the
  binary name, and changed the systemd C helper so direct mode uses
  `arg_max_size` while directory mode uses `arg_rotation_max_size` for the
  `JournalMetrics.max_size` passed to systemd.
- Targeted post-fix validation passed:
  `python3 -m py_compile tests/benchmarks/run_writer_directory_benchmarks.py python/cmd/writer_core_bench.py`,
  `tests/benchmarks/systemd/build_writer_core_bench.sh`, and
  `python3 tests/benchmarks/run_writer_directory_benchmarks.py --rows 12000 --repetitions 1 --warmups 0 --rotation-max-size-bytes 8388608 --max-size-bytes 8388608 --api-mode raw-payload`.
  The smoke report is
  `.local/benchmarks/writer-directory/compact-none-fss-off-directory-api-raw-payload-live-every-1-rotate-8388608-20260528T202015097438Z/report.json`;
  result PASS, three files for every implementation, and stock verification
  and directory readback succeeded.
- Fixed remaining kimi findings: the systemd C helper now emits actual
  `journal_files` paths, the directory benchmark now has a cross-driver
  consistency gate for hash-table buckets and max-size fields, and the systemd
  C reference now opens `system.journal` active files and relies on
  `journal_file_archive()` to produce native system archived names with the
  source, sequence id, head sequence number, and head realtime suffix.
- Targeted post-fix validation passed again after the final C/harness changes:
  `python3 -m py_compile tests/benchmarks/run_writer_directory_benchmarks.py python/cmd/writer_core_bench.py`,
  `tests/benchmarks/systemd/build_writer_core_bench.sh`, and
  `python3 tests/benchmarks/run_writer_directory_benchmarks.py --rows 12000 --repetitions 1 --warmups 0 --rotation-max-size-bytes 8388608 --max-size-bytes 8388608 --api-mode raw-payload`.
  The smoke report is
  `.local/benchmarks/writer-directory/compact-none-fss-off-directory-api-raw-payload-live-every-1-rotate-8388608-20260528T202653372185Z/report.json`;
  result PASS, three files for every implementation, no consistency failures,
  and systemd filenames use the native single-`@` archive pattern.
- Regenerated production-sized directory certification reports after the
  systemd C naming fix and directory consistency gate:
  `.local/benchmarks/writer-directory/compact-none-fss-off-directory-api-raw-payload-live-every-1-rotate-134217728-20260528T202753909806Z/report.json`
  and
  `.local/benchmarks/writer-directory/compact-none-fss-off-directory-api-structured-field-live-every-1-rotate-134217728-20260528T203710888471Z/report.json`.
  Both passed with stock per-file verify, stock directory readback, and no
  cross-driver consistency failures.
- Read-only reviewer round 2 completed with minimax, kimi, qwen, and glm.
  All four returned `PRODUCTION GRADE`. Reviewers still identified closure
  hygiene and latent benchmark-maintainability findings that needed
  disposition before the SOW could close.
- Fixed round-2 reviewer findings: removed a dead Node.js directory benchmark
  config key, changed the systemd C directory report to emit the active
  max-size actually used for directory files, and made the directory harness
  reject incomparable runs where `--max-size-bytes` differs from
  `--rotation-max-size-bytes`.
- Targeted validation passed after the round-2 cleanup:
  `python3 -m py_compile tests/benchmarks/run_writer_directory_benchmarks.py python/cmd/writer_core_bench.py`,
  `node --check node/internal/testcmd/writer-core-bench.js`,
  `tests/benchmarks/systemd/build_writer_core_bench.sh`,
  `python3 tests/benchmarks/run_writer_directory_benchmarks.py --rows 12000 --repetitions 1 --warmups 0 --rotation-max-size-bytes 8388608 --max-size-bytes 8388608 --api-mode raw-payload`,
  and the negative guard check
  `python3 tests/benchmarks/run_writer_directory_benchmarks.py --rows 1 --repetitions 1 --warmups 0 --rotation-max-size-bytes 8388608 --max-size-bytes 134217728 --languages rust`.
  The smoke report is
  `.local/benchmarks/writer-directory/compact-none-fss-off-directory-api-raw-payload-live-every-1-rotate-8388608-20260528T205800277548Z/report.json`;
  result PASS, three files for every implementation, no consistency failures,
  and stock verification and directory readback succeeded.

## Validation

Acceptance criteria evidence:

- Satisfied for the accepted SOW-0042 scope after the 2026-05-29 user decision.
- Benchmark coverage includes Rust, Go, Python, Node.js, and systemd C.
- Baseline settings are compact format, compression disabled, FSS disabled,
  explicit max-size, explicit live cadence, one writer, and generated data only.
- Accepted reports separately cover direct single-file writer behavior and
  high-level directory rotation behavior.
- Accepted reports record append rows/sec, process rows/sec, output size,
  process timing, file count, validation status, and command lines. Allocation
  and detailed profiler reports were not added for Rust and Go because they
  already exceed the systemd baseline for the production-relevant writer path.
  Node.js and Python profiling/optimization is tracked by SOW-0051.
- Raw full-payload and structured-field append modes were benchmarked for SDK
  writers. Systemd C remains raw full-payload by definition.
- Compatible outputs pass stock `journalctl --verify --file` and stock
  `journalctl --directory` readback in the accepted directory runs.
- Residual performance gap for Node.js and Python is accepted as a known
  limitation for this close and tracked by SOW-0051.

Tests or equivalent validation:

- Passed before reviewer fixes:
  `python3 -m py_compile tests/benchmarks/run_writer_core_benchmarks.py tests/benchmarks/run_writer_directory_benchmarks.py python/cmd/writer_core_bench.py`.
- Passed before reviewer fixes:
  `go test ./journal ./internal/testcmd/writer_core_bench`.
- Passed before reviewer fixes:
  `cargo test --manifest-path rust/Cargo.toml --release -p writer_core_bench`.
- Passed before reviewer fixes:
  `node --check node/internal/testcmd/writer-core-bench.js`.
- Passed before reviewer fixes:
  `tests/benchmarks/systemd/build_writer_core_bench.sh`.
- Passed before reviewer fixes:
  direct writer-core 10-row smoke across all languages and systemd,
  `.local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-1-mmap-windowed-20260528T201057735805Z/report.json`.
- Passed after reviewer fixes:
  `python3 -m py_compile tests/benchmarks/run_writer_directory_benchmarks.py python/cmd/writer_core_bench.py`.
- Passed after reviewer fixes:
  `tests/benchmarks/systemd/build_writer_core_bench.sh`.
- Passed after reviewer fixes:
  12,000-row all-language directory-rotation smoke with 8 MiB rotation,
  `.local/benchmarks/writer-directory/compact-none-fss-off-directory-api-raw-payload-live-every-1-rotate-8388608-20260528T202015097438Z/report.json`.
- Passed after round-2 reviewer cleanup:
  `python3 -m py_compile tests/benchmarks/run_writer_directory_benchmarks.py python/cmd/writer_core_bench.py`.
- Passed after round-2 reviewer cleanup:
  `node --check node/internal/testcmd/writer-core-bench.js`.
- Passed after round-2 reviewer cleanup:
  `tests/benchmarks/systemd/build_writer_core_bench.sh`.
- Passed after round-2 reviewer cleanup:
  12,000-row all-language directory-rotation smoke with 8 MiB rotation,
  `.local/benchmarks/writer-directory/compact-none-fss-off-directory-api-raw-payload-live-every-1-rotate-8388608-20260528T205800277548Z/report.json`.
- Passed after round-2 reviewer cleanup:
  divergent directory max-size guard rejected
  `--max-size-bytes 134217728 --rotation-max-size-bytes 8388608` before
  benchmark execution.
- Passed after user decision recording:
  `git diff --check` and `.agents/sow/audit.sh`.

Real-use evidence:

- Accepted stock-reader validation reports:
  `.local/benchmarks/writer-directory/compact-none-fss-off-directory-api-raw-payload-live-every-1-rotate-134217728-20260528T202753909806Z/report.json`
  and
  `.local/benchmarks/writer-directory/compact-none-fss-off-directory-api-structured-field-live-every-1-rotate-134217728-20260528T203710888471Z/report.json`.
- Each accepted directory report uses stock `journalctl --verify --file` for
  every generated file and stock `journalctl --directory` to read back exactly
  200,000 rows.
- The user-reported SNMP traps integration benchmark improved from about
  5.5k traps/s on `v0.1.0` to about 170k traps/s on `v0.3.0`; this remains
  external integration evidence, not a substitute for the controlled SDK
  benchmark reports above.

Reviewer findings:

- Qwen finding: Python directory benchmark passed a dead top-level
  `max_file_size` config key when `rotation_policy.max_file_size` already
  controls high-level rotation. Disposition: fixed by removing the dead key.
- Qwen finding: systemd C directory helper used `arg_max_size` for
  `JournalMetrics.max_size` even in directory mode. Disposition: fixed by
  passing `arg_rotation_max_size` for directory mode and preserving
  `arg_max_size` for direct mode.
- Qwen finding: directory harness inferred systemd from binary name.
  Disposition: fixed by passing the explicit language into `bench_command()`.
- Kimi finding: systemd C directory helper emitted an empty `journal_files`
  array and relied on harness fallback discovery. Disposition: fixed by
  emitting actual discovered journal file paths from the helper.
- Kimi finding: directory benchmark lacked the cross-driver consistency gate
  present in the single-file benchmark. Disposition: fixed by failing the
  directory benchmark if passing drivers disagree on data hash buckets, field
  hash buckets, max-size, or rotation max-size.
- Local finding while validating Kimi fixes: systemd C helper created
  already-archived paths and then called `journal_file_archive()`, producing
  double-`@` filenames. Disposition: fixed by opening `system.journal` active
  files and letting systemd archive them.
- Qwen and glm finding: SOW validation section was pending. Disposition:
  validation was populated, the user gap decision was recorded, and the SOW was
  closed after the clean audit.
- GLM finding: systemd C reports an empty `journal_files` array and relies on
  harness fallback discovery. Disposition: accepted as low risk because the
  harness records `journal_file_count`, discovers files from the directory, and
  verifies every discovered file with stock `journalctl`; later fixed after
  Kimi raised the same issue as medium.
- GLM finding: Node.js benchmark error output may include stack paths.
  Disposition: accepted for benchmark-only synthetic `.local/` paths; no
  benchmark report is planned for commit.
- Minimax finding: Node.js/Python performance gap still needs explicit
  acceptance or tracking before closure. Disposition: user selected option A;
  tracked by SOW-0051.
- Kimi low finding: Node.js and Python rely on directory writer defaults for
  hash bucket sizing. Disposition: fixed for the benchmark contract by making
  directory benchmarks reject divergent `max_size_bytes` and
  `rotation_max_size_bytes`; changing high-level Log constructors to expose
  independent bucket overrides is broader SDK API work and is not needed for
  the current production baseline where both values are intentionally equal.
- Kimi low finding: single-file reports retain legacy `rust_api_mode` next to
  `api_mode`. Disposition: accepted for backward report compatibility; `api_mode`
  is the canonical field.
- Round-2 GLM finding: Node.js directory benchmark still passed a dead
  top-level `maxFileSize` config key when `rotationPolicy.maxFileSize` already
  controls high-level rotation. Disposition: fixed by removing the dead key.
- Round-2 Kimi finding: systemd C directory-mode JSON could misreport
  `data_hash_table_buckets` and `max_size_bytes` when `arg_max_size` differed
  from `arg_rotation_max_size`, because directory files are opened with
  `arg_rotation_max_size`. Disposition: fixed by reporting buckets and
  `max_size_bytes` from `arg_rotation_max_size`, and by adding the directory
  harness guard that rejects incomparable divergent settings.
- Round-2 Kimi low finding: direct-mode systemd archived-path reporting uses a
  helper path that assumes sequence number 1. Disposition: not in the accepted
  performance surface because accepted single-file reports use online final
  state; no follow-up is needed for SOW-0042 unless a future scope explicitly
  certifies direct archived-state performance reports.

Same-failure scan:

- Rechecked same benchmark-maintainability patterns in affected files:
  Python dead config was localized to `python/cmd/writer_core_bench.py`;
  Node.js dead config was localized to
  `node/internal/testcmd/writer-core-bench.js`;
  language inference was localized to
  `tests/benchmarks/run_writer_directory_benchmarks.py`; systemd
  `open_journal()` callers were updated for both direct and directory modes;
  systemd `journal_files` emission and directory consistency checks were added
  after Kimi identified the same failure classes; divergent max-size handling
  is now rejected in `tests/benchmarks/run_writer_directory_benchmarks.py`.

Sensitive data gate:

- Passed so far. All benchmark datasets are synthetic deterministic fields.
  No real logs, SNMP communities, customer data, credentials, private
  endpoints, or production incident content were written to durable artifacts.
  Generated benchmark reports remain under `.local/` and are not planned for
  commit.

Artifact maintenance gate:

- AGENTS.md: no update needed; routing, repository-boundary, SOW, and
  benchmark requirements already covered the writer work. Updated to record
  the 2026-05-29 whole-SOW external-review cadence.
- Runtime project skills: `.agents/skills/project-agent-orchestration/SKILL.md`
  updated to record the whole-SOW external-review cadence.
- Specs: `.agents/sow/specs/product-scope.md` updated with current writer
  performance certification status and the SOW-0051 Node.js/Python follow-up.
- End-user/operator docs: `tests/benchmarks/README.md` updated for the new
  benchmark surfaces and API-mode behavior.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: SOW moved from pending/open to current/in-progress, then to
  done/completed for the close commit.
- SOW-status.md: updated on activation and final close.

Specs update:

- `.agents/sow/specs/product-scope.md` updated with current writer
  performance certification status and the SOW-0051 Node.js/Python follow-up.

Project skills update:

- `.agents/skills/project-agent-orchestration/SKILL.md` updated with the
  2026-05-29 whole-SOW external-review cadence.

End-user/operator docs update:

- `tests/benchmarks/README.md` updated to document writer-core,
  writer-directory, and writer-ingestion benchmark surfaces and raw vs
  structured API modes.

End-user/operator skills update:

- No end-user/operator skill exists for this benchmark surface, so no update
  was needed.

Lessons:

- A 128 MiB single-file benchmark cannot use the 200,000-row corpus; systemd
  correctly reaches the file cap first. Single-file baseline uses 100,000 rows,
  while directory-rotation baseline uses 200,000 rows.
- The benchmark harness should identify systemd from explicit language metadata,
  not from helper binary names.
- Writer certification needs explicit per-language wording when correctness
  and high-throughput performance do not have the same status.

Follow-up mapping:

- Implemented: user selected option A. Node.js/Python writer performance is a
  known limitation tracked by
  `.agents/sow/pending/SOW-0051-20260529-node-python-writer-performance.md`.

## Outcome

Completed. Rust and Go writers are performance-certified for the accepted
compact, no-compression, FSS-off direct and directory writer production
baseline. Node.js and Python writers are correctness-certified for the same
baseline, but their writer performance remains limited and is tracked by
SOW-0051.

## Lessons Extracted

- Benchmarks must keep the active-file max-size, hash-table sizing, and
  rotation cap aligned; the directory harness now rejects incomparable
  max-size/rotation-size splits.
- Rust and Go writer performance exceeded the systemd C reference on the
  accepted production baseline, so reader performance work can proceed without
  more writer changes on the Netdata-critical path.
- Node.js and Python can pass compatibility while still being far below
  systemd/Rust/Go throughput; compatibility certification and performance
  certification must be stated separately for each language.
- External reviews are most useful after a complete SOW is implemented and
  locally validated, not after small partial edits.

## Followup

- SOW-0051 tracks Node.js/Python writer profiling and optimization.

## Regression Log

None yet.
