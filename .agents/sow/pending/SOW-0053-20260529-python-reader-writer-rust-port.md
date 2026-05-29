# SOW-0053 - Python Reader And Writer Rust Port

## Status

Status: open

Sub-state: pending until SOW-0052 closes. This SOW supersedes the Python part
of the earlier combined Python/Node reader and writer follow-ups.

## Requirements

### Purpose

Bring Python reader and writer behavior as close as practical to the finalized
Rust reference implementation while preserving pure-Python maintainability,
systemd journal compatibility where applicable, and clear runtime limitations.

### User Request

After Rust reader optimization, the user wants the Rust reader and writer
ported to Python before Node.js.

### Assistant Understanding

Facts:

- Python writer correctness is already certified for the accepted writer
  baseline, but performance remains limited.
- Python reader still needs alignment to the finalized Rust reader contract,
  including mmap evaluation and byte-preserving field access.
- The user wants Python before Node.js.

Inferences:

- Python should be treated as one full-language port SOW instead of splitting
  reader and writer work across two mixed-language SOWs.
- Python should copy Rust API semantics where practical, but runtime-specific
  limits must be recorded instead of hidden.

Unknowns:

- Whether Python mmap should be default for reading after measurement.
- Whether Python writer performance can reach systemd/Rust/Go class without
  native extension code.

### Acceptance Criteria

- Python reader API and behavior align to the finalized Rust reader contract.
- Python writer API and behavior align to the finalized Rust writer contract.
- Python supports byte-preserving RAW field access and the same writer field
  policy layers as Rust.
- Python reader mmap is implemented or explicitly rejected with measured
  evidence.
- Python writer hot paths are profiled and optimized where practical.
- Python passes shared reader/writer conformance, mixed-directory, and relevant
  interoperability tests.
- Python single-file and ordered directory reader benchmarks are recorded.
- Python direct and directory writer benchmarks are recorded.
- Remaining Python runtime performance gaps are documented with evidence.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/pending/SOW-0046-20260528-python-node-reader-alignment.md`
- `.agents/sow/pending/SOW-0051-20260529-node-python-writer-performance.md`
- `.agents/sow/done/SOW-0040-20260528-python-writer-mmap-and-rust-parity.md`
- `.agents/sow/specs/product-scope.md`

Current state:

- Python writer correctness is in better shape than Python reader parity.
- Python writer performance was previously recorded as far below Rust/Go.
- Python reader and writer work is currently split across combined-language
  SOWs, which no longer matches the user's priority order.

Risks:

- Python runtime limitations may prevent Rust/Go-level throughput without
  native extension code.
- Combining reader and writer in one language SOW increases scope, but it also
  improves API consistency for Python consumers.

## Pre-Implementation Gate

Status: blocked until SOW-0052 closes

Problem / root-cause model:

- Python must follow the final Rust reference. Starting before SOW-0052 closes
  risks porting behavior that will immediately change.

Evidence reviewed:

- Prior Python writer parity SOW.
- Combined Python/Node reader and writer follow-up SOWs.
- Product scope specs.

Affected contracts and surfaces:

- Python public reader and writer APIs.
- Python directory reader/writer behavior.
- Python journalctl rewrite behavior where reader changes apply.
- Python benchmark and documentation surfaces.

Existing patterns to reuse:

- Rust reader/writer reference after SOW-0052.
- Existing Python writer mmap arena from SOW-0040.
- Shared conformance and interoperability harnesses.

Risk and blast radius:

- Medium. Python API changes can affect SDK users, but current Netdata hot-path
  integrations primarily depend on Rust/Go.

Sensitive data handling plan:

- Use generated fixtures and sanitized benchmark data only.
- Do not record real logs, SNMP communities, credentials, bearer tokens,
  customer data, personal data, private endpoints, or production incident
  details.

Implementation plan:

1. Wait for SOW-0052 to define the final Rust reader reference.
2. Compare Python reader and writer APIs to Rust.
3. Implement Python reader parity and mmap/read-path changes.
4. Profile and optimize Python writer hot paths.
5. Validate correctness, interoperability, and performance.

Validation plan:

- Python package tests.
- Shared reader and writer conformance.
- Directory, mixed-directory, compression/compact/live matrices where touched.
- Python reader and writer benchmarks.
- Read-only reviewer passes.
- `.agents/sow/audit.sh`

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if a durable Python runtime policy
  changes.
- Specs: update Python feature/performance status.
- End-user/operator docs: update Python README/API docs.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: activate after SOW-0052.
- SOW-status.md: update when activated and closed.

Open-source reference evidence:

- Existing systemd reference evidence comes through Rust SOWs unless new
  compatibility-sensitive behavior is changed here.

Open decisions:

- None blocking creation. Runtime limitations may require a later user decision
  if performance remains below target.

## Implications And Decisions

- 2026-05-29: user prioritized Python after Rust and before Node.js.

## Plan

1. Wait for Rust reader last-mile optimization closure.
2. Port final Rust reader behavior to Python.
3. Align Python writer behavior and optimize measured bottlenecks.
4. Validate, review, commit, and push.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current user routing.

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

- If Python cannot meet the performance target without native code, record
  profiler evidence and ask for a product decision before claiming
  production-grade throughput.

## Execution Log

### 2026-05-29

- Created from the user's updated Rust -> Python -> Node.js priority.

## Validation

Acceptance criteria evidence:

- Pending activation.

Tests or equivalent validation:

- Pending activation.

Real-use evidence:

- Pending activation.

Reviewer findings:

- Pending activation.

Same-failure scan:

- Pending activation.

Sensitive data gate:

- Planning text contains no raw sensitive data.

Artifact maintenance gate:

- Pending activation.

Specs update:

- Pending activation.

Project skills update:

- Pending activation.

End-user/operator docs update:

- Pending activation.

End-user/operator skills update:

- Pending activation.

Lessons:

- Pending activation.

Follow-up mapping:

- Pending activation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.

