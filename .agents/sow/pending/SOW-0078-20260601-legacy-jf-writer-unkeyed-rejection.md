# SOW-0078 - Legacy jf Writer Unkeyed Rejection

## Status

Status: open

Sub-state: pending after SOW-0077 same-failure scan.

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

## Validation

Acceptance criteria evidence:

- Pending.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending.

Reviewer findings:

- Pending.

Same-failure scan:

- Pending.

Sensitive data gate:

- Pending.

Artifact maintenance gate:

- AGENTS.md: pending.
- Runtime project skills: pending.
- Specs: pending.
- End-user/operator docs: pending.
- End-user/operator skills: pending.
- SOW lifecycle: pending.
- SOW-status.md: added as pending during SOW creation.

Specs update:

- Pending.

Project skills update:

- Pending.

End-user/operator docs update:

- Pending.

End-user/operator skills update:

- Pending.

Lessons:

- Pending.

Follow-up mapping:

- Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
