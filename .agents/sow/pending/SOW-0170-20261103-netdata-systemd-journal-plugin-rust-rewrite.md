# SOW-0170 - Rust Rewrite Of Netdata systemd-journal.plugin

## Status

Status: open

Sub-state: tracked 2026-11-03 by user decision; not now. Activation needs a new
user decision and explicit authorization to change the Netdata repository.

## Requirements

### Purpose

Netdata's `systemd-journal.plugin` uses the SDK natively, without FFI, so the
SDK's reader, index and facet paths serve the system logs viewer directly.

### User Request

"The right solution is to rewrite the systemd-journal.plugin in rust for that
it is natural without FFI. but not yet please." (2026-11-03, while deciding
SOW-0164.)

### Assistant Understanding

Facts:

- `systemd-journal.plugin` is C. It reads journals through libsystemd, or
  through vendored `jf` (`journal_reader_ffi`) in Docker and static builds
  (`netdata/netdata @ eaeb95f5c4`, `packaging/docker/Dockerfile:42`,
  `packaging/makeself/jobs/70-netdata-git.install.sh:40`).
- SOW-0164 keeps libsystemd where available and replaces vendored `jf` with
  the SDK's published C reader crate (SOW-0162); that is the transition step.

Inferences:

- A Rust plugin removes the C/Rust FFI boundary and can use the SDK's
  indexed query paths instead of a libsystemd-shaped facade.

Unknowns:

- Plugin feature inventory, the Netdata function protocol surface it must
  keep, and whether libsystemd-only features (for example namespaces or the
  live system journal) must stay.

### Acceptance Criteria

- To be written at activation.

## Analysis

Sources checked:

- SOW-0164 facts and the Netdata citations above.

Current state:

- Not started.

Risks:

- Large cross-repository change to a user-facing Netdata feature.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Not analyzed yet; not scheduled.

Evidence reviewed:

- See Assistant Understanding.

Affected contracts and surfaces:

- Netdata `systemd-journal.plugin`, its function protocol and packaging; SDK
  Rust reader and explorer APIs.

Existing patterns to reuse:

- Netdata `netflow-plugin`, a Rust plugin that already uses published SDK
  crates.

Risk and blast radius:

- See Risks.

Sensitive data handling plan:

- Synthetic journals only in SDK artifacts.

Implementation plan:

1. To be written at activation, after SOW-0164.

Validation plan:

- To be written at activation.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected.
- Specs: Netdata integration status in product scope.
- End-user/operator docs: likely unaffected in this repository.
- End-user/operator skills: none exist.
- SOW lifecycle: follows SOW-0164.
- SOW-status.md: updated on activation and close.

Open-source reference evidence:

- `netdata/netdata @ eaeb95f5c4`: paths cited under Facts.

Open decisions:

1. When to schedule it, and Netdata repository write authorization.

## Implications And Decisions

1. 2026-11-03 (user): track it; do not do it yet.

## Plan

Pending activation.

## Implementation And Review Plan

Implementation:

- Project manager, at activation.

Reviewers:

- Whole-SOW self-review after local validation; tests are the gate. External
  reviewers are not used.

## Execution Log

### 2026-11-03

- Created from the SOW-0164 decision.

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
