# Product Scope Specification

## Purpose

This project produces pure SDKs and file-backed journalctl-compatible tools for
systemd journal files in Rust and Go.

## Language Targets

- Rust 1.91 or newer
- Go 1.26.2 or newer

## Rust Registry Packages

The Rust SDK's public crates.io package is `systemd-journal-sdk`. Consumers may
alias it as `journal` in Cargo dependencies to keep the existing
`journal::...` source path:

```toml
[dependencies]
journal = { package = "systemd-journal-sdk", version = "0.10.0" }
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

## Go Module Release

Go consumers install `github.com/netdata/systemd-journal-sdk/go@v0.10.0`.
The paired Rust/Go `0.10.0` release adds opt-in dedicated root history
retention and stricter stable-header validation for append recovery and
excluded-writer snapshots. Root `v0.10.0` and submodule
`go/v0.10.0` tags must identify the same release source commit, and Rust
registry packages `0.10.0` are published from that commit. The previous paired
`0.9.0` release added the IndexedSnapshot surface and strict offline index
verification.

Earlier staggered release: Go `0.8.2` added `Log.CloseWithoutRetention()`; Rust
registry packages advanced separately to `0.8.2` from a later merged commit, and
the root and Go `v0.8.2` tags remain at the earlier Go release commit.

## Consumer Documentation

Committed consumer documentation lives under `docs/` as GitHub wiki source.
The docs explain package selection, reader and writer API layers, Explorer and
Netdata-shaped query APIs, hot-path behavior, production profiles, and options
that can make a consumer accidentally leave the optimized path. The repository
publishes these pages to the GitHub wiki on trusted `master` pushes through the
wiki publication workflow.

## Paired CI Releases

The manual `Release` workflow runs on canonical `master` and accepts a stable
version plus a full merged source SHA. Its own code stays at the dispatch's
master SHA; the selected release source is checked out separately. PRs and
ordinary pushes run helper tests only.

Both language suites use their declared compiler minimums before publication.
The eight existing public Rust crates are dry-run and published in dependency
order through Trusted Publishing in the `release` environment, configured for
the master branch only. Every crate is checked against API/index metadata,
archive checksum and clean Cargo VCS source. Existing versions are skipped
only after these checks, supporting partial-publication recovery.

After all Rust packages and an exact-version registry consumer pass, CI
reports the exact version/source to the maintainer. An authorized maintainer
uses existing GitHub access to create annotated `vVERSION` and `go/vVERSION`
at that selected source and pushes both new tags atomically. The operator
verification helper checks both canonical targets, matching Rust archive
sources and an exact-version Go
consumer using the selected commit's module/compiler declaration. Correct
existing tags are preserved; conflicting tags or crate sources stop the
release. CI uses Trusted Publishing for crates.io and requires no additional
GitHub App or account token for tags. Multiple authorized maintainers can run
CI and perform the documented tag handoff.

Repository/crate settings are configured once by their owners; `RELEASING.md`
contains the setup and recovery procedure. Workflow implementation completion
does not claim that a particular version has been published.

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
- Go SDK implementations must not use CGO. Native PCRE2-8 >=10.42 is permitted
  only in the standalone journalctl command, which requires CGO and a C compiler.
  Rust also confines native PCRE2 and its timezone provider to that command.
- Both commands may use the platform C library for Unix file globbing,
  temporary thread-local locales and terminal queries. Native Unix filename
  bytes survive explicit files, directory discovery and facade opening.
  File patterns use native BRACE/NOCHECK/NOSORT semantics in the environment
  locale; literal paths bypass expansion. Windows filenames are Unicode,
  with brace alternatives and Unicode wildcard matching.
- Rust facade constructors have additive native-path forms: SdJournalOpenPath,
  SdJournalOpenFilePath, SdJournalOpenDirectoryPath and SdJournalOpenFilePaths,
  including file/directory/list WithOptions forms. Existing string functions
  retain their signatures and delegate to the same readers. Journal and
  whole-file Zstandard suffix checks preserve native filename bytes.
  Registry File::as_path supplies the original filesystem path. File::path
  retains ordinary UTF8 text; newly supported non-UTF8 paths return a reserved
  NUL-prefixed hexadecimal Unix key, which must not be used for filesystem I/O.
  Metadata deserialization reconstructs those native paths. CACHE_VERSION stays
  at 4: ordinary serialization and Hash inputs are unchanged, and new raw-path
  keys cannot alias ordinary paths. Per-file metadata adds one optional pointer;
  no per-row storage or index interpretation changes.
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

Go `LogConfig.RootRetention` and Rust `Config::with_root_retention(true)` provide
equivalent caller-owned root/source retention across machine identities. This
opt-in requires strict active naming,
caller writer exclusion, verified recovery provenance/indexes and no artifact
sizer. `InspectRootRetention` reads fixed-size headers, validates canonical
machine/sequence/archive identities, header extents and internal population/pair
consistency, and fails the complete inventory for unsafe source candidates. Missing roots are errors; unrelated
caller metadata directories are allowed when readable to exclude source
candidates, but canonical machine-named entries must be directories. After open,
`Log.InspectRootRetention` additionally proves
the active path still names the live writer file and journal identities; it is
available after writer failure and is reused by maintenance before mutations.
Root disposal, closing and rotation also preflight the held file's pathname and
header identity before mutation. Archive and lazy-creation destinations must be
absent; a safe rejection preserves file contents and names. Creation/rotation
preflight rejections leave ownership healthy for retry after restoring paths;
explicit close releases resources without writing after preflight rejection. Empty root
writers accept their first entry before size/duration threshold rotation, including
after a policy update. Default-mode and low-level creation contracts are unchanged.
An inventory mismatch alone does not poison the writer. These boundary checks
assume caller exclusion and do not promise protection against concurrent
external modification. Ordinary nonrotating appends add no filesystem checks.
Inventory is not full verification. Counts of disjoint present object categories
must fit within the total object count, including one object for each declared
DATA_HASH_TABLE and FIELD_HASH_TABLE. Absent historical counters are ignored;
the table declarations are present in every supported header version. When the
ENTRY_ARRAY counter is present, it must cover the distinct nonzero first/cached-tail
array locations declared by complete header fields. Equal pointers count once;
additional arrays, including DATA-associated arrays, are permitted.
Stable-header population checks also apply to append recovery and excluded-writer
snapshots, and remain separate from ordinary live-reader mapping validation.
Root retention counts file lengths including preallocation, expires by tail
saved time, and evicts whole files in tail/path order. Default retention policies
and file format are unchanged; Go now applies cleanup after a successful
successor-creation retry following a safe creation failure.
`MaintainRootRetention` finalizes verified retired active
files and only seals live actives for idle expiry or required size-policy
allocation changes, leaving successors lazy even after eager startup. Startup,
creation, rotation, closing a nonempty active and explicit maintenance enforce
the policy. Closing an unopened or empty Log does not sweep history. New SDK files
allocate at least 8 MiB; existing files are counted at their actual length and
smaller allowances are accepted; finalized files have no newest-file grace period and
may be deleted immediately under a smaller allowance. Close without retention
finalizes without pruning. Positive ages below one microsecond normalize to
one microsecond. Safe pre-mutation open/validation and cleanup errors remain in
`RootRetentionResult` (attempt/last-success times and valid-or-unknown sample)
without failing healthy appends; uncertain archive mutation still fails the
writer. `SetRootRetentionPolicy` validates/copies/installs before enforcement;
explicit rotation limits survive changes. This is neither exact TTL nor a
physical disk cap; preallocation/active growth/pinned readers and failed cleanup
can exceed the allowance. Rust exposes equivalent snake_case methods, Copy policy
values, SystemTime timestamps, and retained typed errors through Arc. The public
Rust SDK reexports inventory/result APIs. Both SDKs emit an Archived event after
successful finalization without a successor and before deletion, including
retired files and close; ordinary Rotated events remain unchanged. Go result
inventories do not alias retained status. New root files and allocation checks
use current configuration, including compact layout, while recovered files
retain their on-disk layout until finalized.
Rust root configuration rejects namespaces and artifact sizing; adding an artifact
sizer with the infallible builder causes mutation validation errors and preserves
files on close/drop. Root functionality ships in both languages; publication
remains a separate release operation.

Current Go writer feature slice:

- regular journal files by default and compact journal files when
  `Options.Compact` is enabled;
- uncompressed DATA objects by default;
- optional zstd, xz, and lz4-compressed DATA object writing with configurable
  compression threshold using the shared systemd threshold policy, including
  zstd frame content-size metadata retained for the v260.1 writer compatibility
  target and readback of large compressed payloads;
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
  `EnforceRetention()` applies retention without requiring a rotation or close.
  Go `CloseWithoutRetention()`, available since `v0.8.2`, archives/releases the
  current file with the normal durability path but skips retention, allowing a
  caller to reopen with a changed
  policy without applying stale limits. The caller owns subsequent enforcement;
  normal `Close()` applies its policy on the archive path, except for an unopened
  lazy log or an empty strict-named active file. Both methods are idempotent;
  empty active files are removed with strict systemd naming and archived with
  chain naming;
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
    default; boot ID `/proc/sys/kernel/random/boot_id`; monotonic
    `clock_gettime(CLOCK_MONOTONIC)`. When callers explicitly configure a host
    filesystem prefix, the helper checks `<prefix>/etc/machine-id` and
    `<prefix>/var/lib/dbus/machine-id` before the container-local paths, and
    checks `<prefix>/proc/sys/kernel/random/boot_id` before the
    container-local boot-id path, so containerized collectors can intentionally
    use host machine and boot identity. Missing host-prefixed identity files
    fall back to container-local paths; present but invalid host-prefixed files
    fail instead of silently switching identity. Parsed machine IDs must be
    non-zero.
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

Traversed hash, FIELD DATA and ENTRY-array chains must terminate on cycles with
a controlled error. Ordinary indexed reads retain their native traversal paths
and accepted acyclic link orders; they do not run a full integrity scan at open.
A detected cycle must not trigger the field-catalog compatibility row fallback.
Counted ENTRY-array traversal rejects a required array revisit before consuming
duplicate slots, including a revisit after an acyclic backward link. Reader
links beyond the declared population are not traversed or validated. Writer
tail selection rejects required revisits before modifying a repeated array.

Zstandard DATA and whole-file staging consume complete supported streams,
including unknown-size, concatenated and skippable frames. Invalid checksums,
truncated later frames and trailing garbage are errors, not successful decoded
prefixes. The 768 MiB DATA output limit applies to the aggregate logical DATA
payload. Whole-file staging is streaming and does not apply that per-DATA limit
to an entire journal. Known-size single-frame DATA retains its native fast path.

Ordinary Rust/Go SDK file, directory and facade readers recover the intact entry
prefix by default after observed unavailable ENTRY/ENTRY-array tails. Iteration
ends with normal EOF; per-file copyable recovery status records the reason,
associated loss offset, declared count and surviving count. Explicit collection
diagnostics identify files and allocate on request. Raw header counters remain
declared. A zero status reports no observed loss, not an integrity certificate;
native catalog access does not preflight unrelated arrays. Recovery changes
in-memory reader state only; it never reconstructs source journal bytes.
Ordinary writer publication may make temporarily unavailable data readable.
Arbitrary external restoration of damaged entries is not a supported automatic
refresh contract and must not justify added runtime overhead. Synthetic
damage/restore tests establish fault-injection behavior, not journald parity.
Recovered reader EOF polling memoizes direction and declared entry count, so
unchanged terminal polls do not rediscover the prefix. Successful movement,
explicit positioning/filter changes and changed declared counts invalidate that
terminal observation. Go explicit Refresh remains an independent refresh request.
Rust applies this policy in JournalReader without changing the public
JournalCursor layout; the low-level cursor handles positioning and initial
error recovery, not automatic EOF refresh.

Recovery retains native index/header access and does not expand DATA rows to
establish the prefix. Interior holes followed by readable entries, wrong types,
cycles, malformed nonzero sizes, decompression failures and general I/O errors
remain errors. Unknown secondary posting tails that could contain surviving
matches remain errors. Strict verification, excluded-writer IndexedSnapshot
capture, append-open and retention preconditions remain separate and strict.

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
  Internal indexed facet/histogram aggregation must also preserve DATA values
  across nested posting and ENTRY reads, including mmap eviction and positioned
  read scratch reuse. Aggregation map keys are owned. Go materializes map keys
  for contributing values and reuses a histogram key across the DATA object's
  matching entries without repeated decompression. Indexed results and counters
  must match across access bounds.
  Go ENTRY-index header validation uses fitting native cache views and a fixed
  reader-owned buffer for cached crossing/non-power-of-two-window reads. Header
  acquisition uses constant per-reader storage instead of a fresh header buffer
  for each offset; cache-window metadata follows the existing bounded-accessor
  contract. Header bytes are consumed before any subsequent accessor read.
  Native Go journal input on Linux uses a blocking read-only descriptor with
  atomic close-on-exec, interrupted-open retry, typed path errors and normal
  symlink following. This avoids unnecessary regular-file poller setup while
  preserving sized random access. Whole-file compressed sources and non-Linux
  readers retain their existing platform opening paths.
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
  Unreleased source builds use generation 4 after native-array completeness
  repairs. Generation-3 entries miss: valid files rebuild on demand once after
  upgrade. Encountered required native-chain or posting failures fail indexing;
  existing per-object value tolerance remains. Old storage follows normal
  cache eviction. Cache-hit algorithms and consumer namespaces are unchanged.
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
- File and directory readers expose explicit eager directional cursor seeking:
  Go `SeekCursorDirection`, Rust `seek_cursor_direction`, and facade extension
  `SdJournalSeekCursorDirection`. Existing cursor seek methods remain unchanged.
  Forward/backward selects the inclusive first/last eligible boundary using
  native per-file sequence, boot/monotonic and realtime key precedence, with
  DATA-index filtering and directory merge ordering. Getters access the selected
  row immediately; no eligible row succeeds with no current entry. Exactness
  remains a separate cursor test. IDs normalize to 32 hexadecimal digits.
  This extension adds no persistent reader fields or changes to Netdata paths.
- Go direct `Reader.TestCursor` and `DirectoryReader.TestCursor` return
  `false, ErrEndOfEntries` at EOF. The libsystemd-compatible facade returns
  `false, nil` instead; other errors remain errors. The Go facade cursor getter
  returns public `ErrNoEntry` when no current entry exists, matching Rust
  facade `Error::NoEntry`; other getter failures retain their original errors.
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
- complete whole-file zstd streams and zstd, xz, and lz4-compressed DATA objects
  through pure-Go dependencies, including the stream variants above;
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
- file-backed journalctl output modes cover the stock v262 family:
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

- `Log::close_without_retention(self)`, available in Rust `0.8.2`, consumes
  the writer and uses the existing archive/sync/resource-release path while
  skipping retention. Normal `close(self)` still applies its policy when
  archiving, except for an empty strict-named file or unopened lazy writer.
  Both methods keep chain-named empty archives and discard strict-named empty
  active files. Subsequent writers own retention under the caller's new policy;
- regular journal files by default and compact journal files when
  `JournalFileOptions::with_compact(true)` or `journal::Config::with_compact(true)`
  is enabled;
- uncompressed DATA objects by default;
- optional zstd, xz, and lz4-compressed DATA object writing with configurable
  compression threshold, including Rust zstd frame content-size metadata
  retained for the v260.1 writer compatibility target, using the shared systemd
  threshold policy;
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
- complete whole-file zstd streams and zstd, lz4, and xz-compressed DATA objects
  through the existing compression dependencies, including the stream variants
  above;
- directory iteration across active and archived files with stock-compatible
  root plus one machine-id subdirectory traversal and interleaved multi-file
  ordering;
- forward/backward iteration, cursors, realtime and monotonic timestamps,
  seqnum metadata, binary field values, repeated field values, field
  enumeration, current-entry data enumeration, unique value enumeration, and
  systemd-compatible export/json/text formatting;
- configurable reader bounds through `ReaderOptions`: default `Live` mode uses
  systemd-style cached mutable bounds and refreshes file size only when a read
  would exceed the cached end of file, while `Snapshot` mode caps reads at the
  original open extent for polling/query use cases that do not need to observe
  appends during the current scan. Cold recovery can observe physical shrink
  without increasing that original cap;
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
- file-backed journalctl output modes cover the stock v262 family:
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
- unreleased exact SDK/facade filters resolve native DATA hash/posting indexes
  in both languages, preserving same-field OR and different-field AND grouping
  without expanding unrelated rows. Go `Step`/`StepBack` apply filters; its raw
  `Next`/`Previous` movement helpers do not. File/directory/facade group operators return
  errors, and an empty pending Go filter builder returns `ErrMalformedFilter`.
  Required unknown secondary posting tails remain errors. Healthy installed
  posting populations stay fixed until rebuilding; read-only recovery can clip
  existing DATA identities to the intact prefix, while absent DATA stays
  absent until rebuilding. Failed group/build operations retain pending matches
  for retry. Recovery requalification preserves known DATA identities and retries
  the whole Boolean view after partial failure. Clearing matches rewinds file,
  directory and facade readers to the head; callers restore saved locations
  explicitly when continuation is required. Filtered movement returns the
  nearest preceding/following match relative to the last successful row,
  including filter installation/replacement and direction changes after EOF.
  Directory mutations preserve the committed key and discard prefetched
  candidates. Proven sequential positions retain sequential traversal rather
  than probing every file; reversals and merged/recovered states use key-based
  positioning. Failed selected ENTRY loading retains
  the previous successful position on failure. Explicit filtered
  realtime seeks search native posting nodes and binary-search their captured
  slots, selecting the first match at/after the requested time forward and the
  last at/before it backward. Errors from visited slots/headers propagate;
  skipped postings are not certified. Ordinary sequential posting traversal
  retains its existing error-discovery behavior;
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

The command contract targets `systemd/systemd` tag `v262`, commit
8cc40e0c5e9234bf45084751ac53b1fbfe70b492. SDK journal-format compatibility
continues to target v260.1. The complete command option/action classifications
are in `journalctl-v260-parity-matrix.md` (historical filename retained).

Both commands parse setters in encounter order, preserve required values and
explicit-empty semantics, and stop at terminal help/version/introspection.
Opaque Unix match, identifier, exclusion and projection arguments preserve
raw bytes. Non-UTF8 projection names retain explicit projection presence and
rendering-time cat validation; they do not expand to unrestricted output.
The final action controls dispatch and conflicts. PCRE2 compiles the final raw
pattern once after global parser checks; matching uses the first raw MESSAGE,
native byte/case behavior and native errors. A show command with no grep-selected
entries exits 1 without a diagnostic. Rendering exclusions do not erase a
successful grep selection.

The native CLI dependencies have an accepted small fixed startup cost, measured
on the tiny Linux control as Go about 3.0ms to 3.8ms and Rust 1.5ms to 1.8ms.
This allowance is confined to command startup; SDK and sustained query
performance retain their no-regression requirement. Static native builds are
not a separate supported packaging profile.

With final `-F`/`-N`, entry/time/cursor/grep/explicit boot or invocation filters
are errors under v262. Unrestricted enumeration still uses native FIELD/DATA
indexes. `-F` honors value limits; field-name order is unspecified. Time bounds
are parsed once per occurrence, including checked duration/epoch arithmetic,
calendar rounding, local/UTC/offset/IANA zones, TZif leaps and cached DST
selection. Cursor-relative limits begin at the cursor. Follow streams resolved
cursor history regardless of numeric line caps; since-only follow, explicit all
and no-tail include the complete existing matching backlog. Repeated projections
form a union without trimming field-name bytes. Cat preserves raw first values;
boot/invocation tables support all four JSON framings.
Follow without a since bound defaults to ten existing rows, including with
pager-end; follow with since and pager-end defaults to one thousand. Explicit
line counts, all/no-tail and cursor positioning retain their precedence.

System/user unit wildcard expansion uses indexed unique field values. If no
exact selector or wildcard expansion adds a unit, return the native no-data
error before row output, including with a zero line limit. Exact absent unit
names and mixed exact/wildcard selectors remain valid. An ordinary journal
field cannot turn an empty expansion into a match through a private sentinel.
Unit arguments retain native glob/suffix rules, byte escaping, pure path-to-unit
conversion and long-path hashing. Empty names are invalid. Unix wildcard
matching uses fnmatch NOESCAPE in the environment locale. FreeBSD uses a
shared CLI-private compatibility matcher for extended bracket syntax missing
from its libc, including named character classes and single-character
collating elements. Ordinary patterns keep native matching. The Windows fallback
matches unit payload bytes using the C-locale grammar. Current-user conversion
deduplicates moved system selectors. Follow retains the initial wildcard
expansion and invocation selection.

Invocation discovery uses the three base system/user unit fields, excluding
show-only coredump and slice groups. It reads the first valid nonzero ID from
the ordered invocation fields and walks indexed matches in native direction.
ID bounds span every matching row in the selected boot, including rows without
unit metadata. Explicit IDs without offsets may match secondary ID fields;
relative offsets advance from the reference ID's boundary. Invocation selection
replaces boot/unit show matches while retaining identifier, transport, priority
and facility filters. Readers are reused and metadata getters avoid expanding
unrelated payloads. Broader input envelopes remain separate performance work.

Native integer options preserve base-prefix/octal syntax and signed 32-bit
limits for line counts and boot/invocation offsets; vacuum file counts retain
the unsigned 64-bit range. Descriptor IDs use 32 hexadecimal digits, with an
explicit sign before an offset. Priority symbols are canonical and case
sensitive; numeric facilities range through 127. Invalid unique-field names
fail even with a zero limit. Empty requested cursor-file paths retain their
requested update semantics.

Bounded show output stops before reading or matching a further row. Boot
transitions are observed before grep in both directions and follow polls;
initial follow-tail selection counts native rows before grep and poll resume
state includes nonmatching visits. Zero-limit boot/invocation tables exit one,
with quiet/JSON suppressing informational diagnostics. Quiet verification
retains errors while suppressing successful PASS messages.
Show-cursor and cursor-file checkpoints retain the final native position,
including grep misses, bound-crossing rows and zero-line positioning. Finite
selected counts still stop immediately without another regex evaluation.

Short output selects the last repeated MESSAGE; grep and cat retain their
first-value rules. No-full output applies native per-line shortening,
prefix/suffix retention, three-line/300-byte thresholds and the v262 fixed
Unicode width table. COLUMNS is resolved once per shortened-output renderer;
valid values from 1 through 65535 override stdout's terminal width, followed by
an 80-column fallback. The default full-width path bypasses shortening.
Displayed realtime uses the last source-realtime C-string value when native
integer parsing yields a value strictly between zero and 2^55; otherwise it
uses the ENTRY realtime. This does not replace exported ENTRY metadata.
Before short rendering, native tab/ANSI/CR preparation strips completed color
and OSC sequences, expands tabs to eight spaces and drops trailing/pre-newline
CR. Show-all bypasses this step; cat/grep continue to use raw MESSAGE bytes.
JSON aggregation keeps a single binary byte array distinct from an outer array
of repeated values in all four framings. Vacuum-size parsing uses native
ordered uppercase compound suffixes and checked decimal/fraction arithmetic.

Color output follows SYSTEMD_COLORS fixed/automatic/boolean modes, NO_COLOR,
stdout/stderr terminal state, TERM and COLORTERM. Enabled output includes native
priority/audit colors, first-match grep highlighting, verbose timestamps,
cursors and trusted fields, JSON primitive colors, boot separators and table
headers. Underlining is disabled on the Linux console. Grep captures offsets
from its existing PCRE2 match; only colored buffered output retains those
offsets. Plain output avoids color formatting. Verbose multiline fields indent
continuations; short-delta saturates negative differences to zero on reverse
output, following native timestamp subtraction.
Embedded DOCUMENTATION and CONFIG_FILE links follow SYSTEMD_URLIFY; optional
file links discover the command's hostname lazily outside the SDK. Host catalog
lookup, including implicit documentation-link lookup, remains outside portable
mode. Header path output and vacuum filename tie ordering preserve native bytes.

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
  explicit `--lines`, `--head`, or `--tail` option is present. Follow without
  since retains its implicit ten-entry tail instead.
- Bare `--lines` and `-n` select the stock default 10-entry tail. Explicit
  empty `--lines=` is a parse error, matching stock `journalctl`.
- Empty default/verbose show results print `-- No entries --`; `--quiet` and
  auto-quiet modes such as JSON, export, and cat suppress that line.
- `--cursor`, `--after-cursor`, `--cursor-file`, and `--show-cursor` use
  official systemd cursor strings for file-backed reads and cursor-file
  updates.
- `--list-invocations` uses native directional indexed discovery and global
  boot-scoped ID bounds. SOW-0122 retains broader large-input envelope and
  polling work, beyond the required correctness and performance repair.
- Recognized options may appear before or after show-action match arguments,
  matching stock argument permutation. Short options accept stock attached
  values and clusters such as `-n2`, `-ball`, and `-rn2`, while stock-invalid
  forms such as `-n=2` and `-b=true` remain invalid.
- `--identifier`, `--exclude-identifier`, `--priority`, `--facility`,
  `--grep`, `--case-sensitive`, `--dmesg`, `--unit`, and `--user-unit` are
  file-backed filters. `--exclude-identifier` follows stock v262 output-mode
  scope: short-family outputs filter matching `SYSLOG_IDENTIFIER=` entries,
  while JSON, export, verbose, and cat remain unchanged. `--grep` with
  tail-style `--lines=N` implies reverse traversal unless `--follow` is set.
  Unit filters use the same systemd v262 unit-related fields, disjunction
  groups, unit suffix mangling, glob expansion over FIELD/DATA indexes, and
  `--user --unit=` rewrite (unless `--system` is also present) as stock file-backed `journalctl`.
- `--disk-usage` reports allocated filesystem usage for explicit
  `--file`/`--directory` journal inputs. Without explicit input, it remains an
  unsupported host-journal discovery action.
- `--vacuum-size`, `--vacuum-files`, and `--vacuum-time` mutate only explicit
  `--directory` inputs. They delete stock-recognized archived `.journal` and
  `.journal~` files from that directory while protecting active/current,
  non-matching, unknown, and subdirectory files. `--vacuum-files` uses the
  stock count condition: protected active/current files plus remaining archived
  candidates must be at or below the requested count.
  Age is the earliest available filename, modification, access, change or
  creation timestamp. Linux includes a valid legacy `user.crtime_usec` xattr
  and attempts header reads without updating atime, retrying ordinary reads
  when that open mode is unavailable. Metadata checks are CLI maintenance
  operations; SDK and query traversal do not perform them.
  The user accepted the creation-metadata cost for explicit age-based vacuum
  maintenance: the measured warm control adds about 7 ms in Go and 3.8 ms in
  Rust for 1000 recent archive names. This is a maintenance-only exception;
  SDK and sustained query performance requirements remain unchanged.
- `--new-id128` is a portable standalone utility action and prints a new v4
  ID128 in the same string, UUID, `SD_ID128_MAKE()`, and Python constant forms
  as stock systemd v262.
- Positional `FIELD=value`, path, and `+` disjunction arguments belong to the
  show action. Non-show actions other than catalog listing/dumping reject
  extra positional arguments with the stock `Extraneous arguments starting with
  '<token>'` parser error before portable unsupported dispatch.
- `--follow` and `-f` follow explicit inputs. `--file` and `-i` select files;
  the old conflicting `-f` file alias is not part of the v262 contract.

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

## Indexed Snapshot And Recovery Contract

Go and Rust MUST provide the additive IndexedSnapshot surface documented in
`docs/Indexed-Snapshots.md`. Capture freezes committed population bounds, declared
FIELD heads and exact-value counts under caller-owned writer exclusion.
Go IsArchived and Rust is_archived MUST read the owned captured header state,
returning true only for archived state. Later archival MUST NOT change that
result. This metadata alone MUST NOT be treated as integrity certification.
Traversal MUST use native indexes without eager O(entries) offset lists. Go uses
bounded chunks; Rust retains offsets for one ENTRY, so scratch grows with entry
width rather than file entry count. Selected compressed payloads require decoding
space. Go guards expired payload visitation, not reads of exported metadata fields;
FIELD predicates expose posting multiplicity rather than implicit union semantics.
Callbacks MUST honor borrowed payload lifetimes and cooperative cancellation.
Historical/layout/compression support is shared with existing readers.

Strict VerifyIndex/verify_index MUST certify full bidirectional native index
membership before uncertain files are reused; the compatibility verifier remains
separate. This offline check has graph-sized time and memory, not query-open cost.
Strict verification MUST bound every committed object by the declared arena,
not just physical preallocation. Snapshot capture and append-open MUST reject
declared extents that exclude their required objects before variable index
mapping or writer mutation. Empty populations MAY inherit tail sequence state;
current-file entry pointers, head sequence and timestamps MUST be empty. Tail
boot metadata MUST follow historical header field presence and compatible flags.
These shared header checks MUST NOT turn compatibility readers into full graph
certifiers or reject the historical damaged-tail fixtures they already support. Ordinary live readers MUST permit declared capacity to exceed physical size
during normal append preparation while validating header arithmetic and the
physical bounds of every mapped or accessed region. Stable-file integrity checks
MUST remain owned by writer-excluded capture, strict verification and append-open,
not unconditionally applied at ordinary reader open.
Rejected append-open validation MUST leave file bytes and extent unchanged.
Writable reuse and sync MUST preserve existing/published allocation, including
final-object alignment padding. Temporary growth caused only by mapping windows
MAY be trimmed back to that retained allocation. Resumed appends MUST use the
aligned end of the last object.
Errors after the first possible file or publication-state mutation MUST poison
writers. Cleanup MUST preserve uncertain files without
clean-state publication, archival retries, empty-file deletion or retention.
Validation and capacity errors proven to precede mutation MUST leave writers
reusable. Open itself MUST NOT silently
incur a full strict verification pass. Applications own recovery/provenance policy.
