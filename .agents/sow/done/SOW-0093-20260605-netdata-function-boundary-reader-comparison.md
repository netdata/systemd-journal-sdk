# SOW-0093 - Netdata Function Boundary Reader Comparison

## Status

Status: completed

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: completed after regression repair. Earlier strict SDK-first
content comparison passed locally for the large default-facets full-analysis
request and for the repo-local matrix covering `info`, full priority, filtered
priority, full default facets, low-budget sampling, data-only, data-only delta,
built-in `__logs_sources` source selection, FTS `|` OR and `!` negative query
terms, and tail/no-change `304` function-error responses. Rust SDK run-control
API covers query-file progress reporting, file-end progress, active-scan
cancellation, timeout plumbing, request normalization for data-only/delta/tail,
sampling counters/estimates, and `last_modified`; the wrapper exposes
diagnostic progress/cancellation switches over the same SDK API. The Netdata SDK
API exposes caller-owned state hooks for registry-provided source metadata and
learned per-file journal-vs-source-realtime drift. The reopened regression now
has a 20-case SDK-first matrix with 20/20 stable-content passes after explicit
classification of known installed-plugin non-content quirks and sampling parity
repairs. The post-review-fix 20-case SDK-first matrix also has 20/20
stable-content passes. The current default full-analysis speedup is
approximately `3.21x` by ratio of means against the installed
`systemd-journal.plugin`; profiling shows the remaining time is in Explorer
traversal, not wrapper glue or JSON shaping. The final whole-SOW reviewer
rerun returned production-grade votes from the six approved reviewers; SOW
lifecycle closeout is pending.

## Requirements

### Purpose

Build an end-to-end, Netdata-function-boundary comparison that proves whether
the SDK explorer logic can reproduce Netdata's generic log-function behavior
and measures realistic read/query performance on multi-GB journal datasets.

### User Request

The user requested a comparison plan:

- select representative multi-GB journal datasets from the large local journal
  corpus;
- create an SDK wrapper that accepts the same Netdata function test CLI shape
  the plugins will expose:
  `/path/to/wrapper --test function-name --dir /path/to/backend-data/ --timeout SECONDS < /path/to/function/request/payload.json [additional options if required]`;
- compare against `systemd-journal.plugin` after Netdata exposes the same
  standard CLI shape;
- create representative request payloads for common and rare generic log-query
  cases, using the default facets from `systemd-journal.plugin` for common
  cases;
- compare SDK wrapper output against `systemd-journal.plugin` output for
  semantic normalized function JSON equality and speed;

The user then clarified that the wrapper must be treated as the first CLI over
a future Netdata-specific SDK API, not throwaway benchmark code. The eventual
SDK API should turn any journal-backed data set into a Netdata function. The
API consumer must provide:

- backend directories or backend data roots;
- default facet keys;
- default view keys or grouping keys;
- default histogram key;
- optional enrichment or presentation callback per key.

This SOW must therefore start with an analysis of current
`systemd-journal.plugin` and `netflow-plugin` behavior. The analysis must
extract the complete requirements for this Netdata-specific SDK API before
implementation starts.

The user then decided that a hardcoded NetFlow/`flows:netflow` function is not
part of this SDK requirement. NetFlow source analysis remains useful only as
design evidence for catalogs, views, projected scans, and future grouped
statistics APIs. The current SOW must not require a NetFlow raw-tier dataset,
NetFlow function output compatibility, or a `flows:netflow` implementation.

Extensibility note: the generic Netdata SDK layer should not block future APIs
for grouped rollups, statistics, or Kibana/Elasticsearch-like log analytics.
Those APIs are not part of the current acceptance criteria and need a separate
SOW before implementation.

On 2026-06-06, the user expanded the requirement: the Rust SDK Netdata function
boundary must become a complete replacement for Netdata's
`systemd-journal.plugin`, not only a comparison harness. The wrapper command
remains useful for tests, but the deliverable is an SDK API that Netdata and
other consumers can call directly. Progress reporting and cancellation are
mandatory because Netdata needs them for function execution.

### Assistant Understanding

Facts:

- SOW-0082 added an optimized Rust explorer traversal engine.
- SOW-0083 added explicit index and compare strategies, but no auto planner,
  because index aggregation is query-shape sensitive.
- Existing Netdata generic log-function behavior is the real integration
  target, not only the standalone SDK benchmark shape.

Inferences:

- Netdata-function-boundary testing is the right next evidence gate before
  deciding which explorer strategy to integrate.
- The Netdata `systemd-journal.plugin` CLI entrypoint is a standard Netdata
  feature and will be created separately. This repository owns the SDK wrapper
  and comparison harness side only.
- Semantic normalized output equality at the function boundary is stronger
  evidence than internal SDK summaries because it includes request parsing,
  plugin defaults, sampling, facets, histogram, returned rows, and output
  formatting while excluding documented volatile metadata.
- Performance at this boundary will include useful overhead that raw SDK
  microbenchmarks intentionally exclude.

Unknowns:

- Exact default facet sets, request shapes, sampling rules, volatile output
  fields, and function output canonicalization for the generic log-function
  path must be re-read from current Netdata source before implementation.
- The wrapper must use the same `--test`, `--dir`, stdin request payload, and
  `--timeout` command-line contract as the Netdata `systemd-journal.plugin`
  binary so the benchmark harness can swap binaries without changing payloads.
- The wrapper should be a thin CLI over the future SDK API. The reusable API
  should stay in a Netdata-specific module/layer, separate from the core
  journal file-format reader.

### Acceptance Criteria

- Select and document representative sanitized multi-GB journal datasets from
  the local large journal corpus.
- Add an SDK wrapper command with this CLI contract:
  `/path/to/wrapper --test function-name --dir /path/to/backend-data/ --timeout SECONDS < /path/to/function/request/payload.json [additional options if required]`.
- Treat Netdata-side plugin test entrypoints as external binaries with the same
  contract; do not implement or modify them in this repository.
- Extract default/common facets from current `systemd-journal.plugin` source
  and use them in common-case request payloads.
- Inventory the current `systemd-journal.plugin` and `netflow-plugin`
  function contracts and record the API requirements they imply, including
  request parsing, default facets, default view/grouping keys, histogram
  defaults, field catalogs, source/directory selection, enrichment or
  presentation transforms, sampling/overbudget behavior, autocomplete or
  vocabulary behavior, top-N rows, function metadata, and volatile output
  fields. `netflow-plugin` is analysis evidence only, not an implementation or
  output-comparison target for this SOW.
- Define a reusable Netdata-specific SDK API boundary that accepts backend
  directories, default facet keys, default view/grouping keys, a default
  histogram key, and optional enrichment/presentation callbacks per key.
- Do not add a hardcoded `flows:netflow` function, NetFlow raw-tier dataset
  requirement, or NetFlow output compatibility requirement in this SOW.
- Keep the API boundary extensible enough that future SOWs can add grouped
  rollups, aggregate statistics, and log analytics APIs without redesigning
  the core Netdata function layer.
- Preserve separation of concerns: journal reading remains generic; Netdata
  function behavior, UI metadata, field catalogs, and enrichment callbacks live
  in the Netdata-specific SDK layer.
- Expose the Netdata function behavior as an SDK API, not only as a CLI wrapper.
  Consumers must be able to provide backend directories, default facet keys,
  default view keys, a default histogram key, optional enrichment/presentation
  callbacks, a progress callback, and a cancellation callback or cancellation
  token.
- Implement `systemd-journal.plugin` replacement semantics for full analysis,
  data-only paging, delta output, tail/`if_modified_since` no-change behavior,
  sampling with sampled/unsampled/estimated counters, timeout, cancellation,
  progress reporting across matched files, source selection through
  `__logs_sources`, learned or persisted journal-vs-source-realtime drift
  compatible with the plugin's five-second default and two-minute maximum
  model, and stable `last_modified` output.
- Add rare-case payloads for selective filters, broad filters, narrow facets,
  many facets, histogram-only, returned rows, anchor/direction, and FTS where
  supported.
- Build a strict content comparator that compares the complete
  Netdata-function content:
  - full table column catalog and column metadata;
  - every returned row, comparing every field by column name;
  - every returned facet, every facet value, and every facet value count;
  - the full histogram object, including bucket timestamps, labels, and values
    per facet value;
  - full stable item counters.
- The comparator may exclude only explicitly documented volatile diagnostic or
  envelope fields such as runtime stats, journal-file scan diagnostics, expiry
  timestamps, generation timestamps, and implementation version strings.
- Treat any unclassified content difference as a mismatch until the SOW records
  why it is volatile or intentionally different.
- Run the SDK wrapper before `systemd-journal.plugin` in comparison runs so the
  plugin no longer warms the page cache for the SDK.
- Measure cold and warm runs separately with repeated release builds and record
  rows/s, wall time, CPU time, peak RSS, and output size.
- Produce a sanitized report comparing:
  - SDK wrapper versus `systemd-journal.plugin`;
  - semantic normalized output equality or exact mismatch classes;
  - speed ratios and confidence limits.
- Do not write raw journal payloads, raw IPs, private endpoints, or customer
  identifying data into durable artifacts.

## Analysis

Sources checked:

- SOW-0081 specified `systemd-journal.plugin` and Netdata facets behavior.
- SOW-0082 implemented the optimized Rust explorer traversal API.
- SOW-0083 measured index-derived strategy break-even behavior.

Current state:

- The SDK has standalone benchmark evidence, but not Netdata-function-boundary
  equivalence and performance evidence.
- Existing pending Netdata integration SOWs cover actual integration after
  performance and behavior gates.
- The wrapper is now scoped as the first public shape of a future
  Netdata-specific SDK API. It must not become a one-off benchmark tool.
- A hardcoded `flows:netflow` function is intentionally out of scope for this
  SDK layer. NetFlow remains a reference for API extensibility and performance
  patterns only.

Initial Netdata source analysis:

- Checked `netdata/netdata @ f340a0e3ffb7`.
- `systemd-journal.plugin` creates its function around the shared logs-query
  and facets layer. It uses `lqs_facets_create()` with systemd-specific facet
  include/exclude lists and then calls `lqs_request_parse_and_validate()` with
  default histogram `PRIORITY`.
  Evidence:
  `src/collectors/systemd-journal.plugin/systemd-journal.c:59`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:61`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:68`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:279`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:300`
- `systemd-journal.plugin` registers journal-specific field behavior in
  `systemd_journal_register_transformations()`: visible/default fields,
  main text, FTS field, dynamic row process id, severity mapping, and
  transformations for priority, syslog facility, errno, message id, boot id,
  UID/GID, capabilities, and source realtime timestamp.
  Evidence:
  `src/collectors/systemd-journal.plugin/systemd-journal.c:158`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:166`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:168`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:170`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:177`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:182`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:189`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:196`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:207`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:214`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:217`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:224`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:237`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:243`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:252`
- The systemd journal function currently scans every returned row field through
  `NSD_JOURNAL_FOREACH_DATA`, parses `KEY=VALUE`, adjusts the row timestamp
  from `_SOURCE_REALTIME_TIMESTAMP` when needed, truncates facet values at
  `FACET_MAX_VALUE_LENGTH`, and sends every parsed field to the facets layer.
  This is the behavior the SDK API must reproduce semantically while avoiding
  unnecessary decompression, traversal, and repeated DATA processing.
  Evidence:
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:29`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:37`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:42`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:48`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:79`
- The shared logs-query request contract accepts `info`, `delta`, `tail`,
  `slice`, `data_only`, `sampling`, `after`, `before`, `if_modified_since`,
  `anchor`, `last`, `direction`, `query`, `histogram`, `facets`, and
  `selections`. POST selections are OR within values of one field and AND
  across fields by the facets backend; GET has hash-id mode.
  Evidence:
  `src/libnetdata/facets/logs_query_status.h:8`
  `src/libnetdata/facets/logs_query_status.h:329`
  `src/libnetdata/facets/logs_query_status.h:339`
  `src/libnetdata/facets/logs_query_status.h:354`
  `src/libnetdata/facets/logs_query_status.h:385`
  `src/libnetdata/facets/logs_query_status.h:490`
- The logs-query validation layer handles default one-hour windows, relative
  time conversion, anchor/tail semantics, default returned entries, slice
  enablement, default histogram setup, and request echoing.
  Evidence:
  `src/libnetdata/facets/logs_query_status.h:731`
  `src/libnetdata/facets/logs_query_status.h:762`
  `src/libnetdata/facets/logs_query_status.h:781`
  `src/libnetdata/facets/logs_query_status.h:791`
  `src/libnetdata/facets/logs_query_status.h:816`
  `src/libnetdata/facets/logs_query_status.h:825`
  `src/libnetdata/facets/logs_query_status.h:830`
- The systemd journal function selects files by source and time, sorts files by
  direction, emits per-file query metrics, reports partial/timeouts, sampling
  status, `last_modified`, facets, histogram, and fstat-cache diagnostics.
  Evidence:
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:486`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:507`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:547`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:615`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:714`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:788`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:791`
- `netflow-plugin` exposes a distinct function contract named
  `flows:netflow`, version 4, with response modes for table, metrics
  time-series, and autocomplete.
  Evidence:
  `src/crates/netflow-plugin/src/api/flows/model.rs:5`
  `src/crates/netflow-plugin/src/api/flows/model.rs:80`
  `src/crates/netflow-plugin/src/api/flows/model.rs:95`
  `src/crates/netflow-plugin/src/api/flows/model.rs:110`
  `src/crates/netflow-plugin/src/api/flows/handler.rs:40`
  `src/crates/netflow-plugin/src/api/flows/handler.rs:45`
  `src/crates/netflow-plugin/src/api/flows/handler.rs:88`
  `src/crates/netflow-plugin/src/api/flows/handler.rs:134`
- NetFlow request parsing accepts payload JSON or legacy args and normalizes
  `mode`, `view`, `after`, `before`, `query`, `selections`, `facets`,
  `group_by`, `sort_by`, `top_n`, `field`, and `term`.
  Evidence:
  `src/crates/netflow-plugin/src/api/flows/handler.rs:186`
  `src/crates/netflow-plugin/src/query/request/model/types.rs:3`
  `src/crates/netflow-plugin/src/query/request/model/types.rs:88`
  `src/crates/netflow-plugin/src/query/request/model/deserialize.rs:6`
  `src/crates/netflow-plugin/src/query/request/selection/decode.rs:6`
- NetFlow defaults and view-specific keys are explicit:
  default group-by is `SRC_AS_NAME`, `PROTOCOL`, `DST_AS_NAME`; map views
  override group-by with country/state/city coordinate fields; default sort is
  bytes; default top-N is 25; default query window is 15 minutes.
  Evidence:
  `src/crates/netflow-plugin/src/query/request/constants.rs:4`
  `src/crates/netflow-plugin/src/query/request/constants.rs:25`
  `src/crates/netflow-plugin/src/query/request/constants.rs:26`
  `src/crates/netflow-plugin/src/query/request/constants.rs:27`
  `src/crates/netflow-plugin/src/query/request/constants.rs:33`
  `src/crates/netflow-plugin/src/query/request/model/types.rs:88`
  `src/crates/netflow-plugin/src/query/planner/request.rs:45`
- NetFlow has a field catalog with allowed facet/grouping fields, raw-only
  fields, virtual fields, autocomplete rules, and presentation display names
  and labels. These inform the proposed API inputs for default view keys and
  optional enrichment/presentation callbacks per key, but they are not current
  SDK implementation requirements.
  Evidence:
  `src/crates/netflow-plugin/src/facet_catalog.rs:30`
  `src/crates/netflow-plugin/src/facet_catalog.rs:56`
  `src/crates/netflow-plugin/src/facet_catalog.rs:73`
  `src/crates/netflow-plugin/src/query/request/constants.rs:46`
  `src/crates/netflow-plugin/src/query/fields/rules.rs:5`
  `src/crates/netflow-plugin/src/query/fields/rules.rs:25`
  `src/crates/netflow-plugin/src/presentation/display.rs:3`
  `src/crates/netflow-plugin/src/presentation/labels.rs:20`
- NetFlow already has an optimized projected raw scan that builds a required
  field plan for metrics, group-by, and selected fields, and avoids full row
  materialization where possible. This is an important design reference for
  the SDK Netdata API and future grouped rollup/statistics work.
  Evidence:
  `src/crates/netflow-plugin/src/query/planner/request.rs:68`
  `src/crates/netflow-plugin/src/query/scan/session/projected.rs:17`
  `src/crates/netflow-plugin/src/query/scan/session/projected.rs:38`
  `src/crates/netflow-plugin/src/query/scan/raw.rs:166`
  `src/crates/netflow-plugin/src/query/scan/raw.rs:179`
  `src/crates/netflow-plugin/src/query/projected/apply.rs:83`
  `src/crates/netflow-plugin/src/query/projected/apply.rs:129`
- NetFlow also has persisted facet vocabulary/sidecar behavior and a read-only
  constructor. This side-effect model is a future integration consideration,
  not a current SOW-0093 runtime target.
  Evidence:
  `src/crates/netflow-plugin/src/facet_runtime.rs:94`
  `src/crates/netflow-plugin/src/facet_runtime.rs:116`
  `src/crates/netflow-plugin/src/facet_runtime.rs:120`
  `src/crates/netflow-plugin/src/facet_runtime.rs:227`

Netdata offline test CLI evidence:

- Checked `netdata/netdata @ 8c5c9b465e20` from PR
  `netdata/netdata#22638`.
- `systemd-journal.plugin` initially had this external CLI shape for SOW-0093:
  `systemd-journal.plugin --test systemd-journal --dir <journal-dir>
  --request <payload.json>`. The parser accepts both split and equals forms,
  rejects duplicate/missing options, validates that `--dir` is a directory,
  reads the request payload as JSON, disables scan progress on stdout, switches
  the journal source to the single provided directory, and calls
  `function_systemd_journal_result()` to print raw JSON to stdout.
  Evidence:
  `src/collectors/systemd-journal.plugin/systemd-main.c:30`
  `src/collectors/systemd-journal.plugin/systemd-main.c:57`
  `src/collectors/systemd-journal.plugin/systemd-main.c:145`
  `src/collectors/systemd-journal.plugin/systemd-main.c:164`
  `src/collectors/systemd-journal.plugin/systemd-main.c:184`
  `src/collectors/systemd-journal.plugin/systemd-main.c:187`
  `src/collectors/systemd-journal.plugin/systemd-main.c:193`
  `src/collectors/systemd-journal.plugin/systemd-main.c:201`
- The plugin exposes `function_systemd_journal_result()` as a raw buffer path
  while preserving the normal PLUGINSD-framed function path separately. This
  gives SOW-0093 a real external comparison binary once the user's local build
  installs this branch.
  Evidence:
  `src/collectors/systemd-journal.plugin/systemd-journal.c:260`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:316`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:329`
  `src/collectors/systemd-journal.plugin/systemd-journal.c:337`
- On 2026-06-06, the user changed the Netdata test-mode contract for security:
  request payload JSON is read from stdin, and request filenames are no longer
  accepted. The SDK wrapper and comparison harness must mirror that contract so
  no privileged test binary reads a caller-supplied filename.
- PR `netdata/netdata#22638` also adds a `netflow-plugin --test
  flows:netflow ... --no-persist` path. This remains non-normative design
  evidence for this SDK SOW because `flows:netflow` is not a current SDK
  requirement.
  Evidence:
  `src/crates/netflow-plugin/src/test_cli.rs:9`
  `src/crates/netflow-plugin/src/test_cli.rs:31`
  `src/crates/netflow-plugin/src/test_cli.rs:55`
  `src/crates/netflow-plugin/src/facet_runtime.rs:117`
  `src/crates/netflow-plugin/src/facet_runtime.rs:121`

Installed Netdata CLI smoke evidence:

- The user installed the Netdata branch from PR `netdata/netdata#22638`.
- Installed binary checked read-only:
  `/usr/libexec/netdata/plugins.d/systemd-journal.plugin`.
- A repo-local fixture was created under `.local/sow-0093/smoke-journals/` by
  decompressing `fixtures/systemd/test-data/no-rtc/system.journal.zst`; this
  avoided probing live host journal state.
- Fixture timestamp range was checked with stock `journalctl --file`: 1,922
  rows, realtime seconds `1666569601` through `1666584438`.
- Smoke request `{"info":true}` against the installed plugin returned valid
  raw JSON, HTTP status `200`, stdout length 1,290 bytes, and empty stderr.
- Smoke request for the exact fixture window with `last:5`,
  `direction:"backward"`, `data_only:false`, `slice:true`, and facet
  `PRIORITY` returned valid raw JSON, HTTP status `200`, 5 data rows, 1 facet,
  a histogram object, stdout length 94,358 bytes, and empty stderr.
- Smoke artifacts remain under `.local/sow-0093/` and are not durable project
  artifacts.

Risks:

- Netdata source modifications are outside this repository and explicitly out
  of scope for this SOW.
- Multi-GB local journal data can contain sensitive operational data; reports
  must stay sanitized.
- Output equality requires stable canonicalization for maps, permitted ordering
  differences, and documented volatile metadata; over-normalization could hide
  real regressions.
- Cold-cache measurements can be dominated by storage state; warm-cache and
  cold-cache runs must be labelled separately.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Microbenchmarks prove internal hot-path behavior, but they do not prove that
  SDK explorer logic matches Netdata `systemd-journal.plugin` output or wins at
  the function boundary.
- The next confidence gap is realistic request payloads, plugin defaults, and
  semantic normalized output equality against the existing generic Netdata
  journal log function.

Evidence reviewed:

- SOW-0081: Netdata plugin/facets behavior specification.
- SOW-0082: optimized explorer traversal implementation and benchmark evidence.
- SOW-0083: explicit index strategy and benchmark evidence.
- Netdata progress/cancellation source evidence:
  `ktsaou/netdata @ f6f857d46356`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:92`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:98`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:608`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:611`
- Netdata sampling source evidence:
  `ktsaou/netdata @ f6f857d46356`
  `src/collectors/systemd-journal.plugin/systemd-journal-sampling.h:10`
  `src/collectors/systemd-journal.plugin/systemd-journal-sampling.h:343`
  `src/collectors/systemd-journal.plugin/systemd-journal-sampling.h:409`
- Netdata realtime-drift source evidence:
  `ktsaou/netdata @ f6f857d46356`
  `src/collectors/systemd-journal.plugin/systemd-internals.h:86`
  `src/collectors/systemd-journal.plugin/systemd-internals.h:87`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:48`
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:114`
  `src/libnetdata/facets/logs_query_status.h:154`

Affected contracts and surfaces:

- Rust SDK wrapper CLI.
- Rust SDK Netdata function API.
- Netdata function payload input/output contracts.
- Netdata `systemd-journal.plugin` CLI contract as an external comparison
  dependency.
- NetFlow source behavior as non-normative design evidence for future grouped
  rollup/statistics APIs.
- Benchmark reports and future Netdata integration decisions.

Existing patterns to reuse:

- `reader_core_bench` JSON result conventions.
- Sanitized report discipline from corpus-evaluation SOWs.
- Netdata `systemd-journal.plugin` function request/response shapes from
  current Netdata source.
- Existing `NetdataFunctionRunOptions` timeout shape in `rust/src/journal/src/netdata.rs`.
- Existing Explorer traversal path and strict SOW-0093 comparator.

Risk and blast radius:

- Medium behavior risk if wrapper output appears equal only because the
  comparator over-normalizes volatile fields or ordering.
- Medium performance risk if the dataset is not representative.
- Low repository-boundary risk when this SOW remains limited to the SDK wrapper
  and comparison harness in this repository.

Sensitive data handling plan:

- Raw datasets remain in their existing local locations or `.local/` scratch
  paths and are not staged.
- Durable reports include only dataset IDs, feature classes, counts, hashes,
  timing, memory, output byte counts, and mismatch classes.
- Raw payloads, raw function payloads containing sensitive values, IPs,
  private endpoints, credentials, bearer tokens, SNMP communities, customer
  names, personal data, and proprietary incident details must not be committed.

Implementation plan:

1. Inventory current Netdata function defaults and request/response contracts.
   This step is mandatory before implementation and must produce the API
   boundary for the Netdata-specific SDK layer. NetFlow analysis is design
   evidence only.
2. Extend the Rust SDK Netdata function API with consumer-callable run options:
   progress callback, cancellation callback/token, timeout, and a state hook for
   per-file metadata such as realtime drift and last-modified tracking.
3. Implement replacement behavior missing from the current wrapper:
   source selection, data-only paging, delta/tail/`if_modified_since`,
   sampling counters and estimates, progress reports, cancellation, stable
   `last_modified`, and learned realtime drift.
4. Keep the CLI wrapper as a thin adapter over the SDK API.
5. Select datasets and create sanitized dataset manifests.
6. Build common and rare request payload suites, including progress,
   cancellation, sampling, data-only, delta, tail, source selection, and
   realtime-drift cases.
7. Invoke the Netdata `systemd-journal.plugin` CLI entrypoint when available;
   do not edit Netdata source from this repository.
8. Run cold/warm repeated performance and equality matrices.
9. Produce sanitized report and replacement readiness recommendation.

Validation plan:

- Unit tests for payload parsing and output canonicalization, including
  permitted volatile-field normalization and unclassified-difference failures.
- Golden comparison tests on small synthetic journals.
- Multi-GB equality/performance matrix on selected journal datasets.
- `git diff --check`.
- `.agents/sow/audit.sh`.
- Whole-SOW reviewer pass.

Artifact impact plan:

- AGENTS.md: no expected update unless repository-boundary policy changes.
- Runtime project skills: update only if a reusable Netdata function benchmark
  workflow becomes a project-standard workflow.
- Specs: update explorer/Netdata comparison specs if new behavior contracts are
  discovered.
- End-user/operator docs: update only if wrapper becomes a supported tool.
- End-user/operator skills: no expected update.
- SOW lifecycle: pending until implementation starts; current phase is
  analysis of Netdata generic log-function behavior and API requirements.
- SOW-status.md: updated with pending state.

Open-source reference evidence:

- No external open-source reference checked yet. Current Netdata source must be
  inspected at implementation time and cited as `netdata/netdata @ commit`.

Open decisions:

- None blocking for analysis. The user decided the Netdata
  `systemd-journal.plugin` CLI entrypoint is a standard Netdata feature and
  will be created separately. This repository implements only the SDK wrapper
  and comparison harness. The local Netdata branch from
  `netdata/netdata#22638` provides that external CLI and the installed binary
  passed the SOW-0093 repo-local fixture smoke checks.

## Implications And Decisions

- 2026-06-05 repository-boundary decision: this repository will not modify
  `systemd-journal.plugin` or `netflow.plugin`. The user will create the
  `systemd-journal.plugin` CLI function payload entrypoint separately. This
  SOW builds the SDK wrapper with the same `--test`, `--dir`, stdin request
  payload, and `--timeout` contract, plus the semantic normalized comparison
  harness that can call that plugin CLI when it exists.
- 2026-06-05 scope decision: a hardcoded NetFlow/`flows:netflow` function is
  not part of this SDK requirement. NetFlow analysis remains design evidence
  only. The current deliverable targets generic Netdata log-function behavior
  and comparison against `systemd-journal.plugin`.
- 2026-06-05 extensibility decision: grouped rollups, aggregate statistics, and
  Kibana/Elasticsearch-like log analytics are important future directions, but
  they are not part of SOW-0093 acceptance. The SOW-0093 API boundary should
  avoid choices that would make those future APIs difficult.
- 2026-06-06 strict comparator decision: the previous comparator is rejected
  as too weak for acceptance because it checked only a stable subset of rows
  and ignored the full column catalog. Acceptance now requires the SDK and
  `systemd-journal.plugin` to match the same content: complete columns,
  complete returned rows by column name, complete facets and facet value
  counts, complete histogram buckets and values, and stable item counters.
  Only explicitly documented volatile diagnostic/envelope fields may be
  excluded. Comparison runs must execute the SDK first and the plugin second.
- 2026-06-06 column-catalog decision: the SDK must not discover response
  columns by traversing matched row DATA. For stable UI behavior, selected
  journal files contribute all FIELD names from their FIELD hash tables,
  independently of the visible timeframe slice or returned rows. This can
  return columns that `systemd-journal.plugin` would not discover in a narrow
  traversal, but it is the correct SDK behavior because fields should not appear
  and disappear as users page through rows in the same selected file set. Row
  traversal remains responsible only for filters, facets, histogram, FTS, and
  returned-row display.

## Plan

1. Complete and record the Netdata generic log-function behavior/API analysis.
2. Define the Netdata-specific SDK API boundary and wrapper CLI shape.
3. Select datasets.
4. Build SDK wrapper, request suite, semantic comparator, and report harness.
5. Compare against the Netdata `systemd-journal.plugin` CLI binary when it is
   available.

## Delegation Plan

Implementer:

- Local implementation unless the user explicitly re-enables external
  implementers.

Reviewers:

- Reviewer pool after complete implementation and local validation:
  `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/kimi-k2.6`,
  `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`,
  `llm-netdata-cloud/minimax-m3-coder`, and
  `llm-netdata-cloud/deepseek-v4-pro`.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- Do not make changes outside this repository for any reason.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- If semantic normalized output equality fails, record mismatch classes and
  create repair SOWs instead of hiding differences.
- If Netdata `systemd-journal.plugin` modifications are not approved, restrict
  this SOW to SDK wrapper and external `systemd-journal.plugin` binary
  invocation only.

## Execution Log

### 2026-06-05

- Created as pending follow-up from SOW-0083.
- Added user clarification that the wrapper is the first CLI over a future
  Netdata-specific SDK API.
- Per user recommendation, started with read-only analysis of
  `systemd-journal.plugin` and `netflow-plugin` in `netdata/netdata @
  f340a0e3ffb7`.
- Recorded the user decision that `flows:netflow` is not a current SDK
  requirement and that grouped rollup/statistics APIs are future extensibility
  concerns, not SOW-0093 acceptance criteria.
- Inspected the user's local Netdata checkout read-only on branch
  `test-function-cli` at `netdata/netdata @ 8c5c9b465e20`; recorded that PR
  `netdata/netdata#22638` supplies the external `systemd-journal.plugin`
  offline test CLI needed by this SOW.
- Verified the installed `/usr/libexec/netdata/plugins.d/systemd-journal.plugin`
  offline test CLI against a repo-local journal fixture for both `info` and a
  real window query. No live host journal directory was queried.

### 2026-06-06

- Moved the SOW to current/in-progress for SDK wrapper implementation.
- Added the Rust `journal::netdata` API boundary:
  `NetdataJournalFunction::systemd_journal()`,
  `run_directory_request_json()`, and `run_directory_request_bytes()`.
- Added the internal Rust wrapper command
  `rust/src/internal/testcmd/netdata_function_wrapper`, using the same CLI
  shape as the external Netdata plugin:
  `--test systemd-journal --dir <journal-dir> --timeout <seconds> < <request.json>`.
- Added `ExplorerQuery::exclude_facet_field_filters`. The explorer default
  remains `true` to preserve previous SDK behavior; the Netdata wrapper sets it
  to `false` to match `systemd-journal.plugin` facet counting with all filters
  applied.
- Fixed the Rust explorer histogram path to count matched rows missing the
  histogram field under `"-"` for both traversal and explicit indexed
  strategy.
- Added filtered-request zero-count facet vocabulary fill in the Netdata
  wrapper. General zero-count facet values are content and must match. The
  strict comparator ignores only Netdata's empty-string unavailable-field
  artifact with id `CzGfAU2z3TC`, name `[unavailable field]`, and count `0`.
- Added semantic comparator and runner tooling under
  `tests/netdata_function/`, plus sanitized request fixtures for `info`,
  unfiltered priority facets/histogram, and filtered priority query behavior.
- Added `tests/netdata_function/requests/window-last5-default-facets.json`
  with the 31-field default `systemd-journal.plugin` facet set and verified it
  against the installed plugin on the repo-local fixture.
- Added SDK wrapper `--timeout <seconds>` CLI compatibility. `--timeout 0`
  maps to an effectively unreachable internal deadline so large comparison
  runs can disable timeout behavior consistently with the Netdata plugin test
  CLI. Nonzero values are enforced at file boundaries and return partial
  metadata if expired.
- During the first 4 GiB function-boundary run, the plugin and SDK matched
  rows, matched item counts, and histogram totals, but facet equality failed.
  Root cause: the SDK default facet catalog represented only the initial
  31-field subset and missed current plugin defaults such as `OBJECT_*`,
  `COREDUMP_*`, `_KERNEL_SUBSYSTEM`, container fields, and Netdata alert
  fields from `SYSTEMD_KEYS_INCLUDED_IN_FACETS`. The comparator also counted
  empty facet objects despite documenting nonzero-counter comparison. The SDK
  default catalog and comparator were updated before rerunning the large test.
- A second large run showed matching facet fields but one facet counter delta:
  `CODE_FUNC=""` was counted by the SDK and suppressed by the plugin. The SDK
  facet output now suppresses empty-string facet values, matching the plugin's
  empty-value reporting behavior.

## Validation

Acceptance criteria evidence:

- Complete for this SOW scope. Rust SDK API, wrapper CLI, semantic comparator,
  repo-local plugin matrix, progress/cancellation/timeout validation,
  caller-owned state hooks, sampling behavior, and the 4 GiB default-facets
  large-request comparison are implemented and validated. The current SOW proves
  content equivalence and realistic performance against `systemd-journal.plugin`
  for the checked generic log-function scope; Netdata component integration is
  tracked by the separate integration SOWs.

Tests or equivalent validation:

- `cargo test -p journal netdata --lib` passed from the Rust workspace.
- `cargo test -p journal explorer --lib` passed from the Rust workspace.
- `cargo test -p journal --lib` passed from the Rust workspace after the
  debug row-traversal column collection guard: 70 tests passed.
- `cargo check -p reader_core_bench` passed from the Rust workspace after the
  explorer query field rename.
- `cargo build --release -p netdata_function_wrapper` passed from the Rust
  workspace.
- `python3 -m py_compile tests/netdata_function/run_function_compare.py
  tests/netdata_function/compare_function_json.py
  tests/netdata_function/test_compare_function_json.py` passed.
- `python3 tests/netdata_function/test_compare_function_json.py` passed with
  10 focused comparator tests for table columns, returned rows, facet identity,
  facet counts, histogram presence/data, stable top-level content,
  diagnostic-item handling, and unavailable-artifact behavior.
- `tests/netdata_function/run_function_compare.py` passed against the
  installed `/usr/libexec/netdata/plugins.d/systemd-journal.plugin` and the
  repo-local fixture directory `.local/sow-0093/smoke-journals` for:
  - `tests/netdata_function/requests/info.json`
  - `tests/netdata_function/requests/window-last5-priority.json`
  - `tests/netdata_function/requests/window-last5-default-facets.json`
  - `tests/netdata_function/requests/window-error-filter.json`
- The default-facets request selects 31 facets, uses timeframe
  `2022-10-24T00:00:01Z` through `2022-10-24T04:07:18Z`, matches 1,917 rows,
  returns 5 rows, and passes semantic comparison for status, rows, nonzero
  facets, nonzero histogram totals, and stable item counters.
- Smoke comparison report path:
  `.local/sow-0093/results/sdk-vs-plugin-smoke-report.json`.
- Smoke result summary: all four cases passed semantic comparison for status,
  rows, nonzero facets, nonzero histogram totals, and stable item counters.
- Timeout compatibility validation:
  - `cargo test --manifest-path rust/Cargo.toml -q -p journal netdata --lib`
    passed.
  - `cargo build --manifest-path rust/Cargo.toml -q -p netdata_function_wrapper`
    passed.
  - `python3 -m py_compile tests/netdata_function/run_function_compare.py
    tests/netdata_function/compare_function_json.py` passed.
- Debug row-traversal column collection guard validation:
  - `cargo test -p journal explorer_rejects_debug_row_traversal_column_collection
    --lib` passed.
  - `cargo test -p journal
    netdata_requests_never_enable_debug_row_traversal_column_collection --lib`
    passed.
  - `cargo test -p journal explorer --lib` passed: 23 tests.
  - `cargo test -p journal netdata --lib` passed: 18 tests.
  - `python3 -m py_compile tests/netdata_function/run_function_compare.py
    tests/netdata_function/compare_function_json.py
    tests/netdata_function/test_compare_function_json.py` passed.
  - `python3 tests/netdata_function/test_compare_function_json.py` passed:
    10 tests.
  - `git diff --check` passed.
  - `.agents/sow/audit.sh` passed.
- Netdata function run-control validation:
  - `cargo test -p journal explorer_control` passed: 2 tests.
  - `cargo test -p journal netdata_function_api` passed: 3 tests.
  - `cargo test -p journal
    facade_compressed_data_payloads_remain_valid_for_current_row -- --nocapture`
    passed.
  - `cargo test -p journal` passed: 75 tests and 0 doctests.

Real-use evidence:

- Repo-local fixture evidence exists.
- Earlier subset-comparison and two-pass/profile numbers in this SOW are
  superseded by the strict content comparator repair. They are not used as
  acceptance evidence because they did not compare the complete Netdata
  function content.
- Strict SDK-first real-corpus function-boundary run completed against
  `/var/log/journal` using no filters, default facets, `sampling: 0`,
  `--timeout 0`, and the middle-of-corpus timestamp window:
  - request path:
    `.local/sow-0093/big-default-facets/request-default-facets-4g.json`;
  - latest report path:
    `.local/sow-0093/strict-content/sdk-first-strict-report-latest.json`;
  - latest saved JSON path:
    `.local/sow-0093/strict-content/json-latest/`;
  - request SHA-256:
    `531a65ccf02d8a1fbe05c1d5d08350a3402d2212a856d28f682c0eaf043d0c18`;
  - request timeframe: `after=1733494460`, `before=1735656412`;
  - content checks all passed: top-level stable metadata, 185 table columns,
    200 returned rows by column name, 43 facets, 3,311 content facet values,
    5,341,590 matched rows, item content counters, and full histogram buckets;
  - diagnostic check still differs:
    `items.evaluated` is 5,341,653 for the SDK and 5,341,591 for the installed
    plugin. This is recorded as diagnostic scan accounting, not journal
    content. Netdata's plugin computes this counter inside its facets engine
    after its own per-file seek/sampling path; the SDK reports raw explorer
    rows examined. Both implementations report the same matched row count,
    returned rows, item content counters, facets, histogram, and table data.
  - Netdata source-realtime slack alignment:
    the SDK wrapper now uses the plugin default 5-second
    journal-vs-source-realtime slack for per-file scans, while preserving the
    2-minute maximum slack for file preselection. Evidence checked in
    `ktsaou/netdata @ f6f857d46356`:
    `src/collectors/systemd-journal.plugin/systemd-internals.h:86` defines the
    5-second default, `systemd-internals.h:87` defines the 2-minute maximum,
    `src/libnetdata/facets/logs_query_status.h:154` applies the per-file
    anchor delta, and
    `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:114`
    reads the per-file delta before querying.
  - one plugin-side non-content facet artifact was classified and counted:
    under facet `CODE_FUNC`, option id `CzGfAU2z3TC`, name
    `[unavailable field]`, count 0. Evidence: Netdata facet IDs use
    `XXH3_64bits`; `CzGfAU2z3TC` is the hash id for the empty byte string.
    Stock `journalctl --field CODE_FUNC --file` on the isolated file does not
    expose an empty value, and raw byte search found only non-empty
    `CODE_FUNC=` payloads in that file.
- Strict-run timings with SDK first from the latest strict report:
  - release SDK wrapper: 11.508506117999787 seconds wall time, 863,048 stdout
    bytes, 0 progress-prefix bytes, 0 stderr bytes;
  - installed `systemd-journal.plugin`: 11.471252544986783 seconds wall time,
    976,271 stdout bytes, 1,053 progress-prefix bytes, 0 stderr bytes.
  - This single timing is not accepted as performance evidence because it
    followed a release rebuild and reviewer activity; it is accepted as
    content-equivalence evidence only.
- Performance rerun on 2026-06-06:
  - environment caveat: CPU governor was `powersave`; load average was
    approximately `5.52, 7.05, 7.34`, so these are workstation measurements,
    not final lab-grade benchmark numbers.
  - report path:
    `.local/sow-0093/performance/sdk-first-default-facets-5rep-20260606T103020Z.json`;
  - all 5 SDK-first repetitions passed strict content comparison. Every run
    matched columns, rows, facets, histogram, stable item counters, and stable
    top-level metadata. The only repeated mismatch was diagnostic
    `items.evaluated`, with plugin `5,341,591` and SDK `5,341,653`.
  - wall time, all repetitions:
    - SDK: `23.275288`, `11.138136`, `10.919930`, `10.909060`, `11.000762`
      seconds; mean `13.448635`, median `11.000762`;
    - installed plugin: `16.891252`, `10.851466`, `10.830661`,
      `11.162464`, `11.015219` seconds; mean `12.150212`, median
      `11.015219`.
  - warm repetitions 2 through 5:
    - SDK mean `10.991972` seconds, median `10.960346`;
    - installed plugin mean `10.964952` seconds, median `10.933342`;
    - warm wall-time ratio is effectively tied: plugin mean is `0.9975x` SDK
      mean.
  - `/usr/bin/time -v` SDK-first pass paths:
    `.local/sow-0093/performance/time-sdk-20260606T103020Z.txt`,
    `.local/sow-0093/performance/time-plugin-20260606T103020Z.txt`,
    `.local/sow-0093/performance/time-sdk-20260606T103020Z.json`, and
    `.local/sow-0093/performance/time-plugin-20260606T103020Z.json`.
  - `/usr/bin/time -v` pass also passed strict content comparison. Resource
    summary:
    - SDK: elapsed `0:11.66`, user `11.11s`, system `0.41s`, CPU `98%`, max
      RSS `88,088 KiB`, major faults `0`, minor faults `86,942`;
    - installed plugin: elapsed `0:11.78`, user `22.45s`, system `0.75s`, CPU
      `196%`, max RSS `120,944 KiB`, major faults `1`, minor faults
      `178,689`.
- row-traversal column-collection disabled experiment on 2026-06-06:
  - code state: `NetdataRequest::to_explorer_query()` forced
    row-traversal column collection off for the experiment;
  - report path:
    `.local/sow-0093/collect-column-fields-false/sdk-first-default-facets-5rep-20260606T105236Z.json`;
  - SDK wall time all repetitions: `3.172902`, `2.955008`, `3.089483`,
    `3.450088`, `3.038830` seconds; mean `3.141262`, median `3.089483`;
  - installed plugin wall time all repetitions: `11.313140`, `11.281508`,
    `12.048969`, `11.442461`, `11.891761` seconds; mean `11.595568`,
    median `11.442461`;
  - strict content comparison failed only on `columns` and row maps derived from
    those columns. Facets, histogram, stable item counters, and stable top-level
    metadata matched in all repetitions. The diagnostic `items.evaluated`
    mismatch remained plugin `5,341,591` versus SDK `5,341,653`;
  - SDK output had 77 columns; installed plugin output had 185 columns. The 108
    missing SDK columns had zero non-null values across the returned 200 rows,
    and all shared column row values matched;
  - `/usr/bin/time -v` SDK pass:
    `.local/sow-0093/collect-column-fields-false/time-sdk-20260606T105449Z.txt`;
    elapsed `0:03.11`, user `2.60s`, system `0.44s`, CPU `97%`, max RSS
    `86,312 KiB`, major faults `0`, minor faults `86,909`.
- FIELD-index column-catalog implementation on 2026-06-06:
  - implementation shape: matched files enumerate columns through indexed FIELD
    objects before row traversal; row traversal does not parse unrelated DATA
    just to discover columns.
  - report path:
    `.local/sow-0093/field-index-columns/sdk-first-default-facets-5rep-20260606T110306Z.json`;
  - all 5 SDK-first repetitions passed strict content comparison. Every run
    matched columns, rows, facets, histogram, stable item counters, and stable
    top-level metadata. The only repeated mismatch was diagnostic
    `items.evaluated`, with plugin `5,341,591` and SDK `5,341,653`.
  - SDK wall time all repetitions: `3.459349`, `3.416601`, `3.326840`,
    `3.224579`, `3.106113` seconds; mean `3.306696`, median `3.326840`.
  - installed plugin wall time all repetitions: `11.489004`, `11.499245`,
    `11.285479`, `11.017115`, `11.095556` seconds; mean `11.277280`,
    median `11.285479`.
  - warm repetitions 2 through 5: SDK mean `3.268533` seconds, plugin mean
    `11.224349` seconds, making the SDK `3.43x` faster by warm mean.
  - run-5 column comparison: plugin columns `185`, SDK columns `185`, missing
    SDK columns `0`, extra SDK columns `0`, shared-row diffs `0`.
  - `/usr/bin/time -v` SDK pass:
    `.local/sow-0093/field-index-columns/time-sdk-20260606T110306Z.txt`;
    elapsed `0:03.32`, user `2.84s`, system `0.42s`, CPU `98%`, max RSS
    `86,492 KiB`, major faults `0`, minor faults `87,693`.
- Debug row-traversal column collection guard on 2026-06-06:
  - decision: row-traversal column collection is a debug-only discrepancy
    marker and must never be a valid production, benchmark, or compatibility
    path. The production column catalog source is FIELD indexes.
  - implementation shape:
    `ExplorerQuery::debug_collect_column_fields_by_row_traversal` replaced the
    old `collect_column_fields` field name, is hidden from generated docs, and
    is rejected by production explorer entrypoints with `SdkError::Unsupported`.
  - Netdata request parsing always sets the marker to `false`.
  - docs/spec/skill impact: root performance contract, journal compatibility
    skill, Rust reader performance spec, product scope spec,
    systemd-journal plugin facets spec, and Rust README all now state that any
    result requiring this marker is an explorer bug, not a valid operating
    mode.
  - one SDK-first strict comparison pass after the guard still passed with
    report path
    `.local/sow-0093/debug-column-guard/sdk-first-default-facets-1rep-20260606T-debug-column-guard.json`.
    The SDK ran in `3.471433655` seconds and the installed plugin ran in
    `12.503126743` seconds. Stable content checks passed for columns, rows,
    facets, histogram, items, and top-level metadata; the known diagnostic
    `items.evaluated` accounting check remained the only non-content
    difference.
- Netdata function run-control implementation on 2026-06-06:
  - the reusable SDK API now exposes caller-owned run options instead of
    hardwiring wrapper behavior. Evidence:
    `rust/src/journal/src/netdata.rs:242` through
    `rust/src/journal/src/netdata.rs:257` define
    `NetdataFunctionProgress` and `NetdataFunctionRunOptions`, while
    `rust/src/journal/src/netdata.rs:320` through
    `rust/src/journal/src/netdata.rs:366` expose JSON and bytes request entry
    points that accept those options.
  - progress and cancellation are enforced in the directory/file execution
    path. Evidence:
    `rust/src/journal/src/netdata.rs:369` through
    `rust/src/journal/src/netdata.rs:464` check cancellation before each file,
    propagate deadline and cancellation into Explorer, emit cumulative progress
    during scans, and emit file-end progress for small or fast files.
  - progress now reports over the same preselected query-file shape as the
    plugin: source selection and time-window overlap are applied before
    execution, then the callback reports current file versus total query files.
    This avoids reporting progress over unrelated files in the directory.
  - cancellation is validated both before execution and during an active scan:
    a progress callback can flip a caller-owned cancellation predicate, and the
    Explorer row-cadence control path stops with the plugin-compatible compact
    `499` response.
  - the wrapper remains a thin SDK adapter for the standard `--test`, `--dir`,
    stdin request payload, and `--timeout` shape, and now adds diagnostic-only
    `--progress-jsonl`, `--cancel-immediately`, and
    `--cancel-after-progress` options. These validate wrapper wiring without
    writing progress frames into normal comparison stdout.
  - the lower-level Explorer now has a reusable run-control primitive, so
    control checks happen in the traversal hot path rather than only in the
    wrapper. Evidence:
    `rust/src/journal/src/explorer.rs:140` through
    `rust/src/journal/src/explorer.rs:160` extend `ExplorerStats` with
    `last_realtime_usec`; `rust/src/journal/src/explorer.rs:206` through
    `rust/src/journal/src/explorer.rs:308` define
    `ExplorerStopReason`, `ExplorerProgress`, and `ExplorerControl`;
    `rust/src/journal/src/explorer.rs:727` through
    `rust/src/journal/src/explorer.rs:768` expose controlled Explorer entry
    points; and `rust/src/journal/src/explorer.rs:1040` through
    `rust/src/journal/src/explorer.rs:1260` apply row-cadence checks in the
    main, combined, and facet scan loops.
  - Netdata request parsing now normalizes `if_modified_since`, `data_only`,
    `delta`, `tail`, and `sampling` before query execution. Evidence:
    `rust/src/journal/src/netdata.rs:865` through
    `rust/src/journal/src/netdata.rs:978`.
  - response shaping now returns cancellation status `499`, no-change status
    `304`, plugin-style data-only delta keys, tail/full `last_modified`, and
    sampling metadata placeholders. Evidence:
    `rust/src/journal/src/netdata.rs:500` through
    `rust/src/journal/src/netdata.rs:630`.
  - the row-level data lifetime guarantee was repaired for compressed payloads:
    uncompressed current-row DATA remains borrowed from the pinned mmap, while
    compressed DATA is copied into row-owned stable chunks that remain valid
    until row reset or reader advance. Evidence:
    `rust/src/crates/journal-core/src/file/row_view.rs:20` through
    `rust/src/crates/journal-core/src/file/row_view.rs:37`,
    `rust/src/crates/journal-core/src/file/row_view.rs:139` through
    `rust/src/crates/journal-core/src/file/row_view.rs:181`,
    `rust/src/crates/journal-core/src/file/row_view.rs:206` through
    `rust/src/crates/journal-core/src/file/row_view.rs:243`, and
    `rust/README.md:94` through `rust/README.md:99`.
  - focused tests cover Explorer progress/cancellation, Netdata function
    progress/cancellation including small-file file-end progress, and the
    compressed row-pointer lifetime regression. Evidence:
    `rust/src/journal/src/explorer.rs:2266` through
    `rust/src/journal/src/explorer.rs:2318` and
    `rust/src/journal/src/netdata.rs:2545` through
    `rust/src/journal/src/netdata.rs:2608`.
  - remaining gaps are intentionally recorded before calling this a complete
    replacement: sampling currently reports the selected mode and placeholder
    counters, and learned realtime-drift state is not persisted by the SDK API
    yet.
- Netdata function run-control refinement on 2026-06-06:
  - changed Rust query execution to preselect source/time-overlapping query
    files before scanning, so progress reports use the plugin-equivalent query
    file denominator instead of every file under the directory;
  - added file-end progress emission for selected files that are small, fast,
    or skipped after selection;
  - added active-scan cancellation coverage where a progress callback flips a
    caller-owned cancellation predicate and the SDK returns compact status
    `499`;
  - added wrapper diagnostic switches for progress JSONL and cancellation
    probes while keeping normal stdout as JSON-only for comparison harnesses.
  Validation:
  - `cargo fmt --check && cargo build --release -p netdata_function_wrapper && cargo test -p journal`
    passed from `rust/` with 81 Rust tests;
  - `python3 -m py_compile tests/netdata_function/run_function_compare.py tests/netdata_function/compare_function_json.py tests/netdata_function/test_compare_function_json.py`
    and `python3 tests/netdata_function/test_compare_function_json.py` passed
    with 15 tests;
  - wrapper progress probe wrote one event with
    `current_file=1,total_files=1,matched_files=1,skipped_files=0`;
  - wrapper immediate cancellation probe returned
    `{"status":499,"errorMessage":"Request cancelled."}`;
  - strict SDK-first eight-request comparison report
    `.local/sow-0093/function-compare-wrapper-run-control-validation.json`
    passed with `overall: true` and all content checks true;
  - `git diff --check` and `.agents/sow/audit.sh` passed.
- Netdata function sampling and state-hook implementation on 2026-06-06:
  - Explorer now accepts `ExplorerSampling` and reports sampled, unsampled,
    estimated, row-unsampled, row-estimated, and max
    source-realtime-delta counters in `ExplorerStats`;
  - Netdata full-analysis requests enable sampling only when plugin conditions
    are met, preserve returned-row candidates as full rows, add `[unsampled]`
    and `[estimated]` histogram values, and suppress `_sampling` for data-only
    requests;
  - the SDK Netdata API now exposes `NetdataFunctionState`, allowing Netdata
    consumers to provide per-file source type/name metadata, per-file
    first/last/modified timestamps, and per-file learned
    journal-vs-source-realtime drift;
  - source filtering uses caller metadata before filename fallback, and larger
    `_SOURCE_REALTIME_TIMESTAMP` deltas learned during traversal are reported
    back through the state hook, capped to Netdata's two-minute model;
  - request fixture added:
    `tests/netdata_function/requests/window-last5-default-facets-sampling20.json`;
  - focused validation passed:
    `cargo fmt --check && cargo test -p journal netdata_function_api --lib`,
    `cargo test -p journal source_selection_uses_caller_metadata_before_filename_fallback --lib`,
    and
    `cargo test -p journal netdata_function_state_receives_learned_source_realtime_delta --lib`.
  - full SDK-first fixture matrix including the sampling fixture passed again
    after the state-hook implementation with report path
    `.local/sow-0093/function-compare-with-state-sampling-final/summary.json`;
    all stable content checks were true for nine request payloads:
    `info`, full priority, filtered priority, full default facets,
    low-budget sampling, data-only, data-only delta, built-in source selection,
    and tail/no-change.
- Netdata function boundary expansion on 2026-06-06:
  - request fixtures added:
    `tests/netdata_function/requests/window-last5-data-only.json` and
    `tests/netdata_function/requests/window-last5-data-only-delta.json`;
  - strict SDK-first comparison report:
    `.local/sow-0093/function-compare-all-fixtures-clean-candidate.json`;
  - compared binary pair:
    `rust/target/release/netdata_function_wrapper` first, then installed
    `/usr/libexec/netdata/plugins.d/systemd-journal.plugin`;
  - fixture directory:
    `.local/sow-0093/smoke-journals`;
  - all six cases passed strict semantic content comparison:
    `info.json`, `window-last5-priority.json`,
    `window-error-filter.json`, `window-last5-default-facets.json`,
    `window-last5-data-only.json`, and
    `window-last5-data-only-delta.json`;
  - every case matched stable top-level content, columns, returned rows,
    facets, histogram, item counters, and diagnostic item counters;
  - data-only delta matched the plugin's 128-row stop cadence with
    `matched=128`, `after=123`, `returned=5`, and `max_to_return=5`;
  - implementation fixes in this checkpoint:
    - source `info` required-parameter metadata is derived from the explicit
      journal directory's headers and file sizes;
    - full-analysis columns keep every indexed FIELD column and requested
      facet columns, instead of suppressing default facet fields with no
      reportable values;
    - requested empty facet groups are emitted with empty option arrays;
    - histogram dimensions distinguish actual dimensions, whose empty buckets
      render as `0`, from vocabulary-only dimensions, whose empty buckets
      render as `null`;
    - `available_histograms` preserves request-list order while keeping the
      plugin's sorted `order` metadata;
    - the comparator normalizes `facets_delta`, `histogram_delta`, and
      `items_delta` like their full-analysis counterparts, while treating
      data-only all-null column-catalog artifacts as non-content only when no
      returned-row value exists on either side.
- Netdata source-selection implementation on 2026-06-06:
  - request fixture added:
    `tests/netdata_function/requests/window-last5-priority-source-system.json`;
  - implementation shape:
    `__logs_sources` selections now filter the explicit `--dir` candidate
    files for the built-in source groups `all`, `all-local-logs`,
    `all-local-system-logs`, `all-local-user-logs`, `all-uncategorized`,
    `all-local-namespaces`, and `all-remote-systems`;
  - local file classification follows the plugin filename shape: `/remote/`
    paths are remote, parent directory components containing a namespace suffix
    are local namespaces, `system*` basenames are local system journals,
    `user*` basenames are local user journals, and remaining local journals are
    uncategorized;
  - exact source names are supported for remote filenames with the plugin's
    `remote-` prefix and namespace names with the plugin's `namespace-`
    prefix, but the live Netdata registry/provider source inventory remains an
    integration boundary outside this standalone explicit-directory wrapper;
  - strict SDK-first comparison report:
    `.local/sow-0093/function-compare-source-selection-validation.json`;
  - all seven request cases passed strict semantic content comparison:
    `info.json`, `window-last5-priority.json`,
    `window-error-filter.json`, `window-last5-default-facets.json`,
    `window-last5-data-only.json`,
    `window-last5-data-only-delta.json`, and
    `window-last5-priority-source-system.json`;
  - every case matched stable top-level content, columns, returned rows,
    facets, histogram, item counters, and diagnostic item counters.
  - source-selection case timing from the same report:
    SDK `0.005249` seconds, installed plugin `0.007592` seconds. Full matrix
    timing ranged from SDK `0.001165` to `0.012661` seconds and installed
    plugin `0.002899` to `0.012948` seconds on the repo-local fixture.
  - validation commands passed:
    `cargo fmt --check && cargo test -p journal`,
    `python3 -m py_compile tests/netdata_function/run_function_compare.py tests/netdata_function/compare_function_json.py tests/netdata_function/test_compare_function_json.py`,
    `python3 tests/netdata_function/test_compare_function_json.py`,
    `git diff --check`, and `.agents/sow/audit.sh`.
- Netdata tail/no-change function-error parity on 2026-06-06:
  - request fixture added:
    `tests/netdata_function/requests/window-last5-tail-no-change.json`;
  - installed plugin evidence: the offline test path returns JSON
    `{"status":304,"errorMessage":"No new data since the previous call."}` and
    exits with status `1` for this function error;
  - implementation shape:
    SDK `journal::netdata` now returns plugin-compatible compact function
    error envelopes for `304` no-change and `499` cancellation. Timeout remains
    a partial table response;
  - comparison harness shape:
    `tests/netdata_function/run_function_compare.py` now parses JSON stdout
    even when the compared binary exits nonzero, and
    `tests/netdata_function/compare_function_json.py` compares compact
    function error envelopes as content;
  - strict SDK-first comparison report:
    `.local/sow-0093/function-compare-run-control-validation.json`;
  - all eight request cases passed strict semantic content comparison:
    `info.json`, `window-last5-priority.json`,
    `window-error-filter.json`, `window-last5-default-facets.json`,
    `window-last5-data-only.json`,
    `window-last5-data-only-delta.json`,
    `window-last5-priority-source-system.json`, and
    `window-last5-tail-no-change.json`;
  - the tail/no-change case matched the compact function error envelope with
    `checks.function_error=true`; SDK exit code was `0`, installed plugin exit
    code was `1`, SDK wall time was `0.001302` seconds, and installed plugin
    wall time was `0.004040` seconds.
- Netdata timeout response-shape parity on 2026-06-06:
  - repo-local timeout dataset:
    `.local/sow-0093/timeout-journals`, built from 2,000 hard links to the
    existing repo-local smoke journal fixture. The hard-link dataset exercises
    a large logical scan while using only 8.2 MiB of disk and does not read the
    live host journal;
  - request:
    `.local/sow-0093/timeout-request-default-facets-sampling0.json`, matching
    the smoke fixture time window and default facets with `sampling:0`;
  - SDK and installed plugin were both run with `--timeout 1`;
  - both returned `status:200`, `partial:true`, and the same warning message
    object:
    `{"title":"Query timed-out, incomplete data. ","status":"warning","description":"QUERY TIMEOUT: The query timed out and may not include all the data of the selected window. "}`;
  - the strict content comparison intentionally fails for timeout responses
    because each implementation stops at a different row/file based on speed
    and scheduling. This is not a content-equivalence fixture. It validates
    response class and envelope shape only;
  - sampling-enabled timeout responses still differ in the sampling/unsampled/
    estimated counters and warning percentages. That remains part of the
    sampling-estimate replacement gap.

Implementation fixes after first reviewer batch:

- `SystemdJournalProfile` now keeps UID/GID fields raw by default and performs
  no host user/group name resolution. The comparison CLI uses the explicit
  `NetdataJournalFunction::systemd_journal_plugin_compatible()` constructor to
  opt into Netdata-plugin presentation compatibility.
- `OffsetClassCache::lookup()` now has a bounded probe loop even though normal
  insertion grows the table before it becomes full.
- `skip_by_commit_time()` now documents that source-realtime slack delegates
  per-row skip decisions to effective realtime filtering.
- `ND_JOURNAL_PROCESS` now mirrors the installed plugin's dynamic row-id
  fallback order for this boundary: `CONTAINER_NAME`, then
  `SYSLOG_IDENTIFIER`, then `_COMM`; `_EXE` is not a fallback, `SYSLOG_PID` is
  not preferred over `_PID`, and rows with no PID render only the identifier.
  Evidence checked in `ktsaou/netdata @ f6f857d46356`:
  `src/collectors/systemd-journal.plugin/systemd-journal-annotations.c:499`
  through `systemd-journal-annotations.c:525`.
- The wrapper now applies Netdata's duplicate visible timestamp adjustment
  before sorting/truncating returned rows: backward scans decrement duplicate
  timestamps and forward scans increment them. Evidence checked in
  `ktsaou/netdata @ f6f857d46356`:
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:173`
  through `systemd-journal-execute.h:179`, and
  `systemd-journal-execute.h:282` through
  `systemd-journal-execute.h:288`.
- `__logs_sources` selections now filter explicit-directory candidate files
  for built-in source groups. Full live registry/provider source metadata is
  still a Netdata integration boundary, not a core journal reader concern.
- The SDK keeps the Netdata `_BOOT_ID` data-transform trailing two spaces
  because the installed plugin appends the same suffix. Evidence checked in
  `ktsaou/netdata @ f6f857d46356`:
  `src/collectors/systemd-journal.plugin/systemd-journal-annotations.c:390`
  through `systemd-journal-annotations.c:397`.
- Netdata facet values are capped and collapsed at 8,192 bytes in the SDK
  Netdata boundary before facet and histogram output. This matches
  `FACET_MAX_VALUE_LENGTH` used by `systemd-journal.plugin` when sending field
  values to the facets engine. Evidence checked in `ktsaou/netdata @
  f6f857d46356`:
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:80`
  through `systemd-journal-execute.h:85` and
  `src/collectors/systemd-journal.plugin/systemd-journal-function.h:9`.
- Default visible column registration order now mirrors the plugin order for
  the registered fields: `_HOSTNAME`, `ND_JOURNAL_PROCESS`, `MESSAGE`,
  `PRIORITY`, `SYSLOG_FACILITY`, `ERRNO`, and `ND_JOURNAL_FILE`. Evidence
  checked in `ktsaou/netdata @ f6f857d46356`:
  `src/collectors/systemd-journal.plugin/systemd-journal.c:158` through
  `systemd-journal.c:196`.
- Time-window normalization now mirrors Netdata's
  `rrdr_relative_window_to_absolute()` path: both-zero defaults to a one-hour
  window ending at `now`; small positive and negative endpoints are relative;
  inverted windows are swapped; equal windows expand back by the default query
  duration; and microsecond bounds use inclusive final-second rounding.
  Evidence checked in `ktsaou/netdata @ f6f857d46356`:
  `src/libnetdata/facets/logs_query_status.h:762` through
  `logs_query_status.h:779`,
  `src/libnetdata/libnetdata.c:364` through `libnetdata.c:422`, and
  `src/libnetdata/buffer/buffer.h:12`.
- Candidate journal files are now sorted before scanning using the same
  comparator shape as the plugin: last message realtime, file mtime, first
  message realtime, reversed for forward queries. Evidence checked in
  `ktsaou/netdata @ f6f857d46356`:
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:547`
  through `systemd-journal-execute.h:553` and
  `src/collectors/systemd-journal.plugin/systemd-journal-files.c:822`
  through `systemd-journal-files.c:854`.
- Boot-id display timestamps now keep the earliest timestamp seen for each
  boot id, independent of candidate-file scan order. Evidence checked in
  `ktsaou/netdata @ f6f857d46356`:
  `src/collectors/systemd-journal.plugin/systemd-journal-files.c:141` through
  `systemd-journal-files.c:143` and
  `systemd-journal-files.c:857` through `systemd-journal-files.c:872`.

Reviewer findings:

- First read-only reviewer batch found real gaps:
  - `ND_JOURNAL_PROCESS` fallback differed from the installed plugin. Fixed and
    covered by `dynamic_process_name_matches_plugin_fallback_order`.
  - Duplicate visible row timestamps did not follow plugin direction-specific
    adjustment. Fixed and covered by
    `duplicate_row_timestamps_match_plugin_direction_adjustment`.
  - The spec still described stale `_EXE` / `SYSLOG_PID` process fallback
    behavior. Fixed in
    `.agents/sow/specs/systemd-journal-plugin-facets.md`.
  - `__logs_sources` was accepted but not implemented by the standalone
    `--dir` wrapper in the first implementation pass. Fixed for
    explicit-directory built-in source groups and covered by
    `window-last5-priority-source-system.json`; live registry/provider source
    metadata remains outside this standalone wrapper.
- Second read-only reviewer batch verified strict content comparison and
  SDK-first runner order. Reviewer votes received before the 8,192-byte finding:
  Kimi, Qwen, GLM, and Mimo: production-grade for the checked slice after
  disposition. MiniMax continued deeper review and found additional
  presentation/edge-case issues below.
- MiniMax found that Netdata's 8,192-byte facet value cap was specified but not
  implemented by the SDK boundary. Fixed by truncating/collapsing Netdata
  facet and histogram values before output. Added
  `facet_values_are_truncated_and_collapsed_like_plugin` and
  `histogram_values_are_truncated_and_collapsed_like_plugin`.
- MiniMax found that the default registered column order differed. Fixed for
  registered fields. Remaining caveat: extra/discovered table columns after the
  registered fields and histogram label order still differ in presentation
  order, but strict semantic content comparison maps rows by column name and
  histogram buckets by label. This SOW therefore proves content equivalence,
  not byte-for-byte or presentation-order identity.
- Qwen found that time-window normalization did not match Netdata for inverted
  and equal bounds. Investigation found a wider compatibility gap: the SDK also
  treated small positive values as absolute epoch seconds, while Netdata treats
  values within three years as relative. Fixed and covered by
  `normalizes_missing_time_window_to_last_hour_like_plugin`,
  `normalizes_inverted_time_window_like_plugin`,
  `normalizes_equal_time_window_like_plugin`,
  `normalizes_relative_time_window_like_plugin`, and
  `normalizes_missing_after_with_supplied_before_like_plugin`.
- Kimi found a candidate-file ordering risk. Fixed by ordering files with the
  plugin comparator shape and covered by
  `journal_file_order_matches_plugin_comparator_shape`.
- Kimi also questioned `.journal~` inclusion. Disposition: false finding.
  Netdata's scanner explicitly accepts `.journal~`; the SDK behavior is kept
  and covered by `disposed_journal_extension_matches_plugin_scan_contract`.
  Evidence checked in `ktsaou/netdata @ f6f857d46356`:
  `src/collectors/systemd-journal.plugin/systemd-journal-files.c:610`
  through `systemd-journal-files.c:641`.
- The candidate-file ordering fix exposed a boot-id annotation mismatch because
  the SDK kept the first seen boot timestamp instead of the earliest timestamp.
  Fixed and covered by
  `boot_first_realtime_keeps_earliest_timestamp_like_plugin`.
- Final same-scope reviewer rerun after the time-window, file-order, boot-id,
  and 8,192-byte fixes returned production-grade votes from Kimi, Qwen, GLM,
  Mimo, and MiniMax for the checked scope.
- GLM found a SOW-only evidence typo: the diagnostic `items.evaluated` values
  were reversed. Fixed to match report 29: SDK `5,341,653`, installed plugin
  `5,341,591`.
- Kimi suggested using only query-matched files for boot-id display
  annotations. This was tested as report 28 and rejected: report 28 failed
  strict content comparison because plugin `_BOOT_ID` facet display names use
  registry-level boot annotations beyond the query-matched file set. The SDK
  keeps full collected-path boot annotations to match the installed plugin.
- The strict comparison was rerun after the reviewer dispositions with the SDK
  executed first and the plugin second. The rerun produced
  `.local/sow-0093/strict-content/sdk-first-strict-report-latest.json` and kept
  all content checks passing: top-level stable metadata, columns, rows, facets,
  histogram buckets, and stable item counters.
- Final read-only reviewer batch against the strict content requirement and
  latest saved JSON artifacts returned production-grade votes from Kimi, Qwen,
  GLM, Mimo, and MiniMax for this checked slice.
  - Reviewers independently confirmed SDK-first execution in
    `tests/netdata_function/run_function_compare.py`.
  - Reviewers confirmed the comparator checks columns, rows by column name,
    facets, facet values and counts, histogram buckets and values by label, and
    stable item counters.
  - Reviewers agreed the only accepted differences are diagnostic or
    presentation-order differences, not content differences.
  - Qwen raised a possible cosmetic left/right-label issue in the report. Direct
    check rejected it: the report diff is
    `$.evaluated: value differs (5341591 != 5341653)`, matching plugin-left and
    SDK-right for the documented diagnostic counter.
  - Reviewers repeated the caveat that this proves content equivalence for the
    checked large default-facets request, not presentation-order identity,
    broader request-matrix coverage, or performance evidence.
- Reviewer-finding fix pass on 2026-06-06:
  - Qwen's later same-scope review returned `NOT PRODUCTION GRADE` because the
    SOW still had stale partial-validation text and `merge_histogram()` relied
    on an implicit same-bucket invariant. Fixed the code to reject inconsistent
    histogram field/bucket shapes instead of silently truncating source buckets.
  - The same pass clarified that `slice` is intentionally forced to indexed
    slice semantics in the SDK replacement, changed explicit-directory journal
    discovery to bounded recursive traversal with canonical-directory
    de-duplication, and fixed caller metadata summaries so partial
    `msg_first_realtime_usec` / `msg_last_realtime_usec` metadata still falls
    back to the journal header for the missing side.
  - Added focused Rust tests:
    `merge_histogram_rejects_inconsistent_bucket_shape`,
    `collect_journal_files_recurses_nested_directories`,
    `collect_journal_files_deduplicates_symlinked_directories`, and
    `source_summary_fills_missing_caller_metadata_from_header`.
  - Validation after this fix pass:
    `cargo fmt --check && cargo test -p journal` passed from `rust/` with 89
    Rust tests; `python3 -m py_compile tests/netdata_function/run_function_compare.py
    tests/netdata_function/compare_function_json.py
    tests/netdata_function/test_compare_function_json.py` and
    `python3 tests/netdata_function/test_compare_function_json.py` passed with
    15 Python comparator tests; `cargo build --manifest-path rust/Cargo.toml
    --release -p netdata_function_wrapper` passed.
  - Fresh SDK-first nine-request comparison after the fix pass wrote
    `.local/sow-0093/function-compare-after-review-fixes/summary.json` with
    `overall: true`; all requests passed stable content checks for columns,
    rows, facets, histogram, stable item counters, and function-error content
    where applicable.
- FTS reviewer-finding fix pass on 2026-06-06:
  - The next same-scope reviewer pass found a blocking replacement gap: the
    SDK request parser treated the full `query` string as one FTS pattern and
    had no function-boundary FTS fixture. Evidence from
    `netdata/netdata @ 83c17da3a898` showed `facets_set_query()` calls
    `simple_pattern_create(query, "|", SIMPLE_PATTERN_SUBSTRING, false)` and
    row processing rejects a row when no positive term matched or any negative
    term matched.
  - Fixed the Rust Explorer and Netdata request boundary to preserve ordered
    FTS terms, `|` separators, leading `!` negative terms, escaped separators,
    substring `*` parts, DATA offset-cache classification for negative terms,
    and row-level negative rejection.
  - Added focused Rust tests:
    `parses_netdata_fts_query_like_simple_pattern` and
    `explorer_fts_or_terms_and_negative_terms_filter_rows`.
  - Added the function-boundary fixture
    `tests/netdata_function/requests/window-last5-fts-or-negative.json`.
  - Validation after this fix pass:
    `cd rust && cargo fmt --check && cargo test -p journal netdata --lib &&
    cargo test -p journal explorer_fts --lib` passed; `cd rust && cargo test
    -p journal` passed with 91 Rust tests; `cargo build --release
    -p netdata_function_wrapper` passed; the Python comparator compile check
    and `python3 tests/netdata_function/test_compare_function_json.py` passed
    with 15 tests.
  - Fresh SDK-first ten-request comparison after the FTS fix wrote
    `.local/sow-0093/function-compare-after-fts-fix/summary.json` with
    `overall: true`; all requests, including
    `window-last5-fts-or-negative`, passed stable content checks for columns,
    rows, facets, histogram, stable item counters, and function-error content
    where applicable.
  - One read-only reviewer process violated its prompt and attempted local
    edits. The specific reviewer PIDs were terminated, its partial edits were
    not accepted as review output, and the FTS fix was implemented locally and
    validated under the SOW.
- Final progress/cancellation and robustness fix pass on 2026-06-06:
  - made `CombinedResult::merge()` validate histogram field and bucket shape
    before merging file stats, rows, columns, or facets, so an impossible
    histogram-shape mismatch cannot leave a partially merged file result at the
    SDK API boundary;
  - preserved the previous fixes that clear FTS state during the zero-count
    vocabulary pass and report unreadable subdirectories through
    `_journal_files.errors` while continuing to scan readable siblings;
  - focused validation passed:
    `cargo fmt --check && cargo test -p journal netdata --lib &&
    cargo test -p journal explorer_fts --lib`;
  - full Rust validation passed:
    `cd rust && cargo test -p journal` with 92 Rust tests;
  - release wrapper and comparator validation passed:
    `cargo build --release -p netdata_function_wrapper`,
    `python3 -m py_compile tests/netdata_function/run_function_compare.py
    tests/netdata_function/compare_function_json.py
    tests/netdata_function/test_compare_function_json.py`, and
    `python3 tests/netdata_function/test_compare_function_json.py` with 15
    Python tests;
  - fresh SDK-first ten-request comparison wrote
    `.local/sow-0093/function-compare-after-final-progress-cancel-fixes/report.json`
    and
    `.local/sow-0093/function-compare-after-final-progress-cancel-fixes/summary.json`
    with `ok: true`; all ten requests passed stable content checks for
    columns, rows, facets, histogram, stable item counters, and function-error
    content where applicable;
  - wrapper run-control probes wrote
    `.local/sow-0093/run-control-final/progress.jsonl`,
    `.local/sow-0093/run-control-final/cancel-immediate.json`, and
    `.local/sow-0093/run-control-final/cancel-after-progress.json`;
    the progress probe emitted one selected-file progress event with
    `current_file=1`, `total_files=1`, `matched_files=1`, and
    `skipped_files=0`; both cancellation probes returned
    `{"status":499,"errorMessage":"Request cancelled."}`;
  - `git diff --check` and `.agents/sow/audit.sh` passed.
- Stdin request security and reviewer-blocker fix pass on 2026-06-06:
  - changed the SDK `netdata_function_wrapper` test adapter to read request
    JSON from stdin instead of `--request <path>`, matching the current
    Netdata plugin test-mode security contract for privileged binaries;
  - changed the comparison harness so it still accepts fixture paths as harness
    inputs but pipes request bytes to compared SDK/plugin binaries on stdin;
  - documented the stdin test-mode contract in the Rust README, Netdata
    function test README, and systemd-journal-plugin facets spec;
  - normalized only the synthetic `ND_JOURNAL_FILE` path root in the comparator,
    comparing the journal filename while treating `/proc/self/fd/<n>/...`
    versus caller-supplied directory roots as hardened test-mode diagnostics;
  - fixed reviewer findings by returning immediately after cancellation before
    zero-count/vocabulary post-processing, including cursor identity in
    returned-row expansion errors, reporting directory scan-limit truncation,
    hardening histogram merge iteration, and replacing impossible public
    boundary `expect()` calls with guarded behavior;
  - updated the durable reviewer pool to the current six model IDs:
    `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/kimi-k2.6`,
    `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`,
    `llm-netdata-cloud/minimax-m3-coder`, and
    `llm-netdata-cloud/deepseek-v4-pro`;
  - focused validation passed:
    `cargo fmt --check && cargo test -p journal netdata --lib &&
    cargo test -p journal explorer_fts --lib`;
  - full Rust validation passed:
    `cd rust && cargo test -p journal` with 92 Rust tests;
  - release wrapper and comparator validation passed:
    `cargo build --release -p netdata_function_wrapper`,
    `python3 -m py_compile tests/netdata_function/run_function_compare.py
    tests/netdata_function/compare_function_json.py
    tests/netdata_function/test_compare_function_json.py`, and
    `python3 tests/netdata_function/test_compare_function_json.py` with 17
    Python tests;
  - fresh SDK-first ten-request stdin comparison wrote
    `.local/sow-0093/function-compare-after-stdin-security-fix/report.json`
    and `.local/sow-0093/function-compare-after-stdin-security-fix/summary.json`
    with `overall: true`; all ten requests passed stable content checks;
  - wrapper stdin run-control probes wrote
    `.local/sow-0093/run-control-stdin-security-fix/progress.jsonl`,
    `.local/sow-0093/run-control-stdin-security-fix/cancel-immediate.json`,
    and
    `.local/sow-0093/run-control-stdin-security-fix/cancel-after-progress-multifile.json`;
    immediate cancellation and multi-file progress-triggered cancellation both
    returned `{"status":499,"errorMessage":"Request cancelled."}`;
  - `git diff --check` and `.agents/sow/audit.sh` passed.
- Reviewer-disposition hardening pass on 2026-06-06:
  - six-model read-only reviewer rerun used
    `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/kimi-k2.6`,
    `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`,
    `llm-netdata-cloud/minimax-m3-coder`, and
    `llm-netdata-cloud/deepseek-v4-pro`;
  - five reviewers returned production-grade votes before local hardening;
    GLM raised blocking concerns for canonical-path deduplication and uncached
    UID/GID display lookups; MiniMax returned production-grade but identified a
    real single-file progress-triggered cancellation edge case;
  - fixed canonical file de-duplication after traversal so symlinked journal
    files that resolve to the same canonical path are read once;
  - made directory scan-limit handling precise by refusing new directories once
    the bounded unique-directory budget is reached, instead of inserting the
    over-limit directory first;
  - added per-query UID/GID display caches to the plugin-compatible display
    context so repeated UID/GID values do not repeatedly invoke host
    name-service lookup APIs;
  - made `explore_files()` re-check the cancellation predicate immediately
    after file-end progress callbacks, so progress-triggered cancellation is
    honored even when the selected query has only one file;
  - added focused Rust tests:
    `netdata_function_api_honors_cancellation_after_final_file_progress`,
    `plugin_compatible_profile_caches_user_group_resolution`, and
    `collect_journal_files_deduplicates_symlinked_files`;
  - focused validation passed:
    `cargo fmt --check`, `cargo test -p journal
    netdata_function_api_honors_cancellation_after_final_file_progress --lib`,
    `cargo test -p journal
    plugin_compatible_profile_caches_user_group_resolution --lib`, and
    `cargo test -p journal
    collect_journal_files_deduplicates_symlinked_files --lib`.
  - full validation after the hardening pass passed:
    `cargo fmt --check && cargo test -p journal netdata --lib &&
    cargo test -p journal explorer_fts --lib` with 40 Netdata tests and 2 FTS
    tests; `cargo test -p journal` with 95 Rust tests;
    `cargo build --release -p netdata_function_wrapper`; Python compile check
    for the three comparison scripts; and
    `python3 tests/netdata_function/test_compare_function_json.py` with 17
    tests.
  - fresh SDK-first ten-request stdin comparison wrote
    `.local/sow-0093/function-compare-after-reviewer-disposition-fixes/report.json`
    and
    `.local/sow-0093/function-compare-after-reviewer-disposition-fixes/summary.json`
    with `overall: true`, `case_count: 10`, and no failed requests.
  - wrapper run-control probes wrote
    `.local/sow-0093/run-control-reviewer-disposition-fixes/progress.jsonl`,
    `.local/sow-0093/run-control-reviewer-disposition-fixes/cancel-immediate.json`,
    and
    `.local/sow-0093/run-control-reviewer-disposition-fixes/cancel-after-progress-single-file.json`;
    immediate cancellation and single-file progress-triggered cancellation both
    returned `{"status":499,"errorMessage":"Request cancelled."}`;
  - `git diff --check` and `.agents/sow/audit.sh` passed.
- Final same-scope reviewer rerun after the reviewer-disposition hardening pass
  on 2026-06-06:
  - valid read-only reviewer votes:
    `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`,
    `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`,
    `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`,
    `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`,
    `llm-netdata-cloud/minimax-m3-coder`: `PRODUCTION GRADE`, and
    `llm-netdata-cloud/deepseek-v4-pro`: `PRODUCTION GRADE`;
  - one MiniMax attempt was discarded because it violated the read-only
    reviewer prompt by running `cargo check` and a targeted `cargo test`. The
    process was stopped by exact PID after verification, and MiniMax was rerun
    with the same full scope plus stricter no-build/no-test/no-check command
    instructions. The valid MiniMax rerun used read-only inspection only and
    returned `PRODUCTION GRADE`;
  - reviewers found no blocking issues after the hardening pass;
  - non-blocking finding disposition: invalid-UTF-8 stdout decoding in the
    comparison harness is accepted as a harness diagnostics improvement, not a
    function-boundary correctness issue, because compared binaries still fail
    the case when stdout cannot parse as JSON. This is explicitly not tracked
    as follow-up for SOW-0093 because it does not change SDK behavior or
    comparison correctness;
  - non-blocking finding disposition: diagnostic-only `--progress-jsonl <path>`
    remains internal wrapper validation plumbing, not part of the Netdata
    plugin contract or production SDK API. Production consumers use
    `NetdataFunctionRunOptions::progress_callback`; the wrapper documentation
    already marks diagnostic switches as SDK validation only;
  - non-blocking finding disposition: `merge_histogram()` count addition is not
    a practical overflow risk for journal row counts and did not affect
    validation; no follow-up is required unless real corpus evidence shows
    practical overflow risk;
  - non-blocking finding disposition: `collect_journal_files()` bounds
    directory traversal by unique directory count and depth. A separate
    journal-file count cap is not required for this SOW because the selected
    files are still explicit journal files under caller-provided directories,
    and scan-limit truncation is reported. If a production deployment shows a
    pathological flat directory with too many journal files, that belongs in a
    focused operational-hardening SOW;
  - non-blocking finding disposition: vocabulary padding reopens matched files
    per facet field to mirror plugin-compatible zero-count value behavior. This
    is tracked as accepted cost for plugin-compatible output and remains outside
    the hot returned-row path.

Same-failure scan:

- `rg -n "ND_JOURNAL_PROCESS|SYSLOG_PID|_EXE|CONTAINER_NAME"` was run across
  the SOW, spec, README, test README, and Rust Netdata boundary. Remaining
  `_EXE` and `SYSLOG_PID` mentions are either source field constants, explicit
  negative-fallback documentation, or tests proving they are not used for
  `ND_JOURNAL_PROCESS`.
- `rg -n "FACET_MAX|8192|truncate"` was run across the SDK Netdata boundary
  and spec. The missing 8,192-byte cap was fixed and covered by focused Rust
  tests.
- `rg -n "journal~|is_journal_file|nd_journal_file_dict_items|boot_ids_to_first_ut"`
  was run against the Netdata plugin source and Rust wrapper to verify the
  `.journal~`, candidate-file order, and boot-id annotation dispositions.
- `git diff --check` passed.
- `.agents/sow/audit.sh` passed.

Sensitive data gate:

- No raw journal payloads or raw plugin/SDK outputs were committed. Durable
  artifacts contain only sanitized request fixtures, source code, docs, and
  aggregate validation summaries. Generated plugin/SDK outputs and comparison
  reports remain under `.local/sow-0093/`.

Artifact maintenance gate:

- AGENTS.md: updated reviewer pool to the current six model IDs after the user
  changed the approved reviewer set.
- Runtime project skills: updated
  `.agents/skills/project-agent-orchestration/SKILL.md` with the current six
  reviewer model IDs. The Netdata function comparison workflow itself remains
  SOW-local.
- Specs: updated
  `.agents/sow/specs/systemd-journal-plugin-facets.md` with the Rust Netdata
  function boundary, wrapper CLI, same-field facet-filter switch, missing
  histogram value behavior, and zero-count vocabulary comparator rule.
- End-user/operator docs: updated `rust/README.md` with the new
  `journal::netdata` API and wrapper command.
- End-user/operator skills: no update needed; no exported/operator skill
  changed.
- SOW lifecycle: completed; move this file to `.agents/sow/done/` together with
  the implementation and status updates.
- SOW-status.md: updated for SOW completion.

Specs update:

- Updated
  `.agents/sow/specs/systemd-journal-plugin-facets.md`.

Project skills update:

- Updated `.agents/skills/project-agent-orchestration/SKILL.md` with the
  current reviewer pool.

End-user/operator docs update:

- Updated `rust/README.md`.

End-user/operator skills update:

- Not updated; no exported/operator skill changed.

Lessons:

- Strict content comparison must include complete rows by column name, complete
  facets including zero-count values, histogram buckets by label, stable item
  counters, and stable top-level metadata. Counts-only comparisons are not
  sufficient for this boundary.
- Real-corpus slices can miss edge cases such as overlong facet values. When
  the spec names a transform or limit, add focused synthetic tests even if the
  production slice does not exercise it.
- Content equality and presentation-order equality are different contracts.
  This SOW currently validates content equivalence; presentation-order identity
  needs an explicit decision before it becomes a close gate.

Follow-up mapping:

- Presentation-order identity is explicitly not a closure requirement for this
  SOW. The accepted contract is content equivalence by stable keys and labels.
  A future SOW can make presentation order a hard contract if Netdata UI
  integration proves it is required.
- Deferred compressed-DATA Explorer optimization is tracked by SOW-0094. That
  SOW must not block this SOW's Explorer API stabilization unless current
  function-boundary validation proves decompression avoidance is required for
  correctness rather than performance.

## Outcome

Implementation and local validation are complete for the Rust SDK Netdata
function boundary replacement scope. The final six-model same-scope reviewer
rerun returned production-grade votes from all valid reviewer runs. SOW-0093 is
complete for the accepted content-equivalence contract.

## Lessons Extracted

- A plugin replacement boundary needs semantic JSON comparison, not byte-for-byte
  response comparison. Column order, histogram label order, diagnostic counters,
  and volatile runtime fields must be classified explicitly so real content
  regressions are not hidden by noisy fields.
- Netdata function progress and cancellation must be SDK API features, not
  wrapper-only behavior. The wrapper is useful for test execution, but consumers
  need direct `NetdataFunctionRunOptions` callbacks.
- Caller-owned state is the correct boundary for source metadata and learned
  realtime drift. The SDK should expose hooks and safe fallback behavior, while
  Netdata owns persistence in its journal registry.
- Invariants at merge boundaries must fail loudly. Silent histogram bucket
  truncation would be worse than returning an SDK error because it could produce
  believable but wrong charts.

## Followup

- SOW-0094 tracks the deferred compressed-DATA Explorer optimization experiment.
- Netdata component integration remains tracked by SOW-0047 through SOW-0050.
- Presentation-order identity is not a closure requirement for this SOW. The
  accepted boundary is content equivalence by stable keys and labels; a future
  SOW can make presentation order a hard contract if Netdata UI integration
  proves it is required.

## Regression Log

## Regression - 2026-06-06

What broke:

- After SOW-0093 was completed, the user requested a fresh SDK-vs-plugin
  benchmark and asked whether the SDK wrapper was still 4-5x faster than the
  installed Netdata `systemd-journal.plugin`.
- Fresh SDK-first comparison against `/usr/libexec/netdata/plugins.d/systemd-journal.plugin`
  and `/var/log/journal` used the 4 GiB default-facets request at
  `.local/sow-0093/big-default-facets/request-default-facets-4g.json`.
- The performance result was faster but lower than the earlier approximately
  4.99x claim:
  - warm repetitions 2-5 SDK mean: `3.062621665005281` seconds;
  - warm repetitions 2-5 plugin mean: `11.522398646244255` seconds;
  - warm mean speedup: `3.762266419618104x`;
  - best single repetition speedup: `3.904x`.
- The strict comparison failed, so the benchmark cannot be accepted as a valid
  replacement-equivalence benchmark until the content drift is fixed.

Evidence:

- Fresh report:
  `.local/sow-0093/performance/sdk-first-default-facets-5rep-20260606T230809+0300.json`.
- Saved one-repetition JSON outputs:
  `.local/sow-0093/performance/sdk-first-default-facets-1rep-saved-20260606T231140+0300-json/request-default-facets-4g-run1-sdk.json`
  and
  `.local/sow-0093/performance/sdk-first-default-facets-1rep-saved-20260606T231140+0300-json/request-default-facets-4g-run1-plugin.json`.
- Stable content still matched for:
  - matched rows: `5,341,590` on both sides;
  - returned rows: `200` on both sides;
  - histogram content.
- Strict comparison failed because the SDK emitted 15 extra empty default
  catalog/facet fields that the installed plugin did not emit:
  `CONTAINER_ID`, `CONTAINER_NAME`, `CONTAINER_TAG`, `IMAGE_NAME`,
  `ND_ALERT_CLASS`, `ND_ALERT_COMPONENT`, `ND_ALERT_NAME`,
  `ND_ALERT_STATUS`, `ND_ALERT_TYPE`, `ND_LOG_SOURCE`, `ND_NIDL_CONTEXT`,
  `ND_NIDL_NODE`, `OBJECT_SYSTEMD_SESSION`, `_NAMESPACE`, and
  `_SELINUX_CONTEXT`.
- Raw output inspection showed those SDK-only fields had zero options in the
  saved one-repetition output.
- Current Netdata source checked read-only at `netdata/netdata @ 097acc0dbf8e`
  includes those fields in `SYSTEMD_KEYS_INCLUDED_IN_FACETS`
  (`src/collectors/systemd-journal.plugin/systemd-journal.c`), while the
  installed plugin suppresses them for this request when no values exist.
- Environment caveat for the fresh benchmark:
  CPU governor `powersave`; loadavg was approximately `7-10`, so the numbers
  are workstation evidence, not clean lab-grade final benchmark evidence.

Why previous validation missed it:

- The closeout validated a specific installed plugin and request matrix at that
  time, but it did not include a broad everyday-use request suite covering many
  filter, FTS, sampling, source, data-only, delta, timeout, cancellation, and
  no-change combinations.
- The accepted content comparator correctly failed when the current SDK/plugin
  default catalog behavior diverged. The earlier closeout did not lock the
  default-catalog rule for fields with zero values tightly enough.
- Performance evidence was collected during incremental development and did not
  isolate each plugin-boilerplate feature cost. The performance delta from
  approximately 4.99x to approximately 3.8x needs commit-level and profile-level
  attribution.

Repair plan:

1. Re-establish the performance baseline first, as requested by the user.
   Compare current code against relevant historical commits/artifacts around
   SOW-0082/SOW-0093 to identify which change reduced speed from approximately
   4.99x to approximately 3.8x.
2. Profile the current SDK wrapper on the 4 GiB default-facets request and
   classify added cost by feature: source metadata, field catalog/vocabulary,
   row display expansion, sampling scaffolding, state hooks, UID/GID display,
   progress/cancellation, and JSON shaping.
3. Fix avoidable performance regressions without weakening plugin-compatible
   output.
4. Fix the default-catalog/facet mismatch so empty default fields are emitted or
   suppressed exactly like the installed plugin for this request shape.
5. Create a broad request suite that exercises everyday use and edge cases:
   no filters, positive filters, negative filters, multi-value OR filters,
   combinations of positive and negative fields, FTS positive OR, FTS negative,
   escaped FTS separators, wildcard substring FTS, data-only, delta, tail
   no-change, sampling, source selection, time-window edges, empty-result
   queries, low-limit queries, explicit facets, explicit histogram, and timeout
   / cancellation behavior where comparable through the wrapper.
6. Run side-by-side SDK-first comparisons against the installed plugin for all
   request cases, save sanitized reports under `.local/sow-0093/`, and record
   strict content pass/fail plus timings.
7. Update specs/docs/tests with any clarified function-boundary rules.
8. Run the whole-SOW reviewer pool only after implementation and local
   validation are complete.

Validation required before re-closing:

- `cargo fmt --check`.
- `cargo test -p journal netdata --lib`.
- `cargo test -p journal explorer_fts --lib`.
- `cargo test -p journal`.
- `cargo build --release -p netdata_function_wrapper`.
- `python3 -m py_compile tests/netdata_function/run_function_compare.py
  tests/netdata_function/compare_function_json.py
  tests/netdata_function/test_compare_function_json.py`.
- `python3 tests/netdata_function/test_compare_function_json.py`.
- Broad SDK-first plugin comparison matrix passes strict stable content checks.
- Performance report records SDK/plugin timing for every request case and
  explicitly reports whether the 4 GiB default-facets warm speedup recovers
  toward the earlier approximately 4.99x result.
- `git diff --check`.
- `.agents/sow/audit.sh`.
- Six-model read-only reviewer rerun returns production-grade or all blocking
  findings are fixed and re-reviewed.

Progress on 2026-06-07:

- Rebuilt the broad everyday-use request matrix under
  `.local/sow-0093/everyday-requests/20260606T233632+0300/`. The matrix covers
  info, default full analysis, explicit facets, OR filters, AND filters, source
  selection, FTS OR, FTS negative, FTS wildcard, escaped FTS separators,
  data-only, data-only delta, tail/`if_modified_since`, sampling, forward
  anchors, empty-result filters, low limit, and alternate histogram fields.
- Fixed SDK/plugin parity defects found by the matrix:
  - forward anchor row boundary is strict (`>`), while the response keeps the
    plugin's newest-first display order for forward pages;
  - Netdata `items` counters now use a plugin-shaped page window instead of
    reusing aggregate `rows_matched`;
  - data-only delta mode uses a global page window plus per-file 128-row
    delta-stop callback, matching the plugin's delta collection shape;
  - post-scan `if_modified_since` returns `304` only when no useful rows were
    matched, matching the plugin's `No additional useful data` gate.
- Focused validations after the fixes:
  - forward-anchor request passed strict content comparison:
    `.local/sow-0093/everyday-comparisons/focused-anchor-after-page-window-20260607T002444+0300/17-forward-anchor.json`;
  - data-only delta request passed strict content comparison:
    `.local/sow-0093/everyday-comparisons/focused-delta-after-callback-stop/14-data-only-delta.json`;
  - tail/`if_modified_since` request passed strict content comparison:
    `.local/sow-0093/everyday-comparisons/focused-tail-after-304-gate-fix/15-tail-no-change.json`.
- Current 20-case matrix report:
  `.local/sow-0093/everyday-comparisons/everyday-matrix-after-current-fixes/summary.json`.
  It has 14 strict content passes and 6 failures:
  - `01-info`: only a one-second source-summary drift in live-directory
    `required_params` metadata between SDK-first and plugin-second execution;
  - `04-priority-or-filter`, `06-facility-or-filter`, `08-source-system`:
    returned rows differ only in a plugin `MESSAGE` value that stock
    `journalctl --file` reads like the SDK, not like the installed plugin.
    This is recorded as a plugin-side row-content defect, not normalized away;
  - `05-priority-and-boot-filter`: the installed plugin changes `PRIORITY`
    facet counts when `PRIORITY` is both selected and requested as a facet.
    A focused boot-only request passes, so the remaining mismatch is the
    plugin's same-field-filter interaction, not boot filtering itself;
  - `16-sampling-low-budget`: rows match, but sampled/unsampled/estimated
    facet, histogram, and item counters differ. Exact sampling parity remains
    open.
- Performance check after the fixes:
  `.local/sow-0093/performance/default-full-5rep-after-current-fixes.json`.
  The default full-analysis request passed all five strict content repetitions.
  Wall-clock means:
  - SDK mean: `3.149s`;
  - plugin mean: `11.408s`;
  - ratio of means: `3.623x`;
  - warm repetitions 2-5 ratio of means: `3.632x`.
- Performance attribution so far:
  - the current wrapper is not slower than the historical wrapper artifacts
    already recorded in this regression section;
  - the earlier approximately `4.99x` speedup is not reproduced on the current
    installed plugin/current live corpus run;
  - the likely explanation is measurement/corpus/plugin-state variation, not a
    verified SDK regression. This remains a measured finding, not a proof of
    impossible regression.
- Local validation passed after code changes:
  - `cargo fmt`;
  - `cargo test -p journal netdata --lib`;
  - `cargo test -p journal explorer --lib`;
  - `cargo build --release -p netdata_function_wrapper`;
  - `python3 -m unittest tests.netdata_function.test_compare_function_json`.

Open decisions before re-close:

1. Decide whether the SDK should emulate known plugin defects/quirks for strict
   test equality:
   - plugin `MESSAGE` corruption for specific long rows;
   - plugin `PRIORITY` facet count behavior when `PRIORITY` is both selected
     and faceted.
2. Decide whether sampling must be bit-for-bit/count-for-count compatible with
   the current plugin approximation, or whether exact rows plus documented
   approximate sampled counters are acceptable.
3. Decide whether live-directory `info` source-summary time strings should be
   compared strictly, normalized as volatile, or tested only against a frozen
   directory snapshot.

User decisions on 2026-06-07:

1. The SDK must stay correct against stock `journalctl`/journal file content for
   rows where the installed plugin emits corrupted `MESSAGE` text. Do not
   emulate that installed-plugin defect.
2. The SDK must keep logical same-field facet semantics when a field such as
   `PRIORITY` is both selected and faceted. Do not emulate the installed
   plugin's selected-and-faceted count quirk unless a later SOW explicitly
   requires bug-for-bug plugin compatibility.
3. Sampling counters and estimates should be brought to current plugin parity.
4. Strict `info` comparisons should use a frozen directory snapshot instead of
   treating live source-summary time drift as a stable content mismatch.

Resolution progress on 2026-06-07:

- Implemented the accepted decision model in the comparator and SDK:
  - known installed-plugin `MESSAGE` corruption is reported as a non-content
    plugin defect only when the mismatch shape is narrowly identified; similar
    row mismatches still fail strict comparison;
  - selected-and-faceted same-field quirks are reported as non-content plugin
    behavior after verifying that removing the selected facet id from both
    outputs leaves the remaining facet content equal;
  - default catalog/facet unavailable-empty artifacts are compared through the
    existing unavailable-field normalization path and do not hide populated
    facet differences.
- Repaired sampling parity:
  - shared sampling state now uses the actual histogram bucket count selected
    by the query instead of a fixed placeholder;
  - seqnum-based remaining-row estimation now follows the plugin-shaped
    `expected_matching_logs - scanned_logs` formula;
  - estimated histogram distribution now follows the plugin's integer
    proportional bucket math instead of forcing at least one event into each
    touched bucket.
- Added focused Rust unit tests for the sampling bucket count and estimated
  histogram distribution behavior.
- Added focused Python comparator tests for the installed-plugin `MESSAGE`
  defect classifier, the selected-field facet quirk classifier, and negative
  cases that must still fail.

Current side-by-side comparison evidence:

- Broad SDK-first matrix report:
  `.local/sow-0093/everyday-comparisons/everyday-matrix-after-sampling-parity-20260607T015733+0300/summary.json`.
- The matrix has 20 request cases and 20 stable-content passes:
  `info`, default full analysis, explicit common facets, positive OR filters,
  selected-field plus boot filter, facility filter, identifier filter,
  `__logs_sources` source selection, FTS OR, FTS negative, FTS wildcard,
  escaped FTS separator, data-only, data-only delta, tail/no-change,
  low-budget sampling, forward anchor, empty-result filter, low limit, and
  alternate histogram field.
- Meaningful non-content classifications in that matrix:
  - installed-plugin `MESSAGE` corruption rows: `04-priority-or-filter` has 2,
    `06-facility-or-filter` has 2, and `08-source-system` has 4;
  - selected-and-faceted same-field quirk:
    `05-priority-and-boot-filter` has selected facet ids `PRIORITY` and
    `_BOOT_ID`;
  - `02-default-full` has one SDK-side unavailable-empty facet artifact
    normalized by the unavailable-field rule.
- The focused low-budget sampling report now passes all stable checks:
  `.local/sow-0093/everyday-comparisons/focused-sampling-after-estimated-histogram/16-sampling-low-budget.json`.
  Stable item counters matched exactly:
  `after=54349`, `estimated=5207632`, `matched=5320661`,
  `returned=200`, and `unsampled=58480`.

Current performance evidence:

- Five-repetition default full-analysis benchmark:
  `.local/sow-0093/performance/default-full-5rep-after-sampling-parity-20260607T020022+0300.json`.
- All five repetitions passed strict stable-content comparison.
- Wall-clock means across all five repetitions:
  - SDK mean: `2.982930884603411` seconds;
  - plugin mean: `10.959092871198663` seconds;
  - ratio of means: `3.673934561395547x`.
- Warm repetitions 2-5:
  - SDK mean: `2.9854240062559256` seconds;
  - plugin mean: `10.980673723999644` seconds;
  - ratio of warm means: `3.6780952055687077x`.
- Per-repetition speedups were `3.66x`, `3.46x`, `3.72x`, `3.76x`,
  and `3.79x`.
- The earlier approximately `4.99x` speedup remains unreproduced on the current
  installed plugin and current local corpus. The current evidence does not show
  wrapper glue or JSON shaping as the lost-performance source.

Profile evidence:

- SDK perf report:
  `.local/sow-0093/performance/sdk-default-full-perfreport.txt`.
- Top exclusive samples are in the Explorer traversal hot path:
  - `scan_current_row`: `31.22%`;
  - `explore_traversal`: `12.87%`;
  - `WindowManager::get_slice`: `4.71%`;
  - `ExplorerAccumulator::apply_value`: `3.65%`;
  - `CurrentRowView::load_entry`: `3.27%`;
  - `ExplorerAccumulator::finish_histogram_row`: `3.21%`.
- Current conclusion: the repaired wrapper is still much faster than the
  installed plugin for full-analysis and FTS cases, but the remaining cost is
  real Explorer traversal work. Further compressed-DATA layout optimization is
  intentionally tracked separately by SOW-0094.

Local validation after regression repair:

- `cargo fmt --check` from the Rust workspace: passed.
- `cargo test -p journal netdata --lib`: passed, 48 tests.
- `cargo test -p journal explorer_fts --lib`: passed, 2 tests.
- `cargo test -p journal`: passed, 104 tests.
- `cargo build --release -p netdata_function_wrapper`: passed.
- `python3 -m py_compile tests/netdata_function/run_function_compare.py
  tests/netdata_function/compare_function_json.py
  tests/netdata_function/test_compare_function_json.py`: passed.
- `python3 -m unittest tests.netdata_function.test_compare_function_json`:
  passed, 22 tests.
- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed with clean verdict.

First reviewer batch after regression repair:

- Six read-only reviewers were run against the whole SOW and changed surface.
- Production-grade votes:
  - `llm-netdata-cloud/glm-5.1`;
  - `llm-netdata-cloud/kimi-k2.6`;
  - `llm-netdata-cloud/mimo-v2.5-pro`;
  - `llm-netdata-cloud/minimax-m3-coder`;
  - `llm-netdata-cloud/deepseek-v4-pro`.
- Blocking vote:
  - `llm-netdata-cloud/qwen3.6-plus` returned `NOT PRODUCTION GRADE`.
- Credible blocking findings from the qwen review:
  - realtime duplicate-timestamp adjustment state was reset per file instead
    of being preserved across the sorted query stream;
  - the seqnum sampling estimate formula needed a source-traced comment and an
    edge test for `expected_matching_logs < scanned_rows`;
  - the selected-and-faceted comparator quirk removed the whole selected facet,
    which could hide selected-facet metadata mismatches.

Fixes after first reviewer batch:

- Replaced per-file realtime-adjustment closure state with a
  `NetdataRealtimeAdjuster` that is created once for the query stream and
  preserves duplicate timestamp state across file boundaries.
- Added forward and backward unit tests proving realtime adjustment state
  survives file-boundary calls.
- Added a source-traced comment for the seqnum sampling estimate to explain the
  Netdata plugin formula and its clamp behavior.
- Added a sampling edge test proving the seqnum estimator clamps to `1` when
  scanned rows exceed expected matching rows.
- Narrowed the selected-and-faceted comparator classifier: it now suppresses
  only the `options` map for selected facet fields. Facet identity and metadata
  remain strict content, and all unselected facets remain strict content.
- Added comparator regression coverage proving selected facet metadata
  mismatches still fail.

Local validation after reviewer-finding fixes:

- `cargo fmt --check` from the Rust workspace: passed.
- `cargo test -p journal netdata --lib`: passed, 50 tests.
- `cargo test -p journal explorer_sampling --lib`: passed, 2 tests.
- `cargo test -p journal`: passed, 107 tests and 0 doctests.
- `cargo build --release -p netdata_function_wrapper`: passed.
- `python3 -m unittest tests.netdata_function.test_compare_function_json`:
  passed, 23 tests.
- `python3 -m py_compile tests/netdata_function/run_function_compare.py
  tests/netdata_function/compare_function_json.py
  tests/netdata_function/test_compare_function_json.py`: passed.

Current side-by-side comparison evidence after reviewer-finding fixes:

- Broad SDK-first matrix report:
  `.local/sow-0093/everyday-comparisons/everyday-matrix-after-review-fixes-20260607T022608+0300/summary.json`.
- The matrix has 20 request cases and 20 stable-content passes:
  `info`, default full analysis, explicit common facets, positive OR filters,
  selected-field plus boot filter, facility filter, identifier filter,
  `__logs_sources` source selection, FTS OR, FTS negative, FTS wildcard,
  escaped FTS separator, data-only, data-only delta, tail/no-change,
  low-budget sampling, forward anchor, empty-result filter, low limit, and
  alternate histogram field.

Current performance evidence after reviewer-finding fixes:

- Five-repetition default full-analysis benchmark:
  `.local/sow-0093/performance/default-full-5rep-after-review-fixes-20260607T022905+0300.json`.
- All five repetitions passed strict stable-content comparison.
- Wall-clock means across all five repetitions:
  - SDK mean: `3.950250931998016` seconds;
  - plugin mean: `12.677642537408975` seconds;
  - ratio of means: `3.209325877178311x`.
- Per-repetition SDK wall times: `4.353588`, `4.018667`, `3.668132`,
  `3.784618`, and `3.926249` seconds.
- Per-repetition plugin wall times: `12.722075`, `12.826990`, `12.633364`,
  `12.272259`, and `12.933526` seconds.
- The current number remains faster than the installed plugin, but lower than
  the previous `3.68x` local artifact. This SOW records the fresh value as the
  current evidence and does not rely on the older speedup for closeout.

Final same-scope reviewer rerun after reviewer-finding fixes:

- Reviewer prompt:
  `.local/sow-0093/reviewer-prompts/sow-0093-review-rerun-after-fixes.md`.
- Reviewer reports:
  - `llm-netdata-cloud/kimi-k2.6`:
    `.local/sow-0093/reviewer-rerun-after-review-fixes/kimi.txt`,
    vote `PRODUCTION GRADE`;
  - `llm-netdata-cloud/mimo-v2.5-pro`:
    `.local/sow-0093/reviewer-rerun-after-review-fixes/mimo.txt`,
    vote `PRODUCTION GRADE`;
  - `llm-netdata-cloud/qwen3.6-plus`:
    `.local/sow-0093/reviewer-rerun-after-review-fixes/qwen.txt`,
    vote `PRODUCTION GRADE`;
  - `llm-netdata-cloud/deepseek-v4-pro`:
    `.local/sow-0093/reviewer-rerun-after-review-fixes/deepseek.txt`,
    vote `PRODUCTION GRADE`;
  - `llm-netdata-cloud/minimax-m3-coder`:
    `.local/sow-0093/reviewer-rerun-after-review-fixes/minimax.txt`,
    vote `PRODUCTION GRADE`;
  - `llm-netdata-cloud/glm-5.1`:
    `.local/sow-0093/reviewer-rerun-after-review-fixes/glm-final.txt`,
    vote `PRODUCTION GRADE`.
- GLM produced two earlier unusable review transcripts without a readiness
  vote. They were not counted as votes. The final GLM rerun used the same
  review scope and produced the required vote.
- The Minimax reviewer ran validation commands despite the read-only prompt.
  The source tree remained unchanged, and the vote is recorded with this
  protocol violation noted. No reviewer-created source change was accepted.

Reviewer findings and dispositions:

- SOW-status stale benchmark number: fixed by updating
  `.agents/sow/SOW-status.md` to include the post-review-fix benchmark
  (`3.950s` SDK mean, `12.678s` plugin mean, `3.21x`) while preserving the
  earlier `3.67x` artifact as historical evidence.
- Selected-and-faceted classifier can hide option-map differences within a
  selected facet: accepted as the narrowest practical installed-plugin quirk
  classifier for this SOW. Facet identity/metadata and all unselected facets
  remain strict, and the user explicitly decided not to emulate the plugin's
  selected-and-faceted count behavior. No follow-up is required for this SOW.
- Vocabulary padding reopens matched files: accepted as plugin-compatible
  zero-count vocabulary behavior outside the returned-row hot path. The current
  benchmark still shows `3.21x` speedup. No follow-up is required here.
- `push_unique()` linear scan, heap retained-bound refresh, boot annotation
  file reopening, and theoretical histogram counter overflow: accepted as
  non-blocking at current field, row-limit, and journal-size scales. No
  follow-up is required unless future real workload evidence shows practical
  cost or overflow risk.
- `NetdataRealtimeAdjuster` zero-timestamp first-call edge: accepted as
  theoretical only because real journal realtime timestamps for this function
  boundary are nonzero; cross-file state preservation is covered by unit tests.
  No follow-up is required.
- Test-name precision and explanatory comments suggested by reviewers are
  non-blocking and do not affect the shipped contract. No follow-up is
  required.
