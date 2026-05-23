# SOW-0007 - Python SDK And journalctl

## Status

Status: open

Sub-state: pending after Rust API and shared harness stabilize. May run before or after Go/Node.js SOWs, but only one implementation SOW may be active at a time.

## Requirements

### Purpose

Implement the Python SDK and file-backed journalctl rewrite without native journal bindings.

### Assistant Understanding

Facts:

- Python must implement the shared SDK contract without native journal bindings or system journal library linkage.
- This phase is blocked until Rust and the shared harness stabilize.

Inferences:

- Python typing, memory mapping, GIL, and performance risks must be enriched before activation.

Unknowns:

- Final Python API mapping is blocked on prerequisite SOW outcomes.

### Acceptance Criteria

- Python reader and writer expose idiomatic APIs equivalent to the shared SDK contract, plus a libsystemd-compatible reader facade unless a SOW records concrete evidence for a scoped exception.
- Python uses no native journal bindings and no system journal library linkage.
- Python passes the shared conformance suite.
- Python participates in the cross-language interoperability matrix.
- Python journalctl rewrite passes file-backed/query behavior tests.
- Python journalctl implements repeated same-field OR matching and the `+` disjunction separator for file-backed behavior.
- No changes are made outside this repository.

## Analysis

Sources checked:

- Product scope spec.
- Pending Rust and harness SOWs.

Current state:

- Blocked until SOW-0003 and SOW-0004 complete.

Risks:

- Native journal binding leakage would violate the project goal.
- Pure-Python parsing and GIL behavior can become bottlenecks.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Python implementation must follow established shared SDK and test contracts.

Evidence reviewed:

- Product scope spec.
- Pending Rust and harness SOWs.

Affected contracts and surfaces:

- Python package API.
- Python CLI.
- Shared harness adapter.
- Dependency policy.

Existing patterns to reuse:

- Shared SDK contract and Rust behavior.

Risk and blast radius:

- Native dependency leakage would violate the project goal.
- Pure Python performance may need profiling before optimization claims.
- Python-specific risks, such as pure-Python binary parsing cost, GIL effects, typing strategy, and memory mapping options, must be enriched before this SOW moves to current.

Sensitive data handling plan:

- No sensitive runtime data expected.

Implementation plan:

1. Implement Python reader.
2. Implement Python writer.
3. Implement Python journalctl CLI.
4. Wire shared tests.

Validation plan:

- Shared conformance suite passes Python.
- Package tests pass.
- Dependency audit confirms no native journal bindings.

Artifact impact plan:

- Specs: update if Python exposes language-specific contract differences.
- End-user/operator docs: create Python SDK docs.
- SOW lifecycle: blocked until prerequisites complete.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

- Blocked on prerequisite SOW outcomes.

## Implications And Decisions

1. Python API and pure-Python implementation strategy
   - Current state: blocked on SOW-0003 and SOW-0004.
   - Required before activation: record how the shared SDK contract maps to Python APIs, typing, file I/O, memory mapping, concurrency expectations, and dependency constraints.
   - Implication: Python must remain free of native journal bindings while still passing shared tests.
   - Risk: pure-Python performance and GIL behavior can become bottlenecks unless benchmark work verifies real hot paths later.

## Plan

1. Wait for the shared harness and Rust SDK contract to stabilize.
2. Enrich Python-specific risks and API mapping before activation.
3. Delegate Python SDK and journalctl implementation using the repository-boundary block.
4. Review conformance, dependency audit, interoperability participation, docs, and audit output before closing.

## Delegation Plan

- Implementer: `llm-netdata-cloud/minimax-m2.7-coder`, or fallback to `llm-netdata-cloud/qwen3.6-plus` then `llm-netdata-cloud/glm-5.1` if minimax fails or is unavailable.
- Reviewers: at least two from `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/glm-5.1`.
- Every prompt must include the canonical repository-boundary block from `AGENTS.md`.
- Failure handling: record implementer or reviewer model failure in this SOW, substitute only from the approved model list, rerun full-scope review after fixes, and do not close if `.agents/sow/audit.sh` fails.

## Execution Log

Pending activation.

## Validation

Pending activation and implementation.

## Outcome

Pending.

## Lessons Extracted

Pending activation.

## Followup

Pending activation.

## Regression Log

None yet.
