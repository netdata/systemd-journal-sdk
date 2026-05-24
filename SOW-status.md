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

- `.agents/sow/current/SOW-0007-20260523-python-sdk-and-journalctl.md`

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

## Next SOW

- Current active: `.agents/sow/current/SOW-0007-20260523-python-sdk-and-journalctl.md`
- Rationale: the Go, Rust, and Node.js SDK/journalctl slices are complete; Python is the remaining per-language SDK/journalctl slice before full interoperability and benchmark/profiling phases.

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
