# Explorer Query Comparison

This directory contains shared query specifications and harnesses for
SOW-0074. The goal is to compare two isolated Rust implementations with the
same external query and report contract:

- `explorer_query_baseline`: existing expanded-entry reader APIs.
- `explorer_query_optimized`: new SDK-native explorer/query APIs.

The smoke suite is intentionally small for quick iteration. The full suite will
expand the same contract across all SOW-0074 query families before the SOW is
closed.

Run the Rust smoke suite:

```bash
python3 tests/explorer_query/run_rust_smoke.py
```

Run the Rust full correctness suite against uncompressed and zstd-compressed
fixtures:

```bash
python3 tests/explorer_query/run_rust_smoke.py --suite full
python3 tests/explorer_query/run_rust_smoke.py --suite full --compression zstd
python3 tests/explorer_query/run_rust_smoke.py --suite full --surface directory
```

Run the Rust performance comparison on the generated 200k-row / 32-field
corpus. Reports are written under `.local/explorer-query/benchmarks/`.

```bash
python3 tests/explorer_query/run_rust_benchmarks.py
```
