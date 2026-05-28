# SOW Status

Last updated: 2026-05-28

## Current

- SOW-0009 - Benchmark Profile Optimize: paused. Broad performance work waits
  for the remaining reference-drift work to resume and settle.
- SOW-0037 - Reference Drift Audit: in-progress. Current slice removes the
  unhelpful Rust and Go recent DATA caches after measurement showed no useful
  throughput benefit.

## Pending

- SOW-0026 - Netdata SDK Integration: open. Integration should wait until
  performance gates are acceptable for Netdata hot paths.

## Recently Completed

- SOW-0036 - Live Publication Modes And Fast Consumers: completed. Rust, Go,
  Node.js, and Python expose the shared `live_publish_every_entries` writer
  option. Default `1` keeps stock-compatible publication after every entry;
  `0` and `N > 1` are narrower latency-tolerant contracts. Whole-file mmap and
  Rust recent-DATA-cache-size changes were measured and not kept.
- SOW-0035 - Derived Rotation Policy: completed.
