# Interoperability Matrix

This directory contains a repo-local matrix runner for SOW-0008.

The runner generates synthetic journal files with the current Go, Rust, Node.js,
and Python writers. It then reads each generated file with:

- stock `journalctl --file`;
- the Go file-backed journalctl implementation;
- the Rust file-backed journalctl implementation;
- the Node.js file-backed journalctl implementation;
- the Python file-backed journalctl implementation.

Generated journals, helper binaries, and JSON result files are written under
the repository-level `.local/interoperability/` directory.

## Run

```bash
python3 tests/interoperability/run_matrix.py
```

Useful options:

```bash
python3 tests/interoperability/run_matrix.py --entries 100
python3 tests/interoperability/run_matrix.py --writers go python --readers stock python
```

## Checks

For each writer/reader pair, the matrix validates:

- `PRIORITY=6` reads exactly the expected number of entries;
- `PRIORITY=1` reads zero entries;
- `LIVE_SEQ` is present and ordered from `000000` onward;
- repeated same-field matches implement OR semantics across two distinct
  `MESSAGE` values;
- `+` disjunctions return the union of two distinct `MESSAGE` values;
- cross-field matches implement AND semantics.

For each generated writer file, stock `journalctl --verify --file` must pass.

The matrix is closed-file only. Live cross-language reader/writer concurrency is
tracked separately in SOW-0008 and should reuse `tests/conformance/live/`.

## Current Writer Gaps

The current writer feature slices are regular, non-compact, keyed-hash journal
files with uncompressed DATA objects. Remaining writer work includes:

- compressed DATA object writing;
- compact journal format support;
- Forward Secure Sealing and full verification;
- full live cross-language reader/writer matrix;
- deeper binary-field stress coverage across all writer/reader pairs.
