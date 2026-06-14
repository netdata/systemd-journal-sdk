# SOW-0047 - Netdata NetFlow SDK Integration

## Status

Status: completed

Sub-state: completed on 2026-06-14 after read-only verification that the
Netdata NetFlow plugin consumes the published Rust SDK crates at `0.7.0` for
writer and reader/query paths.

## Requirements

### Purpose

Integrate this SDK into Netdata NetFlow writer and reader paths only after the
SDK writer and reader performance gates are acceptable.

### User Request

The user identified NetFlow as needing both reader and writer integration, and
as a critical performance path with existing fast vendored Rust behavior.

### Assistant Understanding

Facts:

- NetFlow currently has journal writer and reader/query consumers.
- Writers should default to compact journal format after integration.
- Existing journals and query/facet behavior must continue to work.

Inferences:

- NetFlow should not be migrated until writer and reader performance SOWs pass.

Known closeout target:

- Netdata checkout inspected at `ktsaou/netdata @
  36050079cfa90c6d2775512c4b11bc5ca376aef5`, branch `master`.
- Dependency strategy is crates.io consumption with package aliases preserving
  the historical `journal_*` call-site names.

### Acceptance Criteria

- SOW-0026 inventory identifies exact NetFlow writer and reader paths.
- SOW-0042 writer certification passes for NetFlow-shaped writer workloads.
- SOW-0044/SOW-0045 reader performance passes for NetFlow-shaped reader/query
  workloads as applicable.
- NetFlow writers use SDK compact-default writer behavior.
- NetFlow readers/query/rebuild/facet paths use SDK reader/query APIs or an
  approved facade.
- Existing regular, compact, compressed, sealed, open, and closed files remain
  readable in mixed directories.
- No changes are made outside this repository unless the user explicitly
  authorizes a Netdata repository target for this SOW.

## Analysis

Sources checked:

- `.agents/sow/pending/SOW-0026-20260526-netdata-sdk-integration.md`

Current state:

- NetFlow has been migrated to the published SDK crates at `0.7.0`.
- The migration is scoped to `netflow-plugin`; other Netdata journal consumers
  remain tracked by SOW-0048, SOW-0049, and SOW-0050.

Risks:

- High ingestion and query performance risk.
- Data compatibility risk if mixed-directory behavior regresses.

## Pre-Implementation Gate

Status: blocked until SOW-0026, SOW-0042, and reader performance gates close

Problem / root-cause model:

- NetFlow is a high-performance production path. It must not be migrated until
  the SDK is demonstrably fit for the same workload.

Evidence reviewed:

- SOW-0026 integration scope and user prioritization.

Affected contracts and surfaces:

- NetFlow ingestion, query, replay, rebuild, facets, compact default, and
  storage compatibility.

Existing patterns to reuse:

- Existing NetFlow journal integration discovered by SOW-0026.
- SDK writer and reader APIs after performance SOWs.

Risk and blast radius:

- High.

Sensitive data handling plan:

- Use synthetic NetFlow-shaped fixtures. Do not record real flow payloads,
  customer data, credentials, bearer tokens, SNMP communities, private
  endpoints, or production incident details.

Implementation plan:

1. Wait for SOW-0026 cut plan and performance gates.
2. Obtain explicit Netdata repository authorization.
3. Implement writer replacement.
4. Implement reader/query replacement.
5. Validate NetFlow benchmarks and behavior.

Validation plan:

- NetFlow unit/integration tests.
- SDK conformance tests.
- NetFlow benchmarks.
- Read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: update only if cross-repo workflow is authorized.
- Runtime project skills: update only if integration workflow becomes durable.
- Specs: update integration status.
- End-user/operator docs: update Netdata docs if behavior/config changes.
- End-user/operator skills: update only if docs/spec changes affect them.
- SOW lifecycle: blocked until prerequisites close.
- SOW-status.md: update on activation and close.

Open-source reference evidence:

- Netdata evidence will be refreshed in SOW-0026.

Open decisions:

- Netdata repository target and dependency strategy.

## Implications And Decisions

- 2026-05-28: user agreed actual Netdata integration happens after performance
  gates, with NetFlow split as its own component SOW.

## Plan

1. Wait for prerequisites.
2. Integrate NetFlow writer.
3. Integrate NetFlow readers.
4. Validate and review.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Read-only reviewers from the approved pool.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- DO NOT MAKE CHANGES OUTSIDE THIS REPOSITORY FOR ANY REASON.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- Record NetFlow blockers, benchmark failures, and missing SDK APIs before
  changing scope.

## Execution Log

### 2026-05-28

- Created from the agreed SOW restructuring.

### 2026-06-14

- Verified the NetFlow plugin read-only in the user's Netdata checkout.
- Confirmed `netflow-plugin` depends on the published `systemd-journal-sdk-*`
  crates at `0.7.0`, aliased to the historical `journal-*` dependency names.
- Confirmed the ingest writer constructs SDK `journal_log_writer::Log` writers
  for raw and materialized tiers, uses compact format, disables compression,
  and writes rows through `write_entry_with_timestamps`.
- Confirmed query/facet paths open journal files through SDK
  `JournalFile<Mmap>` and `JournalFileMap`, step rows with `JournalReader`, and
  access DATA through `data_ref` and field DATA object iteration.

## Validation

Acceptance criteria evidence:

- SOW-0026 inventory prerequisite: completed before this closeout and recorded
  in the project SOW ledger.
- SDK performance and release gates: the integration consumes SDK release
  `0.7.0`, after writer/reader performance and parity work landed in prior
  completed SOWs.
- NetFlow writer dependency:
  - `ktsaou/netdata @ 36050079cfa9`
  - `src/crates/netflow-plugin/Cargo.toml:23-32` documents migration to the
    published SDK and pins `systemd-journal-sdk-common`,
    `systemd-journal-sdk-core`, `systemd-journal-sdk-engine`,
    `systemd-journal-sdk-index`, `systemd-journal-sdk-log-writer`, and
    `systemd-journal-sdk-registry` at `0.7.0`.
  - `src/crates/Cargo.lock:4285-4410` locks the same SDK packages to `0.7.0`
    from crates.io with checksums.
- NetFlow writer path:
  - `ktsaou/netdata @ 36050079cfa9`
  - `src/crates/netflow-plugin/src/ingest.rs:18-21` imports SDK identity,
    index, writer, and registry aliases.
  - `src/crates/netflow-plugin/src/ingest/service/init.rs:49-57` configures
    compact journal storage, no DATA compression, no FSS, and disables live
    publication for NetFlow's file-open reader pattern.
  - `src/crates/netflow-plugin/src/ingest/service/init.rs:108-158` constructs
    SDK `Log` writers for raw, 1m, 5m, and 1h tiers.
  - `src/crates/netflow-plugin/src/ingest/encode.rs:25-39` writes raw flow
    records through `journal_log_writer::Log::write_entry_with_timestamps`.
  - `src/crates/netflow-plugin/src/ingest/encode.rs:87-99` writes tier rows
    through the same SDK writer call.
- NetFlow reader/query path:
  - `ktsaou/netdata @ 36050079cfa9`
  - `src/crates/netflow-plugin/src/query.rs:14-19` imports SDK reader,
    registry, and mmap-backed file types.
  - `src/crates/netflow-plugin/src/query/scan/raw.rs:110-190` opens raw
    journal files with `JournalFile::<Mmap>::open`, steps rows with
    `JournalReader`, collects ENTRY DATA offsets, and applies payloads.
  - `src/crates/netflow-plugin/src/query/scan/raw.rs:245-285` reads DATA with
    `journal.data_ref`, using SDK decompression only when the DATA is
    compressed.
  - `src/crates/netflow-plugin/src/query/facets/cache/scan.rs:3-33`
    enumerates field values through `JournalFileMap::field_data_objects`.
  - `src/crates/netflow-plugin/src/facet_runtime.rs:643-667` scans closed
    NetFlow journals through SDK `JournalFileMap` for facet contributions.

Tests or equivalent validation:

- Equivalent closeout validation was read-only source and lockfile inspection
  of the NetFlow integration in the external Netdata checkout.
- A Netdata build/test was not run by this SDK closeout because it would write
  build artifacts outside this repository. The user reported the NetFlow
  integration as complete, and this SOW records the SDK-side evidence needed to
  close the SDK ledger.

Real-use evidence:

- The NetFlow plugin dependency lock proves it consumes crates.io SDK packages
  at `0.7.0` instead of this repository's vendored Rust source for the scoped
  NetFlow binary.

Reviewer findings:

- No external reviewer pass was run for this closeout. The implementation
  landed in the external Netdata repository, not in this SDK repository; the
  SDK closeout is based on direct read-only verification plus the user's
  integration report.

Same-failure scan:

- `rg` over `src/crates/netflow-plugin` and `src/crates/Cargo.lock` found the
  expected SDK aliases and no contradictory NetFlow journal dependency path for
  the scoped binary. The scan also showed other Netdata journal consumers are
  still separate work, as already tracked by SOW-0048, SOW-0049, and SOW-0050.

Sensitive data gate:

- Passed. This SOW records only source paths, dependency versions, package
  names, and sanitized commit evidence. No flow payloads, credentials, private
  endpoints, customer data, or runtime logs were copied.

Artifact maintenance gate:

- AGENTS.md: no change needed; this closeout does not change project workflow.
- Runtime project skills: no change needed; no durable workflow changed.
- Specs: no change needed; SDK public behavior did not change.
- End-user/operator docs: no change needed in this repository; Netdata
  integration documentation belongs with Netdata if user-facing behavior
  changes.
- End-user/operator skills: no change needed.
- SOW lifecycle: this SOW is moved from pending to done with `Status:
  completed`.
- SOW-status.md: updated to remove SOW-0047 from pending and add it to the
  completed list.

Specs update:

- No SDK spec update needed; this SOW closes an external component integration
  against already released SDK APIs.

Project skills update:

- No project skill update needed; orchestration and repository-boundary rules
  remain valid.

End-user/operator docs update:

- No SDK docs update needed; the consumer-facing SDK APIs were already
  released and documented before NetFlow consumed them.

End-user/operator skills update:

- No end-user/operator skill update needed.

Lessons:

- Component-integration SOWs that close based on external repository evidence
  must record the inspected repository commit and whether tests were run or
  intentionally not run.

Follow-up mapping:

- SOW-0048 remains open for Netdata OTEL writer integration.
- SOW-0049 remains open for Netdata reader plugin and static packaging
  integration.
- SOW-0050 remains open for final vendored journal code removal after all
  component integrations are complete.

## Outcome

Completed.

NetFlow is integrated with the published SDK crates at `0.7.0` for both writer
and reader/query paths in the inspected Netdata checkout. This closes the SDK
ledger item for NetFlow integration. The remaining Netdata integration work is
still tracked separately by SOW-0048, SOW-0049, and SOW-0050.

## Lessons Extracted

- External consumer integrations should be closed with exact consumer commit,
  dependency lock evidence, and concrete call-site evidence.

## Followup

- SOW-0048 - Netdata OTEL Writer SDK Integration.
- SOW-0049 - Netdata Reader Plugin SDK Integration.
- SOW-0050 - Netdata Vendored Journal Removal.

## Regression Log

None yet.
