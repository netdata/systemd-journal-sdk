# Rust Crates And Packages

## Public Package

Use `systemd-journal-sdk` for normal Rust integrations:

```toml
[dependencies]
journal = { package = "systemd-journal-sdk", version = "0.7.4" }
```

The alias keeps source imports in the form:

<!-- illustrative-only: import fragment shown alone -->
```rust
use journal::{FileReader, Log};
```

## Published Lower-Level Packages

The workspace also publishes internal layers for consumers that need direct
control. Prefer the public package unless the use case really needs one of
these surfaces.

| Package | Purpose | Normal Consumer Use |
|---|---|---|
| `systemd-journal-sdk-common` | Shared byte, time, and utility primitives. | Rare. Mostly for internal layering. |
| `systemd-journal-sdk-registry` | Journal repository path and naming helpers. | Rare. Use when building custom directory managers. |
| `systemd-journal-sdk-core` | Low-level journal file parser, mmap access, writer, verifier, compression, FSS primitives. | Direct-file readers/writers and maximum control. |
| `systemd-journal-sdk-log-writer` | High-level directory writer with rotation and retention. | Use for production log ingestion directories. |
| `systemd-journal-sdk-host` | Optional local-host machine ID, boot ID, and monotonic timestamp helpers. | Use only when the event source is intentionally the local host. |
| `systemd-journal-sdk-index` | Index/filter structures used by query engines. | Specialized query/index consumers. |
| `systemd-journal-sdk-engine` | Higher-level query/index engine building blocks. | Specialized query engines. |

## Recommended Rust Dependency Choices

- Application logging or ingestion: depend on `systemd-journal-sdk` and use
  `journal::Log`.
- Single journal file writer: depend on `systemd-journal-sdk` and use the
  direct-file APIs only when directory lifecycle is not needed. Low-level
  structured write types such as `EntryField` and `StructuredField` live in
  `systemd-journal-sdk-log-writer` and `systemd-journal-sdk-core`.
- Single file scanner: depend on `systemd-journal-sdk` and use
  `journal::FileReader`.
- Directory scanner: depend on `systemd-journal-sdk` and use
  `journal::DirectoryReader`.
- Netdata-shaped logs function: depend on `systemd-journal-sdk` and use
  `journal::netdata`.
- Local-host identity helper: depend on `systemd-journal-sdk-host` and import
  `journal_host`, then pass returned values to the writer explicitly.

Avoid depending directly on internal packages only to save compile time. That
creates a tighter coupling to internal layering without improving runtime
performance.

See [[Rust-API|Rust API]] for examples and [[API-Overview|API Overview]] for
the layer model.
