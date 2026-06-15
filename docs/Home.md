# systemd Journal SDK

Pure systemd journal file SDKs for Rust and Go, with compatibility surfaces for
Node.js and Python.

The SDK reads and writes journal files directly. It does not link to
libsystemd, does not call journal libraries, and does not require the local
host to run systemd. The production goal is stronger than compatibility alone:
the SDK should use journal-native indexes, offset arrays, reusable DATA
objects, mmap-backed data, and lazy decompression whenever those structures can
answer the request.

## What To Read First

- [[Getting-Started|Getting Started]]: install paths and the shortest route to
  the right API.
- [[API-Overview|API Overview]]: the API layers and when to use each one.
- [[Rust-API|Rust API]]: Rust examples for readers, writers, Explorer, and the
  Netdata function boundary.
- [[Go-API|Go API]]: Go examples for readers, writers, Explorer, and the
  Netdata function boundary.
- [[Hot-Path-Guide|Hot Path Guide]]: performance rules that affect production
  ingestion and query speed.
- [[Journalctl-CLI|Journalctl CLI]]: the file-backed journalctl rewrites for
  operators and scripts.

Every Rust and Go code example in this wiki is compiled and executed against
synthetic fixtures by repository CI, except blocks explicitly marked
illustrative-only.

## Production Rule

Use the narrowest API that matches the job.

- Structured producers should use structured append APIs.
- Consumers that only need immediate current-row `FIELD=value` payloads should
  use payload visitors. Go consumers needing row-level lifetime should use
  `EnumerateEntryPayload` because `VisitEntryPayloads` is callback-scoped.
- Field-name and unique-value queries should use FIELD/DATA indexes.
- Explorer queries should expand only fields needed for facets, histograms,
  FTS, or returned rows.
- Debug row-traversal options are not production options.

If an API path expands every row, decompresses unrelated DATA, or materializes
maps when the journal index can answer the request, treat it as a performance
bug unless a SOW records measured evidence for that choice.

## Published Wiki Source

The committed wiki source lives in `docs/`. The repository also contains
`documentation/` for internal project notes; that directory is not published to
the consumer wiki.
