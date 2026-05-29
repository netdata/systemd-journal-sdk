# SOW-0054 - Node.js Reader And Writer Rust Port

## Status

Status: completed

Sub-state: activated after SOW-0052 closed in commit `0997adb` and SOW-0053
closed in commit `fe98be5`. This SOW supersedes the Node.js part of the
earlier combined Python/Node reader and writer follow-ups.

## Requirements

### Purpose

Bring Node.js reader and writer behavior as close as practical to the finalized
Rust reference implementation after Python is complete, while preserving the
project's non-native-runtime policy and documenting any Node.js runtime limits.

### User Request

After Rust reader optimization and Python reader/writer porting, the user wants
the Rust reader and writer ported to Node.js.

### Assistant Understanding

Facts:

- Node.js writer correctness is already certified for the accepted writer
  baseline, but performance remains limited.
- Node.js reader needs alignment to the finalized Rust reader contract.
- Node.js currently has no accepted native mmap dependency path in the SDK
  runtime.
- The user wants Node.js after Python.

Inferences:

- Node.js should be treated as a full-language port after Python, so Python
  lessons can be reused before tackling Node runtime constraints.
- Node.js mmap alternatives should be investigated, but a native addon should
  not be introduced without an explicit user policy decision.

Unknowns:

- Whether a maintainable, non-native Node.js mmap path exists.
- Whether Node.js writer performance can reach systemd/Rust/Go class without a
  native addon.

### Acceptance Criteria

- Node.js reader API and behavior align to the finalized Rust reader contract.
- Node.js writer API and behavior align to the finalized Rust writer contract.
- Node.js supports byte-preserving RAW field access and the same writer field
  policy layers as Rust.
- Node.js mmap/runtime options are investigated and either implemented or
  explicitly rejected with evidence.
- Node.js writer hot paths are profiled and optimized where practical.
- Node.js passes shared reader/writer conformance, mixed-directory, and
  relevant interoperability tests.
- Node.js single-file and ordered directory reader benchmarks are recorded.
- Node.js direct and directory writer benchmarks are recorded.
- Remaining Node.js runtime performance gaps are documented with evidence.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/pending/SOW-0046-20260528-python-node-reader-alignment.md`
- `.agents/sow/pending/SOW-0051-20260529-node-python-writer-performance.md`
- `.agents/sow/done/SOW-0041-20260528-node-writer-rust-parity.md`
- `.agents/sow/specs/product-scope.md`

Current state:

- Node.js writer correctness is in better shape than Node.js reader parity.
- Node.js writer performance was previously recorded as far below Rust/Go.
- Node.js reader/writer work is currently split across combined-language SOWs,
  which no longer matches the user's priority order.

Risks:

- Node.js runtime file I/O and Buffer handling may impose a lower performance
  ceiling than Rust/Go.
- Native addon dependencies could violate the current SDK policy unless the
  user explicitly changes it.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Node.js must now inherit the final Rust reference and the Python porting
  lessons.
- Current Node.js reader uses whole-file `readFileSync()` Buffers, materializes
  full entries for facade DATA enumeration, has no active-file refresh at
  tail/end, and does not yet expose the Python/Rust byte-preserving RAW reader
  surface.
- Current Node.js facade `seekCursor()` still throws when a valid parsed cursor
  is not found. SOW-0053 verified against `systemd/systemd @ cf3156842209`,
  `src/libsystemd/sd-journal/sd-journal.c:1263-1363`, that
  `sd_journal_seek_cursor()` parses/stores the requested location and returns
  success without proving the entry exists.
- Current Node.js directory merge materializes full entries for candidate
  ordering; Rust/Python now have lighter current-entry key paths.
- Node.js has no accepted non-native mmap runtime path. Reader improvements
  should therefore focus on Buffer/positional-I/O hot paths unless a
  maintainable non-native alternative is found.

Evidence reviewed:

- `.agents/sow/done/SOW-0052-20260529-rust-reader-last-mile-optimization.md`
  and commit `0997adb`.
- `.agents/sow/done/SOW-0053-20260529-python-reader-writer-rust-port.md`
  and commit `fe98be5`.
- `.agents/sow/done/SOW-0041-20260528-node-writer-rust-parity.md`.
- `.agents/sow/specs/product-scope.md`.
- `node/src/lib/reader.js`: whole-file Buffer reader, entry/data parsing,
  filter behavior, no active refresh.
- `node/src/lib/directory-reader.js`: ordered directory merge and current
  full-entry candidate materialization.
- `node/src/facade.js`: libsystemd-like facade, materialized DATA
  enumeration, throwing `seekCursor()`.
- `node/src/lib/writer.js`: direct writer raw/structured append surfaces and
  writer options.
- `node/test/all.js`: package-level Node tests.

Affected contracts and surfaces:

- Node.js public reader and writer APIs.
- Node.js directory reader/writer behavior.
- Node.js journalctl rewrite behavior where reader changes apply.
- Node.js benchmark and documentation surfaces.

Existing patterns to reuse:

- Rust reader/writer reference after SOW-0052.
- Python porting lessons from SOW-0053: byte-preserving RAW reader maps,
  current-entry payload enumeration, active-file refresh, context-manager
  cleanup semantics, failed-refresh rollback hardening, and explicit runtime
  limitation documentation.
- Existing Node.js writer correctness implementation from SOW-0041.
- Shared conformance and interoperability harnesses.

Risk and blast radius:

- Medium. Node.js API changes can affect SDK users, but runtime dependency
  choices may have larger maintainability and portability implications.

Sensitive data handling plan:

- Use generated fixtures and sanitized benchmark data only.
- Do not record real logs, SNMP communities, credentials, bearer tokens,
  customer data, personal data, private endpoints, or production incident
  details.

Implementation plan:

1. Compare Node.js reader/writer public APIs against Rust and Python.
2. Investigate Node.js mmap/runtime options with current package metadata while
   keeping any cache/output under `.local/`; do not add native runtime addon
   loading without a user decision.
3. Port reader parity:
   current-entry DATA offsets/payload enumeration, byte-preserving raw field
   maps, non-UTF8 match rejection, bounds checks, active-file refresh, lighter
   directory merge keys, and facade enumeration/get-data fast paths.
4. Align facade behavior with systemd/Python/Rust, especially
   no-existence-proof `seekCursor()` and raw bytes output behavior.
5. Compare Node.js writer API/options to Rust/Python, profile writer hot paths,
   and keep only evidence-backed JavaScript optimizations.
6. Update docs/specs and benchmark harnesses.
7. Run full validation, then read-only whole-SOW external reviews.

Validation plan:

- `node node/test/all.js`
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_directory_matrix.py`
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_mixed_directory_matrix.py`
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_live_matrix.py`
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_journalctl_query_matrix.py`
- Node.js reader benchmarks added to or run through
  `tests/benchmarks/run_reader_core_benchmarks.py`.
- Node.js writer direct and directory benchmarks.
- `git diff --check`
- `.agents/sow/audit.sh`
- Read-only whole-SOW reviewer passes.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if a durable Node.js runtime policy
  changes.
- Specs: update Node.js feature/performance status.
- End-user/operator docs: update Node.js README/API docs.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: activate after SOW-0053.
- SOW-status.md: update when activated and closed.

Open-source reference evidence:

- Existing systemd reference evidence comes through Rust SOWs unless new
  compatibility-sensitive behavior is changed here.

Open decisions:

- None blocking implementation. Native Node.js runtime addon loading remains
  disallowed by project policy unless the user makes a separate explicit
  decision.

## Implications And Decisions

- 2026-05-29: user prioritized Node.js after Rust and Python.

## Plan

1. Wait for Rust reader and Python full-language port closure.
2. Port final Rust reader behavior to Node.js.
3. Align Node.js writer behavior and optimize measured bottlenecks.
4. Validate, review, commit, and push.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current user routing.

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

- If Node.js cannot meet the performance target without native code, record
  profiler evidence and ask for a product decision before claiming
  production-grade throughput.

## Execution Log

### 2026-05-29

- Created from the user's updated Rust -> Python -> Node.js priority.
- Activated after SOW-0053 closure and push.
- Implemented Node.js reader parity with the Rust/Python reader contract:
  active-file refresh at tail/end, byte-preserving RAW field names,
  current-entry DATA payload enumeration, raw `getRaw()`/`getRawValues()`,
  lighter directory merge keys, directory reader `next()`/`previous()` aliases,
  close-all-before-raise cleanup, DATA/ENTRY declared-size bounds checks, and
  libsystemd-style no-existence-proof `seekCursor()`.
- Updated the Node.js facade to enumerate current-entry payloads without full
  entry materialization when the reader supports it and to support byte field
  names in `getData()`.
- Added a small FIELD object cache to the Node.js writer. This is a bounded
  optimization for repeated field names and does not change file layout.
- Investigated npm mmap package candidates with cache under `.local/npm-cache`.
  `mmap-io@1.1.7` depends on `bindings`/`nan`; `node-mmap-io` is not in npm;
  npm search showed mmap candidates are native binding/N-API addon packages or
  unrelated packages. No mmap dependency was added.
- Added `node/cmd/reader_core_bench.js` and wired Node.js into
  `tests/benchmarks/run_reader_core_benchmarks.py`.
- Updated `.agents/sow/specs/product-scope.md` and `node/README.md` with the
  Node.js reader/raw/facade behavior and the no-native-mmap runtime limit.
- First read-only review round found no blockers. Valid low-risk hardening
  notes were fixed: facade `getData()` fallback now handles non-UTF8 Buffer
  field names through `rawFieldValues` when `getEntryPayload()` is unavailable,
  and `_readCurrentHeader()` now parses only bytes actually read.
- Same-scope re-review after those fixes returned PRODUCTION GRADE from
  Minimax, Kimi, Qwen, and GLM.

## Validation

Acceptance criteria evidence:

- Node.js reader API/behavior alignment:
  `node/src/lib/reader.js` now exposes current-entry raw payload scanning,
  byte-preserving RAW field access, active refresh, and cursor/timestamp
  behavior aligned with Python/Rust where practical.
- Node.js writer API/behavior alignment:
  existing raw/structured append, compact/compression/FSS, field policy, live
  publication, and directory writer contracts remain covered by
  `node node/test/all.js` plus the live/mixed/directory matrices.
- Byte-preserving RAW field access:
  `node/test/all.js` adds invalid-UTF8 and NUL-containing field-name coverage
  for `entry.rawFields`, `entry.rawFieldValues`, `reader.getRaw()`,
  `reader.getRawValues()`, facade DATA enumeration, and export output.
- Node.js mmap/runtime options:
  npm evidence recorded in the execution log; no non-native mmap path was
  accepted, so the reader uses Buffer plus header-refresh reads.
- Node.js performance evidence:
  reader benchmark report
  `.local/benchmarks/reader-core/20260529T060127Z/summary.json`;
  direct writer report
  `.local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-0-20260529T060339841823Z/report.json`;
  directory writer report
  `.local/benchmarks/writer-directory/compact-none-fss-off-directory-api-raw-payload-live-every-0-rotate-134217728-20260529T060821117947Z/report.json`.

Tests or equivalent validation:

- `node node/test/all.js`: PASS.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_directory_matrix.py`: PASS.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_mixed_directory_matrix.py`: PASS, 72/72; result
  `.local/interoperability/mixed-directory-matrix-results-20260529-085950.json`.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_live_matrix.py`: PASS, 36/36; result
  `.local/interoperability/live-feature-matrix-results-20260529-090038.json`.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_journalctl_query_matrix.py`: PASS.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/benchmarks/run_reader_core_benchmarks.py --rows 100000 --directory-rows 100000 --repetitions 1 --warmups 1 --format compact --final-state online --max-size-bytes 134217728 --directory-max-size-bytes 33554432`: PASS.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/benchmarks/run_writer_core_benchmarks.py --languages node --rows 100000 --repetitions 1 --warmups 1 --format compact --final-state online --max-size-bytes 134217728 --api-mode raw-payload --live-publish-every-entries 0`: PASS.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/benchmarks/run_writer_directory_benchmarks.py --languages node --rows 100000 --repetitions 1 --warmups 0 --format compact --max-size-bytes 134217728 --rotation-max-size-bytes 134217728 --api-mode raw-payload --live-publish-every-entries 0`: PASS.
- `node --check node/src/lib/reader.js`, `node --check node/src/lib/directory-reader.js`, `node --check node/src/facade.js`, `node --check node/src/lib/writer.js`, `node --check node/cmd/reader_core_bench.js`, and `node --check node/test/all.js`: PASS.
- `python3 -m py_compile tests/benchmarks/run_reader_core_benchmarks.py`: PASS.
- `git diff --check`: PASS.

Real-use evidence:

- Stock `journalctl --file`/`--directory`, stock libsystemd live readers, Rust,
  Go, Python, and Node.js readers all passed the live matrix against Node.js
  writer outputs for regular, zstd, xz, lz4, compact, compact+compression, and
  sealed slices.
- File-backed Node.js journalctl passed directory, mixed-directory, and query /
  follow matrices against stock expectations.

Benchmark evidence:

- Node.js single-file reader, compact 100k rows / 32 fields:
  `sdk-payloads` 90,412 rows/s, 2.89M fields/s; `facade-data` 90,116 rows/s,
  2.88M fields/s.
- Node.js open-files reader, compact 100k rows / 33 fields:
  `sdk-payloads` 57,609 rows/s, 1.90M fields/s; `facade-data` 55,701 rows/s,
  1.84M fields/s.
- Node.js direct writer, compact 100k rows / 32 fields, raw-payload,
  live-publication disabled: 933 append rows/s.
- Node.js directory writer, compact 100k rows / 32 fields, raw-payload,
  live-publication disabled: 934 append rows/s.
- Remaining runtime gap: Node.js reader is substantially slower than Rust but
  faster than Python for the measured payload/facade paths. Node.js writer
  throughput remains far below Rust/Go/systemd-class performance; no native
  mmap or native journal library path was added in this SOW.

Reviewer findings:

- First round:
  - GLM: PRODUCTION GRADE. No blockers; noted only minor optimization and
    coverage observations.
  - Minimax: PRODUCTION GRADE. No concrete blockers; noted `getRawValues()`
    materialization and other low-severity observations.
  - Qwen: PRODUCTION GRADE. Flagged a latent facade `getData()` fallback issue
    for non-UTF8 Buffer field names if a future reader lacks
    `getEntryPayload()`. Fixed in `node/src/facade.js` and covered in
    `node/test/all.js`.
  - Kimi: PRODUCTION GRADE. Flagged truncated active-file header parsing as a
    low-severity hardening item. Fixed in `node/src/lib/reader.js`.
- Same-scope re-review after fixes:
  - GLM: PRODUCTION GRADE. Verified the fallback and truncated-header fixes.
  - Minimax: PRODUCTION GRADE. Verified all three fix notes and found no
    blockers.
  - Kimi: PRODUCTION GRADE. Found no blockers; remaining observations were
    documented Node.js runtime limitations or non-hot-path performance notes.
  - Qwen: PRODUCTION GRADE. Verified active refresh, RAW byte fields,
    directory ordering, no-throw `seekCursor()`, parser bounds checks,
    benchmark JSON output, and both fixes.

Same-failure scan:

- The prior "reader materializes full entry for facade DATA" pattern was
  searched and replaced in Node.js facade paths with reader current-entry
  payload methods where available.
- The prior "directory merge materializes full entry before ordering" pattern
  was changed to use `currentEntryKey()` before full entry reads, except when
  filters require full entry materialization.
- DATA and ENTRY declared-size bounds checks were added to shared parse helpers
  and covered by package tests.

Sensitive data gate:

- Planning text contains no raw sensitive data.
- Validation used generated fixtures and benchmark corpora only. No real logs,
  SNMP communities, credentials, bearer tokens, customer data, personal data,
  private endpoints, or production incident details were written to durable
  artifacts.

Artifact maintenance gate:

- `AGENTS.md`: no update required. No project-wide workflow, responsibility, or
  guardrail changed.
- Runtime project skills: no update required. No durable compatibility or
  orchestration workflow changed.
- Specs: `.agents/sow/specs/product-scope.md` updated with the Node.js feature
  slice, raw byte reader surface, active-refresh behavior, facade DATA fast
  path, and no-native-mmap runtime limit.
- End-user/operator docs: `node/README.md` updated with Node.js reader behavior,
  raw byte field APIs, active refresh, and runtime limitations.
- End-user/operator skills: no output/reference skill exists for this SDK
  surface, so no update required.
- SOW lifecycle: SOW-0054 activated from pending to current and is ready to
  close after audit.
- `SOW-status.md`: updated for activation; final close update is part of this
  SOW completion commit.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` to describe current Node.js
  reader/writer reality. No aspirational claims were added.

Project skills update:

- No project skill update required. The work followed existing orchestration and
  journal compatibility rules; no reusable workflow changed.

End-user/operator docs update:

- Updated `node/README.md` for the new reader raw-byte APIs, current-entry
  payload scanning, active-file refresh behavior, and no-native-mmap
  limitation.

End-user/operator skills update:

- No end-user/operator skills are shipped for the Node.js SDK surface.

Lessons:

- Node.js can match the reader/facade compatibility contract without native
  mmap, but the accepted runtime path requires whole-file Buffer reloads on
  structural active-file changes.
- Reviewer passes should cover fallback paths that are not hit by current
  reader implementations, because those paths become future extension points.

Follow-up mapping:

- Node.js whole-file active refresh and directory-wide unique/field enumeration
  performance are documented runtime limitations, not blockers for this SOW.
  Future optimization requires a separate SOW if a maintainable non-native mmap
  or incremental file-window strategy becomes available.
- Node.js writer throughput remains far below Rust/Go/systemd-class writers and
  is documented in this SOW and product scope. No untracked follow-up is left;
  performance-sensitive consumers should use Rust or Go unless a future Node.js
  performance SOW is explicitly opened.

## Outcome

Completed. Node.js now carries the finalized reader/writer compatibility slice
where practical under the no-native-runtime policy: byte-preserving RAW field
access, active-file refresh, current-entry payload scanning, libsystemd-like
facade DATA fast paths, no-existence-proof `seekCursor()`, parser bounds
hardening, reader benchmarks, writer benchmark evidence, and updated docs/specs.

## Lessons Extracted

- Preserve raw byte field names at the reader core and let UTF-8 convenience
  maps be a lossy view. This keeps RAW mode usable without contaminating the
  string-keyed API.
- When Node.js cannot use mmap without native addons, correctness and API
  parity are still achievable, but large-file refresh and writer throughput
  limits must be documented explicitly.

## Followup

No untracked follow-up. Future Node.js mmap/incremental-window or writer
throughput work requires a new SOW if it becomes a product priority.

## Regression Log

None yet.
