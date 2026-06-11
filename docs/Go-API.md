# Go API

Every example on this page is compiled and executed by repository CI against
synthetic fixtures, except blocks marked illustrative-only.

Install the Go submodule:

<!-- illustrative-only: registry install command -->
```sh
go get github.com/netdata/systemd-journal-sdk/go@v0.6.4
```

Import the journal package:

<!-- illustrative-only: import fragment shown alone -->
```go
import "github.com/netdata/systemd-journal-sdk/go/journal"
```

The examples focus on SDK calls. Add ordinary Go standard-library imports such
as `bytes`, `encoding/json`, `fmt`, and `time` when a snippet uses them.

The Go SDK is pure Go and does not use CGO.

## Read One File

Use `OpenFile` for one journal file.

<!-- verify-example: lang=go id=go-read-one-file -->
```go
r, err := journal.OpenFile("/var/log/journal/example/system.journal")
if err != nil {
    return err
}
defer r.Close()

r.AddMatch([]byte("PRIORITY=6"))
r.SeekHead()

for {
    ok, err := r.Step()
    if err != nil || !ok {
        return err
    }
    entry, err := r.GetEntry()
    if err != nil {
        return err
    }
    if message, ok := entry.Fields["MESSAGE"]; ok {
        fmt.Println(string(message))
    }
}
```

`GetEntry()` materializes maps and owned payloads. It is convenient, but it is
not the lowest-cost scan path.

## Scan Payloads With Minimal Work

Use `VisitEntryPayloads` when the consumer can work with `FIELD=value` bytes.

<!-- verify-example: lang=go id=go-visit-payloads -->
```go
r, err := journal.OpenFile("/var/log/journal/example/system.journal")
if err != nil {
    return err
}
defer r.Close()

r.SeekHead()
for {
    ok, err := r.Step()
    if err != nil || !ok {
        return err
    }
    err = r.VisitEntryPayloads(func(payload []byte) error {
        if bytes.HasPrefix(payload, []byte("MESSAGE=")) {
            fmt.Println(string(payload[len("MESSAGE="):]))
        }
        return nil
    })
    if err != nil {
        return err
    }
}
```

This avoids entry map construction and lets the callback decide which payloads
to inspect.

## Enumerate Current-Row DATA With Row Lifetime

Use `EntryDataRestart` plus `EnumerateEntryPayload` for facade-style
current-row enumeration. The snippet continues from an open reader `r`.

<!-- verify-example: lang=go id=go-entry-data-enumeration prelude=open-reader -->
```go
if ok, err := r.Step(); err != nil || !ok {
    return err
}
if err := r.EntryDataRestart(); err != nil {
    return err
}
for {
    payload, ok, err := r.EnumerateEntryPayload()
    if err != nil || !ok {
        return err
    }
    fmt.Println(string(payload))
}
```

Payloads may alias reader-owned mmap or row storage. They remain valid until
the reader advances, seeks, clears DATA state, refreshes/remaps, or closes.
Copy when longer ownership is required.

## Read A Directory

Use `OpenDirectory` for stock-like ordering across active and archived files.

<!-- verify-example: lang=go id=go-read-directory -->
```go
dr, err := journal.OpenDirectory("/var/log/journal")
if err != nil {
    return err
}
defer dr.Close()

dr.SeekTail()
for {
    ok, err := dr.StepBack()
    if err != nil || !ok {
        return err
    }
    entry, err := dr.GetEntry()
    if err != nil {
        return err
    }
    fmt.Println(string(entry.Fields["MESSAGE"]))
}
```

Directory reading discovers root journal files plus one machine-ID subdirectory
level and merges files in journal order.

## Use Snapshot Bounds For Query Workloads

The default Go reader uses mmap-backed live bounds on Unix. Use snapshot bounds
when a query may ignore entries appended after it starts.

<!-- verify-example: lang=go id=go-snapshot-bounds -->
```go
opts := journal.DefaultReaderOptions().
    WithBounds(journal.ReaderBoundsSnapshot)

r, err := journal.OpenFileWithOptions(
    "/var/log/journal/example/system.journal",
    opts,
)
if err != nil {
    return err
}
defer r.Close()
```

Use `WithAccessMode(journal.ReaderAccessReadAt)` only when mmap is undesirable
for diagnostics or constrained environments.

## Query Unique Values Through Indexes

<!-- verify-example: lang=go id=go-unique-values -->
```go
r, err := journal.OpenFile("/var/log/journal/example/system.journal")
if err != nil {
    return err
}
defer r.Close()

err = r.VisitUnique("SYSLOG_IDENTIFIER", func(value []byte) error {
    fmt.Println(string(value))
    return nil
})
if err != nil {
    return err
}
```

Use `QueryUnique` only when the caller needs an owned slice of all values.

## Explorer Query

Explorer is the API for filters, facets, histogram, FTS, and selected returned
rows.

<!-- verify-example: lang=go id=go-explorer-query -->
```go
r, err := journal.OpenFile("/var/log/journal/example/system.journal")
if err != nil {
    return err
}
defer r.Close()

query := journal.DefaultExplorerQuery().
    WithFilter([]byte("PRIORITY"), []byte("3"), []byte("4")).
    WithFacet([]byte("SYSLOG_IDENTIFIER")).
    WithHistogram([]byte("PRIORITY"))

result, err := r.Explore(query)
if err != nil {
    return err
}
fmt.Println(result.Stats.RowsMatched)
```

Default Explorer behavior:

- `ExplorerStrategyTraversal`;
- `ExplorerFieldModeFirstValue`;
- source realtime enabled;
- indexed filters;
- all-field expansion only for returned rows.

Do not set `DebugCollectColumnFieldsByRowTraversal` in production.

## Compare Explorer Strategies

The snippet continues from an open reader `r`.

<!-- verify-example: lang=go id=go-explorer-compare prelude=open-reader -->
```go
query := journal.DefaultExplorerQuery().
    WithFacet([]byte("PRIORITY"))
query.FieldMode = journal.ExplorerFieldModeAllValues
query.UseSourceRealtime = false
query.Limit = 0

result, err := r.ExploreWithStrategy(query, journal.ExplorerStrategyCompare)
if err != nil {
    return err
}
fmt.Println(result.Comparison.TraversalDuration)
fmt.Println(result.Comparison.IndexDuration)
```

Use compare mode to validate correctness and timing before selecting the index
strategy for a query shape.

## Write One File

Use direct-file writing when the caller owns the file lifecycle.

<!-- verify-example: lang=go id=go-write-one-file -->
```go
w, err := journal.Create("/var/log/journal-sdk/example.journal", journal.Options{})
if err != nil {
    return err
}
defer w.Close()

return w.Append([]journal.Field{
    journal.StringField("MESSAGE", "plugin started"),
    journal.StringField("PRIORITY", "6"),
    journal.StringField("SYSLOG_IDENTIFIER", "example-plugin"),
}, journal.EntryOptions{})
```

`Append` is the structured hot path for producers that already have field names
and values split.

## Write Binary Fields

The snippet continues from an open writer `w`.

<!-- verify-example: lang=go id=go-write-binary prelude=open-writer -->
```go
if err := w.Append([]journal.Field{
    journal.StringField("MESSAGE", "sample with binary payload"),
    {Name: "BINARY_PAYLOAD", Value: []byte{0x00, 0x01, 0x02, 0xff}},
}, journal.EntryOptions{}); err != nil {
    return err
}
```

Binary values are preserved as field values. The field name remains text.

## Raw Append

Use `AppendRaw` only when the caller already has `KEY=value` payloads. The
snippet continues from an open writer `w`.

<!-- verify-example: lang=go id=go-raw-append prelude=open-writer -->
```go
if err := w.AppendRaw([][]byte{
    []byte("MESSAGE=prebuilt payload"),
    []byte("_HOSTNAME=synthetic-host"),
    []byte("BINARY_PAYLOAD=\x00\x01\x02\xff"),
}, journal.EntryOptions{}); err != nil {
    return err
}
```

The first `=` byte splits the field name from the value. Later `=` bytes and
arbitrary value bytes are preserved.

## Directory Writer With Rotation And Retention

<!-- verify-example: lang=go id=go-directory-writer -->
```go
machineID, err := journal.ParseUUID("00112233445566778899aabbccddeeff")
if err != nil {
    return err
}
bootID, err := journal.NewUUID()
if err != nil {
    return err
}

log, err := journal.NewLog("/var/log/journal-sdk", journal.LogConfig{
    Source:       "example-plugin",
    OpenMode:     journal.LogOpenEager,
    IdentityMode: journal.LogIdentityStrict,
    Options: journal.Options{
        MachineID: machineID,
        BootID:    bootID,
        Compact:   true,
        LivePublishEveryEntries: journal.PublishEveryEntries(64),
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

`NewLog` stores files below `<directory>/<machine-id>/`. The active filename
uses chain naming by default. Set `StrictSystemdNaming: true` only when the
consumer needs `<source>.journal` active naming.

## Field-Name Policy

<!-- verify-example: lang=go id=go-field-name-policy -->
```go
w, err := journal.Create("/tmp/example.journal", journal.Options{
    FieldNamePolicy: journal.FieldNamePolicyJournald,
})
if err != nil {
    return err
}
defer w.Close()
```

Use:

- `FieldNamePolicyJournald` for trusted journald-like producers;
- `FieldNamePolicyJournalApp` for untrusted application-facing rules;
- `FieldNamePolicyRaw` only for file-format-level tools and tests.

`Raw` files are journal files, but stock systemd tooling is not guaranteed to
accept invalid systemd field names.

## Optional Writer Lock

Core writers do not lock. Acquire the optional cooperating-writer lock helper
when the deployment needs SDK-level exclusion.

<!-- verify-example: lang=go id=go-writer-lock -->
```go
lock, err := journal.AcquireWriterLock("/var/log/journal-sdk/example.journal")
if err != nil {
    return err
}
defer lock.Release()
```

This helper is independent from systemd compatibility.

## Netdata Function Boundary

Use the Netdata function API when the consumer needs Netdata-shaped logs
function output.

<!-- verify-example: lang=go id=go-netdata-function -->
```go
function := journal.SystemdJournalNetdataFunction()
request := map[string]any{
    "after":     0,
    "before":    0,
    "last":      200,
    "facets":    []any{"PRIORITY", "SYSLOG_IDENTIFIER"},
    "histogram": "PRIORITY",
}

response, err := function.RunDirectoryRequestJSONWithOptions(
    "/var/log/journal",
    request,
    journal.NetdataFunctionRunOptionsFromTimeoutSeconds(30),
)
if err != nil {
    return err
}
encoded, err := json.Marshal(response)
if err != nil {
    return err
}
fmt.Println(string(encoded))
```

Customize `NetdataFunctionConfig.SourceSelectorName` and
`SourceSelectorHelp` when the same function shape serves a domain-specific
journal backend. The wire id remains `__logs_sources`; only the label and help
shown by Netdata change.

<!-- verify-example: lang=go id=go-netdata-source-selector mode=build -->
```go
config := journal.SystemdJournalNetdataFunctionConfig()
config.SourceSelectorName = "Trap Jobs"
config.SourceSelectorHelp = "Select the trap job to query"
function := journal.NewNetdataJournalFunction(config, journal.SystemdJournalProfile{})
_ = function // run requests with it as in the previous example
```

This layer is Netdata-specific. Generic log explorers should use Explorer
directly unless they need the Netdata request and response shape.

## Verify A File

<!-- verify-example: lang=go id=go-verify-file -->
```go
if err := journal.VerifyFile("/var/log/journal/example/system.journal"); err != nil {
    return err
}
```

Use `VerifyFileWithKey` for sealed files when a verification key is available.
Verification is for integrity checks, not normal query serving.
