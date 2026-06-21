# Journalctl CLI

Each product SDK language ships a file-backed `journalctl` rewrite. It is the fifth
consumption surface: a command-line tool, not a library API. Use it when an
operator or a script needs stock-like `journalctl` behavior against journal
files or directories without systemd, libsystemd, or a running journald.

The Rust and Go rewrites implement the same file-backed contract and are
validated against stock `journalctl` by the shared interoperability matrices: directory
traversal, mixed-format directories, query/follow behavior, and verification.

## Scope

The rewrites cover file-backed query behavior only:

- input selection: `--file <path>` or `--directory <path>`;
- output: all stock v260.1 file-backed modes: `short`, `short-full`,
  `short-iso`, `short-iso-precise`, `short-precise`, `short-monotonic`,
  `short-delta`, `short-unix`, `verbose`, `export`, `json`,
  `json-pretty`, `json-sse`, `json-seq`, `cat`, and `with-unit`;
- short-style labels: hostname, identifier/unit, and PID are rendered from
  journal fields like stock file-backed `journalctl`; `--no-hostname`
  suppresses only the hostname component;
- field projection: `--output-fields` for `verbose`, `export`, `json*`, and
  `cat`, with stock metadata retention for JSON/export modes;
- metadata: `--list-boots`, `--list-invocations`, `--fields`,
  `--field <NAME>` (unique values), and `--header`;
- filtering: `FIELD=value` matches, repeated same-field matches as OR,
  different fields as AND, and the `+` disjunction separator;
- journalctl filters: `--identifier`, `--priority`, `--facility`, `--grep`,
  `--case-sensitive`, `--dmesg`, `--unit`, `--user-unit`, `--invocation`,
  and `-I`;
- unit filters: exact and glob system/user units use the same journal fields
  and disjunction groups as stock file-backed `journalctl`;
- time and boot windows: `--since`, `--until`, `--boot [ID]`;
- cursors: `--cursor`, `--after-cursor`, `--cursor-file`, and
  `--show-cursor`, using official systemd cursor strings;
- paging: `--head <n>`, `--tail <n>`, `--no-tail`;
- live reads: `--follow` on actively appended files and directories;
- utility actions: `--new-id128` and `--disk-usage` for explicit
  `--file`/`--directory` inputs;
- integrity: `--verify`, and `--verify-key <key>` for sealed files, where
  `<key>` is the systemd-style verification key `<seed>/<start>-<interval>`
  produced when sealing was set up (the same value stock
  `journalctl --verify-key` accepts).

Host and daemon operations are out of scope by design. `--sync`, `--flush`,
`--rotate`, `--relinquish-var`, and `--setup-keys` return a controlled error
in every language.

## Running Each Rewrite

<!-- illustrative-only: build/run commands depend on local toolchains and paths -->
```sh
# Rust (builds the journalctl binary from the workspace)
cargo run --manifest-path rust/Cargo.toml --bin journalctl -- \
    --file ./fixtures/system.journal --output json PRIORITY=3

# Go
go run ./go/cmd/journalctl --directory ./journal-dir --list-boots
go run ./go/cmd/journalctl --file ./fixtures/system.journal --disk-usage
go run ./go/cmd/journalctl --new-id128

```

The Rust and Go rewrites accept the same flags and produce matching output for
the shared matrices' covered behavior. When outputs disagree, treat it as an SDK
bug and report it.

## When To Use The CLI Versus The APIs

| Need | Use |
|---|---|
| operator inspection of journal files | journalctl rewrite |
| scripted text/JSON/export extraction | journalctl rewrite |
| integrity check in a pipeline | journalctl rewrite `--verify` |
| application reading rows programmatically | reader APIs |
| log explorer queries (facets, histogram, FTS) | Explorer API |
| Netdata-shaped function output | Netdata function boundary |

The CLI is a consumer of the same SDK reader paths; it adds no private
capabilities. Anything the CLI can do, the library APIs can do with more
control.
