# Explorer And Netdata Queries

The Explorer API is for log-explorer workloads:

- exact indexed filters;
- selected facet counters;
- optional histogram;
- optional full-text search;
- optional returned rows;
- progress and cancellation control in Netdata-oriented wrappers.

It exists because the generic row enumeration APIs cannot be optimal for every
log-explorer shape.

## Query Flow

```text
files selected by directory/time/source
  -> indexed filters create candidate ENTRY offsets
  -> candidate rows are scanned only when facets, histogram, FTS, or returned
     rows require row data
  -> reusable DATA offsets are classified once per traversal pass
  -> selected facets and histogram values are counted
  -> returned rows are expanded only for rows that will be returned
```

## Default Strategy

`Traversal` is the default Explorer strategy.

It is the production default because it supports:

- first-value accounting;
- source-realtime-bounded queries;
- FTS;
- selected returned rows;
- selective filters.

It uses indexes to slice candidates, then scans only the candidate rows needed
for the requested outputs.

`ExplorerAnchor::Auto` is the default scan-start policy. Forward queries start
near `after_realtime_usec` when present, otherwise at the head. Backward queries
start near `before_realtime_usec` when present, otherwise at the tail. Explicit
head, tail, or realtime anchors are for callers that need a specific cursor
behavior.

## Index Strategy

`Index` derives all-values facet and histogram counts from FIELD/DATA chains
and DATA entry posting lists.

It is exact only for a narrower query shape:

- all-values accounting;
- commit-realtime time semantics;
- no FTS;
- supported facets/histogram only.

It can be much faster for narrow unfiltered all-values queries and
histogram-only queries. It can be slower for many facets or selective filters.
Use it only after measuring the target query shape, or use `Compare` to verify
logical equality and timing.

## Compare Strategy

`Compare` runs traversal and index, verifies logical equality, and returns
timing/counter diagnostics. Use it for testing and query-shape validation, not
normal production serving.

## FirstValue Versus AllValues

`FirstValue` is the default:

- one selected field contributes at most one value per row;
- duplicate values for the same field in the same row are ignored;
- traversal may stop after all selected fields are found.

`AllValues` is exact for rows that intentionally contain repeated values for
the same field. It scans more DATA and is slower.

## FTS

FTS requires inspecting searchable payloads. It disables many shortcuts because
the query asks about content, not only indexed field/value membership.

Use FTS only when the request needs text search. Do not enable FTS to implement
exact field filters; field filters should use journal indexes.

## Facets And Histogram

Facet and histogram fields are the fields that must be expanded during
candidate-row traversal.

Filter fields do not need expansion for exact filtering; filters use indexes.
Returned rows are expanded only after the selected rows are known.

## Debug Row Traversal Is Not Production

Do not enable `debug_collect_column_fields_by_row_traversal` in production.

This flag exists only to debug Explorer discrepancies. If setting it changes
the result from wrong to right, the Explorer has a bug or an incomplete column
catalog path. Benchmark or compatibility success with this flag enabled is
invalid evidence.

## Netdata Function Boundary

All four SDKs expose Netdata-shaped logs function APIs over Explorer.

That layer handles:

- Netdata request parsing;
- default facets, view columns, and histogram field;
- field presentation transforms;
- progress callbacks;
- cancellation callbacks;
- timeout behavior;
- plugin-compatible response envelopes.

Histogram responses include the libnetdata-style chart envelope used by the
Netdata UI: `summary`, `totals`, `result`, `db`, `view`, and `agents`.
`view.dimensions.names` and sibling dimension arrays are present even when the
selected time window has no values for the histogram field.

The generic SDK Explorer stays separate from Netdata-specific response shaping.
