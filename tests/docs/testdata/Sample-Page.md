# Sample Page

A minimal wiki page used by the verified-examples harness end-to-end tests.
Every fenced code block here is intentionally short, in scope, and known to
compile/run against the local Rust workspace and Go module.

## Rust Run-Mode Example

Open a file-backed reader, scan MESSAGE values, and stop on EOF.

<!-- verify-example: lang=rust id=rust-read-one-file -->
```rust
use journal::FileReader;

let mut reader = FileReader::open("/var/log/journal/example/system.journal")?;
reader.seek_head();
let mut count: u64 = 0;
while reader.next()? {
    let entry = reader.get_entry()?;
    if entry.get_str("MESSAGE").is_some() {
        count += 1;
    }
}
if count == 0 {
    eprintln!("no entries seen");
}
```

## Rust Build-Mode Example With Prelude

Open a file-backed reader, then materialize it as a compile-only example that
imports the netdata config types via a prelude.

<!-- verify-example: lang=rust id=rust-netdata-config-build prelude=netdata-config-imports mode=build -->
```rust
fn describe() -> NetdataFunctionConfig {
    let cfg = NetdataFunctionConfig::default();
    let profile = SystemdJournalProfile::default();
    let _f = NetdataJournalFunction::new(cfg.clone(), profile);
    cfg
}
```

## Go Run-Mode Reader Example

Open a file-backed reader with the `open-reader` prelude, scan all entries,
and count the ones with a MESSAGE field.

<!-- verify-example: lang=go id=go-read-one-file prelude=open-reader -->
```go
count := 0
for {
    if err := r.Next(); err != nil {
        break
    }
    entry, err := r.GetEntry()
    if err != nil {
        return err
    }
    if _, ok := entry.Fields["MESSAGE"]; ok {
        count++
    }
}
fmt.Fprintf(os.Stderr, "entries=%d\n", count)
return nil
```

## Go Run-Mode Writer Example

Create a new journal file at the scratch path, append three entries with
synthetic fields, and close it.

<!-- verify-example: lang=go id=go-write-three-entries -->
```go
w, err := journal.Create("/var/log/journal-sdk/example.journal", journal.Options{})
if err != nil {
    return err
}
defer w.Close()
for i := 0; i < 3; i++ {
    err := w.Append([]journal.Field{
        journal.StringField("MESSAGE", fmt.Sprintf("hello-%d", i)),
        journal.StringField("PRIORITY", "6"),
        journal.StringField("SYSLOG_IDENTIFIER", "demo-agent"),
    }, journal.EntryOptions{})
    if err != nil {
        return err
    }
}
return nil
```

## Go Example Using `prelude=open-writer`

Create and close a journal file, demonstrating the writer prelude path. The
prelude supplies the `w, err := ...; defer w.Close()` boilerplate so the body
only describes the meaningful work.

<!-- verify-example: lang=go id=go-open-writer prelude=open-writer -->
```go
if err := w.Append([]journal.Field{
    journal.StringField("MESSAGE", "writer prelude worked"),
    journal.StringField("PRIORITY", "6"),
    journal.StringField("SYSLOG_IDENTIFIER", "web-server"),
}, journal.EntryOptions{}); err != nil {
    return err
}
```

## Python Run-Mode Reader Example

Open a file-backed reader, scan all entries, and count MESSAGE values.

<!-- verify-example: lang=python id=python-read-one-file -->
```python
from journal import FileReader

reader = FileReader.open("/var/log/journal/example/system.journal")
try:
    reader.seek_head()
    count = 0
    while reader.next():
        entry = reader.get_entry()
        if entry and entry["fields"].get("MESSAGE"):
            count += 1
    if count == 0:
        raise RuntimeError("no entries seen")
finally:
    reader.close()
```

## JavaScript Run-Mode Reader Example

Open a file-backed reader, scan all entries, and count MESSAGE values through
the Node.js package surface.

<!-- verify-example: lang=javascript id=javascript-read-one-file -->
```javascript
import { FileReader } from '@netdata/systemd-journal-sdk';

const reader = FileReader.open('/var/log/journal/example/system.journal');
try {
  reader.seekHead();
  let count = 0;
  while (reader.step()) {
    const entry = reader.getEntry();
    if (entry.fields.MESSAGE) {
      count += 1;
    }
  }
  if (count === 0) {
    throw new Error('no entries seen');
  }
} finally {
  reader.close();
}
```

## Illustrative-Only Block

This block is intentionally illustrative; the harness must skip it. The body
is hand-edited pseudo-code that does not actually run.

<!-- illustrative-only: pseudo-code for documentation, intentionally not compilable -->
```rust
let _ = journal::FileReader::open("/tmp/this/path/does/not/need/to/exist.journal")?;
```
