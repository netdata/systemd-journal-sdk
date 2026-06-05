# Netdata Function Boundary Tests

This directory contains the SDK-side tools for comparing the Rust Netdata
function wrapper with an external Netdata `systemd-journal.plugin` binary.

The external plugin and the SDK wrapper use the same CLI shape:

```bash
<binary> --test systemd-journal --dir <journal-dir> --request <request.json>
```

The comparator checks semantic function output, not byte-for-byte JSON:

- HTTP/status fields that should be stable.
- Returned rows projected through stable common columns.
- Nonzero facet counters.
- Nonzero histogram totals.
- Stable item counters: `matched`, `returned`, and `max_to_return`.

Zero-count facet and histogram values are ignored by default. The current
Netdata plugin can emit zero-count vocabulary padding for values observed while
scanning rows that do not contribute to the result. Those values affect UI
vocabulary completeness, but they do not change returned rows or result
counters.

Sanitized reports should be written under `.local/`. Do not commit raw plugin
or SDK JSON generated from real journal data.
