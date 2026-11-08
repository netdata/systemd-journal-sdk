# SOW-0176 - Single Ownership Follow-ups: Open Decisions And Divergences

## Status

Status: paused

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: activated 2026-11-08 on branch `sow-0176/single-ownership-followups`
(stacked on SOW-0097); decisions 8 and 9 made; paused 2026-11-08 by user
decision while the stacked PRs' review findings are fixed (decision 1A);
items 3-9 investigated, user asked for more information before deciding.

## Requirements

### Purpose

Finish what SOW-0097 (single ownership of journal behavior) left for the
user: two public-contract decisions, the verify-key item that needs an FSS
oracle, and Go/Rust or SDK/systemd divergences found while consolidating.

### User Request

SOW-0097 user decisions: one owner per journal behavior, public SDK function
in both languages, tests first, Go/Rust parity. Public API and user-visible
changes are the user's to decide.

### Assistant Understanding

Facts (each found during SOW-0097, behavior unchanged by it):

1. Decision 8: legacy `ExportEntry` (Go) / `export_entry_bytes` (Rust) keep
   the SOW-0043 lossless contract (invalid and non-UTF-8 field names stay in
   the output), while `EntryEncoder` stops at them as systemd v262 does.
2. Decision 9: Rust `DirectoryReader::list_boots` and `SdJournal::list_boots`
   return `Vec<BootInfo>` and fall back to one boot per file header on read
   errors; Go `DirectoryReader.ListBoots` returns an error.
3. I7: both CLIs follow systemd 255 verify-key handling; v262 parses per file
   with different messages; the pinned v262 oracle is built without FSS, so
   parity cannot be certified; the SDK key parser matches neither.
4. For a file without `_BOOT_ID` DATA, v262 `--list-boots` prints "No boot
   found." while both rewrites list ENTRY header boot IDs.
5. On `main_entry_array_missing.journal` (verify negative corpus) v262 fails
   to open the file, Go `--list-boots` prints "No boot found." and Rust lists
   the boot.
6. Go directory retention never deletes `*.journal~` files; check against the
   Rust writer and the systemd vacuum rule.
7. Rust `DirectoryReader::open_files` does not deduplicate repeated paths;
   Go does (both CLIs now call `UniqueFiles` / `unique_files`).
8. Subdirectory read errors during directory discovery are handled
   differently by Go and Rust.
9. The Go reader fails a whole entry on a payload without `=`; the Rust
   reader keeps the entry.

Unknowns:

- Which side is right for items 4-9, against systemd v260.1/v262 sources.

### Acceptance Criteria

- Decisions 8 and 9 made by the user and applied in both languages, or
  recorded as rejected.
- I7 done with an FSS-enabled v262 oracle, or rejected with evidence.
- Items 4-9: each checked against the owning oracle and fixed in both
  languages with tests, or rejected with evidence.

## Analysis

Evidence is in SOW-0097 (Execution Log, F2 sweep, Implications And
Decisions), plus the 2026-11-08 investigations:

- Item 4: v262 `discover_next_id` (systemd/systemd @ v262
  src/shared/logs-show.c:2021-2159) needs `_BOOT_ID` DATA; with none,
  `--list-boots` prints "No boot found." and `-b 0` fails. The SDK header
  fallback (`go/journal/boots.go`, `rust/src/journal/src/boots.rs`) lists the
  header boot, after which `-b 0` prints "-- No entries --". No oracle
  fixture lacks `_BOOT_ID`. Related review threads: PR#26 T88/T90/T91/T92/T95.
- Item 5: v262 refuses `entry_array_offset == 0 && tail_entry_array_offset !=
  0` at open (systemd/systemd @ v260.1 src/libsystemd/sd-journal/
  journal-file.c:646). Go recovers an empty prefix (0 entries everywhere);
  Rust activates recovery lazily, so filtered reads and `boot_ranges` see 5
  entries while unfiltered reads see 0.
- Item 6 confirmed: Go retention parses only `.journal` names
  (`go/journal/log.go:1297-1322`); Rust counts and deletes `.journal~` first
  (`journal-log-writer/src/log/chain.rs:142-162`); journald vacuums them
  (journal-vacuum.c:217-248).
- Item 7 confirmed: Rust `DirectoryReader::open_files_with_options` opens
  duplicates (same file twice: Rust 10 entries, Go 5); the facade already
  dedupes; systemd dedupes (sd-journal.c:1611-1643).
- Item 8 mostly refuted: both skip unreadable subdirectories and fail on an
  unreadable root; narrow differences: mid-listing read errors and the error
  class for a missing root.
- Item 9 confirmed: Go `readEntryAt` fails the whole entry on a payload
  without `=` or an unreadable DATA object (`go/journal/reader_entry.go:
  146-155`); Rust keeps the raw payload (`reader_helpers.rs:61-79`); systemd
  returns payloads as-is (sd-journal.c:2889-2898). Affects the Go facade
  seqnum/monotonic getters and Netdata row expansion.
- New: single-value field maps keep the first repeated value in Go and the
  last in Rust (`reader_helpers.rs:75`); systemd returns the first.
- I7: an FSS-enabled v262 oracle needs libgcrypt headers (not installed).

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model: parallel Go/Rust and CLI/SDK implementations
drifted before single ownership was a project rule.

Evidence reviewed: SOW-0097.

Affected contracts and surfaces: Go and Rust reader, directory and export
APIs; both journalctl rewrites; Go directory retention.

Existing patterns to reuse: SOW-0097 goldens (`tests/output/`), query and
verify matrices.

Risk and blast radius: decisions 8 and 9 change public contracts; item 6
changes retention (data deletion); item 9 changes what readers return.

Sensitive data handling plan:

- Synthetic fixtures and public systemd test data only.

Implementation plan: per item, test first, both languages.

Validation plan: unit tests in both languages plus the query, directory and
verify matrices with the pinned oracles.

Artifact impact plan: specs and API docs for any contract change.

Open decisions: 8 and 9 above; whether to build an FSS-enabled v262 oracle
for I7.

## Implications And Decisions

User decisions on 2026-11-08 (user agreed with both recommendations):

- Decision 8 = A: legacy `ExportEntry` / `export_entry_bytes` keep the
  SOW-0043 lossless contract. No code change; rejected as a change.
- Decision 9 = B: Rust `DirectoryReader::list_boots` and
  `SdJournal::list_boots` return `Result`, as Go does; the header-summary
  fallback is removed.

User decisions on 2026-11-08 about the open stacked PRs' bot reviews:

- 1 = A: fix every valid finding on the branch it belongs to, bottom-up,
  merge up the stack, rerun validation, reply in and resolve every thread.
  This SOW pauses until that is done.
- 2 = agree: not fixed in the PRs, replied with reasons: test-helper
  duplication (T33/T44), npm lockfile (T86), `runFollow` complexity
  (T54, SOW-0175); the reader-level live posting bound (T67) and Netdata
  non-UTF-8 namespace-name normalization (T69/T84) are added to this SOW.

Investigation results for items 3-9 (2026-11-08, read-only helpers, verified
where noted): see `## Analysis`.

## Plan

Pending activation.

## Execution Log

### 2026-11-08

- Decision 9 implemented: `DirectoryReader::list_boots` returns
  `Result<Vec<BootInfo>>` (the internal `try_list_boots` and the header
  fallback are gone); `SdJournal::list_boots` returns
  `Result<Vec<BootInfo>, Error>`. Callers were the facade, tests and the
  conformance adapter (already on `SdJournalListBoots`); Netdata does not
  call it. Docs (`rust/README.md`, `docs/Rust-API.md`) and
  `product-scope.md` updated. `cargo test --workspace` PASS; wiki check and
  the `rust-list-boots` example PASS.

## Validation

Pending.

Sensitive data gate:

- Pending; the work uses synthetic fixtures and public systemd test data only.

## Outcome

Pending.

## Lessons Extracted

None yet.

## Followup

None yet.

## Regression Log

None.
