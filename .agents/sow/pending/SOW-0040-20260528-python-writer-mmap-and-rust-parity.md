# SOW-0040 - Python Writer Mmap And Rust Parity

## Status

Status: open

Sub-state: created on 2026-05-28 from the agreed writer completion plan.

## Requirements

### Purpose

Bring the Python writer as close as practical to the Rust writer contract after
Rust and Go writer closure, including API behavior, file-format behavior, and
hot-path implementation choices that are available in Python.

### User Request

The user identified Python as the next writer implementation needing mmap and
alignment with Rust after Rust and Go writer closure.

### Assistant Understanding

Facts:

- Python writer currently uses `pwrite`/`pread`-style file access rather than
  the Rust windowed mmap strategy.
- Python must expose the same writer API concepts as Rust, Go, and Node.js.
- Common libraries are allowed for compression; journal parsing/writing must
  remain independent of systemd/libjournal.

Inferences:

- Python may not match Rust/Go hot-path performance, but it should avoid
  unnecessary divergence where Python runtime features can support the same
  contract.

Unknowns:

- The best Python mmap strategy must be measured before it becomes the default.

### Acceptance Criteria

- Python writer API and options match the agreed writer contract from SOW-0037.
- Python writer supports the same field-name policy modes and raw/structured
  append semantics as the other languages.
- Python writer uses mmap or a measured alternative with evidence explaining
  any difference from Rust.
- Python writer passes shared writer conformance and interoperability tests.
- Python writer outputs remain readable by stock systemd tooling where the
  selected policy mode is systemd-friendly.
- Any performance tradeoff is measured and recorded.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `python/journal/writer.py`
- `.agents/sow/specs/product-scope.md`

Current state:

- Python writer is functionally capable but not yet aligned with the Rust mmap
  model.

Risks:

- Python mmap behavior differs by platform and may complicate flush and resize
  semantics.
- Performance work can accidentally weaken compatibility if not paired with
  conformance tests.

## Pre-Implementation Gate

Status: blocked until SOW-0037 closes the writer reference contract

Problem / root-cause model:

- Python writer should follow the finalized Rust/Go writer contract rather than
  chase a moving reference.

Evidence reviewed:

- Current Python writer implementation and product scope spec.

Affected contracts and surfaces:

- Python writer API, directory writer behavior, compression/FSS/compact output,
  field-name policy, binary fields, and benchmark claims.

Existing patterns to reuse:

- Rust writer behavior from SOW-0037.
- Go writer behavior from SOW-0037.
- Existing Python tests and shared conformance fixtures.

Risk and blast radius:

- Medium. Python users may rely on current public API behavior.

Sensitive data handling plan:

- Use only synthetic fixtures. Do not record real logs, SNMP communities,
  customer data, personal data, credentials, bearer tokens, private endpoints,
  or production incident details.

Implementation plan:

1. Wait for SOW-0037.
2. Compare Python writer against the finalized Rust writer matrix.
3. Implement mmap or measured equivalent behavior.
4. Align API/docs/tests with Rust/Go.
5. Run conformance, interoperability, and benchmark checks.

Validation plan:

- Python test suite.
- Shared writer conformance suite.
- Cross-language readback and stock `journalctl --verify --file`.
- Read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if Python-specific compatibility workflow
  becomes durable.
- Specs: update Python writer feature slice.
- End-user/operator docs: update Python API docs.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: pending until SOW-0037 closes.
- SOW-status.md: updated when activated and closed.

Open-source reference evidence:

- No external open-source source was checked for this planning SOW.

Open decisions:

- None now. Python mmap default may require a decision after measurement.

## Implications And Decisions

- 2026-05-28: user agreed Python writer parity follows Rust/Go writer closure.

## Plan

1. Activate after SOW-0037.
2. Implement Python writer parity.
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

- Record mmap limitations, reviewer findings, and benchmark failures in this
  SOW before changing scope.

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
