# Product Scope Specification

## Purpose

This project produces pure SDKs and file-backed journalctl-compatible tools for systemd journal files.

## Language Targets

- Rust
- Go
- Node.js
- Python

## Delivery Priority

- The Go writer is the first implementation deliverable after the shared test harness is accepted.
- The Go writer is prioritized because the user needs a pure-Go journal writer for a Netdata plugin integration.
- The Go writer must support binary field values before later SDK phases continue, because the Netdata plugin integration requires byte-safe payloads.
- Rust, Go reader/journalctl completion, Node.js, Python, full interoperability, benchmarks, and optimization remain required, but they must not be started ahead of the Go writer unless the user changes this priority.

## Core Contracts

- Implementations must not link to system journal libraries.
- Go implementations must not use CGO.
- Node.js implementations must not use native addons.
- Python implementations must not use native journal bindings.
- Each language must provide two API layers: an idiomatic SDK API and a libsystemd-compatible reader facade.
- The libsystemd-compatible reader facade is required unless a SOW records concrete evidence that it would require native bindings, violate the pure-language policy, or create an unsafe/unrepresentable API in that language.
- Pure-language dependencies are allowed after dependency review.
- Cross-language interoperability is mandatory: every reader must read journal files produced by every writer.
- The system must preserve systemd journal file concurrency expectations: one writer and multiple readers may operate on the same journal file according to journal rules.
- Live concurrency compatibility is a MUST, not a follow-up optimization. No writer or reader implementation may be called production-compatible until this is confirmed with stock systemd tooling and the shared cross-language suite.

## Live Concurrency Compatibility Contract

Writer compatibility:

- Every writer implementation must produce files that stock `journalctl --file` can read while the writer is still appending.
- Every writer implementation must produce files that stock libsystemd journal readers can read while the writer is still appending.
- Every writer implementation must support one active writer and multiple concurrent readers on the same file.
- Every writer implementation must keep stock readers safe during append publication windows; stock readers must not crash, report corruption for committed entries, spin, or require the writer to close the file before reading.
- Every writer implementation must pass stock `journalctl --verify --file` after clean close and after tested interruption/reopen scenarios for the feature slice claimed by that writer.

Reader compatibility:

- Every reader implementation must read files while they are being appended by stock systemd journal writers when the test environment can provide one.
- Every reader implementation must read files while they are being appended by every writer implementation in this repository.
- Every reader implementation must correctly handle online journal state, tail metadata changes, entry-array growth, data hash-table growth by chaining, and observable file-size changes without treating normal live updates as corruption.
- Every reader implementation must support multiple readers observing the same live file concurrently.
- Reader follow/tail behavior must be validated against stock `journalctl` semantics for file-backed operation.

Required validation evidence:

- A committed live-concurrency harness must exercise stock `journalctl --file` readers against each repository writer while appends are in progress.
- A committed live-concurrency harness must exercise stock libsystemd reader APIs against each repository writer while appends are in progress.
- The shared live-concurrency harness lives under `tests/conformance/live/` and uses a configurable monotonically increasing sequence field, defaulting to `LIVE_SEQ`, so stock readers prove ordered complete visibility.
- Stock reader adapters may retry transient active-writer `ENODATA` open/read failures or partial snapshots only while the writer is active; after the writer exits, final ordered reads and `journalctl --verify --file` must pass without masking incompatibility.
- A committed live-concurrency harness must exercise each repository reader against live files produced by each repository writer.
- Where a stock systemd writer can be exercised without violating repository-boundary rules, each repository reader must be tested against it. If the environment cannot provide a stock writer safely, the SOW must record the missing evidence and cannot claim full reader compatibility.
- Compatibility claims must record exact stock systemd version, commands/helpers used, stress duration, entry counts, reader counts, and failure criteria.
- Smoke tests are not sufficient for production compatibility; stress tests and race-window tests are required.

## Compatibility Baseline

Baseline compatibility target:

```text
systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced
tag: v260.1
```

Known reference evidence:

```text
systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced
man/journalctl.xml
src/libsystemd/sd-journal/journal-def.h
src/libsystemd/sd-journal/sd-journal.c
test/journal-data/
test/test-journals/
test/units/TEST-04-JOURNAL*.sh
```

Netdata Rust source evidence:

```text
ktsaou/netdata @ 6a515000ac89
src/crates/jf/
src/crates/journal-core/
src/crates/journal-log-writer/
```

## Test Scope

In scope:

- systemd journal file/API tests applicable to pure SDK behavior.
- systemd importer tests applicable to journal file parsing.
- systemd journal fixtures and corrupted journal fixtures.
- File-backed journalctl behavior against journal files or journal directories.
- Live stock `journalctl` and stock libsystemd reader behavior against actively written journal files.
- Cross-language writer/reader interoperability tests.
- Cross-language live writer/reader concurrency tests.
- Benchmarks, profiling, and optimization evidence.

Out of scope:

- journald daemon lifecycle.
- systemd service management.
- journal-remote, journal-gatewayd, and journal-upload services.
- varlink service APIs.
- socket activation.
- daemon setup for Forward Secure Sealing.
- reboot/boot lifecycle tests.

## Writer Target

Final writer target:

- keyed hash;
- regular and compact journal formats where applicable;
- compression where systemd journal files define it;
- Forward Secure Sealing where systemd journal files define it.

Delivery may be phased. Earlier phases may write a smaller feature subset if the SOW records the gap, shared readers/tests support the compatibility envelope, and follow-up SOWs track the remaining writer features.

Current Go writer feature slice:

- regular, non-compact, uncompressed journal files;
- keyed hash tables using the journal file ID;
- byte-safe DATA field values through `Field.Value []byte`;
- high-level directory writing with systemd-compatible active/archive naming;
- rotation by entry count and active file size;
- retention by archived file count and total byte size, scoped to the configured source/prefix and never deleting the active file;
- live one-writer/multiple-reader compatibility with stock `journalctl --file` and stock libsystemd readers for the current writer slice.

## Reader Target

Readers must support applicable historical journal files represented by the shared fixture suite, including corrupted fixture behavior where the expected result is a controlled error or partial recovery.

Current Go reader feature slice:

- regular, non-compact journal files;
- files named `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst`;
- whole-file zstd fixtures and zstd-compressed DATA objects through a pure-Go
  dependency;
- directory iteration across active and archived files;
- forward/backward iteration, cursors, realtime timestamps, binary field
  values, field enumeration, and unique value enumeration;
- systemd-compatible export output for binary fields using size-prefixed field
  values and blank-line entry separators;
- systemd-compatible JSON output for duplicate fields and binary values;
- libsystemd-style match tree behavior from `sd_journal_add_match()`,
  `sd_journal_add_disjunction()`, and `sd_journal_add_conjunction()`;
- file-backed Go journalctl behavior for `--file`, `--directory`, text/json/export
  output, field listing, boot listing, repeated same-field OR matches, and `+`
  disjunction;
- Go conformance adapter support for reader, matching, importer, compression,
  cursor, enumeration, stream, export, and file-backed journalctl cases.

Current Go reader limitations:

- compact journal files are rejected;
- xz/lz4-compressed DATA objects are rejected;
- directory iteration is sequential by journal file and validated for
  non-overlapping active/archive files; realtime interleaving across
  overlapping multi-file directories is tracked under the interoperability
  phase;
- full journal verification and FSS validation are not implemented;
- daemon-only journalctl operations remain unsupported.

Current Rust writer feature slice:

- regular, non-compact journal files;
- uncompressed DATA objects;
- keyed hash tables using the journal file ID;
- byte-safe field values through `&[u8]` field payloads;
- direct-file writing through `journal_core`;
- high-level directory writing with systemd-compatible active/archive naming;
- entry-count and file-size rotation;
- archived file-count and byte-size retention, without deleting the active file
  to satisfy retention limits;
- live one-writer/multiple-reader compatibility with stock `journalctl --file`
  and stock libsystemd readers for the current writer slice.

Current Rust reader feature slice:

- regular, non-compact journal files;
- files named `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst`;
- whole-file zstd fixtures and zstd, lz4, and xz-compressed DATA objects through
  pure-Rust dependencies;
- directory iteration across active and archived files;
- forward/backward iteration, cursors, realtime and monotonic timestamps, binary
  field values, field enumeration, and systemd-compatible export/json/text
  formatting;
- libsystemd-style match tree behavior from `SdJournalAddMatch()`,
  `SdJournalAddDisjunction()`, and `SdJournalAddConjunction()`;
- file-backed Rust journalctl behavior for `--file`, `--directory`, text/json/export
  output, field listing, boot listing, repeated same-field OR matches, and `+`
  disjunction;
- Rust conformance adapter support for reader, matching, importer, compression,
  cursor, enumeration, stream, export, header parsing, and file-backed
  journalctl cases.

Current Rust reader limitations:

- compact journal files are not part of the accepted feature slice;
- directory iteration is sequential by journal file and validated for
  non-overlapping active/archive files; realtime interleaving across overlapping
  multi-file directories is tracked under the interoperability phase;
- boot listing uses file-level boot metadata in this slice;
- full journal verification and FSS validation are not implemented;
- daemon-only journalctl operations remain unsupported.

Current Node.js writer feature slice:

- regular, non-compact journal files;
- uncompressed DATA objects;
- keyed hash tables using the journal file ID;
- byte-safe field values through `Buffer`, `Uint8Array`, and string-compatible
  field values;
- direct-file writing through `Writer`;
- high-level directory writing through `Log` with systemd-compatible
  active/archive naming;
- entry-count and file-size rotation;
- archived file-count and byte-size retention, without deleting the active file
  to satisfy retention limits;
- live one-writer/multiple-reader compatibility with stock `journalctl --file`
  and stock libsystemd readers for the current writer slice.

Current Node.js reader feature slice:

- regular, non-compact journal files;
- files named `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst`;
- whole-file zstd fixtures through Node.js built-in `node:zlib`;
- directory iteration across active and archived files;
- forward/backward iteration, cursors, realtime and monotonic timestamps, binary
  field values as `Buffer`, field enumeration, and unique value enumeration;
- systemd-compatible export/json/text formatting for the accepted fixture set;
- libsystemd-style match tree behavior from `SdJournalAddMatch()`,
  `SdJournalAddDisjunction()`, and `SdJournalAddConjunction()`;
- file-backed Node.js journalctl behavior for `--file`, `--directory`,
  text/json/export output, field listing, boot listing, repeated same-field OR
  matches, and `+` disjunction;
- Node.js conformance adapter support for reader, matching, importer,
  compression, cursor, enumeration, stream, export, header parsing, and
  file-backed journalctl cases.

Current Node.js reader/writer limitations:

- compact journal files are rejected;
- xz/lz4-compressed DATA objects are rejected;
- compressed DATA object writing is not implemented;
- directory iteration is sequential by journal file and validated for
  non-overlapping active/archive files; realtime interleaving across overlapping
  multi-file directories is tracked under the interoperability phase;
- boot listing uses file-level boot metadata in this slice;
- full journal verification and FSS validation are not implemented;
- daemon-only journalctl operations remain unsupported.

Current Python writer feature slice:

- regular, non-compact journal files;
- uncompressed DATA objects;
- keyed hash tables using the journal file ID;
- byte-safe field values through `bytes`, `bytearray`, `memoryview`, and
  string-compatible field values;
- direct-file writing through `Writer`;
- high-level directory writing through `Log` with systemd-compatible
  active/archive naming;
- entry-count and file-size rotation;
- archived file-count and byte-size retention, without deleting the active file
  to satisfy retention limits;
- exclusive non-blocking writer file locks to protect the one-writer contract
  among cooperating repository writers;
- live one-writer/multiple-reader compatibility with stock `journalctl --file`
  and stock libsystemd readers for the current writer slice.

Current Python reader feature slice:

- regular, non-compact journal files;
- files named `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst`;
- whole-file zstd fixtures and zstd-compressed DATA objects through Python
  `compression.zstd` where the optional standard-library module is available;
- directory iteration across active and archived files, including one machine-id
  subdirectory level;
- forward/backward iteration, cursors, realtime and monotonic timestamps, binary
  field values as `bytes`, field enumeration, and unique value enumeration;
- systemd-compatible export/json/text formatting for the accepted fixture set;
- libsystemd-style match tree behavior from `SdJournalAddMatch()`,
  `SdJournalAddDisjunction()`, and `SdJournalAddConjunction()`;
- file-backed Python journalctl behavior for `--file`, `--directory`,
  text/json/export output, field listing, boot listing, repeated same-field OR
  matches, and `+` disjunction;
- Python conformance adapter support for reader, matching, importer,
  compression, cursor, enumeration, stream, export, header parsing, and
  file-backed journalctl cases.

Current Python reader/writer limitations:

- compact journal files are rejected;
- xz/lz4-compressed DATA objects are rejected;
- compressed DATA object writing is not implemented;
- directory iteration is sequential by journal file and validated for
  non-overlapping active/archive files; realtime interleaving across overlapping
  multi-file directories is tracked under the interoperability phase;
- boot listing uses file-level boot metadata in this slice;
- full journal verification and FSS validation are not implemented;
- daemon-only journalctl operations remain unsupported.

## journalctl Target

Implement journalctl rewrites in Rust, Go, Node.js, and Python for file-backed/query behavior.

Matching semantics:

- Different fields are ANDed.
- Repeated matches for the same field are OR alternatives.
- The `+` separator creates explicit disjunction groups and must be replicated for file-backed journalctl behavior.
- No new `KEY in [values]` syntax is required.

Daemon-only commands are not implemented in this project. They must return documented unsupported behavior rather than silently pretending to perform daemon operations.

Daemon-only commands include:

- sync;
- flush;
- rotate;
- relinquish-var;
- smart-relinquish-var.

## Repository Boundary

Implementation and review agents may inspect external references read-only when the active SOW requires it.

They must not write, edit, delete, move, reset, checkout, install, generate, cache, or format anything outside this repository.

The only write exception outside the repository is `/tmp`. Prefer `.local/` inside this repository for scratch work.

## Open Questions

None currently blocking bootstrap. Implementation-phase SOWs may expose narrower decisions and must record them before coding starts.
