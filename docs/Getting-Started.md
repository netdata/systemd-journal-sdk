# Getting Started

## Scope

The SDK works with journal files directly:

- read `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst` files;
- write regular or compact journal files;
- read and write zstd, xz, and lz4 DATA compression where implemented;
- verify journal files and sealed journal TAG/HMAC data where supported;
- provide file-backed `journalctl` behavior for files and directories.

The SDK does not implement daemon-only `journalctl` operations such as daemon
sync, flush, rotate, or relinquish-var.

## Install Rust

Use the public Rust package for normal integrations:

```toml
[dependencies]
journal = { package = "systemd-journal-sdk", version = "0.7.1" }
```

The alias keeps imports short:

<!-- illustrative-only: import fragment shown alone -->
```rust
use journal::{FileReader, Log};
```

See [[Rust-API|Rust API]] for examples and
[[Rust-Crates-And-Packages|Rust Crates And Packages]] for the published crate
layout.

## Install Go

```sh
go get github.com/netdata/systemd-journal-sdk/go@v0.7.1
```

Then import:

<!-- illustrative-only: import fragment shown alone -->
```go
import "github.com/netdata/systemd-journal-sdk/go/journal"
```

The Go module is pure Go and does not use CGO. See [[Go-API|Go API]] for
examples.

## Pick The Reader API

| Need | Start With |
|---|---|
| One file, row scan, maximum speed | payload visitor for immediate processing |
| One file, convenient row maps | `FileReader` / `Reader` entry APIs |
| Ordered reads across a directory | `DirectoryReader` |
| Porting libsystemd-style code | facade API |
| Facets, histogram, filters, FTS, returned rows | Explorer API |
| Netdata-shaped logs function output | Netdata function boundary |
| Operator or script, no code | [[Journalctl-CLI|journalctl rewrite CLI]] |
| Integrity check | verifier APIs |

Details are in [[API-Overview|API Overview]] and
[[Reader-APIs|Reader APIs]].

## Pick The Writer API

| Need | Start With |
|---|---|
| Structured producer | structured append |
| Caller already owns `KEY=value` bytes | raw append |
| One file under caller lifecycle | direct-file writer |
| Directory backend with rotation and retention | high-level directory writer |
| Tamper-evident output | writer plus FSS options |
| SDK-level writer exclusion | optional lock helper around writer use |

Details are in [[Writer-APIs|Writer APIs]] and
[[Options-Reference|Options Reference]].

## Runtime Purity

Core readers and writers operate on caller-provided paths, bytes, options,
timestamps, machine IDs, boot IDs, and metadata. They do not discover host
identity, run external programs, read `/proc`, or acquire writer locks
implicitly.

Host identity discovery and cooperating-writer locks are optional helpers. Use
them only when the integration explicitly needs them.

## Production Defaults

For high-throughput ingestion:

- use Rust or Go;
- use structured append when the producer already has structured fields;
- use compact format only after target-reader compatibility is validated;
- disable compression unless disk footprint is the bottleneck;
- tune live publication only when stock live-follow freshness does not matter.

For high-throughput reads:

- use mmap-backed readers on platforms that support them;
- use snapshot bounds for query workloads where appends after query start do
  not matter;
- use payload visitors for immediate processing, or Explorer instead of full
  entry materialization.

See [[Hot-Path-Guide|Hot Path Guide]] before choosing production options.
