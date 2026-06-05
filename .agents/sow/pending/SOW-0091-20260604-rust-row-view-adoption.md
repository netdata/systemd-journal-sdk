# SOW-0091 - Rust Row View Adoption

## Status

Status: open

Sub-state: pending and dependent on SOW-0087.

## Requirements

### Purpose

Adopt the future Rust core row-view primitive across directory reading,
`FileReader` callback payload traversal, `journal-engine`, and
`journal-index` so performance fixes do not remain limited to the
single-payload facade enumeration path.

### User Request

The user requires no duplicated hot-path row extraction, parsing,
decompression, or allocation logic when a shared Rust reader primitive can serve
the same purpose.

### Acceptance Criteria

- Directory reader payload access delegates to the shared row-view primitive.
- `FileReader::visit_entry_payloads()` either uses the row-pinned row-view
  payload path or records benchmark evidence proving the transient visitor path
  is intentionally faster for its contract.
- `journal-engine` projected field extraction delegates to the shared row-view
  primitive or records why its path is intentionally separate and faster.
- `journal-index` DATA extraction/parsing delegates to the shared byte-oriented
  primitive or records why its path is intentionally separate and faster.
- Existing Netdata-style benchmark candidates and query/index tests are rerun.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- SOW-0086 found duplicated row extraction/parsing across directory, engine,
  index, and callback payload paths. This cannot be cleanly fixed until
  SOW-0087 creates the lower row-view primitive.

Evidence reviewed:

- SOW-0086 separation, ownership, and duplication audit.

Affected contracts and surfaces:

- Rust directory reader, indexed query engine, journal indexer, and future
  explorer API work.

Existing patterns to reuse:

- Future SOW-0087 row-view primitive.

Risk and blast radius:

- High: this touches multiple Rust crates and could affect query/index
  correctness and performance.

Sensitive data handling plan:

- Use generated fixtures and sanitized benchmark/query labels only.

Implementation plan:

1. Wait for SOW-0087 completion.
2. Replace duplicated row extraction in directory, `visit_entry_payloads()`,
   engine, and index surfaces.
3. Benchmark each replaced path independently.

Validation plan:

- Rust tests, query/index tests, reader benchmarks, SOW-0086 candidate matrix,
  `git diff --check`, SOW audit, and whole-SOW read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected.
- Specs: update Rust reader performance spec if shared primitive adoption
  changes durable guarantees.
- End-user/operator docs: likely unaffected unless public query API behavior
  changes.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: blocked until SOW-0087 completes.
- SOW-status.md: update when state changes.

Open-source reference evidence:

- Not yet checked.

Open decisions:

- User must approve activating this SOW after SOW-0087.

## Outcome

Pending.
