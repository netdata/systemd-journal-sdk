# Writer APIs

The writer API is split by lifecycle and append shape.

## Quick Selection

| Consumer Need | Rust | Go | Use This When |
|---|---|---|---|
| directory backend | `Log` | `Log` | production ingestion with rotation and retention |
| one file | low-level writer | `Writer` | caller owns lifecycle |
| structured fields | `write_fields` | `Append` | producer already has field/value pairs |
| prebuilt payloads | `write_entry` | `AppendRaw` | caller already has `KEY=value` bytes |
| optional lock | `journal_core::file::lock` | `AcquireWriterLock` | cooperating SDK writer exclusion |
| FSS | `SealOptions` in core writer | `SealOptions` | tamper-evident journal files |

## Directory Writer

Use the directory writer for production log backends.

It handles:

- active file creation and reopen;
- journald-style reliable replacement when an active file cannot be appended;
- chain active naming by default;
- optional strict `<source>.journal` active naming;
- rotation by entry count, file size, or file duration;
- retention by file count, byte envelope, or age;
- retention on open and on demand;
- lifecycle events for created, rotated, and retained-deleted files.

Directory writers store files below `<directory>/<machine-id>/`. They are
single-writer objects; callers must serialize method calls on one instance.

By default, directory writers sync each archived journal file on the caller path
during rotation, explicit close, and stale-active startup archive. Rust exposes
`Config::with_sync_on_archive(false)`, and Go exposes
`LogConfig.SyncOnArchive: journal.SyncOnArchive(false)`, for latency-sensitive
callers that intentionally move archived-file durability responsibility outside
the SDK. With this opt-out, callers must make archived files durable before
relying on side indexes or allowing retention to delete them.

## Direct-File Writer

Use direct-file writing when the caller owns exactly one journal file.

Direct writers can:

- create supported journal files;
- append to files created by this SDK when append-open is safe;
- choose regular or compact layout;
- choose DATA compression;
- set live publication cadence;
- close online, close offline, or archive through explicit calls;
- enable FSS where supported.

Direct append-open is not a promise to mutate arbitrary historical or
systemd-created files. Unsupported append targets must fail before entry
mutation.

## Structured Append

Structured append is the production hot path when the producer already has
field names and values separately.

Use structured append for:

- NetFlow, SNMP traps, OTEL logs, and other structured ingestion;
- binary values;
- avoiding `KEY=value` construction;
- avoiding avoidable parsing and allocation.

Rust:

- directory writer: `Log::write_fields`;
- structured type: `journal_log_writer::StructuredField`;
- mixed low-level field type: `journal_core::file::EntryField`.

Go:

- direct writer: `Writer.Append([]journal.Field, journal.EntryOptions)`;
- directory writer: `Log.Append([]journal.Field, journal.EntryOptions)`;
- helper: `journal.StringField(name, value)`.

## Raw Append

Raw append accepts full `KEY=value` bytes. The first `=` splits the field name
from the value. Later `=` bytes and arbitrary value bytes are preserved.

Use raw append only when:

- the caller already has valid payload bytes;
- the caller is implementing a systemd-like low-level payload path;
- a benchmark or compatibility test intentionally needs raw payload parity.

Do not convert structured data to `KEY=value` only to call raw append. That is
avoidable work.

## Field-Name Policies

| Spec Policy | Rust | Go | Use Case | Stock systemd |
|---|---|---|---|---|
| `JOURNALD` | `FieldNamePolicy::Journald` | `FieldNamePolicyJournald` | trusted journald-like producer | intended friendly |
| `RAW` | `FieldNamePolicy::Raw` | `FieldNamePolicyRaw` | file-format-level tools and tests | not guaranteed |
| `JOURNAL-APP` | `FieldNamePolicy::JournalApp` | `FieldNamePolicyJournalApp` | untrusted app input under journald rules | intended friendly |

Producer-specific remapping does not belong in the SDK. Transform fields before
calling the writer.

## Format Options

| Option | Default | Use When |
|---|---|---|
| regular format | yes | maximum stock compatibility baseline |
| compact format | no | footprint-sensitive backends with validated readers |
| no DATA compression | yes | maximum write and read speed |
| zstd compression | no | disk footprint matters and query paths rarely need compressed DATA |
| xz or lz4 compression | no | compatibility or measured workload fit |
| FSS | no | tamper evidence is required |

Compact format has a 4 GiB offset ceiling. Compression stores the whole
`FIELD=value` payload compressed, so the field name is not visible without
decompression.

## Live Publication

Live publication controls when writer metadata is made visible to live stock
readers:

- `1`: default, systemd-compatible publication after every entry;
- `0`: disable explicit SDK publication for poll/snapshot consumers;
- `N > 1`: publish after every `N` entries.

This is not `fsync` and not a durability policy. It is a visibility and wakeup
policy for live readers.

## Rotation And Retention

Rotation starts a new active file. Retention deletes older files owned by the
directory writer.

Default behavior:

- unset rotation limits mean no automatic rotation;
- unset retention limits mean no automatic deletion;
- the active/current file counts toward file and byte envelopes;
- the active/current file is never selected for deletion to satisfy retention;
- retention is enforced when an active file is opened or created, and can also
  be enforced explicitly.

Use explicit size and duration limits for production backends. The SDK derives
systemd-like file-size defaults from retention envelopes when supported by the
language implementation, with a one-twentieth rotation step by default.

## Identity And Locking

Core writers do not discover host identity. Pass machine ID, boot ID, and
generated-entry monotonic timestamps explicitly. The optional host identity
helper is for integrations that intentionally want local-host values. In Go it
lives in `github.com/netdata/systemd-journal-sdk/go/journalhost`; in Rust it is
the `systemd-journal-sdk-host` crate with lib name `journal_host`. Callers
still pass the returned values to the writer. Linux callers running inside a
container can opt into a host filesystem prefix such as `/host`; the helper then
checks host machine-id files under that prefix before container-local files.
Missing host files fall back to container-local files; present invalid host
files return an error so collectors do not silently switch identity.

Core writers also do not lock. The journal file contract is one writer per
file, but systemd does not define a portable journal-file lock protocol. The
SDK lock helpers are optional cooperating-writer helpers and must be acquired
explicitly by the caller.

## Production Checklist

- Use directory writers for long-running ingestion backends.
- Use structured append for structured producers.
- Keep compression off until a footprint benchmark justifies it.
- Tune live publication only when stock live-follow freshness is not required.
- Use `JOURNALD` policy for trusted backends that need stock systemd tooling.
- Keep `RAW` policy out of stock compatibility claims.
