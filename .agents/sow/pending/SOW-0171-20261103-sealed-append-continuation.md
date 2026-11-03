# SOW-0171 - Continue Sealing When Reopening A Sealed File

## Status

Status: open

Sub-state: tracked 2026-11-03 by user decision N1 (option B as follow-up);
not scheduled.

## Requirements

### Purpose

Let writers keep appending to an existing sealed journal file and keep sealing
it, when the caller supplies the sealing key, as systemd does. Without the key
the file stays refused for append.

### User Request

Decision "n1a" (2026-11-03, SOW-0156 finding N1): refuse append-open of sealed
files now (option A, done in SOW-0156) and track continuing sealing with the
key (option B) as its own SOW.

### Assistant Understanding

Facts:

- Since SOW-0156, Go `validateEntryAppendTarget` and Rust
  `validate_entry_append_target` refuse sealed files as unsupported, so `Log`
  sets a sealed active aside and starts a new sealed file.
- systemd opens a sealed file for writing only after loading its sealing key
  (systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced,
  src/libsystemd/sd-journal/journal-file.c:4230-4233).
- The Go `Log` creates sealed files from `LogConfig.Options.Seal`; the Rust
  `journal-log-writer` `Config` has no sealing option (no `seal` reference in
  `rust/src/crates/journal-log-writer/src`). Rust sealing is available only
  through the core writer (`JournalFileOptions::with_seal`).

Inferences:

- Continuing a file needs the sealing state at the last TAG (epoch and HMAC
  chain position) restored from the key and the file, before the first new
  entry.

Unknowns:

- Whether the Rust `Log` sealing gap above is a deliberate scope choice or an
  unrecorded parity gap; it needs a user decision before or with this SOW.

### Acceptance Criteria

- To be written at activation.

## Analysis

Sources checked:

- SOW-0156 finding N1 and the code paths cited above.

Current state:

- Not started.

Risks:

- A wrong restored state produces files that fail keyed verification, which
  is worse than refusing the append.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Not analyzed yet; not scheduled.

Evidence reviewed:

- See Assistant Understanding.

Affected contracts and surfaces:

- Go `Writer` append-open and `Log`; Rust core append-open and, if decided,
  Rust `Log` sealing configuration.

Existing patterns to reuse:

- The existing sealing writers and keyed verifiers in both languages.

Risk and blast radius:

- See Risks.

Sensitive data handling plan:

- Synthetic seeds and journals only.

Implementation plan:

1. To be written at activation.

Validation plan:

- To be written at activation; keyed verification by both SDK verifiers and
  stock `journalctl --verify --verify-key` on continued files.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected.
- Specs: writer append-open rules in product scope.
- End-user/operator docs: `docs/Writer-APIs.md` append-open refusals.
- End-user/operator skills: none exist.
- SOW lifecycle: standard.
- SOW-status.md: updated on activation and close.

Open-source reference evidence:

- systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced, cited above.

Open decisions:

1. When to schedule it.
2. Whether the Rust `Log` gains a sealing option (parity with Go).

## Implications And Decisions

1. 2026-11-03 (user): refuse now (SOW-0156); track continuation here.

## Plan

Pending activation.

## Implementation And Review Plan

Implementation:

- Project manager, at activation.

Reviewers:

- Whole-SOW self-review after local validation; tests are the gate. External
  reviewers are not used.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- Do not make changes outside this repository for any reason.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- Validation or audit failures are recorded in the Execution Log with
  evidence; the SOW does not advance until they are fixed or a user decision
  is recorded.

## Execution Log

### 2026-11-03

- Created from SOW-0156 decision N1.

## Validation

Pending implementation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
