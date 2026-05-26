# Go API Stability

This module is imported as:

```go
import "github.com/netdata/systemd-journal-sdk/go/journal"
```

The first consumable tag for this subdirectory module is expected to be
`go/v0.1.0`.

## Stability Contract

The `v0.1.x` Go API is intended to be stable enough for Netdata integration.
Breaking changes to the following public surfaces should be avoided inside the
`v0.1.x` line:

- `journal.NewLog(directory, journal.LogConfig)`
- `journal.LogConfig`
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

By default, the active file uses the Netdata Rust-compatible chain filename.
`StrictSystemdNaming` uses `<source>.journal` as the active file. When strict
naming finds a stale chain-named `ONLINE` active file, `NewLog()` archives it
before creating `<source>.journal`, preserving sequence continuity and avoiding
parallel active files.

`Log` is a single-writer object. Callers must serialize method calls on one
instance; the SDK writer lock prevents a second cooperating SDK writer from
owning the same file, but it is not a per-append goroutine mutex.

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

High-level `Log.Append` clamps non-progressing realtime and non-zero monotonic
overrides forward to preserve strict journal ordering in the generated chain.

## Field Names

The low-level `Writer` accepts systemd-compatible field names only: names must
start with an uppercase ASCII letter, contain only uppercase ASCII letters,
digits, and underscores, and be at most 64 bytes.

The high-level `Log` writer accepts Netdata/OTEL-style field names and remaps
non-systemd-compatible names before writing. The remapping format matches the
Rust writer contract: each journal file gets `ND_REMAPPING=1` metadata rows for
new mappings, and data rows use stock-compatible `ND_*` field names.
User-supplied protected names beginning with `_` are remapped; SDK-owned
protected fields such as `_BOOT_ID` and `_SOURCE_REALTIME_TIMESTAMP` are
injected internally.
