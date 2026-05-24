# SOW-0017 - XZ And LZ4 DATA Writing

## Status

Status: in-progress

Sub-state: active after SOW-0016 byte-identical regular writer closeout.

## Requirements

### Purpose

Complete the remaining systemd-defined DATA-object compression writer formats beyond zstd, while preserving pure-language SDK guarantees and stock reader compatibility.

### User Request

The user requires journal writers to be compatible with stock journalctl and libsystemd readers. SOW-0008 delivered zstd DATA writing; xz and lz4 DATA writing remain open writer feature gaps.

### Assistant Understanding

Facts:

- zstd-compressed DATA object writing is implemented and validated across Rust, Go, Node.js, and Python.
- xz and lz4-compressed DATA object writing is not implemented.
- Rust reader support already handles xz/lz4 DATA objects through pure Rust dependencies; Go, Node.js, and Python current reader slices reject xz/lz4 DATA objects.
- Pure-language dependencies are allowed after dependency review. CGO, native Node.js addons, and system journal libraries remain forbidden.

Inferences:

- Reader support for xz/lz4 may need to be implemented before writer parity can be claimed for all languages.
- Dependency availability may differ by language and compression family, so this SOW may split xz and lz4 into separate implementation chunks if one format is ready before the other.

Unknowns:

- Which pure-language xz/lz4 dependencies are acceptable for Go, Node.js, and Python after license, maintenance, performance, and compatibility review.
- Whether stock systemd v260.1 accepts all chosen pure-library frame outputs without additional frame metadata normalization.

### Acceptance Criteria

- A dependency review records pure-language xz and lz4 options for Rust, Go, Node.js, and Python.
- Readers in all four languages either support xz/lz4-compressed DATA objects or the SOW stops with evidence and a user decision before writer claims are weakened.
- Writers in all four languages can write xz and lz4-compressed DATA objects when configured, or the SOW is split by compression family with evidence.
- A shared compression matrix proves header/object flags, stock `journalctl --verify --file`, stock journalctl reads, stock libsystemd reads, and all repository readers for every implemented compression family.
- Uncompressed and zstd writing remain compatible and unchanged.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0008-20260523-interoperability-and-full-writer-features.md`
- `.agents/sow/specs/product-scope.md`
- `tests/interoperability/run_compression_matrix.py`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/journal-def.h`
- `src/libsystemd/sd-journal/journal-file.c`

Current state:

- zstd DATA writing passes the compression interoperability matrix.
- xz/lz4 writing remains unimplemented; Go, Node.js, and Python readers currently reject xz/lz4 DATA objects.

Risks:

- Compression frame details can be library-specific while still semantically valid.
- New compression dependencies can add maintenance or security risk.
- Performance can regress if compression objects are allocated per DATA value without pooling.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The final writer target includes compression where systemd journal files define it. zstd is complete, but xz and lz4 are still missing, leaving the compression writer target incomplete.

Evidence reviewed:

- SOW-0008 records xz/lz4 DATA writing as an explicit remaining writer gap.
- Product scope lists xz/lz4 DATA object writing as unimplemented for current SDK slices.
- systemd journal object flags define xz, lz4, and zstd DATA compression families.

Affected contracts and surfaces:

- Writer compression options.
- Reader DATA decompression behavior.
- File-backed journalctl JSON/export/text output.
- Compression interoperability matrix.
- Dependency policy and documentation.

Existing patterns to reuse:

- zstd writer options and threshold behavior from SOW-0008.
- `tests/interoperability/run_compression_matrix.py`.
- Per-language livewriter compression fixture modes.
- Stock journalctl and libsystemd validation helpers.

Risk and blast radius:

- Medium to high. Compression touches writer object storage, reader parsing, matching, and dependency surfaces across all languages.

Sensitive data handling plan:

- Use synthetic compression fixtures only. Durable artifacts record commands, verdicts, dependency names, licenses, and sanitized diagnostics; no secrets or customer data.

Implementation plan:

1. Inventory systemd xz/lz4 frame requirements and pure-language dependency candidates per language.
2. Add reader support where missing before writer compatibility is claimed.
3. Add writer options for xz/lz4 using the existing compression-option pattern.
4. Extend the compression matrix by compression family.
5. Run full regression matrices and dependency review.

Validation plan:

- Extended compression matrix for every implemented family.
- Existing zstd, binary, live, and closed-file matrices remain passing.
- Language package tests remain passing.
- Dependency audit records pure-language status and licenses.
- External reviewers confirm no native linkage or compatibility weakening.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update if compression workflow changes durable future validation.
- Specs: update product scope with exact reader/writer support per language.
- End-user/operator docs: update README support matrices.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: remains pending until activated; may split xz/lz4 if dependency evidence requires.
- SOW-status.md: update when activated or closed.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/journal-def.h`
- `src/libsystemd/sd-journal/journal-file.c`

Open decisions:

- None blocking activation. If pure-language dependency review fails for a language/compression family, stop and present evidence before changing scope.

## Implications And Decisions

1. Compression-family boundary
   - Decision: track xz and lz4 in one SOW initially, but allow splitting by compression family after dependency evidence.
   - Reason: both families share the same journal object/header mechanics, but dependency feasibility may differ.
   - Risk: forcing both families into one implementation chunk could delay a production-ready subset.

## Plan

1. Review systemd xz/lz4 implementation and pure dependency options.
2. Implement missing reader support.
3. Implement writer support by compression family.
4. Extend shared matrix and docs.
5. Review and commit verified chunks.

## Delegation Plan

Implementer:

- Preferred implementer is `llm-netdata-cloud/minimax-m2.7-coder`.

Reviewers:

- At least two reviewers from the approved pool.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- DO NOT MAKE CHANGES OUTSIDE THIS REPOSITORY FOR ANY REASON.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- Record implementer failure, reviewer failure, audit failure, dependency rejection, or model unavailability before changing plan or model.

## Execution Log

- 2026-05-24: Activated after SOW-0016 completion and after sequencing SOW-0009 behind remaining feature-completeness SOWs.

## Validation

Acceptance criteria evidence:

- Pending implementation.

Tests or equivalent validation:

- Pending implementation.

Real-use evidence:

- Pending implementation.

Reviewer findings:

- Pending implementation.

Same-failure scan:

- Pending implementation.

Sensitive data gate:

- Pending implementation. Planned evidence uses synthetic compression fixtures, dependency names, licenses, commands, and sanitized diagnostics only; durable artifacts must not contain secrets, credentials, bearer tokens, SNMP communities, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details.

Artifact maintenance gate:

- AGENTS.md: pending implementation.
- Runtime project skills: pending implementation.
- Specs: pending implementation.
- End-user/operator docs: pending implementation.
- End-user/operator skills: pending implementation.
- SOW lifecycle: activated in `.agents/sow/current/` with `Status: in-progress`; completion requires status update to `completed`, move to `.agents/sow/done/`, audit, review disposition, and one implementation-close commit.
- SOW-status.md: updated on activation.

Specs update:

- Pending implementation.

Project skills update:

- Pending implementation.

End-user/operator docs update:

- Pending implementation.

End-user/operator skills update:

- Pending implementation.

Lessons:

- Pending implementation.

Follow-up mapping:

- Pending implementation.

## Outcome

Pending.

## Lessons Extracted

Pending activation.

## Followup

Pending activation.

## Regression Log

None yet.
