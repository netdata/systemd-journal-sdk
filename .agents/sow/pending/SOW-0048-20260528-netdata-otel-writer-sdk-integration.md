# SOW-0048 - Netdata OTEL Writer SDK Integration

## Status

Status: open

Sub-state: created on 2026-05-28 as a component integration SOW, blocked by
SOW-0026 inventory and writer performance gates.

## Requirements

### Purpose

Integrate this SDK into Netdata OTEL logs writer paths after writer performance
and API stability are acceptable.

### User Request

The user identified `otel.plugin` writer integration as one of the required
Netdata consumers. Writers should default to compact format.

### Assistant Understanding

Facts:

- OTEL logs are structured before writing, so the SDK structured writer API
  should avoid unnecessary text conversion.
- OTEL writer integration should preserve existing batching, timestamps, and
  sync behavior.

Inferences:

- OTEL can move after writer certification; reader performance is not a direct
  prerequisite unless OTEL tests consume the generated files.

Unknowns:

- Exact Netdata commit and dependency strategy.

### Acceptance Criteria

- SOW-0026 inventory identifies exact OTEL writer paths.
- SOW-0042 writer certification passes for OTEL-shaped workloads.
- OTEL logs writer uses SDK compact-default writer behavior.
- Existing OTEL batching, timestamps, source realtime handling, and sync
  semantics are preserved.
- Stock systemd tooling can read compatible outputs.
- No changes are made outside this repository unless the user explicitly
  authorizes a Netdata repository target for this SOW.

## Analysis

Sources checked:

- `.agents/sow/pending/SOW-0026-20260526-netdata-sdk-integration.md`

Current state:

- Pending cut-plan and writer performance gate.

Risks:

- Writer API mismatch could reintroduce unnecessary allocations or field
  conversion.
- Compact default must not break existing readers.

## Pre-Implementation Gate

Status: blocked until SOW-0026 and SOW-0042 close

Problem / root-cause model:

- OTEL writer integration should consume the stable writer API and measured
  performance path rather than early SDK revisions.

Evidence reviewed:

- SOW-0026 integration scope and user prioritization.

Affected contracts and surfaces:

- OTEL logs ingestion, compact default, file compatibility, and dependency
  strategy.

Existing patterns to reuse:

- Existing OTEL writer integration discovered by SOW-0026.
- SDK high-level writer API after SOW-0042.

Risk and blast radius:

- Medium to high depending on OTEL deployment volume.

Sensitive data handling plan:

- Use synthetic OTEL-shaped fixtures. Do not record real logs, customer data,
  credentials, bearer tokens, private endpoints, personal data, or production
  incident details.

Implementation plan:

1. Wait for SOW-0026 cut plan and SOW-0042.
2. Obtain explicit Netdata repository authorization.
3. Replace OTEL writer path.
4. Validate behavior and performance.

Validation plan:

- OTEL writer tests.
- SDK conformance tests.
- Stock systemd readback.
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
  gates, with OTEL writer split as its own component SOW.

## Plan

1. Wait for prerequisites.
2. Integrate OTEL writer.
3. Validate and review.

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

- Record OTEL blockers, benchmark failures, and missing SDK APIs before changing
  scope.

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
