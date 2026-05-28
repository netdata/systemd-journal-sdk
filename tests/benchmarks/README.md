# Benchmark Harnesses

This directory has two writer benchmark surfaces:

- `run_writer_core_benchmarks.py` measures the actual append loop. Each driver
  pre-materializes deterministic rows before timing, creates the writer before
  timing, stops timing immediately after the last append, and reports final
  close/sync separately.
- `run_writer_benchmarks.py` measures deterministic JSONL ingestion. It is
  useful for end-to-end stress and corpus ingestion checks, but its rows/sec
  includes JSON parsing, value materialization, and caller allocation overhead.

Use writer-core results when comparing SDK writer performance against systemd
and Netdata hot-path expectations.

The writer-core harness aligns initial hash table sizing across systemd and
SDK drivers with the systemd v260.1 formula:
`data=max(max_size_bytes*4/768/3,2047)` and `field=1023`. The default
`--max-size-bytes` is 128 MiB, matching the production baseline for the
systemd-compatible per-file max-size calculation.

Result JSON records `api_mode` because not every language exposes the same
lowest-level append surface. `systemd` and the current Rust driver use
prebuilt raw `KEY=VALUE` payloads in the timed loop; Go, Node.js, and Python
use their public field APIs, which construct payloads inside append. Those are
the actual public writer paths for those SDKs, but raw-vs-field differences
must be considered when interpreting cross-language ratios.

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
