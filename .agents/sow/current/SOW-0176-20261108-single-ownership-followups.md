# SOW-0176 - Single Ownership Follow-ups: Open Decisions And Divergences

## Status

Status: in-progress

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: activated 2026-11-08 on branch `sow-0176/single-ownership-followups`
(stacked on SOW-0097); decisions 8 and 9 made; paused 2026-11-08 by user
decision while the stacked PRs' review findings are fixed (decision 1A);
resumed 2026-11-08 with every decision made (see `## Implications And
Decisions`).

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
- I7: an FSS-enabled v262 oracle needs OpenSSL (corrected 2026-11-08; the first
  analysis said libgcrypt).

## Pre-Implementation Gate

Status: ready

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

Open decisions: none; all recorded under `## Implications And Decisions`.

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

User decisions on 2026-11-08 on the options below: item 4 = B, 5 = B, 6 = A,
7 = A, 8 = A, 9 = A, new = yes, I7 = A. I7 = A is the user's approval to install
`libgcrypt20-dev` system-wide, the one exception to the repository boundary.

Options as presented (evidence in `## Analysis`):

- Item 4, boot listing without `_BOOT_ID` DATA. A: drop the header fallback
  everywhere (matches v262, but `ListBoots` returns nothing for files the core
  writer makes without that field). B: CLIs match v262 and SDK callers keep the
  fallback through a call option (CLI parity and useful SDK results, at the cost
  of two documented behaviors). C: keep as is (the CLI contradiction stays).
  Recommendation: B, long-term-best.
- Item 5, main entry array missing. A: reject at open as systemd does (changes
  the SOW-0159 recovery contract). B: Rust detects it at open as Go does (parity;
  still differs from systemd for this corruption). C: both. Recommendation: B,
  surgical.
- Item 6, Go retention ignores `.journal~`. A: Go adopts Rust's counting and
  deletion (bounded disk use; Go starts deleting disposed files, oldest first).
  B: Rust stops counting them (unbounded growth in both). Recommendation: A,
  long-term-best.
- Item 7, Rust `open_files` with repeated paths. A: call the existing
  `unique_files` helper (parity, no known cost). Recommendation: A, surgical.
- Item 8, unreadable subdirectories. A: record the two narrow differences and
  close. B: align both (low value). Recommendation: A, surgical.
- Item 9, Go fails a whole entry on a damaged field. A: Go behaves like Rust and
  systemd (no lost rows; callers no longer get an error for that corruption). B:
  Rust errors like Go (diverges from systemd, loses rows). Recommendation: A,
  long-term-best.
- New, repeated field in the single-value map. Rust keeps the first value, as Go
  and `sd_journal_get_data` do; callers relying on the last value would see a
  change. Recommendation: yes, surgical.
- I7, verify-key parity with v262. A: install `libgcrypt20-dev` and rebuild the
  oracle (system package, needs approval). B: build libgcrypt inside `.local/`
  (no system change, more effort). C: reject for now; the CLIs keep the 255
  behavior until a release needs it. Recommendation: C, surgical.

## Plan

One chunk per item, test first, both languages, committed when verified:

1. Item 7, Rust `open_files` deduplicates.
2. New, Rust single-value maps keep the first repeated value.
3. Item 9, Go keeps entries with damaged fields.
4. Item 5, Rust detects a missing main entry array at open.
5. Item 6, Go retention counts and deletes `*.journal~`.
6. Item 4, CLIs match v262; SDK callers opt into the header fallback.
7. T67 reader-level live posting bound; T69/T84 Netdata non-UTF-8 namespace
   names.
8. I7, FSS-enabled v262 oracle and verify-key parity.
9. Item 8, record the two differences and close.

## Implementation And Review Plan

Implementation: the project manager implements test-first, one item at a time,
committing verified chunks, in both languages unless a decision says otherwise.

Reviewers: whole-SOW self-review after local validation; tests are the gate.
External reviewers are not used.

Repository boundary block for every helper-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- Do not make changes outside this repository for any reason.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling: implementation, validation, reviewer and audit failures stop
advancement until fixed or decided by the user, and are recorded in the
Execution Log.

## Execution Log

### 2026-11-08

- I7 prerequisite, correction: `libgcrypt20-dev` 1.10.3-2ubuntu0.2 and its
  dependency `libgpg-error-dev` were installed with the user's approval, then
  found unnecessary: v262 sealing uses OpenSSL (`src/shared/journal-authenticate.c:21`,
  `fsprg-openssl.c`), and the earlier analysis naming libgcrypt was wrong.
  `libssl-dev` was already installed, so the oracle is rebuilt with
  `-Dopenssl=enabled` (`+OPENSSL`).

- Item 4: the default boot list has no header fallback, as v262; a
  `BootListOptions` flag opts in. Noncanonical `_BOOT_ID` DATA counts as present
  (T90/T91), and read errors are returned.
- Item 5: Rust activates recovery at open when the main entry array is missing.
- Item 6: Go retention counts and deletes `*.journal~`, first and oldest first.
  Same-path fix: Rust did not count disposed chain-named files
  (`<source>@...@<usec>-<n>.journal~`), which parse as an unknown source. Go
  max-age stops depending on the delete order.
- T67: Rust `file_boot_ranges` stops at `reader_last_visible_entry`.
- T69/T84: non-UTF-8 namespace and remote source names become U+FFFD in both.

- Item 7: Rust `DirectoryReader::open_files_with_options` calls `unique_files`.
- New: Rust single-value maps keep the first repeated value.
- Item 9: Go keeps an entry with a damaged field and returns a payload without
  `=` in `Payloads` only. Where Rust and systemd differed, systemd decided:
  a DATA object with the wrong type or a bad size is skipped in both languages
  (`sd-journal.c` skips `-EADDRNOTAVAIL` and `-EBADMSG`). Decompression failures
  still fail the entry in both, as before.
  Tests: `tests/entry_damage.rs`, `reader_entry_damage_test.go`.

- Bot-review decision 1A delivered on the stack below this branch and merged
  up; PR #26 T60 and T72 are answered by this file's decision list and
  Implementation And Review Plan.

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
