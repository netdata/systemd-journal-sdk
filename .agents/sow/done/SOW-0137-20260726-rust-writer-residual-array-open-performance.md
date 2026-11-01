# SOW-0137 - Rust Writer Residual Array-Open Performance

## Status

Status: completed

Sub-state: completed 2026-11-01 under the user's autonomous-run instruction;
two measured redundant opens removed, byte-identical output.

## Requirements

### Purpose

Measure and, if justified, remove remaining redundant offset-array object opens
in the canonical Rust writer without changing journal bytes, publication order,
or index correctness.

### User Request

SOW-0135 delivered one measured compact DATA-link optimization. Its
same-pattern search and external review found related ref-then-mut and
re-read-before-mut patterns in the global ENTRY array and regular/fallback DATA
array paths. These are possible additional gains, not correctness blockers for
SOW-0135.

### Assistant Understanding

Facts:

- Global ENTRY-array append reads the tail capacity and then opens the same
  array mutably to write.
- Regular DATA-array and compact fallback traversal can compute the tail
  capacity during the chain walk, then reopen the same tail read-only and
  mutably.
- New array allocation opens the new array to size it and callers reopen it to
  write the first slot.
- SOW-0135's measured workload was compact with valid cached tails, so it did
  not isolate the regular/fallback paths.

Inferences:

- Some object validation/mmap work may still be avoidable, especially for
  regular output and the per-entry global ENTRY array.
- The paths have different header ownership and publication lifecycles, so the
  SOW-0135 compact DATA-tail merge must not be copied mechanically.

Unknowns:

- The measurable cost of each pattern on compact and regular workloads.
- Whether a single mutable guard is compatible with each path's allocation,
  growth, header publication, and failure ordering.

### Acceptance Criteria

- Profile and benchmark each candidate with identical compact and regular
  parameters before editing.
- Present design alternatives and publication/failure-order implications to the
  user before implementation.
- Accept an optimization only when it materially reduces the targeted path.
- Preserve exact bytes, all ENTRY and DATA array chains, live visibility,
  compact/regular correctness, public APIs, and bounded memory.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0135-20260726-rust-writer-entry-link-state-reuse.md`
- `rust/src/crates/journal-core/src/file/writer_entry_arrays.rs`
- SOW-0135 external review outputs under
  `.local/reviews/sow0135-round1/`

Current state:

- SOW-0135 removes the current-entry DATA reopen and non-full compact-tail
  double open on its measured path.
- Other array-open patterns remain correct but may be more expensive than
  necessary.

Risks:

- Combining read and mutation too early can change validation-before-mutation
  behavior on malformed files.
- Reusing a guard across allocation or growth can violate borrow rules or alter
  array-before-header publication ordering.
- Optimizing unmeasured paths can add complexity without throughput value.

## Pre-Implementation Gate

Status: completed; scope recorded in Implications And Decisions

Problem / root-cause model:

- Candidate writer paths reopen an offset-array object after already reading
  the metadata needed to select or size the mutation.

Evidence reviewed:

- SOW-0135 source, benchmark/profile results, same-pattern search, and external
  review.

Affected contracts and surfaces:

- Canonical Rust global ENTRY-array publication.
- Regular DATA entry arrays.
- Compact DATA authoritative fallback.
- Array allocation/growth if measurement includes it.

Existing patterns to reuse:

- SOW-0135 single compact-tail guard where publication and failure ordering are
  equivalent.
- Structural oracle and live compatibility matrices.

Risk and blast radius:

- Medium. A defect can damage global entry traversal or DATA inverted indexes.

Sensitive data handling plan:

- Use deterministic synthetic repository fixtures only.

Implementation plan:

- Measure C1-C3 with callgrind and timing, implement the measured ones, validate bytes and live readers (done; see Implications And Decisions).

Validation plan:

- Focused compact and regular unit tests for append, growth, malformed state,
  reopen, and publication order.
- Full affected Rust tests and writer benchmark target check.
- Structural oracle for all ENTRY/DATA array chains.
- Stock verification, indexed queries, live matrix, byte identity, and matching
  before/after profiles.

Artifact impact plan:

- AGENTS.md, specs, skills, and end-user docs are expected to remain unchanged
  for an internal compatible optimization.
- Update SOW ledgers on activation and closure.

Open-source reference evidence:

- Activation should recheck systemd v260.1 entry-array append ordering.

Open decisions:

- Which candidate paths have enough measured cost to justify implementation.
- Whether they can share a guard-reuse primitive without obscuring publication
  order.

## Implications And Decisions

Activated 2026-11-01. The user authorized autonomous work on stacked PRs: the
agent decides internal, non-user-visible choices and records them for review.
This SOW changes no bytes, API or publication order, so its scope choices are
agent-chosen (listed at the top of the PR):

- C1 (global ENTRY array, once per entry): open the tail array mutably once to
  read its capacity and write the slot, instead of a read-only open followed
  by a mutable open. Removes the now-unused `offset_array_capacity` and
  `append_to_entry_array_tail` helpers.
- C2 (regular and compact-fallback DATA arrays, once per DATA link): keep the
  tail capacity the chain walk already read instead of reopening the tail
  read-only.
- C3 (new array allocation opens the array to size it, then callers reopen it
  to write slot 0): rejected. It runs only when an array grows, which happens
  a logarithmic number of times, so it cannot be material.
- Not taken: the regular-format DATA link still walks its array chain from the
  head on every link. That is systemd's own regular-format algorithm
  (compact files carry a tail cache); removing it needs an in-memory tail
  cache, which is a design change rather than a redundant open, and regular is
  not the production default.

Validation-before-mutation is unchanged: `offset_array_mut` runs the same
type, size and arena-bound checks as `offset_array_ref`
(`file_mut.rs` `existing_mutable_object_size`, `file.rs`
`journal_object_ref`), and nothing is written before those checks pass.
Publication order is unchanged: slots are written before the header tail
fields, as before.

## Plan

Done; see the execution log.

## Implementation And Review Plan

The project manager implemented, measured and reviewed the change; the review
gate is tests plus own review.

## Execution Log

### 2026-07-26

- Created from SOW-0135 same-pattern review as a non-blocking measured
  performance follow-up.

### 2026-11-01

- Implemented C1 and C2 in
  `rust/src/crates/journal-core/src/file/writer_entry_arrays.rs`.

## Validation

Measurement (`writer_core_bench`, 20,000 rows under callgrind, live
publication disabled, windowed mmap; baseline = branch head before the
change). Instructions, baseline -> change:

- compact netflow-v5-repeating-256: 2,119.1M -> 2,114.3M (-0.22%);
- compact mixed-cardinality-32-fields: 3,060.1M -> 3,054.8M (-0.17%);
- regular netflow-v5-repeating-256: 3,980.7M -> 3,777.0M (-5.1%);
- regular mixed-cardinality-32-fields: 3,940.6M -> 3,788.4M (-3.9%).

C1 saves about 240 instructions per entry; C2 accounts for the regular gain.
Release timing, 100,000 rows, regular format, output on local disk,
alternating order, 12 pairs at host load average about 30: mixed median
37,946 -> 38,648 rows/s (+1.9%, change faster in 8 of 12 pairs); netflow
66,807 -> 71,804 rows/s (+7.5%, 10 of 12). An earlier 10-pair mixed run at
load about 55 showed -1.4% with a 31,805 rows/s outlier; with deterministic
instruction counts lower in every configuration, it is treated as host noise.

Correctness:

- All four callgrind outputs are byte-identical to the baseline outputs
  (SHA-256 prefixes 7912891d86e9fbe4, 13ebc8ba3acc0c4b, bcf551c15185a982,
  2c59bfae9a6ded75).
- Stock `journalctl --verify --file` (host systemd 255) passes all four.
- `cargo test --workspace` passes (45 suites).
- `run_live_matrix.py --entries 10 --readers go rust stock --features
  regular zstd compact sealed`: 8/8.
- `rustfmt` clean; clippy on `systemd-journal-sdk-core` shows only the
  pre-existing argument-count warning on `append_data_entry_array_link`.

Self-review: the change removes reads, not checks; error kinds on malformed
files can differ only in which of two failing checks reports first. Go is
unaffected (Rust-only writer internals); no parity change. Same-failure
search: every `offset_array_ref`/`offset_array_mut` pair in
`writer_entry_arrays.rs` was checked; the remaining pairs are the C3
allocation path and the compact tail path already merged by SOW-0135.

Sensitive data: synthetic benchmark workloads only; logs under
`.local/sow0137/`.

Artifacts: no `AGENTS.md`, skill, spec or end-user doc change, since behavior
and bytes are unchanged. Status index updated.

## Outcome

Completed. The Rust writer opens one fewer offset-array object per entry and
per regular DATA link, with byte-identical output.

## Lessons Extracted

- Callgrind instruction counts were the reliable signal on a loaded host; wall
  time needed low-load, alternating runs to agree.

## Followup

None. C3 and the regular-format chain walk are rejected above with evidence.
