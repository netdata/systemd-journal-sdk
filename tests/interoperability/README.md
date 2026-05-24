# Interoperability Matrix

This directory contains two repo-local matrix runners for SOW-0008.

## Closed-File Matrix (`run_matrix.py`)

Generates synthetic journal files with the current Go, Rust, Node.js, and
Python writers, then reads each generated file with stock journalctl plus every
repository journalctl implementation.

```bash
python3 tests/interoperability/run_matrix.py
```

Useful options:

```bash
python3 tests/interoperability/run_matrix.py --entries 100
python3 tests/interoperability/run_matrix.py --writers go python --readers stock python
```

For each writer/reader pair, the matrix validates:

- `PRIORITY=6` reads exactly the expected number of entries;
- `PRIORITY=1` reads zero entries;
- `LIVE_SEQ` is present and ordered from `000000` onward;
- repeated same-field matches implement OR semantics across two distinct
  `MESSAGE` values;
- `+` disjunctions return the union of two distinct `MESSAGE` values;
- cross-field matches implement AND semantics.

For each generated writer file, stock `journalctl --verify --file` must pass.

## Live Matrix (`run_live_matrix.py`)

Starts one writer per language and polls multiple readers (stock journalctl plus
all four repository readers) while the writer is actively appending entries.
After the writer exits, final reader snapshots are collected and validated.

```bash
python3 tests/interoperability/run_live_matrix.py
```

Useful options:

```bash
python3 tests/interoperability/run_live_matrix.py --entries 50
python3 tests/interoperability/run_live_matrix.py --writers go rust --poll-readers 3
python3 tests/interoperability/run_live_matrix.py --writers rust --poll-readers 4 --keep-files
```

### What the live matrix proves

For each writer language, the matrix proves all of the following:

1. **Active observation**: at least one polling reader saw entries while the
   writer was still actively appending. Sequences observed during this window
   are ordered prefixes of `LIVE_SEQ`.

2. **Final completeness**: after the writer exits, every eligible reader
   observed the complete set of expected entries in the correct order.

3. **File integrity**: `journalctl --verify --file` passes for each generated
   journal file. For the Rust directory writer, the runner discovers the active
   `.journal` file and verifies that file directly.

4. **Cross-language live compatibility**: Go, Rust, Node.js, and Python
   repository readers plus stock journalctl all succeed against Go, Rust,
   Node.js, and Python writers while each writer is active.

### What the live matrix does NOT prove

- **Daemon-only behavior**: the live matrix does not require `--follow` for any
  reader. Repository readers that do not implement follow mode (Go, Rust) are
  polled via file-backed `--file --output=json` queries. This is intentional
  per SOW-0008 requirements.

- **Directory traversal parity**: for directory-mode writers, the live matrix
  validates the active `.journal` file after discovering it under the generated
  directory. It does not prove every repository `--directory` implementation can
  traverse every directory layout; that remains tracked separately.

- **Compression, compact journal, or FSS**: these are out of scope for the live
  matrix. Binary stress fixtures are also out of scope.

- **Multi-writer scenarios**: the live matrix tests one writer at a time with
  multiple concurrent readers, not multiple concurrent writers.

### Validation

The live matrix reports:

- `systemd_version` — stock journalctl version used as reference reader
- `writer` — language name
- `journal_mode` — `file` or `directory`
- `entries` — number of entries written
- `exit_code` — writer process exit code
- `active_polls` — observations made while writer was still running
- `final_reads` — complete snapshots after writer exited
- `verify` — stock journalctl --verify result for the generated journal file
- `writer_stderr` — writer stderr tail for failure diagnostics
- `writer_delay_ms` — configured delay between writer appends

A `status` of `PASS` means: writer exited cleanly, every polling reader saw
entries while the writer was active, and all final reads observed the complete
ordered sequence.

## Shared Conventions

All runtime artifacts (generated journals, binaries, result JSON files) live
under `.local/interoperability/`. Both runners clean up `*.ready` files on
completion unless `--keep-files` is passed.

## Current Writer Feature Gaps

| Gap | Status | Evidence | Follow-up |
|-----|--------|----------|-----------|
| Compressed DATA object writing | Not implemented | Current writers emit uncompressed DATA objects | SOW-0008 or split compression SOW |
| xz/lz4/zstd writer parity | Not implemented | Writers do not write compressed DATA | SOW-0008 or split by compression family |
| Compact journal format | Not implemented | Writers create regular non-compact journals | Requires systemd reference inventory |
| Forward Secure Sealing / verification | Not implemented | Verification/FSS tests skipped in earlier SOWs | Split dedicated FSS SOW |
| Cross-language binary stress | Not complete | Livewriter fixtures do not include binary fields | Add binary fixture generation before SOW-0008 close |
| Writer locking parity | Partial | Go and Python use fcntl; Node.js has no native flock; Rust writer lock was removed from scope | Track whether Node/Rust need advisory lock behavior |
| Directory reader subdirectory traversal | Partial | Live matrix validates discovered files; full `--directory` traversal parity remains separate | Address in SDK follow-up work |
