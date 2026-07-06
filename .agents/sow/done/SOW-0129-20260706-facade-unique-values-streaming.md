# SOW-0129 - Facade Unique-Values Streaming Enumeration

## Status

Status: completed

Sub-state: created 2026-07-06 from gaps surfaced by the Netdata
vendored-journal elimination analysis; ready for prioritization (no user
decision required beyond scheduling). Completed 2026-07-06 by local
project-manager implementation per user routing decision.

## Requirements

### Purpose

Remove the eager, multiple-copy materialization in the facade's unique-value
enumeration so sd_journal-style consumers (query_unique + enumerate loops)
get the same lazy zero/one-copy behavior the low-level reader already
provides.

### User Request

The user directed (2026-07-06, during the Netdata vendored-journal
elimination work): SDK functionality gaps found by consumer integration are
filled in the SDK via SDK SOWs.

### Assistant Understanding

Facts:

- `SdJournalQueryUniqueState` (`rust/src/journal/src/facade.rs:357-425`,
  v0.7.6) eagerly collects ALL unique values for a field at query time, with
  roughly three allocations/copies per value along
  `query_unique_values` → `payload_from_field_value` → per-enumerate clone;
  `SdJournalEnumerateAvailableUnique` (`facade.rs:614`) then returns an
  owned `Vec<u8>` per call.
- The low-level streaming path already exists and is public:
  `FileReader::visit_unique_values` (`rust/src/journal/src/lib.rs:797-806`)
  and `journal_core` `field_data_query_unique` (lazy chain walk over the
  field hash table, no materialization).
- Workload where this bites: facets-style consumers enumerate unique values
  for MANY fields across MANY files per query. High-cardinality fields
  (for example `_PID`) multiply the cost. The pre-SDK Netdata FFI used the
  zero-copy lazy walk; the Netdata FFI shim being built during the
  vendored-journal elimination deliberately bypasses the facade for exactly
  this reason and uses the re-exported low-level reader instead
  (netdata-side decision, 2026-07-06).
- Additional semantic wrinkle worth aligning while here: the facade errors
  mid-collection with a `VerificationError` if a chain payload does not
  start with `FIELD=` (`reader_helpers.rs:141-148`), moving error timing
  from enumerate-time (libsystemd/low-level behavior) to query-time.

Inferences:

- Any facade consumer implementing facets or field-value pickers pays this
  cost today; fixing it removes the main reason for consumers to drop down
  to the low-level API for read paths.

Unknowns:

- Whether the facade's current API contract (enumerate returns owned bytes)
  must be preserved exactly for existing consumers, or can return borrows
  with documented validity (until next call on the handle) as sd_journal
  does.

### Acceptance Criteria

- `SdJournalQueryUnique*` path streams lazily: no upfront full-field
  materialization; at most one copy per returned value (or zero-copy
  borrow with documented lifetime).
- Behavior parity tests: same value sets/order as before on the existing
  fixtures, including compressed payloads.
- Benchmark evidence on a high-cardinality field (before/after allocations
  and time), added to the benchmarks tree.
- Error-timing semantics documented (and aligned with enumerate-time
  reporting if the contract change is accepted).

## Analysis

Sources checked:

- `rust/src/journal/src/facade.rs:357-425,561-620`,
  `rust/src/journal/src/lib.rs:797-806`,
  `rust/src/journal/src/reader_helpers.rs:95-148`.
- Netdata FFI usage pattern: `netdata/netdata @ 17a7eb31da`
  `src/crates/jf/journal_reader_ffi/src/lib.rs:343-401` (the zero-copy
  behavior being preserved consumer-side).

Current state:

- Facade unique enumeration is eager and triple-copy; low-level streaming
  exists but the facade does not use it.

Risks:

- Lifetime/borrow changes on a published facade API are a compatibility
  decision; a conservative fix (stream + single copy per call, keep owned
  returns) avoids any API break.

## Pre-Implementation Gate

Status: ready (user authorized local project-manager implementation on 2026-07-06)

Problem / root-cause model:

- The facade prioritized a simple owned-value implementation; consumer-scale
  facets workloads expose the cost. The streaming primitive exists; the
  facade simply does not use it.

Evidence reviewed:

- See facts (file:line above).

Affected contracts and surfaces:

- Facade `SdJournal` unique-value API internals; benchmarks; docs if error
  timing changes.

Existing patterns to reuse:

- `FileReader::visit_unique_values`; row-arena pinning pattern from
  `row_view.rs` for validity windows.

Risk and blast radius:

- Low for the conservative variant (internal change, same API).

Sensitive data handling plan:

- Synthetic fixtures only.

Implementation plan:

1. Rework `SdJournalQueryUniqueState` to hold a lazy iterator + per-call
   buffer instead of a materialized Vec.
2. Parity tests + benchmark.
3. Optional (user decision): borrow-returning API and enumerate-time errors.

Validation plan:

- Existing facade tests; new parity test on compressed/uncompressed unique
  chains; benchmark delta recorded.

Artifact impact plan:

- Benchmarks tree updated; docs updated if semantics change; SOW-status
  ledgers updated.

Open-source reference evidence:

- `netdata/netdata @ 17a7eb31da`
  `src/crates/jf/journal_reader_ffi/src/lib.rs:343-401`

Open decisions:

- 2026-07-06 user routing/design decision: the project manager implements and
  orchestrates this SOW directly; no separate external implementer model is
  used. Existing SOW analysis is planning evidence and hints, not a frozen
  design.
- Implement the conservative streaming variant: keep the existing owned
  `FIELD=value` return contract for stateful unique enumeration while removing
  upfront all-values materialization. This is the surgical option because it
  preserves published API shape and reduces allocation pressure.
- Borrow-returning unique enumeration is out of scope for this SOW. A public
  contract change would require a separate SOW and consumer evidence.

## Implications And Decisions

- 2026-07-06: created per user direction that SDK gaps found during Netdata
  integration are filled in the SDK.

## Plan

1. Implement conservative streaming variant.
2. Parity tests + benchmark evidence.
3. Review.

## Delegation Plan

Implementer:

- Local project-manager implementation per user routing decision on
  2026-07-06. No separate external implementer model is used for this SOW.

Reviewers:

- Read-only reviewers from the approved pool.

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

- Record blockers and missing evidence before changing scope.

## Execution Log

### 2026-07-06

- Created from the Netdata vendored-journal elimination deep analysis
  (netdata-side SOW: eliminate-vendored-journal-crates).
- Reworked Rust facade stateful unique enumeration so
  `SdJournalQueryUniqueState` initializes FIELD/DATA iterator state instead of
  materializing all values into `SdJournal`.
- Added `FileReader` and `DirectoryReader` stateful unique enumeration, with
  directory-level cross-file dedupe.
- Fixed core FIELD/DATA iterator restart semantics so restart resets to the
  field DATA chain head.
- Second reviewer pass found that the legacy `jf` reader still exposed a
  no-op FIELD/DATA restart path through `journal_reader_ffi`. The legacy
  `jf/journal_file` FIELD/DATA iterators now retain the head DATA offset and
  implement real restart semantics.
- Added focused coverage for compressed stateful unique enumeration,
  missing-field unique state, direct `DirectoryReader` stateful enumeration
  across seeks, and the legacy facade compressed/restart path.
- `docs/Rust-API.md` now documents the newly public stateful unique methods on
  `FileReader` and `DirectoryReader`, including the fact that streaming
  enumeration can fail after query setup.

## Validation

Acceptance criteria evidence:

- `rust/src/journal/src/facade.rs` stores only active unique field state and
  enumerates one returned `FIELD=value` payload at a time.
- `rust/src/journal/src/lib.rs` wraps the core FIELD/DATA iterator for
  stateful unique enumeration.
- `rust/src/journal/src/directory.rs` streams per-file unique payloads and
  deduplicates values across files without pre-materializing all payloads.
- `rust/src/crates/journal-core/src/file/reader.rs` and
  `rust/src/crates/journal-core/src/file/file_iterators.rs` now support
  correct FIELD/DATA restart.

Tests or equivalent validation:

- `cargo test --manifest-path rust/Cargo.toml --workspace` - passed.
- `go test ./...` from `go/` - passed.
- `python3 tests/docs/check_wiki_docs.py` - passed.
- `python3 tests/docs/verify_examples.py` - passed 31/31 examples.
- Focused repair tests passed:
  `jf_facade_unique_state_handles_compressed_payloads_and_restart`,
  `jf_facade_data_enumeration_handles_compressed_payloads`,
  `jf_facade_stateful_reader_operations`, and
  `directory_reader_query_unique_deduplicates_indexed_values_across_files`.

Real-use evidence:

- Facade tests cover stateful unique enumeration, restart, and multi-file
  dedupe using generated journal files.

Reviewer findings:

- 2026-07-06: read-only reviewers were run with the SOW filename and complete
  changed surface. Reviewers found two actionable Rust issues that were handled:
  `SdJournal::restart_unique()` briefly became source-breaking by returning a
  `Result`, and the cross-file stateful unique test used a `HashSet` in a way
  that could hide duplicate emissions. The public method is infallible again,
  restart resets existing FIELD/DATA iterator state without re-querying, and
  the test now checks emitted payload counts before set comparison.
- Reviewers also found that Go facade streaming parity and high-cardinality
  benchmark evidence were not covered by this Rust implementation chunk. Those
  are tracked by pending SOW-0131.
- 2026-07-06 repeat review: Claude voted not production-grade as closed SOWs
  and found the legacy `jf` restart gap, missing compressed stateful unique
  coverage, direct `DirectoryReader` seek/state asymmetry, missing public API
  docs, and unrecorded Rust public API contract implications. GLM timed out
  before a final verdict after partially reproducing real concerns. Minimax,
  Deepseek, Kimi, and Qwen returned production-grade or production-grade with
  non-blocking notes. The Rust/jf findings are fixed and SOW-0131 remains the
  explicit Go facade parity and benchmark follow-up.
- 2026-07-06 second repeat review: Claude failed with an API connection reset;
  GLM and Minimax timed out without final verdicts; Deepseek, Kimi, and Qwen
  returned production-grade with non-blocking notes. Deepseek noted the
  `DirectoryReader` cross-file dedupe state retains returned unique values for
  correctness; `docs/Rust-API.md` now states that the iterator avoids
  pre-materializing all payloads but still keeps the emitted-value set needed
  for cross-file deduplication.

Same-failure scan:

- Direct materializing `query_unique()` remains as a convenience API; the
  sd-journal-style stateful `query_unique` + enumerate loop no longer stores
  all unique payloads in `SdJournal`. Legacy `jf` FIELD/DATA restart now
  restarts from the FIELD object's DATA-chain head instead of doing nothing.
  Go facade stateful unique enumeration still materializes and is tracked
  separately by SOW-0131.

Sensitive data gate:

- Synthetic journal fixtures only; no sensitive data was used or recorded.

Artifact maintenance gate:

- `AGENTS.md`: no project-wide workflow change required.
- Runtime project skills: no HOW-to-work rule changed.
- Specs: no product spec update needed for the Rust implementation path; the Go
  parity/benchmark follow-up is tracked by SOW-0131.
- End-user/operator docs: `docs/Rust-API.md` updated for the newly public
  `FileReader` and `DirectoryReader` stateful unique methods. Existing facade
  function signatures and owned return shapes are unchanged. Corrupt FIELD/DATA
  chain errors now surface during enumeration instead of upfront query, aligning
  with the streaming facade behavior.
- SOW lifecycle: completed and moved to done.
- `.agents/sow/SOW-status.md`: updated.

Specs update:

- Product scope already records stateful unique enumeration as
  FIELD/DATA-chain based; SOW-0131 tracks the Go parity and benchmark evidence
  gap found by reviewers.

Project skills update:

- No project skill update needed; this changed SDK behavior, not repository
  workflow.

End-user/operator docs update:

- `docs/Rust-API.md` documents direct stateful unique enumeration methods and
  their streaming error timing.

End-user/operator skills update:

- No output/operator skills are maintained for this SDK surface.

Lessons:

- Stateful facade APIs should preserve iterator state and avoid hidden
  materialization when the journal format already provides a native chain.

Follow-up mapping:

- SOW-0131 tracks Go facade stateful unique streaming parity and benchmark
  evidence.
- Pending SOW-0066 release planning must classify new Rust public reader
  methods as additive API additions. They do not require Netdata source changes
  unless Netdata chooses to call the stateful direct reader methods.

## Outcome

Completed. Rust facade unique enumeration now streams over FIELD/DATA chains
for the stateful enumerate API and preserves existing direct-query behavior.
Go facade parity and benchmark evidence are tracked by SOW-0131.

## Lessons Extracted

Compatibility facades must not hide row-scan or full-materialization behavior
when native journal indexes can answer incrementally.

## Followup

SOW-0131 tracks Go facade unique streaming parity and high-cardinality
benchmark evidence.

## Regression Log

None yet.
