# SOW-0086 - Rust Reader Performance Contract And Gap Analysis

## Status

Status: completed

Sub-state: completed after local validation, benchmark evidence, final whole-SOW
review, and SOW audit.

## Requirements

### Purpose

Define and enforce the Rust reader performance contract before any further
reader-performance implementation work. The purpose is top Rust reader
performance for production journal exploration and Netdata-style polling reads;
correctness remains mandatory, but correctness alone is not sufficient.

### User Request

The user explicitly narrowed the scope:

- Ignore all past SOWs as decision authority for this work.
- Discuss only Rust.
- Discuss only reader performance.
- Create a new SOW with the mandatory Rust reader performance rules.
- Do not work on Go, Python, Node.js, writers, cross-language parity, packaging,
  registry publishing, or Netdata integration in this SOW.

Mandatory Rust reader performance rules from the user:

1. Cache the file header.
2. Use rolling mmaps.
3. Cache current-row field pointers for uncompressed DATA as direct pointers
   into mmap-backed data.
4. Use a separate append-only current-row allocation arena only for compressed
   DATA; the arena is reset on every row.
5. Provide row-level validity guarantees for all pointers/slices returned by
   Rust reader APIs.
6. Maintain zero allocations in the uncompressed hot path after warmup. The
   only accepted hot-path allocation is the current-row compressed-DATA arena
   growth needed for decompression.

### Assistant Understanding

Facts:

- This SOW is Rust-only and reader-only.
- Existing Python and Node.js implementations remain in the repository but are
  out of scope for this SOW.
- Existing Go implementation remains in the repository but is out of scope for
  this SOW.
- Writer performance and writer format compatibility are out of scope for this
  SOW.
- Prior SOWs may be inspected as historical notes only when useful for locating
  current Rust code paths, but they must not weaken the mandatory rules above.

Inferences:

- This SOW should first produce a Rust-reader performance specification and
  file/line gap analysis before code changes.
- If a current Rust API cannot satisfy the mandatory rules, that is a gap to
  record. It must not be hidden behind a looser compatibility interpretation.
- The most important early risks are silent copies, per-row/per-field
  allocations, header rereads, mmap window invalidation, repeated parsing of
  reusable DATA objects, and row scans where journal indexes can answer the
  request.

Unknowns:

- Which Rust reader APIs currently violate each mandatory rule.
- Whether every public Rust reader API can support the row-level guarantee
  without changing its public type shape.
- Whether current benchmarks measure the exact APIs that must satisfy this
  contract.

### Acceptance Criteria

- A new Rust-only reader performance spec exists under `.agents/sow/specs/`.
- The spec states mandatory Rust reader performance rules, accepted exceptions,
  and validation requirements.
- A Rust-only gap analysis maps every public Rust reader path to each mandatory
  rule with file/line evidence.
- The gap analysis explicitly covers:
  - file header access;
  - mmap/window strategy;
  - uncompressed DATA pointer provenance;
  - compressed DATA arena behavior;
  - row-level pointer/slice lifetime;
  - hot-path allocation behavior;
  - index use versus row scan behavior;
  - repeated DATA parsing behavior.
- No Go, Python, Node.js, writer, packaging, release, registry, or Netdata
  integration files are changed.
- Implementation fixes may be made in this SOW after the user reviewed the
  Rust-only gap analysis and approved proceeding on 2026-06-04.
- Follow-up implementation SOWs are created for valid Rust reader performance
  gaps that are not completed in this SOW.
- No changes are made outside this repository.

## Analysis

Sources checked:

- User-provided Rust-only scope and mandatory performance rules.
- `AGENTS.md` for project SOW and repository-boundary rules.
- `.agents/sow/specs/product-scope.md` for existing broad product scope, used
  only to avoid contradicting repository structure.

Current state:

- The repository still contains multiple language implementations, but this SOW
  intentionally ignores non-Rust implementations.
- Existing broad performance language in `AGENTS.md` and product specs is not
  precise enough to mechanically enforce the user's Rust reader rules.
- A Rust-specific performance spec is missing.

Risks:

- If this SOW drifts into cross-language parity, the Rust reader performance
  contract will remain ambiguous.

## Baseline Test Environment - 2026-06-04

Purpose:

- Establish the Rust reader baseline before implementing performance changes.
- Use real journal corpus files where available.
- Keep raw local paths outside durable committed artifacts.

Environment:

- Local scratch directory:
  `.local/sow-0086/reader-baseline/`
- Harness:
  `.local/sow-0086/reader-baseline/run_baseline.py`
- Report:
  `.local/sow-0086/reader-baseline/report.json`
- Human-readable report:
  `.local/sow-0086/reader-baseline/report.md`
- Rust benchmark binary:
  `.local/cargo-target/release/reader_core_bench`
- systemd comparison binary:
  `.local/benchmarks/bin/systemd-reader-core-bench`
- Repetitions:
  3 per candidate/mode.
- Rust reader options:
  single-file, forward, `ReaderBounds::Snapshot`, windowed mmap, 32 MiB window.

Corpus candidate scan:

- Real corpus roots scanned:
  `/var/log/journal` and `/run/log/journal`.
- Real journal files found:
  7,258.
- Real journal bytes found:
  153,556,646,720.
- Real compressed-capable files:
  7,258.
- Real uncompressed-capable files:
  0.
- Real FSS/sealed files:
  0.
- Additional Netdata flow raw corpus was scanned after the user requested it.
  Raw local paths remain only in `.local/sow-0086/reader-baseline/report.json`.
- Netdata flow raw files found:
  53.
- Netdata flow raw bytes found:
  10,015,997,952.
- Netdata flow raw compressed-capable files:
  0.
- Netdata flow raw uncompressed-capable files:
  53.
- Netdata flow raw FSS/sealed files:
  0.
- Netdata flow raw states:
  52 archived files and 1 online file.

Implications:

- The local 150 GB real corpus currently covers compact Zstandard-compressed
  production journals well.
- The local 150 GB real corpus does not cover uncompressed or FSS/sealed
  journals.
- The Netdata flow raw corpus covers real uncompressed, non-compact keyed-hash
  journals well.
- Uncompressed and FSS baseline candidates therefore come from previously
  generated systemd-matrix journals under `.local/systemd-matrix/corpus/` only
  for FSS/sealed coverage. They are feature-valid but too small for stable
  performance conclusions.
- The Netdata flow raw files use older/shorter journal headers where `n_data`,
  `n_fields`, and `n_tags` are not present in the header. Field counts in the
  benchmark table therefore come from actual traversal, not from header
  counters.
- The online Netdata flow candidate was snapshotted under `.local/` before
  benchmarking to keep counts comparable across benchmark modes.

Candidate classes:

| Candidate | Source | Size | Entries | DATA objects | FIELD objects | Flags |
|---|---|---:|---:|---:|---:|---|
| real-compressed-multiboot-high-entry | real corpus | 128 MiB | 305,093 | 1,817 | 72 | keyed-hash, compressed-zstd, compact |
| real-compressed-high-cardinality | real corpus | 56 MiB | 91,551 | 174,768 | 41 | keyed-hash, compressed-zstd, compact |
| real-compressed-high-field-count | real corpus | 24 MiB | 22,887 | 28,054 | 194 | keyed-hash, compressed-zstd, compact |
| netdata-flow-largest-uncompressed | Netdata flow raw corpus | 200 MiB | 114,552 | header counter absent | header counter absent | keyed-hash |
| netdata-flow-most-entries-uncompressed | Netdata flow raw corpus | 192 MiB | 122,646 | header counter absent | header counter absent | keyed-hash |
| netdata-flow-online-uncompressed | Netdata flow raw corpus snapshot | 80 MiB | 44,532 | header counter absent | header counter absent | keyed-hash |
| systemd-matrix-uncompressed-regular | generated systemd matrix | 8 MiB | 349 | 1,494 | 175 | keyed-hash |
| systemd-matrix-fss-compact | generated systemd matrix | 8 MiB | 349 | 1,494 | 175 | sealed, sealed-continuous, keyed-hash, compact |

Baseline results, median of 3 repetitions:

| Candidate | Driver | Mode | Rows/s | Fields/s | MiB/s | Notes |
|---|---|---|---:|---:|---:|---|
| real-compressed-multiboot-high-entry | rust | core-next | 12,445,308 | 0 | 0 | row stepping only |
| real-compressed-multiboot-high-entry | rust | core-offsets | 8,164,850 | 204,142,749 | 0 | DATA offsets only |
| real-compressed-multiboot-high-entry | rust | core-payloads | 1,715,291 | 42,886,790 | 1,606 | payload access/decompression |
| real-compressed-multiboot-high-entry | rust | sdk-payloads | 3,547,978 | 88,708,785 | 3,322 | SDK payload visitor |
| real-compressed-multiboot-high-entry | rust | sdk-entry | 166,823 | 4,171,005 | 156 | owned entry materialization |
| real-compressed-multiboot-high-entry | rust | facade-data | 1,987,485 | 49,692,354 | 1,861 | libsystemd-like facade |
| real-compressed-multiboot-high-entry | systemd | next | 6,332,806 | 0 | 0 | systemd row stepping |
| real-compressed-multiboot-high-entry | systemd | data | 828,960 | 20,726,180 | 776 | systemd data enumeration |
| real-compressed-high-cardinality | rust | core-next | 9,265,805 | 0 | 0 | row stepping only |
| real-compressed-high-cardinality | rust | core-offsets | 8,275,135 | 132,744,906 | 0 | DATA offsets only |
| real-compressed-high-cardinality | rust | core-payloads | 2,397,992 | 38,467,202 | 1,224 | payload access/decompression |
| real-compressed-high-cardinality | rust | sdk-payloads | 4,578,647 | 73,448,002 | 2,337 | SDK payload visitor |
| real-compressed-high-cardinality | rust | sdk-entry | 241,975 | 3,881,618 | 124 | owned entry materialization |
| real-compressed-high-cardinality | rust | facade-data | 2,488,132 | 39,913,174 | 1,270 | libsystemd-like facade |
| real-compressed-high-cardinality | systemd | next | 5,636,756 | 0 | 0 | systemd row stepping |
| real-compressed-high-cardinality | systemd | data | 1,104,982 | 17,725,477 | 564 | systemd data enumeration |
| real-compressed-high-field-count | rust | core-next | 10,108,577 | 0 | 0 | row stepping only |
| real-compressed-high-field-count | rust | core-offsets | 7,331,141 | 193,736,290 | 0 | DATA offsets only |
| real-compressed-high-field-count | rust | core-payloads | 262,535 | 6,937,878 | 786 | payload access/decompression |
| real-compressed-high-field-count | rust | sdk-payloads | 275,358 | 7,276,749 | 824 | SDK payload visitor |
| real-compressed-high-field-count | rust | sdk-entry | 100,648 | 2,659,762 | 301 | owned entry materialization |
| real-compressed-high-field-count | rust | facade-data | 248,713 | 6,572,615 | 745 | libsystemd-like facade |
| real-compressed-high-field-count | systemd | next | 5,940,206 | 0 | 0 | systemd row stepping |
| real-compressed-high-field-count | systemd | data | 487,016 | 12,870,110 | 1,458 | systemd data enumeration |
| netdata-flow-largest-uncompressed | rust | core-next | 5,744,130 | 0 | 0 | row stepping only |
| netdata-flow-largest-uncompressed | rust | core-offsets | 4,470,543 | 185,601,528 | 0 | DATA offsets only |
| netdata-flow-largest-uncompressed | rust | core-payloads | 881,490 | 36,596,440 | 664 | payload access |
| netdata-flow-largest-uncompressed | rust | sdk-payloads | 1,623,342 | 67,395,580 | 1,223 | SDK payload visitor |
| netdata-flow-largest-uncompressed | rust | sdk-entry | 75,544 | 3,136,335 | 57 | owned entry materialization |
| netdata-flow-largest-uncompressed | rust | facade-data | 688,001 | 28,563,422 | 519 | libsystemd-like facade |
| netdata-flow-largest-uncompressed | systemd | next | 3,822,721 | 0 | 0 | systemd row stepping |
| netdata-flow-largest-uncompressed | systemd | data | 459,965 | 19,096,161 | 347 | systemd data enumeration |
| netdata-flow-most-entries-uncompressed | rust | core-next | 4,824,858 | 0 | 0 | row stepping only |
| netdata-flow-most-entries-uncompressed | rust | core-offsets | 4,165,610 | 172,884,632 | 0 | DATA offsets only |
| netdata-flow-most-entries-uncompressed | rust | core-payloads | 857,908 | 35,605,598 | 644 | payload access |
| netdata-flow-most-entries-uncompressed | rust | sdk-payloads | 1,555,639 | 64,563,429 | 1,168 | SDK payload visitor |
| netdata-flow-most-entries-uncompressed | rust | sdk-entry | 73,168 | 3,036,664 | 55 | owned entry materialization |
| netdata-flow-most-entries-uncompressed | rust | facade-data | 611,054 | 25,360,494 | 459 | libsystemd-like facade |
| netdata-flow-most-entries-uncompressed | systemd | next | 3,667,582 | 0 | 0 | systemd row stepping |
| netdata-flow-most-entries-uncompressed | systemd | data | 377,401 | 15,663,216 | 283 | systemd data enumeration |
| netdata-flow-online-uncompressed | rust | core-next | 5,515,269 | 0 | 0 | row stepping only, snapshotted |
| netdata-flow-online-uncompressed | rust | core-offsets | 4,767,307 | 197,783,200 | 0 | DATA offsets only, snapshotted |
| netdata-flow-online-uncompressed | rust | core-payloads | 924,932 | 38,373,025 | 694 | payload access, snapshotted |
| netdata-flow-online-uncompressed | rust | sdk-payloads | 1,584,251 | 65,726,438 | 1,189 | SDK payload visitor, snapshotted |
| netdata-flow-online-uncompressed | rust | sdk-entry | 72,551 | 3,009,962 | 54 | owned entry materialization, snapshotted |
| netdata-flow-online-uncompressed | rust | facade-data | 737,179 | 30,583,634 | 553 | libsystemd-like facade, snapshotted |
| netdata-flow-online-uncompressed | systemd | next | 3,826,144 | 0 | 0 | systemd row stepping, snapshotted |
| netdata-flow-online-uncompressed | systemd | data | 325,919 | 13,521,515 | 245 | systemd data enumeration, snapshotted |

Initial interpretation:

- Rust row stepping is faster than systemd on the real compressed sample.
- Rust row stepping is also faster than systemd on the real Netdata flow
  uncompressed sample.
- Rust DATA-offset enumeration is very fast; the baseline gap is not row
  cursor movement.
- `sdk-entry` is not acceptable as a hot-path API because it materializes owned
  entries and drops to roughly 73k-242k rows/s on the real sample.
- `facade-data` and `core-payloads` are not consistently better than systemd.
  On the high-field-count file, systemd data enumeration is faster than Rust
  payload enumeration.
- On the Netdata flow uncompressed sample, `sdk-payloads` is materially faster
  than systemd data enumeration, but `facade-data` still leaves a large gap
  versus `sdk-payloads`.
- The high-field-count file is the most important early profiler target because
  it exposes the cost of per-field work.
- The generated uncompressed and FSS candidates are feature coverage only; they
  are too small for stable throughput decisions.

### Baseline Recapture - 2026-06-04T18:52:59Z

Reason:

- The user requested a second capture because other workload may have stressed
  the workstation during the earlier baseline.

Local artifacts:

- Previous capture archived under:
  `.local/sow-0086/reader-baseline/archive/20260604T185243Z/`
- Current report:
  `.local/sow-0086/reader-baseline/report.json`
- Current human-readable report:
  `.local/sow-0086/reader-baseline/report.md`

Host context:

- Before run:
  load average `5.54 / 5.43 / 11.42`, 16 CPUs, governor `powersave`.
- After run:
  load average `13.97 / 8.94 / 11.07`, 16 CPUs, governor `powersave`.
- Memory was not constrained before or after the run.

Recapture result, median of 3 repetitions:

| Candidate | Driver | Mode | Rows/s | Fields/s | Records | Fields |
|---|---|---|---:|---:|---:|---:|
| real-compressed-multiboot-high-entry | rust | core-next | 12,007,987 | 0 | 305,093 | 0 |
| real-compressed-multiboot-high-entry | rust | sdk-payloads | 3,587,851 | 89,705,711 | 305,093 | 7,628,128 |
| real-compressed-multiboot-high-entry | rust | facade-data | 1,943,603 | 48,595,194 | 305,093 | 7,628,128 |
| real-compressed-multiboot-high-entry | rust | sdk-entry | 165,900 | 4,147,942 | 305,093 | 7,628,128 |
| real-compressed-multiboot-high-entry | systemd | data | 819,244 | 20,483,262 | 305,093 | 7,628,128 |
| real-compressed-high-cardinality | rust | core-next | 9,070,370 | 0 | 91,551 | 0 |
| real-compressed-high-cardinality | rust | sdk-payloads | 4,428,974 | 71,047,034 | 91,551 | 1,468,608 |
| real-compressed-high-cardinality | rust | facade-data | 2,547,019 | 40,857,807 | 91,551 | 1,468,608 |
| real-compressed-high-cardinality | rust | sdk-entry | 246,338 | 3,951,621 | 91,551 | 1,468,608 |
| real-compressed-high-cardinality | systemd | data | 1,136,179 | 18,225,928 | 91,551 | 1,468,608 |
| real-compressed-high-field-count | rust | core-next | 10,625,294 | 0 | 22,887 | 0 |
| real-compressed-high-field-count | rust | sdk-payloads | 284,399 | 7,515,672 | 22,887 | 604,823 |
| real-compressed-high-field-count | rust | facade-data | 243,490 | 6,434,589 | 22,887 | 604,823 |
| real-compressed-high-field-count | rust | sdk-entry | 99,301 | 2,624,183 | 22,887 | 604,823 |
| real-compressed-high-field-count | systemd | data | 486,707 | 12,861,957 | 22,887 | 604,823 |
| netdata-flow-largest-uncompressed | rust | core-next | 6,460,300 | 0 | 114,552 | 0 |
| netdata-flow-largest-uncompressed | rust | sdk-payloads | 1,634,613 | 67,863,492 | 114,552 | 4,755,804 |
| netdata-flow-largest-uncompressed | rust | facade-data | 639,394 | 26,545,423 | 114,552 | 4,755,804 |
| netdata-flow-largest-uncompressed | rust | sdk-entry | 76,993 | 3,196,502 | 114,552 | 4,755,804 |
| netdata-flow-largest-uncompressed | systemd | data | 478,611 | 19,870,287 | 114,552 | 4,755,804 |
| netdata-flow-most-entries-uncompressed | rust | core-next | 5,334,287 | 0 | 122,646 | 0 |
| netdata-flow-most-entries-uncompressed | rust | sdk-payloads | 1,692,424 | 70,240,411 | 122,646 | 5,090,157 |
| netdata-flow-most-entries-uncompressed | rust | facade-data | 723,122 | 30,011,594 | 122,646 | 5,090,157 |
| netdata-flow-most-entries-uncompressed | rust | sdk-entry | 76,444 | 3,172,622 | 122,646 | 5,090,157 |
| netdata-flow-most-entries-uncompressed | systemd | data | 479,083 | 19,883,310 | 122,646 | 5,090,157 |
| netdata-flow-online-uncompressed | rust | core-next | 5,754,597 | 0 | 122,544 | 0 |
| netdata-flow-online-uncompressed | rust | sdk-payloads | 1,577,662 | 65,453,055 | 122,544 | 5,084,030 |
| netdata-flow-online-uncompressed | rust | facade-data | 711,469 | 29,517,007 | 122,544 | 5,084,030 |
| netdata-flow-online-uncompressed | rust | sdk-entry | 76,830 | 3,187,467 | 122,544 | 5,084,030 |
| netdata-flow-online-uncompressed | systemd | data | 454,172 | 18,842,423 | 122,544 | 5,084,030 |

Recapture interpretation:

- The main conclusions are stable after retesting.
- Rust row stepping remains strong.
- `sdk-payloads` is the fastest existing Rust public payload-reading path in
  the large real-file cases.
- `facade-data` remains materially slower than `sdk-payloads`, especially on
  Netdata flow files where it is roughly 2.2x-2.6x slower.
- `sdk-entry` remains a convenience/materialization API, not a hot-path API.
- The real compressed high-field-count file remains the worst Rust payload
  case and should be the first profiler target.
- systemd data enumeration improved in the recapture on several files,
  confirming that the first run had measurement noise, but systemd comparison
  does not change the Rust-side priority.
- If this SOW starts implementation before the gap analysis, fixes may optimize
  the wrong API path or preserve a non-optimal abstraction.
- If the allocation and pointer-provenance rules are not tested mechanically,
  future changes can satisfy the semantic API contract while losing the
  performance contract.

### Rust Reader API Taxonomy

This SOW treats the Rust reader as several related APIs, not one API. The
performance contract applies strictly to the hot traversal APIs. Owned
materialization helpers may allocate, but they must be explicitly documented as
non-hot convenience APIs and must not sit underneath facade/data-enumeration hot
paths.

1. Core file-format primitives
   - Surfaces:
     - `rust/src/crates/journal-core/src/file/file.rs`
     - `rust/src/crates/journal-core/src/file/reader.rs`
     - `rust/src/crates/journal-core/src/file/cursor.rs`
     - `rust/src/crates/journal-core/src/file/offset_array.rs`
     - `rust/src/crates/journal-core/src/file/file_iterators.rs`
     - `rust/src/crates/journal-core/src/file/file_payload.rs`
   - Role: lowest-level journal-file access, cursor movement, object reads,
     offset-array walking, FIELD/DATA/ENTRY traversal, filters, and payload
     access.
   - Expected performance contract: this is the primary hot path. It must use
     cached header state, rolling mmap access, justified object/window reads,
     no avoidable per-row/per-field allocation, and direct mmap-backed slices
     for uncompressed DATA.

2. Single-file SDK reader API
   - Surface: `rust/src/journal/src/lib.rs`, `FileReader`.
   - Role: idiomatic single-file reader built on the core primitives.
   - Sub-paths:
     - Row movement and metadata: `next()`, `previous()`,
       `get_realtime_usec()`, `get_cursor()`.
     - Callback data traversal: `visit_entry_payloads()`.
     - libsystemd-style row data traversal:
       `entry_data_restart()` plus `enumerate_entry_payload()`.
     - Owned materialization: `get_entry()`, `collect_entry_payloads()`,
       `get_entry_payload()`, `query_unique()`, `enumerate_fields()`.
   - Expected performance contract: row movement, metadata, callback traversal,
     and libsystemd-style row data traversal must satisfy the hot-path rules.
     Owned materialization APIs may allocate but must remain outside hot
     traversal and facade data-enumeration internals.

3. Directory SDK reader API
   - Surface: `rust/src/journal/src/directory.rs`, `DirectoryReader`.
   - Role: ordered multi-file reader that wraps several `FileReader`
     instances, chooses candidates, and delegates payload access to the active
     file.
   - Expected performance contract: single-file row/payload access rules still
     apply through the active `FileReader`. Directory merge logic may need
     extra state for candidates, but repeated file header reads, avoidable
     candidate object reads, and avoidable allocations remain performance
     gaps.

4. libsystemd-like facade API
   - Surface: `rust/src/journal/src/facade.rs`, `SdJournal` and
     `SdJournal*` functions.
   - Role: compatibility layer for libsystemd-style callers.
   - Expected performance contract: `next()`/`previous()` plus
     `restart_data()`/`enumerate_available_data()` must be thin wrappers over
     the same hot row-level traversal guarantees as `FileReader`. The facade
     must not copy uncompressed mmap-backed DATA just to emulate libsystemd.
     Metadata calls must not materialize a full `Entry`.

5. Formatting/export/convenience APIs
   - Surfaces:
     - `rust/src/journal/src/export.rs`
     - `rust/src/journal/src/reader_helpers.rs`
     - owned `Entry` APIs in `rust/src/journal/src/lib.rs`
   - Role: build owned maps, vectors, strings, JSON, export, and text output.
   - Expected performance contract: these APIs may allocate by design and are
     not the low-level hot path. The requirement is separation: hot traversal
     must not accidentally call these APIs for simple metadata, data
     enumeration, filtering, or row scanning.

6. `.journal.zst` wrapper path
   - Surfaces: `FileReader::open_zst()` and directory file discovery.
   - Role: opens externally compressed journal archives by decompressing to a
     temporary journal file before normal reading.
   - Expected performance contract: this is archive/container handling, not the
     normal journal mmap hot path. It must not be used as evidence for hot
     in-file DATA decompression performance.

### First Pass Rust Reader Hot-Path Findings

This is a code-read finding list, not a profiler result. Impact ordering is a
working theory until benchmarks and profiles confirm it. The facts below are
line-level observations from the Rust reader code.

1. ENTRY object is reread once per field in the core iterator.
   - Evidence:
     `rust/src/crates/journal-core/src/file/file_iterators.rs:113` reads the
     current ENTRY object in every `EntryDataIterator::next()` call.
     `rust/src/crates/journal-core/src/file/file_iterators.rs:146` then reads
     the DATA object for the selected item.
   - Why this is a performance gap: a row with 32 DATA items can reread the
     same ENTRY object 32 times just to enumerate offsets.
   - Required direction: cache the current row's DATA object offsets once per
     row and enumerate cached offsets.

2. Offset-array cursor movement repeatedly rebuilds and rereads array nodes.
   - Evidence:
     `rust/src/crates/journal-core/src/file/offset_array.rs:408` builds a
     `Node` from the cursor's current array offset.
     `rust/src/crates/journal-core/src/file/offset_array.rs:417` calls
     `self.node(journal_file)` during `Cursor::next()`.
     `rust/src/crates/journal-core/src/file/offset_array.rs:412` calls
     `self.node(journal_file)?.get(...)` during `Cursor::value()`.
     `rust/src/crates/journal-core/src/file/offset_array.rs:77` rereads the
     offset-array object for `Node::get()`.
   - Why this is a performance gap: a single row step can touch the same
     offset-array node multiple times before the ENTRY object is even read.
   - Required direction: cursor state should cache the active offset-array node
     metadata/value position so forward movement inside the same node does not
     rebuild the node or reread the object.

3. Reverse offset-array traversal is potentially expensive at node boundaries.
   - Evidence:
     `rust/src/crates/journal-core/src/file/offset_array.rs:468` walks from the
     list head to find the previous node when a cursor is at index 0.
   - Why this is a performance gap: reverse scans over many offset-array nodes
     can pay repeated list walks.
   - Required direction: reverse cursor state needs a previous-node strategy or
     a lightweight per-file offset-array node cache built outside the hot row
     step.

4. Journal header access is not cached as hot-path state.
   - Evidence:
     `rust/src/crates/journal-core/src/file/file.rs:711` returns a sanitized
     header if present, otherwise calls `JournalHeader::ref_from_prefix()` on
     the header mmap.
     `rust/src/journal/src/lib.rs:511` reads the header while stepping rows.
     `rust/src/crates/journal-core/src/file/file_payload.rs:71` reads the
     header to build DATA payload context.
   - Why this is a performance gap: header fields needed by hot paths are
     immutable for snapshot readers and should be cached as plain reader state.
   - Required direction: cache the needed header fields at open/snapshot time
     and refresh explicitly only for live mode if live mode remains supported.

5. Generic object access performs repeated mmap/window work.
   - Evidence:
     `rust/src/crates/journal-core/src/file/file.rs:771` reads the object
     header slice to determine size.
     `rust/src/crates/journal-core/src/file/file.rs:790` reads the full object
     data after validating bounds.
   - Why this is a performance gap: generic access may need this shape, but the
     row hot path knows whether it needs ENTRY metadata, ENTRY item offsets,
     DATA header, DATA payload, or offset-array values. Those paths should not
     pay unnecessary generic object parsing and second window lookups.
   - Required direction: add specialized hot readers where the access pattern
     is known and keep generic `journal_object_ref()` for non-hot callers.

6. Per-object guard machinery is correctness-oriented but hot-path expensive.
   - Evidence:
     `rust/src/crates/journal-core/src/file/guarded_cell.rs:244` uses guarded
     access around the window manager.
     `rust/src/crates/journal-core/src/file/value_guard.rs:64` resets the guard
     on drop.
     `rust/src/crates/journal-core/src/file/reader.rs:134` drops guards before
     every step.
   - Why this is a performance gap: per-object guard churn is avoidable if row
     traversal pins the current row/window lifetime once and returns row-valid
     slices from that pinned state.
   - Required direction: keep object guards for generic APIs, but make the hot
     row API use row-level mmap/window pinning instead.

7. Windowed facade row data still copies uncompressed DATA.
   - Evidence:
     `rust/src/journal/src/lib.rs:379` enables borrowed uncompressed row
     payloads only for whole-file mmap strategy.
     `rust/src/journal/src/lib.rs:692` converts uncompressed payloads to owned
     `Vec` when the reader is not in whole-file mmap mode.
   - Why this is a performance gap: the mandatory rule requires row-level
     pointers into mmap-backed DATA for uncompressed payloads with rolling
     mmaps too.
   - Required direction: row traversal must pin the required mmap window(s) for
     the row and return direct row-valid slices without copying.

8. Compressed row data allocates a fresh `Vec` per DATA object.
   - Evidence:
     `rust/src/journal/src/lib.rs:701` creates `let mut decompressed =
     Vec::new()` for each compressed row payload.
   - Why this is a performance gap: compressed DATA is the only accepted
     hot-path allocation case, but it must use a row-level append-only arena
     reset per row, not a fresh allocation per compressed DATA object.
   - Required direction: introduce a row arena for compressed payloads and
     return row-valid slices into that arena.

9. Facade metadata calls materialize full entries.
   - Evidence:
     `rust/src/journal/src/facade.rs:276` implements `get_seqnum()` through
     `self.get_entry()`.
     `rust/src/journal/src/facade.rs:282` implements `get_monotonic_usec()`
     through `self.get_entry()`.
     `rust/src/journal/src/reader_helpers.rs:44` shows `read_entry_at()` builds
     owned maps, vectors, payload copies, and cursor strings.
   - Why this is a performance gap: simple metadata reads should use cached
     current row metadata and must not expand all DATA fields.
   - Required direction: facade metadata methods should read from
     `DirectoryEntryKey`/current row metadata, not from `Entry`.

10. `FileReader::next()` currently prepares all DATA offsets for every row.
    - Evidence:
      `rust/src/journal/src/lib.rs:511` reads the ENTRY and
      `rust/src/journal/src/lib.rs:514` collects all DATA offsets during every
      successful row step.
    - Why this may be a performance gap: if a caller only needs row metadata or
      top-N seek positioning, collecting all DATA offsets is extra work. If the
      public contract defines `next()` as preparing row data for zero-copy
      enumeration, this may be justified.
    - Required direction: make this an explicit API decision. Either keep
      `next()` as "prepare current row for data enumeration" or add a separate
      metadata-only step path. Do not leave the behavior implicit.

11. Convenience entry materialization allocates heavily and must stay non-hot.
    - Evidence:
      `rust/src/journal/src/reader_helpers.rs:49` creates `HashMap` state.
      `rust/src/journal/src/reader_helpers.rs:54` creates `Vec` state.
      `rust/src/journal/src/reader_helpers.rs:71` copies payloads with
      `to_vec()`.
    - Why this is acceptable only as non-hot behavior: callers that explicitly
      request an owned `Entry` need owned data. Hot traversal and facade data
      enumeration must not call this path.
    - Required direction: document and test that `get_entry()`/export paths are
      owned convenience APIs, not the fastest traversal APIs.

12. Indexed field and unique-value APIs are directionally correct, but
    materializing wrappers allocate by design.
    - Evidence:
      `rust/src/journal/src/reader_helpers.rs:129` walks FIELD indexes for
      field enumeration.
      `rust/src/journal/src/reader_helpers.rs:157` walks FIELD DATA chains for
      unique values.
      `rust/src/journal/src/lib.rs:807` materializes unique values into
      `Vec<Vec<u8>>`.
    - Why this matters: index use is correct for unfiltered field/value
      enumeration, but the hot API should be the visitor form. The materialized
      `query_unique()` wrapper is a convenience API.
    - Required direction: preserve indexed visitor APIs as the hot path and
      keep materializing list APIs separate.

13. Directory reader adds merge/candidate overhead that is separate from
    single-file hot-path work.
    - Evidence:
      `rust/src/journal/src/directory.rs:120` iterates all files to fill merge
      candidates.
      `rust/src/journal/src/directory.rs:203` advances one candidate reader at
      a time.
      `rust/src/journal/src/directory.rs:429` deduplicates unique values across
      files with a `HashSet`.
    - Why this is not the first target: the user's current focus is the lowest
      level single-file reader hot path. Directory work depends on the
      single-file hot path being correct first.
    - Required direction: record as secondary Rust reader work after
      single-file row/payload traversal is fixed.

### Separation, Ownership, And Duplication Audit

This section answers the user's architecture question directly. The Rust reader
code has useful layers, but it is not currently cleanly separated. There is
duplicated ownership of row state and duplicated logic for entry DATA offset
collection, payload extraction, field parsing, filter evaluation, and multi-file
query ordering. No implementation work has started.

Scope checked:

- `rust/src/crates/journal-core/src/file/*.rs`
- `rust/src/journal/src/*.rs`
- `rust/src/crates/journal-index/src/*.rs`
- `rust/src/crates/journal-engine/src/logs/query.rs`

Findings:

1. Current-row ownership is split between `JournalReader` and `FileReader`.
   - Evidence:
     `rust/src/crates/journal-core/src/file/reader.rs:14` defines
     `JournalReader`, which owns cursor, filters, field iterators, DATA
     iterator, and object guards at
     `rust/src/crates/journal-core/src/file/reader.rs:15`.
     `rust/src/journal/src/lib.rs:305` defines `FileReader`, which separately
     owns `current_key`, `data_offsets`, DATA payload context, row payload
     cache, and owned row payload buffers at `rust/src/journal/src/lib.rs:309`.
   - Why this is wrong separation: both layers know about current-row
     traversal state. The core layer has `entry_data_iterator`; the SDK layer
     has another DATA-offset cursor and row payload cache. That means fixes for
     row lifetime, mmap pinning, and allocation behavior can land in one layer
     while another layer still uses the old path.
   - Required direction: there should be one current-row owner for the hot path.
     It should own row metadata, DATA offsets, row-pinned mmap windows, and the
     compressed row arena. Higher APIs should borrow from that owner.

2. Entry DATA offset collection is implemented in multiple places.
   - Evidence:
     `JournalFile::entry_data_object_offsets()` collects entry DATA offsets at
     `rust/src/crates/journal-core/src/file/file.rs:702`.
     `JournalFile::entry_data_objects()` creates an iterator at
     `rust/src/crates/journal-core/src/file/file.rs:897`.
     `JournalReader::entry_data_offsets()` duplicates offset collection at
     `rust/src/crates/journal-core/src/file/reader.rs:324`.
     `reader_helpers::collect_entry_data_offsets()` duplicates it at
     `rust/src/journal/src/reader_helpers.rs:205`.
     `journal-engine` has another local `collect_entry_data_offsets()` at
     `rust/src/crates/journal-engine/src/logs/query.rs:631`.
     `journal-index` has direct `entry.collect_offsets()` use at
     `rust/src/crates/journal-index/src/file_index.rs:471`.
   - Why this is wrong ownership: ENTRY layout knowledge is scattered. Compact
     and regular ENTRY handling should not be reimplemented by every consumer.
   - Required direction: one reusable hot primitive should expose current-row
     DATA offsets. Consumers that need owned/materialized data can wrap it.

3. Payload extraction and decompression ownership is duplicated.
   - Evidence:
     `FileReader::read_row_payload()` handles borrowed-vs-owned row payloads at
     `rust/src/journal/src/lib.rs:673`.
     `reader_helpers::visit_entry_payload_offsets()` has a separate visitor path
     at `rust/src/journal/src/reader_helpers.rs:98`.
     `reader_helpers::read_entry_at()` separately decompresses and copies at
     `rust/src/journal/src/reader_helpers.rs:60`.
     `journal-engine` separately opens DATA objects and decompresses in
     `read_projected_pair()` at
     `rust/src/crates/journal-engine/src/logs/query.rs:642`.
     `journal-index` separately decompresses or parses DATA in regex and
     timestamp paths at `rust/src/crates/journal-index/src/file_index.rs:486`.
   - Why this is wrong ownership: compressed/uncompressed DATA policy is a core
     row-reader concern, but multiple higher layers decide independently whether
     to borrow, copy, decompress, or parse. This can bypass the performance
     contract.
   - Required direction: uncompressed DATA borrowing and compressed DATA arena
     allocation should be provided by one row-view API. Higher layers should not
     open DATA objects directly unless they are explicitly non-hot tooling.

4. Row-level pointer lifetime is currently owned by `FileReader`, not by the
   lower reader primitive, and only works for whole-file mmap.
   - Evidence:
     `FileReader` enables borrowed row payloads only when the mmap strategy is
     `WholeFile` at `rust/src/journal/src/lib.rs:379`.
     The unsafe slice reconstruction comment states that borrowed row payloads
     are only created for whole-file mmap at `rust/src/journal/src/lib.rs:335`.
     The windowed path copies uncompressed payloads with `to_vec()` at
     `rust/src/journal/src/lib.rs:692`.
   - Why this is wrong separation: the row lifetime guarantee is a core reader
     contract, but today it is a high-level workaround. The lower reader does not
     own row-pinned windows and therefore cannot guarantee rolling-mmap row
     slices.
   - Required direction: the lower hot path must pin all windows needed for the
     current row, and expose row-valid slices to `FileReader`, facade, and query
     code.

5. The facade is not a thin compatibility shell in all paths.
   - Evidence:
     `SdJournal::restart_data()` delegates to row data traversal at
     `rust/src/journal/src/facade.rs:295`, which is the correct direction.
     But `SdJournal::get_seqnum()` materializes a full `Entry` at
     `rust/src/journal/src/facade.rs:276`, and
     `SdJournal::get_monotonic_usec()` does the same at
     `rust/src/journal/src/facade.rs:282`.
     Field and unique enumeration are materialized into facade-owned vectors at
     `rust/src/journal/src/facade.rs:319` and
     `rust/src/journal/src/facade.rs:381`.
   - Why this is mixed: data enumeration is layered over `FileReader`, but
     metadata and enumeration-state helpers sometimes own data that should be
     current-row or visitor-backed.
   - Required direction: facade row movement and data enumeration should be thin
     wrappers over the same row-view primitive. Owned list APIs can remain, but
     they must be explicit convenience paths.

6. FIELD=value parsing is duplicated and partly string-owned.
   - Evidence:
     `reader_helpers::read_entry_at()` splits byte payloads at
     `rust/src/journal/src/reader_helpers.rs:75`.
     `FieldValuePair::parse()` parses owned strings at
     `rust/src/crates/journal-index/src/field_types.rs:101`.
     `FieldValuePair::strip_field_prefix()` is a separate byte-oriented helper
     at `rust/src/crates/journal-index/src/field_types.rs:173`.
     `journal-engine` converts payload bytes through
     `String::from_utf8_lossy()` before parsing at
     `rust/src/crates/journal-engine/src/logs/query.rs:656`.
     `journal-index` also converts DATA payloads with
     `String::from_utf8_lossy()` at
     `rust/src/crates/journal-index/src/file_indexer.rs:300`.
   - Why this is wrong ownership: parsing the journal DATA payload contract
     should be byte-first. String-owned parsing is a higher-level policy and is
     not safe as the shared primitive for RAW or binary-capable journal data.
   - Required direction: create one byte-oriented field/value splitter for hot
     paths and keep string conversion as a convenience wrapper.

7. Filter logic exists in multiple independent forms.
   - Evidence:
     Core journal-native filtering is `JournalFilter` at
     `rust/src/crates/journal-core/src/file/filter.rs:157`.
     A separate `index_filter.rs` file defines `IndexFilter` at
     `rust/src/crates/journal-core/src/file/index_filter.rs:87`, but
     `rust/src/crates/journal-core/src/file/mod.rs:1` through
     `rust/src/crates/journal-core/src/file/mod.rs:55` do not include an
     `index_filter` module, and repository search found no non-test use outside
     that file.
     `journal-index` has its own bitmap filter model, with
     `FileIndex::query_bitmap()` at
     `rust/src/crates/journal-index/src/file_index.rs:578`.
   - Why this is wrong separation: there appear to be two real filter layers
     plus one stale/uncompiled implementation. The difference between
     journal-native DATA-chain filtering and external bitmap-index filtering is
     legitimate, but the boundary is not documented and stale code adds noise.
   - Required direction: keep exactly two documented filter domains if both are
     needed: journal-file native filters and external index filters. Remove or
     quarantine stale filter code after verification.

8. Directory ordering is implemented in more than one subsystem.
   - Evidence:
     `DirectoryReader::step_merged()` owns multi-file candidate selection at
     `rust/src/journal/src/directory.rs:156`.
     `journal-engine::retrieve_log_entries()` owns another multi-file query and
     merge pipeline at `rust/src/crates/journal-engine/src/logs/query.rs:245`.
     `journal-engine::merge_log_entries()` merges sorted file results at
     `rust/src/crates/journal-engine/src/logs/query.rs:489`.
   - Why this is duplicated ownership: ordered multi-file traversal exists once
     for SDK directory reading and again for indexed query results. They may
     serve different use cases, but the ordering contract is duplicated.
   - Required direction: either share a common ordering/key comparison primitive
     or explicitly document the different contracts so future fixes do not update
     one and miss the other.

9. Indexed query/explorer code bypasses `FileReader` and reimplements row
   extraction.
   - Evidence:
     `journal-engine::extract_entry_data()` opens `JournalFile` directly at
     `rust/src/crates/journal-engine/src/logs/query.rs:575`.
     It keeps its own DATA offset scratch vector at
     `rust/src/crates/journal-engine/src/logs/query.rs:576`.
     It extracts projected fields through local helpers at
     `rust/src/crates/journal-engine/src/logs/query.rs:609`.
     `journal-index::FileIndexer::index()` also opens `JournalFile` directly at
     `rust/src/crates/journal-index/src/file_indexer.rs:150`.
   - Why this matters: this can be justified for index-building and indexed
     query performance, but it means `FileReader` hot-path fixes will not
     automatically improve engine/index extraction. The shared primitive must be
     below both `FileReader` and these crates.
   - Required direction: move the row-view primitive into `journal-core` or an
     equivalent lower shared crate so `FileReader`, facade, engine, and indexer
     can use the same hot-path implementation.

10. Header/cache ownership is unclear.
    - Evidence:
      `FileReader::header()` reconstructs a `FileHeader` from
      `JournalFile::journal_header_ref()` at `rust/src/journal/src/lib.rs:423`.
      `FileReader::step_valid()` reads the header while stepping rows at
      `rust/src/journal/src/lib.rs:513`.
      `reader_helpers::build_cursor()` obtains sequence metadata by calling
      `reader.get_seqnum()` and then rereads the ENTRY at
      `rust/src/journal/src/reader_helpers.rs:21`.
      `DirectoryReader::from_readers()` sorts by header-derived values at
      `rust/src/journal/src/directory.rs:98`.
   - Why this is wrong separation: header fields needed by hot traversal,
     cursor formatting, and directory ordering should be cached as snapshot
     reader state. Today each layer decides when to read or copy header data.
   - Required direction: cache immutable snapshot header fields once and expose
     them through the row-view/file snapshot state. Live refresh behavior must be
     explicit and separate.

11. Verification/export/materialization helpers are separate enough, but they
    must not leak into hot paths.
    - Evidence:
      `read_entry_at()` builds owned `HashMap`, `Vec`, and cursor strings at
      `rust/src/journal/src/reader_helpers.rs:44`.
      `verify_journal_file_strict()` intentionally walks and validates every
      object at `rust/src/journal/src/reader_helpers.rs:240`.
      Export formatting lives in `rust/src/journal/src/export.rs`.
   - Assessment: these are acceptable as non-hot paths. The problem is not their
     existence; the problem is when facade metadata or hot traversal calls them.

Conclusion:

- The code is not one clean ownership chain today.
- The intended API layering exists, but important hot-path responsibilities are
  split and duplicated.
- The correct target is one lower row-view primitive that owns current-row state,
  row lifetime, DATA offset enumeration, uncompressed mmap slices, compressed
  arena slices, and cached metadata/header context.
- `FileReader`, `DirectoryReader`, `SdJournal`, `journal-engine`, and
  `journal-index` should build on that primitive or explicitly document
  themselves as owned/non-hot tooling.

### Netdata Vendored Rust Comparison

The user asked whether the vendored Rust reader code in the Netdata working tree
has the same problems. This was checked read-only against Netdata working tree
commit `b695fa41f8ef`. Paths below are relative to that working tree.

Facts:

- The older `jf` compatibility reader is simpler than this SDK's current
  `FileReader` stack. It does not have the SDK's extra high-level `FileReader`
  row-payload cache, so it has fewer duplicated ownership layers.
- The lower-level `jf` reader still has the same fundamental performance/lifetime
  limitations: per-object guards, ENTRY reread during DATA enumeration, no
  row-pinned window abstraction, and no compressed row arena.
- Netdata's newer `journal-core`, `journal-engine`, `journal-index`, and
  `netflow-plugin` paths have many of the same duplicated extraction/parsing
  patterns seen in this SDK, plus Netdata-specific field-remapping logic that
  belongs outside a generic SDK core.

Evidence:

1. Old `jf` has per-object ownership, not row-level ownership.
   - `src/crates/jf/journal_file/src/reader.rs:13` defines `JournalReader`.
   - `src/crates/jf/journal_file/src/reader.rs:16` through
     `src/crates/jf/journal_file/src/reader.rs:22` own filter, iterators, and
     per-object guards.
   - `src/crates/jf/journal_file/src/value_guard.rs:8` through
     `src/crates/jf/journal_file/src/value_guard.rs:28` explicitly state that
     the window manager may reuse memory-mapped regions and that the guard
     prevents access after memory may be repurposed.
   - `src/crates/jf/journal_file/src/reader.rs:180` drops the previous guard
     before enumerating the next entry DATA object.
   - Assessment: this is not the row-level guarantee the SDK now needs. A caller
     caching pointers for every field in one row depends on mmap windows not
     being evicted, not on an explicit row pin.

2. Old `jf` rereads the ENTRY object for each field during DATA enumeration.
   - `src/crates/jf/journal_file/src/file.rs:665` reads the ENTRY once only to
     count total DATA items.
   - `src/crates/jf/journal_file/src/file.rs:1141` reads the same ENTRY again
     in every `EntryDataIterator::next()` call.
   - Assessment: same root performance bug as the SDK core iterator.

3. Old `jf` FFI returns direct uncompressed mmap payloads, but compressed data
   uses one reusable buffer, not a row arena.
   - `src/crates/jf/journal_reader_ffi/src/lib.rs:98` defines `RsdJournal`.
   - `src/crates/jf/journal_reader_ffi/src/lib.rs:102` owns one
     `decompressed_payload` buffer.
   - `src/crates/jf/journal_reader_ffi/src/lib.rs:285` enumerates the next DATA
     object.
   - `src/crates/jf/journal_reader_ffi/src/lib.rs:287` through
     `src/crates/jf/journal_reader_ffi/src/lib.rs:291` decompress into that one
     buffer and return its pointer.
   - `src/crates/jf/journal_reader_ffi/src/lib.rs:297` through
     `src/crates/jf/journal_reader_ffi/src/lib.rs:299` return the uncompressed
     mmap payload pointer directly.
   - Assessment: old `jf` is closer to the desired no-copy uncompressed path
     than the SDK's windowed-copy path, but it still does not preserve every
     compressed field pointer for the whole row.

4. Netdata's newer `journal-core` has the same core reader/iterator shape.
   - `src/crates/journal-core/src/file/reader.rs:15` defines `JournalReader`.
   - `src/crates/journal-core/src/file/reader.rs:18` through
     `src/crates/journal-core/src/file/reader.rs:24` own the same filter,
     iterator, and guard state.
   - `src/crates/journal-core/src/file/file.rs:628` creates
     `EntryDataIterator`.
   - `src/crates/journal-core/src/file/file.rs:1166` rereads the ENTRY object in
     every iterator step.
   - Assessment: same lower-level issues as the SDK's core reader, though the
     exact files differ because this SDK has since split some code into
     `file_iterators.rs` and `file_payload.rs`.

5. Netdata's newer `journal-core` contains Netdata-specific remapping in reader
   core.
   - `src/crates/journal-core/src/file/reader.rs:26` introduces field-name
     remapping state inside `JournalReader`.
   - `src/crates/journal-core/src/file/reader.rs:173` through
     `src/crates/journal-core/src/file/reader.rs:209` translate match fields
     inside `add_match()`.
   - `src/crates/journal-core/src/file/reader.rs:338` through
     `src/crates/journal-core/src/file/reader.rs:454` scan and parse remapping
     entries.
   - Assessment: this is an explicit separation-of-concerns problem for a
     generic SDK. Netdata may need this as an application layer, but it should
     not be in the SDK core.

6. Netflow scan code bypasses `JournalReader` and reimplements row extraction.
   - `src/crates/netflow-plugin/src/query/scan/direct.rs:53` creates a
     `JournalReader` only to build filters.
   - `src/crates/netflow-plugin/src/query/scan/direct.rs:57` creates a separate
     `JournalCursor`, and `src/crates/netflow-plugin/src/query/scan/direct.rs:61`
     drops the reader.
   - `src/crates/netflow-plugin/src/query/scan/direct.rs:105` through
     `src/crates/netflow-plugin/src/query/scan/direct.rs:113` collect DATA
     offsets directly from `JournalFile`.
   - `src/crates/netflow-plugin/src/query/scan/direct.rs:147` through
     `src/crates/netflow-plugin/src/query/scan/direct.rs:157` open DATA objects
     and decompress/read payloads directly.
   - Assessment: this avoids some reader abstraction overhead, but it duplicates
     row extraction logic outside the core reader. That is exactly why this SDK
     needs a shared row-view primitive below all consumers.

7. Netdata query/index crates duplicate parsing and extraction too.
   - `src/crates/journal-engine/src/logs/query.rs:567` opens `JournalFile`
     directly.
   - `src/crates/journal-engine/src/logs/query.rs:587` collects entry DATA
     offsets locally.
   - `src/crates/journal-engine/src/logs/query.rs:598` through
     `src/crates/journal-engine/src/logs/query.rs:611` reads/decompresses DATA
     and converts payloads through `String::from_utf8_lossy()`.
   - `src/crates/journal-index/src/file_indexer.rs:161` opens `JournalFile`
     directly.
   - `src/crates/journal-index/src/field_types.rs:101` implements string-owned
     `FieldValuePair::parse()`, while
     `src/crates/netflow-plugin/src/query/fields/payload.rs:4` implements a
     separate byte splitter.
   - Assessment: the same split between byte-native journal payload handling and
     string-owned query/index abstractions exists in Netdata.

Conclusion:

- Old `jf` does not have every SDK problem because it is thinner.
- Old `jf` still has the core row-lifetime and repeated-ENTRY-read problems.
- Netdata's newer journal crates and netflow query paths do have the broader
  duplication/separation issues.
- Therefore, the SDK should not blindly copy the Netdata reader layering. The
  useful lesson from Netdata is that direct mmap payload access is important;
  the part to fix is making that direct access row-scoped, shared, and owned by
  one lower primitive instead of duplicated by every consumer.

## Pre-Implementation Gate

Status: implementation approved by user on 2026-06-04

Problem / root-cause model:

- The current project has treated row-level validity, mmap-backed reads, and
  low allocation behavior as broad performance goals rather than a hard Rust
  reader contract. That allowed semantically correct but performance-weak
  choices, such as copying uncompressed DATA to satisfy a row-lifetime rule.

Evidence reviewed:

- User-stated mandatory Rust reader performance rules in this conversation.
- `AGENTS.md`: existing performance contract is broad and not Rust-specific.
- `.agents/sow/specs/product-scope.md`: existing reader performance contract is
  product-wide and not precise enough for Rust API enforcement.

Affected contracts and surfaces:

- Rust reader SDK APIs.
- Rust libsystemd-like reader facade APIs.
- Rust file-backed journalctl reader paths only if they use Rust SDK reader
  APIs.
- Rust reader benchmarks and allocation/provenance tests.
- Rust reader performance documentation and specs.

Existing patterns to reuse:

- Project SOW lifecycle and validation gates.
- Existing Rust tests and benchmark harnesses as implementation targets to
  inspect during gap analysis.
- Existing `.local/benchmarks/` convention for later benchmark artifacts.

Risk and blast radius:

- High for Rust reader performance and future Netdata reader integration.
- Medium for public Rust reader API shape if current APIs cannot express
  row-level mmap-backed pointers without copying.
- Low for non-Rust code because this SOW explicitly forbids touching it.

Sensitive data handling plan:

- Use repository code, generated fixtures, and sanitized benchmark artifacts
  only.
- Do not read live host journals.
- Do not record real logs, customer identifiers, personal data, credentials,
  bearer tokens, SNMP communities, private endpoints, or proprietary incident
  details in durable artifacts.

Implementation plan:

1. Create a Rust-only reader performance spec under `.agents/sow/specs/`.
2. Inventory Rust reader public APIs and internal hot paths.
3. Produce a rule-by-rule gap analysis with file/line evidence.
4. Identify which gaps are correctness-neutral performance gaps, API-contract
   gaps, benchmark gaps, or test-enforcement gaps.
5. Stop for user review before any implementation fixes.
6. Create focused follow-up SOWs for accepted Rust reader performance fixes.

Validation plan:

- `git diff --check`.
- `.agents/sow/audit.sh`.
- Same-failure search for non-Rust scope drift in this SOW's changed files.
- User review of the Rust-only gap analysis was completed in conversation.
- Baseline recapture was completed before implementation.
- Rust-only implementation fixes must be validated against the recaptured
  baseline.

Artifact impact plan:

- AGENTS.md: no update in this SOW unless the user approves promoting the final
  Rust reader rules into project-wide guardrails.
- Runtime project skills: no update in this SOW unless the final gap analysis
  exposes a reusable workflow rule for future Rust reader work.
- Specs: add a Rust-only reader performance spec and update it with the final
  implemented contract.
- End-user/operator docs: no public docs update expected during gap analysis.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: new pending SOW under the paused performance umbrella, but
  this SOW owns only Rust reader performance discussion and gap analysis.
- SOW-status.md: update to list this pending SOW.

Open-source reference evidence:

- No external open-source repository evidence is required to create this SOW.
  Later gap analysis may inspect systemd source read-only only when needed to
  understand journal index semantics.

Open decisions:

- Resolved by user: Rust-only.
- Resolved by user: reader-performance-only.
- Resolved by user: no Python or Node.js deletion.
- Resolved by user: no Go/Python/Node.js work in this SOW.
- Resolved by user: implementation may proceed in this SOW after the Rust reader
  performance gap analysis and baseline recapture.

## Implications And Decisions

1. 2026-06-04 Rust-only scope lock
   - Decision: this SOW discusses and analyzes only Rust reader performance.
   - Implication: any Go, Python, Node.js, writer, release, packaging, or
     Netdata integration issue discovered while reading files is recorded
     outside this SOW and not fixed here.
   - Risk: cross-language parity may temporarily lag behind Rust decisions, but
     this is accepted so Rust can become the clean performance reference.

2. 2026-06-04 performance is mandatory, not optional
   - Decision: cached header, rolling mmap, row-level mmap-backed pointers for
     uncompressed DATA, compressed row arena, row-level guarantees, and
     zero-allocation uncompressed hot paths are mandatory Rust reader rules.
   - Implication: semantic compatibility is not enough to close Rust reader
     performance work.
   - Risk: public API changes may be required if existing APIs cannot expose
     these guarantees without copies.

3. 2026-06-04 gap analysis before implementation
   - Decision: this SOW stops at Rust reader spec and gap analysis before fixes.
   - Implication: implementation follow-ups will be smaller and evidence-based.
   - Risk: no immediate performance fix lands from this SOW, but it prevents
     another quick workaround from being accepted as done.

4. 2026-06-04 implementation approval
   - Decision: after reviewing the gap analysis and requesting a second
     baseline capture, the user approved starting Rust reader performance fixes
     in this SOW.
   - Implication: this SOW now owns the first Rust-only implementation batch,
     not only the analysis.
   - Risk: the diff can become larger than a pure analysis SOW. The mitigation
     is to keep implementation phases explicit, validate after each meaningful
     phase, and preserve follow-up SOWs for gaps that are not completed here.

## Plan

1. Write `.agents/sow/specs/rust-reader-performance.md`.
2. Preserve the current recaptured baseline as the control.
3. Fix correctness-neutral facade metadata/materialization leaks first.
4. Implement the Rust current-row hot-path owner needed for row-level lifetime,
   uncompressed mmap-backed slices, and compressed row arena slices.
5. Route `FileReader` payload traversal and facade data enumeration through the
   same row-view primitive.
6. Remove repeated current-row ENTRY reads where the row-view primitive makes
   cached offsets available.
7. Add tests for row-level validity and no accidental owned-entry use in hot
   facade paths.
8. Run Rust unit/integration tests and the SOW-0086 baseline matrix.
9. Record remaining gaps as follow-up SOWs only if they are valid and not fixed
   here.

## Delegation Plan

Implementer:

- Local project-manager work only for the spec and gap analysis. No external
  implementer agents.

Reviewers:

- No external reviewers before the user reviews the gap analysis. If the user
  asks for reviewer input later, reviewers must be read-only and scoped only to
  Rust reader performance.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- Do not make changes outside this repository for any reason.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- If Rust reader evidence exposes a non-Rust issue, record it as out of scope
  and continue.
- If the gap analysis finds a design decision is needed, stop and present
  numbered options to the user before implementation.
- If audit fails, repair only this SOW's SOW/spec/status artifacts.

## Execution Log

### 2026-06-04

- Created this pending Rust-only reader performance SOW from the user's
  narrowed scope instruction.
- Recorded the first-pass Rust reader API taxonomy and low-level hot-path
  findings from a read-only code scan. No implementation changes were made.
- Recaptured the Rust reader baseline after the user reported possible host
  load interference.
- Implemented the first Rust-only performance batch:
  - facade sequence and monotonic metadata now use current-row metadata instead
    of materializing owned `Entry` values;
  - rolling mmap windows now support current-row pins for uncompressed DATA
    payload slices;
  - `FileReader` row DATA enumeration now stores uncompressed payloads as
    row-valid mmap pointers and compressed payloads in a row-level append-only
    arena;
  - standard Zstandard DATA frames now use the native `zstd` fast path, with
    the previous `ruzstd` decoder retained as fallback.
- Re-ran Rust tests and the SOW-0086 benchmark matrix after the implementation
  batch.

## Implementation Results

Implemented fixes:

1. Facade metadata no longer expands full entries.
   - Evidence:
     `rust/src/journal/src/facade.rs:276` delegates `get_seqnum()` to the
     active reader metadata.
     `rust/src/journal/src/facade.rs:284` delegates `get_monotonic_usec()` to
     the active reader metadata.
     `rust/src/journal/src/lib.rs:929` and
     `rust/src/journal/src/directory.rs:291` expose the metadata without
     owned-entry materialization.
   - API shape: the Rust methods and free functions now accept `&SdJournal`
     instead of `&mut SdJournal`. This reduces the required borrow strength and
     remains source-compatible for Rust callers that already hold `&mut`.
   - Result: simple facade metadata is no longer tied to `sdk-entry`.

2. Windowed mmap row-level pins are implemented for uncompressed DATA.
   - Evidence:
     `rust/src/crates/journal-core/src/file/mmap.rs:133` adds row-pinned window
     state.
     `rust/src/crates/journal-core/src/file/mmap.rs:458` clears row pins.
     `rust/src/crates/journal-core/src/file/mmap.rs:468` returns a
     row-pinned slice.
     `rust/src/crates/journal-core/src/file/mmap.rs:489` preserves pinned
     windows while mapping additional row payloads.
   - Result: rolling mmap readers can return direct mmap-backed uncompressed
     payload slices for the current row without copying them.

3. `FileReader` row DATA enumeration uses row-pinned uncompressed payloads and
   a compressed row arena.
   - Evidence:
     `rust/src/journal/src/lib.rs:320` defines borrowed and arena-backed row
     payload descriptors.
     `rust/src/journal/src/lib.rs:609` clears row pins.
     `rust/src/journal/src/lib.rs:624` resets row payload descriptors and the
     compressed row arena.
     `rust/src/journal/src/lib.rs:629` builds the current row payload cache.
     `rust/src/journal/src/lib.rs:685` rebuilds row DATA enumeration state.
     `rust/src/journal/src/lib.rs:708` returns row-valid payload slices.
   - Result: facade-style DATA enumeration has row-level validity for
     uncompressed mmap data and compressed arena data. `FileReader::Drop` now
     also clears row pins before removing temporary `.journal.zst` scratch
     files, even though dropping the window manager would unmap the windows
     immediately afterward.

4. Core DATA payload helpers expose a row-pinned traversal primitive.
   - Evidence:
     `rust/src/crates/journal-core/src/file/file_payload.rs:26` defines
     `RowPinnedPayload`.
     `rust/src/crates/journal-core/src/file/file_payload.rs:269` returns an
     uncompressed row-pinned pointer.
     `rust/src/crates/journal-core/src/file/file_payload.rs:297` fast-paths
     uncompressed row-pinned payloads.
     `rust/src/crates/journal-core/src/file/file_payload.rs:324` visits all row
     payloads with one window-manager access scope.
   - Result: the SDK layer no longer owns the whole uncompressed lifetime
     workaround by itself.

5. Zstandard DATA decompression uses native `zstd` first and preserves
   compatibility fallback.
   - Evidence:
     `rust/src/crates/journal-core/src/file/object_compression.rs:112` reads
     native frame content size.
     `rust/src/crates/journal-core/src/file/object_compression.rs:134` routes
     Zstandard decompression through the native fast path first.
     `rust/src/crates/journal-core/src/file/object_compression.rs:157` retains
     the `ruzstd` streaming fallback.
   - Dependency note: `zstd 0.13.3` is the current crate version observed by
     `cargo search`; it was already present in the Rust workspace dependency
     graph through `journal-engine -> foyer-storage`. This SOW makes
     `journal-core` depend on it directly for reader DATA decompression. The
     project policy explicitly allows common compression-library dependencies
     after dependency review.

Tests added or updated:

- `rust/src/journal/src/tests/facade.rs` now verifies that uncompressed
  windowed facade DATA slices are direct mmap pointers and remain valid while
  later fields in the same row force additional mmap windows.
- `rust/src/journal/src/tests/facade.rs` now also verifies that leaving the
  pressure row clears the current file reader's row-pin state.
- `rust/src/crates/journal-core/src/file/object_compression.rs` now tests the
  native Zstandard path and the `ruzstd` fallback path.

Reviewer-prep dispositions before whole-SOW re-review:

- A row-pinned window that contains a requested start but not the full requested
  range cannot be returned safely; `lookup_window_by_range()` would have matched
  if it covered the full range. The implementation intentionally maps an
  overlapping wider window for the same row rather than remapping the pinned
  window. The spec now records this invariant.
- Row pins may temporarily exceed the normal `max_windows` cache during one
  current row. This is an explicit part of the row-level lifetime contract and
  is bounded by the DATA object spread of that row, not by cross-row growth.
- `RowPinnedPayload` remains public inside `journal-core` because `journal`
  needs it across crate boundaries, but the `journal` crate no longer re-exports
  it as part of its public prelude.
- Reviewer feedback found a misleading row-pinning comment that described a
  row-pinned helper branch as a non-row access. The comment was corrected in
  `rust/src/crates/journal-core/src/file/mmap.rs`.
- Reviewer feedback also found that hostile/corrupt files can force a single
  row to pin many mmap windows. The current behavior is intentional for SOW-0086
  row-level zero-copy validity on normal files, but a hard cap and copy fallback
  are now tracked by pending SOW-0092.

Remaining Rust reader performance gaps:

1. The current row owner is still split between `JournalReader` and
   `FileReader`; a deeper core row-view primitive is still needed.
   - Tracked by pending SOW-0087.
2. Offset-array cursor state still rebuilds/rereads nodes and reverse traversal
   still has node-boundary costs.
   - Tracked by pending SOW-0088.
3. Reusable compressed DATA offsets are still decompressed repeatedly, and the
   native Zstandard context is not reused.
   - Tracked by pending SOW-0089.
4. Header/snapshot state is improved only for facade metadata; broader
   lower-layer header caching and live refresh boundaries still need cleanup.
   - Tracked by pending SOW-0090.
5. Directory, `journal-engine`, and `journal-index` still have duplicated row
   extraction/parsing paths. They should adopt the future core row-view
   primitive after SOW-0087.
   - Tracked by pending SOW-0091.
6. A hostile or corrupt row with DATA objects spread across many windows can
   temporarily exceed the normal rolling-window cache for one row.
   - Tracked by pending SOW-0092.

## Validation

Acceptance criteria evidence:

- Rust-only reader performance spec exists:
  `.agents/sow/specs/rust-reader-performance.md`.
- The active SOW contains a Rust reader API taxonomy, hot-path finding list,
  separation/ownership audit, Netdata vendored Rust comparison, baseline
  benchmark environment, implementation results, and follow-up mapping.
- No Go, Python, Node.js, writer, packaging, release, registry, or Netdata
  integration files were changed by this SOW.
- The Rust row-level guarantee is covered by facade tests for direct mmap
  pointer provenance and survival under rolling-window pressure.
- The current implementation batch improved every measured facade DATA case
  versus systemd DATA enumeration in the final SOW-0086 benchmark run.

Tests or equivalent validation:

- `cargo fmt --all` from `rust/`: passed.
- `cargo test -p journal-core -p journal --target-dir .local/cargo-target`:
  passed. `journal-core` reported 72 passed tests; `journal` reported 26
  passed tests.
- After reviewer-driven row-pin comment/test/follow-up updates,
  `cargo fmt --all && cargo test -p journal-core -p journal --target-dir
  ../.local/cargo-target` from `rust/` passed again. `journal-core` reported
  72 passed tests; `journal` reported 26 passed tests.
- `cargo build --release -p reader_core_bench --manifest-path rust/Cargo.toml
  --target-dir .local/cargo-target`: passed.
- SOW-0086 benchmark matrix:
  `python3 .local/sow-0086/reader-baseline/run_baseline.py`: passed and wrote
  `.local/sow-0086/reader-baseline/report.json` and
  `.local/sow-0086/reader-baseline/report.md`.
- Final benchmark medians, 3 repetitions, 32 MiB reader window:

| Candidate | Rust sdk-payloads rows/s | Rust facade-data rows/s | systemd DATA rows/s | Facade/systemd |
|---|---:|---:|---:|---:|
| real-compressed-multiboot-high-entry | 3,458,741 | 2,596,126 | 777,646 | 3.34x |
| real-compressed-high-cardinality | 3,999,259 | 3,248,324 | 899,581 | 3.61x |
| real-compressed-high-field-count | 824,086 | 717,195 | 460,034 | 1.56x |
| netdata-flow-largest-uncompressed | 1,512,489 | 1,286,078 | 426,785 | 3.01x |
| netdata-flow-most-entries-uncompressed | 1,524,379 | 1,296,100 | 451,205 | 2.87x |
| netdata-flow-online-uncompressed | 1,483,048 | 1,311,319 | 459,229 | 2.86x |
| systemd-matrix-uncompressed-regular | 3,032,304 | 2,434,753 | 1,421,739 | 1.71x |
| systemd-matrix-fss-compact | 3,649,750 | 3,629,594 | 1,516,383 | 2.39x |

- Caveat: the `netdata-flow-online-uncompressed` candidate changed while this
  SOW was active. The final run is valid as a current measurement, but it is not
  directly comparable to the earlier 80 MiB snapshot baseline for that single
  row.

Real-use evidence:

- Real production-style journals were used read-only through sanitized
  candidate labels. Raw local journal paths and payload data remain only in
  `.local/` benchmark artifacts and are not durable committed evidence.
- The final benchmark includes real compressed compact journals, real
  uncompressed Netdata flow journals, and generated FSS/compact coverage.

Reviewer findings:

- First final whole-SOW pass:
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`. Non-blocking notes
    only.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`. Non-blocking
    dependency/portability notes only.
  - `llm-netdata-cloud/kimi-k2.6`: initially `NOT PRODUCTION GRADE` with three
    blockers: redundant post-decompression truncate, missing debug enforcement
    for row-pin reset invariant, and missing spec text for eager
    `entry_data_restart()` caching/transient windows. All three were fixed.
  - `llm-netdata-cloud/minimax-m2.7-coder`: no usable final result; the exact
    stale reviewer PIDs were stopped after later local fixes made the review
    obsolete.
- Rerun after Kimi blockers were fixed:
  - `llm-netdata-cloud/qwen3.6-plus`: `NOT PRODUCTION GRADE` due to a claimed
    row-pinned UAF edge case and a missing hard row-pin bound. The UAF claim was
    dispositioned as a comment/invariant clarity issue because the removed
    window is explicitly checked as not row-pinned before removal. The
    misleading comment was corrected. The hard row-pin bound is real hostile
    input hardening and is tracked by SOW-0092.
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`; it still recommended a
    hostile-file row-pin cap follow-up, a clearer row-pinning comment, and a
    row-advance pin-clear test. The comment and test were added; the cap is
    tracked by SOW-0092.
- Final whole-SOW reviewer rerun after these updates:
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`. Non-blocking findings:
    `visit_entry_payloads()` still uses the older visitor path and should be
    adopted into the future row-view primitive under SOW-0087; a redundant
    `step_valid()` reset is negligible and can be cleaned with adjacent row-view
    work.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`. Verified the native
    zstd fast path after checking the `zstd-safe` `WriteBuf` `Vec<u8>`
    semantics; non-blocking eviction-logic duplication is mapped to SOW-0092.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`. Non-blocking notes only;
    `seek_cursor()` still has a non-hot owned-entry path tracked by later
    reader cleanup.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`. Non-blocking
    dependency/portability notes only.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`. Non-blocking
    note that `clear_row_payload_pins_best_effort()` would leak row pins only
    if the underlying file-access closure failed; this is impractical in the
    current implementation and remains guarded by debug assertions.

Same-failure scan:

- Scope scan via `git status --short` and `git diff --stat` found changes only
  in Rust reader crates, Rust Cargo manifests/lockfile, this SOW, the Rust
  reader performance spec, and SOW status.
- No non-Rust implementation files were changed.

Sensitive data gate:

- Durable SOW/spec/status artifacts contain only sanitized corpus labels,
  aggregate counts, benchmark rates, and repository file references.
- Raw journal paths, raw journal payloads, credentials, bearer tokens, SNMP
  communities, customer names, personal data, non-private customer-identifying
  IPs, private endpoints, and proprietary incident details were not written to
  durable artifacts.

Artifact maintenance gate:

- AGENTS.md: no update in this SOW. The project already contains the broad
  performance contract; the Rust-specific mechanical rules live in the new
  Rust reader performance spec.
- Runtime project skills: no update. The implementation did not change the
  general SOW or compatibility workflow.
- Specs: updated `.agents/sow/specs/rust-reader-performance.md`.
- End-user/operator docs: no update. This SOW changes internal Rust reader
  performance mechanics, not published user-facing SDK usage.
- End-user/operator skills: no update. No output/reference skills were affected.
- SOW lifecycle: SOW completed after reviewer pass, validation, audit, and
  follow-up mapping; follow-up work is mapped to pending SOWs.
- SOW-status.md: updated `.agents/sow/SOW-status.md`.

Specs update:

- Updated `.agents/sow/specs/rust-reader-performance.md` with rolling mmap row
  pin rules, compressed DATA arena expectations, Zstandard fast-path guidance,
  and the non-hot status of owned entry materialization.

Project skills update:

- No project skill update needed. The new durable rule is Rust-specific product
  behavior/performance spec content, not a reusable agent operating procedure.

End-user/operator docs update:

- No end-user/operator docs update needed in this SOW. Public API behavior is
  preserved; the changed behavior is performance/lifetime implementation.

End-user/operator skills update:

- No output/reference skills were affected.

Lessons:

- A semantically correct row-lifetime implementation can still violate the
  performance contract if it copies uncompressed mmap data. Tests now check
  pointer provenance, not only returned bytes.
- The pure-Rust `ruzstd` fallback was correct but too slow for the observed
  high-field-count compressed corpus. Native compression-library use is
  necessary in the Rust reader hot path for this workload.
- A SOW should keep current benchmark reports in one stable table shape; changing
  report formats during performance debugging makes decision-making harder.

Follow-up mapping:

- SOW-0087 tracks the deeper Rust core row-view primitive and current-row
  ownership cleanup.
- SOW-0088 tracks Rust offset-array cursor caching.
- SOW-0089 tracks Rust compressed-DATA reuse and reusable Zstandard context
  experiments.
- SOW-0090 tracks Rust reader header/snapshot cache cleanup.
- SOW-0091 tracks adoption of the future row-view primitive by directory,
  `journal-engine`, and `journal-index`.
- SOW-0092 tracks a hard row-pin bound and copy fallback for hostile or corrupt
  files whose one-row DATA spread would otherwise create excessive transient
  mappings.

## Outcome

Completed. SOW-0086 established the Rust reader performance contract, added the
Rust-specific performance spec, implemented row-level mmap-backed payload
lifetime for uncompressed DATA, added the compressed current-row arena path,
improved facade metadata/data hot paths, added native zstd decompression with
pure fallback, benchmarked the result on representative real and generated
journals, and mapped remaining performance work to concrete pending SOWs.

## Lessons Extracted

- Rust reader hot-path validation must check memory provenance and lifetime, not
  only byte equality.
- Native compression libraries are acceptable and necessary when they are the
  measurable difference between being slower than systemd and materially faster
  than systemd.
- Residual performance debt should be represented as concrete SOW files before
  closing the current performance batch.

## Followup

- `.agents/sow/pending/SOW-0087-20260604-rust-core-row-view-primitive.md`
- `.agents/sow/pending/SOW-0088-20260604-rust-offset-array-cursor-cache.md`
- `.agents/sow/pending/SOW-0089-20260604-rust-compressed-data-reuse.md`
- `.agents/sow/pending/SOW-0090-20260604-rust-reader-header-snapshot-cache.md`
- `.agents/sow/pending/SOW-0091-20260604-rust-row-view-adoption.md`
- `.agents/sow/pending/SOW-0092-20260604-rust-row-pin-hostile-file-bound.md`

## Regression Log

None yet.
