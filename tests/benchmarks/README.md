# Benchmark Harnesses

This directory has three writer benchmark surfaces:

- `run_writer_core_benchmarks.py` measures the actual append loop. Each driver
  pre-materializes deterministic rows before timing, creates the writer before
  timing, stops timing immediately after the last append, and reports final
  close/sync separately.
- `run_writer_directory_benchmarks.py` measures high-level directory `Log`
  writing with active-file rotation. It uses the same pre-materialized rows,
  keeps final close, stock verification, and stock directory readback outside
  the append timer, and records generated file counts and total journal bytes.
- `run_writer_benchmarks.py` measures deterministic JSONL ingestion. It is
  useful for end-to-end stress and corpus ingestion checks, but its rows/sec
  includes JSON parsing, value materialization, and caller allocation overhead.

Use writer-core results when comparing SDK writer performance against systemd
and Netdata hot-path expectations.

`run_reader_core_benchmarks.py` measures read loops separately from fixture
generation. It produces a compact/regular fixture with the Rust writer, then
times single-file Rust core scans (`core-next`, `core-offsets`,
`core-payloads`), Rust/Go SDK/facade entry scans, and public libsystemd
`sd_journal_*` scans. It also generates an explicit multi-file fixture for
ordered `open-files` regression coverage. Treat the single-file `core-payloads`
and `sdk-payloads` cases as the closest Netdata low-level reader hot-path
proxies.

Rust reader benchmark modes separate API cost:

- `sdk-payloads` visits borrowed current-entry `FIELD=value` payload bytes and
  is the SDK hot path for byte-level scanners.
- `facade-data` measures libsystemd-style restart/enumerate data behavior.
- `sdk-entry` materializes convenience maps, repeated-value maps, owned
  payloads, and cursor strings, so it should not be used as the raw scanner
  baseline.

Rust reader benchmark results record `bounds`. `live` is the default
active-file-compatible mode. It follows libsystemd's cached mutable bounds
model: it refreshes cached file size only when a read would exceed the cached
end of file. `snapshot` fixes file size at open time for polling/query
consumers that do not need to observe appends during the current scan.

Both reader helpers provide separate `sdk-digest` and `facade-digest` preflight
modes for full ordered payload checks. Their `full-payload-fnv64-v1` checksum
consumes every byte, payload length and row/time marker. Normal scan timing modes
retain `sampled-payload-v1`, which examines length and endpoint bytes. Compare
the diagnostic counters across the retained implementations before timing;
diagnostic hashing time is not a normal scan measurement.

The file SDK diagnostic mode also records final access/mmap counters before
closing. Go's existing `reader_access_stats` value is captured before scanning;
`reader_access_stats_after` covers the completed diagnostic read. Directory and
facade handles do not expose these counters. When Go `--loops` is used, `extra`
describes the final reader instance, not a cumulative total across loops.

Ordinary native filtering has separate `sdk-filter` and `facade-filter` modes.
They return ordered timestamp markers without requesting DATA values. Repeated
`--match` arguments use the SDK's same-field OR / different-field AND rules;
`--match-hex` accepts binary match bytes. `--filter-repetitions` repeats seeks on
one reader, `--filter-limit 1` isolates first-result work, `--filter-rebuild`
installs a fresh expression before each repetition, and `--filter-realtime`
sets a time anchor. The corresponding `*-filter-digest` modes consume all
selected payload bytes as an independent preflight. They belong outside timing
comparisons of metadata-only movement.

Go reader results record `mmap_strategy`. `mmap` is the default SDK reader
access mode on Unix and is the production hot path. `read-at` remains in the
harness as an explicit comparison and diagnostic mode.

The Go reader benchmark helper also accepts `--cpuprofile`, `--memprofile`,
and `--loops` for targeted profiling outside the full benchmark harness. Keep
profile outputs under `.local/benchmarks/`; `--loops` repeats the selected
read case inside one process to collect enough samples and is not used by the
shared checksum-comparison harness.

`run_reader_regression_comparison.py` compares retained reader helper binaries
on immutable existing fixtures. Pass both languages' `--baseline-*` and
`--candidate-*` binaries, one or more `--fixture` paths, and a new `--out`
directory under `.local/`. Build each pair with matching benchmark-helper
source and compiler settings; retain the original SDK implementation in the
baseline. The runner records binary/fixture SHA-256 hashes and rejects changes
during measurement.

The default is two warmups and ten measured baseline/candidate pairs. Order
alternates between pairs. Every run must return the same complete record,
field, byte and checksum counters. `--case` selects case-name substrings;
`--cpu` pins the harness and children to an available CPU; `--batch` repeats
child measurements within each phase and compares their mean read time.
Catalog cases exercise native FIELD/DATA enumeration, including stateful
restarts, without row scans. `--surface directory` applies the supported SDK,
facade and catalog cases to directory fixtures; the directory fingerprint
includes relative file names and content hashes.

`--surface open-files` treats a directory fixture as an explicit list of its
journal files, fingerprinting the complete directory. The ordinary-filter
`--filter-suite` uses the deterministic mixed-cardinality corpus: dense, sparse,
absent, same-field OR and different-field AND selections, fresh first results
and repeated boundary restarts. It covers both SDK/facade surfaces, bounds and
directions. `--filter-time-usec` adds repeated realtime seeks at an explicit
corpus timestamp that must produce a result in both directions. An independent
native-byte parser computes ordered timestamp checksums before timing and saves
`filter-goldens.json`; every language and build must match them. This preflight
requires native uncompressed mixed-cardinality fixtures. Multi-file inputs must
have distinct realtime values and sequence/boot order consistent with realtime;
validate other merge orders separately. The AND
control intersects overlapping same-field alternatives, with an outcome that
differs from either group and their union.

Metadata-only filter timings must be preceded by equivalent full-payload digest
correctness controls. `fresh-first` rebuilds the filter on one open reader;
`restart-first` repositions the installed filter. Neither repeats reader opening.
Each helper timer includes one open and close, and reports returned rows rather
than the number of filter operations. Complete filter arguments are retained in
each `.command.json`. Scenario names remain separate in comparisons and summaries.
The normal matrix also includes facade metadata stepping and Rust core modes
under both bounds, directions and mapping strategies.

`--auxiliary-suite` adds file-only indexed snapshot and verification lifecycles.
Use `--case indexed` or `--case verify` to select those cases. Snapshot capture
declares `LOW_CARD_00=low-00-00`; exact and FIELD equality visitors select the
same population, and every visitor consumes every payload. `indexed-open`
repeats 1,000 complete opens and reports completed operations in `records`;
captured entry/value populations are separate `extra` counters. Visitor cases
report visited rows. All modes include SDK opening, work and cleanup in the
timer. Indexed snapshots require forward traversal and snapshot bounds.

`verify-index` and `verify-file` report completed successful verifications.
Their public SDK APIs select their own reader defaults; helpers reject custom
window/access labels they cannot honor. Go uses `auto`, Rust uses `windowed`,
and both use the default window size. Use bounded fixtures spanning multiple
ENTRY arrays for compatibility verification, whose repeated-value work can be
quadratic. Keep capture, graph verification and ordinary snapshot-bounds reader
scans separate when interpreting results.

The auxiliary runner saves independent native-byte expectations in
`auxiliary-goldens.json`, including capture metadata. Before timing, also run
`indexed-exact-digest`, `indexed-field-digest` and `indexed-all-digest` against
full-byte expectations on every measured layout and access strategy. Those
digest companions are deliberately absent from normal timing cases. For one
compressed/staged fixture, `--auxiliary-golden-source` identifies its native,
uncompressed original and freezes its hash. This supplies expected contents;
the full-byte preflight must establish the measured fixture's equivalence.
Single-frame writer-produced Zstd DATA and whole-file streams need distinct
controls; skippable-padding fixtures do not establish the native single-frame
fast path. Fixture creation and external fixture verification stay outside
the timer.

Raw samples, process statistics, paired timing ratios and bootstrap intervals
are retained. A reproducible slower case is a regression; an inconclusive
interval is not evidence of equivalence. Investigate noise or repeated
slowdowns before accepting a candidate. Select representative cases from the
actual changed work with `--case`; the full matrix is not a mandatory gate for
every edit. Careful code review and high-level controls can establish that
cold-only checks or unchanged hot work do not negatively affect performance.
Use targeted measurements for credible added costs and adverse observations;
an uncertain interval alone does not require expanding the measurement suite.
Standard `summary.json` and
`manifest.json` files are emitted under each phase and direction. Feed the
direction-specific directories to `report_benchmarks.py` so forward/backward
results are compared separately.

Go indexed aggregation has focused lifetime and allocation controls in
`go/journal/explorer_window_lifetime_test.go`. From `go/`, run:

```sh
go test -count=1 ./journal -run '^TestExplorerIndexedWindow'
go test ./journal -run '^$' -bench '^BenchmarkExplorerIndexedValueOwnership$' -benchmem
```

The first command forces bounded-window eviction and temporary-buffer reuse,
checks both journal layouts and all codecs, and compares complete indexed
results and counters. The raw benchmark covers dense/sparse, low/high-cardinality
facets and histograms. Reader opening, fixture construction and reader close are
outside its timer.

`BenchmarkExplorerIndexedCompressedValueOwnership` reads a prebuilt corpus
selected by `JOURNAL_SDK_EXPLORER_COMPRESSED_CORPUS`; it skips when that variable
is unset. The directory contains
`{regular,compact}/{zstd,xz,lz4}/{low,high,singleton}.journal`. Each file has 2,048
rows using the test helper's synthetic `VALUE`/`SELECT` patterns. Low cardinality
uses the dense query with 2,197 updates; high cardinality uses the sparse query
with 34 updates; true singletons use the sparse query with 32 updates. The
benchmark checks indexed/traversal output, actual decompression, update totals,
and a window large enough to avoid eviction or temporary copies before timing.
Generate and validate the corpus once, then freeze its hashes and the complete
matching benchmark helper in both builds. Record per-case timing and allocation
samples; a successful forced-window repair must still pass the valid-control
performance gate.

Pilot one CLI-helper iteration before increasing `--loops`: a sequential full
scan can cost much more than an indexed query. Each Go helper loop opens and
closes a fresh reader and computes result checksums/sorted summaries; these costs
are inside `read_seconds`. Reported ExplorerStats describe the final loop.
CPU profiling also covers post-timer work; `--memprofile` forces GC after reader
close, so cumulative allocation and live-after-GC memory answer different
questions. These helper timings differ from the aggregation-only Go benchmarks
above.

Go ENTRY-slot header reads have additional controls in
`go/journal/reader_header_access_test.go` and
`go/journal/reader_header_bench_test.go`. From `go/`, run:

```sh
go test -count=1 ./journal -run '^(TestEntrySlotHeader|TestReaderAllocationShape|TestNativeReaderFile|TestReaderAccessorInitial)'
go test ./journal -run '^$' -bench '^BenchmarkReader(Allocation|Open|EntryHeader)$' -benchmem
```

The access tests cover default, tiny and non-power-of-two windows, fitting and
crossing headers, cached work, partial failures, closed readers, row pins,
eviction and live ENTRY-array growth. Header acquisition preserves raw bytes;
the ENTRY validator retains its separate format/error policy.

The opening benchmark times fresh open/count checks/close for valid empty and
five-row journals in both layouts and backends. Its optional large cases use
`JOURNAL_SDK_READER_OPEN_CORPUS/{regular,compact}/system.journal`, each containing
100,000 rows. Freeze those files before comparing builds. Fixture generation,
preflight and teardown are outside the benchmark timer. The header benchmark
also measures constrained eviction and pinned-window paths.

To compare small opens against the exact same files, optionally set
`JOURNAL_SDK_READER_OPEN_SMALL_CORPUS` to a directory containing
`{regular,compact}/system.journal`, each with five entries. The normal generated
fixtures remain the default. Freeze the shared files' hashes and retain a
generated-file control with the same binaries. Extending a benchmark helper
also changes test linking; a timing change after rebuilding does not by itself
establish that fixture generation caused an earlier difference.

Measure allocation class as well as raw struct size: `TestReaderAllocationShape`
retains 4,096 separately allocated readers and reports runtime size-class counts;
`BenchmarkReaderAllocation` records the corresponding allocation cost. A small
field can cross an allocator class. Reader storage reuse must be checked for
each architecture; identical 64-bit offsets do not establish identical 32-bit
layout. Keep this complete benchmark helper identical in both timing builds.
The direct-helper access, native-file opener and initial-statistics tests cover
candidate contracts; exclude those correctness-only files from both timing builds, record
the exclusions/hashes, and run them separately with the candidate. Preserve
matching pure-build flags and record per-case allocation counts/bytes alongside
complete query measurements.

Linux native-journal opening avoids regular-file poller setup through a blocking
descriptor. Its tests cover read-only/close-on-exec flags, path errors, symlinks,
descriptor closure and sparse offsets above 2 GiB; run these on amd64 and 386.
The direct opener bypasses Go's internal test-cache input observation. Run
fixture-dependent validation uncached with `-count=1` and retain file hashes.
Compare complete reader operations as well as opener primitives; fewer syscalls
alone do not establish whole-reader performance. Whole-file `.zst` sources use
the existing opener and retain their streaming/staging checks.

Constructor-statistics controls check empty/nonempty backends and initial mmap
fallback. They compare explicit counters, modes and fallback reason with a fresh
statistics query when constructor work reuses an existing snapshot.

The writer-core harness aligns initial hash table sizing across systemd and
SDK drivers with the systemd v260.1 formula:
`data=max(max_size_bytes*4/768/3,2047)` and `field=1023`. The default
`--max-size-bytes` is 128 MiB, matching the production baseline for the
systemd-compatible per-file max-size calculation.

The writer-directory harness intentionally requires `--max-size-bytes` and
`--rotation-max-size-bytes` to match. The directory surface is a comparable
high-level rotation benchmark, so the active-file max-size, hash-table sizing,
and reported rotation cap must describe the same production baseline.

Result JSON records `api_mode`. `--api-mode raw-payload` times prebuilt
`KEY=value` byte payload append for SDKs that expose it, matching the systemd C
helper's `iovec` shape. `--api-mode structured-field` times the SDK field-name
plus value append shape for producers that already hold structured data.
`systemd` always remains `raw-payload`.

SDK benchmark results record `live_publish_every_entries` so stock-compatible
per-entry publication is never compared silently against latency-tolerant
modes. The value is `1` by default, `0` disables explicit SDK live publication,
and `N > 1` publishes after every `N` appended entries. Rust benchmark results
also record `mmap_strategy` when the internal writer mapping switch is used.

The default writer-core workload remains `mixed-cardinality-32-fields`.
`--workload netflow-v5-repeating-256` is a focused Rust/Go diagnostic for a
synthetic NetFlow v5 row shape:

- 29 structured application fields and 450 application `KEY=value` bytes;
- one fixed raw `_BOOT_ID=...` field prepended to the low-level call;
- 30 low-level ENTRY items and 491 total logical DATA bytes;
- 256 repeating identities, with 385 distinct application DATA payloads;
- source-address cardinality 100, destination-address cardinality 3, and
  input-interface cardinality 256.

This workload supports the direct Rust and Go surfaces, individually or
together. The shared runner rejects systemd and duplicate language selections,
and reports record the workload and row-shape metadata. Rust can opt into its
trusted-unique payload mode; Go preserves its standard sorting and duplicate
elimination. Treat the workload as a causal writer-path diagnostic, not as a
universal SDK or NetFlow capacity claim.

Focused repeated-DATA measurement:

```bash
python3 tests/benchmarks/run_writer_core_benchmarks.py \
  --languages rust go \
  --workload netflow-v5-repeating-256 \
  --rows 100000 \
  --warmups 2 \
  --repetitions 10 \
  --format compact \
  --api-mode structured-field \
  --rust-trusted-unique-payloads \
  --live-publish-every-entries 0 \
  --rust-mmap-strategy windowed \
  --rust-compare-api-modes
```

Example:

```bash
python3 tests/benchmarks/run_writer_core_benchmarks.py \
  --languages systemd rust go \
  --rows 100000 \
  --repetitions 3 \
  --warmups 1 \
  --format compact \
  --final-state online \
  --keep-journals
```

```bash
python3 tests/benchmarks/run_reader_core_benchmarks.py \
  --rows 100000 \
  --directory-rows 100000 \
  --repetitions 3 \
  --warmups 1 \
  --languages rust,go,systemd \
  --format compact \
  --final-state online \
  --keep-fixtures
```

## Standard benchmark reports

Use `report_benchmarks.py` to render benchmark results. Do not hand-compose
benchmark tables in SOWs or status updates when a `summary.json` or
`report.json` exists.

The report format is fixed:

1. `Run Identity`
2. `Configuration`
3. `Production Comparison`
4. `Diagnostic Modes` for reader-core reports
5. `Change Comparison` when `--before` and `--after` are provided
6. `Conclusion`
7. `Raw Evidence`

Writer-core reports omit `Diagnostic Modes`, so subsequent sections move up by
one position for writer-only reports.

Use either `--run` for a single run report or `--before` plus `--after` for a
change report. Do not combine `--run` with `--before` or `--after`.

Reader production comparisons always use this row order per surface:

1. `systemd:data`
2. `rust:core-payloads`
3. `rust:sdk-payloads`
4. `rust:facade-data`
5. `go:sdk-payloads`
6. `go:facade-data`

Reader reports always split `file` and `open-files` surfaces. `sdk-entry`,
`core-next`, `core-offsets`, alternate bounds, alternate mmap strategies, and
other non-production rows appear under `Diagnostic Modes`.
`rust:core-payloads` is a production row only for the `file` surface; when it
appears for `open-files`, the reporter keeps it under `Diagnostic Modes`.

Production comparison tables include a `status` column. Rows expected from the
run's configured languages but missing from the artifact remain visible with
`status=missing` and blank metric columns.

Change reports include an `Unmatched Rows` subsection when a row exists only in
the before run or only in the after run. Treat unmatched rows as a configuration
or benchmark-surface difference before using the deltas for decisions.
Change reports show the after-run configuration in the `Configuration` section,
so verify the before/after manifests match when interpreting deltas.

Writer-core reports always use this language order:

1. `systemd`
2. `rust`
3. `go`

Writer-core change reports also include a `Configuration Differences`
subsection when the same language was measured with different API modes or
access strategies between the before and after artifacts.
The reporter rejects writer before/after comparisons when their workload
identity or jointly recorded row-shape metadata differs. Reports with an
explicit `workload` must contain all four row-shape fields; legacy reports
without `workload` retain compatibility with complete reports for the default
workload.

The reporter does not infer whether a benchmark is a win or regression. Pass an
explicit `--conclusion` value after reviewing the numbers. Accepted conclusion
labels are:

- `clear-win`
- `mixed`
- `no-measurable-change`
- `regression`
- `inconclusive`
- `not-assessed`

Examples:

```bash
python3 tests/benchmarks/report_benchmarks.py \
  --run .local/benchmarks/reader-core/latest \
  --title "Reader Core Benchmark" \
  --conclusion not-assessed
```

```bash
python3 tests/benchmarks/report_benchmarks.py \
  --before .local/benchmarks/reader-core-before/latest \
  --after .local/benchmarks/reader-core-after/latest \
  --title "Reader Core Optimization Benchmark" \
  --conclusion mixed \
  --conclusion-note "Single-file SDK payload improved, open-files did not."
```

```bash
python3 tests/benchmarks/report_benchmarks.py \
  --run .local/benchmarks/writer-core/<run-directory> \
  --title "Writer Core Benchmark" \
  --conclusion not-assessed
```

```bash
python3 tests/benchmarks/report_benchmarks.py \
  --before .local/benchmarks/writer-core-before/<run-directory> \
  --after .local/benchmarks/writer-core-after/<run-directory> \
  --title "Writer Core Optimization Benchmark" \
  --conclusion not-assessed
```
