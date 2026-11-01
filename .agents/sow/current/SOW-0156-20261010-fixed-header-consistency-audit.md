# SOW-0156 - Fixed-Header Consistency Audit

## Status

Status: paused

Sub-state: audit done; the Go/Rust parity gaps (A1, A4, A5) are implemented
and tested on branch `sow-0156/header-audit`. Paused on user decisions D1-D6
below; nothing else in this SOW can proceed without them.

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

Status: passed for A1, A4 and A5 (2026-11-01); D1-D6 are open decisions.

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
Open decisions: D1-D6 under Implications And Decisions.

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

Open decisions (parked for the user; each changes user-visible behavior):

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

## Validation

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

- Artifacts: end-user docs describe append-open only in general terms
  (`docs/Writer-APIs.md:56-68` "validates supported format metadata",
  `docs/Indexed-Snapshots.md:171`, `docs/Reader-APIs.md:164`) and the
  compatibility skill covers stable-file checks
  (`.agents/skills/project-journal-compatibility/SKILL.md:60`); that wording
  stays true, so none changes. No spec states the per-flag rules. Specs and
  docs are revisited when D1-D6 change documented behavior. Both status
  indexes are updated.

## Outcome

Paused. Parity gaps are fixed; D1-D6 wait for the user.

## Lessons Extracted

Derive checks from format invariants, including relationships between fields.

## Followup

No additional work item; this SOW is the agreed follow-up.

## Regression Log

No completed outcome has regressed in this pending SOW.
