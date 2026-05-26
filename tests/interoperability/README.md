# Interoperability Matrix

This directory contains repo-local matrix runners for writer, reader,
live-concurrency, compression, lock, and byte-identity validation.

## Byte-Identity Matrix (`run_byte_identity.py`)

Runs the deterministic systemd C ingester plus the Rust, Go, Node.js, and
Python dataset ingesters, then compares the accepted-corpus output files
byte-for-byte.

```bash
python3 tests/interoperability/run_byte_identity.py --final-state all
```

The byte-identity runner validates the strongest current regular-writer
contract: deterministic uncompressed files from all SDK writers must be
identical to the systemd v260.1 reference writer for the accepted corpus. On
mismatch, it reports exact byte offsets plus header-field or object-span
context. It also runs stock `journalctl --verify --file` for every generated
accepted-corpus file.

The accepted corpus includes deliberate DATA hash-bucket collisions. The runner
requires the generated header `data_hash_chain_depth` to equal the systemd
reference value (`3`) for every language and final state, so hash-chain
publication cannot silently fall back to the no-collision path.

Use `--final-state online`, `--final-state offline`, or `--final-state archived`
to isolate one systemd close path. `online` matches plain `journal_file_close()`,
`offline` matches `journal_file_offline_close()`, and `archived` matches
`journal_file_archive()` followed by offlining.

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
python3 tests/interoperability/run_live_matrix.py --entries 10 --poll-readers 1
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

- **Directory traversal parity**: directory traversal is covered by
  `run_directory_matrix.py`. The live matrix only proves active-file
  compatibility for the file discovered under a generated directory.

- **Compression, compact journal, or FSS**: these are out of scope for the live
  matrix. Binary stress fixtures are covered by `run_binary_matrix.py`; DATA
  object compression is covered by `run_compression_matrix.py`; compact object
  layout is covered by `run_compact_matrix.py`.

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

## Directory Matrix (`run_directory_matrix.py`)

Generates synthetic directory layouts and compares file-backed
`journalctl --directory` behavior across stock journalctl and all repository
rewrites.

```bash
python3 tests/interoperability/run_directory_matrix.py
```

The stock-parity fixture covers:

- root `.journal` and `.journal~` files;
- one immediate 128-bit machine-id subdirectory level, including dashed UUID
  form;
- invalid, nested, and namespace-suffix subdirectories that must be skipped by
  default;
- overlapping realtime ranges that require interleaved multi-file ordering;
- repeated same-field OR matches, cross-field AND matches, and `+`
  disjunction;
- JSON, export, text, field listing, boot listing, empty-directory reads, and
  directory verify behavior.

The runner also validates a corrupt/unreadable-file directory where stock and
repository readers skip files they cannot open, and validates the repository
extension for whole-file `.journal.zst` directory discovery.

## Binary Matrix (`run_binary_matrix.py`)

Generates a binary-field fixture journal with every writer, then validates each
generated file with stock journalctl, stock libsystemd, and every repository
journalctl implementation.

```bash
python3 tests/interoperability/run_binary_matrix.py
```

Useful options:

```bash
python3 tests/interoperability/run_binary_matrix.py --writers go python
python3 tests/interoperability/run_binary_matrix.py --readers stock rust python
```

Each writer fixture includes:

- `TEST_ID=binary-interoperability`
- `MESSAGE=binary interoperability`
- `PRIORITY=6`
- `LIVE_SEQ=000000`
- `BINARY_PAYLOAD` bytes `00 01 02 41 0a 7f 80 ff`
- `BINARY_MATCH` bytes `61 62 63 07 64 65 66`
- `BINARY_EMPTY` as an empty value

For each generated writer file, the binary matrix validates:

- stock `journalctl --verify --file` succeeds;
- stock `journalctl --output=json` returns byte arrays for non-printable
  binary values and an empty string for the empty binary value;
- stock `journalctl --output=export` contains exact size-prefixed binary
  payloads;
- stock libsystemd `sd_journal_get_data()` returns exact bytes for every binary
  field;
- selected repository journalctl rewrites return matching JSON and export
  output;
- `BINARY_MATCH=abc\x07def` works as a stock file-backed match through argv.

## Compression Matrix (`run_compression_matrix.py`)

Generates DATA-compressed fixture journals, then validates each generated file
with stock journalctl, stock libsystemd, and selected repository journalctl
implementations.

```bash
python3 tests/interoperability/run_compression_matrix.py
```

Useful options:

```bash
python3 tests/interoperability/run_compression_matrix.py --writers go python
python3 tests/interoperability/run_compression_matrix.py --readers stock rust python
python3 tests/interoperability/run_compression_matrix.py --writers go rust python --readers stock go rust python --compression xz
python3 tests/interoperability/run_compression_matrix.py --writers go rust node python --readers stock go rust node python --compression lz4
```

By default the runner exercises zstd across all writers/readers. Use
`--compression xz lz4` with writers/readers that support those algorithms.
Current support is:

- zstd: Go, Rust, Node.js, and Python writers/readers;
- xz: Go, Rust, Node.js, and Python writers/readers;
- lz4: Go, Rust, Node.js, and Python writers/readers.

Each writer fixture enables DATA compression with a low threshold and
includes:

- `TEST_ID=<compression>-interoperability`
- `MESSAGE=<compression> interoperability`
- `PRIORITY=6`
- `LIVE_SEQ=000000`
- `COMPRESSED_PAYLOAD` as 256 printable bytes
- `COMPRESSED_MATCH` as the first 32 bytes of `COMPRESSED_PAYLOAD`

For each generated writer file, the compression matrix validates:

- journal header has the expected compression incompatible flag;
- at least one DATA object has the expected compression object flag;
- stock `journalctl --verify --file` succeeds;
- stock journalctl and stock libsystemd read decompressed field values;
- Go, Rust, Node.js, and Python journalctl rewrites return matching JSON and
  export output;
- `COMPRESSED_MATCH=<value>` works as a file-backed match through argv for stock
  journalctl and every repository journalctl rewrite.

## Compact Matrix (`run_compact_matrix.py`)

Generates compact-format fixture journals with every writer, then validates
each generated file with stock journalctl, stock libsystemd, and every
repository journalctl implementation.

```bash
python3 tests/interoperability/run_compact_matrix.py
```

Useful options:

```bash
python3 tests/interoperability/run_compact_matrix.py --writers go python
python3 tests/interoperability/run_compact_matrix.py --readers stock rust python
python3 tests/interoperability/run_compact_matrix.py --compression zstd --compression-threshold-bytes 1
```

Each writer fixture enables explicit compact output while keeping the binary
fixture fields used by the binary matrix. For each generated writer file, the
compact matrix validates:

- `HEADER_INCOMPATIBLE_COMPACT` is set;
- ENTRY items are 4-byte compact offsets;
- DATA payloads begin after the compact DATA tail fields;
- compact ENTRY_ARRAY layout is present;
- stock `journalctl --verify --file` succeeds;
- stock journalctl and stock libsystemd read binary fields;
- Go, Rust, Node.js, and Python journalctl rewrites return matching JSON and
  export output.

The `--compression` option can be used to validate compact journals whose DATA
objects are compressed with `zstd`, `xz`, or `lz4`.

## Mixed Directory Matrix (`run_mixed_directory_matrix.py`)

Generates a synthetic fixture tree containing mixed per-file journal feature
sets, then compares file-backed `journalctl --directory` behavior across stock
journalctl and all repository rewrites.

```bash
python3 tests/interoperability/run_mixed_directory_matrix.py
```

The stock-supported directory covers:

- regular and compact files;
- uncompressed, zstd, xz, and lz4 DATA-object compression;
- sealed and unsealed files sharing one verification key;
- active `.journal` and archived `.journal~` names;
- JSON, export, text, field listing, boot listing, repeated-match OR,
  cross-field AND, and `+` disjunction;
- directory verification success for unsealed-only directories, missing-key
  failure for sealed files, correct-key success, and wrong-key failure.

The runner also validates the repository extension for mixed active and archived
whole-file `.journal.zst` / `.journal~.zst` directory discovery, including
sealed whole-file zstd verification with and without `--verify-key`.

## Writer Lock Matrix (`run_lock_matrix.py`)

Starts one SDK writer as the active lock holder, then starts every SDK writer as
a contender against the same journal file. The contender must fail before it can
publish a ready file or append data while the holder is active.

```bash
python3 tests/interoperability/run_lock_matrix.py --entries 200 --delay-ms 20
```

For each holder/contender pair, the lock matrix validates:

- the holder publishes a ready marker and keeps the journal file open;
- the contender exits non-zero before publishing its ready marker;
- the holder exits cleanly and removes the lock file;
- stock `journalctl --verify --file` passes after contention;
- stale lock files left by crashed writers are cleaned by the next SDK writer.

## Shared Conventions

All runtime artifacts (generated journals, binaries, result JSON files) live
under `.local/interoperability/`. The runners clean up `*.ready` files on
completion unless `--keep-files` is passed.

## Current Writer Feature Gaps

| Gap | Status | Evidence | Follow-up |
|-----|--------|----------|-----------|
| Deterministic uncompressed byte identity | Complete for accepted corpus | `run_byte_identity.py` compares systemd, Rust, Go, Node.js, and Python byte-for-byte | Closed in SOW-0016 |
| zstd compressed DATA object writing | Complete | `run_compression_matrix.py` validates zstd header/object flags plus stock/repository reads | Closed in SOW-0008 |
| xz compressed DATA object writing | Complete | Rust, Go, Node.js, and Python write/read xz; stock journalctl, stock libsystemd, and all repository readers pass | Closed in SOW-0021 |
| lz4 compressed DATA object writing | Complete | Rust, Go, Node.js, and Python write/read lz4 when Python `lz4==4.4.5` is installed; stock journalctl, stock libsystemd, and all repository readers pass | Closed in SOW-0017 |
| Compact journal format | Complete | `run_compact_matrix.py` validates compact layout plus stock/repository reads across all writers | Closed in SOW-0018 |
| Forward Secure Sealing / verification | Complete | Sealed writers and `--verify-key` APIs/CLIs pass stock and repository validation for generated sealed files | Closed in SOW-0019 |
| Cross-language binary stress | Complete | `run_binary_matrix.py` passes 52/52 across all writer/reader pairs plus stock libsystemd | Closed |
| Writer locking parity | Complete | `run_lock_matrix.py` passes 8/8; all SDK writer pairs reject concurrent writers and stale locks left by crashed writers are cleaned | Closed |
| Directory reader subdirectory traversal | Complete | `run_directory_matrix.py` passes across stock journalctl plus Rust, Go, Node.js, and Python rewrites | Closed in SOW-0020 |
| Mixed-format directory readers | Complete | `run_mixed_directory_matrix.py` passes 72/72 across stock journalctl plus Rust, Go, Node.js, and Python rewrites | Closed in SOW-0024 |
