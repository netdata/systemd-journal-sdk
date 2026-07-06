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
  `journal.Options`, using systemd's 512-byte default threshold and an 8-byte
  minimum clamp for positive configured thresholds. Zstd frames include
  content-size metadata so stock systemd can verify and read back large
  compressed payloads;
- keyed hash tables using the journal file ID;
- byte-safe field values through `journal.Field{Name, Value []byte}`;
- create, online close, explicit offline close, archive close, and
  reopen/append for files created by this writer;
- data and field de-duplication;
- global entry arrays and per-DATA entry links;
- optional pure cross-SDK cooperative lockfile with stale-owner detection when
  callers explicitly acquire `journal.AcquireWriterLock(path)`. The journal
  file format itself does not define a lock protocol, so core writers do not
  lock;
- Forward Secure Sealing TAG writing with `journal.SealOptions`, including
  stock `journalctl --verify --verify-key` coverage for sealed files generated
  by this writer;
- native systemd writers do not participate in the SDK lock protocol and remain
  an operational exclusion;
- live stock-reader validation for the current writer slice with `journalctl
  --file`, `journalctl --file --follow --no-tail --boot=all`, and libsystemd
  reader APIs, including live sequence-order checks;
- configurable explicit live-reader publication cadence through
  `Options.LivePublishEveryEntries`, defaulting to systemd-compatible
  publication after every entry;
- high-level directory writing with chain active naming by
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
- historical unkeyed-hash journal reading, including LZ4-compressed DATA
  objects;
- mmap-backed live reading by default on supported Unix-family and Windows
  targets, with explicit `ReadAt` access retained through `ReaderOptions` only
  for tests, diagnostics, constrained-platform investigation, and controlled
  fallback evidence. `ReadAt` is not a production reader mode; unsupported
  targets that fall back to it must be benchmarked and explicitly accepted
  before production use;
- directory reading across active and archived files with stock-compatible
  root plus one machine-id subdirectory traversal and interleaved multi-file
  ordering, including mixed regular/compact, compressed/uncompressed,
  sealed/unsealed, and whole-file `.journal.zst` files in one directory;
- forward/backward iteration, cursors, realtime and monotonic timestamps,
  seqnum metadata, field enumeration, unique values, binary field values,
  repeated field values, stateful current-entry data enumeration, and
  export/json/text formatting;
- libsystemd-compatible facade functions for open file/directory/files, close,
  seek head/tail/realtime/cursor, next/previous/skip, match groups,
  current-entry data enumeration, field enumeration, unique value enumeration,
  realtime/monotonic/seqnum/cursor metadata, and boot listing;
- cursor metadata is emitted in the official systemd cursor shape; cursor seek
  and test also accept the older SDK cursor shape for compatibility;
- file-backed `journalctl` output covers the stock v260.1 short family,
  `verbose`, `with-unit`, `cat`, `export`, `json`, `json-pretty`,
  `json-sse`, and `json-seq` modes; `--output-fields` projects requested
  fields while JSON/export retain stock metadata fields;
- short-style `journalctl` labels include hostname, identifier/unit, and PID
  components from journal fields, and `--no-hostname` suppresses the hostname
  component;
- `--output=export` uses systemd's size-prefixed binary field encoding and
  blank-line entry separator; JSON output encodes duplicate fields as arrays
  and non-printable/non-UTF-8 values as arrays of unsigned bytes;
- libsystemd-style match behavior: AND between different fields, OR between
  values for the same field, `AddDisjunction()` for `+`, and
  `AddConjunction()` for explicit AND groups;
- a file-backed `journalctl` command under `cmd/journalctl` with
  `--since`, `--until`, `--boot`, `--invocation`, `-I`,
  `--list-invocations`, `--header`, and `--follow` support for
  repository-backed files and directories;
- verification APIs: `journal.VerifyFile()` for structural verification and
  `journal.VerifyFileWithKey()` for sealed TAG/HMAC verification;
- a conformance adapter under `adapter`.

Reader limitations:

- full systemd object-graph verification parity is tracked separately;
- daemon-only journalctl operations are not implemented.

Platform behavior:

- Linux is the stock systemd validation target for `journalctl --file`,
  `journalctl --directory`, live follow, and libsystemd reader checks.
- FreeBSD, macOS, and Windows build the Go SDK without CGO or libsystemd.
  Files generated on those targets are expected to be copied to Linux for stock
  systemd verification when stock tooling is required.
- Writers require explicit machine ID, boot ID, and per-entry monotonic
  timestamps. Callers that need collector-host values can opt into the
  `journalhost` helper and pass its results to the writer explicitly. On Linux,
  containerized callers can set `journalhost.LoadOptions.HostFilesystemPrefix`
  to a mount such as `/host` when they intentionally want host machine identity.
  Missing host files fall back to container-local files; present invalid host
  files return an error so collectors do not silently switch identity.
- Optional writer locking is a separate helper acquired with
  `journal.AcquireWriterLock(path)`. Linux uses procfs boot/process-start
  evidence; FreeBSD and macOS use native boot-time plus conservative process
  liveness evidence; Windows uses process creation-time evidence. The core
  writer constructors never acquire this helper.
- Directory fsync is performed on Unix. Non-Unix targets still sync journal
  file contents, but parent-directory metadata is not fsynced by this SDK; newly
  created or renamed files rely on the target filesystem's crash semantics.
- Unknown non-Unix/non-Windows targets fail optional lock acquisition instead
  of silently pretending to lock.

Basic usage:

```go
machineID, err := journal.ParseUUID("00112233445566778899aabbccddeeff")
if err != nil {
    return err
}
bootID, err := journal.ParseUUID("ffeeddccbbaa99887766554433221100")
if err != nil {
    return err
}
w, err := journal.Create("/path/to/plugin.journal", journal.Options{
    MachineID: machineID,
    BootID:    bootID,
})
if err != nil {
    return err
}
defer w.Close()

return w.Append([]journal.Field{
    journal.StringField("MESSAGE", "plugin started"),
    journal.StringField("PRIORITY", "6"),
    journal.StringField("SYSLOG_IDENTIFIER", "example-plugin"),
}, journal.EntryOptions{
    RealtimeUsec:     1_700_000_000_000_000,
    RealtimeUsecSet:  true,
    MonotonicUsec:    1,
    MonotonicUsecSet: true,
})
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
}, journal.EntryOptions{
    RealtimeUsec:     1_700_000_000_000_001,
    RealtimeUsecSet:  true,
    MonotonicUsec:    2,
    MonotonicUsecSet: true,
})
```

Use `Append([]journal.Field{...})` for binary payloads. `AppendMap()` and
`StringField()` are convenience helpers for string-valued fields.

Raw systemd-compatible payloads:

```go
err := w.AppendRaw([][]byte{
    []byte("MESSAGE=prebuilt payload"),
    []byte("_HOSTNAME=synthetic-host"),
    []byte("BINARY_PAYLOAD=\x00\x01\x02\xff"),
}, journal.EntryOptions{
    RealtimeUsec:     1_700_000_000_000_002,
    RealtimeUsecSet:  true,
    MonotonicUsec:    3,
    MonotonicUsecSet: true,
})
```

`AppendRaw()` accepts complete `KEY=value` byte payloads. The first `=` splits
the field name from the value; later `=` bytes and arbitrary value bytes are
preserved.

Live-reader publication:

```go
w, err := journal.Create("/path/to/plugin.journal", journal.Options{
    MachineID: machineID,
    BootID:    bootID,
    LivePublishEveryEntries: journal.PublishEveryEntries(64),
})
```

`nil` or `1` publishes after every entry and is the stock-compatible default.
`0` disables explicit SDK live publication for poll/snapshot consumers.
`N > 1` publishes after every `N` entries. This is not an `fsync` or durability
setting.

Journal files are created with systemd journald's `0640` default permissions.
Use `Options.FileMode` when a consumer needs a different mode:

```go
w, err := journal.Create("/path/to/private.journal", journal.Options{
    MachineID: machineID,
    BootID:    bootID,
    FileMode:  journal.JournalFileMode(0o600),
})
```

The override applies only to newly-created files; existing files keep their
current filesystem permissions. POSIX modes remain subject to the process
umask, matching systemd/open semantics. Non-POSIX platforms may ignore POSIX
mode bits.

Directory writer with rotation and retention:

```go
log, err := journal.NewLog("/var/log/journal-sdk", journal.LogConfig{
    Source:       "example-plugin",
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
}, journal.EntryOptions{
    RealtimeUsec:     1_700_000_000_000_003,
    RealtimeUsecSet:  true,
    MonotonicUsec:    4,
    MonotonicUsecSet: true,
})
```

`NewLog()` stores files below `<directory>/<machine-id>/`. Rotation archives the
current active file and opens a new active file. By default the active file uses
the chain filename form
`<source>@<seqnum-id>-<head-seqnum>-<head-realtime>.journal`; set
`StrictSystemdNaming: true` to use `<source>.journal` as the active file.
If strict naming opens a directory with a stale chain-named `ONLINE` active
file, it archives that file before creating `<source>.journal`, so the directory
does not keep parallel active files.
If an existing active file is rejected by the low-level append-open path as
unsupported, `Log` follows journald's reliable-open behavior: it uses readable
header metadata to continue sequence identity where possible, moves the old
active file to a collision-safe `*.journal~` disposed name, and creates a fresh
active file. Direct low-level append-open still returns an unsupported error.
Unset rotation and retention limits are disabled; enabling a limit with zero or
a negative value makes `NewLog()` fail. `LogOpenEager` creates or opens the
active file during construction so callers can reject a job before accepting
input. `LogIdentityStrict` is the default and only supported identity mode; it
requires explicit machine and boot IDs. Per-entry appends require explicit
monotonic timestamps.

`ConfiguredDirectory()` returns the root passed to `NewLog()`.
`JournalDirectory()` returns the effective `<directory>/<machine-id>` directory
to pass to stock `journalctl --directory`. `ActivePath()` returns the exact
active journal path after eager open or a successful append; it is empty before
lazy-open creation.
`Log` is a single-writer object; callers must serialize method calls on one
instance. The journal file contract is one writer per file. Acquire
`journal.AcquireWriterLock(path)` when the caller wants the optional
cooperating-writer lock helper to reject another SDK writer for the same file.

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
non-progressing realtime and monotonic overrides forward to preserve strict
ordering in the chain. `EntryOptions.MonotonicUsecSet` and
`EntryOptions.RealtimeUsecSet` allow explicit zero timestamp overrides; without
those flags, zero-value timestamp fields keep the default timestamp behavior.
The low-level `Writer.Append` path preserves explicit caller-provided realtime
and monotonic timestamps without clamping or rejecting them; callers using that
raw API are responsible for not producing same-boot backward monotonic entries
unless they are intentionally creating invalid fixtures. On reopen, `Log`
seeds the monotonic clamp floor from a persisted chain tail only when the tail
entry boot ID matches the current writer boot ID.
`EntryOptions.Seqnum` is a low-level exact-regeneration override. Leave it zero
for normal auto-incrementing sequence numbers; when set, it must move forward
from the writer's next sequence number and may contain gaps.
`SealOptions.StartUsec` is normalized to systemd's FSS verification-key epoch
boundary so stock `journalctl --verify --verify-key` can validate sealed
outputs created from unaligned source timestamps.

For consumer-owned side indexes, `LogConfig.Lifecycle` reports created, rotated,
and retention-deleted journal paths, and `LogConfig.ArtifactSizer` includes
consumer-owned sidecar bytes in size-based retention. See `go/API.md` for the
versioned public API contract.

By default, `Log` syncs each archived journal file on the caller path during
rotation, explicit close, and stale-active startup archive. Latency-sensitive
callers may set `SyncOnArchive: journal.SyncOnArchive(false)` in `LogConfig` to
skip that archive-file sync. With the opt-out, the caller owns archived-file
durability before relying on side indexes or allowing retention to delete
archived files.

The low-level Go `Writer` and high-level `Log` writer support the same
structured `Append()` and raw full-payload `AppendRaw()` entry shapes plus the
same field-name policy layers. The default `FieldNamePolicyJournald` preserves
trusted systemd fields such as `_HOSTNAME` and `_TRANSPORT`.
`FieldNamePolicyJournalApp` drops caller fields that journald would reject from
untrusted applications and fails only when no caller fields remain.
`FieldNamePolicyRaw` accepts any non-empty field name that does not contain
`=`, but RAW-mode files are not guaranteed to be accepted by stock systemd
tooling. High-level `Log` applies these policies to caller fields before
adding SDK-owned fields such as `_BOOT_ID` and
`_SOURCE_REALTIME_TIMESTAMP`. Producer-specific field transformations belong
outside the SDK.

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

Hot-path reader usage:

```go
r, err := journal.OpenFileWithOptions(
    "/path/to/system.journal",
    journal.DefaultReaderOptions().
        WithAccessMode(journal.ReaderAccessMmap).
        WithBounds(journal.ReaderBoundsLive),
)
if err != nil {
    return err
}
defer r.Close()

for {
    ok, err := r.Step()
    if err != nil || !ok {
        return err
    }
    if err := r.VisitEntryPayloads(func(payload []byte) error {
        // payload is FIELD=value bytes. In mmap mode, do not retain it after
        // the callback returns unless it is copied.
        return nil
    }); err != nil {
        return err
    }
}
```

`VisitEntryPayloads`, `EnumerateEntryPayload`, and the facade data enumerator
are zero-copy paths. In mmap mode their payload slices may alias reader storage.
`VisitEntryPayloads` is callback-scoped and does not provide a row-level
guarantee; do not retain its slice after the callback returns. Payloads returned
by `EnumerateEntryPayload` and the facade data enumerator are row-scoped. Copy
the slice, or use `CollectEntryPayloads` / `GetEntryPayload`, when the data must
outlive the documented scope.
For high-cardinality unique-value queries, prefer `VisitUnique` when the caller
can stream results; `QueryUnique` is the owned-result convenience wrapper.

File-backed journalctl:

```sh
go run ./cmd/journalctl --file ../fixtures/systemd/test-data/no-rtc/system.journal.zst --head 1 --output=json
```

`--file`/`-i` may be repeated and accepts glob patterns, matching stock
file-backed `journalctl`. `--file=-` is intentionally unsupported because
portable stdin journals would need seekable mmap-capable descriptors.

Repeated matches for the same field are OR alternatives. Matches for different
fields are ANDed. A separate `+` argument creates an explicit disjunction:

```sh
go run ./cmd/journalctl --file ./sample.journal PRIORITY=3 PRIORITY=4 + MESSAGE=boot
```

Syslog identifier, priority, facility, grep, dmesg, cursor, system unit, user
unit, and invocation filters are supported for file-backed inputs. Unit filters
support exact units and glob expansion over journal unit fields.
`--list-invocations` and `--header` operate on explicit file/directory inputs.
`--new-id128` is a portable standalone utility action, and `--disk-usage`
reports allocated filesystem usage for explicit `--file` or `--directory`
inputs.
`--vacuum-size`, `--vacuum-files`, and `--vacuum-time` operate on explicit
`--directory` inputs, deleting only stock-recognized archived `.journal` and
`.journal~` files while protecting active/current and unknown files.
The CLI implements the stock v260.1 output-mode family, `--output-fields`
projection for `verbose`, `export`, JSON modes, and `cat`, and stock output
controls such as `--all`, `--full`, `--no-full`, empty-result messages, and
`--pager-end` implicit 1000-line tail behavior. `--output=help` prints the
official mode list without opening a journal.

Realtime ranges, boot filters, and follow mode are supported for file-backed
inputs:

```sh
go run ./cmd/journalctl --directory ./journals --boot=all --since @1700000000 --until @1700003600
go run ./cmd/journalctl --file ./active.journal --follow --no-tail --boot=all
```
