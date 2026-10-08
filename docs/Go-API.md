# Go API

Every example on this page is compiled and executed by repository CI against
synthetic fixtures, except blocks marked illustrative-only.

Install the Go submodule:

<!-- illustrative-only: registry install command -->
```sh
go get github.com/netdata/systemd-journal-sdk/go@v0.9.0
```

Import the journal package:

<!-- illustrative-only: import fragment shown alone -->
```go
import "github.com/netdata/systemd-journal-sdk/go/journal"
```

Callers that intentionally want local-host identity can also import the
optional helper package `github.com/netdata/systemd-journal-sdk/go/journalhost`
and pass its returned values to the writer explicitly. Core writers never
import or call that helper automatically. On Linux, containerized callers can
set `journalhost.LoadOptions.HostFilesystemPrefix` to a mount such as `/host`
when they intentionally want host machine identity instead of container-local
identity. Missing host files fall back to container-local files; present invalid
host files return an error so collectors do not silently switch identity.
`Provider.Diagnostics().MachineIDSource` reports the selected Linux path, for
example `linux:/etc/machine-id` or `linux:/host/etc/machine-id`, so callers can
log which identity source was used.

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
to inspect. `VisitEntryPayloads` is callback-scoped in Go: the `[]byte` passed
to the callback is valid only until that callback returns. It does not provide
the row-level lifetime guarantee. Use `EnumerateEntryPayload` for row-level
borrowed payloads, or copy/use `CollectEntryPayloads` when data must be kept.

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

The default Go reader uses mmap-backed live bounds on supported Unix-family and
Windows targets. Use snapshot bounds when a query may ignore entries appended
after it starts.

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

`ReaderAccessReadAt` is not a production reader mode. It is retained only for
tests, diagnostics, constrained-platform investigation, and controlled fallback
evidence. If `ReaderAccessAuto` selects read-at in production, treat that as a
deployment issue to investigate and benchmark before accepting.

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
Directory-level unique enumeration builds an exact 8-entry per-open-reader LRU
cache from per-file FIELD/DATA chains and reuses it for repeated queries or
facade stateful restarts while the already-open file set's header signatures
stay unchanged. The entry count is bounded, but each cache entry keeps the full
exact unique set for one field.

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
machineID, err := journal.ParseUUID("00112233445566778899aabbccddeeff")
if err != nil {
    return err
}
bootID, err := journal.ParseUUID("ffeeddccbbaa99887766554433221100")
if err != nil {
    return err
}
w, err := journal.Create("/var/log/journal-sdk/example.journal", journal.Options{
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

`Append` is the structured hot path for producers that already have field names
and values split.

## Write Binary Fields

The snippet continues from an open writer `w`.

<!-- verify-example: lang=go id=go-write-binary prelude=open-writer -->
```go
if err := w.Append([]journal.Field{
    journal.StringField("MESSAGE", "sample with binary payload"),
    {Name: "BINARY_PAYLOAD", Value: []byte{0x00, 0x01, 0x02, 0xff}},
}, journal.EntryOptions{
    RealtimeUsec:     1_700_000_000_000_001,
    RealtimeUsecSet:  true,
    MonotonicUsec:    2,
    MonotonicUsecSet: true,
}); err != nil {
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
}, journal.EntryOptions{
    RealtimeUsec:     1_700_000_000_000_002,
    RealtimeUsecSet:  true,
    MonotonicUsec:    3,
    MonotonicUsecSet: true,
}); err != nil {
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
}, journal.EntryOptions{
    RealtimeUsec:     1_700_000_000_000_003,
    RealtimeUsecSet:  true,
    MonotonicUsec:    4,
    MonotonicUsecSet: true,
})
```

`NewLog` stores files below `<directory>/<machine-id>/`. The active filename
uses chain naming by default. Set `StrictSystemdNaming: true` only when the
consumer needs `<source>.journal` active naming.

By default, `Log` syncs each archived journal file on the caller path during
rotation, explicit close, and stale-active startup archive. Latency-sensitive
callers may set `SyncOnArchive: journal.SyncOnArchive(false)` in `LogConfig` to
skip that archive-file sync. With the opt-out, the caller owns archived-file
durability before relying on side indexes or allowing retention to delete
archived files.

`Log.Close()` applies retention when it archives an active writer, except when
strict naming discards an empty file. An unopened lazy log also skips retention.
`Log.CloseWithoutRetention()` is available in Go `v0.8.2` and later. Use it when
closing before a policy change: it follows the same archive/durability path as
`Close()` and skips retention. The caller must
reopen with the new policy or enforce retention separately. Both methods
remove an empty active file with strict systemd naming, retain its archive with
chain naming, and are idempotent after closure; calling `Close()`
after `CloseWithoutRetention()` does not retroactively apply the old policy.

## Dedicated Root Retention

This feature is unreleased.

Use `LogConfig.RootRetention: true` when one caller owns a dedicated root and
source across machine identity changes. This opt-in requires
`StrictSystemdNaming: true` and does not accept an `ArtifactSizer`. The default
Log behavior remains machine-local, committed-byte and archive-head-age based.

The caller MUST serialize Log calls and directory changes, exclude independent
writers, and verify the provenance and index integrity of recovered active
files before `NewLog`. Owned files must use nondecreasing saved realtime (as
Log does). `InspectRootRetention(root, source)` supplies header-only inventory
before open. After open, use `log.InspectRootRetention()` for queries and status,
including after writer failure: it also checks that the active path still names
the live writer's filesystem file and journal identities. Missing or replaced
live files are errors, not empty history; inventory errors alone do not poison
the writer. The method requires an open root-retention Log. Both forms validate
canonical machine directories, filename/header machine and sequence identities,
archive state and header bounds. It rejects source quarantine names, symlinks,
ambiguous identities and malformed candidates before pruning. Unrelated caller
metadata directories, such as `identity/`, are permitted when they contain no
source candidates and must be readable so inventory can establish that exclusion. A missing or inaccessible root is an error, not zero history.
This inventory does not verify payloads/indexes or certify provenance. Checks
run at inventory/maintenance boundaries and do not protect against concurrent
external modification. Canonical machine-named entries must be directories.

<!-- verify-example: lang=go id=go-root-retention -->
```go
machineID, err := journal.ParseUUID("00112233445566778899aabbccddeeff")
if err != nil { return err }
bootID, err := journal.ParseUUID("ffeeddccbbaa99887766554433221100")
if err != nil { return err }
log, err := journal.NewLog("/var/log/journal-sdk", journal.LogConfig{
    Source: "example-history",
    Options: journal.Options{MachineID: machineID, BootID: bootID},
    StrictSystemdNaming: true,
    RootRetention: true,
    RotationPolicy: journal.RotationPolicy{}.WithMaxDuration(24 * time.Hour),
    RetentionPolicy: journal.RetentionPolicy{}.
        WithMaxAge(30 * 24 * time.Hour).WithMaxBytes(1024 * 1024 * 1024),
})
if err != nil { return err }
defer log.Close()
if err := log.Append([]journal.Field{journal.StringField("MESSAGE", "saved")},
    journal.EntryOptions{MonotonicUsec: 1}); err != nil { return err }
// Install a valid replacement before maintenance; invalid policies change nothing.
if err := log.SetRootRetentionPolicy(journal.RetentionPolicy{}.
    WithMaxAge(30 * 24 * time.Hour).WithMaxBytes(2 * 1024 * 1024 * 1024)); err != nil {
    return err
}
result, err := log.MaintainRootRetention(time.Now())
if err != nil { return err }
inventory, err := log.InspectRootRetention()
if err != nil { return err }
fmt.Println(inventory.Bytes, len(inventory.Files), result.LastSuccessfulAt)
```

Root maintenance counts full directory-visible file lengths, including
preallocation, across all owned machine directories. Age eviction uses each
file's tail saved realtime; size/file-count eviction chooses oldest tail first,
then canonical path as a deterministic identity/sequence tie-breaker. Saved
realtime is not producer event time. Whole-file expiry is not an exact row TTL;
a live file is protected from size pressure, and an append/allocation can exceed
the allowance. With the example's 24-hour file span and successful hourly
maintenance, age overhang is about one day plus one hour under advancing clocks.
The SDK does not schedule maintenance. Existing files keep their actual spans.
New SDK files allocate at least 8 MiB; existing files are counted at their
actual length, and smaller allowances are accepted. Once an active file is
finalized, it loses size protection: a smaller allowance or policy shrink can delete even the newest
file in the same maintenance pass. There is no newest-file grace period.

`NewLog` applies root maintenance even for lazy archived-only histories.
Automatic cleanup also runs on active-file creation, rotation and `Close` of
a nonempty active file. Closing an unopened or empty Log does not sweep history.
`CloseWithoutRetention` finalizes the active file without pruning. Explicit
`MaintainRootRetention(now)` (or `EnforceRetention()`) finalizes an idle active
only when its tail expires or a changed derived size policy needs fresh
allocation geometry; empty files are discarded. Ordinary idle sweeps keep the
same file, and the successor stays lazy. Even eager construction may therefore
return without an active file after startup maintenance finalizes a recovered
file. Verified retired-machine active files are archived through the existing
writer lifecycle. `SetRootRetentionPolicy`
installs copied valid limits without applying either old or new limits;
`RootRetentionPolicy()` returns a copy. Policy-derived hash-table sizing is
recomputed from the new allowance; caller-explicit hash buckets or allocation
size remain fixed. Maintenance compares the live allocation geometry with the
effective policy, so edits reverted before maintenance do not force an archive.
Each newly created successor receives creation cleanup. Explicit rotation limits
stay fixed.

Successful root finalization emits `LogLifecycleArchived` before any retention
deletion, including retired-file maintenance and close. Its `ArchivedPath` names
the finalized file and `ActivePath` is empty; ordinary rotations retain their
existing `LogLifecycleRotated` event with both paths. Empty-file disposal is not
an archive event.

Creation cleanup also runs in default mode when a retry successfully creates
a successor after an earlier safe creation failure. This corrects the previous
case where that retry could skip retention.

Safe inventory/unlink/directory-sync errors are maintenance failures and do not
turn successful appends into failures. Read `LastRootRetentionResult()` after
automatic boundaries: it retains `AttemptedAt`, `LastSuccessfulAt`, `Err`, and a
post-attempt inventory only when `InventoryValid` is true. Explicit maintenance
also returns the error. Returned inventories are independent copies of retained
status. Unknown inventory must not be displayed as zero. Failure to open or
validate a retired file before mutation leaves the current writer healthy.
Uncertain writer/archive mutations still fail the Log and preserve evidence;
writer failure must be reported separately. Failed maintenance can leave excess
bytes indefinitely. Readers holding a removed file may continue reading it;
those pinned bytes remain allocated until readers close and are excluded from
directory-visible accounting. Windows opens permit delete sharing, but runtime
unlink behavior must be validated on the consumer's supported Windows setup.

## Field-Name Policy

<!-- verify-example: lang=go id=go-field-name-policy -->
```go
machineID, err := journal.ParseUUID("00112233445566778899aabbccddeeff")
if err != nil {
    return err
}
bootID, err := journal.ParseUUID("ffeeddccbbaa99887766554433221100")
if err != nil {
    return err
}
w, err := journal.Create("/tmp/example.journal", journal.Options{
    MachineID:       machineID,
    BootID:          bootID,
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
File-path verification uses the same bounded reader access architecture as
normal file reads, so it avoids whole-file resident buffers while still walking
the object graph and sealed HMAC ranges.
