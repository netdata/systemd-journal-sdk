# SOW Status

Last updated: 2026-06-03

## Current

- SOW-0009 - Benchmark Profile Optimize: paused umbrella. Writer and reader
  performance work is split into focused child SOWs; this file remains the
  program index.
- SOW-0084 - Code Scanning And Codacy Gate: in-progress. Advanced GitHub
  CodeQL and Codacy SARIF reporting are active; the Codacy SARIF workflow now
  uses tuned Cloud configuration when `CODACY_API_TOKEN` is available. Security
  findings are cleared in the latest direct Codacy export. The generated
  `rust/Cargo.lock` Lizard finding is excluded in Codacy Cloud, the corrected
  actionable inventory is 161 runtime, 201 test/harness, and 17 other findings.
  Batch 1 made the Go reader/verifier touched files locally Lizard-clean.
  Batch 2 made all non-test Go runtime files under `go/journal/*.go` and
  `go/cmd/journalctl/*.go` locally Lizard-clean and passed Go tests plus
  matrix, directory, and full verifier interoperability checks. Batch 3 cleared
  the touched Rust public reader/verifier runtime findings in
  `rust/src/journal/src/lib.rs` and `rust/src/journal/src/parse.rs`, preserving
  parser APIs and passing Rust plus interoperability validation. Batch 4 made
  the Rust object-graph verifier locally Lizard-clean and passed Rust plus full
  verifier interoperability validation. Batch 5 made the Rust core writer
  runtime locally Lizard-clean and passed journal-core tests plus writer and
  verifier interoperability validation. Batch 6 made Rust core file/mmap object
  access runtime locally Lizard-clean and passed journal-core plus writer
  interoperability validation. Batch 7 made the remaining Rust core/log-writer
  runtime files locally Lizard-clean and passed journal-core, journal-log-writer,
  journal/adapter, writer-reader, directory, and verifier validation. Batch 8
  made the legacy Rust `jf` compatibility copy and `journal-registry` runtime
  touched files locally Lizard-clean, with affected crate tests passing. Batch
  9 made Rust `journal-engine` and `journal-index` query/index runtime files
  locally Lizard-clean, with full affected crate tests passing. Batch 10 made
  Python verifier and reader touched runtime files locally Lizard-clean, fixed
  a corrupted ENTRY_ARRAY hang, and passed the full Python package test suite.
  Batch 11 made Python writer and directory-writer touched runtime files
  locally Lizard-clean, fixed a temporary DATA/FIELD linking regression found
  during diff review, passed the full Python package test suite, and reduced
  the refreshed local all-tracked-file Lizard inventory to 220 critical
  findings. Batch 12 made all remaining `python/journal/*` core runtime files
  locally Lizard-clean, passed the full Python package test suite, and reduced
  the refreshed local all-tracked-file Lizard inventory to 213 critical
  findings. Batch 13 made all remaining Python adapter, CLI, and test harness
  files locally Lizard-clean, passed the full Python package test suite, and
  reduced the refreshed local all-tracked-file Lizard inventory to 203
  critical findings. Batch 14 made the first Node.js core runtime group
  locally Lizard-clean, passed the Node package test suite, and reduced the
  refreshed local all-tracked-file Lizard inventory to 199 critical findings.
  Batch 15 made the remaining `node/src/lib/*` core runtime files locally
  Lizard-clean, passed the Node package test suite, and reduced the refreshed
  local all-tracked-file Lizard inventory to 186 critical findings. Batch 16
  made all remaining Node adapter, CLI, facade, benchmark, and testcmd files
  locally Lizard-clean, passed the Node package test suite, and reduced the
  refreshed local all-tracked-file Lizard inventory to 174 critical findings.
  Node has no remaining critical Lizard findings. Batch 17 made Go adapter and
  internal command-tool files locally Lizard-clean, passed `go test ./...`, and
  reduced the refreshed local all-tracked-file Lizard inventory to 160 critical
  findings. Remaining critical findings are `go: 31`, `rust: 53`, and
  `tests: 76`; Go findings are limited to `go/journal/*_test.go`. Batch 18
  made all remaining Go journal test files locally Lizard-clean, passed
  `go test ./...`, and reduced the refreshed local all-tracked-file Lizard
  inventory to 129 critical findings. Go has no remaining critical Lizard
  findings; remaining critical findings are `rust: 53` and `tests: 76`.
  Batch 19 made Rust internal benchmark and corpus helper files locally
  Lizard-clean, passed affected Rust helper package checks, and reduced the
  refreshed local all-tracked-file Lizard inventory to 114 critical findings.
  Rust internal helper files have no remaining critical Lizard findings;
  remaining critical findings are `rust: 38` and `tests: 76`. Batch 20 made
  Rust adapter, legacy `jf`, and core file/writer touched files locally
  Lizard-clean, passed affected Rust and legacy `jf` tests, and reduced the
  refreshed local all-tracked-file Lizard inventory to 103 critical findings.
  Remaining critical findings are `rust: 27` and `tests: 76`; Go, Node.js,
  and Python remain at zero. Batch 21 made Rust journal facade, log-writer, and
  journal-index pagination test files locally Lizard-clean, passed affected
  Rust tests, and reduced the refreshed local all-tracked-file Lizard inventory
  to 82 critical findings. Remaining critical findings are `rust: 19` and
  `tests: 63`; Go, Node.js, and Python remain at zero. Batch 22 made the
  remaining Rust `journal-engine` multi-file pagination test file locally
  Lizard-clean, passed its full integration test file, and reduced the
  refreshed local all-tracked-file Lizard inventory to 63 critical findings.
  All remaining critical findings are under `tests/`; Rust, Go, Node.js, and
  Python are at zero. Batch 23 made dataset, code-scanning, conformance
  manifest, and live concurrency utility harness files locally Lizard-clean,
  passed dataset validation, code-scanning pytest coverage, conformance
  manifest validation, and reduced the refreshed local test-file Lizard
  inventory to 56 critical findings. Remaining groups are
  `tests/interoperability`: 21, `tests/benchmarks`: 13,
  `tests/corpus_eval`: 12, `tests/systemd_matrix`: 6, and
  `tests/vm_matrix`: 4. Batch 24 made all benchmark report/runner harness
  files locally Lizard-clean, passed Python compile checks and CLI help smoke
  checks for the touched benchmark entrypoints, and reduced the refreshed
  local test-file Lizard inventory to 43 critical findings. Remaining groups
  are `tests/interoperability`: 21, `tests/corpus_eval`: 12,
  `tests/systemd_matrix`: 6, and `tests/vm_matrix`: 4. Batch 25 made all
  `tests/corpus_eval/*` harness files locally Lizard-clean, passed corpus
  compile checks, canonical digest unit tests, and CLI help smoke checks for
  the corpus entrypoints. The refreshed full `tests/` Lizard inventory now
  reports 44 critical findings: `tests/interoperability`: 21,
  `tests/systemd_matrix`: 6, `tests/vm_matrix`: 4,
  `tests/benchmarks/systemd`: 4, `tests/datasets`: 6,
  `tests/conformance`: 2, and `tests/fss`: 1. Batch 26 made all
  `tests/interoperability/*` harness files locally Lizard-clean, passed Python
  compile checks, CLI help smoke checks for the touched interoperability
  entrypoints, and targeted interoperability smokes for closed-file, binary,
  zstd compression, compact, directory, mixed-directory, and live Go/stock
  cases. The refreshed full `tests/` Lizard inventory now reports 23 critical
  findings: `tests/systemd_matrix`: 6, `tests/vm_matrix`: 4,
  `tests/benchmarks/systemd`: 4, `tests/datasets`: 6,
  `tests/conformance`: 2, and `tests/fss`: 1. Batch 27 made the C systemd
  helper harnesses under `tests/conformance`, `tests/fss`,
  `tests/benchmarks/systemd`, and `tests/datasets/ingesters/systemd` locally
  Lizard-clean, passed FSS vector regeneration, systemd benchmark helper
  builds and smokes, direct conformance helper builds, and deterministic
  ingester validation across systemd, Rust, Go, Node.js, and Python. The
  refreshed full `tests/` Lizard inventory now reports 10 critical findings:
  `tests/systemd_matrix`: 6 and `tests/vm_matrix`: 4. Batch 28 made
  `tests/systemd_matrix/run_systemd_matrix.py` and
  `tests/vm_matrix/run_vm_matrix.py` locally Lizard-clean, passed syntax
  checks, CLI help smoke checks, a systemd-matrix summarize smoke, and a
  single-target VM validation smoke against existing repo-local raw data. The
  final local whole-repository Lizard run with `-C 12 -L 100 -a 12 -w .`
  completed with no warnings.
  Remaining work is to refresh non-complexity scanner inventories, record
  final scanner results, and complete whole-SOW review.

## Pending

- SOW-0047 - Netdata NetFlow SDK Integration: open. Component integration for
  NetFlow reader and writer paths after inventory, performance, and
  code-scanning gates.
- SOW-0048 - Netdata OTEL Writer SDK Integration: open. Component integration
  for OTEL writer paths after inventory, writer, and code-scanning gates.
- SOW-0049 - Netdata Reader Plugin SDK Integration: open. Component integration
  for OTEL signal viewer, no-libsystemd systemd journal reading, and static
  packaging after reader and code-scanning gates.
- SOW-0050 - Netdata Vendored Journal Removal: open. Final cleanup after all
  Netdata component integrations are complete.
- SOW-0065 - Parallel Language Parity Closure: open. Future per-language
  parity/performance closure after Rust is stable and corpus validation is
  complete, using isolated worktrees and one language per authorized agent if
  the user approves parallel implementation.
- SOW-0066 - V1 Release And Registry Publication: open. Final `v1.0.0`
  release, language registry/package publication, and clean consumer install
  validation after compatibility, portability, corpus, integration, and parity
  gates are complete.
- SOW-0081 - systemd-journal Plugin And Facets Specification: open. Documents
  Netdata `systemd-journal.plugin` and facets behavior in full detail before a
  replacement API is implemented.
- SOW-0082 - Rust Optimized Journal Explorer API: open. Depends on SOW-0081;
  implements a legacy-like optimized Rust API that fixes unnecessary
  compressed-DATA decompression, unnecessary field traversal, and repeated
  processing of deduplicated DATA objects.
- SOW-0083 - Index-Derived Facet And Histogram Optimization: open. Depends on
  SOW-0082; measures and implements optional index-derived facet and histogram
  strategies with break-even evidence from generated and real-corpus queries.

## Recently Closed Or Completed

- SOW-0075 - VM Historical systemd Validation: completed. Ubuntu
  18.04/systemd 237, Ubuntu 22.04/systemd 249, and Ubuntu 24.04/systemd 255
  VM-generated journals passed 18/18 cases with stock, Rust, Go, Python using
  repo-local `lz4==4.4.5`, and Node matching. RHEL 8.10/systemd 239 archived
  read-only validation also passed. Debian 11 is an accepted recorded blocker
  after SSH refused connections, QEMU guest agent was unavailable, no raw
  journals were generated, and the four-new-VM cap was exhausted. Five
  second-round read-only reviewers voted `PRODUCTION GRADE`.
- SOW-0076 - Independent Selective Real Corpus Verification: completed. The
  selective real-corpus runner now discovers real journal files read-only,
  selects representative sanitized feature classes, snapshots active files,
  compares systemd/Rust/Go reader digests, regenerates Rust/Go outputs in
  regular, compact, compact-zstd, and compact-fss modes, verifies generated
  files with stock journalctl, and writes sanitized JSON/Markdown reports. The
  recorded run selected 7 files from 7,195 discovered files and produced 77/77
  `ok` result rows with 0 discrepancies; five read-only reviewers voted
  `PRODUCTION GRADE`.
- SOW-0078 - Legacy jf Writer Unkeyed Rejection: completed. The legacy Rust
  `jf` writer remains public but now returns `UnsupportedJournalFile` before
  mutation when asked to append to historical unkeyed journal files. The same
  failure class found during review in the current `journal-core` append path
  was also fixed; five second-round read-only reviewers voted
  `PRODUCTION GRADE`.
- SOW-0079 - Directory Writer Reliable Active Replacement: completed. Rust,
  Go, Python, and Node.js high-level directory writers now treat
  append-incompatible or outdated active files like journald reliable-open:
  move the old active file to a collision-safe disposed `*.journal~` name and
  create a fresh active file. Low-level direct writer opens still return
  controlled unsupported-file errors; stock directory and cross-language
  matrices passed; five read-only reviewers voted `PRODUCTION GRADE` in the
  second whole-SOW review batch.
- SOW-0077 - Rust Historical Unkeyed Writer Rejection: completed. The current
  Rust writer stack now rejects historical unkeyed append-open and direct
  writer construction with `UnsupportedJournalFile` before entry mutation or
  assertion panic. Go, Python, and Node.js already had controlled writer
  rejection; historical reader support from SOW-0073 remains intact. Five
  read-only reviewers voted `PRODUCTION GRADE`; the related legacy `jf` writer
  assertion path is tracked by SOW-0078.
- SOW-0073 - Historical Unkeyed Journal Reader Parity: completed. A RHEL
  8.10/systemd 239 check found an unkeyed LZ4 journal that stock systemd
  verifies and reads. Go, Python, and Node.js reader-only keyed-hash gates were
  removed; Rust already selected keyed versus unkeyed hash by header flag. The
  v239 synthetic unkeyed/LZ4 offline and online matrices pass with current
  stock journalctl plus Rust, Go, Python, and Node.js matching 7 entries, 39
  payloads, and the same logical digest. Five read-only reviewers voted
  `PRODUCTION GRADE`; the Rust writer assertion follow-up is tracked by
  SOW-0077.
- SOW-0064 - Real World Journal Corpus Evaluation: completed after regression
  repair. The corpus harness, single-file repair work, focused 100-file
  real-corpus checks, raw reader/spool-writer experiments, systemd-version
  matrix, and sealed/FSS systemd-generated supplement are merged. Sealed/FSS
  historical coverage passed v252, v254, v258.8, v260.1, and v260.2 in regular
  and compact forms with 10/10 files passing and 0 discrepancies; durable
  reports store only sanitized counts, digests, command hashes, and FSS
  verification-key hashes.
- SOW-0027 - Netdata Reader API And jf Facade: completed after reopening two
  regressions. Field-name and unique-value enumeration use journal-native
  FIELD/DATA index traversal; Rust and Go now provide streaming unique-value
  visitor APIs used by file-backed `journalctl -F`; list-return APIs reuse the
  streaming path; Python and Node.js no longer do redundant same-file unique
  de-duplication; Rust's public default reader window is 32 MiB to avoid mmap
  churn; real-corpus high-cardinality benchmarks now match or beat libsystemd,
  and second-pass reviewers voted production-grade after the Go directory
  error-propagation fix.
- SOW-0055 - Rust Seek Cursor Systemd Parity: completed. Rust, Go, Python, and
  Node.js cursor conformance now covers found cursors, malformed cursor
  rejection, valid-missing cursor seek behavior, missing-cursor post-seek
  position, and Rust multi-file directory cursor positioning.
- SOW-0026 - Netdata SDK Integration Inventory And Cut Plan: completed. Netdata
  journal SDK integration inventory and cut plan are merged; no Netdata source
  edits were made. Component integrations remain mapped to SOW-0047 through
  SOW-0050.
- SOW-0063 - Cross Platform Portability: completed. Native Linux/macOS/Windows
  and repo-local QEMU FreeBSD validation passed for Rust, Go, Python, and
  Node.js; FreeBSD no-stock single-file and directory matrices passed; files
  generated on macOS, Windows, and FreeBSD passed Linux stock
  `journalctl --verify --file`; SOW-0071 and SOW-0072 blockers are completed.
- SOW-0071 - Runtime Purity And Optional Platform Services: completed. Core
  reader/writer paths in Rust, Go, Node.js, and Python no longer host-probe,
  execute subprocesses, or acquire writer locks implicitly; identity discovery
  and writer locks are optional helpers; legacy Rust `jf` host identity helpers
  were removed; runtime-purity scans cover core, facade, Python I/O helper, and
  legacy `jf` runtime files; Linux/macOS/Windows validation and three
  whole-SOW reviewer rounds passed.
- SOW-0067 - Go Cross Platform Portability: completed. Go SDK portability
  implementation, whole-SOW reviews, Linux/Windows tests, FreeBSD/macOS compile
  checks, and parent native macOS/Windows generated-file validation passed.
- SOW-0068 - Rust Cross Platform Portability: completed. Rust SDK portability
  implementation, whole-SOW reviews, Linux tests, Windows target checks, native
  macOS/Windows validation, and Linux stock verification of non-Linux generated
  journal files passed.
- SOW-0069 - Python Cross Platform Portability: completed. Python SDK
  portability implementation, whole-SOW reviews, Linux tests, import-safety
  checks, native macOS/Windows validation, and Linux stock verification of
  non-Linux generated journal files passed.
- SOW-0070 - Node Cross Platform Portability: completed. Node.js SDK
  portability implementation, whole-SOW reviews, Linux package tests, native
  macOS/Windows validation, Node.js `>=22.15.0` runtime-floor repair, and Linux
  stock verification of non-Linux generated journal files passed.
- SOW-0072 - Dependency And Package Hygiene: completed. Removed the hidden
  Node native-install dependency risk by vendoring only the XZ WASM runtime
  files with license and hash provenance, added package tarball hygiene and
  tests, strengthened Rust serde flattener parity/provenance, and updated
  reviewer-pool instructions.
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
  DATA at 532k rows/s. Rust remains faster; the cursor-seek systemd divergence
  discovered during review was closed by SOW-0055.
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
