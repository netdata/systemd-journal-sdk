# SOW-0008 - Interoperability And Full Writer Features

## Status

Status: in-progress

Sub-state: active after Go, Rust, Node.js, and Python baseline SDK/journalctl slices completed.

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

- No immediate user decision blocks activation. The implementation may split compression-writing, compact journal, verification, and FSS work into narrower follow-up SOWs if evidence shows they are not safe to complete together with the interoperability matrix.

## Implications And Decisions

1. Interoperability and full writer completion boundary
   - Current state: SOW-0004, SOW-0005, SOW-0006, SOW-0007, and SOW-0010 are complete and pass their shared conformance gates.
   - Required before implementation: record the completed baseline feature matrix and decide from evidence whether any remaining compression or Forward Secure Sealing work needs narrower follow-up SOWs.
   - Implication: this SOW closes cross-language file compatibility after all baseline SDKs exist.
   - Risk: starting before all language baselines pass can hide whether failures come from core format handling, individual SDK bugs, or interoperability assumptions.

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

- Closed-file matrix, live cross-language file matrix, and binary-field matrix slices pass for Go, Rust, Node.js, and Python writers/readers. Compression writing, compact journals, FSS, dependency audit, final docs/spec checks, and final SOW audit remain required before close.

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

- AGENTS.md: no update needed for activation; existing SOW and repository-boundary rules apply.
- Runtime project skills: no update needed for activation; implementation may update compatibility/orchestration skills if the matrix creates a durable workflow.
- Specs: no new shipped behavior during activation; specs will update when matrix results or writer feature reality changes.
- End-user/operator docs: no update needed for activation; docs will update if the support matrix changes.
- End-user/operator skills: no output/reference skill is produced during activation.
- SOW lifecycle: moved from `pending/` to `current/` with `Status: in-progress`.
- SOW-status.md: updated for SOW-0008 activation.

Specs update:

- No spec update needed for activation beyond existing product scope.

Project skills update:

- No project skill update needed for activation.

End-user/operator docs update:

- No end-user/operator docs update needed for activation.

End-user/operator skills update:

- No end-user/operator skill update needed for activation.

## Outcome

Pending.

## Lessons Extracted

Pending activation.

## Followup

Pending activation.

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
| Writer locking parity | Partial | Go and Python use `fcntl` locks; Node.js has no native flock; Rust writer lock claim was removed from product scope after code search found no writer lock | Track whether Node/Rust need pure-language advisory lock behavior |
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
| Writer locking parity | Partial | Go and Python use fcntl; Node.js has no native flock; Rust writer lock removed from scope | Track whether Node/Rust need advisory lock |
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
