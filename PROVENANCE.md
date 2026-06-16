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

- `roaring`: Apache-2.0 licensed, from crates.io version 0.11 (local); upstream Netdata uses git branch `allocative` from `https://github.com/netdata/roaring-rs.git`

All other dependencies are from crates.io with their respective licenses.

The optional Rust `journal-log-writer` `serde-api` JSON-flattening helper
mirrors the behavior of Meilisearch's MIT-licensed `flatten-serde-json`
implementation:

- **Repository**: `https://github.com/meilisearch/meilisearch`
- **Commit**: `077ec2ab11bb4daefcb57f89eab9cff16e075fdc`
- **Path**: `crates/flatten-serde-json/src/lib.rs`
- **License**: MIT, copyright 2019-2025 Meili SAS

The Git dependency was removed to avoid checking out the full upstream
monorepo in package builds. Local parity tests cover the upstream edge cases
used by this SDK.

The retired Node.js experiment vendors the WASM runtime files from
`node-liblzma` for XZ DATA object support:

- **Package**: `node-liblzma`
- **Version**: `5.0.1`
- **Repository**: `https://github.com/oorabona/node-liblzma`
- **Local path**: `experiments/node/vendor/node-liblzma-wasm/`
- **Included files**: `liblzma.js`, `liblzma.wasm`, `LICENSE`, `README.md`
- **License**: LGPL-3.0
- **Vendored file SHA-256 values**:
  - `liblzma.js`: `f33997f0c680a29fd307d18b8336325949811c78bb00ad9a038bf8f205623e02`
  - `liblzma.wasm`: `a9216b509c9bf0006f306e85f696bd67d31e4ca1972b9e35307aef8650fe705c`
  - `LICENSE`: `f97bc4bb9b7ae8a653941073678b5c7775e8de44a01c3bcc21e7cdc148b90e61`

Only the WASM runtime files are included. The full npm package's native addon
prebuilds and install hook are intentionally not included in this repository or
the retired Node.js experiment package.

## Modifications

The following minimal modifications were made to the copied source to enable building in this repository:

1. **Workspace edition**: Root `rust/Cargo.toml` uses edition `2024` for journal crates; nested `jf` workspace uses edition `2021` for JF sub-crates; `journal_reader_ffi` explicitly uses edition `2021` and builds successfully.
2. **Roaring version**: Local uses crates.io `0.11`; upstream Netdata uses `netdata/roaring-rs` git branch `allocative`. This is a known divergence.
3. **Workspace lints**: Added `[workspace.lints]` configuration to satisfy Rust build requirements.
4. **Internal crate path dependencies**: Added path dependencies for internal crate references.
5. **Serde JSON flattening**: Replaced the `flatten-serde-json` Git dependency
   with an in-crate helper for the optional `serde-api` feature, preserving the
   upstream behavior with local parity tests and recorded provenance.
6. **Node.js XZ runtime**: Replaced the full `node-liblzma` npm dependency with
   vendored WASM runtime files from `node-liblzma@5.0.1`, preserving
   systemd-compatible XZ `CHECK_NONE` output without native install hooks.

These modifications are intended to preserve the functional behavior of the
copied code while making the repository buildable and portable in this
standalone SDK layout.
