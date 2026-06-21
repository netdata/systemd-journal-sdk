# Go API Stability

This module is imported as:

```go
import "github.com/netdata/systemd-journal-sdk/go/journal"
```

The current consumable tag for this subdirectory module is expected to be
`go/v0.3.0`.

## Stability Contract

The `v0.x` Go API is intended to be stable enough for integration work while
the SDK is still pre-1.0. Breaking changes to the following public surfaces
should require a new minor release tag and an explicit SOW decision:

- `journal.NewLog(directory, journal.LogConfig)`
- `journal.LogConfig`
- `journal.Options`
- `journal.PublishEveryEntries`
- `journal.LogOpenLazy` and `journal.LogOpenEager`
- `journal.RotationPolicy` and `journal.RetentionPolicy` builder methods
- `journal.EntryOptions`
- `journal.Field` and `journal.StringField`
- `(*journal.Log).Append`
- `(*journal.Log).AppendMap`
- `(*journal.Log).AppendMapWithOptions`
- `(*journal.Log).Sync`
- `(*journal.Log).Close`
- `(*journal.Log).EnforceRetention`
- `(*journal.Log).ConfiguredDirectory`
- `(*journal.Log).JournalDirectory`
- `(*journal.Log).ActivePath`
- `(*journal.Log).MachineID`
- `(*journal.Log).BootID`
- `(*journal.Log).Source`
- `journal.ReaderOptions`, `journal.ReaderAccessMode`, and
  `journal.ReaderBounds`
- `journal.OpenFileWithOptions`, `journal.OpenDirectoryWithOptions`, and
  `journal.OpenFilesWithOptions`
- `(*journal.Reader).VisitEntryPayloads`,
  `(*journal.Reader).CollectEntryPayloads`,
  `(*journal.Reader).GetEntryPayload`, `(*journal.Reader).GetRaw`,
  `(*journal.Reader).GetRawValues`, `(*journal.Reader).EntryDataRestart`,
  and `(*journal.Reader).EnumerateEntryPayload`
- lifecycle and artifact-size callback interfaces
- lifecycle event type and reason constants
- exported sentinel errors

SOW-0115 intentionally removes the previous generated identity/time fallback
contract. Writer integrations now provide machine ID, boot ID, and
generated-entry monotonic timestamps explicitly, or explicitly call an optional
host helper and pass those values to the writer.

Future `v0.x` changes should be additive where practical, except where an
accepted SOW records a breaking cleanup before `v1.0`.

## Reader Facade Contract

The public reader facade intentionally mirrors the file-backed subset of
libsystemd/Netdata `jf` needed by Netdata readers:

- `SdJournalOpen`, `SdJournalOpenFile`, `SdJournalOpenDirectory`,
  `SdJournalOpenFiles`, and `SdJournalClose`
- `SdJournalSeekHead`, `SdJournalSeekTail`, `SdJournalSeekRealtimeUsec`,
  `SdJournalSeekCursor`
- `SdJournalNext`, `SdJournalPrevious`, `SdJournalNextSkip`,
  `SdJournalPreviousSkip`
- `SdJournalAddMatch`, `SdJournalAddDisjunction`,
  `SdJournalAddConjunction`, and `SdJournalFlushMatches`
- `SdJournalGetEntry`, `SdJournalGetData`, `SdJournalRestartData`, and
  `SdJournalEnumerateAvailableData`
- `SdJournalEnumerateFields`, `SdJournalRestartFields`, and
  `SdJournalEnumerateField`
- `SdJournalQueryUnique`, `SdJournalQueryUniqueState`,
  `SdJournalRestartUnique`, and `SdJournalEnumerateAvailableUnique`
- `SdJournalGetRealtimeUsec`, `SdJournalGetMonotonicUsec`,
  `SdJournalGetSeqnum`, `SdJournalGetCursor`, `SdJournalTestCursor`, and
  `SdJournalListBoots`

`SdJournalSeekCursor` accepts a syntactically valid cursor as a seek location
even when no exact entry exists. Use `SdJournalTestCursor` to check whether the
current entry exactly matches a cursor. Cursor metadata is emitted in the
official systemd cursor shape; seek and test also accept the older SDK cursor
shape for compatibility.

Stateful data and unique enumeration return full `FIELD=value` payloads and are
binary-safe. `SdJournalGetData` returns the first value for a repeated field;
callers that need all repeated values must use the restart/enumerate data API.
Direct `SdJournalQueryUnique` returns `[]UniqueValue`, where `Field` is the
field name and `Value` is the binary-safe raw field value.
`Reader.VisitUnique` and `DirectoryReader.VisitUnique` stream indexed unique
values without first materializing the full result set; use `QueryUnique` only
when the caller needs an owned slice of all values.

`DefaultReaderOptions()` uses live mmap-backed reads on supported Unix-family
and Windows targets. `ReaderAccessReadAt` is retained only for tests,
diagnostics, constrained-platform investigation, and controlled fallback
evidence. It is not a production reader mode. If `ReaderAccessAuto` selects
read-at in production, treat that as a deployment issue to investigate and
benchmark before accepting. `ReaderBoundsLive` refreshes visible entries when
active files grow; `ReaderBoundsSnapshot` fixes the visible file state at open
time.

`VisitEntryPayloads`, `EnumerateEntryPayload`, and
`SdJournalEnumerateAvailableData` are zero-copy hot paths. Returned or callback
payload slices may alias reader-owned storage in mmap mode. Current-row
payloads returned by `EnumerateEntryPayload` or
`SdJournalEnumerateAvailableData` stay valid after end-of-row enumeration and
until the reader advances, seeks, clears/restarts DATA enumeration,
refreshes/remaps the file, or closes. Callback payload slices passed to
`VisitEntryPayloads` remain callback-scoped and do not have the row-level
guarantee. Use `CollectEntryPayloads`, `GetEntryPayload`, `GetRaw`,
`GetRawValues`, or an explicit copy when longer ownership is required.

## Explorer And Netdata Function APIs

`Reader.Explore`, `Reader.ExploreWithStrategy`, and
`Reader.ExploreWithStrategyAndControl` expose the optimized Explorer API for
log-viewer workloads. The Explorer API supports indexed filters, FTS,
facets, histograms, row limits, direction, anchors, sampling, progress,
cancellation, and timeout control while avoiding full-row expansion except for
returned rows.

`SystemdJournalNetdataFunction()` and
`SystemdJournalPluginCompatibleNetdataFunction()` expose a Netdata-compatible
generic logs function API over journal directories. Requests are JSON objects
matching the systemd-journal function shape: `info`, `after`, `before`,
`anchor`, `direction`, `last`, `query`, `facets`, `histogram`, `data_only`,
`delta`, `tail`, `sampling`, and `selections`.

Use `RunDirectoryRequestJSONWithOptions` when the caller already parsed the
request. Use `RunDirectoryRequestBytesWithOptions` when the caller has raw
JSON bytes. Test wrappers must read request bytes from stdin; do not pass a
request filename to privileged wrappers.

Use `NetdataFunctionConfig.SourceSelectorName` and `SourceSelectorHelp` when a
consumer needs domain-specific wording for the `__logs_sources` selector. The
systemd-journal defaults remain `Journal Sources` and `Select the logs source
to query`.

`NetdataFunctionRunOptions` carries timeout, progress, cancellation, and
optional state callbacks. A cancelled run returns a table response with status
`499`; timeout returns a partial table response with status `200` and a
warning message. These are controlled stops, not Go errors.

The default Netdata profile keeps UID/GID journal fields as raw numeric
strings. The plugin-compatible profile may resolve UID/GID display names for
comparison with Netdata's installed `systemd-journal.plugin`, but the Go SDK
keeps the no-CGO contract and uses a pure-Go passwd/group file lookup instead
of NSS. Callers that need a different identity-display policy should provide a
custom profile above the core journal reader.

## Directory Contract

`NewLog` takes the configured root directory. The SDK appends the machine ID and
writes journal files below:

```text
<configured-directory>/<machine-id>/
```

Use:

- `ConfiguredDirectory()` for the original root passed to `NewLog`.
- `JournalDirectory()` for the effective directory to pass to
  `journalctl --directory`.
- `ActivePath()` after a successful append or eager open for the exact active
  journal file path.

In lazy mode, `ActivePath()` is empty before a journal file exists.

By default, the active file uses the chain filename form.
`StrictSystemdNaming` uses `<source>.journal` as the active file. When strict
naming finds a stale chain-named `ONLINE` active file, `NewLog()` archives it
before creating `<source>.journal`, preserving sequence continuity and avoiding
parallel active files.

`Log` is a single-writer object. Callers must serialize method calls on one
instance. The journal file format requires one active writer per file, but the
SDK core writer does not enforce that contract by default.

Acquire `journal.AcquireWriterLock(path)` when the caller explicitly wants the
optional cooperating-writer lock helper. That helper is independent from
systemd compatibility and from core writer constructors. Linux keeps exact
`/proc` stale-owner checks; FreeBSD and macOS use boot-time plus conservative
process-liveness checks; Windows uses process creation-time checks. Unknown
non-Unix/non-Windows targets fail optional lock acquisition instead of silently
pretending to lock.

## Open And Identity Modes

`LogOpenLazy` is the default. It validates the configured directory and existing
chain state, but creates a new active file on first append.

`LogOpenEager` creates or opens the active journal file during `NewLog`, proving
file creation/open and configured writer options before callers accept work.

Writer identity is strict. Callers provide `Options.MachineID` and
`Options.BootID` explicitly, and append calls provide generated-entry
monotonic timestamps explicitly through `EntryOptions`.

Callers that need a host's systemd/journald identity must obtain it through an
opt-in identity helper and pass the returned values to the writer. The writer
does not read host identity files, platform identity services, or SDK-local
identity fallbacks.

## Live Publication Cadence

`Options.LivePublishEveryEntries` controls explicit live-reader publication
cadence for low-level `Writer` and high-level `Log` writes.

Use `journal.PublishEveryEntries(n)` to set it:

- `nil` or `1`: default systemd-compatible publication after every entry.
- `0`: disable explicit SDK live publication for latency-tolerant
  poll/snapshot consumers.
- `N > 1`: publish after every `N` appended entries.

This is not a durability sync or `fsync` cadence. Modes other than `1` must not
be claimed as stock `journalctl --follow` compatible unless their own live
matrix has been validated.

## Journal File Mode

`Options.FileMode` controls the POSIX permissions used when a writer creates a
new journal file. Leave it nil to use systemd journald's `0640` default. Use
`journal.JournalFileMode(0o600)` or another permission-only value when a
consumer needs a different mode.

The mode applies only to newly-created files. Opening an existing journal keeps
its current filesystem permissions. POSIX modes remain subject to the process
umask, matching systemd/open semantics. Non-POSIX platforms may ignore POSIX
mode bits.

## Rotation And Retention

Rotation and retention limits are optional. Use builder methods to enable a
limit:

```go
journal.RotationPolicy{}.
    WithMaxEntries(100000).
    WithMaxFileSize(128 * 1024 * 1024).
    WithMaxDuration(time.Hour)

journal.RetentionPolicy{}.
    WithMaxFiles(8).
    WithMaxBytes(1024 * 1024 * 1024).
    WithMaxAge(7 * 24 * time.Hour)
```

Leaving a limit unset disables it. Calling a builder with zero or a negative
value makes `NewLog` fail with `ErrInvalidJournal`.

The tracked active/current file counts toward file and byte retention envelopes
but is never deleted to satisfy retention.

Retention is also applied once when an active writer is opened or created.
Existing-active reopen and `LogOpenEager` enforce retention during `NewLog`.
Lazy archived-only construction remains side-effect-free until the first
append opens the active file; retention then runs before the entry is written.

## Lifecycle And Artifact Accounting

`LogConfig.Lifecycle` receives synchronous created, rotated, and retention
deleted events. These events expose concrete journal file paths so consumers can
maintain side indexes or sidecars without polling.

`LogConfig.ArtifactSizer` lets consumers include sidecar bytes in size-based
retention decisions. It is called with the journal file path. Missing sidecars
should return `0, nil`; unexpected errors abort retention/preflight.

## Timestamp Contract

`EntryOptions.RealtimeUsec` controls the journal entry realtime timestamp.
`EntryOptions.MonotonicUsec` controls the journal entry monotonic timestamp.
`EntryOptions.SourceRealtimeUsec`, when non-zero, injects
`_SOURCE_REALTIME_TIMESTAMP`.

High-level `Log.Append` clamps non-progressing realtime and monotonic overrides
forward to preserve strict journal ordering in the generated chain.
`EntryOptions.RealtimeUsecSet` and `EntryOptions.MonotonicUsecSet` distinguish
explicit zero timestamp overrides from omitted zero-value struct fields.

`EntryOptions.Seqnum` is a low-level exact-regeneration override for direct
writer use. Leave it zero for normal auto-incrementing sequence numbers. When
set, it must be greater than or equal to the writer's next sequence number; gaps
are allowed, but rewinding is rejected. High-level `Log` users should normally
leave it unset because the directory writer manages chain sequence continuity.

## Append Shapes

Writers expose two append shapes:

- `Append([]journal.Field, journal.EntryOptions)` accepts structured
  binary-safe `{Name, Value}` fields and is the preferred SDK hot path for
  producers that already hold structured data.
- `AppendRaw([][]byte, journal.EntryOptions)` accepts complete `KEY=value`
  byte payloads matching the low-level systemd writer shape. The first `=`
  byte separates the field name from the value; later `=` bytes and arbitrary
  value bytes are preserved.

Both shapes apply the configured `FieldNamePolicy` to caller-provided fields
or payloads. High-level `Log` appends SDK-owned fields such as `_BOOT_ID` and
`_SOURCE_REALTIME_TIMESTAMP` after caller policy filtering; `_BOOT_ID` is also
written as journal entry metadata.

Forward Secure Sealing `SealOptions.StartUsec` is normalized to systemd's
verification-key epoch boundary: `floor(StartUsec / IntervalUsec) *
IntervalUsec`. This keeps generated sealed files compatible with stock
`journalctl --verify --verify-key`.

## Field Names

The low-level `Writer` and high-level `Log` writer expose the same
`FieldNamePolicy` contract:

- `FieldNamePolicyJournald` is the default. It accepts trusted journald-style
  names: non-empty, at most 64 bytes, not digit-first, uppercase ASCII letters,
  digits, and underscores, with leading `_` allowed.
- `FieldNamePolicyJournalApp` applies journald's untrusted application-facing
  rules. It uses the same character and length limits, disallows leading `_`,
  drops invalid caller fields, and returns an error only when no caller field
  remains.
- `FieldNamePolicyRaw` accepts every field name the journal DATA structure can
  represent directly: non-empty and no `=` in the name. RAW-mode files are
  journal files, but they are not guaranteed to be accepted by stock systemd
  tooling when names violate systemd conventions.

SDK-owned fields and metadata are written under journald-compatible rules.
Producer-specific field transformations belong outside the SDK.
