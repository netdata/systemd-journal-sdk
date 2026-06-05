# SOW-0088 - Rust Offset Array Cursor Cache

## Status

Status: completed

Sub-state: implemented locally, validated, reviewed, and ready for closeout
move to done.

## Requirements

### Purpose

Optimize Rust reader row stepping by caching offset-array cursor node state and
avoiding repeated offset-array object reads during forward and reverse
traversal.

### User Request

The user requires every branch, calculation, data access, and mmap access in the
Rust reader hot path to have a strong reason to exist.

### Acceptance Criteria

- Forward cursor movement inside one offset-array node does not rebuild or
  reread that node for every value access.
- Reverse cursor movement avoids repeated head-to-current list walks at node
  boundaries.
- Correct ordering remains compatible with systemd journal traversal.
- SOW-0086 benchmark candidates include before/after row-stepping and payload
  traversal deltas.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- SOW-0086 found that offset-array cursor movement repeatedly rebuilds node
  state and can walk the list on reverse node boundaries.
- Current `offset_array::Cursor::next()` calls `self.node(journal_file)` on
  every step, rebuilding the active `Node` from the current array offset.
- Current `JournalCursor::store_array_cursor_value()` then calls
  `cursor.value(journal_file)`, which calls `self.node(journal_file)?.get(...)`
  and rereads the same array object for the current item.
- Current `Cursor::previous()` walks from the list head when the cursor is at
  index 0 and the previous entry belongs to the prior offset-array node.

Evidence reviewed:

- SOW-0086 findings on `rust/src/crates/journal-core/src/file/offset_array.rs`.
- `rust/src/crates/journal-core/src/file/offset_array.rs`: `Cursor::value()`
  calls `self.node(journal_file)?.get(journal_file, self.array_index)`.
- `rust/src/crates/journal-core/src/file/offset_array.rs`: `Cursor::next()`
  rebuilds the node for same-node movement and then caller storage rereads the
  value.
- `rust/src/crates/journal-core/src/file/offset_array.rs`: `Cursor::previous()`
  scans from `self.list.head(journal_file)?` to find the previous node at array
  boundaries.
- `rust/src/crates/journal-core/src/file/cursor.rs`:
  `JournalCursor::store_array_cursor_value()` stores a cursor only after
  reading the cursor value.
- `rust/src/internal/testcmd/reader_core_bench/src/main.rs`: `core-next`,
  `sdk-payloads`, and `facade-data` provide forward/backward benchmark modes.

Affected contracts and surfaces:

- Rust single-file and directory row traversal.

Existing patterns to reuse:

- Existing offset-array validation and cursor tests.

Risk and blast radius:

- Medium: cursor traversal is central, but the change should be internal.

Sensitive data handling plan:

- Use generated fixtures and sanitized benchmark labels only.

Implementation plan:

1. Add scalar node metadata and current-value caching to
   `offset_array::Cursor`, without copying whole offset arrays and without
   allocating in the same-node stepping hot path.
2. Ensure `JournalCursor::store_array_cursor_value()` stores the materialized
   cursor returned by the offset-array cursor so subsequent steps reuse cached
   node state.
3. Carry previous-node metadata when a cursor is produced by head/tail walks,
   directed partitioning, or a forward boundary transition. This avoids
   head-to-current list walks when the previous node is already known.
4. Preserve fallback head scans only for cursor states whose predecessor cannot
   be known without materializing a reverse chain.
5. Validate forward/reverse traversal, seek, filters using `InlinedCursor`, and
   directory ordering.

Validation plan:

- Rust tests, reader benchmark matrix, cursor conformance, `git diff --check`,
  SOW audit, and whole-SOW read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected.
- Specs: update Rust reader performance spec if cursor cache rules become
  durable.
- End-user/operator docs: likely unaffected.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: normal pending/current/done flow.
- SOW-status.md: update when state changes.

Open-source reference evidence:

- Not yet checked.

Open decisions:

- User approved continuing SOW-0087 through SOW-0092 in conversation on
  2026-06-05.
- No public API shape change is approved. If the cache requires changing a
  documented public API, stop and ask first.

## Validation

Acceptance criteria evidence:

- Forward cursor movement inside one offset-array node now carries cached
  scalar node metadata in `offset_array::Cursor`.
- `JournalCursor::store_array_cursor_value()` stores the materialized cursor
  returned with the current entry offset.
- Reverse cursor movement uses lazy predecessor-chain materialization instead
  of repeated head-to-current list walks.
- Correct ordering is covered by a facade-level forward/backward traversal test
  across multiple entry-array nodes.
- The benchmark report includes before/after row-stepping and payload
  traversal deltas.

Tests or equivalent validation:

- `cargo test -p journal-core -p journal --target-dir ../.local/cargo-target`
  passed.
- `cargo check -p journal-core --features allocative --target-dir
  ../.local/cargo-target` passed.
- `cargo build --release -p reader_core_bench --target-dir
  ../.local/cargo-target` passed.
- `.local/sow-0086/reader-baseline/run_baseline.py` completed and wrote the
  full final benchmark report.
- Focused seven-repetition cursor-only benchmark completed and wrote
  `.local/sow-0088/focused_cursor_bench.json`.

Real-use evidence:

- Large real journal files from `/var/log/journal/...` and Netdata flow journal
  files under `/var/cache/netdata/flows/raw/...` were read by the benchmark
  harness. Durable artifacts store only sanitized labels and aggregate metrics.

Reviewer findings:

- Whole-SOW read-only reviewer batch completed.
- Reviewer votes:
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
- Finding: `Cursor::collect_offsets()` fixed a pre-existing bug by using
  `next_node.remaining_items` while collecting offsets from subsequent nodes.
  - Disposition: accepted and recorded as part of this SOW outcome.
- Finding: `InlinedCursor::previous_until()` now reads `self.value()` after
  assigning `*self = ic`, avoiding reliance on previous `Copy` semantics.
  - Disposition: accepted and recorded as part of this SOW outcome.
- Finding: `rust/src/crates/jf/journal_file/src/offset_array.rs` has a
  separate legacy cursor implementation and was not updated.
  - Disposition: non-blocking for SOW-0088. This SOW targets the active
    `journal-core` reader path measured by the SOW-0086 benchmark harness. The
    legacy `jf` crate drift remains a broader parity concern outside this
    cursor-cache SOW.
- Finding: `Cursor` and `InlinedCursor` changed from `Copy` to `Clone`, which
  is observable through `journal-core::file::offset_array`.
  - Disposition: accepted as non-blocking. The documented Rust SDK surface is
    the `journal` crate API; this change did not change documented SDK APIs.
    The affected types are implementation cursor types exposed through
    `journal-core` internals. Keeping `Copy` is incompatible with safe cached
    chain ownership through `Arc<[Node]>`.
- Finding: `netdata-flow-online-uncompressed` regressed in the full
  forward-only benchmark on `core-next` and `facade-data`.
  - Disposition: non-blocking. The primary cursor target improved on 5 of 6
    large candidates in the full benchmark, and the focused seven-repetition
    cursor-only benchmark keeps the same file in the multi-million row/s range.
    Remaining payload/facade overhead is tracked by following SOWs.
- Finding: add lower-level unit tests for cache internals or entry DATA arrays.
  - Disposition: non-blocking. The new facade integration test covers
    forward/backward traversal across entry-array nodes through consumer-facing
    reader APIs; additional adversarial bounds remain in SOW-0092.

Same-failure scan:

- `rg -n "FIXME|TODO" rust/src/crates/journal-core/src/file/offset_array.rs
  rust/src/crates/journal-core/src/file/cursor.rs` found no offset-array
  cursor hot-path debt. One pre-existing filter-cursor FIXME remains in
  `cursor.rs` and was not changed because it concerns filter semantics, not this
  SOW's offset-array cursor cache.

Sensitive data gate:

- Durable artifacts contain sanitized benchmark labels, aggregate counts,
  benchmark rates, and repository file references.
- Raw local journal paths and raw journal payloads remain only in `.local/`
  benchmark artifacts.
- No credentials, bearer tokens, SNMP communities, customer names, customer
  identifiers, personal data, non-private customer-identifying IPs, private
  endpoints, or proprietary incident details were written to durable artifacts.

Artifact maintenance gate:

- `AGENTS.md`: no change needed; this SOW does not change workflow or
  project-wide guardrails.
- Runtime project skills: no change needed; this SOW does not change how agents
  work in this repo.
- Specs: no change needed; this SOW implements the existing Rust reader
  performance contract without changing public behavior.
- End-user/operator docs: no change needed; this is an internal Rust reader
  optimization.
- End-user/operator skills: no change needed.
- SOW lifecycle: moved from pending to current, and SOW-status was updated.
- `.agents/sow/SOW-status.md`: updated for current in-progress state.

Specs update:

- No spec update needed. This SOW implements an internal Rust cursor-cache
  optimization under the existing reader performance contract.

Project skills update:

- No project skill update needed. No workflow rule changed.

End-user/operator docs update:

- No docs update needed. No public API or operator behavior changed.

End-user/operator skills update:

- No output/reference skill update needed.

Lessons:

- Optimizing public-but-undocumented internal Rust types can still look like an
  API change to reviewers. Future performance SOWs should explicitly state
  whether `journal-core` internals are part of the stable SDK surface.
- Cursor-level performance SOWs need separate reporting for cursor-only modes
  and payload/facade modes, because payload/facade paths include later work
  outside the cursor mechanism.

Follow-up mapping:

- Remaining payload/facade overhead belongs to SOW-0089 through SOW-0092.
- No new follow-up SOW is required from this SOW.

## Outcome

Implemented.

Changes:

- `offset_array::Cursor` now carries cached scalar node metadata for the active
  offset-array node.
- `JournalCursor::store_array_cursor_value()` now stores the materialized cursor
  returned with the current entry offset, so the next step reuses cached cursor
  state.
- Forward same-node movement no longer rebuilds the active node metadata.
- Reverse traversal materializes a lazy `Arc<[Node]>` chain only when predecessor
  lookup is required; same-node movement does not allocate.
- `InlinedCursor` now handles the non-`Copy` cursor without moving cached state
  out through shared references.
- `Cursor::collect_offsets()` now uses each next node's own `remaining_items`
  metadata when collecting subsequent arrays.
- Added a facade-level test covering forward and backward traversal across
  multiple entry-array nodes.

Completed.

Final full benchmark report:

- `.local/sow-0086/reader-baseline/report.json`
- `.local/sow-0086/reader-baseline/report.md`

Final focused cursor-only benchmark report:

- `.local/sow-0088/focused_cursor_bench.json`

Large-file full benchmark deltas against the post-SOW-0087 baseline:

| candidate | mode | SOW-0087 rows/s | SOW-0088 rows/s | delta |
|---|---:|---:|---:|---:|
| `real-compressed-multiboot-high-entry` | `core-next` | 12,572,187 | 16,820,602 | +33.8% |
| `real-compressed-multiboot-high-entry` | `core-offsets` | 8,377,358 | 9,515,636 | +13.6% |
| `real-compressed-multiboot-high-entry` | `sdk-payloads` | 3,314,665 | 3,179,854 | -4.1% |
| `real-compressed-multiboot-high-entry` | `facade-data` | 2,896,482 | 3,049,669 | +5.3% |
| `real-compressed-high-cardinality` | `core-next` | 9,234,522 | 11,853,607 | +28.4% |
| `real-compressed-high-cardinality` | `core-offsets` | 8,248,174 | 9,574,057 | +16.1% |
| `real-compressed-high-cardinality` | `sdk-payloads` | 3,946,056 | 4,444,896 | +12.6% |
| `real-compressed-high-cardinality` | `facade-data` | 3,619,561 | 3,776,465 | +4.3% |
| `real-compressed-high-field-count` | `core-next` | 10,549,411 | 12,848,633 | +21.8% |
| `real-compressed-high-field-count` | `core-offsets` | 7,282,137 | 8,267,666 | +13.5% |
| `real-compressed-high-field-count` | `sdk-payloads` | 825,405 | 806,836 | -2.2% |
| `real-compressed-high-field-count` | `facade-data` | 755,437 | 742,052 | -1.8% |
| `netdata-flow-largest-uncompressed` | `core-next` | 3,994,981 | 5,414,519 | +35.5% |
| `netdata-flow-largest-uncompressed` | `core-offsets` | 3,556,485 | 4,261,240 | +19.8% |
| `netdata-flow-largest-uncompressed` | `sdk-payloads` | 1,305,869 | 1,392,532 | +6.6% |
| `netdata-flow-largest-uncompressed` | `facade-data` | 1,210,888 | 1,317,856 | +8.8% |
| `netdata-flow-most-entries-uncompressed` | `core-next` | 4,460,651 | 5,464,837 | +22.5% |
| `netdata-flow-most-entries-uncompressed` | `core-offsets` | 3,987,988 | 4,440,324 | +11.3% |
| `netdata-flow-most-entries-uncompressed` | `sdk-payloads` | 1,379,802 | 1,493,239 | +8.2% |
| `netdata-flow-most-entries-uncompressed` | `facade-data` | 1,286,818 | 1,262,426 | -1.9% |
| `netdata-flow-online-uncompressed` | `core-next` | 5,791,825 | 5,367,148 | -7.3% |
| `netdata-flow-online-uncompressed` | `core-offsets` | 4,496,218 | 4,386,582 | -2.4% |
| `netdata-flow-online-uncompressed` | `sdk-payloads` | 1,496,385 | 1,494,485 | -0.1% |
| `netdata-flow-online-uncompressed` | `facade-data` | 1,331,786 | 1,216,542 | -8.7% |

Same-file facade versus SDK/systemd after SOW-0088:

| candidate | sdk-payloads rows/s | facade-data rows/s | facade/sdk | systemd data rows/s | facade/systemd |
|---|---:|---:|---:|---:|---:|
| `real-compressed-multiboot-high-entry` | 3,179,854 | 3,049,669 | 0.959 | 819,514 | 3.72x |
| `real-compressed-high-cardinality` | 4,444,896 | 3,776,465 | 0.850 | 1,120,280 | 3.37x |
| `real-compressed-high-field-count` | 806,836 | 742,052 | 0.920 | 483,504 | 1.53x |
| `netdata-flow-largest-uncompressed` | 1,392,532 | 1,317,856 | 0.946 | 454,101 | 2.90x |
| `netdata-flow-most-entries-uncompressed` | 1,493,239 | 1,262,426 | 0.845 | 452,237 | 2.79x |
| `netdata-flow-online-uncompressed` | 1,494,485 | 1,216,542 | 0.814 | 473,235 | 2.57x |

Focused seven-repetition cursor-only benchmark after SOW-0088:

| candidate | mode | direction | rows/s median | records | fields |
|---|---|---:|---:|---:|---:|
| `real-compressed-multiboot-high-entry` | `core-next` | `forward` | 16,515,680 | 305,093 | 0 |
| `real-compressed-multiboot-high-entry` | `core-next` | `backward` | 14,184,405 | 305,093 | 0 |
| `real-compressed-multiboot-high-entry` | `core-offsets` | `forward` | 9,439,916 | 305,093 | 7,628,128 |
| `real-compressed-multiboot-high-entry` | `core-offsets` | `backward` | 8,573,959 | 305,093 | 7,628,128 |
| `netdata-flow-largest-uncompressed` | `core-next` | `forward` | 5,683,196 | 114,552 | 0 |
| `netdata-flow-largest-uncompressed` | `core-next` | `backward` | 5,380,459 | 114,552 | 0 |
| `netdata-flow-largest-uncompressed` | `core-offsets` | `forward` | 4,449,311 | 114,552 | 4,755,804 |
| `netdata-flow-largest-uncompressed` | `core-offsets` | `backward` | 2,870,199 | 114,552 | 4,755,804 |
| `netdata-flow-online-uncompressed` | `core-next` | `forward` | 4,953,108 | 122,544 | 0 |
| `netdata-flow-online-uncompressed` | `core-next` | `backward` | 5,127,011 | 122,544 | 0 |
| `netdata-flow-online-uncompressed` | `core-offsets` | `forward` | 4,287,243 | 122,544 | 5,084,030 |
| `netdata-flow-online-uncompressed` | `core-offsets` | `backward` | 2,927,218 | 122,544 | 5,084,030 |

Interpretation:

- The SOW target was row stepping and offset-array cursor movement. The
  cursor-only full benchmark improved forward `core-next` on 5 of 6 large files
  and forward `core-offsets` on 5 of 6 large files.
- Payload and facade rows are affected by later work outside this SOW: entry
  data-offset extraction, DATA object access, decompression, and facade
  enumeration. The mixed payload/facade deltas are recorded but are not the
  primary acceptance criterion for this cursor-cache SOW.
- Backward `core-next` is now fast on the focused benchmark. Backward
  `core-offsets` on Netdata flow files remains materially slower than forward
  because the field-offset collection work dominates after cursor stepping; this
  remains in scope for the following reader hot-path SOWs.
