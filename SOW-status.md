# SOW Deployment Status

This is a human-readable status summary. Canonical state lives in `AGENTS.md`, `.agents/sow/`, and `.agents/sow/audit.sh`.

## Repository

Status: initialized

Bootstrap mode: empty/new project.

## Installed Framework

- Root `AGENTS.md` created.
- `CLAUDE.md` and `GEMINI.md` point to `AGENTS.md`.
- `.agents/skills/` created for project runtime skills.
- `.claude/skills` points to `.agents/skills/`.
- `.agents/sow/{pending,current,done,specs}/` created.
- `.agents/sow/SOW.template.md` installed.
- `.agents/sow/audit.sh` installed.

## Active SOW

- `.agents/sow/current/SOW-0008-20260523-interoperability-and-full-writer-features.md`

## Completed SOWs

- `.agents/sow/done/SOW-0001-20260523-project-bootstrap-and-orchestration.md`
- `.agents/sow/done/SOW-0002-20260523-repo-scaffold-and-rust-source-import.md`
- `.agents/sow/done/SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md`
- `.agents/sow/done/SOW-0005-20260523-go-sdk-and-journalctl.md`
- `.agents/sow/done/SOW-0011-20260523-live-concurrency-compatibility-gate.md`
- `.agents/sow/done/SOW-0012-20260523-go-writer-binary-fields.md`
- `.agents/sow/done/SOW-0013-20260523-go-directory-writer-rotation-retention.md`
- `.agents/sow/done/SOW-0010-20260523-go-reader-and-journalctl-completion.md`
- `.agents/sow/done/SOW-0004-20260523-rust-sdk-and-journalctl.md`
- `.agents/sow/done/SOW-0006-20260523-node-sdk-and-journalctl.md`
- `.agents/sow/done/SOW-0007-20260523-python-sdk-and-journalctl.md`

## Next SOW

- Current active: `.agents/sow/current/SOW-0008-20260523-interoperability-and-full-writer-features.md`
- Rationale: the Go, Rust, Node.js, and Python SDK/journalctl slices are complete; full interoperability and remaining writer-format features are next before benchmark/profiling.

## Guardrails

- CRITICAL: Never write raw sensitive data to durable artifacts.
- CRITICAL: Do not make changes outside this repository. The only write exception is `/tmp`; prefer `.local/` inside this repository.
- Prefer committing each completed and verified SOW chunk before starting the next chunk. Stage explicit paths only.

## Residuals

- Phase breakdown and implementation SOWs have been created as SOW-0002 through SOW-0009.
- Bootstrap external review ran in repeated rounds; governance findings were dispositioned in SOW-0001.
- SOW-0002 external review completed with all four round-3 reviewers returning `PRODUCTION GRADE`.
- SOW-0002 close commit completed.
- SOW-0003 external review completed with all round-6 reviewers returning `PRODUCTION GRADE`.
- SOW-0003 closeout cleanup completed and is ready for commit.
- 2026-05-23 priority update: after SOW-0003, activate Go writer-first work before Rust, Node.js, Python, full interoperability, or benchmarks.
- SOW-0005 completed the pure-Go writer-first implementation and was committed.
- 2026-05-23 compatibility clarification: live one-writer/multiple-reader compatibility with stock readers is mandatory. SOW-0011 completed the reusable live gate and applied it to the Go writer feature slice.
- 2026-05-23 binary field priority update: SOW-0012 completed binary field compatibility validation for the Go writer before later SDK phases continue.
- 2026-05-23 rotation/retention priority update: SOW-0013 completed the Go high-level directory writer with rotation and retention before later SDK phases continue.
- 2026-05-23 Go completion update: SOW-0010 is active for the Go reader, libsystemd-compatible reader facade, file-backed journalctl rewrite, and Go conformance adapter.
- 2026-05-23 Go completion progress: Minimax and Qwen implementation attempts did not finish cleanly; local repair completed a pre-review Go reader/journalctl/adapter slice with Go tests passing and verification/FSS intentionally still out of scope for this slice.
- 2026-05-23 Go completion closed: SOW-0010 completed the Go reader, libsystemd-style facade, file-backed journalctl command, and Go conformance adapter with repeated external production-grade reviews and final validation passing.
- 2026-05-23 Rust activation update: SOW-0004 is active after the SOW-0010 commit `2d349ad`; implementation is delegated under the shared conformance and live compatibility gates.
- 2026-05-23 Rust closed: SOW-0004 completed the Rust SDK, libsystemd-style facade, file-backed journalctl command, Rust conformance adapter, current Rust writer/reader feature-slice docs, and Rust product-scope updates. Final validation passed with Rust workspace tests, real adapter execution at 13 PASS / 2 SKIP, full `no-rtc` fixture JSON drain, 6,516-row repeated same-field OR and `+` disjunction checks matching the JSON oracle, stock-reader live writer compatibility, `git diff --check`, and SOW audit. Mimo and GLM closeout reviews returned `PRODUCTION GRADE`.
- 2026-05-23 Node.js activation update: SOW-0006 is active after the Rust closeout commit `97506b8`. The pre-implementation gate records plain JavaScript, no native addons, Buffer/Uint8Array binary values, BigInt internal 64-bit handling, and built-in Node runtime zstd support from Node.js `v22.22.2`.
- 2026-05-24 Node.js pre-review progress: SOW-0006 had a locally validated Node.js reader, writer, directory writer, libsystemd-style facade, journalctl command, adapter, package test runner, README, product-scope update, and livewriter command. Validation passed syntax/runtime import checks, `npm test`, adapter manifest at 13 PASS / 2 SKIP, fixture journalctl counts, directory writer rotation/retention smoke, stock-reader live concurrency, and Node-reader live polling before external closeout review.
- 2026-05-24 Node.js closed: SOW-0006 completed the pure Node.js SDK, libsystemd-style facade, file-backed journalctl command, conformance adapter, package tests, README, directory writer rotation/retention, and livewriter harness command. Final validation passed syntax/runtime import checks, `npm test`, shared conformance at 13 PASS / 2 SKIP, stock `journalctl --directory` live/closed reads, live stock `journalctl` and libsystemd reader concurrency on systemd `260 (260.1-2-manjaro)`, cross-language reads by stock/Go/Rust readers, dependency/native marker audit, `git diff --check`, and `.agents/sow/audit.sh`. Mimo and Minimax returned `VERDICT: PRODUCTION GRADE`; Kimi and GLM ran useful checks but timed out before final verdicts.
- 2026-05-24 Python activation update: SOW-0007 is active after the Node.js closeout commit `3dd2a58`. The pre-implementation gate records plain Python, no native journal bindings, bytes-like binary values, Python integer 64-bit handling, Python `3.14.5`, and available standard-library `compression.zstd` support.
- 2026-05-24 Python pre-review progress: SOW-0007 has a locally validated Python reader, writer, directory writer, libsystemd-style facade, journalctl command, adapter, package test runner, README, product-scope update, and livewriter command. Validation currently passes `python3 -m compileall python`, `python3 python/test_all.py`, full shared conformance at 15 results / 0 failures / 2 expected verification SKIPs, fixture journalctl counts, stock-reader live concurrency, and directory writer stock open/closed reads. Read-only external production-grade review is pending before close.
- 2026-05-24 Python closed: SOW-0007 completed the pure Python SDK, libsystemd-style facade, file-backed journalctl command, conformance adapter, package tests, README, directory writer rotation/retention, writer exclusive locking, and livewriter harness command. Final validation passed `python3 python/test_all.py`, `python3 -m compileall python`, shared conformance at 15 results / 0 failures / 2 expected verification SKIPs, stock-reader live concurrency on systemd `260 (260.1-2-manjaro)`, dependency/native marker audit, `git diff --check`, and `.agents/sow/audit.sh`. Minimax and GLM returned `VERDICT: PRODUCTION GRADE`; Kimi previously returned `VERDICT: PRODUCTION GRADE` with non-blocking findings that were fixed, and Qwen's lowercase-field finding was dispositioned as a false positive with tests.
- 2026-05-24 Interoperability activation update: SOW-0008 is active after the Python closeout commit `b1276a0`. The pre-implementation gate records that all baseline language SDK/journalctl slices are complete and that the phase starts with closed-file and live cross-language matrix evidence before deciding whether compression writing, compact journal support, verification, or FSS work must split into narrower follow-up SOWs.

## 2026-05-24 SOW-0008 Interoperability Implementation

### Progress

SOW-0008 (interoperability and full writer features) is in progress. First slice complete:

- **Interoperability matrix runner created** at `tests/interoperability/run_matrix.py` and `tests/interoperability/README.md`
- **Matrix executed**: 104/104 checks PASS | 0 FAIL on systemd 260 (260.1-2-manjaro)
- **Coverage**: all 4 language writers (Go, Rust, Node.js, Python) x stock/Go/Rust/Node.js/Python file-backed journalctl readers, with priority-read, zero-match filter, repeated same-field OR, `+` disjunction, cross-field AND, ordered `LIVE_SEQ`, and stock `journalctl --verify --file` checks

### Commands Run

```bash
python3 tests/interoperability/run_matrix.py
```

### Key Findings

1. All 4 language writers produce valid closed journal files readable by stock and repository file-backed journalctl implementations.
2. All 4 generated journals pass stock `journalctl --verify --file`.
3. Repeated same-field OR, `+` disjunction, zero-match filtering, and cross-field AND semantics are consistent across stock, Go, Rust, Node.js, and Python file-backed journalctl readers for generated files.
4. Product-scope writer lock reality corrected: Go and Python use `fcntl` locks; Node.js has no native flock; Rust writer lock support was not found by code search and remains a SOW-0008 gap.

### Writer Feature Gaps Inventoryed

| Gap | Status | Notes |
|-----|--------|-------|
| Compressed DATA writing | Not implemented | zstd/xz/lz4 need dedicated implementation or split SOW |
| Compact journal format | Not implemented | Requires investigation |
| Writer FSS | Not implemented | Split follow-up SOW unless a safe narrow scope emerges |
| Live cross-language matrix | Not complete | Next functional validation target in SOW-0008 |
| Cross-language binary stress | Not complete | Livewriter fixtures do not include binary fields yet |
| Writer locking parity | Partial | Go/Python use fcntl; Node/Rust need decision/evidence |

### Next Steps

- SOW-0008 remains open for the live cross-language matrix, binary stress fixtures, compression-writing/compact/FSS split decisions, and review.
- SOW-0009 remains pending; no benchmark/profiling work has started.

### Review

- GLM and Mimo returned `PRODUCTION GRADE` for committing the first slice in both initial review and rerun after coverage improvements.
- Their shared non-blocking OR/disjunction coverage concern was fixed before commit by adding two-entry same-field OR, two-entry `+` disjunction, zero-match, and cross-field AND checks.
