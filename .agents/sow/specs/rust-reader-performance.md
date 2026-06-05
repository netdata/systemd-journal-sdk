# Rust Reader Performance Contract

## Scope

This specification applies to the Rust journal reader implementation only.
It defines performance requirements for reader hot paths. Correctness and
systemd journal compatibility remain mandatory, but correctness alone is not
sufficient for this project.

## Purpose

The Rust reader must be suitable for high-rate production journal exploration,
including Netdata-style polling readers and future query APIs that need to
scan large journal sets with predictable CPU, memory, and I/O behavior.

## Mandatory Hot-Path Rules

1. Cache journal file header snapshot metadata in the read-only reader/file
   layer. Snapshot readers, directory ordering, boot metadata, and cursor
   formatting must use the cached snapshot for stable fields such as
   `seqnum_id`, head/tail realtime, head/tail seqnum, boot ID, machine ID, and
   tail monotonic. Live public `header()` calls may explicitly refresh from the
   mapped header, but hot row traversal must not rematerialize header state on
   every row or field.
2. Use rolling mmap windows for normal file access. Whole-file mmap may exist
   only as an explicit experimental option, not as the default answer to
   lifetime or performance problems.
3. For current-row uncompressed DATA payloads, expose direct pointers/slices
   into mmap-backed bytes. Do not copy uncompressed DATA solely to satisfy API
   lifetime management.
4. For compressed DATA payloads, use a separate current-row append-only arena.
   The arena may grow when decompression needs more space, and it must be reset
   when the reader moves to another row.
5. Every slice or pointer returned by a row payload API must remain valid until
   the next outer row fetch or explicit row reset. A later field fetch within
   the same row must not invalidate earlier field slices from that row.
6. The uncompressed row hot path must perform zero allocations after warmup.
   The only accepted allocation in the row hot path is compressed-DATA arena
   growth required by decompression.
7. Reusable DATA objects must not be repeatedly parsed, decompressed, hashed,
   sorted, or copied when a row-scoped or query-scoped cache can preserve the
   same result without weakening correctness.
8. Journal indexes and object chains must be used when they answer the request.
   Entry scans are acceptable only when the requested operation is inherently
   row-oriented or when an active SOW records evidence that the indexed path is
   unsafe or slower for that operation.

## Rolling Mmap Row Pins

Rolling mmap row pins are allowed to temporarily retain more windows than the
normal steady-state window cache while one current row is active. This is an
intentional performance tradeoff: the row-level API contract requires all
uncompressed DATA payload slices returned for one row to remain valid until row
advance or explicit row reset.

Rules:

- Row pins must be cleared before advancing to another row, seeking, resetting
  row data state, or dropping the reader.
- A later field fetch within the same row must not evict or remap a window that
  backs an earlier uncompressed field slice from that same row.
- The additional mapped memory is scoped to one current row. It must not grow
  across rows.
- If a pinned window contains the requested start but not the full later
  requested range, the reader must map an overlapping wider window instead of
  remapping the pinned one. Returning the narrower pinned window would violate
  bounds safety; remapping it would violate row-level pointer lifetime.
- Non-row APIs that encounter a pinned window which does not cover their full
  requested range must create a separate transient overlapping window. That
  transient window is intentionally not row-pinned and may be evicted normally.
- Non-row APIs must keep using the normal bounded rolling-window behavior unless
  their contract explicitly requires row-level payload lifetime.
- A hard cap for hostile or corrupt rows that reference DATA objects across many
  windows is enforced by SOW-0092. The cap is the normal rolling-window cache
  limit for the `WindowManager` instance. When a row would need another
  row-pinned mmap window after the cap is reached, the mmap manager reads that
  DATA object into row-scoped boxed overflow storage and returns a slice from
  that stable storage. Overflow storage is cleared with row pins on row advance,
  seek, explicit row reset, or reader cleanup.

## DATA Decompression

Compressed DATA is the only accepted row hot-path reason to allocate payload
storage. The implementation should prefer the fastest compatible decompressor
available in the Rust dependency set, while preserving format compatibility.

Rules:

- Zstandard DATA should use the native `zstd` crate path for standard frames
  when the frame content size is available and bounded by the journal DATA
  maximum.
- The Rust `ruzstd` fallback remains required for compatibility with frames the
  native fast path rejects or cannot size safely.
- Decompressed bytes live in the current-row arena and are invalidated on row
  advance or explicit row reset.
- A query-scoped reusable decompression cache is not part of the current
  contract. It may be added only after a SOW records benchmark evidence that
  cache lookup and memory costs are lower than repeated decompression on real
  corpora.

## API Classes

The Rust reader exposes several API classes with different performance
contracts:

- Core cursor/offset APIs: row stepping and DATA offset enumeration. These are
  the lowest-level traversal primitives and should not materialize payloads.
- Core payload visitor APIs: immediate row DATA payload visitation. These may
  borrow mmap-backed bytes during the callback and must avoid allocation for
  uncompressed DATA.
- SDK row enumeration APIs: `entry_data_restart()` plus repeated
  `enumerate_entry_payload()`. These must provide row-level slice validity.
  Callers that need the hot row-level cache must call `entry_data_restart()`
  before enumeration, matching libsystemd-style `restart_data` semantics.
  `entry_data_restart()` eagerly caches all current-row DATA payloads so
  subsequent enumeration returns row-valid slices without per-field remapping,
  decompressor setup, or ownership changes. This favors complete row traversal;
  callers that need only specific fields should use field-specific APIs.
- libsystemd-compatible facade APIs: `SdJournal` data enumeration must be a
  thin layer over the SDK row enumeration path and must not introduce extra
  copies or ENTRY materialization in hot metadata/data paths.
- Optimized explorer APIs: `FileReader::explore()` must use native filter
  indexes for exact field/value slicing, lazy candidate-row DATA-offset
  classification caches for reusable DATA objects, and owned cached value
  labels for required DATA that must be returned in facet, histogram, FTS, or
  row results. The default field mode is first-value row semantics: one selected
  facet/histogram/source field contributes at most one value per row, and the
  row DATA loop may stop after all requested field identities are found. This
  early stop must avoid touching and decompressing unrelated trailing DATA.
  Facets that share the same effective filter set must be grouped into one
  traversal pass instead of multiplying row scans by facet count.
  `ExplorerAnchor::Auto` is the default anchor policy: forward scans start from
  the lower query bound or file head, while backward scans start from the upper
  query bound or file tail.
  `ExplorerFieldMode::FirstValue` is the default. `ExplorerFieldMode::AllValues`
  is an explicit slower mode for exact duplicate-value accounting and scans the
  whole row for repeated-field correctness.
- `FileReader::explore_with_strategy()` exposes explicit strategy selection for
  performance experiments and specialized callers. `ExplorerStrategy::Traversal`
  is the default and must remain the general-purpose path. `ExplorerStrategy::Index`
  may answer selected all-values facet and histogram queries from FIELD/DATA
  chains and DATA entry posting lists, but it must reject query modes it cannot
  answer exactly. Do not silently approximate `FirstValue`, FTS, or
  source-realtime semantics. `ExplorerStrategy::Compare` must run both
  strategies, reject mismatches, and expose traversal/index timing and counter
  diagnostics. There is no auto planner; SOW-0083 evidence shows index
  aggregation is shape-sensitive and can regress badly on selective filters or
  many facets.
- Owned convenience APIs: APIs that return fully materialized entries may copy
  and allocate, but they must be documented and benchmarked as non-hot paths.

`sdk-entry`/owned entry materialization is explicitly a convenience API class.
It must not be used as the implementation path for facade metadata, facade DATA
enumeration, row scanning, filtering, faceting, or other reader hot paths.

## Accepted Exceptions

- Compressed DATA must be decompressed before returning a plain `FIELD=value`
  payload to APIs that require plain bytes. The decompressed bytes live in the
  current-row arena.
- Corruption handling may allocate diagnostic data outside the steady-state hot
  path.
- Compatibility fallback paths for damaged or historical indexes may scan rows
  if the active SOW records why the indexed path cannot be trusted.

## Validation Requirements

Reader performance work must record:

- which Rust reader API class was changed;
- whether uncompressed DATA is copied or mmap-borrowed;
- whether compressed DATA uses the current-row arena;
- whether current-row returned slices survive subsequent field fetches in the
  same row;
- allocation evidence for the affected uncompressed hot path;
- benchmark results against the SOW-0086 baseline candidates;
- any remaining slower path, with the reason and a tracked follow-up.
