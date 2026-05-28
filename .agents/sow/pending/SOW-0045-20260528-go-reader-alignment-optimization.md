# SOW-0045 - Go Reader Alignment Optimization

## Status

Status: open

Sub-state: created on 2026-05-28 as the Go reader alignment and performance SOW.

## Requirements

### Purpose

Align the Go reader with the optimized Rust reader reference and make it fit for
Netdata reader use cases where Go is relevant.

### User Request

After Rust reader parity and optimization, the user wants Go fixed and
optimized next. The user also noted that the Go writer uses mmap while the Go
reader does not, and that this likely affects performance.

### Assistant Understanding

Facts:

- Go reader currently uses `ReadAt()`-style reads rather than mmap-backed object
  access.
- Go reader must support ordered directory reading and shared reader API
  behavior.
- Go reader performance must be measured separately for single-file and
  directory readers.

Inferences:

- Go reader should be compared against optimized Rust, not the current
  pre-optimization Rust reader.

Unknowns:

- Whether Go mmap reader access should be default, optional, or avoided after
  measurement.

### Acceptance Criteria

- Go reader behavior matches the Rust reader contract from SOW-0043.
- Go reader performance is measured for single-file and ordered directory
  reading.
- Go reader mmap versus `ReadAt()` behavior is measured and decided with
  evidence.
- Go reader passes shared reader conformance and mixed-directory tests.
- Go reader performance gaps versus Rust are profiled and either fixed or
  explicitly recorded.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `go/journal/reader.go`
- `.agents/sow/specs/product-scope.md`

Current state:

- Go reader is functionally capable but likely not optimized for hot-path
  reader use.

Risks:

- Mmap can improve read hot paths but adds resize/SIGBUS/platform complexity.
- Optimizing Go before Rust reader closure could copy the wrong behavior.

## Pre-Implementation Gate

Status: blocked until SOW-0044 closes

Problem / root-cause model:

- Go reader should follow the optimized Rust reader reference after Rust
  parity/performance are known.

Evidence reviewed:

- Current Go reader implementation and product scope spec.

Affected contracts and surfaces:

- Go reader API, directory reader, libsystemd-compatible facade, journalctl
  rewrite, and Netdata integration readiness.

Existing patterns to reuse:

- Optimized Rust reader behavior from SOW-0044.
- Go writer mmap support where applicable.
- Shared reader fixtures.

Risk and blast radius:

- High for Go reader consumers once integrated.

Sensitive data handling plan:

- Use generated or public fixtures only. Do not record real customer logs,
  SNMP communities, credentials, bearer tokens, personal data, private
  endpoints, or production incident details.

Implementation plan:

1. Compare Go reader against Rust reader contract.
2. Add missing API/behavior parity.
3. Benchmark `ReadAt()` and mmap strategies.
4. Optimize based on profiles.
5. Validate conformance and performance.

Validation plan:

- Go tests.
- Shared reader conformance and mixed-directory tests.
- Benchmark/profiler artifacts.
- Read-only reviewer passes.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if Go reader workflow changes.
- Specs: update Go reader feature/performance status.
- End-user/operator docs: update Go reader docs.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: activate after Rust reader optimization.
- SOW-status.md: update when activated and closed.

Open-source reference evidence:

- systemd/libsystemd evidence comes through SOW-0043 unless new evidence is
  required.

Open decisions:

- Go mmap default/option decision requires measurement.

## Implications And Decisions

- 2026-05-28: user agreed Go reader follows Rust reader parity and
  optimization.

## Plan

1. Wait for SOW-0044.
2. Align Go reader with Rust.
3. Measure mmap/read strategies.
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

- Record mmap findings, performance gaps, reviewer findings, and audit failures
  in this SOW.

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
