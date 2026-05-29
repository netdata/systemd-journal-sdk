# SOW-0054 - Node.js Reader And Writer Rust Port

## Status

Status: open

Sub-state: pending until SOW-0052 and SOW-0053 close. This SOW supersedes the
Node.js part of the earlier combined Python/Node reader and writer follow-ups.

## Requirements

### Purpose

Bring Node.js reader and writer behavior as close as practical to the finalized
Rust reference implementation after Python is complete, while preserving the
project's non-native-runtime policy and documenting any Node.js runtime limits.

### User Request

After Rust reader optimization and Python reader/writer porting, the user wants
the Rust reader and writer ported to Node.js.

### Assistant Understanding

Facts:

- Node.js writer correctness is already certified for the accepted writer
  baseline, but performance remains limited.
- Node.js reader needs alignment to the finalized Rust reader contract.
- Node.js currently has no accepted native mmap dependency path in the SDK
  runtime.
- The user wants Node.js after Python.

Inferences:

- Node.js should be treated as a full-language port after Python, so Python
  lessons can be reused before tackling Node runtime constraints.
- Node.js mmap alternatives should be investigated, but a native addon should
  not be introduced without an explicit user policy decision.

Unknowns:

- Whether a maintainable, non-native Node.js mmap path exists.
- Whether Node.js writer performance can reach systemd/Rust/Go class without a
  native addon.

### Acceptance Criteria

- Node.js reader API and behavior align to the finalized Rust reader contract.
- Node.js writer API and behavior align to the finalized Rust writer contract.
- Node.js supports byte-preserving RAW field access and the same writer field
  policy layers as Rust.
- Node.js mmap/runtime options are investigated and either implemented or
  explicitly rejected with evidence.
- Node.js writer hot paths are profiled and optimized where practical.
- Node.js passes shared reader/writer conformance, mixed-directory, and
  relevant interoperability tests.
- Node.js single-file and ordered directory reader benchmarks are recorded.
- Node.js direct and directory writer benchmarks are recorded.
- Remaining Node.js runtime performance gaps are documented with evidence.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/pending/SOW-0046-20260528-python-node-reader-alignment.md`
- `.agents/sow/pending/SOW-0051-20260529-node-python-writer-performance.md`
- `.agents/sow/done/SOW-0041-20260528-node-writer-rust-parity.md`
- `.agents/sow/specs/product-scope.md`

Current state:

- Node.js writer correctness is in better shape than Node.js reader parity.
- Node.js writer performance was previously recorded as far below Rust/Go.
- Node.js reader/writer work is currently split across combined-language SOWs,
  which no longer matches the user's priority order.

Risks:

- Node.js runtime file I/O and Buffer handling may impose a lower performance
  ceiling than Rust/Go.
- Native addon dependencies could violate the current SDK policy unless the
  user explicitly changes it.

## Pre-Implementation Gate

Status: blocked until SOW-0052 and SOW-0053 close

Problem / root-cause model:

- Node.js should inherit both the final Rust reference and any Python porting
  lessons. Starting early would risk rework and duplicated design mistakes.

Evidence reviewed:

- Prior Node.js writer parity SOW.
- Combined Python/Node reader and writer follow-up SOWs.
- Product scope specs.

Affected contracts and surfaces:

- Node.js public reader and writer APIs.
- Node.js directory reader/writer behavior.
- Node.js journalctl rewrite behavior where reader changes apply.
- Node.js benchmark and documentation surfaces.

Existing patterns to reuse:

- Rust reader/writer reference after SOW-0052.
- Python porting lessons from SOW-0053.
- Existing Node.js writer correctness implementation from SOW-0041.
- Shared conformance and interoperability harnesses.

Risk and blast radius:

- Medium. Node.js API changes can affect SDK users, but runtime dependency
  choices may have larger maintainability and portability implications.

Sensitive data handling plan:

- Use generated fixtures and sanitized benchmark data only.
- Do not record real logs, SNMP communities, credentials, bearer tokens,
  customer data, personal data, private endpoints, or production incident
  details.

Implementation plan:

1. Wait for SOW-0052 and SOW-0053.
2. Compare Node.js reader and writer APIs to Rust and Python.
3. Implement Node.js reader parity and runtime read-path improvements.
4. Profile and optimize Node.js writer hot paths.
5. Validate correctness, interoperability, and performance.

Validation plan:

- Node.js package tests.
- Shared reader and writer conformance.
- Directory, mixed-directory, compression/compact/live matrices where touched.
- Node.js reader and writer benchmarks.
- Read-only reviewer passes.
- `.agents/sow/audit.sh`

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if a durable Node.js runtime policy
  changes.
- Specs: update Node.js feature/performance status.
- End-user/operator docs: update Node.js README/API docs.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: activate after SOW-0053.
- SOW-status.md: update when activated and closed.

Open-source reference evidence:

- Existing systemd reference evidence comes through Rust SOWs unless new
  compatibility-sensitive behavior is changed here.

Open decisions:

- None blocking creation. Runtime limitations or native dependency choices may
  require a later user decision.

## Implications And Decisions

- 2026-05-29: user prioritized Node.js after Rust and Python.

## Plan

1. Wait for Rust reader and Python full-language port closure.
2. Port final Rust reader behavior to Node.js.
3. Align Node.js writer behavior and optimize measured bottlenecks.
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

- If Node.js cannot meet the performance target without native code, record
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

