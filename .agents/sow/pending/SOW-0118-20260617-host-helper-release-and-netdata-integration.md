# SOW-0118 - Host helper release and Netdata integration planning

## Status

Status: open

Sub-state: pending after SOW-0115; blocked until SOW-0115 is completed and committed.

## Requirements

### Purpose

Release the Rust/Go strict writer and optional host-helper work from SOW-0115 in a version Netdata can consume, then plan the Netdata-side adoption for NetFlow and SNMP traps without changing files outside this repository.

### User Request

After finishing the portable writer identity/helper work, discuss how the resulting SDK version should be integrated into Netdata for NetFlow and SNMP traps on Windows, FreeBSD, and macOS.

### Assistant Understanding

Facts:

- SOW-0115 changes the Rust and Go writer/helper API surface after the already completed `0.7.2` release.
- This repository cannot modify the Netdata repository under the repository-boundary rule.
- NetFlow is a Rust consumer and SNMP traps is a Go consumer.

Inferences:

- A follow-up SDK release is needed before Netdata can consume the new helper APIs from a published version.
- Netdata-side code changes should be planned with explicit caller-owned identity choices, not automatic SDK writer fallback behavior.

Unknowns:

- The exact next SDK release version is not decided here.
- Netdata-side integration details must be confirmed in the Netdata repository under a separate user-approved work item.

### Acceptance Criteria

- Decide and execute or explicitly defer the SDK release containing SOW-0115.
- Produce a concrete Netdata integration plan for Rust NetFlow and Go SNMP traps, with no writes outside this repository unless the user starts a Netdata-repo SOW.
- Record any required Netdata-repo follow-up with exact files/surfaces after read-only inspection.

## Analysis

Sources checked:

- SOW-0115 outcome and follow-up mapping.
- Existing pending release/integration SOW list.

Current state:

- SOW-0115 is still in progress; this SOW must not start implementation until SOW-0115 closes.

Risks:

- Releasing without confirming downstream import paths may force another SDK release.
- Netdata event sources may need different identity semantics: local collector host, remote network device, or synthetic per-flow/per-trap anchors.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- SOW-0115 introduces new public Rust/Go helper packages and stricter writer behavior that downstream Netdata consumers cannot use until released and planned.

Evidence reviewed:

- SOW-0115 active SOW and validation.

Affected contracts and surfaces:

- Rust crates, Go module tags, docs, and downstream Netdata integration guidance.

Existing patterns to reuse:

- Project release SOWs, especially SOW-0117.
- Prior Netdata integration SOWs, especially SOW-0047 through SOW-0050.

Risk and blast radius:

- Release/versioning, downstream build breakage, and incorrect event identity semantics.

Sensitive data handling plan:

- No raw Netdata customer or production data is needed. Any Netdata examples must use synthetic IDs and redacted paths.

Implementation plan:

1. Confirm SOW-0115 close state and decide the next SDK release version.
2. Run release validation and publish/tag only after user approval.
3. Inspect Netdata consumers read-only and write a concrete integration plan.

Validation plan:

- Release validation follows the project release-tagging skill.
- Integration planning cites file paths and line numbers from read-only inspection.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected unless release process changes.
- Specs: update only if release/integration changes public contracts.
- End-user/operator docs: update release/install docs if version changes.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: stays pending until SOW-0115 closes.
- SOW-status.md: updated when this SOW becomes active or changes status.

Open-source reference evidence:

- None yet; this pending SOW is a follow-up tracker.

Open decisions:

- Next SDK release version.
- Whether Netdata integration work happens in this repository as planning only or in the Netdata repository under a separate SOW.

## Implications And Decisions

Pending.

## Plan

1. Close SOW-0115.
2. Decide release version and validation scope.
3. Inspect Netdata Rust/Go consumers read-only.
4. Produce integration plan and follow-up mapping.

## Delegation Plan

Implementer:

- Pending; decide when this SOW starts.

Reviewers:

- Pending; follow project reviewer cadence.

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

- Record release, validation, or integration-inspection blockers in this SOW.

## Execution Log

### 2026-06-17

- Pending SOW created as SOW-0115 follow-up mapping for release and Netdata integration planning.

## Validation

Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

Pending.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
