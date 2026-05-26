# SOW-0028 - Historical Header Parsing Parity

## Status

Status: open

Sub-state: Split from SOW-0022 Gap 4. Ready for implementation after activation.

## Requirements

### Purpose

Ensure Rust, Go, Node.js, and Python readers expose historical journal header fields exactly according to the on-disk `header_size`, so valid older journal files are not misread or silently reported with zeroed metadata.

### User Request

Port/fix compatibility gaps found during the journal compatibility audit, while keeping work in small production-grade chunks before performance work and Netdata integration.

### Assistant Understanding

Facts:

- `product-scope.md` requires header parsing to respect on-disk `header_size`.
- Rust already sanitizes header fields per field.
- Go, Node.js, and Python currently read extension fields only when the header is at the current v260 size.

Inferences:

- Historical files with `header_size` between older extension boundaries and the v260 header size can be read with valid fields incorrectly reported as zero in Go, Node.js, and Python.

Unknowns:

- None requiring user input. Exact historical boundary values are defined by systemd and can be validated locally.

### Acceptance Criteria

- Shared tests cover historical header sizes `208`, `216`, `224`, `232`, `240`, `248`, `256`, `260`, `264`, and `272`.
- Rust, Go, Node.js, and Python expose present fields and zero/default absent fields consistently.
- Existing fixture readers still pass.
- Stock systemd fixture evidence is recorded where used.

## Analysis

Sources checked:

- `product-scope.md`
- `SOW-0022-20260525-compatibility-test-gap-audit.md`
- `go/journal/format.go`
- `node/src/lib/header.js`
- `python/journal/header.py`
- `rust/src/crates/journal-core/src/file/file.rs`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`

Current state:

- Go parses `n_data`, `n_fields`, `n_tags`, `n_entry_arrays`, hash-chain depths, and tail-entry-array fields only when `headerSize >= headerSize` for the current 272-byte header.
- Node.js and Python initialize extension fields to zero and then parse them only when `header_size >= HEADER_SIZE`.
- Rust uses per-field handling and is the local pattern to reuse.

Risks:

- Incorrect header metadata can break validation, count reporting, historical fixture compatibility, and future repair/append workflows.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The parsers in Go, Node.js, and Python use an all-or-nothing current-header-size guard. Systemd uses per-field containment checks, so some older but valid headers contain fields the SDKs currently leave as zero.

Evidence reviewed:

- `go/journal/format.go:270-285`
- `node/src/lib/header.js:132-152`
- `python/journal/header.py:108-129`
- `rust/src/crates/journal-core/src/file/file.rs:343-371`
- `product-scope.md` header parsing contract
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-file.h:179-180`

Affected contracts and surfaces:

- Reader header APIs.
- Verification code that relies on header counters.
- Conformance manifests/adapters.
- Product scope if the behavior changes in a documented way.

Existing patterns to reuse:

- Rust per-field sanitization.
- Shared conformance manifest/adapters.
- Synthetic fixture generation under repository-local test paths.

Risk and blast radius:

- Medium compatibility risk, low implementation risk. Changes are limited to header parsing and tests, but affect all reads in three languages.

Sensitive data handling plan:

- Use synthetic fixtures or committed public fixtures only. Do not read or copy live host journals.

Implementation plan:

1. Add shared historical header parsing fixture cases or a generator that produces minimal valid headers at each boundary size.
2. Update Go, Node.js, and Python parsers to use per-field containment checks.
3. Ensure Rust passes the same tests without behavior regression.
4. Update specs/docs only if public behavior is newly documented.

Validation plan:

- Run targeted header parsing tests in all four languages.
- Run shared conformance readers for all languages.
- Run stock journalctl/libsystemd checks for any full journal fixtures used.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: no expected update unless a new mandatory header-fixture workflow is introduced.
- Specs: update `product-scope.md` if the current behavior text needs more precision.
- End-user/operator docs: likely unaffected.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: activate only this SOW when implementing.
- SOW-status.md: update on activation and completion.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  - `src/libsystemd/sd-journal/journal-def.h`
  - `src/libsystemd/sd-journal/journal-file.h`

Open decisions:

- None.

## Implications And Decisions

- No user decision is required before implementation.

## Plan

1. Add failing tests first.
2. Patch affected parsers.
3. Run cross-language validation.
4. Use read-only reviewers after the implementation batch.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current routing decision.

Reviewers:

- Read-only reviewers from the active pool after implementation: minimax, kimi, qwen, glm. Mimo is skipped.

Failure handling:

- Record implementation, reviewer, or audit failure here before changing scope.

## Execution Log

Pending.

## Validation

Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
