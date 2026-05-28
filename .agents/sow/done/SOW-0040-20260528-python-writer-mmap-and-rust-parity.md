# SOW-0040 - Python Writer Mmap And Rust Parity

## Status

Status: completed

Sub-state: completed on 2026-05-28 after implementation, validation, reviewer
disposition, and SOW audit.

## Requirements

### Purpose

Bring the Python writer as close as practical to the Rust writer contract after
Rust and Go writer closure, including API behavior, file-format behavior, and
hot-path implementation choices that are available in Python.

### User Request

The user identified Python as the next writer implementation needing mmap and
alignment with Rust after Rust and Go writer closure.

### Assistant Understanding

Facts:

- Python writer currently uses `pwrite`/`pread`-style file access rather than
  the Rust windowed mmap strategy.
- Python must expose the same writer API concepts as Rust, Go, and Node.js.
- SOW-0037 initially suspected a Python cooperative writer lock contention bug
  from a too-short lock-matrix run. A manual Python-vs-Go probe and rerun with
  a longer holder window passed, so there is no known Python lock bug at SOW
  activation time.
- Common libraries are allowed for compression; journal parsing/writing must
  remain independent of systemd/libjournal.

Inferences:

- Python may not match Rust/Go hot-path performance, but it should avoid
  unnecessary divergence where Python runtime features can support the same
  contract.

Unknowns:

- Whether Python needs a Rust-style windowed mmap strategy remains unknown
  until benchmarks show whole-file mmap is not fit for purpose.

### Acceptance Criteria

- Python writer API and options match the agreed writer contract from SOW-0037.
- Python writer supports the same field-name policy modes and raw/structured
  append semantics as the other languages.
- Python writer continues to participate in the same cooperative lock contract
  as Rust and Go, including contention rejection and stale lock cleanup.
- Python writer uses mmap or a measured alternative with evidence explaining
  any difference from Rust.
- Python writer passes shared writer conformance and interoperability tests.
- Python writer outputs remain readable by stock systemd tooling where the
  selected policy mode is systemd-friendly.
- Any performance tradeoff is measured and recorded.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `python/journal/writer.py`
- `python/journal/directory_writer.py`
- `python/test_all.py`
- `python/README.md`
- `python/cmd/writer_core_bench.py`
- `go/journal/writer.go`
- `go/journal/log.go`
- `go/journal/mmap_unix.go`
- `rust/src/crates/journal-core/src/file/mmap.rs`
- `.agents/sow/done/SOW-0037-20260527-reference-drift-audit.md`
- `.agents/sow/specs/product-scope.md`

Current state:

- Python writer is functionally capable but not yet aligned with the Rust mmap
  model. Its cooperative writer lock implementation passed the longer
  all-language lock matrix, so lock validation remains a required regression
  check rather than a known bug fix.
- Direct-file Python `Writer` exposes only structured `append(fields, opts)`;
  it lacks the raw full-payload `append_raw(payloads, opts)` layer required by
  the finalized writer API hierarchy.
- Python directory `Log` exposes only structured `append(fields, opts)` and
  appends `_SOURCE_REALTIME_TIMESTAMP` only when requested. It does not append
  indexed `_BOOT_ID=<boot-id>` DATA payloads like Rust and Go high-level
  writers.
- Python direct writer uses `os.pread` and `os.pwrite` for hot-path object and
  metadata access; no mapped arena abstraction exists yet.

Risks:

- Python mmap behavior differs by platform and may complicate flush and resize
  semantics.
- Performance work can accidentally weaken compatibility if not paired with
  conformance tests.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- SOW-0037 finalized Rust/Go writer contracts for raw and structured append,
  high-level metadata injection, field-name policies, live publication, and
  cooperative writer locking.
- Python predates the finalized direct/raw writer API and still has Python-only
  hot-path file access, so it is now behind the Rust/Go reference.
- The earlier suspected Python cooperative lock bug was not real; it came from
  a too-short lock-matrix holder window. SOW-0040 must keep lock regression
  coverage but should not spend implementation effort fixing a non-existent
  lock bug.

Evidence reviewed:

- `.agents/sow/done/SOW-0037-20260527-reference-drift-audit.md`: writer
  closure matrix and lock timing-artifact correction.
- `.agents/sow/specs/product-scope.md`: writer API hierarchy, field policy
  modes, malformed raw payload semantics, live publication contract, and lock
  validation expectations.
- `python/journal/writer.py`: structured-only direct writer, no `append_raw`,
  `os.pread`/`os.pwrite` hot path, field policy helpers already present.
- `python/journal/directory_writer.py`: structured-only high-level `Log`, no
  `_BOOT_ID` DATA injection, `_SOURCE_REALTIME_TIMESTAMP` helper present.
- `python/test_all.py`: existing Python coverage for field policies,
  compression, compact, FSS, rotation, retention, source realtime, and lock
  matrix integration through shared harnesses.
- `go/journal/writer.go` and `go/journal/log.go`: reference API shape for
  `AppendRaw` and high-level `_BOOT_ID` / `_SOURCE_REALTIME_TIMESTAMP`
  metadata injection.
- `go/journal/mmap_unix.go`: whole-file mapped arena strategy already accepted
  for Go on Unix.
- `rust/src/crates/journal-core/src/file/mmap.rs`: Rust windowed mmap
  reference and whole-file experimental strategy.

Affected contracts and surfaces:

- Python writer API, directory writer behavior, compression/FSS/compact output,
  field-name policy, binary fields, cooperative writer lock behavior, and
  benchmark claims.

Existing patterns to reuse:

- Rust writer behavior from SOW-0037.
- Go writer behavior from SOW-0037.
- Existing Python tests and shared conformance fixtures.
- Python field-policy helpers in `python/journal/writer.py`.
- Python source-realtime metadata helper in `python/journal/directory_writer.py`.
- Go whole-file mapped arena structure as a practical Python first pass.

Risk and blast radius:

- Medium. Python users may rely on current public API behavior, and mmap
  resize/flush semantics can regress compact/FSS/retention behavior if not
  covered by the existing broad Python suite.

Sensitive data handling plan:

- Use only synthetic fixtures. Do not record real logs, SNMP communities,
  customer data, personal data, credentials, bearer tokens, private endpoints,
  or production incident details.

Implementation plan:

1. Add Python direct-file raw full-payload append support, sharing the same
   object-writing path as structured append after field-policy preparation.
2. Add Python directory `Log.append_raw()` and high-level metadata injection
   for `_BOOT_ID` and `_SOURCE_REALTIME_TIMESTAMP`, matching Rust/Go behavior.
3. Add a Python mapped arena abstraction using whole-file `mmap` as the first
   strategy, because it matches Go's Unix writer and avoids a Python-specific
   window manager unless benchmarks prove it is needed.
4. Route Python writer hot-path reads/writes through the mapped arena while
   preserving fallback fd reads needed before mapping and during cleanup.
5. Update Python tests and docs for raw append, `_BOOT_ID` visibility, mmap
   behavior, and lock regression validation.
6. Run Python tests, focused interoperability matrices, all-language lock
   matrix with a sufficient holder window, and Python writer benchmarks before
   review.

Validation plan:

- Python test suite.
- Shared writer conformance suite.
- Cross-language readback and stock `journalctl --verify --file`.
- All-language lock matrix with a holder window long enough for all contenders.
- Python writer benchmark before/after mmap/raw-path changes.
- Read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if Python-specific compatibility workflow
  becomes durable.
- Specs: update only if the shipped Python behavior changes cross-language
  writer contracts beyond the existing product-scope rules.
- End-user/operator docs: update Python README/API docs for raw append and mmap
  behavior.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: move from pending to current when activated; close only after
  implementation, validation, reviewer disposition, and audit.
- SOW-status.md: update when activated and closed.

Open-source reference evidence:

- No external open-source source was checked for this planning SOW.

Open decisions:

- None now. Whole-file mmap is the first Python strategy because it matches the
  current Go writer and is simpler than a Rust-style window manager. A future
  decision is required only if benchmarks show whole-file mmap is not fit for
  purpose.

## Implications And Decisions

- 2026-05-28: user agreed Python writer parity follows Rust/Go writer closure.
- 2026-05-28: SOW-0037 follow-up validation corrected the earlier Python lock
  bug suspicion. The short-hold matrix failure was a timing artifact; a manual
  Python-vs-Go probe and longer all-language lock matrix passed.
- 2026-05-28: SOW-0040 activated with whole-file Python mmap as the first
  implementation strategy; this can be revisited only with benchmark evidence.

## Plan

1. Implement direct and directory raw append parity.
2. Implement high-level `_BOOT_ID` metadata injection.
3. Implement Python whole-file mapped arena hot path.
4. Validate, benchmark, and review.

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

- Record mmap limitations, reviewer findings, and benchmark failures in this
  SOW before changing scope.

## Execution Log

### 2026-05-28

- Created from the agreed SOW restructuring.
- Activated after SOW-0037 closed and the lock-matrix timing-artifact
  correction was committed.
- Implemented Python direct-file `Writer.append_raw()` and shared raw
  full-payload field-policy preparation.
- Implemented Python high-level `Log.append_raw()`.
- Updated Python high-level `Log.append()` and `Log.append_raw()` to prepend
  indexed `_BOOT_ID=<boot-id>` metadata and optional
  `_SOURCE_REALTIME_TIMESTAMP=<usec>`, matching the Rust/Go high-level writer
  contract.
- Implemented a Python whole-file mapped arena for direct writer hot-path
  reads and writes. Fd reads/writes remain only as fallback before mapping and
  during cleanup.
- Fixed the mmap growth ordering found by the Python suite: mmap-backed writes
  must grow the mapped arena before writing a new object because mmap cannot
  extend past the current mapping the way `pwrite` can.
- Updated Python tests for direct raw append, binary raw payloads, journal-app
  filtering, RAW-mode byte payloads, directory raw append metadata, and
  structured directory `_BOOT_ID` metadata.
- Updated `python/README.md` and `.agents/sow/specs/product-scope.md` for the
  shipped Python writer behavior.
- Benchmark baseline before this SOW:
  `PYTHONPATH=python python3 python/cmd/writer_core_bench.py --output .local/sow0040-python-baseline.journal --rows 20000 --format compact --final-state online --max-size-bytes 134217728 --live-publish-every-entries 0`
  produced `468.4632466227913` append rows/s over `42.6927835730603`
  append seconds.
- Benchmark after mmap/raw-path implementation:
  `PYTHONPATH=.local/python-deps:python python3 python/cmd/writer_core_bench.py --output .local/sow0040-python-mmap.journal --rows 20000 --format compact --final-state online --max-size-bytes 134217728 --live-publish-every-entries 0`
  produced `995.7239545368724` append rows/s over `20.085888170986436`
  append seconds.
- First reviewer pass returned four `PRODUCTION GRADE` verdicts. Findings
  were non-blocking code quality and validation improvements:
  remove dead `_read_object_header_from_fd`, avoid no-op mapped-arena resize
  syscalls, remove the unused `memoryview.tobytes()` branch, and add the
  Python equivalent of Go's structured-vs-raw byte-identity test.
- Applied the reviewer cleanup batch and added
  `test_writer_append_raw_matches_structured_bytes()`.
- Post-cleanup benchmark:
  `PYTHONPATH=.local/python-deps:python python3 python/cmd/writer_core_bench.py --output .local/sow0040-python-mmap-cleanup.journal --rows 20000 --format compact --final-state online --max-size-bytes 134217728 --live-publish-every-entries 0`
  produced `929.823050226254` append rows/s over `21.509468920063227`
  append seconds.

## Validation

Acceptance criteria evidence:

- Python writer API and options now include direct `Writer.append_raw()` and
  high-level `Log.append_raw()` in addition to structured append.
- Python field-name policy behavior for raw and structured paths is covered by
  `python/test_all.py`.
- Python cooperative writer lock behavior remains covered by the all-language
  lock matrix with a sufficient holder window:
  `.local/interoperability/lock-matrix-results-20260528-204441.json`.
- Python writer uses whole-file mmap for the direct writer hot path. Remaining
  direct `os.pread(self._fd, ...)` / `os.pwrite(self._fd, ...)` calls are the
  fallback path inside `_read_at()` / `_write_at()` when no mapping is active.
- Stock systemd compatibility evidence is recorded in the binary, compression,
  compact, and live matrices below.

Tests or equivalent validation:

- `python3 -m py_compile python/journal/writer.py python/journal/directory_writer.py python/test_all.py` passed.
- `PYTHONPATH=.local/python-deps:python python3 python/test_all.py` passed with
  `PASS python package tests (python/test_all.py)`.
- After reviewer cleanup, `python3 -m py_compile python/journal/writer.py python/test_all.py` passed.
- After reviewer cleanup,
  `PYTHONPATH=.local/python-deps:python python3 - <<'PY' ... test_writer_append_raw_matches_structured_bytes() ... PY`
  passed with `PASS focused append_raw byte identity test`.
- After reviewer cleanup, `PYTHONPATH=.local/python-deps:python python3 python/test_all.py`
  passed with `PASS python package tests (python/test_all.py)`.
- `tests/interoperability/run_binary_matrix.py --writers python --readers stock go rust node python`
  passed 13/13:
  `.local/interoperability/binary-matrix-results-20260528-204354.json`.
- `tests/interoperability/run_compression_matrix.py --writers python --readers stock go rust node python --compression zstd xz lz4 --entries 2`
  passed 54/54:
  `.local/interoperability/compression-matrix-results-20260528-204408.json`.
- `tests/interoperability/run_compact_matrix.py --writers python --readers stock go rust node python --entries 2 --compression none`
  passed 14/14:
  `.local/interoperability/compact-matrix-none-results-20260528-204417.json`.
- `tests/interoperability/run_lock_matrix.py --entries 200 --delay-ms 20`
  passed 8/8:
  `.local/interoperability/lock-matrix-results-20260528-204441.json`.
- `tests/interoperability/run_live_matrix.py --writers python --readers stock go rust node python --features regular compact zstd xz lz4 compact-zstd compact-xz compact-lz4 sealed --entries 20 --writer-delay-ms 20`
  passed 9/9:
  `.local/interoperability/live-feature-matrix-results-20260528-204504.json`.
- `git diff --check` passed.
- `.agents/sow/audit.sh` passed with clean verdict:
  `=== SOW initialization complete and clean. ===`

Real-use evidence:

- Stock `journalctl --verify --file`, stock `journalctl --file`/`--directory`
  JSON/export reads, and stock libsystemd live readers all passed through the
  shared matrices above against Python-generated files.

Reviewer findings:

- First pass:
  - `glm-5.1`: `PRODUCTION GRADE`; LOW notes on dead
    `_read_object_header_from_fd`, redundant `_ensure_arena_size`, `_post_change`
    bypassing the arena abstraction by design like Go, and no-op resize syscall.
  - `minimax-m2.7-coder`: `PRODUCTION GRADE with minor notes`; LOW/INFO notes
    on pre-mapping fd reads and archive error-path arena size asymmetry.
  - `qwen3.6-plus`: `PRODUCTION GRADE`; LOW notes on mmap constructor/error
    handling, memoryview copy, raw field-name slice copy, and validation gaps.
  - `kimi-k2.6`: `PRODUCTION GRADE`; recommended removing the `memoryview`
    branch and adding a structured-vs-raw byte-identity test.
- Disposition:
  - Removed dead `_read_object_header_from_fd`.
  - Added no-op early return in `_MappedArena.resize()`.
  - Removed the `memoryview.tobytes()` branch.
  - Added `test_writer_append_raw_matches_structured_bytes()`.
  - Kept the post-write `_ensure_arena_size()` no-op in `_object_added` because
    it matches Go's defensive pattern and is harmless.
  - Kept `_post_change()` direct `ftruncate` because it matches Go's
    same-size truncate live-reader publication pattern.
- Second pass after fixes:
  - `glm-5.1`: `PRODUCTION GRADE`; LOW notes only on the intentional
    `_post_change()` direct truncate, defensive `_ensure_arena_size()` no-op,
    double policy application matching Go, and low-priority future tests for
    mmap fallback error paths.
  - `kimi-k2.6`: stalled during the second pass after reading the full diff
    and checking Go/Rust references; stopped by exact PID after no final verdict.
    Its first pass was already `PRODUCTION GRADE`, and its actionable first-pass
    findings were fixed and revalidated.

Same-failure scan:

- Searched `python/journal/writer.py` for direct
  `os.pread(self._fd, ...)`, `os.pwrite(self._fd, ...)`,
  `_read_object_header_from_fd(self._fd, ...)`, and
  `_read_object_size_from_fd(self._fd, ...)`. Only the intended fallback
  `_read_at()` / `_write_at()` calls remain.
- After reviewer cleanup, `_read_object_header_from_fd` has been removed.

Sensitive data gate:

- Passed. Validation used synthetic fixtures only. No raw sensitive data was
  added to durable artifacts.

Artifact maintenance gate:

- `AGENTS.md`: no update needed; no workflow or responsibility change.
- Runtime project skills: no update needed; no durable workflow change beyond
  existing journal compatibility rules.
- Specs: updated `.agents/sow/specs/product-scope.md` Python writer feature
  slice.
- End-user/operator docs: updated `python/README.md`.
- End-user/operator skills: no output/reference skill exists for this SOW.
- SOW lifecycle: SOW moved from pending to current at activation and then to
  done with `Status: completed`.
- `SOW-status.md`: updated at activation and final close.

Specs update:

- Updated `.agents/sow/specs/product-scope.md`.

Project skills update:

- No project skill update needed. The existing compatibility skill already
  covers raw full-payload append, field-name policy layers, live matrices, and
  lock matrices.

End-user/operator docs update:

- Updated `python/README.md` for `Writer.append_raw()`, `Log.append_raw()`,
  high-level metadata injection, and whole-file mapped arena behavior.

End-user/operator skills update:

- No output/reference skill exists for this SOW.

Lessons:

- mmap-backed writers must grow the mapping before writing objects; the old
  `pwrite` path hid this ordering requirement by allowing writes past the
  current file end.

Follow-up mapping:

- Node.js writer parity remains tracked by SOW-0041.
- Final writer benchmark/profiling/certification remains tracked by SOW-0042.
- Reader parity and optimization remain tracked by SOW-0043 through SOW-0046.

## Outcome

Completed.

Python writer now exposes the same writer API concepts as the Rust/Go writer
contract for this slice:

- direct `Writer.append_raw()` for full `KEY=value` byte payloads;
- high-level `Log.append_raw()`;
- structured and raw high-level metadata injection for indexed `_BOOT_ID` and
  optional `_SOURCE_REALTIME_TIMESTAMP`;
- whole-file mapped arena hot path for direct writer reads/writes;
- unchanged cooperative lock contract.

Validation passed across Python unit coverage, stock systemd verification,
cross-language binary/compression/compact readback, all-language lock matrix,
and Python live writer compatibility across regular, compact, compressed, and
sealed files.

Writer-core benchmark result for the same 20k-row compact/no-compression/no-FSS
baseline improved from `468.46` append rows/s before this SOW to `929.82`
append rows/s after the final cleanup run.

## Lessons Extracted

- mmap-backed writers must grow the mapped arena before writing a new object.
  The old `pwrite` implementation hid this because writes past the current file
  extent implicitly extended the file.
- Structured-vs-raw byte identity is an important API contract test. Python now
  has the same coverage class Go already had.
- The Python direct-writer hot path can improve materially from mmap, but final
  writer certification and deeper profiling remain in SOW-0042.

## Followup

- SOW-0041 tracks Node.js writer parity.
- SOW-0042 tracks final writer benchmarks, profiling, and certification.
- SOW-0043 through SOW-0046 track reader parity and optimization.
- Low-priority future test ideas from reviewers can be considered in SOW-0042
  if writer certification needs mmap failure-path fault injection.

## Regression Log

None yet.
