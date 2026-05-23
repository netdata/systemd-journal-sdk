# SOW-0006 - Node.js SDK And journalctl

## Status

Status: open

Sub-state: pending after Rust API and shared harness stabilize. May run before or after Go/Python SOWs, but only one implementation SOW may be active at a time.

## Requirements

### Purpose

Implement the Node.js SDK and file-backed journalctl rewrite without native addons.

### Assistant Understanding

Facts:

- Node.js must implement the shared SDK contract without native addons or system journal library linkage.
- This phase is blocked until Rust and the shared harness stabilize.

Inferences:

- Node.js Buffer, streaming, and event-loop risks must be enriched before activation.

Unknowns:

- Final Node.js API mapping is blocked on prerequisite SOW outcomes.

### Acceptance Criteria

- Node.js reader and writer expose idiomatic APIs equivalent to the shared SDK contract, plus a libsystemd-compatible reader facade unless a SOW records concrete evidence for a scoped exception.
- Node.js uses no native addons and no system journal library linkage.
- Node.js passes the shared conformance suite.
- Node.js participates in the cross-language interoperability matrix.
- Node.js journalctl rewrite passes file-backed/query behavior tests.
- Node.js journalctl implements repeated same-field OR matching and the `+` disjunction separator for file-backed behavior.
- No changes are made outside this repository.

## Analysis

Sources checked:

- Product scope spec.
- Pending Rust and harness SOWs.

Current state:

- Blocked until SOW-0003 and SOW-0004 complete.

Risks:

- Native dependency leakage would violate the project goal.
- Event-loop blocking and Buffer handling can create correctness or performance issues.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Node.js implementation must follow established shared SDK and test contracts.

Evidence reviewed:

- Product scope spec.
- Pending Rust and harness SOWs.

Affected contracts and surfaces:

- Node.js package API.
- Node.js CLI.
- Shared harness adapter.
- Dependency policy.

Existing patterns to reuse:

- Shared SDK contract and Rust behavior.

Risk and blast radius:

- Native dependency leakage would violate the project goal.
- Pure JavaScript performance may need profiling before optimization claims.
- Node.js-specific risks, such as Buffer/Uint8Array API shape, streaming/backpressure, and event-loop blocking, must be enriched before this SOW moves to current.

Sensitive data handling plan:

- No sensitive runtime data expected.

Implementation plan:

1. Implement Node.js reader.
2. Implement Node.js writer.
3. Implement Node.js journalctl CLI.
4. Wire shared tests.

Validation plan:

- Shared conformance suite passes Node.js.
- Package tests pass.
- Dependency audit confirms no native addons.

Artifact impact plan:

- Specs: update if Node.js exposes language-specific contract differences.
- End-user/operator docs: create Node.js SDK docs.
- SOW lifecycle: blocked until prerequisites complete.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

- Blocked on prerequisite SOW outcomes.

## Implications And Decisions

1. Node.js API and pure-JavaScript implementation strategy
   - Current state: blocked on SOW-0003 and SOW-0004.
   - Required before activation: record how the shared SDK contract maps to Node.js APIs, Buffer/Uint8Array use, streaming/backpressure, CLI packaging, and dependency constraints.
   - Implication: Node.js must remain pure JavaScript/TypeScript with no native addons while still passing shared tests.
   - Risk: event-loop blocking or native dependency leakage can violate project goals even when correctness tests pass.

## Plan

1. Wait for the shared harness and Rust SDK contract to stabilize.
2. Enrich Node.js-specific risks and API mapping before activation.
3. Delegate Node.js SDK and journalctl implementation using the repository-boundary block.
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
