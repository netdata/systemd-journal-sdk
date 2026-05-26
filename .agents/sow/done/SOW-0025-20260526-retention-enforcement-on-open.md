# SOW-0025 - Retention Enforcement On Writer Open

## Status

Status: completed

Sub-state: Implemented, validated, reviewed, and ready for the completion commit on 2026-05-26.

## Requirements

### Purpose

Make high-level directory writers enforce the configured retention policy when they open an existing journal directory, not only after rotation or new-file creation. This is required so a restarted Netdata plugin immediately honors a smaller retention policy without waiting for a new journal file.

### User Request

Create an SOW to ensure different retention policies are applied when a directory is opened by a writer. Restarting a writer with smaller retention should enforce the new policy on start too, not only on new file creation.

### Assistant Understanding

Facts:

- SOW-0023 is defining the high-level Netdata-compatible writer API across Rust, Go, Node.js, and Python.
- SOW-0023 acceptance already requires explicit retention enforcement and active/current file protection.
- Current high-level writers scan existing journal chains to resume sequence, path, size, and active-file state.
- Existing retention behavior is heavily tied to append, rotation, explicit enforcement calls, or close paths depending on language and mode.

Inferences:

- A smaller policy after restart is a normal operational event, especially when users reduce disk budget.
- Retention-on-open must run after chain scan identifies the current active/current file and before the writer publishes a new ready file or accepts writes.
- Lazy construction must be distinguished from writer open. If a language preserves a lazy constructor, the first real open/preflight/append path must enforce retention before creating or writing a successor.

Unknowns:

- Whether all four high-level writers currently have the same eager/lazy open semantics after SOW-0023 completes.
- Whether lifecycle deletion callbacks should fire during open-time retention by default in every language, or whether open-time deletion should be reported through a separate initialization result.

### Acceptance Criteria

- Rust, Go, Node.js, and Python high-level directory writers enforce enabled retention limits immediately when an existing directory is opened by the writer.
- Opening a writer with a smaller max-files, max-bytes, or max-age policy deletes eligible SDK-owned archived files before the first append or new-file creation.
- Retention-on-open never deletes the tracked active/current file, even if count or byte limits are smaller than that file.
- Retention-on-open scopes deletion to SDK-owned journal files for the configured source, machine-id layout, and naming mode. Non-journal files, unrelated sources, unrelated machine IDs, and consumer side artifacts are preserved unless the configured artifact-size provider only counts them.
- Artifact-inclusive size accounting applies to open-time retention when an artifact-size provider is configured.
- Lifecycle deletion events or an equivalent initialization result report open-time deletions with full paths in all languages.
- Lazy mode is explicitly tested: construction alone may remain side-effect-free if documented, but the first actual open/preflight/append must enforce retention before creating/writing a file.
- Tests cover restart with smaller file-count, byte, and age retention policies without appending new entries.
- Tests cover impossible retention limits where the current active/current file survives.
- Stock `journalctl --directory` and `journalctl --verify --file` pass on the retained directory after open-time enforcement.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0023-20260525-netdata-ingestion-writer-api.md`
- `.agents/sow/specs/product-scope.md`
- `SOW-status.md`
- Rust, Go, Node.js, and Python high-level writer tests referenced by SOW-0023 status

Current state:

- SOW-0023 records explicit retention enforcement and active/current file protection as part of the high-level writer API work.
- SOW-0023 does not fully close the restart-with-smaller-policy case as a dedicated acceptance target.
- Existing tests cover retention during rotation and explicit enforcement paths, but this SOW exists to make open-time enforcement an explicit cross-language guarantee.

Risks:

- Users reducing retention can see no effect until enough new data triggers rotation, leaving disk usage above policy indefinitely.
- Enforcing retention before active/current file discovery can delete the wrong file.
- Open-time deletion callbacks can surprise consumers if callback timing and error behavior are not documented.
- Lazy constructors can accidentally perform destructive work earlier than callers expect.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Retention enforcement currently has strong append/rotation coverage, but restart with a stricter policy is a distinct lifecycle point. The writer already scans existing files on open; that scan is the correct point to apply the current policy to existing archived files before accepting writes.

Evidence reviewed:

- SOW-0023 purpose and acceptance criteria for Netdata-compatible writer policy behavior.
- Product scope requires directory-owned writers, rotation/retention support, and stock-reader compatibility.
- The user explicitly required retention to apply when the directory is opened by a writer.
- Rust `Log::new_inner()` opens an existing active file or creates one for eager open, while `Log::write_entry_with_timestamps()` creates a lazy active file on first append. Existing retention logic is `Log::enforce_retention()` / `Log::apply_retention()`.
- Go `NewLog()` opens an existing active file or eager active file, while `Append()` opens a lazy active file through `ensureWriter()`. Existing retention logic is `EnforceRetention()` / `enforceRetention()`.
- Node.js `Log` mirrors the same constructor/open split through `_attachExistingActive()`, `_openWriter()`, and `_applyRetention()`.
- Python `Log` mirrors the same constructor/open split through `_attach_existing_active()`, `_open_writer()`, and `_apply_retention()`.

Affected contracts and surfaces:

- Rust `journal-log-writer::Log` high-level writer.
- Go `journal.Log` high-level writer.
- Node.js `Log` high-level writer.
- Python `Log` high-level writer.
- Lifecycle event/callback APIs.
- Artifact-size accounting APIs.
- Netdata plugin restart behavior and disk budget enforcement.

Existing patterns to reuse:

- Existing scan-and-resume chain logic in SOW-0023.
- Existing explicit retention enforcement methods and tests.
- Existing active/current file protection tests.
- Existing lifecycle observer tests.
- Existing lifecycle deletion events are the reporting shape for open-time retention; no new initialization result type is needed.

Risk and blast radius:

- Medium. The change can delete files earlier than today, so tests must prove deletion scope and active-file protection.
- Operational impact is high because this controls disk retention after configuration changes.

Sensitive data handling plan:

- Use synthetic journal fixtures and placeholder field values only. Do not inspect or persist real Netdata journal content, private endpoints, SNMP community strings, customer logs, or personal data.

Implementation plan:

1. Add a per-writer-instance retention-on-open flag in Rust, Go, Node.js, and Python.
2. Run the existing retention method once after an active writer has been opened or created, protecting that active/current file.
3. In lazy archived-only construction, do not delete during construction because no writer has been opened; run retention immediately after first active creation/open and before the first entry write.
4. In eager construction or existing-active reopen, run retention during construction because the writer has actually opened an active file.
5. Keep rotation, close, and explicit retention behavior on the existing paths.
6. Add restart-with-smaller-policy tests for all four languages.

Validation plan:

- Cross-language high-level writer tests for open-time max-files, max-bytes, and max-age enforcement.
- Tests for impossible retention limits preserving the active/current file.
- Tests proving no append or rotation is needed to enforce smaller policy.
- Tests proving lazy archived-only construction stays side-effect-free until first append/open.
- Stock `journalctl --directory` and `journalctl --verify --file` after enforcement.
- Same-failure scan for deletion paths that bypass active/current protection.
- External reviewer pass for data-loss and lifecycle-callback risks.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update journal compatibility skill if open-time retention becomes mandatory for writer changes.
- Specs: update product scope with open-time retention semantics.
- End-user/operator docs: update README/API docs for retention timing.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: pending until activated; likely follows SOW-0023 API stabilization.
- SOW-status.md: updated when created, activated, and closed.

Open-source reference evidence:

- No new external repository inspection was required for SOW creation.

Open decisions:

- Resolved: lifecycle deletion events are reused for open-time retention in all languages. This avoids adding a second retention reporting channel.
- Resolved: lazy archived-only construction remains side-effect-free. Existing-active reopen and eager open enforce retention during construction because those paths already open a writer and acquire the active file.

## Implications And Decisions

1. Open-time retention timing
   - Decision: retention enforcement belongs to the writer open/preflight path, before first append or successor file creation.
   - Reason: this is the earliest point where the writer has scanned the chain and can safely apply the current policy.
   - Risk: lazy constructors must remain explicit so construction does not unexpectedly delete files before the caller considers the writer open.

## Plan

1. Define open lifecycle semantics across languages.
2. Implement retention-on-open using existing chain scan and active/current protection.
3. Add cross-language restart-with-smaller-policy tests.
4. Update specs/docs and reviewer evidence.

## Delegation Plan

Implementer:

- Current routing is local implementation by the project manager unless the user explicitly re-enables external implementers.

Reviewers:

- Use read-only reviewers from the approved pool: `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`. Skip `llm-netdata-cloud/mimo-v2.5-pro` while unavailable.

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

- Record implementation failures, reviewer failures, audit failures, lifecycle contract uncertainty, or retention-scope uncertainty in this SOW before changing scope.

## Execution Log

### 2026-05-26

- Created SOW from user request while SOW-0023 review was running.
- Activated after SOW-0023 completed, committed, and pushed.
- Implemented retention-on-open in Rust, Go, Node.js, and Python high-level directory writers using the existing retention deletion path and active/current file protection.
- Kept lazy archived-only construction side-effect-free. Lazy writers now enforce retention immediately after the first active writer is opened and before the first entry is written.
- Added cross-language tests for lazy first-open retention, eager/existing-open retention, file-count retention, byte retention with artifact-size callbacks, age retention, active/current file protection, stock `journalctl --directory` readback, and `journalctl --verify --file` on retained active files.
- Updated the product scope spec, language README/API docs, and journal compatibility runtime skill with the new open-time retention contract.
- Read-only review round 1 found no blockers. Two reviewers requested stronger `journalctl --directory` evidence and artifact-size callback assertions; both gaps were fixed before round 2.
- Read-only review round 2 by `llm-netdata-cloud/glm-5.1` and `llm-netdata-cloud/qwen3.6-plus` reported production-grade status with no blocking findings.
- `llm-netdata-cloud/kimi-k2.6` and one rerun of `llm-netdata-cloud/minimax-m2.7-coder` did not produce usable final read-only review output before timeout/stall and were terminated by exact process IDs after verifying they were the SOW-0025 reviewer processes.

## Validation

Acceptance criteria evidence:

- Rust `Log` records `retention_on_open_applied` and calls `apply_retention_on_open()` after eager/existing-active open and after lazy first active creation, before entry write: `rust/src/crates/journal-log-writer/src/log/mod.rs`.
- Go `Log` records `openRetention` and calls `enforceRetentionOnOpen()` during `NewLog()` and after lazy first writer open in `Append()`: `go/journal/log.go`.
- Node.js `Log` records `openRetentionApplied` and calls `_applyRetentionOnOpen()` during construction and lazy first append: `node/src/lib/directory-writer.js`.
- Python `Log` records `_open_retention_applied` and calls `_apply_retention_on_open()` during construction and lazy first append: `python/journal/directory_writer.py`.
- Tests verify open-time retention with stock reader evidence:
  - Rust: `test_lazy_retention_runs_on_first_open` and `test_eager_retention_runs_on_open_for_all_policies`.
  - Go: `TestNewLogLazyRetentionRunsOnFirstOpen` and `TestNewLogEagerRetentionRunsOnOpenForAllPolicies`.
  - Node.js: `testLogLazyRetentionRunsOnFirstOpen` and `testLogEagerRetentionRunsOnOpenForAllPolicies`.
  - Python: `test_log_lazy_retention_runs_on_first_open` and `test_log_eager_retention_runs_on_open_for_all_policies`.

Tests or equivalent validation:

- `go test -count=1 ./journal -run 'TestNewLog(LazyRetentionRunsOnFirstOpen|EagerRetentionRunsOnOpenForAllPolicies)|TestLogEnforceRetention'` - passed.
- `cargo test --manifest-path rust/src/crates/journal-log-writer/Cargo.toml retention --test log_writer` - passed.
- `node node/test/all.js` - passed.
- `PYTHONPATH=.local/python-deps python3 python/test_all.py` - passed.
- `go test -count=1 ./...` from `go/` - passed.
- `cargo test --manifest-path rust/src/crates/journal-log-writer/Cargo.toml` - passed.
- After reviewer-requested test hardening:
  - `go test -count=1 ./journal -run 'TestNewLog(LazyRetentionRunsOnFirstOpen|EagerRetentionRunsOnOpenForAllPolicies)'` - passed.
  - `cargo test --manifest-path rust/src/crates/journal-log-writer/Cargo.toml retention --test log_writer` - passed.
  - `node node/test/all.js` - passed.
  - `PYTHONPATH=.local/python-deps python3 python/test_all.py` - passed.
  - `go test -count=1 ./...` from `go/` - passed.
  - `cargo test --manifest-path rust/src/crates/journal-log-writer/Cargo.toml` - passed.
- `git diff --check` - passed.

Real-use evidence:

- Each language's lazy first-open test writes a retained entry with a dedicated `TEST_ID` and reads it back through stock `journalctl --directory`.
- Each language's retained active file is verified through stock `journalctl --verify --file` where stock tooling is available.

Reviewer findings:

- Round 1 `llm-netdata-cloud/glm-5.1`: production-grade, non-blocking request for stronger directory readback coverage and lifecycle setup hardening. Disposition: fixed with cross-language `journalctl --directory` assertions and constructor-time Node.js lifecycle setup.
- Round 1 `llm-netdata-cloud/qwen3.6-plus`: production-grade, non-blocking request for directory readback and artifact-size callback evidence. Disposition: fixed with cross-language directory readback and byte-retention callback assertions.
- Round 2 `llm-netdata-cloud/glm-5.1`: production-grade, no blocking findings after fixes.
- Round 2 `llm-netdata-cloud/qwen3.6-plus`: production-grade, no blocking findings after fixes.
- Reviewer runs that did not produce usable final output were not used as completion evidence.

Same-failure scan:

- Searched for retention-on-open hooks across `rust`, `go`, `node`, and `python` with `rg`. Every implementation has a one-shot open-retention guard and calls it in both eager/existing-active and lazy first-open paths.
- Searched changed tests for `journalctl --directory` and artifact-size callback assertions. The first-open stock readback and byte-retention artifact callback paths are covered in all four languages.

Sensitive data gate:

- Only synthetic test fields and local temporary journal directories were used. No raw secrets, credentials, bearer tokens, SNMP communities, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details were added to durable artifacts.

Artifact maintenance gate:

- AGENTS.md: no update needed; the existing project-wide SOW, repository-boundary, and writer goals remain correct.
- Runtime project skills: updated `.agents/skills/project-journal-compatibility/SKILL.md` with the mandatory open-time retention rule.
- Specs: updated `.agents/sow/specs/product-scope.md` with shared high-level writer retention-on-open semantics.
- End-user/operator docs: updated `rust/README.md`, `go/README.md`, `go/API.md`, `node/README.md`, and `python/README.md`.
- End-user/operator skills: none exist for this project, so no update was needed.
- SOW lifecycle: status is `completed`; the SOW is moved to `.agents/sow/done/` and committed together with the implementation.
- SOW-status.md: updated to remove this SOW from current work and list it under completed work.

Specs update:

- `.agents/sow/specs/product-scope.md` now states that Rust, Go, Node.js, and Python apply configured retention once when an active writer is opened or created, while preserving lazy archived-only construction as side-effect-free.

Project skills update:

- `.agents/skills/project-journal-compatibility/SKILL.md` now requires future high-level writer work to preserve open-time retention semantics.

End-user/operator docs update:

- Language README/API files now describe eager/existing-active open enforcement and lazy first-append enforcement.

End-user/operator skills update:

- No output/reference skills exist in this repository; no update was needed.

Lessons:

- Open-time retention is a lifecycle behavior, not just a retention helper behavior. Future writer changes should test constructor/open timing, lazy timing, stock directory readback, and artifact-inclusive byte retention together.

Follow-up mapping:

- No deferred items remain for SOW-0025. Broader reader mixed-format work, Netdata reader facade work, Netdata integration, directory traversal parity, compatibility gap audit, and benchmark/optimization work are already tracked by existing pending SOWs.

## Outcome

Completed. Rust, Go, Node.js, and Python high-level directory writers now enforce configured retention when an active writer is opened or created, including restarted writers with smaller retention policies.

## Lessons Extracted

Open-time retention must be validated as part of the writer lifecycle. The same tests need to cover when deletion happens, what is protected, which callbacks fire, and whether stock tooling still reads the retained directory.

## Followup

None for this SOW.

## Regression Log

None yet.
