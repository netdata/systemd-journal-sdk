# SOW-0093 - Netdata Function Boundary Reader Comparison

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: created as follow-up after SOW-0083; repository-boundary decision recorded.

## Requirements

### Purpose

Build an end-to-end, Netdata-function-boundary comparison that proves whether
the SDK explorer logic produces the same function output as existing Netdata
plugins and measures realistic read/query performance on multi-GB datasets.

### User Request

The user requested a comparison plan:

- select two 4-5 GB datasets, one from the large local journal corpus and one
  from the NetFlow raw tier;
- create an SDK wrapper that accepts the same Netdata function test CLI shape
  the plugins will expose:
  `/path/to/wrapper --test function-name --dir /path/to/backend-data/ --request /path/to/function/request/payload.json [additional options if required]`;
- compare against `systemd-journal.plugin` after Netdata exposes the same
  standard CLI shape;
- compare against `netflow.plugin` after Netdata exposes the same standard CLI
  shape;
- create representative request payloads for common and rare cases, using the
  default facets from the Netdata plugins for common cases;
- compare SDK wrapper output against `systemd-journal.plugin` output for
  semantic normalized function JSON equality and speed;
- compare SDK wrapper output against `netflow.plugin` output for semantic
  normalized function JSON equality and speed.

### Assistant Understanding

Facts:

- SOW-0082 added an optimized Rust explorer traversal engine.
- SOW-0083 added explicit index and compare strategies, but no auto planner,
  because index aggregation is query-shape sensitive.
- Existing Netdata plugin behavior is the real integration target, not only the
  standalone SDK benchmark shape.

Inferences:

- Netdata-function-boundary testing is the right next evidence gate before
  deciding which explorer strategy to integrate.
- The Netdata plugin CLI entrypoints are standard Netdata features and will be
  created separately. This repository owns the SDK wrapper and comparison
  harness side only.
- Semantic normalized output equality at the function boundary is stronger
  evidence than internal SDK summaries because it includes request parsing,
  plugin defaults, sampling, facets, histogram, returned rows, and output
  formatting while excluding documented volatile metadata.
- Performance at this boundary will include useful overhead that raw SDK
  microbenchmarks intentionally exclude.

Unknowns:

- Exact default facet sets, request shapes, sampling rules, volatile output
  fields, and function output canonicalization must be re-read from current
  Netdata source before implementation.
- The wrapper must use the same `--test`, `--dir`, and `--request` command-line
  contract as the Netdata plugin binaries so the benchmark harness can swap
  binaries without changing payloads.

### Acceptance Criteria

- Select and document two sanitized 4-5 GB datasets:
  - one from the local large journal corpus;
  - one from the NetFlow raw tier.
- Add an SDK wrapper command with this CLI contract:
  `/path/to/wrapper --test function-name --dir /path/to/backend-data/ --request /path/to/function/request/payload.json [additional options if required]`.
- Treat Netdata-side plugin test entrypoints as external binaries with the same
  contract; do not implement or modify them in this repository.
- Extract default/common facets from current Netdata plugin source and use them
  in common-case request payloads.
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
  - SDK wrapper versus `netflow.plugin`;
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

Risks:

- Netdata source modifications are outside this repository and explicitly out
  of scope for this SOW.
- Multi-GB local journal data can contain sensitive operational data; reports
  must stay sanitized.
- Output equality requires stable canonicalization for maps, permitted ordering
  differences, and documented volatile metadata; over-normalization could hide
  real regressions.
- NetFlow may persist facet state or sidecar files during test initialization.
  Prefer a Netdata-side read-only/no-persist mode when practical. If unavailable,
  use a scratch dataset created through a low-overhead copy-on-write/reflink
  mechanism when the filesystem supports it; plain 4-5 GB copies are allowed
  only when the report labels the copy cost separately from query performance.
- Cold-cache measurements can be dominated by storage state; warm-cache and
  cold-cache runs must be labelled separately.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Microbenchmarks prove internal hot-path behavior, but they do not prove that
  SDK explorer logic matches Netdata plugin output or wins at the function
  boundary.
- The next confidence gap is realistic request payloads, plugin defaults, and
  semantic normalized output equality against existing Netdata implementations.

Evidence reviewed:

- SOW-0081: Netdata plugin/facets behavior specification.
- SOW-0082: optimized explorer traversal implementation and benchmark evidence.
- SOW-0083: explicit index strategy and benchmark evidence.

Affected contracts and surfaces:

- Rust SDK wrapper CLI.
- Netdata function payload input/output contracts.
- Netdata `systemd-journal.plugin` and `netflow.plugin` CLI contracts as
  external comparison dependencies.
- Benchmark reports and future Netdata integration decisions.

Existing patterns to reuse:

- `reader_core_bench` JSON result conventions.
- Sanitized report discipline from corpus-evaluation SOWs.
- Netdata plugin function request/response shapes from current Netdata source.

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

1. Inventory current Netdata plugin function defaults and request/response
   contracts.
2. Select datasets and create sanitized dataset manifests.
3. Implement SDK wrapper and canonical semantic comparator.
4. Invoke Netdata plugin CLI entrypoints when available; do not edit Netdata
   source from this repository.
5. Build common and rare request payload suites.
6. Run cold/warm repeated performance and equality matrices.
7. Produce sanitized report and decision recommendation.

Validation plan:

- Unit tests for payload parsing and output canonicalization, including
  permitted volatile-field normalization and unclassified-difference failures.
- Golden comparison tests on small synthetic journals.
- Multi-GB equality/performance matrix on selected datasets.
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
- SOW lifecycle: pending until implementation starts.
- SOW-status.md: updated with pending state.

Open-source reference evidence:

- No external open-source reference checked yet. Current Netdata source must be
  inspected at implementation time and cited as `netdata/netdata @ commit`.

Open decisions:

- None blocking. The user decided the Netdata plugin CLI entrypoints are
  standard Netdata features and will be created separately. This repository
  implements only the SDK wrapper and comparison harness.

## Implications And Decisions

- 2026-06-05 repository-boundary decision: this repository will not modify
  `systemd-journal.plugin` or `netflow.plugin`. The user will create their
  standard CLI function payload entrypoints separately. This SOW builds the SDK
  wrapper with the same `--test`, `--dir`, and `--request` contract, plus the
  semantic normalized comparison harness that can call those plugin CLIs when
  they exist.

## Plan

1. Inventory Netdata plugin defaults.
2. Select datasets.
3. Build SDK wrapper, request suite, semantic comparator, and report harness.
4. Compare against Netdata plugin CLI binaries when they are available.

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
- If Netdata plugin modifications are not approved, restrict this SOW to SDK
  wrapper and external binary invocation only.

## Execution Log

### 2026-06-05

- Created as pending follow-up from SOW-0083.

## Validation

Acceptance criteria evidence:

- Pending implementation.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending selected 4-5 GB datasets and function-boundary matrices.

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
