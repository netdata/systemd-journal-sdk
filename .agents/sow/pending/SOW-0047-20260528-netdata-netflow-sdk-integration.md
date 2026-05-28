# SOW-0047 - Netdata NetFlow SDK Integration

## Status

Status: open

Sub-state: created on 2026-05-28 as a component integration SOW, blocked by
SOW-0026 inventory and performance gates.

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

Unknowns:

- Exact Netdata commit and dependency strategy.

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

- Pending cut-plan and performance gates.

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

## Validation

Acceptance criteria evidence:

- Pending.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending.

Reviewer findings:

- Pending.

Same-failure scan:

- Pending.

Sensitive data gate:

- Pending implementation; planning text contains no raw sensitive data.

Artifact maintenance gate:

- Pending implementation.

Specs update:

- Pending implementation.

Project skills update:

- Pending implementation.

End-user/operator docs update:

- Pending implementation.

End-user/operator skills update:

- Pending implementation.

Lessons:

- Pending.

Follow-up mapping:

- Pending implementation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
