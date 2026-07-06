# Product Scope Specification

## Purpose

This project produces pure SDKs and file-backed journalctl-compatible tools for
systemd journal files in Rust and Go.

## Language Targets

- Rust
- Go

## Rust Registry Packages

The Rust SDK's public crates.io package is `systemd-journal-sdk`. Consumers may
alias it as `journal` in Cargo dependencies to keep the existing
`journal::...` source path:

```toml
[dependencies]
journal = { package = "systemd-journal-sdk", version = "0.7.4" }
```

The Rust workspace also publishes lower-level project-prefixed packages for
consumers that need the same internal layers used by the SDK and by current
Netdata Rust integrations:

- `systemd-journal-sdk-common`
- `systemd-journal-sdk-core`
- `systemd-journal-sdk-registry`
- `systemd-journal-sdk-host`
- `systemd-journal-sdk-log-writer`
- `systemd-journal-sdk-index`
- `systemd-journal-sdk-engine`

## Consumer Documentation

Committed consumer documentation lives under `docs/` as GitHub wiki source.
The docs explain package selection, reader and writer API layers, Explorer and
Netdata-shaped query APIs, hot-path behavior, production profiles, and options
that can make a consumer accidentally leave the optimized path. The repository
publishes these pages to the GitHub wiki on trusted `master` pushes through the
wiki publication workflow.

## Delivery Priority

- Rust and Go are the only required product language targets.
- Go is required for Netdata Go integrations such as direct journal writing.
- Rust is required for Netdata Rust integrations and remains the audited
  reference for low-level journal behavior.
- Python and Node.js implementations were retired from product scope by
  SOW-0116 and moved under `experiments/`; they do not participate in parity,
  release, validation, or documentation gates.

## Core Contracts

- Implementations must not link to system journal libraries.
- Go implementations must not use CGO.
- Core journal readers and writers are file-format implementations only. They
  must not execute external programs, probe host identity, read host identity
  files or registries, or enforce writer locks by default.
- Core journal readers and writers operate only on caller-provided paths,
  journal bytes, timestamps, machine IDs, boot IDs, seqnum IDs, and options.
- Systemd/journald compatibility is a policy/API layer above the core writer.
  It may require caller-provided machine and boot IDs, but it must not silently
  discover host identity.
- The strict writer contract below is enforced by every product writer:
  - `_MACHINE_ID` and the default `_BOOT_ID` must be explicit caller
    inputs to every direct-file or directory writer construction.
  - Every appended entry's `__MONOTONIC_TIMESTAMP` must be an explicit
    caller input or an explicit helper return value.
  - Explicit zero monotonic remains valid only through the language's
    idiomatic "set" representation; omitted monotonic fails before
    entry mutation.
  - SDK writers do not silently fall back to generated machine IDs, boot
    IDs, or monotonic timestamps.
  - Opening an existing non-empty file may continue from the file's
    on-disk tail boot ID; that is file state, not host discovery.
  - File ID, seqnum ID, and generated sequence numbers may remain
    SDK-local defaults because they are not the three event
    identity/time anchors.
- Automatic machine/boot identity discovery is an optional helper service. A
  caller must explicitly invoke the helper and pass the result to the SDK.
- Cooperating-writer locking is an optional helper/wrapper service, independent
  from systemd compatibility. The journal file-format contract is one writer
  per file, but the file format does not define or enforce a portable lock
  protocol. Core writer constructors do not expose lock-enable options; callers
  acquire and release the optional lock helper separately around writer use.
- Host-observation mechanisms such as `/proc`, `/host/proc`,
  `/etc/machine-id`, platform registries, `sysctl`, `system_profiler`, `ps`,
  shell commands, subprocess APIs, and equivalent OS-specific identity sources
  are forbidden in core reader/writer runtime paths. They are allowed only in
  explicitly named optional helper code and tests for those helpers.
- Each product language must provide two API layers: an idiomatic SDK API and a libsystemd-compatible reader facade.
- The libsystemd-compatible reader facade is required unless a SOW records concrete evidence that it would require native bindings, violate the pure-language policy, or create an unsafe/unrepresentable API in that language.
- Common compression-library dependencies are allowed after dependency review.
  Journal parsing/writing must not depend on systemd or libjournal libraries.
- Rust/Go interoperability is mandatory: every product reader must read journal files produced by every product writer.
- The system must preserve systemd journal file concurrency expectations: one writer and multiple readers may operate on the same journal file according to journal rules.
- Live concurrency compatibility is a MUST, not a follow-up optimization. No writer or reader implementation may be called production-compatible until this is confirmed with stock systemd tooling and the shared cross-language suite.
- Writer live-reader publication cadence is configurable, but the default is
  systemd-compatible. A non-default cadence narrows the live-reader visibility
  contract and must be labelled in tests, benchmarks, and integration
  guidance.

## Rust Platform Behavior

- Linux remains the Rust reference runtime. Rust uses monotonic timestamps,
  mmap-backed hot paths, Unix directory sync, and a SIGBUS handler for mmap
  fault recovery.
- FreeBSD and macOS Rust builds use monotonic timestamps and the same core
  file-format reader/writer paths. Optional identity and writer-lock helpers
  are separate from the core file-format writer.
- Windows Rust builds use the same strict core file-format writer contract:
  callers provide generated-entry monotonic timestamps explicitly. The optional
  `journal_host` helper uses Windows unbiased interrupt time when callers
  intentionally want local-host helper timestamps. Optional identity and
  writer-lock helpers are separate from the core file-format writer. Directory
  fsync and SIGBUS handling are no-ops on Windows because those Unix
  durability/fault mechanisms do not have the same portable API surface.
- Non-Linux Rust target checks prove compilation only unless a SOW records
  runtime evidence from that operating system. Files generated on non-Linux
  targets still require Linux stock `journalctl --verify --file` and
  repository interoperability validation before production compatibility is
  claimed for those target/runtime combinations.

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
- Directory writers sync archived journal files on the caller path by default
  before rotation, close, or stale-active startup archive completes. Rust
  `Config::with_sync_on_archive(false)` and Go
  `LogConfig.SyncOnArchive: journal.SyncOnArchive(false)` are explicit
  latency-oriented opt-outs. When disabled, the caller owns archived-file
  durability before relying on, externally indexing, or deleting archived
  files. The SDK still performs the archive rename/close flow and parent
  directory sync where that implementation already does so.

Reader compatibility:

- Every reader implementation must read files while they are being appended by stock systemd journal writers when the test environment can provide one.
- Every reader implementation must read files while they are being appended by every writer implementation in this repository.
- Every reader implementation must correctly handle online journal state, tail metadata changes, entry-array growth, data hash-table growth by chaining, and observable file-size changes without treating normal live updates as corruption.
- Every reader implementation must support multiple readers observing the same live file concurrently.
- Reader follow/tail behavior must be validated against stock `journalctl` semantics for file-backed operation.
- Go reader default access mode is mmap-backed live reading on supported
  Unix-family and Windows targets, matching the optimized reader hot path.
  `ReadAt` remains an explicit option for tests, diagnostics,
  constrained-platform investigation, and controlled fallback evidence only; it
  is not a production reader mode.
- Reader SDKs should expose a current-entry payload visitor/enumerator hot path
  so consumers that already operate on `FIELD=value` bytes do not need to
  materialize full entry maps.

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
- Rust's legacy `jf` `journal_file::JournalWriter` remains a compatibility
  surface for the imported Netdata-era crate, but it is not the supported
  production writer path. It must not panic on unsupported append targets:
  historical unkeyed-hash files return a controlled unsupported-file error
  before entry mutation. New Rust writer integrations should use
  `journal-core` direct-file writing or `journal` / `journal-log-writer`
  directory writing.
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
  behavior; it is not a durability sync or `fsync` cadence.

Current shared writer layout contract:

- Deterministic regular uncompressed files written by Rust, Go,
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
- Timestamp policy follows the Netdata vendored writer split. Rust and Go
  writer append surfaces require caller-provided generated-entry monotonic
  timestamps; core writers and high-level `Log` writers do not synthesize a
  boot-monotonic value when the caller omits one. Low-level single-file writers
  preserve explicit caller-provided realtime and monotonic timestamps for
  byte-exact regeneration and corrupt-test files. High-level Rust and Go `Log`
  writers clamp non-progressing entry realtime and same-boot monotonic values,
  including explicit zero monotonic values, forward so ingestion outputs remain
  stock-verifiable. On reopen, high-level writers seed the monotonic clamp
  floor from the persisted chain tail only when the tail entry boot ID matches
  the current writer boot ID.

Current writer performance certification status:

- SOW-0042 certified Rust and Go writer performance for the accepted compact,
  no-compression, FSS-off direct and directory production baselines.
- Python and Node.js writer certifications are historical only after SOW-0116;
  those implementations now live under `experiments/` and are not product
  performance or correctness gates.

Current Go writer feature slice:

- regular journal files by default and compact journal files when
  `Options.Compact` is enabled;
- uncompressed DATA objects by default;
- optional zstd, xz, and lz4-compressed DATA object writing with configurable
  compression threshold using the shared systemd threshold policy, including
  zstd frame content-size metadata required by stock systemd verification and
  readback of large compressed payloads;
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
  file creation/open and writer options before accepting work;
- high-level Go identity handling is strict for the journal anchors owned by
  this project: callers provide machine ID, boot ID, and generated-entry
  monotonic timestamps explicitly, or they explicitly call the optional host
  helper and pass its values to the writer. Host identity discovery belongs to
  optional helpers that callers invoke explicitly;
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
- low-level Go `EntryOptions.Seqnum` can preserve original ENTRY sequence
  numbers during exact journal regeneration. Normal writers leave it zero for
  auto-incrementing sequence numbers. Overrides must move forward from the
  writer's next sequence number; gaps are allowed and rewinds are rejected;
- high-level Rust and Go `Log` writers use `JOURNALD`
  field-name policy by default, preserving caller-provided protected systemd
  fields such as `_HOSTNAME`. SDK-owned protected fields such as `_BOOT_ID` and
  `_SOURCE_REALTIME_TIMESTAMP` are injected internally under journald-compatible
  rules. `JOURNAL-APP` and `RAW` are explicit caller-selected policies;
- optional pure cross-SDK cooperative lockfile with stale-owner detection when
  callers explicitly enable the lock helper. The lock helper protects the
  one-writer contract among cooperating SDK writers, but it is independent from
  systemd compatibility and is not part of the core writer default;
- Forward Secure Sealing TAG writing with configurable deterministic test
  options and stock `journalctl --verify --verify-key` validation for generated
  sealed files. Go normalizes FSS start timestamps to systemd's verification-key
  epoch boundary: `floor(start / interval) * interval`;
- default live publication mode one-writer/multiple-reader compatibility with
  stock `journalctl --file`, stock libsystemd readers, and all repository
  readers for regular,
  zstd/xz/lz4-compressed DATA, compact, compact plus compressed DATA, and
  sealed writer slices.

Current shared high-level directory writer API slice:

- Rust and Go expose lazy open by default and an eager open
  mode that creates or opens the active journal file during construction.
- Rust and Go apply configured retention once when an active
  writer is opened or created. Existing-active reopen and eager open enforce
  retention during construction; lazy archived-only construction remains
  side-effect-free until the first append opens the active file, then retention
  runs before the first entry is written. The active/current file is protected
  and normal retention deletion lifecycle events are reused.
- Rust and Go derive default active-file rotation thresholds
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
- Rust and Go direct-file and high-level writers use the
  effective max file size to choose systemd-compatible hash-table sizing:
  data buckets are `max(max_file_size * 4 / 768 / 3, 2047)` and field buckets
  are `1023`, unless the direct-file caller explicitly overrides bucket counts.
- Rust and Go strict systemd naming mode archives any stale
  Netdata chain-named `ONLINE` active file before creating `<source>.journal`,
  preserving sequence continuity without leaving parallel active files in the
  same journal directory.
- Rust and Go high-level directory writers treat low-level
  append-open `unsupported journal` failures on existing active files as
  replaceable active-file failures. They preserve sequence identity when the
  header can still be read, move the old active file to a collision-safe
  disposed `*.journal~` name, and create a fresh active file. Low-level direct
  writer opens still return controlled unsupported errors.
- Rust and Go require explicit machine ID, boot ID, and generated-entry
  monotonic timestamps for normal writer/log appends. Optional host helpers may
  produce local-host values, but callers still pass those values explicitly to
  the writer; core writers do not generate SDK-local identity/time anchors and
  do not probe host identity.
- The strict writer contract for `_MACHINE_ID`, `_BOOT_ID`, and the
  generated-entry `__MONOTONIC_TIMESTAMP` is enforced by `journal.Create` /
  `journal.Log` in Go and by `journal_log_writer::Log` in Rust. Rust
  `JournalFileOptions::new` remains an explicit low-level constructor for
  caller-provided file identities and exact-regeneration paths; it does not
  discover or synthesize host identity. Missing or nil high-level log identity
  anchors and missing generated-entry monotonic timestamps are rejected before
  entry mutation. `LogOpenMode::Eager` may create an empty active file at
  construction time after explicit identity validation; it still cannot append
  an entry without explicit monotonic metadata. `LogIdentityAuto` and the Go
  writer-start-relative monotonic fallback are removed; the Rust log-writer
  `monotonic_now()` fallback is removed from the default append path. The
  `file_id`, `seqnum_id`, generated sequence number, and realtime commit
  timestamp defaults may remain because they are not the three event
  identity/time anchors owned by this contract.
- A separate optional host helper API is the only sanctioned way for callers
  to obtain local-host machine ID, boot ID, and a boot-anchored monotonic
  timestamp source. The Go helper is `go/journalhost`; the Rust helper is the
  `systemd-journal-sdk-host` workspace crate (lib name `journal_host`). The
  helper is opt-in; core writer and reader code does not import the helper
  package. Core Rust crate `journal-common` does not import host identity
  discovery; the previous `journal-common::system` host-identity module was
  removed. `journal_common::time::monotonic_now` remains a generic explicit
  utility, but core and log-writer append paths do not use it to fill missing
  entry monotonic timestamps.
- Per-platform host-helper source matrix (native APIs only; no subprocess, no
  CGO):
  - Linux: machine ID `/etc/machine-id` then `/var/lib/dbus/machine-id` by
    default. When callers explicitly configure a host filesystem prefix, the
    helper checks `<prefix>/etc/machine-id` and
    `<prefix>/var/lib/dbus/machine-id` before the container-local paths so
    containerized collectors can intentionally use host identity. Missing
    host-prefixed machine-id files fall back to container-local paths; present
    but invalid host-prefixed files fail instead of silently switching identity.
    Parsed machine IDs must be non-zero. Boot ID remains
    `/proc/sys/kernel/random/boot_id`; monotonic remains
    `clock_gettime(CLOCK_MONOTONIC)`.
  - FreeBSD 13+: machine ID `kern.hostuuid` sysctl (validate and reject
    all-zero jail sentinel); boot ID native `kern.boot_id`; monotonic
    `clock_gettime(CLOCK_UPTIME)`. FreeBSD 12 or environments where
    `kern.boot_id` is unavailable fall back to the state-backed synthesis
    path below.
  - macOS: machine ID native `gethostuuid(3)`; boot ID native
    `kern.bootsessionuuid` sysctl; monotonic `clock_gettime(CLOCK_UPTIME_RAW)`
    or equivalent.
  - Windows: machine ID `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid`
    registry or documented SMBIOS UUID fallback; boot ID state-backed
    synthesis; per-entry monotonic `QueryUnbiasedInterruptTime`; state-file
    boot marker `GetTickCount64`.
- The state-backed boot-ID synthesis file is the primary Windows boot-ID
  source and the FreeBSD fallback when `kern.boot_id` is unavailable. The
  state file contains exactly two ASCII lines:
  `last_estimated_boottime=<decimal unix microseconds>` and
  `last_boot_id=<32 lowercase hex UUID bytes>`. Initialization takes an
  exclusive lock; the helper serializes cross-process read/write. Same-boot
  reads do not rewrite the file. New-boot detection is
  `estimated_boottime > last_estimated_boottime + 30s`. Corrupt state is
  preserved as `.corrupt` when safe, a fresh UUID is generated, and a clean
  state file is written when possible. Any state open, lock, read, parse,
  copy, write, fsync, rename, or permission failure generates a fresh
  boot ID for this provider instance, continues without hard failure, and
  exposes degraded diagnostics. New Unix state files are created with
  `0600` permissions unless the caller provisions the path.
- The host helper must distinguish three boot-ID source classes in its
  diagnostics: `Native` (kernel/native boot ID), `StateBacked` (synthesized
  boot ID from a state file with healthy state), and `Degraded` (synthesized
  boot ID from a state file or fallback path where state failed or native
  discovery failed). Degraded boot IDs are still valid 128-bit UUIDs but
  the helper does not claim cross-process same-boot stability for them.
- Rust and Go expose configured-root, effective machine-id
  journal directory, active path, machine ID, boot ID, and source-prefix
  accessors on the high-level directory writer.
- Rust and Go lifecycle observers/callbacks report active
  file creation, archive/rotation, and retention deletion with concrete journal
  paths. Callback failures are best-effort and do not roll back completed
  journal operations by default.
- Rust and Go high-level `Log` instances are single-writer
  mutable objects. Callers must serialize method calls on one instance. The
  journal contract is one writer per file; optional SDK writer locks protect
  that contract across cooperating SDK instances only when explicitly enabled.
- Rust and Go support artifact-size providers/callbacks so
  consumer-owned per-journal sidecar bytes are included in size-based retention
  decisions. Missing artifacts should be reported by returning zero; unexpected
  provider errors abort retention where the API can surface the error.
- Rust and Go high-level append paths support source realtime
  injection through `_SOURCE_REALTIME_TIMESTAMP`, require explicit generated
  monotonic timestamps, and clamp non-progressing realtime / monotonic values
  forward to preserve strict journal ordering, including explicit zero
  monotonic values.
- Rust and Go reject explicitly enabled zero policy limits in
  the newer optional-policy API surface.

## Reader Target

Readers must support applicable historical journal files represented by the shared fixture suite, including corrupted fixture behavior where the expected result is a controlled error or partial recovery.

Accepted reader API layers:

- Rust and Go readers use bounded reader-memory access for
  production file reads. Rust and Go use rolling mmap where their runtimes
  support it. Go retains rolling positioned-read fallbacks for tests,
  diagnostics, constrained-platform investigation, and controlled fallback
  evidence only; read-at is not a production reader mode.
  Production readers must not load a whole journal file into resident memory
  as the default path. Current-row DATA returned by low-level/facade payload
  enumeration remains valid until the reader advances to another row or closes;
  compressed and cross-window DATA use row-scoped arena storage. Public
  file-path verification APIs are covered by the same bounded access contract:
  object-graph and sealed TAG/HMAC verification read through reader-backed byte
  sources rather than materializing the whole journal file in memory.
- Idiomatic file and directory readers expose language-native entry objects,
  binary field values, repeated field values, cursor/realtime metadata, field
  enumeration, unique value enumeration, and boot listing for the accepted file
  slice.
- RAW-mode reader representation treats full `FIELD=value` DATA payload bytes
  as the canonical byte-identical surface. String-keyed field maps are
  convenience views for UTF-8 field names and must not invent lossy replacement
  names for non-UTF8 RAW field names. Rust currently exposes split
  byte-preserving `Entry::raw_fields()`, `Entry::get_raw()`, and
  `Entry::get_raw_values()` methods; Go reader alignment
  SOWs must expose equivalent idiomatic byte-name surfaces before claiming RAW
  reader parity.
- JSON output, field enumeration, unique queries, and `get_data`-style facade
  helpers are UTF-8 field-name surfaces. Byte-exact RAW names are available
  through full payload/data enumeration and idiomatic byte-name APIs.
- Field-name enumeration is a journal-index operation on valid indexed files.
  Readers should walk FIELD hash tables instead of expanding every entry. A
  compatibility fallback may scan entries only when a historical or damaged
  FIELD table cannot be traversed safely.
- Unfiltered unique value enumeration is a journal-index operation, not an
  entry-scan operation. Readers must find the requested FIELD object, walk that
  FIELD object's DATA chain, decode only matching DATA payloads, and de-duplicate
  across files. This matches systemd's `sd_journal_query_unique()` /
  `sd_journal_enumerate_unique()` algorithmic contract and avoids expanding
  unrelated entries or fields. Directory readers keep an exact 8-entry
  per-open-reader LRU cache of directory-wide unique payloads per requested
  field and current already-open file header signatures, so repeated directory
  unique queries and stateful restarts reuse the built index while invalidating
  when opened-file journal counters or tail metadata change. The cache entry
  count is bounded, but each entry keeps the full exact unique set for one
  field.
- Performance-sensitive readers should use the raw current-entry payload
  visitor/enumeration APIs when they already need byte-level `FIELD=value`
  payloads. Convenience entry materialization APIs may build maps, repeated
  value maps, owned payload vectors, and cursor strings and are not the
  primary hot path.
- Rust and Go expose optimized single-file log-explorer query
  surfaces for exact indexed filters, selected facet counters, optional
  histogram, optional FTS, and optional returned rows. Rust exposes
  `FileReader::explore()`; Go exposes `Reader.Explore()`. Both
  use native filter indexes for exact slicing, lazy candidate-row
  DATA-offset classification caches to avoid reprocessing reusable
  `FIELD=value` objects within each traversal pass, and owned cached value
  labels for required DATA that must be returned in facet, histogram, FTS, or
  row results.
  Facets with the same effective filter set are grouped into one traversal
  pass. `ExplorerAnchor::Auto` is the default scan-start policy, using the
  lower time bound or head for forward queries and the upper time bound or tail
  for backward queries. `ExplorerFieldMode::FirstValue` is the default explorer
  accounting mode: one selected facet/histogram/source field contributes at
  most one value per row, so traversal may stop after all required fields are
  found and avoid unrelated trailing DATA, including compressed DATA.
  Column catalogs must come from FIELD indexes, not row traversal. The
  debug-only `ExplorerQuery::debug_collect_column_fields_by_row_traversal`
  marker is rejected by production explorer entrypoints; any benchmark or
  compatibility claim that depends on it is invalid.
  `ExplorerFieldMode::AllValues` is an explicit slower mode for exact
  duplicate-value accounting and scans the whole row for repeated-field
  correctness.
- Rust `journal-engine` file-index caches are versioned by SDK-owned
  `CACHE_VERSION` and by an optional consumer namespace. Any change to what the
  indexer includes, excludes, computes, or serializes must bump
  `CACHE_VERSION` in the same change. Consumers can use
  `FileIndexCacheBuilder::with_cache_namespace()` plus
  `FileIndexCacheBuilder::file_index_key()` or
  `FileIndexKey::new_with_namespace()` to force a clean rebuild for
  consumer-side semantic migrations without changing the physical cache
  directory. Setting the builder namespace does not rewrite keys constructed
  elsewhere with `FileIndexKey::new()`.
- Rust and Go also expose explicit Explorer execution
  strategy controls through their idiomatic APIs. `ExplorerStrategy::Traversal`
  is the default and remains the behavior of each product language's ordinary
  Explorer query method.
  `ExplorerStrategy::Index` walks FIELD/DATA chains and DATA entry posting
  lists to derive facet and histogram counts without candidate-row field
  traversal, but it is intentionally limited to exact `AllValues` accounting,
  commit-realtime time semantics, and no FTS. It rejects default
  `FirstValue`, source-realtime-bounded, and FTS queries instead of returning
  approximate results. `ExplorerStrategy::Compare` runs traversal and index,
  fails if the logical row/facet/histogram output differs, and returns
  traversal/index timing and counter diagnostics in the result. No automatic
  planner is enabled; SOW-0083 showed index aggregation is a large win for narrow
  unfiltered all-values queries and histogram-only queries, but slower for many
  facets and can be catastrophically slower for selective filters.
- The libsystemd-compatible facade is available in Rust and Go for file-backed
  use. It includes open file, open directory, open files,
  close, seek head/tail/realtime/cursor, next/previous/skip, add match,
  add conjunction/disjunction, flush matches, get entry, get data, restart and
  enumerate current-entry data, enumerate fields, direct unique queries as
  language-native `(field, raw value)` pairs, stateful unique enumeration as
  `FIELD=value` payloads, get realtime, get monotonic/boot metadata, get
  seqnum, get cursor, test cursor, output formatting, and boot listing.
- `seek_cursor()` follows libsystemd's no-existence-proof contract: a
  syntactically valid cursor is accepted as a seek location even when no current
  entry has that exact cursor. Invalid cursor syntax fails. `test_cursor()`
  remains the exact-current-position check.
- `get_cursor()`, JSON output, export output, and file-backed `journalctl`
  cursor files emit official systemd cursor strings with `s=`, `i=`, `b=`,
  `m=`, `t=`, and `x=` fields. `seek_cursor()` and `test_cursor()` accept both
  official systemd cursor strings and the older SDK cursor shape with `s=`,
  `j=`, `c=`, and `n=` fields for compatibility with pre-existing SDK callers.
- Current-entry data enumeration and query-unique stateful enumeration are
  binary-safe and preserve repeated values. `GetData` returns the first value
  for a repeated field; callers that need every repeated value use
  restart/enumerate data.
- Rust current-entry facade data enumeration returns borrowed `FIELD=value`
  bytes for the current DATA object with a stronger row-scoped lifetime than
  stock libsystemd documents. Payload slices returned while enumerating the
  current row remain valid until the reader advances to another row, seeks,
  closes, restarts/releases current-entry DATA state, or remaps the backing
  file. The end-of-data result for the current row does not release those
  slices, so consumers may cache field pointers during enumeration and process
  them after the inner data loop finishes. Rust returns uncompressed DATA
  directly from stable mmap-backed journal payloads when that path preserves the
  row-scoped lifetime, stores compressed DATA in row-scoped owned buffers, and
  uses row-scoped owned buffers when a windowed mapping could invalidate a
  borrowed pointer. Go exposes the same row-scoped facade
  contract through its idiomatic borrowed or copy-on-iteration forms: Go returns
  mmap/read-at slices or fresh decompressed slices. Callback-style visitor APIs
  remain callback-scoped.
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
- Rust and Go verification APIs and file-backed
  `journalctl --verify` perform raw object-graph verification for the supported
  feature slices before normal reader traversal. File-path verification uses
  bounded reader-backed byte sources; it may allocate per-object and
  per-decompressed-payload scratch memory, but it must not load the whole
  journal file into a resident byte buffer. The shared parity matrix
  `tests/interoperability/run_verify_matrix.py` validates stock systemd and all
  repository verifiers against positive regular, zstd/xz/lz4 DATA-compressed,
  compact, compact plus DATA-compressed, and sealed files, plus negative object
  type, object size, DATA/FIELD payload hash, DATA hash-table membership,
  entry-array ordering, header counter, missing main entry-array, entry seqnum,
  tail seqnum, tail monotonic, and TAG/FSS HMAC corruption classes.
- Daemon-only libsystemd/journalctl operations remain outside the SDK facade
  target and must fail with controlled unsupported behavior when exposed.
- Rust and Go readers accept historical unkeyed-hash journal
  files, including systemd 239-era LZ4-compressed DATA files with
  `header_size=240`. Core reader traversal exposes the current-systemd/file
  format entry set, not old systemd 239 same-file duplicate suppression in its
  CLI traversal.
- Rust and Go writers create keyed-hash journal files for
  the supported writer slice. Append-open on historical unkeyed-hash files is
  unsupported and must fail with a controlled error before entry mutation.

Current Go reader feature slice:

- regular and compact journal files;
- files named `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst`;
- bounded rolling reader access with `ReaderAccessAuto` as the default.
  `Auto` selects rolling mmap on supported Unix-family and Windows targets,
  while explicit `ReadAt` uses bounded positioned-read windows for tests,
  diagnostics, constrained-platform investigation, and controlled fallback
  evidence only. Access stats expose the selected backend, fallback reason,
  window budget, mapped/read-buffer bytes, row-arena peak, and refresh counters;
- whole-file zstd fixtures and zstd, xz, and lz4-compressed DATA objects
  through pure-Go dependencies;
- historical unkeyed-hash journal reading, including LZ4-compressed DATA
  objects, with core reader traversal exposing the file-format entry set rather
  than old systemd 239 same-file duplicate suppression behavior;
- directory iteration across active and archived files with stock-compatible
  root plus one machine-id subdirectory traversal and interleaved multi-file
  ordering;
- forward/backward iteration, cursors, realtime and monotonic timestamps,
  seqnum metadata, binary field values, repeated field values, field
  enumeration, current-entry data enumeration, and unique value enumeration;
- systemd-compatible export output for binary fields using size-prefixed field
  values and blank-line entry separators;
- systemd-compatible JSON output for duplicate fields and binary values;
- file-backed journalctl output modes cover the stock v260.1 family:
  `short`, `short-full`, `short-iso`, `short-iso-precise`,
  `short-precise`, `short-monotonic`, `short-delta`, `short-unix`,
  `verbose`, `export`, `json`, `json-pretty`, `json-sse`, `json-seq`,
  `cat`, and `with-unit`; `--output=help` prints the official mode list
  without opening a journal;
- short-style output renders stock-compatible hostname, identifier/unit, and
  PID labels where the corresponding journal fields are present, and
  `--no-hostname` suppresses the hostname component;
- `--output-fields` is supported for `verbose`, `export`, JSON modes, and
  `cat`; JSON/export preserve stock metadata fields such as cursor, realtime,
  monotonic, seqnum, seqnum ID, and boot ID;
- output controls preserve stock v260.1 behavior: full-width text output is the
  default, `--no-full` enables stock text ellipsization and blob suppression,
  `--all` exposes long/non-printable text where stock does, JSON applies the
  stock non-`--all` large-field `null` threshold, and empty default/verbose
  results print `-- No entries --` unless quiet or an auto-quiet output mode is
  active;
- libsystemd-style match tree behavior from `sd_journal_add_match()`,
  `sd_journal_add_disjunction()`, and `sd_journal_add_conjunction()`;
- file-backed Go journalctl source selection supports `--file`/`-i`,
  repeated `--file` inputs, stock-style glob expansion with no-match
  preservation, and `--directory`; `--file=-` is explicitly unsupported
  because portable stdin journals would require seekable mmap-capable file
  descriptors;
- file-backed Go journalctl behavior for `--file`, `--directory`,
  text/json/export output, field listing, `--list-boots` stock table output
  with `--lines` and `--reverse`, realtime range filtering with
  `--since`/`--until`, boot filtering with `--boot`, follow mode with
  `--follow`, cursor seek/update behavior, repeated same-field OR
  matches, `+` disjunction, syslog identifier and short-output exclude
  identifier filtering, priority, facility, grep including stock reverse
  implication for tail-style `--lines`, dmesg, system/user unit filters
  including stock `--user --unit=` rewrite, invocation filters, `--list-invocations`,
  `--header`, `--pager-end` implicit 1000-line tail behavior, explicit-input
  `--disk-usage`, and explicit-directory `--vacuum-size`/`--vacuum-files`/
  `--vacuum-time`;
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
- low-level `EntryWriteOptions::seqnum(...)` and
  `EntryWriteOptions::boot_id(...)` can preserve original ENTRY sequence
  numbers and per-entry boot IDs during exact journal regeneration. Normal
  writers leave them unset for auto-incrementing sequence numbers and the
  writer-wide boot ID. Sequence overrides must move forward from the writer's
  next sequence number; gaps are allowed and rewinds are rejected;
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
- optional pure cross-SDK cooperative lockfile with stale-owner detection when
  callers explicitly acquire `journal_core::file::lock::WriterLock`;
- Forward Secure Sealing TAG writing with configurable deterministic test
  options and stock `journalctl --verify --verify-key` validation for generated
  sealed files. Rust normalizes FSS start timestamps to systemd's
  verification-key epoch boundary: `floor(start / interval) * interval`;
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
- `ReaderOptions` exposes live/snapshot bounds and window size for public
  consumers. Default Rust live readers remain windowed with a 32 MiB window so
  indexed DATA-chain traversal does not remap per small object. Whole-file mmap
  remains an internal/test/benchmark hook only and is not a production or
  normal consumer option because it increases virtual-memory pressure on large
  active files;
- raw current-entry payload visitors on file and directory readers for
  allocation-light scans that operate on borrowed `FIELD=value` bytes;
- single-file Rust optimized explorer API for exact indexed filters, selected
  facets, optional histogram, optional FTS, optional Top-N rows, query counters,
  per-pass DATA classification caching, unrelated-compressed-DATA skipping,
  and repeated-field mode selection;
- byte-preserving RAW field-name representation through `Entry::raw_fields()`,
  `Entry::get_raw()`, and `Entry::get_raw_values()`. `Entry.fields` and
  `Entry.field_values` remain UTF-8 string-keyed convenience maps and do not
  synthesize lossy names for non-UTF8 RAW field names;
- export byte output preserves non-UTF8 RAW field names; JSON output, field
  enumeration, unique queries, and `get_data` facade helpers remain UTF-8
  field-name surfaces;
- file-backed journalctl output modes cover the stock v260.1 family:
  `short`, `short-full`, `short-iso`, `short-iso-precise`,
  `short-precise`, `short-monotonic`, `short-delta`, `short-unix`,
  `verbose`, `export`, `json`, `json-pretty`, `json-sse`, `json-seq`,
  `cat`, and `with-unit`; `--output=help` prints the official mode list
  without opening a journal;
- short-style output renders stock-compatible hostname, identifier/unit, and
  PID labels where the corresponding journal fields are present, and
  `--no-hostname` suppresses the hostname component;
- `--output-fields` is supported for `verbose`, `export`, JSON modes, and
  `cat`; JSON/export preserve stock metadata fields such as cursor, realtime,
  monotonic, seqnum, seqnum ID, and boot ID;
- output controls preserve stock v260.1 behavior: full-width text output is the
  default, `--no-full` enables stock text ellipsization and blob suppression,
  `--all` exposes long/non-printable text where stock does, JSON applies the
  stock non-`--all` large-field `null` threshold, and empty default/verbose
  results print `-- No entries --` unless quiet or an auto-quiet output mode is
  active;
- libsystemd-style match tree behavior from `SdJournalAddMatch()`,
  `SdJournalAddDisjunction()`, and `SdJournalAddConjunction()`;
- file-backed Rust journalctl source selection supports `--file`/`-i`,
  repeated `--file` inputs, stock-style glob expansion with no-match
  preservation, and `--directory`; `--file=-` is explicitly unsupported
  because portable stdin journals would require seekable mmap-capable file
  descriptors;
- file-backed Rust journalctl behavior for `--file`, `--directory`,
  text/json/export output, field listing, `--list-boots` stock table output
  with `--lines` and `--reverse`, realtime range filtering with
  `--since`/`--until`, boot filtering with `--boot`, follow mode with
  `--follow`, cursor seek/update behavior, repeated same-field OR
  matches, `+` disjunction, syslog identifier and short-output exclude
  identifier filtering, priority, facility, grep including stock reverse
  implication for tail-style `--lines`, dmesg, system/user unit filters
  including stock `--user --unit=` rewrite, invocation filters, `--list-invocations`,
  `--header`, `--pager-end` implicit 1000-line tail behavior, explicit-input
  `--disk-usage`, and explicit-directory `--vacuum-size`/`--vacuum-files`/
  `--vacuum-time`;
- Rust conformance adapter support for reader, matching, importer, compression,
  cursor, enumeration, stream, export, header parsing, and file-backed
  journalctl cases.
- Current SOW-0044 regression benchmark evidence on a 100k-row compact fixture
  with 32 fields per row shows Rust single-file `sdk-payloads` live/windowed at
  about 1.34M rows/s and snapshot/windowed at about 1.36M rows/s versus stock
  libsystemd data enumeration at about 660k rows/s. The fixed live mode uses
  6 `statx` calls in the profiled hot-path run instead of the previous
  7,600,032-call refresh-every-slice behavior.
- Current SOW-0057 measurement evidence on a 100k-row compact/offline fixture
  shows Rust single-file `sdk-payloads` live/windowed at about 2.52M rows/s and
  live/whole-file at about 2.52M rows/s; live whole-file mmap did not explain
  the Go/Rust payload-reader gap on that corpus.

Current Rust reader limitations:

- boot listing APIs use file-level boot metadata in this slice; file-backed
  `--boot` filtering scans entry `_BOOT_ID` values;
- daemon-only journalctl operations remain unsupported.

## journalctl Target

Implement journalctl rewrites in Rust and Go for file-backed/query behavior.

Matching semantics:

- Different fields are ANDed.
- Repeated matches for the same field are OR alternatives.
- The `+` separator creates explicit disjunction groups and must be replicated for file-backed journalctl behavior.
- No new `KEY in [values]` syntax is required.

File-backed query semantics:

- `--since` and `--until` apply inclusive realtime timestamp boundaries.
- `--boot` supports `all`, the latest boot by default for bare `--boot`,
  numeric offsets, boot UUIDs, and boot UUID plus signed offsets for files and
  directories whose entries contain `_BOOT_ID`. Explicit empty `--boot=` is a
  parse error, matching stock `journalctl`.
- `--follow` follows repository-supported file and directory inputs by polling
  file-backed readers and emitting newly appended entries in cursor order.
  This is the portable non-daemon behavior; it may cost more than
  daemon-backed Linux journal wait APIs on very large inputs.
- `--pager-end` does not spawn a pager in portable mode, but it preserves stock
  line-selection semantics by applying an implicit 1000-entry tail when no
  explicit `--lines`, `--head`, or `--tail` option is present.
- Bare `--lines` and `-n` select the stock default 10-entry tail. Explicit
  empty `--lines=` is a parse error, matching stock `journalctl`.
- Empty default/verbose show results print `-- No entries --`; `--quiet` and
  auto-quiet modes such as JSON, export, and cat suppress that line.
- `--cursor`, `--after-cursor`, `--cursor-file`, and `--show-cursor` use
  official systemd cursor strings for file-backed reads and cursor-file
  updates.
- `--list-invocations` is correctness-compatible for covered file-backed unit
  contexts, but the current implementation derives invocation ranges from
  matching entries. SOW-0122 tracks the remaining performance work before any
  large-archive performance claim.
- Recognized options may appear before or after show-action match arguments,
  matching stock argument permutation. Short options accept stock attached
  values and clusters such as `-n2`, `-ball`, and `-rn2`, while stock-invalid
  forms such as `-n=2` and `-b=true` remain invalid.
- `--identifier`, `--exclude-identifier`, `--priority`, `--facility`,
  `--grep`, `--case-sensitive`, `--dmesg`, `--unit`, and `--user-unit` are
  file-backed filters. `--exclude-identifier` follows stock v260.1 output-mode
  scope: short-family outputs filter matching `SYSLOG_IDENTIFIER=` entries,
  while JSON, export, verbose, and cat remain unchanged. `--grep` with
  tail-style `--lines=N` implies reverse traversal unless `--follow` is set.
  Unit filters use the same systemd v260.1 unit-related fields, disjunction
  groups, unit suffix mangling, glob expansion over FIELD/DATA indexes, and
  `--user --unit=` rewrite as stock file-backed `journalctl`.
- `--disk-usage` reports allocated filesystem usage for explicit
  `--file`/`--directory` journal inputs. Without explicit input, it remains an
  unsupported host-journal discovery action.
- `--vacuum-size`, `--vacuum-files`, and `--vacuum-time` mutate only explicit
  `--directory` inputs. They delete stock-recognized archived `.journal` and
  `.journal~` files from that directory while protecting active/current,
  non-matching, unknown, and subdirectory files. `--vacuum-files` uses the
  stock count condition: protected active/current files plus remaining archived
  candidates must be at or below the requested count.
- `--new-id128` is a portable standalone utility action and prints a new v4
  ID128 in the same string, UUID, `SD_ID128_MAKE()`, and Python constant forms
  as stock systemd v260.1.
- Positional `FIELD=value`, path, and `+` disjunction arguments belong to the
  show action. Non-show actions other than catalog listing/dumping reject
  extra positional arguments with the stock `Extraneous arguments starting with
  '<token>'` parser error before portable unsupported dispatch.
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
