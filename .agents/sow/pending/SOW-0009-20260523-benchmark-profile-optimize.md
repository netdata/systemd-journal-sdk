# SOW-0009 - Benchmark Profile Optimize

## Status

Status: open

Sub-state: pending after functional correctness and interoperability are proven.

## Requirements

### Purpose

Benchmark, profile, and optimize every SDK implementation after correctness is established.

### Assistant Understanding

Facts:

- Benchmarking and optimization must happen after correctness and interoperability are proven.
- Optimizations must be measurement-driven and must not weaken conformance.

Inferences:

- Representative datasets and target environments must be selected before implementation.

Unknowns:

- Benchmark fixture sizes and target environments are not selected yet.

### Acceptance Criteria

- Benchmarks cover reading, writing, filtering, journalctl queries, corruption handling where relevant, and cross-language file sizes.
- Profiling identifies bottlenecks before optimization work.
- Optimizations are driven by measurements and do not weaken conformance.
- Performance results are documented with reproducible commands.
- No changes are made outside this repository.

## Analysis

Sources checked:

- Product scope spec.
- Pending implementation and interoperability SOWs.

Current state:

- Blocked until SOW-0008 and correctness/interoperability evidence complete.

Risks:

- Premature optimization can introduce compatibility regressions.
- Unrepresentative fixtures can create misleading performance claims.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Optimization before correctness would risk hard-to-diagnose compatibility bugs.

Evidence reviewed:

- Product scope spec.
- Pending implementation and interoperability SOWs.

Affected contracts and surfaces:

- Benchmark harness.
- SDK hot paths.
- CLI query performance.
- Documentation.

Existing patterns to reuse:

- Shared conformance fixtures and interoperability matrix.

Risk and blast radius:

- Optimizations can introduce file-format or concurrency regressions.
- Benchmarks can mislead if fixtures are not representative.

Sensitive data handling plan:

- Benchmark data must be public fixtures, generated data, or sanitized.

Implementation plan:

1. Define benchmark datasets and commands.
2. Run baseline measurements.
3. Profile bottlenecks.
4. Optimize measured hot paths.
5. Re-run conformance and interoperability tests.

Validation plan:

- Benchmark commands and results recorded.
- Conformance suite remains passing.
- Interoperability matrix remains passing.
- Reviewers confirm no correctness tradeoff.

Artifact impact plan:

- Specs: update performance guarantees only if they become product promises.
- End-user/operator docs: publish benchmark methodology/results.
- Runtime project skills: update if benchmark workflow becomes durable.
- SOW lifecycle: blocked until correctness phases complete.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

- Representative benchmark fixture sizes and target environments must be selected before execution.

## Implications And Decisions

1. Benchmark and optimization boundary
   - Current state: blocked on SOW-0008 and all correctness/interoperability evidence from SOW-0003 through SOW-0008.
   - Required before activation: select representative benchmark datasets, target environments, commands, and reporting format.
   - Implication: optimization work must be driven by measured bottlenecks after correctness is proven.
   - Risk: premature optimization can invalidate conformance, and unrepresentative fixtures can create misleading performance claims.

## Plan

1. Wait for correctness and interoperability SOWs to complete.
2. Select benchmark datasets, target environments, commands, and reporting format before activation.
3. Delegate benchmark, profiling, and optimization work using the repository-boundary block.
4. Review performance evidence plus full conformance/interoperability reruns before closing.

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
