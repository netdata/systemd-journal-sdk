# SOW-0004 - Rust SDK And journalctl

## Status

Status: open

Sub-state: pending after shared harness exists; deprioritized until the Go writer-first SOW is completed.

## Requirements

### Purpose

Implement the Rust SDK and file-backed journalctl rewrite against the shared tests after the Go writer-first SOW completes.

### User Request

Rust must provide the same reader/writer APIs and journalctl rewrite behavior as the other languages.

### Assistant Understanding

Facts:

- Rust is blocked on source import, shared harness completion, and the user-directed Go writer-first priority.
- Rust remains required, but it is no longer the first complete SDK implementation target.
- The Go writer must be completed first for the Netdata plugin use case.

Inferences:

- Rust API decisions should not be finalized until SOW-0002, SOW-0003, and the Go writer-first SOW produce concrete contracts.

Unknowns:

- Final Rust public API shape is blocked on prerequisite SOW outcomes and the Go writer-first priority.

### Acceptance Criteria

- Rust exposes idiomatic SDK APIs and a libsystemd-compatible reader facade, unless a SOW records concrete evidence for a scoped exception.
- Rust writer and reader pass the shared conformance suite.
- Rust journalctl rewrite passes file-backed/query behavior tests.
- Rust journalctl implements repeated same-field OR matching and the `+` disjunction separator for file-backed behavior.
- Daemon-only journalctl commands are not implemented and return documented unsupported behavior.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/done/SOW-0002-20260523-repo-scaffold-and-rust-source-import.md`
- `.agents/sow/done/SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md`

Current state:

- Blocked until SOW-0002, SOW-0003, and the Go writer-first SOW complete.

Risks:

- Rust API decisions still shape later language ports, but the Go writer now shapes the first production-oriented writer contract.
- CLI behavior drift can multiply across implementations.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Rust implementation depends on SOW-0002 source import, SOW-0003 shared tests, and the Go writer-first priority.

Evidence reviewed:

- `.agents/sow/done/SOW-0002-20260523-repo-scaffold-and-rust-source-import.md`
- `.agents/sow/done/SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md`
- `.agents/sow/pending/SOW-0005-20260523-go-sdk-and-journalctl.md`

Affected contracts and surfaces:

- Rust public APIs.
- Rust CLI.
- Shared harness adapters.
- Documentation.

Existing patterns to reuse:

- Imported Netdata Rust reader and writer code.
- Shared conformance runner.

Risk and blast radius:

- Rust API decisions shape later language ports, but must not block the Go writer deliverable.
- CLI behavior drift would multiply into Go, Node.js, and Python.

Sensitive data handling plan:

- No sensitive runtime data expected.
- Test fixtures are public or generated.

Implementation plan:

1. Finalize Rust API shape.
2. Wire reader facade and writer.
3. Implement file-backed journalctl CLI.
4. Run shared tests and Rust-specific unit tests.

Validation plan:

- Shared conformance suite passes Rust.
- Rust package tests pass.
- journalctl CLI tests pass.

Artifact impact plan:

- Specs: update API and CLI behavior.
- Runtime project skills: update if Rust workflow becomes durable.
- End-user/operator docs: create Rust SDK docs.
- SOW lifecycle: blocked until prerequisites complete.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

- Blocked on prerequisite SOW outcomes.

## Implications And Decisions

1. Rust API and CLI contract
   - Current state: blocked on SOW-0002, SOW-0003, and the Go writer-first SOW.
   - Required before activation: record the imported Rust layout, shared harness contract, Go writer lessons, and exact Rust public API shape.
   - Implication: Rust remains a required implementation, but it is no longer the first production-oriented writer contract.
   - Risk: premature API choices can force incompatible or unnatural APIs in Go, Node.js, and Python.

## Plan

1. Wait for SOW-0002, SOW-0003, and the Go writer-first SOW to complete.
2. Enrich this SOW with the imported Rust layout and shared harness contract before activation.
3. Delegate Rust SDK and journalctl implementation using the repository-boundary block.
4. Review shared test results, Rust tests, audit output, and docs before closing.

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
