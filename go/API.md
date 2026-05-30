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
- `journal.LogIdentityAuto` and `journal.LogIdentityStrict`
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

Future `v0.1.x` changes should be additive where practical.

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

Stateful data and unique enumeration return full `FIELD=value` payloads and are
binary-safe. `SdJournalGetData` returns the first value for a repeated field;
callers that need all repeated values must use the restart/enumerate data API.
Direct `SdJournalQueryUnique` returns `[]UniqueValue`, where `Field` is the
field name and `Value` is the binary-safe raw field value.

`DefaultReaderOptions()` uses live mmap-backed reads on Unix. Use
`WithAccessMode(journal.ReaderAccessReadAt)` only when mmap is undesirable for
diagnostics or a constrained environment. `ReaderBoundsLive` refreshes visible
entries when active files grow; `ReaderBoundsSnapshot` fixes the visible file
state at open time.

`VisitEntryPayloads`, `EnumerateEntryPayload`, and
`SdJournalEnumerateAvailableData` are zero-copy hot paths. Returned or callback
payload slices may alias reader-owned storage in mmap mode. Current-row
payloads returned by `EnumerateEntryPayload` or
`SdJournalEnumerateAvailableData` stay valid after end-of-row enumeration and
until the reader advances, seeks, clears/restarts DATA enumeration,
refreshes/remaps the file, or closes. Callback payload slices passed to
`VisitEntryPayloads` remain callback-scoped. Use `CollectEntryPayloads`,
`GetEntryPayload`, `GetRaw`, `GetRawValues`, or an explicit copy when longer
ownership is required.

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
instance; the SDK writer lock prevents a second cooperating SDK writer from
owning the same file, but it is not a per-append goroutine mutex.

The lock is platform-specific behind the same public contract: Linux keeps
exact `/proc` stale-owner checks plus `flock`; FreeBSD and macOS use boot-time
and `ps` process-start checks plus `flock`; Windows uses process creation time
checks plus a non-blocking byte-range lock outside journal data. FreeBSD and
macOS require `ps` in `PATH`, and the SDK forces `LC_ALL=C` for locale-stable
process-start evidence. Unknown non-Unix/non-Windows targets fail writer open
instead of silently writing without a platform file lock.

## Open And Identity Modes

`LogOpenLazy` is the default. It validates the configured directory and existing
chain state, but creates a new active file on first append.

`LogOpenEager` creates or opens the active journal file during `NewLog`, proving
file creation, writer lock acquisition, and writer options before callers accept
work.

`LogIdentityAuto` is the default. It loads host machine/boot IDs when available
and generates missing IDs.

`LogIdentityStrict` requires `Options.MachineID` and `Options.BootID` to be
provided explicitly.

Host boot ID auto-loading is Linux-only. FreeBSD, macOS, and Windows callers
that need deterministic boot identity should provide `Options.BootID`, or use
`LogIdentityStrict` to make missing IDs an error.

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
