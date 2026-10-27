# Indexed Snapshots

Use `IndexedSnapshot` when an application needs exact field/value postings or
FIELD-value predicates over one fixed population, without allocating all entry
offsets at open. The application owns time semantics, aggregation and any union
across files. Existing file/directory readers, facades and Explorer keep their
existing contracts; this API does not change their snapshot behavior.

## Choose The Operation

| Operation | Work and result |
|---|---|
| Open/capture | Header, global array headers and declared FIELD heads/value counts; no entry-offset vector |
| Exact match | Native DATA hash lookup, then that value's original postings |
| FIELD predicate | Captured FIELD's distinct values, then accepted values' original postings |
| All entries | Lazy global ENTRY-array traversal; O(entries) work without a whole-file offset vector |
| Payload visitor | Decode only the visited entry's payloads; bytes live only during the callback |
| Strict verification | Offline graph certification with time and memory proportional to graph size |

Traversal scratch does not grow with the total entry count. Rust copies the current
entry's DATA offsets, so its scratch grows with the widest visited entry; payload
decompression also needs space for the selected value. Go uses bounded posting
and entry-offset chunks. These are not fixed total-memory guarantees.

FIELD traversal is a posting traversal: a row with two accepted values is
visited twice. There is no implicit deduplication, sorting or multi-file merge.
An application requiring a union must implement that contract explicitly.
Missing values return no rows. Missing/corrupt native indexes return errors;
there is no hidden full-row fallback. Discard partial aggregates after an error.

## Capture And Ownership

The caller MUST exclude append, rotation and truncation while opening each
snapshot. The SDK acquires no implicit lock. After capture, supported append-only
writers can resume; the snapshot continues to visit its original population.
Truncation and in-place modification are unsupported. For a consistent multi-file
query, open every file while holding the application's admission gate.

Declare FIELDs used by predicate traversal and exact values whose counts will be
needed. Declarations are copied. A captured value distinguishes absent from
present, and its count does not change after append. Asking for an undeclared
count or FIELD is an error. Exact-match traversal itself needs no declaration.

Go `IsArchived()` and Rust `is_archived()` report whether the captured header was
archived; online and offline states return false. The result remains fixed if
the file is archived after capture and requires no extra file read. Archived
state alone does not establish clean provenance or index integrity.

Entry views and payload slices MUST NOT be retained. Copy what is needed before
the callback returns. A snapshot has one consumer; nested snapshot operations
are unsupported. Rust scopes the borrows. Go detects nested operations and rejects
`VisitPayloads` on an expired entry view. Its exported metadata fields remain
readable, and retaining a reused view is still a caller error; copy needed
metadata during the callback. Independent queries use separate snapshots.

Context/control cancellation is checked between objects, chunks and payloads.
One syscall or decompression is cooperative, not interruptible. Regular and
compact layouts, historical unkeyed files and supported DATA compression work.
Whole-file `.journal.zst` inputs use staging and incur complete-stream
decompression at open, including concatenated and unknown-size frames; ordinary
`.journal` files use bounded reader windows. See [[Reader-APIs|Reader APIs]] for
compressed-input limits and errors.

## Go

This immutable fixture needs no writer exclusion. For an active file, hold the
application's writer gate through `OpenIndexedSnapshot`.

<!-- verify-example: lang=go id=go-indexed-snapshot -->
```go
ctx := context.Background()
snapshot, err := journal.OpenIndexedSnapshot(ctx,
    "/var/log/journal/example/system.journal",
    journal.IndexedSnapshotOptions{
        CaptureFields: [][]byte{[]byte("PRIORITY")},
        CaptureValues: []journal.Field{journal.StringField("PRIORITY", "6")},
    })
if err != nil { return err }
defer snapshot.Close()
count, err := snapshot.CapturedValue([]byte("PRIORITY"), []byte("6"))
if err != nil { return err }
fmt.Println("committed", snapshot.EntryCount(), "matching", count.EntryCount, "archived", snapshot.IsArchived())
return snapshot.VisitMatch(ctx, []byte("PRIORITY"), []byte("6"),
    func(entry *journal.SnapshotEntry) error {
        return entry.VisitPayloads(func(payload []byte) error {
            if bytes.HasPrefix(payload, []byte("MESSAGE=")) {
                fmt.Println(string(payload))
            }
            return nil
        })
    })
```

Zero `IndexedSnapshotOptions.Reader` uses the default production reader options.
Explicit nonzero reader options retain their access mode; bounds are always
snapshot bounds. `VisitField` accepts a value predicate;
`VisitEntries` streams all captured rows without allocating an offset vector.
`Close` is idempotent and releases resources even after traversal errors.

## Rust

<!-- verify-example: lang=rust id=rust-indexed-snapshot -->
```rust
use journal::{IndexedSnapshot, IndexedSnapshotOptions, SnapshotControl};
let control = SnapshotControl::default();
let mut snapshot = IndexedSnapshot::open(
    "/var/log/journal/example/system.journal",
    IndexedSnapshotOptions {
        capture_fields: vec![b"PRIORITY".to_vec()],
        capture_values: vec![(b"PRIORITY".to_vec(), b"6".to_vec())],
        ..Default::default()
    },
    &control,
)?;
let count = snapshot.captured_value(b"PRIORITY", b"6")?;
println!("committed {} matching {} archived {}", snapshot.entry_count(), count.entry_count, snapshot.is_archived());
snapshot.visit_match(b"PRIORITY", b"6", &control, |entry| {
    entry.visit_payloads(|payload| {
        if payload.starts_with(b"MESSAGE=") {
            println!("{}", String::from_utf8_lossy(payload));
        }
        Ok(())
    })
})?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Use `SnapshotControl { cancelled: Some(&predicate) }` for cooperative
cancellation. `visit_field` and `visit_entries` mirror the Go operations.
Resources are released on drop.

## Integrity Before Recovery

Unreleased ordinary readers recover an observed intact prefix by default and
report tail loss separately from normal EOF. That policy also applies to
ordinary readers with snapshot bounds. `IndexedSnapshot` capture retains strict
validation and does not adopt the ordinary-reader recovery policy. See
[[Reader-APIs|Reader APIs]] for status inspection and its limits.

Snapshot capture checks local bounds and requested metadata; it does not
certify every index edge. Its precondition is a valid native index graph.
An interrupted append can leave postings ahead of the committed global count.
A later append must not turn those orphan links into apparently valid history.

For uncertain files, call Go `VerifyIndex(ctx, path)` or Rust
`verify_index(path, &control)` while all writers are excluded, **before** opening
a writer or directory `Log`. Strict verification requires complete bidirectional
ENTRY/DATA membership, FIELD/hash coverage, consistent arrays and payload hashes.
It rejects incomplete publication, including a missing final reverse link.
Committed objects must fit within the header's declared arena, even when the
physical file contains extra preallocated bytes. An empty rotated file may
inherit a tail sequence counter, but must not claim current-file entry offsets,
head sequence or timestamps. Tail boot metadata follows the header version and
its compatibility flag.
These checks assume writer exclusion. Ordinary live readers instead bound their
mappings and payload reads by physical bytes: a writer may announce a larger
arena while still preparing the next entry. Such readers can read already
committed entries during that growth window. This tolerance does not apply to
indexed snapshot capture, strict verification or opening a writer for append.

It does not authenticate sealed TAGs; use the existing keyed verifier for that.
`VerifyFile`/`verify_file` retain their compatibility-oriented tolerance and do
not establish this stronger recovery guarantee.

Strict verification is an offline operation, not query-open work. It retains a
graph-sized working set; it is not constant-memory. Applications may avoid
re-verifying known finalized archives only with a documented provenance policy:
fixed writer lifecycle, successful archive file sync, distinct active/archive
names, and no external mutation. A schema marker alone is not proof of index
integrity. Always verify an uncertain active file, regardless of its header state.
Append-open performs bounded header and tail checks before mutation; those checks
do not replace strict verification of an uncertain file's complete graph.

After an uncertain mutating failure, writers reject further mutation and close
only resources, without publishing clean metadata or deleting an empty-looking
file. Go exposes `ErrWriterFailed`, preserving the original error; Rust exposes
its failed-writer error. Pre-mutation validation failures do not poison a writer.
No automatic repair, deletion or replay is part of strict verification. See
[[Writer-APIs|Writer APIs]] for lifecycle and durability choices.
