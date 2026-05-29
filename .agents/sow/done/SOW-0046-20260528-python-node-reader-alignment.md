# SOW-0046 - Python Node Reader Alignment

## Status

Status: closed

Sub-state: superseded on 2026-05-29 by language-specific SOWs after the user
changed the priority order to Rust reader optimization, then Python
reader/writer port, then Node.js reader/writer port.

## Requirements

### Purpose

Align Python and Node.js readers with the Rust/Go reader contracts, including
byte-preserving RAW field-name access and measured runtime-specific performance
limits.

### User Request

After Rust and Go reader work, the user wants Python moved to mmap where
practical and both Python and Node.js aligned with Rust API and internal
operations.

### Assistant Understanding

Facts:

- Python reader currently uses regular file reads.
- Node.js reader currently uses Buffer/file reads and string-keyed convenience
  maps.
- Both languages need byte-preserving RAW reader representation.
- Both languages need single-file and ordered directory reader benchmarks.

Inferences:

- Python and Node.js should follow the Rust/Go reader contract but may expose
  runtime-specific implementation limitations.

Unknowns:

- Whether Python mmap reader access should become default.
- Whether Node.js has a maintainable non-native mmap path suitable for this
  SDK.

### Acceptance Criteria

- Python and Node.js reader APIs match the shared reader contract from SOW-0043.
- Both expose byte-preserving RAW field-name access while preserving documented
  convenience map behavior.
- Python mmap reader behavior is measured and either implemented or explicitly
  rejected with evidence.
- Node.js mmap options are investigated and either implemented or explicitly
  rejected with evidence.
- Both pass shared reader conformance, mixed-directory, and cross-language
  fixture tests.
- Both benchmark single-file and ordered directory reading.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `python/journal/reader.py`
- `node/src/lib/reader.js`
- `.agents/sow/specs/product-scope.md`

Current state:

- Python and Node.js readers preserve raw payloads but need explicit
  byte-preserving field-name API design.

Risks:

- Changing existing map key behavior could break callers.
- Runtime mmap packages may add maintenance or runtime-linking risks.

## Pre-Implementation Gate

Status: blocked until SOW-0045 closes

Problem / root-cause model:

- Python and Node.js should align to the settled Rust/Go reader reference, not
  pre-optimize around temporary behavior.

Evidence reviewed:

- Current Python and Node.js reader implementation files.
- Product scope reader sections.

Affected contracts and surfaces:

- Python and Node.js reader APIs, directory readers, query/export helpers,
  journalctl rewrites, and docs.

Existing patterns to reuse:

- Rust/Go reader contract from SOW-0043 through SOW-0045.
- Existing raw payload arrays.
- Shared reader fixtures and tests.

Risk and blast radius:

- Medium for language users; lower for immediate Netdata hot paths unless those
  languages become integration targets.

Sensitive data handling plan:

- Use generated or public fixtures only. Do not record real customer logs,
  SNMP communities, credentials, bearer tokens, personal data, private
  endpoints, or production incident details.

Implementation plan:

1. Compare Python and Node.js readers to the shared contract.
2. Add byte-preserving RAW field-name APIs.
3. Investigate mmap/runtime options.
4. Optimize where practical.
5. Validate conformance and performance.

Validation plan:

- Python and Node.js tests.
- Shared reader conformance and mixed-directory tests.
- Benchmark/profiler artifacts.
- Read-only reviewer passes.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if runtime dependency workflow changes.
- Specs: update Python and Node.js reader feature slices.
- End-user/operator docs: update Python and Node.js reader docs.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: activate after Go reader optimization.
- SOW-status.md: update when activated and closed.

Open-source reference evidence:

- systemd/libsystemd evidence comes through SOW-0043 unless new evidence is
  required.

Open decisions:

- Python mmap default and Node.js mmap/dependency choices require measurement.

## Implications And Decisions

- 2026-05-28: user agreed Python and Node.js reader alignment follows Rust and
  Go reader work.

## Plan

1. Wait for SOW-0045.
2. Align Python and Node.js reader APIs.
3. Investigate mmap/runtime options.
4. Optimize and validate.

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

- Record runtime limits, API compatibility issues, reviewer findings, and audit
  failures in this SOW.

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

Closed without implementation. Superseded by:

- `SOW-0053-20260529-python-reader-writer-rust-port.md`
- `SOW-0054-20260529-node-reader-writer-rust-port.md`

## Lessons Extracted

Mixed-language reader SOWs hide priority and API sequencing. For the current
project state, language-specific reader/writer port SOWs are clearer.

## Followup

- SOW-0053 owns Python reader and writer Rust-port work.
- SOW-0054 owns Node.js reader and writer Rust-port work.

## Regression Log

None yet.
