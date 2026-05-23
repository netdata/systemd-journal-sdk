# Go Journal Writer

This module contains a pure-Go systemd journal writer. It does not use CGO,
native addons, or libsystemd linkage.

Current writer scope:

- regular, non-compact journal files;
- uncompressed DATA objects;
- keyed hash tables using the journal file ID;
- byte-safe field values through `journal.Field{Name, Value []byte}`;
- create, close, and reopen/append for files created by this writer;
- data and field de-duplication;
- global entry arrays and per-DATA entry links;
- Linux writer locking with advisory `flock`;
- live stock-reader validation for the current writer slice with `journalctl
  --file`, `journalctl --file --follow --no-tail --boot=all`, and libsystemd
  reader APIs, including live sequence-order checks;
- high-level directory writing with systemd-compatible active/archive file
  naming, entry-count and file-size rotation, and file-count and byte-size
  retention.

Deferred scope:

- DATA compression;
- Forward Secure Sealing and TAG objects;
- compact-format writer support;
- appending to arbitrary historical or systemd-created journal variants;
- duration-based directory rotation and retention;
- Go reader facade and journalctl-compatible CLI.

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
        WithMaxFileSize(128 * 1024 * 1024),
    RetentionPolicy: journal.RetentionPolicy{}.
        WithMaxFiles(8).
        WithMaxBytes(1024 * 1024 * 1024),
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
current active file and opens a new active file. Retention deletes only archived
files owned by the configured `Source`; the active file is never deleted to
satisfy a retention limit.
