# Reader APIs

## Reader Layers

```text
Consumer
  |
  +-- FileReader / DirectoryReader
  |     idiomatic entries, metadata, matches, field and unique queries
  |
  +-- Payload visitor
  |     current-entry borrowed FIELD=value bytes, low allocation
  |
  +-- Facade API
  |     libsystemd-like open/seek/next/get_data/query_unique behavior
  |
  +-- Explorer API
  |     filters, facets, histogram, FTS, returned rows
  |
  +-- Formatter / file-backed journalctl
        text, json, export, verify, query CLI behavior
```

All reader paths ultimately use the same journal file parser and object access
primitives. Choose the layer that returns exactly what the consumer needs.

## FileReader

Use `FileReader` for one journal file when the caller controls ordering and
query shape.

Good for:

- high-throughput scans;
- exact cursor/realtime navigation;
- direct field or unique-value queries;
- Explorer queries on one file;
- verification and export helpers.

Hot-path notes:

- default Rust readers use live/windowed mmap;
- snapshot bounds are better when the caller accepts the file as it existed at
  open time;
- field-name enumeration should walk FIELD indexes;
- unique-value enumeration should walk one FIELD object's DATA chain;
- entry materialization into maps is a convenience path, not the fastest scan.

## DirectoryReader

Use `DirectoryReader` when the caller needs stock-like file-backed directory
ordering across active and archived files.

Good for:

- mixed directories containing regular, compact, compressed, sealed, and
  unsealed files;
- multi-file cursor/realtime ordering;
- file-backed `journalctl --directory` behavior.

Directory traversal is more expensive than one file because the reader must
select files, merge order, and de-duplicate cross-file unique values where
needed.

## Payload Visitor

Use payload visitors when the consumer already works with `FIELD=value` bytes.

This path avoids:

- building string maps;
- copying uncompressed DATA;
- splitting values unless the callback does it;
- materializing repeated-value structures.

Uncompressed payloads are borrowed from mmap-backed journal data where the
language can support that safely. Compressed DATA must be decompressed into
row-owned storage before being returned.

## Facade API

Use the facade when porting code that expects libsystemd-style behavior:

- open file, directory, or explicit file list;
- seek head, tail, realtime, or cursor;
- next, previous, skip;
- add matches, conjunctions, and disjunctions;
- enumerate current-entry DATA;
- enumerate fields and unique values;
- read realtime, monotonic, seqnum, boot, and cursor metadata.

Current-entry facade DATA enumeration uses the same row-scoped borrowed payload
contract as the lower-level reader path where the language can support it
safely. The facade is compatibility-oriented in call shape, not a license to
copy every payload.

New code that does not need libsystemd-compatible call shapes should still use
the idiomatic reader or Explorer API.

## Export, JSON, And Text

Formatting APIs are for output generation:

- export output uses systemd's size-prefixed binary field encoding;
- JSON preserves UTF-8 strings and encodes binary values as byte arrays;
- text output is display-oriented.

Do not use formatters as an internal data API for high-throughput pipelines.
They do presentation work that the hot path does not need.

## Verification

Verification reads the object graph to prove file integrity. It is a correctness
tool, not a fast query path. Use it at ingestion boundaries, corpus validation,
or diagnostics, not inside normal read loops.
