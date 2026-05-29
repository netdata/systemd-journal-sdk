# SOW-0058 - Rust DATA Header Fast Path

## Status

Status: completed

Sub-state: completed. Rust DATA payload hot paths now parse only the 16-byte
object header before validation and payload slicing. Correctness tests pass,
read-only reviewers found no blockers, and benchmark evidence shows mixed
results that do not conclusively explain the Go/Rust reader gap.

## Requirements

### Purpose

Verify whether Rust reader throughput is affected by parsing the full DATA
object header in hot payload enumeration paths, while preserving journal format
compatibility and the current Rust reader API.

### User Request

The user asked to test DATA header parsing changes after the Go/Rust reader
comparison showed Go parses only the 16-byte object header for DATA payload
reads while Rust parses the full DATA object header.

### Assistant Understanding

Facts:

- Rust payload scans currently parse `DataObjectHeader`, which is 64 bytes, in
  both `visit_data_payload_at_with_context()` and
  `data_payload_object_info_at()`.
- Go payload scans parse only the 16-byte `objectHeader`, then validate object
  type, object size, and the compact/regular DATA payload prefix before slicing
  the payload.
- The DATA payload prefix remains 64 bytes for regular files and 72 bytes for
  compact files; the optimization must not change payload slicing or corruption
  validation.

Inferences:

- Rust can avoid parsing DATA hash-chain and entry-list fields in the payload
  enumeration hot path because that path only needs object type, compression
  flags, object size, and the format-specific payload prefix size.
- Compressed DATA still needs full object decoding after the object slice is
  available, because decompression is implemented through `DataObject`.

Unknowns:

- Whether this isolated parse reduction measurably changes Rust reader
  throughput on the compact/offline benchmark corpus.

### Acceptance Criteria

- Rust DATA payload hot paths parse only `ObjectHeader` before object type,
  size, bounds, compression, and prefix validation.
- Compact and regular DATA payload slicing remains unchanged.
- Compressed DATA payload reads still pass existing decompression tests.
- Rust package tests for affected crates pass.
- Reader-core benchmark results are recorded and compared with the prior Rust
  live/windowed result.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/done/SOW-0052-20260529-rust-reader-last-mile-optimization.md`
- `.agents/sow/done/SOW-0057-20260529-rust-live-whole-file-mmap-reader-option.md`
- `rust/src/crates/journal-core/src/file/object.rs`
- `rust/src/crates/journal-core/src/file/file.rs`
- `go/journal/reader.go`
- `go/journal/format.go`

Current state:

- `ObjectHeader` contains object type, flags, reserved bytes, and size.
- `DataObjectHeader` embeds `ObjectHeader` and adds hash/list linkage fields
  that are not used by the Rust payload visitor or payload-info hot path.
- Rust already computes a separate DATA payload prefix size from the file
  format, so it does not need to parse the DATA-specific tail fields before
  slicing uncompressed payloads.
- Go already uses the 16-byte object header approach for DATA payload reads.

Risks:

- Accepting a corrupt object that is smaller than the full DATA prefix would
  break journal validation; this must remain rejected by the existing
  `size_needed < payload_prefix_size` check.
- Compressed object handling must still use the full DATA object representation
  before decompression.
- Benchmark differences can be within run-to-run noise; results must be
  reported as measured, not overclaimed.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The Rust reader hot path does extra header parsing before returning DATA
  payloads. This may contribute to the measured Rust/Go reader gap, but this is
  a working theory that requires direct measurement.

Evidence reviewed:

- `rust/src/crates/journal-core/src/file/file.rs` has
  `visit_data_payload_at_with_context()` and `data_payload_object_info_at()`
  reading `size_of::<DataObjectHeader>()` before parsing object type, flags, and
  size.
- `rust/src/crates/journal-core/src/file/object.rs` defines `ObjectHeader` as
  16 bytes and `DataObjectHeader` as the larger DATA-specific header.
- `go/journal/reader.go` reads `objectHeaderSize` and uses `parseObjectHeader()`
  before slicing DATA payloads.
- `.agents/sow/done/SOW-0057-20260529-rust-live-whole-file-mmap-reader-option.md`
  recorded the prior compact/offline Rust benchmark result used as comparison
  baseline.

Affected contracts and surfaces:

- Rust `journal-core` internal payload enumeration.
- Rust SDK entry/data/facade reader benchmark paths that enumerate DATA
  payloads.
- Journal corruption validation for DATA object type, size, bounds, compact
  prefix, and compression flags.

Existing patterns to reuse:

- Existing `ObjectHeader::validated_size()` and compression flag helpers.
- Existing `DataPayloadReadContext::payload_prefix_size` compact/regular
  calculation.
- Existing compact uncompressed and compressed DATA payload unit tests.
- Existing reader-core benchmark harness.

Risk and blast radius:

- Low to medium. The change is internal and hot-path only, but an incorrect
  size or prefix check could weaken corrupt-file rejection. Shared readers and
  public APIs should remain unchanged.

Sensitive data handling plan:

- Use generated fixtures and benchmark summaries only.
- Do not record real logs, SNMP communities, credentials, bearer tokens,
  customer data, personal data, private endpoints, or production incident
  details.

Implementation plan:

1. Add an internal Rust helper that reads and validates only `ObjectHeader` for
   DATA payload paths.
2. Replace full `DataObjectHeader` pre-parse in payload visitor and payload-info
   paths with the helper.
3. Keep compressed DATA decompression through `DataObject::from_data()`.
4. Run Rust tests and benchmark the same compact/offline Rust reader matrix used
   in SOW-0057.

Validation plan:

- `cargo fmt --manifest-path rust/Cargo.toml --all`
- targeted Rust DATA payload tests
- `cargo test --manifest-path rust/Cargo.toml -p journal-core`
- `cargo test --manifest-path rust/Cargo.toml -p journal`
- Rust-only reader-core benchmark with 100k compact/offline rows and fixed
  128 MiB max-size settings.
- `git diff --check`
- `.agents/sow/audit.sh`

Artifact impact plan:

- AGENTS.md: no update expected; project workflow does not change.
- Runtime project skills: no update expected; compatibility workflow does not
  change.
- Specs: no public behavior change expected; update only if the measured option
  changes a documented contract.
- End-user/operator docs: no update expected; internal optimization only.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: close after tests, benchmark evidence, review disposition, and
  audit pass.
- SOW-status.md: update when this SOW opens and closes.

Open-source reference evidence:

- No external open-source checkout was needed. This is an SDK-local hot-path
  measurement using the already imported journal format structures and existing
  benchmark harness.

Open decisions:

- None. The user requested the measurement.

## Implications And Decisions

1. 2026-05-29 DATA header parse measurement
   - Decision: implement the Rust object-header-only DATA payload parse and
     benchmark it without changing public API or default reader behavior.
   - Implication: if the result is faster, Rust can keep the internal
     optimization; if not, the benchmark records that this is not the cause of
     the Rust/Go gap.

## Plan

1. Implement the object-header-only DATA payload helper in Rust.
2. Reuse existing compact and compressed DATA tests, adding targeted coverage
   only if existing tests do not hit the changed paths.
3. Run Rust tests and the Rust-only reader-core benchmark.
4. Record results, run review/audit, and close if clean.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current user routing.

Reviewers:

- Whole-SOW read-only review if the benchmarked code change is kept.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- DO NOT MAKE CHANGES OUTSIDE THIS REPOSITORY FOR ANY REASON.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- If tests fail, fix within this SOW or revert the specific internal change.
- If the benchmark shows no useful gain, record the result and decide whether
  the simplification is still worth keeping based on correctness and reviewer
  feedback.

## Execution Log

### 2026-05-29

- Created SOW after the user requested testing DATA header parsing changes.
- Implemented `parse_data_payload_object_header()` so Rust DATA payload hot
  paths parse only the 16-byte `ObjectHeader` before DATA type, size,
  compression, bounds, and payload-prefix validation.
- Replaced full `DataObjectHeader` pre-parsing in:
  - `JournalFile::visit_data_payload_at_with_context()`
  - `JournalFile::data_payload_object_info_at()`
- Preserved compressed DATA handling through `DataObject::from_data()` after
  the full object slice is available.
- Ran targeted compact and compressed DATA tests, full affected Rust package
  tests, and repeated reader-core benchmarks.
- Benchmark result summary:
  - The isolated DATA header parse change does not conclusively explain the
    Go/Rust reader gap.
  - Single-file `sdk-payloads` and `facade-data` improved in the clean
    baseline/current comparison.
  - Low-level `core-payloads` and open-files medians were flat or lower across
    some runs, showing enough variance that this SOW must not overclaim a
    broad performance win.
- Clean baseline/current-after-baseline comparison, compact/offline 100k rows,
  live/windowed, 128 MiB max-size:

| Surface | Mode | Clean baseline rows/s | Current rows/s | Ratio |
| --- | --- | ---: | ---: | ---: |
| file | core-payloads | 1,301,134 | 1,224,826 | 0.941 |
| file | sdk-entry | 105,436 | 113,568 | 1.077 |
| file | sdk-payloads | 2,251,233 | 2,511,708 | 1.116 |
| file | facade-data | 1,613,530 | 2,209,762 | 1.369 |
| open-files | sdk-entry | 112,688 | 113,741 | 1.009 |
| open-files | sdk-payloads | 2,094,501 | 2,011,444 | 0.960 |
| open-files | facade-data | 2,171,309 | 2,079,130 | 0.958 |

- Earlier comparison against SOW-0057's committed benchmark baseline also mixed
  positive and negative results. The two current-tree repeat runs averaged
  single-file `sdk-payloads` at 2,614,059 rows/s versus SOW-0057's 2,515,113
  rows/s, but averaged open-files `sdk-payloads` at 1,799,911 rows/s versus
  SOW-0057's 1,985,906 rows/s.

## Validation

Acceptance criteria evidence:

- Rust DATA payload hot paths now parse only `ObjectHeader` before validation:
  `rust/src/crates/journal-core/src/file/file.rs`.
- Compact and regular DATA payload slicing still uses
  `DataPayloadReadContext::payload_prefix_size`.
- Compressed DATA still calls `DataObject::from_data()` before decompression.

Tests or equivalent validation:

- `cargo fmt --manifest-path rust/Cargo.toml --all`
- `cargo test --manifest-path rust/Cargo.toml -p journal-core visit_data_payload_at_returns_compact_uncompressed_payload`
- `cargo test --manifest-path rust/Cargo.toml -p journal-core visit_data_payload_at_decompresses_payload`
- `cargo test --manifest-path rust/Cargo.toml -p journal-core`
- `cargo test --manifest-path rust/Cargo.toml -p journal`
- `python3 tests/benchmarks/run_reader_core_benchmarks.py --languages rust --rows 100000 --directory-rows 100000 --repetitions 5 --warmups 1 --format compact --final-state offline --max-size-bytes 134217728 --directory-max-size-bytes 134217728 --window-size 33554432 --out .local/benchmarks/reader-core-rust-data-header-fast-path`
- `python3 tests/benchmarks/run_reader_core_benchmarks.py --languages rust --rows 100000 --directory-rows 100000 --repetitions 5 --warmups 1 --format compact --final-state offline --max-size-bytes 134217728 --directory-max-size-bytes 134217728 --window-size 33554432 --out .local/benchmarks/reader-core-rust-data-header-fast-path-repeat`
- Created clean baseline archive under `.local/bench-ab-0058/base` from `HEAD`
  and ran:
  `python3 tests/benchmarks/run_reader_core_benchmarks.py --languages rust --rows 100000 --directory-rows 100000 --repetitions 5 --warmups 1 --format compact --final-state offline --max-size-bytes 134217728 --directory-max-size-bytes 134217728 --window-size 33554432 --out .local/benchmarks/reader-core-rust-data-header-baseline`
- `python3 tests/benchmarks/run_reader_core_benchmarks.py --languages rust --rows 100000 --directory-rows 100000 --repetitions 5 --warmups 1 --format compact --final-state offline --max-size-bytes 134217728 --directory-max-size-bytes 134217728 --window-size 33554432 --out .local/benchmarks/reader-core-rust-data-header-fast-path-after-baseline`
- `git diff --check`

Real-use evidence:

- The reader-core benchmark used repository-generated compact/offline journal
  fixtures and Rust SDK reader paths for single-file and open-files reads.

Reviewer findings:

- Minimax: PRODUCTION GRADE. It found no correctness, security, lifetime,
  mmap, or compatibility blockers. It raised informational notes that the
  duplicated bounds checks in the two changed functions are a low maintenance
  risk, and that a targeted regular uncompressed DATA test could be added if a
  future SOW touches this path again. Disposition: accepted as non-blocking;
  duplication already existed logically in both paths and the full Rust tests
  cover broader regular reads.
- Kimi: PRODUCTION GRADE. It independently verified object-header-only parsing,
  preserved DATA prefix validation, compressed and compact behavior, mmap
  safety, benchmark honesty, and no unexpected side effects. It recommended
  updating this SOW's reviewer findings before close. Disposition: done.
- Qwen: PRODUCTION GRADE. It found no production blockers and recommended
  updating the SOW outcome from pending before close. It noted the benchmark
  conclusion is conservative and evidence-backed. Disposition: done.

Same-failure scan:

- Searched current Rust DATA payload paths with `rg` for
  `DataObjectHeader`, `visit_data_payload_at`, and
  `data_payload_object_info_at`. Only the two hot-path pre-parses were changed;
  full object readers and writer/verifier object parsing remain unchanged.

Sensitive data gate:

- Durable artifacts contain generated benchmark paths, aggregate throughput
  numbers, and code paths only. No raw secrets, credentials, bearer tokens,
  SNMP communities, customer data, personal data, private endpoints, or
  production logs were recorded.

Artifact maintenance gate:

- AGENTS.md: no update needed; project workflow and guardrails did not change.
- Runtime project skills: no update needed; compatibility workflow did not
  change.
- Specs: no update needed; this is an internal Rust reader optimization and
  does not change public behavior or compatibility contract.
- End-user/operator docs: no update needed; public API and documented behavior
  did not change.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: marked `Status: completed` and moved to `.agents/sow/done/`
  with implementation and artifact updates before commit.
- SOW-status.md: root and project-local status files updated when this SOW
  opened and closed.

Specs update:

- No spec update needed; public behavior did not change.

Project skills update:

- No project skill update needed; this did not change how agents should work in
  the repository.

End-user/operator docs update:

- No end-user/operator docs update needed; this is an internal optimization.

End-user/operator skills update:

- No output/reference skill update needed.

Lessons:

- DATA header pre-parse reduction alone is not enough to explain the Go/Rust
  reader performance difference. The benchmark signal is mixed and needs
  follow-up profiling before attributing remaining gaps.

Follow-up mapping:

- Existing reader-performance work remains under SOW-0009. No new follow-up SOW
  is needed solely from this measurement.

## Outcome

Completed. The Rust DATA payload hot path now parses only the 16-byte
`ObjectHeader` before validation and payload slicing. The change is correct and
kept because it removes unnecessary DATA linkage parsing without changing
public behavior. The benchmark result is mixed and is not accepted as proof that
this optimization explains the remaining Go/Rust reader performance gap.

## Lessons Extracted

- Small hot-path parser reductions can produce noisy benchmark results when
  surrounding iterator, facade, and directory traversal costs dominate.
- Future reader optimization work should continue profiling with file and
  open-files surfaces separated, because a single-file improvement does not
  automatically imply a directory/open-files improvement.

## Followup

None yet.

## Regression Log

None yet.
