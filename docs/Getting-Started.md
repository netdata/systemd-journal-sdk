# Getting Started

## What This SDK Does

The SDK works with journal files directly:

- read existing `.journal`, `.journal~`, `.journal.zst`, and `.journal~.zst`
  files;
- write regular or compact journal files;
- read and write DATA compression where implemented;
- verify journal files and sealed journal TAG/HMAC data where supported;
- provide file-backed `journalctl` behavior for files and directories.

The SDK does not implement daemon-only `journalctl` commands such as sync,
flush, rotate, or relinquish-var.

## Layer Model

The project separates four concerns:

- core file-format SDK: parses and writes journal file structures only;
- systemd/journald compatibility layer: applies journald-compatible field
  naming and API conventions;
- optional identity helpers: discover host identity only when the caller asks;
- optional writer-lock helpers: enforce cooperating SDK writers only when the
  caller asks.

Core readers and writers do not discover host identity, run external programs,
or acquire writer locks implicitly.

## Rust Install

The public Rust package is `systemd-journal-sdk`. To keep the existing
`journal::...` import path:

```toml
[dependencies]
journal = { package = "systemd-journal-sdk", version = "0.6.0" }
```

The lower-level Rust packages are documented in
[Rust Crates And Packages](Rust-Crates-And-Packages.md).

## Go Import

```go
import "github.com/netdata/systemd-journal-sdk/go/journal"
```

The Go module is pure Go and does not use CGO.

## Node.js And Python

Node.js and Python implementations exist for parity and compatibility testing.
Rust and Go are the high-throughput production targets today. Use Node.js or
Python only after checking the current performance envelope for the workload.

The current writer certification envelope puts Node.js and Python around
0.9k-1.0k append rows/s on the shared 32-field benchmark, while Rust and Go
were around 45k-59k append rows/s depending on options and hardware state.
Treat those figures as a warning about production fit, not as a universal
benchmark.

## First Reader Choice

- One file and maximum speed: use the file reader payload visitor or Explorer.
- Many files ordered like stock file-backed journalctl: use directory reader.
- Porting libsystemd-style reader code: use the facade API.
- One-shot export/text/json output: use the formatter or file-backed
  journalctl command.

## First Writer Choice

- High-throughput structured producer: use structured append.
- Caller already has `KEY=value` byte payloads: use raw append.
- One file lifecycle controlled by the caller: use direct-file writer.
- Directory with rotation and retention: use high-level directory writer.

See [Writer APIs](Writer-APIs.md) and [Hot Path Guide](Hot-Path-Guide.md)
before choosing production options. For a compact option-by-option summary, see
[Options Reference](Options-Reference.md).
