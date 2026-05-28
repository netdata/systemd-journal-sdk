# SOW-0050 - Netdata Vendored Journal Removal

## Status

Status: open

Sub-state: created on 2026-05-28 as the final Netdata cleanup SOW.

## Requirements

### Purpose

Remove obsolete Netdata-local or vendored journal implementations only after all
Netdata writer and reader consumers have moved to the SDK and validation proves
there is no remaining dependency.

### User Request

The user stated that after Netdata integrations finish, all Netdata code should
use the SDK and the old Rust implementation can be removed.

### Assistant Understanding

Facts:

- Vendored removal is last, after component integrations.
- Removal must not happen from this repository without explicit Netdata
  repository authorization.

Inferences:

- This cleanup should be separate because deleting old code is high risk and
  should happen only when searches and build/tests prove it is unused.

Unknowns:

- Exact vendored/local files to remove at the final Netdata commit.

### Acceptance Criteria

- SOW-0047, SOW-0048, and SOW-0049 are complete.
- Fresh Netdata repository search proves no production code still depends on
  the old vendored journal implementation.
- Build and packaging files no longer reference obsolete journal crates/modules.
- Tests cover affected Netdata plugins and static/no-libsystemd paths.
- Docs or comments pointing to old vendored code are updated or removed.
- No changes are made outside this repository unless the user explicitly
  authorizes a Netdata repository target for this SOW.

## Analysis

Sources checked:

- `.agents/sow/pending/SOW-0026-20260526-netdata-sdk-integration.md`

Current state:

- Cleanup is premature until integration SOWs close.

Risks:

- Removing old code too early could break a hidden Netdata path.
- Leaving old code after migration could cause drift and future confusion.

## Pre-Implementation Gate

Status: blocked until all component Netdata integration SOWs close

Problem / root-cause model:

- The old vendored implementation is only safe to remove after every consumer
  has moved and searches prove no references remain.

Evidence reviewed:

- User integration plan and SOW-0026 scope.

Affected contracts and surfaces:

- Netdata build graph, crates/modules, packaging, plugin behavior, and docs.

Existing patterns to reuse:

- Netdata dependency cleanup patterns discovered during SOW-0026.

Risk and blast radius:

- High because removal can break builds or runtime paths.

Sensitive data handling plan:

- Source-only cleanup. Do not record real logs, customer data, credentials,
  bearer tokens, SNMP communities, private endpoints, personal data, or
  production incident details.

Implementation plan:

1. Wait for component integrations.
2. Search Netdata for all old journal references.
3. Remove obsolete code and dependency declarations.
4. Run full affected build/test matrix.
5. Review and close.

Validation plan:

- Netdata build/test matrix.
- Search proving no stale references.
- Read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: update only if cross-repo workflow is authorized.
- Runtime project skills: update only if integration workflow becomes durable.
- Specs: update integration/removal status.
- End-user/operator docs: update Netdata docs if behavior/config changes.
- End-user/operator skills: update only if docs/spec changes affect them.
- SOW lifecycle: final Netdata cleanup SOW.
- SOW-status.md: update on activation and close.

Open-source reference evidence:

- Netdata evidence will be refreshed after component SOWs close.

Open decisions:

- Netdata repository target.

## Implications And Decisions

- 2026-05-28: user agreed actual integration and vendored removal happen after
  SDK performance is acceptable.

## Plan

1. Wait for component integrations.
2. Remove unused vendored/local journal code.
3. Validate build, tests, packaging, and stale-reference search.

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

- Record stale references, build failures, packaging failures, and reviewer
  findings before changing scope.

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
