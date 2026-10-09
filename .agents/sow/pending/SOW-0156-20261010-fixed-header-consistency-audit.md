# SOW-0156 - Fixed-Header Consistency Audit

## Status

Status: open

Sub-state: separate follow-up agreed by the user; not activated by PR #9's
concrete array-count correction. No implementation is authorized here yet.

## Requirements

### Purpose

Establish a systematic, paired model of stable fixed-header consistency instead
of discovering missing field relationships one review finding at a time.

### User Request

The user agreed that defining the invariants, auditing both validators, then
implementing and testing missing checks belongs in separate work and a separate
PR. Scope clarification: PR #9 issuecomment-6090888921.

### Assistant Understanding

Facts: both SDKs have shared stable-header checks and separate live-reader bounds.
PR #9 added population budgets and concrete array-reference consistency checks.
Inference: a systematic model may expose additional missing relationships.
Unknowns: additional verified omissions and their supported-format implications.
No such omission is presumed to be a defect without format and reachability evidence.

### Acceptance Criteria

- Enumerate fixed-header invariants and complete-field gates from the supported
  format, including total budgets, per-type minima and pointer relationships.
- Audit both SDKs against the same model; classify every difference with evidence.
- Implement authorized missing checks and paired tests in a separate PR, preserving
  valid historical files, ordinary live readers and header-only cost.

## Analysis

Source baseline: PR #9 and SOW-0152-20261008; shared header validators, format
parsers and writer publication paths. Re-ground on the merged result before work.
Risks: conflating stable/live states, assuming fields exist in old headers, or
expanding header validation into object traversal without a product decision.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model: successive reviews found relationships missing from
an incomplete header consistency model. This is a hypothesis to audit systematically.
Evidence reviewed: reviews 5475128273 and 5475871638, paired source checks, and the
user-approved scope separation recorded at issuecomment-6090888921.
Affected contracts and surfaces: Go/Rust stable validators, historical field
parsing, inventory/recovery/snapshot consumers, specs and paired tests.
Existing patterns to reuse: complete-field gates, checked population budgets,
real-file preservation tests and separate live-reader mapping validation.
Risk and blast radius: compatibility, safe deletion/recovery and repeated capture
cost. No broader traversal or format change is authorized by this record.
Sensitive data handling plan: synthetic fixtures, sanitized source evidence and
repo-local or /tmp outputs only; no host journals or private identities.
Implementation plan: define invariants, audit both SDKs, present any contract
forks, then implement and test the approved gaps in a separate PR.
Validation plan: positive historical/live fixtures, negative invariant mutations,
paired real-file tests and unchanged complexity of header-only operations.
Artifact impact plan: update product scope, compatibility guidance, tests and
SOW indexes as needed; user guides only if documented behavior needs clarification.
Open-source reference evidence: systemd/systemd @
c0a5a2516d28601fb3afc1a77d7b42fcfe38fced, docs/JOURNAL_FILE_FORMAT.md.
Open decisions: activation and any behavior changes exposed by the audit.

## Implications And Decisions

1. The user placed this broader work in a separate PR. The verified array-count
   finding remains in SOW-0152 and is not deferred here.

## Plan

1. Define the complete fixed-header invariant model.
2. Audit Go/Rust against it and resolve material scope or contract decisions.
3. Implement and test verified missing checks together.

## Implementation And Review Plan

Not active. The future owner must complete the gate and select review coverage
for discovered compatibility and mutation implications before implementation.

## Execution Log

### 2026-10-10

- Recorded the separate follow-up approved during PR #9 review discussion.

## Validation

No implementation or validation run is claimed. Initial scope was checked against
SOW-0152 and the public scope clarification. This record contains no sensitive data.
Artifact maintenance: new pending record and both status indexes; no runtime,
public docs or dependency changes are delivered by this follow-up yet.

## Outcome

Pending activation in a separate PR.

## Lessons Extracted

Derive checks from format invariants, including relationships between fields.

## Followup

No additional work item; this SOW is the agreed follow-up.

## Regression Log

No completed outcome has regressed in this pending SOW.
