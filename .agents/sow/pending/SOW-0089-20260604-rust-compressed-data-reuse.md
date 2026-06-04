# SOW-0089 - Rust Compressed DATA Reuse

## Status

Status: open

Sub-state: pending after SOW-0086 fixed Zstandard decompression speed but left
repeated compressed DATA reuse unoptimized.

## Requirements

### Purpose

Measure and, only if beneficial, implement Rust reader reuse for compressed
DATA decompression results and native Zstandard decompressor context state.

### User Request

The user requires reusable journal DATA objects not to be repeatedly parsed,
decompressed, hashed, sorted, or copied when a cache can preserve the same
result without weakening correctness.

### Acceptance Criteria

- Real-corpus compressed DATA reuse frequency is measured.
- Native Zstandard context creation cost is profiled.
- A bounded row/query/file cache is implemented only if measured benefit exceeds
  lookup and memory cost.
- If caching is rejected, the SOW records benchmark evidence and leaves no
  speculative cache.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- SOW-0086 shows native Zstandard decompression fixed the measured red-flag
  case, but repeated compressed DATA offsets and decompressor context creation
  may still waste CPU.

Evidence reviewed:

- SOW-0086 benchmark and perf findings.

Affected contracts and surfaces:

- Rust compressed DATA reader paths.

Existing patterns to reuse:

- SOW-0086 native Zstandard fast path and compressed current-row arena.

Risk and blast radius:

- Medium: cache lookup overhead can make performance worse if hit rate or
  payload size does not justify it.

Sensitive data handling plan:

- Use aggregate reuse counts and benchmark rates only; do not record raw
  payloads.

Implementation plan:

1. Profile compressed DATA reuse and Zstandard context creation cost.
2. Prototype bounded reuse strategies under measurement.
3. Keep only strategies that improve real benchmark candidates.

Validation plan:

- Rust tests, reader benchmark matrix, compressed real-corpus profiles,
  `git diff --check`, SOW audit, and whole-SOW read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected.
- Specs: update Rust reader performance spec with accepted cache semantics if
  implemented.
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
