# SOW-0162 - Publishable C Reader Crate

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: created on 2026-10-31 by user decision. Not activated; runs after the
debt SOWs in the backlog order in `.agents/sow/SOW-status.md` and before the
v0.11.0 release (SOW-0163).

## Requirements

### Purpose

Let Netdata's `systemd-journal.plugin` consume this SDK's C reader as a
published dependency, the same way `netflow-plugin` consumes the Rust crates,
so Netdata's vendored `src/crates/jf` copy can be deleted (SOW-0164).

### User Request

The user chose to "publish the C reader as an SDK crate, so Netdata uses it
like netflow uses the SDK", with the goal that the old Netdata crates vanish
and Netdata uses the SDK exclusively.

### Assistant Understanding

Facts:

- Netdata's `systemd-journal.plugin` calls the `rsd_journal_*` C functions
  exported by `journal_reader_ffi` from its vendored `src/crates/jf`
  workspace (`packaging/cmake/Modules/NetdataPluginSystemdJournal.cmake:55-57`,
  `NetdataRust.cmake:61-63`). Docker and static builds enable it with
  `--internal-systemd-journal`.
- This repository has its own evolved `rust/src/crates/jf` with the same
  `rsd_journal_*` entry points (spot-checked: `open_files`, `query_unique`,
  `seek_realtime_usec`). It is `publish = false`.
- The two copies differ by roughly 4,000 diff lines; Netdata's lacks later SDK
  fixes.

Inferences:

- The crate should be built on the SDK's published reader/facade crates
  rather than kept as a separate reader implementation, so one reader exists.
  Needs confirmation during analysis.

Unknowns:

- Whether every symbol and struct layout used by the C plugin is present and
  ABI-identical in the SDK copy.
- Whether `jf` can be thinned onto the SDK facade without behavior change.

### Acceptance Criteria

- A publishable crate exposes every C function and type Netdata's
  `systemd-journal.plugin` uses, verified against the plugin's call sites.
- The crate is part of the release workflow's publish order and dry-runs.
- C-level tests exercise the exported API against shared fixtures.
- SOW-0098 (`jf` duplication debt) is resolved or explicitly folded in.

## Analysis

Sources checked:

- Netdata `src/crates/jf`, `src/collectors/systemd-journal.plugin`, CMake modules.
- `rust/src/crates/jf` in this repository.

Current state:

- See Facts above.

Risks:

- ABI mismatch breaks the C plugin at link or run time.
- Adding a published crate changes release order and Trusted Publishing setup
  (a new crate needs an authorized initial publication).

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Netdata vendors a drifting copy of the C reader because the SDK's copy is not published.

Evidence reviewed:

- Listed under Assistant Understanding.

Affected contracts and surfaces:

- New public crate and C ABI; release workflow; `project-release` skill crate list.

Existing patterns to reuse:

- Published `systemd-journal-sdk-*` crates and the release workflow.

Risk and blast radius:

- See Risks. Rust-only by nature (C ABI); the Go SDK is unaffected. Record this
  as a parity exception requiring user confirmation.

Sensitive data handling plan:

- No sensitive data expected; synthetic fixtures only.

Implementation plan:

1. To be written at activation from the symbol inventory.

Validation plan:

- To be written at activation.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: `project-release` crate list and order.
- Specs: product scope gains the C reader crate.
- End-user/operator docs: README and `docs/` for the C reader.
- End-user/operator skills: none exist.
- SOW lifecycle: prerequisite of SOW-0163 and SOW-0164.
- SOW-status.md: updated on activation and close.

Open-source reference evidence:

- Netdata repository (`netdata/netdata`) is the consumer; cite `owner/repo @ commit` at activation.

Open decisions:

1. Go parity exception for a C-ABI crate (expected: Rust-only).
2. Crate name and whether it wraps the SDK facade or keeps `jf` internals.

## Implications And Decisions

- 2026-10-31: user chose a published SDK crate over re-copying `jf` into Netdata.

## Plan

1. Activate after the debt SOWs; fill the gate from a symbol inventory.

## Implementation And Review Plan

Implementation:

- Project manager implements and validates.

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

- Record failures in this SOW and stop for the user on design changes.

## Execution Log

### 2026-10-31

- Created from the SOW backlog review.

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

- Pending.

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

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
