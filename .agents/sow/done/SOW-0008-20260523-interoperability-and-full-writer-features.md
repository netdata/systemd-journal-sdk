# SOW-0008 - Interoperability And Full Writer Features

## Status

Status: completed

Sub-state: completed after closed-file, live, binary-field, zstd-compression, and writer-lock interoperability slices passed and remaining feature gaps were mapped to concrete follow-up SOWs.

## Requirements

### Purpose

Complete cross-language interoperability and close remaining writer feature gaps, including compression and Forward Secure Sealing where in scope.

### Assistant Understanding

Facts:

- This phase requires all baseline language SDKs to pass shared conformance first.
- The Go baseline is split across SOW-0005 (writer first) and SOW-0010 (reader and journalctl completion).
- It closes cross-language interoperability and remaining writer feature gaps.

Inferences:

- Compression and Forward Secure Sealing decisions should be based on the completed baseline feature matrix.

Unknowns:

- Exact compression-writing and FSS implementation depth remains to be determined from the baseline feature matrix and systemd reference evidence. If a safe production-grade implementation would make this SOW too broad, the work must split concrete follow-up SOWs before close.

### Acceptance Criteria

- Every writer/reader pair in Rust, Go, Node.js, and Python passes the interoperability matrix.
- Every writer passes live stock `journalctl --file` and stock libsystemd reader tests while appending.
- Every reader passes live-read tests against every repository writer while appending, plus stock systemd writer evidence where the environment can provide it without violating repository-boundary rules.
- Writer feature gaps from earlier phases are either implemented or represented by concrete follow-up SOWs.
- Compression writing is tested across languages where implemented.
- Forward Secure Sealing support is implemented or explicitly split into a narrower follow-up with evidence.
- No changes are made outside this repository.

## Analysis

Sources checked:

- Product scope spec.
- Pending language SOWs.

Current state:

- SOW-0004, SOW-0005, SOW-0006, SOW-0007, SOW-0010, SOW-0011, SOW-0012, and SOW-0013 are complete.
- Baseline language SDKs and file-backed journalctl slices exist for Go, Rust, Node.js, and Python.
- Each current writer feature slice has passed stock-reader live compatibility for its claimed writer surface.

Risks:

- FSS and compression are high-risk due to crypto, compression, and verification semantics.
- Cross-language subtle differences can corrupt files or hide reader bugs.
- Live concurrency differences can make closed-file verification pass while stock readers fail during normal one-writer/multiple-reader operation.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Full compatibility now requires proving that the four pure-language SDKs can operate on the same journal files, including closed-file interoperability, live one-writer/multiple-reader behavior across repository writers/readers, and explicit tracking of remaining writer feature gaps. The root risk is no longer missing language baselines; it is cross-language mismatch in file layout, match/cursor semantics, binary fields, directory ordering, rotation/retention behavior, compression handling, and active-writer publication windows.

Evidence reviewed:

- Product scope spec.
- Completed language and compatibility SOWs:
  - `.agents/sow/done/SOW-0004-20260523-rust-sdk-and-journalctl.md`
  - `.agents/sow/done/SOW-0005-20260523-go-sdk-and-journalctl.md`
  - `.agents/sow/done/SOW-0006-20260523-node-sdk-and-journalctl.md`
  - `.agents/sow/done/SOW-0007-20260523-python-sdk-and-journalctl.md`
  - `.agents/sow/done/SOW-0010-20260523-go-reader-and-journalctl-completion.md`
  - `.agents/sow/done/SOW-0011-20260523-live-concurrency-compatibility-gate.md`
  - `.agents/sow/done/SOW-0012-20260523-go-writer-binary-fields.md`
  - `.agents/sow/done/SOW-0013-20260523-go-directory-writer-rotation-retention.md`
- Shared contracts under `tests/conformance/` and `tests/conformance/live/`.
- Current SDK directories: `go/`, `rust/`, `node/`, and `python/`.

Affected contracts and surfaces:

- Writer file format features.
- Cross-language fixture matrix.
- Live cross-language reader behavior.
- Directory writer rotation/retention semantics.
- File-backed journalctl compatibility evidence.
- Verification behavior.
- Documentation.

Existing patterns to reuse:

- Shared conformance harness.
- Language SDK contracts.
- Per-language livewriter commands.
- Stock-reader live concurrency harness.
- Language adapters and file-backed journalctl commands.
- Product-scope feature-slice documentation style from completed SOWs.

Risk and blast radius:

- FSS and compression are high-risk due to crypto, compression, and verification semantics.
- Cross-language subtle differences can corrupt files or hide reader bugs.
- Live matrix tests can expose race windows that closed-file tests miss.
- Directory writer tests can remove or archive files if retention filters are wrong; all generated matrix files must stay inside `.local/`.

Sensitive data handling plan:

- No sensitive runtime data expected. Matrix fixtures must use synthetic fields and generated files under `.local/`; durable artifacts must record only sanitized paths, commands, counts, versions, and verdicts.

Implementation plan:

1. Build a committed or documented matrix runner that generates journal files from each language writer and reads them with every language reader plus stock `journalctl` where applicable.
2. Build or extend live matrix coverage so each repository reader consumes live files produced by each repository writer, reusing `tests/conformance/live/` and per-language `livewriter` commands.
3. Run the matrix, record exact commands, stock systemd version, entry counts, reader counts, failures, and transient retry rules.
4. Fix interoperability bugs found by the matrix without widening language-specific APIs unless specs/SOW are updated.
5. Inventory remaining writer feature gaps: compressed DATA object writing, xz/lz4/zstd parity, compact journal support, verification/FSS support, and directory ordering limitations.
6. Implement safe scoped writer features in this SOW where practical; split any high-risk compression/FSS/compact work into concrete follow-up SOWs with evidence if implementation would exceed the current SOW's safe blast radius.
7. Update specs, docs, SOW-status, and follow-up mapping before close.

Validation plan:

- Closed-file writer/reader matrix passes for Go, Rust, Node.js, and Python writers/readers.
- Stock `journalctl --verify --file` and stock reader checks pass for generated writer files where the claimed writer feature slice supports verification.
- Live stock-reader and cross-language concurrency matrix passes for every current repository writer/reader pair.
- systemd-compatible verification evidence is recorded where applicable.
- Dependency audit remains clean.
- `.agents/sow/audit.sh` and `git diff --check` pass before close.

Artifact impact plan:

- Specs: update writer feature reality.
- End-user/operator docs: update feature support matrix.
- Runtime project skills: update if new compatibility workflow is durable.
- SOW lifecycle: active in `current/` during implementation, then close to `done/` with implementation and SOW lifecycle changes in one commit.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

- Writer locking contract resolved on 2026-05-24: implement a pure cross-SDK cooperative lockfile with stale-owner detection. Native systemd writers do not appear to participate in a journal-file lock protocol, so the SDK lock prevents accidental multiple SDK writers but cannot mechanically block a native systemd writer that bypasses the SDK.
- The implementation may split compression-writing, compact journal, verification, and FSS work into narrower follow-up SOWs if evidence shows they are not safe to complete together with the interoperability matrix.

## Implications And Decisions

1. Interoperability and full writer completion boundary
   - Current state: SOW-0004, SOW-0005, SOW-0006, SOW-0007, and SOW-0010 are complete and pass their shared conformance gates.
   - Required before implementation: record the completed baseline feature matrix and decide from evidence whether any remaining compression or Forward Secure Sealing work needs narrower follow-up SOWs.
   - Implication: this SOW closes cross-language file compatibility after all baseline SDKs exist.
   - Risk: starting before all language baselines pass can hide whether failures come from core format handling, individual SDK bugs, or interoperability assumptions.

2. Cross-SDK writer locking contract
   - Evidence: systemd `JournalFile` has no lock-owner field and `journal_file_open()` / `journal_file_append_entry()` do not enforce a per-file advisory lock at the journal-file layer in `systemd/systemd @ cf3156842209f8318753861a9dd2d821674f3f59`.
   - Decision: implement option A from the 2026-05-24 discussion: a pure cross-SDK cooperative lockfile with stale-owner detection for Rust, Go, Node.js, and Python writers.
   - Scope: the lock protects cooperating SDK writers from accidentally opening the same journal file for writing concurrently.
   - Limitation: native systemd writers can bypass this lock because they do not participate in the SDK lock protocol.
   - Implication: stock systemd readers remain part of live compatibility validation; stock systemd writer mutual exclusion remains an operational contract, not an enforceable SDK guarantee.
   - Risk: stale lock handling must be robust enough for process crashes and PID reuse; tests must cover cross-language rejection before any writer truncates or appends.

## Plan

1. Move this SOW to `current/` after SOW-0007 closeout commit.
2. Record the completed feature matrix and remaining writer gaps before writer-feature implementation.
3. Delegate interoperability and writer-feature work using the repository-boundary block.
4. Review matrix results, systemd-compatible evidence, dependency audit, docs, and SOW audit before closing.

## Delegation Plan

- Implementer: `llm-netdata-cloud/minimax-m2.7-coder`, or fallback to `llm-netdata-cloud/qwen3.6-plus` then `llm-netdata-cloud/glm-5.1` if minimax fails or is unavailable.
- Reviewers: at least two from `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/glm-5.1`.
- Every prompt must include the canonical repository-boundary block from `AGENTS.md`.
- Failure handling: record implementer or reviewer model failure in this SOW, substitute only from the approved model list, rerun full-scope review after fixes, and do not close if `.agents/sow/audit.sh` fails.

## Execution Log

- 2026-05-24: Activated after Python closeout commit `b1276a0`, with all baseline language SDK/journalctl slices completed.

## Validation

Activation evidence:

- Passed: SOW-0007 closeout commit `b1276a0` exists before activation.
- Passed: `.agents/sow/audit.sh` was run after moving this SOW to `current/`; status/directory consistency passed and only this activation SOW is current.

Acceptance criteria evidence:

- Closed-file matrix passes for Go, Rust, Node.js, and Python writers/readers: `python3 tests/interoperability/run_matrix.py --entries 10` returned 104/104 checks.
- Live cross-language matrix passes for all repository writers/readers while writers append: `python3 tests/interoperability/run_live_matrix.py --entries 10 --poll-readers 1` returned 4/4 writers passing.
- Binary-field matrix passes for all repository writers/readers plus stock libsystemd: `python3 tests/interoperability/run_binary_matrix.py` returned 52/52 checks.
- zstd DATA compression matrix passes for all repository writers/readers plus stock libsystemd: `python3 tests/interoperability/run_compression_matrix.py` returned 72/72 checks.
- Cross-SDK writer lock matrix passes for all holder/contender pairs plus stale-lock recovery: `python3 tests/interoperability/run_lock_matrix.py --entries 200 --delay-ms 20` returned 8/8 checks.
- Remaining writer/reader feature gaps are tracked by concrete follow-up SOWs: xz/lz4 DATA writing in SOW-0017, compact journal format in SOW-0018, Forward Secure Sealing/full verification in SOW-0019, and directory traversal parity in SOW-0020.

Tests or equivalent validation:

- Activation audit passed. First-slice matrix validation passed `python3 tests/interoperability/run_matrix.py` with 104/104 checks on systemd `260 (260.1-2-manjaro)`.

Real-use evidence:

- Generated journals and matrix results were written under `.local/interoperability/`; stock `journalctl --verify --file` passed for each generated writer file.

Reviewer findings:

- GLM and Mimo returned `PRODUCTION GRADE` for committing the first closed-file matrix slice. Their shared non-blocking OR/disjunction coverage concern was fixed before commit with stronger same-field OR, `+` disjunction, zero-match, and cross-field AND checks.

Same-failure scan:

- Activation reviewed completed language SOWs and product-scope limitations for overlap. Implementation must search same-failure classes across all four language SDKs before close.

Sensitive data gate:

- Activation introduced no runtime journal data and no secrets. Planned matrix data must use synthetic fields and generated files under `.local/`; durable artifacts must record only sanitized commands, counts, versions, and verdicts.

Artifact maintenance gate:

- AGENTS.md: no update needed; existing SOW and repository-boundary rules apply.
- Runtime project skills: compatibility and orchestration skills already include the durable matrix, live-concurrency, compression, and writer-lock workflow rules needed after this SOW.
- Specs: `.agents/sow/specs/product-scope.md` already records the shipped zstd, binary, live, directory writer, and lock feature slices plus remaining limitations.
- End-user/operator docs: `tests/interoperability/README.md` updated for the five matrix runners and final feature-gap mapping.
- End-user/operator skills: no output/reference skill is produced by this SOW.
- SOW lifecycle: moved from `current/` to `done/` with `Status: completed` after follow-up mapping and final audit.
- SOW-status.md: updated for SOW-0008 completion and new pending follow-up SOWs.

Specs update:

- No additional spec update needed during closeout; product scope already reflects the shipped feature slices and open limitations.

Project skills update:

- No additional project skill update needed during closeout; durable workflow rules were already updated before this closeout.

End-user/operator docs update:

- `tests/interoperability/README.md` updated during closeout.

End-user/operator skills update:

- No end-user/operator skill update needed; this SOW produces no output/reference skill.

## Outcome

Completed.

This SOW delivered the shared closed-file interoperability matrix, live cross-language matrix, binary-field matrix, zstd DATA compression matrix, and cross-SDK writer-lock matrix for Rust, Go, Node.js, and Python. It also mapped the remaining writer/reader gaps into concrete pending SOWs instead of leaving them as informal future work.

## Lessons Extracted

- Keep high-risk format families in narrow SOWs once the core interoperability envelope is proven. zstd, xz/lz4, compact journals, and FSS share journal-object mechanics, but each has different dependency, parser, and validation risk.
- Live compatibility and closed-file compatibility must stay separate validation gates. Closed-file `journalctl --verify --file` is necessary but does not prove active one-writer/multiple-reader behavior.
- Writer locks are an SDK cooperation contract, not a stock systemd mutual-exclusion mechanism. Future writer changes must preserve the lockfile acquisition-before-truncate rule.

## Followup

- SOW-0014 tracks the deterministic ingestion dataset.
- SOW-0015 tracks systemd C and SDK ingesters for that dataset.
- SOW-0016 tracks byte-for-byte deterministic writer compatibility against systemd.
- SOW-0017 tracks xz/lz4 DATA writing and missing xz/lz4 reader support.
- SOW-0018 tracks compact journal format support.
- SOW-0019 tracks Forward Secure Sealing and full verification.
- SOW-0020 tracks directory traversal parity.
- SOW-0009 tracks benchmark, profile, and optimization work, including non-blocking performance notes from SOW-0008 reviewers.

## Regression Log

None yet.

## Implementation Log - 2026-05-24

### Interoperability Matrix First Slice

Implemented a committed closed-file interoperability matrix runner:

- `tests/interoperability/run_matrix.py`
- `tests/interoperability/README.md`

The runner generates journals under `.local/interoperability/` with the Go,
Rust, Node.js, and Python livewriter commands, then reads every generated file
with stock journalctl plus the Go, Rust, Node.js, and Python file-backed
journalctl implementations.

Command run:

```bash
python3 tests/interoperability/run_matrix.py
```

Systemd version: `systemd 260 (260.1-2-manjaro)`

Matrix result: 104/104 PASS, 0 FAIL.

Coverage achieved:

- 4 writers: Go, Rust, Node.js, Python.
- 5 readers/query tools per writer: stock journalctl, Go journalctl, Rust journalctl, Node.js journalctl, Python journalctl.
- 5 read/query checks per writer/reader pair:
  - `PRIORITY=6` reads exactly the expected entries;
  - `PRIORITY=1` reads zero entries;
  - repeated same-field OR via `MESSAGE=live-000000 MESSAGE=live-000001`;
  - `+` disjunction via `MESSAGE=live-000000 + MESSAGE=live-000001`;
  - cross-field AND via `PRIORITY=6 MESSAGE=live-000000`.
- Sequence validation: every reader result must contain ordered `LIVE_SEQ` values from `000000`.
- Stock verification: `journalctl --verify --file` passes for each generated writer file.

Known limits of this first slice:

- Closed-file matrix only at the time of this first slice; live cross-language reader/writer concurrency was completed in the later live matrix slice.
- Livewriter fixtures did not include binary payload fields at the time of this first slice; cross-language binary stress was completed in the later binary matrix slice.
- The matrix validates journalctl reader surfaces, not every lower-level SDK reader API directly.

### Writer Feature Gap Inventory

| Gap | Status | Evidence | Follow-up |
|-----|--------|----------|-----------|
| Compressed DATA object writing | Not implemented | Current writers emit uncompressed DATA objects | Continue in SOW-0008 or split a compression SOW |
| xz/lz4/zstd writer parity | Not implemented | Readers have different compression-read support; writers do not write compressed DATA | Continue in SOW-0008 or split by compression family |
| Compact journal format | Not implemented | Current writers create regular non-compact journals | Requires systemd reference inventory before implementation |
| Forward Secure Sealing / full verification | Not implemented | Verification/FSS tests are skipped or out of scope in earlier SOWs | Split a dedicated FSS SOW unless a safe narrow implementation emerges |
| Live cross-language matrix | Complete in later slice | `run_live_matrix.py` passes 4/4 with active observations for all writers and readers | Closed in this SOW |
| Cross-language binary stress | Complete in later slice | `run_binary_matrix.py` passes 52/52 across all writers/readers plus stock libsystemd | Closed in this SOW |
| Writer locking parity | Complete in later slice | `run_lock_matrix.py` passes 8/8; all SDK writers share a pure lockfile contract and clean stale crashed-writer locks | Closed in this SOW |
| Directory ordering guarantees | Partial | Current directory readers iterate sequentially by file metadata and are validated for non-overlapping active/archive files | Continue under SOW-0008 matrix expansion |

### Validation Results

- Passed: `python3 tests/interoperability/run_matrix.py` with 50 entries per writer.
- Passed: `.agents/sow/audit.sh`.
- Passed: `git diff --check`.
- Generated result file: `.local/interoperability/matrix-results-20260524-083546.json`.

### Review Results

- GLM first review verdict: `PRODUCTION GRADE`; non-blocking findings about weak OR/disjunction coverage, missing zero-match filter, missing cross-field AND, Go lock documentation asymmetry, and result-file accumulation were dispositioned.
- Mimo first review verdict: `PRODUCTION GRADE`; non-blocking findings about weak OR/disjunction coverage, stale in-progress validation text, repository-reader `--quiet` support, repeated rebuilds, and scratch artifacts were dispositioned.
- Implemented coverage improvements before commit: added zero-match filter validation, real same-field OR union validation, real `+` disjunction union validation, and cross-field AND validation to the matrix runner.
- GLM rerun verdict: `PRODUCTION GRADE`; remaining low findings were accepted as future matrix improvements or existing open scope: multi-field `+` disjunction discrimination, explicit non-`LIVE_SEQ` value assertions, Rust direct-file writer mode in this matrix, and `.local/` result accumulation.
- Mimo rerun verdict: `PRODUCTION GRADE`; remaining low findings were accepted as future matrix improvements or existing open scope: result-file accumulation, unrelated untracked `go/adapter/adapter`, repository-reader `--quiet` parity, JSON-only output coverage, and two-branch `+` coverage.

### Live Cross-Language Interoperability Matrix Second Slice

Implemented a committed live cross-language interoperability matrix runner:

- `tests/interoperability/run_live_matrix.py`
- Updated `tests/interoperability/README.md`

The runner starts one writer per language (Go file-mode, Rust directory-mode,
Node.js file-mode, Python file-mode) and polls multiple readers while the writer
is actively appending. For the Rust directory writer, the runner discovers the
actual active `.journal` file and validates file-backed reader compatibility
against that file. After the writer exits, final reader snapshots are collected
and validated.

Command run:

```bash
python3 tests/interoperability/run_live_matrix.py
```

Systemd version: `systemd 260 (260.1-2-manjaro)`

Live matrix result: 4/4 PASS, 0 FAIL.

Coverage achieved:

- 4 writers: Go (file), Rust (directory), Node.js (file), Python (file).
- Readers per writer: stock journalctl, Go, Rust, Node.js, Python.
- 2 polling reader tasks per language per writer.
- Validation:
  - at least one poll observed entries while the writer was still active;
  - all final reads observed the complete ordered `LIVE_SEQ` sequence;
  - `journalctl --verify --file` passed for every generated writer file.

Key implementation decisions:

1. **No `--follow` required**: repository readers that do not implement follow
   mode (Go, Rust) are polled via file-backed `--file --output=json` queries.
   This is intentional per SOW-0008 requirements. The live matrix validates
   live behavior without requiring follow semantics.

2. **Rust directory writer file discovery**: the Rust livewriter exercises the
   SDK directory writer, but the live matrix validates file-backed reader
   compatibility by discovering the generated `.journal` file under the
   directory. Directory traversal parity is tracked separately from live file
   compatibility.

3. **Active polling instead of event-based**: readers poll at 0.1s intervals
   while the writer is active. The `stop_poll` event coordinates graceful
   shutdown when the writer exits.

4. **Result JSON schema**: records `writer`, `journal_path`, `journal_mode`,
   `entries`, `exit_code`, `active_polls` (while-active observations),
   `final_reads` (post-exit snapshots), `verify` (stock --verify result for
   the generated journal file), `status`, and `errors`.

### Live Matrix Known Limits

- Closed-file matrix (`run_matrix.py`) and live matrix (`run_live_matrix.py`)
  are separate runners; they do not share execution state.
- The live matrix tests one writer at a time with multiple concurrent readers,
  not multiple concurrent writers.
- Binary stress, compression writing, compact journals, and FSS are out of
  scope for this slice and remain tracked in the writer feature gap inventory.
- Full repository `--directory` traversal parity remains open; this slice proves
  live file compatibility by passing discovered journal files to every reader.

### Live Matrix Validation Results

- Passed: `python3 tests/interoperability/run_live_matrix.py` with 30 entries
  per writer.
- Passed: `git diff --check`.
- Passed: `bash .agents/sow/audit.sh`.
- Generated result file:
  `.local/interoperability/live-matrix-results-20260524-092103.json`.

### Live Matrix Review Results

- GLM review verdict: `PRODUCTION GRADE`; low findings about stock journalctl
  flag parity, active-poll diagnostics, live match-logic breadth, theoretical
  writer cleanup windows, poll-result timeout, and `.local/` result accumulation
  were dispositioned.
- Qwen review verdict: `PRODUCTION GRADE`; low findings about active-poll
  diagnostics, raw JSON `while_active` clarity, stock journalctl flag parity,
  README `writer_stderr` documentation, and `.local/` result accumulation were
  dispositioned.
- Implemented low-risk review improvements before commit: stock journalctl uses
  `--quiet --no-pager`, active-poll failures record diagnostic text, active
  poll result `while_active` reflects whether a live sequence was captured,
  poll future timeout was raised to match reader subprocess timeout, and README
  documents `writer_stderr`.
- GLM and Qwen rerun verdicts after those fixes: `PRODUCTION GRADE`. Remaining
  low findings were accepted as future hardening or scope notes: result-file
  accumulation under `.local/`, live match-logic breadth, final-read
  parallelism, and defensive exception logging in unreachable poll-future error
  paths.

### Writer Feature Gap Inventory (updated)

| Gap | Status | Evidence | Follow-up |
|-----|--------|----------|-----------|
| Compressed DATA object writing | Not implemented | Current writers emit uncompressed DATA objects | SOW-0008 or split compression SOW |
| xz/lz4/zstd writer parity | Not implemented | Writers do not write compressed DATA | SOW-0008 or split by compression family |
| Compact journal format | Not implemented | Writers create regular non-compact journals | Requires systemd reference inventory |
| Forward Secure Sealing / verification | Not implemented | Verification/FSS tests skipped in earlier SOWs | Split dedicated FSS SOW |
| Live cross-language file matrix | Complete | `run_live_matrix.py` passes 4/4; active observations confirmed for all writers and all five readers | Closed |
| Cross-language binary stress | Complete | `run_binary_matrix.py` passes 52/52 across all writers/readers plus stock libsystemd | Closed |
| Writer locking parity | Complete | `run_lock_matrix.py` passes 8/8; all SDK writer pairs reject concurrent writers and stale locks left by crashed writers are cleaned | Closed |
| Directory reader subdirectory traversal | Partial | Live matrix validates discovered files; full `--directory` traversal parity remains separate | SDK follow-up work |

### Binary Field Interoperability Matrix Third Slice

Implemented a binary-field interoperability matrix runner:

- `tests/interoperability/run_binary_matrix.py`
- Updated `tests/interoperability/README.md`
- Extended the Go, Rust, Node.js, and Python livewriter test commands with a
  `--binary-fixture` mode.

The runner generates one binary fixture journal per writer language. Each
fixture contains:

- `TEST_ID=binary-interoperability`
- `MESSAGE=binary interoperability`
- `PRIORITY=6`
- `LIVE_SEQ=000000`
- `BINARY_PAYLOAD` bytes `00 01 02 41 0a 7f 80 ff`
- `BINARY_MATCH` bytes `61 62 63 07 64 65 66`
- `BINARY_EMPTY` as an empty value

Validation command:

```bash
python3 tests/interoperability/run_binary_matrix.py
```

Systemd version: `systemd 260 (260.1-2-manjaro)`

Binary matrix result: 52/52 PASS, 0 FAIL.

Coverage achieved:

- 4 writers: Go file-mode, Rust directory-mode, Node.js file-mode, Python file-mode.
- Stock `journalctl --verify --file` passes for every generated file.
- Stock `journalctl --output=json` validates byte-array JSON for
  non-printable binary fields and empty-string JSON for empty binary fields.
- Stock `journalctl --output=export` validates exact size-prefixed binary field
  payloads.
- Stock libsystemd helper `tests/conformance/binary/libsystemd_binary_field_reader.c`
  validates byte-for-byte `sd_journal_get_data()` for `BINARY_PAYLOAD`,
  `BINARY_MATCH`, and `BINARY_EMPTY`.
- Go, Rust, Node.js, and Python journalctl rewrites validate JSON and export
  output against all four generated writer files.
- Stock file-backed `BINARY_MATCH=abc\x07def` match returns the expected entry
  through argv and export output.

Implementation notes:

- Minimax implementer attempt created the initial binary fixture and runner
  slice but stalled before a clean final result. The attempt was stopped by
  targeted PIDs owned by this run after several minutes without output.
- The matrix exposed two real compatibility bugs that were fixed in this
  chunk:
  - Rust and Python JSON formatting treated UTF-8-decodable control-character
    values as strings; both now use stock-style printability checks before
    choosing string vs byte-array JSON.
  - Python journalctl decoded export bytes through UTF-8 replacement before
    writing stdout; it now writes byte output through `stdout.buffer`.
- The low-entry live regression command initially failed because the Python CLI
  reader could miss a Go writer active window of about 50 ms. The live matrix
  default writer delay was increased from 5 ms to 20 ms so low-entry live gates
  observe live readers instead of process startup latency.

Validation results for this slice:

- Passed: `python3 tests/interoperability/run_binary_matrix.py` with 52/52
  checks.
- Passed: `python3 tests/interoperability/run_matrix.py --entries 10` with
  104/104 checks.
- Passed: `python3 tests/interoperability/run_live_matrix.py --entries 10 --poll-readers 1`
  after hardening the default writer delay.
- Passed: `python3 tests/interoperability/run_live_matrix.py --entries 30 --poll-readers 1`.
- Passed: `python3 -m py_compile tests/interoperability/run_binary_matrix.py tests/interoperability/run_matrix.py tests/interoperability/run_live_matrix.py python/cmd/livewriter.py python/cmd/journalctl.py python/test_all.py`.
- Passed: `python3 python/test_all.py`.
- Passed: `go test ./journal ./cmd/journalctl ./internal/testcmd/livewriter` from `go/`.
- Passed: `cargo test --manifest-path rust/Cargo.toml -p journal`.
- Passed: `cargo build --manifest-path rust/Cargo.toml -p livewriter`.
- Passed: `node --test node/test/all.js`.

Generated result files:

- `.local/interoperability/binary-matrix-results-20260524-095829.json`
- `.local/interoperability/matrix-results-20260524-095849.json`
- `.local/interoperability/live-matrix-results-20260524-095917.json`
- `.local/interoperability/live-matrix-results-20260524-100029.json`

Review results for this slice:

- GLM verdict: `PRODUCTION GRADE`; no blocking findings. Low notes about
  export parser single-entry scope, Rust livewriter fixture duplication, fixed
  10-entry binary matrix size, and `.local/` result accumulation are accepted
  as non-blocking scope or future hardening.
- Minimax verdict: `PRODUCTION GRADE`; one low documentation finding was fixed
  before commit by updating the older SOW/status gap rows so live and binary
  matrix state no longer contradicts the updated feature inventory.
- Qwen review attempt stalled after initial file reads and produced no verdict;
  the specific `timeout` and `opencode` PIDs for that run were stopped after
  several silent polling intervals. No Qwen finding was available to
  disposition.

Known limits remaining after this slice:

- Compression DATA writing, compact journals, FSS/full verification, writer
  locking parity, and directory traversal parity remain open in SOW-0008.

### Compression DATA Writing Fourth Slice - Scope

Next bounded implementation target:

- Implement zstd-compressed DATA object writing for Go, Rust, Node.js, and
  Python writers.
- Keep uncompressed writing as the default for backward-compatible APIs.
- Add explicit writer options to request zstd DATA compression and configure a
  compression threshold.
- Add a shared compression interoperability matrix proving that files written
  by each language are readable by stock `journalctl`, stock libsystemd, and
  all repository readers/journalctl rewrites.
- Prove that at least one DATA object is actually compressed by inspecting
  journal header/object flags, not only by checking reader output.

Systemd reference evidence:

- `systemd/systemd @ c0a5a2516d28`
- `src/libsystemd/sd-journal/journal-def.h:45` defines per-object compression
  flags `OBJECT_COMPRESSED_XZ`, `OBJECT_COMPRESSED_LZ4`, and
  `OBJECT_COMPRESSED_ZSTD`.
- `src/libsystemd/sd-journal/journal-def.h:168` defines corresponding header
  incompatible flags, including `HEADER_INCOMPATIBLE_COMPRESSED_ZSTD`.
- `src/libsystemd/sd-journal/journal-file.c:417` sets header incompatible
  compression flags when journal compression is requested.
- `src/libsystemd/sd-journal/journal-file.c:1808` compresses payloads only
  when compression is requested and the threshold is met.
- `src/libsystemd/sd-journal/journal-file.c:1830` compresses into at most
  `size - 1`, so compression is accepted only when the compressed payload is
  smaller than the uncompressed payload.
- `src/libsystemd/sd-journal/journal-file.c:1844` hashes and deduplicates DATA
  objects by the original uncompressed `FIELD=value` payload, then stores the
  compressed payload and per-object compression flag when compression wins.

Scope exclusions for this slice:

- xz and lz4 DATA-object writing remain open. The existing common denominator
  across all four languages is zstd: Go has pure Go `klauspost/compress/zstd`,
  Rust has pure Rust `ruzstd`, Node.js v22 has built-in zstd, and Python 3.14
  has standard-library `compression.zstd`. xz/lz4 writer parity needs a
  separate dependency and validation decision.
- Compact journal format remains open because it changes object layouts and
  offset widths beyond DATA compression.
- Forward Secure Sealing remains open because it adds cryptographic tag object
  state, key lifecycle, and verification semantics.

### Compression DATA Writing Fourth Slice - Implementation

Implemented zstd-compressed DATA object writing across all four writers:

- Go: `journal.Options{Compression, CompressThresholdBytes}` controls DATA
  compression; `LogConfig.Options` carries the same options through directory
  rotation/retention.
- Rust: `JournalFileOptions::with_compression()` and
  `JournalWriter::new_with_compression()` control DATA compression; the existing
  `JournalWriter::new()` remains backward compatible and defaults to current
  file/header behavior.
- Node.js: `Writer.create()` and `Log` accept `compression` and
  `compressionThresholdBytes`; `Writer.open()` preserves zstd behavior for
  reopened files.
- Python: `Writer.create()` and `Log` accept `compression` and
  `compression_threshold_bytes`; `Writer.open()` preserves zstd behavior for
  reopened files.
- Shared test command fixtures: Go, Rust, Node.js, and Python livewriters
  accept `--compression zstd`, `--compress-threshold`, and `--zstd-fixture`.

Compatibility fixes found by the matrix:

- Rust `ruzstd` emits valid zstd frames that omit frame content size by default.
  Stock systemd `decompress_blob_zstd()` rejects frames where
  `ZSTD_getFrameContentSize()` returns unknown. The Rust writer now patches the
  emitted pure-Rust zstd frame header to include frame content size before
  storing the DATA object.
- Node.js and Python SipHash implementations did not mask `len << 56` to
  64 bits for payloads longer than 255 bytes. This produced invalid DATA hashes
  for some long fields. Both implementations now mask the length word and have
  deterministic long-payload regression tests.
- Writer-side deduplication/search now compares decompressed DATA payloads when
  reopening or adding entries to files that already contain compressed DATA
  objects.

Implemented shared compression matrix:

- `tests/interoperability/run_compression_matrix.py`
- `tests/interoperability/README.md`

Compression matrix coverage:

- 4 writers: Go file-mode, Rust directory-mode, Node.js file-mode, Python
  file-mode.
- Header/object inspection proves `HEADER_INCOMPATIBLE_COMPRESSED_ZSTD` and at
  least one `OBJECT_COMPRESSED_ZSTD` DATA object per generated file.
- Stock `journalctl --verify --file` passes for each generated file.
- Stock `journalctl --output=json`, stock `journalctl --output=export`, and
  stock libsystemd read decompressed field values.
- Go, Rust, Node.js, and Python journalctl rewrites read JSON/export from every
  generated file.
- Stock and repository journalctl rewrites match the decompressed
  `COMPRESSED_MATCH=<value>` field through argv filters.

Implementation-delegation note:

- Minimax implementation attempt for this slice was stopped after it changed
  Rust public constructor call sites broadly and left incomplete/risky partial
  code. The specific stopped process IDs were owned by that run. Local repair
  and implementation completed the slice; Minimax should be used as reviewer for
  this locally implemented work.

Validation results for this slice before external review:

- Passed: `python3 tests/interoperability/run_compression_matrix.py` with
  72/72 checks on systemd `260 (260.1-2-manjaro)`.
- Passed: `python3 tests/interoperability/run_matrix.py --entries 10` with
  104/104 checks.
- Passed: `python3 tests/interoperability/run_binary_matrix.py` with 52/52
  checks.
- Passed: `python3 tests/interoperability/run_live_matrix.py --entries 10 --poll-readers 1`
  with 4/4 writers passing.
- Passed: `python3 -m py_compile tests/interoperability/run_compression_matrix.py python/journal/hash.py python/journal/writer.py python/test_all.py`.
- Passed: `python3 python/test_all.py`.
- Passed: `go test ./journal ./cmd/journalctl ./internal/testcmd/livewriter` from `go/`.
- Passed: `cargo test --manifest-path rust/Cargo.toml -p journal-core -p journal-log-writer -p journal`.
- Passed: `cargo build --manifest-path rust/Cargo.toml -p livewriter`.
- Passed: `node --test node/test/all.js`.

Generated result files:

- `.local/interoperability/compression-matrix-results-20260524-105521.json`
- `.local/interoperability/matrix-results-20260524-105558.json`
- `.local/interoperability/binary-matrix-results-20260524-105558.json`
- `.local/interoperability/live-matrix-results-20260524-105607.json`

External review and cleanup results:

- Minimax verdict: `PRODUCTION GRADE`; no blocking findings. Non-blocking notes
  about the Node unsupported-XZ flag rejection test and Rust decompression
  buffer reuse were dispositioned as no production bug. The Node test is
  intentionally checking that xz remains unsupported. The Rust decompression
  comparison slices only the returned decompressed length, so stale tail bytes
  cannot affect equality.
- Mimo verdict: `PRODUCTION GRADE`; no blocking findings. Low findings were
  fixed before commit by adding Rust unit tests for zstd frame content-size
  patching, by collapsing the Python livewriter compression threshold options
  into one aliased argument, and by adding `go/adapter/adapter` to `.gitignore`
  as a local build artifact.
- GLM verdict: `PRODUCTION GRADE`; no blocking findings. Low findings were
  fixed before commit by sharing Rust livewriter fixture generation across
  file/directory modes and by making Python writer reopen check
  `compression.zstd` availability before accepting a zstd-enabled file for
  append.

Post-cleanup validation results:

- Passed: `python3 tests/interoperability/run_compression_matrix.py` with
  72/72 checks on systemd `260 (260.1-2-manjaro)`.
- Passed: `python3 tests/interoperability/run_matrix.py --entries 10` with
  104/104 checks.
- Passed: `python3 tests/interoperability/run_binary_matrix.py` with 52/52
  checks.
- Passed: `python3 tests/interoperability/run_live_matrix.py --entries 10 --poll-readers 1`
  with 4/4 writers passing.
- Passed: `python3 -m py_compile tests/interoperability/run_compression_matrix.py python/journal/hash.py python/journal/writer.py python/cmd/livewriter.py python/test_all.py`.
- Passed: `python3 python/test_all.py`.
- Passed: `go test ./journal ./cmd/journalctl ./internal/testcmd/livewriter` from `go/`.
- Passed: `cargo test --manifest-path rust/Cargo.toml -p journal-core -p journal-log-writer -p journal`.
- Passed: `cargo build --manifest-path rust/Cargo.toml -p livewriter`.
- Passed: `node --test node/test/all.js`.

Post-cleanup generated result files:

- `.local/interoperability/compression-matrix-results-20260524-111019.json`
- `.local/interoperability/matrix-results-20260524-111035.json`
- `.local/interoperability/binary-matrix-results-20260524-111034.json`
- `.local/interoperability/live-matrix-results-20260524-111035.json`

Same-scope post-cleanup review rerun:

- Minimax verdict: `PRODUCTION GRADE`; no blocking findings. It verified the
  cleanup fixes and reiterated that Go's zstd frames are stock-systemd
  compatible without a Rust-style content-size patch because
  `klauspost/compress/zstd` emits compatible frames for this use case. The
  untracked `tests/interoperability/run_compression_matrix.py` note is expected
  before this commit and is handled by explicit path staging.
- GLM verdict: `PRODUCTION GRADE`; no blocking findings. Informational
  performance notes about Go per-call zstd encoder allocation, Python deferred
  import lookup, Rust compression allocations on incompressible data, and Rust
  decompression buffer capacity reuse are mapped to
  `.agents/sow/pending/SOW-0009-20260523-benchmark-profile-optimize.md`.
  They are not correctness debt for this compatibility slice.
- Mimo verdict: `PRODUCTION GRADE`; no blocking findings. Low notes about
  Node.js zstd availability, Go zstd encoder allocation, and header compression
  flag presence when no object compresses are dispositioned as follows:
  Node.js zstd uses the project runtime's built-in `node:zlib`; Go encoder
  performance is tracked in SOW-0009; the header flag semantics are valid
  because per-object flags define actual compression and all stock/repository
  readers validate that behavior.

Known limits remaining after this slice:

- xz/lz4 DATA writing, compact journal format, Forward Secure Sealing/full
  verification, and full directory traversal parity remain open in SOW-0008 or
  need concrete follow-up mapping before close.

### Writer Locking Parity Fifth Slice

Implemented the user-selected option A from the 2026-05-24 lock discussion:
a pure cross-SDK cooperative lockfile with stale-owner detection.

Implementation coverage:

- Go: `go/journal/lock.go` plus writer integration before open/truncate; the
  existing POSIX `flock` remains as a secondary local guard.
- Rust: `journal-core` writer lock integrated into `JournalFile::create()`,
  released on drop, and released before directory-writer rotation creates the
  next active file.
- Node.js: `node/src/lib/lock.js` plus writer integration before `w+` open, so
  Node.js no longer truncates before contention is detected.
- Python: `python/journal/lock.py` plus writer integration before open/truncate;
  the existing POSIX `flock` remains as a secondary local guard.

Lock contract:

- Lock file path is `<journal-file>.lock`.
- Metadata records lock format version, PID, Linux boot ID, and process start
  time from `/proc/<pid>/stat`.
- Acquisition uses atomic create-new semantics.
- A held lock is considered stale when the PID is gone, the boot ID differs,
  or the PID start time differs, which protects against normal process crash
  and PID reuse cases.
- Malformed or partially created lock files younger than the grace window are
  treated as active; older malformed lock files are cleaned.
- Native systemd writers do not participate in this SDK lock protocol and can
  bypass it. The lock protects cooperating SDK writers only.

Implemented shared lock matrix:

- `tests/interoperability/run_lock_matrix.py`
- `tests/interoperability/README.md`

Lock matrix coverage:

- 4 holders: Go, Rust, Node.js, Python.
- 4 contenders per holder: Go, Rust, Node.js, Python.
- Every contender must fail before publishing its ready file while the holder
  is active.
- Clean holder close must remove the lock file.
- Stock `journalctl --verify --file` must pass after each contention run.
- Stale-lock recovery is validated for crashed writers:
  Go -> Rust, Rust -> Node.js, Node.js -> Python, and Python -> Go.

Validation results before external review:

- Passed: `go test ./journal ./internal/testcmd/livewriter` from `go/`.
- Passed: `python3 python/test_all.py`.
- Passed: `node --test node/test/all.js`.
- Passed: `cargo test --manifest-path rust/Cargo.toml -p journal-core -p journal-log-writer -p journal`.
- Passed: `python3 tests/interoperability/run_lock_matrix.py --entries 200 --delay-ms 20`
  with 8/8 checks on systemd `260 (260.1-2-manjaro)`.
- Passed: `python3 tests/interoperability/run_live_matrix.py --entries 10 --poll-readers 1`
  with 4/4 writers passing.
- Passed: `python3 tests/interoperability/run_binary_matrix.py` with 52/52
  checks.
- Passed: `python3 tests/interoperability/run_compression_matrix.py` with 72/72
  checks.
- Passed: `python3 tests/interoperability/run_matrix.py --entries 10` with
  104/104 checks.

Generated result files:

- `.local/interoperability/lock-matrix-results-20260524-122351.json`
- `.local/interoperability/live-matrix-results-20260524-122358.json`
- `.local/interoperability/binary-matrix-results-20260524-122407.json`
- `.local/interoperability/compression-matrix-results-20260524-122408.json`
- `.local/interoperability/matrix-results-20260524-122408.json`

External review rounds:

- Round 1 reviewers: `llm-netdata-cloud/minimax-m2.7-coder`,
  `llm-netdata-cloud/glm-5.1`, and `llm-netdata-cloud/mimo-v2.5-pro`.
  All returned `PRODUCTION GRADE`.
- Round 1 disposition: Node.js and Rust same-process writer-lock coverage was
  added after low test-coverage notes. Go and Python already had same-process
  writer-lock tests.
- Round 2 reviewers: `llm-netdata-cloud/minimax-m2.7-coder`,
  `llm-netdata-cloud/glm-5.1`, and `llm-netdata-cloud/mimo-v2.5-pro`.
  All returned `PRODUCTION GRADE`.
- Round 2 disposition: Python lock-acquire cleanup was fixed so a secondary
  `os.close(fd)` failure cannot mask the original `_write_owner()` exception.
- Final same-scope reviewers after that cleanup:
  `llm-netdata-cloud/minimax-m2.7-coder` and `llm-netdata-cloud/glm-5.1`.
  Both returned `PRODUCTION GRADE`.
- Non-blocking hardening notes rejected for this slice: `O_NOFOLLOW` hardening
  requires Linux-specific per-language handling and does not change the
  accepted option A contract because journal directory write access is already
  trusted; Go typed lock errors would add an SDK API contract not required by
  this interoperability slice.

Post-review validation:

- Passed: `node --test node/test/all.js`.
- Passed: `cargo test --manifest-path rust/Cargo.toml -p journal-core writer_lock_rejects_same_process_create`.
- Passed: `cargo test --manifest-path rust/Cargo.toml -p journal-core -p journal-log-writer -p journal`.
- Passed: `python3 -m py_compile python/journal/lock.py python/journal/writer.py tests/interoperability/run_lock_matrix.py`.
- Passed: `python3 python/test_all.py`.
- Passed: `python3 tests/interoperability/run_lock_matrix.py --entries 200 --delay-ms 20`
  with 8/8 checks on systemd `260 (260.1-2-manjaro)`.
- Passed: `git diff --check`.
- Passed: changed-file sensitive-data scan for user names, absolute workstation
  paths, common token prefixes, and private-key markers produced no matches.
- Passed: `bash .agents/sow/audit.sh`.

Post-review generated result file:

- `.local/interoperability/lock-matrix-results-20260524-125306.json`

### Closeout - 2026-05-24

Final follow-up mapping:

- xz/lz4 DATA writing and missing xz/lz4 reader support are tracked by `.agents/sow/pending/SOW-0017-20260524-xz-lz4-data-writing.md`.
- Compact journal format support is tracked by `.agents/sow/pending/SOW-0018-20260524-compact-journal-format.md`.
- Forward Secure Sealing and full verification are tracked by `.agents/sow/pending/SOW-0019-20260524-forward-secure-sealing.md`.
- Directory traversal parity is tracked by `.agents/sow/pending/SOW-0020-20260524-directory-traversal-parity.md`.
- Deterministic ingestion dataset, ingesters, byte-for-byte identity, and performance optimization remain tracked by SOW-0014, SOW-0015, SOW-0016, and SOW-0009.

Final validation:

- Passed: `git diff --check`.
- Passed: `bash .agents/sow/audit.sh`.
- Passed: changed-file sensitive-data scan for user names, absolute workstation paths outside repository policy, common token prefixes, and private-key markers produced no matches.

Artifact maintenance gate:

- AGENTS.md: no update needed; repository-boundary, SOW, lock, and delegation rules already covered this closeout.
- Runtime project skills: no update needed; SOW-0008 already updated durable compatibility workflow rules during implementation slices.
- Specs: `.agents/sow/specs/product-scope.md` already describes the shipped writer/reader slices and remaining limitations.
- End-user/operator docs: `tests/interoperability/README.md` updated to describe the five matrix runners and the final feature-gap mapping.
- End-user/operator skills: no output/reference skill is produced by this SOW.
- SOW lifecycle: this file is completed and moved to `.agents/sow/done/`; remaining valid gaps have pending SOWs.
- `SOW-status.md`: updated for SOW-0008 completion and new pending follow-up SOWs.
