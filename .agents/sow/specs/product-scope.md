# Product Scope Specification

## Purpose

This project produces pure SDKs and file-backed journalctl-compatible tools for systemd journal files.

## Language Targets

- Rust
- Go
- Node.js
- Python

## Delivery Priority

- Current exception: Rust writer parity/API work in SOW-0037 is allowed before
  more Go work so Rust can be the audited project reference for the other
  implementations.
- The Go writer is the first implementation deliverable after the shared test harness is accepted and the SOW-0037 Rust reference slice is stable.
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
- Writer live-reader publication cadence is configurable, but the default is
  systemd-compatible. A non-default cadence narrows the live-reader visibility
  contract and must be labelled in tests, benchmarks, and integration
  guidance.

## Live Concurrency Compatibility Contract

Writer compatibility:

- Every writer implementation's default live publication mode must produce
  files that stock `journalctl --file` can read while the writer is still
  appending.
- Every writer implementation's default live publication mode must produce
  files that stock libsystemd journal readers can read while the writer is
  still appending.
- Configured latency-tolerant publication modes remain valid SDK modes, but
  they must not be claimed as stock live-follow compatible unless the matching
  live matrix proves that mode. They must still pass clean-close verification,
  final reads, and cross-language reads after sync/close.
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
- The live feature matrix must cover regular files, zstd/xz/lz4 DATA-compressed
  files, compact files, compact plus DATA-compressed files, and sealed files.
  For sealed files, final stock verification must pass with the deterministic
  test `--verify-key`.
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

Writer API hierarchy:

- Every language must expose a systemd-compatible raw full-field payload writer
  layer where each field is already encoded as `KEY=value` bytes. This mirrors
  systemd v260.1 `sd_journal_sendv()` / `journal_file_append_entry()` behavior
  and is the low-level compatibility layer. The first `=` byte is the field
  separator; payloads without a separator or with an empty field name are API
  errors in every policy mode.
- Every language must expose a structured binary-safe writer layer where each
  field is represented as `{name, value}` / `Field{Name, Value}` without
  requiring callers to concatenate and then re-parse `KEY=value` bytes. This is
  the canonical SDK hot path for producers that already hold structured values.
- Every direct-file writer and high-level directory writer exposes the same
  field-name policy layers:
  - `RAW`: accepts every field name the journal DATA structure can represent
    directly, currently non-empty and no `=` in the field name. Values are
    arbitrary bytes and may contain `=`, NUL, and other binary data. RAW-mode
    files are journal files, but they are not guaranteed to be accepted by
    stock systemd tooling when field names violate systemd conventions.
  - `JOURNALD`: default trusted-producer mode. It accepts non-empty field names
    up to 64 bytes, rejects digit-first names, allows only uppercase ASCII
    letters, digits, and underscores, and allows leading `_` protected fields
    such as `_HOSTNAME` and `_TRANSPORT`.
  - `JOURNAL-APP`: untrusted application-facing mode. It uses the same
    character and length rules as `JOURNALD`, disallows leading `_`, drops
    invalid caller fields, and fails only when no caller field remains. For raw
    full-payload APIs, malformed payloads are rejected before field-name
    filtering.
- The SDK must not perform producer-specific field-name remapping. Consumers
  that need their own naming scheme must transform fields before calling the
  SDK writer API.
- Low-level writers keep systemd-style ENTRY item normalization by default:
  DATA object references are sorted by on-disk DATA object offset and duplicate
  DATA references in one entry are removed.
- A trusted unique-payload option may skip duplicate DATA reference elimination
  only when the caller guarantees that one entry contains no duplicate full
  `KEY=value` payloads. This option must not skip offset sorting unless a later
  SOW records measured evidence, compatibility validation, and a user decision
  for a non-byte-identity performance mode.
- Jenkins lookup3 hashing follows systemd `jenkins_hashlittle2()` exactly,
  including the empty payload value `0xdeadbeefdeadbeef`.
- Every direct-file writer and high-level directory writer exposes
  `live_publish_every_entries` using the language's idiomatic casing. `1` is
  the default and performs explicit systemd-style live-reader publication after
  every appended entry. `0` disables explicit SDK live publication for
  latency-tolerant poll/snapshot consumers. `N > 1` publishes after every `N`
  appended entries. This setting controls live-reader publication and wakeup
  behavior; it is not a durability sync or `fsync` cadence. Node.js and Python
  direct writers use ordinary file writes, so the option controls explicit
  publication calls but does not promise zero kernel-visible write events.

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
  `tests/interoperability/run_compact_matrix.py`, which checks structural
  layout invariants, compact offset constraints, optional compression flags,
  stock `journalctl --verify --file`, stock journalctl reads, stock libsystemd
  reads, and every repository reader against every repository writer.
- Compressed writer output is structurally validated by
  `tests/interoperability/run_compression_matrix.py`. Compression tests require
  the expected header flag, at least one DATA object with the expected
  compression flag, valid object order and offsets, counter/tail metadata
  parity, hash-chain consistency, stock `journalctl --verify --file`, stock
  journalctl reads, stock libsystemd reads, and every repository reader.
- DATA compression threshold policy follows systemd v260.1: default threshold
  is 512 bytes, configured thresholds below 8 bytes are clamped to 8 bytes, and
  compression is attempted for payloads whose uncompressed DATA payload length
  is greater than or equal to the threshold. The Go zero-value options struct
  treats `CompressThresholdBytes == 0` as unset so `Options{}` still uses the
  systemd default.
- Timestamp policy follows the Netdata vendored writer split. Low-level
  single-file writers preserve explicit caller-provided realtime and monotonic
  timestamps without rejecting or clamping, so callers can deliberately create
  byte-exact or corrupt-test files and are responsible for same-boot monotonic
  validity. High-level Rust, Go, Node.js, and Python `Log` writers clamp
  non-progressing entry realtime and same-boot monotonic overrides, including
  explicit zero monotonic overrides, forward so ingestion outputs remain
  stock-verifiable. On reopen, high-level writers seed the monotonic clamp
  floor from the persisted chain tail only when the tail entry boot ID matches
  the current writer boot ID.

Current writer performance certification status:

- SOW-0042 certified Rust and Go writer performance for the accepted compact,
  no-compression, FSS-off direct and directory production baselines.
- SOW-0042 certified Node.js and Python writer correctness for the same
  baselines, including stock `journalctl --verify --file` and stock
  `journalctl --directory` readback, but did not certify their writer
  performance for high-throughput ingestion.
- Node.js and Python writer performance remains a known limitation tracked by
  SOW-0051. SOW-0042 measured Node.js and Python around 0.9k-1.0k append
  rows/s on the accepted writer baselines, compared with about 31k-38k rows/s
  for systemd C and about 45k-59k rows/s for Rust and Go depending on surface
  and live publication cadence.

Current Go writer feature slice:

- regular journal files by default and compact journal files when
  `Options.Compact` is enabled;
- uncompressed DATA objects by default;
- optional zstd, xz, and lz4-compressed DATA object writing with configurable
  compression threshold using the shared systemd threshold policy;
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
  `_SOURCE_REALTIME_TIMESTAMP`, and non-progressing realtime / monotonic
  overrides are clamped forward for strict chain ordering. `RealtimeUsecSet`
  and `MonotonicUsecSet` distinguish explicit zero timestamp overrides from
  omitted zero-value struct fields;
- high-level Rust, Go, Node.js, and Python `Log` writers use `JOURNALD`
  field-name policy by default, preserving caller-provided protected systemd
  fields such as `_HOSTNAME`. SDK-owned protected fields such as `_BOOT_ID` and
  `_SOURCE_REALTIME_TIMESTAMP` are injected internally under journald-compatible
  rules. `JOURNAL-APP` and `RAW` are explicit caller-selected policies;
- pure cross-SDK cooperative lockfile with stale-owner detection, plus a
  secondary POSIX `flock`, to protect the one-writer contract among
  cooperating SDK writers;
- Forward Secure Sealing TAG writing with configurable deterministic test
  options and stock `journalctl --verify --verify-key` validation for generated
  sealed files;
- default live publication mode one-writer/multiple-reader compatibility with
  stock `journalctl --file`, stock libsystemd readers, and all repository
  readers for regular,
  zstd/xz/lz4-compressed DATA, compact, compact plus compressed DATA, and
  sealed writer slices.

Current shared high-level directory writer API slice:

- Rust, Go, Node.js, and Python expose lazy open by default and an eager open
  mode that creates or opens the active journal file during construction.
- Rust, Go, Node.js, and Python apply configured retention once when an active
  writer is opened or created. Existing-active reopen and eager open enforce
  retention during construction; lazy archived-only construction remains
  side-effect-free until the first append opens the active file, then retention
  runs before the first entry is written. The active/current file is protected
  and normal retention deletion lifecycle events are reused.
- Rust, Go, Node.js, and Python derive default active-file rotation thresholds
  from retention when explicit rotation thresholds are omitted. If size
  retention is configured and rotation max file size is unset, the effective
  active-file max size is `retention_max_bytes / 20`, normalized with
  systemd-compatible minimum/alignment and compact-size guardrails. If age
  retention is configured and rotation max duration is unset, the effective
  active-file max duration is `retention_max_age / 20`, rounded up to the
  implementation's smallest supported positive interval. Explicit rotation max
  file size and max duration override these derived defaults. This contract
  makes default retention operate in 5% chunks by size, by time, or by both
  dimensions when both retention limits are configured.
- Rust, Go, Node.js, and Python direct-file and high-level writers use the
  effective max file size to choose systemd-compatible hash-table sizing:
  data buckets are `max(max_file_size * 4 / 768 / 3, 2047)` and field buckets
  are `1023`, unless the direct-file caller explicitly overrides bucket counts.
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
  realtime / monotonic overrides forward to preserve strict journal ordering,
  including explicit zero monotonic overrides.
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
- RAW-mode reader representation treats full `FIELD=value` DATA payload bytes
  as the canonical byte-identical surface. String-keyed field maps are
  convenience views for UTF-8 field names and must not invent lossy replacement
  names for non-UTF8 RAW field names. Rust currently exposes split
  byte-preserving `Entry::raw_fields()`, `Entry::get_raw()`, and
  `Entry::get_raw_values()` methods; Go, Node.js, and Python reader alignment
  SOWs must expose equivalent idiomatic byte-name surfaces before claiming RAW
  reader parity.
- JSON output, field enumeration, unique queries, and `get_data`-style facade
  helpers are UTF-8 field-name surfaces. Byte-exact RAW names are available
  through full payload/data enumeration and idiomatic byte-name APIs.
- Performance-sensitive readers should use the raw current-entry payload
  visitor/enumeration APIs when they already need byte-level `FIELD=value`
  payloads. Convenience entry materialization APIs may build maps, repeated
  value maps, owned payload vectors, and cursor strings and are not the
  primary hot path.
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
- Directory readers and `OpenFiles` merge candidate entries across all opened
  files using systemd-compatible ordering, including overlapping realtime
  ranges. Same seqnum-source entries compare by seqnum; same boot entries
  compare by monotonic time; otherwise comparable boot order, realtime, and
  entry xor hash are used.
- `OpenDirectory` and file-backed `journalctl --directory` traverse the root
  directory plus one immediate 128-bit machine-id subdirectory level. Accepted
  subdirectory names are 32 hex digits or dashed UUID form; namespace-suffix
  directories are skipped by default because stock file-backed
  `journalctl --directory` does not opt into namespace discovery.
- Directory traversal follows symlinks to regular files, accepts `.journal` and
  `.journal~` names, and additionally accepts whole-file `.journal.zst` and
  `.journal~.zst` as a repository extension. It does not recurse below the one
  accepted subdirectory level.
- Empty directories open successfully and produce no entries. Directory readers
  skip files that cannot be opened as journals, matching stock read behavior.
- File-backed `journalctl --verify --directory` uses the same traversal and
  skips files that cannot be opened by the directory reader. Explicit
  `--verify --file` still reports corruption for the named file.
- Directory readers and file-backed `journalctl --directory` support mixed
  per-file feature sets in one directory: regular and compact files,
  uncompressed and zstd/xz/lz4 DATA-compressed files, sealed and unsealed files,
  active and archived names, and repository whole-file `.journal.zst` files.
  Normal reads do not require a verification key for sealed files. Directory
  verification without a key succeeds for unsealed-only directories and fails
  for sealed files; the correct `--verify-key` validates mixed sealed/unsealed
  directories, and a wrong key fails.
- Rust, Go, Node.js, and Python verification APIs and file-backed
  `journalctl --verify` perform raw object-graph verification for the supported
  feature slices before normal reader traversal. The shared parity matrix
  `tests/interoperability/run_verify_matrix.py` validates stock systemd and all
  repository verifiers against positive regular, zstd/xz/lz4 DATA-compressed,
  compact, compact plus DATA-compressed, and sealed files, plus negative object
  type, object size, DATA/FIELD payload hash, DATA hash-table membership,
  entry-array ordering, header counter, missing main entry-array, entry seqnum,
  tail seqnum, tail monotonic, and TAG/FSS HMAC corruption classes.
- Daemon-only libsystemd/journalctl operations remain outside the SDK facade
  target and must fail with controlled unsupported behavior when exposed.

Current Go reader feature slice:

- regular and compact journal files;
- files named `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst`;
- whole-file zstd fixtures and zstd, xz, and lz4-compressed DATA objects
  through pure-Go dependencies;
- directory iteration across active and archived files with stock-compatible
  root plus one machine-id subdirectory traversal and interleaved multi-file
  ordering;
- forward/backward iteration, cursors, realtime and monotonic timestamps,
  seqnum metadata, binary field values, repeated field values, field
  enumeration, current-entry data enumeration, and unique value enumeration;
- systemd-compatible export output for binary fields using size-prefixed field
  values and blank-line entry separators;
- systemd-compatible JSON output for duplicate fields and binary values;
- libsystemd-style match tree behavior from `sd_journal_add_match()`,
  `sd_journal_add_disjunction()`, and `sd_journal_add_conjunction()`;
- file-backed Go journalctl behavior for `--file`, `--directory`,
  text/json/export output, field listing, boot listing, realtime range
  filtering with `--since`/`--until`, boot filtering with `--boot`, follow mode
  with `--follow`, repeated same-field OR matches, and `+` disjunction;
- Go conformance adapter support for reader, matching, importer, compression,
  cursor, enumeration, stream, export, and file-backed journalctl cases.

Current Go reader limitations:

- daemon-only journalctl operations remain unsupported.

Current Rust writer feature slice:

- regular journal files by default and compact journal files when
  `JournalFileOptions::with_compact(true)` or `journal::Config::with_compact(true)`
  is enabled;
- uncompressed DATA objects by default;
- optional zstd, xz, and lz4-compressed DATA object writing with configurable
  compression threshold, including Rust zstd frame content-size metadata
  required by stock systemd verification, using the shared systemd threshold
  policy;
- keyed hash tables using the journal file ID;
- deterministic file ID selection through `JournalFileOptions::with_file_id()`
  for reference fixture generation and conformance checks;
- byte-safe raw full `KEY=value` field payloads through `&[u8]`;
- byte-safe structured fields through `StructuredField { name, value }` and
  `EntryField`, with structured values written without requiring a contiguous
  `KEY=value` allocation unless compression needs a contiguous buffer;
- direct-file writing through `journal_core`, including raw full-payload append,
  structured append, mixed `EntryField` append, and trusted unique-payload
  options;
- high-level directory writing with Netdata-compatible chain active naming by
  default and an explicit strict systemd active naming option;
- high-level Rust `Log` structured write methods that preserve the existing
  rotation, retention, and timestamp behavior while avoiding raw `KEY=value`
  construction in the structured hot path;
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
- default live publication mode one-writer/multiple-reader compatibility with
  stock `journalctl --file`, stock libsystemd readers, and all repository
  readers for regular,
  zstd/xz/lz4-compressed DATA, compact, compact plus compressed DATA, and
  sealed writer slices.

Current Rust reader feature slice:

- regular and compact journal files;
- files named `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst`;
- whole-file zstd fixtures and zstd, lz4, and xz-compressed DATA objects through
  pure-Rust dependencies;
- directory iteration across active and archived files with stock-compatible
  root plus one machine-id subdirectory traversal and interleaved multi-file
  ordering;
- forward/backward iteration, cursors, realtime and monotonic timestamps,
  seqnum metadata, binary field values, repeated field values, field
  enumeration, current-entry data enumeration, unique value enumeration, and
  systemd-compatible export/json/text formatting;
- configurable reader bounds through `ReaderOptions`: default `Live` mode uses
  systemd-style cached mutable bounds and refreshes file size only when a read
  would exceed the cached end of file, while `Snapshot` mode fixes the file
  size at open time for polling/query use cases that do not need to observe
  appends during the current scan;
- `ReaderOptions` exposes windowed and whole-file mmap strategies for snapshot
  readers. Windowed snapshot is the current Rust single-file hot-path baseline;
- raw current-entry payload visitors on file and directory readers for
  allocation-light scans that operate on borrowed `FIELD=value` bytes;
- byte-preserving RAW field-name representation through `Entry::raw_fields()`,
  `Entry::get_raw()`, and `Entry::get_raw_values()`. `Entry.fields` and
  `Entry.field_values` remain UTF-8 string-keyed convenience maps and do not
  synthesize lossy names for non-UTF8 RAW field names;
- export byte output preserves non-UTF8 RAW field names; JSON output, field
  enumeration, unique queries, and `get_data` facade helpers remain UTF-8
  field-name surfaces;
- libsystemd-style match tree behavior from `SdJournalAddMatch()`,
  `SdJournalAddDisjunction()`, and `SdJournalAddConjunction()`;
- file-backed Rust journalctl behavior for `--file`, `--directory`,
  text/json/export output, field listing, boot listing, realtime range
  filtering with `--since`/`--until`, boot filtering with `--boot`, follow mode
  with `--follow`, repeated same-field OR matches, and `+` disjunction;
- Rust conformance adapter support for reader, matching, importer, compression,
  cursor, enumeration, stream, export, header parsing, and file-backed
  journalctl cases.
- Current SOW-0044 regression benchmark evidence on a 100k-row compact fixture
  with 32 fields per row shows Rust single-file `sdk-payloads` live/windowed at
  about 1.34M rows/s and snapshot/windowed at about 1.36M rows/s versus stock
  libsystemd data enumeration at about 660k rows/s. The fixed live mode uses
  6 `statx` calls in the profiled hot-path run instead of the previous
  7,600,032-call refresh-every-slice behavior.

Current Rust reader limitations:

- boot listing APIs use file-level boot metadata in this slice; file-backed
  `--boot` filtering scans entry `_BOOT_ID` values;
- daemon-only journalctl operations remain unsupported.

Current Node.js writer feature slice:

- regular journal files by default and compact journal files when
  `compact: true` or `format: 'compact'` is enabled;
- uncompressed DATA objects by default;
- optional zstd, xz, and lz4-compressed DATA object writing with configurable
  compression threshold through Node.js built-in `node:zlib`, pure
  JavaScript `lz4js@0.2.0`, and `node-liblzma@5.0.1` WASM path, using the
  shared systemd threshold policy;
- keyed hash tables using the journal file ID;
- byte-safe field values through `Buffer`, `Uint8Array`, and string-compatible
  field values;
- direct-file writing through `Writer`, including structured append and raw
  full-payload append;
- high-level directory writing through `Log` with Netdata-compatible chain
  active naming by default, structured append and raw full-payload append, and
  an explicit strict systemd active naming option;
- high-level `Log` append paths write indexed `_BOOT_ID=<boot-id>` metadata for
  each entry and `_SOURCE_REALTIME_TIMESTAMP=<usec>` when source realtime is
  supplied;
- writer file access uses `Buffer` plus positioned `node:fs` reads/writes; no
  native mmap dependency is loaded by the Node.js SDK runtime path;
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
- default live publication mode one-writer/multiple-reader compatibility with
  stock `journalctl --file`, stock libsystemd readers, and all repository
  readers for regular,
  zstd/xz/lz4-compressed DATA, compact, compact plus compressed DATA, and
  sealed writer slices.

Current Node.js reader feature slice:

- regular and compact journal files;
- files named `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst`;
- whole-file zstd fixtures through Node.js built-in `node:zlib`;
- zstd, xz, and lz4-compressed DATA objects through Node.js built-in `node:zlib`,
  `node-liblzma@5.0.1` WASM path, and pure JavaScript `lz4js@0.2.0`;
- directory iteration across active and archived files with stock-compatible
  root plus one machine-id subdirectory traversal and interleaved multi-file
  ordering;
- forward/backward iteration, cursors, realtime and monotonic timestamps,
  seqnum metadata, binary field values as `Buffer`, repeated field values,
  field enumeration, current-entry data enumeration, and unique value
  enumeration;
- systemd-compatible export/json/text formatting for the accepted fixture set;
- libsystemd-style match tree behavior from `SdJournalAddMatch()`,
  `SdJournalAddDisjunction()`, and `SdJournalAddConjunction()`;
- file-backed Node.js journalctl behavior for `--file`, `--directory`,
  text/json/export output, field listing, boot listing, realtime range
  filtering with `--since`/`--until`, boot filtering with `--boot`, follow mode
  with `--follow`, repeated same-field OR matches, and `+` disjunction;
- Node.js conformance adapter support for reader, matching, importer,
  compression, cursor, enumeration, stream, export, header parsing, and
  file-backed journalctl cases.

Current Node.js reader/writer limitations:

- boot listing APIs use file-level boot metadata in this slice; file-backed
  `--boot` filtering scans entry `_BOOT_ID` values;
- daemon-only journalctl operations remain unsupported.

Current Python writer feature slice:

- regular journal files by default and compact journal files when
  `compact: True` or `format: 'compact'` is enabled;
- uncompressed DATA objects by default;
- optional zstd, xz, and lz4-compressed DATA object writing with configurable
  compression threshold through Python `compression.zstd`, standard-library
  `lzma`, and `lz4==4.4.5`, using the shared systemd threshold policy;
- keyed hash tables using the journal file ID;
- byte-safe field values through `bytes`, `bytearray`, `memoryview`, and
  string-compatible field values;
- direct-file writing through `Writer`, including structured append and raw
  full-payload append;
- high-level directory writing through `Log` with Netdata-compatible chain
  active naming by default, structured append and raw full-payload append, and
  an explicit strict systemd active naming option;
- high-level `Log` append paths write indexed `_BOOT_ID=<boot-id>` metadata for
  each entry and `_SOURCE_REALTIME_TIMESTAMP=<usec>` when source realtime is
  supplied;
- direct-file writer hot-path reads and writes use a whole-file mapped arena,
  with fd fallback before mapping and during cleanup;
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
- default live publication mode one-writer/multiple-reader compatibility with
  stock `journalctl --file`, stock libsystemd readers, and all repository
  readers for regular,
  zstd/xz/lz4-compressed DATA, compact, compact plus compressed DATA, and
  sealed writer slices.

Current Python reader feature slice:

- regular and compact journal files;
- files named `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst`;
- whole-file zstd fixtures and zstd-compressed DATA objects through Python
  `compression.zstd` where the optional standard-library module is available;
- xz and lz4-compressed DATA objects through standard-library `lzma` and
  `lz4==4.4.5`;
- directory iteration across active and archived files with stock-compatible
  root plus one machine-id subdirectory traversal and interleaved multi-file
  ordering;
- forward/backward iteration, cursors, realtime and monotonic timestamps,
  seqnum metadata, binary field values as `bytes`, repeated field values,
  field enumeration, current-entry data enumeration, and unique value
  enumeration;
- systemd-compatible export/json/text formatting for the accepted fixture set;
- libsystemd-style match tree behavior from `SdJournalAddMatch()`,
  `SdJournalAddDisjunction()`, and `SdJournalAddConjunction()`;
- file-backed Python journalctl behavior for `--file`, `--directory`,
  text/json/export output, field listing, boot listing, realtime range
  filtering with `--since`/`--until`, boot filtering with `--boot`, follow mode
  with `--follow`, repeated same-field OR matches, and `+` disjunction;
- Python conformance adapter support for reader, matching, importer,
  compression, cursor, enumeration, stream, export, header parsing, and
  file-backed journalctl cases.

Current Python reader/writer limitations:

- boot listing APIs use file-level boot metadata in this slice; file-backed
  `--boot` filtering scans entry `_BOOT_ID` values;
- daemon-only journalctl operations remain unsupported.

## journalctl Target

Implement journalctl rewrites in Rust, Go, Node.js, and Python for file-backed/query behavior.

Matching semantics:

- Different fields are ANDed.
- Repeated matches for the same field are OR alternatives.
- The `+` separator creates explicit disjunction groups and must be replicated for file-backed journalctl behavior.
- No new `KEY in [values]` syntax is required.

File-backed query semantics:

- `--since` and `--until` apply inclusive realtime timestamp boundaries.
- `--boot` supports `all`, the latest boot by default, numeric offsets, boot
  UUIDs, and boot UUID plus signed offsets for files and directories whose
  entries contain `_BOOT_ID`.
- `--follow` follows repository-supported file and directory inputs by polling
  file-backed readers and emitting newly appended entries in cursor order.
- The long `--follow` option is supported. Existing short `-f` file aliases are
  preserved in languages that already used `-f` for `--file`.

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
