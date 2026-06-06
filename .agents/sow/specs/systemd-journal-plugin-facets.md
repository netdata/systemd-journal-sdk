# systemd-journal Plugin And Facets Behavior

## Scope

This specification records the current behavior of Netdata's
`systemd-journal.plugin` plus the generic facets engine, as the target behavior
for the SDK optimized explorer API.

The goal is not to clone Netdata implementation details blindly. The goal is to
preserve the query semantics Netdata users depend on while giving later SDK
work a precise target for a faster implementation.

## Evidence

Primary source:

- `ktsaou/netdata @ b695fa41f8ef`
  - `src/collectors/systemd-journal.plugin/systemd-journal.c`
  - `src/collectors/systemd-journal.plugin/systemd-journal-execute.h`
  - `src/collectors/systemd-journal.plugin/systemd-journal-function.h`
  - `src/collectors/systemd-journal.plugin/systemd-journal-sampling.h`
  - `src/collectors/systemd-journal.plugin/systemd-internals.h`
  - `src/collectors/systemd-journal.plugin/systemd-journal-files.c`
  - `src/collectors/systemd-journal.plugin/systemd-journal-annotations.c`
  - `src/collectors/systemd-journal.plugin/systemd-main.c`
  - `src/libnetdata/facets/facets.c`
  - `src/libnetdata/facets/facets.h`
  - `src/libnetdata/facets/logs_query_status.h`
  - `src/libnetdata/facets/README.md`

The core facets files are byte-identical in `netdata/netdata @ 5d611c4ce8c2`
for `facets.c`, `facets.h`, and `facets/README.md`. The systemd plugin wrapper
and `logs_query_status.h` differ between the local upstream checkout and the
fork checked here, so this spec cites the fork for integration behavior.

## Glossary

- **Entry / row**: one journal event.
- **Field**: a journal field name, such as `PRIORITY` or `_SYSTEMD_UNIT`.
- **Value**: the field value after the first `=` separator in a journal DATA
  payload.
- **Facet key**: a field whose values are collected and returned as filter
  options.
- **Facet value counter**: a count for one value of one facet key under the
  query's selection semantics.
- **Histogram key**: one facet key whose values become histogram dimensions.
- **Top-N rows**: the `last` retained rows returned to the UI.
- **Anchor**: a microsecond realtime timestamp used for paging relative to a
  previous response.
- **Slice mode**: native backend filtering through journal indexes before row
  traversal.
- **Data-only mode**: fast paging mode that returns rows without full facet and
  histogram analysis unless delta output is requested.
- **Delta mode**: data-only companion mode that emits `*_delta` objects for the
  scanned incremental range.
- **Tail mode**: data-only conditional follow mode using `if_modified_since`.
- **FTS**: full-text search with Netdata `SIMPLE_PATTERN` matching.

## Query Surface

The function name is `systemd-journal`; its description is "View, search and
analyze systemd journal entries." (`systemd-journal.c:13-14`).

Accepted parameters are declared centrally:

- `help`, `info`, `after`, `before`, `anchor`, `last`, `query`, `facets`,
  `histogram`, `direction`, `if_modified_since`, `data_only`, `__logs_sources`,
  `slice`, `delta`, `tail`, and `sampling`
  (`logs_query_status.h:8-24`, `logs_query_status.h:705-728`).

Defaults:

- Direction defaults to backward/newest-first
  (`systemd-journal-function.h:13`, `systemd-journal.c:288`).
- `last` defaults to 200 rows (`systemd-journal.c:42`,
  `logs_query_status.h:781-782`).
- Sampling defaults to 1,000,000 entries (`systemd-journal.c:43`,
  `logs_query_status.h:96`).
- Slice mode defaults to enabled when `sd_journal_restart_fields()` support is
  available, otherwise disabled (`systemd-journal.c:51-55`,
  `systemd-journal.c:273-288`).
- The default histogram field is `PRIORITY` (`systemd-journal.c:300`).
- When neither `after` nor `before` is supplied, the code uses the last one
  hour. If the normalized window collapses to one second, it again uses one
  hour (`logs_query_status.h:27`, `logs_query_status.h:762-779`).

Evidence note:

- `facets/README.md:117-123` says the default time range is 15 minutes. The
  code sets one hour. The SDK target is the code behavior unless a later user
  decision changes it.

## SDK Netdata Function Boundary

The Rust SDK exposes an additive Netdata-specific boundary under
`journal::netdata`.

Current Rust entrypoints:

- `NetdataJournalFunction::systemd_journal()`
- `NetdataJournalFunction::systemd_journal_plugin_compatible()`
- `NetdataJournalFunction::run_directory_request_json()`
- `NetdataJournalFunction::run_directory_request_bytes()`

The first wrapper command is an internal test command named
`netdata_function_wrapper`, with the same external shape as the Netdata plugin:

```bash
netdata_function_wrapper --test systemd-journal --dir <journal-dir> --request <request.json>
```

The Netdata boundary is not part of the core journal file-format layer. It owns
Netdata request parsing, default facets, default display fields, default
histogram, field presentation transforms, row options, and Netdata-shaped JSON.
The core reader remains responsible for journal traversal and object access.

The Netdata-specific profile currently implements the `systemd-journal.plugin`
field presentation needed by the comparison harness:

- `PRIORITY` numeric values render as syslog priority names.
- `SYSLOG_FACILITY` numeric values render as syslog facility names.
- row options derive Netdata severity from `PRIORITY`.
- `ND_JOURNAL_PROCESS` is synthesized from `CONTAINER_NAME`,
  `SYSLOG_IDENTIFIER`, or `_COMM` plus `_PID` when present; `_EXE` and
  `SYSLOG_PID` are not fallbacks in this plugin-compatible boundary.
- `ND_JOURNAL_FILE` is injected from the file path for returned rows.

The default `systemd_journal()` constructor keeps UID/GID journal values as raw
IDs and does not resolve host user or group names. The
`systemd_journal_plugin_compatible()` constructor is the explicit opt-in profile
for comparison with Netdata's installed plugin; it may resolve UID/GID display
names through the host platform when the caller chooses that compatibility mode.

The standalone SDK comparison wrapper accepts requests that contain
`__logs_sources`, but it runs against an explicit `--dir` and does not implement
Netdata's journal registry source-group filtering. Source-group filtering needs
separate integration work at the Netdata registry boundary.

The Rust explorer now has an explicit
`ExplorerQuery::exclude_facet_field_filters` switch:

- default `true` preserves the SDK explorer's original behavior, where a
  facet's own selected values are excluded while counting that facet;
- the Netdata wrapper sets it to `false` because `systemd-journal.plugin`
  counts facets with all filters applied.

The Rust explorer histogram path counts matched rows that do not contain the
histogram field under the `"-"` value. Both traversal and explicit index
strategy produce the same missing-value histogram semantics.

Filtered Netdata wrapper requests add zero-count facet values from an
unfiltered vocabulary pass. This approximates the plugin's vocabulary-padding
behavior while keeping data rows, nonzero facet counters, histogram totals, and
stable item counters comparable.

The comparison harness in `tests/netdata_function/` compares semantic function
output. General zero-count facet values are content and must match. The only
facet-value artifact ignored as non-content is Netdata's empty-string
unavailable-field value with id `CzGfAU2z3TC`, display name
`[unavailable field]`, and count `0`.

The comparator treats `items.evaluated` as a diagnostic scan-accounting counter,
not journal content. It must still report the value and any mismatch, but
content equality is decided by stable item counters (`matched`, `returned`,
`max_to_return`, `after`, `before`, `unsampled`, and `estimated`), columns,
rows, facets, and histogram buckets.

## GET And POST Differences

The query parser supports both GET-style function strings and POST JSON payloads.

- POST works with field names, disables hash IDs, and registers selected
  facets/filters by name (`logs_query_status.h:329-467`).
- GET works with hash IDs for facets and values, enables hash IDs, and registers
  selected facets/filters by ID (`logs_query_status.h:490-660`).
- The request struct documents this split with `fields_are_ids`
  (`logs_query_status.h:41`).

POST shape:

- `facets` must be an array of field-name strings. If supplied, default facets
  are reset and only listed facets are enabled
  (`logs_query_status.h:354-383`).
- `selections` must be an object of arrays. `__logs_sources` is treated as
  source selection. All other keys register selected facet values and increment
  `filters` (`logs_query_status.h:385-461`).

GET shape:

- `facets:<id1>,<id2>` resets default facets and enables the requested facet
  IDs (`logs_query_status.h:609-630`).
- Any unrecognized `key:value1,value2` token registers selected value IDs for
  that key and increments `filters` per value (`logs_query_status.h:631-654`).

## Source Selection

The source selection parameter is `__logs_sources`, reserved so it cannot
conflict with journal fields (`logs_query_status.h:19`).

Built-in source names are:

- `all`
- `all-local-logs`
- `all-local-system-logs`
- `all-local-user-logs`
- `all-uncategorized`
- `all-local-namespaces`
- `all-remote-systems`

Evidence: `systemd-internals.h:76-82` and `systemd-journal.c:20-37`.

Files are classified during registry insertion:

- paths under `/remote/` become remote sources;
- local paths are classified as namespace, system, user, or other based on the
  filename/path shape;
- every file belongs to `all`.

Evidence: `systemd-journal-files.c:357-425`.

`info=true` returns available source groups with counts, size, and time coverage
through `available_journal_file_sources_to_json_array()`
(`logs_query_status.h:663-696`, `systemd-journal-files.c:537-575`).

The SDK Netdata function replacement builds the same source selector for its
explicit directory input. It reports `__logs_sources` options for `all`,
`all-local-logs`, and `all-local-system-logs`, deriving file count, total size,
coverage, and last-entry timestamp from the selected journal files. This is
directory-local source metadata; full Netdata registry/provider source
selection remains a separate replacement requirement.

## Timeframe And Anchor Semantics

Input `after` and `before` are seconds; normalized query bounds are converted to
microseconds. `before_ut` is inclusive for the whole final second:

- `after_ut = after_s * 1_000_000`
- `before_ut = before_s * 1_000_000 + 999_999`

Evidence: `logs_query_status.h:762-779`.

When both `after` and `before` are missing or zero, the plugin uses a default
one-hour window ending at `now`. When either endpoint is present, the plugin
passes the pair through `rrdr_relative_window_to_absolute()`: absolute values
larger than three years in seconds stay absolute, while smaller positive and
negative values are interpreted as relative seconds. A missing `after` with a
supplied `before` uses the Netdata helper's 600-second relative default.
Evidence: `libnetdata.c:364-422` and `buffer.h:12`.

If `after > before`, values are swapped. If equal, `after` is shifted back by
the default query duration (`logs_query_status.h:769-777`).

Anchor behavior:

- `anchor` is a microsecond timestamp.
- If `tail=true` with an anchor, the query is forced backward, `anchor.start_ut`
  is cleared, and `anchor.stop_ut` is set to the supplied anchor
  (`logs_query_status.h:787-799`).
- If the anchor is outside the selected timeframe, it is ignored and direction
  resets to backward (`logs_query_status.h:801-814`).
- The facets engine stores the anchor start, stop, and direction
  (`logs_query_status.h:816`, `facets.c:1790-1801`).

The plugin compensates for journal commit timestamp versus source realtime
timestamp:

- A per-file `max_journal_vs_realtime_delta_ut` starts at five seconds and is
  capped at two minutes (`systemd-internals.h:86-87`,
  `systemd-journal-files.c:760-769`).
- When `_SOURCE_REALTIME_TIMESTAMP` is present and older than the journal
  timestamp, the row timestamp is changed to the source timestamp and the
  per-file max delta is updated atomically (`systemd-journal-execute.h:48-77`).
- Query start/stop bounds are expanded by the learned delta in the scan
  direction (`systemd-journal-execute.h:114-118`,
  `systemd-journal-execute.h:231-235`, `logs_query_status.h:154-167`).
- File selection uses a maximum two-minute delta to avoid skipping files whose
  source realtime timestamps may fall into the query (`systemd-journal-execute.h:486-503`).

Duplicate visible timestamps are made unique inside one query:

- backward scans decrement a duplicate timestamp (`systemd-journal-execute.h:173-179`);
- forward scans increment a duplicate timestamp (`systemd-journal-execute.h:282-288`).

## File Selection And Traversal Order

The plugin queries one journal file at a time. It never opens a multi-file
libsystemd directory reader for a query.

Selection:

- A file is eligible when the source selection matches and its known time range
  overlaps the query timeframe after delta expansion
  (`systemd-journal-execute.h:486-503`).
- The scanner accepts both `.journal` and `.journal~` files. It follows
  symlinked directories and symlinked regular journal files during recursive
  scans (`systemd-journal-files.c:610-641`, `systemd-journal-files.c:643-705`).
- Files without known message timestamps are included so scanning can update
  metadata (`systemd-journal-execute.h:490-493`).
- `if_modified_since` returns HTTP 304 before scanning if no matched file has
  `msg_last_ut` newer than the supplied timestamp
  (`systemd-journal-execute.h:523-545`).

Order:

- Backward queries sort files by newest message timestamp, then file mtime, then
  first message timestamp (`systemd-journal-execute.h:547-553`,
  `systemd-journal-files.c:799-831`).
- Forward queries use the reverse comparator (`systemd-journal-files.c:829-831`).

Per-file metadata:

- Header scans read first/last realtime timestamps and seqnums when available
  (`systemd-journal-files.c:189-321`).
- If the first sequence metadata is unavailable, the scanner tries to recover
  seqnum and timestamp from archived journal filenames
  (`systemd-journal-files.c:259-290`).
- `messages_in_file` is computed only when first and last writer IDs match
  (`systemd-journal-files.c:304-315`).
- Boot-id display annotations keep the earliest realtime timestamp seen for
  each `_BOOT_ID` across all scanned files. The registry conflict callback
  replaces an existing boot timestamp only when the new timestamp is smaller
  (`systemd-journal-files.c:141-143`, `systemd-journal-files.c:857-872`).

## Native Slice Filtering

Slice mode is the current plugin's index-assisted filter path.

The `nsd_journal_*` and `NSD_JOURNAL_*` symbols are Netdata provider wrappers
over libsystemd-like journal operations. The SDK equivalent is direct journal
FIELD/DATA chain and match-index traversal, not a dependency on those wrappers.

When slice is enabled and the build has field restart support:

- The plugin enumerates journal fields (`NSD_JOURNAL_FOREACH_FIELD`).
- In `data_only` mode, it considers only keys with selected filters.
- In full analysis mode, it considers keys that are enabled facets.
- For each interesting key, it enumerates unique values with
  `nsd_journal_query_unique()` and `nsd_journal_enumerate_available_unique()`.
- Every available value is recorded as a possible facet value.
- Only selected values become native journal matches.
- The first selected value for a later key is preceded by
  `nsd_journal_add_conjunction()`, making fields an AND.
- Later selected values for the same key are preceded by
  `nsd_journal_add_disjunction()`, making values an OR.
- If setup fails, matches are flushed and the full query path is used.

Evidence: `systemd-journal-execute.h:355-431`.

If slice mode finds no selected values:

- `matches_filters` is true when there are no filters;
- otherwise the file is treated as not matched.

Evidence: `systemd-journal-execute.h:461-478`.

There is no separate negative field-filter syntax in the systemd-journal plugin
selection parser. Negative matching exists for FTS through `SIMPLE_PATTERN`
results (`facets.c:1963-1978`).

## Row Processing

For each row selected by the journal cursor:

1. The plugin injects `ND_JOURNAL_FILE=<filename>`.
2. It iterates every DATA payload returned by `NSD_JOURNAL_FOREACH_DATA`.
3. It splits the payload on the first `=`.
4. It applies `_SOURCE_REALTIME_TIMESTAMP` timestamp adjustment.
5. It adds the field/value to facets, truncating values to
   `FACET_MAX_VALUE_LENGTH` (`8192`) bytes.

Evidence: `systemd-journal-execute.h:29-88`,
`systemd-internals.h:144-168`, `systemd-journal-function.h:9`.

The current implementation therefore has three known performance costs that
SOW-0082 must avoid when the caller does not need the affected values:

- compressed DATA is expanded by the reader before the facets engine can decide
  whether the value is needed;
- all fields are traversed even after requested facet/filter data has been
  satisfied;
- reusable journal DATA objects are parsed and matched repeatedly across rows.

## FTS Semantics

`query` is compiled as a substring `SIMPLE_PATTERN` split by `|`
(`facets.c:1779-1784`).

Netdata `SIMPLE_PATTERN` supports exact, prefix, suffix, and substring modes;
the facets query uses substring mode and case-insensitive matching
(`simple_pattern.h:9-12`, `simple_pattern.h:29`,
`simple_pattern.c:47-68`, `simple_pattern.c:194-214`,
`facets.c:1783`).

The systemd plugin creates facets with `FACETS_OPTION_ALL_KEYS_FTS`, so all keys
are searchable unless a later implementation changes that option
(`systemd-journal.c:279-286`).

During row processing:

- each eligible field value is copied to a buffer only when FTS or non-view
  transforms require it (`facets.c:427-431`, `facets.c:1944-1967`);
- a positive match increments the row's positive FTS counter;
- a negative match increments the row's negative FTS counter.

A row is rejected if an FTS query exists and either no positive field matched or
any negative field matched (`facets.c:2277-2288`).

## Facet And Filter Counting Semantics

The facets engine stores selected values per key. Newly inserted values inherit
the key's default selected state unless explicitly selected otherwise
(`facets.c:467-514`, `facets.c:600-608`).

Registering a selected filter value makes the key non-default-selected and
stores that selected value (`facets.c:1858-1878`).

At row finish:

- every configured facet/filter key missing from the row receives the special
  unset value `-` (`facets.c:2296-2307`);
- `total_keys` is the number of keys with value indexes;
- `selected_keys` is the number of those keys whose current row value is
  selected;
- facet counters are updated when all keys except possibly the counted key are
  selected;
- the row is retained and histogram is updated only when all keys are selected.

Evidence: `facets.c:2273-2349`.

Implication:

- Facet counters are not simple counts over the already-filtered rows. They
  implement the standard faceted-search rule: for each facet key, count values
  under all other active filters while letting that key vary.

Current limits:

- up to 200 keys may be value-tracked as facets;
- up to 500 keys are tracked in one row;
- key and value hash tables start with 15 entries and resize as needed.

Evidence: `facets.c:4-9`, `facets.c:240-256`, `facets.c:397-416`,
`facets.c:618-628`.

## Top-N Row Retention

The engine keeps only up to `last` rows in memory for returned data:

- the first retained row creates the list;
- later rows are inserted by timestamp order;
- if the list is full, backward scans replace the oldest row and forward scans
  replace the newest row when the candidate belongs in the requested page;
- rows outside the requested anchor side are skipped.

Evidence: `facets.c:2101-2237`.

Rows are returned as arrays:

- timestamp in microseconds;
- row options object containing severity;
- one value per non-hidden registered key.

Evidence: `facets.c:2749-2806`.

Dynamic and transformed fields:

- `ND_JOURNAL_PROCESS` is a dynamic visible column derived from
  `CONTAINER_NAME`, `SYSLOG_IDENTIFIER`, `_COMM`, and `_PID`
  (`systemd-journal.c:170-175`, `systemd-journal-annotations.c:491-528`).
- Severity is derived from `PRIORITY` (`systemd-journal.c:166`,
  `systemd-journal-annotations.c:255-281`).
- Several fields have view-only transformations, such as `PRIORITY`,
  `SYSLOG_FACILITY`, `ERRNO`, `_BOOT_ID`, UID/GID fields, and
  `_SOURCE_REALTIME_TIMESTAMP` (`systemd-journal.c:182-257`).

## Histogram Semantics

The histogram field is configured by name for POST or default behavior and by ID
for GET behavior (`logs_query_status.h:830-837`, `facets.c:876-880`).

Histogram setup:

- swaps `after` and `before` if needed;
- enables the histogram;
- records the query timeframe;
- chooses a slot width from a fixed list of durations targeting roughly 150
  columns;
- aligns the histogram start and end to slot boundaries;
- caps slots at 1001.

Evidence: `facets.c:811-873`.

During row finish, the histogram updates only when:

- histogram is enabled;
- the histogram key has been found and value tracking is enabled;
- the row has a current value for the histogram key;
- the row timestamp is within the histogram timeframe;
- all facet/filter keys are selected.

Evidence: `facets.c:903-919`, `facets.c:2313-2344`.

Output:

- `available_histograms` lists all value-tracked, non-hidden keys;
- for explicit request facets, the list order follows the requested facet list,
  while each item's `order` metadata follows Netdata's sorted field order;
- `histogram` is emitted for full analysis;
- `histogram_delta` is emitted only for data-only plus delta mode;
- data is emitted as stacked-bar chart-compatible rows, one per histogram slot,
  with one dimension per observed value.
- histogram dimensions that occur in at least one bucket use numeric zero in
  buckets where that value is absent. Dimensions known only from the facet
  vocabulary use JSON `null` in every bucket where the value was not observed.

Evidence: `facets.c:2817-2868`, `facets.c:1209-1609`.

## Data-Only, Delta, Tail, And Stop-When-Full

`data_only=true` means "return rows fast" and skips normal `facets`,
`histogram`, `items`, `message`, `update_every`, and `help` output. It still
calls `facets_report()` so data rows and table scaffolding are returned
(`systemd-journal-execute.h:714-792`, `facets.c:2592-2600`,
`facets.c:2808-2884`).

Data-only without delta does not emit `facets`, `histogram`, or `items`. If a
histogram key was requested, `available_histograms` is still emitted so the UI
keeps its histogram selector metadata.

Data-only with delta emits `facets_delta`, `histogram_delta`, and `items_delta`
instead of the full-analysis names.

`delta=true` is forced off unless `data_only=true`
(`logs_query_status.h:752-753`).

`tail=true` is forced off unless both `data_only=true` and
`if_modified_since` are present (`logs_query_status.h:755-756`).

When data-only has no stop anchor, the per-file scanner can stop once enough
rows have been retained and scanning has passed the retained page by the learned
timestamp delta. This is checked every 128 processed rows
(`systemd-journal-execute.h:10-11`, `systemd-journal-execute.h:182-190`,
`systemd-journal-execute.h:291-299`).

`last_modified` is emitted for full queries and for tail queries
(`systemd-journal-execute.h:785-786`).

## Sampling And Estimation

Sampling is disabled when:

- `sampling` is zero;
- slice mode is off;
- data-only mode is on;
- no files matched;
- timeframe information is invalid.

Evidence: `systemd-journal-sampling.h:9-73`.

The relevant current constants are:

- `ND_SD_JOURNAL_SAMPLING_SLOTS`: `1000`;
- `ND_SD_JOURNAL_SAMPLING_RECALIBRATE`: `10000` rows;
- `ND_SD_JOURNAL_DEFAULT_TIMEOUT`: `60` seconds;
- `ND_SD_JOURNAL_PROGRESS_EVERY_UT`: `250 ms`;
- `FUNCTION_PROGRESS_EVERY_ROWS`: `8192` rows.

Evidence: `systemd-journal-function.h:6-11`,
`systemd-journal-execute.h:8-11`.

When enabled:

- histogram slots are used as sampling time slots, clamped to at least 2 and at
  most `ND_SD_JOURNAL_SAMPLING_SLOTS` (`1000`);
- global sampling starts after half the requested sampling budget;
- per-file and per-time-slot thresholds each start at one quarter of the budget,
  divided by matched files or slots respectively;
- thresholds are never below `last`.

Evidence: `systemd-journal-function.h:6`,
`systemd-journal-sampling.h:36-72`.

Sampling decisions:

- rows that may enter the retained Top-N page are always fully processed;
- otherwise rows are fully sampled until global, per-file, and per-slot minimums
  are reached;
- after that, the scanner samples one row every recalibrated `every` interval;
- skipped rows call `facets_row_finished_unsampled()`;
- if unsampled rows exceed sampled rows and at least one percent of the file's
  estimated query progress has elapsed, the file scan stops and remaining rows
  are estimated. This is the over-budget sampling path.

Per-file recalibration estimates remaining rows, divides them by
`(sampling / 2) / files_matched`, and clamps the interval to at least one row.
The recalibration cadence is `ND_SD_JOURNAL_SAMPLING_RECALIBRATE` (`10000`)
rows (`systemd-journal-function.h:7`,
`systemd-journal-sampling.h:313-333`).

Evidence: `systemd-journal-sampling.h:342-405`,
`systemd-internals.h:16`.

Estimation:

- remaining rows are estimated by seqnum when writer IDs and seqnums are usable;
- otherwise they are estimated by time progress;
- remaining estimated rows are distributed into histogram slots by overlap
  duration.

Evidence: `systemd-journal-sampling.h:208-310`,
`systemd-journal-sampling.h:408-424`, `facets.c:931-982`.

Output includes query and per-file sampling counters, and user-facing messages
when data is partial, unsampled, or estimated
(`systemd-journal-execute.h:639-647`, `systemd-journal-execute.h:740-768`,
`systemd-journal-execute.h:801-809`).

## Progress, Timeout, Cancellation, And Partial Results

Progress:

- row and byte counters are updated every 8192 processed rows and at file end
  (`systemd-journal-execute.h:8-10`, `systemd-journal-execute.h:192-210`,
  `systemd-journal-execute.h:301-319`);
- plugin progress is emitted to stdout every 250 ms of accumulated per-file
  query time (`systemd-journal-function.h:11`,
  `systemd-journal-execute.h:607-613`).
- The Rust SDK Netdata function API exposes progress as a caller-provided
  callback instead of writing to stdout. The callback receives the current file
  index, total selected files, matched/skipped file counts, cumulative explorer
  stats, and elapsed time. The CLI wrapper remains a thin adapter over this
  API.

Timeout/cancellation:

- `check_stop()` returns cancelled when the shared cancelled flag is set;
- it returns timed out when monotonic time exceeds `stop_monotonic_ut`;
- the per-file loop checks this at the row progress cadence.
- `stop_monotonic_ut` is supplied by the Netdata function framework; the plugin
  registers `ND_SD_JOURNAL_DEFAULT_TIMEOUT` (`60` seconds) as its default
  function timeout, so timeout budget ownership is Netdata integration policy,
  not a journal file-format rule.
- The Rust SDK Netdata function API exposes timeout and cancellation as
  caller-provided run options. Cancellation is a callback/token-equivalent
  predicate checked at the Explorer row cadence and before starting each file.
  Timeout uses the same run-control path and returns partial results with the
  stop reason recorded in the response status/message.

Evidence: `systemd-main.c:78-91`, `systemd-journal-function.h:10`,
`systemd-journal.c:263-291`, `systemd-journal-execute.h:91-104`,
`systemd-journal-execute.h:192-200`, `systemd-journal-execute.h:301-309`.

Before starting a later file, the query refuses work if the current time plus
three times the slowest prior file duration would exceed the stop time. This
marks the response partial (`systemd-journal-execute.h:572-579`).

Status mapping:

- cancellation returns HTTP 499;
- failed open/seek returns HTTP 500;
- no modification returns HTTP 304;
- timed out and no-file-matched can still return partial/empty JSON status 200
  depending on the final status handling.

Evidence: `systemd-journal-execute.h:651-712`.

## Output Model

All successful query responses include:

- `status: 200`;
- `partial`;
- `type: "table"`;
- table configuration with pagination;
- `columns`;
- `data`;
- `_stats`;
- `_journal_files`;
- `_fstat_caching`;
- `expires`.

Evidence: `systemd-journal-execute.h:714-792`,
`systemd-journal-execute.h:794-812`, `facets.c:2578-2597`,
`facets.c:2674-2974`.

SDK column-catalog policy:

- The SDK builds the table column catalog from FIELD indexes of every selected
  journal file, independently of the visible timeframe slice and independently
  of returned rows.
- This intentionally avoids `systemd-journal.plugin`'s implementation artifact
  of discovering columns while traversing row DATA. It makes the UI field list
  stable while users page through rows in the same selected file set.
- The SDK may therefore expose columns that the plugin would not discover in a
  narrow traversal. Returned rows still contain values only for fields present
  in those rows.
- Row-traversal column discovery is not a production feature. The Rust
  `ExplorerQuery::debug_collect_column_fields_by_row_traversal` marker exists
  only to diagnose explorer discrepancies and is rejected by production
  explorer entrypoints. Any comparison that passes only with that marker enabled
  has found an SDK explorer bug, not a valid compatibility mode.

Full analysis adds:

- `message`;
- `update_every`;
- `help`;
- `accepted_params`;
- `facets`;
- `available_histograms`;
- `histogram`;
- `items`;
- `default_sort_column`;
- `default_charts`.

Evidence: `systemd-journal-execute.h:719-783`, `facets.c:2597-2672`,
`facets.c:2808-2884`.

Data-only plus delta emits `facets_delta`, `histogram_delta`, and `items_delta`
instead of the full analysis names (`facets.c:2604-2616`,
`facets.c:2846-2855`, `facets.c:2873-2884`).

Strict comparison treats data-only all-null column-catalog differences as
non-content only when those columns have no returned-row value on either side.
This preserves the Explorer production rule that column catalogs come from
FIELD indexes while still rejecting any missing non-null returned field.

The Rust SDK Netdata function API validates and echoes `data_only`, `delta`,
`tail`, `sampling`, and `if_modified_since` using the same high-level rules:

- `delta` is effective only when `data_only=true`;
- `tail` is effective only when both `data_only=true` and
  `if_modified_since` is non-zero;
- data-only delta responses use `facets_delta`, `histogram_delta`, and
  `items_delta`;
- full responses and tail responses include `last_modified`, derived from the
  latest journal realtime among matched rows.

## Generic Explorer Semantics

These behaviors are generic and should be part of the SDK optimized explorer API:

- select candidate files by source and timeframe;
- use native journal indexes for filter slicing where possible;
- implement OR within selected values of one key and AND across selected keys;
- keep facet-counter semantics where each facet counts values under all other
  active filters;
- support a histogram for one selected field over the full query timeframe;
- return Top-N rows with anchor and direction;
- support data-only paging and optional delta output;
- support FTS as an opt-in path that necessarily expands searchable values;
- avoid expanding compressed or unrelated DATA unless needed by filters, facets,
  histogram, FTS, or returned-row display;
- enumerate field names through FIELD hash tables for column catalogs instead
  of discovering fields by row traversal;
- avoid repeated work for reusable DATA objects.

## Netdata-Specific Policy

These behaviors are Netdata UI/plugin policy and should be configurable or
kept outside the core SDK unless explicitly accepted later:

- exact response JSON shape and RRDF table/chart envelope;
- field lists included/excluded from default facets
  (`systemd-journal.c:59-154`);
- `ND_JOURNAL_FILE` and `ND_JOURNAL_PROCESS` synthetic fields
  (`systemd-journal-execute.h:6`, `systemd-journal.c:16`,
  `systemd-journal.c:170-175`);
- severity mapping and display transformations
  (`systemd-journal.c:166-257`, `systemd-journal-annotations.c:255-318`);
- source group names and source-class taxonomy;
- progress messages over the pluginsd stdout protocol;
- fstat-cache reporting and `_journal_files` diagnostic output;
- API hash-ID behavior for GET requests.

## Requirements For SOW-0082

The Rust optimized explorer API must:

1. Provide a low-level engine that can scan only requested facet/histogram/FTS
   fields while preserving row-level pointer guarantees from the reader.
2. Avoid decompressing compressed DATA unless the DATA is needed for one of:
   native filter validation fallback, facet counting, histogram dimension,
   FTS, or returned-row display.
3. Stop row field traversal once all required non-FTS fields have been found
   for the current row.
4. Cache per-DATA parsing and per-DATA facet/histogram/FTS classification so
   reused DATA objects are not reprocessed repeatedly.
5. Keep the existing faceted-search counting semantics, especially
   "count this facet while all other selected filters apply".
6. Keep Top-N row semantics, including anchor side filtering, direction, and
   timestamp uniqueness.
7. Keep source realtime timestamp adjustment semantics or explicitly expose a
   configurable replacement.
8. Keep slice/native filtering semantics and preserve the fallback when native
   match setup fails.
9. Expose enough metrics to compare against the current plugin: rows evaluated,
   rows matched, returned rows, bytes touched, decompressions, DATA cache hits,
   skipped compressed DATA, elapsed time, and partial/sampling counters.

## Open Risks For SOW-0082

- The current plugin uses libsystemd unique-value enumeration for slice setup.
  The SDK must implement the equivalent using journal FIELD/DATA chains without
  row scans.
- The current plugin's facet engine stores selected values by hash ID for GET.
  A generic SDK API should probably use explicit field/value bytes and expose
  stable IDs as an adapter layer, but this needs careful compatibility tests.
- Multi-value fields are noted as a TODO for `_UDEV_DEVLINK` in the plugin
  (`systemd-journal.c:3-7`). The SDK must decide whether to preserve current
  overwrite-like behavior or support multiple values per row for explorer
  semantics.
- The one-hour default timeframe differs from the existing Netdata facets
  README. Integration should not rely on that README statement without a
  product decision.
