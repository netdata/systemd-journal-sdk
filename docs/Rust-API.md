# Rust API

The normal Rust dependency is the public SDK package:

```toml
[dependencies]
journal = { package = "systemd-journal-sdk", version = "0.6.0" }
```

Use the lower-level packages only when the public package does not expose the
type you need. For example, structured directory writes currently use
`StructuredField` from `systemd-journal-sdk-log-writer`:

```toml
[dependencies]
journal = { package = "systemd-journal-sdk", version = "0.6.0" }
journal_log_writer = { package = "systemd-journal-sdk-log-writer", version = "0.6.0" }
```

## Read One File

Use `FileReader` when the caller owns ordering and reads one journal file.

```rust
use journal::FileReader;

let mut reader = FileReader::open("/var/log/journal/example/system.journal")?;
reader.add_match(b"PRIORITY=6");
reader.seek_head();

while reader.next()? {
    let entry = reader.get_entry()?;
    if let Some(message) = entry.get_str("MESSAGE") {
        println!("{message}");
    }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

`get_entry()` materializes maps and owned payloads. It is convenient, but it is
not the lowest-cost scan path.

## Scan Payloads With Minimal Work

Use `visit_entry_payloads()` when the consumer can work with `FIELD=value`
bytes directly.

```rust
use journal::FileReader;

let mut reader = FileReader::open("/var/log/journal/example/system.journal")?;
reader.seek_head();

while reader.next()? {
    reader.visit_entry_payloads(|payload| {
        if payload.starts_with(b"MESSAGE=") {
            let value = &payload[b"MESSAGE=".len()..];
            println!("{}", String::from_utf8_lossy(value));
        }
        Ok(())
    })?;
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

Uncompressed payloads are borrowed from mmap-backed journal data. Compressed
payloads are decompressed into row-owned storage. The payload is valid only
inside the callback for this visitor shape.

## Enumerate Current-Row DATA With Row Lifetime

Use `entry_data_restart()` and `enumerate_entry_payload()` when a facade-like
caller needs current-row payloads that stay valid until the row changes.

```rust
use journal::FileReader;

let mut reader = FileReader::open("/var/log/journal/example/system.journal")?;
reader.seek_head();

if reader.next()? {
    reader.entry_data_restart()?;
    while let Some(payload) = reader.enumerate_entry_payload()? {
        println!("{}", String::from_utf8_lossy(payload));
    }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

Do not keep the returned slice after advancing, seeking, restarting DATA
enumeration, remapping, or closing the reader. Copy if longer ownership is
required.

## Read A Directory

Use `DirectoryReader` for stock-like ordering across active and archived files.

```rust
use journal::DirectoryReader;

let mut reader = DirectoryReader::open("/var/log/journal")?;
reader.seek_tail();

while reader.previous()? {
    let realtime = reader.get_realtime_usec()?;
    let entry = reader.get_entry()?;
    println!("{realtime} {:?}", entry.get_str("MESSAGE"));
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

Directory reading merges journal files in journal order. It is the right API
for `journalctl --directory` style behavior.

## Use Snapshot Bounds For Query Workloads

The default reader is live. Use snapshot bounds when a query may ignore entries
appended after it starts.

```rust
use journal::{FileReader, ReaderOptions};

let options = ReaderOptions::snapshot();
let mut reader = FileReader::open_with_options("/var/log/journal/example/system.journal", options)?;
reader.seek_head();
# Ok::<(), Box<dyn std::error::Error>>(())
```

Snapshot bounds avoid live-file refresh work during long scans.

## Query Unique Values Through Indexes

Unique values for one field should use the FIELD object's DATA chain, not a row
scan.

```rust
use journal::FileReader;

let mut reader = FileReader::open("/var/log/journal/example/system.journal")?;
reader.visit_unique_values("SYSLOG_IDENTIFIER", |value| {
    println!("{}", String::from_utf8_lossy(value));
    Ok(())
})?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Use `query_unique()` only when the caller needs an owned vector of all values.

## Explorer Query

Explorer is the API for filters, facets, histogram, FTS, and selected returned
rows.

```rust
use journal::{ExplorerQuery, FileReader};

let mut reader = FileReader::open("/var/log/journal/example/system.journal")?;
let query = ExplorerQuery::default()
    .with_filter("PRIORITY", ["3", "4"])
    .with_facet("SYSLOG_IDENTIFIER")
    .with_histogram("PRIORITY");

let result = reader.explore(&query)?;
println!("matched rows: {}", result.stats.rows_matched);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Default Explorer behavior:

- `ExplorerStrategy::Traversal`;
- `ExplorerFieldMode::FirstValue`;
- source realtime enabled;
- indexed filters;
- all-field expansion only for returned rows.

Do not enable `debug_collect_column_fields_by_row_traversal` in production.

## Compare Explorer Strategies

Use `ExplorerStrategy::Compare` to validate a query shape before using the
index strategy.

```rust
use journal::{ExplorerFieldMode, ExplorerQuery, ExplorerStrategy, FileReader};

let mut reader = FileReader::open("/var/log/journal/example/system.journal")?;
let query = ExplorerQuery {
    facets: vec![b"PRIORITY".to_vec()],
    field_mode: ExplorerFieldMode::AllValues,
    use_source_realtime: false,
    limit: 0,
    ..ExplorerQuery::default()
};

let result = reader.explore_with_strategy(&query, ExplorerStrategy::Compare)?;
if let Some(comparison) = result.comparison {
    println!("traversal: {:?}", comparison.traversal_duration);
    println!("index: {:?}", comparison.index_duration);
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

The index strategy is exact only for its supported subset. It is not a
universal faster mode.

## Write A Directory With Rotation And Retention

Use `Log` for production ingestion directories.

```rust
use journal::{Config, Log, Origin, RetentionPolicy, RotationPolicy, Source};
use std::path::Path;
use std::time::Duration;

let origin = Origin {
    machine_id: None,
    namespace: None,
    source: Source::System,
};

let config = Config::new(
    origin,
    RotationPolicy::default()
        .with_number_of_entries(100_000)
        .with_duration_of_journal_file(Duration::from_secs(3600)),
    RetentionPolicy::default()
        .with_number_of_journal_files(8)
        .with_duration_of_journal_files(Duration::from_secs(7 * 24 * 3600)),
)
.with_compact(true)
.with_live_publish_every_entries(64);

let mut log = Log::new(Path::new("/var/log/journal-sdk"), config)?;
log.write_entry(
    &[
        b"MESSAGE=plugin started".as_slice(),
        b"PRIORITY=6".as_slice(),
        b"SYSLOG_IDENTIFIER=example-plugin".as_slice(),
    ],
    None,
)?;
log.sync()?;
log.close()?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`Log` stores files below `<directory>/<machine-id>/`. By default it uses
Netdata-compatible chain active names. Use
`Config::with_strict_systemd_naming(true)` only when the consumer needs
`<source>.journal` active naming.

## Write Structured Fields

Use structured fields when the producer already has field names and values
split.

```rust
use journal::{Config, Log, Origin, RetentionPolicy, RotationPolicy, Source};
use journal_log_writer::StructuredField;
use std::path::Path;

let origin = Origin {
    machine_id: None,
    namespace: None,
    source: Source::System,
};

let config = Config::new(
    origin,
    RotationPolicy::default(),
    RetentionPolicy::default(),
);

let mut log = Log::new(Path::new("/var/log/journal-sdk"), config)?;
let fields = [
    StructuredField::new(b"MESSAGE", b"binary-safe structured entry"),
    StructuredField::new(b"PRIORITY", b"6"),
    StructuredField::new(b"BINARY_PAYLOAD", b"\x00\x01\x02\xff"),
];
log.write_fields(&fields, None)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

This avoids constructing `KEY=value` bytes only to split them again.

## Field-Name Policy

```rust
use journal::{Config, FieldNamePolicy, Origin, RetentionPolicy, RotationPolicy, Source};

let origin = Origin {
    machine_id: None,
    namespace: None,
    source: Source::System,
};
let config = Config::new(
    origin,
    RotationPolicy::default(),
    RetentionPolicy::default(),
)
.with_field_name_policy(FieldNamePolicy::Journald);
```

Use:

- `FieldNamePolicy::Journald` for trusted journald-like producers;
- `FieldNamePolicy::Raw` only for file-format-level tools and tests.
- `FieldNamePolicy::JournalApp` for untrusted application-facing rules.

`Raw` files are journal files, but stock systemd tooling is not guaranteed to
accept invalid systemd field names.

## Netdata Function Boundary

Use `journal::netdata` when the consumer needs Netdata-shaped function output.

```rust
use journal::netdata::{NetdataFunctionRunOptions, NetdataJournalFunction};
use serde_json::json;
use std::path::Path;

let function = NetdataJournalFunction::systemd_journal();
let request = json!({
    "after": 0,
    "before": 0,
    "last": 200,
    "facets": ["PRIORITY", "SYSLOG_IDENTIFIER"],
    "histogram": "PRIORITY"
});

let response = function.run_directory_request_json_with_options(
    Path::new("/var/log/journal"),
    &request,
    NetdataFunctionRunOptions::from_timeout_seconds(30),
)?;
println!("{}", serde_json::to_string(&response)?);
# Ok::<(), Box<dyn std::error::Error>>(())
```

This layer is Netdata-specific. Generic log explorers should use Explorer
directly unless they need the Netdata request and response shape.

## Verify A File

```rust
use journal::verify_file;

verify_file("/var/log/journal/example/system.journal")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Use `verify_file_with_key()` for sealed files when a verification key is
available. Verification is for integrity checks, not normal query serving.
