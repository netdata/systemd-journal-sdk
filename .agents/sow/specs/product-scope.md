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
- Node.js implementations must not load or link native code at runtime. Dependency packages may ship native artifacts if the SDK runtime path is constrained and tested to use only non-native implementations (e.g. WASM).
- Python implementations must not use native journal bindings.
- Each language must provide two API layers: an idiomatic SDK API and a libsystemd-compatible reader facade.
- The libsystemd-compatible reader facade is required unless a SOW records concrete evidence that it would require native bindings, violate the pure-language policy, or create an unsafe/unrepresentable API in that language.
- Common compression-library dependencies are allowed after dependency review.
  Journal parsing/writing must not depend on systemd or libjournal libraries.
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
- Deterministic dataset ingesters for the systemd C reference helper and every
  SDK writer, with generated source/build/runtime artifacts kept under
  `.local/`.
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
- explicit writer API selection between regular and compact output, with regular
  output remaining the default unless a SOW records a user decision to change it;
- compression where systemd journal files define it;
- Forward Secure Sealing where systemd journal files define it.

Delivery may be phased. Earlier phases may write a smaller feature subset if the SOW records the gap, shared readers/tests support the compatibility envelope, and follow-up SOWs track the remaining writer features.

Current shared writer layout contract:

- Deterministic regular uncompressed files written by Rust, Go, Node.js, Python,
  and the systemd v260.1 reference ingester must be byte-for-byte identical for
  the accepted deterministic corpus across online/plain-close, offline-close,
  and archived-close final states.
- New regular files use v260-size headers, `HEADER_COMPATIBLE_TAIL_ENTRY_BOOT_ID`,
  keyed hash tables, FIELD_HASH_TABLE before DATA_HASH_TABLE, the v260 header
  counters/tail fields, systemd-compatible entry-array growth, and the same
  initial 8 MiB allocation envelope as the systemd reference helper.
- The deterministic accepted corpus intentionally exercises DATA hash-bucket
  collisions. Writer byte identity includes `next_hash_offset` chain traversal
  and exact `data_hash_chain_depth` publication, not only collision-free hash
  table insertion.
- Writer APIs must distinguish systemd's final-state paths: plain close leaves
  `ONLINE`, explicit offline close writes `OFFLINE`, and archive close writes
  `ARCHIVED` after the archive rename path.
- Header parsing must respect the on-disk `header_size` for historical files.
  Readers must not reject valid older files just because the in-memory struct for
  new v260 files is larger.
  Reader APIs must also return zero/default values for fields that are absent
  from the on-disk header, rather than exposing bytes from the object arena as
  newer header fields.
- Compact journal files use `HEADER_INCOMPATIBLE_COMPACT`, 32-bit ENTRY and
  ENTRY_ARRAY item offsets, the compact DATA payload offset, and the compact
  4 GiB offset ceiling. Regular output remains the default. Writer APIs and
  test ingesters expose an explicit compact option.
- Compact interoperability is validated by
  `tests/interoperability/run_compact_matrix.py`, which checks compact layout,
  stock `journalctl --verify --file`, stock journalctl reads, stock libsystemd
  reads, and every repository reader against every repository writer.

Current Go writer feature slice:

- regular journal files by default and compact journal files when
  `Options.Compact` is enabled;
- uncompressed DATA objects by default;
- optional zstd, xz, and lz4-compressed DATA object writing with configurable
  compression threshold;
- keyed hash tables using the journal file ID;
- byte-safe DATA field values through `Field.Value []byte`;
- high-level directory writing with Netdata-compatible chain active naming by
  default (`<source>@<seqnum-id>-<head-seqnum>-<head-realtime>.journal`) and an
  explicit strict systemd active naming option (`<source>.journal`);
- zero-entry crash-created active files are discarded on reopen before append so
  sequence numbers continue from the existing chain tail;
- rotation by entry count, active file size, and active file duration measured
  from the active file head realtime to the incoming entry realtime;
- tracked journal-file-count, committed-byte-size, and archive-head-age
  retention. The tracked active/current file counts toward retention envelopes
  but is never selected for deletion to satisfy retention limits. Unset limits
  are disabled; explicitly enabled zero or negative limits fail construction.
  `EnforceRetention()` applies retention without requiring a rotation or close;
- high-level Go directory writer construction supports lazy open by default and
  eager active-file open through `LogOpenEager`, so integrations can validate
  file creation/open, writer lock acquisition, and writer options before
  accepting work;
- high-level Go identity handling supports host/random fallback by default and
  `LogIdentityStrict` for integrations that require explicit machine and boot
  IDs;
- high-level Go path accessors expose the configured root, effective
  machine-id journal directory, exact active path after file creation, machine
  ID, boot ID, and source prefix;
- high-level Go lifecycle callbacks report created, rotated, and
  retention-deleted journal paths; artifact-size callbacks include
  consumer-owned sidecar bytes in size-based retention decisions;
- high-level Go `EntryOptions.SourceRealtimeUsec` injects
  `_SOURCE_REALTIME_TIMESTAMP`, and non-progressing realtime / non-zero
  monotonic overrides are clamped forward for strict chain ordering;
- high-level Rust, Go, Node.js, and Python `Log` writers accept Netdata/OTEL
  field names and automatically remap non-systemd-compatible names before
  writing. Each active journal file emits `ND_REMAPPING=1` metadata rows for new
  mappings, and data rows use stock-compatible `ND_*` field names. User-supplied
  protected names that begin with `_` are remapped; SDK-owned protected fields
  such as `_BOOT_ID` and `_SOURCE_REALTIME_TIMESTAMP` are injected internally.
  Low-level single-file writers remain strict and reject invalid field names;
- pure cross-SDK cooperative lockfile with stale-owner detection, plus a
  secondary POSIX `flock`, to protect the one-writer contract among
  cooperating SDK writers;
- Forward Secure Sealing TAG writing with configurable deterministic test
  options and stock `journalctl --verify --verify-key` validation for generated
  sealed files;
- live one-writer/multiple-reader compatibility with stock `journalctl --file` and stock libsystemd readers for the current writer slice.

Current shared high-level directory writer API slice:

- Rust, Go, Node.js, and Python expose lazy open by default and an eager open
  mode that creates or opens the active journal file during construction.
- Rust, Go, Node.js, and Python apply configured retention once when an active
  writer is opened or created. Existing-active reopen and eager open enforce
  retention during construction; lazy archived-only construction remains
  side-effect-free until the first append opens the active file, then retention
  runs before the first entry is written. The active/current file is protected
  and normal retention deletion lifecycle events are reused.
- Rust, Go, Node.js, and Python strict systemd naming mode archives any stale
  Netdata chain-named `ONLINE` active file before creating `<source>.journal`,
  preserving sequence continuity without leaving parallel active files in the
  same journal directory.
- Rust, Go, Node.js, and Python expose a strict identity mode requiring
  explicit machine ID and boot ID; default identity mode uses explicit IDs when
  provided, otherwise host/random fallback where the language implementation
  can do so without linking to journald.
- Rust, Go, Node.js, and Python expose configured-root, effective machine-id
  journal directory, active path, machine ID, boot ID, and source-prefix
  accessors on the high-level directory writer.
- Rust, Go, Node.js, and Python lifecycle observers/callbacks report active
  file creation, archive/rotation, and retention deletion with concrete journal
  paths. Callback failures are best-effort and do not roll back completed
  journal operations by default.
- Rust, Go, Node.js, and Python high-level `Log` instances are single-writer
  mutable objects. Callers must serialize method calls on one instance; the SDK
  writer lock protects the one-writer file contract across cooperating SDK
  instances and processes, but it does not add hidden per-append mutex cost
  inside a single `Log`.
- Rust, Go, Node.js, and Python support artifact-size providers/callbacks so
  consumer-owned per-journal sidecar bytes are included in size-based retention
  decisions. Missing artifacts should be reported by returning zero; unexpected
  provider errors abort retention where the API can surface the error.
- Rust, Go, Node.js, and Python high-level append paths support source realtime
  injection through `_SOURCE_REALTIME_TIMESTAMP` and clamp non-progressing
  realtime / non-zero monotonic overrides forward to preserve strict journal
  ordering.
- Rust, Go, Node.js, and Python reject explicitly enabled zero policy limits in
  the newer optional-policy API surface. Existing Node.js and Python legacy
  numeric `max* = 0` options remain accepted as disabled-limit compatibility
  aliases until their public package stability policy is finalized.

## Reader Target

Readers must support applicable historical journal files represented by the shared fixture suite, including corrupted fixture behavior where the expected result is a controlled error or partial recovery.

Accepted reader API layers:

- Idiomatic file and directory readers expose language-native entry objects,
  binary field values, repeated field values, cursor/realtime metadata, field
  enumeration, unique value enumeration, and boot listing for the accepted file
  slice.
- The libsystemd-compatible facade is available in Rust, Go, Node.js, and
  Python for file-backed use. It includes open file, open directory, open files,
  close, seek head/tail/realtime/cursor, next/previous/skip, add match,
  add conjunction/disjunction, flush matches, get entry, get data, restart and
  enumerate current-entry data, enumerate fields, direct unique queries as
  language-native `(field, raw value)` pairs, stateful unique enumeration as
  `FIELD=value` payloads, get realtime, get monotonic/boot metadata, get
  seqnum, get cursor, test cursor, output formatting, and boot listing.
- Current-entry data enumeration and query-unique stateful enumeration are
  binary-safe and preserve repeated values. `GetData` returns the first value
  for a repeated field; callers that need every repeated value use
  restart/enumerate data.
- Directory readers and `OpenFiles` sort accepted non-overlapping journal files
  by file head realtime and support direction-aware realtime seek across file
  boundaries. Realtime interleaving across overlapping journal files remains an
  interoperability-phase target.
- Daemon-only libsystemd/journalctl operations remain outside the SDK facade
  target and must fail with controlled unsupported behavior when exposed.

Current Go reader feature slice:

- regular and compact journal files;
- files named `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst`;
- whole-file zstd fixtures and zstd, xz, and lz4-compressed DATA objects
  through pure-Go dependencies;
- directory iteration across active and archived files;
- forward/backward iteration, cursors, realtime and monotonic timestamps,
  seqnum metadata, binary field values, repeated field values, field
  enumeration, current-entry data enumeration, and unique value enumeration;
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

- directory iteration is sequential by journal file and validated for
  non-overlapping active/archive files; realtime interleaving across
  overlapping multi-file directories is tracked under the interoperability
  phase;
- sealed TAG/HMAC verification APIs and file-backed journalctl `--verify-key`
  are implemented for repository-generated sealed files; full systemd
  object-graph verification parity remains tracked under SOW-0022;
- daemon-only journalctl operations remain unsupported.

Current Rust writer feature slice:

- regular journal files by default and compact journal files when
  `JournalFileOptions::with_compact(true)` or `journal::Config::with_compact(true)`
  is enabled;
- uncompressed DATA objects by default;
- optional zstd, xz, and lz4-compressed DATA object writing with configurable
  compression threshold, including Rust zstd frame content-size metadata
  required by stock systemd verification;
- keyed hash tables using the journal file ID;
- deterministic file ID selection through `JournalFileOptions::with_file_id()`
  for reference fixture generation and conformance checks;
- byte-safe field values through `&[u8]` field payloads;
- direct-file writing through `journal_core`;
- high-level directory writing with Netdata-compatible chain active naming by
  default and an explicit strict systemd active naming option;
- zero-entry crash-created active files are discarded on reopen before append so
  sequence numbers continue from the existing chain tail;
- entry-count, file-size, and active-file-duration rotation. Duration rotation
  uses active file head realtime and the incoming entry realtime;
- tracked journal-file-count, committed-byte-size, and archive-head-age
  retention. The tracked active/current file counts toward retention envelopes
  but is never selected for deletion to satisfy retention limits. Rust uses
  `None` to disable each limit. `Log::enforce_retention()` applies retention
  without requiring a rotation or close;
- pure cross-SDK cooperative lockfile with stale-owner detection to protect the
  one-writer contract among cooperating SDK writers;
- Forward Secure Sealing TAG writing with configurable deterministic test
  options and stock `journalctl --verify --verify-key` validation for generated
  sealed files;
- live one-writer/multiple-reader compatibility with stock `journalctl --file`
  and stock libsystemd readers for the current writer slice.

Current Rust reader feature slice:

- regular and compact journal files;
- files named `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst`;
- whole-file zstd fixtures and zstd, lz4, and xz-compressed DATA objects through
  pure-Rust dependencies;
- directory iteration across active and archived files;
- forward/backward iteration, cursors, realtime and monotonic timestamps,
  seqnum metadata, binary field values, repeated field values, field
  enumeration, current-entry data enumeration, unique value enumeration, and
  systemd-compatible export/json/text formatting;
- libsystemd-style match tree behavior from `SdJournalAddMatch()`,
  `SdJournalAddDisjunction()`, and `SdJournalAddConjunction()`;
- file-backed Rust journalctl behavior for `--file`, `--directory`, text/json/export
  output, field listing, boot listing, repeated same-field OR matches, and `+`
  disjunction;
- Rust conformance adapter support for reader, matching, importer, compression,
  cursor, enumeration, stream, export, header parsing, and file-backed
  journalctl cases.

Current Rust reader limitations:

- directory iteration is sequential by journal file and validated for
  non-overlapping active/archive files; realtime interleaving across overlapping
  multi-file directories is tracked under the interoperability phase;
- boot listing uses file-level boot metadata in this slice;
- sealed TAG/HMAC verification APIs and file-backed journalctl `--verify-key`
  are implemented for repository-generated sealed files; full systemd
  object-graph verification parity remains tracked under SOW-0022;
- daemon-only journalctl operations remain unsupported.

Current Node.js writer feature slice:

- regular journal files by default and compact journal files when
  `compact: true` or `format: 'compact'` is enabled;
- uncompressed DATA objects by default;
- optional zstd, xz, and lz4-compressed DATA object writing with configurable
  compression threshold through Node.js built-in `node:zlib`, pure
  JavaScript `lz4js@0.2.0`, and `node-liblzma@5.0.1` WASM path;
- keyed hash tables using the journal file ID;
- byte-safe field values through `Buffer`, `Uint8Array`, and string-compatible
  field values;
- direct-file writing through `Writer`;
- high-level directory writing through `Log` with Netdata-compatible chain
  active naming by default and an explicit strict systemd active naming option;
- zero-entry crash-created active files are discarded on reopen before append so
  sequence numbers continue from the existing chain tail;
- entry-count, file-size, and active-file-duration rotation. Duration rotation
  uses active file head realtime and the incoming entry realtime;
- tracked journal-file-count, committed-byte-size, and archive-head-age
  retention. The tracked active/current file counts toward retention envelopes
  but is never selected for deletion to satisfy retention limits. Omitted or
  zero-valued limits are disabled. `log.enforceRetention()` applies retention
  without requiring a rotation or close;
- pure cross-SDK cooperative lockfile with stale-owner detection to protect the
  one-writer contract among cooperating SDK writers;
- Forward Secure Sealing TAG writing with configurable deterministic test
  options and stock `journalctl --verify --verify-key` validation for generated
  sealed files;
- live one-writer/multiple-reader compatibility with stock `journalctl --file`
  and stock libsystemd readers for the current writer slice.

Current Node.js reader feature slice:

- regular and compact journal files;
- files named `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst`;
- whole-file zstd fixtures through Node.js built-in `node:zlib`;
- zstd, xz, and lz4-compressed DATA objects through Node.js built-in `node:zlib`,
  `node-liblzma@5.0.1` WASM path, and pure JavaScript `lz4js@0.2.0`;
- directory iteration across active and archived files;
- forward/backward iteration, cursors, realtime and monotonic timestamps,
  seqnum metadata, binary field values as `Buffer`, repeated field values,
  field enumeration, current-entry data enumeration, and unique value
  enumeration;
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

- directory iteration is sequential by journal file and validated for
  non-overlapping active/archive files; realtime interleaving across overlapping
  multi-file directories is tracked under the interoperability phase;
- boot listing uses file-level boot metadata in this slice;
- sealed TAG/HMAC verification APIs and file-backed journalctl `--verify-key`
  are implemented for repository-generated sealed files; full systemd
  object-graph verification parity remains tracked under SOW-0022;
- daemon-only journalctl operations remain unsupported.

Current Python writer feature slice:

- regular journal files by default and compact journal files when
  `compact: True` or `format: 'compact'` is enabled;
- uncompressed DATA objects by default;
- optional zstd, xz, and lz4-compressed DATA object writing with configurable
  compression threshold through Python `compression.zstd`, standard-library
  `lzma`, and `lz4==4.4.5`;
- keyed hash tables using the journal file ID;
- byte-safe field values through `bytes`, `bytearray`, `memoryview`, and
  string-compatible field values;
- direct-file writing through `Writer`;
- high-level directory writing through `Log` with Netdata-compatible chain
  active naming by default and an explicit strict systemd active naming option;
- zero-entry crash-created active files are discarded on reopen before append so
  sequence numbers continue from the existing chain tail;
- entry-count, file-size, and active-file-duration rotation. Duration rotation
  uses active file head realtime and the incoming entry realtime;
- tracked journal-file-count, committed-byte-size, and archive-head-age
  retention. The tracked active/current file counts toward retention envelopes
  but is never selected for deletion to satisfy retention limits. Omitted or
  zero-valued limits are disabled. `log.enforce_retention()` applies retention
  without requiring a rotation or close;
- pure cross-SDK cooperative lockfile with stale-owner detection, plus a
  secondary POSIX `flock`, to protect the one-writer contract among
  cooperating SDK writers;
- Forward Secure Sealing TAG writing with configurable deterministic test
  options and stock `journalctl --verify --verify-key` validation for generated
  sealed files;
- live one-writer/multiple-reader compatibility with stock `journalctl --file`
  and stock libsystemd readers for the current writer slice.

Current Python reader feature slice:

- regular and compact journal files;
- files named `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst`;
- whole-file zstd fixtures and zstd-compressed DATA objects through Python
  `compression.zstd` where the optional standard-library module is available;
- xz and lz4-compressed DATA objects through standard-library `lzma` and
  `lz4==4.4.5`;
- directory iteration across active and archived files, including one machine-id
  subdirectory level;
- forward/backward iteration, cursors, realtime and monotonic timestamps,
  seqnum metadata, binary field values as `bytes`, repeated field values,
  field enumeration, current-entry data enumeration, and unique value
  enumeration;
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

- directory iteration is sequential by journal file and validated for
  non-overlapping active/archive files; realtime interleaving across overlapping
  multi-file directories is tracked under the interoperability phase;
- boot listing uses file-level boot metadata in this slice;
- sealed TAG/HMAC verification APIs and file-backed journalctl `--verify-key`
  are implemented for repository-generated sealed files; full systemd
  object-graph verification parity remains tracked under SOW-0022;
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
