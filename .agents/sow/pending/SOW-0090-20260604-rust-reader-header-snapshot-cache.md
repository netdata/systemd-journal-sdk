# SOW-0090 - Rust Reader Header Snapshot Cache

## Status

Status: open

Sub-state: pending after SOW-0086 fixed facade metadata materialization but did
not fully centralize header/snapshot state.

## Requirements

### Purpose

Centralize Rust reader header and snapshot state so hot paths do not reread or
rematerialize immutable header fields during snapshot traversal.

### User Request

The user requires Rust readers to cache the file header and avoid unnecessary
data access in the hot path.

### Acceptance Criteria

- Snapshot readers cache immutable header fields needed by hot row traversal,
  cursor formatting, payload context, and directory ordering.
- Live-reader refresh boundaries are explicit and benchmarked.
- Facade, `FileReader`, and directory reader metadata calls share the cached
  state instead of independently reading/rematerializing header data.
- SOW-0086 benchmark candidates are rerun.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- SOW-0086 found header access in multiple layers. The first implementation
  batch fixed facade metadata materialization, but broader header ownership
  remains unclear.

Evidence reviewed:

- SOW-0086 findings and implementation results.

Affected contracts and surfaces:

- Rust reader metadata, cursor formatting, directory ordering, and live/snapshot
  behavior.

Existing patterns to reuse:

- Existing `DirectoryEntryKey`, `FileHeader`, and reader options.

Risk and blast radius:

- Medium: live readers must not cache stale mutable tail data beyond the stated
  bounds contract.

Sensitive data handling plan:

- No raw journal payloads in durable artifacts.

Implementation plan:

1. Identify immutable versus live-refresh header fields.
2. Add cached snapshot header state at the correct layer.
3. Route metadata callers through the cache.
4. Validate snapshot and live behavior separately.

Validation plan:

- Rust tests, live/snapshot reader tests, SOW-0086 benchmark matrix,
  `git diff --check`, SOW audit, and whole-SOW read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected.
- Specs: update Rust reader performance spec if live/cache rules change.
- End-user/operator docs: likely unaffected.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: normal pending/current/done flow.
- SOW-status.md: update when state changes.

Open-source reference evidence:

- Not yet checked.

Open decisions:

- User must approve activating this SOW.

## Outcome

Pending.
