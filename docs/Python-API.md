# Python API

Every example on this page is syntax-checked and executed by repository CI
against synthetic fixtures, except blocks marked illustrative-only.

Install the Python package from the repository checkout:

```sh
python3 -m venv .venv
. .venv/bin/activate
pip install -e python/
```

The Python SDK is pure Python. It does not use native journal bindings or link
to system journal libraries.

Python is a compatibility and automation surface. Use Rust or Go for
high-throughput production ingestion and query paths unless a fresh benchmark
proves the Python path fits the deployment.

## Read One File

Use `FileReader` when the caller owns ordering and reads one journal file.

<!-- verify-example: lang=python id=python-read-one-file -->
```python
from journal import FileReader

with FileReader.open("/var/log/journal/example/system.journal") as reader:
    reader.add_match(b"PRIORITY=6")
    reader.seek_head()

    while reader.next():
        entry = reader.get_entry()
        message = entry["fields"].get("MESSAGE")
        if message is not None:
            print(message.decode("utf-8", errors="replace"))
```

`get_entry()` materializes maps and owned payloads. It is convenient, but it is
not the lowest-cost scan path.

## Scan Payloads With Minimal Work

Use `visit_entry_payloads()` when the consumer can work with `FIELD=value`
bytes directly.

<!-- verify-example: lang=python id=python-visit-entry-payloads -->
```python
from journal import FileReader

with FileReader.open("/var/log/journal/example/system.journal") as reader:
    reader.seek_head()

    while reader.next():
        def visit(payload):
            if payload.startswith(b"MESSAGE="):
                value = payload[len(b"MESSAGE="):]
                print(value.decode("utf-8", errors="replace"))

        reader.visit_entry_payloads(visit)
```

Python visitor callbacks receive owned `bytes` values, so they are safe to
retain. This is simpler than the row-borrowed path, but it copies each payload.
Use `enumerate_entry_payload()` when the caller wants row-scoped current-entry
payloads.

## Enumerate Current-Row DATA With Row Lifetime

Use `entry_data_restart()` and `enumerate_entry_payload()` for facade-style
current-row DATA enumeration.

<!-- verify-example: lang=python id=python-entry-data-enumeration -->
```python
from journal import FileReader

with FileReader.open("/var/log/journal/example/system.journal") as reader:
    reader.seek_head()

    if reader.next():
        reader.entry_data_restart()
        while True:
            payload = reader.enumerate_entry_payload()
            if payload is None:
                break
            print(bytes(payload).decode("utf-8", errors="replace"))
```

Do not keep row-scoped payload views after advancing, seeking, restarting DATA
enumeration, remapping, or closing the reader. Copy when longer ownership is
required.

## Read A Directory

Use `DirectoryReader` for stock-like ordering across active and archived files.

<!-- verify-example: lang=python id=python-read-directory -->
```python
from journal import DirectoryReader

with DirectoryReader.open("/var/log/journal") as reader:
    reader.seek_tail()

    while reader.step_back():
        realtime = reader.get_realtime_usec()
        entry = reader.get_entry()
        message = entry["fields"].get("MESSAGE")
        if message is not None:
            print(realtime, message.decode("utf-8", errors="replace"))
```

Directory reading discovers root journal files plus one machine-ID
subdirectory level and merges files in journal order.

## Use Snapshot Bounds For Query Workloads

The default Python reader uses mmap-backed live bounds where Python's standard
library supports them. Use snapshot bounds when a query may ignore entries
appended after it starts.

<!-- verify-example: lang=python id=python-snapshot-bounds -->
```python
from journal import FileReader, ReaderOptions, READER_BOUNDS_SNAPSHOT

options = ReaderOptions(bounds=READER_BOUNDS_SNAPSHOT)

with FileReader.open(
    "/var/log/journal/example/system.journal",
    options=options,
) as reader:
    reader.seek_head()
    print(reader.selected_access_mode())
```

The internal read-at mode is retained for tests, diagnostics, constrained
platform investigation, and fallback evidence. It is not exported from the
top-level `journal` package as a production reader mode.

## Query Unique Values Through Indexes

Unique values for one field should use the FIELD object's DATA chain, not a row
scan.

<!-- verify-example: lang=python id=python-unique-values -->
```python
from journal import FileReader

with FileReader.open("/var/log/journal/example/system.journal") as reader:
    for value in reader.query_unique("SYSLOG_IDENTIFIER"):
        print(value.decode("utf-8", errors="replace"))
```

Use `query_unique()` when the caller needs an owned list of values.

## Explorer Query

Explorer is the API for filters, facets, histogram, FTS, and selected returned
rows.

<!-- verify-example: lang=python id=python-explorer-query -->
```python
from journal import ExplorerQuery, FileReader

with FileReader.open("/var/log/journal/example/system.journal") as reader:
    query = (
        ExplorerQuery()
        .with_filter("PRIORITY", ["3", "4"])
        .with_facet("SYSLOG_IDENTIFIER")
        .with_histogram("PRIORITY")
    )

    result = reader.explore(query)
    print(result.stats.rows_matched)
```

Default Explorer behavior:

- `ExplorerStrategy.TRAVERSAL`;
- `ExplorerFieldMode.FIRST_VALUE`;
- source realtime enabled;
- indexed filters;
- all-field expansion only for returned rows.

Do not enable `debug_collect_column_fields_by_row_traversal` in production.

## Compare Explorer Strategies

Use `ExplorerStrategy.COMPARE` to validate a query shape before selecting the
index strategy.

<!-- verify-example: lang=python id=python-explorer-compare -->
```python
from journal import (
    ExplorerFieldMode,
    ExplorerQuery,
    ExplorerStrategy,
    FileReader,
)

with FileReader.open("/var/log/journal/example/system.journal") as reader:
    query = ExplorerQuery().with_facet("PRIORITY")
    query.field_mode = ExplorerFieldMode.ALL_VALUES
    query.use_source_realtime = False
    query.limit = 0

    result = reader.explore_with_strategy(query, ExplorerStrategy.COMPARE)
    if result.comparison is not None:
        print(result.comparison.traversal_duration)
        print(result.comparison.index_duration)
```

The index strategy is exact only for its supported subset. It is not a
universal faster mode.

## Write One File

Use direct-file writing when the caller owns the file lifecycle.

<!-- verify-example: lang=python id=python-write-one-file -->
```python
from journal import Writer

writer = Writer.create("/var/log/journal-sdk/example.journal")
try:
    writer.append([
        {"name": "MESSAGE", "value": b"plugin started"},
        {"name": "PRIORITY", "value": b"6"},
        {"name": "SYSLOG_IDENTIFIER", "value": b"example-plugin"},
    ])
finally:
    writer.close()
```

`append()` is the structured hot path for producers that already have field
names and values split.

## Write Binary Fields

<!-- verify-example: lang=python id=python-write-binary -->
```python
from journal import Writer

writer = Writer.create("/var/log/journal-sdk/example.journal")
try:
    writer.append([
        {"name": "MESSAGE", "value": b"sample with binary payload"},
        {"name": "BINARY_PAYLOAD", "value": b"\x00\x01\x02\xff"},
    ])
finally:
    writer.close()
```

Binary values are preserved as field values. The field name remains text.

## Raw Append

Use `append_raw()` only when the caller already has `KEY=value` payloads.

<!-- verify-example: lang=python id=python-raw-append -->
```python
from journal import Writer

writer = Writer.create("/var/log/journal-sdk/example.journal")
try:
    writer.append_raw([
        b"MESSAGE=prebuilt payload",
        b"_HOSTNAME=synthetic-host",
        b"BINARY_PAYLOAD=\x00\x01\x02\xff",
    ])
finally:
    writer.close()
```

The first `=` byte splits the field name from the value. Later `=` bytes and
arbitrary value bytes are preserved.

## Directory Writer With Rotation And Retention

Use `Log` for production ingestion directories.

<!-- verify-example: lang=python id=python-directory-writer -->
```python
from journal import LOG_IDENTITY_STRICT, LOG_OPEN_EAGER, Log

machine_id = bytes.fromhex("00112233445566778899aabbccddeeff")
boot_id = bytes.fromhex("ffeeddccbbaa99887766554433221100")

log = Log("/var/log/journal-sdk", {
    "source": "example-plugin",
    "open_mode": LOG_OPEN_EAGER,
    "identity_mode": LOG_IDENTITY_STRICT,
    "machine_id": machine_id,
    "boot_id": boot_id,
    "compact": True,
    "live_publish_every_entries": 64,
    "rotation_policy": {
        "max_entries": 100000,
        "max_bytes": 128 * 1024 * 1024,
        "max_duration_usec": 3_600_000_000,
    },
    "retention_policy": {
        "max_files": 8,
        "max_bytes": 1024 * 1024 * 1024,
        "max_age_usec": 7 * 24 * 3_600_000_000,
    },
})
try:
    log.append([
        {"name": "MESSAGE", "value": b"plugin started"},
        {"name": "PRIORITY", "value": b"6"},
    ])
finally:
    log.close()
```

`Log` stores files below `<directory>/<machine-id>/`. By default it uses
Netdata-compatible chain active names. Use `strict_systemd_naming` only when
the consumer needs `<source>.journal` active naming.

## Field-Name Policy

<!-- verify-example: lang=python id=python-field-name-policy -->
```python
from journal import FIELD_NAME_POLICY_JOURNALD, Writer

writer = Writer.create("/tmp/example.journal", {
    "field_name_policy": FIELD_NAME_POLICY_JOURNALD,
})
try:
    writer.append([
        {"name": "MESSAGE", "value": b"trusted producer"},
    ])
finally:
    writer.close()
```

Use:

- `FIELD_NAME_POLICY_JOURNALD` for trusted journald-like producers;
- `FIELD_NAME_POLICY_JOURNAL_APP` for untrusted application-facing rules;
- `FIELD_NAME_POLICY_RAW` only for file-format-level tools and tests.

`RAW` files are journal files, but stock systemd tooling is not guaranteed to
accept invalid systemd field names.

## Optional Writer Lock

Core writers do not lock. Acquire the optional cooperating-writer lock helper
when the deployment needs SDK-level exclusion.

<!-- verify-example: lang=python id=python-writer-lock -->
```python
from journal.lock import WriterLock

lock = WriterLock.acquire("/var/log/journal-sdk/example.journal")
try:
    print(lock.path.endswith(".lock"))
finally:
    lock.release()
```

This helper is independent from systemd compatibility.

## Netdata Function Boundary

Use the Netdata function API when the consumer needs Netdata-shaped logs
function output.

<!-- verify-example: lang=python id=python-netdata-function -->
```python
import json

from journal import NetdataFunctionRunOptions, NetdataJournalFunction

function = NetdataJournalFunction.systemd_journal()
request = {
    "after": 0,
    "before": 0,
    "last": 200,
    "facets": ["PRIORITY", "SYSLOG_IDENTIFIER"],
    "histogram": "PRIORITY",
}

response = function.run_directory_request_json_with_options(
    "/var/log/journal",
    request,
    NetdataFunctionRunOptions.from_timeout_seconds(30),
)
print(json.dumps(response, sort_keys=True))
```

Customize `NetdataFunctionConfig.source_selector_name` and
`source_selector_help` when the same function shape serves a domain-specific
journal backend. The wire id remains `__logs_sources`; only the label and help
shown by Netdata change.

<!-- verify-example: lang=python id=python-netdata-source-selector -->
```python
from journal import (
    NetdataFunctionConfig,
    NetdataJournalFunction,
    SystemdJournalProfile,
)

config = NetdataFunctionConfig.systemd_journal()
config.source_selector_name = "Trap Jobs"
config.source_selector_help = "Select the trap job to query"
function = NetdataJournalFunction.new(config, SystemdJournalProfile())
print(function is not None)
```

This layer is Netdata-specific. Generic log explorers should use Explorer
directly unless they need the Netdata request and response shape.

## Verify A File

<!-- verify-example: lang=python id=python-verify-file -->
```python
from journal import verify_file

verify_file("/var/log/journal/example/system.journal")
```

Use `verify_file_with_key()` for sealed files when a verification key is
available. Verification is for integrity checks, not normal query serving.
File-path verification uses the same bounded reader access architecture as
normal file reads, so it avoids whole-file resident buffers while still walking
the object graph and sealed HMAC ranges.
