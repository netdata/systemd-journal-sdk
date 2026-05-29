# SOW-0055 - Rust Seek Cursor Systemd Parity

## Status

Status: open

Sub-state: created from SOW-0045 review follow-up. Do not start until the
active SOW is closed and the user agrees this is the next priority.

## Requirements

### Purpose

Keep the Rust libsystemd-compatible reader facade aligned with actual
`sd_journal_seek_cursor()` semantics so Rust remains the behavioral reference
for the other SDKs.

### User Request

The user requested Go reader parity with Rust. SOW-0045 review exposed that Go,
Python, and Node.js follow upstream systemd's no-existence-proof
`seek_cursor()` behavior, while current Rust still scans for an existing cursor
and returns an error when the cursor is not found.

### Assistant Understanding

Facts:

- Upstream systemd `sd_journal_seek_cursor()` parses a cursor, stores the
  requested location, and returns success without scanning for an existing
  entry.
- Current Rust `JournalReader::seek_cursor()` scans entries and returns
  `SdkError::InvalidCursor` when no exact entry is found.
- Python and Node.js SOWs already recorded the systemd no-existence-proof
  behavior as the accepted facade behavior.

Inferences:

- Rust should be corrected to systemd behavior, not used to pull Go/Python/Node
  back to the stricter scan-and-error behavior.

Unknowns:

- Whether the Rust idiomatic SDK API should expose both strict exact-cursor
  search and libsystemd-compatible seek-location behavior, or whether only the
  facade should change.

### Acceptance Criteria

- Rust libsystemd-compatible facade `SdJournalSeekCursor()` matches upstream
  systemd no-existence-proof behavior for syntactically valid cursors.
- Rust retains or adds an explicit exact-cursor helper only if needed by
  existing tests or documented SDK use cases.
- Shared cursor tests cover found cursor, syntactically invalid cursor, and
  syntactically valid but nonexistent cursor behavior across Rust, Go, Python,
  and Node.js.
- Docs/specs state the accepted `seek_cursor()` contract consistently.

## Analysis

Sources checked:

- `rust/src/journal/src/lib.rs`
- `python/journal/facade.py`
- `node/src/facade.js`
- `go/journal/facade.go`
- `.agents/sow/done/SOW-0053-20260529-python-reader-writer-rust-port.md`
- `.agents/sow/done/SOW-0054-20260529-node-reader-writer-rust-port.md`
- `systemd/systemd @ cf3156842209`
  `src/libsystemd/sd-journal/sd-journal.c:1263`

Current state:

- Rust file reader `seek_cursor()` scans entries and returns
  `SdkError::InvalidCursor` on no exact match.
- Rust directory reader treats file-level `seek_cursor()` error as not found
  and returns `SdkError::InvalidCursor` if no file has the cursor.
- Python, Node.js, and Go accept a syntactically valid but nonexistent cursor as
  a seek location rather than an existence proof, matching systemd.

Risks:

- Changing Rust may affect tests that currently assume exact-cursor proof.
- Keeping Rust divergent weakens its role as the cross-language reference.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Rust kept an exact-cursor-search implementation from earlier SDK phases.
  Later Python and Node.js work verified upstream systemd behavior and adopted
  no-existence-proof seek semantics, but Rust was not realigned.

Evidence reviewed:

- `systemd/systemd @ cf3156842209`,
  `src/libsystemd/sd-journal/sd-journal.c:1263-1363`: parses cursor fields,
  stores `current_location`, and returns `0` without scanning entries.
- `rust/src/journal/src/lib.rs`: file and directory `seek_cursor()` return an
  error when no exact cursor is found.
- `.agents/sow/done/SOW-0053-20260529-python-reader-writer-rust-port.md`:
  records the upstream systemd evidence and accepted no-existence-proof Python
  behavior.
- `.agents/sow/done/SOW-0054-20260529-node-reader-writer-rust-port.md`:
  records the same behavior for Node.js.

Affected contracts and surfaces:

- Rust idiomatic reader API.
- Rust libsystemd-compatible facade.
- Shared cross-language cursor conformance tests.
- Reader docs/specs.

Existing patterns to reuse:

- Python and Node.js no-existence-proof facade behavior.
- Go SOW-0045 implementation and tests after review disposition.
- Existing Rust `test_cursor()` exact-match helper.

Risk and blast radius:

- Medium: cursor navigation is a public facade contract and affects journalctl
  query behavior.

Sensitive data handling plan:

- Use generated journal fixtures only. Do not record real logs, credentials,
  SNMP communities, bearer tokens, personal data, customer identifiers, or
  private endpoints.

Implementation plan:

1. Decide whether Rust idiomatic `seek_cursor()` changes directly or whether a
   facade-specific no-existence-proof wrapper is added.
2. Update Rust code and shared tests.
3. Update docs/specs and run Rust plus cross-language cursor tests.

Validation plan:

- Rust tests covering found, invalid, and syntactically valid nonexistent
  cursor cases.
- Shared facade cursor tests across Rust, Go, Python, and Node.js.
- File-backed journalctl cursor/query regression where applicable.
- Whole-SOW read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: no workflow update expected.
- Runtime project skills: no update expected.
- Specs: update reader/facade cursor behavior.
- End-user/operator docs: update Rust docs if public behavior changes.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: follow-up from SOW-0045.
- SOW-status.md: add this pending SOW.

Open-source reference evidence:

- `systemd/systemd @ cf3156842209`
  `src/libsystemd/sd-journal/sd-journal.c:1263-1363`

Open decisions:

- Blocked until prioritized: whether this SOW runs immediately after SOW-0045
  or waits behind reader performance work.

## Implications And Decisions

- No user decision recorded yet.

## Plan

1. Confirm desired Rust API shape for strict exact-cursor proof versus
   libsystemd-compatible seek-location behavior.
2. Implement and test Rust alignment.
3. Update shared tests, docs, and specs.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Read-only reviewers from the approved pool after implementation and local
  validation.

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

- Record any review disagreement about exact-cursor semantics with upstream
  systemd source evidence.

## Execution Log

### 2026-05-29

- Created from SOW-0045 reviewer finding and verified against upstream systemd
  source.

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

- Planning artifact contains only generated-fixture and public upstream source
  references.

Artifact maintenance gate:

- Pending implementation.

Specs update:

- Pending implementation.

Project skills update:

- Pending implementation.

End-user/operator docs update:

- Pending implementation.

End-user/operator skills update:

- Pending implementation.

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
