# SOW-0178 - Undecompressible DATA Costs One Field, Not The Entry

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: created 2026-11-09 from the SOW-0176 PR review (user decision 1B);
waiting on the xz decision below. Branch `sow-0178/undecompressible-data-field`,
stacked on SOW-0176.

## Requirements

### Purpose

A damaged compressed DATA object should cost the field it holds, as it does in
systemd, instead of failing the whole entry.

### User Request

SOW-0176 PR review, decision 1: "B: skip the field, as systemd does", as its
own SOW so the reader-hardening contract change is reviewed separately.

### Assistant Understanding

Facts:

- systemd skips an entry item whose payload read returns `-EADDRNOTAVAIL` or
  `-EBADMSG` (systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced
  `src/libsystemd/sd-journal/sd-journal.c:2852,2896`). Corrupt zstd and lz4
  streams return `-EBADMSG` (`src/basic/compress.c` lz4 and
  `zstd_ret_to_errno`). Corrupt xz returns `-ENOMEM`
  (`decompress_blob_xz`), which is not skipped.
- Stock v262 differential (`.local/sow0178/codec_diff.py`, 5-entry files whose
  one compressed DATA object is shared by every entry):
  - zstd corrupt (bit flip, zeroed tail): stock rc 0, 5 rows, field dropped
    (75 to 70 fields). Go and Rust: rc 1, no rows.
  - lz4 bad stored size (+1, -1): stock rc 0, 5 rows, field dropped. Go and
    Rust: rc 1, no rows.
  - xz corrupt: stock rc 1, `Failed to read journal: Cannot allocate memory`,
    no rows. Go and Rust: rc 1, no rows.
  - Reader-hardening zstd fixtures (bad later checksum, truncated later frame,
    trailing garbage, oversized 768 MiB + 1): stock keeps all 4 rows and drops
    only the damaged MESSAGE field.
- Unique-value enumeration returns decompression errors in systemd
  (`sd-journal.c` `sd_journal_enumerate_unique`), so it is out of scope.
- SDK sites: Go `recoverableEntryDataError` (`go/journal/reader_entry.go`) and
  `decompressDataPayload` (`go/journal/verify_graph.go:1166`); Rust
  `recoverable_entry_data_error` (`rust/src/journal/src/reader_helpers.rs:245`)
  and `JournalError::DecompressorError`.
- `tests/interoperability/run_reader_hardening.py` streams cases expect an
  entry error for the corrupt and oversized cases.

Unknowns:

- None beyond the open decision.

### Acceptance Criteria

- In both languages, current-entry reads (`GetEntry`/`get_entry`, payload
  visitors, entry enumeration, facade data getters, Netdata row expansion)
  drop a field whose compressed DATA fails to decompress, per the decision.
- Strict verification still fails such files.
- The hardening streams cases expect stock's rows, and the codec differential
  matches stock exit status and row and field counts.

## Analysis

See Facts. The change is a classification of decompression errors in one
predicate per language, plus the consequences in the hardening matrix, tests
and docs (`docs/Reader-APIs.md`, `docs/Rust-API.md`, `product-scope.md`).

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- Decompression errors are not in the recoverable set in either language.

Evidence reviewed:

- See Facts.

Affected contracts and surfaces:

- Go and Rust reader current-entry APIs, facades, Netdata row expansion, both
  journalctl rewrites through them.

Existing patterns to reuse:

- The SOW-0176 recoverable-error predicates and damage tests.

Risk and blast radius:

- A row can silently lose a field; the field-level drop is what systemd and the
  Netdata plugin already show. Verification stays strict.

Sensitive data handling plan:

- Synthetic fixtures only.

Implementation plan:

1. Classify decompression failures per the decision in both languages.
2. Paired damage tests per codec; update hardening expectations; docs and spec.

Validation plan:

- Unit tests, `run_reader_hardening.py`, `codec_diff.py`, query and verify
  matrices.

Artifact impact plan:

- AGENTS.md: none expected.
- Runtime project skills: none expected.
- Specs: `product-scope.md` recovery text.
- End-user/operator docs: `docs/Reader-APIs.md`, `docs/Rust-API.md`.
- End-user/operator skills: none.
- SOW lifecycle: pending until the decision.

Open-source reference evidence:

- Cited in Facts.

Open decisions:

- xz handling (see Implications And Decisions).

## Implications And Decisions

- 2026-11-09, decision 1B (SOW-0176 review): skip the field, as systemd does.
- Open: systemd fails the whole read on corrupt xz (`-ENOMEM`) while skipping
  corrupt zstd and lz4. A: follow systemd per codec (xz keeps failing). B: skip
  the field for every codec, a recorded deviation for xz.

## Plan

See the implementation plan above.

## Implementation And Review Plan

Implementation:

- The project manager implements both languages in one chunk with tests.

Reviewers:

- Whole-SOW self-review after local validation; tests are the gate. External
  reviewers are not used.

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

## Execution Log

### 2026-11-09

- Created with the stock differential above.

## Validation

Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None.
