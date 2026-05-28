# SOW-0041 - Node.js Writer Rust Parity

## Status

Status: open

Sub-state: created on 2026-05-28 from the agreed writer completion plan.

## Requirements

### Purpose

Align Node.js writer API and file-format behavior with the finalized Rust writer
contract, while recording runtime-specific implementation limits honestly.

### User Request

The user agreed Node.js must align its API and internal operations with Rust
after Rust and Go writer closure. Node.js may not support the same mmap strategy
in the common runtime path, but this must be evidence-based.

### Assistant Understanding

Facts:

- Node.js writer currently uses Buffer construction and file writes rather than
  a Rust-like mmap strategy.
- Node.js writer must share the same field-name policy modes and public writer
  API concepts as the other languages.
- SOW-0037 validation found Node.js has a cooperative writer lock
  implementation, but cross-process contention fails because Node.js contenders
  can still acquire/publish while another SDK writer holds the lock.
- Common compression libraries are allowed, including packages that provide a
  maintainable pure-runtime or acceptable non-linking path.

Inferences:

- The target is API and compatibility parity first; performance parity may be
  limited by runtime constraints.

Unknowns:

- Whether a practical Node.js mmap package is acceptable under the project
  runtime constraints.

### Acceptance Criteria

- Node.js writer API and options match the agreed writer contract from SOW-0037.
- Node.js writer supports the same field-name policy modes and raw/structured
  append semantics.
- Node.js writer fixes the existing cooperative writer lock cross-process
  contention bug and participates in the same lock contract as Rust and Go,
  including contention rejection and stale lock cleanup.
- Node.js writer internal behavior is aligned with Rust where practical, and
  every runtime-specific difference is recorded with evidence.
- Node.js writer passes shared writer conformance and interoperability tests.
- Node.js writer outputs remain readable by stock systemd tooling where the
  selected policy mode is systemd-friendly.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `node/src/lib/writer.js`
- `.agents/sow/specs/product-scope.md`

Current state:

- Node.js writer is functionally capable but has runtime-specific file access
  and allocation behavior that must be classified. It has an existing
  cooperative writer lock implementation, but the all-language lock matrix
  shows a cross-process contention bug versus the Rust/Go lock behavior.

Risks:

- Native addon dependencies could violate the runtime policy if they are loaded
  in the SDK path.
- Trying to force Rust internals into Node.js may reduce maintainability without
  helping users.

## Pre-Implementation Gate

Status: blocked until SOW-0037 closes the writer reference contract

Problem / root-cause model:

- Node.js should implement the finalized writer contract, not chase
  implementation details before the Rust/Go reference is closed.

Evidence reviewed:

- Current Node.js writer implementation and product scope spec.

Affected contracts and surfaces:

- Node.js writer API, directory writer behavior, compression/FSS/compact output,
  field-name policy, binary fields, cooperative writer lock contention
  behavior, and benchmark claims.

Existing patterns to reuse:

- Rust and Go writer contracts from SOW-0037.
- Existing Node.js tests and shared conformance fixtures.

Risk and blast radius:

- Medium for Node.js users; low for Rust/Go production hot paths.

Sensitive data handling plan:

- Use synthetic fixtures only. Do not record real logs, SNMP communities,
  customer data, personal data, credentials, bearer tokens, private endpoints,
  or production incident details.

Implementation plan:

1. Wait for SOW-0037.
2. Compare Node.js writer against the finalized Rust writer matrix.
3. Align API and internal behavior where practical, including fixing the
   cooperative writer lock cross-process contention bug.
4. Record measured runtime-specific differences.
5. Run conformance, interoperability, lock, and benchmark checks.

Validation plan:

- Node.js test suite.
- Shared writer conformance suite.
- Cross-language readback and stock `journalctl --verify --file`.
- Read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if dependency/runtime workflow changes.
- Specs: update Node.js writer feature slice.
- End-user/operator docs: update Node.js API docs.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: pending until SOW-0037 closes.
- SOW-status.md: updated when activated and closed.

Open-source reference evidence:

- No external open-source source was checked for this planning SOW.

Open decisions:

- None now. Any Node.js mmap/dependency choice requires evidence before
  selection.

## Implications And Decisions

- 2026-05-28: user agreed Node.js writer parity follows Rust/Go writer closure.
- 2026-05-28: SOW-0037 lock validation showed Node.js has cooperative writer
  lock code, but Node.js contenders can still acquire/publish while another SDK
  writer holds the lock; this SOW owns that Node.js cross-process contention
  bug fix.

## Plan

1. Activate after SOW-0037.
2. Implement Node.js writer parity.
3. Validate and review.

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

- Record runtime limits, dependency findings, reviewer findings, and benchmark
  failures in this SOW before changing scope.

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
