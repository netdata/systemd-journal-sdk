# Reader APIs

The reader API is intentionally layered. Each layer answers a different
consumer problem and carries a different cost.

## Quick Selection

| Consumer Need | Rust | Go | Use This When |
|---|---|---|---|
| one file, scan rows | `FileReader` | `Reader` | caller owns file ordering |
| many files, journal order | `DirectoryReader` | `DirectoryReader` | directory behaves like file-backed `journalctl` |
| current-row payload bytes | `visit_entry_payloads` | `VisitEntryPayloads` | callback can process `FIELD=value` bytes |
| row-lifetime DATA enumeration | `enumerate_entry_payload` | `EnumerateEntryPayload` | porting libsystemd-style DATA loops |
| convenient maps | `get_entry` | `GetEntry` | selected rows, not hot inner loops |
| fields and unique values | `enumerate_fields`, `visit_unique_values` | `EnumerateFields`, `VisitUnique` | use FIELD/DATA indexes |
| libsystemd-style port | `SdJournal*` facade | `SdJournalOpen*` functions returning a facade handle | compatibility with existing call shape |
| facets/histogram/FTS | Explorer | Explorer | log explorer or UI query |
| Netdata function output | `journal::netdata` | Netdata function API | Netdata request/response shape |
| stock-like CLI behavior | journalctl rewrite | journalctl rewrite | operator/script use; see [[Journalctl-CLI|Journalctl CLI]] |
| integrity validation | `verify_file` | `VerifyFile` | diagnostics and corpus gates |

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
- use payload visitors for hot scans;
- use field and unique APIs for index-backed metadata queries;
- use full entry maps only for rows that will be returned or displayed.

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
