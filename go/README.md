# Go Journal Writer

This module contains a pure-Go systemd journal writer. It does not use CGO,
native addons, or libsystemd linkage.

Current writer scope:

- regular, non-compact journal files;
- uncompressed DATA objects;
- keyed hash tables using the journal file ID;
- create, close, and reopen/append for files created by this writer;
- data and field de-duplication;
- global entry arrays and per-DATA entry links;
- Linux writer locking with advisory `flock`;
- live stock-reader validation for the current writer slice with `journalctl
  --file`, `journalctl --file --follow --no-tail --boot=all`, and libsystemd
  reader APIs, including live sequence-order checks.

Deferred scope:

- DATA compression;
- Forward Secure Sealing and TAG objects;
- compact-format writer support;
- appending to arbitrary historical or systemd-created journal variants;
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
