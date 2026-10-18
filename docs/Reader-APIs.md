# Reader APIs

The reader API is intentionally layered. Each layer answers a different
consumer problem and carries a different cost.

## Quick Selection

| Consumer Need | Rust | Go | Use This When |
|---|---|---|---|
| committed native postings | `IndexedSnapshot` | `IndexedSnapshot` | fixed population without eager entry offsets; see [[Indexed-Snapshots]] |
| one file, scan rows | `FileReader` | `Reader` | caller owns file ordering |
| many files, journal order | `DirectoryReader` | `DirectoryReader` | directory behaves like file-backed `journalctl` |
| immediate payload callback | `visit_entry_payloads` | `VisitEntryPayloads` | callback can process `FIELD=value` bytes |
| row-lifetime DATA enumeration | `enumerate_entry_payload` | `EnumerateEntryPayload` | porting libsystemd-style DATA loops |
| convenient maps | `get_entry` | `GetEntry` | selected rows, not hot inner loops |
| fields and unique values | `enumerate_fields`, `visit_unique_values` | `EnumerateFields`, `VisitUnique` | use FIELD/DATA indexes |
| libsystemd-style port | `SdJournal*` facade | `SdJournalOpen*` functions returning a facade handle | compatibility with existing call shape |
| facets/histogram/FTS | Explorer | Explorer | log explorer or UI query |
| Netdata function output | `journal::netdata` | Netdata function API | Netdata request/response shape |
| stock-like CLI behavior | journalctl rewrite | journalctl rewrite | operator/script use; see [[Journalctl-CLI]] |
| integrity validation | `verify_file` | `VerifyFile` | diagnostics and corpus gates |

## Native File Paths

Rust callers with native filesystem paths can use `SdJournalOpenPath`,
`SdJournalOpenFilePath`, `SdJournalOpenDirectoryPath` and `SdJournalOpenFilePaths`.
The file, directory and list forms also have `WithOptions` variants. These
constructors preserve non-UTF-8 Unix filenames; the existing string constructors
remain available. Go path strings already preserve Unix filename bytes.

Low-level Rust registry consumers should use `File::as_path()` for filesystem
operations. `File::path()` remains a text accessor for ordinary UTF-8 paths;
newly supported non-UTF-8 paths return an encoded metadata key. That key is
serializable and distinct from replacement-character filenames, and cannot be
opened as a filesystem path. Windows readers use Unicode filenames.

## Data Ownership

The reader has three ownership modes:

- borrowed current-row bytes: fastest path for uncompressed DATA;
- row-owned temporary bytes: used for compressed DATA that had to be
  decompressed;
- caller-owned maps/vectors: convenient but slower.

The row-level guarantee is:

```text
read row
  -> enumerate current-row payloads
  -> use or copy payloads
advance row
  -> previous current-row payloads are invalid
```

Consumers that keep values after advancing must copy them.

Go `VisitEntryPayloads` is not part of this row-level guarantee. Its callback
slice is valid only until the callback returns. Go consumers that need row-level
borrowed DATA must use `EnumerateEntryPayload`, and consumers that need longer
ownership must copy or use owned helpers.

## File Reader

Use the file reader when the caller controls one file and wants direct access to
entries, metadata, matches, field indexes, unique values, or Explorer.

Good uses:

- high-throughput row scans;
- exact cursor and realtime navigation;
- field-name and unique-value queries;
- single-file Explorer queries;
- verification or export on one file.

Performance rules:

- use snapshot bounds for query workloads that do not need appended rows;
- use payload visitors for hot scans when immediate callback processing is enough;
- use field and unique APIs for index-backed metadata queries;
- use full entry maps only for rows that will be returned or displayed.
- use verifier APIs only for integrity checks; file-path verification is
  separate from query opening. Strict index certification has graph-sized
  time and memory costs; sealed TAG/HMAC authentication is a separate check.

## Observed Tail Recovery

Unreleased source builds recover the intact entry prefix by default when an
interrupted append leaves a final ENTRY or ENTRY array unavailable. This applies
to ordinary file readers, directory readers and facades, including ordinary
readers with snapshot bounds. Iteration ends normally after that prefix: Go
`Step` returns `false, nil`, and Rust `next` returns `Ok(false)`.

Inspect diagnostics after reading:

| Surface | Rust | Go |
|---|---|---|
| One file | `recovery_status()` | `RecoveryStatus()` |
| File or directory collection | `recovery_statuses()` | `RecoveryStatuses()` |
| Facade | `SdJournal::recovery_statuses()` | `SdJournalRecoveryStatuses()` |

The per-file status reports the reason, offset associated with the observed
loss, declared entry count and recovered entry count. Collection records include
the file ID. A status value is a copy; collections allocate only when explicitly
requested. Header counters retain their declared values.

Detection is lazy where the native access path permits it. A zero/`None` status
means no loss has been observed; it does not certify integrity. A catalog query
does not validate unrelated entry arrays. Live refresh can clear the status
after observing a complete repair.

Tail recovery does not reclassify interior holes followed by readable entries,
wrong object types, cycles, malformed indexes, decompression failures or general
I/O failures as an interrupted append. Existing catalog compatibility fallbacks
remain available. A damaged secondary posting tail remains an error when its
unknown targets could include surviving entries. Discard incomplete aggregates
after such a failure.

Exact matches use DATA hash lookup and posting arrays in both languages. They
do not expand every row to find matching values. The same-field alternatives
are OR groups; different field groups are combined with AND. Check errors from
group operators and iteration: a missing required posting tail can fail even
when an unfiltered read can return an intact prefix.

Filtered movement selects the nearest match before or after the last successful
row, including after installing or replacing a filter on a directory reader.
Proven sequential directory traversal remains sequential after a filter change;
errors in a later file are discovered when traversal reaches that file.
Reversing after EOF also selects the nearest match. If loading a selected ENTRY fails,
the failed movement keeps the previous successful position. After repairing the
input, retry the movement to read that candidate.

Explicit filtered realtime seeks search native posting arrays: forward selects
the first matching entry at or after the timestamp, and backward selects the
last at or before it. These searches validate the slots and ENTRY headers they
visit. They do not inspect every skipped posting; use strict verification when
the whole index must be checked. Ordinary sequential filtering continues to
report malformed postings as it encounters them.

Group/build failures retain the pending matches for retry. After repairing the
input, retry the failed operation; flush matches to discard the pending builder.
Flushing also rewinds the reader to the head in both languages, including
directory and facade readers. Explicitly reposition afterward when continuing
from a saved location.

A built healthy filter retains the posting population it resolved. To include
newly published matches, flush and rebuild the filter. Recovery refresh can
requalify an existing DATA posting list after repair; a value absent when the
filter was built remains absent until rebuilding. Ordinary snapshot bounds cap
the byte extent; they do not create an immutable logical view. A new filter can
observe publications inside the preallocated original extent. Use an
`IndexedSnapshot` captured with writers excluded for a stable logical artifact.

Strict verification, excluded-writer `IndexedSnapshot` capture and append-open
retain their rejection rules. Use [[Indexed-Snapshots|Indexed Snapshots]] when
deciding whether an uncertain file is suitable for writable reuse.

## Directory Reader

Use the directory reader when the caller needs stock-like ordering across active
and archived files.

The directory reader:

- discovers journal files in the configured directory;
- opens supported `.journal`, `.journal~`, `.journal.zst`, and
  `.journal~.zst` files;
- merges files in journal order;
- supports file-backed match, seek, cursor, field, and unique behavior.

Directory reading costs more than one file because it must merge candidate
entries across files. Use a file reader when the consumer already knows the
exact file.

## Payload Visitor

Payload visitors are the reader hot path when the consumer can process raw
`FIELD=value` bytes.

They avoid:

- entry map allocation;
- repeated field/value splitting unless the callback does it;
- copying uncompressed DATA;
- repeated-value map materialization.

Use this path for scanners, digest tools, filters that inspect only a few
fields, and consumers that already parse `FIELD=value`.

## Stateful DATA Enumeration

The stateful current-row enumerator mirrors the libsystemd pattern:

```text
seek or step to row
restart DATA enumeration
while DATA exists:
  read FIELD=value payload
advance to next row
```

This is the correct compatibility path for code that used
`SD_JOURNAL_FOREACH_DATA`.

## Field Names And Unique Values

Field-name enumeration and unique-value queries must use journal indexes.

- Field names are stored as FIELD objects.
- Unique values for one field are reachable from that FIELD object's DATA
  chain.
- Row scans for unfiltered field names or unique values are regressions unless
  an active SOW records a compatibility fallback.

Use list-return APIs only when the caller needs the full owned result set. Use
visitor APIs when streaming is enough.

Unreleased source builds return an error for detected cycles in traversed hash,
FIELD DATA or ENTRY-array chains. This check uses the native indexes; opening a
file does not certify the
whole graph. Handle errors from query setup and from individual enumeration
steps, and discard incomplete aggregates after a failure.

## Facade API

The facade is for ports from libsystemd or Netdata `jf`-style code.

It supports:

- open file, directory, or file list;
- seek head, tail, realtime, or cursor;
- next, previous, and skip;
- match groups, disjunctions, and conjunctions;
- current-entry DATA enumeration;
- field and unique enumeration;
- realtime, monotonic, seqnum, cursor, and boot metadata.

Cursor metadata is emitted in the official systemd cursor shape. Cursor seek
and test also accept the older SDK cursor shape for compatibility with existing
callers.

The facade should reuse the row-level borrowed-data contract. It should not
copy every payload just because the call shape is libsystemd-like.

## Explorer

Explorer is not a generic row iterator. It is a query engine for log explorer
workloads:

- indexed filters;
- selected facets;
- selected histogram;
- optional FTS;
- selected returned rows;
- progress, cancellation, and timeout control.

See [[Explorer-And-Netdata-Queries|Explorer And Netdata Queries]].

## Formatting And Verification

Formatting APIs are output boundaries:

- export output uses systemd's size-prefixed binary field encoding;
- JSON preserves UTF-8 strings and encodes binary values as byte arrays;
- text output is display-oriented.

Verification reads the object graph to prove integrity. It is not a query hot
path.

## Compressed Input

With the unreleased reader hardening changes, both SDKs decode complete supported
Zstandard streams, including unknown-size,
concatenated and skippable frames. A bad checksum, truncated later frame or
trailing garbage returns an error. The DATA output limit is 768 MiB across all
frames belonging to that DATA object.

Whole-file `.journal.zst` and `.journal~.zst` inputs are SDK extensions. They are
streamed into staging storage before reading, so opening incurs full-file
decompression. The per-DATA limit does not cap the size of a whole journal.
