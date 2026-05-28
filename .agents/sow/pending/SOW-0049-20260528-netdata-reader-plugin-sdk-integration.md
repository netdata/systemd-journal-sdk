# SOW-0049 - Netdata Reader Plugin SDK Integration

## Status

Status: open

Sub-state: created on 2026-05-28 as the Netdata reader component integration
SOW, blocked by SOW-0026 inventory and reader performance gates.

## Requirements

### Purpose

Integrate SDK reader APIs into Netdata reader plugins and no-libsystemd build
paths after reader parity and performance are acceptable.

### User Request

The user identified these reader integrations:

- `otel-signal-viewer.plugin` reader;
- `systemd-journal.plugin` reader when compiled without libsystemd;
- static packaging that depends on the pure reader.

### Assistant Understanding

Facts:

- Reader integration depends on the `jf`/libsystemd-compatible facade.
- Reader integration depends on ordered directory reading and mixed-format
  directory support.
- `systemd-journal.plugin` no-libsystemd mode is a critical static-build use
  case.

Inferences:

- This SOW should wait for Rust/Go/Python/Node.js reader parity and performance
  decisions where they affect Netdata.

Unknowns:

- Exact Netdata commit and packaging matrix.

### Acceptance Criteria

- SOW-0026 inventory identifies exact reader plugin paths and build options.
- SOW-0043 through SOW-0046 close relevant reader parity/performance gaps.
- `otel-signal-viewer.plugin` uses SDK reader/query APIs or an approved facade.
- `systemd-journal.plugin` uses SDK reader path when compiled without
  libsystemd, with no runtime link to libsystemd in that mode.
- Static packaging uses the pure reader path successfully.
- Mixed directories with regular/compact, compressed/uncompressed, sealed,
  open, and closed files remain readable.
- No changes are made outside this repository unless the user explicitly
  authorizes a Netdata repository target for this SOW.

## Analysis

Sources checked:

- `.agents/sow/pending/SOW-0026-20260526-netdata-sdk-integration.md`

Current state:

- Pending reader parity/performance and Netdata cut plan.

Risks:

- Reader regressions can break query latency, static builds, or no-libsystemd
  deployments.
- Linking accidentally to libsystemd in the fallback path would violate the
  purpose of the integration.

## Pre-Implementation Gate

Status: blocked until SOW-0026 and reader performance SOWs close

Problem / root-cause model:

- Reader plugin integration should use the finalized reader facade and measured
  hot paths, not temporary reader APIs.

Evidence reviewed:

- SOW-0026 integration scope and user prioritization.

Affected contracts and surfaces:

- OTEL signal viewer reader behavior.
- systemd journal plugin no-libsystemd behavior.
- Static packaging.
- SDK reader API and `jf` facade.

Existing patterns to reuse:

- Existing Netdata reader/facade patterns discovered by SOW-0026.
- SDK reader APIs after SOW-0043 through SOW-0046.

Risk and blast radius:

- High for static builds and reader reliability.

Sensitive data handling plan:

- Use synthetic fixtures only. Do not record real logs, customer data,
  credentials, bearer tokens, SNMP communities, private endpoints, personal
  data, or production incident details.

Implementation plan:

1. Wait for SOW-0026 cut plan and reader performance gates.
2. Obtain explicit Netdata repository authorization.
3. Integrate OTEL signal viewer reader.
4. Integrate systemd journal plugin no-libsystemd reader.
5. Validate static packaging and reader behavior.

Validation plan:

- Netdata reader tests.
- Static/no-libsystemd build tests.
- Mixed-format directory tests.
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

- Netdata repository target and packaging matrix.

## Implications And Decisions

- 2026-05-28: user agreed reader plugin integrations happen after reader
  performance work.

## Plan

1. Wait for prerequisites.
2. Integrate reader plugins.
3. Validate static/no-libsystemd behavior.
4. Review and close.

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

- Record packaging failures, reader gaps, and missing SDK APIs before changing
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
