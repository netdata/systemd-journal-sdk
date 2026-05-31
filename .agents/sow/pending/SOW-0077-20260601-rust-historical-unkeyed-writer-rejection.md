# SOW-0077 - Rust Historical Unkeyed Writer Rejection

## Status

Status: open

Sub-state: tracked from SOW-0073 reviewer finding; not started.

## Requirements

### Purpose

Prevent a historical-reader compatibility fix from leaving a writer API footgun:
attempting to append through the Rust writer to an old unkeyed journal should
fail with a controlled error, not an assertion panic.

### User Request

The user asked that technical debt not be left behind as journal compatibility
work progresses. During SOW-0073 review, a non-blocking Rust writer behavior
difference was identified and accepted as out of scope for that reader-only SOW.

### Assistant Understanding

Facts:

- SOW-0073 removed reader-only keyed-hash open gates for historical unkeyed
  journal files.
- Go, Python, and Node.js writer append-open paths explicitly reject historical
  unkeyed files.
- Rust-created files are keyed-hash by default.
- The Rust append hot path asserts that the opened journal has the keyed-hash
  flag before writing an entry.

Inferences:

- Rust currently prevents writing unkeyed append data, but the failure mode may
  be an assertion panic instead of a controlled API error.
- This is a writer API hardening/parity item, not a reader compatibility item.

Unknowns:

- Which public Rust writer constructors expose append-open on an existing
  historical unkeyed file and where the earliest clean error should be returned.

### Acceptance Criteria

- Rust writer append-open behavior for historical unkeyed files is audited from
  public API entry points down to `JournalWriter`.
- Attempting to append through supported Rust writer APIs to an unkeyed
  historical file returns a documented error before entry mutation, without
  panicking.
- Go, Python, and Node.js behavior is rechecked so the SOW records the
  cross-language writer contract.
- Tests cover Rust historical unkeyed append rejection using a synthetic
  fixture, without committing raw host journals.
- Reader support from SOW-0073 remains unchanged.

## Analysis

Sources checked:

- `rust/src/crates/journal-core/src/file/writer.rs`: the append hot path asserts
  `HeaderIncompatibleFlags::KeyedHash`.
- `rust/src/crates/journal-log-writer/src/log/mod.rs`: Rust creates new journal
  files with keyed-hash enabled.
- SOW-0073 reviewer batch: all reviewers accepted the reader work as
  production-grade and treated this writer item as non-blocking.

Current state:

- The exact Rust user-facing failure mode for appending to historical unkeyed
  files needs a focused audit and test.

Risks:

- Changing writer open behavior can affect directory writer reopen paths and
  append-to-existing workflows.
- This work must not weaken reader support for historical unkeyed files.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- The Rust writer has a keyed-hash assertion in the entry append path. If a
  public append-open API can reach that path with a historical unkeyed file, the
  caller may see a panic instead of a controlled unsupported-file error.

Evidence reviewed:

- `rust/src/crates/journal-core/src/file/writer.rs`: keyed-hash assertion in
  `add_entry_fields_with_options`.
- `rust/src/crates/journal-log-writer/src/log/mod.rs`: new files use
  `.with_keyed_hash(true)`.
- SOW-0073 reviewer notes.

Affected contracts and surfaces:

- Rust writer append-open API.
- Rust directory writer reopen behavior, if it can reopen historical files.
- Cross-language writer unsupported-file behavior.
- Tests and documentation for historical unkeyed writer behavior.

Existing patterns to reuse:

- Historical 240-byte header fixtures from SOW-0073.
- Existing writer append-open rejection tests in Go, Python, and Node.js.
- Rust `JournalError` unsupported/invalid-file errors.

Risk and blast radius:

- Low if the change is limited to an earlier explicit Rust error.
- Medium if directory writer active-file discovery needs to distinguish old
  historical files from current active SDK files.

Sensitive data handling plan:

- Use synthetic historical headers or generated fixtures only. Do not copy raw
  host journal payloads or durable field values into this repository.

Implementation plan:

1. Audit Rust public append-open entry points and identify the earliest common
   place to reject unkeyed historical files with an error.
2. Add a Rust test that opens a synthetic unkeyed historical journal for append
   and verifies a controlled error before mutation.
3. Recheck Go, Python, and Node.js writer behavior and document the shared
   contract.

Validation plan:

- Rust targeted tests for append rejection.
- Existing Rust reader historical unkeyed test/matrix still passes.
- Relevant writer smoke tests still pass.
- Whole-SOW read-only reviewer batch after implementation.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: update only if a new mandatory writer rule is
  established.
- Specs: update historical writer behavior if the public contract changes.
- End-user/operator docs: update Rust writer docs if append-open behavior is
  documented there.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: normal pending to current to done lifecycle.
- SOW-status.md: update when started/completed.

Open-source reference evidence:

- None needed yet; this is a local SDK API hardening item.

Open decisions:

- None yet. Investigation may expose whether Rust should reject at file-open
  time or writer-construction time.

## Implications And Decisions

No user decision is required before investigation. If the audit finds multiple
publicly visible failure locations with different compatibility implications,
the options should be brought back to the user before implementation.

## Plan

1. Audit Rust append-open paths and writer construction.
2. Implement controlled error and tests.
3. Validate reader support remains unchanged.
4. Run whole-SOW reviewer batch.

## Delegation Plan

Implementer:

- Local implementation unless the user explicitly changes routing.

Reviewers:

- Reviewer pool after complete implementation and local validation: minimax,
  kimi, qwen, glm, and mimo.

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

- If a clean error path requires a public API decision, pause and ask the user
  with evidence and options.

## Execution Log

### 2026-06-01

- Created as follow-up tracking from SOW-0073 reviewer findings.

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
- SOW-status.md: pending.

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

Pending.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
