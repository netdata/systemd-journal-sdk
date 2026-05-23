# SOW-0005 - Go SDK And journalctl

## Status

Status: open

Sub-state: pending after Rust API and shared harness stabilize. May run before or after Node.js/Python SOWs, but only one implementation SOW may be active at a time.

## Requirements

### Purpose

Implement the Go SDK and file-backed journalctl rewrite with no CGO.

### Assistant Understanding

Facts:

- Go must implement the shared SDK contract without CGO or system journal library linkage.
- This phase is blocked until Rust and the shared harness stabilize.

Inferences:

- Go-specific file I/O, locking, and dependency risks must be enriched before activation.

Unknowns:

- Final Go API mapping is blocked on prerequisite SOW outcomes.

### Acceptance Criteria

- Go reader and writer expose idiomatic APIs equivalent to the shared SDK contract, plus a libsystemd-compatible reader facade unless a SOW records concrete evidence for a scoped exception.
- Go uses no CGO and no system journal library linkage.
- Go passes the shared conformance suite.
- Go participates in the cross-language interoperability matrix.
- Go journalctl rewrite passes file-backed/query behavior tests.
- Go journalctl implements repeated same-field OR matching and the `+` disjunction separator for file-backed behavior.
- No changes are made outside this repository.

## Analysis

Sources checked:

- Product scope spec.
- Pending Rust and harness SOWs.

Current state:

- Blocked until SOW-0003 and SOW-0004 complete.

Risks:

- CGO or native dependency leakage would violate the project goal.
- Go binary parsing and file locking mistakes can break interoperability.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Go implementation must follow the SDK/API/test contracts established by earlier phases.

Evidence reviewed:

- Product scope spec.
- Pending Rust and harness SOWs.

Affected contracts and surfaces:

- Go public APIs.
- Go CLI.
- Shared harness adapter.
- Dependency policy.

Existing patterns to reuse:

- Shared SDK contract and Rust behavior.

Risk and blast radius:

- CGO or native dependency leakage would violate the project goal.
- Writer behavior must remain interoperable with Rust and future languages.
- Go-specific risks, such as pure-Go binary parsing, file mapping strategy, and concurrency/locking behavior, must be enriched before this SOW moves to current.

Sensitive data handling plan:

- No sensitive runtime data expected.

Implementation plan:

1. Implement Go reader.
2. Implement Go writer.
3. Implement Go journalctl CLI.
4. Wire shared tests.

Validation plan:

- Shared conformance suite passes Go.
- Go package tests pass.
- Dependency audit confirms no CGO.

Artifact impact plan:

- Specs: update if Go exposes language-specific contract differences.
- End-user/operator docs: create Go SDK docs.
- SOW lifecycle: blocked until prerequisites complete.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

- Blocked on prerequisite SOW outcomes.

## Implications And Decisions

1. Go API and no-CGO implementation strategy
   - Current state: blocked on SOW-0003 and SOW-0004.
   - Required before activation: record how the shared SDK contract maps to idiomatic Go APIs, file I/O, locking, memory mapping, and dependency constraints.
   - Implication: the Go implementation must pass the same suite without CGO or native journal linkage.
   - Risk: incorrect binary parsing or locking assumptions can corrupt interoperability tests even if local Go tests pass.

## Plan

1. Wait for the shared harness and Rust SDK contract to stabilize.
2. Enrich Go-specific risks and API mapping before activation.
3. Delegate Go SDK and journalctl implementation using the repository-boundary block.
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
