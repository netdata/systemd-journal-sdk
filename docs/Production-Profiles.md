# Production Profiles

## High-Throughput Ingestion

Use this for NetFlow, SNMP traps, OTEL logs, and similar structured producers.

Rust and Go are the production ingestion targets. Current shared writer
certification measured Rust/Go tens of thousands of append rows/s on the
32-field benchmark, while Node.js and Python were around 0.9k-1.0k append
rows/s. Do not choose Node.js or Python for high-throughput ingestion without a
fresh workload-specific benchmark.

Recommended:

- structured append;
- compact format when the target readers support it and file size matters;
- no compression for maximum write throughput unless disk footprint is the
  bottleneck;
- FSS disabled unless tamper evidence is required;
- `live_publish_every_entries` tuned above `1` or set to `0` for poll/snapshot
  consumers;
- high-level directory writer for rotation and retention;
- optional writer lock helper only when the deployment needs SDK-level
  cooperating-writer exclusion.

Avoid:

- converting structured data to raw `KEY=value` payloads;
- doing producer-specific field remapping inside SDK calls;
- relying on core writers to discover host identity;
- treating live publication cadence as durability.

## Stock systemd Compatibility Writer

Use this when stock `journalctl` and libsystemd readers must read files while
the writer is active.

Recommended:

- `JOURNALD` field-name policy;
- regular format unless compact compatibility is explicitly validated;
- default live publication after every entry;
- caller-provided machine and boot IDs when stable identity matters;
- stock `journalctl --verify --file` validation in release tests.

Avoid:

- `RAW` field names that stock systemd may reject;
- disabled live publication when live-follow freshness is required.

## High-Throughput Reader

Use this for pipelines that traverse rows and process payloads directly.

Rust and Go are the production reader targets. On platforms without mmap-backed
access, Go falls back to positioned file I/O and should be re-benchmarked for
the target workload. Node.js and Python are compatibility/parity surfaces today,
not high-throughput reader baselines.

Recommended:

- payload visitor;
- snapshot bounds when appends during the scan do not matter;
- FIELD/DATA index APIs for field names and unique values;
- avoid formatting until the final output boundary.

Avoid:

- full entry map materialization in the inner loop;
- JSON/export/text formatters as internal APIs;
- verification in normal query paths.

## Logs Explorer

Use this for UI/API queries that need filters, facets, histograms, returned
rows, and optional FTS.

Recommended:

- Explorer `Traversal` default;
- indexed filters;
- selected facets only;
- selected histogram only;
- `FirstValue` unless duplicate same-field values must count;
- returned-row expansion only after row selection;
- progress and cancellation callbacks for interactive services.

Avoid:

- enabling FTS unless the user requested text search;
- enabling `AllValues` without a correctness reason;
- using `debug_collect_column_fields_by_row_traversal`;
- assuming `Index` strategy is faster without measuring the query shape.

## Netdata Function Boundary

Use the SDK Netdata function API when the consumer needs Netdata-shaped logs
function output.

Recommended:

- provide journal directories;
- provide default facet keys;
- provide default view keys;
- provide default histogram key;
- wire progress and cancellation into the caller's function runtime;
- keep Netdata-specific presentation outside core reader paths.

The Netdata wrapper is not the generic Explorer. It is an adapter that converts
Netdata request/response behavior to Explorer queries.
