# SOW-0026 - Netdata SDK Integration Inventory And Cut Plan

## Status

Status: in-progress

Sub-state: implemented; ready for orchestrator review.

Actual Netdata component edits remain out of scope for this SOW.

## Requirements

### Purpose

Create the exact Netdata integration map before changing Netdata. The final
integration should look natural in Netdata, preserve existing behavior, and use
this SDK everywhere journal reading or writing is needed.

### User Request

The user asked to track integration of the SDK into Netdata:

- `netflow.plugin` reader and writer paths;
- `otel.plugin` writer path;
- `otel-signal-viewer.plugin` reader path;
- `systemd-journal.plugin` reader path when compiled without libsystemd;
- static packaging that needs the no-libsystemd reader;
- removal of the old vendored Rust journal implementation after all consumers
  move to the SDK.

The user clarified that actual integration should happen last because old
vendored libraries may still be faster until SOW-0009 child SOWs are complete.

### Assistant Understanding

Facts:

- Writes outside this repository are forbidden unless the user explicitly
  authorizes a specific Netdata repository target.
- SNMP traps has already been integrated externally by the user against
  `v0.3.0` / `go/v0.3.0`.
- The user reported SNMP traps improved from about 5.5k traps/s on `v0.1.0` to
  about 170k traps/s on `v0.3.0`.
- NetFlow needs both writer and reader integration.
- OTEL logs need writer integration.
- OTEL signal viewer needs reader integration.
- `systemd-journal.plugin` needs the pure reader path for builds without
  libsystemd.
- Writers should default to compact journal format in Netdata integrations.

Inferences:

- The first Netdata SOW should be an inventory/cut-plan SOW, not direct code
  integration.
- Component integration SOWs should be split to avoid mixing writer, reader,
  packaging, and vendored-removal risks.
- Dependency strategy should use a versioned SDK tag or commit, not a moving
  target, once the API is stable enough for Netdata.

Fresh inventory target:

- Repository: `ktsaou/netdata`
- Remote observed read-only: GitHub SSH remote for `ktsaou/netdata` redacted
  to avoid durable-artifact sensitive-data patterns.
- Branch observed read-only: `split-systemd-journal`
- Commit observed read-only: `445dd8eb845c`

Remaining open items for component SOWs:

- Exact SDK tag/commit to use after the writer and reader performance gates.
- Whether OTEL keeps current flattened field names through SDK `RAW` policy or
  changes producer-side field naming before adopting `JOURNALD` policy.

### Acceptance Criteria

- Inventory every Netdata journal reader and writer consumer at a specific
  Netdata commit.
- Record exact files, functions, crates/modules, and current dependencies for
  each consumer.
- Confirm SNMP traps current state as already integrated externally, without
  changing Netdata from this repository.
- Produce a cut plan for:
  - NetFlow writer and reader integration;
  - OTEL writer integration;
  - OTEL signal viewer reader integration;
  - `systemd-journal.plugin` no-libsystemd reader integration;
  - static packaging implications;
  - vendored journal implementation removal.
- Map each component to a real pending component SOW.
- Record performance prerequisites from SOW-0042 through SOW-0046.
- Record exact repository boundary and user authorization needed before any
  Netdata-side edit.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`
- `.agents/sow/pending/SOW-0047-20260528-netdata-netflow-sdk-integration.md`
- `.agents/sow/pending/SOW-0048-20260528-netdata-otel-writer-sdk-integration.md`
- `.agents/sow/pending/SOW-0049-20260528-netdata-reader-plugin-sdk-integration.md`
- `.agents/sow/pending/SOW-0050-20260528-netdata-vendored-journal-removal.md`
- `.agents/sow/specs/product-scope.md`
- Read-only Netdata source evidence from `ktsaou/netdata @ 445dd8eb845c`.

Current state:

- This SDK repository contains the SDK work and SOW plan.
- The Netdata source tree was inspected read-only. No Netdata files were
  modified.
- Component integration SOWs exist and remain the correct implementation
  targets after this inventory.

Open-source reference evidence:

```text
ktsaou/netdata @ 445dd8eb845c
```

### Build And Packaging Surfaces

Facts:

- `CMakeLists.txt:242` defines `ENABLE_NETDATA_JOURNAL_FILE_READER`.
- `CMakeLists.txt:245-263` imports Corrosion and `journal_reader_ffi` from
  `src/crates/jf/Cargo.toml` when the Netdata journal reader is enabled.
- `CMakeLists.txt:267-278` imports workspace crates for `otel-plugin`,
  `otel-signal-viewer-plugin`, and `netflow-plugin`.
- `CMakeLists.txt:2940-2945` enables the internal journal reader when
  `systemd-journal.plugin` is enabled and `SYSTEMD_FOUND` is false.
- `CMakeLists.txt:2948-2956` links `systemd-journal.plugin` against
  `journal_reader_ffi` and defines `HAVE_RUST_PROVIDER`.
- `CMakeLists.txt:3484-3520` installs the Rust OTEL, signal viewer, and NetFlow
  plugins and their configs.
- `packaging/docker/Dockerfile:37-45` enables OTEL, signal viewer, and
  `--internal-systemd-journal` in the Docker build.
- `packaging/docker/Dockerfile:85-88` and `packaging/docker/Dockerfile:170-173`
  check installed permissions for `otel-plugin`,
  `otel-signal-viewer-plugin`, and `systemd-journal.plugin`.
- `packaging/makeself/jobs/70-netdata-git.install.sh:25-37` sets static Rust
  flags and enables systemd-journal, internal journal, OTEL, and signal viewer
  on non-armv6 builds.
- `packaging/makeself/install-or-update.sh:183-248` applies ownership,
  capability, and mode handling for `otel-plugin`,
  `otel-signal-viewer-plugin`, `systemd-journal.plugin`, and `netflow-plugin`.

Implication:

- SDK integration must preserve Corrosion/static build behavior, install names,
  generated C header inclusion, plugin permissions, and armv6/static exceptions.

### Vendored Journal Crates

Facts:

- `src/crates/Cargo.toml:3` excludes `jf` from the main workspace.
- `src/crates/Cargo.toml:6-11` includes local `journal-common`,
  `journal-core`, `journal-index`, `journal-log-writer`, `journal-engine`, and
  `journal-registry`.
- `src/crates/Cargo.toml:24-32` includes `journal-function`,
  `otel-signal-viewer-plugin`, `otel-plugin`, and `netflow-plugin`.
- `src/crates/Cargo.toml:159-164` wires the local journal crates as workspace
  dependencies.
- `src/crates/Cargo.toml:178-183` wires `journal-function`, signal viewer,
  OTEL, and OTEL flattening crates.
- `src/crates/jf/Cargo.toml:5-8` contains `journal_file`,
  `journal_reader_ffi`, `window_manager`, and `sigbus`.
- `src/crates/jf/journal_reader_ffi/Cargo.toml:8-13` depends on the old
  `journal_file` and support crates.

Implication:

- SOW-0050 must run after all component consumers move. `journal-function` may
  remain as a Netdata query/index layer if it is refit over SDK APIs; removal
  must be proof-driven, not a directory delete.

### NetFlow Writer And Reader

Facts:

- `src/crates/netflow-plugin/Cargo.toml:23-28` depends on all local journal
  crates.
- `src/crates/netflow-plugin/src/ingest.rs:18-24` imports
  `journal_common`, `journal_engine`, `journal_index`, `journal_log_writer`,
  and `journal_registry`.
- `src/crates/netflow-plugin/src/ingest/encode.rs:4-10` uses a reusable
  `JournalEncodeBuffer` to avoid per-flow allocation.
- `src/crates/netflow-plugin/src/ingest/encode.rs:25-39` builds stack slices
  and calls `write_entry_with_timestamps`.
- `src/crates/netflow-plugin/src/ingest/encode.rs:87-99` writes materialized
  tier rows through the same timestamped writer path.
- `src/crates/netflow-plugin/src/ingest/service/init.rs:25-57` builds writer
  config, origin, rotation, retention, and raw/materialized writers.
- `src/crates/netflow-plugin/src/ingest/service/init.rs:98-148` creates raw,
  `minute_1`, `minute_5`, and `hour_1` `Log` writers.
- `src/crates/netflow-plugin/src/ingest/service/runtime.rs:92-165` writes raw
  flow records with source realtime and entry realtime timestamps, then updates
  metrics and facet state.
- `src/crates/netflow-plugin/src/ingest/service/runtime.rs:191-234` syncs raw
  and tier journals according to the NetFlow sync cadence.
- `src/crates/netflow-plugin/src/ingest/service/tiers.rs:69-99` flushes closed
  materialized tier rows through journal writers.
- `src/crates/netflow-plugin/src/flow/record/journal.rs:17-37` encodes only
  non-default flow fields.
- `src/crates/netflow-plugin/src/flow/record/journal/writer.rs:21-128` formats
  raw `FIELD=value` payloads and skips empty or zero values except timestamp
  fields.
- `src/crates/netflow-plugin/src/plugin_config/types/journal.rs:11-49`
  defines journal directory, per-tier retention overrides, CLI retention
  aliases, and query group limits.
- `src/crates/netflow-plugin/src/plugin_config/types/journal.rs:182-240`
  defines base/tier directories, retention per tier, rotation size, and
  rotation duration.
- `src/crates/netflow-plugin/src/plugin_config/defaults.rs:87-104` defaults
  retention to 10 GB and 7 days, with 1 hour rotation duration.
- `src/crates/netflow-plugin/src/query.rs:14-19` imports the old reader,
  cursor, registry, and machine-id crates.
- `src/crates/netflow-plugin/src/query/service.rs:23-47` watches all tier
  directories through `journal_registry`.
- `src/crates/netflow-plugin/src/query/service.rs:75-137` initializes facets by
  scanning archived and active files.
- `src/crates/netflow-plugin/src/query/scan/direct.rs:11-24` exposes the
  direct scan callback over `JournalFile<Mmap>`, data offsets, and a
  decompression buffer.
- `src/crates/netflow-plugin/src/query/scan/direct.rs:28-134` filters sorted
  registry files, opens journal files, seeks by time, and iterates cursors.
- `src/crates/netflow-plugin/src/query/scan/direct.rs:137-161` visits
  compressed and uncompressed DATA payloads without materializing full entries.
- `src/crates/netflow-plugin/src/query/scan/raw.rs:104-177` opens journal
  files, steps the reader, filters by time, and collects entry data offsets.
- `src/crates/netflow-plugin/src/query/scan/raw.rs:246-303` applies projected
  raw payload logic with decompression as needed.
- `src/crates/netflow-plugin/src/ingest/rebuild.rs:37-57` uses batch file
  indexes and `LogQuery` to find the latest tier timestamp.
- `src/crates/netflow-plugin/src/ingest/rebuild.rs:130-173` replays raw journal
  files into materialized tiers through `scan_journal_files_forward` and
  payload visitors.

Cut-plan requirements:

- Replace writer use with the Rust SDK high-level directory writer, configured
  for compact output in Netdata.
- Preserve caller-owned slice semantics: NetFlow can pass borrowed raw
  `FIELD=value` buffers only for the append call; the SDK must finish copying
  or committing synchronously before returning.
- Preserve both source realtime and entry realtime timestamps.
- Preserve per-tier directories, retention, rotation, lifecycle observer hooks,
  sync cadence, metrics, and facet sidecars.
- Replace reader/query paths only with SDK APIs that can scan by file/time,
  visit raw DATA payloads, decompress on demand, and avoid full-entry maps on
  hot paths.

Risk:

- NetFlow is the highest-risk component. A naive SDK reader adapter that
  allocates maps per entry or hides DATA offsets would regress projected query,
  facet rebuild, and tier rebuild paths.

### OTEL Writer

Facts:

- `src/crates/netdata-otel/otel-plugin/Cargo.toml:33-36` depends on local
  journal crates.
- `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:3-5` imports
  `load_machine_id`, `journal_log_writer`, and `journal_registry::Origin`.
- `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:15-44` owns a
  mutex-wrapped `Log` and constructs rotation, retention, origin, config, path,
  and writer.
- `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:58-76` selects the
  source timestamp from `time_unix_nano` or `observed_time_unix_nano`.
- `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:78-124` builds
  `Vec<Vec<u8>>` raw `key=value` payloads and optionally stores `OTLP_JSON`.
- `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:138-161` sorts log
  entries by creation timestamp and calls `log.write_entry`.
- `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:170-176` syncs after
  each export batch.
- `src/crates/netdata-otel/otel-plugin/src/plugin_config/logs.rs:54-115`
  defines logs writer configuration.
- `src/crates/netdata-otel/otel-plugin/src/plugin_config/logs.rs:117-129`
  defaults the journal path, file size, entries per file, file count, total
  size, retention age, rotation duration, and `store_otlp_json=false`.

Cut-plan requirements:

- Replace writer use with the Rust SDK high-level directory writer, configured
  for compact output in Netdata.
- Preserve sorted append order, timestamp selection, retention, rotation, and
  sync-after-export-batch behavior.
- Preserve `OTLP_JSON` storage behavior.
- Use SDK `RAW` field policy if keeping current flattened OTEL field names.

Risk:

- Current OTEL keys include producer-level flattened names that can contain
  lowercase characters or dots. The SDK `JOURNALD` field policy is designed for
  journald-style uppercase field names, so forcing OTEL through that default
  would be a behavior-changing producer decision. SOW-0048 should either keep
  `RAW` policy or record an explicit field-normalization decision before
  implementation.

### OTEL Signal Viewer Reader

Facts:

- `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/Cargo.toml:42-45`
  depends on `journal-core`, `journal-index`, `journal-function`, and
  `journal-registry`.
- `src/crates/netdata-log-viewer/journal-function/Cargo.toml:29-32` depends on
  local `journal-core`, `journal-index`, `journal-engine`, and
  `journal-registry`.
- `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/main.rs:15-22`
  installs a SIGBUS handler before mmap operations.
- `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/main.rs:41-80`
  loads config, creates the monitor, indexing limits, and `CatalogFunction`.
- `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/main.rs:82-109`
  watches configured journal directories and processes notify events.
- `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/plugin_config.rs:14-29`
  defaults journal paths to `/var/log/journal` and `/run/log/journal`.
- `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/plugin_config.rs:55-106`
  defines cache defaults and indexing limits.
- `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/configs/otel-signal-viewer.yaml.in:4-10`
  points stock config at the OTEL journal path.
- `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/catalog.rs:31-69`
  converts filters into indexed field filters with OR within a field and AND
  across fields.
- `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/catalog.rs:203-287`
  builds time-range queries, limits, filters, regex, and executes `LogQuery`.
- `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/catalog.rs:295-367`
  performs opposite-direction pagination checks and sorts logs for UI output.
- `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/catalog.rs:378-425`
  builds the registry, cache, histogram engine, and directory watchers.
- `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/catalog.rs:470-531`
  finds files, builds cache keys, and computes file indexes.

Cut-plan requirements:

- Preserve the current query contract: time range, limit, pagination flags,
  regex/filter behavior, field facets, histograms, cache behavior, active file
  handling, and directory watch updates.
- Either keep Netdata's `journal-function` as the query/index layer over SDK
  low-level readers, or add SDK reader/index APIs that cover the same behavior
  before replacing it.
- Preserve SIGBUS/live-file safety and the cache ownership model.

Risk:

- Treating this as a simple sequential reader swap would miss the index,
  histogram, cache, and pagination contracts used by the UI.

### systemd-journal.plugin No-Libsystemd Reader

Facts:

- `src/collectors/systemd-journal.plugin/provider/netdata_provider.h:10-27`
  selects `rust_provider.h` when `HAVE_RUST_PROVIDER` is defined and defines
  Rust-compatible typedefs and macros.
- `src/collectors/systemd-journal.plugin/provider/netdata_provider.h:46-73`
  declares the provider facade for open/close, seek, next/previous, seqnum,
  realtime, data/fields/unique enumeration, and match operations.
- `src/collectors/systemd-journal.plugin/provider/netdata_provider.c:21-199`
  dispatches every provider operation to either `rsd_journal_*` or
  `sd_journal_*`.
- `src/collectors/systemd-journal.plugin/provider/rust_provider.h:4-25`
  includes the generated FFI header and defines enumeration macros plus
  `HAVE_SD_JOURNAL_RESTART_FIELDS` and `HAVE_SD_JOURNAL_GET_SEQNUM`.
- `src/crates/jf/journal_reader_ffi/src/lib.rs:98-151` stores
  `JournalFile<Mmap>`, `JournalReader`, field buffers, and decompression
  buffer, installs SIGBUS handling, and opens the first path.
- `src/crates/jf/journal_reader_ffi/src/lib.rs:160-260` implements seek,
  next/previous, seqnum, and realtime functions.
- `src/crates/jf/journal_reader_ffi/src/lib.rs:265-340` implements data and
  field enumeration with decompression.
- `src/crates/jf/journal_reader_ffi/src/lib.rs:342-454` implements unique
  queries, match conjunctions, disjunctions, and flush.
- `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:30-88`
  iterates entry DATA, parses `FIELD=value`, handles
  `_SOURCE_REALTIME_TIMESTAMP`, and adds facets.
- `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:107-320`
  implements backward and forward query loops using provider seek, step,
  timestamp, seqnum, sampling, and row processing functions.
- `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:355-430`
  applies filters through field enumeration, unique queries, conjunctions,
  disjunctions, and matches.
- `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:435-610`
  opens one file at a time, sorts files by direction, queries each file, and
  closes the provider handle.
- `src/collectors/systemd-journal.plugin/systemd-journal-files.c:201-318`
  opens a provider handle to update first/last timestamps, seqnums, writer IDs,
  message counts, and boot ID annotations.
- `src/collectors/systemd-journal.plugin/systemd-main.c:69-94` watches journal
  directories and registers the `systemd-journal` function with sensitive data
  access.
- `src/collectors/systemd-journal.plugin/systemd-journal-annotations.c:344-381`
  contains a direct `sd_journal_open_files` fallback only when
  `HAVE_SD_JOURNAL_RESTART_FIELDS` is not defined.

Cut-plan requirements:

- Replace the vendored `journal_reader_ffi` with an SDK-backed C facade or keep
  the existing provider C shape and change only the Rust implementation behind
  it.
- Preserve pointer lifetime contracts: data and field pointers are valid only
  until the next enumerate/restart/close call.
- Preserve seek, next/previous, realtime, seqnum, field enumeration, unique
  enumeration, match conjunction/disjunction, decompression, SIGBUS/live-file
  handling, and one-file-at-a-time query behavior.
- Preserve `HAVE_SD_JOURNAL_RESTART_FIELDS` availability or refactor the
  annotation fallback so the no-libsystemd build does not reintroduce a direct
  libsystemd dependency.

Risk:

- This path has C ABI, generated-header, packaging, and static-link behavior in
  addition to reader semantics. It should be handled in SOW-0049 after the SDK
  reader facade is proven.

### SNMP Traps

Facts:

- The user reported SNMP traps is already integrated externally against
  `v0.3.0` / `go/v0.3.0`.
- The user reported performance improved from about 5.5k traps/s on `v0.1.0`
  to about 170k traps/s on `v0.3.0`.
- Read-only searches in `ktsaou/netdata @ 445dd8eb845c` did not find a local
  SNMP trap journal integration under the checked Rust crates or CMake journal
  integration surfaces.

Implication:

- SNMP traps is treated as external state for this SOW. This inventory does not
  change it and does not use it as evidence that the remaining Rust reader and
  writer paths are performance-ready.

### Same-Failure Search Summary

Facts:

- Writer search across `src/crates` and `src/collectors` for
  `journal_log_writer`, `write_entry_with_timestamps`, `write_entry(`,
  `Log::new(`, and related terms found production writer consumers in NetFlow
  and OTEL, plus local journal crate internals/tests.
- Reader search across `src/crates` and `src/collectors/systemd-journal.plugin`
  for `JournalFile::<Mmap>::open`, `JournalReader`, `JournalCursor`,
  `batch_compute_file_indexes`, `LogQuery::new`, `FileIndexer`,
  `nsd_journal_open_files`, `sd_journal_open_files`, `journal_reader_ffi`, and
  local journal crate names found production reader consumers in NetFlow query
  and rebuild paths, OTEL signal viewer and `journal-function`, and
  `systemd-journal.plugin`.

Conclusion:

- No other production journal SDK integration consumer was found in the checked
  Netdata commit.

## Pre-Implementation Gate

Status: satisfied for this read-only inventory and cut plan. Component
integration remains gated by the performance SOWs and by explicit Netdata
repository implementation authorization.

Problem / root-cause model:

- Netdata has multiple journal producers and consumers. The SDK can replace
  them only after public APIs and performance are fit for production use.
- The current work is a read-only inventory plus component cut plan. It does
  not require Netdata source edits.
- Component implementation requires a selected SDK tag/commit, Netdata branch,
  and component-specific build/test validation.

Evidence reviewed:

- Current SOW inventory and product-scope spec.
- SOW-0047 through SOW-0050 component SOWs.
- Read-only Netdata evidence from `ktsaou/netdata @ 445dd8eb845c`.

Affected contracts and surfaces:

- NetFlow ingestion, replay, query, facet behavior, and tier rebuild.
- OTEL logs ingestion and flattened field-name handling.
- OTEL signal viewer reading, indexing, cache, histogram, filter, and
  pagination behavior.
- `systemd-journal.plugin` fallback reading without libsystemd.
- Netdata static packaging, generated FFI header inclusion, and plugin
  permissions.
- SDK versioning, Rust crate use, C facade shape, and public API stability.

Existing patterns to reuse:

- Existing Netdata `journal-log-writer` integration shape.
- SDK high-level writer APIs with explicit compact output configuration.
- SDK reader and C/libsystemd-compatible facade patterns.
- Existing Netdata lifecycle, registry/watch, cache, and packaging conventions
  discovered during the fresh inventory.

Risk and blast radius:

- High for component SOWs. This inventory itself changes only planning
  artifacts, but the mapped component work affects production ingestion,
  reader/query behavior, storage format defaults, and Netdata packaging.

Sensitive data handling plan:

- Use only source code, synthetic fixtures, and sanitized examples.
- Do not record real customer logs, SNMP communities, trap payloads, flow
  payloads, credentials, bearer tokens, private endpoints, personal data, or
  production incident details.

Implementation plan:

1. Read the selected Netdata commit read-only.
2. Inventory all reader and writer consumers with file/function evidence.
3. Map each consumer to SDK contracts, risks, and component SOWs.
4. Record the ordered cut plan and validation expectations.
5. Leave component implementation to SOW-0047 through SOW-0050.

Validation plan:

- `git diff --check`.
- `.agents/sow/audit.sh`.
- Component SOWs own Netdata build/test validation.
- Orchestrator review owns external reviewer routing for this implemented
  inventory.

Artifact impact plan:

- AGENTS.md: no update needed; repository boundary and external implementer
  exception are already covered by project instructions and the user prompt.
- Runtime project skills: no update needed; no durable workflow change.
- Specs: no update needed; this SOW records integration inventory but does not
  change shipped SDK behavior or public contracts.
- End-user/operator docs: update only when Netdata integration changes shipped
  behavior in component SOWs.
- End-user/operator skills: update only if Netdata docs/spec changes affect
  output/reference skills.
- SOW lifecycle: SOW-0026 remains in-progress for orchestrator review; SOW-0047
  through SOW-0050 remain the implementation follow-ons.
- SOW-status.md: not updated in this worktree because the user explicitly
  required the SOW to remain in-progress and the current prompt scope is the
  assigned SOW file.

Open-source reference evidence:

- `ktsaou/netdata @ 445dd8eb845c`
  - `CMakeLists.txt:242`
  - `CMakeLists.txt:245-263`
  - `CMakeLists.txt:267-278`
  - `CMakeLists.txt:2940-2956`
  - `CMakeLists.txt:3484-3520`
  - `src/crates/Cargo.toml:3-32`
  - `src/crates/Cargo.toml:159-183`
  - `src/crates/netflow-plugin/src/ingest/encode.rs:4-99`
  - `src/crates/netflow-plugin/src/query/scan/direct.rs:11-161`
  - `src/crates/netflow-plugin/src/query/scan/raw.rs:104-303`
  - `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:3-176`
  - `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/catalog.rs:31-531`
  - `src/collectors/systemd-journal.plugin/provider/netdata_provider.h:10-73`
  - `src/collectors/systemd-journal.plugin/provider/netdata_provider.c:21-199`
  - `src/crates/jf/journal_reader_ffi/src/lib.rs:98-454`

Open decisions:

1. Dependency strategy for component implementation
   - Status: component SOWs should use a versioned SDK tag or pinned commit
     after applicable performance gates pass.
2. OTEL field policy
   - Status: SOW-0048 should keep SDK `RAW` policy for current flattened field
     names unless the user accepts a producer-side field normalization change.

## Implications And Decisions

1. 2026-05-28 integration split
   - Decision: SOW-0026 becomes inventory/cut-plan only.
   - Implication: component SOWs own implementation after performance gates.
   - Risk: this delays integration, but avoids replacing fast vendored paths
     with slower or incomplete SDK paths.

## Plan

1. Keep SOW-0026 limited to inventory and cut planning.
2. Use SOW-0048 for OTEL writer integration after writer performance gates.
3. Use SOW-0047 for NetFlow writer, reader, query, and rebuild integration
   after writer and reader performance gates.
4. Use SOW-0049 for OTEL signal viewer and `systemd-journal.plugin`
   no-libsystemd reader integration after reader/facade gates.
5. Use SOW-0050 for vendored journal removal only after SOW-0047, SOW-0048,
   and SOW-0049 are implemented, reviewed, validated, and merged.

### Ordered Cut Plan

1. Prerequisite SDK gates
   - SOW-0042 must certify writer performance for the Rust/Go surfaces needed
     by Netdata writer integrations.
   - SOW-0044 through SOW-0046 must cover reader/query/facade performance and
     compatibility before reader-heavy Netdata paths are cut over.
   - Risk: skipping this gate could regress NetFlow hot ingestion, projected
     grouped queries, OTEL signal viewer queries, or the no-libsystemd
     `systemd-journal.plugin` path.

2. SOW-0048 - OTEL writer
   - Scope: replace `journal_log_writer::Log` usage in OTEL logs with SDK Rust
     directory writer.
   - Required settings: compact output, retention/rotation parity,
     sync-after-export-batch parity, and likely SDK `RAW` field policy.
   - Validation: write/readback fixtures for current flattened OTEL fields,
     `OTLP_JSON`, timestamp selection, rotation, retention, and signal viewer
     consumption.

3. SOW-0047 - NetFlow writer and reader
   - Scope: replace raw and materialized tier writers, direct scan reader,
     projected raw scan, facet initialization, and tier rebuild readers.
   - Required settings: compact output, borrowed raw payload append API,
     source/entry realtime parity, lifecycle observer parity, tier retention
     and rotation parity, sync cadence parity, and zero-allocation hot scans.
   - Validation: flow encoding round trips, raw/tier writes, projected grouped
     query parity, facet sidecar parity, rebuild parity, retention/rotation,
     active file handling, and performance gates.

4. SOW-0049 - Reader plugins
   - Scope: replace signal viewer reader/index dependencies and the
     `systemd-journal.plugin` no-libsystemd `journal_reader_ffi` implementation
     with SDK-backed APIs.
   - Required settings: query/index/cache/histogram/pagination parity for
     signal viewer; C facade ABI, pointer lifetime, match/unique semantics,
     seqnum/realtime, decompression, generated header, and static build parity
     for systemd-journal.
   - Validation: signal viewer query parity, cache/index rebuild, live active
     file watch behavior, C facade tests, no-libsystemd static build, and
     packaging smoke checks.

5. SOW-0050 - Vendored journal removal
   - Scope: remove or replace `src/crates/jf`, local `journal-*` crates, and
     workspace/CMake references only after all consumers are gone.
   - Required proof: search, build, package, and runtime smoke evidence that no
     old journal crates remain on production paths.

## Delegation Plan

Implementer:

- User-authorized parallel worktree implementation for this assigned inventory
  SOW. No Netdata source edits are in scope.

Reviewers:

- Orchestrator-owned read-only review after this worktree commit. This
  implementation agent does not run external reviewers.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- DO NOT MAKE CHANGES OUTSIDE THIS REPOSITORY FOR ANY REASON.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- Record missing SDK APIs, performance blockers, Netdata repository blockers,
  and reviewer findings in this SOW before activating component work.

## Execution Log

### 2026-05-30

- Confirmed user-authorized parallel implementation routing; AGENTS.md
  external-implementer exception applies for this worktree.
- Moved this SOW from `pending/` to `current/` and changed status from `open`
  to `in-progress`.
- Inspected `ktsaou/netdata @ 445dd8eb845c` read-only for writer, reader,
  query, rebuild, packaging, and vendored-crate surfaces.
- Recorded the component inventory, SDK cut plan, performance risks, and
  follow-on SOW mapping. No Netdata source files were changed.
- Redacted the observed GitHub SSH remote string after the SOW audit flagged
  it as a durable-artifact email-address pattern.

### 2026-05-28

- Rescoped from direct Netdata integration to integration inventory and cut
  planning after user agreement.

## Validation

Acceptance criteria evidence:

- Inventory every Netdata journal reader and writer consumer at a specific
  Netdata commit:
  - `ktsaou/netdata @ 445dd8eb845c` recorded in this SOW.
  - Writer consumers found: NetFlow raw/tier writers and OTEL logs writer.
  - Reader consumers found: NetFlow query/rebuild readers, OTEL signal viewer
    and `journal-function`, and `systemd-journal.plugin` no-libsystemd facade.
- Record exact files, functions, crates/modules, and current dependencies:
  - Build/package evidence recorded under `Build And Packaging Surfaces`.
  - Vendored crate evidence recorded under `Vendored Journal Crates`.
  - Component evidence recorded under NetFlow, OTEL, signal viewer, and
    systemd-journal sections.
- Confirm SNMP traps current state:
  - User-reported external integration and performance improvement recorded
    under `SNMP Traps`.
  - Read-only search did not find a local SNMP trap journal integration in the
    checked Netdata Rust/CMake journal surfaces.
- Produce a cut plan:
  - Ordered cut plan recorded under `Plan`.
  - SOW-0047 covers NetFlow writer/reader.
  - SOW-0048 covers OTEL writer.
  - SOW-0049 covers OTEL signal viewer and systemd-journal no-libsystemd
    readers.
  - SOW-0050 covers vendored removal.
- Record performance prerequisites:
  - SOW-0042 and SOW-0044 through SOW-0046 prerequisites recorded in the
    ordered cut plan.
- Record repository boundary and authorization:
  - SOW states Netdata source was inspected read-only and not modified.
  - User-authorized parallel implementation routing note is recorded in the
    execution log.
- No changes outside this repository:
  - Only this SOW file was edited in this worktree; Netdata was read-only.

Tests or equivalent validation:

- `git diff --check` passed on 2026-05-30.
- `.agents/sow/audit.sh` passed on 2026-05-30 after the observed SSH remote was
  redacted from the durable SOW artifact.

Real-use evidence:

- User reported SNMP traps integration performance improved to about 170k
  traps/s on `v0.3.0`; this informs the cut plan but does not replace the
  remaining reader/writer performance gates.
- Netdata real-use build/runtime validation is intentionally assigned to
  SOW-0047, SOW-0048, SOW-0049, and SOW-0050 because this SOW performs no
  Netdata source edits.

Reviewer findings:

- External reviewers were not run by this implementation worktree. The user
  required this SOW to remain `in-progress` with sub-state
  `implemented; ready for orchestrator review`, so reviewer routing remains an
  orchestrator gate.

Same-failure scan:

- Writer scan terms included `journal_log_writer`,
  `write_entry_with_timestamps`, `write_entry(`, `Log::new(`, and related
  writer construction terms across `src/crates` and `src/collectors`.
- Reader scan terms included `JournalFile::<Mmap>::open`, `JournalReader`,
  `JournalCursor`, `batch_compute_file_indexes`, `LogQuery::new`,
  `FileIndexer`, `nsd_journal_open_files`, `sd_journal_open_files`,
  `journal_reader_ffi`, and local journal crate names across `src/crates` and
  `src/collectors/systemd-journal.plugin`.
- Production journal consumers found are the ones recorded in this SOW:
  NetFlow, OTEL writer, OTEL signal viewer, and systemd-journal no-libsystemd
  reader. Remaining matches were local journal crate internals, tests, benches,
  packaging, or unrelated Netdata engine/logging code.

Sensitive data gate:

- This rescope records no raw secrets, credentials, bearer tokens, SNMP
  communities, customer names, personal data, non-private customer-identifying
  IPs, private endpoints, or proprietary incident details.
- The Netdata source inspection used only source paths, line numbers, build
  files, and configuration defaults.

Artifact maintenance gate:

- AGENTS.md: no update needed; project repository boundary and SOW routing
  rules already cover this work.
- Runtime project skills: no update needed; no durable workflow rule changed.
- Specs: no update needed; this SOW records integration inventory and cut
  planning, not a shipped SDK contract change.
- End-user/operator docs: no update needed; no shipped behavior changed.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: SOW moved from `pending/` to `current/`, status changed to
  `in-progress`, and sub-state left as `implemented; ready for orchestrator
  review` per user instruction.
- SOW-status.md: not updated in this worktree because the user explicitly
  required the assigned SOW to remain in progress for orchestrator review.

Specs update:

- No spec update needed. Product-scope behavior and public SDK contracts did
  not change; this SOW is the durable integration inventory.

Project skills update:

- No project skill update needed.

End-user/operator docs update:

- No docs update needed for this rescope.

End-user/operator skills update:

- No output/reference skill update needed.

Lessons:

- Netdata integration should be planned from a fresh inventory and then split
  by component, not implemented as one broad cut.
- OTEL field policy needs explicit handling because current flattened field
  names may not satisfy journald-style field validation.
- The systemd-journal no-libsystemd path is a C facade and packaging problem,
  not just a Rust reader replacement.

Follow-up mapping:

- NetFlow integration: SOW-0047.
- OTEL writer integration: SOW-0048.
- Reader plugin integrations: SOW-0049.
- Vendored journal removal: SOW-0050.

## Outcome

Implemented inventory and cut plan. This SOW remains `in-progress` with
sub-state `implemented; ready for orchestrator review` as requested.

## Lessons Extracted

- Keep Netdata component cutovers split by runtime contract: writer hot paths,
  indexed/query readers, C facade readers, packaging, and vendored-removal are
  separate risk surfaces.
- Preserve raw payload visitor APIs for NetFlow reader paths; a convenient
  map-per-entry abstraction is likely the wrong shape for projected queries and
  rebuild.
- Keep OTEL field policy explicit. `RAW` policy preserves current flattened
  fields; `JOURNALD` policy requires a producer-side naming decision.
- Do not remove `journal-function` mechanically. It may remain as a Netdata
  query/index layer over SDK readers unless SOW-0049 proves a full SDK
  replacement is available.

## Followup

- SOW-0047 - Netdata NetFlow SDK Integration.
- SOW-0048 - Netdata OTEL Writer SDK Integration.
- SOW-0049 - Netdata Reader Plugin SDK Integration.
- SOW-0050 - Netdata Vendored Journal Removal.

## Regression Log

None yet.
