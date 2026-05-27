# SOW Status

Last updated: 2026-05-27

## Current

- SOW-0009 - Benchmark Profile Optimize: paused. Broad performance work waits
  for SOW-0037 to establish Rust as the systemd-compatible reference and to
  measure Rust raw-payload versus structured writer APIs apples-to-apples.
- SOW-0037 - Reference Drift Audit: in-progress. Rust parity with systemd,
  Rust dual-layer raw/structured writer API, and Rust writer performance retest
  are now the active priority before more Go/Node.js/Python optimization.

## Pending

- SOW-0026 - Netdata SDK Integration: open. Integration should wait until
  performance gates are acceptable for Netdata hot paths.
- SOW-0036 - Live Publication Modes And Fast Consumers: open. Analyze and
  decide configurable live publication modes and related compatibility/
  performance opportunities before implementation, including measured
  windowed-versus-whole-file mmap writer strategy tradeoffs and Go reader
  `ReadAt`-versus-mmap strategy tradeoffs.

## Recently Completed

- SOW-0035 - Derived Rotation Policy: completed.
