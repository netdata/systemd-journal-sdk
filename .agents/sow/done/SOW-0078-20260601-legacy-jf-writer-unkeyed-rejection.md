# SOW-0078 - Legacy jf Writer Unkeyed Rejection

## Status

Status: completed

Sub-state: implemented, validated, reviewed, and closed.

## Requirements

### Purpose

Remove or explicitly guard a legacy Rust writer assertion path so the repository
does not keep known writer failure-mode debt while current journal writer APIs
move toward production integration.

### User Request

The user asked that technical debt not be left behind as journal compatibility
work progresses. During SOW-0077 review, a related legacy `jf` writer assertion
path was identified outside the current supported Rust writer stack.

### Assistant Understanding

Facts:

- SOW-0077 hardened the current Rust writer stack in `journal-core` and
  `journal-log-writer`.
- `rust/src/crates/jf/journal_file/src/lib.rs` publicly re-exports
  `writer::JournalWriter`.
- `rust/src/crates/jf/journal_file/src/writer.rs` still asserts
  `HeaderIncompatibleFlags::KeyedHash` in `add_entry()`.
- Prior Netdata integration planning keeps legacy `jf` primarily as a reader
  compatibility layer and tracks eventual vendored Rust removal separately.

Inferences:

- The legacy `jf` writer may be unused or unsupported, but while it is publicly
  exported it can still be accidentally used and panic on historical unkeyed
  files.
- The correct fix may be controlled rejection, deprecation, or removal, but the
  choice needs a focused audit of current users and package surface.

Unknowns:

- Whether any current tests, examples, or downstream integration path still
  expects the legacy `jf` writer to be usable.

### Acceptance Criteria

- Audit the legacy `jf` writer export and all in-repository uses.
- Decide, with evidence, whether the legacy writer should be kept with a
  controlled unsupported-file error, hidden/deprecated, or removed.
- If kept, attempting to write through the legacy `jf` writer to an unkeyed
  historical file returns a controlled error before entry mutation and without
  assertion panic.
- If deprecated or removed, docs/specs explain the supported replacement path
  and tests confirm no public examples or active integrations depend on it.
- SOW-0077's current writer behavior and SOW-0073's historical reader behavior
  remain unchanged.

## Analysis

Sources checked:

- `rust/src/crates/jf/journal_file/src/lib.rs`: public `JournalWriter`
  re-export.
- `rust/src/crates/jf/journal_file/src/writer.rs`: legacy writer
  `add_entry()` keyed-hash assertion.
- SOW-0077 reviewer findings.

Current state:

- The current supported Rust writer stack is guarded by SOW-0077.
- The legacy `jf` writer path remains separate and unguarded until this SOW is
  executed.

Risks:

- Silently keeping the legacy writer public can create a panic footgun for
  callers that discover it through the Rust crate surface.
- Removing or hiding it without auditing users can break compatibility if an
  internal test or downstream caller still relies on it.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The same class of writer failure-mode issue fixed in SOW-0077 also appears
  in the legacy `jf` writer, but that path was outside the current supported
  writer stack. It must be explicitly classified and fixed, deprecated, or
  removed so the repository does not retain known assertion-panic behavior.

Evidence reviewed:

- `rust/src/crates/jf/journal_file/src/lib.rs`: public writer re-export.
- `rust/src/crates/jf/journal_file/src/writer.rs`: keyed-hash assertion in
  `add_entry()`.
- `.agents/sow/done/SOW-0077-20260601-rust-historical-unkeyed-writer-rejection.md`:
  current writer stack closure and follow-up mapping.

Affected contracts and surfaces:

- Legacy Rust `jf` crate public API.
- Netdata compatibility-layer migration planning.
- Rust docs/examples/tests if they mention the legacy writer.

Existing patterns to reuse:

- SOW-0077 `UnsupportedJournalFile` guard pattern for current Rust writer APIs.
- SOW-0073 synthetic historical unkeyed fixtures.
- Existing deprecation/removal tracking in Netdata vendored journal removal
  SOWs.

Risk and blast radius:

- Low if the writer is unused and can be deprecated or removed cleanly.
- Medium if any in-repository code still uses the legacy writer API.
- Compatibility risk is controlled by auditing references before changing the
  public surface.

Sensitive data handling plan:

- Use synthetic fixtures only. Do not copy raw host journals, field payloads,
  credentials, SNMP communities, personal data, private endpoints, or customer
  identifiers into durable artifacts.

Implementation plan:

1. Search all Rust crates, tests, docs, and examples for legacy `jf`
   `JournalWriter` use.
2. Choose the least-surprising fix based on evidence: guarded keep,
   deprecation, or removal.
3. Implement the selected fix with tests for controlled behavior or absence of
   public use.
4. Update specs/docs/SOW status with the supported legacy `jf` writer stance.

Validation plan:

- Rust tests covering the legacy `jf` crate and any affected workspace crates.
- Same-failure scan for remaining keyed-hash writer assertions.
- Project audit.
- Whole-SOW read-only reviewer batch if implementation changes runtime code or
  public API.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: update only if this exposes a recurring workflow
  rule not already covered by SOW-0077.
- Specs: update if the legacy `jf` writer supported/deprecated/removed status
  changes.
- End-user/operator docs: update if public Rust docs mention the legacy writer.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: normal pending to current to done lifecycle.
- SOW-status.md: add as pending now and update when started/completed.

Open-source reference evidence:

- None needed for SOW creation; this is a local legacy API classification and
  same-failure cleanup.

Open decisions:

- None at creation. If the audit finds real downstream use that makes removal
  risky, bring guarded-keep versus deprecation/removal options back to the user
  with evidence.

## Implications And Decisions

No user decision is required before the audit. A decision is required only if
the audit finds conflicting compatibility evidence.

## Plan

1. Audit legacy `jf` writer use and public surface.
2. Implement guarded rejection, deprecation, or removal based on evidence.
3. Validate and run whole-SOW reviewers if runtime/public API changes.

## Delegation Plan

Implementer:

- Local implementation unless the user explicitly changes routing.

Reviewers:

- Reviewer pool after complete implementation and local validation if the SOW
  changes code or public API: minimax, kimi, qwen, glm, and mimo.

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

- If removal/deprecation would affect an active integration, pause and ask the
  user with evidence and options.

## Execution Log

### 2026-06-01

- Created from SOW-0077 same-failure scan and reviewer non-blocking finding.
- Moved to `current/` after user selected SOW-0078.
- Audited in-repository legacy `jf` writer references. The legacy writer is
  publicly re-exported from `rust/src/crates/jf/journal_file/src/lib.rs`, but
  repository use is limited to the legacy `jf` crate's own tests. Active SDK
  writer code uses `journal_core::file::JournalWriter`.
- Chose guarded keep, not removal: removing a public legacy export would be a
  public API break without evidence of benefit, while a controlled error removes
  the known assertion-panic debt.
- Added `JournalError::UnsupportedJournalFile` to the legacy `jf` error crate.
- Replaced the legacy writer keyed-hash assertion path with controlled
  unsupported-file errors in both `JournalWriter::new()` and
  `JournalWriter::add_entry()`.
- Added legacy writer tests for unkeyed construction rejection and post-constructor
  unkeyed `add_entry()` rejection without header mutation.
- Updated product scope and Rust README to record the supported writer path:
  legacy `jf` writer remains a compatibility surface, while production writer
  integrations should use `journal-core` direct-file writing or the high-level
  `journal::Log` directory writer.
- First whole-SOW reviewer batch results:
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `NOT PRODUCTION GRADE`.
- Fixed the minimax blocker: the current `journal-core` writer still had a
  keyed-hash assertion in the append hot path if the keyed flag was cleared
  after construction. Replaced it with `UnsupportedJournalFile` and added a
  no-mutation regression test mirroring the legacy `jf` post-construction test.
- Second whole-SOW reviewer batch results:
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
- Closed the SOW after the second reviewer batch found no blockers.

## Validation

Acceptance criteria evidence:

- Audit the legacy `jf` writer export and all in-repository uses:
  - `rust/src/crates/jf/journal_file/src/lib.rs:15` publicly re-exports
    `writer::JournalWriter`.
  - `rg` for `journal_file::JournalWriter`, `use journal_file::{...JournalWriter}`,
    and `use crate::JournalWriter` found no repository use of the legacy writer
    outside the legacy `jf` crate's own tests.
  - `rust/src/crates/jf/journal_reader_ffi/src/lib.rs` imports
    `journal_file::{Direction, HashableObject, JournalFile, JournalReader,
    Location}` only, so the FFI compatibility layer is reader-only for this
    surface.
- Decision:
  - Kept the public legacy writer surface with a controlled unsupported-file
    error. Removal was rejected as an unnecessary public API break because no
    active integration needed removal and a guard fully addresses the panic
    failure mode.
- Controlled rejection before mutation:
  - `rust/src/crates/jf/error/src/lib.rs:102` adds
    `JournalError::UnsupportedJournalFile`; `:142` gives it stable error code
    `-32`.
  - `rust/src/crates/jf/journal_file/src/writer.rs:49` rejects unkeyed files in
    `JournalWriter::new()` before tail-object or append state setup.
  - `rust/src/crates/jf/journal_file/src/writer.rs:87` rejects unkeyed files in
    `JournalWriter::add_entry()` before DATA, FIELD, ENTRY, hash-chain, or
    header mutation.
  - `rust/src/crates/jf/journal_file/src/writer.rs:830` tests constructor
    rejection without panic and with zero entries/tail seqnum.
  - `rust/src/crates/jf/journal_file/src/writer.rs:855` tests `add_entry()`
    rejection without mutation after the keyed flag is cleared after writer
    construction.
- Current writer behavior remains unchanged:
  - The current `journal-core` writer now also returns
    `JournalError::UnsupportedJournalFile` if the keyed flag is absent at
    append time after writer construction:
    `rust/src/crates/journal-core/src/file/writer.rs:691`.
  - `rust/src/crates/journal-core/src/file/writer.rs:1842` tests
    post-construction unkeyed `add_entry()` rejection without mutating
    `n_entries`, `tail_entry_seqnum`, or `tail_object_offset`.
  - SOW-0077 current writer regression tests still pass through
    `cargo test -p journal-core unkeyed_journal -- --nocapture`.
- Historical reader behavior remains unchanged:
  - This SOW changes only the legacy `jf` writer and error enum; no reader
    open/hash path was changed.

Tests or equivalent validation:

- PASS: `cargo fmt`
- PASS: `cargo test -p journal_file writer_rejects_unkeyed -- --nocapture`
- PASS: `cargo test -p journal_file -p error`
- PASS: `cargo test -p journal_reader_ffi`
- PASS: `cargo test -p journal-core unkeyed_journal -- --nocapture`
- PASS after reviewer blocker fix: `cargo fmt`
- PASS after reviewer blocker fix:
  `cargo test -p journal-core unkeyed_journal -- --nocapture`
- PASS after reviewer blocker fix:
  `cargo test -p journal_file -p error -p journal_reader_ffi -p journal-core`
- PASS after reviewer blocker fix: `git diff --check`
- NOTE: one attempted `cargo test` invocation passed two test filters and failed
  at Cargo argument parsing before running tests; it was rerun with the correct
  single filter shown above.

Real-use evidence:

- The legacy `journal_reader_ffi` crate still compiles and tests after adding
  the error variant. It does not expose the legacy writer.

Reviewer findings:

- Round 1:
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`. No blockers. Noted a
    pre-existing legacy `unreachable!()` invariant path, but not related to
    keyed-hash writer rejection.
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`. No blockers.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`. No blockers.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`. No blockers.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `NOT PRODUCTION GRADE`. Blocking
    finding: `rust/src/crates/journal-core/src/file/writer.rs` still had a
    keyed-hash `assert!` in the current writer append hot path. Fixed by
    returning `JournalError::UnsupportedJournalFile` before mutation and adding
    the post-construction no-mutation regression test.
- Round 2:
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`. Verified the
    `journal-core` append guard, legacy `jf` guards, same-failure scan, docs,
    specs, and SOW lifecycle. Noted the pre-existing non-keyed-hash
    `unreachable!()` invariant path as non-blocking.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`. Verified the
    `journal-core` append guard, legacy `jf` guards, guarded-keep decision,
    FFI safety, and same-failure scan.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`. Verified the
    round-1 blocker fix, full writer guard coverage, legacy error code safety,
    docs/spec consistency, and no data-loss risk.
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`. Verified no remaining
    keyed-hash assertion panic, no mutation before unsupported-file rejection,
    FFI reader-only impact, and no unwanted side effects.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`. Verified the
    original blocker was fixed, the current and legacy writer stacks reject
    unkeyed append targets before mutation, and no keyed-hash assertion path
    remains.

Same-failure scan:

- PASS after reviewer blocker fix. `rg` for keyed-hash `assert!` / `assert_eq!`
  in `rust/src/crates/jf` and `rust/src/crates/journal-core` found no
  remaining matches.

Sensitive data gate:

- PASS. Tests use synthetic temporary journal files and synthetic payloads only.

Artifact maintenance gate:

- AGENTS.md: no update needed; repository-wide writer compatibility rules
  already cover controlled errors and legacy/new writer separation.
- Runtime project skills: no update needed; this is a one-time legacy surface
  cleanup and existing journal compatibility skill already forbids writer
  assertion-panic failure modes.
- Specs: updated `.agents/sow/specs/product-scope.md` with the legacy `jf`
  writer stance.
- End-user/operator docs: updated `rust/README.md`.
- End-user/operator skills: no output/reference skills are affected.
- SOW lifecycle: moved from `pending/` to `current/`, then to `done/` after
  validation, reviewer closure, final audit, and commit preparation.
- SOW-status.md: updated from pending to current, then to recently completed.

Specs update:

- Updated `.agents/sow/specs/product-scope.md`.

Project skills update:

- No project skill update required.

End-user/operator docs update:

- Updated `rust/README.md`.

End-user/operator skills update:

- No end-user/operator skills are present for this behavior.

Lessons:

- Public legacy surfaces should be guarded rather than silently left with panic
  behavior, even when they are not the preferred production API.
- Removing a legacy public export is not necessary when the known failure mode
  can be fixed with a narrow controlled error and documentation.
- Same-failure scans must search both legacy and current writer stacks. If a
  legacy SOW exposes a remaining current-stack assertion of the same class, fix
  it immediately rather than leaving a second known panic path behind.

Follow-up mapping:

- No new follow-up is currently required. If reviewers find remaining legacy
  writer public-surface risk, disposition it in this SOW.

## Outcome

Completed.

The legacy Rust `jf` writer remains public but no longer panics on historical
unkeyed append targets. It now returns `JournalError::UnsupportedJournalFile`
from both construction and append paths before mutation. The same failure class
found during review in the current `journal-core` append path was fixed in the
same SOW, so both legacy and current Rust writer stacks now reject unkeyed
append targets with controlled errors.

## Lessons Extracted

- A legacy compatibility surface can still be operational debt when it is
  publicly exported, even if new integrations do not use it.
- Same-failure scans must include old compatibility crates and current
  production crates; the first reviewer batch found a current writer append
  assertion only because the review scope included both.
- Guarded keep was lower risk than removal here: it preserves the public API
  while removing the assertion-panic behavior.

## Followup

None.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
