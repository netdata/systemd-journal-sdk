# SOW-0025 - Retention Enforcement On Writer Open

## Status

Status: open

Sub-state: Created from user request on 2026-05-26. Pending activation after the current SOW completes.

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

Risk and blast radius:

- Medium. The change can delete files earlier than today, so tests must prove deletion scope and active-file protection.
- Operational impact is high because this controls disk retention after configuration changes.

Sensitive data handling plan:

- Use synthetic journal fixtures and placeholder field values only. Do not inspect or persist real Netdata journal content, private endpoints, SNMP community strings, customer logs, or personal data.

Implementation plan:

1. Define the exact open lifecycle point in each language where retention-on-open runs.
2. Reuse chain scan results to identify protected active/current files before deletion.
3. Apply file-count, byte, age, and artifact-inclusive retention to eligible archived files.
4. Report deletion events/results consistently.
5. Add restart-with-smaller-policy tests for all four languages.

Validation plan:

- Cross-language high-level writer tests for open-time max-files, max-bytes, and max-age enforcement.
- Tests for impossible retention limits preserving the active/current file.
- Tests proving no append or rotation is needed to enforce smaller policy.
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

- Lifecycle reporting shape for open-time retention must be resolved in the pre-implementation pass if existing language APIs cannot expose the same information cleanly.

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

## Validation

Pending activation and implementation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
