# SOW-0176 - Single Ownership Follow-ups: Open Decisions And Divergences

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: needs user decisions; tracked from SOW-0097 on 2026-11-08.

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
Decisions). Refresh at activation.

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

Sensitive data handling plan: synthetic fixtures only.

Implementation plan: per item, test first, both languages.

Validation plan: unit tests in both languages plus the query, directory and
verify matrices with the pinned oracles.

Artifact impact plan: specs and API docs for any contract change.

Open decisions: 8 and 9 above; whether to build an FSS-enabled v262 oracle
for I7.

## Implications And Decisions

Pending.

## Plan

Pending activation.

## Execution Log

None.

## Validation

Pending.

## Outcome

Pending.

## Lessons Extracted

None yet.

## Followup

None yet.

## Regression Log

None.
