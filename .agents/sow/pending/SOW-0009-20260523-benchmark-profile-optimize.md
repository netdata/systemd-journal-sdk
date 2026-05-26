# SOW-0009 - Benchmark Profile Optimize

## Status

Status: open

Sub-state: critical Netdata integration gate, sequenced after feature
completion. Finish SOW-0023 and the remaining feature SOWs that affect writer
or reader hot paths before optimizing, so performance work is not invalidated by
later feature/API changes.

## Requirements

### Purpose

Benchmark, profile, and optimize every SDK implementation after correctness and
the relevant Netdata-facing writer/reader API surfaces are established, using
the deterministic ingestion dataset, Netdata-shaped workloads, the systemd
reference writer, and Netdata's current Rust implementation as baselines. This
SOW is a production gate for replacing Netdata's vendored NetFlow/OTEL journal
logic and for the no-libsystemd `systemd-journal.plugin` reader path.

### User Request

The user requested performance validation because the journal format is used in
Netdata ingestion paths including NetFlow, OTEL logs, and SNMP traps. The
benchmark should use a large dataset of about 200k rows, compare every language
writer against the systemd reference writer, profile poor performers, and
produce optimization plans or fixes.

On 2026-05-26, the user reported that the SNMP traps ingestion worker sees the
current Go SDK writer at about 5k logs/s, while Netdata's NetFlow path with the
vendored Rust implementation reaches about 25k logs/s and prior measurements of
the Rust implementation reached about 30k logs/s on a suitable workload. The
user stated this makes the performance SOW critical for Netdata integration,
including replacing NetFlow and OTEL vendored copies, and also critical for the
`systemd-journal.plugin` reader path, which must be significantly faster than a
naive reader.

### Assistant Understanding

Facts:

- Benchmarking and optimization must happen after correctness, interoperability, and deterministic writer-equivalence evidence are proven.
- The user decided on 2026-05-24 to push this SOW to the end, after remaining feature-completeness SOWs, because benchmarking is expected to reveal discrepancies that require profiling, allocation reduction, buffer reuse, and refactoring.
- The user updated the priority on 2026-05-26 after SNMP traps integration work reported about 5k logs/s from the current Go SDK writer. This datapoint is user-reported and must be reproduced under controlled benchmark conditions before it is treated as a measured project result.
- The user clarified on 2026-05-26 that feature development must finish before optimization. Performance must not preempt SOW-0023 or other feature work, because optimizing before the feature surface is complete will let performance slip repeatedly as later features change the hot paths.
- Netdata replacement is not production-ready if the SDK writer or reader hot paths are materially slower than the current vendored Rust implementation without a measured explanation and a user-approved exception.
- Optimizations must be measurement-driven and must not weaken conformance.
- The user knows the Rust implementation can commit around 30k rows per second on one core for about 32 mixed-cardinality fields, and this is useful context but not a formal pass/fail threshold until measured on the project benchmark environment.
- Reader performance is also a production gate for the no-libsystemd `systemd-journal.plugin` path and for Netdata reader/query/rebuild consumers.

Inferences:

- SOW-0014 provides the accepted performance corpus.
- SOW-0015 provides the systemd and SDK ingesters used by benchmarks.
- systemd should be the reference baseline, with Rust also tracked as a known high-performance implementation.
- Netdata's vendored Rust writer and reader/index/query paths must be measured as practical replacement baselines, not only stock systemd.
- The benchmark plan needs both microbenchmarks and end-to-end Netdata-shaped paths. The Go writer result reported by the user may include SDK overhead, SNMP traps worker overhead, sync cadence, compact/compression choices, field remapping, locking, retention, or dataset/cardinality effects; these must be separated by profiling.

Unknowns:

- Exact target machine, CPU governor, filesystem, sync policy, compression modes, compact/regular mode, FSS mode, lock mode, and performance reporting format are not selected yet.
- Exact pass/fail thresholds are not selected yet. The default recommended gate is to require the Go writer to be close enough to the vendored Rust baseline for Netdata ingestion use, and to reject replacement if the gap remains in the 5k vs 25k logs/s class after profiling.

### Acceptance Criteria

- Benchmarks cover deterministic ingestion writing for systemd, Rust, Go, Node.js, and Python using the SOW-0014 performance corpus of about 200k accepted rows.
- Benchmarks cover Netdata-shaped writer workloads for SNMP traps, NetFlow, and OTEL logs, including the sync cadence, field counts, value cardinality, binary values, source realtime handling, remapping behavior, compact/regular output selection, compression, FSS, rotation, and retention policies expected by Netdata.
- Benchmarks reproduce or reject the user-reported Go SNMP traps writer result of about 5k logs/s with controlled measurements and a breakdown of SDK time versus caller/worker overhead.
- Benchmarks cover reading, live one-writer/multiple-reader operation, filtering, journalctl queries, query-unique/facet-style scans, cursor/seek behavior, directory traversal, corruption handling where relevant, and cross-language file sizes.
- Reader benchmarks include SDK idiomatic readers, libsystemd-compatible facades, file-backed journalctl rewrites, Netdata `jf`-style behavior after SOW-0027, stock `journalctl`, stock libsystemd where allowed, and Netdata's current reader/index/query paths where practical.
- Benchmarks include mixed-cardinality profiles centered around about 32 fields per row, plus cardinality sweeps that isolate low-cardinality, high-cardinality, mostly-unique, binary-heavy, and large-value workloads.
- Benchmark results include rows per second, bytes per second, output file size, CPU time, wall time, memory allocation/heap behavior where available, fsync/sync policy, and compression mode.
- systemd reference ingestion is measured as the format baseline, and Netdata's vendored Rust implementation is measured as the practical replacement baseline.
- Profiling identifies bottlenecks before optimization work.
- Optimizations are driven by measurements and do not weaken conformance. Profiling must specifically account for allocations, buffer reuse, hashing, object lookup, data/field deduplication, compression, sealing, remapping, lock handling, append publication, reader decompression, query filtering, and directory traversal.
- Performance results are documented with reproducible commands.
- The SOW cannot close with a material Go writer or reader performance deficit against the selected Netdata baseline unless the user explicitly accepts the residual gap after seeing profiling evidence and options.
- SOW-0026 Netdata integration cannot claim production replacement of NetFlow/OTEL vendored journal logic or no-libsystemd `systemd-journal.plugin` reader readiness until this SOW passes for those hot paths, or the user explicitly accepts a staged integration exception.
- No changes are made outside this repository.

## Analysis

Sources checked:

- Product scope spec.
- Pending implementation and interoperability SOWs.

Current state:

- Correctness, deterministic ingestion, and byte-level compatibility evidence are complete through SOW-0016.
- SOW-0017, SOW-0018, SOW-0019, and SOW-0021 are complete. SOW-0020 directory traversal parity remains pending.
- User decision on 2026-05-24 kept this SOW blocked until later feature work. The 2026-05-26 SNMP traps performance report changes this from final polish into a Netdata integration blocker, but the same-day sequencing clarification keeps feature completion first.
- SOW-0023 is stabilizing the high-level writer API used by Netdata writer integrations.
- SOW-0027 is planned to define reader-side SDK and `jf` facade behavior needed for Netdata reader integrations.

Risks:

- Premature optimization can introduce compatibility regressions.
- Unrepresentative fixtures can create misleading performance claims.
- Comparing writers without controlling sync policy, compression, CPU governor, and filesystem can produce invalid conclusions.
- Performance refactors made before xz/lz4, compact journal, FSS, and directory traversal parity work may be invalidated by those later feature changes.
- Waiting too long now risks blocking SNMP traps, NetFlow, OTEL, and no-libsystemd reader integration after API work is otherwise ready.
- Optimizing only synthetic deterministic fixtures can miss the actual SNMP traps, NetFlow, OTEL, and systemd-journal reader bottlenecks.

## Pre-Implementation Gate

Status: blocked until feature completion - critical integration gate

Problem / root-cause model:

- Optimization before the relevant API and file-format surfaces are complete risks hard-to-diagnose compatibility bugs, repeated performance regressions, and churn. The user-reported Go writer result of about 5k logs/s versus the current Netdata Rust path around 25k logs/s is too large to ignore, but it should be handled after feature development stabilizes, not before SOW-0023 and other hot-path feature SOWs finish.
- Working theory: the Go writer deficit may come from allocation-heavy field handling, repeated buffer construction, object/hash lookup cost, remapping overhead, compression/compact choices, lock/sync cadence, caller overhead, or a combination. This is speculation until profiles isolate the hot paths.
- Reader performance risk is separate: no-libsystemd `systemd-journal.plugin` and Netdata reader/query/rebuild paths need efficient sequential scan, filtering, field extraction, directory traversal, and compressed/compact/sealed handling.

Evidence reviewed:

- Product scope spec.
- Pending implementation and interoperability SOWs.
- User performance requirement from 2026-05-24.
- User sequencing decision from 2026-05-24: push SOW-0009 to the end instead of running a baseline-only benchmark now.
- User performance update from 2026-05-26: SNMP traps worker reports the Go SDK writer at about 5k logs/s, compared with Netdata NetFlow vendored Rust around 25k logs/s and prior Rust measurements around 30k logs/s.
- User sequencing clarification from 2026-05-26: finish SOW-0023 and the remaining feature work first; optimize only after feature development so performance work does not slip as features change.
- SOW-0026 Netdata integration scope includes replacing NetFlow/OTEL vendored copies and the no-libsystemd `systemd-journal.plugin` reader path, making performance a production gate.

Affected contracts and surfaces:

- Deterministic ingestion benchmark harness.
- SDK hot paths.
- CLI query performance.
- Netdata SNMP traps writer path.
- Netdata NetFlow writer, replay/query/rebuild/facet paths.
- Netdata OTEL writer path.
- Netdata no-libsystemd `systemd-journal.plugin` reader path.
- SDK libsystemd-compatible reader facades and `jf`-style behavior.
- Documentation.

Existing patterns to reuse:

- Shared conformance fixtures and interoperability matrix.
- SOW-0014 deterministic performance corpus.
- SOW-0015 deterministic ingesters.
- Netdata vendored Rust writer and reader/query paths as practical replacement baselines.
- Go `testing.B`, `pprof`, `benchstat`, CPU and heap profiles.
- Rust `criterion` or cargo bench plus profiler support where available.
- Node.js built-in profiler/heap snapshots and Python `cProfile`/allocation tooling for their SDK paths.

Risk and blast radius:

- Optimizations can introduce file-format or concurrency regressions.
- Benchmarks can mislead if fixtures are not representative of SNMP traps, NetFlow, OTEL, and systemd-journal plugin reader workloads.
- File sync policy and compression settings can dominate results and must be recorded explicitly.
- Performance fixes can change object ordering, timing, buffering, or publication windows and must be followed by full conformance/interoperability/live-reader validation.

Sensitive data handling plan:

- Benchmark data must be public fixtures, generated data, or sanitized.

Implementation plan:

1. Keep this SOW pending while SOW-0023 and remaining feature SOWs that affect writer/reader hot paths are completed.
2. Define benchmark environment controls, commands, sync policy, compression/compact/FSS modes, lock mode, dataset selection, and reporting format.
3. Add or extend benchmark harnesses for deterministic corpus, SNMP traps-shaped writes, NetFlow-shaped writes/reads, OTEL-shaped writes, and systemd-journal.plugin-shaped reads.
4. Run baseline measurements for systemd, SDK Rust, SDK Go, SDK Node.js, SDK Python, and Netdata vendored Rust where applicable.
5. Reproduce or reject the user-reported Go SNMP traps writer result and separate SDK cost from caller/worker cost.
6. Profile bottlenecks in implementations that lag the selected baseline or show pathological allocation/CPU behavior.
7. Optimize measured hot paths without changing journal semantics.
8. Re-run conformance, interoperability, live concurrency, byte-compatibility, and mixed-format reader tests after each optimization.

Validation plan:

- Benchmark commands, environment, raw results, and summarized results recorded.
- User-reported Go SNMP traps writer performance is either reproduced with controlled evidence or rejected with controlled evidence explaining the mismatch.
- Writer results compare systemd, Netdata vendored Rust, SDK Rust, SDK Go, SDK Node.js, and SDK Python where applicable.
- Reader results compare SDK readers/facades, stock tools, and Netdata current paths where applicable.
- Conformance suite remains passing.
- Interoperability matrix remains passing.
- Live stock-reader and cross-language concurrency matrix remains passing after optimization.
- Byte-compatibility matrix remains passing for slices that claim byte identity.
- Reviewers confirm no correctness tradeoff.
- If optimized performance remains materially below the selected Netdata baseline, the SOW records profiles, attempted fixes, remaining bottlenecks, and a user decision before any production replacement claim.

Artifact impact plan:

- Specs: update performance guarantees only if they become product promises.
- End-user/operator docs: publish benchmark methodology/results if this repository has user-facing benchmark docs at that point.
- Runtime project skills: update if benchmark workflow becomes durable.
- SOW lifecycle: critical pending SOW. It should be activated after feature completion and before SOW-0026 production Netdata replacement claims.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

- Target environment, sync policy, compression modes, compact/regular mode, FSS mode, lock mode, filesystem, CPU governor, and reporting format must be selected before execution.
- Exact production thresholds must be selected before close. Recommended default: replacement is not acceptable while Go writer or critical reader paths remain in the same class as the user-reported 5k logs/s result against a roughly 25k logs/s current Netdata Rust baseline.
- Decide whether the first activated benchmark slice focuses on Go SNMP traps writer performance, reader performance for `systemd-journal.plugin`, or both in one SOW phase.

## Implications And Decisions

1. Benchmark and optimization boundary
   - Current state: correctness, deterministic ingestion, and byte-level writer identity are complete through SOW-0016.
   - Required before activation: select target environment, sync policy, compression modes, filesystem, CPU governor, commands, and reporting format.
   - Implication: optimization work must be driven by measured bottlenecks after correctness is proven.
   - Risk: premature optimization can invalidate conformance, and unrepresentative fixtures can create misleading performance claims.

2. Benchmark sequencing after remaining feature work
   - Previous decision: push this SOW to the end, after SOW-0017, SOW-0018, SOW-0019, and SOW-0020.
   - Evidence: SOW-0017 adds xz/lz4 compression, SOW-0018 changes compact journal layout, SOW-0019 adds FSS cryptographic tag/verification behavior, and SOW-0020 changes directory traversal and journalctl directory behavior.
   - Reason: performance findings are expected to require profiling, allocation reduction, buffer reuse, and refactoring; doing that before remaining feature work risks rework and invalidated results.
   - 2026-05-26 update: the SNMP traps performance report makes SOW-0009 a critical integration gate before production replacement in SOW-0026, not only a final broad pass.
   - 2026-05-26 clarification: this does not mean SOW-0009 should preempt SOW-0023 or remaining feature work. Feature development still comes first.
   - Implication: benchmark/profiling can be phased by hot path after the relevant feature surface is complete. The first phase may focus on Go writer/SNMP traps and critical reader paths while still preserving full conformance gates after optimization.
   - Risk: performance work before every reader/directory feature is done can still create rework, so activation must wait until the features that affect the measured hot path are complete.

3. Netdata production replacement gate
   - Decision: this SOW now blocks claims that the SDK can replace Netdata's vendored NetFlow/OTEL journal logic or no-libsystemd reader path in production.
   - Evidence: user-reported Go SDK writer throughput is about 5k logs/s in the SNMP traps ingestion worker, compared with about 25k logs/s for Netdata NetFlow with the vendored Rust implementation.
   - Implication: SOW-0026 can still do integration scaffolding or staged experiments, but production replacement needs SOW-0009 evidence or a user-approved exception.
   - Risk: without this gate, Netdata could replace proven hot paths with SDK paths that are correct but operationally too slow.

## Plan

1. Finish SOW-0023 and remaining feature SOWs that affect writer/reader hot paths.
2. Select benchmark environment controls, commands, and reporting format before activation.
3. Run systemd, Netdata vendored Rust, SDK Rust, SDK Go, SDK Node.js, and SDK Python baselines on the SOW-0014 performance corpus and Netdata-shaped corpora.
4. Profile and optimize measured bottlenecks, starting with any path that reproduces the Go writer 5k logs/s class result.
5. Review performance evidence plus full conformance/interoperability/byte-compatibility/live-reader reruns before closing.

## Delegation Plan

- Implementation routing: local implementation unless the user explicitly re-enables external implementers.
- Reviewers: at least two from `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`.
- Skip `llm-netdata-cloud/mimo-v2.5-pro` while the user reports it is out of quota.
- Every prompt must include the canonical repository-boundary block from `AGENTS.md`.
- Failure handling: record implementer or reviewer model failure in this SOW, substitute only from the approved model list, rerun full-scope review after fixes, and do not close if `.agents/sow/audit.sh` fails.

## Execution Log

2026-05-26:

- Updated priority from final broad benchmark pass to critical Netdata integration gate.
- Recorded user-reported performance issue: Go SDK writer around 5k logs/s in the SNMP traps ingestion worker, compared with Netdata NetFlow vendored Rust around 25k logs/s and prior Rust measurements around 30k logs/s.
- Expanded scope to require Netdata-shaped writer and reader benchmarks, including `systemd-journal.plugin` no-libsystemd reader performance.
- Added production replacement gate for SOW-0026.
- Recorded user clarification that SOW-0023 and remaining feature work should finish before optimization, so performance work does not slip with later feature changes.

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
