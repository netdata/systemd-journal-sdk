# SOW-0028 - Historical Header Parsing Parity

## Status

Status: completed

Sub-state: Completed after local implementation, read-only reviewer batch, hardening follow-up, and cross-language validation.

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

- Activated this SOW from pending to current and updated `SOW-status.md`.
- Implemented per-field historical header exposure in:
  - `go/journal/format.go`
  - `node/src/lib/header.js`
  - `python/journal/header.py`
- Added or expanded shared boundary tests in:
  - `go/journal/format_test.go`
  - `node/test/all.js`
  - `python/test_all.py`
  - `rust/src/crates/journal-core/src/file/file.rs`
  - `rust/src/crates/jf/journal_file/src/file.rs`
- Hardened future-header parsing after reviewer feedback:
  - Go now requires the buffer to contain `min(on_disk_header_size, current_known_header_size)`.
  - Node.js and Python now pass the source buffer to per-field containment helpers and require `len >= field_end` before reading.
  - Go, Node.js, and Python now test a `header_size=300` future header and reject a truncated 208-byte known-prefix buffer.
- Ran formatter commands:
  - `gofmt -w go/journal/format.go go/journal/format_test.go`
  - `cargo fmt`

## Validation

- Acceptance criteria:
  - Boundary coverage includes `208`, `216`, `224`, `232`, `240`, `248`, `256`, `260`, `264`, and `272` in all four languages.
  - Additional defense-in-depth cases cover intermediate sizes `220`, `250`, `268`, future size `300`, and truncated future known-prefix rejection for Go, Node.js, and Python.
  - Rust `journal-core` and vendored-compatible `jf/journal_file` both cover the same sanitize boundary matrix.
- Test evidence:
  - `go test ./...` from `go/`: passed.
  - `cargo test` from `rust/`: passed.
  - `node test/all.js` from `node/`: passed.
  - `.local/python-venv/bin/python python/test_all.py` from repo root: passed.
  - `python3 python/test_all.py` from repo root: failed because the base interpreter lacks `lz4.block`; repository venv validation above is the valid Python test path for this environment.
- Reviewer evidence:
  - `llm-netdata-cloud/minimax-m2.7-coder`: PASS, no blocking findings.
  - `llm-netdata-cloud/kimi-k2.6`: PASS, recommended Node.js/Python buffer-length hardening and `jf` test expansion.
  - `llm-netdata-cloud/qwen3.6-plus`: reported Node.js/Python buffer-length handling as blocking. The exact `header_size=272, len=200` example was already rejected by the existing guard, but the broader future-header truncated-prefix concern was valid and fixed.
  - `llm-netdata-cloud/glm-5.1`: PASS, recommended the same Node.js/Python future-header hardening.
- Reviewer disposition:
  - All repeated or actionable reviewer findings were implemented in this SOW.
  - The Qwen overstatement about current-size truncation was rejected with code evidence, while the underlying future-header safety issue was fixed.
  - Second full-scope review pass after fixes returned PASS / PRODUCTION GRADE from minimax, kimi, qwen, and glm with no blocking findings.
- Same-failure search:
  - `rg -n "header_size.*>=.*HEADER_SIZE|headerSize.*>=.*headerSize|if h\\.headerSize >= headerSize|if header\\['header_size'\\] >= HEADER_SIZE|if \\(header\\.header_size >= BigInt\\(HEADER_SIZE\\)\\)|headerContainsField\\(header\\.header_size|header_contains_field\\(header\\['header_size'\\]" go node python rust`
  - Result: no remaining all-or-nothing current-header-size guards or stale helper call sites.
- Sensitive data gate:
  - Only synthetic header fixtures and public source references were used.
  - No live journal contents, host logs, credentials, customer data, or private identifiers were copied into durable artifacts.
- Artifact maintenance gate:
  - `AGENTS.md`: no update required; workflow and guardrails unchanged.
  - Runtime project skills: no update required; compatibility process already requires cross-language reader/writer validation.
  - Specs: no update required; `.agents/sow/specs/product-scope.md` already states that header parsing must respect on-disk `header_size` and zero/default absent fields.
  - End-user/operator docs: no update required; this is internal parser compatibility behavior with no public API change.
  - End-user/operator skills: no update required; no published operator workflow changed.
  - SOW lifecycle: this SOW moved from pending to current, then to done with this implementation commit.
  - `SOW-status.md`: updated for activation and will be updated for completion.
- SOW audit:
  - `SOW_AUDIT_SENSITIVE_CHANGED=1 .agents/sow/audit.sh`: passed after moving this SOW to `done/`.

## Outcome

Rust, Go, Node.js, and Python now expose historical journal header extension fields per field, based on the on-disk `header_size`, instead of using a current-size all-or-nothing gate. Future header sizes preserve all known fields, truncated known-prefix buffers are rejected consistently, and both Rust reader crates carry the same boundary coverage.

## Lessons Extracted

- Historical journal compatibility needs per-field tests, not only version-boundary tests. Intermediate sizes caught the intended containment semantics more directly.
- Rust `journal-core` and `jf/journal_file` should stay aligned for compatibility-critical reader behavior because Netdata compatibility depends on the `jf` facade path too.
- Reviewer findings can be partially wrong but still useful; the current-size truncation example was already handled, while the future-header short-prefix variant exposed a real hardening gap.

## Followup

None for this SOW. Remaining compatibility and performance work is already tracked by pending SOWs.

## Regression Log

None yet.
