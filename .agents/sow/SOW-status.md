# SOW Status

Last updated: 2026-05-30

## Current

- SOW-0009 - Benchmark Profile Optimize: paused umbrella. Writer and reader
  performance work is split into focused child SOWs; this file remains the
  program index.

## Pending

- SOW-0071 - Runtime Purity And Optional Platform Services: open. Architectural
  correction to keep core journal readers/writers file-format-only, move host
  identity discovery to optional helpers, move writer locking to independent
  optional helpers, update instructions/specs/docs, and validate on Linux plus
  provided macOS/Windows hosts before closing portability or releasing stable
  APIs.
- SOW-0026 - Netdata SDK Integration Inventory And Cut Plan: open. Produces the
  fresh Netdata consumer inventory and cut plan after performance gates.
- SOW-0047 - Netdata NetFlow SDK Integration: open. Component integration for
  NetFlow reader and writer paths after inventory and performance gates.
- SOW-0048 - Netdata OTEL Writer SDK Integration: open. Component integration
  for OTEL writer paths after inventory and writer gates.
- SOW-0049 - Netdata Reader Plugin SDK Integration: open. Component integration
  for OTEL signal viewer, no-libsystemd systemd journal reading, and static
  packaging after reader gates.
- SOW-0050 - Netdata Vendored Journal Removal: open. Final cleanup after all
  Netdata component integrations are complete.
- SOW-0055 - Rust Seek Cursor Systemd Parity: open. Follow-up from SOW-0045
  review to realign Rust `SdJournalSeekCursor()` with upstream systemd's
  no-existence-proof seek-location behavior.

## Recently Closed Or Completed

- SOW-0059 - Standard Benchmark Reporting: completed. Added a stdlib-only
  benchmark report generator for reader-core and writer-core JSON artifacts,
  documented the canonical report shape, added 15 report-shape/unit tests, and
  validated output against existing SOW-0058 reader and writer-core artifacts.
- SOW-0058 - Rust DATA Header Fast Path: completed. Rust DATA payload hot paths
  now parse only the 16-byte object header before validation and payload
  slicing. Correctness tests passed, read-only reviewers found no blockers, and
  benchmark evidence was mixed: single-file `sdk-payloads` and `facade-data`
  improved in the clean baseline/current comparison, but low-level
  `core-payloads` and some open-files medians were flat or lower, so this
  change does not conclusively explain the Go/Rust reader gap.
- SOW-0057 - Rust Live Whole-File Mmap Reader Option: completed. Rust live
  readers can explicitly opt into whole-file mmap through the existing
  experimental mmap strategy option while default live readers remain windowed.
  The compact/offline 100k-row benchmark measured Rust single-file
  `sdk-payloads` live/windowed at 2.52M rows/s and live/whole-file at 2.52M
  rows/s, so whole-file mmap does not explain the Go/Rust reader gap on this
  corpus.
- SOW-0056 - Go Reader Hot-Path Optimization Phase 2: completed. Go reader
  hot-path internals now avoid redundant DATA header parsing, preserve
  current-entry DATA-offset slice backing storage safely, return ENTRY headers
  by value, cache immutable compact/regular layout constants, and specialize
  regular/compact offset loops. The compact 100k-row reader benchmark measured
  Go single-file `sdk-payloads` live/mmap at 2.74M rows/s and `facade-data` at
  2.33M rows/s; Go open-files `sdk-payloads` live/mmap measured 2.40M rows/s
  and `facade-data` 1.99M rows/s, versus stock systemd DATA medians of 634k
  rows/s single-file and 628k rows/s open-files. Go tests, mixed-directory,
  cross-language, live regular/compact matrices, read-only reviewers, and audit
  passed.
- SOW-0045 - Go Reader Alignment Optimization: completed. Go reader now has
  mmap-backed Unix access by default, live/snapshot bounds, byte-preserving RAW
  field APIs, current-entry payload visitor/enumerator APIs, libsystemd facade
  DATA fast paths, non-overlapping directory fast-path coverage, and shared
  reader benchmark integration. Compact 100k-row benchmark medians measured Go
  single-file `sdk-payloads` live/mmap at 1.07M rows/s and `facade-data` at
  1.09M rows/s versus stock systemd DATA enumeration at 565k rows/s; Go
  open-files `sdk-payloads` measured 697k rows/s versus stock systemd open-files
  DATA at 532k rows/s. Rust remains faster and Rust cursor-seek systemd
  divergence discovered during review is tracked by SOW-0055.
- SOW-0054 - Node.js Reader And Writer Rust Port: completed. Node.js now carries
  the finalized reader/writer compatibility slice where practical under the
  no-native-runtime policy: byte-preserving RAW field access, active-file
  refresh, current-entry payload scanning, libsystemd-like facade DATA fast
  paths, no-existence-proof `seekCursor()`, parser bounds hardening, reader
  benchmarks, writer benchmark evidence, and updated docs/specs. Node.js package
  tests, directory/mixed/live/journalctl matrices, writer and reader
  benchmarks, same-scope read-only reviewer rechecks, and audit passed.
- SOW-0053 - Python Reader And Writer Rust Port: completed. Python now carries
  the finalized Rust reader/writer contract where practical for pure Python:
  mmap-backed normal and decompressed `.journal.zst` reads, active-file
  refresh at tail/end, byte-preserving raw payload access, current-entry
  facade DATA enumeration without full-entry materialization, context-manager
  cleanup, and retained writer policy/compression/compact/FSS parity. Python
  package tests, directory/mixed/live/journalctl matrices, reader benchmarks,
  and audit passed; remaining writer throughput limits are documented as a
  pure-Python runtime limitation.
- SOW-0052 - Rust Reader Last-Mile Optimization: completed. Rust reader payload
  scans now avoid redundant ENTRY/DATA materialization, reuse active mmap
  windows, cache current-entry DATA offsets safely, and return mmap-backed
  uncompressed facade payloads while preserving compressed reusable-buffer
  fallback. The compact 200k-row benchmark measured Rust single-file
  `sdk-payloads` live/windowed at 2.44M rows/s and `facade-data` at 2.24M
  rows/s versus stock systemd data enumeration at 537k rows/s; full Rust,
  directory, mixed-directory, live, journalctl query, and read-only reviewer
  gates passed.
- SOW-0051 - Node.js And Python Writer Performance: closed without
  implementation. Superseded by language-specific SOW-0053 and SOW-0054 after
  the user changed priority to Rust -> Python -> Node.js full-language ports.
- SOW-0046 - Python Node Reader Alignment: closed without implementation.
  Superseded by language-specific SOW-0053 and SOW-0054 after the user changed
  priority to Rust -> Python -> Node.js full-language ports.
- SOW-0043 - Rust Reader Libsystemd/Jf Parity: completed after second
  regression repair. Rust facade current-entry DATA enumeration now matches the
  systemd/libsystemd and old Netdata `jf` model: uncompressed DATA is returned
  directly from mmap-backed journal payloads, compressed DATA uses one reusable
  reader-owned decompression buffer, and active current-DATA state is
  invalidated only when a later operation supersedes that pointer. The compact
  100k-row benchmark measured Rust single-file `facade-data` live/windowed at
  about 1.17M rows/s versus stock libsystemd data enumeration at about 645k
  rows/s.
- SOW-0044 - Rust Reader Hot-Path Optimization: completed after regression
  repair. Rust `Live` reader bounds now use systemd-style cached mutable bounds
  instead of refresh-every-slice behavior; 100k-row compact `sdk-payloads`
  live/windowed measured about 1.34M rows/s versus stock libsystemd data
  enumeration at about 660k rows/s, with 6 `statx` calls in the profiled live
  hot-path run and passing Rust, directory, mixed-directory, live matrix, and
  read-only reviewer gates.
- SOW-0042 - Writer Final Certification: completed. Rust and Go writers are
  performance-certified for the accepted compact, no-compression, FSS-off direct
  and directory writer baselines. Node.js and Python writers are
  correctness-certified for the same baselines, but their high-throughput writer
  performance remains limited and is tracked by SOW-0051.
- SOW-0041 - Node.js Writer Rust Parity: completed. Node.js direct and
  directory writers now expose raw full-payload append, high-level `Log`
  entries inject indexed `_BOOT_ID` plus optional
  `_SOURCE_REALTIME_TIMESTAMP`, Node.js docs/specs record the Buffer plus
  positioned `node:fs` no-mmap runtime path, and Node package tests plus
  stock/cross-language binary, compression, compact, lock, and live matrices
  passed. Reviewer findings were resolved, with final Minimax and GLM
  confirmation at PRODUCTION GRADE.
- SOW-0040 - Python Writer Mmap And Rust Parity: completed. Python direct and
  directory writers now expose raw append parity, high-level `_BOOT_ID` /
  `_SOURCE_REALTIME_TIMESTAMP` metadata injection, and a whole-file mapped
  arena hot path. Python package tests, binary/compression/compact/live
  interoperability, and all-language lock matrix passed; writer-core compact
  baseline improved from ~468 to ~930 append rows/s.
- SOW-0037 - Writer Reference Closure: completed. Closed the Rust/systemd and
  Go/Rust writer reference matrix, fixed Go/Rust writer drift found during the
  pass, mapped Python/Node.js writer parity to SOW-0040 and SOW-0041, and
  corrected the initial short-hold lock-matrix failure as a timing artifact
  after a longer all-language lock run passed 8/8.
- SOW-0039 - RAW Byte Field Name Reader Representation: closed. Superseded by
  SOW-0043 so byte-preserving RAW reader representation is designed with the
  full reader parity work.
- SOW-0038 - Field Name Policy Layers: completed. Rust, Go, Node.js, and
  Python now expose RAW, JOURNALD, and JOURNAL-APP writer field-name policies;
  producer-specific field-name remapping has been removed from SDK code, docs,
  and public API. This is the `v0.3.0` / `go/v0.3.0` release target.
- SOW-0036 - Live Publication Modes And Fast Consumers: completed. Rust, Go,
  Node.js, and Python expose the shared `live_publish_every_entries` writer
  option. Default `1` keeps stock-compatible publication after every entry;
  `0` and `N > 1` are narrower latency-tolerant contracts. Whole-file mmap and
  Rust recent-DATA-cache-size changes were measured and not kept.
- SOW-0035 - Derived Rotation Policy: completed.
