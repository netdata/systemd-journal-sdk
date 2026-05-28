# SOW-0026 - Netdata SDK Integration Inventory And Cut Plan

## Status

Status: open

Sub-state: rescope agreed on 2026-05-28. This pending SOW now owns the Netdata
integration inventory and cut plan only. Actual component edits are split into
component SOWs after writer and reader performance gates pass.

## Requirements

### Purpose

Create the exact Netdata integration map before changing Netdata. The final
integration should look natural in Netdata, preserve existing behavior, and use
this SDK everywhere journal reading or writing is needed.

### User Request

The user asked to track integration of the SDK into Netdata:

- `netflow.plugin` reader and writer paths;
- `otel.plugin` writer path;
- `otel-signal-viewer.plugin` reader path;
- `systemd-journal.plugin` reader path when compiled without libsystemd;
- static packaging that needs the no-libsystemd reader;
- removal of the old vendored Rust journal implementation after all consumers
  move to the SDK.

The user clarified that actual integration should happen last because old
vendored libraries may still be faster until SOW-0009 child SOWs are complete.

### Assistant Understanding

Facts:

- Writes outside this repository are forbidden unless the user explicitly
  authorizes a specific Netdata repository target.
- SNMP traps has already been integrated externally by the user against
  `v0.3.0` / `go/v0.3.0`.
- The user reported SNMP traps improved from about 5.5k traps/s on `v0.1.0` to
  about 170k traps/s on `v0.3.0`.
- NetFlow needs both writer and reader integration.
- OTEL logs need writer integration.
- OTEL signal viewer needs reader integration.
- `systemd-journal.plugin` needs the pure reader path for builds without
  libsystemd.
- Writers should default to compact journal format in Netdata integrations.

Inferences:

- The first Netdata SOW should be an inventory/cut-plan SOW, not direct code
  integration.
- Component integration SOWs should be split to avoid mixing writer, reader,
  packaging, and vendored-removal risks.
- Dependency strategy should use a versioned SDK tag or commit, not a moving
  target, once the API is stable enough for Netdata.

Unknowns:

- Exact Netdata repository commit/branch to target.
- Exact SDK tag to use for the next Netdata integration after performance work.
- Whether Netdata integration should be performed in this thread after explicit
  authorization or handled by separate Netdata-side agents.

### Acceptance Criteria

- Inventory every Netdata journal reader and writer consumer at a specific
  Netdata commit.
- Record exact files, functions, crates/modules, and current dependencies for
  each consumer.
- Confirm SNMP traps current state as already integrated externally, without
  changing Netdata from this repository.
- Produce a cut plan for:
  - NetFlow writer and reader integration;
  - OTEL writer integration;
  - OTEL signal viewer reader integration;
  - `systemd-journal.plugin` no-libsystemd reader integration;
  - static packaging implications;
  - vendored journal implementation removal.
- Map each component to a real pending component SOW.
- Record performance prerequisites from SOW-0042 through SOW-0046.
- Record exact repository boundary and user authorization needed before any
  Netdata-side edit.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`
- `.agents/sow/current/SOW-0037-20260527-reference-drift-audit.md`
- `.agents/sow/specs/product-scope.md`
- Earlier Netdata evidence recorded from `ktsaou/netdata @ 00305266364e`.

Current state:

- This SDK repository contains the SDK work and SOW plan.
- The Netdata source tree is outside this repository and must remain read-only
  unless explicitly authorized.
- Component integration SOWs now exist as follow-ups.

Risks:

- Direct integration before performance gates could regress Netdata hot paths.
- Direct integration without a fresh inventory could miss a reader, query,
  rebuild, or packaging consumer.
- Removing vendored code too early could strand a Netdata path not yet covered
  by SDK APIs.

## Pre-Implementation Gate

Status: blocked until writer/reader performance gates and Netdata repository
authorization

Problem / root-cause model:

- Netdata has multiple journal producers and consumers. The SDK can replace
  them only after public APIs and performance are fit for production use.
- Integration planning can happen read-only, but implementation requires an
  explicit Netdata repository target decision because this project forbids
  writes outside the SDK repository.

Evidence reviewed:

- Current SOW inventory and product-scope spec.
- Earlier read-only Netdata evidence from `ktsaou/netdata @ 00305266364e`.

Affected contracts and surfaces:

- NetFlow ingestion, replay, query, and facet behavior.
- OTEL logs ingestion.
- OTEL signal viewer reading.
- systemd journal plugin fallback reading without libsystemd.
- Netdata static packaging.
- SDK versioning, Go module tags, Rust crate use, and public API stability.

Existing patterns to reuse:

- Existing Netdata `journal-log-writer` integration shape.
- SDK high-level writer APIs.
- SDK reader and `jf` facade.
- Existing Netdata lifecycle and packaging conventions discovered during the
  fresh inventory.

Risk and blast radius:

- High. This affects production ingestion, reader/query behavior, storage
  format defaults, and Netdata packaging.

Sensitive data handling plan:

- Use only source code, synthetic fixtures, and sanitized examples.
- Do not record real customer logs, SNMP communities, trap payloads, flow
  payloads, credentials, bearer tokens, private endpoints, personal data, or
  production incident details.

Implementation plan:

1. After performance gates, read the selected Netdata commit read-only.
2. Inventory all reader and writer consumers with file/function evidence.
3. Decide dependency/tag strategy with the user if not already decided.
4. Update this SOW with the cut plan.
5. Activate component integration SOWs one at a time.

Validation plan:

- SOW audit for plan updates.
- External read-only review of the inventory before integration.
- Component SOWs own Netdata build/test validation.

Artifact impact plan:

- AGENTS.md: update only if the user authorizes a cross-repo workflow.
- Runtime project skills: update only if integration workflow becomes durable.
- Specs: update product-scope integration status after inventory.
- End-user/operator docs: update only when Netdata integration changes shipped
  behavior.
- End-user/operator skills: update only if Netdata docs/spec changes affect
  output/reference skills.
- SOW lifecycle: component SOWs created and ordered from this inventory.
- SOW-status.md: updated by the restructuring commit.

Open-source reference evidence:

- `ktsaou/netdata @ 00305266364e`
  - `src/crates/netflow-plugin/src/ingest/service/init.rs`
  - `src/crates/netflow-plugin/src/ingest/service/runtime.rs`
  - `src/crates/netflow-plugin/src/query/scan/direct.rs`
  - `src/crates/netdata-otel/otel-plugin/src/logs_service.rs`
  - `src/crates/Cargo.toml`

Open decisions:

1. Netdata repository target
   - Status: blocked until the user explicitly authorizes a path/branch or
     keeps the work read-only.
2. Dependency strategy
   - Status: likely versioned SDK tag/commit, but final decision belongs in the
     cut-plan update after performance SOWs.

## Implications And Decisions

1. 2026-05-28 integration split
   - Decision: SOW-0026 becomes inventory/cut-plan only.
   - Implication: component SOWs own implementation after performance gates.
   - Risk: this delays integration, but avoids replacing fast vendored paths
     with slower or incomplete SDK paths.

## Plan

1. Wait for SOW-0042 through SOW-0046 as applicable.
2. Inventory Netdata consumers at the selected commit.
3. Produce cut plan and dependency decision.
4. Activate component SOWs one at a time.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Read-only reviewers from the approved pool for the completed inventory.

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

- Record missing SDK APIs, performance blockers, Netdata repository blockers,
  and reviewer findings in this SOW before activating component work.

## Execution Log

### 2026-05-28

- Rescoped from direct Netdata integration to integration inventory and cut
  planning after user agreement.

## Validation

Acceptance criteria evidence:

- Pending activation.

Tests or equivalent validation:

- SOW audit validates this planning change.

Real-use evidence:

- User reported SNMP traps integration performance improved to about 170k
  traps/s on `v0.3.0`; this informs the cut plan but does not replace the
  remaining reader/writer performance gates.

Reviewer findings:

- Pending activation.

Same-failure scan:

- Pending activation.

Sensitive data gate:

- This rescope records no raw secrets, credentials, bearer tokens, SNMP
  communities, customer names, personal data, non-private customer-identifying
  IPs, private endpoints, or proprietary incident details.

Artifact maintenance gate:

- AGENTS.md: no update needed for this rescope.
- Runtime project skills: no update needed for this rescope.
- Specs: no behavior change in this rescope.
- End-user/operator docs: no behavior change in this rescope.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: component SOWs created by the restructuring commit.
- SOW-status.md: updated by the restructuring commit.

Specs update:

- No spec update needed for this rescope.

Project skills update:

- No project skill update needed.

End-user/operator docs update:

- No docs update needed for this rescope.

End-user/operator skills update:

- No output/reference skill update needed.

Lessons:

- Netdata integration should be planned from a fresh inventory and then split by
  component, not implemented as one broad cut.

Follow-up mapping:

- NetFlow integration: SOW-0047.
- OTEL writer integration: SOW-0048.
- Reader plugin integrations: SOW-0049.
- Vendored journal removal: SOW-0050.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

- SOW-0047 - Netdata NetFlow SDK Integration.
- SOW-0048 - Netdata OTEL Writer SDK Integration.
- SOW-0049 - Netdata Reader Plugin SDK Integration.
- SOW-0050 - Netdata Vendored Journal Removal.

## Regression Log

None yet.
