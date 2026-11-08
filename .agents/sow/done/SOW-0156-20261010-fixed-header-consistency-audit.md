# SOW-0156 - Fixed-Header Consistency Audit

## Status

Status: completed

Sub-state: completed 2026-11-03. D1-D6, N1 and N2 decided by the user and
implemented in two chunks (chunk 1: D1, D2, D4, D6 and old-large-file
support; chunk 2: the 4 GiB ceiling for new files). A1, A4 and A5 were
implemented earlier and approved. The user kept the error names on
2026-11-04.

2026-10-31: SOW-0155 (writer tail offset representability) merged into this SOW
by user decision. Its scope, evidence and open questions in
`.agents/sow/done/SOW-0155-20261009-writer-tail-offset-representability.md`
are part of this audit.

## Requirements

### Purpose

Establish a systematic, paired model of stable fixed-header consistency instead
of discovering missing field relationships one review finding at a time.

### User Request

The user agreed that defining the invariants, auditing both validators, then
implementing and testing missing checks belongs in separate work and a separate
PR. Scope clarification: PR #9 issuecomment-6090888921.

### Assistant Understanding

Facts: both SDKs have shared stable-header checks and separate live-reader bounds.
PR #9 added population budgets and concrete array-reference consistency checks.
Inference: a systematic model may expose additional missing relationships.
Unknowns: additional verified omissions and their supported-format implications.
No such omission is presumed to be a defect without format and reachability evidence.

### Acceptance Criteria

- Enumerate fixed-header invariants and complete-field gates from the supported
  format, including total budgets, per-type minima and pointer relationships.
- Audit both SDKs against the same model; classify every difference with evidence.
- Implement authorized missing checks and paired tests in a separate PR, preserving
  valid historical files, ordinary live readers and header-only cost.

## Analysis

Source baseline: PR #9 and SOW-0152-20261008; shared header validators, format
parsers and writer publication paths. Re-ground on the merged result before work.
Risks: conflating stable/live states, assuming fields exist in old headers, or
expanding header validation into object traversal without a product decision.

## Pre-Implementation Gate

Status: passed for A1, A4 and A5 (2026-11-01); passed for D1-D6 (2026-11-03,
plan under "Implementation plan 2026-11-03").

Problem / root-cause model: successive reviews found relationships missing from
an incomplete header consistency model. This is a hypothesis to audit systematically.
Evidence reviewed: reviews 5475128273 and 5475871638, paired source checks, and the
user-approved scope separation recorded at issuecomment-6090888921.
Affected contracts and surfaces: Go/Rust stable validators, historical field
parsing, inventory/recovery/snapshot consumers, specs and paired tests.
Existing patterns to reuse: complete-field gates, checked population budgets,
real-file preservation tests and separate live-reader mapping validation.
Risk and blast radius: compatibility, safe deletion/recovery and repeated capture
cost. No broader traversal or format change is authorized by this record.
Sensitive data handling plan:
synthetic fixtures, sanitized source evidence and
repo-local or /tmp outputs only; no host journals or private identities.
Implementation plan: define invariants, audit both SDKs, present any contract
forks, then implement and test the approved gaps in a separate PR.
Validation plan: positive historical/live fixtures, negative invariant mutations,
paired real-file tests and unchanged complexity of header-only operations.
Artifact impact plan: update product scope, compatibility guidance, tests and
SOW indexes as needed; user guides only if documented behavior needs clarification.
Open-source reference evidence: systemd/systemd @
c0a5a2516d28601fb3afc1a77d7b42fcfe38fced, docs/JOURNAL_FILE_FORMAT.md.
Open decisions: none. N1 = A and N2 = A (user, 2026-11-03); N1 option B is
tracked in SOW-0171.

Implementation plan 2026-11-03 (both languages, paired tests for every rule):

Chunk 1:

1. D1 append-open (Go `validateAppendHeader`/`newAppendWriter`, Rust
   `JournalFile::open_for_append`): refuse header_size != 272, unknown
   compatible flags, missing TAIL_ENTRY_BOOT_ID, ARCHIVED state, and (as the
   consequence of D2) non-empty files whose header head/tail realtime, tail
   monotonic or tail boot ID break systemd's rules (journal-file.c:641-661).
   All return the "unsupported" kind (Go `errUnsupportedJournal`, Rust
   `JournalError::UnsupportedJournalFile`), so `Log` moves the file to
   `.journal~` and continues (Go `log.go` `replaceableActiveOpenError`, Rust
   `startup.rs` `replaceable_active_open_error`).
2. Old large files (decision 19A): append-open refuses any file whose size is
   above 4 GiB with the same "unsupported" kind (this also replaces the A4
   compact-only error, so `Log` rotates instead of failing to start). Readers
   ignore the 32-bit tail entry array hint and its count when the declared
   arena ends above 4 GiB: Go `header_validation.go` and
   `indexed_snapshot_capture.go`, Rust `object.rs` header validators and
   `indexed_snapshot.rs`.
3. D2 writers (Go `prepareEntryOptions`, Rust `add_entry_fields_with_options`):
   after defaulting and same-boot clamping, refuse realtime outside
   0 < t < 2^55 and monotonic >= 2^55 with a new pre-mutation error (Go
   `ErrTimestampOutOfRange`, Rust `JournalError::TimestampOutOfRange`); the
   writer stays usable. `Log` ignores out-of-range tail values when it
   inherits a floor from an existing file (Go `replaceActiveFile` and active
   reopen, Rust `initial_log_clock`/`initial_last_monotonic`). Readers are
   unchanged.
4. D4: Rust `map_hash_table` accepts a single 16-byte bucket, as Go and
   systemd do.
5. D6: open-time header validation in both readers refuses state >= 3 and
   sealed headers smaller than 240 bytes (no n_entry_arrays).

Chunk 2 (D3, decision 20.3A, regular and compact files):

6. Before any write, the writer bounds the entry's growth from payload sizes
   alone: every item as a new DATA and FIELD object, the entry object, up to
   two TAG objects when sealing, and one minimum (4-slot) entry array per item
   plus one for the global array. If the bound passes 4 GiB it returns a new
   pre-mutation error (Go `ErrJournalFileFull`, Rust
   `JournalError::JournalFileFull`); nothing is written and the writer stays
   usable. Rust buffers the field iterator on the stack for this pass (no heap
   allocation for typical entries).
7. During the write, an entry array that would normally double is shrunk to
   the 4-slot minimum when the doubled size would not leave the reserved
   minimum arrays below 4 GiB. Array capacity is a writer heuristic; readers
   and verifiers use the size each array object declares.
8. `Log` (both languages) rotates on that error and retries the entry once.

Validation plan additions:

- Release-compatibility fixtures written by the v0.10.0 writers (built from
  the tag into `.local/`, no worktree): regular, compact, compressed and
  sealed files, files with out-of-range caller timestamps, an ARCHIVED file
  under the active name, and regular files over 4 GiB. Current Go and Rust
  readers must read all of them; current `Log` must continue after them.
- Byte identity, compression, compact, live, verify and query matrices rerun;
  writer benchmarks before and after chunk 2 (hot append path).
- Error names (`TimestampOutOfRange`, `JournalFileFull`) were agent-chosen;
  the user kept them on 2026-11-04. Rust variants join the unreleased variants already
  listed in `docs/Rust-API.md`.

New finding N1 (2026-11-03, parked for the user): neither SDK can continue
sealing when it reopens a sealed file for append (Go `newAppendWriter` ignores
the SEALED flag; Rust `open_for_append` sets `seal_options: None`), so later
entries are written unsealed. systemd refuses a writable open of a sealed file
unless it loads the sealing key (journal-file.c:4230-4233). Not in the approved
D1 list. Decided N1 = A (user, 2026-11-03): append-open refuses sealed files
as unsupported, so `Log` sets them aside and starts a new sealed file;
continuing sealing with the key (option B) is SOW-0171.

New finding N2 (2026-11-03, parked for the user): root-retention `Log`s never
set files aside, so a current-name active file that chunk 1 now refuses for
append (an out-of-range tail, ARCHIVED state at the active name, or a size over
4 GiB, all writable by v0.10.0) makes root-mode `NewLog` fail instead of
continuing (Go `log.go` `openActivePath`: `l.rootRetention ||
!replaceableActiveOpenError(err)` returns the error; Rust is equivalent).
Default-mode logs set such files aside. Root retention shipped in v0.10.0.
Decided N2 = A (user, 2026-11-03): root mode archives such a file under its
archive name through the archive-only open and starts a new active. A file
with a far-future tail realtime then expires only under size pressure.

Audit model (2026-11-01): systemd's header gate is `journal_file_verify_header`
(systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced,
src/libsystemd/sd-journal/journal-file.c:555-720). Each check was compared
with Go (`go/journal/reader.go`, `writer.go` `validateAppendHeader` and
`newAppendWriter`, `header_validation.go`) and Rust (`journal-core`
`file.rs` reader open, `file_mut.rs` `open_for_append`, `object.rs` header
validators, `header_inventory.rs`). Arena, hash-table extent, tail-object,
entry-array tail and population checks (jf.c:590-694) already match in both
languages through the SOW-0152 validators. The differences:

| Id | systemd rule (jf.c) | Go | Rust | Class |
|---|---|---|---|---|
| A1 | unknown incompatible flags refused for read and write (563) | rejects (`reader.go` open, `validateAppendHeader`) | rejected only in `header_inventory.rs` | parity gap, fixed |
| A2 | write refuses header_size != 272 (581), unknown compatible flags (567), missing TAIL_ENTRY_BOOT_ID (585), ARCHIVED and ONLINE state (711-720) | accepts all | accepts all | D1 |
| A3 | non-empty 272-byte header: non-null tail boot ID, valid head/tail realtime and monotonic (641-661) | missing | missing | D2 |
| A4 | (SDK rule) compact files cannot exceed 4 GiB | rejects at append-open (`writer.go` `newAppendWriter`) | accepted | parity gap, fixed |
| A5 | append needs both hash tables (systemd creates them; append without them is undefined) | rejects (`validateAppendHeader`) | accepted, fails later | parity gap, fixed |
| A6 | empty-file metadata must be zero for headers under 272 bytes | strict | strict | D5 |
| A7 | hash tables need only nonzero size (`hash_table_is_valid`, 536-550) | accepts a 16-byte (1-bucket) table | rejects size <= 16 (`file.rs` `map_hash_table`) | D4 |
| A8 | state >= 3 refused (570); sealed header must contain n_entry_arrays (588) | missing | missing | D6 |
| A9 | writers refuse realtime outside 0 < t < 2^55 (2551) | accepts caller realtime | accepts caller realtime | D2 |
| A10 | (SOW-0155) tail_entry_array_offset is u32 | narrows with `uint32(...)` (`writer_arrays.go:53,64,128`) | narrows with `as u32` (`writer_entry_arrays.rs:151,177,304`) | D3 |

## Implications And Decisions

1. The user placed this broader work in a separate PR. The verified array-count
   finding remains in SOW-0152 and is not deferred here.
2. Agent-chosen, pending user review (autonomous run, 2026-11-01): implement
   A1, A4 and A5 in Rust now. They are Go/Rust parity gaps where Go already
   follows systemd, and the project rule says both languages behave the same.
   Rust now refuses to read files with unknown incompatible flags (as Go and
   systemd do), and refuses to append to compact files over 4 GiB or to files
   missing a hash table (as Go does). Error kinds: `UnsupportedJournalFile`,
   `ObjectExceedsFileBounds` and `InvalidObjectLocation`, matching the nearest
   existing Rust errors for the same conditions.

Decisions (user, 2026-11-03). The compatibility rule: stricter rules may
reject files systemd also rejects, but files written by earlier SDK releases
(v0.2.0-v0.10.0) must stay supported. Every release wrote 272-byte headers
with TAIL_ENTRY_BOOT_ID and only known flags (verified at each tag). A1, A4
and A5 approved. D1 = A; D2 = writers refuse, readers unchanged (option B,
because old writers accepted any caller timestamp); D3 = A with old large
files supported (decisions 19A and 20.3A); D4 = A; D5 = A; D6 = A. The
original options follow for the record.

- D1 (A2): should append-open refuse what systemd refuses for writing?
  A: refuse all of them (header_size != 272, unknown compatible flags, no
  TAIL_ENTRY_BOOT_ID, ARCHIVED); keep accepting ONLINE, because the SDK's
  log writer reopens its own uncleanly closed active file. B: refuse only
  ARCHIVED and unknown compatible flags. C: keep today's behavior.
  Recommendation: A (long-term-best): writing into a file systemd would not
  write risks producing a file systemd rejects; the ONLINE exception is a
  documented SDK feature.
- D2 (A3, A9): should both writers refuse caller realtime outside
  0 < t < 2^55 and monotonic == UINT64_MAX, and should stable readers then
  enforce A3? These are coupled: enforcing A3 alone would make readers reject
  files our own writers can still produce. A: writers refuse, then readers
  enforce A3 (stable/verify paths only, not live readers). B: writers refuse,
  readers unchanged. C: neither. Recommendation: A (long-term-best).
- D3 (A10): regular-format writers can pass 4 GiB with default settings (no
  size rotation by default), and the u32 tail field then silently truncates.
  systemd has the same narrowing but its default size cap is far lower.
  A: cap regular files at 4 GiB like compact (rotate or refuse). B: write 0
  to the tail cache past 4 GiB (readers fall back to the chain walk; needs a
  stock-reader check). C: document only. Recommendation: A (surgical, and the
  same rule compact already has); B needs live-reader evidence first.
- D4 (A7): should Rust accept a 1-bucket hash table like Go and systemd?
  A: yes (Rust follows Go). B: Go follows Rust. Recommendation: A, since
  systemd accepts it.
- D5 (A6): keep the strict empty-file rule for headers under 272 bytes?
  A: keep (no historical file found that violates it). B: relax it to the
  fields systemd checks. Recommendation: A, revisit only on a real file.
- D6 (A8): add the state >= 3 and sealed-header-size rejections to both
  readers? A: yes, stable/verify paths. B: no. Recommendation: A (small, both
  are corrupt-file cases systemd rejects).

## Plan

1. Define the complete fixed-header invariant model (done).
2. Audit Go/Rust against it and resolve material scope or contract decisions
   (audit done; D1-D6 open).
3. Implement and test verified missing checks together (A1, A4, A5 done;
   the rest after D1-D6).

## Implementation And Review Plan

The project manager implements and reviews; the review gate is tests plus own
review. Each check gets paired Go/Rust tests that also prove a rejected open
leaves the file unchanged.

## Execution Log

### 2026-10-10

- Recorded the separate follow-up approved during PR #9 review discussion.

### 2026-11-01

- Activated in the autonomous backlog run; merged SOW-0155 scope.
- Audited both SDKs against `journal_file_verify_header` (table above).
- Implemented A1, A4, A5 in Rust: `SUPPORTED_INCOMPATIBLE_FLAGS` in
  `object.rs` (also used by `header_inventory.rs`), the reader-open check in
  `file.rs`, and the append-open checks in `file_mut.rs`.
- Added paired tests: Go `go/journal/header_open_gate_test.go`; Rust
  `file_tests.rs` `open_rejects_unknown_incompatible_flag`,
  `append_open_requires_both_hash_tables`,
  `append_open_rejects_compact_file_beyond_four_gib`.
- Paused on D1-D6.

### 2026-11-03

- Recorded D1-D6 and the compatibility rule; built v0.10.0 from `git archive`
  into `.local/sow0156/` and wrote release-compatibility fixtures with it.
- Chunk 1, Go: `format.go` timestamp helpers and `tailEntryArrayHintUsable`;
  `parseHeader` refuses state > ARCHIVED and sealed headers under 240 bytes;
  `writer.go` `validateEntryAppendTarget` (called before arena checks, as
  systemd orders them), `openForArchive`, `ErrTimestampOutOfRange` in
  `prepareEntryOptions`; `log.go` ignores out-of-range inherited tails and
  refuses out-of-range timestamps before retention or rotation;
  `root_retention.go` finalizes retired files with `openForArchive`; the
  tail-array hint checks in `header_validation.go`,
  `indexed_snapshot_capture.go` and `verify_index.go` apply only at or below
  4 GiB.
- Chunk 1, Rust: the same rules in `journal-core` `object.rs`, `file.rs`,
  `file_mut.rs` (`validate_entry_append_target`, `open_for_archive`),
  `writer.rs` and `error.rs` (`TimestampOutOfRange`); `map_hash_table`
  accepts one bucket (D4); `journal-log-writer` `chain.rs`, `mod.rs`
  (timestamp check before `ensure_healthy`, since
  `capture_dual_timestamp` advances the clocks) and `root_retention.rs`;
  `journal` `indexed_snapshot.rs` and `verify_graph/strict.rs` hint guards.
- Root-retention inventory keeps the old `validateAppendHeader` rules: the
  first run broke root retention when it reused the new append-target checks,
  so they were split out.
- Paired tests: Go `header_append_target_test.go`; Rust `file_tests.rs`
  (append-target cases, timestamp refusal, state/sealed-header refusal, hint
  above 4 GiB, single-bucket tables), `journal-log-writer`
  `tests/log_writer/append_target.rs`, `journal` `tests/indexed_snapshot.rs`.
  Go tests that deliberately write out-of-range timestamps use the unexported
  `legacyUncheckedTimestamps` option; Rust has no such bypass.
- Docs and specs updated (see Artifacts below).
- Committed chunk 1 (4cd0b7c) and merged it up the stack.
- N1 = A: Go `validateEntryAppendTarget` and Rust
  `validate_entry_append_target` refuse sealed files. N2 = A: Go
  `replaceActiveFile` calls `archiveUnsupportedRootActive` in root mode; Rust
  `open_existing_active_file` takes `UnsupportedActive::{Dispose, Archive}` and
  root mode uses `OwnedChain::archive_unsupported_active_file`. Go archive-only
  opens no longer require a boot ID, since they never append (Rust already
  fell back to the configured one).
- Tests: Go `TestAppendOpenRefusesSealedFile`,
  `TestLogSetsAsideSealedActiveFile` (default and root mode),
  `TestRootLogArchivesActiveFileItCannotExtend`; Rust
  `append_open_refuses_sealed_file`, a `sealed` case in
  `log_sets_aside_active_file_it_cannot_extend`, and
  `root_log_archives_active_file_it_cannot_extend`. Root cases leave out
  head realtime above tail realtime, which root inventory refused before this
  SOW. The Rust `Log` has no sealing option, so its sealed case patches the
  flag; that parity gap is recorded in SOW-0171.
- Created SOW-0171 for N1 option B.
- Created SOW-0172 for the Rust relative-directory finding (chunk 2 real-use
  run).
- Same-path review: in Go, `replaceActiveFile` also serves the stale
  chain-named ONLINE path; in root mode it now archives where Rust's
  `replace_strict_online_chain_file` still sets aside. Neither runs in root
  mode: both languages inventory the root before opening (Go `log.go`
  `NewLog`, Rust `build_startup_state`), and inventory already refuses a
  chain-named ONLINE file there ("archive name/state disagrees with
  header", confirmed by a probe test, not kept). No code was added for it.
- Committed N1/N2 (5463664).
- Chunk 2, Go: `writer.go` `ErrJournalFileFull`, `journalAppendCeiling`
  (`2^32 - 8 MiB`: the arena grows in 8 MiB steps, so the last object must end
  there for the file to stay at or below `2^32 - 1`), `boundEntryGrowth`,
  `entryArrayCapacity` and the pre-check in `appendPayloads`; the three
  array-doubling sites in `writer_arrays.go`; creation and `ensureArenaSize`
  in `writer_init.go` refuse sizes above `2^32 - 1` for both layouts; `log.go`
  rotates once on a full active file that holds entries and retries.
- Chunk 2, Rust: `error.rs` `JournalFileFull`; `writer.rs`
  `JOURNAL_APPEND_CEILING`, `bound_entry_growth`, `entry_array_capacity`,
  the pre-check in `add_entry_fields_with_options` (fields buffered in a
  64-slot stack array, heap only beyond that) and the growth refusal in
  `object_added`; `writer_entry_arrays.rs` doubling sites; `file_mut.rs`
  creation; `journal-log-writer` `mod.rs` rotate-and-retry in both write
  paths. Rust `capture_dual_timestamp` advances the clocks before the write,
  so a final `JournalFileFull` restores them, as Go never moves its clocks
  on a failed append.
- Refinements of plan items 6-8 found while implementing: the ceiling is
  `2^32 - 8 MiB`, not 4 GiB, for the reason above; TAGs are bounded as the
  epochs the entry crosses (Rust adds one when the first TAG is still
  unwritten) rather than "up to two", since a late entry can cross many
  epochs; `Log` retries only when the full file holds entries, so an entry
  too large for an empty file is returned instead of rotating forever.
- Existing test changes: Go `TestWriterSealingBoundaryRefusedBeforeMutation`
  and Rust `compact_preflight` now expect the new error, an unchanged file and
  a usable writer. Rust `live_readers_open_and_follow_during_arena_growth`
  used to observe the writer through the field iterator; the pre-check now
  drains the iterator first, so it uses the test-only `AFTER_ENTRY_ITEM`
  hook instead.
- New paired tests: Go `writer_file_full_test.go`
  (`TestAppendRefusesEntryThatCouldReachFourGiB`,
  `TestEntryArraysStopDoublingNearTheCeiling`,
  `TestLogContinuesInNewFileWhenActiveFileIsFull`,
  `TestLogRefusesEntryThatDoesNotFitAnEmptyFile`); Rust `file_tests.rs`
  `append_refuses_entry_that_could_reach_four_gib`,
  `entry_arrays_stop_doubling_near_the_ceiling`, and `journal-log-writer`
  `log_continues_in_new_file_when_active_file_is_full`,
  `log_refuses_entry_that_does_not_fit_an_empty_file` (also asserts the
  clocks did not move).

## Validation

Chunk 1 (2026-11-03):

- `go test ./...` and `cargo test --workspace` pass. Clippy reports only
  warnings that predate this SOW.
- `tests/docs/check_wiki_docs.py` validates 16 pages; `verify_examples.py`
  passes 35 of 35.
- Release-compatibility fixtures written by the v0.10.0 Go writer
  (`.local/sow0156/`, synthetic identities), checked with current Go and Rust:

| Fixture | Read (Go, Rust) | Append-open | `Log` |
|---|---|---|---|
| regular, compact, zstd, xz | yes | appends | - |
| sealed | yes | unsupported (N1 = A) | set aside to `.journal~`, a new sealed file starts (Go `TestLogSetsAsideSealedActiveFile`, Rust `append_target.rs` table case `sealed`) |
| zero realtime entry | yes | appends; Go `VerifyIndex` flags the entry, as systemd verify does | - |
| tail realtime 2^60, monotonic 2^55 | yes | unsupported | - |
| ARCHIVED state at the active name | yes | - | set aside to `.journal~`, sequence continues (4 rows) |
| out-of-range tail at the active name | yes | - | set aside, sequence continues (4 rows) |
| regular 9.9 GB, last array at 9.94 GB | yes (9,477 rows); snapshot opens | unsupported | set aside, continues (Go and Rust) |

- A file above 4 GiB whose arrays all sit below 4 GiB was not kept as a
  fixture. Its hint is exact, and the paired tests
  `TestAppendOpenRefusesRegularFileBeyondFourGiB` and
  `append_open_refuses_file_beyond_four_gib` cover its append-open refusal.
- The 9.9 GB file shows how v0.10.0 behaves past 4 GiB. Both v0.10.0
  writers reuse the truncated in-memory tail array hint (Go
  `writer_arrays.go` `entryArrayTailOffset`, Rust `writer_entry_arrays.rs`
  `entry_array_tail_offset`). Here, the first append after an array was
  placed above 4 GiB read the truncated offset, landed inside a DATA payload
  and failed the writer (`writer failed; verify before recovery`, invalid
  journal file) after writing the entry's DATA and ENTRY objects. So the file
  holds 9,478 ENTRY objects against a header count of 9,477, and current Go
  `VerifyIndex` correctly reports `tail_entry_seqnum mismatch` for it, as
  for any uncertain tail. The writers check the object type there, so a
  silent mis-write needs the truncated offset to hit bytes that parse as an
  entry array. The current writer keeps this behavior for live files that
  grow past 4 GiB until chunk 2 caps growth.
- Host stock journalctl (v255) drops the out-of-range rows; it is not the
  v260.1 target and was not used as evidence.
- Runtime purity: the diff adds no process launch or host-path access.
- Same-failure search: every reader of the tail entry array hint
  (`header_validation.go`, `indexed_snapshot_capture.go`, `verify_index.go`,
  Rust `object.rs`, `indexed_snapshot.rs`, `verify_graph/strict.rs`) goes
  through the 4 GiB guard; the writers' own use is chunk 2.

Chunk 2 (2026-11-03):

- `go test ./...` and `cargo test --workspace` pass; rustfmt and gofmt clean.
- Real use (`.local/sow0156/fill*`, synthetic identities): a strict-naming
  `Log` with no rotation limits, fed 1 MiB random entries.
  - Go: 22 files, 87,457 rows. Every rotated file is exactly 4,286,578,688
    bytes (`2^32 - 8 MiB`) with 4,082 rows; seqnums run 1..87,457 with no gap
    across 21 rotations. The run was stopped by signal, leaving the active
    file online.
  - Rust: the first file is exactly 4,286,578,688 bytes with rows 1..4,082;
    the next file continues at 4,083.
  - Cross-read: Go and Rust readers each read every file of both runs with the
    same row counts and seqnum ranges, and Go `VerifyIndex` and Rust
    `verify_index` pass on all 24 files, including the interrupted active
    file.
- Benchmarks (writer-core, 200k rows, 32 fields, raw-payload and
  structured-field, regular and compact, Rust and Go; base = HEAD before
  chunk 2): six interleaved base/new pairs with live publication every 1,000
  entries (`.local/sow0156/bench_paired.sh`). The host was loaded (load
  average 40-54 from unrelated work), so wall-clock rows/sec swung up to
  +/-30-100% within a pair and is inconclusive in both directions; a first
  sequential run showed the baseline alone moving 22-30% between rounds.
  Paired user-CPU medians stay within noise in every configuration (Go
  -1.0% to +4.3%, Rust -8.1% to +3.9%, mixed signs). The largest adverse
  wall-clock median, Rust raw-payload compact (-22.9%), comes from system
  time, which follows minor page faults that are bimodal (about 0.5M or 1.1M
  per run) in both trees; file size, mmap remap and map counts are identical
  in every pair, so the writer did the same I/O. Review agrees: the
  pre-check adds one pass of additions per field and, in Rust, a 64-slot
  stack buffer per entry, against tens of microseconds per entry; the clamp
  runs only when an array doubles. No regression is shown; none is claimed
  absent beyond this noise floor.
- Same-failure search: every array allocation that can grow capacity (Go
  `writer_arrays.go` three sites, Rust `writer_entry_arrays.rs` three sites)
  goes through the clamp; DATA, FIELD, ENTRY and TAG allocations are covered
  by the pre-check bound; arena growth and creation refuse sizes above
  `2^32 - 1` in both languages.
- Finding outside scope: the Rust `Log` accepts a relative directory but
  fails on the first write, where Go works. Pre-existing and user-visible, so
  it is tracked in SOW-0172 for a decision.
- Runtime purity: the chunk 2 diff adds no process launch or host-path
  access.
- Matrices: byte identity (`all_equal: true`), compression (24/24), compact
  (20/20), live (18/18), verify (PASS) and root-retention parity (PASS); the
  journalctl query matrix passes against the systemd v262 oracle build. Two
  harness-environment notes: the dataset ingester builds Go with its own
  `.local/gomodcache`, so offline runs need `GOPROXY` pointed at a populated
  local module cache (`file://.../cache/download`); and the query matrix
  needs `--journalctl PATH` since SOW-0169's decision.
- `verify_examples.py` passes 35 of 35; `check_wiki_docs.py` validates 16
  pages.
- Clippy: two `type_complexity` warnings introduced by this SOW's tests
  (`append_target.rs`) were fixed with a type alias; the remaining warnings
  are outside the lines this SOW changed.
- Self-review of chunk 2 (whole changed surface, both languages): pre-check
  ordering after option preparation and before the first mutation; Go
  resets the reservation by `defer`, Rust after the call returns on every
  path; the clamp only shrinks capacity, never below the reserved minimum;
  TAG counts match each writer's sealing loop (Go writes its first TAG at
  creation, Rust may write it with the first entry); `Log` retries once, with
  the same timestamps, and rotation errors return as from the existing
  pre-append rotation. No blocking findings remain.
- Artifacts, chunk 2: `docs/Writer-APIs.md` (the ceiling, the error, the
  `Log` retry), `docs/Rust-API.md` (unreleased `JournalFileFull`),
  `docs/Options-Reference.md` (both formats stop below 4 GiB) and
  `product-scope.md` (writer rule; the compact-only ceiling wording
  removed). The READMEs, `docs/Indexed-Snapshots.md` and the compatibility
  skill stay accurate: they describe reading files above 4 GiB, which is
  unchanged. `AGENTS.md` needs no change. `SOW-status.md` is updated.

Partial (A1, A4, A5):

- The three Rust tests fail with the production changes reverted (append-open
  and reader open succeeded) and pass with them.
- The Go tests pass; Go already enforced these rules.
- `cargo test --workspace`: 45 suites pass. `go test ./...`: all packages
  pass.
- Conformance manifests have no fixture expecting unknown incompatible flags
  to be accepted.
- Same-failure search: the other Rust open paths (snapshot open, recovery)
  go through `open_repository_file_with_window_manager`, which now checks the
  flags; `header_inventory.rs` already did.
- Runtime purity: no process launch or host access added.

Sensitive data gate:

- Synthetic test journals only; no host journals, identities, credentials or
  customer data in the SOW, tests or PR text.

- Artifacts, chunk 1: `docs/Writer-APIs.md` (append-open refusals, timestamp
  range, root archive-only finalization), `docs/Rust-API.md` (unreleased
  `TimestampOutOfRange`), `docs/Indexed-Snapshots.md` (hint above 4 GiB),
  `go/README.md` and `rust/README.md` (what unsupported covers, root mode),
  `.agents/sow/specs/product-scope.md` (writer, reader and snapshot rules) and
  the journal-compatibility skill (the earlier-release rule and the hint).
  `docs/Reader-APIs.md` stays accurate. `AGENTS.md` needs no change.
- Artifacts, A1/A4/A5: end-user docs describe append-open only in general terms
  (`docs/Writer-APIs.md:56-68` "validates supported format metadata",
  `docs/Indexed-Snapshots.md:171`, `docs/Reader-APIs.md:164`) and the
  compatibility skill covers stable-file checks
  (`.agents/skills/project-journal-compatibility/SKILL.md:60`); that wording
  stays true, so none changes. No spec states the per-flag rules. Specs and
  docs are revisited when D1-D6 change documented behavior. Both status
  indexes are updated.

PR review follow-up (2026-11-08, bot review of PR #18, user decision 1A):

Acceptance criteria evidence:

- Invariants enumerated and both SDKs audited: Pre-Implementation Gate and the
  D1-D6, A1, A4, A5, N1 and N2 records above. Authorized checks and paired
  tests: chunks 1 and 2 above, extended by this follow-up.

Tests or equivalent validation:

- Rust writable opens lacked the every-open refusals readers had (unknown
  state, sealed header below 240 bytes). `JournalHeader::validate_open_state`
  now backs reader, append and archive-only opens. Paired tests:
  Rust `writable_open_refuses_unknown_state_and_short_sealed_header`,
  Go `TestWritableOpenRefusesUnknownStateAndShortSealedHeader`.
- A refused oversize `create` left an empty or truncated target in both SDKs;
  the layout check now runs before the open. Paired tests:
  `create_refuses_oversize_layout_without_touching_target`,
  `TestCreateRefusesOversizeLayoutWithoutTouchingTarget`. Go's invalid sealing
  options had the same flaw and are now validated before the open as well.
  `TestLogCreateFailureBlocksTruncatingRetry` used the oversize layout as its
  post-truncation failure, so it now injects an arena-mapping fault.
- The Go parity test now asserts archive-only open for every refused target;
  the Rust 4 GiB hint test now iterates entries.
- Rust `Log` gained a retry-then-refuse test. Go has no fault hook for the
  retry's new file, so the case is covered in Rust only; the retry code it
  exercises is the Rust `Log`'s own.
- Go `parseHeader` errors now name the refused state or size and still match
  `errInvalidJournal`, and the unreachable verifier state branch is removed.

Real-use evidence:

- Earlier-release fixtures, extending the v0.10.0 set above: the Go writers of
  tags go/v0.2.0, v0.3.0, v0.4.0, v0.5.1, v0.6.4, v0.7.8, v0.8.2 and v0.9.0
  (`git archive` into `.local/sow0156/multi/`, the newest patch of each minor;
  compression modules swapped for cached versions because the files are
  uncompressed) each wrote a regular and a compact file of 50 rows. Current
  Go and Rust read 50 rows from each, append-open appends one, Go snapshot and
  `VerifyIndex` pass, and the v262 oracle `--verify` exits 0.
- Not covered by fixtures: Rust-writer output of earlier tags, and patch
  releases other than the ones listed. The per-tag header check recorded under
  the compatibility rule above covers them at header level only.
- Oracle identity for the query matrix: `systemd 262 (262)`, built from
  `systemd/systemd` tag `v262` at `.local/sow0045-oracle/systemd-v262-build/journalctl`,
  features `+PCRE2 +LZ4 +XZ +ZSTD`, no FSS (libgcrypt headers were absent).

PR review follow-up, second pass (2026-11-08, after the first fixes were pushed):

- Valid, fixed: an active `system.journal` with an unknown state or a short
  sealed header stopped `Log` startup, while systemd returns `-EBADMSG`
  (`systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  `src/libsystemd/sd-journal/journal-file.c:561,579`) and journald sets the file
  aside (`src/shared/journal-file-util.c:516-522`). Both `Log`s now do that and
  keep the sequence. Rust reports both headers as `InvalidJournalFileState`;
  Go marks them with `errRefusedHeader`, which stays an invalid-journal error.
  Root retention cannot archive such a file or keep a set-aside one, so it
  reports the error and leaves the file untouched.
- Same-path parity fix: Go's chain scan applied the every-open refusals and
  skipped such files, restarting the sequence at 1 where Rust continued it from
  the raw header. It now reads the header without those refusals.
- Valid, fixed: the spec implied directory writers refuse a zero realtime; they
  raise it to the clock floor.
- Codacy, fixed: `open_writable` complexity 23 (limit 20). Header refusals moved
  to `validate_writable_header`, same order.
- Tests: `log_sets_aside_active_file_with_refused_header`,
  `root_log_refuses_active_file_with_refused_header` and their Go pairs. Go
  `go test ./...` and Rust `cargo test --workspace` pass.

Reviewer findings:

- PR #18 bot threads T3-T26, each answered in its thread. Valid and fixed:
  T6-T14, T16, T17, T19-T26. Rejected with evidence in the thread: T4, T5, T15,
  T18. T3 was already fixed.

Same-failure scan:

- Other Rust writable paths go through `open_writable`; Go's go through
  `parseHeader`. The rotating `log_continues_in_new_file_when_active_file_is_full`
  test had the archive-sync counter race the new test exposed, and now takes
  the same lock.

Artifact maintenance gate:

- AGENTS.md: no change; project rules are unaffected.
- Runtime project skills: no change; the compatibility skill's earlier-release
  rule already requires what this follow-up did.
- Specs: `product-scope.md` (stale realtime gap removed, 4 GiB boundary,
  every-open refusals).
- End-user/operator docs: `docs/Writer-APIs.md` (invalid-file refusals, 4 GiB
  boundary, empty refused actives removed), `docs/Rust-API.md` (append ceiling).
- End-user/operator skills: none exist.
- SOW lifecycle: stays `completed` in `done/`; this is pre-merge PR follow-up.
- SOW-status.md: no change; the SOW's status is unchanged.

Specs update:

- See the artifact gate.

Project skills update:

- None needed; see the artifact gate.

End-user/operator docs update:

- See the artifact gate.

End-user/operator skills update:

- None exist.

Lessons:

- A check added to one open path needs the same check on its siblings; a
  shared header method makes that the default.

Follow-up mapping:

- All PR #18 findings are fixed or rejected above. SOW-0171 and SOW-0172 remain
  as recorded under Followup.

## Outcome

Completed. Both SDKs now apply systemd v260.1's writable-open header rules,
refuse timestamps systemd would not write, keep new regular and compact files
below 4 GiB with a pre-mutation `ErrJournalFileFull` /
`JournalError::JournalFileFull` that `Log` handles by rotating, and keep
reading and continuing every file earlier SDK releases wrote. Sealed files are
refused for append (continuation tracked in SOW-0171); root-mode `Log`s
archive an active they cannot extend.

## Lessons Extracted

Derive checks from format invariants, including relationships between fields.

- Test earlier-release compatibility with files written by the released code,
  not by patched headers: the v0.10.0 large file exposed the truncated-hint
  behavior that no unit test had shown.
- Benchmark on a loaded host with interleaved pairs and read user CPU and
  deterministic counters (file size, remaps, faults) before trusting
  wall-clock deltas.

## Followup

- SOW-0171 (pending): continue sealing when reopening a sealed file (N1
  option B) and the Rust `Log` sealing-option parity question.
- SOW-0172: Rust `Log` fails late on a relative directory; the user chose
  option A (make the directory absolute) on 2026-11-04.
- Error names `TimestampOutOfRange` and `JournalFileFull`: the user kept them
  on 2026-11-04; no further work.

## Regression Log

No completed outcome has regressed in this pending SOW.
