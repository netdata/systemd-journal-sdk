# SOW-0093 - Netdata Function Boundary Reader Comparison

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: created as follow-up after SOW-0083; repository-boundary decision recorded.

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
- Build a canonical output comparator that compares semantic normalized
  function JSON, not byte-for-byte JSON. The comparator may remove or normalize
  only documented volatile fields, such as expiry timestamps, generation
  timestamps, durations, runtime stats, and agent/runtime identifiers. It must
  normalize ordering only where the Netdata function contract permits it.
- Treat any unclassified output difference as a mismatch until the SOW records
  why it is volatile or intentionally different.
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
  `netdata/netdata#22638` provides that external CLI once built and installed.

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

## Validation

Acceptance criteria evidence:

- Pending implementation.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending selected multi-GB journal datasets and function-boundary matrices.

Reviewer findings:

- Pending.

Same-failure scan:

- Pending.

Sensitive data gate:

- Pending implementation; durable artifacts must remain sanitized.

Artifact maintenance gate:

- AGENTS.md: pending closeout decision.
- Runtime project skills: pending closeout decision.
- Specs: pending closeout decision.
- End-user/operator docs: pending closeout decision.
- End-user/operator skills: pending closeout decision.
- SOW lifecycle: pending/open and not implemented.
- SOW-status.md: updated when SOW state changes.

Specs update:

- Pending.

Project skills update:

- Pending.

End-user/operator docs update:

- Pending.

End-user/operator skills update:

- Pending.

Lessons:

- Pending.

Follow-up mapping:

- Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
