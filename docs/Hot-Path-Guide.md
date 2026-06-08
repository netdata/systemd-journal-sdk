# Hot Path Guide

This page lists the choices that most affect performance.

## Reader Hot Paths

| Need | Preferred Path | Avoid |
|---|---|---|
| Count or scan current-row payloads | payload visitor over `FIELD=value` bytes | materializing full entry maps |
| Enumerate field names | FIELD hash table traversal | row scan over every entry |
| Enumerate unique values for one field | FIELD object's DATA chain | row scan and de-duplication |
| Exact filters | DATA entry posting lists / indexed candidate offsets | expanding every row and comparing strings |
| Facets/histogram | Explorer traversal over candidate rows | generic entry materialization |
| FTS | Explorer FTS path | using FTS for exact field filters |
| Output rows | expand only selected returned rows | expanding all rows before slicing |
| Verification | verifier APIs | using verifier as normal query path |

## Writer Hot Paths

| Need | Preferred Path | Avoid |
|---|---|---|
| Structured producer | structured append | building `KEY=value` only to parse it again |
| Already encoded payloads | raw append | splitting/rebuilding payloads outside the SDK |
| Directory backend | high-level directory writer | manual active-file lifecycle unless required |
| One file under caller lifecycle | direct-file writer | directory writer with disabled lifecycle |
| Poll/snapshot consumers | tuned live publication cadence | publishing every entry when freshness is not needed |
| Stock live-follow readers | default live publication cadence | disabling publication |

## Options That Commonly Slow Reads

- DATA compression: saves disk, costs CPU when selected DATA must be read.
- FTS: requires inspecting payload content.
- `ExplorerFieldMode::AllValues`: scans more row DATA for duplicate field
  correctness.
- Formatting as JSON/export/text: does presentation work.
- Verification: reads object graph for integrity, not query speed.
- Very small mmap windows: can make indexed DATA-chain traversal remap-bound.
- Debug row traversal in Explorer: invalid for production.

## Options That Commonly Slow Writes

- Compression: adds CPU work and can need contiguous payload buffers.
- FSS: writes seal TAG/HMAC state and later verification work.
- Live publication after every entry: best stock follow compatibility, less
  batching.
- Strict durability outside SDK live publication: filesystem sync policy is a
  separate operational choice.
- Raw append for structured producers: forces avoidable `KEY=value` assembly
  and validation.

## Borrowed Data Lifetime

Reader hot paths return borrowed mmap-backed data where the language can safely
provide it. The contract is row-scoped:

- data returned for the current row remains valid until the reader advances or
  resets the current row;
- uncompressed DATA can be returned directly from mmap-backed journal bytes;
- compressed DATA is decompressed into row-owned storage and released when the
  row changes.

Consumers that need data after advancing must copy it.

## Compression And Explorer

Compressed DATA stores the compressed `FIELD=value` payload. The field name is
not visible without decompression.

Explorer avoids decompressing unrelated DATA by:

- using filter indexes before row traversal;
- classifying reusable DATA offsets once per pass;
- stopping row traversal when selected first-value fields are satisfied;
- expanding all fields only for returned rows.

If the selected facet/histogram/FTS data is compressed, decompression is still
required.

## Benchmark Rule

Benchmark the API path the consumer will actually use. A benchmark using raw
append, debug traversal, all-values mode, or full entry materialization is not
evidence for a structured producer, normal Explorer query, or payload visitor.

For option-by-option behavior, see [[Options-Reference|Options Reference]].
