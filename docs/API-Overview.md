# API Overview

This page explains the public API model. The language pages show exact Rust,
Go, Python, and Node.js code, and every example in those languages is compiled
or syntax-checked and executed against synthetic fixtures by repository CI.
Examples are contracts, not illustrations, unless a page explicitly marks a
block illustrative-only.

## Layer Map

```text
Application                          Operator / script
  |                                    |
  |                                    +-- journalctl rewrite (CLI)
  |                                          file-backed stock-like behavior
  |
  +-- Netdata function boundary
  |     Netdata request JSON -> Explorer -> Netdata response JSON
  |
  +-- Explorer API
  |     filters, facets, histogram, FTS, selected returned rows
  |
  +-- Facade API
  |     libsystemd-style reader calls for compatibility ports
  |
  +-- Idiomatic reader and writer APIs
  |     FileReader, DirectoryReader, Log, direct-file Writer
  |
  +-- Core file-format primitives
        mmap, journal objects, hash tables, offset arrays, compression, FSS
```

The layers are not separate implementations. Higher-level APIs reuse the same
core file parser and writer primitives. Choose the highest layer that matches
the contract you need without forcing extra decoding, allocation, or
presentation work.

## Choosing An API Surface

Start from the consumer problem, not from the lowest layer:

1. "I produce log entries" - structured append on the directory writer
   (`Log`). Use the direct-file writer only when the caller owns one file's
   lifecycle. Use raw append only when valid `KEY=value` bytes already exist.
2. "I read rows programmatically" - file or directory reader. Inside hot
   loops use payload visitors for immediate payload processing; materialize
   full entry maps only for rows that will be returned or displayed.
3. "I am porting libsystemd or sd_journal-style code" - the facade API. Keep
   the row-scoped data lifetime; do not add copies the original code did not
   have.
4. "I build a log explorer UI or API" - Explorer. Filters use indexes;
   only facets, histogram, FTS, and returned rows expand data.
5. "I serve Netdata logs functions" - the Netdata function boundary over
   Explorer.
6. "An operator or script needs journalctl behavior" - the
   [[Journalctl-CLI|journalctl rewrite CLI]].
7. "I must prove a file is intact" - verifier APIs. Verification is an
   integrity path, not a query path.

When two surfaces both work, prefer the one that decodes, allocates, and
presents less. The layers below explain what each surface costs.

## Reader Surfaces

| Surface | Best For | Performance Notes |
|---|---|---|
| payload visitor | scanning current-row `FIELD=value` bytes | avoids maps and copies uncompressed mmap data where the language can expose that path; Go `VisitEntryPayloads` is callback-scoped and Python visitors return owned `bytes` |
| file reader | one journal file with cursor, matches, metadata, fields | flexible, but full entry materialization is not the fastest path |
| directory reader | ordered reads across active and archived files | merges files in journal order |
| facade API | libsystemd-style ports | compatibility call shape over SDK reader primitives |
| Explorer | filters, facets, histogram, FTS, returned rows | expands only requested data where possible |
| Netdata function boundary | Netdata logs function request/response | Explorer plus Netdata request parsing and presentation |
| journalctl rewrite (CLI) | operator and script access without systemd | same reader paths; adds process startup and text/JSON output cost |
| verifier | integrity checks | correctness path, not a query path |

## Writer Surfaces

| Surface | Best For | Performance Notes |
|---|---|---|
| structured append | producers with field names and values already split | fastest normal producer path |
| raw append | callers that already have `KEY=value` bytes | preserves caller payloads; do not use as a detour from structured data |
| direct-file writer | caller owns one file lifecycle | lowest lifecycle overhead |
| directory writer | active file, rotation, retention, naming | production backend path |
| optional lock helper | cooperating SDK writer exclusion | explicit helper, not part of core writer construction |
| optional identity helper | host machine or boot identity discovery | explicit helper; core writers do not probe host identity |

## Field-Name Policy

The journal file structure can store more field names than stock systemd
tooling accepts. The SDK exposes three policy levels:

| Policy | Meaning | Stock systemd tooling |
|---|---|---|
| `RAW` | any non-empty field name without `=` | not guaranteed |
| `JOURNALD` | trusted journald-style field names, including protected `_` fields | intended to be friendly |
| `JOURNAL-APP` | untrusted application-facing journald rules, protected fields rejected | intended to be friendly |

Producer-specific remapping does not belong in the SDK. Apply OTEL, Netdata,
or application-specific transformations before calling the writer.

## Data Lifetime

Reader hot paths use row-scoped data lifetimes:

- uncompressed current-row DATA can be returned as borrowed bytes from the
  mmap-backed journal file;
- compressed DATA is decompressed into row-owned storage;
- returned current-row payloads stay valid until the reader advances, seeks,
  resets DATA enumeration, remaps, or closes;
- callers that need data after advancing must copy it.

This is the intended contract for high-throughput readers and facade ports. Go
`VisitEntryPayloads` is callback-scoped and does not provide this row-level
guarantee; use Go `EnumerateEntryPayload` when row-level lifetime matters.
Python's visitor shape returns owned `bytes` and therefore copies; use Python
`enumerate_entry_payload()` when row-scoped DATA access matters. Node.js uses
bounded positioned-read windows in the default package, so treat returned
current-row buffers as row-scoped even though the backing implementation is not
mmap.

## Explorer Query Model

Explorer is for log UI and API workloads:

```text
select files
  -> apply indexed filters
  -> traverse candidate rows only when needed
  -> update selected facets and histogram
  -> expand all fields only for returned rows
```

Filter fields use indexes. Facet and histogram fields need value expansion.
FTS needs payload inspection. Returned rows are expanded only after the rows are
selected.

See [[Explorer-And-Netdata-Queries|Explorer And Netdata Queries]] for the
details and strategy choices.

## Compatibility Model

The SDK writes journal files that compatible stock systemd tools can read when
the caller selects systemd-compatible options:

- valid systemd field-name policy;
- supported regular or compact format;
- supported compression algorithm;
- compatible active/archived state handling;
- caller-provided machine and boot identity when stable identity matters.

`RAW` mode is a file-format mode, not a stock systemd promise.

## Production Checklist

- Pick Rust or Go for high-throughput production ingestion or query paths.
- Use Python and Node.js for compatibility, automation, integration, and
  verified parity surfaces unless a workload benchmark proves they fit the
  production path.
- Use structured append unless payloads are already `KEY=value` bytes.
- Use payload visitors or Explorer instead of full entry maps inside hot loops;
  in Go, use `EnumerateEntryPayload` when row-level payload lifetime is needed.
- Use snapshot reader bounds for query workloads that do not need same-session
  appended rows.
- Keep debug-only Explorer row traversal disabled.
- Benchmark the exact API path and options the consumer will ship.
