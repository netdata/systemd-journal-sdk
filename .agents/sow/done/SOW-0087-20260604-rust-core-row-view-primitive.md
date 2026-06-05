# SOW-0087 - Rust Core Row View Primitive

## Status

Status: completed

Sub-state: implemented, locally validated, reviewed, and ready for closeout.

## Requirements

### Purpose

Create one Rust core current-row owner for reader hot paths so performance rules
are enforced below `FileReader`, facade, index, and engine consumers.

### User Request

The user requires Rust reader performance work to satisfy cached header state,
rolling mmap, row-level mmap-backed uncompressed DATA pointers, compressed
current-row arena storage, row-level validity, and zero uncompressed hot-path
allocations.

### Assistant Understanding

Facts:

- SOW-0086 implemented row-pinned DATA enumeration in `FileReader`.
- SOW-0086 did not fully remove split row ownership between `JournalReader` and
  `FileReader`.
- The user clarified after SOW-0086 that tiny generated files are meaningless
  for performance decisions. Tiny files may remain compatibility fixtures only.
- Same-file `sdk-payloads` versus `facade-data` gaps are implementation/API
  overhead, not data-shape effects. SOW-0087 must target those gaps directly.

Inferences:

- The next clean step is a lower `journal-core` row-view primitive that owns row
  metadata, DATA offsets, row-pinned mmap windows, and compressed arena state.
- The first implementation should preserve the existing public Rust API shape
  and move ownership internally. If a public API shape change becomes necessary,
  stop and ask for a user decision before implementing that change.

Unknowns:

- Whether the existing `visit_entry_payloads()` callback path can adopt the
  primitive without losing its current throughput advantage.
- Whether lazy row-scoped facade enumeration closes the measured same-file
  facade gap enough, or whether deeper core access specialization is needed in
  SOW-0088/SOW-0090.

### Acceptance Criteria

- A `journal-core` row-view primitive owns current-row metadata, DATA offsets,
  row pins, and compressed row arena state.
- `FileReader` and facade DATA enumeration use the row-view primitive directly.
- Current-row ENTRY rereads for per-field enumeration are removed from the hot
  path.
- Uncompressed row enumeration has no steady-state allocations after warmup.
- SOW-0086 large benchmark candidates are rerun and compared with the SOW-0086
  final benchmark. Generated tiny fixtures must not be used as performance
  evidence.
- Same-file `sdk-payloads` versus `facade-data` deltas are reported explicitly.

## Analysis

Sources checked:

- `.agents/sow/done/SOW-0086-20260604-rust-reader-performance-contract-gap-analysis.md`

Current state:

- Row state is still split across core and SDK layers.

Risks:

- This is a central reader refactor and can affect facade, directory, and
  file-backed journalctl behavior if not tested broadly.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Multiple Rust layers own row traversal details, causing duplicated ENTRY
  offset collection and inconsistent lifetime/performance guarantees.

Evidence reviewed:

- SOW-0086 findings and implementation results.
- SOW-0086 final benchmark showed `facade-data` slower than `sdk-payloads` on
  the same files. Examples:
  - `real-compressed-multiboot-high-entry`: `sdk-payloads` 3,458,741 rows/s;
    `facade-data` 2,596,126 rows/s.
  - `netdata-flow-largest-uncompressed`: `sdk-payloads` 1,512,489 rows/s;
    `facade-data` 1,286,078 rows/s.
- Code evidence before implementation:
  - `rust/src/internal/testcmd/reader_core_bench/src/main.rs` routes
    `sdk-payloads` through `FileReader::visit_entry_payloads()`.
  - `rust/src/internal/testcmd/reader_core_bench/src/main.rs` routes
    `facade-data` through `SdJournalRestartData()` and
    `SdJournalEnumerateAvailableData()`.
  - `rust/src/journal/src/lib.rs` still stores row ownership state in
    `FileReader`.
  - `rust/src/crates/journal-core/src/file/file_payload.rs` has the low-level
    row-pinned payload access helpers but no single current-row owner.

Affected contracts and surfaces:

- Rust `journal-core`, `journal`, facade, file-backed journalctl readers, and
  reader benchmarks.

Existing patterns to reuse:

- SOW-0086 row-pinned mmap helpers and compressed row arena behavior.

Risk and blast radius:

- High for Rust reader internals; medium for public API if new borrow shapes are
  needed.
- Current implementation plan avoids public API shape changes. Any required
  public API change is an open decision and must stop implementation first.

Sensitive data handling plan:

- Use generated fixtures and sanitized benchmark labels only.

Implementation plan:

1. Add a `journal-core` current-row view primitive with row metadata, entry
   offset, DATA offsets, payload read context, row-pinned mmap payload handles,
   compressed row arena, and row-pin cleanup.
2. Preserve the existing public Rust `journal` API shape; keep the primitive
   `#[doc(hidden)]`/internal across the crate boundary.
3. Move `FileReader` current-row payload state into the primitive.
4. Route `FileReader::visit_entry_payloads()` and facade data enumeration
   through the primitive while keeping the callback path as lean as possible.
5. Remove duplicated ENTRY offset collection where the primitive owns current
   row offsets.
6. Add pointer-provenance, lifetime, and row reset tests.

Validation plan:

- Rust tests for `journal-core` and `journal`.
- Large-file reader benchmarks only for performance decisions. Candidate set
  should include large real compressed files and large uncompressed Netdata or
  generated files. Tiny generated fixtures may be run for compatibility but
  must not be used for throughput conclusions.
- Same-file `sdk-payloads` versus `facade-data` comparison before/after this
  SOW.
- `git diff --check`, SOW audit, and whole-SOW read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected.
- Specs: update Rust reader performance spec if the row-view contract changes.
- End-user/operator docs: likely unaffected.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: normal pending/current/done flow.
- SOW-status.md: update when state changes.

Open-source reference evidence:

- Not needed for this internal Rust ownership refactor. Compatibility behavior
  remains governed by SOW-0086 tests and the existing systemd-compatible reader
  facade contract.

Open decisions:

- User approved activating SOW-0087 in conversation on 2026-06-05.
- Public API shape changes are not approved. If needed, stop and ask.

## Implementation

Changed files:

- `rust/src/crates/journal-core/src/file/row_view.rs`: added the
  `CurrentRowView` primitive. It owns current ENTRY metadata, DATA offsets,
  payload read context, facade DATA iterator index, row-pinned mmap state,
  decompression scratch, and compressed row arena.
- `rust/src/crates/journal-core/src/file/mod.rs`: exposes the row-view
  primitive as `#[doc(hidden)]` for the Rust SDK layer.
- `rust/src/journal/src/lib.rs`: moved `FileReader` current-row state into
  `CurrentRowView`; `get_realtime_usec()`, cursor generation, facade DATA
  enumeration, and SDK payload visitor now reuse the same current-row metadata
  and offsets.
- `rust/src/journal/src/reader_helpers.rs`: removed obsolete duplicated helper
  functions that the row-view path replaced.
- `rust/src/journal/src/tests/facade.rs`: kept the existing pointer-provenance
  and row-lifetime assertions, updated to inspect row state through the new
  primitive.

Important behavior:

- Public Rust API shape was not changed.
- Uncompressed `SdJournalEnumerateAvailableData()` returns slices borrowed from
  row-pinned mmap data. The row pins are cleared before row advance, seek, or
  explicit data-state reset.
- Compressed DATA is copied into a row-level arena. The arena is cleared on
  current-row data restart or row advance.
- `CurrentRowView::load_entry()` deliberately collects only non-zero ENTRY DATA
  offsets and defers bad DATA object handling to payload iteration. This
  preserves the historical fixture behavior where invalid DATA references are
  recoverable and skipped at payload-read time.
- The uncompressed per-payload facade path no longer stores a per-field
  `Vec<RowPayload>` descriptor. After row-offset capacity warmup, uncompressed
  facade enumeration uses the row offsets, row-pinned mmap payload handles, and
  scalar iterator state.

## Validation

Local tests:

- `cargo fmt --all` from `rust/`: passed.
- `cargo test -p journal-core -p journal --target-dir ../.local/cargo-target`
  from `rust/`: passed.
  - `journal-core`: 72 passed.
  - `journal`: 26 passed.
  - `journal-core` doc tests: 0 passed, 3 ignored.

Large-file benchmark validation:

- Exact SOW-0086 candidate rerun command:
  `python3 .local/sow-0086/reader-baseline/run_baseline.py`.
- Report artifacts:
  - `.local/sow-0086/reader-baseline/report.json`
  - `.local/sow-0086/reader-baseline/report.md`
- Measurements below are median of 3 repetitions, Rust snapshot/windowed,
  32 MiB window. Tiny `systemd-matrix-*` rows were generated but are excluded
  from performance conclusions because they are 8 MiB compatibility fixtures.

| candidate | Rust sdk-payloads rows/s | Rust facade-data rows/s | systemd DATA rows/s | facade/sdk | facade/systemd |
|---|---:|---:|---:|---:|---:|
| real-compressed-multiboot-high-entry | 3,314,665 | 2,896,482 | 841,003 | 0.874 | 3.44x |
| real-compressed-high-cardinality | 3,946,056 | 3,619,561 | 1,160,813 | 0.917 | 3.12x |
| real-compressed-high-field-count | 825,405 | 755,437 | 478,418 | 0.915 | 1.58x |
| netdata-flow-largest-uncompressed | 1,305,869 | 1,210,888 | 444,788 | 0.927 | 2.72x |
| netdata-flow-most-entries-uncompressed | 1,379,802 | 1,286,818 | 476,904 | 0.933 | 2.70x |
| netdata-flow-online-uncompressed | 1,496,385 | 1,331,786 | 462,336 | 0.890 | 2.88x |

SOW-0086 final table before this row-view change reported these `facade/sdk`
ratios for the same labels:

| candidate | SOW-0086 facade/sdk | SOW-0087 facade/sdk |
|---|---:|---:|
| real-compressed-multiboot-high-entry | 0.751 | 0.874 |
| real-compressed-high-cardinality | 0.812 | 0.917 |
| real-compressed-high-field-count | 0.870 | 0.915 |
| netdata-flow-largest-uncompressed | 0.850 | 0.927 |
| netdata-flow-most-entries-uncompressed | 0.850 | 0.933 |
| netdata-flow-online-uncompressed | 0.884 | 0.890 |

Additional large-file sanity benchmark:

- Report artifacts:
  - `.local/sow-0087-row-view/bench-20260604T232756Z/report.json`
  - `.local/sow-0087-row-view/bench-20260604T232756Z/report.md`
- Scope: Rust live/windowed, 32 MiB window, forward scan, 1 warmup plus
  3 measured repetitions.
- Result: same-file `facade-data` was 85.1% to 95.8% of `sdk-payloads` across
  four large generated, Netdata-flow, real-system, and FSS candidates.

Reviewer status:

- Whole-SOW read-only reviewer batch completed after local validation.
- Reviewer votes:
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
- Final closeout read-only reviewer batch completed after cleanup.
- Final closeout reviewer votes:
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.

Reviewer findings and dispositions:

- Finding: `CurrentRowMetadata` and `CurrentRowView` were re-exported from the
  public `journal` crate root even though the SOW intended an internal
  primitive.
  - Disposition: fixed. The `journal` crate now imports these types privately
    and no longer exports them from the crate root. `journal-core` keeps the
    `#[doc(hidden)]` re-export because the `journal` crate is a separate crate
    boundary.
- Finding: `row_view.rs` used a helper with an unconstrained output lifetime
  for borrowed mmap payloads.
  - Disposition: fixed. The unsafe slice construction is now local to
    `CurrentRowView::payload_slice()`, tying the returned borrow to `&self`.
- Finding: `restart_data()` clears the compressed row arena but keeps the
  decompression scratch buffer allocated.
  - Disposition: intentional and documented in code. The scratch buffer is not
    returned to callers and is retained only to preserve hot-path capacity.
- Finding: `step_valid()` does not call
  `JournalReader::release_object_guards()` before loading row metadata.
  - Disposition: no code change. `JournalReader::step()` does not leave
    payload object guards active, and the per-row unconditional guard-clear call
    would add hot-path work without current correctness benefit. Guard clearing
    remains explicit before payload enumeration, where row-pinned DATA access
    needs it.
- Finding: `get_entry()` still uses the owned `Entry` helper path and duplicates
  some offset collection shape.
  - Disposition: accepted for this SOW. `get_entry()` is the owned/copying API,
    not the facade DATA hot path targeted by SOW-0087. Broader adoption and
    remaining duplication are tracked by SOW-0091.
- Finding: row-pinned mmap growth should have hostile-file bounds.
  - Disposition: already tracked by SOW-0092.
- Finding: dedicated low-level `journal-core` unit coverage for
  `CurrentRowView` could be stronger.
  - Disposition: existing Rust facade tests cover row-level pointer provenance,
    row lifetime, restart/reset, and compressed arena behavior through the
    public facade path used by consumers. Additional hostile/corrupt-file
    bounds are tracked by SOW-0092.
- Finding: compressed-arena tests use unsafe pointer observation internally.
  - Disposition: no production API change required. The production API keeps
    row-level borrows behind Rust lifetimes; the test uses pointer observation
    only to prove arena invalidation behavior.
- Finding: `data_offsets_mut()` and `visit_payloads_row_pinned()` were unused
  internal row-view methods.
  - Disposition: fixed. Both unused methods were removed.
- Finding: `clear_pins()` contained a tautological debug assertion immediately
  after setting `row_pins_active = false`.
  - Disposition: fixed. The no-op assertion was removed.
- Finding: `CurrentRowPayload` remains a hidden `journal-core` re-export.
  - Disposition: intentional. `CurrentRowView` methods consumed across the
    `journal-core`/`journal` crate boundary use `CurrentRowPayload` in method
    signatures, so it must remain reachable while hidden from documented SDK
    API.
- Finding: `FileReader::visit_entry_payloads()` still uses the transient
  payload visitor path rather than the single-payload row-pinned facade
  enumerator path.
  - Disposition: non-blocking for SOW-0087 and now explicitly tracked by
    SOW-0091. SOW-0087 optimized facade DATA enumeration and centralized row
    ownership; SOW-0091 covers broader row-view adoption.

Same-failure scan:

- Scope scan: Rust reader current-row state, facade DATA path, and obsolete
  helper duplication were searched. Remaining owned-entry and callback visitor
  duplication is mapped to SOW-0091 and row-pin hostile bounds are mapped to
  SOW-0092.

Sensitive data gate:

- Durable artifacts contain sanitized benchmark labels, aggregate counts,
  benchmark rates, and repository file references.
- Raw local journal paths and raw journal payloads remain only in `.local/`
  benchmark artifacts.
- No credentials, bearer tokens, SNMP communities, customer names, customer
  identifiers, personal data, non-private customer-identifying IPs, private
  endpoints, or proprietary incident details were written to durable artifacts.

Artifact maintenance gate:

- `AGENTS.md`: no change needed; SOW-0087 does not change project-wide workflow
  rules.
- Runtime project skills: no change needed; SOW-0087 does not change how future
  agents should operate beyond the already tracked Rust reader performance
  SOWs.
- Specs: no change needed. SOW-0087 is an internal Rust ownership primitive
  with unchanged public SDK behavior; the durable reader performance contract
  was added by SOW-0086 and remaining public/query API work is tracked by
  SOW-0081 through SOW-0083 and SOW-0091.
- End-user/operator docs: no public API change was made, so none expected.
- End-user/operator skills: none affected.
- SOW lifecycle: completed and moved to done as part of the closeout commit.
- `.agents/sow/SOW-status.md`: updated as part of closeout.

## Outcome

Completed.

SOW-0087 moved Rust current-row ownership into `CurrentRowView` in
`journal-core`, routed `FileReader` and facade DATA enumeration through that
primitive, preserved public Rust API shape, removed unused internal row-view
surface, and documented the remaining row-view adoption work in SOW-0091 plus
row-pin hostile-file bounds in SOW-0092.

Final large-file benchmark evidence shows the same-file `facade-data` path now
runs at 87.4% to 93.3% of `sdk-payloads` across the six large SOW-0086
candidates, up from 75.1% to 88.4% before SOW-0087. All five final closeout
reviewers voted `PRODUCTION GRADE`.
