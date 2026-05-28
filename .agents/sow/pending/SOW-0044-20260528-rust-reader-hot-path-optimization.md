# SOW-0044 - Rust Reader Hot-Path Optimization

## Status

Status: open

Sub-state: created on 2026-05-28 as the Rust reader performance SOW.

## Requirements

### Purpose

Optimize the Rust reader after SOW-0043 establishes the reader compatibility
target, then use Rust as the performance reference for Go, Python, and Node.js.

### User Request

The user wants the reader phase to start with Rust:

- align Rust to libsystemd first;
- eliminate system calls and allocations in hot paths;
- measure single-file and ordered directory reader performance;
- compare against systemd C and Netdata's current reader behavior.

### Assistant Understanding

Facts:

- Reader performance is a production gate for Netdata integrations.
- Directory readers must support ordered reading across multiple files.
- Single-file and directory reader performance must be measured separately.

Inferences:

- Rust reader optimization should not start until SOW-0043 closes the parity
  target.

Unknowns:

- Actual Rust reader bottlenecks after parity work.

### Acceptance Criteria

- Rust single-file reader benchmarks exist and are reproducible.
- Rust ordered directory reader benchmarks exist and are reproducible.
- Benchmarks compare against systemd C/libsystemd or `journalctl` where
  applicable, and against Netdata's current reader behavior where practical.
- Profiles identify hot-path allocations, syscalls, decompression cost,
  filtering cost, cursor/seek cost, and directory merge cost.
- Optimizations preserve conformance and interoperability.
- Final Rust reader results and remaining risks are documented.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/specs/product-scope.md`
- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`

Current state:

- Rust reader performance has not been systematically optimized after reader
  parity.

Risks:

- Optimizing before SOW-0043 could target incomplete behavior.
- Directory merge performance can hide single-file performance regressions if
  they are measured together.

## Pre-Implementation Gate

Status: blocked until SOW-0043 closes

Problem / root-cause model:

- Rust reader needs a stable compatibility target before hot-path changes.

Evidence reviewed:

- SOW-0009 umbrella performance requirements.
- Product scope reader sections.

Affected contracts and surfaces:

- Rust reader API, libsystemd facade, directory reader, journalctl rewrite,
  Netdata reader integration readiness.

Existing patterns to reuse:

- Existing Rust reader implementation.
- Shared fixtures and mixed-directory tests.
- Benchmark result convention under `.local/benchmarks/`.

Risk and blast radius:

- High for reader correctness and Netdata integration readiness.

Sensitive data handling plan:

- Use generated or public fixtures only. Do not record real customer logs,
  SNMP communities, credentials, bearer tokens, personal data, private
  endpoints, or production incident details.

Implementation plan:

1. Build Rust reader baseline after SOW-0043.
2. Profile single-file and directory readers separately.
3. Optimize in batches.
4. Re-run conformance and benchmarks after each accepted batch.
5. Document results and residual risks.

Validation plan:

- Rust tests.
- Shared reader conformance and mixed-directory tests.
- Benchmark/profiler artifacts.
- Read-only reviewer passes.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update if reader benchmark workflow becomes durable.
- Specs: update Rust reader performance status if public.
- End-user/operator docs: update benchmark docs if public.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: close before Go reader performance SOW.
- SOW-status.md: update when activated and closed.

Open-source reference evidence:

- systemd/libsystemd evidence must be collected during implementation.

Open decisions:

- Final performance thresholds require first valid baseline evidence.

## Implications And Decisions

- 2026-05-28: user agreed reader optimization starts with Rust after parity.

## Plan

1. Wait for SOW-0043.
2. Benchmark and profile Rust reader.
3. Optimize and validate.

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

- Record invalid benchmarks, profiler findings, reviewer findings, and residual
  gaps in this SOW.

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
