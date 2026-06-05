# SOW-0091 - Rust Row View Adoption

## Status

Status: completed

Sub-state: implemented, locally validated, reviewed, and closed.

## Requirements

### Purpose

Adopt the future Rust core row-view primitive across directory reading,
`FileReader` callback payload traversal, `journal-engine`, and
`journal-index` so performance fixes do not remain limited to the
single-payload facade enumeration path.

### User Request

The user requires no duplicated hot-path row extraction, parsing,
decompression, or allocation logic when a shared Rust reader primitive can serve
the same purpose.

### Acceptance Criteria

- Directory reader payload access delegates to the shared row-view primitive.
- `FileReader::visit_entry_payloads()` either uses the row-pinned row-view
  payload path or records benchmark evidence proving the transient visitor path
  is intentionally faster for its contract.
- `journal-engine` projected field extraction delegates to the shared row-view
  primitive or records why its path is intentionally separate and faster.
- `journal-index` DATA extraction/parsing delegates to the shared byte-oriented
  primitive or records why its path is intentionally separate and faster.
- Existing Netdata-style benchmark candidates and query/index tests are rerun.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- SOW-0087 created `journal-core::file::CurrentRowView` and routed Rust facade
  DATA enumeration through it, but several row-oriented readers still use older
  ENTRY/DATA loops.
- `FileReader::visit_entry_payloads()` still calls
  `CurrentRowView::visit_payload_at_transient()` per DATA offset. That path
  borrows only for the callback and does not use the row-pinned/row-arena
  enumeration contract used by facade DATA.
- `FileReader::get_entry()` still calls `reader_helpers::read_entry_at()`,
  which recollects ENTRY DATA offsets and loops over `file.data_ref()` instead
  of materializing the owned entry from the current row view.
- `journal-engine` projected field extraction duplicates row offset collection,
  DATA loading, decompression, UTF-8 conversion, and `FieldValuePair` parsing in
  `logs/query.rs`.
- `journal-index` query-time source timestamp extraction and regex matching
  duplicate row offset collection and DATA loading in `file_index.rs`.
- `journal-index` index construction also walks FIELD DATA chains in
  `file_indexer.rs`; that path is intentionally separate because FIELD/DATA
  chain traversal is the journal-native indexed path for field-value indexing,
  not a row traversal path.

Evidence reviewed:

- SOW-0086 separation, ownership, and duplication audit.
- SOW-0087 outcome and implementation:
  - `rust/src/crates/journal-core/src/file/row_view.rs` defines
    `CurrentRowView`, current-row metadata, DATA offsets, row pins,
    decompression scratch, and compressed row arena.
  - `rust/src/journal/src/lib.rs` already stores one `CurrentRowView` per
    `FileReader`.
- SOW-0090 outcome and implementation:
  - `rust/src/journal/src/lib.rs` now owns cached read-only header snapshot
    metadata in `FileReader`; `journal-core` header behavior remains writer
    visible and unchanged.
- Current duplication evidence:
  - `rust/src/journal/src/lib.rs`: `visit_entry_payloads()` loops over
    `row.data_offset_at()` and calls `visit_payload_at_transient()`.
  - `rust/src/journal/src/reader_helpers.rs`: `read_entry_at()` calls
    `collect_entry_metadata_and_data_offsets()`, then `file.data_ref()` and
    `data.decompress()`/`data.raw_payload()` for every DATA object.
  - `rust/src/crates/journal-engine/src/logs/query.rs`: `read_entry_fields()`
    and `read_projected_pair()` recollect offsets, call `data_ref()`, decompress
    DATA, convert payloads with `String::from_utf8_lossy()`, and parse
    `FieldValuePair`.
  - `rust/src/crates/journal-index/src/file_index.rs`: `get_timestamp_field()`
    uses `entry_data_objects()` and `parse_timestamp()` per DATA object, while
    `entry_matches_regex()` recollects offsets and calls `data_ref()` and
    `decompress()` itself.
  - `rust/src/crates/journal-index/src/file_indexer.rs` uses
    `field_data_objects()` for bitmap indexing; this is journal-native indexed
    traversal and should not be replaced by row scanning.

Affected contracts and surfaces:

- Rust directory reader, indexed query engine, journal indexer, and future
  explorer API work.
- Rust SDK `FileReader::visit_entry_payloads()`,
  `FileReader::collect_entry_payloads()`, `FileReader::get_entry_payload()`,
  `FileReader::get_entry()`, and the `DirectoryReader`/facade methods layered
  on top of them.
- `journal-index` query-time timestamp and regex matching.
- `journal-engine` projected field extraction and pagination output.

Existing patterns to reuse:

- SOW-0087 `CurrentRowView` row-level metadata, DATA offsets, row pins,
  compressed row arena, and row reset rules.
- SOW-0086/SOW-0087 facade pointer-provenance tests for row-level validity.
- SOW-0090 cached header snapshot for immutable file metadata in SDK-level
  cursor construction.

Risk and blast radius:

- High: this touches multiple Rust crates and could affect query/index
  correctness and performance.
- Medium: making callback payload traversal use row-pinned storage can keep
  per-row pins/arena active for longer than the old transient visitor path. It
  must clear pins and arena before row advance or explicit data-state reset.
- Medium: engine/index currently use owned `FieldValuePair` and UTF-8 parsing;
  adopting byte-oriented helpers must preserve current invalid/non-UTF-8
  behavior.
- Low: directory reader should remain a thin delegating layer to `FileReader`.

Sensitive data handling plan:

- Use generated fixtures and sanitized benchmark/query labels only.

Implementation plan:

1. Extend `CurrentRowView` with row-pinned payload access by DATA offset and a
   row-payload visitor that can return DATA offsets to callers that cache by
   offset.
2. Route `FileReader::visit_entry_payloads()` through the row-pinned row-view
   path and clear the row data state after the callback visitor completes.
3. Route `FileReader::get_entry()` owned materialization through the current row
   view to avoid recollecting ENTRY offsets and to keep one payload access
   implementation.
4. Add byte-oriented `FieldValuePair` and timestamp parsing helpers so
   engine/index callers do not need `String::from_utf8_lossy()` before deciding
   whether a payload is relevant.
5. Route `journal-engine` projected field extraction through `CurrentRowView`
   and byte-oriented parsing.
6. Route `journal-index` query-time timestamp extraction and regex matching
   through `CurrentRowView`, keeping DATA-offset regex cache behavior.
7. Keep `journal-index` field-chain index construction on
   `field_data_objects()` and record why it remains intentionally separate.
8. Rerun Rust tests and the SOW-0086 reader benchmark matrix.

Validation plan:

- Rust tests, query/index tests, reader benchmarks, SOW-0086 candidate matrix,
  `git diff --check`, SOW audit, and whole-SOW read-only reviewer pass.
- Specifically run `cargo test -p journal-core -p journal -p journal-index
  -p journal-engine`.
- Use the standard large-file reader baseline harness for performance
  comparison against SOW-0090. Tiny generated fixtures may run for correctness
  but are not accepted as performance evidence.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected.
- Specs: update Rust reader performance spec if shared primitive adoption
  changes durable guarantees.
- End-user/operator docs: likely unaffected unless public query API behavior
  changes.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: normal pending/current/done flow.
- SOW-status.md: update when state changes.

Open-source reference evidence:

- Not required for this SOW. The work changes internal Rust row ownership and
  helper reuse. File-format compatibility remains governed by existing
  conformance and systemd-compatible reader tests.

Open decisions:

- User approved proceeding through SOW-0087 to SOW-0092 on 2026-06-05, with a
  performance improvement review after each SOW and immediate continuation to
  the next SOW.
- Public API shape changes are not approved. If a public API change becomes
  necessary, stop and ask before implementing it.

## Outcome

Local implementation is complete.

Implemented changes:

- Added offset-aware row payload access in
  `rust/src/crates/journal-core/src/file/row_view.rs` with
  `read_next_payload_with_offset()` and `read_payload_at()`.
- Kept the payload-only `read_next_payload()` body direct and inline because it
  is the hottest facade enumeration path. A helper-only version was tested and
  showed avoidable facade-data noise/regression on large Netdata flow files.
- Routed `FileReader::visit_entry_payloads()` through the row-pinned row-view
  path instead of the removed transient payload visitor, including explicit row
  data cleanup when the visitor returns an error.
- Routed `FileReader::get_entry()` through current-row materialization instead
  of recollecting ENTRY DATA offsets through `read_entry_at()`.
- Added byte-oriented field-value parsing and timestamp parsing helpers in
  `journal-index`.
- Routed `journal-engine` projected field extraction through
  `CurrentRowView`, including a byte prefilter that avoids lossy value parsing
  when the projected UTF-8 field name does not match. Valid UTF-8 payloads use
  the byte parser; invalid UTF-8 payloads keep the legacy lossy fallback.
- Routed `journal-index` query-time timestamp extraction and regex matching
  through `CurrentRowView`, preserving DATA-offset regex cache behavior.
- Kept `journal-index` FIELD-chain index construction on
  `field_data_objects()`, because that is the journal-native indexed path for
  FIELD/DATA chains and is not row traversal.

Local validation:

- `cargo fmt --manifest-path rust/Cargo.toml -p journal-core -p journal-index
  -p journal-engine -p journal`: passed.
- `cargo test --manifest-path rust/Cargo.toml -p journal-core -p journal -p
  journal-index -p journal-engine --target-dir .local/cargo-target`: passed,
  including `visit_entry_payloads_clears_row_pins_when_visitor_returns_error`.
- `cargo build --release --manifest-path rust/Cargo.toml -p
  reader_core_bench --target-dir .local/cargo-target`: passed.
- `python3 .local/sow-0086/reader-baseline/run_baseline.py`: passed and wrote
  `.local/sow-0086/reader-baseline/report.json`.
- Same-failure search for removed duplicate helpers returned no remaining hits
  in the changed reader paths:
  `payload_work_buffers_mut`, `visit_payload_at_transient`, `read_entry_at`,
  `collect_entry_metadata_and_data_offsets`, and
  `collect_offsets_from_entry_items`.
- Final same-failure search for the same helper names returned no remaining
  hits in `rust/src`.
- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed with clean verdict.

Performance review against SOW-0090:

| candidate | mode | SOW-0090 rows/s | SOW-0091 final rows/s | delta |
|---|---|---:|---:|---:|
| `real-compressed-multiboot-high-entry` | `core-payloads` | 1,736,053 | 1,739,585 | +0.2% |
| `real-compressed-multiboot-high-entry` | `sdk-payloads` | 3,265,738 | 3,451,986 | +5.7% |
| `real-compressed-multiboot-high-entry` | `sdk-entry` | 163,562 | 178,509 | +9.1% |
| `real-compressed-multiboot-high-entry` | `facade-data` | 2,786,958 | 2,743,878 | -1.5% |
| `real-compressed-high-cardinality` | `core-payloads` | 2,384,793 | 2,434,538 | +2.1% |
| `real-compressed-high-cardinality` | `sdk-payloads` | 3,876,293 | 4,242,277 | +9.4% |
| `real-compressed-high-cardinality` | `sdk-entry` | 235,687 | 257,186 | +9.1% |
| `real-compressed-high-cardinality` | `facade-data` | 3,263,432 | 3,351,706 | +2.7% |
| `real-compressed-high-field-count` | `core-payloads` | 609,530 | 634,562 | +4.1% |
| `real-compressed-high-field-count` | `sdk-payloads` | 733,112 | 771,728 | +5.3% |
| `real-compressed-high-field-count` | `sdk-entry` | 96,094 | 120,252 | +25.1% |
| `real-compressed-high-field-count` | `facade-data` | 679,843 | 704,215 | +3.6% |
| `netdata-flow-largest-uncompressed` | `core-payloads` | 871,671 | 845,450 | -3.0% |
| `netdata-flow-largest-uncompressed` | `sdk-payloads` | 1,491,319 | 1,617,312 | +8.4% |
| `netdata-flow-largest-uncompressed` | `sdk-entry` | 77,350 | 79,871 | +3.3% |
| `netdata-flow-largest-uncompressed` | `facade-data` | 1,372,708 | 1,250,754 | -8.9% |
| `netdata-flow-most-entries-uncompressed` | `core-payloads` | 938,895 | 909,240 | -3.2% |
| `netdata-flow-most-entries-uncompressed` | `sdk-payloads` | 1,526,228 | 1,687,886 | +10.6% |
| `netdata-flow-most-entries-uncompressed` | `sdk-entry` | 73,655 | 79,584 | +8.0% |
| `netdata-flow-most-entries-uncompressed` | `facade-data` | 1,320,277 | 1,308,401 | -0.9% |
| `netdata-flow-online-uncompressed` | `core-payloads` | 753,034 | 905,066 | +20.2% |
| `netdata-flow-online-uncompressed` | `sdk-payloads` | 1,197,765 | 1,626,422 | +35.8% |
| `netdata-flow-online-uncompressed` | `sdk-entry` | 75,356 | 78,954 | +4.8% |
| `netdata-flow-online-uncompressed` | `facade-data` | 1,228,804 | 1,324,597 | +7.8% |

Mode medians:

- `core-payloads`: +1.1% median, -3.2% min, +20.2% max.
- `sdk-payloads`: +8.9% median, +5.3% min, +35.8% max.
- `sdk-entry`: +8.6% median, +3.3% min, +25.1% max.
- `facade-data`: +0.9% median, -8.9% min, +7.8% max.

Interpretation:

- The main SOW-0091 affected SDK paths improved: `sdk-payloads` and
  `sdk-entry` both improved on every large-file candidate.
- `core-payloads` is a lower-level baseline not expected to improve from this
  SOW, and its mixed deltas are treated as measurement noise.
- `facade-data` is mixed and near flat on median. The payload-only facade path
  was kept as a direct inline hot path to avoid helper overhead while the new
  offset-aware row-view path serves engine/index users.
- The largest apparent `facade-data` regression was retested with an isolated
  A/B run using a clean SOW-0090 `HEAD` binary built under `.local` and the
  final SOW-0091 binary on the same candidate. Median rows/s were 1,231,400 for
  SOW-0090 and 1,229,610 for SOW-0091, or -0.1%. The historical -8.9% row in
  the full matrix is therefore run-to-run drift, not a reproduced code
  regression.

Artifact maintenance gate:

- `AGENTS.md`: no update needed; no project-wide workflow or responsibility
  change.
- Runtime project skills: no update needed; no HOW-to-work-here change.
- Specs: no update needed; public reader guarantees and file-format contracts
  did not change.
- End-user/operator docs: no update needed; public API shape did not change.
- End-user/operator skills: no update needed.
- SOW lifecycle: SOW moved from pending to current, then completed and moved to
  done with this implementation chunk.
- `.agents/sow/SOW-status.md`: updated for completed SOW-0091 state.

Sensitive data gate:

- Durable artifacts contain only sanitized benchmark labels and relative code
  paths. Raw workstation journal paths remain only in `.local` benchmark
  reports and were not copied here.

Reviewer findings:

- First reviewer round:
  - `llm-netdata-cloud/kimi-k2.6`: PRODUCTION GRADE: NO.
    - Finding: `FileReader::visit_entry_payloads()` did not clear row data
      state when the visitor callback returned an error.
    - Disposition: fixed by explicitly resetting row data state before
      propagating visitor errors, and added
      `visit_entry_payloads_clears_row_pins_when_visitor_returns_error`.
    - Finding: `FieldValuePair::parse_bytes()` had no production caller.
    - Disposition: fixed by using `parse_bytes()` in the `journal-engine`
      projection path before the legacy lossy fallback.
  - `llm-netdata-cloud/qwen3.6-plus`: PRODUCTION GRADE: NO.
    - Finding: claimed closure early returns in index/query helpers bypassed
      post-closure cleanup.
    - Disposition: rejected. In Rust, `return` inside those closure bodies
      returns from the closure; the subsequent `row.reset_data_state(...)`
      statement still runs. The real visitor-error cleanup issue was covered
      by the Kimi finding and fixed.
  - `llm-netdata-cloud/glm-5.1`: PRODUCTION GRADE: YES.
  - `llm-netdata-cloud/mimo-v2.5-pro`: PRODUCTION GRADE: YES.
  - `llm-netdata-cloud/minimax-m2.7-coder`: first run did not produce a final
    captured verdict and is being rerun with the full scope.
- Second whole-SOW reviewer round after fixes:
  - `llm-netdata-cloud/kimi-k2.6`: PRODUCTION GRADE: YES.
    - Finding: isolated A/B check requested for the largest `facade-data`
      negative historical delta.
    - Disposition: performed. Same-condition SOW-0090 vs SOW-0091 medians
      were 1,231,400 vs 1,229,610 rows/s, or -0.1%.
  - `llm-netdata-cloud/qwen3.6-plus`: PRODUCTION GRADE: YES.
    - Finding: post-fix `git diff --check` and SOW audit results should be
      recorded before close.
    - Disposition: fixed. `git diff --check` and `.agents/sow/audit.sh`
      passed before close.
  - `llm-netdata-cloud/glm-5.1`: PRODUCTION GRADE: YES.
    - Findings were low-risk hygiene/coverage notes, no blocker.
  - `llm-netdata-cloud/minimax-m2.7-coder`: PRODUCTION GRADE: YES.
    - Findings were low-risk informational notes, no blocker.
  - `llm-netdata-cloud/mimo-v2.5-pro`: PRODUCTION GRADE: YES.
    - Findings were low-risk informational notes, no blocker.

Follow-up mapping:

- SOW-0092 remains the next tracked reader-performance SOW for hostile-file row
  pin bounds.
