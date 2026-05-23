# SOW-0008 - Interoperability And Full Writer Features

## Status

Status: open

Sub-state: pending after all baseline language SDKs pass shared conformance.

## Requirements

### Purpose

Complete cross-language interoperability and close remaining writer feature gaps, including compression and Forward Secure Sealing where in scope.

### Assistant Understanding

Facts:

- This phase requires all baseline language SDKs to pass shared conformance first.
- It closes cross-language interoperability and remaining writer feature gaps.

Inferences:

- Compression and Forward Secure Sealing decisions should be based on the completed baseline feature matrix.

Unknowns:

- Exact FSS phase split is blocked until baseline implementations exist.

### Acceptance Criteria

- Every writer/reader pair in Rust, Go, Node.js, and Python passes the interoperability matrix.
- Writer feature gaps from earlier phases are either implemented or represented by concrete follow-up SOWs.
- Compression writing is tested across languages where implemented.
- Forward Secure Sealing support is implemented or explicitly split into a narrower follow-up with evidence.
- No changes are made outside this repository.

## Analysis

Sources checked:

- Product scope spec.
- Pending language SOWs.

Current state:

- Blocked until SOW-0004, SOW-0005, SOW-0006, and SOW-0007 complete.

Risks:

- FSS and compression are high-risk due to crypto, compression, and verification semantics.
- Cross-language subtle differences can corrupt files or hide reader bugs.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Full writer support and interoperability require all baseline language implementations first.

Evidence reviewed:

- Product scope spec.
- Pending language SOWs.

Affected contracts and surfaces:

- Writer file format features.
- Cross-language fixture matrix.
- Verification behavior.
- Documentation.

Existing patterns to reuse:

- Shared conformance harness.
- Language SDK contracts.

Risk and blast radius:

- FSS and compression are high-risk due to crypto, compression, and verification semantics.
- Cross-language subtle differences can corrupt files or hide reader bugs.

Sensitive data handling plan:

- No sensitive runtime data expected.

Implementation plan:

1. Build full writer/reader matrix.
2. Implement remaining compression writing features.
3. Implement or split FSS/sealing work.
4. Validate with systemd-compatible tooling where applicable.

Validation plan:

- Full matrix passes.
- systemd-compatible verification evidence is recorded where applicable.
- Dependency audit remains clean.

Artifact impact plan:

- Specs: update writer feature reality.
- End-user/operator docs: update feature support matrix.
- Runtime project skills: update if new compatibility workflow is durable.
- SOW lifecycle: blocked until prerequisites complete.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

- Exact FSS phase split may need renewed decision after baseline implementations exist.

## Implications And Decisions

1. Interoperability and full writer completion boundary
   - Current state: blocked on SOW-0004, SOW-0005, SOW-0006, and SOW-0007 being completed and passing the shared conformance suite.
   - Required before activation: record the completed baseline feature matrix and decide whether any remaining compression or Forward Secure Sealing work needs narrower follow-up SOWs.
   - Implication: this SOW closes cross-language file compatibility after all baseline SDKs exist.
   - Risk: starting before all language baselines pass can hide whether failures come from core format handling, individual SDK bugs, or interoperability assumptions.

## Plan

1. Wait for all baseline language SDK SOWs to complete.
2. Record the completed feature matrix and remaining writer gaps before activation.
3. Delegate interoperability and writer-feature work using the repository-boundary block.
4. Review matrix results, systemd-compatible evidence, dependency audit, docs, and SOW audit before closing.

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
