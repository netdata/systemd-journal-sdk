# API Overview

This page explains the public API model. The language pages show exact Rust
and Go code.

## Layer Map

```text
Application
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

## Reader Surfaces

| Surface | Best For | Performance Notes |
|---|---|---|
| payload visitor | scanning current-row `FIELD=value` bytes | avoids maps and copies uncompressed mmap data |
| file reader | one journal file with cursor, matches, metadata, fields | flexible, but full entry materialization is not the fastest path |
| directory reader | ordered reads across active and archived files | merges files in journal order |
| facade API | libsystemd-style ports | compatibility call shape over SDK reader primitives |
| Explorer | filters, facets, histogram, FTS, returned rows | expands only requested data where possible |
| Netdata function boundary | Netdata logs function request/response | Explorer plus Netdata request parsing and presentation |
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

This is the intended contract for high-throughput readers and facade ports.

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
- Use structured append unless payloads are already `KEY=value` bytes.
- Use payload visitors or Explorer instead of full entry maps inside hot loops.
- Use snapshot reader bounds for query workloads that do not need same-session
  appended rows.
- Keep debug-only Explorer row traversal disabled.
- Benchmark the exact API path and options the consumer will ship.
