# Go Journal SDK

This module contains pure-Go systemd journal reader and writer components. It
does not use CGO, native addons, or libsystemd linkage.

Import path:

```go
import "github.com/netdata/systemd-journal-sdk/go/journal"
```

Current writer scope:

- regular journal files by default and compact journal files with
  `journal.Options{Compact: true}`;
- uncompressed DATA objects by default;
- optional zstd, xz, and lz4-compressed DATA object writing with
  `journal.Options`;
- keyed hash tables using the journal file ID;
- byte-safe field values through `journal.Field{Name, Value []byte}`;
- create, online close, explicit offline close, archive close, and
  reopen/append for files created by this writer;
- data and field de-duplication;
- global entry arrays and per-DATA entry links;
- pure cross-SDK cooperative lockfile with stale-owner detection, plus a
  secondary advisory `flock`, to prevent multiple SDK writers from opening the
  same file;
- Forward Secure Sealing TAG writing with `journal.SealOptions`, including
  stock `journalctl --verify --verify-key` coverage for sealed files generated
  by this writer;
- native systemd writers do not participate in the SDK lock protocol and remain
  an operational exclusion;
- live stock-reader validation for the current writer slice with `journalctl
  --file`, `journalctl --file --follow --no-tail --boot=all`, and libsystemd
  reader APIs, including live sequence-order checks;
- high-level directory writing with Netdata-compatible chain active naming by
  default, opt-in strict systemd active naming, entry-count, file-size, and
  duration rotation, plus tracked journal-file-count, byte-size, and age
  retention.

Deferred scope:

- appending to arbitrary historical or systemd-created journal variants;
- full systemd object-graph verification parity beyond the current repository
  verification API.

Current reader scope:

- regular and compact journal files;
- `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst` files;
- zstd-compressed fixture files and zstd, xz, and lz4-compressed DATA objects
  through pure-Go dependencies;
- directory reading across active and archived files with stock-compatible
  root plus one machine-id subdirectory traversal and interleaved multi-file
  ordering;
- forward/backward iteration, cursors, realtime and monotonic timestamps,
  seqnum metadata, field enumeration, unique values, binary field values,
  repeated field values, stateful current-entry data enumeration, and
  export/json/text formatting;
- libsystemd-compatible facade functions for open file/directory/files, close,
  seek head/tail/realtime/cursor, next/previous/skip, match groups,
  current-entry data enumeration, field enumeration, unique value enumeration,
  realtime/monotonic/seqnum/cursor metadata, and boot listing;
- `--output=export` uses systemd's size-prefixed binary field encoding and
  blank-line entry separator; `--output=json` encodes duplicate fields as
  arrays and non-printable/non-UTF-8 values as arrays of unsigned bytes;
- libsystemd-style match behavior: AND between different fields, OR between
  values for the same field, `AddDisjunction()` for `+`, and
  `AddConjunction()` for explicit AND groups;
- a file-backed `journalctl` command under `cmd/journalctl`;
- verification APIs: `journal.VerifyFile()` for structural verification and
  `journal.VerifyFileWithKey()` for sealed TAG/HMAC verification;
- a conformance adapter under `adapter`.

Reader limitations:

- mixed-format directory validation across compact/regular, compression
  variants, and sealed/unsealed files is tracked separately;
- full systemd object-graph verification parity is tracked separately;
- daemon-only journalctl operations are not implemented.

Basic usage:

```go
w, err := journal.Create("/path/to/plugin.journal", journal.Options{})
if err != nil {
    return err
}
defer w.Close()

return w.Append([]journal.Field{
    journal.StringField("MESSAGE", "plugin started"),
    journal.StringField("PRIORITY", "6"),
    journal.StringField("SYSLOG_IDENTIFIER", "netdata-plugin"),
}, journal.EntryOptions{})
```

`Close()` matches systemd's plain `journal_file_close()` behavior and leaves the
file in `ONLINE` state. Use `CloseOffline()` when a single file should be
finalized as `OFFLINE`; directory rotation uses `ArchiveTo()` internally to
produce `ARCHIVED` files.

Binary-safe values:

```go
err := w.Append([]journal.Field{
    journal.StringField("MESSAGE", "sample with binary payload"),
    {Name: "BINARY_PAYLOAD", Value: []byte{0x00, 0x01, 0x02, 0xff}},
}, journal.EntryOptions{})
```

Use `Append([]journal.Field{...})` for binary payloads. `AppendMap()` and
`StringField()` are convenience helpers for string-valued fields.

Directory writer with rotation and retention:

```go
log, err := journal.NewLog("/var/log/journal-sdk", journal.LogConfig{
    Source:       "netdata-plugin",
    OpenMode:     journal.LogOpenEager,
    IdentityMode: journal.LogIdentityStrict,
    Options: journal.Options{
        MachineID: machineID,
        BootID:    bootID,
    },
    RotationPolicy: journal.RotationPolicy{}.
        WithMaxEntries(100000).
        WithMaxFileSize(128 * 1024 * 1024).
        WithMaxDuration(time.Hour),
    RetentionPolicy: journal.RetentionPolicy{}.
        WithMaxFiles(8).
        WithMaxBytes(1024 * 1024 * 1024).
        WithMaxAge(7 * 24 * time.Hour),
})
if err != nil {
    return err
}
defer log.Close()

return log.Append([]journal.Field{
    journal.StringField("MESSAGE", "plugin started"),
    journal.StringField("PRIORITY", "6"),
}, journal.EntryOptions{})
```

`NewLog()` stores files below `<directory>/<machine-id>/`. Rotation archives the
current active file and opens a new active file. By default the active file uses
the Netdata Rust writer chain filename form
`<source>@<seqnum-id>-<head-seqnum>-<head-realtime>.journal`; set
`StrictSystemdNaming: true` to use `<source>.journal` as the active file.
If strict naming opens a directory with a stale chain-named `ONLINE` active
file, it archives that file before creating `<source>.journal`, so the directory
does not keep parallel active files.
Unset rotation and retention limits are disabled; enabling a limit with zero or
a negative value makes `NewLog()` fail. `LogOpenEager` creates or opens the
active file during construction so callers can reject a job before accepting
input. `LogIdentityStrict` requires explicit machine and boot IDs instead of
falling back to host files or generated IDs.

`ConfiguredDirectory()` returns the root passed to `NewLog()`.
`JournalDirectory()` returns the effective `<directory>/<machine-id>` directory
to pass to stock `journalctl --directory`. `ActivePath()` returns the exact
active journal path after eager open or a successful append; it is empty before
lazy-open creation.
`Log` is a single-writer object; callers must serialize method calls on one
instance. The SDK writer lock prevents another cooperating SDK writer from
owning the same file, but it is not a per-append goroutine mutex.

Duration rotation is checked before append using the incoming entry realtime and
the active file head realtime. Retention counts the tracked active/current file
in file-count and committed-byte limits, but deletion only selects older
unprotected files owned by the configured `Source`; the tracked active/current
file is never deleted to satisfy a retention limit. Call
`log.EnforceRetention()` to apply age/count/byte retention without waiting for
another append-triggered rotation or close.
Retention also runs once when a writer opens or creates the active file:
existing-active reopen and `LogOpenEager` enforce it during `NewLog()`, while
lazy archived-only construction defers enforcement until the first append opens
the active file, before the first entry is written.

`EntryOptions.SourceRealtimeUsec` injects `_SOURCE_REALTIME_TIMESTAMP` when the
source timestamp differs from the journal entry timestamp. `Log.Append` clamps
non-progressing realtime and non-zero monotonic overrides forward to preserve
strict ordering in the chain.

For Netdata-style side indexes, `LogConfig.Lifecycle` reports created, rotated,
and retention-deleted journal paths, and `LogConfig.ArtifactSizer` includes
consumer-owned sidecar bytes in size-based retention. See `go/API.md` for the
versioned public API contract.

The low-level Go `Writer` accepts systemd-compatible field names only. The
high-level `Log` writer remaps Netdata/OTEL-style dotted, lowercase, or
otherwise incompatible field names into stock-compatible `ND_*` names and emits
`ND_REMAPPING=1` metadata rows that preserve the original names. User-supplied
protected names beginning with `_` are remapped; SDK-owned protected fields such
as `_BOOT_ID` and `_SOURCE_REALTIME_TIMESTAMP` are injected internally.

Basic reader usage:

```go
r, err := journal.OpenFile("/path/to/system.journal")
if err != nil {
    return err
}
defer r.Close()

r.AddMatch([]byte("PRIORITY=6"))
for {
    ok, err := r.Step()
    if err != nil || !ok {
        return err
    }
    entry, err := r.GetEntry()
    if err != nil {
        return err
    }
    _ = entry.Fields["MESSAGE"]
}
```

File-backed journalctl:

```sh
go run ./cmd/journalctl --file ../fixtures/systemd/test-data/no-rtc/system.journal.zst --head 1 --output=json
```

Repeated matches for the same field are OR alternatives. Matches for different
fields are ANDed. A separate `+` argument creates an explicit disjunction:

```sh
go run ./cmd/journalctl --file ./sample.journal PRIORITY=3 PRIORITY=4 + MESSAGE=boot
```
