# Go Journal SDK

This module contains pure-Go systemd journal reader and writer components. It
does not use CGO, native addons, or libsystemd linkage.

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
- directory reading across active and archived files;
- forward/backward iteration, cursors, realtime timestamps, field enumeration,
  unique values, binary field values, and export/json/text formatting;
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

- directory iteration is sequential by journal file and intended for
  non-overlapping rotated files in this slice; realtime interleaving across
  overlapping multi-file directories is tracked with the broader
  interoperability phase;
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
    Source: "netdata-plugin",
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
Zero-valued rotation and retention limits are disabled. Duration rotation is
checked before append using the incoming entry realtime and the active file head
realtime. Retention counts the tracked active/current file in file-count and
committed-byte limits, but deletion only selects older unprotected files owned
by the configured `Source`; the tracked active/current file is never deleted to
satisfy a retention limit. Call `log.EnforceRetention()` to apply age/count/byte
retention without waiting for another append-triggered rotation or close.

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
