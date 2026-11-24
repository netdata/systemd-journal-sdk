# Journal Compatibility Validation Recipes

Detailed controls accumulated from past SOW findings. They are not a checklist
for every change: read only the section matching the surface you are changing,
and use it to choose tests that cover the actual risk. Each control exists
because a past change got that case wrong.

## journalctl CLI parity controls

Use when changing the Rust or Go journalctl commands.

- The portable file-backed journalctl contract targets tag `v262`
  (8cc40e0c5e9234bf45084751ac53b1fbfe70b492). Run the pinned full-feature
  oracle; host versions and feature-limited builds cannot certify this target.
  Query-matrix qualification requires `--journalctl PATH`; every helper must use
  that selected oracle, including version reporting and empty-result cases.
- Native PCRE2-8 >=10.42 is CLI-private. Enable CGO only for Go CLI builds
  and tests; retain CGO-disabled SDK checks. Declare the C compiler, pkg-config
  and PCRE2 development prerequisites in command-inclusive coverage/CI.
- File-backed CLIs preserve native Unix path bytes through selection, facade
  opening and discovery. File patterns use platform globbing with a temporary
  thread-local environment locale; literal paths bypass expansion. Windows
  uses Unicode paths and brace/wildcard selection. Keep these native calls in
  CLI modules and retain pure SDK checks. Test replacement-named decoys and
  locale-sensitive patterns against the pinned oracle, with portable Unicode
  controls in native OS CI.
- Color parity includes policy/environment selection, priority and audit
  styles, PCRE2 first-match offsets, multiline preparation, verbose fields,
  JSON primitives, separators and table headers. Compare ANSI bytes as well
  as visible content; normalize JSON key order without discarding its styles.
  Retain buffered match offsets without a second regex evaluation and keep
  color-only work off the plain-output path.
- Parser recognition is insufficient. Compare encounter order and terminal
  actions, raw required values, empty-versus-absent options, final-action scope,
  timestamps/DST/TZif state, binary MESSAGE behavior, status and complete output.
  Check calendar rendering independently from bound parsing: POSIX TZ, custom
  TZDIR, leap records, transition-free footers and malformed-rule fallback must
  agree in entry output, headers and tables. Omitted DST dates must exercise
  the configured posixrules file, absent-file defaults and transition flags;
  provider defaults alone do not establish native parity. Preserve raw epoch widths and
  lazily resolve rendering zones once, without per-row file reads or zone allocation.
  Do not let intentional follow termination hide a child failure or a blocked
  stdout pipe. Retain native exit status and drain large backlogs to files.
- Use native OS jobs for commands with native dependencies. A CGO-disabled
  cross-build of the SDK does not establish native CLI portability.
- Audit complete CLI selector components after a verified mismatch. Unit show
  groups and invocation-discovery groups differ; test naming/glob boundaries,
  raw field bytes, current-user conversion, ID precedence, global bounds,
  directional limits, secondary-ID offsets and frozen follow selection.
  Compare supplied oracle binaries explicitly, including live command builders.
  Exercise platform fallback matchers on the primary test host. FreeBSD unit
  brackets need the shared CLI compatibility matcher: native fnmatch lacks
  named classes and collating elements. Validate malformed brackets as well
  as positive matches, multibyte-to-byte retry, and native OS execution. Keep
  ordinary patterns on native fnmatch without compatibility allocations.
  Use indexed
  eligibility and metadata getters; do not expand unrelated message payloads.
- For journalctl parser or option-surface changes, run
  `tests/parser-parity/check_v260_manifest.py` and
  `tests/parser-parity/run_parser_parity.py`. Parser parity must cover option
  names, short aliases, output modes, parser-required conflicts, recognized
  options interspersed after show-action match arguments, stock short-option
  attached values and clusters such as `-n2`, `-ball`, and `-rn2`, and
  optional-argument edge cases where bare and explicit-empty forms differ, such
  as `--boot` versus `--boot=`, bare `--lines` versus `--lines=`, and bare
  `--case-sensitive` versus `--case-sensitive=`.
- CLI integer controls must exercise native base prefixes/octal, leading and
  trailing whitespace, signed 32-bit descriptor limits and explicit ID-offset
  signs. Preserve raw Unix match operands separately from raw grep patterns.
- Short-output controls must include multiline and distinct-suffix messages,
  width/line/byte thresholds and repeated MESSAGE values; grep/cat select the
  first value while short output selects the last. Prove bounded reads stop
  before further matching, boot observation precedes grep, and follow-tail
  selection/resume state includes nonmatching native rows.
- Check short MESSAGE preparation before printability/truncation: tabs,
  completed and malformed CSI/OSC sequences, CR boundaries and show-all raw
  controls. JSON duplicate controls must include a binary first occurrence;
  a byte array must not become the repeated-values collection.
- Cursor controls must compare final native position as well as selected
  output: grep misses, bound crossings, zero counts and after-terminal seeks.
  Verify follow cursor-file contents after a skipped suffix or no matches,
  rather than accepting file existence alone. Size parsing needs compound
  suffix, fraction, unit-order/case and overflow controls, including terminal
  help encounter order. Preserve raw source-option presence on Unix.

## Live growth and publication cadence

Use when changing writer publication, arena growth or live readers.

- Distinguish live mapping/access bounds from stable-file integrity checks. A
  writer may publish rounded arena capacity before physical allocation reaches
  it. Ordinary readers must still read committed entries safely; excluded-writer
  snapshots, strict verification and append-open retain declared-arena checks.
  Exercise the actual DATA-preparation window and physical growth boundary, not
  only completed appends. `tests/interoperability/run_live_growth.py` supplies
  deterministic regular/compact SDK writer-reader coverage; it supplements the
  stock/live feature matrix rather than replacing it.
- Writers expose a cross-language `live_publish_every_entries` publication
  cadence. The default `1` is the stock live-reader compatibility mode. `0`
  disables explicit SDK live publication, and `N > 1` publishes after every
  `N` appended entries for latency-tolerant consumers. Non-default modes must
  not be described as stock `journalctl --follow` compatible unless their exact
  mode is covered by live matrix evidence; they still must pass clean-close
  verify and final reads.
- The reusable live-concurrency harness is under `tests/conformance/live/`. Writer tests should use the configured monotonically increasing sequence field, default `LIVE_SEQ`, so stock readers prove complete ordered visibility.
- Stock reader harness adapters may retry transient active-writer `ENODATA` open/read failures or partial snapshots only while the writer is active. After the writer exits, final ordered reads and `journalctl --verify --file` must pass.

## Root retention and fixed-header validation

Use when changing directory writer retention, rotation, root inventory or header validation.

- High-level directory writers must apply configured retention once when an active writer is opened or created. Existing-active reopen and eager open enforce during construction; lazy archived-only construction remains side-effect-free until the first append opens the active file, then retention runs before the first entry is written. Active/current files must remain protected and normal retention deletion lifecycle events must be reused.
- Go's explicit `LogConfig.RootRetention` and Rust's
  `Config::with_root_retention(true)` are exceptions to the default
  machine-local lifecycle rules above: archived-only startup also maintains
  the owned root, file lengths and tail times govern deletion, and live files
  may be finalized for idle expiry or allocation-policy changes. Recovered
  active files require caller verification/exclusion. Use header-native
  `InspectRootRetention` before open and `Log.InspectRootRetention` for live
  query/status inventory (Rust exposes equivalent snake_case APIs); do not
  substitute Reader entry-vector expansion.
  Live inventory must reject missing/replaced active files instead of reporting
  false-empty success, and maintenance must validate it before mutation.
  Root close, disposal and rotation MUST share live-path/header ownership
  preflight before mutation or ownership detachment. Archive and lazy-creation
  destinations MUST be absent. Reject safely, preserve bytes and permit healthy
  rotation/creation retry after path restoration; rejected close releases only.
  Test changes completed between SDK calls, with caller exclusion during calls,
  and cover empty actives after policy setters as well as lazy successors.
  Preserve separate safe-maintenance errors and fatal archive-mutation errors:
  failed retired-file open/validation before mutation must not poison a healthy
  current writer. Detach outgoing ownership before archive mutations so even
  a post-rename failure leaves evidence inspectable. New root files, successors
  and allocation checks must share configured options rather than inheriting
  stale recovered layout. Exercise regular/compact transitions through both
  append APIs, pre/post-mutation failures, and archive-before-delete events.
  Fixed-header inventory MUST validate every header-addressed object extent
  against the declared arena and allocated object tail, including ENTRY-array
  and tail-entry hints. Reuse stable-header validation with append recovery and
  excluded-writer snapshots; corruption in array contents remains outside the
  header-only inventory contract. Exercise malformed header offsets in both
  languages and require failed preflight to preserve journal bytes.
  Stable validation MUST also enforce fixed-header object/category population
  bounds and cached offset/count agreement, using complete on-disk field gates
  for historical headers. The aggregate of present disjoint object categories
  MUST fit within the total, including one object for each declared DATA/FIELD
  hash table; those two object types have no dedicated count fields. Use checked
  addition or a remaining-object budget. Test exact and insufficient budgets
  with zero, one and two table declarations, including historical headers.
  A present ENTRY_ARRAY counter MUST cover the distinct nonzero first/tail
  array locations declared by complete header fields. Equal pointers count once;
  additional arrays are permitted. Test historical counters before cached-tail
  fields exist and preserve bytes on inventory/startup rejection.
  Test small in-bounds contradictions, not only overflowing offsets. Apply these counter checks to stable files only, not to live mapping validation.

## Byte identity and historical files

Use when changing deterministic writer layout, header parsing or historical-file support.

- For deterministic regular uncompressed writer output, the layout target is byte-for-byte identity with the systemd v260.1 reference ingester for the accepted corpus. Writers must match systemd object order, alignment, initial allocation envelope, v260 header fields, entry-array growth, tail metadata, and hash-chain header behavior for that slice.
- Deterministic byte-identity validation must cover systemd final-state variants: online/plain close, offline close, and archived close.
- Header readers must use the on-disk `header_size` when validating object and hash-table locations. Do not compare historical file offsets against the current in-memory v260 `JournalHeader` struct size, and do not expose bytes beyond the on-disk header as newer header fields.
- Readers must not require `HEADER_INCOMPATIBLE_KEYED_HASH` for opening existing historical files. If that flag is absent, readers must use the unkeyed Jenkins lookup3 hash path and still support applicable compression flags such as LZ4. Writers may remain conservative when appending and reject unkeyed historical files unless a SOW explicitly proves safe mutation of that historical format.
- Writer append-open rejection for historical unkeyed files must be controlled and must occur before entry mutation. Do not leave assertion panics or partial state changes as the user-facing failure mode.
- High-level directory writers own reliable active replacement. If a low-level
  append-open rejects an existing active file as unsupported, the directory
  writer must follow systemd's reliable-open shape: use readable header
  metadata to continue sequence identity where possible, move the old active
  file out of the normal `.journal` set with a collision-safe `*.journal~`
  disposed name, and create a fresh active file. Do not archive unsupported
  active files into normal readable history.

## Indexed aggregation and engine cache

Use when changing Explorer/indexed aggregation or journal-engine index semantics.

- Changes to `journal-engine` index semantics, `FileIndexKey`, serialized index
  contents, cache schema, or consumer-visible cache partitioning must explicitly
  decide whether `CACHE_VERSION` changes. If bumped, record the one-time
  reindex/invalidation impact in the active SOW, product scope, and release
  notes. If not bumped, record why old cache entries remain semantically safe.
- Indexed aggregation changes must force temporary DATA reuse during nested
  posting and ENTRY reads. From `go/`, run
  `go test -count=1 ./journal -run '^TestExplorerIndexedWindow'` for both layouts,
  all DATA compression modes,
  mmap/auto eviction and ReadAt scratch reuse. Keep possible mmap faults in child
  processes. Compare full results and indexed counters, including empty/binary
  values, singleton DATA, unset/repeated values, time filters, bucket integer
  boundaries and partial read-error counters. A small fixture or
  a successful zero-row query does not establish the default-window lifetime.
  Benchmark matching and unmatched low/high-cardinality queries with
  `BenchmarkExplorerIndexedValueOwnership`; record allocations separately from
  elapsed time. Use `BenchmarkExplorerIndexedCompressedValueOwnership` with an
  explicit `JOURNAL_SDK_EXPLORER_COMPRESSED_CORPUS` for prebuilt codec controls;
  check indexed/traversal outputs, actual decompression and update totals before
  timing. Freeze the same complete test helper for baseline and candidate, and
  exclude fixture construction and reader teardown from timing. An incorrect or
  crashing baseline cannot certify a speedup.

## Go reader micro-path controls

Use when changing Go ENTRY-slot headers, native file opening or constructor statistics.

- Inspect CLI-helper timer boundaries before interpreting profiles or batching.
  The Go reader helper opens/closes a fresh reader and checksums/sorts results
  on every `--loops` iteration; its final ExplorerStats describe the last loop.
  Pilot one iteration before selecting a batch count for each workload. Keep
  CPU-profile post-timer activity and heap-profile forced GC separate from
  query-cost claims; sampled cumulative allocation is not peak live memory.
- For Go ENTRY-slot header acquisition, run `TestEntrySlotHeader` controls on
  both backends, default/tiny/non-power-of-two windows, boundary headers,
  partial errors, closed readers, row pins, eviction and live array growth.
  Measure `BenchmarkReaderOpen`, `BenchmarkReaderEntryHeader` and
  `BenchmarkReaderAllocation` with the same complete header benchmark helper
  in both builds. Record actual allocation classes with
  `TestReaderAllocationShape`; raw sizeof does not establish heap cost, and
  64-bit layout preservation does not establish 32-bit layout preservation.
  Exclude candidate-only direct-helper correctness tests from both timing
  binaries, document that exclusion and run those tests separately. Preserve
  the baseline validator's policy when isolating acquisition changes from
  other paused work, and verify matching pure-build flags and source hashes.
  For uncertain small-open timings, compare the same immutable five-entry
  corpus using `JOURNAL_SDK_READER_OPEN_SMALL_CORPUS` and retain a generated-file
  control with those same binaries. Preserve earlier results: a changed helper
  can alter test linking even when production source is byte-identical.
- For Go native-file opener changes, run `TestNativeReaderFile` uncached on
  amd64 and 386. Verify read-only/blocking/close-on-exec flags, typed path
  errors, symlink identity, descriptor closure and sparse reads above 2 GiB.
  Preserve interrupted-open retry, native malformed-file cleanup and existing
  `.zst` source/staging behavior; build non-Linux delegation targets. The direct
  Linux opener bypasses Go's internal test-cache input hook, so use `-count=1`
  for fixture-dependent tests and retain explicit immutable fixture hashes.
  Exclude candidate-only opener contract tests from both timing builds, as for
  direct header tests. Primitive syscall savings do not establish full-reader
  performance; repeat small, large and higher-level reader/query controls.
- For cached constructor statistics, run `TestReaderAccessorInitial` for
  empty/nonempty files and initial mmap failure. Verify explicit counters,
  selected/requested modes and fallback reason against a fresh statistics
  query. Windows zero-byte raw files fail explicit mmap construction and Auto
  falls back to ReadAt; preserve that platform behavior in test expectations.
  Exclude candidate-only statistics tests from both timing builds.

## Reader hardening, recovery and filter positioning

Use when changing ordinary traversal, compressed readers, recovery, cursors or filters.

- For ordinary traversal or compressed-reader changes, build both conformance
  adapters and run `tests/interoperability/run_reader_hardening.py` against an
  explicitly selected file-backed stock oracle. Use a new `.local/` output
  directory. The paired corpus covers cycles, missing required posting links,
  acyclic compatibility controls, facade/directory callers, complete Zstandard
  streams and whole-file staging. `--only chains --only streams` does not
  validate public crash recovery. Strict verification is a separate gate;
  ordinary stock corruption handling can differ from the SDK's completeness
  errors, so record the actual expected contract for each fixture.
- For recovery changes, test fresh exact queries as well as queries after a
  full unfiltered drain. Earlier traversal can hide a missing-pointer bug by
  activating recovery first. Include sparse postings, same-size unpublished
  tails, partial arrays, forward/reverse/time positioning, directory refresh,
  and strict capture/append rejection. Release DATA guards before cold recovery
  reads. Do not replay controlled Explorer callbacks or sampling state to retry
  a failed indexed operation.
- Reader recovery must not mutate source journals. Separate static damaged-file
  extraction, actual writer publication, and synthetic fault injection in test
  evidence. Restoring previously committed bytes is not evidence of journald
  publication or a required repair lifecycle. Before adding state or repeated
  scans for a restore/retry scenario, establish its supported upstream trigger.
- Recovery EOF controls must separate repeated same-direction/count polls from
  reversal, explicit seeks and actual append count changes. Check bounded read
  work on an unchanged damaged file and verify source bytes remain unchanged.
  Cover raw as well as filtered readers; Go explicit Refresh is a separate path.
- Count-bounded ENTRY-array traversal must reject the first required revisit;
  a lazy unbounded-cycle guard alone can miss it before the count completes.
  Cover short cycles, descending-prefix/ascending revisits, public Rust Node
  iteration and writer tail selection before mutation. Preserve acyclic
  backward links and ignore reader links beyond the declared population.
  Prefix checks may inspect only already visited arrays, not future headers.
- Requalification must use one stable observed recovery view across every
  Boolean branch. Test a failure after an earlier branch was clipped; do not
  install a partially qualified expression. Synthetic ENTRY/DATA restoration
  can exercise rollback but does not establish a production repair contract.
  Pending builders and installed expressions
  must retain retry state and known DATA identities, including empty lists.
  Installed filters must observe a healthy append, including a value absent at
  install time and an append after a failed backward preparation, as libsystemd
  does (SOW-0045); a raw Rust `FilterExpr` stays a resolved snapshot. Check
  public expression composition
  where exposed. Clearing matches must rewind file/directory/facade readers
  without an explicit seek; test this after multiple sparse matches.
- Filter position controls must cover installing at a resolved row, reversal
  after EOF and early AND/OR exhaustion, duplicate realtime bounds, and failed
  selected ENTRY loads followed by getters and transient I/O retry. Test both SDK and
  facade APIs, regular/compact layouts and Go access backends. Directory
  controls must include sequential file transitions, partial merge prefetch,
  initial unset errors and pending realtime seeks; preserve candidate order
  through complete retry/EOF traversal. Explicit filtered realtime search checks
  visited posting slots; separately prove ordinary traversal still encounters
  damaged skipped slots. Use probe counts plus isolated release comparisons to
  establish search work and elapsed cost respectively.
  Test recovery first discovered while sequential traversal crosses files,
  followed by a later merge-probe error. Internal merge restarts
  must reuse the inner routine; one public movement owns rollback of its
  selected child without discarding other prefetched candidates.
  Install and replace filters at a committed directory row, including group
  operators. Preserve pending seeks after an initial failed load. Prove native
  sequential discovery with an unvisited later-file error: nearer rows must
  remain readable before that file is reached. Raw/filtered mode changes must
  reuse proven sequential positions as well as filter replacements.
  Audit strict index consumers for suppressed new traversal errors and cached
  partial results. Exercise requested facet/source posting chains, positive
  counts with missing pointers and actual cache omission with a healthy sibling;
  retain documented value tolerance and propagate required traversal failures.
- Exercise recovery through public default constructors, not only a private
  enablement switch. Cover file/directory/facade and whole-file compressed input,
  normal EOF plus observed status, unchanged source bytes and declared counters,
  and strict verification/capture/append rejection. Test live publication with
  actual writers separately from synthetic restoration. A zero status is not a
  full-index integrity result. Preserve errors for unknown secondary posting
  tails and malformed compressed streams.
  Run `TestRecoveryEqualCountFacetWithoutPostings` and measure
  `BenchmarkRecoveryUnmatchedFacet` when changing Go facet shortcuts: incomplete
  unused array capacity can leave visible and declared counts equal while
  suppressing postings. Keep the large orphan value out of unmatched key copies;
  record allocation evidence rather than inferring it from correct results.

## Performance comparisons

Use when making a reader or writer performance claim.

- Relocated baseline/candidate source copies must include the tests' referenced
  fixture and helper files at their original relative paths. Check those
  dependencies before running the suite and verify copied bytes. Select Cargo
  packages from their manifest names, not directory names or dependency aliases.
- Before reader performance acceptance, freeze matching helper sources against
  baseline and candidate SDKs. Keep immutable fixtures and complete semantic
  counters. `tests/benchmarks/run_reader_regression_comparison.py` alternates
  order and retains per-process output, measured runs and comparisons even when
  a later case fails. An unsuccessful baseline is not a valid timing reference.
- Select representative performance controls from the actual changed work.
  Careful code review and high-level tests can suffice for unchanged hot work
  or cold-only checks. Use targeted timing/profiling for credible added costs
  and observed regressions; do not mechanically expand every API, layout and
  backend combination or build a large framework for small changes. Retain
  adverse evidence, and distinguish uncertainty from established equality.

## Vacuum ordering

Use when changing archive vacuum ordering.

- Exercise valid cross-history clock rollback, reversed creation order, ties
  and unsequenced archives. Native sorting can have contradictory comparisons;
  the approved deterministic history-head merge is an explicit parity exception.
  Compare ordinary compatible histories with the pinned oracle and ambiguous
  histories with the documented policy, including actual retained files.
- Retain count, size and age controls and single/many-history cost evidence.
  Do not catch Rust sort panics or blindly substitute another sorting algorithm.
