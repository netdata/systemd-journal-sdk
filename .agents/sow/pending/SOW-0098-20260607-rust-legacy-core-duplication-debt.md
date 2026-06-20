# SOW-0098 - Rust Legacy Core Duplication Debt

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: opened from SOW-0096 Codacy file metrics audit; refreshed again on
2026-06-21 with current local code evidence, Lizard metrics, and Codacy Cloud
repository-level evidence. Still open, but narrowed to a decision on
uncompiled legacy source plus explicitly high-risk Rust legacy/core
deduplication; blocked on user priority decision before implementation.

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
On 2026-06-21, the user asked to refresh SOW-0097 and SOW-0098 again and show
what remains to be done.

### Assistant Understanding

Facts:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md` classifies Rust
  duplication using Codacy file metrics plus local Lizard max function CCN. The
  spec is a 2026-06-07 point-in-time snapshot and must not be used as activation
  evidence without refresh.
- The 2026-06-15 refresh checked Codacy branch `master`, fetched at
  `2026-06-15T17:28:26.099Z`, with `222` Rust/Go files in the file-metric
  export. Treat those per-file Codacy grades as a historical baseline, not as
  current activation evidence.
- Codacy Cloud repository refresh on 2026-06-21 reports default branch
  `master`, last analysed commit `c9f5caac804f` with analysis ending
  `2026-06-19T06:57:32.973Z`, `27` open issue rows, coverage `72%`, complex
  files `18%`, and duplication `30%`. The current Cloud issue set is unrelated
  to this refactor SOW: it is the approved Go directive SCA cluster plus
  markdownlint rows.
- Current local Rust target evidence from 2026-06-21:
  - `rust/src/crates/journal-core/src/file/offset_array.rs`: `912` lines,
    Lizard `744` NLOC, `48` functions, average CCN `3.4`.
  - `rust/src/crates/jf/journal_file/src/offset_array.rs`: `716` lines,
    Lizard `573` NLOC, `41` functions, average CCN `3.2`.
  - `rust/src/crates/jf/journal_file/src/journal_file.rs`: `829` lines,
    Lizard `598` NLOC, `40` functions, average CCN `2.7`.
  - `rust/src/crates/jf/journal_file/src/file.rs`: `912` lines, Lizard `733`
    NLOC, `55` functions, average CCN `2.3`.
  - `rust/src/crates/journal-core/src/file/file.rs`: `1000` lines, Lizard
    `793` NLOC, `83` functions, average CCN `2.0`.
- Current local Lizard did not report targeted Rust threshold failures. This
  SOW is about duplication and ownership risk, not urgent function CCN.
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
- Offset-array duplication is still live and concrete:
  `Node`, `List`, `Cursor`, and `InlinedCursor` exist in both
  `rust/src/crates/journal-core/src/file/offset_array.rs` and
  `rust/src/crates/jf/journal_file/src/offset_array.rs`.
- Core and legacy filter semantics currently differ on missing matches:
  `rust/src/crates/journal-core/src/file/filter.rs:182` returns
  `FilterExpr::None`, while `rust/src/crates/jf/journal_file/src/filter.rs:340`
  and `rust/src/crates/jf/journal_file/src/filter.rs:352` return
  `JournalError::InvalidOffset`.
- Smaller duplicated helpers still exist in live code, including
  `map_hash_table`, `sanitize_header_for_size`, and `BucketUtilization` across
  the legacy and core `file.rs` implementations.

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
- Whether sharing lower-level primitives affects row-lifetime guarantees, mmap
  behavior, or historical compatibility.
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
  2026-06-15 Codacy file-metric baseline evidence.
- `.local/codacy-validity/lizard-sow-0097-0098.csv` as refreshed 2026-06-15
  local Lizard baseline evidence.
- 2026-06-21 current line counts from `wc -l`.
- 2026-06-21 targeted Lizard run over the Rust target files.
- 2026-06-21 Codacy Cloud repository query for repo-level metrics and issue
  categories. Raw user/account metadata was not written to durable artifacts.
- 2026-06-21 `git log --oneline --since=2026-06-15 -- ...` over the Rust
  target files showed no Rust target-file commits after the 2026-06-15 refresh,
  but the local metrics were rerun anyway.
- Read-only explorer subagent `019ecc61-3293-7ac1-938d-972a80ae9722`
  completed on 2026-06-15 and returned concrete Rust refactor candidates,
  estimated metric movement, risks, and validation scope.

Current state:

- The largest production duplication remains Rust legacy/core overlap.
- The current Rust reader performance work recently optimized `journal-core`
  hot paths; any deduplication must not undo those guarantees.
- The highest-value live duplication target is still the `offset_array.rs`
  pair, but it is high-risk because it sits on reader traversal and cache
  behavior.
- `rust/src/crates/jf/journal_file/src/journal_file.rs` appears to be
  uncompiled legacy source and is therefore a possible metric-only cleanup, not
  evidence of live runtime duplication by itself.
- `rust/src/crates/journal-core/src/file/file.rs` should be removed from this
  SOW's first-target list based on the 2026-06-15 file-metric baseline, where
  it was no longer a duplication target.
- The safest first question is the disposition of the uncompiled tracked legacy
  file. The safest live-code refactor is small pure helper extraction. The
  highest-impact live-code refactor is offset-array unification, but that should
  not happen without a full compatibility/performance gate.

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

Status: refreshed-needs-user-decision

Problem / root-cause model:

- Codacy duplication is high because legacy `jf` and current `journal-core`
  contain overlapping implementations of journal file/object/offset-array,
  cursor, and filter mechanics. Current evidence keeps offset-array traversal
  as the first live target, keeps `journal_file.rs` as a separate uncompiled
  source disposition decision, and marks broad `journal-core/src/file/file.rs`
  work as lower priority.

Evidence reviewed:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md`: historical Rust top
  duplication table and file-by-file classifications from 2026-06-07.
- `.local/codacy-validity/current-file-metrics-rust-go.json`: refreshed
  2026-06-15 Codacy file-metric baseline export.
- `.local/codacy-validity/lizard-sow-0097-0098.csv`: refreshed local Lizard
  function metric baseline for the target files.
- 2026-06-21 current line counts, current targeted Lizard, and Codacy Cloud
  repository-level state.
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

1. Surgical decision item, recommended first decision: resolve uncompiled
   legacy `journal_file.rs`.
   - Scope: `rust/src/crates/jf/journal_file/src/journal_file.rs`.
   - Evidence: `rust/src/crates/jf/journal_file/src/lib.rs:1-35` does not
     declare a `journal_file` module.
   - Action options to decide before implementation: delete with explicit user
     approval, preserve useful notes in SOW evidence before deletion, or keep
     until broader legacy removal work.
   - Estimated improvement if deleted: remove `829` tracked lines, about `410`
     Codacy duplication points, and about `27` clones. This is a metric cleanup
     and dead-source cleanup, not live architecture deduplication.
   - Current validity: still valid on 2026-06-21. This is the only apparently
     low-risk cleanup, but deletion still requires explicit user approval.
2. Surgical: extract small pure file-format helpers before changing behavior.
   - Candidate scopes: `map_hash_table`, `sanitize_header_for_size`,
     `BucketUtilization`, and iterator skeletons shared by
     `rust/src/crates/jf/journal_file/src/file.rs` and
     `rust/src/crates/journal-core/src/file/file.rs`.
   - Estimated improvement: `120-250` lines moved/shared and `80-180`
     duplication points removed. Lower risk, but `journal-core/src/file/file.rs`
     had duplication `0` in the 2026-06-15 file-metric baseline, so Codacy
     movement may be limited.
   - Current validity: optional low-risk live-code cleanup after the
     `journal_file.rs` decision.
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
   - Current validity: still valid but high-risk. Do only if Rust maintenance
     debt is a priority before integration/release work.
4. Conditional surgical follow-up: share cursor stepping after offset-array
   consolidation.
   - Scope: `rust/src/crates/jf/journal_file/src/cursor.rs` and
     `rust/src/crates/journal-core/src/file/cursor.rs`.
   - Required design: preserve core materialized-value behavior while sharing
     location/cursor resolution logic.
   - Estimated improvement: `220-280` lines moved/shared and `220-300`
     duplication points removed.
   - Current validity: defer until offset-array direction is approved.
5. Long-term-best: refactor filter builder/evaluator only with an explicit
   compatibility policy.
   - Scope: `rust/src/crates/jf/journal_file/src/filter.rs` and
     `rust/src/crates/journal-core/src/file/filter.rs`.
   - Open decision: keep legacy `InvalidOffset` behavior for missing matches or
     adopt core `FilterExpr::None`.
   - Estimated improvement: `180-300` lines moved/shared and `120-220`
     duplication points removed, depending on the chosen compatibility policy.
   - Current validity: defer until the missing-match compatibility policy is
     explicitly decided.
6. Split or defer: shared object layout/parsing.
   - Scope: `rust/src/crates/jf/journal_file/src/object.rs`,
     `rust/src/crates/journal-core/src/file/object.rs`, and related object hash
     helpers.
   - Estimated improvement: `700-1000` lines moved/shared and `250-450`
     duplication points removed, but with high compatibility, compression, and
     dependency-footprint risk. This should be a later phase or separate SOW
     unless the user explicitly chooses a larger architecture cleanup.
   - Current validity: not recommended as near-term work.

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
3. If live-code deduplication is still wanted, prefer small pure-helper
   extraction before hot-path offset-array consolidation.
4. Do not unify filter behavior until the missing-match compatibility policy is
   explicitly decided.
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

2026-06-21:

- Refreshed this pending SOW again from current local code and Codacy Cloud
  repository-level state.
- Confirmed the SOW remains valid as duplication debt, not as an urgent
  function-CCN problem.
- Narrowed remaining work into: first decide the uncompiled
  `journal_file.rs` disposition; optionally extract small pure helpers; only
  then consider high-risk offset-array/cursor/filter consolidation.

## Validation

Refresh validation:

- Current line counts checked for the Rust target files.
- Targeted Lizard run completed for the Rust target files; no targeted Rust
  file exceeded Lizard's configured local threshold.
- Codacy Cloud repository state checked read-only on 2026-06-21.
- No implementation, deletion, source behavior, tests, public docs, specs, or
  runtime skills changed by this refresh.

## Outcome

Open. The SOW is still valid, but it should not be treated as urgent before the
Netdata integration/release backlog unless Rust maintainability is explicitly
prioritized. The only low-risk first action is deciding what to do with the
uncompiled legacy `journal_file.rs`; the main live-code deduplication is
high-risk and needs a full compatibility/performance gate.

## Lessons Extracted

Refresh lesson: Rust legacy/core duplication is real, but the cleanup is not
one kind of work. Dead-source cleanup, pure-helper extraction, offset-array
unification, and filter semantics each have different risk and need separate
decisions.

## Followup

If activated, resolve the uncompiled `journal_file.rs` decision before any
live-path refactor. Keep offset-array/cursor/filter consolidation pending unless
the user explicitly prioritizes Rust debt over integration/release work.
