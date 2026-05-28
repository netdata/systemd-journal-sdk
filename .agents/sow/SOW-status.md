# SOW Status

Last updated: 2026-05-28

## Current

- SOW-0009 - Benchmark Profile Optimize: paused. Broad performance work waits
  for the remaining reference-drift work to resume and settle.
- SOW-0037 - Reference Drift Audit: paused. Paused while the SNMP traps critical
  field-name policy correction is implemented and released.

## Pending

- SOW-0026 - Netdata SDK Integration: open. Integration should wait until
  performance gates are acceptable for Netdata hot paths.
- SOW-0039 - RAW Byte Field Name Reader Representation: open. Tracks the
  byte-preserving reader API decision for RAW non-UTF8 field names discovered
  during SOW-0038 review.

## Recently Completed

- SOW-0038 - Field Name Policy Layers: completed. Rust, Go, Node.js, and
  Python now expose RAW, JOURNALD, and JOURNAL-APP writer field-name policies;
  producer-specific field-name remapping has been removed from SDK code, docs,
  and public API. This is the `v0.3.0` / `go/v0.3.0` release target.
- SOW-0036 - Live Publication Modes And Fast Consumers: completed. Rust, Go,
  Node.js, and Python expose the shared `live_publish_every_entries` writer
  option. Default `1` keeps stock-compatible publication after every entry;
  `0` and `N > 1` are narrower latency-tolerant contracts. Whole-file mmap and
  Rust recent-DATA-cache-size changes were measured and not kept.
- SOW-0035 - Derived Rotation Policy: completed.
