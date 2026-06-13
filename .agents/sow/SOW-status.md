# SOW Status

Last updated: 2026-06-11

## Current

- SOW-0105 - Node.js Explorer And Netdata Parity To Rust: in-progress.
  Activated after SOW-0104 completed; gate refreshed with a project-
  manager-verified API-diff inventory (two inventory claims refuted
  against code before entering the gate). Ports Explorer, the Netdata
  function API, the stdin wrapper, source selector labels, and the
  facade unique-values visitor to Node.js; adds hand-written `.d.ts`
  with CI type-check; joins the Netdata comparator matrices; inherits
  the SOW-0104 porting playbook with frontloaded Rust mechanisms.
- SOW-0009 - Benchmark Profile Optimize: paused umbrella. Writer and reader
  performance work is split into focused child SOWs; this file remains the
  program index.

## Pending

- SOW-0108 - Node Reader Memory Architecture: open. The Node reader loads
  whole files into resident memory (`readFileSync`) and OOMs on real large
  journals (a live-journal query reached ~49 GB RSS and was OOM-killed),
  whereas Rust uses rolling-window mmap and Go/Python use demand-paged mmap
  plus positioned-read fallbacks. User decision (2026-06-14): add a unified
  `Accessor` abstraction with a pure-JS positioned-read rolling-window default
  and an OPTIONAL native mmap backend (`@riaskov/mmap-io`, consumer-installed,
  never an SDK dependency, dynamically loaded only when `accessMode: Mmap`) -
  the single approved exception to the no-native-addon purity policy, due to
  the nature of Node. Blocked on SOW-0105 close.
- SOW-0107 - Python And Node Explorer Engine Parity Gaps: open. Discovered
  during SOW-0105 round-2 review: the Rust `ExplorerSamplingState` budget-based
  sampling/estimation engine is unported in both the Python and Node Explorer
  traversals (only the data structures and stats plumbing shipped). Zero
  observable impact on every validated fixture/gate because no fixture exceeds
  the sampling budget; needs a high-row fixture to validate and a faithful
  engine port in both languages.
- SOW-0047 - Netdata NetFlow SDK Integration: open. Component integration for
  NetFlow reader and writer paths after inventory and performance gates.
- SOW-0048 - Netdata OTEL Writer SDK Integration: open. Component integration
  for OTEL writer paths after inventory and writer gates.
- SOW-0049 - Netdata Reader Plugin SDK Integration: open. Component integration
  for OTEL signal viewer, no-libsystemd systemd journal reading, and static
  packaging after reader gates.
- SOW-0050 - Netdata Vendored Journal Removal: open. Final cleanup after all
  Netdata component integrations are complete.
- SOW-0104 - Python Explorer And Netdata Parity To Rust: open. Activates after
  SOW-0103. Ports Explorer, Netdata function API, stdin wrapper, and source
  selector labels to Python; joins the Netdata function comparator matrices;
  adds `pyproject.toml` (no publication).
- SOW-0105 - Node.js Explorer And Netdata Parity To Rust: open. Activates
  after SOW-0104. Same parity surface for Node.js plus hand-written `.d.ts`
  TypeScript definitions with CI type-check; pure JS, no native addons.
- SOW-0106 - Python And Node.js Docs With Verified Examples: open. Activates
  after SOW-0105. Adds Python-API/Node-API wiki pages and Python/Node columns
  to shared pages; extends the verified-examples harness to all four
  languages.
- SOW-0066 - V1 Release And Registry Publication: open. Final `v1.0.0`
  release, language registry/package publication, and clean consumer install
  validation after compatibility, portability, corpus, integration, and parity
  gates are complete.
- SOW-0094 - Rust Explorer Lazy Compressed Field Inference Experiment: open.
  Deferred Rust Explorer optimization experiment to skip compressed DATA until
  uncompressed row DATA are examined, infer compressed DATA field identity from
  cached `next_field_offset` chains when possible, and decompress only when
  facets or histogram requirements still need it; blocked until SOW-0093
  stabilizes and promotes the Explorer API.
- SOW-0097 - Go Codacy Metric Debt Refactor: open. Follow-up from SOW-0096 to
  reduce Go production file-size/ownership and duplication metrics only where
  the refactor improves maintainability without hurting compatibility or
  performance.
- SOW-0098 - Rust Legacy Core Duplication Debt: open. Follow-up from SOW-0096
  to analyze and reduce real Rust `jf`/`journal-core` duplication only where
  shared primitives preserve historical compatibility and reader performance.
## Recently Closed Or Completed

- SOW-0104 - Python Explorer And Netdata Parity To Rust: completed. The
  Python SDK now carries the full Rust feature surface: Explorer
  (filters, facets, histogram, FTS, Traversal/Index/Compare strategies,
  control callbacks), the Netdata logs function API (profiles, SOW-0102
  source selector labels, the complete request/response contract
  including verbatim window normalization, delta keys, the SOW-0093
  tail contract, and sampling), the stdin function wrapper,
  `python/pyproject.toml` (metadata only), and the facade unique-values
  visitor. Parity proven by three-peer content comparison against the
  Rust wrapper and the installed Netdata plugin: 10/10 one-shot
  fixtures on the live journal (read-only) and 5/5 stateful sequences
  on a frozen fresh-data fixture; the comparators gained two documented
  bounded tolerances for invocation-time content and a frozen-fixture
  protocol for slow third peers. All five pool reviewers returned
  `PRODUCTION GRADE: YES` in a single round; audit clean. SOW-0105
  (Node parity) activates next.
- SOW-0103 - Docs API Perception Restructure And Verified Examples:
  completed. The consumer wiki is restructured around API-perception
  decision paths with the journalctl rewrite CLI documented as the fifth
  consumption surface (new `Journalctl-CLI` page), and every Rust and Go
  wiki example is now machine-verified: 31 examples compile against the
  local workspace/module and run against synthetic fixtures via
  `tests/docs/verify_examples.py`, enforced by the extended wiki validator
  and the new `docs-examples.yml` CI workflow. A new
  `project-docs-authoring` skill records the authoring rules. Reviewer
  round 3 returned 5/5 `PRODUCTION GRADE: YES` (glm, kimi, mimo, qwen,
  deepseek); audit clean. First SOW of the 2026-06-11 docs-and-parity
  program; SOW-0104 (Python parity) activates next.
- SOW-0065 - Parallel Language Parity Closure: closed without implementation
  on 2026-06-11, superseded by the user-approved docs-and-parity program
  SOW-0103 through SOW-0106. All prerequisites had completed and Go parity was
  already delivered by SOW-0095/SOW-0102, so the remaining Python and Node.js
  scope moved into focused sequential child SOWs. The user resolved the open
  execution-topology decision: sequential SOWs, no worktrees, external
  implementer `llm-netdata-cloud/minimax-m3-coder`, all other
  `llm-netdata-cloud` pool models as read-only reviewers.
- SOW-0102 - Netdata Function Source Selector Labels: completed. Rust and Go
  Netdata function configs now expose source selector name/help metadata for
  the stable `__logs_sources` wire id, preserving `Journal Sources` defaults
  while allowing consumers such as SNMP traps to show domain wording like
  `Trap Jobs`. Focused Rust and Go tests passed, docs/specs were updated, and
  all six approved reviewers returned `PRODUCTION GRADE`. Rust crates were
  published to crates.io at `0.6.4`, and release tags are `v0.6.4` plus
  `go/v0.6.4`.
- SOW-0101 - Netdata Function Stateful Equivalence: completed. Added stateful
  SDK-wrapper versus installed Netdata `systemd-journal.plugin` side-by-side
  tests for anchors, forward/backward paging, tail 304 behavior, filtered tail
  empty-200 behavior, and delta facets/histograms. Final validation passed 10/10
  one-shot request fixtures plus all five stateful sequences; Rust and Go also
  have focused boundary tests for the repaired wrapper behavior. Rust crates
  were published to crates.io at `0.6.3`, and release tags are `v0.6.3` plus
  `go/v0.6.3`.
- SOW-0093 - Netdata Function Boundary Reader Comparison: completed after
  tail-anchor regression repair. Rust and Go now use libnetdata-compatible
  tail stop-anchor semantics, backward page anchors are exclusive, tail
  no-change returns `304`, focused paging/tail/delta contract tests pass, five
  available reviewers returned `PRODUCTION GRADE`, Kimi was unavailable due
  quota, Rust crates were published to crates.io at `0.6.2`, and release tags
  are `v0.6.2` plus `go/v0.6.2`.
- SOW-0100 - Consumer Docs And GitHub Wiki Publication: completed after
  regression repair. GitHub wiki navigation now uses `[[Target|Label]]` wiki
  links, the wiki has professional API overview plus Rust and Go language
  guides with examples, and the docs validator rejects production `*.md` wiki
  links while allowing fenced anti-pattern examples.
- SOW-0099 - Rust crates.io Publication: completed. Rust SDK packages were
  published to crates.io at `0.6.0` under `systemd-journal-sdk` and
  project-prefixed internal package names; release tags are created on the SOW
  close commit.
- SOW-0096 - Codacy Metrics And Coverage Hygiene: completed. Go and Rust
  coverage reports now remove test/test-harness paths before Codacy upload;
  Python and Node.js coverage were verified source-scoped. The Rust/Go
  file-by-file Codacy metrics audit is committed, GitHub code scanning has zero
  open alerts on final implementation commit `7e3d3e5d`, Codacy reports
  `issuesCount = 0`, coverage `73%`, complexity `46%`, and duplication `30%`.
  Remaining production metric debt is tracked by SOW-0097 and SOW-0098.
- SOW-0084 - Code Scanning And Codacy Gate: completed after regression repair.
  GitHub CodeQL alert `3341` is closed on head `1d7006ae`; GitHub code
  scanning has zero open alerts; Codacy Cloud reports `issuesCount = 0` and
  `codacy issues` returns zero issues on the same head.
- SOW-0095 - Go Explorer And Netdata Function Parity: completed. Go now
  exposes Explorer, generic Netdata logs function, and stdin-based Netdata
  function wrapper APIs as Rust peers. The committed 10-request comparator
  matrix passes 10/10 SDK-first Go-wrapper vs Rust-wrapper cases, the larger
  SOW-0093 `/var/log/journal` request passes 3/3 content comparisons with Go
  averaging `3.534s` and Rust averaging `2.929s`, focused Go Netdata parity
  tests cover 14 request/source/profile/progress/timeout/sampling/time
  behavior clusters, and all approved reviewers returned
  `PRODUCTION GRADE: YES`.
- SOW-0082 - Rust Optimized Journal Explorer API: completed after regression
  repair. Normal Netdata-shaped Explorer queries now use one candidate-row
  traversal for rows, facets, and histogram; the Netdata function wrapper keeps
  cursor-only row candidates, truncates to the final global row limit, and then
  expands only selected rows. The 4 GiB journal-window request now matches the
  installed `systemd-journal.plugin` semantically, has `returned_row_expansions`
  200, and measured 83,876 KiB maximum RSS on the final cold-I/O run. Five
  final reviewers voted `PRODUCTION GRADE`.
- SOW-0083 - Index-Derived Facet And Histogram Optimization: completed. Rust
  now exposes explicit `ExplorerStrategy::{Traversal, Index, Compare}` controls
  for the explorer API. `Traversal` remains the default, `Index` is exact only
  for all-values, no-FTS, commit-realtime query shapes, and `Compare` returns
  traversal/index timing and counter diagnostics after logical equality
  verification. Benchmarks show large wins for narrow unfiltered all-values
  facets and histogram-only queries, but regressions for many facets and
  selective filters, so no auto planner was added. Five final reviewers voted
  `PRODUCTION GRADE`.
- SOW-0081 - systemd-journal Plugin And Facets Specification: completed.
  Added an evidence-backed specification for Netdata `systemd-journal.plugin`
  and facets behavior, documented the code-vs-README default timeframe
  discrepancy, separated generic SDK explorer semantics from Netdata-specific
  adapter policy, mapped SOW-0082 and SOW-0083 follow-up, and completed final
  reviewer closeout with no remaining content blockers.
- SOW-0092 - Rust Row Pin Hostile File Bound: completed. Rust row-pinned
  mmap windows are now capped at the normal rolling-window budget; hostile
  overflow DATA falls back to row-scoped boxed storage, normal uncompressed
  rows remain zero-copy, low-limit and hostile pressure tests pass, and five
  final reviewers voted `PRODUCTION GRADE`.
- SOW-0091 - Rust Row View Adoption: completed. Rust `CurrentRowView` now
  serves remaining SDK visitor, owned-entry, engine projection, and index query
  row-oriented DATA paths; `sdk-payloads` improved on every large-file
  candidate with +8.9% median, `sdk-entry` improved on every candidate with
  +8.6% median, and five final reviewers voted `PRODUCTION GRADE`.
- SOW-0090 - Rust Reader Header Snapshot Cache: completed. Rust `FileReader`
  now captures read-only header snapshot metadata at open, uses it for snapshot
  headers, directory ordering, boot metadata, facade boot listing, and fallback
  cursor/key construction, while live `header()` refreshes from the mapped
  header and `journal-core` writer-visible header behavior is unchanged; five
  reviewers voted `PRODUCTION GRADE`.
- SOW-0089 - Rust Compressed DATA Reuse: completed. Added an internal Rust
  benchmark mode that measures compressed DATA reuse by offset and algorithm,
  proved the available large-file corpus has too little repeated compressed
  DATA to justify a production decompressed-DATA cache or reusable Zstandard
  context, and left production Rust reader paths unchanged; five reviewers
  voted `PRODUCTION GRADE`.
- SOW-0088 - Rust Offset Array Cursor Cache: completed. Rust offset-array
  cursor movement now caches scalar node metadata/current values, avoids
  same-node node rebuilds, avoids repeated reverse head-to-current walks through
  lazy node-chain reuse, fixes a `collect_offsets()` remaining-items bug, and
  adds forward/backward multi-node traversal coverage. Final large-file
  benchmark evidence shows forward `core-next` improved on 5/6 candidates and
  forward `core-offsets` improved on 5/6 candidates; five reviewers voted
  `PRODUCTION GRADE`.
- SOW-0087 - Rust Core Row View Primitive: completed. Rust current-row reader
  ownership now lives in a `journal-core` `CurrentRowView` primitive used by
  `FileReader` and facade DATA enumeration; public Rust API shape is
  unchanged, unused internal row-view surface was removed, large-file
  `facade/sdk` ratios improved from 0.751-0.884 to 0.874-0.933, and five final
  closeout reviewers voted `PRODUCTION GRADE`. Remaining callback/owned-entry
  row-view adoption is tracked by SOW-0091, and hostile row-pin bounds are
  tracked by SOW-0092.
- SOW-0086 - Rust Reader Performance Contract And Gap Analysis: completed.
  Established the Rust reader performance contract, added the Rust reader
  performance spec, implemented row-level mmap-backed payload lifetime for
  uncompressed DATA, added the compressed current-row arena path, improved
  facade metadata/data hot paths, added native zstd decompression with pure
  fallback, and mapped remaining Rust reader performance work to SOW-0087
  through SOW-0092. Final benchmark evidence shows Rust facade DATA enumeration
  faster than systemd DATA enumeration on every measured candidate, from 1.56x
  to 3.61x. Five read-only reviewers voted `PRODUCTION GRADE`.
- SOW-0085 - Codacy Coverage Reporting: completed. GitHub Actions now
  generates Rust, Go, Python, and Node.js coverage reports, uploads partial
  reports to Codacy with the selected account-token environment, and finalizes
  the Codacy coverage report. Final validation: GitHub Coverage run
  `26941281896` succeeded, Codacy analyzed commit `a822d23d`, coverage is
  62.0%, issues are 0, and security findings are 0.
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
