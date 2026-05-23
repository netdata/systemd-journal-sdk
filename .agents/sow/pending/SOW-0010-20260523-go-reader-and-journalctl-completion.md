# SOW-0010 - Go Reader And journalctl Completion

## Status

Status: open

Sub-state: pending after Go writer-first delivery.

## Requirements

### Purpose

Complete the remaining Go SDK reader facade and file-backed journalctl rewrite after the user-prioritized Go writer is delivered.

### User Request

Go writer must be delivered first for Netdata plugin use. The rest of the Go SDK and journalctl work remains required after the writer-first chunk.

### Assistant Understanding

Facts:

- Go writer-first work is tracked in SOW-0005.
- Go must still provide an idiomatic reader API, a libsystemd-compatible reader facade, and a file-backed journalctl rewrite.
- Go must remain pure Go: no CGO and no system journal library linkage.

Inferences:

- Go reader and journalctl should reuse the writer's package layout and test adapter once SOW-0005 is complete.

Unknowns:

- Final Go reader API mapping is blocked on the writer-first implementation and shared harness results.

### Acceptance Criteria

- Go exposes idiomatic reader APIs equivalent to the shared SDK contract.
- Go exposes a libsystemd-compatible reader facade unless this SOW records concrete evidence for a scoped exception.
- Go journalctl rewrite passes file-backed/query behavior tests.
- Go journalctl implements repeated same-field OR matching and the `+` disjunction separator for file-backed behavior.
- Daemon-only journalctl commands return documented unsupported behavior.
- Go remains no-CGO with no system journal library linkage.
- No changes are made outside this repository.

## Analysis

Sources checked:

- Product scope spec.
- SOW-0005 Go writer-first priority.

Current state:

- Blocked until SOW-0005 completes.

Risks:

- Reader facade API may be shaped by writer package layout.
- journalctl match behavior drift can diverge from the other language implementations.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Go reader and journalctl completion should not delay the user-prioritized writer deliverable.

Evidence reviewed:

- `.agents/sow/pending/SOW-0005-20260523-go-sdk-and-journalctl.md`
- `.agents/sow/specs/product-scope.md`

Affected contracts and surfaces:

- Go reader APIs.
- Go libsystemd-compatible reader facade.
- Go file-backed journalctl CLI.
- Shared harness adapter.
- Documentation.

Existing patterns to reuse:

- Go writer package layout from SOW-0005.
- Shared conformance harness from SOW-0003.
- journalctl matching semantics from the product scope spec.

Risk and blast radius:

- A reader facade that diverges from the shared contract can break cross-language compatibility.
- Incorrect journalctl boolean matching can make file-backed query behavior unreliable.

Sensitive data handling plan:

- No sensitive runtime data expected.
- Fixtures remain public upstream artifacts or repo-generated test files.

Implementation plan:

1. Wait for SOW-0005 to complete.
2. Enrich Go reader and journalctl API mapping from writer package layout and shared tests.
3. Implement Go reader APIs and libsystemd-compatible facade.
4. Implement Go file-backed journalctl rewrite.
5. Run shared reader, writer read-back, and journalctl tests.

Validation plan:

- Shared conformance suite passes Go for reader and file-backed journalctl behavior.
- Go package tests pass.
- Dependency audit confirms no CGO and no system journal library linkage.
- journalctl same-key OR and `+` disjunction tests pass.

Artifact impact plan:

- Specs: update if Go reader or CLI exposes language-specific contract differences.
- End-user/operator docs: add Go reader and journalctl docs.
- Runtime project skills: update only if a durable Go workflow emerges.
- SOW lifecycle: blocked until SOW-0005 completes.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

- Blocked on SOW-0005 outcome.

## Implications And Decisions

1. Go writer-first split
   - Current state: resolved by user decision on 2026-05-23.
   - Selection: Go writer is delivered first in SOW-0005; this SOW tracks the deferred Go reader and journalctl completion.
   - Implication: Go SDK is not complete until this SOW completes.
   - Risk: follow-up tracking must remain visible so writer-first delivery does not accidentally drop reader or CLI requirements.

## Plan

1. Wait for SOW-0005 to complete.
2. Enrich this SOW with concrete Go writer package layout and shared harness results.
3. Delegate Go reader and journalctl implementation using the repository-boundary block.
4. Review conformance, dependency audit, docs, and audit output before closing.

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
