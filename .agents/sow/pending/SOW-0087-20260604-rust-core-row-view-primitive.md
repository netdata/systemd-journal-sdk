# SOW-0087 - Rust Core Row View Primitive

## Status

Status: open

Sub-state: pending after SOW-0086 identified remaining Rust reader hot-path
ownership gaps.

## Requirements

### Purpose

Create one Rust core current-row owner for reader hot paths so performance rules
are enforced below `FileReader`, facade, index, and engine consumers.

### User Request

The user requires Rust reader performance work to satisfy cached header state,
rolling mmap, row-level mmap-backed uncompressed DATA pointers, compressed
current-row arena storage, row-level validity, and zero uncompressed hot-path
allocations.

### Assistant Understanding

Facts:

- SOW-0086 implemented row-pinned DATA enumeration in `FileReader`.
- SOW-0086 did not fully remove split row ownership between `JournalReader` and
  `FileReader`.

Inferences:

- The next clean step is a lower `journal-core` row-view primitive that owns row
  metadata, DATA offsets, row-pinned mmap windows, and compressed arena state.

Unknowns:

- The exact public Rust type shape required to let `FileReader`, facade, index,
  and engine share this primitive without API churn.

### Acceptance Criteria

- A `journal-core` row-view primitive owns current-row metadata, DATA offsets,
  row pins, and compressed row arena state.
- `FileReader` and facade DATA enumeration use the row-view primitive directly.
- Current-row ENTRY rereads for per-field enumeration are removed from the hot
  path.
- Uncompressed row enumeration has no steady-state allocations after warmup.
- SOW-0086 benchmark candidates are rerun and compared with the SOW-0086 final
  benchmark.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0086-20260604-rust-reader-performance-contract-gap-analysis.md`

Current state:

- Row state is still split across core and SDK layers.

Risks:

- This is a central reader refactor and can affect facade, directory, and
  file-backed journalctl behavior if not tested broadly.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- Multiple Rust layers own row traversal details, causing duplicated ENTRY
  offset collection and inconsistent lifetime/performance guarantees.

Evidence reviewed:

- SOW-0086 findings and implementation results.

Affected contracts and surfaces:

- Rust `journal-core`, `journal`, facade, file-backed journalctl readers, and
  reader benchmarks.

Existing patterns to reuse:

- SOW-0086 row-pinned mmap helpers and compressed row arena behavior.

Risk and blast radius:

- High for Rust reader internals; medium for public API if new borrow shapes are
  needed.

Sensitive data handling plan:

- Use generated fixtures and sanitized benchmark labels only.

Implementation plan:

1. Design the row-view primitive and public/internal borrow contract.
2. Port `FileReader` and facade row data enumeration to it.
3. Remove duplicated ENTRY offset collection where the primitive replaces it.
4. Add pointer-provenance, lifetime, and allocation tests.

Validation plan:

- Rust tests, SOW-0086 benchmark matrix, `git diff --check`, SOW audit, and
  whole-SOW read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected.
- Specs: update Rust reader performance spec if the row-view contract changes.
- End-user/operator docs: likely unaffected.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: normal pending/current/done flow.
- SOW-status.md: update when state changes.

Open-source reference evidence:

- Not yet checked.

Open decisions:

- User must approve activating this SOW and any public API shape changes.

## Outcome

Pending.
