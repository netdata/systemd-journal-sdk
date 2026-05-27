# SOW-0009 - Benchmark Profile Optimize

## Status

Status: in-progress

Sub-state: resumed after SOW-0035 defined and implemented the production
rotation/max-size contract. Existing benchmark results that used a 2 GiB
default max-size are retained as invalidated evidence and must not be used as
production baseline evidence. Next work is writer profiling and optimization
from the fixed-128 MiB compact/no-compression/no-FSS baseline.

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
- The user clarified on 2026-05-26 after SOW-0027 completed that actual Netdata integration should happen last because the SDK does not yet perform well enough to replace the older vendored libraries. The remaining compatibility feature/gap SOWs should complete first, then this performance SOW, then SOW-0026 integration.
- Netdata replacement is not production-ready if the SDK writer or reader hot paths are materially slower than the current vendored Rust implementation without a measured explanation and a user-approved exception.
- Optimizations must be measurement-driven and must not weaken conformance.
- The user knows the Rust implementation can commit around 30k rows per second on one core for about 32 mixed-cardinality fields, and this is useful context but not a formal pass/fail threshold until measured on the project benchmark environment.
- Reader performance is also a production gate for the no-libsystemd `systemd-journal.plugin` path and for Netdata reader/query/rebuild consumers.
- The user clarified on 2026-05-27 that writer and reader benchmarks must be independent, writers come first, and the first writer baseline is compact format with no compression and no FSS.
- Current SDK readers do support ordered multi-file reading. The shared spec records that directory readers and `OpenFiles` merge candidate entries across all opened files using systemd-compatible ordering, and Go/Rust/Node.js/Python all have `DirectoryReader` merge implementations.
- The user clarified on 2026-05-27 that reader benchmarking must stress both single-file reader performance and directory/multi-file ordered reader performance.
- The user clarified on 2026-05-27 that systemd C ingestion and reading performance is the absolute reference baseline. Rust and Go should be significantly faster than systemd C for the hot Netdata use cases, not merely comparable.
- The user clarified on 2026-05-27 that previous benchmark numbers were not acceptable if they measured JSON ingestion overhead instead of actual writer performance. Writer tests must measure the append loop itself.
- The first all-language compact/no-compression/no-FSS writer-core run with aligned hash-table sizing on 2026-05-27 produced about 39.9k rows/sec for systemd C, 3.1k rows/sec for Rust, 2.4k rows/sec for Go, 957 rows/sec for Node.js, and 695 rows/sec for Python at a 2 GiB max-size setting. These numbers remain useful diagnostic evidence for the measured exposed writer paths, but SOW-0035 invalidated them as production baseline evidence because production max-size must be fixed to 128 MiB unless the benchmark explicitly tests directory rotation.
- The valid fixed-128 MiB single-file compact/no-compression/no-FSS writer-core baseline on 2026-05-27 used 100k rows because stock systemd correctly rejects additional appends once the configured 128 MiB max-size is reached. That run produced about 32.1k rows/sec for systemd C, 2.3k rows/sec for Rust, 2.3k rows/sec for Go, 956 rows/sec for Node.js, and 649 rows/sec for Python.
- The user clarified on 2026-05-27 that production writers must be designed around exclusive writer ownership of the journal file: no hot-path allocations where caller-owned data can be used for the duration of append, no hot-path file-state syscalls except extension and explicit sync/close, careful publication ordering for one writer with multiple lockless readers, careful write planning to avoid I/O amplification, and rare flushes.
- The Rust SDK has a concrete hot-path regression versus the vendored Rust source: `rust/src/crates/journal-core/src/file/mmap.rs` calls `file.metadata()?.len()` in `create_window`, `get_slice`, and `get_slice_mut`. Commit `6368d5f` introduced these checks after the initial Rust import. The Netdata vendored Rust source at `ktsaou/netdata @ 00305266364e`, `src/crates/journal-core/src/file/mmap.rs`, does not do per-access metadata checks in `get_slice` or `get_slice_mut`.
- systemd's writer publication model is not "all header fields last". In `systemd/systemd @ c0a5a2516d28`, `src/libsystemd/sd-journal/journal-file.c`, systemd updates object and data/field counters during object linking, but it writes target objects before publishing references and uses an explicit atomic fence before entry reachability in `journal_file_link_entry`. The SDK optimization target must mirror this real publication model instead of inventing a stricter but incompatible one.

Inferences:

- SOW-0014 provides the accepted performance corpus.
- SOW-0015 provides the systemd and SDK ingesters used by benchmarks.
- systemd should be the reference baseline, with Rust also tracked as a known high-performance implementation.
- Netdata's vendored Rust writer and reader/index/query paths must be measured as practical replacement baselines, not only stock systemd.
- The benchmark plan needs both microbenchmarks and end-to-end Netdata-shaped paths. The Go writer result reported by the user may include SDK overhead, SNMP traps worker overhead, sync cadence, compact/compression choices, field remapping, locking, retention, or dataset/cardinality effects; these must be separated by profiling.

Unknowns:

- Exact CPU governor and final production thresholds are not selected yet.
- The initial fixed-max-size single-file writer baseline parameters are selected: current workstation, repository-local `.local/` output, 100k-row deterministic performance subset that fits a 128 MiB single journal file, compact format, compression disabled, FSS disabled, one writer process, one final writer sync/close, and JSON result reporting.
- The 200k-row deterministic performance corpus remains required for SOW-0009, but it must be measured with directory rotation or an explicitly larger max-size diagnostic run. It must not be forced into a single 128 MiB file.
- Exact pass/fail thresholds are not selected yet. The default recommended gate is to require the Go writer to be close enough to the vendored Rust baseline for Netdata ingestion use, and to reject replacement if the gap remains in the 5k vs 25k logs/s class after profiling.

### Acceptance Criteria

- Benchmarks cover deterministic ingestion writing for systemd, Rust, Go, Node.js, and Python using the SOW-0014 performance corpus of about 200k accepted rows.
- Benchmarks cover Netdata-shaped writer workloads for SNMP traps, NetFlow, and OTEL logs, including the sync cadence, field counts, value cardinality, binary values, source realtime handling, remapping behavior, compact/regular output selection, compression, FSS, rotation, and retention policies expected by Netdata.
- Benchmarks reproduce or reject the user-reported Go SNMP traps writer result of about 5k logs/s with controlled measurements and a breakdown of SDK time versus caller/worker overhead.
- Benchmarks cover reading, live one-writer/multiple-reader operation, filtering, journalctl queries, query-unique/facet-style scans, cursor/seek behavior, directory traversal, corruption handling where relevant, and cross-language file sizes.
- Reader stress benchmarks must separately measure single-file readers and ordered multi-file/directory readers.
- Reader benchmarks include SDK idiomatic readers, libsystemd-compatible facades, file-backed journalctl rewrites, Netdata `jf`-style behavior after SOW-0027, stock `journalctl`, stock libsystemd where allowed, and Netdata's current reader/index/query paths where practical.
- Benchmarks include mixed-cardinality profiles centered around about 32 fields per row, plus cardinality sweeps that isolate low-cardinality, high-cardinality, mostly-unique, binary-heavy, and large-value workloads.
- Benchmark results include rows per second, bytes per second, output file size, CPU time, wall time, memory allocation/heap behavior where available, fsync/sync policy, and compression mode.
- systemd C ingestion and reading are measured as the absolute reference baselines, and Netdata's vendored Rust implementation is measured as the practical replacement baseline where applicable.
- Rust and Go writer/reader paths must be optimized to substantially exceed the systemd C baseline for hot Netdata workloads. If they do not, this SOW must record profiles, bottlenecks, attempted fixes, and a user decision before closing.
- Profiling identifies bottlenecks before optimization work.
- Optimizations are driven by measurements and do not weaken conformance. Profiling must specifically account for allocations, buffer reuse, hashing, object lookup, data/field deduplication, compression, sealing, remapping, lock handling, append publication, reader decompression, query filtering, and directory traversal.
- Writer optimization must explicitly report per-language hot-path allocation behavior, file/syscall behavior, mmap or pwrite strategy, publish ordering, write amplification, and flush policy.
- Rust and Go optimized writer paths must target production hot-path use. For compact/no-compression/no-FSS append, the target design is no per-field heap allocation, no per-object file metadata syscall, bounded scratch/state allocation after warmup, and no readback of writer-owned state.
- Node.js and Python optimizations must still reduce allocation/syscall/readback overhead, but this SOW must record whether pure-runtime constraints prevent them from being production hot-path replacements.
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
- SOW-0023 completed the high-level writer API used by Netdata writer integrations.
- SOW-0027 completed the reader-side SDK and `jf` facade behavior needed for Netdata reader integrations.
- Compatibility feature/gap SOWs are complete through SOW-0034. This SOW is now active before SOW-0026 Netdata integration.

Risks:

- Premature optimization can introduce compatibility regressions.
- Unrepresentative fixtures can create misleading performance claims.
- Comparing writers without controlling sync policy, compression, CPU governor, and filesystem can produce invalid conclusions.
- Performance refactors made before xz/lz4, compact journal, FSS, and directory traversal parity work may be invalidated by those later feature changes.
- Waiting too long now risks blocking SNMP traps, NetFlow, OTEL, and no-libsystemd reader integration after API work is otherwise ready.
- Optimizing only synthetic deterministic fixtures can miss the actual SNMP traps, NetFlow, OTEL, and systemd-journal reader bottlenecks.

## Pre-Implementation Gate

Status: ready - active writer benchmark phase

Problem / root-cause model:

- Optimization before the relevant API and file-format surfaces are complete risks hard-to-diagnose compatibility bugs, repeated performance regressions, and churn. That prerequisite is now satisfied for the compatibility feature/gap chain through SOW-0034.
- The user-reported Go writer result of about 5k logs/s versus the current Netdata Rust path around 25k logs/s is too large to ignore. Start with independent writer measurements so SDK writer cost is separated from reader behavior and from Netdata caller overhead.
- Working theory: the Go writer deficit may come from allocation-heavy field handling, repeated buffer construction, object/hash lookup cost, remapping overhead, compression/compact choices, lock/sync cadence, caller overhead, or a combination. This is speculation until profiles isolate the hot paths.
- Reader performance risk is separate: no-libsystemd `systemd-journal.plugin` and Netdata reader/query/rebuild paths need efficient sequential scan, filtering, field extraction, directory traversal, and compressed/compact/sealed handling.
- Reader performance must be split into at least two stress surfaces: single-file sequential/query reading and ordered multi-file/directory reading.
- The systemd C implementation is the reference floor, not the aspirational target. Rust and Go should beat it substantially in the Netdata hot paths.
- The current all-language baseline shows a common writer-architecture problem, not only a Go syscall problem. Rust is also far below systemd despite using mmap, so profiling must inspect per-object accessor cost, cached file/window state, data/field lookup behavior, entry-array updates, header publication, and write amplification across every language.
- The writer correctness target is one exclusive writer with concurrent lockless readers. Optimization must preserve live-reader safety by writing object bytes before publishing references, publishing entry visibility last, and using the appropriate ordering primitive for each language/runtime.

Evidence reviewed:

- Product scope spec.
- Pending implementation and interoperability SOWs.
- User performance requirement from 2026-05-24.
- User sequencing decision from 2026-05-24: push SOW-0009 to the end instead of running a baseline-only benchmark now.
- User performance update from 2026-05-26: SNMP traps worker reports the Go SDK writer at about 5k logs/s, compared with Netdata NetFlow vendored Rust around 25k logs/s and prior Rust measurements around 30k logs/s.
- User sequencing clarification from 2026-05-26: finish SOW-0023 and the remaining feature work first; optimize only after feature development so performance work does not slip as features change.
- SOW-0026 Netdata integration scope includes replacing NetFlow/OTEL vendored copies and the no-libsystemd `systemd-journal.plugin` reader path, making performance a production gate.
- SOW-0034 completed the remaining file-backed journalctl query/follow gap, so the compatibility feature/gap chain is complete enough to start this SOW.
- User requirement from 2026-05-27: stress both single-file reader and directory-reader performance, with systemd C ingestion and reading as the absolute baseline.
- 2026-05-27 reader behavior check: `.agents/sow/specs/product-scope.md` records that directory readers and `OpenFiles` merge candidate entries across all opened files using systemd-compatible ordering. Go `go/journal/reader.go` exposes `OpenDirectory`, `OpenFiles`, and `DirectoryReader.stepMerged`; Rust `rust/src/journal/src/lib.rs` exposes `DirectoryReader::open`, `DirectoryReader::open_files`, and `step_merged`; Node.js `node/src/lib/directory-reader.js` exposes `DirectoryReader.open`, `openFiles`, and `_stepMerged`; Python `python/journal/directory_reader.py` exposes `DirectoryReader.open`, `open_files`, and `_step_merged`.
- 2026-05-27 all-language writer baseline: `.local/benchmarks/writer-core/compact-none-fss-off-20260527T140002Z/report.json` recorded systemd C at about 39949.794 rows/sec, Rust SDK at about 3144.473 rows/sec, Go SDK at about 2418.997 rows/sec, Node.js SDK at about 956.737 rows/sec, and Python SDK at about 695.491 rows/sec. All outputs were 310378496 bytes and passed stock `journalctl --verify --file`.
- Rust hot-path regression evidence: `rust/src/crates/journal-core/src/file/mmap.rs:165`, `rust/src/crates/journal-core/src/file/mmap.rs:284`, and `rust/src/crates/journal-core/src/file/mmap.rs:297` call file metadata on window creation or object access. The regression was introduced in commit `6368d5f` on 2026-05-24.
- Go allocation/syscall evidence: `go/journal/writer.go:269` builds per-entry payload slices, `go/journal/writer.go:274` allocates a `name=value` buffer per field, `go/journal/writer.go:301` allocates an entry buffer per entry, `go/journal/writer.go:643` and `go/journal/writer.go:668` publish header metadata through many separate writes, and `go/journal/writer.go:1046` allocates an 8-byte buffer per integer write.
- Node.js allocation/syscall evidence: `node/src/lib/writer.js:269` builds payload arrays, `node/src/lib/writer.js:275` allocates a payload buffer per field, `node/src/lib/writer.js:304` allocates an entry buffer per entry, `node/src/lib/writer.js:597` and `node/src/lib/writer.js:608` publish header metadata through many small writes, and `node/src/lib/writer.js:935` reads written objects back for sealing.
- Python allocation/syscall evidence: `python/journal/writer.py:287` builds payload lists, `python/journal/writer.py:298` allocates concatenated payload bytes per field, `python/journal/writer.py:319` allocates an entry bytearray per entry, `python/journal/writer.py:574` and `python/journal/writer.py:584` publish header metadata through many small writes, and `python/journal/writer.py:681` reads data state back while linking entries.

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

Sensitive data gate:

- Durable artifacts changed in this chunk contain generated benchmark commands,
  synthetic fixture names, code paths, and aggregate performance numbers only.
  No raw secrets, customer identifiers, production endpoints, or private
  operational data are required for this SOW.

Implementation plan:

1. Add a writer-only benchmark harness for compact format, compression disabled, FSS disabled, and the deterministic writer corpus. Use a 100k-row subset for the fixed-128 MiB single-file baseline and the full 200k-row corpus for directory-rotation stress.
2. Build optimized SDK ingesters and the systemd v260.1 reference ingester inside `.local/`.
3. Run baseline measurements for systemd compact, SDK Rust compact, SDK Go compact, SDK Node.js compact, and SDK Python compact.
4. Record wall time, user/system CPU time, peak RSS where available, output file size, bytes/sec, rows/sec, commands, tool versions, and verification results.
5. Profile Rust first enough to confirm the `metadata()?.len()` regression and window behavior, because Rust is the cleanest comparison against the vendored mmap implementation.
6. Repair Rust hot-path metadata/window behavior and rerun the all-language writer baseline to separate one concrete regression from the deeper writer-architecture issues.
7. Profile Go next and design the mmap-backed writer core plus zero-allocation fast API before broad Go rewrites.
8. Profile Node.js and Python enough to classify runtime overhead versus shared writer-architecture overhead.
9. Add Netdata-shaped writer benchmarks after the generic 32-field compact baseline.
10. Add reader benchmarks only after the writer baseline/profiling phase is established.
11. Re-run conformance, interoperability, live concurrency, byte-compatibility, and mixed-format tests after each optimization.

Validation plan:

- Benchmark commands, environment, raw results, and summarized results recorded.
- Initial writer baseline records compact=true, compression=none, FSS=false, generated corpus hash, rows, fields per row, sync policy, final state, and output path.
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
- SOW lifecycle: active SOW before SOW-0026 production Netdata replacement claims.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

- Exact production thresholds must be selected before close. Recommended default: replacement is not acceptable while Go writer or critical reader paths remain in the same class as the user-reported 5k logs/s result against a roughly 25k logs/s current Netdata Rust baseline.
- CPU governor/filesystem controls should be recorded from the environment. If the environment is not stable enough for final production evidence, this SOW will still use it for directional profiling and record the limitation.
- Reader benchmark workload order remains open until the writer phase produces a stable harness and first optimization targets.

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

4. 2026-05-27 writer/reader benchmark split
   - Decision: benchmark writers and readers independently.
   - Decision: start with writers.
   - Decision: first writer baseline is compact journal format, compression disabled, FSS disabled.
   - Implication: the first numbers isolate core writer append/object/hash behavior for the format Netdata writers will prefer by default.
   - Risk: this first baseline does not yet prove compressed, sealed, rotation/retention, SNMP traps, NetFlow, OTEL, or reader performance; those remain in this SOW.

5. 2026-05-27 reader multi-file support answer
   - Fact: the current SDK readers are no longer single-file only. Rust, Go, Node.js, and Python support directory/open-files readers that merge entries across files with systemd-compatible ordering.
   - Evidence: `.agents/sow/specs/product-scope.md` records the contract; each language has a `DirectoryReader` with a candidate-merge loop.
   - Implication: reader benchmarks later in this SOW must test both single-file and ordered multi-file/directory reading, because both are production surfaces now.

6. 2026-05-27 systemd C baseline
   - Decision: systemd C is the absolute baseline for both ingestion and reading.
   - Decision: Rust and Go should be a lot faster than systemd C for the hot Netdata writer/reader paths.
   - Implication: benchmark summaries must report ratios against systemd C, not only absolute rows/sec.
   - Risk: if Rust or Go cannot beat systemd C after profiling and optimization, this SOW cannot close without profiles, bottleneck evidence, and a user decision.

7. 2026-05-27 writer-core measurement correction
   - Decision: writer performance benchmarks must time only the writer append loop.
   - Evidence: the first ingestion harness timed the full external ingester process, which includes JSONL parsing and value materialization before each append.
   - Decision: the corrected writer-core benchmark pre-materializes rows before timing, creates the writer before timing, stops timing immediately after the last append, and reports final close/sync separately.
   - Implication: writer-core append rows/sec is the primary writer metric. Process rows/sec and JSONL ingestion rows/sec remain useful secondary stress metrics, but they must not be used as the core writer throughput number.
   - Risk: this benchmark still measures each language's public/low-level append API overhead. That is intentional for SDK use, but lower-level future APIs may need separate microbenchmarks if optimization work adds them.

8. 2026-05-27 hash-table sizing and API-mode correction
   - Fact: systemd v260.1 sizes the data hash table as `MAX(max_size * 4 / 768 / 3, 2047)` and the field hash table as `1023`.
   - Evidence: `systemd/systemd @ c0a5a2516d28`, `src/libsystemd/sd-journal/journal-file.c:48`, `src/libsystemd/sd-journal/journal-file.c:49`, `src/libsystemd/sd-journal/journal-file.c:1292`, `src/libsystemd/sd-journal/journal-file.c:1293`, and `src/libsystemd/sd-journal/journal-file.c:1323`.
   - Finding: the first 200k Rust/Go-only benchmark compared systemd with 2 GiB max-size hash-table sizing against SDK drivers using smaller defaults, so its SDK/systemd ratios were not valid.
   - Decision: writer-core drivers now all accept `--max-size-bytes`, default to 128 MiB, and report `data_hash_table_buckets`, `field_hash_table_buckets`, and `max_size_bytes`.
   - Decision: writer-core results now report `api_mode`. systemd and the current Rust driver use prebuilt raw `KEY=VALUE` payloads; Go, Node.js, and Python use public field APIs that construct payloads inside append.
   - Implication: the corrected baseline is valid for the actual exposed writer paths it measures, but raw-payload and field-api ratios are not a pure language-runtime comparison.
   - Risk: larger systemd-style hash tables expose large initial arena behavior; writers must create an initial file large enough to cover hash tables before the first append.

9. 2026-05-27 writer ownership and publication principles
   - Decision: production writer paths must assume exclusive writer ownership after a file is opened for writing. The writer must trust its in-memory state unless it deliberately extends, syncs, closes, or reopens the file.
   - Decision: compact/no-compression/no-FSS hot append paths should avoid per-field and per-object heap allocation where caller-owned data can remain valid until append returns.
   - Decision: writes must preserve live-reader safety: object bytes are written before references to them, indexes are published only after their target objects are complete, and entry visibility is published last with an explicit ordering primitive appropriate to the language/runtime.
   - Decision: file growth should happen in large planned increments, not as a side effect of every small object access.
   - Decision: flushes are explicit and rare. Benchmarks must report whether a run includes only final close/sync or periodic sync.
   - Evidence: `systemd/systemd @ c0a5a2516d28`, `src/libsystemd/sd-journal/journal-file.c:753`, `src/libsystemd/sd-journal/journal-file.c:1230`, and `src/libsystemd/sd-journal/journal-file.c:2253`.
   - Implication: optimization work is allowed to add fast lower-level APIs and internal state caches as long as conformance and live-reader compatibility are preserved.
   - Risk: optimizing by only removing syscalls or only reducing allocations will not be enough if publish ordering and object revisit patterns still amplify writes.

10. 2026-05-27 Rust writer optimization requirements
   - Fact: the Rust SDK currently calls `file.metadata()?.len()` in `WindowManager::create_window`, `get_slice`, and `get_slice_mut`, while the vendored Rust implementation does not do this in `get_slice` or `get_slice_mut`.
   - Evidence: `rust/src/crates/journal-core/src/file/mmap.rs:165`, `rust/src/crates/journal-core/src/file/mmap.rs:284`, `rust/src/crates/journal-core/src/file/mmap.rs:297`, and commit `6368d5f`.
   - Requirement: split writer-owned mmap behavior from reader validation behavior. The writer path must cache file size and update it only on deliberate extension.
   - Requirement: remove tail-window clamping that makes append-tail windows end at the previous file length and forces remapping on the next extension.
   - Requirement: preserve integer overflow hardening added in `6368d5f`.
   - Requirement: add explicit publish ordering before entry reachability.
   - Requirement: inspect and reduce cache-key allocations in `FieldCache` and `RecentDataCache`, because both currently copy payloads into owned boxed slices.
   - Requirement: pin or otherwise optimize hot regions: header, hash tables, append tail, and entry-array tails.
   - Validation: rerun the compact/no-compression/no-FSS writer-core baseline before broad Rust refactors so the `metadata()` regression impact is measured separately.

11. 2026-05-27 Go writer optimization requirements
   - Fact: the Go writer uses a field API that forces internal `name=value` construction and many small `pread`/`pwrite` operations.
   - Evidence: `go/journal/writer.go:77`, `go/journal/writer.go:269`, `go/journal/writer.go:274`, `go/journal/writer.go:301`, `go/journal/writer.go:643`, `go/journal/writer.go:668`, `go/journal/writer.go:883`, and `go/journal/writer.go:1046`.
   - Requirement: keep the existing public API as a compatibility wrapper, but add a fast append API that accepts caller-owned byte slices or name/value views and does not concatenate unless unavoidable.
   - Requirement: replace the current small-`pwrite` writer core with a mmap-backed or equivalently mapped arena path that does not use CGO or system journal libraries.
   - Requirement: cache writer-owned state including append offset, file size, hash bucket tails, entry-array tail offset/capacity/count, recent data offsets, and per-data entry-array tail state.
   - Requirement: remove hot sanity reads of writer-owned objects, including previous-tail object checks, unless profiling and compatibility evidence prove they are required.
   - Requirement: hash and seal from constructed object bytes or in-memory views instead of reading objects back from the file.
   - Validation: profile syscall counts, allocation counts, and CPU before and after the mmap/fast-API work; Go cannot close with a material deficit against the selected Netdata baseline without a user decision.

12. 2026-05-27 Node.js writer optimization requirements
   - Fact: the Node.js writer currently allocates buffers per field/object and uses many synchronous small writes and reads.
   - Evidence: `node/src/lib/writer.js:269`, `node/src/lib/writer.js:275`, `node/src/lib/writer.js:304`, `node/src/lib/writer.js:597`, `node/src/lib/writer.js:608`, `node/src/lib/writer.js:678`, `node/src/lib/writer.js:719`, and `node/src/lib/writer.js:935`.
   - Requirement: do not add native runtime addons for mmap unless the user explicitly changes the pure Node.js runtime constraint.
   - Requirement: add a compact fast path that avoids unnecessary `BigInt` where compact offsets fit in safe numeric ranges, if profiling shows `BigInt` is significant.
   - Requirement: add Buffer-oriented fast APIs and reduce `Buffer.alloc`/`Buffer.from` churn with reusable scratch buffers or fully initialized unsafe buffers where safe.
   - Requirement: batch or group metadata writes where live-reader publication order permits, keeping entry visibility last.
   - Requirement: cache hash bucket and entry-array tail state and reduce readback-heavy sealing paths.
   - Validation: this SOW must record whether pure Node.js runtime constraints make Rust/Go-class throughput unrealistic.

13. 2026-05-27 Python writer optimization requirements
   - Fact: the Python writer currently constructs payload bytes, bytearrays, dict/list entry items, and many small `pread`/`pwrite` operations in the hot path.
   - Evidence: `python/journal/writer.py:287`, `python/journal/writer.py:298`, `python/journal/writer.py:319`, `python/journal/writer.py:391`, `python/journal/writer.py:574`, `python/journal/writer.py:584`, `python/journal/writer.py:647`, and `python/journal/writer.py:681`.
   - Requirement: use Python's standard `mmap` module or an equivalent pure-runtime mapped writer path for the arena.
   - Requirement: add bytes/memoryview-oriented fast APIs and avoid converting bytearray or memoryview values to owned bytes when append can consume them before returning.
   - Requirement: use `struct.pack_into` directly into mmap or reusable scratch buffers instead of per-integer `struct.pack` plus `os.pwrite`.
   - Requirement: cache writer-owned hash bucket, entry-array, and data-entry-array tail state instead of reading it back.
   - Requirement: reduce dict/list churn for entry items where a fixed tuple, lightweight object, or reusable array is sufficient.
   - Validation: this SOW must record whether pure Python runtime constraints make Rust/Go-class throughput unrealistic.

14. 2026-05-27 production hot-path tiering open decision
   - Open decision: whether production hot-path guarantees should be required only for Rust and Go, with Node.js and Python kept as correctness/interoperability SDKs, or whether all four languages must target production-hot-path throughput.
   - Option A: Rust and Go are Tier 1 production hot path; Node.js and Python are Tier 2 correctness/interoperability and moderate-throughput SDKs.
   - Option B: all four languages target production hot path.
   - Recommendation: Option A, unless a concrete Netdata production ingestion path requires Node.js or Python as the writer runtime. This aligns engineering effort with runtime limits and still preserves correctness and interoperability for every language.
   - Risk of Option A: Node.js/Python users may see lower throughput than Rust/Go and need documentation.
   - Risk of Option B: significant effort may be spent chasing runtime ceilings that still cannot reach the Rust/Go/systemd class without violating pure-runtime constraints.

## Plan

1. Implement and run writer compact/no-compression/no-FSS baseline harness.
2. Profile Rust enough to confirm and isolate the `metadata()`/window regression.
3. Repair Rust writer-owned mmap behavior and rerun the all-language writer baseline.
4. Profile and redesign the Go writer core around a mmap-backed or equivalently mapped arena plus a fast caller-owned-data API.
5. Optimize measured writer hot paths without changing file compatibility or live-reader safety.
6. Profile Node.js and Python to separate shared writer-architecture overhead from runtime overhead and decide production tiering.
7. Add Netdata-shaped writer workloads for SNMP traps, NetFlow, and OTEL.
8. Add reader benchmarks for single-file and ordered multi-file/directory reading.
9. Review performance evidence plus full conformance/interoperability/byte-compatibility/live-reader reruns before closing.

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

2026-05-27:

- Activated SOW-0009 after compatibility feature/gap completion through SOW-0034.
- Recorded user decision to benchmark writers and readers independently, start with writers, and use compact/no-compression/no-FSS as the first writer baseline.
- Verified current readers support ordered multi-file/directory reading in all four languages; reader benchmarks must include that surface later.
- Added writer-core benchmark drivers for systemd C, Rust, Go, Node.js, and Python.
- Added `tests/benchmarks/run_writer_core_benchmarks.py`, which reports append-loop time separately from row generation, writer creation, close/sync, verification, and process wall time.
- Kept `tests/benchmarks/run_writer_benchmarks.py` as the JSONL ingestion benchmark and documented the distinction in `tests/benchmarks/README.md`.
- Preserved the compact arena-growth correctness fix in Rust, Go, Node.js, and Python writers; removed the incomplete Go cache optimization scaffolding from this chunk.
- Corrected benchmark methodology evidence:
  - `tests/datasets/generate.py` produces exactly 32 fields per performance row: 4 fixed, 12 low-cardinality, 8 medium-cardinality, and 8 high-cardinality fields.
  - The JSONL ingestion benchmark allocates/materializes records before append, so it is not writer-core evidence.
- Writer-core smoke and validation:
  - `python3 tests/benchmarks/run_writer_core_benchmarks.py --languages go rust node python --rows 10 --repetitions 1 --warmups 0 --format compact --final-state online --skip-verify --keep-journals` passed.
  - `python3 tests/benchmarks/run_writer_core_benchmarks.py --languages systemd --rows 10 --repetitions 1 --warmups 0 --format compact --final-state online --skip-verify --keep-journals` passed.
  - `python3 tests/benchmarks/run_writer_core_benchmarks.py --languages systemd rust go node python --rows 1000 --repetitions 1 --warmups 0 --format compact --final-state online --keep-journals` passed.
  - `python3 tests/benchmarks/run_writer_core_benchmarks.py --languages systemd --rows 10 --repetitions 1 --warmups 0 --format compact --final-state archived --keep-journals` passed.
- First 200k writer-core baseline attempt, later invalidated for cross-language ratios:
  - Command: `python3 tests/benchmarks/run_writer_core_benchmarks.py --languages systemd rust go --rows 200000 --repetitions 1 --warmups 0 --format compact --final-state online --keep-journals`.
  - Report: `.local/benchmarks/writer-core/compact-none-fss-off-20260527T131857Z/report.json`.
  - Environment: systemd 260.1-2-manjaro, CPU governor `powersave`, ext2/ext3 filesystem type from `stat -f`, Go 1.26.3, Rust 1.91.1.
  - systemd C: 200000 rows, 4.964381125 append seconds, 40286.995491306 append rows/sec, 310378496-byte journal, stock `journalctl --verify --file` passed.
  - Rust SDK: 200000 rows, 159.858116552 append seconds, 1251.1094482646572 append rows/sec, 251658240-byte journal, stock `journalctl --verify --file` passed.
  - Go SDK: 200000 rows, 104.772125549 append seconds, 1908.9046724213272 append rows/sec, 251658240-byte journal, stock `journalctl --verify --file` passed.
  - Disposition: do not use these ratios as evidence. External review found that systemd used the 2 GiB max-size hash-table formula while SDK drivers used smaller default hash tables. The file-size mismatch, systemd 310378496 bytes vs SDK 251658240 bytes, was concrete evidence of different initial layout parameters.
- First reviewer pass for the writer-core harness chunk:
  - `llm-netdata-cloud/glm-5.1`: production-grade with a profiling note about Rust arena/window behavior.
  - `llm-netdata-cloud/minimax-m2.7-coder`: production-grade with a note that the severe SDK deficit is a benchmark finding, not a harness defect.
  - `llm-netdata-cloud/qwen3.6-plus`: reviewer command exited successfully but produced no useful review output, so no acceptance evidence was taken from it.
  - `llm-netdata-cloud/kimi-k2.6`: not production-grade because hash-table sizing differed and API modes were not labelled. Disposition: fixed in this chunk before accepting benchmark numbers.
- Corrected writer-core harness and writer fixes:
  - The harness passes `--max-size-bytes` to all drivers and records `api_mode`, `data_hash_table_buckets`, `field_hash_table_buckets`, and `max_size_bytes`.
  - Rust, Go, Node.js, and Python writers now size the initial arena by rounding the initial append offset up to systemd's 8 MiB file-size increment. This covers large initial hash tables before the first append.
  - Added large-hash-table writer tests in Rust, Go, Node.js, and Python. Each creates a compact writer with `600000` data buckets, appends one entry, verifies the initial arena grew beyond 8 MiB, and runs stock `journalctl --verify --file` when available.
- Corrected all-language 1000-row smoke:
  - Command: `python3 tests/benchmarks/run_writer_core_benchmarks.py --languages systemd rust go node python --rows 1000 --repetitions 1 --warmups 0 --format compact --final-state online --keep-journals`.
  - Report: `.local/benchmarks/writer-core/compact-none-fss-off-20260527T135929Z/report.json`.
  - Result: passed for systemd, Rust, Go, Node.js, and Python. Every driver reported `3728270` data buckets, `1023` field buckets, `2147483648` max-size bytes, 67108864-byte output, compact=true, compression flags=0, and stock `journalctl --verify --file` passed.
- Corrected all-language 200k writer-core diagnostic run with aligned 2 GiB
  max-size sizing:
  - Command: `timeout 1800 python3 tests/benchmarks/run_writer_core_benchmarks.py --languages systemd rust go node python --rows 200000 --repetitions 1 --warmups 0 --format compact --final-state online --keep-journals`.
  - Report: `.local/benchmarks/writer-core/compact-none-fss-off-20260527T140002Z/report.json`.
  - Environment: systemd 260.1-2-manjaro, Intel i9-12900K, CPU governor `powersave`, ext2/ext3 filesystem type from `stat -f`, Go 1.26.3, Rust 1.91.1, Node.js 22.22.2, Python 3.14.5.
  - systemd C: 200000 rows, 5.006284 append seconds, 39949.794 rows/sec, 310378496-byte journal, raw-payload API mode, stock `journalctl --verify --file` passed.
  - Rust SDK: 200000 rows, 63.603658 append seconds, 3144.473 rows/sec, 310378496-byte journal, raw-payload API mode, stock `journalctl --verify --file` passed.
  - Go SDK: 200000 rows, 82.678886 append seconds, 2418.997 rows/sec, 310378496-byte journal, field-api mode, stock `journalctl --verify --file` passed.
  - Node.js SDK: 200000 rows, 209.043956 append seconds, 956.737 rows/sec, 310378496-byte journal, field-api mode, stock `journalctl --verify --file` passed.
  - Python SDK: 200000 rows, 287.566478 append seconds, 695.491 rows/sec, 310378496-byte journal, field-api mode, stock `journalctl --verify --file` passed.
  - Fact: with aligned hash-table sizing and identical output size, the SDK writer paths are still materially slower than systemd C on this workload. Rust is 0.079x systemd C, Go is 0.061x, Node.js is 0.024x, and Python is 0.017x.
  - Disposition after SOW-0035: keep this as diagnostic evidence only. It is
    not the production baseline because it used a 2 GiB max-size setting.
- Fixed-128 MiB production single-file benchmark correction from SOW-0035:
  - Attempted command: `timeout 1800 python3 tests/benchmarks/run_writer_core_benchmarks.py --languages systemd rust go node python --rows 200000 --repetitions 1 --warmups 0 --format compact --final-state online --keep-journals`.
  - Report: `.local/benchmarks/writer-core/compact-none-fss-off-20260527T151651Z/report.json`.
  - Result: invalid as a single-file comparison. Stock systemd correctly stopped at 104628 records when the 128 MiB max-size was reached, while the SDK direct-file benchmark drivers do not use `MaxFileSize` as an append limiter. This proves 200k rows cannot be the fixed-128 MiB single-file baseline.
  - Valid command: `timeout 1800 python3 tests/benchmarks/run_writer_core_benchmarks.py --languages systemd rust go node python --rows 100000 --repetitions 1 --warmups 0 --format compact --final-state online --keep-journals`.
  - Report: `.local/benchmarks/writer-core/compact-none-fss-off-20260527T153135Z/report.json`.
  - Result: passed for systemd, Rust, Go, Node.js, and Python. Every driver reported `233016` data buckets, `1023` field buckets, `134217728` max-size bytes, 134217728-byte output, compact=true, compression flags=0, and stock `journalctl --verify --file` passed.
  - systemd C: 100000 rows, 3.117248526 append seconds, 32079.572 rows/sec, raw-payload API mode.
  - Rust SDK: 100000 rows, 43.123063825 append seconds, 2318.945 rows/sec, raw-payload API mode, 0.072x systemd C.
  - Go SDK: 100000 rows, 43.509241408 append seconds, 2298.362 rows/sec, field-api mode, 0.072x systemd C.
  - Node.js SDK: 100000 rows, 104.618315844 append seconds, 955.856 rows/sec, field-api mode, 0.030x systemd C.
  - Python SDK: 100000 rows, 154.153818954 append seconds, 648.703 rows/sec, field-api mode, 0.020x systemd C.
  - Process-time evidence: systemd process time was 5.05s user and 0.78s system; Rust was 14.04s user and 47.83s system; Go was 15.67s user and 67.97s system; Node.js was 137.90s user and 90.97s system; Python was 210.59s user and 78.08s system.
  - Working theory, not yet proven: Rust and Go are likely dominated by many small file/window/syscall operations during object/hash-table lookup and updates. Node.js and Python additionally show heavy language-level/user CPU. This must be confirmed with profiling before optimization.
- Recorded writer optimization requirements from the 2026-05-27 performance analysis:
  - Production writers assume exclusive writer ownership after open, trust in-memory writer state, avoid hot-path file metadata syscalls, plan file extension, preserve lockless reader safety, minimize write amplification, and flush rarely.
  - The Rust SDK has a concrete `metadata()?.len()` hot-path regression versus the vendored Rust implementation; this must be repaired and remeasured before broad Rust refactors.
  - Go needs a fast caller-owned-data API plus mmap-backed or equivalently mapped writer core; the existing field API remains as a compatibility wrapper.
  - Node.js and Python need allocation/syscall/readback reductions, but this SOW must record whether pure-runtime constraints make them non-hot-path SDKs.
  - The correct publication model is systemd's actual model: target objects before references, entry reachability after object/index publication, and explicit ordering before live-reader-visible entry links.
- Repaired the Rust writer-owned mmap hot path enough to remove per-access
  file-size metadata calls from the writer path:
  - `rust/src/crates/journal-core/src/file/mmap.rs` now separates live-reader
    bounds from writer-owned bounds. Writer-owned windows cache file size and
    extend only when a requested writer window exceeds the cached size.
  - `rust/src/crates/journal-core/src/file/file.rs` now opens writer paths with
    `WindowManager::new_writer_owned`.
  - Validation: `cargo test -p journal-core` passed with 44 unit tests and 4
    doctests, including mmap consistency, compact writer growth, large hash
    table sizing, compression, and sealed writer stock-verify tests.
- Repaired the Go writer hot path around a mapped arena and bounded writer-owned
  caches:
  - Added `go/journal/mmap_unix.go` for a MAP_SHARED writer arena and
    `go/journal/mmap_other.go` as a non-Unix file-backed fallback for the arena
    abstraction.
  - `go/journal/writer.go` now routes writer-owned reads/writes through the
    mapped arena, reuses entry and payload scratch buffers, caches recent DATA
    and FIELD lookups, and avoids the prior small `pread`/`pwrite` writer-core
    path.
  - `go/journal/seal.go` now reads seal HMAC material through the writer arena
    abstraction instead of direct file reads.
  - Added the systemd-compatible post-change notification after entry
    publication by issuing same-size `ftruncate`. This is required because
    MAP_SHARED memory writes do not notify stock follow readers on their own.
    Validation caught the failure before this fix: stock follow readers saw
    only partial live output while poll/libsystemd readers completed.
  - Added hash-chain-depth update when appending to a non-empty hash bucket.
    This repaired the Go header regression where cached lookups could leave
    `data_hash_chain_depth` stale.
- Go writer-core fixed-128 MiB compact/no-compression/no-FSS benchmark after
  optimization:
  - Command: `timeout 1800 python3 tests/benchmarks/run_writer_core_benchmarks.py --languages systemd go --rows 100000 --repetitions 3 --warmups 1 --format compact --final-state online --keep-journals`.
  - Report: `.local/benchmarks/writer-core/compact-none-fss-off-20260527T173925Z/report.json`.
  - systemd C median append rate: `27947.902006207` rows/sec.
  - Go median append rate: `40738.31131758062` rows/sec.
  - Go/systemd median append ratio: `1.457651859110318`.
  - Both drivers used `134217728` max-size bytes, `233016` data buckets,
    `1023` field buckets, compact format, no compression, no FSS, and
    134217728-byte journals.
  - Environment limitation: the workstation CPU governor was still `powersave`,
    so the absolute rate is directional. The same aligned benchmark earlier in
    this chunk also measured Go around `49818.39208331786` rows/sec versus
    systemd around `36203.094714318` rows/sec. Ratios stayed above systemd in
    both runs.
- Rust/Go writer-core fixed-128 MiB compact/no-compression/no-FSS benchmark
  after the Rust writer-owned mmap repair and the Go mapped-arena repair:
  - Command: `timeout 1800 python3 tests/benchmarks/run_writer_core_benchmarks.py --languages systemd rust go --rows 100000 --repetitions 3 --warmups 1 --format compact --final-state online --keep-journals`.
  - Report: `.local/benchmarks/writer-core/compact-none-fss-off-20260527T180437Z/report.json`.
  - systemd C median append rate: `36058.362815549` rows/sec.
  - Rust median append rate: `56225.90863893713` rows/sec.
  - Go median append rate: `47773.01926391824` rows/sec.
  - Rust/systemd median append ratio: `1.5593028703646947`.
  - Go/systemd median append ratio: `1.3248804308807298`.
  - All three drivers used `134217728` max-size bytes, `233016` data buckets,
    `1023` field buckets, compact format, no compression, no FSS, and
    stock `journalctl --verify --file` passed.
  - Interpretation: the first Rust/Go writer-core optimization chunk changed
    both Rust and Go from materially slower than systemd to faster than systemd
    on the fixed-128 MiB single-file compact baseline. This does not close the
    SOW because Netdata-shaped writer benchmarks, remaining language writer
    profiling, and reader benchmarks are still required.
- Go syscall profile after mmap optimization:
  - Command: `timeout 1800 strace -f -c .local/benchmarks/bin/go-writer-core-bench --rows 1000 --output .local/benchmarks/profiles/go-strace-after-mmap.journal --format compact --final-state online --max-size-bytes 134217728`.
  - Result: no `pread64` or `pwrite64` calls remained in the 1000-row writer
    run. The profile showed `1002` `ftruncate` calls, one `msync`, two `fsync`,
    and stock `journalctl --verify --file` passed for the generated journal.
  - Interpretation: the previous read/write syscall flood is removed. The
    remaining per-entry syscall is the intentional stock follow-reader
    notification.
- Go writer compatibility matrices after mmap optimization:
  - `tests/interoperability/run_live_matrix.py --writers go --entries 30 --writer-delay-ms 20 --poll-readers 2 --libsystemd-readers 1 --keep-files` passed `9/9` feature variants. Result file: `.local/interoperability/live-feature-matrix-results-20260527-204101.json`.
  - `tests/interoperability/run_compression_matrix.py --writers go --compression zstd xz lz4 --keep-files` passed `54/54`. Result file: `.local/interoperability/compression-matrix-results-20260527-204109.json`.
  - `tests/interoperability/run_compact_matrix.py --writers go --compression none --keep-files` passed `14/14`. Result file: `.local/interoperability/compact-matrix-none-results-20260527-204121.json`.
  - `tests/interoperability/run_compact_matrix.py --writers go --compression zstd --keep-files` passed `14/14`. Result file: `.local/interoperability/compact-matrix-zstd-results-20260527-204122.json`.
  - `tests/interoperability/run_compact_matrix.py --writers go --compression xz --keep-files` passed `14/14`. Result file: `.local/interoperability/compact-matrix-xz-results-20260527-204123.json`.
  - `tests/interoperability/run_compact_matrix.py --writers go --compression lz4 --keep-files` passed `14/14`. Result file: `.local/interoperability/compact-matrix-lz4-results-20260527-204124.json`.
- Repaired a validation-harness regression exposed by byte-identity:
  - Root cause: the systemd deterministic ingester defaulted to 64 MiB
    `--max-size-bytes`, while SDK ingesters defaulted to the production 128 MiB
    value from SOW-0035. This changed the number of DATA hash buckets and split
    the deliberate collision corpus, so some SDK outputs had
    `data_hash_chain_depth=1` instead of the expected `3`.
  - Fix: dataset ingesters for Rust, Go, Node.js, and Python now accept
    `--max-size-bytes`; the shared ingester runner passes an explicit 64 MiB
    default for deterministic byte-identity; `run_byte_identity.py` records that
    explicit value.
  - This does not change the SDK production default or the writer-core
    benchmark default, both of which remain 128 MiB unless explicitly
    overridden.
  - Validation: `timeout 1800 python3 tests/interoperability/run_byte_identity.py --final-state all` passed with `all_equal: true` for online, offline, and archived states. All languages and systemd reported `data_hash_chain_depth=3`.
- Fixed the compression and compact matrix runners to include
  `.local/python-deps` on `PYTHONPATH` for all subprocesses. This repaired a
  false Python LZ4 reader failure in the matrix harness without changing SDK
  runtime behavior.
- Broadened compatibility validation after the Rust and Go writer changes:
  - `timeout 1800 python3 tests/interoperability/run_live_matrix.py --writers rust go node python --entries 30 --writer-delay-ms 20 --poll-readers 2 --libsystemd-readers 1 --keep-files` passed `36/36` feature variants. Result file: `.local/interoperability/live-feature-matrix-results-20260527-210650.json`.
  - `timeout 1800 python3 tests/interoperability/run_compression_matrix.py --writers rust go node python --compression zstd xz lz4 --keep-files` passed `216/216`. Result file: `.local/interoperability/compression-matrix-results-20260527-210753.json`.
  - `timeout 1800 python3 tests/interoperability/run_compact_matrix.py --writers rust go node python --compression none --keep-files` passed `56/56`. Result file: `.local/interoperability/compact-matrix-none-results-20260527-210749.json`.
  - `timeout 1800 python3 tests/interoperability/run_compact_matrix.py --writers rust go node python --compression zstd --keep-files` passed `56/56`. Result file: `.local/interoperability/compact-matrix-zstd-results-20260527-210751.json`.
  - `timeout 1800 python3 tests/interoperability/run_compact_matrix.py --writers rust go node python --compression xz --keep-files` passed `56/56`. Result file: `.local/interoperability/compact-matrix-xz-results-20260527-210753.json`.
  - `timeout 1800 python3 tests/interoperability/run_compact_matrix.py --writers rust go node python --compression lz4 --keep-files` passed `56/56`. Result file: `.local/interoperability/compact-matrix-lz4-results-20260527-210754.json`.
- First reviewer pass for the Rust/Go writer optimization and harness repair
  chunk:
  - `llm-netdata-cloud/glm-5.1`: production-grade. Non-blocking note: the Go
    `postChangeFence` looked like dead code without a documented publication
    purpose.
  - `llm-netdata-cloud/kimi-k2.6`: production-grade. Non-blocking concern:
    the Go same-size `ftruncate` notification path depends on syscall ordering
    and should be documented/validated on weakly ordered platforms when such a
    runtime environment is available.
  - `llm-netdata-cloud/qwen3.6-plus`: production-grade. Non-blocking note:
    Rust `WindowManager::sync` had a redundant second `set_len` after clearing
    windows.
  - `llm-netdata-cloud/minimax-m2.7-coder`: production-grade with no blocking
    findings.
  - Disposition: added an explicit comment to the Go post-change fence, removed
    the redundant Rust `set_len`, and ran a Go linux/arm64 build-only check.
    No ARM64 runtime live-matrix validation is available on this workstation;
    this remains a validation limitation for future hardware, not a blocker for
    the current x86_64 chunk because reviewers still marked the chunk
    production-grade.
- Repeat reviewer pass after the non-blocking fixes:
  - `llm-netdata-cloud/glm-5.1`: production-grade. Non-blocking notes:
    redundant initial Go truncate before mapping, O(chain_length) Go
    hash-depth recalculation for non-empty buckets, `int(hash)` cache slot
    truncation only on hypothetical 32-bit targets, non-Unix fallback
    performance, and the ARM64 live-runtime validation gap.
  - `llm-netdata-cloud/kimi-k2.6`: production-grade. Non-blocking notes:
    Go `updateHashChainDepth` is O(chain_length), `mmap_other.go` could mirror
    the Unix same-size `remap` short-circuit, and same-size `ftruncate`
    notification is a Linux/filesystem assumption to keep documented.
  - `llm-netdata-cloud/qwen3.6-plus`: production-grade. Non-blocking notes:
    per-append `ftruncate` is the remaining notification syscall, small buffer
    allocations remain in helper paths, header metadata writes could be
    batched, `updateHashChainDepth` is O(chain_length), `postChangeFence` is
    write-only by design, and bounds checks are deliberately defensive.
  - `llm-netdata-cloud/minimax-m2.7-coder`: production-grade. Non-blocking
    notes: `postChangeFence` is write-only and ARM64 runtime validation remains
    unavailable.
  - Disposition: no blocking findings. The Go post-change atomic is retained
    for this chunk because Go has no standalone public fence operation; the
    sequentially consistent atomic RMW is the explicit compiler/CPU ordering
    point before the same-size `ftruncate` notification syscall. The
    hash-depth walk, helper allocation cleanup, metadata batching, and
    non-Unix fallback refinements remain valid future profiling/cleanup targets
    inside SOW-0009, but they are not blockers for this intermediate commit.

## Validation

Current chunk validation:

- `go test ./...` passed.
- `node node/test/all.js` passed.
- `CARGO_HOME=../.local/cargo-home CARGO_TARGET_DIR=../.local/cargo-target cargo check -p writer_core_bench` passed.
- `CARGO_HOME=../.local/cargo-home CARGO_TARGET_DIR=../.local/cargo-target cargo test -p journal-core arena -- --nocapture` passed; it ran `compact_writer_grows_arena_past_initial_allocation` and `writer_initial_arena_covers_large_hash_tables`.
- `PYTHONPATH=python python3 - <<'PY' ... test_all.test_compact_writer_grows_arena_past_initial_allocation(); test_all.test_writer_initial_arena_covers_large_hash_tables() ... PY` passed.
- `PYTHONPATH=python python3 python/test_all.py` did not complete because the current Python environment lacks `lz4.block`; failure was `ModuleNotFoundError: No module named 'lz4'` in the existing lz4 compression test path.
- Writer-core 1000-row compact/no-compression/no-FSS matrix passed for systemd, Rust, Go, Node.js, and Python with stock `journalctl --verify --file`.
- Writer-core 200000-row compact/no-compression/no-FSS diagnostic run passed
  at 2 GiB max-size for systemd, Rust, Go, Node.js, and Python with stock
  `journalctl --verify --file`; it is not production baseline evidence.
- Writer-core 100000-row compact/no-compression/no-FSS fixed-128 MiB baseline
  passed for systemd, Rust, Go, Node.js, and Python with stock
  `journalctl --verify --file`.
- Writer-core 200000-row compact/no-compression/no-FSS fixed-128 MiB single-file
  run was rejected as an invalid baseline because stock systemd correctly
  stopped at the configured max-size before 200000 rows.
- `.agents/sow/audit.sh` passed.
- `GOCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-cache GOMODCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-mod-cache go test ./... -count=1` passed after the Go mmap optimization and dataset ingester max-size plumbing.
- `CARGO_HOME=/home/costa/Documents/systemd-journal-sdk/.local/cargo-home CARGO_TARGET_DIR=/home/costa/Documents/systemd-journal-sdk/.local/cargo-target cargo check -p dataset_ingester` passed.
- `CARGO_HOME=/home/costa/Documents/systemd-journal-sdk/.local/cargo-home CARGO_TARGET_DIR=/home/costa/Documents/systemd-journal-sdk/.local/cargo-target cargo test -p journal-core` passed.
- `node --check node/cmd/dataset_ingester.js` passed.
- `node node/test/all.js` passed.
- `PYTHONPATH=/home/costa/Documents/systemd-journal-sdk/.local/python-deps:/home/costa/Documents/systemd-journal-sdk/python python3 python/test_all.py` passed.
- `python3 -m py_compile python/cmd/dataset_ingester.py tests/datasets/ingesters/run_dataset_ingesters.py tests/interoperability/run_byte_identity.py tests/interoperability/run_compact_matrix.py tests/interoperability/run_compression_matrix.py` passed.
- `timeout 1800 python3 tests/datasets/ingesters/run_dataset_ingesters.py --language go --both --final-state online` passed: 349 accepted records, 9 rejection records, and stock `journalctl --verify --file` passed.
- `timeout 1800 python3 tests/interoperability/run_byte_identity.py --final-state all` passed with byte-for-byte equality across systemd, Rust, Go, Node.js, and Python for online, offline, and archived states.
- `timeout 1800 python3 tests/interoperability/run_live_matrix.py --writers go --entries 30 --writer-delay-ms 20 --poll-readers 2 --libsystemd-readers 1 --keep-files` passed `9/9`.
- `timeout 1800 python3 tests/interoperability/run_compression_matrix.py --writers go --compression zstd xz lz4 --keep-files` passed `54/54`.
- `timeout 1800 python3 tests/interoperability/run_compact_matrix.py --writers go --compression none --keep-files` passed `14/14`.
- `timeout 1800 python3 tests/interoperability/run_compact_matrix.py --writers go --compression zstd --keep-files` passed `14/14`.
- `timeout 1800 python3 tests/interoperability/run_compact_matrix.py --writers go --compression xz --keep-files` passed `14/14`.
- `timeout 1800 python3 tests/interoperability/run_compact_matrix.py --writers go --compression lz4 --keep-files` passed `14/14`.
- `timeout 1800 strace -f -c .local/benchmarks/bin/go-writer-core-bench --rows 1000 --output .local/benchmarks/profiles/go-strace-after-mmap.journal --format compact --final-state online --max-size-bytes 134217728` completed and showed no `pread64`/`pwrite64` calls in the Go writer-core path; stock `journalctl --verify --file` passed on the generated journal.
- `GOCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-cache GOMODCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-mod-cache GOOS=linux GOARCH=arm64 go test -c ./journal -o /tmp/systemd-journal-sdk-go-journal-arm64.test` passed as a build-only ARM64 check for the Go journal package.
- `timeout 1800 python3 tests/benchmarks/run_writer_core_benchmarks.py --languages systemd rust go --rows 100000 --repetitions 3 --warmups 1 --format compact --final-state online --keep-journals` passed and produced `.local/benchmarks/writer-core/compact-none-fss-off-20260527T180437Z/report.json`; systemd median append rate was `36058.362815549` rows/sec, Rust was `56225.90863893713` rows/sec, and Go was `47773.01926391824` rows/sec.
- `timeout 1800 python3 tests/interoperability/run_live_matrix.py --writers rust go node python --entries 30 --writer-delay-ms 20 --poll-readers 2 --libsystemd-readers 1 --keep-files` passed `36/36`.
- `timeout 1800 python3 tests/interoperability/run_compression_matrix.py --writers rust go node python --compression zstd xz lz4 --keep-files` passed `216/216`.
- `timeout 1800 python3 tests/interoperability/run_compact_matrix.py --writers rust go node python --compression none --keep-files` passed `56/56`.
- `timeout 1800 python3 tests/interoperability/run_compact_matrix.py --writers rust go node python --compression zstd --keep-files` passed `56/56`.
- `timeout 1800 python3 tests/interoperability/run_compact_matrix.py --writers rust go node python --compression xz --keep-files` passed `56/56`.
- `timeout 1800 python3 tests/interoperability/run_compact_matrix.py --writers rust go node python --compression lz4 --keep-files` passed `56/56`.
- Repeat external reviewer pass for this chunk returned production-grade from
  `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/kimi-k2.6`,
  `llm-netdata-cloud/qwen3.6-plus`, and
  `llm-netdata-cloud/minimax-m2.7-coder`; only non-blocking optimization and
  portability watch-points were reported.
- `git diff --check` passed.
- `.agents/sow/audit.sh` passed after the validation evidence update.

Pending validation before this SOW can close:

- Full benchmark/profiling iterations for the slow writer paths.
- Reader benchmark harness and baselines for single-file and ordered multi-file/directory readers.
- Post-optimization conformance, interoperability, live compatibility, and byte-compatibility reruns.

## Outcome

Pending.

## Lessons Extracted

Pending active implementation.

## Followup

Pending active implementation.

## Regression Log

None yet.
