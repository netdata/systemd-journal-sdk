# SOW-0092 - Rust Row Pin Hostile File Bound

## Status

Status: completed

Sub-state: implemented, validated, reviewed, and closed.

## Requirements

### Purpose

Keep the Rust reader's row-level mmap-backed zero-copy contract fast for normal
production journals while bounding virtual-memory growth for hostile, corrupt,
or deliberately pathological journal files.

### User Request

The user requires top Rust reader performance. SOW-0086 introduced row-pinned
rolling mmap windows so uncompressed current-row payloads can stay borrowed from
mmap-backed DATA objects until the reader leaves the row. Reviewers identified
that a single pathological row can reference DATA objects spread across many
windows and temporarily exceed the normal rolling-window cache.

### Acceptance Criteria

- The Rust reader has a measured and documented per-row bound for row-pinned
  mmap windows or row-pinned mapped bytes.
- If the bound is exceeded, the reader preserves correctness by falling back to
  copying uncompressed DATA into the current-row arena or another bounded
  current-row buffer.
- Normal production benchmark candidates from SOW-0086 do not regress beyond
  measured noise unless the SOW records a user-approved tradeoff.
- A synthetic hostile-file test proves memory remains bounded when one entry
  references DATA objects spread across many mmap windows.
- The row-level validity guarantee remains true for borrowed and copied
  payloads.

## Analysis

Sources checked:

- SOW-0086 implementation and reviewer findings.
- `rust/src/crates/journal-core/src/file/mmap.rs` row-pinned window logic.
- `.agents/sow/specs/rust-reader-performance.md` row-level lifetime contract.

Current state:

- SOW-0086 allows row-pinned windows to exceed the steady-state window-cache
  limit for one current row.
- That is required for zero-copy row-level payload validity with rolling mmaps.
- The current implementation does not define a hard per-row cap.

Risks:

- A malicious or corrupt journal can force excessive transient mappings by
  placing current-row DATA objects far apart.
- A naive cap can silently break row-level pointer validity or add copies to the
  normal hot path.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The row-pinned mmap contract protects borrowed uncompressed DATA payloads by
  keeping backing windows mapped until the reader leaves the row. A row with
  widely scattered DATA objects can pin many windows before the row ends.

Evidence reviewed:

- SOW-0086 reviewer finding on unbounded per-row pinned-window growth.
- `rust/src/crates/journal-core/src/file/mmap.rs` row-pinned window creation and
  eviction code.

Affected contracts and surfaces:

- Rust `FileReader` row payload enumeration.
- Rust facade DATA enumeration.
- Rust reader performance spec.
- Hostile/corrupt journal behavior.

Existing patterns to reuse:

- SOW-0086 current-row arena for compressed DATA.
- SOW-0086 row-pinned mmap window lifetime tests.

Risk and blast radius:

- Medium. The change touches mmap lifetime and fallback ownership behavior.
- The main risk is accidentally adding copies to normal uncompressed production
  traversal.

Sensitive data handling plan:

- Use generated hostile fixtures only. Do not record real journal payloads in
  durable artifacts.

Implementation plan:

1. Measure realistic row-pinned window counts on SOW-0086 real and generated
   benchmark candidates.
2. Cap current-row pinned mmap windows at the existing rolling mmap window
   budget. This keeps the normal steady-state mmap footprint contract and uses
   row-scoped mmap-manager overflow storage only for hostile or unusually
   scattered rows that would exceed that budget.
3. Add a fallback path that copies only overflow DATA objects into row-scoped
   stable boxed storage owned by the mmap manager.
4. Add hostile-file tests and rerun SOW-0086 reader benchmarks.

Validation plan:

- Rust tests for normal row-pinned borrowing and hostile fallback.
- SOW-0086 benchmark candidates before/after.
- `git diff --check`, SOW audit, and whole-SOW read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected.
- Specs: update `.agents/sow/specs/rust-reader-performance.md` with the final
  per-row cap and fallback behavior.
- End-user/operator docs: likely unaffected.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: tracked child SOW created from SOW-0086 closeout.
- SOW-status.md: update when state changes.

Open-source reference evidence:

- Not yet checked; this is an internal SDK hardening SOW.

Open decisions:

- None. The cap uses the existing rolling mmap window budget rather than adding
  a new public option or a second independent tuning knob.

Implementation decision:

- Row-pinned window cap: equal to the window manager's configured steady-state
  window limit. In the current Rust reader benchmark configuration this is 16
  windows. Whole-file mmap remains uncapped because there is no rolling-window
  growth to bound.
- Fallback behavior: if borrowing an uncompressed DATA payload would require
  pinning a new mmap window beyond the cap, the mmap manager copies that full
  DATA object into row-scoped boxed overflow storage and returns a slice from
  that stable storage. The returned pointer still satisfies the row-level
  lifetime guarantee. Overflow storage is cleared together with row pins when
  the reader advances, seeks, resets row data state, or clears the current row.
- Non-row immediate accesses while row pins are active may use one unpinned
  transient mapping when every cached window is row-pinned. That transient
  mapping is replaceable by later non-row access and does not grow with the row;
  row-pinned mapping count remains capped.

## Implementation

Changed files:

- `rust/src/crates/journal-core/src/file/mmap.rs`
  - Added `row_overflow_objects` as cold row-scoped boxed storage.
  - Added `WindowManagerStats.row_pin_count`, `row_pin_limit`, and
    `row_overflow_object_count`.
  - Added direct positional read support for the cold overflow-copy path.
  - Kept normal `get_row_pinned_slice()` mmap-backed behavior and changed only
    the growth case that would exceed the row-pin cap.
  - Clears overflow storage defensively anywhere mappings and row-pin counters
    are reset, including writer-side sync and post-change paths that do not
    currently populate overflow storage.
- `rust/src/crates/journal-core/src/file/row_view.rs`
  - Kept the SOW-0091 fast path shape for uncompressed row payloads.
  - Explicitly drops the ENTRY object guard before loading row metadata state,
    avoiding guard conflicts when later row state needs the window manager.
- `rust/src/crates/journal-core/src/file/file_payload.rs`
  - Normal row-pinned payload helper remains the single fast-path entrypoint.
- `rust/src/journal/src/tests/facade.rs`
  - Added hostile window-pressure coverage proving the cap and row-level
    pointer validity.
- `rust/src/journal/src/lib.rs`
  - Exposes hidden mmap stats for tests/benchmark evidence.
- `rust/src/internal/testcmd/writer_core_bench/src/main.rs`
  - Keeps benchmark JSON compatible with the expanded mmap stats structure.

Design notes:

- The cap is enforced at the point where the reader would otherwise add another
  row-pinned window and no unpinned cached window can be reused safely.
- Normal rows do not copy uncompressed DATA.
- Whole-file mmap remains uncapped because it does not create rolling-window
  growth.
- Compressed DATA remains row-arena owned after decompression. It does not need
  row-pinned backing storage after decompression, so immediate compressed-object
  reads use the normal mapping path and remain bounded by the row-pinned window
  limit plus one replaceable unpinned transient mapping under pressure.

## Validation

Local validation:

- `cargo fmt --manifest-path rust/Cargo.toml -p journal-core -p journal -p reader_core_bench -p writer_core_bench`: passed.
- `cargo test --manifest-path rust/Cargo.toml -p journal-core -p journal --target-dir .local/cargo-target`: passed.
- Targeted hostile test:
  `cargo test --manifest-path rust/Cargo.toml -p journal facade_uncompressed_windowed_row_pins_are_bounded_under_window_pressure --target-dir .local/cargo-target`: passed.
- `cargo build --release --manifest-path rust/Cargo.toml -p reader_core_bench -p writer_core_bench --target-dir .local/cargo-target`: passed.
- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed.

Hostile-file evidence:

- The synthetic hostile test writes one entry with 24 large uncompressed fields
  and opens the reader with a 4096-byte rolling mmap window.
- The test confirms all 24 payloads are readable.
- The test confirms `row_pin_count <= row_pin_limit` and that the pressure row
  reaches the cap.
- The test confirms `row_overflow_object_count > 0`, proving the hostile row
  used row-scoped overflow storage after the row-pin cap was reached.
- The test keeps a pointer from the first payload and confirms it remains valid
  after later payload enumeration forced additional window pressure.
- A direct `WindowManager` low-limit test opens a two-page file with
  `max_windows = 1`, proves the second row-pinned slice uses overflow storage,
  proves the first mmap-backed pointer remains valid, and proves
  `clear_row_pins()` resets the overflow count.

Final benchmark artifact:

- `.local/sow-0086/reader-baseline/report.json`
- `.local/sow-0086/reader-baseline/report.md`
- Created by `python3 .local/sow-0086/reader-baseline/run_baseline.py`.
- The report is a scratch artifact and is not committed.

Final large-candidate rows from the current implementation:

| candidate | sdk-payloads rows/s | sdk-entry rows/s | facade-data rows/s | systemd-data rows/s |
|---|---:|---:|---:|---:|
| `real-compressed-multiboot-high-entry` | 3,482,395 | 176,258 | 2,495,283 | 813,355 |
| `real-compressed-high-cardinality` | 4,168,978 | 248,116 | 3,339,493 | 1,101,066 |
| `real-compressed-high-field-count` | 741,840 | 124,029 | 722,933 | 482,735 |
| `netdata-flow-largest-uncompressed` | 1,391,780 | 78,998 | 1,267,330 | 466,677 |
| `netdata-flow-most-entries-uncompressed` | 1,509,292 | 79,209 | 1,272,691 | 451,025 |
| `netdata-flow-online-uncompressed` | 1,634,454 | 80,404 | 1,252,372 | 461,486 |

Performance comparison notes:

- Direct A/B benchmark tables against the SOW-0091 binary were noisy and varied
  by run order.
- Targeted `perf stat -r 10` checks on the apparent outlier
  `netdata-flow-most-entries-uncompressed` did not confirm the A/B table
  regression: current measured `0.083244831s +/- 1.55%` versus SOW-0091
  binary `0.087606999s +/- 2.30%` for `sdk-payloads`.
- Targeted `perf stat -r 10` on `real-compressed-high-cardinality` also did
  not confirm a stable instruction regression: current and SOW-0091 binary had
  effectively equal instruction counts in that run, while current wall time was
  faster.
- Accepted conclusion: the final overflow-box design bounds hostile rows and
  keeps normal production traversal within measured benchmark noise.

Sensitive data gate:

- No raw journal payloads, hostnames, customer data, secrets, or personal data
  were written to durable artifacts.
- Durable evidence records only sanitized labels, counts, rates, and command
  names.

Artifact maintenance:

- `AGENTS.md`: no change needed; no project-wide process rule changed.
- Runtime project skills: no change needed; no workflow changed.
- Specs: updated `.agents/sow/specs/rust-reader-performance.md`.
- End-user/operator docs: no change needed; this is internal Rust reader
  hardening with no public API change.
- End-user/operator skills: no change needed.
- SOW lifecycle: this SOW moved from pending to current and then to done after
  reviewer acceptance.
- `.agents/sow/SOW-status.md`: updated for current state and closeout.

Reviewer pass:

- First whole-SOW reviewer batch:
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/qwen3.6-plus`: timed out at 30 minutes after reading the
    SOW, changed files, and confirming the SOW audit tail.
- Non-blocking reviewer findings handled before final closeout:
  - Defensive cleanup: writer-side `sync()` and `post_change()` now clear
    row-overflow storage together with mappings and row-pin counters, even
    though writers do not currently populate overflow storage.
  - Added direct `WindowManager` `max_windows = 1` coverage for the low-limit
    cap/overflow path.
- Final whole-SOW reviewer rerun after cleanup and local validation:
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
- Residual non-blocking observations disposition:
  - Row overflow heap memory is row-scoped and bounded by the current entry's
    overflow DATA sizes; this is the documented correctness fallback for
    hostile rows and does not change normal production zero-copy traversal.
  - One replaceable unpinned transient mapping may exist for immediate non-row
    access while all normal windows are row-pinned; it is documented, does not
    grow with the row, and does not affect the row-pinned mapping cap.
  - Whole-file mmap remains uncapped by design because it does not create
    rolling-window growth.

## Outcome

SOW-0092 is completed.

The Rust reader now bounds row-pinned mmap windows at the normal
`WindowManager` rolling-window budget. When a hostile or corrupt row would
require another row-pinned window beyond that budget, the reader copies that
DATA object into row-scoped boxed overflow storage and returns a row-valid
slice from that storage. Normal uncompressed production rows remain
mmap-backed and zero-copy.

Final validation passed, the Rust reader performance spec was updated, and all
five final reviewers voted `PRODUCTION GRADE`.
