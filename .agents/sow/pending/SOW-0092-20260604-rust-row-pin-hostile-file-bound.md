# SOW-0092 - Rust Row Pin Hostile File Bound

## Status

Status: open

Sub-state: pending follow-up from SOW-0086 reviewer findings.

## Requirements

### Purpose

Keep the Rust reader's row-level mmap-backed zero-copy contract fast for normal
production journals while bounding virtual-memory growth for hostile, corrupt,
or deliberately pathological journal files.

### User Request

The user requires top Rust reader performance. SOW-0086 introduced row-pinned
rolling mmap windows so uncompressed current-row payloads can stay borrowed from
mmap-backed DATA objects until the reader leaves the row. Reviewers identified
that a single pathological row can reference DATA objects spread across many
windows and temporarily exceed the normal rolling-window cache.

### Acceptance Criteria

- The Rust reader has a measured and documented per-row bound for row-pinned
  mmap windows or row-pinned mapped bytes.
- If the bound is exceeded, the reader preserves correctness by falling back to
  copying uncompressed DATA into the current-row arena or another bounded
  current-row buffer.
- Normal production benchmark candidates from SOW-0086 do not regress beyond
  measured noise unless the SOW records a user-approved tradeoff.
- A synthetic hostile-file test proves memory remains bounded when one entry
  references DATA objects spread across many mmap windows.
- The row-level validity guarantee remains true for borrowed and copied
  payloads.

## Analysis

Sources checked:

- SOW-0086 implementation and reviewer findings.
- `rust/src/crates/journal-core/src/file/mmap.rs` row-pinned window logic.
- `.agents/sow/specs/rust-reader-performance.md` row-level lifetime contract.

Current state:

- SOW-0086 allows row-pinned windows to exceed the steady-state window-cache
  limit for one current row.
- That is required for zero-copy row-level payload validity with rolling mmaps.
- The current implementation does not define a hard per-row cap.

Risks:

- A malicious or corrupt journal can force excessive transient mappings by
  placing current-row DATA objects far apart.
- A naive cap can silently break row-level pointer validity or add copies to the
  normal hot path.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- The row-pinned mmap contract protects borrowed uncompressed DATA payloads by
  keeping backing windows mapped until the reader leaves the row. A row with
  widely scattered DATA objects can pin many windows before the row ends.

Evidence reviewed:

- SOW-0086 reviewer finding on unbounded per-row pinned-window growth.
- `rust/src/crates/journal-core/src/file/mmap.rs` row-pinned window creation and
  eviction code.

Affected contracts and surfaces:

- Rust `FileReader` row payload enumeration.
- Rust facade DATA enumeration.
- Rust reader performance spec.
- Hostile/corrupt journal behavior.

Existing patterns to reuse:

- SOW-0086 current-row arena for compressed DATA.
- SOW-0086 row-pinned mmap window lifetime tests.

Risk and blast radius:

- Medium. The change touches mmap lifetime and fallback ownership behavior.
- The main risk is accidentally adding copies to normal uncompressed production
  traversal.

Sensitive data handling plan:

- Use generated hostile fixtures only. Do not record real journal payloads in
  durable artifacts.

Implementation plan:

1. Measure realistic row-pinned window counts on SOW-0086 real and generated
   benchmark candidates.
2. Choose a cap based on evidence and record the tradeoff.
3. Add a fallback path that copies only payloads that would exceed the cap.
4. Add hostile-file tests and rerun SOW-0086 reader benchmarks.

Validation plan:

- Rust tests for normal row-pinned borrowing and hostile fallback.
- SOW-0086 benchmark candidates before/after.
- `git diff --check`, SOW audit, and whole-SOW read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected.
- Specs: update `.agents/sow/specs/rust-reader-performance.md` with the final
  per-row cap and fallback behavior.
- End-user/operator docs: likely unaffected.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: pending follow-up from SOW-0086.
- SOW-status.md: update when state changes.

Open-source reference evidence:

- Not yet checked; this is an internal SDK hardening SOW.

Open decisions:

- The cap value must be evidence-based and recorded before implementation.

## Outcome

Pending.
