# systemd Journal SDK

Pure systemd journal file SDKs for Rust, Go, Node.js, and Python.

The SDK reads and writes journal files without linking to libsystemd or other
system journal libraries. Compatibility is required, but the project goal is
also high performance: readers and writers should use journal-native indexes,
offset arrays, reusable DATA objects, mmap-backed data, and lazy decompression
whenever those structures can answer the request.

## Start Here

- [Getting Started](Getting-Started.md)
- [Rust Crates And Packages](Rust-Crates-And-Packages.md)
- [Reader APIs](Reader-APIs.md)
- [Writer APIs](Writer-APIs.md)
- [Explorer And Netdata Queries](Explorer-And-Netdata-Queries.md)
- [Hot Path Guide](Hot-Path-Guide.md)
- [Production Profiles](Production-Profiles.md)
- [Options Reference](Options-Reference.md)
- [Wiki Publishing](Wiki-Publishing.md)

`docs/` is the committed consumer wiki source. The repository also contains
`documentation/` for project/internal operational notes; that directory is not
published to the consumer wiki.

## Production Rule

Use the narrowest API that matches the job:

- writers with structured fields should use structured append APIs;
- readers that only need `FIELD=value` payloads should use payload visitors;
- field enumeration and unique values should use FIELD/DATA indexes;
- Explorer queries should expand only fields needed for facets, histogram, FTS,
  or returned rows;
- debug row-traversal options are not production options.

If a path expands every row, decompresses unrelated DATA, or materializes maps
when the journal index can answer the request, treat that as a performance bug
unless a SOW records measured evidence for that choice.
