# Netdata Function Boundary Tests

This directory contains the SDK-side tools for comparing the Rust Netdata
function wrapper with an external Netdata `systemd-journal.plugin` binary.

The external plugin and the SDK wrapper use the same CLI shape:

```bash
<binary> --test systemd-journal --dir <journal-dir> --timeout <seconds> < <request.json>
```

`--timeout 0` disables the test timeout by mapping it to an effectively
unreachable internal deadline. Nonzero values are seconds.
The request JSON is read from stdin in test mode. Do not pass request filenames
to compared binaries; test binaries may run with elevated privileges.

The SDK wrapper also exposes diagnostic-only options for validating the SDK
run-control API without changing the standard plugin-compatible shape:

- `--progress-jsonl <path>` writes one progress event per line.
- `--cancel-immediately` asks the SDK cancellation predicate to stop before
  work starts.
- `--cancel-after-progress <n>` asks the progress callback to request
  cancellation after `n` progress events. The SDK checks that predicate during
  active scans, before later selected files, and after file-end progress.

These options are for SDK validation. Netdata production consumers should call
the `journal::netdata` API directly and connect progress/cancellation to the
Netdata function framework.

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

The source-option `info` string for a journal source embeds two
live-volatile components derived from the tail of the journal at the
time the peer ran the request:

```text
N files, total size S, covering <duration>, last entry at <iso>
```

The two `covering` (duration) and `last entry at` (RFC-3339 UTC)
components are reported from the live tail. With any optional third
experiment peer, the slow peer can see a tail several seconds newer than
the fast peers; observed up to ~6 seconds while SDK and plugin agree to
the second. The 2-peer design relied on back-to-back invocations; with a
third slow peer the race is structural, not a transient timing bug.

The comparator therefore accepts a bounded skew on those two
components only:

- `N files` and `total size S` are compared exactly.
- `<duration>` is parsed back to seconds (using the same `y/mo/d/h/m/s`
  unit grammar that produced it, with `1y = 365d`, `1mo = 30d`) and
  accepted when `|delta| <= 300` seconds. The literal `off` only
  equals `off`.
- `<iso>` is parsed as a UTC RFC-3339 timestamp and accepted when
  `|delta| <= 300` seconds. The literal `unknown` only equals
  `unknown`.
- Strings that do not match the shape fall back to exact comparison.

The rule is `|delta| <= 300` seconds (the 300 second bound includes
both endpoints), and the tolerance is symmetric for every peer pair
because the comparison is shape-based, not peer-based. The
tolerated skews are surfaced in the comparison's
`non_content.source_option_info_skew_tolerances` list (one entry per
tolerated pair, with the field name, the parsed left/right seconds,
the delta, and the bound) so reports show what was tolerated, with
values. File counts and total sizes stay strict: a real divergence
in those components still fails the comparison.

The response `_request.after` / `_request.before` echoes embed parse-time
`unix_now_seconds()` by reference design (Rust L1418 -> L3624-3690). Two
peers invoked seconds apart legitimately produce different echoes; the
slow third peer is not a real content mismatch. The comparator
therefore accepts a bounded skew on those two echoes ONLY
(`REQUEST_WINDOW_SKEW_TOLERANCE_SECONDS = 300`, same value and
diagnostic style as the source-info tolerance). Other `_request`
fields stay strict: a real divergence in `data_only`, `direction`,
`facets`, `last`, etc. still fails the comparison. The tolerated
pair is reported in
`non_content.request_window_skew_tolerances` (one entry per
tolerated field, with the field name, the two raw echo values,
the delta in seconds, and the bound) so reports show what was
tolerated, with values. The rule is `|delta| <= 300` seconds
and the tolerance is symmetric for every peer pair because the
comparison is shape-based, not peer-based.

`ND_JOURNAL_FILE` row values are compared by journal filename only. Netdata's
hardened test mode may report the selected directory through a transient
`/proc/self/fd/<n>/...` path, while the SDK wrapper reports the caller-supplied
directory path.

The request suite includes a low-budget sampling fixture. `_sampling` itself is
reported as a diagnostic top-level object, but sampling changes stable content
through `items.unsampled`, `items.estimated`, facet counts, and histogram
`[unsampled]`/`[estimated]` buckets, so those values are compared strictly.

The request suite also includes an FTS fixture. It verifies Netdata
`SIMPLE_PATTERN` query behavior for `|`-separated OR terms and `!` negative
terms at the function boundary.

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

`run_stateful_function_compare.py` covers request sequences where later calls
depend on earlier responses. It runs the SDK wrapper first, then the external
plugin, compares every step with the same semantic comparator, and derives
anchors from response rows. The committed sequence suite covers:

- backward paging without duplicate or missing rows;
- forward paging without duplicate or missing rows;
- tail polling that returns only rows newer than the anchor and then `304`;
- filtered tail polling where newer rows exist but no newer matching rows
  returns `200` with empty data, matching `systemd-journal.plugin`;
- data-only delta output for facets, histogram, and item counters.

The stateful harness uses `after=1` as its default lower bound. In Netdata
request semantics, `after=0` is a relative UI window, not the Unix epoch.

`run_anchor_regression.py` covers multi-source anchor behavior with committed
fixture specs and request JSONs. It generates synthetic journal directories
from `tests/netdata_function/fixtures/*.json`, runs each peer binary with the
plugin-compatible `--test systemd-journal --dir ...` shape, and validates
query-wide page collection instead of comparing peers to each other.

The anchor regression suite currently includes two scenarios:

- `query-wide-noncollision`: three sources with distinct non-continuous
  internal realtime timestamps. This is the positive-control case; page 1 plus
  page 2 must collect all expected messages without duplicates.
- `same-anchor-boundary`: three sources with the same internal realtime
  timestamp and a page size of two. A correct scalar-anchor implementation must
  not split that query-wide boundary group in a way that makes any row
  unreachable. Returning more than the requested page size is acceptable for
  this boundary case.

The runner also validates the ordered-scalar anchor invariant. A page may skip
timestamp values, but rows in each page must be ordered, the next page must be
strictly outside the previous scalar anchor, and the next page's own edge anchor
must move in the requested direction when that page has rows.

Example:

```bash
python3 tests/netdata_function/run_anchor_regression.py \
  --peer plugin=/usr/libexec/netdata/plugins.d/systemd-journal.plugin \
  --peer rust=rust/target/debug/netdata_function_wrapper \
  --peer go=.local/sow-0124/bin/go-netdata_function_wrapper \
  --scenario query-wide-noncollision \
  --out .local/sow-0124/query-wide-noncollision-report.json
```

The installed Netdata `systemd-journal.plugin` is useful as current-behavior
evidence, but it is not a correctness oracle for the `same-anchor-boundary`
scenario; current plugin behavior also loses rows there. Use `--allow-fail
plugin` when documenting that known plugin gap while validating SDK fixes.

The stateful gate freezes a fresh-data synthetic fixture so an optional
extra peer is not divergent because of live tail movement. The runner
exposes `--make-static-fixture <dir>` which
generates a fresh fixture (100 entries by default, timestamped in
`[now-3000s, now-600s]`, well inside every peer's `[now-3600, now]`
window even with the ±60s parse skew observed at the stateful gate) using
the in-repo Go SDK, and writes a JSON report describing the fixture. The
runner does not invoke the SDK/plugin binaries in
fixture mode. Default behavior (no flag) is unchanged: the runner
expects `--dir` to point at an existing directory and runs the sequences
against it.

Sanitized reports should be written under `.local/`. Do not commit raw plugin
or SDK JSON generated from real journal data.
