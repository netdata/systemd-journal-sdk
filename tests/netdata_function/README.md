# Netdata Function Boundary Tests

This directory contains the SDK-side tools for comparing the Rust Netdata
function wrapper with an external Netdata `systemd-journal.plugin` binary.

The external plugin and the SDK wrapper use the same CLI shape:

```bash
<binary> --test systemd-journal --dir <journal-dir> --request <request.json> --timeout <seconds>
```

`--timeout 0` disables the test timeout by mapping it to an effectively
unreachable internal deadline. Nonzero values are seconds.

The comparator checks function content, not byte-for-byte JSON serialization:

- Stable top-level response fields.
- The complete table column catalog, ignoring only column index order.
- Every returned row value by column name.
- Every content facet field, facet value id, display name, and count.
- Histogram buckets by timestamp and dimension label.
- Stable item counters: `matched`, `returned`, `max_to_return`, `after`,
  `before`, `unsampled`, and `estimated`.
- Compact function error envelopes such as
  `{"status":304,"errorMessage":"No new data since the previous call."}`.

Dictionary and array emission order is not treated as content when Netdata
derives that order from hash-table traversal. Runtime envelope fields such as
`_stats`, `_journal_files`, `expires`, and `last_modified` are diagnostics.
`items.evaluated` is also diagnostic because it counts internal scan work, not
journal content.

`data_only=true` has two extra compatibility rules:

- `facets_delta`, `histogram_delta`, and `items_delta` are compared with the
  same semantic rules as `facets`, `histogram`, and `items`.
- Plugin-only or SDK-only columns that are `null` in every returned row are
  reported as non-content. A missing column with any non-null returned-row value
  remains a content mismatch.

The comparator reports, but does not treat as content, Netdata's zero-count
empty-string unavailable-field artifact:

```json
{"id": "CzGfAU2z3TC", "name": "[unavailable field]", "count": 0}
```

All other zero-count facet values remain content and must match.

The runner parses JSON stdout even when a compared binary exits nonzero,
because Netdata's plugin test path exits nonzero for function error responses
such as HTTP 304 no-change.

Sanitized reports should be written under `.local/`. Do not commit raw plugin
or SDK JSON generated from real journal data.
