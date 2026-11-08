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
Both languages accept a relative directory and refuse an empty one. Rust
resolves a relative directory once, when the writer is created, by lexical
join with the working directory (no symlink resolution), and its path
accessors return absolute paths. Go keeps the path as given, so it follows
later changes of the process working directory.

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

Append-open applies systemd v260.1's writable-open header rules and refuses,
with Go `ErrUnsupportedJournal` and Rust
`JournalError::UnsupportedJournalFile`, files that systemd would not extend:

- a header size other than the current 272 bytes;
- unknown compatible flags, or a missing tail-entry-boot-ID flag;
- an archived file;
- a sealed file, because the writer cannot restore its sealing state and would
  otherwise append an unsealed tail;
- a file with entries whose head realtime is zero or at least 2^55, whose tail
  realtime or tail monotonic is at least 2^55, or whose tail boot ID is null;
- a regular file of 4 GiB or more, where the 32-bit tail entry array hint can
  no longer locate the tail array.

These files stay readable. Readers ignore the tail entry array hint when the
declared arena ends above 4 GiB, because earlier writers stored it truncated.

Append-open also refuses, as invalid rather than unsupported, files missing the
DATA or FIELD hash table or the tail object, because every append indexes
through both tables (Go returns an invalid-journal error that is not
`ErrUnsupportedJournal`; Rust returns `JournalError::InvalidObjectLocation`).
Files with an unknown state, or a sealed header shorter than 240 bytes, are
refused by every open, readers included, as systemd does (Go returns an
invalid-journal error, Rust `JournalError::InvalidJournalFileState`). The
directory writer sets such an active file aside and continues its sequence, as
journald does. With root retention it reports the error instead and leaves the
file in place, because it can neither archive such a file nor keep a set-aside
one.

Entries must carry a realtime in `1..2^55-1` microseconds and a monotonic below
2^55 microseconds. Go returns `ErrTimestampOutOfRange` and Rust returns
`JournalError::TimestampOutOfRange` before any file mutation; the writer stays
usable. The directory writer raises a zero realtime to its clock floor and
refuses other out-of-range values without moving its clocks.

Regular and compact files both stop below 4 GiB, as systemd's do: the
header's 32-bit tail entry array hint cannot address anything higher. Before
writing an entry, the writer bounds the space the entry could need from its
payload sizes and refuses it with Go `ErrJournalFileFull` or Rust
`JournalError::JournalFileFull` when the file might cross 4 GiB minus one
8 MiB growth step. Nothing is written and the writer stays usable, so the
caller can rotate. Entry arrays stop doubling when the next one would not fit.
The directory writer rotates once on this error and writes the entry to the new
file; an entry that does not fit an empty file is refused and its clocks do not
move.

## Uncertain Failures And Reopen

A mutating append, sync or archive failure poisons the writer. Further mutation
is rejected. Close/drop releases resources without rewriting metadata, applying
retention or deleting an uncertain file, including a failed first append whose
cached entry count is zero. Pre-mutation input validation errors remain reusable.
Go exposes `ErrWriterFailed` and preserves the original cause. In Rust the
failing call returns its own error; keep it, because later mutation attempts
return only `JournalError::WriterPoisoned`. The low-level `JournalWriter`
returns that variant directly, and `Log` wraps it as
`WriterError::Journal(JournalError::WriterPoisoned)`. `JournalWriter::is_poisoned()`
and `Log::is_poisoned()` report the state without attempting a mutation.

Append-open validates supported format metadata; it does not certify the full
native index graph. Before a `Writer` or `Log` can reuse uncertain files, exclude
writers and run strict `VerifyIndex`/`verify_index`. Failure leaves evidence
intact. See [[Indexed-Snapshots|Indexed snapshots and recovery]] for the stronger
integrity contract and its offline cost.

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

Both formats stop below 4 GiB (see above). Compression stores the whole
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
  be enforced explicitly;
- disposed `.journal~` files count toward the limits and are deleted before
  archived history, as journald does.

Use explicit size and duration limits for production backends. The SDK derives
systemd-like file-size defaults from retention envelopes when supported by the
language implementation, with a one-twentieth rotation step by default.

In Go, `Log.Close()` applies retention when it archives an active writer, except when
strict naming discards an empty file. An unopened lazy log also skips retention.
`Log.CloseWithoutRetention()` is available in Go `v0.8.2` and later. Use it when
closing before a policy change: it follows the same archive/durability path as
`Close()` and skips retention. The caller must
reopen with the new policy or enforce retention separately. Both methods
remove an empty active file with strict systemd naming, retain its archive with
chain naming, and are idempotent after closure; calling `Close()`
after `CloseWithoutRetention()` does not retroactively apply the old policy.

Rust `0.8.2` provides `Log::close_without_retention()` for the same policy
handoff. It shares `Log::close()`'s archive and sync path, skips retention,
and consumes the writer. Both Rust methods leave an unopened lazy writer
unopened and discard an empty strict-named active file; chain naming keeps
its empty archive. Construct a new writer with the intended policy for
subsequent retention enforcement. Normal Rust `close()` keeps applying its
policy when archiving a file, except when discarding an empty strict-named file.

## Dedicated Root History

Rust and Go `0.10.0` add this feature.

Go `LogConfig.RootRetention` and Rust `Config::with_root_retention(true)` add
an explicit policy for a caller-owned root/source across machine identities.
Use this when retained history must share one allowance even after machine
identity changes. Default retention policies remain machine-local, with
committed-byte accounting and archive-head-age expiry. Go also corrects an
existing default-mode gap: a successful successor-creation retry now applies
retention after a preceding safe creation failure.

Both implementations require strict active names and exclusive caller ownership.
The caller must verify recovered active files before opening the writer. Inventory
reads directory entries, file metadata and fixed-size headers without expanding
records. It rejects malformed or quarantined source candidates, symlinks,
ambiguous identities, filename/header mismatches and inconsistent fixed-header
counts or cached offset/count pairs before pruning. A missing
root is an error. Canonical machine-named entries must be directories; unrelated
metadata directories are allowed when they contain no source candidates and
must be readable so inventory can establish that exclusion.
Live-aware inventory also rejects a missing or replaced active file.

Before root close, empty-file disposal or rotation, both SDKs check that the active
pathname still names the owned file. They reject occupied archive destinations
and unexpected active paths at lazy creation before changing file contents or
names. Failed creation/rotation preflight returns an error without poisoning the
writer; callers can restore the expected paths and retry. Rejected close releases
resources without rewriting the files. These checks cover changes completed
between calls and require caller exclusion throughout each call; they add no
filesystem checks to ordinary nonrotating appends.

The shared policy:

- Counts full directory-visible file lengths, including preallocation, across
  all retained identities. New SDK files allocate at least 8 MiB; existing files
  are counted at their actual length, and smaller allowances are accepted.
  Unlinked files pinned by readers are excluded.
- Expires whole files by their newest saved journal realtime, independently of
  producer event time. Size/count pressure removes oldest tails first, with
  canonical path as the deterministic tie-breaker.
- Protects a live active from size eviction. Explicit maintenance finalizes it
  for idle age expiry or required allocation changes, leaving the next file lazy.
  Eager startup can also leave a lazy successor after finalizing a recovered file.
- Runs on startup, active-file creation, rotation, closing a nonempty active,
  and explicit maintenance. Closing an unopened or empty Log does not sweep
  history. Close without retention finalizes without pruning. Finalized files
  lose live protection: policy shrinkage or a too-small allowance can delete even the
  newest file immediately, without a grace period.
- Emits `Archived` when finalization succeeds without a successor, before any
  deletion; ordinary `Rotated` events keep both old and successor identities.
  Empty-file disposal is not an archive event.
- Installs valid replacement policies before enforcement, preserves explicit
  rotation limits, and recalculates derived allocation geometry.
- Finalizes retired files through an archive-only open that skips the
  append-target refusals above, so old files written by earlier releases can
  still be archived and pruned. At startup, a current active file that
  append-open refuses is archived under its archive name the same way, instead
  of being set aside, because inventory rejects set-aside files. An empty one
  is removed instead, since it holds no history. A header every open refuses
  cannot be archived either, so startup reports that error.
- Reports safe cleanup failures independently from healthy appends; uncertain
  archive mutation remains a writer failure and stops pruning. Opening or
  validating a retired file before mutation may fail without poisoning the
  healthy current writer. Failed writers remain inspectable for status.

Positive age limits below one microsecond normalize to one microsecond in both
SDKs; larger limits use whole microseconds.

This is neither an exact record TTL nor a hard physical disk cap. With a 24-hour
file span and successful hourly cleanup, age overhang is about one day plus one
hour under advancing clocks; existing files keep their actual spans. Size pressure
can remove history earlier. Preallocation, active growth, pinned readers and
failed cleanup can exceed the allowance. The SDK does not schedule cleanup.

See [[Go-API|Go API]] and [[Rust-API|Rust API]] for executable examples, policy
updates, inventory and maintenance outcomes.

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
