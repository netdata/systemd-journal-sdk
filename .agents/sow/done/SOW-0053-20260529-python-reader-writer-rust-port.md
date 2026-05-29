# SOW-0053 - Python Reader And Writer Rust Port

## Status

Status: completed

Sub-state: completed after Python reader/writer Rust-port implementation,
validation, reviewer disposition, and audit.

## Requirements

### Purpose

Bring Python reader and writer behavior as close as practical to the finalized
Rust reference implementation while preserving pure-Python maintainability,
systemd journal compatibility where applicable, and clear runtime limitations.

### User Request

After Rust reader optimization, the user wants the Rust reader and writer
ported to Python before Node.js.

### Assistant Understanding

Facts:

- Python writer correctness is already certified for the accepted writer
  baseline, but performance remains limited.
- Python reader still needs alignment to the finalized Rust reader contract,
  including mmap evaluation and byte-preserving field access.
- The user wants Python before Node.js.
- SOW-0052 finalized the Rust reader hot path around mmap-backed current-entry
  DATA payload visitors/enumerators, cached current-entry DATA offsets, and
  allocation-light libsystemd facade enumeration.
- `python/journal/reader.py` currently reads non-zstd journal files into a
  single immutable `bytes` object during `FileReader.open()`.
- `python/journal/facade.py` currently implements current-entry DATA
  enumeration by materializing the full entry and copying its payload list.
- `tests/benchmarks/run_reader_core_benchmarks.py` currently benchmarks Rust
  and stock systemd reader paths, but does not provide a Python reader case.

Inferences:

- Python should be treated as one full-language port SOW instead of splitting
  reader and writer work across two mixed-language SOWs.
- Python should copy Rust API semantics where practical, but runtime-specific
  limits must be recorded instead of hidden.
- The Python reader can improve substantially without changing the public
  facade contract by mmap-backing file reads and adding a current-entry DATA
  payload path that avoids full entry materialization.
- Python writer work should be measured against the finalized Rust writer API
  and optimized only where profiler evidence shows practical pure-Python wins.

Unknowns:

- Whether Python mmap-backed reading is always faster than whole-file `bytes`
  for this workload; this SOW will measure before recording it as a performance
  claim.
- Whether Python writer performance can reach systemd/Rust/Go class without
  native extension code. This is unlikely based on previous benchmark evidence,
  but must be measured rather than assumed.

### Acceptance Criteria

- Python reader API and behavior align to the finalized Rust reader contract.
- Python writer API and behavior align to the finalized Rust writer contract.
- Python supports byte-preserving RAW field access and the same writer field
  policy layers as Rust.
- Python reader mmap is implemented or explicitly rejected with measured
  evidence.
- Python writer hot paths are profiled and optimized where practical.
- Python passes shared reader/writer conformance, mixed-directory, and relevant
  interoperability tests.
- Python single-file and ordered directory reader benchmarks are recorded.
- Python direct and directory writer benchmarks are recorded.
- Remaining Python runtime performance gaps are documented with evidence.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/pending/SOW-0046-20260528-python-node-reader-alignment.md`
- `.agents/sow/pending/SOW-0051-20260529-node-python-writer-performance.md`
- `.agents/sow/done/SOW-0040-20260528-python-writer-mmap-and-rust-parity.md`
- `.agents/sow/specs/product-scope.md`

Current state:

- Python writer correctness is in better shape than Python reader parity.
- Python writer performance was previously recorded as far below Rust/Go.
- Python reader and writer work is currently split across combined-language
  SOWs, which no longer matches the user's priority order.

Risks:

- Python runtime limitations may prevent Rust/Go-level throughput without
  native extension code.
- Combining reader and writer in one language SOW increases scope, but it also
  improves API consistency for Python consumers.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Python must follow the final Rust reference now that SOW-0052 is closed.
- The current Python reader path pays avoidable costs on hot scans:
  whole-file immutable `bytes` loading, full entry map materialization, and
  facade enumeration through copied payload lists.
- The current benchmark harness cannot prove Python reader gains because it
  lacks Python reader benchmark cases.
- The Python writer already has mmap-based correctness parity, but its public
  API and hot paths need a fresh comparison against the final Rust writer
  contract before the Python port can be called complete.

Evidence reviewed:

- `.agents/sow/done/SOW-0052-20260529-rust-reader-last-mile-optimization.md`
  and commit `0997adb`.
- `.agents/sow/done/SOW-0040-20260528-python-writer-mmap-and-rust-parity.md`.
- `.agents/sow/specs/product-scope.md` accepted reader and Python feature
  slices.
- `python/journal/reader.py`: whole-file `bytes` load, entry/data parsing,
  compression handling, and current entry API.
- `python/journal/facade.py`: libsystemd facade data enumeration path.
- `python/journal/directory_reader.py`: ordered directory merge over
  `FileReader` instances.
- `python/journal/writer.py`: mmap writer arena and structured/raw append
  surfaces.
- `tests/benchmarks/run_reader_core_benchmarks.py`: Rust/systemd reader
  benchmark coverage.
- `tests/benchmarks/run_writer_core_benchmarks.py`: all-language writer
  benchmark coverage.

Affected contracts and surfaces:

- Python public reader and writer APIs.
- Python directory reader/writer behavior.
- Python journalctl rewrite behavior where reader changes apply.
- Python benchmark and documentation surfaces.
- Shared interoperability matrices that include Python readers and writers.
- Product scope spec Python feature and limitation sections.

Existing patterns to reuse:

- Rust reader/writer reference after SOW-0052.
- Existing Python writer mmap arena from SOW-0040.
- Shared conformance and interoperability harnesses.
- Rust reader current-entry DATA payload visitor/enumerator contract.
- Existing Python directory traversal and match/filter behavior.

Risk and blast radius:

- Medium. Python API changes can affect SDK users and the Python journalctl
  rewrite. Current Netdata hot-path integrations primarily depend on Rust/Go,
  but cross-language conformance requires Python behavior to stay compatible.
- mmap-backed readers can create resource lifetime hazards if Python
  `memoryview` objects outlive the mapped file. This SOW must avoid unsafe
  public lifetime behavior or document and test it explicitly.
- Reader changes can affect directory, mixed-format, live, and journalctl query
  matrices because those all consume Python `FileReader`.

Sensitive data handling plan:

- Use generated fixtures and sanitized benchmark data only.
- Do not record real logs, SNMP communities, credentials, bearer tokens,
  customer data, personal data, private endpoints, or production incident
  details.

Implementation plan:

1. Establish Python baseline tests and writer benchmarks before behavioral
   edits.
2. Extend reader benchmarks so Python single-file and ordered directory
   reader paths are measured against Rust and stock systemd on the same
   fixtures.
3. Port the Rust reader hot-path shape to Python in idiomatic form:
   mmap-backed file buffers, current-entry DATA offset caching, raw
   current-entry payload enumeration, and facade enumeration that does not
   pre-materialize every field.
4. Preserve Python API compatibility unless a change is required for
   byte-preserving RAW access; add explicit byte-name/raw-payload APIs if the
   existing convenience maps are insufficient.
5. Compare Python writer API/options to the finalized Rust writer, fill any
   semantic gaps, profile writer hot paths, and keep only evidence-backed
   pure-Python optimizations.
6. Update docs/specs with the measured Python feature and performance status.
7. Run full validation, then run read-only whole-SOW external reviews.

Validation plan:

- `PYTHON=.local/python-venv/bin/python PYTHONPATH=python .local/python-venv/bin/python python/test_all.py`
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_directory_matrix.py`
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_mixed_directory_matrix.py`
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_live_matrix.py`
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_journalctl_query_matrix.py`
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/benchmarks/run_reader_core_benchmarks.py`
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/benchmarks/run_writer_core_benchmarks.py`
- `git diff --check`
- `.agents/sow/audit.sh`
- Read-only reviewer passes.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if a durable Python runtime policy
  changes.
- Specs: update Python feature/performance status, especially mmap reader,
  raw payload enumeration, and any remaining runtime limitations.
- End-user/operator docs: update Python README/API docs when public Python
  behavior changes.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: move this file from pending to current on activation and to
  done only with implementation, validation, review, and status updates in one
  closing commit.
- SOW-status.md: update when activated and closed.

Open-source reference evidence:

- Existing systemd reference evidence comes through Rust SOWs unless new
  compatibility-sensitive behavior is changed here.
- `systemd/systemd @ cf3156842209`
  `src/libsystemd/sd-journal/sd-journal.c:1263-1363` was checked for
  `sd_journal_seek_cursor()` behavior. It parses/stores the requested
  location and returns success without scanning for an existing entry.

Open decisions:

- None blocking closure. Any native-acceleration or narrower Python
  performance-contract work requires a separate user decision and SOW.

## Implications And Decisions

- 2026-05-29: user prioritized Python after Rust and before Node.js.

## Plan

1. Wait for Rust reader last-mile optimization closure.
2. Port final Rust reader behavior to Python.
3. Align Python writer behavior and optimize measured bottlenecks.
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

- If Python cannot meet the performance target without native code, record
  profiler evidence and ask for a product decision before claiming
  production-grade throughput.

## Execution Log

### 2026-05-29

- Created from the user's updated Rust -> Python -> Node.js priority.
- Activated after SOW-0052 closed in commit `0997adb`.
- Ported the finalized Rust reader shape to Python:
  mmap-backed file buffers, cached entry/object sizing, current-entry
  payload visitation, facade data enumeration without full-entry
  materialization, byte-preserving raw field-name maps, and ordered
  directory merge keys that avoid materializing entries for candidate
  comparison.
- Added `python/cmd/reader_core_bench.py` and wired Python cases into
  `tests/benchmarks/run_reader_core_benchmarks.py`.
- Added Python package tests for RAW byte field-name preservation, full
  payload enumeration, facade enumeration, export behavior, and UTF-8
  convenience-map separation.
- Profiled the Python writer and added a small bounded FIELD object cache
  for short field names. This removes repeated FIELD hash-table walks for
  common field names while preserving on-disk behavior.
- Measured the remaining writer bottleneck. The hot path is still dominated
  by pure-Python journal DATA hashing/linking work, so Rust/Go-class writer
  throughput is not realistic in this slice without native acceleration or a
  different performance policy.
- Addressed first-round reviewer findings:
  Python facade fallback payload construction now accepts byte field names,
  resource-owning Python objects support context-manager cleanup, raw entry
  parsing no longer depends on UTF-8 decode success, active `.journal` /
  `.journal~` readers refresh header and entry-array state at tail/end,
  non-UTF8 match field names fail fast, truncated DATA objects are rejected
  with a clear bounds error, and `SdJournal.seek_cursor()` no longer raises
  after a valid parsed cursor is not found.
- Addressed second-round reviewer findings:
  `DirectoryReader.close()` now attempts to close every underlying reader
  before raising the first close error, context managers preserve the original
  body exception when cleanup also fails, active-file refresh validates the new
  mmap/header/entry-array state before discarding the old mapping, oversized
  ENTRY objects are rejected before item parsing, and filter bugs are no
  longer swallowed by the unreadable-entry skip path.
- Addressed final Qwen reviewer hardening findings:
  the failed refresh rollback path now also resets current-entry DATA
  enumeration cache state, and `_MappedArena.resize()` now clears `_mmap`
  before fallback remap so a double-failure cannot leave a closed mmap object
  referenced as active.
- Rechecked `sd_journal_seek_cursor()` against upstream systemd source and did
  not change Python's no-existence-proof behavior:
  `systemd/systemd @ cf3156842209`,
  `src/libsystemd/sd-journal/sd-journal.c:1263` parses the cursor, stores the
  requested location, and returns `0` at line 1363 without scanning for an
  existing entry.

## Validation

Acceptance criteria evidence:

- Python reader API and behavior now align to the finalized Rust reader
  contract where Python can represent it safely:
  `FileReader.visit_entry_payloads()`, `collect_entry_payloads()`,
  `get_entry_payload()`, `entry_data_restart()`,
  `enumerate_entry_payload()`, byte-preserving `raw_fields` and
  `raw_field_values`, and facade enumeration through
  `SdJournalRestartData()` / `SdJournalEnumerateAvailableData()`.
- Python writer retains the shared raw-payload and structured-field append
  surfaces plus RAW/JOURNALD/JOURNAL-APP field policy layers, and the
  package tests plus interoperability matrices passed after the changes.
- Python normal journal files and decompressed `.journal.zst` inputs are now
  mmap-backed. Public payload APIs still return `bytes`, not borrowed
  views, to avoid unsafe lifetime semantics in Python. Normal active files
  also refresh published appends at tail/end during the same reader session.
- Python reader benchmarks were added and digest-validated against stock
  systemd data enumeration for comparable payload modes.
- Remaining runtime limitation is recorded in specs and docs: Python remains
  much slower than Rust/Go/systemd on writer throughput and slower than
  systemd/Rust on reader payload scans.

Tests or equivalent validation:

- `PYTHON=.local/python-venv/bin/python PYTHONPATH=python .local/python-venv/bin/python python/test_all.py`
  - PASS: `PASS python package tests (python/test_all.py)`.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_directory_matrix.py`
  - PASS: stock, Go, Rust, Node.js, and Python directory behavior agreed;
    systemd version `systemd 260 (260.1-2-manjaro)`.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_mixed_directory_matrix.py`
  - PASS: 72/72 checks passed for mixed regular/compact,
    uncompressed/compressed, sealed/unsealed, and whole-file `.journal.zst`
    directory fixtures; results file
    `.local/interoperability/mixed-directory-matrix-results-20260529-082000.json`.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_live_matrix.py`
  - PASS: 36/36 checks passed across regular, zstd, xz, lz4, compact,
    compact-zstd, compact-xz, compact-lz4, and sealed files; results file
    `.local/interoperability/live-feature-matrix-results-20260529-084135.json`.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_journalctl_query_matrix.py`
  - PASS: stock, Go, Rust, Node.js, and Python agreed on file/directory
    query, boot, since/until, and follow cases; systemd version
    `systemd 260 (260.1-2-manjaro)`.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/benchmarks/run_reader_core_benchmarks.py --rows 100000 --directory-rows 100000 --repetitions 1 --warmups 1`
  - PASS: digest parity for comparable Python/Rust payload modes against
    stock systemd data enumeration.
  - Run directory:
    `.local/benchmarks/reader-core/20260529T052053Z`.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/benchmarks/run_writer_core_benchmarks.py --languages python --rows 10000 --repetitions 1 --warmups 1 --api-mode raw-payload --live-publish-every-entries 0`
  - PASS: report
    `.local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-0-20260529T043810780894Z/report.json`.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/benchmarks/run_writer_core_benchmarks.py --languages python --rows 10000 --repetitions 1 --warmups 1 --api-mode structured-field --live-publish-every-entries 0`
  - PASS: report
    `.local/benchmarks/writer-core/compact-none-fss-off-api-structured-field-live-every-0-20260529T043836033175Z/report.json`.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/benchmarks/run_writer_directory_benchmarks.py --languages python --rows 10000 --repetitions 1 --warmups 1 --api-mode raw-payload --live-publish-every-entries 0`
  - PASS: report
    `.local/benchmarks/writer-directory/compact-none-fss-off-directory-api-raw-payload-live-every-0-rotate-134217728-20260529T043901768799Z/report.json`.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/benchmarks/run_writer_directory_benchmarks.py --languages python --rows 10000 --repetitions 1 --warmups 1 --api-mode structured-field --live-publish-every-entries 0`
  - PASS: report
    `.local/benchmarks/writer-directory/compact-none-fss-off-directory-api-structured-field-live-every-0-rotate-134217728-20260529T043927806822Z/report.json`.
- `git diff --check`
  - PASS.

Post-final-review hardening validation:

- `PYTHON=.local/python-venv/bin/python PYTHONPATH=python .local/python-venv/bin/python python/test_all.py`
  - PASS after failed-refresh cache reset and `_MappedArena.resize()`
    double-failure hardening.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_live_matrix.py`
  - PASS: 36/36 checks, results file
    `.local/interoperability/live-feature-matrix-results-20260529-084135.json`.
- `git diff --check`
  - PASS after final hardening changes.

Benchmark evidence:

- Python reader, single file, `sdk-payloads`, mmap, 100k rows:
  40,628 rows/s and 1,300,102 fields/s.
- Python reader, single file, facade data enumeration, mmap, 100k rows:
  28,935 rows/s and 925,940 fields/s.
- Python reader, ordered open-files, `sdk-payloads`, mmap, 100k rows:
  23,973 rows/s and 791,112 fields/s.
- Python reader, ordered open-files, facade data enumeration, mmap, 100k rows:
  22,115 rows/s and 729,821 fields/s.
- Python writer, direct raw-payload compact baseline, live publication
  disabled, 10k rows: 1,019 append rows/s.
- Python writer, direct structured-field compact baseline, live publication
  disabled, 10k rows: 1,064 append rows/s.
- Python writer, directory raw-payload compact baseline, live publication
  disabled, 10k rows: 1,057 append rows/s.
- Python writer, directory structured-field compact baseline, live publication
  disabled, 10k rows: 1,005 append rows/s.

Real-use evidence:

- The live matrix exercised Python as both writer and reader while files were
  active, with stock libsystemd live readers, stock `journalctl --file`
  verification after close, and all repository readers in final reads.
- The journalctl query matrix exercised the Python file-backed journalctl
  rewrite for file, directory, follow, boot, since, and until behavior.

Reviewer findings:

- First whole-SOW read-only review round:
  Minimax, GLM, Qwen, and Kimi found no product-blocking design issue, but
  identified hardening gaps around byte field-name facade fallback, active-file
  refresh, non-UTF8 match handling, truncated DATA bounds, context cleanup,
  seek cursor behavior, docs/spec updates, and benchmark naming. All
  substantive issues were fixed before the second round.
- Second whole-SOW read-only review round:
  Minimax and GLM returned PRODUCTION GRADE. Kimi and Qwen found real
  hardening issues in close/error handling and active-refresh rollback, plus
  one incorrect `seek_cursor` claim. The hardening issues were fixed and
  tested. The `seek_cursor` claim was rejected with upstream systemd evidence:
  `sd_journal_seek_cursor()` parses and stores the requested location; it does
  not scan for an existing entry before returning success.
- Third whole-SOW read-only review round:
  Minimax and GLM returned status summaries instead of useful final reviews,
  so they were not counted as approval. Kimi and one Qwen run became silent
  for more than ten minutes after reading files; their exact PIDs were
  stopped as hung reviewer runs. A final timeboxed Qwen pass produced one
  false HIGH finding and two valid hardening findings. The false finding
  claimed Python context managers suppress body exceptions when cleanup also
  fails; this was rejected because Python propagates the body exception when
  `__exit__` returns `False`, and `python/test_all.py` explicitly tests this
  with a body `ValueError` plus close `RuntimeError`. The valid findings were
  fixed: failed refresh rollback now resets current-entry DATA cache state,
  and `_MappedArena.resize()` now clears `_mmap` before fallback remap after a
  resize failure.

Same-failure scan:

- Raw byte field-name behavior was searched through Python package coverage
  and added where missing. The new test verifies that non-UTF8 and NUL byte
  field names stay available through raw byte APIs and are not synthesized into
  lossy string-map names.
- Reader benchmark digest validation now includes Python comparable payload
  modes, so subsequent Python reader hot-path changes must preserve field counts,
  byte counts, and payload digest parity against stock systemd.

Sensitive data gate:

- Only generated fixture data and benchmark output paths are recorded. No real
  logs, credentials, SNMP communities, customer data, personal data, private
  endpoints, or production incident details were used or written.

Artifact maintenance gate:

- AGENTS.md: no update needed; workflow and routing did not change.
- Runtime project skills: no update needed; no durable workflow rule changed.
- Specs: updated `.agents/sow/specs/product-scope.md` for Python mmap reader,
  raw byte-preserving field access, and Python facade copy semantics.
- End-user/operator docs: updated `python/README.md` for new Python reader
  APIs and mmap/raw access behavior.
- End-user/operator skills: no output/reference skills exist for this SDK
  slice.
- SOW lifecycle: this SOW is completed and will be moved to `done/` with the
  implementation commit.
- `SOW-status.md`: updated for activation and closure.

Specs update:

- Updated `.agents/sow/specs/product-scope.md`.

Project skills update:

- Not needed. The implementation did not expose a new repository workflow or
  compatibility rule that future agents need beyond existing project skills.

End-user/operator docs update:

- Updated `python/README.md`.

End-user/operator skills update:

- Not needed. No end-user/operator skill artifact exists for this SDK slice.

Lessons:

- Python mmap improves memory behavior and avoids one up-front whole-file
  `bytes` load, but Python `mmap` slicing still returns `bytes`, so current
  public payload enumeration is copy-per-field rather than true borrowed-view
  zero-copy.
- Python writer field-name caching helps repeated short field names, but the
  remaining pure-Python writer bottleneck is DATA object hashing/linking.

Follow-up mapping:

- Node.js reader/writer Rust port remains tracked by
  `.agents/sow/pending/SOW-0054-20260529-node-reader-writer-rust-port.md`.
- Further high-throughput Python writer work would require a new product
  decision because the likely path is native acceleration, CFFI, or a narrower
  performance contract, all outside the current pure-Python assumption.

## Outcome

Completed.

Python now carries the finalized Rust reader/writer public contract as far as
is practical for pure Python:

- mmap-backed normal file reads and mmap-backed decompressed whole-file
  `.journal.zst` reads;
- active-file refresh at tail/end for published appends;
- byte-preserving raw `FIELD=value` payload access and raw byte field-name
  maps;
- current-entry payload visitation/enumeration used by the libsystemd-like
  facade instead of pre-materializing full entries;
- context-manager cleanup across reader, directory reader, facade, and writer;
- writer field-name policies, raw/structured append surfaces, compression,
  compact, and sealing parity retained.

Known runtime limitation:

- Python writer throughput remains roughly 1k rows/s in the compact,
  no-compression, FSS-off baseline. The measured bottleneck is pure-Python
  DATA hashing/linking work. This is documented and not hidden as a
  production-grade throughput claim.

## Lessons Extracted

- Python mmap removes the up-front whole-file `bytes` copy and improves memory
  behavior, but Python mmap slicing still produces copied `bytes`, so Python
  cannot expose Rust-like borrowed mmap payload lifetimes safely through the
  public facade.
- Active-file refresh must validate the new mapping before dropping the old
  mapping. Failed refresh rollback also needs to clear current-entry
  enumeration state so facade users cannot continue from stale cached offsets.
- External reviewer runs can drift into status summaries or hang after reading
  large SOW surfaces. Such runs must be recorded as failed reviews, not counted
  as production-grade approval.

## Followup

- Node.js reader/writer Rust port remains tracked by
  `.agents/sow/pending/SOW-0054-20260529-node-reader-writer-rust-port.md`.
- Native-accelerated or alternative Python writer throughput work is not part
  of this SOW and requires a separate user decision and SOW.

## Regression Log

None yet.
