# SOW-0178 - Undecompressible DATA Costs One Field, Not The Entry

## Status

Status: completed

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: completed 2026-11-09 (user decisions 1B and xz = B). Branch `sow-0178/undecompressible-data-field`,
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

Status: ready

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

- None; xz decided 2026-11-09 (B).

## Implications And Decisions

- 2026-11-09, decision 1B (SOW-0176 review): skip the field, as systemd does.
- 2026-11-09, xz = B (user): systemd fails the whole read on corrupt xz
  (`-ENOMEM`) while skipping corrupt zstd and lz4; the SDK skips the field for
  every codec. The xz difference is recorded in docs, spec and code comments.

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
- Implemented: Go wraps every decompression failure in `errDataDecompression`
  (`decompressDataPayload`), which `recoverableEntryDataError` accepts. Rust
  moves the predicate to `JournalError::is_recoverable_entry_data` in
  `journal-core` (one owner for the SDK and `journal-engine`) and adds
  `DecompressorError`. Explorer facet classification (Go
  `classifyDataForAccumulator`, Rust `classify_data_for_accumulator`) and the
  Rust `journal-engine` row projection skip the field too. Go Netdata rows go
  through `GetEntry`/`CollectEntryPayloads`, already covered.
- `TestNativeFilterDoesNotDecodeUnrelatedCompressedData` proved a payload read
  decodes the damaged DATA by expecting `GetEntry` to fail; it now checks that
  MESSAGE is dropped, which proves the same thing.
- Duplication gate: the new Rust `impl` first lengthened a baselined block shared
  with the legacy `jf` copy; placing it at the end of `error.rs` fixed it.

## Validation

Acceptance criteria evidence:

- Current-entry reads, visitors, enumeration, facade getters, Explorer facets
  and Rust row projection drop the damaged field: paired tests
  `TestDamagedCompressedDataCostsTheFieldNotTheEntry` /
  `damaged_compressed_data_costs_the_field` (zstd, xz, lz4) and
  `TestExplorerSkipsDamagedCompressedData` /
  `explorer_skips_damaged_compressed_data`. Both explorer tests fail with the
  explorer change disabled.
- Strict verification still fails: asserted in the same tests and in the
  hardening matrix (`verify` probe and stock verify).
- Stock parity (`.local/sow0178/codec_diff.py`): zstd bit flip and zeroed tail,
  lz4 stored size +1 and -1: stock, Go and Rust all rc 0, 5 rows, 70 of 75
  fields. xz: Go and Rust rc 0, 5 rows, 70 fields; stock rc 1 (decided
  difference).

Tests or equivalent validation (2026-11-09, stock = systemd 262 oracle):

- `go test ./journal ./cmd/...` PASS; `cargo test --workspace` PASS;
  `cargo fmt --check` clean; no new clippy warnings in changed code.
- `run_reader_hardening.py` (both adapters, v262 `--journalctl`, new `.local`
  `--out`, all groups): PASS, 552 observations; the streams cases now expect
  stock's rows and check them against stock `-o export`.
- `run_compression_matrix.py` 24/24, `run_verify_matrix.py` PASS,
  `run_mixed_directory_matrix.py` 42/42, `run_live_matrix.py` 18/18,
  `verify_examples.py` and `check_wiki_docs.py` PASS, `check_duplication.py`
  PASS.
- `run_journalctl_query_matrix.py` with follow: first run had one failure,
  `follow-grep-boot-events` with the stock reader ("go live fixture writer did
  not become ready", fixture start-up timing); the rerun passed with 0 failures.

Real-use evidence:

- Both built CLIs against stock v262 on the corrupted codec files above.

Reviewer findings:

- Project-manager review of the whole change; no external reviewers. Unique
  enumeration (`EnumerateUniquePayload`, `enumerate_unique_payload`, Explorer
  indexed values) still returns the error, as systemd's
  `sd_journal_enumerate_unique` does. `UnknownCompressionMethod` stays an
  error (systemd: `-EPROTONOSUPPORT`, not skipped by `enumerate_data`).

Same-failure scan:

- Every caller of `decompressDataPayload` and `.decompress(` was checked:
  writer dedup and verification keep failing (the wrapped error still fails
  them); unique and boot-listing paths keep the error by design.

Sensitive data gate:

- Synthetic fixtures and public systemd references only.

Artifact maintenance gate:

- AGENTS.md: no change.
- Runtime project skills: no change; the hardening recipe is unchanged.
- Specs: `product-scope.md` (field-level DATA damage).
- End-user/operator docs: `docs/Reader-APIs.md`.
- End-user/operator skills: none exist.
- SOW lifecycle: `Status: completed`, moved to `done/`.
- SOW-status.md: updated.

Specs update:

- `product-scope.md`.

Project skills update:

- None needed.

End-user/operator docs update:

- `docs/Reader-APIs.md`.

End-user/operator skills update:

- None exist.

Lessons:

- See `## Lessons Extracted`.

Follow-up mapping:

- None.

## Outcome

A damaged compressed DATA object now costs one field in both SDKs and CLIs,
matching stock v262 for zstd and lz4; xz deliberately drops the field where
stock fails the read.

## Lessons Extracted

- "As systemd does" must be checked per code path: systemd's own errno choice
  (xz `-ENOMEM`) decided its behavior, not the kind of damage.

## Followup

None.

## Regression Log

None.
