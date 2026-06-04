# SOW-0088 - Rust Offset Array Cursor Cache

## Status

Status: open

Sub-state: pending after SOW-0086 identified offset-array cursor hot-path costs.

## Requirements

### Purpose

Optimize Rust reader row stepping by caching offset-array cursor node state and
avoiding repeated offset-array object reads during forward and reverse
traversal.

### User Request

The user requires every branch, calculation, data access, and mmap access in the
Rust reader hot path to have a strong reason to exist.

### Acceptance Criteria

- Forward cursor movement inside one offset-array node does not rebuild or
  reread that node for every value access.
- Reverse cursor movement avoids repeated head-to-current list walks at node
  boundaries.
- Correct ordering remains compatible with systemd journal traversal.
- SOW-0086 benchmark candidates include before/after row-stepping and payload
  traversal deltas.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- SOW-0086 found that offset-array cursor movement repeatedly rebuilds node
  state and can walk the list on reverse node boundaries.

Evidence reviewed:

- SOW-0086 findings on `rust/src/crates/journal-core/src/file/offset_array.rs`.

Affected contracts and surfaces:

- Rust single-file and directory row traversal.

Existing patterns to reuse:

- Existing offset-array validation and cursor tests.

Risk and blast radius:

- Medium: cursor traversal is central, but the change should be internal.

Sensitive data handling plan:

- Use generated fixtures and sanitized benchmark labels only.

Implementation plan:

1. Add cursor node metadata caching.
2. Add or reuse reverse-node lookup state.
3. Validate forward/reverse traversal, seek, and directory ordering.

Validation plan:

- Rust tests, reader benchmark matrix, cursor conformance, `git diff --check`,
  SOW audit, and whole-SOW read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected.
- Specs: update Rust reader performance spec if cursor cache rules become
  durable.
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
