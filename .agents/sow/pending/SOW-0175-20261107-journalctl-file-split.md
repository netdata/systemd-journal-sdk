# SOW-0175 - Split The journalctl Rewrites By Responsibility

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: tracked by user decision on 2026-11-07 (SOW-0097 decision 7).

## Requirements

### Purpose

Make the journalctl rewrites easier to maintain by splitting their large
entry files by responsibility, without changing behavior.

### User Request

During SOW-0097 the user allowed free refactoring of the journalctl rewrites
and chose to track the file split separately, so the single-ownership diff
stays reviewable.

### Assistant Understanding

Facts:

- `go/cmd/journalctl/main.go` was 4,131 lines on 2026-11-07. The Rust
  `rust/src/cmd/journalctl/main.rs` is similarly large.
- SOW-0097 removes duplicated logic from both files first.

Inferences:

- A pure move within one package/crate has low behavioral risk but a large
  diff that conflicts with concurrent CLI work (for example SOW-0168).

Unknowns:

- Final responsibility boundaries; decided at activation from the post-SOW-0097
  code.

### Acceptance Criteria

- Both CLIs split into files by responsibility (flags/dispatch, time parsing,
  boots, output, verify/header/vacuum), with no logic change.
- Unit tests and the journalctl query and directory oracle matrices pass
  unchanged.

## Analysis

To be refreshed at activation, after SOW-0097 lands.

## Pre-Implementation Gate

Status: not started

Problem / root-cause model: entry files grew as features landed.

Evidence reviewed: line counts on 2026-11-07.

Affected contracts and surfaces: none intended; CLI internals only.

Existing patterns to reuse: existing split files such as
`go/cmd/journalctl/output.go`, `timestamp.go`.

Risk and blast radius: low behavior risk; merge conflicts with other CLI work.

Sensitive data handling plan:

- Not applicable; code moves only.

Implementation plan: decide boundaries, move code, run tests and matrices.

Validation plan: `go test ./...`, Rust workspace tests,
`run_journalctl_query_matrix.py`, `run_directory_matrix.py`.

Artifact impact plan: none expected beyond SOW-status.md.

Open-source reference evidence: not applicable.

Open decisions: none; scheduling is the user's.

## Implications And Decisions

- 2026-11-07: user chose a separate SOW for the split.

## Plan

Activate after SOW-0097.

## Implementation And Review Plan

Project manager implements; test suite plus self-review.

## Execution Log

None.

## Validation

Pending.

Sensitive data gate:

- Pending.

## Outcome

Pending.

## Lessons Extracted

None yet.

## Followup

None.

## Regression Log

None.
