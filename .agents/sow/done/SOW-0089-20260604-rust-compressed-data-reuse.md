# SOW-0089 - Rust Compressed DATA Reuse

## Status

Status: completed

Sub-state: completed after measurement, local validation, whole-SOW reviewer
pass, and closeout audit.

## Requirements

### Purpose

Measure and, only if beneficial, implement Rust reader reuse for compressed
DATA decompression results and native Zstandard decompressor context state.

### User Request

The user requires reusable journal DATA objects not to be repeatedly parsed,
decompressed, hashed, sorted, or copied when a cache can preserve the same
result without weakening correctness.

### Acceptance Criteria

- Real-corpus compressed DATA reuse frequency is measured.
- Native Zstandard context creation cost is profiled.
- A bounded row/query/file cache is implemented only if measured benefit exceeds
  lookup and memory cost.
- If caching is rejected, the SOW records benchmark evidence and leaves no
  speculative cache.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- SOW-0086 shows native Zstandard decompression fixed the measured red-flag
  case, but repeated compressed DATA offsets and decompressor context creation
  may still waste CPU.
- SOW-0088 improved offset-array cursor movement, so remaining compressed
  payload and facade deltas should be investigated in the DATA access and
  decompression paths rather than row cursor movement.
- The required model is evidence-first: measure compressed DATA reuse and
  decompressor cost before accepting any cache. A speculative cache that adds
  lookup overhead without real benefit is a performance regression.

Evidence reviewed:

- SOW-0086 benchmark and perf findings.
- SOW-0088 final large-file benchmark deltas.
- `.agents/sow/specs/rust-reader-performance.md`: compressed DATA arena and
  reusable DATA object rules.
- `rust/src/crates/journal-core/src/file/row_view.rs`: current row view
  decompresses compressed DATA into row storage when row payload APIs need
  plain bytes.
- `rust/src/crates/journal-core/src/file/file_payload.rs`: payload visitor and
  lookup paths decompress compressed DATA through a caller-provided scratch
  buffer.
- `rust/src/crates/journal-core/src/file/object_compression.rs`: Zstandard
  decompression uses the native `zstd` path first with `ruzstd` fallback.

Affected contracts and surfaces:

- Rust compressed DATA reader paths.
- Rust row-level payload lifetime guarantees.
- Rust facade DATA enumeration.
- Rust SDK payload visitor.
- Rust lookup/filter paths that compare against compressed DATA.

Existing patterns to reuse:

- SOW-0086 native Zstandard fast path and compressed current-row arena.
- SOW-0086 benchmark harness and real-corpus candidate labels.
- Current row-view ownership: uncompressed payloads remain mmap-backed; only
  compressed payloads may allocate/copy into row storage.

Risk and blast radius:

- Medium: cache lookup overhead can make performance worse if hit rate or
  payload size does not justify it.
- Medium: a query/file cache can accidentally weaken row-level lifetime rules
  if returned slices outlive the row arena or are invalidated while exposed.
- Medium: unbounded caching can make large high-cardinality compressed files
  consume impractical memory.

Sensitive data handling plan:

- Use aggregate reuse counts and benchmark rates only; do not record raw
  payloads.
- Durable artifacts must use sanitized candidate labels, counts, rates, and
  cache hit/miss totals only.

Implementation plan:

1. Add a repo-local measurement helper or benchmark mode that reports
   compressed DATA reuse by offset, compressed algorithm, row references,
   unique compressed DATA count, hit ratio for repeated offsets, and total
   compressed/decompressed bytes.
2. Measure Zstandard decompression cost and context/setup cost on the same
   real compressed candidates.
3. Prototype only bounded strategies that preserve row-level slice lifetime.
4. Keep only strategies that improve the real compressed candidates without
   regressing uncompressed candidates beyond measurement noise.
5. If no strategy wins, remove the prototype and close the SOW with evidence
   that no cache belongs in the hot path.

Validation plan:

- Rust tests, reader benchmark matrix, compressed real-corpus profiles,
  `git diff --check`, SOW audit, and whole-SOW read-only reviewer pass.
- Benchmark evidence must report before/after deltas in the standard table
  format, with compressed candidates separated from uncompressed control
  candidates.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected.
- Specs: update Rust reader performance spec with accepted cache semantics if
  implemented.
- End-user/operator docs: likely unaffected.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: normal pending/current/done flow.
- SOW-status.md: update when state changes.

Open-source reference evidence:

- Not required for the first measurement pass; this SOW concerns current Rust
  SDK hot-path cost and journal DATA reuse behavior measured from files.

Open decisions:

- User approved proceeding through SOW-0087 to SOW-0092 on 2026-06-05, with a
  performance improvement review after each SOW and immediate continuation to
  the next SOW.
- No public API change is approved. If a winning strategy requires public API
  changes, stop and ask before implementation.

## Local Validation

Compressed DATA reuse profile:

| candidate | fields | compressed refs | unique compressed | repeat refs | reuse ratio | avoidable decompressed MiB | zstd refs |
|---|---:|---:|---:|---:|---:|---:|---:|
| `real-compressed-multiboot-high-entry` | 7,628,128 | 5 | 5 | 0 | 0.0% | 0.00 | 5 |
| `real-compressed-high-cardinality` | 1,468,608 | 0 | 0 | 0 | 0.0% | 0.00 | 0 |
| `real-compressed-high-field-count` | 604,823 | 775 | 296 | 479 | 61.8% | 0.39 | 775 |
| `netdata-flow-largest-uncompressed` | 4,755,804 | 0 | 0 | 0 | 0.0% | 0.00 | 0 |
| `netdata-flow-most-entries-uncompressed` | 5,090,157 | 0 | 0 | 0 | 0.0% | 0.00 | 0 |

Interpretation:

- The local real compressed-capable corpus has almost no compressed DATA on
  the high-entry and high-cardinality candidates.
- The only measured reusable compressed DATA candidate has 775 compressed DATA
  references, 296 unique compressed offsets, and only 0.39 MiB of avoidable
  decompressed bytes if every unique compressed object were cached.
- A decompressed DATA cache is therefore rejected for this SOW. It would add
  lookup cost and memory policy complexity to the row hot path without enough
  real-corpus reuse to justify it.

Zstandard context evidence:

- `Cargo.lock` uses `zstd` 0.13.3 and `zstd-safe` 7.2.4.
- The current SDK native fast path uses `zstd_safe::decompress`, the one-shot
  wrapper around `ZSTD_decompress`.
- The dependency source also exposes `DCtx::decompress`, which wraps
  `ZSTD_decompressDCtx`.
- A reusable `DCtx` prototype was implemented inside the Rust row-view
  decompression path, tested, benchmarked, and then removed because the real
  compressed DATA profile did not provide enough decompression work to justify
  keeping it.

Final standard benchmark, compared with the SOW-0088 report:

| candidate | mode | SOW-0088 rows/s | SOW-0089 rows/s | delta | SOW-0089 fields/s |
|---|---|---:|---:|---:|---:|
| `real-compressed-multiboot-high-entry` | `core-payloads` | 1,825,414 | 1,754,319 | -3.9% | 43,862,601 |
| `real-compressed-multiboot-high-entry` | `sdk-payloads` | 3,179,854 | 2,992,329 | -5.9% | 74,816,092 |
| `real-compressed-multiboot-high-entry` | `facade-data` | 3,049,669 | 2,869,075 | -5.9% | 71,734,416 |
| `real-compressed-high-cardinality` | `core-payloads` | 2,460,434 | 2,412,040 | -2.0% | 38,692,548 |
| `real-compressed-high-cardinality` | `sdk-payloads` | 4,444,896 | 4,250,795 | -4.4% | 68,188,789 |
| `real-compressed-high-cardinality` | `facade-data` | 3,776,465 | 3,484,704 | -7.7% | 55,899,598 |
| `real-compressed-high-field-count` | `core-payloads` | 683,722 | 659,684 | -3.5% | 17,433,124 |
| `real-compressed-high-field-count` | `sdk-payloads` | 806,836 | 800,630 | -0.8% | 21,157,839 |
| `real-compressed-high-field-count` | `facade-data` | 742,052 | 688,144 | -7.3% | 18,185,223 |
| `netdata-flow-largest-uncompressed` | `core-payloads` | 895,616 | 845,479 | -5.6% | 35,101,364 |
| `netdata-flow-largest-uncompressed` | `sdk-payloads` | 1,392,532 | 1,435,182 | +3.1% | 59,583,798 |
| `netdata-flow-largest-uncompressed` | `facade-data` | 1,317,856 | 1,269,973 | -3.6% | 52,724,917 |
| `netdata-flow-most-entries-uncompressed` | `core-payloads` | 920,561 | 868,458 | -5.7% | 36,043,482 |
| `netdata-flow-most-entries-uncompressed` | `sdk-payloads` | 1,493,239 | 1,343,025 | -10.1% | 55,739,367 |
| `netdata-flow-most-entries-uncompressed` | `facade-data` | 1,262,426 | 1,314,840 | +4.2% | 54,569,602 |
| `netdata-flow-online-uncompressed` | `core-payloads` | 903,143 | 931,983 | +3.2% | 38,665,532 |
| `netdata-flow-online-uncompressed` | `sdk-payloads` | 1,494,485 | 1,548,942 | +3.6% | 64,261,537 |
| `netdata-flow-online-uncompressed` | `facade-data` | 1,216,542 | 1,219,732 | +0.3% | 50,603,494 |

Benchmark interpretation:

- Final production reader paths are unchanged by this SOW. The table therefore
  reflects benchmark noise around the SOW-0088 code, plus the new measurement
  mode being available for future checks.
- No positive SOW-0089 runtime speedup is claimed.
- No production compressed-DATA cache was kept.

Validation commands:

- `cargo test -p journal-core -p journal --target-dir ../.local/cargo-target`
  passed.
- `cargo build --release -p reader_core_bench --target-dir
  ../.local/cargo-target` passed.
- `.local/sow-0086/reader-baseline/run_baseline.py` completed after the
  prototype was removed and wrote the final standard benchmark report.
- `.local/sow-0089/compressed-reuse-stats.json` was generated from the final
  benchmark binary.

Sensitive data gate:

- Durable SOW evidence uses sanitized candidate labels, counts, rates, and
  aggregate compressed DATA statistics only.
- Raw local journal paths remain only in `.local/` benchmark reports.

Artifact maintenance gate:

- `AGENTS.md`: no change needed; no project-wide workflow or guardrail changed.
- Runtime project skills: no change needed; no agent workflow changed.
- Specs: no change needed; the existing Rust reader performance spec already
  says a reusable decompression cache may be added only after evidence proves
  lookup and memory costs are lower than repeated decompression.
- End-user/operator docs: no change needed; no public SDK behavior changed.
- End-user/operator skills: no change needed; no operator workflow changed.
- SOW lifecycle: SOW moved from `pending/` to `current/`; closeout will move it
  to `done/` after reviewer and audit gates.
- `.agents/sow/SOW-status.md`: updated for active SOW-0089.

Same-failure search:

- `git diff` shows no production reader path changes after the rejected
  prototype was removed.
- The remaining code change is isolated to
  `rust/src/internal/testcmd/reader_core_bench/src/main.rs`.

Reviewer findings:

- `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  - Verified production reader paths were unchanged.
  - Verified no `DCtx`, decompressed-cache, or prototype remnants remained.
  - Confirmed cache rejection was supported by 0.39 MiB maximum avoidable
    decompressed bytes in the measured corpus.
  - Non-blocking observations were limited to internal benchmark-tool style:
    `record_compressed()` can return `()`, `refs` is initialized at insertion,
    and all modes now emit an empty `extra` JSON object.
- `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  - Verified zero production diff in `journal-core` and `journal`.
  - Confirmed the new mode reports aggregate statistics only and does not
    expose raw payload bytes.
  - Confirmed benchmark deltas should be interpreted as noise because
    production paths are unchanged.
- `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  - Verified `record_core_compressed_stats()` walks entry DATA offsets,
    classifies compressed/uncompressed DATA, and only decompresses compressed
    DATA to measure decompressed length.
  - Confirmed no production SDK, facade, writer, or cross-language path was
    changed.
  - Non-blocking observations were limited to dispatch-arm duplication,
    algorithm bools in an internal struct, and creating an empty stats object
    for all core modes.
- `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
  - Verified the only code change is in the internal benchmark tool.
  - Confirmed raw payloads are not serialized in the measurement output.
  - Confirmed the evidence supports rejecting a production cache for this
    corpus.
- `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
  - Verified the current working-tree diff, not older SOW commits.
  - Confirmed `git diff --stat -- rust/src/crates/journal-core/ rust/src/journal/`
    is empty.
  - Confirmed no `DCtx`, decompression-cache, or prototype references remain
    anywhere under `rust/`.
  - Confirmed `.local/sow-0089/compressed-reuse-stats.json` contains only
    sanitized labels and aggregate statistics.

Disposition:

- No blocking reviewer findings were raised.
- Non-blocking style observations were left unchanged because the affected code
  is an internal measurement tool, the current shape keeps the implementation
  simple, and no production path is affected.

Follow-up mapping:

- No follow-up SOW is needed for compressed DATA reuse from this evidence.
- If future real corpus or generated fixtures show high repeated compressed
  DATA volume, reopen this decision with fresh benchmark evidence before adding
  a cache.

## Outcome

Completed.

- Added `core-compressed-stats` to the internal Rust reader benchmark tool.
- Measured compressed DATA reuse on the available large-file corpus.
- Rejected a production decompressed-DATA cache and reusable Zstandard context
  because measured avoidable repeated decompression was too small to justify
  cache lookup and memory-policy cost.
- Left production Rust reader paths unchanged.
- Local validation passed:
  - `cargo test -p journal-core -p journal --target-dir ../.local/cargo-target`
  - `cargo build --release -p reader_core_bench --target-dir ../.local/cargo-target`
  - `.local/sow-0086/reader-baseline/run_baseline.py`
  - `git diff --check`
  - `.agents/sow/audit.sh`
- Five whole-SOW read-only reviewers voted `PRODUCTION GRADE`.
