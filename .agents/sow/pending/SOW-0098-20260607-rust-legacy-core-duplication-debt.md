# SOW-0098 - Rust Legacy Core Duplication Debt

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: opened from SOW-0096 Codacy file metrics audit; refreshed on
2026-06-15 with current local/Codacy evidence and read-only subagent analysis;
blocked on user priority decision before implementation.

## Requirements

### Purpose

Reduce Rust duplication between the legacy `jf` compatibility implementation
and the current `journal-core` implementation only where doing so preserves the
historical-reader compatibility contract, performance contract, and Netdata
integration path.

### User Request

The user asked for Codacy file-by-file Rust/Go complexity and duplication
analysis. SOW-0096 found that the largest Rust production duplication is real
overlap between legacy `jf` code and `journal-core`.

On 2026-06-15, the user asked to check whether this SOW was still valid, then
to update it with current evidence, concrete solution candidates, estimated size
or metric improvement, and refactor risks from a read-only subagent analysis.

### Assistant Understanding

Facts:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md` classifies Rust
  duplication using Codacy file metrics plus local Lizard max function CCN. The
  spec is a 2026-06-07 point-in-time snapshot and must not be used as activation
  evidence without refresh.
- The 2026-06-15 refresh checked Codacy branch `master`, fetched at
  `2026-06-15T17:28:26.099Z`, with `222` Rust/Go files in the export.
- Current strongest Rust production duplication targets are:
  - `rust/src/crates/journal-core/src/file/offset_array.rs`: `912` lines,
    local Lizard `48` functions, sum CCN `165`, max CCN `11`, Codacy
    duplication `662`, grade `F`.
  - `rust/src/crates/jf/journal_file/src/offset_array.rs`: `716` lines,
    local Lizard `41` functions, sum CCN `132`, max CCN `11`, Codacy
    duplication `600`, grade `F`.
  - `rust/src/crates/jf/journal_file/src/journal_file.rs`: `829` lines,
    local Lizard `40` functions, sum CCN `109`, max CCN `11`, Codacy
    duplication `410`, grade `F`.
  - `rust/src/crates/jf/journal_file/src/file.rs`: `912` lines, local Lizard
    `55` functions, sum CCN `126`, max CCN `10`, Codacy duplication `383`,
    grade `F`.
- The old target `rust/src/crates/journal-core/src/file/file.rs` is stale as a
  duplication target: current Codacy reports duplication `0`, grade `B`, with
  local Lizard sum CCN `170` and max CCN `11`.
- The `jf` crate is historically important because it is the compatibility
  layer for a libsystemd-like reader API and historical journal support.
- `rust/src/crates/jf/journal_file/src/journal_file.rs` appears to be tracked
  but uncompiled legacy source: `rust/src/crates/jf/journal_file/src/lib.rs:1`
  declares `cursor`, `file`, `filter`, `offset_array`, `reader`, and `writer`,
  but not `journal_file`.
- `rust/src/crates/jf/journal_reader_ffi/src/lib.rs:1` imports the legacy
  `journal_file` crate API directly, and `rust/src/crates/jf/journal_reader_ffi/src/lib.rs:98`
  stores `JournalFile<Mmap>` and `JournalReader`, so live `jf` API shape still
  matters for FFI compatibility.
- Core and legacy filter semantics currently differ on missing matches:
  `rust/src/crates/journal-core/src/file/filter.rs:182` returns
  `FilterExpr::None`, while `rust/src/crates/jf/journal_file/src/filter.rs:340`
  and `rust/src/crates/jf/journal_file/src/filter.rs:352` return
  `JournalError::InvalidOffset`.

Inferences:

- The live Rust duplication is not scanner noise. The offset-array, cursor, and
  filter overlaps represent real architectural debt created while preserving
  battle-tested `jf` behavior and building the current core SDK.
- Some reported debt may be removable without runtime behavior change:
  `journal_file.rs` appears uncompiled, but deleting a tracked source file still
  requires explicit user approval and a same-failure search.
- Any live-path deduplication must be compatibility-first and benchmark-backed.
  Redirecting legacy paths to newer primitives may be correct, but only if
  historical fixtures, FFI behavior, facade behavior, mmap lifetimes, and reader
  performance stay intact.
- Moving broad object/compression/zerocopy logic into `journal-common` would
  change that crate's dependency footprint; a new private format-helper crate
  may be safer if shared lower-level code grows beyond small pure helpers.

Unknowns:

- Which duplicated primitives can be safely shared without changing `jf`
  semantics.
- Whether sharing lower-level primitives affects row-lifetime guarantees,
  rolling mmap behavior, or historical compatibility.
- Whether the uncompiled `journal_file.rs` file should be deleted, archived in
  SOW evidence, or left until legacy removal work.
- Whether legacy missing-match filter behavior should remain
  `InvalidOffset` or adopt core `FilterExpr::None` behavior.
- Whether shared Rust primitives belong in `journal-common` or a new private
  format-helper crate.

### Acceptance Criteria

- User-approved Rust duplication target list and migration direction are
  recorded before implementation.
- Compatibility tests covering `jf`, `journal-core`, facade APIs, historical
  fixtures, and real-corpus representative files pass.
- Reader performance benchmarks prove no regression in the hot paths touched.
- Codacy file metrics are rechecked after push and compared against SOW-0096
  baseline.

## Analysis

Sources checked:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md`.
- SOW-0086 through SOW-0092 Rust reader performance work.
- SOW-0027 Netdata reader API and `jf` facade status.
- `.local/codacy-validity/current-file-metrics-rust-go.json` as refreshed
  2026-06-15 Codacy evidence.
- `.local/codacy-validity/lizard-sow-0097-0098.csv` as refreshed 2026-06-15
  local Lizard evidence.
- Read-only explorer subagent `019ecc61-3293-7ac1-938d-972a80ae9722`
  completed on 2026-06-15 and returned concrete Rust refactor candidates,
  estimated metric movement, risks, and validation scope.

Current state:

- The largest production duplication is Rust legacy/core overlap.
- The current Rust reader performance work recently optimized `journal-core`
  hot paths; any deduplication must not undo those guarantees.
- The highest-value live duplication target is the `offset_array.rs` pair.
- `rust/src/crates/jf/journal_file/src/journal_file.rs` appears to be
  uncompiled legacy source and is therefore a possible metric-only cleanup, not
  evidence of live runtime duplication by itself.
- `rust/src/crates/journal-core/src/file/file.rs` should be removed from this
  SOW's current first-target list because refreshed Codacy duplication is `0`.

Risks:

- Consolidating `jf` and `journal-core` too aggressively can break historical
  journal compatibility.
- Moving legacy code onto current primitives can accidentally change error
  behavior, mmap lifetime guarantees, or compressed DATA handling.
- Metric-driven deduplication without performance proof can make the SDK worse.
- Deleting or moving tracked legacy source can remove useful forensic context
  unless the decision and evidence are recorded first.
- Sharing filter code without a user-approved missing-match policy can silently
  change query/facade behavior.
- Moving broad format/object helpers into `journal-common` can bloat a currently
  lightweight crate and expand dependency surface.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- Codacy duplication is high because legacy `jf` and current `journal-core`
  contain overlapping implementations of journal file/object/offset-array,
  cursor, and filter mechanics. Current evidence narrows the first live target
  to offset-array traversal and marks `journal-core/src/file/file.rs` as stale
  for duplication.

Evidence reviewed:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md`: historical Rust top
  duplication table and file-by-file classifications from 2026-06-07.
- `.local/codacy-validity/current-file-metrics-rust-go.json`: refreshed
  2026-06-15 Codacy export.
- `.local/codacy-validity/lizard-sow-0097-0098.csv`: refreshed local Lizard
  function metrics for the target files.
- Read-only explorer subagent `019ecc61-3293-7ac1-938d-972a80ae9722`.

Affected contracts and surfaces:

- Rust `journal-core` reader/writer behavior.
- Rust `jf` compatibility layer.
- Libsystemd-like facade behavior.
- Historical journal file compatibility.
- Reader hot-path performance contract.

Existing patterns to reuse:

- Rust row-view and mmap lifetime architecture from SOW-0086 through SOW-0092.
- Existing shared `journal-common` crate for truly common primitives.

Concrete solution candidates:

1. Surgical decision item: resolve uncompiled legacy `journal_file.rs`.
   - Scope: `rust/src/crates/jf/journal_file/src/journal_file.rs`.
   - Evidence: `rust/src/crates/jf/journal_file/src/lib.rs:1-35` does not
     declare a `journal_file` module.
   - Action options to decide before implementation: delete with explicit user
     approval, preserve useful notes in SOW evidence before deletion, or keep
     until broader legacy removal work.
   - Estimated improvement if deleted: remove `829` tracked lines, about `410`
     Codacy duplication points, and about `27` clones. This is a metric cleanup
     and dead-source cleanup, not live architecture deduplication.
2. Surgical: extract small pure file-format helpers before changing behavior.
   - Candidate scopes: `map_hash_table`, `sanitize_header_for_size`,
     `BucketUtilization`, and iterator skeletons shared by
     `rust/src/crates/jf/journal_file/src/file.rs` and
     `rust/src/crates/journal-core/src/file/file.rs`.
   - Estimated improvement: `120-250` lines moved/shared and `80-180`
     duplication points removed. Lower risk, but `journal-core/src/file/file.rs`
     currently has duplication `0`, so Codacy movement may be limited.
3. Long-term-best: unify offset-array traversal as a shared state machine.
   - Scope: `Node`, `List`, `Cursor`, and `InlinedCursor` in
     `rust/src/crates/jf/journal_file/src/offset_array.rs:12-716` and
     `rust/src/crates/journal-core/src/file/offset_array.rs:13-912`.
   - Required design: a generic access trait or private shared helper must
     preserve core cache fields and behavior, including node-chain and cached
     value paths from `journal-core`.
   - Estimated improvement: `600-800` lines moved/split and `450-650`
     duplication points removed per affected offset-array file. This is the
     biggest live production duplication win.
4. Conditional surgical follow-up: share cursor stepping after offset-array
   consolidation.
   - Scope: `rust/src/crates/jf/journal_file/src/cursor.rs` and
     `rust/src/crates/journal-core/src/file/cursor.rs`.
   - Required design: preserve core materialized-value behavior while sharing
     location/cursor resolution logic.
   - Estimated improvement: `220-280` lines moved/shared and `220-300`
     duplication points removed.
5. Long-term-best: refactor filter builder/evaluator only with an explicit
   compatibility policy.
   - Scope: `rust/src/crates/jf/journal_file/src/filter.rs` and
     `rust/src/crates/journal-core/src/file/filter.rs`.
   - Open decision: keep legacy `InvalidOffset` behavior for missing matches or
     adopt core `FilterExpr::None`.
   - Estimated improvement: `180-300` lines moved/shared and `120-220`
     duplication points removed, depending on the chosen compatibility policy.
6. Split or defer: shared object layout/parsing.
   - Scope: `rust/src/crates/jf/journal_file/src/object.rs`,
     `rust/src/crates/journal-core/src/file/object.rs`, and related object hash
     helpers.
   - Estimated improvement: `700-1000` lines moved/shared and `250-450`
     duplication points removed, but with high compatibility, compression, and
     dependency-footprint risk. This should be a later phase or separate SOW
     unless the user explicitly chooses a larger architecture cleanup.

Risk and blast radius:

- High for reader compatibility and performance.
- Medium for public APIs if deduplication is limited to internal helpers.
- Low for uncompiled-source cleanup only if same-failure search confirms no
  module inclusion and the user explicitly approves deletion.
- Medium-to-high for offset-array and cursor sharing because `journal-core`
  contains newer hot-path cache behavior that must not regress.
- High for filter sharing until the missing-match behavior policy is decided.

Sensitive data handling plan:

- Do not commit raw Codacy API exports or real-corpus payloads. Durable
  artifacts may include file paths, numeric metrics, sanitized counts, and
  benchmark summaries only.

Implementation plan:

1. Ask the user to approve whether Rust duplication reduction should happen
   before Netdata integration.
2. Ask the user to decide the uncompiled `journal_file.rs` disposition before
   any deletion.
3. Analyze duplicated code clusters and identify shareable primitives versus
   intentionally divergent compatibility logic.
4. Prefer low-risk helper extraction before live hot-path consolidation.
5. Refactor one cluster at a time with historical fixture and benchmark proof.

Validation plan:

- `cargo test --manifest-path rust/Cargo.toml -p journal_file -p journal_reader_ffi -p systemd-journal-sdk-core -p systemd-journal-sdk --target-dir .local/cargo-target`.
- Rust tests for affected crates.
- Shared conformance and interoperability tests for reader/facade paths.
- Historical fixture validation from existing SOW harnesses.
- Reader benchmark comparison against SOW-0092/SOW-0093 baselines where hot
  paths are affected.
- `python3 -m pytest tests/runtime_purity/test_core_runtime_purity.py`.
- `python3 tests/interoperability/run_journalctl_query_matrix.py`.
- `python3 tests/interoperability/run_directory_matrix.py --readers rust`.
- `python3 tests/interoperability/run_verify_matrix.py`.
- `python3 tests/benchmarks/run_reader_core_benchmarks.py --languages rust,systemd --rows 100000 --repetitions 3` for offset/cursor/filter hot paths.
- `python3 tests/interoperability/run_compression_matrix.py` and
  `python3 tests/interoperability/run_mixed_directory_matrix.py` if object,
  DATA payload, compression, or mixed-directory behavior changes.
- Codacy file metrics export after push.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: no expected update unless a new duplication-review
  rule is established.
- Specs: update if Rust public/internal compatibility contracts change.
- End-user/operator docs: update only if APIs change.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: complete this SOW after implementation, review, validation,
  and remote Codacy evidence.
- SOW-status.md: update on activation and close.

Open-source reference evidence:

- None checked yet. This SOW is pending and blocked on user priority.

Open decisions:

1. Whether to reduce Rust legacy/core duplication before Netdata integration.
2. Whether `jf` should remain structurally separate until Netdata vendored code
   removal is complete.
3. Whether uncompiled legacy `journal_file.rs` should be deleted, archived into
   SOW evidence then deleted, or kept until broader legacy removal work.
4. Whether legacy filter missing-match behavior remains `InvalidOffset` or
   adopts core `FilterExpr::None`.
5. Whether shared Rust primitives belong in `journal-common` or a new private
   format-helper crate.

## Implications And Decisions

User decision on 2026-06-15:

- Refresh this pending SOW with current evidence and read-only subagent
  findings.
- Record concrete solution candidates, estimated size or metric improvement,
  and refactor risks.
- No implementation, deletion, API change, behavior change, or commit was
  approved by this decision.

## Plan

1. Decide priority and safety constraints.
2. Decide the uncompiled `journal_file.rs` disposition.
3. Identify duplicated clusters and accepted sharing boundaries.
4. Prefer small pure-helper extraction before live hot-path consolidation.
5. Refactor one cluster, validate compatibility/performance, and recheck
   Codacy metrics.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Run the approved reviewer pool after the complete SOW implementation and
  local validation.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- Do not make changes outside this repository for any reason.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- If compatibility or performance risk exceeds metric value, record evidence
  and ask the user before continuing.

## Execution Log

2026-06-15:

- Refreshed current Rust metric evidence from local Codacy export and Lizard
  output under `.local/codacy-validity/`.
- Ran read-only explorer subagent
  `019ecc61-3293-7ac1-938d-972a80ae9722` for Rust SOW analysis.
- Updated this SOW with current evidence, solution candidates, estimated
  improvement, and risks.

## Validation

Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

Pending.
