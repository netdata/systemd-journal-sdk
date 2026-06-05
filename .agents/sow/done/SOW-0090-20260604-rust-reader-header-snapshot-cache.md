# SOW-0090 - Rust Reader Header Snapshot Cache

## Status

Status: completed

Sub-state: completed after local validation, benchmark capture, whole-SOW
read-only review, audit, and closeout.

## Requirements

### Purpose

Centralize Rust reader header and snapshot state so hot paths do not reread or
rematerialize immutable header fields during snapshot traversal.

### User Request

The user requires Rust readers to cache the file header and avoid unnecessary
data access in the hot path.

### Acceptance Criteria

- Snapshot readers cache immutable header fields needed by hot row traversal,
  cursor formatting, payload context, and directory ordering.
- Live-reader refresh boundaries are explicit and benchmarked.
- Facade, `FileReader`, and directory reader metadata calls share the cached
  state instead of independently reading/rematerializing header data.
- SOW-0086 benchmark candidates are rerun.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- SOW-0086 found header access in multiple layers. The first implementation
  batch fixed facade metadata materialization, but broader header ownership
  remains unclear.
- The public Rust reader currently rematerializes `FileHeader` from
  `JournalFile::journal_header_ref()` on every `FileReader::header()` call.
- Directory open and directory metadata paths call `header()` repeatedly for
  file sort, boot newest calculation, boot listing, and non-overlap checks.
- `journal-core::JournalFile` is also used by mutable writer paths, so changing
  `journal_header_ref()` itself into a cached immutable snapshot would risk
  stale writer metadata. The safe layer for this SOW is the read-only
  `FileReader`.

Evidence reviewed:

- SOW-0086 findings and implementation results.
- SOW-0089 final benchmark report, used as the before baseline because SOW-0089
  left production reader paths unchanged.
- `rust/src/journal/src/lib.rs`: `FileReader::header()` copies header fields
  from `journal_header_ref()` on demand; `current_directory_entry_key()` falls
  back to `journal_header_ref().seqnum_id` when current-row metadata is absent.
- `rust/src/journal/src/directory.rs`: `from_readers()` sorts by
  `FileReader::header_realtime_start()`, `list_boots()` calls `reader.header()`
  for every file, `build_directory_boot_newest()` reads the underlying
  `JournalFile` header directly, and `directory_files_non_overlapping()` calls
  `header()` for every adjacent pair.
- `rust/src/journal/src/facade.rs`: file-backed `list_boots()` calls
  `reader.header()`.
- `rust/src/crates/journal-core/src/file/file.rs`: `JournalFile` owns the
  persistent header map and mutable writers update the mapped header, so
  reader-only snapshot caching must not change writer-facing
  `journal_header_ref()`.

Affected contracts and surfaces:

- Rust reader metadata, cursor formatting, directory ordering, and live/snapshot
  behavior.
- Public `FileReader::header()` and directory/facade boot metadata behavior.
- Internal directory ordering and sequential fast-path detection.

Existing patterns to reuse:

- Existing `DirectoryEntryKey`, `FileHeader`, and reader options.
- Current row metadata fast path from SOW-0087: current-row entry metadata is
  already cached and should remain the source for per-row cursor/timestamp
  calls after `next()`/`previous()`.

Risk and blast radius:

- Medium: live readers must not cache stale mutable tail data beyond the stated
  bounds contract.
- Low-to-medium: caching open-time header fields can accidentally change
  `FileReader::header()` semantics for live readers if the public method stops
  refreshing mutable tail fields. Keep public live header refresh explicit and
  use cached snapshot only where snapshot/open-time semantics are sufficient.
- Low: directory setup operates over open-time file snapshots and benefits from
  cached header metadata.

Sensitive data handling plan:

- No raw journal payloads in durable artifacts.

Implementation plan:

1. Add a private read-only header snapshot type in the Rust SDK layer with the
   public `FileHeader` fields plus internal directory fields such as
   `machine_id` and `tail_entry_monotonic`.
2. Capture the snapshot once when `FileReader` opens a file.
3. Keep `FileReader::header()` public semantics by refreshing from the mapped
   header only for `ReaderBounds::Live`; return the cached snapshot for
   `ReaderBounds::Snapshot`.
4. Route directory ordering, boot newest calculation, boot listing, and
   fallback cursor key construction through the cached snapshot.
5. Avoid changing `journal-core::JournalFile::journal_header_ref()` because
   mutable writer paths require current mapped header state.
6. Rerun Rust tests and the SOW-0086 large-file benchmark matrix.

Validation plan:

- Rust tests, snapshot/live header behavior tests if needed, SOW-0086 benchmark
  matrix, `git diff --check`, SOW audit, and whole-SOW read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected.
- Specs: update Rust reader performance spec if live/cache rules change or if
  cached snapshot semantics need to be made explicit.
- End-user/operator docs: likely unaffected.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: normal pending/current/done flow.
- SOW-status.md: update when state changes.

Open-source reference evidence:

- Not required for this SOW. The change is internal Rust SDK header ownership
  and does not alter journal file-format compatibility.

Open decisions:

- User approved proceeding through SOW-0087 to SOW-0092 on 2026-06-05, with a
  performance improvement review after each SOW and immediate continuation to
  the next SOW.
- No public API shape change is approved. Additive private implementation
  details are allowed; public `FileReader::header()` must keep compatible
  behavior.

## Outcome

Completed.

Rust `FileReader` now captures read-only header snapshot metadata at open time.
Snapshot readers use the cached header, live public `header()` refreshes from
the mapped header, and directory/facade metadata paths share the cached
snapshot. `journal-core::JournalFile::journal_header_ref()` remains unchanged
because writer-visible mutable header behavior must stay live.

## Local Validation

Implementation summary:

- Added private `FileHeaderSnapshot` in the Rust SDK layer.
- `FileReader` captures `FileHeaderSnapshot` once at open.
- `FileReader::header()` returns the cached header for
  `ReaderBounds::Snapshot` and explicitly refreshes from the mapped header for
  `ReaderBounds::Live`.
- Directory ordering, directory boot-newest metadata, directory boot listing,
  facade file boot listing, fallback cursor construction, and fallback
  directory-entry-key construction now use the cached snapshot instead of
  rematerializing header data.
- `journal-core::JournalFile::journal_header_ref()` was not changed because
  mutable writer paths need current mapped header state.

Live/snapshot boundary validation:

- Added `snapshot_header_is_fixed_while_live_header_refreshes`.
- The test opens snapshot and live readers after one entry, appends a second
  entry with the writer, syncs, then verifies:
  - snapshot `header().tail_entry_seqnum` remains `1`;
  - live `header().tail_entry_seqnum` refreshes to `2`.

Validation commands:

- `cargo fmt --manifest-path Cargo.toml -p journal` passed from `rust/`.
- `cargo test -p journal-core -p journal --target-dir ../.local/cargo-target`
  passed from `rust/`: 72 `journal-core` tests, 28 `journal` tests, and doctest
  checks passed.
- `cargo build --release --manifest-path rust/Cargo.toml -p reader_core_bench
  --target-dir .local/cargo-target` passed.
- `.local/sow-0086/reader-baseline/run_baseline.py` was run twice after the
  change. The second run is recorded below.

Final standard benchmark, compared with the SOW-0089 report:

| candidate | mode | SOW-0089 rows/s | SOW-0090 rows/s | delta |
|---|---|---:|---:|---:|
| `real-compressed-multiboot-high-entry` | `core-payloads` | 1,754,319 | 1,736,053 | -1.0% |
| `real-compressed-multiboot-high-entry` | `sdk-payloads` | 2,992,329 | 3,265,738 | +9.1% |
| `real-compressed-multiboot-high-entry` | `facade-data` | 2,869,075 | 2,786,958 | -2.9% |
| `real-compressed-high-cardinality` | `core-payloads` | 2,412,040 | 2,384,793 | -1.1% |
| `real-compressed-high-cardinality` | `sdk-payloads` | 4,250,795 | 3,876,293 | -8.8% |
| `real-compressed-high-cardinality` | `facade-data` | 3,484,704 | 3,263,432 | -6.3% |
| `real-compressed-high-field-count` | `core-payloads` | 659,684 | 609,530 | -7.6% |
| `real-compressed-high-field-count` | `sdk-payloads` | 800,630 | 733,112 | -8.4% |
| `real-compressed-high-field-count` | `facade-data` | 688,144 | 679,843 | -1.2% |
| `netdata-flow-largest-uncompressed` | `core-payloads` | 845,479 | 871,671 | +3.1% |
| `netdata-flow-largest-uncompressed` | `sdk-payloads` | 1,435,182 | 1,491,319 | +3.9% |
| `netdata-flow-largest-uncompressed` | `facade-data` | 1,269,973 | 1,372,708 | +8.1% |
| `netdata-flow-most-entries-uncompressed` | `core-payloads` | 868,458 | 938,895 | +8.1% |
| `netdata-flow-most-entries-uncompressed` | `sdk-payloads` | 1,343,025 | 1,526,228 | +13.6% |
| `netdata-flow-most-entries-uncompressed` | `facade-data` | 1,314,840 | 1,320,277 | +0.4% |
| `netdata-flow-online-uncompressed` | `core-payloads` | 931,983 | 753,034 | -19.2% |
| `netdata-flow-online-uncompressed` | `sdk-payloads` | 1,548,942 | 1,197,765 | -22.7% |
| `netdata-flow-online-uncompressed` | `facade-data` | 1,219,732 | 1,228,804 | +0.7% |

Benchmark interpretation:

- The change affects header snapshot ownership, directory metadata, facade boot
  metadata, and cursor fallback construction.
- The file-mode payload hot loops do not call `FileReader::header()` and are
  not expected to move materially from this SOW.
- Two post-change benchmark runs showed significant host variance in unrelated
  `sdk-payloads` and `core-payloads` modes. Therefore no broad file hot-loop
  speedup is claimed for SOW-0090.
- `facade-data` on the Netdata flow files improved from +0.7% to +8.1% in the
  final run, but compressed-file facade changes were mixed. Treat this as
  directional only.
- The accepted result is a performance-contract cleanup: repeated SDK-level
  header rematerialization is removed from snapshot/directory/facade metadata
  paths, while live public header refresh remains explicit.

Sensitive data gate:

- Durable SOW evidence uses only sanitized benchmark labels and aggregate
  counts/rates.
- Raw local journal paths remain only in `.local/` benchmark reports.

Artifact maintenance gate:

- `AGENTS.md`: no change needed; no project-wide workflow changed.
- Runtime project skills: no change needed; no agent workflow changed.
- Specs: `.agents/sow/specs/rust-reader-performance.md` updated to make the
  cached snapshot/live-refresh header boundary explicit.
- End-user/operator docs: no change needed; no public SDK API shape changed.
- End-user/operator skills: no change needed; no operator workflow changed.
- SOW lifecycle: SOW moved from `pending/` to `current/` for implementation and
  to `done/` during closeout.
- `.agents/sow/SOW-status.md`: updated for active SOW-0090 and closeout.

Same-failure search:

- `rg -n "journal_header_ref\\(|header\\(\\)|cached_header\\(|get_seqnum\\("
  rust/src/journal/src rust/src/crates/journal-core/src/file/file_payload.rs
  rust/src/crates/journal-core/src/file/cursor.rs
  rust/src/crates/journal-core/src/file/reader.rs` was used to inspect
  remaining header accesses.
- Remaining `journal-core` header reads are left unchanged because
  `JournalFile` is shared by read-only and mutable writer paths.
- `DataPayloadReadContext` still reads the header once per row load; this is
  not converted to a global `JournalFile` cache in this SOW because doing so at
  `journal-core` level would risk stale writer-visible metadata. Further row
  view adoption is tracked by SOW-0091.

Reviewer findings:

- `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  - Verified `journal-core` was unchanged, snapshot/live header semantics were
    correct, directory/facade metadata used cached snapshots, local Rust tests
    passed, audit passed, and benchmark claims were not overclaimed.
- `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  - Verified the same scope and noted one low observation: `live_header()`
    constructs a temporary `FileHeaderSnapshot` only to return `header`.
    Disposition: accepted as non-blocking because this public live header call
    is not in the traversal hot path; changing it would not affect the SOW's
    measured performance contract.
- `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  - Verified semantic equivalence for cursor construction, explicit
    live/snapshot boundary, unchanged `journal-core`, and clean SOW lifecycle
    state.
- `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
  - Verified private SDK-layer snapshot ownership, preserved public live
    refresh, unchanged writer-visible core behavior, and no security findings.
- `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
  - Verified directory/facade/metadata paths and noted one low observation:
    live `list_boots()` now uses open-time metadata. Disposition: accepted as
    non-blocking because boot listing is a setup/metadata operation and the
    public live `header()` refresh boundary remains explicit and tested.

Reviewer conclusion:

- No reviewer voted against production grade.
- No blocking findings remained.
- Low observations were dispositioned as non-hot-path behavior or acceptable
  setup-time metadata semantics.

Follow-up mapping:

- SOW-0091 remains the tracked follow-up for broader row-view adoption and
  remaining reader helper paths.
- SOW-0092 remains the tracked follow-up for hostile/corrupt row-pin bounds.
