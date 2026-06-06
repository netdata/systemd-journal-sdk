# SOW-0093 - Netdata Function Boundary Reader Comparison

## Status

Status: in-progress

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: strict SDK-first content comparison passes locally; reviewer rerun
after the latest fixes and broader matrix validation remain pending. The
previous semantic-subset comparator proved only a narrow stable subset of the
Netdata-function response and is no longer sufficient for acceptance.

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
  `/path/to/wrapper --test function-name --dir /path/to/backend-data/ --request /path/to/function/request/payload.json [additional options if required]`;
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
- The wrapper must use the same `--test`, `--dir`, and `--request` command-line
  contract as the Netdata `systemd-journal.plugin` binary so the benchmark
  harness can swap binaries without changing payloads.
- The wrapper should be a thin CLI over the future SDK API. The reusable API
  should stay in a Netdata-specific module/layer, separate from the core
  journal file-format reader.

### Acceptance Criteria

- Select and document representative sanitized multi-GB journal datasets from
  the local large journal corpus.
- Add an SDK wrapper command with this CLI contract:
  `/path/to/wrapper --test function-name --dir /path/to/backend-data/ --request /path/to/function/request/payload.json [additional options if required]`.
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
- `systemd-journal.plugin` now has the external CLI shape SOW-0093 needs:
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

Affected contracts and surfaces:

- Rust SDK wrapper CLI.
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

1. Inventory current Netdata function defaults and request/response
   contracts. This step is mandatory before implementation and must produce
   the API boundary for the Netdata-specific SDK layer. NetFlow analysis is
   design evidence only.
2. Select datasets and create sanitized dataset manifests.
3. Implement SDK wrapper and canonical semantic comparator.
4. Invoke the Netdata `systemd-journal.plugin` CLI entrypoint when available;
   do not edit Netdata
   source from this repository.
5. Build common and rare request payload suites.
6. Run cold/warm repeated performance and equality matrices.
7. Produce sanitized report and decision recommendation.

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
  SOW builds the SDK wrapper with the same `--test`, `--dir`, and `--request`
  contract, plus the semantic normalized comparison harness that can call that
  plugin CLI when it exists.
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

- Reviewer pool after complete implementation and local validation: minimax,
  kimi, qwen, glm, and mimo.

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
  `--test systemd-journal --dir <journal-dir> --request <request.json>`.
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

- Partial. Rust SDK API, wrapper CLI, semantic comparator, and repo-local
  plugin smoke are implemented. Multi-GB dataset selection and repeated
  performance matrix remain pending.

Tests or equivalent validation:

- `cargo test -p journal netdata --lib` passed from the Rust workspace.
- `cargo test -p journal explorer --lib` passed from the Rust workspace.
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
- `collect_column_fields=false` experiment on 2026-06-06:
  - code state: `NetdataRequest::to_explorer_query()` forced
    `collect_column_fields: false` for the experiment;
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
    objects before row traversal; `ExplorerQuery::collect_column_fields` remains
    false so row traversal does not parse unrelated DATA just to discover
    columns.
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
- `__logs_sources` selections are explicitly ignored by this standalone
  `--dir` wrapper. Source-group filtering depends on Netdata's journal registry
  and is not implemented in this repository-local comparison wrapper yet.
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
    `--dir` wrapper. Disposition: documented as an explicit limitation because
    source groups require the Netdata journal registry boundary.
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

- AGENTS.md: no update needed; repository-boundary and runtime purity
  policies did not change.
- Runtime project skills: no update yet; the Netdata function comparison
  workflow is SOW-local until the full matrix is complete.
- Specs: updated
  `.agents/sow/specs/systemd-journal-plugin-facets.md` with the Rust Netdata
  function boundary, wrapper CLI, same-field facet-filter switch, missing
  histogram value behavior, and zero-count vocabulary comparator rule.
- End-user/operator docs: updated `rust/README.md` with the new
  `journal::netdata` API and wrapper command.
- End-user/operator skills: pending closeout decision.
- SOW lifecycle: current/in-progress; do not close until the multi-GB matrix is
  complete or explicitly split.
- SOW-status.md: updated when SOW state changes.

Specs update:

- Updated
  `.agents/sow/specs/systemd-journal-plugin-facets.md`.

Project skills update:

- Not updated; no reusable project-wide work procedure has been accepted yet.

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

- Decide whether presentation-order identity is a required Netdata boundary
  contract. If yes, this SOW must add comparator checks for column indexes and
  histogram label order, then align the SDK output before close.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
