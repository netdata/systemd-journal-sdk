# Provenance

This repository contains source code copied from the Netdata project.

## Upstream Source

- **Repository**: `https://github.com/ktsaou/netdata` (Netdata fork by ktsaou)
- **Commit**: `6a515000ac89`

## Copied Paths

The following source directories were copied from the upstream Netdata repository at commit `6a515000ac89`:

| Local Path | Upstream Path | Description |
|------------|---------------|-------------|
| `rust/src/crates/jf/error/` | `src/crates/jf/error/` | Error types |
| `rust/src/crates/jf/journal_file/` | `src/crates/jf/journal_file/` | Journal file reader |
| `rust/src/crates/jf/journal_reader_ffi/` | `src/crates/jf/journal_reader_ffi/` | FFI bindings |
| `rust/src/crates/jf/window_manager/` | `src/crates/jf/window_manager/` | Window management |
| `rust/src/crates/jf/sigbus/` | `src/crates/jf/sigbus/` | SIGBUS handler |
| `rust/src/crates/journal-common/` | `src/crates/journal-common/` | Common types |
| `rust/src/crates/journal-core/` | `src/crates/journal-core/` | Core journal implementation |
| `rust/src/crates/journal-index/` | `src/crates/journal-index/` | Indexing functionality |
| `rust/src/crates/journal-log-writer/` | `src/crates/journal-log-writer/` | Log writer |
| `rust/src/crates/journal-registry/` | `src/crates/journal-registry/` | Registry/watch functionality |
| `rust/src/crates/journal-engine/` | `src/crates/journal-engine/` | Engine |

## License

The copied Netdata source code is licensed under the **GNU General Public License version 3 (GPL-3.0-or-later)**.

See the root `LICENSE` file for the full license text.

## Third-Party Dependencies

The imported Rust code depends on third-party crates from crates.io and git repositories:

- `flatten-serde-json`: MIT licensed, from `https://github.com/meilisearch/meilisearch` tag v1.22.1 (git dependency)
- `roaring`: Apache-2.0 licensed, from crates.io version 0.11 (local); upstream Netdata uses git branch `allocative` from `https://github.com/netdata/roaring-rs.git`

All other dependencies are from crates.io with their respective licenses.

## Modifications

The following minimal modifications were made to the copied source to enable building in this repository:

1. **Workspace edition**: Root `rust/Cargo.toml` uses edition `2024` for journal crates; nested `jf` workspace uses edition `2021` for JF sub-crates; `journal_reader_ffi` explicitly uses edition `2021` and builds successfully.
2. **Roaring version**: Local uses crates.io `0.11`; upstream Netdata uses `netdata/roaring-rs` git branch `allocative`. This is a known divergence.
3. **Workspace lints**: Added `[workspace.lints]` configuration to satisfy Rust build requirements.
4. **Internal crate path dependencies**: Added path dependencies for internal crate references.
5. **flatten-serde-json**: Added git dependency to restore the optional `serde-api` feature.

These modifications were limited to build configuration only and do not alter the functional behavior of the copied code.
