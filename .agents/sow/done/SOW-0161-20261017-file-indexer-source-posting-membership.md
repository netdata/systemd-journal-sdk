# SOW-0161 - File Indexer Source Posting Membership

## Status

Status: completed

Sub-state: reproduced and fixed 2026-11-01 in the autonomous backlog run;
scope choices reviewed by the user 2026-11-03 (decision 5 revises decision 2).

## Requirements

### Purpose

Determine the correct membership and captured-tail contract for source-timestamp
postings used by native metadata indexes, preserving Go/Rust behavior and speed.

### User Request

Follow-up tracking from the requested SDK compatibility and performance work.
This record does not authorize an additional implementation workstream.

### Assistant Understanding

Facts:

- Rust FileIndexer collects source postings without dereferencing their ENTRY
  targets, then fills missing timestamps from global ENTRY offsets.
- The algorithm predates SOW-0159 and is present in prerequisite commit 4c49205.
- Strict bidirectional native membership certification belongs to verify_index.

Inferences:

- Replacing the source DATA inline target with that DATA object's own offset
  should admit an extra histogram record. This is a source prediction, not an
  executed reproduction or an established new regression.

Unknowns:

- Actual baseline/current results, equivalent Go surfaces and live-publication
  treatment must be established before selecting a repair.

### Acceptance Criteria

- Reproduce or reject the source prediction on baseline/current regular and
  compact fixtures, including an actively growing captured-tail case.
- Establish the affected native-index contract in both languages and obtain
  approval for any behavior/performance change before implementation.

## Analysis

Sources checked:

- rust/src/crates/journal-index/src/file_indexer.rs: collect_source_field_info,
  build_histogram and collect_data_entry_indices.
- rust/src/crates/journal-core/src/file/offset_array.rs: collect_offsets.
- .agents/sow/specs/product-scope.md: strict verification contract.
- SOW-0159: required-chain error propagation and documented value tolerance.

Current state:

- With the existing 14-row source fixture, source inspection predicts that
  inline DATA target plus entries 2..14 enter the source map; global fallback
  then adds entry 1, yielding 15 records. No traversal error is generated.
- Source postings beyond the captured tail are another condition to examine.
  Rejecting them mechanically could alter live-file behavior.

Risks:

- Full row parsing, an extra membership container or repeated searches could
  hurt indexing performance. Additional certification must not be introduced
  casually into this native metadata path.

## Pre-Implementation Gate

Status: passed 2026-11-01 (reproduction below; the original text is kept as
the activation record).

Problem / root-cause model:

- A native posting target may be accepted as a source-map key without proof
  that the global ENTRY population contains it. Reproduction remains required.

Evidence reviewed:

- The source and inherited algorithm listed above; no runtime reproduction.

Affected contracts and surfaces:

- Optional Rust FileIndexer/engine histogram and cache results; investigate Go
  aggregation equivalents. Core strict verification remains the certification API.

Existing patterns to reuse:

- Existing source timestamp fixtures, global offset pass, entry-offset map,
  engine batch isolation and versioned cache keys.

Risk and blast radius:

- Membership policy, live snapshots, repeated values and indexing costs.
  This lead does not change SOW-0159's decoder or bounded-chain repair.

Sensitive data handling plan:

- Synthetic journal identities and payloads only; generated evidence stays
  ignored under .local/. Durable records contain no raw private data.

Implementation plan:

1. Reproduce baseline/current behavior without changing runtime code.
2. Inspect both language surfaces and native publication semantics.
3. Present behavior and cost alternatives if a repair is justified, then update
   this gate with the approved plan before implementation.

Validation plan:

- Regular/compact, healthy/invalid targets, captured-tail growth and required
  Go/Rust parity; representative cost evidence appropriate to the chosen change.

Artifact impact plan:

- AGENTS.md: existing parity/performance rules apply; no change identified.
- Runtime project skills: update if validation exposes a reusable missed pattern.
- Specs: record an approved membership and captured-tail contract if changed.
- End-user/operator docs: document any changed index/cache behavior.
- End-user/operator skills: none currently exist.
- SOW lifecycle: pending only; no new prerequisite for SDK then CLI delivery.
- SOW-status.md: add this investigation lead to both status indexes.

Open-source reference evidence:

- No external repository was needed for this local source-analysis lead.

Open decisions:

- No implementation choice is approved. First establish actual behavior and
  scope; user design decisions follow concrete evidence.

## Implications And Decisions

1. SOW-0159 fixes an encountered absent-map facet panic on its existing cold
   error branch. Source-map certification is a separate inherited question,
   rather than another suppressed traversal error. Track it here without
   introducing additional indexing passes into the current repair.

Reproduction (2026-11-01, `journal-index/tests/source_postings.rs`, regular
and compact, before the fix):

- Captured tail: 14 rows indexed, 2 rows published after the captured tail;
  the histogram counted 16. The global pass already ignores them; the source
  pass did not.
- Membership: the source DATA inline target replaced by the DATA object's own
  offset (the predicted case); the file indexed successfully with 15
  records.
- Repeated field: an entry carrying the source field twice was counted twice
  (5 records for 4 entries).
- Live race: the Rust writer advances the tail when it allocates an ENTRY,
  links it into the global and DATA arrays, and increments `n_entries` last
  (`journal-core` `writer.rs` `publish_entry_links` then `entry_added`). A
  reader that reads the global array in that window sees DATA postings for an
  entry the global array does not yet count. Simulated by lowering
  `n_entries` by one: the source index counted the extra entry.

Go: no equivalent surface. `journal-index` and `journal-engine` are Rust-only;
the Go explorer reads `_SOURCE_REALTIME_TIMESTAMP` per row
(`go/journal/explorer.go`) and builds no posting-based histogram. Parity is
not affected.

2. Agent-chosen, pending user review: the contract implemented is the one the
   field-bitmap path already uses in the same file. Postings past the
   captured tail are ignored. A posting at or below the tail that the global
   array does not contain is `IndexError::MissingOffset` on a stable file and
   is dropped on a live (online or active-name) file, because the live race
   above makes it legitimate there. The bitmap path
   (`collect_data_entry_indices`) previously returned `MissingOffset` on live
   files too, so the same live race could fail indexing of a requested field;
   it now drops the posting on live files, matching the source path.
3. Agent-chosen: an entry with the source field more than once keeps its
   earliest source timestamp, so every entry is counted once.
4. `CACHE_VERSION` bumps from 4 to 5, as `product-scope.md` requires for any
   change to what the indexer computes. An earlier draft of this SOW skipped
   the bump; PR review pointed to the spec rule. Impact: every cached file
   reindexes once after upgrade; the spec records generation 5.
5. 2026-11-03 (user): decisions 3 and 4 approved. Decision 2 is revised: a
   posting at or below the captured tail that the global array does not
   contain is dropped on every file, stable or live, in both the source and
   the requested-field paths. Reason: files written by earlier SDK releases
   must stay readable, and writers before v0.9.0 had no poisoning, so a
   failed append followed by a clean close could leave an archived file with
   DATA links to an uncounted entry (v0.8.0 `writer_entry_arrays.rs` linked
   DATA before `n_entries` advanced). Rejecting such files would make them
   unindexable. Generation 5 stays: it is unreleased, and the change only
   turns former failures into indexes, which are never cached as failures.
6. 2026-11-24 (user, PR review): decision 3 is revised. An entry with the
   source field more than once keeps the value with the lowest DATA offset,
   not the earliest. Reason: the query scanner (`get_timestamp_field`,
   `file_index.rs`) reads the first matching payload in entry-item order, and
   items are sorted by DATA offset, so an index sorted by the earliest value
   disagreed with the scanner and a bounded forward scan could stop early and
   lose rows. Alternatives rejected: the scanner taking the minimum (reads
   every item of every scanned entry, a per-query cost) and storing per-entry
   timestamps in the index (memory and a larger change). Generation 5 stays:
   it is still unreleased.
7. 2026-11-25 (user, PR review): decision 6 is revised. An entry with the
   source field more than once keeps the first matching value in the entry's
   own item order, exactly what `get_timestamp_field` reads. Reason: journald
   flushes `/run` to `/var` with `journal_file_copy_entry`
   (systemd/systemd @ v260.1 `src/libsystemd/sd-journal/journal-file.c:4430`,
   called from `src/journal/journald-manager.c:1355,1383`), which appends the
   source entry's items without re-sorting while DATA already in the target
   keeps its older offset, so "lowest DATA offset" and "first item" can
   differ. Only entries that carry the field twice read their item list.
   Alternative rejected: keeping decision 6 and documenting that flushed files
   can disagree (a known wrong-results case on real journald files). The same
   review found that a compressed source timestamp was parsed raw by the
   indexer (`parse_timestamp` uses `raw_payload()`), falling back to realtime
   while the scanner decompresses it; the indexer now decompresses it.
   Generation 5 stays: it is still unreleased.

Cost: the healthy path adds one comparison per source posting, one counter
per global entry and no extra lookups (the deduplicating insert replaces the
previous insert). The member set is built only when a mismatch is found.

## Plan

Done; see the execution log.

## Implementation And Review Plan

Implementation:

- Not started. The project manager owns any subsequently approved changes.

Reviewers:

- No review has run for this pending investigation. Follow the repository's
  standing review authorization and system-wide skill at a meaningful boundary.

Repository boundary block for every external-reviewer prompt:

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

- Preserve reproductions and qualify uncertainty; do not change live-file
  behavior or allocate a verification framework without an approved plan.

## Execution Log

### 2026-10-17

- Recorded the source-analysis lead discovered during SOW-0159's consumer audit.

### 2026-11-01

- Activated in the autonomous backlog run; reproduced all predictions plus the
  repeated-field and live-race cases.
- Fixed `rust/src/crates/journal-index/src/file_indexer.rs`
  (`collect_source_field_info`, new `rebuild_entry_offset_index`,
  `build_histogram`, `collect_data_entry_indices`).
- Added `rust/src/crates/journal-index/tests/source_postings.rs` (5 tests).

### 2026-11-03

- Applied decision 5: `collect_data_entry_indices` and `build_histogram` drop
  non-member postings on every file; the now-unused `live` parameters are
  removed. `source_postings.rs` covers the bogus-inline and linked-but-uncounted
  cases on both live and stable files; the `array_integrity.rs`
  `unindexed-inline` mutation now expects a 14-entry index on archived files,
  for both the source and requested-field paths.

### 2026-11-24

- Applied decision 6: `collect_source_field_info` walks the source DATA chain
  with offsets, orders the values by DATA offset and keeps the first value per
  entry. Index cost: one sort of the distinct source values and one hash
  insert per posting; the query path is unchanged.
- `repeated_source_field_uses_the_value_the_scanner_reads` (regular and
  compact) is the reviewer's case: A carries 3s then 1s, B carries 2s, and a
  forward scan over [2s, 3s) must return B. It returned no rows before the fix.

### 2026-11-25

- Applied decision 7: `collect_source_field_info` keeps the first value per
  entry and, when a second value for the same entry appears, compares the two
  DATA offsets in the entry's item list (`precedes_in_entry`). A compressed
  source value is decompressed before parsing; a value that cannot be
  decompressed is skipped like an unparsable one.
- `repeated_source_field_follows_unsorted_entry_items` (regular and compact)
  swaps the two source items of an entry, as a flushed file can order them,
  and expects the scanner's value. `compressed_source_timestamp_is_indexed_by_its_value`
  (zstd, lz4, xz) stores a compressed source value whose realtime would sort
  it after a later row.

## Validation

- All 5 new tests failed on the original indexer (16, 15, 5 records, accepted
  corruption, extra live entry) and passed after the fix. After decision 5
  (2026-11-03) the suite has 4 tests (live and stable cases merged into
  loops); `cargo test --workspace` passes again, all suites ok.
- `cargo test --workspace`: 46 suites pass, including `journal-engine`
  `index_integrity` and the existing `array_integrity` source cases.
- Clippy: no warning in the changed files.
- Decision 6: the new test fails on the earliest-value indexer (`[]`, B lost)
  and passes after the fix; `source_postings` 5/5 and the index and engine
  suites pass.
- Decision 7: both new tests fail on the decision 6 indexer (`[]` for each)
  and pass after the fix; `source_postings` 7/7 and `cargo test --workspace`
  (46 suites) pass. Clippy reports nothing in the changed files.
- Same-failure search: the only other consumer of DATA postings in the
  indexer is `collect_data_entry_indices`, fixed the same way. The engine
  consumes `FileIndex` only.
- Real-use path: the indexer is exercised end to end by the engine tests; no
  Netdata run was made (Netdata is out of scope without authorization).
- Runtime purity: no process launch or host access added.

Sensitive data gate:

- Synthetic journals and payloads only.

Artifact maintenance gate:

- AGENTS.md and project skills: no new rule; the cache-version rule in the
  compatibility skill and product scope was applied (decision 4).
- Specs: `product-scope.md` records cache generation 5 and its reindex
  impact.
- End-user docs: the indexer is internal to the engine; no doc describes it.
- SOW-status.md: both indexes updated.
- Follow-up mapping: none; native traversal errors remain SOW-0159.

## Outcome

Completed. Source-timestamp histograms now count exactly the captured ENTRY
population once per entry. Postings outside that population are dropped on
every file, so live files tolerate the writer's link-before-count window and
files left by pre-v0.9.0 writers stay indexable.

## Lessons Extracted

Chain completion and cross-index membership are distinct guarantees. Writer
publication order decides which membership checks are valid on live files.
Read the spec rules for the changed surface before choosing a default: the
cache-version rule was a spec requirement, not a judgment call.

## Followup

No additional follow-up is identified before investigation.

## Regression Log

No completed outcome has been reopened.
