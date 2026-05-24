# SOW-0007 - Python SDK And journalctl

## Status

Status: completed

Sub-state: closed after implementation, validation, repeated external review, and final audit.

## Requirements

### Purpose

Implement the Python SDK and file-backed journalctl rewrite without native journal bindings.

### Assistant Understanding

Facts:

- Python must implement the shared SDK contract without native journal bindings or system journal library linkage.
- The shared harness, Go SDK, Rust SDK, and Node.js SDK slices are complete enough to act as implementation references for this phase.
- Runtime check on this workstation returned Python `3.14.5`.
- Runtime check showed Python `compression.zstd` is present at `/usr/lib/python3.14/compression/zstd/__init__.py`.
- Official Python 3.14.5 documentation says the `compression` package and `compression.zstd` were added in Python 3.14, and that `compression.zstd` provides Zstandard file I/O plus in-memory compression/decompression while remaining an optional CPython module.

Inferences:

- Python should expose binary journal field values as `bytes` in the idiomatic API and accept `bytes`, `bytearray`, `memoryview`, or strings where writing field values is ergonomic and unambiguous.
- The initial Python implementation should be plain Python with no build step, no native journal bindings, and no dependency on system journal libraries.
- Python may use standard-library runtime modules, including `compression.zstd` when present, but must not add native journal bindings or packages that link to system journal libraries.
- Synchronous file parsing/writing is acceptable for the first conformance-compatible SDK slice; memory mapping and streaming can be considered during the benchmark/profiling phase unless correctness validation exposes a need now.

Unknowns:

- No activation-blocking unknowns remain. If implementation exposes a needed behavior that is not representable with bytes-like values, Python integers, iterators, context managers, or explicit facade functions, stop and record the concrete API issue before proceeding.

### Acceptance Criteria

- Python reader and writer expose idiomatic APIs equivalent to the shared SDK contract, plus a libsystemd-compatible reader facade unless a SOW records concrete evidence for a scoped exception.
- Python uses no native journal bindings and no system journal library linkage.
- Python passes the shared conformance suite.
- Python writer passes live one-writer/multiple-reader tests with stock `journalctl --file` and stock libsystemd readers while the writer is appending.
- Python reader passes live-read tests against files actively appended by every repository writer available at this phase, and against stock systemd writers where the environment can provide them without violating repository-boundary rules.
- Python participates in the cross-language interoperability matrix.
- Python journalctl rewrite passes file-backed/query behavior tests.
- Python journalctl implements repeated same-field OR matching and the `+` disjunction separator for file-backed behavior.
- No changes are made outside this repository.

## Analysis

Sources checked:

- Product scope spec.
- Completed Go, Rust, and Node.js SDK SOWs.
- Shared conformance and live harness contracts.
- Official Python 3.14.5 `compression.zstd` documentation.

Current state:

- SOW-0003, SOW-0004, SOW-0005, SOW-0006, SOW-0010, SOW-0011, SOW-0012, and SOW-0013 are complete.
- Go, Rust, and Node.js provide current reference behavior for reader/facade/journalctl/adapter slices.
- A placeholder `python/.gitkeep` exists; no Python package scaffold exists yet.

Risks:

- Native journal binding leakage would violate the project goal.
- Pure-Python parsing and GIL behavior can become bottlenecks.
- Live concurrency bugs can make the Python writer unreadable by stock readers until close, which is not compatible.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Python currently has no SDK package, journalctl command, adapter, reader, writer, or docs in this repository. The implementation must port the established Go/Rust/Node.js feature slice into a no-native-journal-binding Python package while preserving systemd journal file compatibility, byte-safe fields, match semantics, and live stock-reader writer compatibility.

Evidence reviewed:

- `.agents/sow/specs/product-scope.md`
- `.agents/sow/done/SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md`
- `.agents/sow/done/SOW-0004-20260523-rust-sdk-and-journalctl.md`
- `.agents/sow/done/SOW-0005-20260523-go-sdk-and-journalctl.md`
- `.agents/sow/done/SOW-0006-20260523-node-sdk-and-journalctl.md`
- `.agents/sow/done/SOW-0010-20260523-go-reader-and-journalctl-completion.md`
- `.agents/sow/done/SOW-0011-20260523-live-concurrency-compatibility-gate.md`
- `.agents/sow/done/SOW-0012-20260523-go-writer-binary-fields.md`
- `.agents/sow/done/SOW-0013-20260523-go-directory-writer-rotation-retention.md`
- `go/journal/doc.go`
- `go/adapter/main.go`
- `go/cmd/journalctl/main.go`
- `rust/README.md`
- `rust/src/journal/src/lib.rs`
- `rust/src/adapter/main.rs`
- `rust/src/cmd/journalctl/main.rs`
- `node/README.md`
- `node/src/index.js`
- `node/adapter/index.js`
- `node/cmd/journalctl/index.js`
- `tests/conformance/ADAPTER_CONTRACT.md`
- `tests/conformance/manifests/conformance-v01.json`
- `tests/conformance/live/run_live_concurrency.py`
- Local runtime evidence: `python3 --version` returned `Python 3.14.5`; `importlib.util.find_spec("compression.zstd")` found the standard-library module.
- Official docs evidence: `https://docs.python.org/3.14/library/compression.zstd.html` documents `compression.zstd` as added in Python 3.14 with `.zst` file I/O and in-memory decompression.

Affected contracts and surfaces:

- Python package API.
- Python libsystemd-style reader facade.
- Python file-backed journalctl CLI.
- Shared harness adapter.
- Dependency policy.
- Live writer command or harness integration.
- Python README/package metadata.
- Product scope spec and SOW status.

Existing patterns to reuse:

- Go SDK API/adapter/journalctl behavior as the most complete non-Rust language reference.
- Rust SDK and adapter closeout behavior for header parsing, boolean match semantics, and zstd/lz4/xz reader coverage where applicable.
- Node.js SDK package organization for language-local reader/writer/facade/adapter/journalctl/test files.
- Shared conformance adapter contract and live concurrency harness.
- Directory writer rotation/retention behavior from Go, Rust, and Node.js.
- File-backed journalctl behavior for repeated same-field OR and `+` disjunction.

Risk and blast radius:

- Native dependency leakage would violate the project goal.
- Pure Python performance may need profiling before optimization claims.
- Python integer precision avoids JavaScript-style 64-bit truncation, but struct packing/unpacking and signedness mistakes can still corrupt offsets, timestamps, sequence numbers, and object sizes.
- `bytes` and `str` boundaries can corrupt binary field values if not kept explicit.
- Pure-Python binary parsing, GIL behavior, and synchronous file I/O can become performance bottlenecks; this SOW must avoid performance claims beyond conformance/live compatibility.
- Python `compression.zstd` is available on this workstation but is an optional CPython module; implementation must fail clearly or report adapter SKIP where a runtime lacks it.
- Live concurrency bugs can make the Python writer unreadable by stock readers until close, which is not compatible.

Sensitive data handling plan:

- No sensitive runtime data expected.
- Test fixtures are committed project fixtures or generated local temporary journal files.
- Durable artifacts must avoid personal names, secrets, production endpoints, account identifiers, and raw private data.

Implementation plan:

1. Create a `python/` package scaffold with plain Python runtime code, package tests, adapter executable, journalctl executable, README, and livewriter test command.
2. Implement low-level binary helpers using `bytes`, `bytearray`, `memoryview`, `struct`, and Python integers for journal file structures, little-endian integer parsing/writing, alignment, object traversal, Jenkins lookup3 hashing, and SipHash where required.
3. Implement the current writer slice: regular non-compact uncompressed journal files, byte-safe field values, keyed hash tables, direct-file writer, high-level directory writer, rotation, retention, sync/close/reopen behavior, and live stock-reader-compatible append publication.
4. Implement the current reader slice: `.journal`, `.journal~`, `.journal.zst`, `.journal~.zst`, zstd fixture decompression through `compression.zstd`, forward/backward iteration, cursors, timestamps, binary field values, field/unique enumeration, export/json/text formatting, and match tree behavior.
5. Implement a libsystemd-style reader facade with Python function names that mirror the C/Rust/Go/Node facade semantics where practical.
6. Implement file-backed Python journalctl behavior for `--file`, `--directory`, text/json/export output, `--fields`, `--list-boots`, repeated same-field OR matches, `+` disjunction, and documented unsupported daemon-only commands.
7. Implement `python/adapter` with `run`, `list`, and `probe` subcommands for the shared manifest.
8. Wire Python writer/livewriter command into the live concurrency harness for this feature slice.
9. Update product specs, Python README, SOW validation, SOW-status, and follow-up mapping.

Validation plan:

- Shared conformance suite passes Python.
- Python package tests pass.
- Live stock-reader concurrency suite passes Python writer.
- Live repository-reader concurrency suite passes Python reader.
- Dependency audit confirms no native journal bindings.
- Journalctl fixture checks pass for full-directory JSON drain, list-boots, fields, repeated same-field OR, `+` disjunction, and unsupported `--verify`.
- Cross-language smoke checks read Go, Rust, and Node writer output where practical in this phase; full matrix remains SOW-0008.
- `python3 -m compileall` passes for the package.
- `git diff --check` passes.
- `.agents/sow/audit.sh` passes before close.

Artifact impact plan:

- Specs: update if Python exposes language-specific contract differences.
- End-user/operator docs: create Python SDK docs.
- Runtime project skills: update only if a durable Python workflow lesson is discovered.
- SOW lifecycle: active in `current/` during implementation, then move to `done/` at close.
- SOW-status.md: update when this SOW moves to current or closes.

Open-source reference evidence:

- No local mirrored open-source repositories outside the project were checked for this gate. The implementation references are the completed project-local Go, Rust, and Node.js SDK slices plus official Python documentation for standard-library zstd behavior.

Open decisions:

- Python implementation strategy is recorded for this SOW: plain Python, no native journal bindings, bytes-like binary values, Python integers for 64-bit journal values, standard-library modules allowed, and native journal-binding packages forbidden.
- Full writer compression, FSS, compact journal support, full cross-language interoperability, and benchmark/profiling optimization remain tracked by SOW-0008/SOW-0009 unless the user changes scope.

## Implications And Decisions

1. Python API and pure-Python implementation strategy
   - Current state: activated after SOW-0006 completed.
   - Decision: use a plain Python package, `bytes`/`bytearray`/`memoryview` for binary payloads, Python integers for 64-bit journal values, no native journal bindings, and standard-library runtime modules where available.
   - Implication: Python must remain free of native journal bindings while still passing shared tests.
   - Risk: pure-Python performance and GIL behavior can become bottlenecks unless benchmark work verifies real hot paths later.

## Plan

1. Activate this SOW by moving it to `current/` and setting active status.
2. Delegate Python SDK, writer, reader, facade, adapter, journalctl, docs, and validation implementation using the repository-boundary block.
3. Run independent read-only reviewers against the full SOW scope.
4. Iterate fixes and repeated full-scope reviews until reviewer findings are resolved and production-grade verdicts are reached.
5. Run shared conformance, Python package tests, live compatibility tests, audit output, and docs/spec checks before closing.

## Delegation Plan

- Implementer: `llm-netdata-cloud/minimax-m2.7-coder`, or fallback to `llm-netdata-cloud/qwen3.6-plus` then `llm-netdata-cloud/glm-5.1` if minimax fails or is unavailable.
- Reviewers: at least two from `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/glm-5.1`.
- Every prompt must include the canonical repository-boundary block from `AGENTS.md`.
- Failure handling: record implementer or reviewer model failure in this SOW, substitute only from the approved model list, rerun full-scope review after fixes, and do not close if `.agents/sow/audit.sh` fails.

## Execution Log

- 2026-05-24: Activated SOW-0007 after SOW-0006 completed and commit `3dd2a58` created the rollback point for the Node.js SDK/journalctl chunk.
- 2026-05-24: Refreshed the pre-implementation gate using completed Go, Rust, and Node.js SDK slices, shared conformance/live harness contracts, Python runtime evidence (`Python 3.14.5`), and standard-library `compression.zstd` availability.
- 2026-05-24: Committed SOW activation as `48a0998` before implementation, creating a rollback point for the Python phase.
- 2026-05-24: Minimax implementer created the initial Python package scaffold, reader, writer, directory writer, facade, adapter, journalctl command, livewriter command, and README. The run was stopped by exact verified PIDs after it looped on filter-builder debugging. Accepted useful output: initial module structure, basic writer/reader roundtrip, adapter list/probe, and compile pass. Repaired locally afterward: Python `os.pwrite()` call signatures, filter-builder single-match wrapping, adapter temporary-file bug, directory reader step/subdirectory behavior, livewriter harness arguments, directory writer active/archive naming, UUID option handling, package tests, and product-scope updates.
- 2026-05-24: Implemented the current Python SDK slice under `python/`: reader, writer, directory writer, libsystemd-style facade, journalctl CLI, conformance adapter, package test runner, README, and livewriter test command.
- 2026-05-24: First read-only review cycle was stopped after Minimax found a real zstd DATA-object blocker: `python/journal/entry.py` referenced `compression.zstd.decompress()` without importing `compression`. Fixed by routing DATA-object zstd decompression through `decompress_zst_sync()` and added `python/test_all.py` coverage that constructs a compressed DATA object in memory.
- 2026-05-24: Full read-only review cycle after the zstd fix: Minimax returned `VERDICT: PRODUCTION GRADE`; Kimi returned `VERDICT: PRODUCTION GRADE` with non-blocking findings; Qwen returned `VERDICT: NOT PRODUCTION GRADE` based on a lowercase-field claim contradicted by the cited code, and local proof/tests confirmed lowercase match and writer field names are rejected.
- 2026-05-24: Addressed concrete non-blocking review findings: added writer `fcntl.flock()` exclusive non-blocking lock before create/truncate and open-for-append, fixed zstd temp-dir cleanup on write failure, fixed livewriter ns/us delay parsing, made adapter probe report runtime `_HAS_ZSTD`, replaced Python byte-by-byte comparison with bytes equality, added lowercase rejection tests, added exclusive writer-lock tests, changed match parsing from dead `index()`/`eq < 0` logic to `find()`, used a context manager for `/etc/machine-id`, and reused the shared journal filename helper in the directory reader.
- 2026-05-24: Post-fix review cycle: Minimax returned `VERDICT: PRODUCTION GRADE`; Kimi second-cycle reviewer hung without a final verdict and was stopped by exact verified PIDs; GLM replacement review returned `VERDICT: PRODUCTION GRADE` with only low cosmetic findings, which were repaired and locally revalidated.

## Validation

Acceptance criteria evidence:

- Implemented: plain Python package, no native journal bindings, byte-safe `bytes` field values, Python integer internal 64-bit journal values, and standard-library `compression.zstd` support where present.
- Implemented: idiomatic SDK API plus libsystemd-style facade functions.
- Implemented: file-backed Python journalctl for `--file`, `--directory`, default/json/export output, `--fields`, `--list-boots`, `--head`, `--tail`, repeated same-field OR, `+` disjunction, and documented unsupported daemon-only operations including `--verify`.
- Implemented: direct `Writer` and high-level `Log` directory writer with active/archive naming, entry-count and byte-size rotation, archived file-count and byte-size retention.
- Implemented: Python livewriter command for the shared live concurrency harness.

Tests or equivalent validation:

- Passed: `python3 -m compileall python`.
- Passed: `python3 python/test_all.py`; package tests include writer/reader binary export, zstd DATA-object parsing, directory writer rotation, libsystemd-style unique binary values, shared conformance execution, lowercase field rejection, livewriter delay parsing, and exclusive writer lock behavior.
- Passed: full shared conformance manifest through `python/adapter.py run`: 15 results, 0 failures. The two accepted SKIPs are `journal-verify-sealed` and `journal-verify-corruption-detection`, both tracked by SOW-0008/FSS verification scope.
- Passed: Python journalctl full `fixtures/systemd/test-data/no-rtc` JSON drain returned 10,757 rows.
- Passed: Python journalctl `--list-boots` returned 4 rows.
- Passed: Python journalctl `--fields` returned 202 rows.
- Passed: Python journalctl repeated same-field OR check `SYSLOG_IDENTIFIER=kernel SYSLOG_IDENTIFIER=systemd` returned 6,516 rows.
- Passed: Python journalctl `+` disjunction check `SYSLOG_IDENTIFIER=kernel + SYSLOG_IDENTIFIER=systemd` returned 6,516 rows.
- Passed: direct file-writer stock compatibility smoke created `.local/python-stock-smoke.journal`; stock `journalctl --verify --file` returned PASS and stock `journalctl --file --output=json` read 10 rows.
- Passed: directory writer stock-read validation wrote 5 entries with `source=netdata-test` and `machine_id=00112233445566778899aabbccddeeff`; stock `journalctl --directory` read 3 entries while `Log` was still open and 5 after close; Python `DirectoryReader` read ordered `LIVE_SEQ` values `000000` through `000004`.
- Passed: dependency/native marker audit found no imports of `systemd`, `systemd.journal`, `ctypes`, or `cffi` in Python package code. Runtime `cffi` exists on the workstation but the package does not import it.
- Passed: `git diff --check`.
- Passed: `.agents/sow/audit.sh`.

Real-use evidence:

- Passed: shared live concurrency harness with systemd `260 (260.1-2-manjaro)`, 100 entries, 2 stock polling `journalctl --file` readers, 1 stock `journalctl --file --follow --no-tail --boot=all` reader, 1 stock libsystemd reader, ordered `LIVE_SEQ`, and final `journalctl --verify --file` PASS.

Reviewer findings:

- Minimax first focused review found the zstd DATA-object blocker and the implementation was repaired before close.
- Minimax full review after zstd repair returned `VERDICT: PRODUCTION GRADE`.
- Kimi full review after zstd repair returned `VERDICT: PRODUCTION GRADE`; non-blocking findings were either repaired or tracked by existing interoperability/performance SOWs.
- Qwen full review reported `VERDICT: NOT PRODUCTION GRADE` for an asserted lowercase-field acceptance bug. Disposition: false positive. Evidence: `python/journal/hash.py` and `python/journal/writer.py` only accept `_`, `A-Z`, and `0-9` after a non-digit first-character check; local proof and `python/test_all.py` confirm lowercase match and writer field names are rejected.
- Minimax post-fix review returned `VERDICT: PRODUCTION GRADE`.
- Kimi post-fix rerun hung without a final verdict and was stopped by exact verified PIDs after silence; GLM was substituted as the second post-fix reviewer.
- GLM post-fix review returned `VERDICT: PRODUCTION GRADE`; its low cosmetic findings were repaired and final local validation still passed.

Same-failure scan:

- Completed for repaired failure classes before review and close: searched for wrong `os.pwrite()` call patterns, malformed temporary-file `.name()` calls, native journal binding imports, `cffi`/`ctypes` imports, personal-name strings, match expression failures, directory reader missing `step()`, livewriter argument mismatch, directory writer active/archive naming mismatches, direct `compression.zstd` references outside `python/journal/compress.py`, lowercase field acceptance, missing writer file locks, and stale duplicate journal filename helpers.

Sensitive data gate:

- Passed pre-review scan by inspection and `rg`: durable artifacts added for Python contain only synthetic fixture paths, `.local` scratch examples, and public SDK documentation. No raw secrets, credentials, bearer tokens, SNMP communities, community member names, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details were introduced.

Artifact maintenance gate:

- AGENTS.md: no update needed; repository-wide workflow and compatibility guardrails already cover the Python slice.
- Runtime project skills: no update needed; local repairs did not expose a durable workflow rule beyond existing orchestration and compatibility skills.
- Specs: updated `.agents/sow/specs/product-scope.md` with the current Python writer, reader, journalctl, and limitation slice.
- End-user/operator docs: added `python/README.md`.
- End-user/operator skills: no output/reference skill is produced or consumed by this SOW.
- SOW lifecycle: closed with `Status: completed` and moved to `done/` with implementation and artifact updates in the same commit.
- SOW-status.md: updated for Python closeout.

Specs update:

- Updated `.agents/sow/specs/product-scope.md`.

Project skills update:

- No update needed; existing project skills already cover the required workflow.

End-user/operator docs update:

- Added `python/README.md`.

End-user/operator skills update:

- No update needed; no end-user/operator skill artifact exists for this SDK slice.

## Outcome

Implementation completed the current Python SDK and file-backed journalctl slice.

## Lessons Extracted

- Python `os.pwrite()` uses `os.pwrite(fd, buffer, file_offset)`, unlike Node's explicit buffer-offset/length file write API; mechanical porting from Node is risky here.
- Python match-builder tests need to assert exact match counts; a script that prints `ok=False` but exits 0 is not validation evidence.
- Directory writer compatibility must validate both the active open file and archived close state with stock `journalctl --directory`.

## Followup

Full writer compression, FSS, compact journal support, full cross-language interoperability, and benchmark/profiling optimization are tracked by SOW-0008/SOW-0009.

## Regression Log

None yet.
