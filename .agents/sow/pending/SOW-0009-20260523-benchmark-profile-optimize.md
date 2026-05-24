# SOW-0009 - Benchmark Profile Optimize

## Status

Status: open

Sub-state: pending after functional correctness, deterministic ingesters, and byte-level writer compatibility evidence are complete.

## Requirements

### Purpose

Benchmark, profile, and optimize every SDK implementation after correctness is established, using the deterministic ingestion dataset and systemd reference writer as the baseline.

### User Request

The user requested performance validation because the journal format is used in Netdata ingestion paths including netflow, OTEL logs, and SNMP traps. The benchmark should use a large dataset of about 200k rows, compare every language writer against the systemd reference writer, profile poor performers, and produce optimization plans or fixes.

### Assistant Understanding

Facts:

- Benchmarking and optimization must happen after correctness, interoperability, and deterministic writer-equivalence evidence are proven.
- Optimizations must be measurement-driven and must not weaken conformance.
- The user knows the Rust implementation can commit around 30k rows per second on one core for about 32 mixed-cardinality fields, and this is useful context but not a formal pass/fail threshold until measured on the project benchmark environment.

Inferences:

- SOW-0014 should provide the accepted performance corpus.
- SOW-0015 should provide the systemd and SDK ingesters used by benchmarks.
- systemd should be the reference baseline, with Rust also tracked as a known high-performance implementation.

Unknowns:

- Exact target machine, CPU governor, filesystem, sync policy, compression modes, and performance reporting format are not selected yet.

### Acceptance Criteria

- Benchmarks cover deterministic ingestion writing for systemd, Rust, Go, Node.js, and Python using the SOW-0014 performance corpus of about 200k accepted rows.
- Benchmarks cover reading, live one-writer/multiple-reader operation, filtering, journalctl queries, corruption handling where relevant, and cross-language file sizes.
- Benchmarks include mixed-cardinality profiles centered around about 32 fields per row, plus cardinality sweeps that isolate low-cardinality, high-cardinality, mostly-unique, binary-heavy, and large-value workloads.
- Benchmark results include rows per second, bytes per second, output file size, CPU time, wall time, memory allocation/heap behavior where available, fsync/sync policy, and compression mode.
- systemd reference ingestion is measured as the baseline.
- Profiling identifies bottlenecks before optimization work.
- Optimizations are driven by measurements and do not weaken conformance.
- Performance results are documented with reproducible commands.
- No changes are made outside this repository.

## Analysis

Sources checked:

- Product scope spec.
- Pending implementation and interoperability SOWs.

Current state:

- Blocked until SOW-0008, SOW-0014, SOW-0015, and SOW-0016 provide correctness, deterministic ingestion, and byte-level compatibility evidence.

Risks:

- Premature optimization can introduce compatibility regressions.
- Unrepresentative fixtures can create misleading performance claims.
- Comparing writers without controlling sync policy, compression, CPU governor, and filesystem can produce invalid conclusions.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Optimization before correctness would risk hard-to-diagnose compatibility bugs. Benchmarks also need deterministic datasets and ingesters so performance results measure writer implementations instead of generator differences.

Evidence reviewed:

- Product scope spec.
- Pending implementation and interoperability SOWs.
- User performance requirement from 2026-05-24.

Affected contracts and surfaces:

- Deterministic ingestion benchmark harness.
- SDK hot paths.
- CLI query performance.
- Documentation.

Existing patterns to reuse:

- Shared conformance fixtures and interoperability matrix.
- SOW-0014 deterministic performance corpus after it exists.
- SOW-0015 deterministic ingesters after they exist.

Risk and blast radius:

- Optimizations can introduce file-format or concurrency regressions.
- Benchmarks can mislead if fixtures are not representative.
- File sync policy and compression settings can dominate results and must be recorded explicitly.

Sensitive data handling plan:

- Benchmark data must be public fixtures, generated data, or sanitized.

Implementation plan:

1. Wait for the deterministic dataset, ingesters, and byte-compatibility gates.
2. Define benchmark environment controls, commands, sync policy, compression modes, and reporting format.
3. Run baseline measurements for systemd, Rust, Go, Node.js, and Python.
4. Profile bottlenecks in implementations that lag the baseline or show pathological allocation/CPU behavior.
5. Optimize measured hot paths without changing journal semantics.
6. Re-run conformance, interoperability, live concurrency, and byte-compatibility tests after each optimization.

Validation plan:

- Benchmark commands, environment, raw results, and summarized results recorded.
- Conformance suite remains passing.
- Interoperability matrix remains passing.
- Live stock-reader and cross-language concurrency matrix remains passing after optimization.
- Byte-compatibility matrix remains passing for slices that claim byte identity.
- Reviewers confirm no correctness tradeoff.

Artifact impact plan:

- Specs: update performance guarantees only if they become product promises.
- End-user/operator docs: publish benchmark methodology/results if this repository has user-facing benchmark docs at that point.
- Runtime project skills: update if benchmark workflow becomes durable.
- SOW lifecycle: blocked until correctness and deterministic-ingestion phases complete.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

- Target environment, sync policy, compression modes, filesystem, CPU governor, and reporting format must be selected before execution.

## Implications And Decisions

1. Benchmark and optimization boundary
   - Current state: blocked on SOW-0008, SOW-0014, SOW-0015, SOW-0016, and all correctness/interoperability evidence from SOW-0003 through SOW-0008.
   - Required before activation: select target environment, sync policy, compression modes, filesystem, CPU governor, commands, and reporting format.
   - Implication: optimization work must be driven by measured bottlenecks after correctness is proven.
   - Risk: premature optimization can invalidate conformance, and unrepresentative fixtures can create misleading performance claims.

## Plan

1. Wait for correctness, interoperability, deterministic dataset, deterministic ingester, and byte-compatibility SOWs to complete.
2. Select benchmark environment controls, commands, and reporting format before activation.
3. Run systemd, Rust, Go, Node.js, and Python baselines on the same SOW-0014 performance corpus.
4. Profile and optimize measured bottlenecks.
5. Review performance evidence plus full conformance/interoperability/byte-compatibility reruns before closing.

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
