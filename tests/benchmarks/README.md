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
`core-payloads`), Rust SDK/facade entry scans, and public libsystemd
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
active-file-compatible mode and refreshes file bounds while reading.
`snapshot` fixes file size at open time for polling/query consumers that do not
need to observe appends during the current scan.

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
  --format compact \
  --final-state online \
  --keep-fixtures
```
