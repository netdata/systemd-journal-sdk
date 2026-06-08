# systemd Journal SDK

Pure systemd journal file readers, writers, and file-backed `journalctl`
implementations for Rust, Go, Node.js, and Python.

## Purpose

This project provides SDKs that can read and write systemd journal files without
linking to libsystemd or other external journal libraries. Files written by one
language are expected to be readable by the other SDKs and by compatible stock
systemd tools when the selected field policy and file options target systemd
compatibility.

## Performance Contract

Compatibility is necessary but not sufficient. The SDK must also exercise the
journal format's native performance capabilities.

Reader and query APIs must use journal-native structures whenever they answer
the request:

- FIELD and DATA hash tables;
- FIELD hash traversal for field-name enumeration;
- FIELD DATA chains for unique values;
- DATA entry arrays and ENTRY arrays for indexed row selection;
- reusable DATA object identities to avoid repeated `FIELD=value` parsing;
- mmap-backed slices where supported and safe;
- lazy decompression and value materialization.

An implementation that scans all rows, repeatedly parses reusable DATA objects,
decompresses irrelevant DATA, or allocates in a hot path when the format exposes
a cheaper path is a regression unless a SOW records measured evidence and an
explicit accepted reason.

## Compatibility Scope

- Core readers and writers operate on explicit caller-provided paths, bytes,
  timestamps, IDs, and options.
- Writers use journald's `0640` file permission default for newly-created
  journal files and expose explicit per-language overrides for consumers that
  need a different mode. POSIX creation modes remain subject to the caller's
  process umask, matching normal systemd/open semantics.
- Core runtime paths do not discover host identity, execute external programs,
  or acquire writer locks implicitly.
- Systemd/journald compatibility policy, optional host identity discovery, and
  optional cooperating-writer locks are separate layers.
- Daemon-only `journalctl` operations are out of scope.

## Languages

- `rust/` - Rust SDK and journalctl implementation.
- `go/` - Go SDK and journalctl implementation.
- `node/` - Node.js SDK and journalctl implementation.
- `python/` - Python SDK and journalctl implementation.

Shared tests, interoperability matrices, corpus evaluation tooling, and
benchmarks live under `tests/`.

## Documentation

Consumer documentation lives under `docs/` and is published to the repository
GitHub wiki. Start with [docs/Home.md](docs/Home.md) for API selection,
hot-path guidance, production profiles, and wiki publishing details.

The `documentation/` directory contains project/internal operational notes.
It is not the consumer wiki source.

## Rust Package

The Rust SDK is published as the crates.io package `systemd-journal-sdk`.
Consumers that want the existing `journal::...` crate path should use a Cargo
dependency alias:

```toml
[dependencies]
journal = { package = "systemd-journal-sdk", version = "0.6.0" }
```

Advanced Rust consumers that need lower-level building blocks can also depend
on the project-prefixed internal packages:

- `systemd-journal-sdk-common`
- `systemd-journal-sdk-core`
- `systemd-journal-sdk-registry`
- `systemd-journal-sdk-log-writer`
- `systemd-journal-sdk-index`
- `systemd-journal-sdk-engine`
