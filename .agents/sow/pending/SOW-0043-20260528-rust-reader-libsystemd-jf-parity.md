# SOW-0043 - Rust Reader Libsystemd/Jf Parity

## Status

Status: open

Sub-state: created on 2026-05-28 as the reader compatibility baseline SOW.

## Requirements

### Purpose

Define and close the Rust reader compatibility target before reader performance
optimization or other language reader alignment. Rust must be the reader
reference only after it is audited against libsystemd behavior and Netdata's
`jf` facade needs.

### User Request

The user wants the reader phase to mirror the writer phase:

- align Rust to libsystemd and the Netdata `jf` facade;
- preserve and port the `jf` libsystemd-like reader API concept;
- then optimize Rust reader performance;
- then align Go, Python, and Node.js.

The user also agreed to fold RAW byte field-name reader representation into
this reader parity SOW.

### Assistant Understanding

Facts:

- Netdata has a `jf` crate that provides a libsystemd-like API for reader
  callers.
- SDK readers must support ordered multi-file directory reading.
- Reader APIs must represent RAW byte field names without losing information.
- Reader work must include single-file and directory readers.

Inferences:

- Rust reader parity must come before Rust reader optimization; otherwise
  optimization may target the wrong API/behavior.

Unknowns:

- Exact libsystemd reader calls required by all Netdata consumers at the final
  integration commit.
- Exact byte-preserving reader API shape across all languages.

### Acceptance Criteria

- Audit Rust reader against libsystemd file-backed behavior relevant to this
  SDK.
- Audit Rust reader against Netdata `jf` facade behavior required by known
  Netdata consumers.
- Define the shared reader API layers: idiomatic SDK reader and
  libsystemd-compatible facade.
- Define RAW byte field-name representation for Rust, Go, Node.js, and Python.
- Ensure directory readers support mixed formats in one directory:
  compression on/off, mixed compression algorithms, compact on/off, FSS on/off,
  open/closed files, and historical compatible files.
- Identify any Rust reader correctness gaps and either fix them here or split a
  concrete follow-up SOW before close.
- Update specs and docs for reader contracts.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/specs/product-scope.md`
- `.agents/sow/done/SOW-0027-20260526-netdata-reader-api-and-jf-facade.md`
- `.agents/sow/done/SOW-0024-20260526-mixed-format-directory-readers.md`
- `.agents/sow/done/SOW-0039-20260528-raw-byte-field-name-reader-representation.md`

Current state:

- Rust reader exists and supports directory reading, but a fresh parity audit is
  needed before performance work.
- RAW byte-name reader representation was originally tracked separately and is
  now folded into this SOW.

Risks:

- Optimizing Rust reader before parity may bake in an incomplete API.
- String-keyed convenience maps can lose RAW byte field-name identity unless a
  byte-preserving surface is defined.

## Pre-Implementation Gate

Status: blocked until SOW-0037 writer closure is complete or explicitly paused
in favor of reader work

Problem / root-cause model:

- Reader performance work needs a stable correctness target. The target is
  libsystemd-compatible file-backed behavior plus Netdata `jf` facade needs,
  not just current SDK reader behavior.

Evidence reviewed:

- Product scope reader sections.
- SOW-0027 reader API/facade history.
- SOW-0024 mixed-directory reader history.
- SOW-0039 RAW byte-name gap.

Affected contracts and surfaces:

- Rust reader API.
- Cross-language reader API model.
- `jf`/libsystemd-compatible facades.
- Directory readers, query, unique/facet scans, cursors, seek behavior,
  journalctl rewrites, and Netdata integration readiness.

Existing patterns to reuse:

- Existing Rust `DirectoryReader`.
- Existing shared fixtures and conformance tests.
- Existing `jf` facade analysis from SOW-0027.

Risk and blast radius:

- High. This defines the reader reference for all other languages and Netdata
  reader integrations.

Sensitive data handling plan:

- Use generated or public fixtures only. Do not record real customer logs,
  SNMP communities, credentials, bearer tokens, personal data, private
  endpoints, or production incident details.

Implementation plan:

1. Inventory libsystemd and `jf` reader calls relevant to this SDK.
2. Audit Rust reader behavior against that inventory.
3. Design byte-preserving RAW field-name representation.
4. Fix or track correctness gaps.
5. Update specs/docs/tests.

Validation plan:

- Rust reader tests.
- Shared reader conformance and mixed-directory tests.
- Cross-language fixture readback where relevant.
- Read-only reviewer passes.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update compatibility skill if reader workflow changes.
- Specs: update reader contract.
- End-user/operator docs: update reader API docs.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: close before reader optimization SOWs.
- SOW-status.md: update when activated and closed.

Open-source reference evidence:

- systemd/libsystemd evidence must be collected during implementation and
  cited as owner/repo plus commit and repository-relative paths.

Open decisions:

- Byte-preserving reader API shape may need a user decision after evidence is
  presented.

## Implications And Decisions

- 2026-05-28: user agreed RAW byte field-name representation folds into reader
  parity instead of remaining a standalone SOW.

## Plan

1. Activate after writer closure or explicit user reprioritization.
2. Complete Rust reader parity audit.
3. Fix or track correctness gaps.
4. Update specs and docs.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Read-only reviewers from the approved pool.

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

- Record parity gaps, user decisions, reviewer findings, and audit failures in
  this SOW before moving to performance work.

## Execution Log

### 2026-05-28

- Created from the agreed SOW restructuring.

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

- Pending implementation; planning text contains no raw sensitive data.

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

- Pending implementation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
