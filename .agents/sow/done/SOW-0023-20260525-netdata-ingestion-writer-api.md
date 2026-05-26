# SOW-0023 - Netdata Ingestion Writer API

## Status

Status: completed

Sub-state: completed and ready for rollback-point commit. SOW-0019 is completed, and the user explicitly picked up this SOW on 2026-05-26. The verified implementation covers shared high-level naming, chain resume, rotation/retention hardening, duration rotation, age retention, explicit retention enforcement, field-name remapping, strict/default migration polish, single-writer API policy, and the Go `go/v0.1.0` integration contract across Rust, Go, Node.js, and Python.

Current slice on 2026-05-26: completed SOW-0023 close-out. Field-name remapping and strict/default cross-mode migration are implemented consistently across Rust, Go, Node.js, and Python. Performance optimization remains explicitly sequenced to SOW-0009 after feature completion.

## Requirements

### Purpose

Provide stable, production-usable journal writer APIs for Netdata Agent ingestion paths in Rust, Go, Node.js, and Python that need file-backed systemd journal storage without requiring live journald. The high-level writer behavior must use the existing Netdata vendored Rust writer as the compatibility reference for existing NetFlow and OTEL logs integrations, while adding stricter creation-time and field-shape capabilities needed by future ingestion consumers such as SNMP traps.

### User Request

The user said the journal SDK is being prepared in this repository and asked whether the pending SDK SOWs already cover the API needed by the Netdata SNMP trap integration. If not, create a pending SOW here for the API the integration needs.

On 2026-05-25, the user clarified that this SOW must not be SNMP-traps-specific. The API and implementation must be a superset replacement for the existing Netdata journal writer behavior used by NetFlow, OTEL logs, and future ingestion consumers. It must cover time, size, count, path, retention, rotation, nullable policy values that disable individual checks, minimum-file-size and file-count edge cases, and every other behavior required to replace existing Netdata-side writer logic while satisfying all API consumers.

On 2026-05-25, the user clarified the migration constraint: existing Netdata users must not lose their existing journals. After migrating Netdata plugins to this SDK, the plugins must continue writing to the same effective journal directories without requiring manual configuration changes. The SOW must explicitly distinguish whether machine-id path components are added by the SDK, supplied by the consumer, or absent, based on current Netdata behavior.

On 2026-05-26, the user clarified the cross-language contract: all four SDK implementations must follow the same high-level writer rules and API shape. The existing Netdata vendored Rust writer is the behavior reference for Netdata compatibility. Stock systemd naming parity is an optional mode, not the default. The default and unset behavior must keep Netdata vendored Rust chain naming; an explicit `strict_systemd_naming` option may select `system.journal` active naming for consumers that intentionally want that policy.

### Assistant Understanding

Facts:

- The SDK currently has Go, Node.js, and Python high-level directory writers plus the imported Rust high-level writer.
- The current repo Rust and Go high-level writers diverged from the Netdata vendored Rust writer by using `system.journal` as the active filename.
- The Netdata vendored Rust writer creates high-level active files with the chain filename form `system@<seqnum_id>-<head_seqnum>-<head_realtime>.journal`.
- Go, Node.js, and Python high-level writers currently store files below `dir/<machine-id>/`.
- Go currently supports rotation by maximum file size and entry count, and retention by maximum archived-file count and total bytes.
- Go currently attempts to load missing machine ID and boot ID from host files, then falls through to random UUID defaults through `normalizeOptions`.
- Existing tests prove stock `journalctl --directory` readback against the machine-id subdirectory returned by the test helper.
- Existing pending SOWs do not define a Netdata-focused ingestion writer contract.

Inferences:

- Netdata ingestion plugins should not depend on package-local journal writing logic once this SDK has the needed Go API.
- Netdata dynamic configuration needs creation-time failure detection, so writer setup must validate directory creation, permissions, active-file open/create, option validity, lock acquisition, and retention preflight before the job is accepted.
- Existing Netdata consumers use writer-owned machine-id path handling. The SDK must preserve that as the default Netdata-compatible mode: consumers pass the same configured base/tier paths, and the SDK appends `<machine-id>` internally.
- The API should expose both the configured base/tier path and the effective machine-id journal directory so Netdata can preserve existing storage and still document the correct `journalctl --directory` path.
- The API should not describe Netdata-compatible behavior as a migration-only mode. It is the default high-level writer contract for all four SDKs.

Resolved requirements:

- Existing NetFlow and OTEL logs migration must keep the current `configured-dir/<machine-id>/` effective path layout. A flat/direct layout may exist only as an explicit mode for consumers that already use or intentionally request it.
- Default high-level active naming must match Netdata vendored Rust chain naming in all four SDKs: `<source>@<seqnum_id>-<head_seqnum>-<head_realtime>.journal`.
- Strict systemd active naming must be opt-in only: `strict_systemd_naming=false` or unset means Netdata chain naming; `strict_systemd_naming=true` means `<source>.journal` active naming and archive-on-rotation naming.
- Duration rotation and age retention are in scope for this SOW because they are part of the existing NetFlow and OTEL logs policy surface, even though the current Rust writer appears to expose duration rotation without enforcing it.

Resolved final items:

- Automatic Netdata/OTEL field-name remapping is now implemented in the high-level Rust, Go, Node.js, and Python `Log` writers. Low-level single-file writers remain strict.
- Strict/default cross-mode migration is implemented in Rust, Go, Node.js, and Python: when strict systemd naming opens a directory containing a stale chain-named `ONLINE` active file, the writer archives that chain file before creating `<source>.journal`, preserving sequence continuity and avoiding parallel active files.
- High-level `Log` instances are documented as single-writer mutable objects. Callers must serialize method calls on one instance; SDK lockfiles protect the one-writer file contract across cooperating SDK instances/processes without adding hidden per-append mutex cost.

### Acceptance Criteria

- Rust, Go, Node.js, and Python expose documented high-level ingestion writer contracts usable by Netdata without copying SDK internals into Netdata.
- The contract is a superset of the current NetFlow and OTEL logs writer behavior: directory-owned journal files, source/prefix selection, machine-id directory layout, size/count/time rotation, file-count/byte/age retention, binary fields, strict source validation, and stock-reader compatibility.
- Writer creation exposes a synchronous preflight mode that fails before consumer/job acceptance when any configured creation-time check fails: invalid source/name, invalid directory path, directory creation failure, active journal create/open failure when eager open is requested, writer lock failure, invalid compression/sealing options, invalid identity options in strict mode, and retention preflight errors.
- Lazy open remains supported only if it is explicitly documented and not used by consumers that require creation-time active-file validation.
- The API has a strict identity mode, or equivalent validated constructor, where missing or malformed required machine ID and boot ID are returned as errors instead of becoming random IDs.
- The API exposes all relevant paths explicitly: configured root, machine-id journal directory, active file path, archived file paths as they are created, and the exact directory that stock `journalctl --directory` should use.
- Migration compatibility is mandatory: for every existing Netdata consumer in scope, the SDK-backed configuration must resolve to the same effective on-disk journal chain as the current implementation, including machine-id path handling, without requiring users to edit configuration.
- The API must make machine-id ownership explicit. The default Netdata-compatible mode must match the existing Rust writer behavior: the consumer supplies the base stream/tier directory, and the SDK loads the machine ID and appends `<machine-id>` internally.
- The API must not force existing Netdata consumers to start including `<machine-id>` in configured paths if they do not do that today.
- The default layout preserves the systemd-compatible `configured-dir/<machine-id>/` behavior used by the existing Rust writer. Any flat/direct directory mode must be a first-class, tested compatibility mode, not an accidental side effect, and must not be used for existing Netdata consumer migration unless evidence shows that consumer already writes flat directories.
- Writer construction or explicit open/preflight scans existing SDK-owned journal files in the effective machine-id directory and initializes chain state from disk: total retained journal bytes, tail sequence number, tail realtime timestamp, tail monotonic timestamp for the current boot, active-file identity if one exists, and the exact naming convention already present. Migration must not create a parallel `system.journal` active file when an existing Rust-format `system` + `@` + sequence metadata journal chain is the current Netdata chain.
- The default high-level active filename policy in all four languages is Netdata chain naming, matching the existing Netdata vendored Rust writer: `<source>@<seqnum_id>-<head_seqnum>-<head_realtime>.journal`.
- `strict_systemd_naming` is a cross-language opt-in flag. When it is false or unset, all SDKs use Netdata chain naming. When it is true, all SDKs use strict systemd active naming with `<source>.journal` as the active file and `<source>@...journal` as archived files.
- Retention must track the active file explicitly and skip it in every language, because Netdata chain naming makes an active file look archive-named to simple filename parsers.
- Rotation supports all required limits: maximum active file size, maximum entries per file, and maximum active file duration. Each limit must be independently nullable/optional so consumers can disable individual checks without overloading zero values.
- Retention supports all required limits: maximum tracked journal file count, maximum total retained bytes, and maximum retained age. The tracked active/current file counts toward count and byte envelopes but must never be selected for deletion. Each limit must be independently nullable/optional so consumers can disable individual checks without overloading zero values.
- Retention never deletes the tracked active file, even when configured byte/count limits are lower than the active file size or count envelope. Implementations must track the active file explicitly and must not rely only on filename/status parsing, because the existing Rust writer creates active files using the `system` + `@` + sequence metadata journal-name pattern.
- Retention deletion scope is limited to SDK-owned journal files for the configured directory/source/prefix/machine-id chain. Unrelated journal files, unrelated sources, disposed files outside the supported lifecycle, and non-journal files are preserved unless the API explicitly documents another tested mode.
- Retention does not directly delete Netdata side artifacts such as `decoder-state.d`, `facet-state.bin`, or per-journal facet sidecars. Journal lifecycle events must give consumers the created, archived, and deleted journal paths they need to update or delete their own side artifacts.
- Size accounting must support consumer-owned per-journal artifacts. The API must provide a hook, callback, sidecar-size provider, or equivalent accounting surface so consumers can include artifact bytes associated with each journal path in size-based decisions.
- The artifact accounting contract must be concrete before implementation: it must define provider/registration shape, lookup key, call timing, missing-artifact behavior, error behavior, caching rules, and whether active-file artifacts are included in active rotation preflight or only retained-total enforcement.
- Artifact-inclusive size accounting must apply to total retained byte enforcement and any API-provided rotation-size derivation or preflight budget calculation. If active-file `MaxFileSize` remains strictly journal-bytes-only by default, the API must document that default and provide a tested mode for consumers that need external artifact bytes to make rotation happen earlier.
- Policy validation distinguishes disabled limits from invalid values. Enabled size/count/time limits must reject ambiguous or unsafe values unless the API provides an explicit named mode for that behavior.
- Duration rotation must be real behavior, not just a configuration field. The clock model must be documented; the Netdata-compatible default measures the active file span from active file head/first-entry realtime or an equivalent persisted head timestamp, checks it before append, and rotates once the configured duration is exceeded while size and count limits are disabled. Tests must use a deterministic clock or timestamp override.
- Retention execution timing must be explicit. If retention is coupled to rotation in the default writer path, the API must also provide an explicit `EnforceRetention`, `Tick`, or equivalent call so consumers can enforce age/size/count retention when no append-triggered rotation occurs.
- The API can reproduce NetFlow's current policy model: per-tier retention, `10GB / 7d` defaults, `null` size or duration to disable that limit, no file-count retention by default, at least one positive limit per tier, minimum enabled size retention of `100MB`, rotation size derived as `clamp(size_of_journal_files / 20, 5MB, 200MB)`, `100MB` rotation size when size retention is disabled, and hardcoded `1h` internal duration rotation.
- The API can reproduce OTEL logs' current policy model: one logs writer, rotation by `size_of_journal_file`, `entries_of_journal_file`, and `duration_of_journal_file`, plus retention by `number_of_journal_files`, `size_of_journal_files`, and `duration_of_journal_files`.
- The API supports optional lifecycle hooks, callbacks, or equivalent observable results for every journal file creation, archive/rotation, and deletion so Netdata consumers that maintain auxiliary indexes, such as NetFlow facets, can update those indexes without polling. OTEL does not wire lifecycle observers today, so the observer surface must be omissible. Active-file creation events must include the new active path and creation reason. Archive/rotation events must include the previous active path, resulting archived path, and replacement active path, so both same-path and rename-based implementations are representable. Retention deletion events must include every deleted journal path, either as per-file callbacks or a batch with full paths.
- Active-file creation events are new API capability, not current Rust writer parity. Current NetFlow infers the active path after append; the SDK still needs creation events so consumers are not forced to infer lifecycle state from append side effects.
- Lifecycle callback error behavior must be explicit and tested. Netdata-compatible default behavior must match the current Rust observer model: lifecycle notifications are best-effort, happen after the journal operation point they describe, report/log callback failures, and do not roll back completed journal operations. A stricter fail-on-callback-error mode may exist only as an explicit tested mode.
- The API exposes the active journal path immediately after a successful append so NetFlow-style consumers can index the record against the exact active file that received it.
- The append API supports both current consumer paths: OTEL-style `WriteEntry(items, sourceRealtime)` and NetFlow-style `WriteEntryWithTimestamps(items, timestamps)` with source realtime, entry realtime, and optional entry monotonic overrides. Timestamp handling must preserve strict journal ordering by clamping or rejecting non-progressing values in a documented way.
- The writer preserves append order. Entries must be written to a journal file in the order the consumer appends them; OTEL sorts entries before calling the writer and the writer must not reorder them internally.
- The API keeps sync strategy under consumer control. Consumers must be able to call `Sync` per batch, periodically, on threshold, and during shutdown without the SDK forcing one Netdata plugin's durability cadence on another.
- Close/drop behavior is explicit and tested, including lazy-open no-entry close, eager-open empty active file close, non-empty active file close, and best-effort shutdown behavior. The Netdata-compatible default archives and syncs any active file on close/drop instead of deleting it, matching the Rust writer's `Drop` behavior for active files. The default migration mode must not delete or hide existing journals.
- The concurrency contract is explicit. The writer must either be safe for concurrent appends or clearly require external serialization, with tests covering the selected contract.
- The default source/prefix for Netdata migration remains the `system` source with system-prefixed active and archived journal filenames. Configurable source support may exist only as an additive mode and must not change existing NetFlow or OTEL paths by default.
- `Origin` or equivalent metadata must not silently override the machine-id path ownership model. If the SDK owns machine-id path expansion, consumer-provided origin metadata cannot make the writer choose a different directory unless an explicit tested mode says so.
- Query-only plugin settings such as NetFlow `query_max_groups` are out of scope for the writer API and must not leak into retention/rotation behavior.
- The API supports binary field values and normal string fields through the same append path.
- The API supports large string fields needed by OTEL's optional `OTLP_JSON` field without truncation, unsafe copying, or reader incompatibility.
- The API supports Rust-writer-compatible remapping for non-systemd-compatible field names, including OTEL dotted field names such as `log.time_unix_nano`. The writer must either emit compatible `ND_REMAPPING=1` entries and remapped `ND_*` field names, or provide a documented equivalent that preserves stock journal compatibility and SDK reader query compatibility.
- Stock `journalctl --directory` and `journalctl --verify` validation pass against generated active and archived files, including the documented query directory.
- Tests include synthetic NetFlow-shaped, OTEL-log-shaped, and trap-shaped fixtures with representative fields, binary payload coverage, dotted OTEL field names, and large single-field `OTLP_JSON` payload coverage without using real customer, environment, community string, endpoint, or incident data.
- Tests cover nullable policy behavior, invalid enabled values, minimum-size and file-count edge cases, active-file survival under impossible retention limits, source/prefix retention scoping, default Netdata chain naming in Rust, Go, Node.js, and Python, opt-in strict systemd active naming in Rust, Go, Node.js, and Python, scan-and-resume of existing Rust-format `system` + `@` + sequence metadata journal chains, prevention of parallel `system.journal` chain creation in Netdata-compatible mode, reopen/interruption behavior, eager preflight failures, lazy-open behavior where kept, create/archive/delete lifecycle notifications, side-artifact preservation, artifact-inclusive size accounting, append timestamp overrides, active-path exposure after append, field-name remapping, and consumer-controlled sync cadence.
- Initial non-gating benchmark hooks or smoke benchmarks are available for this API path, while the broad benchmark/profile/optimize pass remains owned by SOW-0009.

## Analysis

Sources checked:

- `SOW-status.md`
- `.agents/sow/pending/SOW-0009-20260523-benchmark-profile-optimize.md`
- `.agents/sow/pending/SOW-0020-20260524-directory-traversal-parity.md`
- `.agents/sow/pending/SOW-0022-20260525-compatibility-test-gap-audit.md`
- `.agents/sow/current/SOW-0019-20260524-forward-secure-sealing.md`
- `.agents/sow/done/SOW-0013-20260523-go-directory-writer-rotation-retention.md`
- `go/journal/log.go`
- `go/journal/writer.go`
- `go/journal/log_test.go`
- `ktsaou/netdata @ 00305266364e`
  - `src/crates/journal-log-writer/src/log/config.rs`
  - `src/crates/journal-log-writer/src/log/mod.rs`
  - `src/crates/journal-log-writer/src/log/chain.rs`
  - `src/crates/journal-registry/src/repository/collection.rs`
  - `src/crates/netflow-plugin/src/plugin_config/types/journal.rs`
  - `src/crates/netflow-plugin/src/plugin_config/validation/journal.rs`
  - `src/crates/netflow-plugin/src/plugin_config/defaults.rs`
  - `src/crates/netflow-plugin/src/plugin_config/runtime.rs`
  - `src/crates/netflow-plugin/src/ingest/service/init.rs`
  - `src/crates/netflow-plugin/src/ingest/service.rs`
  - `src/crates/netflow-plugin/configs/netflow.yaml`
  - `src/crates/netdata-otel/otel-plugin/src/plugin_config/logs.rs`
  - `src/crates/netdata-otel/otel-plugin/src/logs_service.rs`
  - `src/crates/netdata-otel/otel-plugin/configs/otel.yaml.in`
- Live local cache inspection on 2026-05-25:
  - `/var/cache/netdata/flows/{raw,1m,5m,1h}/[machine-id]/` exists, where `[machine-id]` matches the host machine ID.
  - The newest sampled journal files were under the machine-id child directories, not directly under the tier roots.
  - Older flat tier-root journal files also exist under `/var/cache/netdata/flows/{raw,1m,5m,1h}/`; migration must not delete or obscure them accidentally, but the SDK-compatible current write target is the machine-id child path.

### Integration Analysis - 2026-05-26

External source reference:

- `ktsaou/netdata @ 00305266364e`

Confirmed production writer consumers:

- NetFlow plugin:
  - `src/crates/netflow-plugin/Cargo.toml:27` depends on `journal-log-writer`.
  - `src/crates/netflow-plugin/src/ingest.rs:24` imports `Config`, `EntryTimestamps`, `Log`, `RetentionPolicy`, and `RotationPolicy`.
  - `src/crates/netflow-plugin/src/ingest/service/init.rs:30` builds one policy closure per tier.
  - `src/crates/netflow-plugin/src/ingest/service/init.rs:103` constructs the raw writer with `Log::new(&raw_dir, ...)`.
  - `src/crates/netflow-plugin/src/ingest/service/init.rs:124`, `:132`, and `:140` construct materialized tier writers with the same `Log::new` shape.
  - `src/crates/netflow-plugin/src/ingest/encode.rs:25` and `:87` encode directly into a `journal_log_writer::Log`; this makes the high-level `Log` type part of the natural integration point, not an implementation detail hidden behind another NetFlow-local abstraction.
  - `src/crates/netflow-plugin/src/ingest/service/runtime.rs:138` uses `EntryTimestamps` with source realtime and entry realtime.
  - `src/crates/netflow-plugin/src/ingest/service/runtime.rs:153` reads the active file path immediately after append.
  - `src/crates/netflow-plugin/src/ingest/service/runtime.rs:191` controls raw sync cadence by entry threshold and timer.
  - `src/crates/netflow-plugin/src/ingest/service.rs:19` implements `LogLifecycleObserver` and consumes concrete rotated/deleted journal paths.
- OTEL logs plugin:
  - `src/crates/netdata-otel/otel-plugin/Cargo.toml:35` depends on `journal-log-writer`.
  - `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:4` imports `Config`, `Log`, `RetentionPolicy`, and `RotationPolicy`.
  - `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:24` builds rotation policy directly from plugin config.
  - `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:29` builds retention policy directly from plugin config.
  - `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:44` constructs a single `Arc<Mutex<Log>>`.
  - `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:78` creates `Vec<Vec<u8>>` journal items from OTLP JSON.
  - `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:155` writes a batch item with `write_entry(&entry_refs, source_timestamp_usec)`.
  - `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:170` syncs once per exported batch.

Non-consumers:

- `journal-log-writer` tests and examples exercise the API but are not product integration surfaces.
- `journal-registry`, `journal-engine`, `journal-index`, and log-viewer crates consume journal files or registry metadata, not the writer API.
- A scoped search of existing SNMP Go collector code found no journal writer or trap writer integration yet. Existing SNMP references are polling/topology/docs, so the trap writer can be treated as a future Go consumer rather than a migration source.

Design conclusion:

- A separate top-level `IngestionWriter` layer would not look natural for the existing Netdata integrations. The production consumers already treat the high-level directory writer as the integration type: construction returns `Log`, encode buffers accept `Log`, lifecycle observers attach to `Log`, sync is called on `Log`, and active path is read from `Log`.
- The natural SDK design is therefore to evolve the high-level Go `Log` API into the Netdata-compatible directory writer, while keeping the low-level single-file `Writer` unchanged as the journal-object primitive.
- Backward compatibility can be preserved by keeping existing `NewLog(dir, LogConfig)` call sites and adding strictly additive options, builders, and methods. If a stricter constructor is needed, it should return the same `*Log` type rather than a separate wrapper type.
- The public API should avoid Netdata-specific names. The behavior is generic directory journal writing with explicit layout, identity, lifecycle, retention, and artifact-accounting policy.

Recommended cross-language API direction:

- Keep `Log` as the high-level directory writer type in every language.
- Keep `Writer` as the low-level single-file primitive in every language.
- Extend `LogConfig` and policy builders rather than introducing a separate `IngestionWriter` type.
- Change policy internals to represent nullable limits explicitly. Nil/None/undefined means disabled; enabled zero is invalid during validation.
- Add strict/eager configuration to the high-level log config, but return the same `Log` type:
  - identity policy: strict required IDs vs host fallback vs random fallback;
  - open mode: lazy vs eager preflight;
  - layout: default machine-id child directory, with flat/direct only as explicit mode;
  - naming mode: default Netdata chain naming using `source@seqnum_id-head_seqnum-head_realtime.journal`, plus `strict_systemd_naming=true` for `source.journal` active naming only as explicit compatibility mode;
  - field-name policy: strict systemd names vs Rust-compatible remapping;
  - lifecycle observer and callback error policy;
  - artifact size provider.
- Add `AppendWithTimestamps` or extend `Append` options with source realtime, entry realtime, and entry monotonic override. Preserve NetFlow's post-append active-path read by guaranteeing `ActivePath()` returns the file that received the last successful append.
- Add a raw item append path for high-throughput encoders, because NetFlow currently writes stack-backed `KEY=value` slices directly and OTEL builds `Vec<Vec<u8>>` before writing. The low-level `Writer` can remain field-based; the high-level `Log` should support both `[]Field` and raw `KEY=value` items with the same remapping/timestamp path.
- Make high-level `Log` safe for concurrent method calls unless implementation evidence shows the mutex materially harms the Netdata ingestion path. Existing OTEL already wraps `Log` in a mutex; a Go `Log` that serializes internally is natural for future trap receivers and avoids pushing one-writer serialization into every consumer.
- Lifecycle callbacks should be synchronous and ordered with writer operations. The Netdata-compatible default should ignore callback errors after reporting them through an optional error hook; fail-on-callback-error can exist only as an explicit tested mode.
- Artifact accounting should be a provider/callback tied to journal paths, not part of lifecycle callbacks. The provider should be called during scan/preflight and retention accounting. Missing artifacts should count as zero; unexpected provider errors should fail strict preflight or retention enforcement.

Current state:

- `SOW-status.md` lists SOW-0019 as current and SOW-0022, SOW-0020, and SOW-0009 as pending.
- SOW-0009 is a final benchmark/profile/optimize pass after feature completeness; it is not an ingestion API SOW.
- SOW-0020 is directory traversal parity for SDK readers and file-backed journalctl behavior; it is relevant to query behavior but does not define a Netdata writer API.
- SOW-0022 is a compatibility-gap audit; it records gaps but does not implement the Netdata writer API.
- SOW-0013 completed a Go high-level directory writer with rotation and retention, but it did not settle the stricter Netdata job-creation contract.
- `go/journal/log.go` stores files below `dir/<machine-id>/` and exposes that child path via `JournalDirectory`.
- `go/journal/log.go` supports size and entry-count rotation, but not duration rotation.
- `go/journal/log.go` supports file-count and byte retention, but not age retention.
- `go/journal/log.go` normalizes missing IDs from host files and then falls through to random UUID generation via `normalizeOptions`.
- `go/journal/log_test.go` validates stock `journalctl --directory` against the machine-id subdirectory returned by the test helper.

Superset consumer analysis:

- Existing Netdata Rust writer policy shape:
  - `src/crates/journal-log-writer/src/log/config.rs:4` defines rotation by size, duration, and entry count.
  - `src/crates/journal-log-writer/src/log/config.rs:38` defines retention by file count, total size, and duration.
  - `src/crates/journal-log-writer/src/log/mod.rs:25` creates `configured-dir/<machine-id>/` as the actual journal chain directory.
  - `src/crates/journal-log-writer/src/log/mod.rs:25` loads the machine ID inside the writer, and `src/crates/journal-log-writer/src/log/mod.rs:39` appends that machine ID to the caller-provided path. This means the existing Rust writer, not the consumer config, owns the machine-id path component.
  - `src/crates/journal-log-writer/src/log/chain.rs:172` applies retention by deleting oldest files.
  - `src/crates/journal-registry/src/repository/collection.rs:62` prevents age retention from draining active files.
- NetFlow consumer needs:
  - `src/crates/netflow-plugin/src/plugin_config/types/journal.rs:11` defines the NetFlow journal config.
  - `src/crates/netflow-plugin/src/plugin_config/runtime.rs:14` resolves the configured `journal_dir` relative to `NETDATA_CACHE_DIR` when applicable.
  - `src/crates/netflow-plugin/src/plugin_config/types/journal.rs:187` appends tier names such as `raw`, `minute_1`, `minute_5`, and `hour_1` to the configured base directory.
  - `src/crates/netflow-plugin/src/ingest/service/init.rs:103` passes the tier directory to `Log::new`. It does not append `<machine-id>` before calling the writer; the writer appends machine-id internally.
  - `src/crates/netflow-plugin/src/plugin_config/types/journal.rs:54` defines per-tier nullable `size_of_journal_files` and `duration_of_journal_files`.
  - `src/crates/netflow-plugin/src/plugin_config/defaults.rs:87` defines `10GB` retention size defaults and `7d` retention duration defaults.
  - `src/crates/netflow-plugin/src/plugin_config/types/journal.rs:228` derives rotation size from the enabled retention size and falls back to `100MB` for time-only retention.
  - `src/crates/netflow-plugin/src/plugin_config/validation/journal.rs:26` requires each tier to have at least one positive retention limit and rejects enabled sizes below `100MB`.
  - `src/crates/netflow-plugin/src/ingest/service/init.rs:36` builds the Rust writer rotation policy per tier.
  - `src/crates/netflow-plugin/src/ingest/service/init.rs:39` builds the Rust writer retention policy per tier.
  - `src/crates/netflow-plugin/src/ingest/service.rs:19` observes rotation and retention deletion events for facet-runtime maintenance.
- OTEL logs consumer needs:
  - `src/crates/netdata-otel/otel-plugin/src/plugin_config/logs.rs:54` defines a single logs journal config with rotation size, entry count, rotation duration, retention file count, retention size, and retention duration.
  - `src/crates/netdata-otel/otel-plugin/src/plugin_config/logs.rs:117` sets defaults of `100MB`, `50000` entries, `10` files, `1GB`, `7d`, and `2h`.
  - `src/crates/netdata-otel/otel-plugin/configs/otel.yaml.in:36` stores logs under the packaged OTEL logs journal directory.
  - `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:41` passes `logs_config.journal_dir` directly to `Log::new`. It does not append `<machine-id>` before calling the writer; the writer appends machine-id internally.
  - `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:24` passes rotation size, duration, and entry count to `journal-log-writer`.
  - `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:29` passes file count, total size, and duration retention to `journal-log-writer`.
- Existing implementation gap to avoid copying blindly:
  - `src/crates/journal-log-writer/src/log/config.rs:12` exposes `duration_of_journal_file`.
  - `src/crates/journal-log-writer/src/log/mod.rs:59` tracks only size and count in `RotationState`.
  - `src/crates/journal-log-writer/src/log/mod.rs:73` checks only size and count in `should_rotate`.
  - Therefore SOW-0023 must implement and validate duration rotation as real behavior, not merely expose a configuration field.
- Machine-id migration conclusion:
  - Existing NetFlow and OTEL consumers provide base stream/tier directories. The writer appends `<machine-id>` internally.
  - SDK migration must preserve this exact ownership model by default: callers keep the same configured paths, and the SDK writes to the same effective `configured-dir/<machine-id>/` journal chain.
  - Any API mode where the caller supplies the already-expanded machine-id directory must be explicit and must not become the default for Netdata migrations.

External reviewer gap synthesis on 2026-05-25:

- Reviewers run:
  - `llm-netdata-cloud/glm-5.1`
  - `llm-netdata-cloud/kimi-k2.6`
  - `llm-netdata-cloud/qwen3.6-plus`
  - `llm-netdata-cloud/minimax-m2.7-coder`
- Confirmed gap: duration rotation needs precise implementation and tests.
  - Current Netdata Rust policy exposes `duration_of_journal_file` at `src/crates/journal-log-writer/src/log/config.rs:12`.
  - Current Netdata Rust rotation state tracks only size and count at `src/crates/journal-log-writer/src/log/mod.rs:59` and checks only size/count at `src/crates/journal-log-writer/src/log/mod.rs:73`.
  - Current Go SDK rotation policy has only `MaxFileSize` and `MaxEntries` at `go/journal/log.go:16`.
  - SOW consequence: duration rotation must be implemented as observable behavior, using active-file head/first-entry time or an equivalent documented clock model, and validated with size/count disabled.
- Confirmed gap: age retention needs Go API coverage.
  - Current Netdata Rust retention policy includes `duration_of_journal_files` at `src/crates/journal-log-writer/src/log/config.rs:48`.
  - Current Go SDK retention policy has only `MaxFiles` and `MaxBytes` at `go/journal/log.go:38`.
  - SOW consequence: Go retention must support nullable age limits independently from count and bytes.
- Confirmed gap: append timestamp controls are required.
  - Current Netdata Rust writer exposes `EntryTimestamps` with source realtime, entry realtime, and entry monotonic fields at `src/crates/journal-log-writer/src/log/mod.rs:204`.
  - Current Netdata Rust writer clamps realtime/monotonic progression in `capture_dual_timestamp` at `src/crates/journal-log-writer/src/log/mod.rs:237`.
  - NetFlow raw writes pass both source realtime and entry realtime at `src/crates/netflow-plugin/src/ingest/service/runtime.rs:138`.
  - NetFlow tier writes pass source realtime and entry realtime at `src/crates/netflow-plugin/src/ingest/service/tiers.rs:93`.
  - OTEL extracts source timestamps from OTLP fields at `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:78` and passes them to `write_entry` at `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:155`.
  - SOW consequence: SDK append must support source realtime injection and entry timestamp overrides, not only low-level writer realtime options.
- Confirmed gap: active-file path exposure after append is required by NetFlow facets.
  - NetFlow raw writes call `active_file()` after appending and pass the path into facet observation at `src/crates/netflow-plugin/src/ingest/service/runtime.rs:153`.
  - NetFlow tier writes do the same at `src/crates/netflow-plugin/src/ingest/service/tiers.rs:107`.
  - Current Rust writer exposes `active_file()` at `src/crates/journal-log-writer/src/log/mod.rs:558`.
  - Current Go SDK exposes `ActivePath()` at `go/journal/log.go:248`, but the contract must guarantee it maps to the actual post-append file.
  - SOW consequence: active path must be observable immediately after append and must survive rotation decisions.
- Confirmed gap: lifecycle events must include concrete paths for side indexes.
  - Current Rust events include rotated archived/active files and retained deleted files at `src/crates/journal-log-writer/src/log/mod.rs:189`.
  - NetFlow consumes those paths to update facets at `src/crates/netflow-plugin/src/ingest/service.rs:19`.
  - SOW consequence: lifecycle notifications cannot be generic counters; they must expose the actual affected journal paths.
- Confirmed gap: NetFlow keeps archive-time per-journal-file side artifacts and the API still needs complete file lifecycle hooks.
  - NetFlow writes facet sidecars using the journal path as the sidecar basename at `src/crates/netflow-plugin/src/facet_runtime/sidecar.rs:11` and `src/crates/netflow-plugin/src/facet_runtime/sidecar.rs:33`.
  - NetFlow deletes those sidecars from the same journal path at `src/crates/netflow-plugin/src/facet_runtime/sidecar.rs:26`.
  - NetFlow records active contributions in memory under the active journal path at `src/crates/netflow-plugin/src/facet_runtime.rs:279`.
  - NetFlow writes per-file sidecars when an active contribution is promoted to an archived journal file at `src/crates/netflow-plugin/src/facet_runtime.rs:302`.
  - NetFlow deletes sidecars for retained/deleted journal paths at `src/crates/netflow-plugin/src/facet_runtime.rs:327`.
  - SOW consequence: NetFlow currently creates sidecar files on archive, not active-file creation. The SDK API still needs create, archive, and delete hooks with concrete paths so NetFlow can handle archive/delete exactly and future or existing consumers are not forced to infer file creation from first append or rotation side effects.
- Confirmed gap: consumer artifacts must be available to size accounting.
  - Current Rust chain initialization totals only journal file sizes by calling `metadata(file.path()).len()` for repository journal files at `src/crates/journal-log-writer/src/log/chain.rs:74` and `src/crates/journal-log-writer/src/log/chain.rs:83`.
  - Current Rust total-size retention compares only that tracked journal total with `size_of_journal_files` at `src/crates/journal-log-writer/src/log/chain.rs:194`.
  - NetFlow creates sidecar files separately from the journal file by creating a temporary sidecar and renaming it into the per-journal sidecar path at `src/crates/netflow-plugin/src/facet_runtime/sidecar.rs:104` and `src/crates/netflow-plugin/src/facet_runtime/sidecar.rs:124`.
  - NetFlow derives tier rotation size from `size_of_journal_files / 20`, clamped to `5MB..200MB`, at `src/crates/netflow-plugin/src/plugin_config/types/journal.rs:228`.
  - SOW consequence: size-based retention, rotation-size derivation, and any artifact-aware rotation mode need a way to account for consumer-owned bytes associated with a journal path, otherwise configured storage limits can be exceeded by sidecars even when journal files alone satisfy the limits.
- Confirmed gap: retention must not remove adjacent Netdata state.
  - NetFlow keeps decoder state under `decoder-state.d` at `src/crates/netflow-plugin/src/plugin_config/types/journal.rs:246` and creates/preloads it at `src/crates/netflow-plugin/src/ingest/service/init.rs:61`.
  - NetFlow keeps facet state in `facet-state.bin` at `src/crates/netflow-plugin/src/facet_runtime.rs:37`.
  - NetFlow sidecars are named from the journal path as `.facet.<FIELD>.fst` at `src/crates/netflow-plugin/src/facet_runtime/sidecar.rs:33`.
  - SOW consequence: SDK retention may delete only SDK-owned journal files in scope and must report deleted journal paths so consumers can clean sidecars themselves.
- Confirmed gap: sync cadence differs by consumer.
  - OTEL syncs after each export batch at `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:170`.
  - NetFlow syncs on interval/threshold/shutdown paths at `src/crates/netflow-plugin/src/ingest/service/runtime.rs:13`, `src/crates/netflow-plugin/src/ingest/service/runtime.rs:183`, and `src/crates/netflow-plugin/src/ingest/service/runtime.rs:168`.
  - SOW consequence: sync must remain a consumer-controlled operation, not a hardcoded SDK policy.
- Confirmed gap: close/drop behavior must be specified for migration.
  - Current Rust `Drop` archives and best-effort syncs an existing active file at `src/crates/journal-log-writer/src/log/mod.rs:745`.
  - Current Go `Close` deletes an empty active file at `go/journal/log.go:199`.
  - SOW consequence: lazy-open and eager-open empty-file behavior need explicit tests so creation preflight does not introduce unexpected data loss or invisible directories.
- Confirmed gap: source and origin semantics need narrowing.
  - Current Rust writer loads machine-id in `create_chain` and appends it to the supplied path at `src/crates/journal-log-writer/src/log/mod.rs:25` and `src/crates/journal-log-writer/src/log/mod.rs:39`.
  - Current Rust chain names files with a hardcoded `system` source prefix at `src/crates/journal-log-writer/src/log/chain.rs:23`.
  - Current Rust `Config` stores `origin` at `src/crates/journal-log-writer/src/log/config.rs:75`, but no `journal-log-writer` runtime code uses `config.origin` outside construction/storage.
  - Current Go SDK defaults empty source to `system` at `go/journal/log.go:98` and builds active/archive names from that source at `go/journal/log.go:392`.
  - SOW consequence: Netdata migration default must remain the `system` source/prefix; any source configurability is additive and cannot change machine-id ownership by accident.
- Confirmed gap: concurrency contract must be explicit.
  - Current Go SDK documents `Log` as not safe for concurrent calls at `go/journal/log.go:68`.
  - OTEL wraps `Log` in `Arc<Mutex<Log>>` at `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:15` and locks before writing at `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:156`.
  - SOW consequence: the SDK must either provide internal serialization or clearly require callers to serialize writes.
- Confirmed non-writer setting: NetFlow `query_max_groups` belongs to query protection, not the writer.
  - NetFlow defines `query_max_groups` at `src/crates/netflow-plugin/src/plugin_config/types/journal.rs:42` and validates it at `src/crates/netflow-plugin/src/plugin_config/validation/journal.rs:5`.
  - SOW consequence: this setting should be documented as out of scope for the writer API.
- Second read-only reviewer round on 2026-05-25 found additional API requirements that were missing or under-specified:
  - Active file naming and resume are migration-critical. Current Netdata Rust writer creates active files with the `system` + `@` + sequence metadata journal-name pattern at `src/crates/journal-log-writer/src/log/chain.rs:23` and does not rename them on `Drop` at `src/crates/journal-log-writer/src/log/mod.rs:745`. Current Go `Log` uses `system.journal` for the active file at `go/journal/log.go:392`. SOW consequence: Netdata migration mode must scan and resume the existing Rust-format chain, or use a compatible naming mode, instead of creating a parallel Go-style active file.
  - Existing chain scanning is required for correctness. Current Rust `OwnedChain::new` scans files and tracks sizes at `src/crates/journal-log-writer/src/log/chain.rs:74`, and `Log::new` initializes tail sequence, realtime clock, and monotonic state at `src/crates/journal-log-writer/src/log/mod.rs:265`, `src/crates/journal-log-writer/src/log/mod.rs:271`, and `src/crates/journal-log-writer/src/log/mod.rs:274`. SOW consequence: startup/preflight cannot initialize a blank chain when existing files are present.
  - OTEL writes dotted field names. OTEL emits fields directly from JSON keys such as `log.time_unix_nano` at `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:63` and builds `key=value` pairs at `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:118`. Current Rust writer remaps incompatible field names at `src/crates/journal-log-writer/src/log/mod.rs:345` and emits remapping entries at `src/crates/journal-log-writer/src/log/mod.rs:502`. Current Go field validation rejects non-uppercase/underscore names at `go/journal/writer.go:465`. SOW consequence: the SDK ingestion API must include Rust-compatible field-name remapping or an equivalent tested compatibility mechanism.
  - NetFlow retention has no file-count setting. NetFlow retention config only exposes nullable size and duration at `src/crates/netflow-plugin/src/plugin_config/types/journal.rs:54`, and `build_journal_cfg` only sets size and duration retention at `src/crates/netflow-plugin/src/ingest/service/init.rs:40`. SOW consequence: NetFlow migration must leave file-count retention disabled by default.
  - OTEL lifecycle observers are not wired. OTEL constructs `Log::new` without `with_lifecycle_observer` at `src/crates/netdata-otel/otel-plugin/src/logs_service.rs:44`, while NetFlow wires the observer at `src/crates/netflow-plugin/src/ingest/service/init.rs:104`. SOW consequence: lifecycle hooks must be optional.
  - Current Rust lifecycle events do not include creation events. `LogLifecycleEvent` contains `Rotated` and `RetainedDeleted` at `src/crates/journal-log-writer/src/log/mod.rs:190`, and initial active creation returns no event at `src/crates/journal-log-writer/src/log/mod.rs:608`. SOW consequence: create events are an additive SDK capability needed for a complete lifecycle API, not current Rust parity.
  - Active-file retention protection must be explicit. Current Rust file-count and byte retention delete via `delete_oldest_file()` at `src/crates/journal-log-writer/src/log/chain.rs:179` and `src/crates/journal-log-writer/src/log/chain.rs:194`, which ultimately pops the front file at `src/crates/journal-log-writer/src/log/chain.rs:232`. SOW consequence: the SDK must explicitly skip the tracked active file for all retention checks, including enabled-zero edge cases.
  - Current Rust observer callbacks are best-effort. The trait returns `()` at `src/crates/journal-log-writer/src/log/mod.rs:200`, the writer ignores callback failures by construction at `src/crates/journal-log-writer/src/log/mod.rs:583`, and NetFlow logs observer errors instead of failing the journal operation at `src/crates/netflow-plugin/src/ingest/service.rs:19`. SOW consequence: Netdata-compatible default callback semantics should be best-effort with observable errors, not rollback semantics.
  - Close/drop migration behavior must match Rust for active files. Current Rust `Drop` archives and syncs an active file at `src/crates/journal-log-writer/src/log/mod.rs:745`; current Go `Close` deletes an empty active file at `go/journal/log.go:208`. SOW consequence: Netdata-compatible mode must not delete active files by default.
  - Retention execution timing must be clear. Current Rust retention is invoked from `rotate()` at `src/crates/journal-log-writer/src/log/mod.rs:580`; if no append-triggered rotation occurs, age retention has no independent trigger. SOW consequence: the API must document this timing and provide an explicit enforcement call if consumers need retention without rotation.

Risks:

- If Netdata accepts a job before the SDK has opened or created the journal writer successfully, users will see dynamic configuration apply succeed and only later see runtime log failures.
- If missing identity inputs silently become random UUIDs, a misconfigured Netdata job can look healthy while producing unexpected journal layout and query behavior.
- If the SDK exposes only one path, callers may confuse the configured base/tier directory with the effective machine-id journal directory. The API must expose both names clearly.
- If retention cannot enforce the required age or byte limits at the SDK layer, Netdata may need extra cleanup code, which defeats the purpose of sharing the writer.
- If this SOW is implemented while SOW-0019 FSS writer changes are still active, merge conflicts or incorrect sealing option propagation are likely.
- If nullable policy values are represented as zero-valued integers, callers cannot safely distinguish "disabled" from "enabled with zero"; this is especially dangerous for file-count retention.
- If file-count retention can delete the active file when configured to zero or one under edge conditions, the writer violates the live one-writer/multiple-reader contract and risks data loss.
- If NetFlow's facet lifecycle notifications are omitted, the SDK may write correct journal files while Netdata's auxiliary facet/index state becomes stale.

## Pre-Implementation Gate

Status: ready for first implementation slice

Problem / root-cause model:

- The SDK has generic writer primitives, but the Netdata integration needs a shared high-level ingestion writer contract in all four languages. The root issue for this slice is that current SDK high-level active naming drifted toward systemd active filename policy in Rust and Go, while the Netdata compatibility reference is the vendored Rust chain filename policy. Broader lifecycle semantics remain part of later SOW-0023 slices.

Evidence reviewed:

- `SOW-status.md`: current and pending work list shows no Netdata ingestion API SOW before this one.
- `.agents/sow/done/SOW-0013-20260523-go-directory-writer-rotation-retention.md`: completed high-level Go directory writer baseline.
- `go/journal/log.go`: current high-level directory writer API, directory layout, rotation, retention, and identity normalization behavior.
- `go/journal/writer.go`: low-level writer options, field model, append API, sync, close, and random UUID fallback.
- `go/journal/log_test.go`: stock `journalctl --directory` tests against the current machine-id directory shape.
- `ktsaou/netdata @ 00305266364e src/crates/journal-log-writer/src/log/chain.rs:23`: vendored Rust high-level writer uses `system@<seqnum_id>-<head_seqnum>-<head_realtime>.journal`.
- `rust/src/crates/journal-log-writer/src/log/chain.rs:15`: current SDK Rust high-level writer uses `system.journal`, which diverges from the vendored Rust reference.
- `go/journal/log.go:392`: current SDK Go high-level writer uses `<source>.journal` active naming.
- `node/src/lib/directory-writer.js` and `python/journal/directory_writer.py`: current high-level writers expose the same `Log` concept and need the same naming contract.

Affected contracts and surfaces:

- Go public API in package `journal`.
- Rust public API in `journal-log-writer` and re-exported `journal` crate.
- Node.js public API exported from `node/src/index.js`.
- Python public API exported from `python/journal/__init__.py`.
- Go writer lifecycle and error semantics.
- Directory layout and the documented `journalctl --directory` path for SDK-generated logs.
- Rotation and retention policy semantics.
- Stock `journalctl` compatibility tests.
- Netdata ingestion plugin dependency contract.
- SDK SOW/spec documentation and status tracking.

Existing patterns to reuse:

- Keep `Writer` as the low-level file primitive.
- Build any Netdata-focused API on top of `Log` or a small wrapper around `Log`, rather than duplicating journal object writing.
- Mirror the same high-level `Log` naming behavior across Rust, Go, Node.js, and Python, using idiomatic option names but identical defaults.
- Reuse `Field`, `StringField`, `EntryOptions`, `RotationPolicy`, `RetentionPolicy`, `ActivePath`, and `JournalDirectory` where they already satisfy the contract.
- Reuse existing `journalctl` test helpers and add Netdata-shaped fixtures instead of relying on real operational data.

Risk and blast radius:

- Medium API risk: changes should add a clearer constructor or options rather than break existing `NewLog` users.
- Medium compatibility risk: active naming changes can affect stock `journalctl --directory` behavior and retention safety, especially because chain-named active files parse like archived files.
- Medium operational risk: retention mistakes can delete too much or leak disk space; tests must prove active files and unrelated files are preserved.
- Low security risk if fixtures remain synthetic; high sensitivity if real trap content or SNMP communities are copied into durable artifacts, so real data must not be used.
- Medium performance risk: the API will be on an ingestion path, but full optimization belongs to SOW-0009 after feature completeness.

Sensitive data handling plan:

- Use only synthetic journal fields and synthetic trap values in tests, docs, specs, SOWs, and prompts.
- Do not store SNMP community strings, customer trap payloads, private endpoints, non-private customer-identifying IPs, hostnames from real environments, or private operational logs in durable artifacts.
- Use placeholders such as `[REDACTED_SECRET]`, `[PRIVATE_ENDPOINT]`, and synthetic values when examples need sensitive-shaped data.

Implementation plan:

1. Record the resolved sequencing, implementation-routing, and cross-language naming decisions.
2. Implement the first shared contract slice: default Netdata chain naming plus opt-in `strict_systemd_naming` across Rust, Go, Node.js, and Python.
3. Add or update tests proving default chain naming, opt-in strict systemd active naming, and active-file retention protection in each language.
4. Decide the remaining public API shape and artifact accounting contract before later SOW-0023 slices.
5. Add a strict constructor or option set that performs all job-creation preflight synchronously.
6. Preserve the existing layout: callers pass the same configured base/tier directory and the SDK appends `<machine-id>` internally by default.
7. Implement duration rotation and age retention as first-class behavior, with tests proving active-file survival and scoped deletion.
8. Add Netdata-shaped journalctl and verify tests with synthetic fields and binary payloads.
9. Update specs, docs, status, and any project skills that become affected by the new durable API contract.

Validation plan:

- Run targeted Go tests for writer, log, retention, and new ingestion API coverage.
- Run stock `journalctl --directory` checks against the documented Netdata query directory while the writer is active and after close/rotation.
- Run stock `journalctl --verify` against active and archived files where stock verification supports the state being tested.
- Add creation-time failure tests for unwritable directories, invalid source, invalid strict identity inputs, active-file open/create failure, lock conflict, and retention preflight error.
- Add same-failure scans for ID fallback, retention deletion, directory selection, and runtime-only error paths.
- Run read-only reviewer rounds after implementation following this repository's orchestration rules.

Artifact impact plan:

- AGENTS.md: likely unaffected unless this SOW discovers a general SDK process rule.
- Runtime project skills: update `project-journal-compatibility` if the Netdata ingestion writer contract becomes a recurring compatibility rule.
- Specs: update `.agents/sow/specs/product-scope.md` with the supported ingestion writer contract and query-directory behavior.
- End-user/operator docs: update README or package docs if this SDK documents public usage examples.
- End-user/operator skills: not expected to be affected because this SDK does not currently publish operator AI skills; record evidence at close.
- SOW lifecycle: this pending SOW was created because no existing pending SOW owned the Netdata ingestion writer API; close only after implementation, validation, artifact updates, and follow-up mapping.
- SOW-status.md: update pending list to include this SOW.

Open-source reference evidence:

- No external open-source repositories were checked for this SOW creation. The request was to classify the SDK's own pending SOWs and create the missing SDK SOW. Implementation should inspect systemd source/docs and relevant SDK-local evidence before code changes.

Open decisions:

1. Sequencing decision
   - Decision recorded: SOW-0019 was completed first, then SOW-0023 became active on 2026-05-26.
   - Implication: SOW-0023 can build on the completed FSS writer and verification changes without pausing or merging against an active cryptographic writer SOW.

2. Machine-id ownership and path compatibility decision
   - Decision recorded from user clarification: existing users must not lose journals, and migrated plugins must continue writing to the same directories without manual configuration changes.
   - Required behavior: keep the existing Netdata-compatible default where callers pass the base stream/tier directory and the SDK appends `<machine-id>` internally.
   - Implication: `ConfiguredDirectory()` and `JournalDirectory()` or equivalent APIs must both exist; `JournalDirectory()` is the effective `journalctl --directory` path for the machine-id child chain.
   - Risk to avoid: making callers include `<machine-id>` in configuration would create a second path and split existing journals.

3. Time-based policy decision
   - Decision recorded from user clarification: this SOW must be a superset of NetFlow, OTEL logs, and future consumers.
   - Required behavior: add duration rotation and age retention in this SOW.
   - Implication: existing Rust writer configuration shape is not enough evidence of working behavior; tests must prove the SDK enforces duration rotation.
   - Risk to avoid: leaving time policies as follow-up work would fail the superset replacement requirement and keep Netdata-side cleanup logic alive.

4. File-count edge-case decision
   - Decision recorded from user clarification and second-round evidence: nullable values disable limits; enabled zero is invalid unless a separate explicit named mode is intentionally added and tested.
   - Required behavior: the active file is always protected, file-count retention defaults to disabled for NetFlow migration, and tests cover enabled-zero rejection plus active-file survival under impossible limits.
   - Implication: consumers that want "keep zero archived files" need an explicit named mode instead of accidentally getting that behavior from a zero value.
   - Risk to avoid: a missing nullable wrapper or default zero value deleting every archived journal.

5. Implementation routing decision
   - Decision recorded from latest user instruction: implement locally in this repository and use external models only as read-only reviewers.
   - Implication: no external implementer prompts will be run for this SOW unless the user explicitly changes routing again.
   - Risk to avoid: stale delegation instructions causing a reviewer-only agent or external implementer to edit the repository.

6. Cross-language naming contract decision
   - Decision recorded from user clarification on 2026-05-26: all four languages must follow the same high-level writer rules and API shape, with Rust vendored Netdata behavior as the reference for existing plugin compatibility.
   - Required behavior: default and unset high-level writer naming is Netdata chain naming, `<source>@<seqnum_id>-<head_seqnum>-<head_realtime>.journal`, in Rust, Go, Node.js, and Python.
   - Required behavior: `strict_systemd_naming=false` or unset means Netdata chain naming; `strict_systemd_naming=true` means strict systemd active naming with `<source>.journal`.
   - Implication: the current SDK Rust and Go high-level writers must be corrected where they use `system.journal` as the default active file. Node.js and Python must be aligned to the same option name and default.
   - Risk to avoid: treating `system.journal` as the default would split existing Netdata journal chains and make SDK behavior diverge from the vendored Rust writer used by current Netdata plugins.

## Implications And Decisions

- 2026-05-25: User clarified that SOW-0023 is not SNMP-specific. It must define a superset Netdata ingestion writer API that can replace existing NetFlow and OTEL logs writer behavior and also satisfy future consumers.
- 2026-05-25: User clarified that existing users must not lose existing journals. Migrated plugins must keep writing to the same effective directories without manual configuration changes.
- 2026-05-25: Evidence shows existing NetFlow and OTEL logs consumers pass base stream/tier directories while the writer appends `<machine-id>` internally. The SDK migration default must preserve this machine-id ownership model.
- 2026-05-25: Live local cache evidence shows current `/var/cache/netdata/flows/{raw,1m,5m,1h}/[machine-id]/` directories matching the host machine ID, with the newest sampled journal files under those machine-id children. Older flat files exist and must not be accidentally deleted or hidden by migration cleanup.
- 2026-05-25: User clarified that if NetFlow keeps per-journal-file artifacts, the SDK API must provide hooks/callbacks on journal file creation, archive, and deletion so consumers can keep those artifacts synchronized with journal rotation. Evidence confirms NetFlow sidecar artifacts are per journal file, but NetFlow writes those sidecar files on archive, not on active-file creation.
- 2026-05-25: User clarified that per-journal-file artifacts also affect journal size-based rotation. The SDK API must provide a way for consumer-owned artifact bytes to be included in size calculations, including total retained bytes and any rotation-size derivation or artifact-aware active rotation mode.

## Plan

1. Confirm prioritization against active SOW-0019 and pending SOW-0020/SOW-0009.
2. Turn the Netdata API requirements into failing Go tests and public API documentation.
3. Implement the smallest additive Go API surface that satisfies creation-time validation, strict identity, query-directory clarity, append, sync, close, rotation, and retention needs.
4. Validate with stock `journalctl --directory`, stock `journalctl --verify`, targeted Go tests, and creation-failure tests.
5. Update product scope/specs, status, and any affected project skill or public docs.
6. Run external read-only review rounds and address findings without narrowing the review scope.

## Delegation Plan

Implementer:

- Implement locally in this repository per the latest user routing decision. Do not run external implementer agents for this SOW unless the user explicitly changes routing again.

Reviewers:

- Use independent read-only reviewers after implementation. Do not use any reviewer that is unavailable in the current user session. Prompts must include this SOW filename and ask for whole-SOW and whole-change review, unwanted side effects, security issues, and API compatibility risks.

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

- If the implementer cannot complete the SOW, record the exact blocker and either reassign or return to the user with evidence.
- If reviewers disagree, resolve by reproducing with tests or source evidence, not by majority vote.
- If the active FSS SOW conflicts with this SOW, pause before implementation and ask the user which SOW should own the conflict.
- If API requirements cannot be satisfied without breaking existing SDK users, return with numbered options and do not implement the breaking change without user approval.

## Execution Log

### 2026-05-25

- Created this pending SOW after reviewing the current and pending SDK SOW queue and the current Go directory writer API.
- Classified SOW-0009, SOW-0020, and SOW-0022 as related but insufficient for the Netdata ingestion writer API need.
- Ran read-only external gap review against SOW-0023 and `ktsaou/netdata @ 00305266364e` using `glm`, `kimi`, `qwen`, and `minimax`. Initial sandboxed runs could not reach the model endpoint; direct approved runs completed. Confirmed findings were incorporated into the acceptance criteria and analysis.
- Added explicit Netdata side-artifact requirements after local verification that NetFlow facet sidecars are per-journal-file artifacts written on archive and removed on journal deletion. The SOW now requires create/archive/delete lifecycle hooks and artifact-inclusive size accounting.
- Ran a second read-only reviewer round against the full SOW and `ktsaou/netdata @ 00305266364e` to search for additional API requirements. Confirmed findings were locally verified before incorporation. One reviewer session attempted nested external reviews despite prompt instructions; its output was treated as candidate evidence only and not accepted without direct source verification.

### 2026-05-26

- Promoted SOW-0023 from pending to current after the user explicitly asked to pick it up.
- Recorded that SOW-0019 is complete, so this SOW no longer competes with an active FSS SOW.
- Updated implementation routing to local implementation with external models used only as read-only reviewers.
- User clarified that the high-level writer contract is cross-language and that the existing Netdata vendored Rust writer is the reference behavior. Default naming must be Netdata chain naming in all four languages; strict systemd active naming is opt-in only.
- Implemented the first shared naming slice:
  - Rust `Config::with_strict_systemd_naming(true)` with default Netdata chain naming.
  - Go `LogConfig.StrictSystemdNaming` with default Netdata chain naming.
  - Node.js `strictSystemdNaming` / `strict_systemd_naming` with default Netdata chain naming.
  - Python `strict_systemd_naming` / `strictSystemdNaming` with default Netdata chain naming.
- Updated retention logic so chain-named active files are explicitly skipped during retention enforcement.
- Updated specs and README files to document default Netdata chain naming and opt-in strict systemd active naming.
- Ran the first read-only implementation review round with `glm`, `kimi`, `qwen`, and `minimax`.
- Fixed accepted reviewer findings:
  - Rust now derives active/archive filename source prefixes from `Config.origin.source` instead of hardcoding `system`.
  - Node.js no longer uses `||` defaults for rotation/retention limits, so explicit zero remains observable and disables the current numeric limits consistently with Go/Python behavior.
  - Node.js and Python now scan existing chain-named files at construction and continue `nextSeqnum` from the highest persisted tail sequence when `headSeqnum`/`head_seqnum` is not explicitly supplied.
  - Added default chain reopen/sequence tests and active-retention guard tests for Node.js and Python; added default chain reopen test for Go; added custom-source naming test for Rust.
- Ran a second read-only implementation review round with `glm`, `kimi`, `qwen`, and `minimax`.
- Fixed accepted second-round findings:
  - Rust, Go, Node.js, and Python now preserve the chain `seqnum_id` from existing chain-named files when construction resumes a default-mode chain and the caller did not explicitly provide a sequence ID.
  - Rust, Go, Node.js, and Python detect an existing chain-named `ONLINE` file and reopen it for append, preventing a second parallel active chain after a crash-style reopen.
  - Rust `journal-core` now exposes a locked mutable open path for files created by this SDK, used by the high-level `Log` only for default-mode chain active resume.
  - Go retention now protects the just-active chain file during rotation and close before deleting older unprotected files.
  - Go, Node.js, and Python file-count retention now counts the protected active/current file in the retention envelope while never selecting that protected file for deletion, matching the Rust reference behavior.
  - Node.js chain scanning now reads only `HEADER_SIZE` bytes from each journal file instead of loading entire files into memory.
  - Node.js and Python now include explicit default chain-named `system` journal tests that also assert `system.journal` is not created in default mode.
  - Rust now has a clean-reopen test proving the default chain preserves `seqnum_id` and continues head sequence numbers across construction.
  - Rust, Go, Node.js, and Python now have crash-style reopen tests proving an existing chain-named `ONLINE` file is reused and sequence numbers continue in the same file.
- Ran a third read-only implementation review round with `glm`, `kimi`, `qwen`, and `minimax`.
- Fixed accepted third-round findings:
  - Rust `RotationState` now initializes its count and size state from a reopened `ONLINE` file, so count-based rotation enforces total entries in the resumed file instead of only entries appended after reopen.
  - Rust crash-style reopen coverage now verifies count rotation after resume by writing past the configured count limit and checking successor head sequence.
  - Node.js and Python now scan existing default-mode chain files unconditionally, so an explicit `headSeqnum`/`head_seqnum` cannot bypass an existing `ONLINE` chain file.
  - Node.js and Python now check rotation before append, matching Rust and Go fail-safe rotation ordering.
  - Node.js and Python `archiveTo`/`archive_to` now mark the writer closed immediately after the file descriptor is closed, so lock-release errors cannot trigger recovery writes through a closed descriptor.
  - Node.js and Python crash-style reopen tests now pass an explicit head sequence value and still assert that the existing `ONLINE` file is reused.
  - Dispositioned as broader SOW-0023 scope: Node.js/Python non-zero default policy values, Go construction-time retention behavior, Rust Drop parent-directory sync, tolerant scan-error policy, and duration/age policy gaps.
  - Fourth and later review rounds completed; accepted findings were fixed and revalidated before this slice commit.

## Validation

Acceptance criteria evidence:

- First naming slice implemented. Evidence:
  - `rust/src/crates/journal-log-writer/src/log/config.rs`: `strict_systemd_naming` defaults to false and exposes `with_strict_systemd_naming`.
  - `rust/src/crates/journal-log-writer/src/log/mod.rs`: default active file creation uses chain naming; strict mode uses `system.journal`.
  - `go/journal/log.go`: `LogConfig.StrictSystemdNaming` defaults to false; default active path is chain-named; strict mode uses `<source>.journal`.
  - `node/src/lib/directory-writer.js`: `strictSystemdNaming` / `strict_systemd_naming` defaults to false; default active path is chain-named.
  - `python/journal/directory_writer.py`: `strict_systemd_naming` / `strictSystemdNaming` defaults to false; default active path is chain-named.
  - `rust/src/crates/journal-log-writer/src/log/chain.rs`, `go/journal/log.go`, `node/src/lib/directory-writer.js`, and `python/journal/directory_writer.py`: construction scans existing chain files and resumes the chain sequence identity where supported by the writer primitive.
  - `go/journal/log.go`, `node/src/lib/directory-writer.js`, `python/journal/directory_writer.py`, and `rust/src/crates/journal-log-writer/src/log/chain.rs`: retention counts the protected active/current file in the retention envelope and skips it as a deletion candidate.
  - `rust/src/crates/journal-log-writer/src/log/mod.rs`, `go/journal/log.go`, `node/src/lib/directory-writer.js`, and `python/journal/directory_writer.py`: rotation creates/opens the post-rotation current file before retention enforcement, so `max_files=1` keeps exactly the current file instead of leaking `max_files + 1`.
  - `rust/src/crates/journal-log-writer/src/log/chain.rs`, `go/journal/log.go`, `node/src/lib/directory-writer.js`, and `python/journal/directory_writer.py`: byte retention accounting uses committed journal size from the header tail object instead of sparse preallocation length where the file can be inspected.
  - `rust/src/crates/journal-log-writer/src/log/mod.rs`, `go/journal/log.go`, `node/src/lib/directory-writer.js`, and `python/journal/directory_writer.py`: duration rotation is enforced before append using the incoming entry realtime and active file head realtime.
  - `rust/src/crates/journal-log-writer/src/log/mod.rs`, `go/journal/log.go`, `node/src/lib/directory-writer.js`, and `python/journal/directory_writer.py`: age/count/byte retention can be applied explicitly without an append-triggered rotation or close.
  - `.agents/sow/specs/product-scope.md`, `rust/README.md`, `go/README.md`, `node/README.md`, and `python/README.md`: public documentation records duration rotation, age retention, and explicit retention enforcement APIs for the high-level writer.
- Cross-language Go `v0.1.0` API parity implemented. Evidence:
  - `rust/src/crates/journal-log-writer/src/log/config.rs`, `rust/src/crates/journal-log-writer/src/log/mod.rs`, `rust/src/crates/journal-log-writer/src/log/chain.rs`, and `rust/src/crates/journal-log-writer/src/error.rs`: Rust now exposes open/identity modes, explicit boot ID, creation lifecycle events, construction hooks, artifact sizers, source realtime injection, timestamp clamping, and path/identity/source accessors.
  - `node/src/lib/directory-writer.js` and `node/src/index.js`: Node.js now exports the same high-level API concepts and keeps legacy numeric zero-disable aliases while rejecting explicit zero in structured policy objects.
  - `python/journal/directory_writer.py` and `python/journal/__init__.py`: Python now exports the same high-level API concepts and keeps legacy numeric zero-disable aliases while rejecting explicit zero in structured policy objects.
  - `node/test/all.js`, `python/test_all.py`, and `rust/src/crates/journal-log-writer/tests/log_writer.rs`: tests cover eager open, strict identity, source realtime injection, timestamp clamping, lifecycle creation/deletion, artifact-size retention accounting, accessors, and explicit structured zero rejection.
  - `.agents/sow/specs/product-scope.md`, `rust/README.md`, `node/README.md`, and `python/README.md`: public contracts and docs record the shared API surface.

Tests or equivalent validation:

- `go test ./...` from `go/`: passed.
- `cargo test -p journal-registry` from `rust/`: passed, 10 unit tests plus 1 doc test after robust mixed disposed/archived drain coverage.
- `cargo test -p journal-log-writer --test log_writer -- --nocapture` from `rust/`: passed, 31 tests after duration-rotation, explicit-age-retention, and active-age-protection coverage.
- `node --check node/src/lib/directory-writer.js && node --check node/test/all.js && node node/test/all.js`: passed.
- `python3 -m py_compile python/journal/directory_writer.py python/test_all.py && PYTHONPATH=.local/python-deps:python python3 python/test_all.py`: passed. The `lz4` test dependency remains under `.local/python-deps` inside this repository; no system Python or home cache dependency was required for the run.
- Cross-language API-parity slice:
  - `go test -count=1 ./...` from `go/`: passed, proving the previously tagged Go API remains source-compatible with this slice.
  - `cargo test -p journal-log-writer`: passed after the Rust API-parity changes.
  - `cargo test` from `rust/`: passed for the full Rust workspace after the Rust API-parity changes.
  - `node --check node/src/lib/directory-writer.js && node --check node/src/index.js && node --check node/test/all.js && node node/test/all.js`: passed after the Node.js API-parity changes.
  - `python3 -m py_compile python/journal/directory_writer.py python/journal/__init__.py python/test_all.py && PYTHONPATH=.local/python-deps:python python3 python/test_all.py`: passed after the Python API-parity changes.
  - `git diff --check && .agents/sow/audit.sh`: passed before final SOW close-out edits; final audit rerun is required before the rollback-point commit.
- Final field-remapping and strict/default migration close-out:
  - `go test -count=1 ./journal -run 'TestLogStrictEmptyCloseClearsActivePath|TestLogStrictSystemdNamingArchivesOnlineChainActive|TestLogDefaultChainReopensOnlineFile|TestLogStrictReopenContinuesSequence'` from `go/`: passed.
  - `go test -count=1 ./...` from `go/`: passed.
  - `cargo fmt --manifest-path rust/src/crates/journal-log-writer/Cargo.toml`: passed.
  - `cargo test --manifest-path rust/src/crates/journal-log-writer/Cargo.toml test_strict_systemd_naming_archives_online_chain_active`: passed.
  - `cargo test --manifest-path rust/src/crates/journal-log-writer/Cargo.toml`: passed.
  - `cargo test --manifest-path rust/src/crates/rdp/Cargo.toml`: passed.
  - `node --check node/src/lib/directory-writer.js && node --check node/test/all.js`: passed.
  - `node node/test/all.js`: passed.
  - `python3 -m py_compile python/journal/directory_writer.py python/test_all.py`: passed.
  - `PYTHONPATH=.local/python-deps python3 python/test_all.py`: passed.
  - `git diff --check`: passed.
  - `.agents/sow/audit.sh`: passed after marking the SOW completed, moving it to `.agents/sow/done/`, and updating `SOW-status.md`.

Real-use evidence:

- Stock `journalctl --directory` and `journalctl --file` checks are included in the Rust and Go validation tests where available, including the final Go strict/default migration test that reads the archived default-chain file and new strict active file through stock `journalctl --directory`. Node.js/Python reader tests verify the generated strict/default files and header states after the final migration changes. This SOW does not replace the broader live stock-reader compatibility gates, which remain governed by the project compatibility SOWs.

Reviewer findings:

- Pre-implementation SOW gap review completed with `glm`, `kimi`, `qwen`, and `minimax` on 2026-05-25. Accepted findings were incorporated under `External reviewer gap synthesis on 2026-05-25`.
- A second pre-implementation reviewer round was run on 2026-05-25. Accepted, source-verified findings were incorporated into the acceptance criteria and second-round analysis bullets. Implementation review rounds for the committed slice are recorded below.
- First naming-slice implementation review completed with `glm`, `kimi`, `qwen`, and `minimax`.
  - Accepted and fixed: Rust source prefix hardcoding in strict/default naming paths.
  - Accepted and fixed: Node.js `||` option defaults hiding explicit zero limits.
  - Accepted and fixed: Node.js/Python missing chain-file tail sequence scan on reopen.
  - Accepted and fixed: Node.js/Python missing tests for retention behavior with chain-named files and reopen/sequence continuity.
  - Dispositioned as pre-existing/future SOW-0023 scope: strict identity/host ID fallback differences and full eager-open preflight behavior.
  - Second review round after fixes completed.
  - Accepted and fixed: chain `seqnum_id` was not preserved across clean default-mode reopen in Go/Node.js/Python, and Rust was also updated to preserve the tail file's `seqnum_id` for cross-language consistency.
  - Accepted and fixed: chain-named `ONLINE` file reopening after crash-style construction in Rust, Go, Node.js, and Python.
  - Accepted and fixed: Node.js full-file reads during chain scanning.
  - Accepted and fixed: Node.js/Python explicit default chain-named `system` journal tests.
  - Accepted and fixed: file-count retention semantics aligned to count the protected active/current file while deleting only older unprotected files.
  - Dispositioned as broader SOW-0023 scope: enabled-zero policy validation, duration rotation, age retention, and strict creation-time preflight.
  - Third review round completed.
  - Accepted and fixed: Rust count rotation after `ONLINE` chain reopen initialized from zero instead of the existing file entry count.
  - Accepted and fixed: Node.js/Python explicit head sequence values could bypass chain scanning and create a parallel active file.
  - Accepted and fixed: Node.js/Python rotation happened after append; they now rotate before append to match Rust/Go error ordering.
  - Accepted and fixed: Node.js/Python archive close-state handling after descriptor close and lock-release errors.
  - Dispositioned as broader SOW-0023 scope: Go construction-time retention policy, Rust Drop parent-directory sync, scan-error strictness beyond the Go same-failure fix, and duration/age retention.
- Ran a fourth read-only implementation review round with `glm`, `kimi`, `qwen`, and `minimax`.
- Fixed accepted fourth-round findings:
  - Rust, Go, Node.js, and Python now enforce retention after the post-rotation current file is created/opened, so the retention protected reference points at the current file instead of the pre-rotation file.
  - Rust, Go, Node.js, and Python close/archive paths now protect the just-closed current file during retention, including strict systemd naming where `<source>.journal` is renamed to the chain filename.
  - Rust, Go, Node.js, and Python now account retention byte limits using committed journal size derived from the header tail object, falling back to file metadata only when the file cannot be inspected.
  - Node.js and Python default rotation/retention limits now match Go's disabled-by-default behavior; explicit zero remains a disabled limit.
  - Go, Node.js, and Python strict systemd naming now scan existing chain files to preserve `seqnum_id` and sequence continuity after a strict close.
  - Go now skips unreadable/corrupt chain files during chain-state scan, matching the tolerant Rust/Node.js/Python behavior for this slice.
  - Node.js/Python and Go low-level archive paths now skip same-path renames, matching Rust's same-path guard.
  - Rust no longer treats `head_entry_realtime == 0` as proof that a strict active file is empty; emptiness is based on `n_entries == 0`.
  - Rust reopens empty `ONLINE` files with the construction fallback boot ID instead of substituting the random file ID as boot ID.
  - Added strict byte-retention, strict sequence-resume, exact max-file current-retention, and same-failure tests across affected languages.
- Ran a fifth read-only implementation review round with `glm`, `kimi`, `qwen`, and `minimax`.
- Fixed accepted fifth-round findings:
  - Rust `Log` now exposes an explicit consuming `close()` method that archives the current file, renames strict `<source>.journal` to the chain archive filename, applies retention with the closed current file protected, and avoids relying on `Drop` for production close behavior.
  - Rust strict close/reopen coverage now proves strict close archive-renames `system.journal` and strict reopen continues the sequence.
  - Rust committed-byte retention accounting now 8-byte-aligns the tail object end, matching Go, Node.js, and Python.
  - Node.js retention deletion now ignores `ENOENT` races while still surfacing other unlink errors, matching Go/Python behavior.
  - `rust/README.md` now documents explicit `Log::close()` as the production archive/retention path and clarifies that `Drop` is best-effort state persistence.
  - Qwen's fifth-round reviewer process stopped making progress and was terminated by exact PID after more than 17 minutes; its partial output was treated as stale and not used as a clean review gate.
- Ran focused final read-only review rounds with `glm` and `kimi` after fifth-round fixes.
- Fixed accepted final-round findings:
  - Go no longer enforces retention at `NewLog()` construction time, preventing deletion of existing archives before any active/current file exists; Go has a regression test proving construction with `MaxFiles=1` preserves existing archives before append.
  - Node.js now refreshes cached writer identity after append, matching Python and preventing stale in-memory `nextSeqnum` between appends.
  - Node.js and Python now include construction-time retention safety tests equivalent to Go's regression coverage.
  - Rust now fsyncs the parent journal directory after strict close renames `<source>.journal` to the chain archive filename.
  - Go committed-size retention accounting uses saturating aligned arithmetic for corrupt huge tail offsets/sizes.
  - Python low-level `Writer.create(..., {'head_seqnum': 0})` now defaults to sequence 1, matching Go and Node.js.
  - Rust, Go, Node.js, and Python now discard zero-entry crash-created `ONLINE` active files before append and continue sequence numbers from the existing chain tail; all four languages have regression coverage for this case.
  - Go, Node.js, and Python reopened nil-tail-boot-id fallback now prefers the host boot ID before falling back to file ID, matching Rust.
  - Node.js and Python rotation paths now clean up closed low-level writer references after a post-archive rotation error, allowing a caller retry to create a fresh active file instead of looping on a closed writer.
- Continued with the duration-rotation and age-retention slice:
  - Rust high-level `Log` now enforces `RotationPolicy::duration_of_journal_file` before append using the incoming entry realtime and active file head realtime, and exposes `Log::enforce_retention()` for explicit retention without rotation or close.
  - Go high-level `Log` now exposes `RotationPolicy.WithMaxDuration`, `RetentionPolicy.WithMaxAge`, and `Log.EnforceRetention()`.
  - Node.js high-level `Log` now supports `maxDurationUsec` / `max_duration_usec`, `maxRetentionAgeUsec` / `max_retention_age_usec`, and `log.enforceRetention()`.
  - Python high-level `Log` now supports `max_duration_usec` / `maxDurationUsec`, `max_retention_age_usec` / `maxRetentionAgeUsec`, and `log.enforce_retention()`.
  - All four implementations preserve the active/current-file protection rule for the age-retention path.
  - Rust, Go, Node.js, and Python tests now prove age retention deletes expired archives while preserving an expired active/current file.
  - Rust age-retention-without-append coverage uses a positive `1us` limit after a deterministic sleep instead of relying only on a zero-duration edge case.
  - Python's manual package test runner now invokes the existing construction-time retention safety test, so the test is no longer just a dormant function.
- Duration-rotation and age-retention read-only review round completed with `glm`, `kimi`, and `minimax`.
  - All three reviewers reported no blocking issues and production-grade readiness for the slice.
  - Accepted and fixed low-risk findings: missing explicit active/current age-retention protection tests across languages, Rust age-retention coverage relying on `Duration::ZERO`, and the dormant Python construction-time retention safety test not being called by the manual runner.
  - Accepted and fixed final low-risk findings: Go/Node.js/Python age-retention loops now subtract deleted file sizes from the running retention total, and Rust `Chain::drain()` no longer relies on `partition_point` monotonicity when disposed and archived timestamps are interleaved.
  - Accepted and fixed final Go retry-safety finding: after a post-archive cleanup error during rotation, Go now preserves the successor sequence identity, clears the default-mode active path, and retries into the next chain file without sequence reuse; `TestLogRotationRetriesAfterArchiveCleanupFailure` covers this path.
  - Dispositioned as non-blocking for this slice: Go, Node.js, and Python treat entry realtime `0` as unset/default-now, which matches their existing timestamp override convention and is broader API polish if callers need literal epoch-zero writes; Go sub-microsecond `time.Duration` retention/rotation rounds up to `1us`, which is the smallest representable on-disk journal time unit; Node.js/Python strict empty-close retention behavior remains an internal/unobservable public path because public empty appends are rejected before active file creation.
- Continued with the Go `v0.1.0` API-stabilization slice needed by the Netdata go.d.plugin SNMP traps backend:
  - Go `LogConfig` now exposes `LogOpenLazy` / `LogOpenEager`, `LogIdentityAuto` / `LogIdentityStrict`, lifecycle callbacks, and artifact-size callbacks.
  - Go rotation and retention policies now use pointer-backed optional limits. Unset limits are disabled; explicitly enabled zero or negative limits fail `NewLog()` with `ErrInvalidJournal`.
  - Go high-level `Log` now exposes `ConfiguredDirectory()`, `JournalDirectory()`, `ActivePath()`, `MachineID()`, `BootID()`, and `Source()`.
  - Go high-level `EntryOptions.SourceRealtimeUsec` injects `_SOURCE_REALTIME_TIMESTAMP` on `Log.Append` and `Log.AppendMapWithOptions`.
  - Go high-level append clamps non-progressing realtime and non-zero monotonic overrides forward to preserve strict chain ordering.
  - Added `go/API.md` with the initial public Go API contract and import/tag guidance for `go/v0.1.0`.
  - Documented that initial `go/v0.1.0` accepts systemd-compatible field names only. Automatic Netdata/OTEL field-name remapping remains an additive follow-up before OTEL migration, not a blocker for the SNMP traps integration surface.
- Go API stabilization validation:
  - `go test -count=1 ./...` from `go/`: passed after the API-stabilization changes and after accepted review fixes.
  - `git diff --check`: passed.
- Go API stabilization review:
  - Initial `glm` review found `AppendMap` lacked an options variant and field-name remapping was not in the initial API. `AppendMapWithOptions` was added and tested. Field-name remapping was documented and explicitly deferred as an additive OTEL-migration follow-up because the immediate SNMP traps integration uses systemd-compatible field names.
  - `kimi` and `minimax` focused review sessions stalled without final findings and were terminated by exact PID after no progress; their output was not used as a clean review gate.
  - Focused `glm` re-review reported production-grade readiness for the initial Go `v0.1.0` SNMP traps integration API.
  - Focused `qwen` re-review reported no blocking bugs. Its medium `Close()` archive-failure concern was dispositioned as incorrect because `Writer.archiveTo()` renames before closing and restores `ONLINE` on rename failure, leaving the log retryable. Accepted low-risk findings were fixed: strict `ActivePath()` is cleared after close, the duration underflow guard is documented, and `go/API.md` lists enum-like constants in the stability contract.
- Continued with the cross-language Go `v0.1.0` API-parity slice:
  - Evidence checked before code changes:
    - `go/API.md` defines the stabilized Go `v0.1.x` contract for open mode, identity mode, path accessors, lifecycle callbacks, artifact-size callbacks, source realtime injection, timestamp clamping, and optional policy validation.
    - `node/src/lib/directory-writer.js` currently has default Netdata chain naming, duration rotation, age retention, and explicit retention enforcement, but lacks explicit open mode, strict identity mode, lifecycle callbacks, artifact-size retention accounting, configured-directory/source/identity accessors, source realtime injection, and timestamp clamping.
    - `python/journal/directory_writer.py` has the same parity gaps as Node.js.
    - `rust/src/crates/journal-log-writer/src/log/config.rs` and `rust/src/crates/journal-log-writer/src/log/mod.rs` already provide Rust-style `EntryTimestamps` and rotated/deleted lifecycle observer support, but still need comparison against the Go API for open mode, strict identity mode, creation lifecycle events, path accessors, and artifact-size retention accounting.
  - Implementation routing remains local implementation with external models used only as read-only reviewers.
  - Design constraint for this slice: keep Go `go/v0.1.0` source-compatible. Rust/Node.js/Python may add idiomatic aliases, but defaults and behavior must match the Go contract where the same concept exists.
- Implemented the cross-language Go `v0.1.0` API-parity slice:
  - Rust `journal-log-writer` now exposes lazy/eager open mode, auto/strict identity mode, explicit boot ID configuration, creation lifecycle events, lifecycle hooks at construction, artifact-size retention accounting, path/identity/source accessors, structured invalid-config errors, source realtime injection, and timestamp clamping.
  - Node.js `Log` now exposes the same API concepts through camelCase and snake_case options/constants, including lifecycle callbacks, artifact sizers, source realtime injection, timestamp clamping, strict identity validation, eager open, structured policy validation, and path/identity/source accessors.
  - Python `Log` now exposes the same API concepts through Pythonic and camelCase aliases, including lifecycle callbacks, artifact sizers, source realtime injection, timestamp clamping, strict identity validation, eager open, structured policy validation, and path/identity/source accessors.
  - Go `go/v0.1.x` public API was not changed in this slice.
  - Updated `.agents/sow/specs/product-scope.md`, `rust/README.md`, `node/README.md`, and `python/README.md` to record the shared high-level directory writer API surface.
- Cross-language API-parity validation:
  - `go test -count=1 ./...` from `go/`: passed.
  - `cargo test -p journal-log-writer`: passed.
  - `cargo test` from `rust/`: passed.
  - `node --check node/src/lib/directory-writer.js && node --check node/src/index.js && node --check node/test/all.js && node node/test/all.js`: passed.
  - `python3 -m py_compile python/journal/directory_writer.py python/journal/__init__.py python/test_all.py && PYTHONPATH=.local/python-deps:python python3 python/test_all.py`: passed.
  - `git diff --check && .agents/sow/audit.sh`: passed before final SOW close-out edits; final audit rerun is required before this slice commit.
- Cross-language API-parity review:
  - Initial `glm` read-only review reported production-grade readiness with non-blocking documentation findings only.
  - Accepted and fixed `glm` documentation findings: `rust/README.md` now documents source realtime injection and timestamp clamping; `node/README.md` and `python/README.md` now document structured rotation/retention policy zero-validation semantics.
  - `qwen` did not produce usable findings for this slice and was terminated by exact PID after no progress; its partial output was not used as a clean review gate.
  - Follow-up `glm` and `minimax` re-review processes for the same fixed scope were verified as current-repository SOW-0023 runs, remained silent after reading/exploration without final findings, and were terminated by exact PIDs. The prior `glm` production-grade review plus fixed non-blocking documentation findings is the clean review evidence for this slice.
- Final field-remapping and strict/default migration close-out:
  - Rust, Go, Node.js, and Python high-level `Log` writers now remap Netdata/OTEL-style dotted, lowercase, invalid, or protected user field names into stock-compatible `ND_*` field names and emit `ND_REMAPPING=1` metadata rows once per new mapping per active journal file. Low-level single-file writers remain strict.
  - Rust RDP field-name encoding now matches current vectors, including MD5 fallback and explicit zero-key SipHash-1-3 checksum behavior.
  - Rust, Go, Node.js, and Python strict systemd naming mode now archives a stale default chain-named `ONLINE` active file before creating `<source>.journal`, preserving sequence continuity and preventing parallel active files in the same directory.
  - Rust, Go, Node.js, Python READMEs, `go/API.md`, and `.agents/sow/specs/product-scope.md` document field remapping, strict/default migration behavior, and the externally serialized single-writer method-call policy.
  - Initial final read-only review found a real Go strict empty-close stale `ActivePath()` bug; fixed in `go/journal/log.go` and covered by `TestLogStrictEmptyCloseClearsActivePath`.
  - Initial final read-only review found Node.js/Python boot-ID auto-identity and default active attach inconsistencies; fixed by adding boot-ID auto initialization and constructor-time default active attach in Node.js and Python, with tests asserting non-null boot ID and pre-append writer/sequence capture.
  - Accepted maintainability finding: removed Rust's unused private remapped-item-count parameter.
  - Dispositioned as false positive: Rust `write_structured` already returns `WriterError::Serialization` for non-object JSON values.
  - Dispositioned as non-blocking maintainability: cross-language field-remap vectors are duplicated but identical in Rust, Go, Node.js, and Python; shared fixture centralization remains compatibility-test hygiene under SOW-0022 if it becomes worthwhile.
  - Final `glm` and `minimax` read-only re-reviews both reported production-grade readiness. `qwen` and `kimi` SOW-0023 reviewer processes repeatedly stalled or became stale and were terminated by exact PIDs; their partial outputs were not used as clean review gates.

Same-failure scan:

- Completed for the first naming/resume/retention slice after the fourth review round:
  - Retention protected-reference ordering was checked and fixed in Rust, Go, Node.js, and Python.
  - Same-path archive rename handling was checked and fixed in Go, Node.js, and Python; Rust already guarded same-path archive moves.
  - Sparse preallocation size accounting was checked and fixed for Rust, Go, Node.js, and Python committed-size retention.
  - Strict close sequence-resume behavior was checked and fixed for Go, Node.js, Python, and Rust. Rust now has explicit `Log::close()` archive/retention behavior instead of relying on deferred `Drop` cleanup.
  - Retention delete error handling was checked; Node.js now ignores `ENOENT` races while preserving non-`ENOENT` errors like Go/Python.
  - Node.js/Python default policy values were aligned with Go's disabled-by-default behavior; Rust already uses `None` for disabled limits.
  - Empty-active crash recovery was checked and fixed in Rust, Go, Node.js, and Python.
  - Reopened nil-tail-boot-id fallback was checked and aligned in Go, Node.js, and Python to Rust's host-boot fallback.
  - Rotation/archive error cleanup was checked in Node.js and Python and fixed for the closed-writer-after-archive case.
- Completed for the duration-rotation and age-retention slice:
  - Duration rotation is tested in Rust, Go, Node.js, and Python with size/count rotation disabled and timestamp overrides straddling the duration boundary.
  - Age retention without append-triggered rotation is tested in Rust, Go, Node.js, and Python.
  - Active/current protection for age retention is tested in Rust, Go, Node.js, and Python with expired archives and an expired active/current file.
  - Rust no longer relies only on a zero-duration age-retention edge case; the explicit no-append retention test uses a positive `1us` limit.
  - Go, Node.js, and Python age-retention deletion accounting was checked and hardened to keep the retained-byte total accurate after age deletes, even though byte retention currently runs before age retention.
  - Rust registry age-drain behavior was checked and hardened for mixed disposed/archived chains; the regression test proves an old archived file is drained even when a newer disposed file sorts before it.
  - Go post-archive cleanup failure handling was checked against the existing Node.js/Python retry-cleanup pattern and fixed so a rotation retry continues with the successor sequence instead of reusing stale state.
- Completed for the Go `v0.1.0` API-stabilization slice:
  - Eager open is tested to create an active file during `NewLog()` and emit a created lifecycle event.
  - Strict identity is tested to reject missing machine and boot IDs and accept explicit IDs.
  - Optional policy validation is tested to reject explicitly enabled zero limits.
  - Path accessors are tested for configured root, effective machine-id journal directory, machine ID, boot ID, source, and lazy empty `ActivePath()`.
  - Source realtime injection and timestamp clamping are tested through both `Append` and `AppendMapWithOptions`.
  - Lifecycle events are tested for created, rotated, and retention-deleted paths.
  - Artifact-size retention accounting is tested with synthetic sidecar byte pressure.
  - Strict close now clears `ActivePath()` after archive, with regression coverage.
- Completed for the cross-language Go `v0.1.0` API-parity slice:
  - Rust, Node.js, and Python were checked for the same open/identity/accessor/lifecycle/artifact/source-timestamp/policy-validation concepts added to the stabilized Go API.
  - Rust, Node.js, and Python now have tests for eager open, strict identity, source realtime injection, timestamp clamping, lifecycle events, artifact-size retention accounting, and explicit structured zero rejection.
  - Node.js and Python preserve legacy flat numeric zero-disable aliases while structured policy objects reject explicit zero, avoiding a breaking change for existing callers while matching the Go pointer-backed policy contract for new structured APIs.
- Completed for the final field-remapping and strict/default migration close-out:
  - Field-name remapping vectors and high-level writer behavior were checked in Rust, Go, Node.js, and Python, including dotted names, lowercase/camel-case names, user-supplied protected names beginning with `_`, invalid names that require MD5 fallback, remapping metadata rows, and remap re-emission after rotation.
  - Strict/default migration behavior was checked in Rust, Go, Node.js, and Python with stale chain-named `ONLINE` active files. The tests verify that strict mode archives the old chain active file, creates `<source>.journal`, and continues the sequence from the chain tail.
  - Method-call concurrency policy was checked across public docs/specs. The final contract is caller-side serialization on a single high-level `Log` instance, with SDK writer locks enforcing one-writer ownership across cooperating SDK instances/processes.

Sensitive data gate:

- This SOW uses only synthetic examples and does not include secrets, credentials, SNMP communities, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details.

Artifact maintenance gate:

- AGENTS.md: no project-wide workflow or responsibility changes in this slice.
- Runtime project skills: no workflow changes needed; existing orchestration and compatibility skills covered the fourth-round retention failure mode.
- Specs: updated `.agents/sow/specs/product-scope.md` for default chain naming, strict naming option, current-file retention protection, committed-byte retention accounting, disabled limit semantics, duration rotation, age retention, explicit retention enforcement, the Go `v0.1.0` integration API contract, and the shared Rust/Go/Node.js/Python high-level directory writer API surface.
- End-user/operator docs: updated Rust, Go, Node.js, and Python READMEs for duration rotation, age retention, explicit retention enforcement, and the shared open/identity/accessor/lifecycle/artifact/source-timestamp policy surface; updated Go README and added `go/API.md` for the public Go integration contract.
- End-user/operator skills: none exist for this repository.
- SOW lifecycle: marked `completed` and moved from `.agents/sow/current/` to `.agents/sow/done/` with this close-out commit.
- SOW-status.md: updated to remove SOW-0023 from current work and list it as completed.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` to record Netdata chain active naming as the default high-level writer behavior, strict systemd active naming as an explicit option, current-file retention protection, committed-byte retention accounting, disabled limit semantics, duration rotation, age retention, explicit retention enforcement, the Go `v0.1.0` SNMP traps integration API surface, and the shared cross-language high-level directory writer API surface.

Project skills update:

- No project skill update needed for this slice. The existing compatibility skill already requires shared tests and reviewer iteration; the retention ordering bug was specific implementation behavior now captured in specs/tests.

End-user/operator docs update:

- Updated `rust/README.md`, `go/README.md`, `node/README.md`, and `python/README.md` to document duration rotation, age retention, explicit retention enforcement, open/identity modes, path accessors, lifecycle/artifact callbacks, source realtime injection, timestamp clamping, and policy semantics in the high-level writer. Added `go/API.md` and updated `go/README.md` for Go import path, field-name limitation, and the `go/v0.1.0` contract.

End-user/operator skills update:

- None. This repository has no end-user/operator skills.

Lessons:

- Cross-language retention must be tested with impossible byte limits and exact file-count limits. Smoke tests that only assert "active was not deleted" can encode `max_files + 1` leaks as expected behavior.

Follow-up mapping:

- Strict/default migration polish and field-name remapping were completed in this SOW.
- Raw-item/high-throughput append fast paths are not required to stabilize the public SOW-0023 API because current Rust already accepts raw `KEY=value` items and Go/Node.js/Python expose binary-safe field APIs. If profiling proves field construction is a hot-path problem, additive fast-path APIs are tracked by SOW-0009 benchmark/profile/optimize.
- Rust `open_for_append` incompatible-flag validation parity remains tracked by SOW-0022 compatibility test gap audit.
- Shared cross-language field-remap vector fixture centralization is test-maintenance hygiene. Current vectors are duplicated but identical and passing in all four language suites; any future centralization belongs in SOW-0022 rather than blocking this production API SOW.
- Mixed-format directory reading, retention enforcement on writer open, reader facade work, and Netdata integration are tracked by SOW-0024, SOW-0025, SOW-0027, and SOW-0026 respectively.
- Rust `Drop` remains documented best-effort behavior; production callers must use explicit `Log::close()` for archive rename and retention.

## Outcome

Naming/resume/retention, duration-rotation, age-retention, explicit-retention-enforcement, Go `v0.1.0` API-stabilization, Rust/Node.js/Python Go `v0.1.0` API parity, field-name remapping, strict/default migration polish, and single-writer API documentation are implemented, locally validated, reviewed, and completed. Final read-only reviewers reported production-grade readiness, and remaining non-blocking observations are mapped to existing follow-up SOWs.

## Lessons Extracted

- Cross-language retention tests must cover exact file-count limits and impossible byte limits; otherwise tests can accidentally encode `max_files + 1` leaks.
- Crash recovery needs zero-entry active-file fixtures, not only non-empty `ONLINE` reopen fixtures. Empty active files exercise sequence-resume paths that non-empty files hide.
- Rotation/archive error handling must be tested separately from close/archive error handling because retry behavior differs.

## Followup

- No remaining feature work is intentionally left inside SOW-0023. Next work should pick one pending SOW at a time; performance optimization remains sequenced after feature SOWs.
