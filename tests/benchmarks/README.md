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

Go reader results record `mmap_strategy`. `mmap` is the default SDK reader
access mode on Unix and is the production hot path. `read-at` remains in the
harness as an explicit comparison and diagnostic mode.

The Go reader benchmark helper also accepts `--cpuprofile`, `--memprofile`,
and `--loops` for targeted profiling outside the full benchmark harness. Keep
profile outputs under `.local/benchmarks/`; `--loops` repeats the selected
read case inside one process to collect enough samples and is not used by the
shared checksum-comparison harness.

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
