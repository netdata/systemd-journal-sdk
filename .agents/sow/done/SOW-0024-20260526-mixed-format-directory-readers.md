# SOW-0024 - Mixed Format Directory Readers

## Status

Status: completed

Sub-state: Completed on 2026-05-26 after local implementation, validation, and two read-only reviewer rounds.

## Requirements

### Purpose

Ensure SDK directory readers and file-backed journalctl rewrites can read realistic journal directories where files do not all share the same on-disk feature set. This is required before Netdata can safely switch writers to compact-by-default output while still reading old regular files, mixed compression histories, and sealed/unsealed files in the same directory.

### User Request

Create an SOW to ensure mixed formats are supported in the same directory for readers: compression on/off, mixed compression algorithms, compact format on/off, FSS on/off, and similar per-file feature combinations. The reader should work in a mixed-files directory without issues.

### Assistant Understanding

Facts:

- The product scope already includes regular and compact journal formats, DATA compression with zstd/xz/lz4, whole-file `.journal.zst` support where implemented, and Forward Secure Sealing support.
- Existing compatibility matrices prove important file-level behavior, but mixed-directory behavior can fail if a reader assumes one directory-wide format, compression mode, sealing mode, or verification policy.
- Directory traversal parity is already tracked by SOW-0020; this SOW is narrower and focuses on per-file feature heterogeneity inside one accepted directory set.

Inferences:

- Real migration directories will contain old regular files and new compact files after writer defaults change.
- Compression mode is a per-DATA-object/per-file feature and must not be inferred from the first file in the directory.
- FSS/sealing is a per-file feature. Normal reading must not require a verification key, while explicit verification must validate sealed files with the provided key and reject missing/wrong keys in the same way as file-level verification.

Unknowns:

- Whether all four SDK directory readers already open every file independently enough for mixed feature flags, or whether some shared directory-level state leaks across files.
- Whether mixed whole-file `.journal.zst` plus uncompressed `.journal` directory traversal belongs here or should remain in SOW-0020. This SOW should include it if the existing file-backed directory APIs already claim `.journal.zst` support.

### Acceptance Criteria

- A shared mixed-format fixture or generator creates one directory containing regular and compact journal files, uncompressed and DATA-compressed files, multiple DATA compression algorithms, sealed and unsealed files, active and archived files where supported, and whole-file `.journal.zst` files where supported.
- Rust, Go, Node.js, and Python directory readers read every accepted entry from that directory without assuming a single directory-wide compact/compression/FSS mode.
- Rust, Go, Node.js, and Python file-backed journalctl rewrites read the same accepted entries from the mixed directory for supported output/query modes.
- Stock `journalctl --directory` is used as the ordering and field-output comparison authority for the supported mixed-directory cases.
- Explicit verification paths validate each file according to its own feature flags: unsealed files verify without keys; sealed files require the correct key; compact and compressed combinations verify when the file-level verifier supports them.
- Tests prove readers continue after encountering a supported different-format file and fail only for deliberately corrupted or unsupported fixtures with controlled errors.
- No implementation writes, edits, or deletes files outside this repository except `/tmp`.

## Analysis

Sources checked:

- `.agents/sow/specs/product-scope.md`
- `SOW-status.md`
- `tests/interoperability/run_compression_matrix.py`
- `tests/interoperability/run_compact_matrix.py`
- `tests/conformance/manifests/conformance-v01.json`
- SOW-0020 directory traversal parity scope
- SOW-0022 compatibility test gap audit

Current state:

- Product scope lists regular/compact, compression, and FSS support as implemented feature areas.
- SOW-0020 remains responsible for broad directory traversal parity and global ordering behavior.
- SOW-0022 records live and compatibility test gaps around compression, compact layout, and FSS.

Risks:

- A directory reader that caches compact/compression/sealing state at directory scope can silently misread later files.
- A migration to compact-by-default writers can make old regular files unreadable if mixed directories are not tested.
- Verification key behavior can become too strict if sealed files force keys for unsealed files in the same directory.
- Adding mixed fixtures without strict stock comparison can hide ordering regressions that belong to SOW-0020.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Feature compatibility has been validated largely file-by-file. Real directories can contain mixed historical and current files, especially after writer defaults change. Readers must detect format, compression, and sealing per file/object instead of treating these as directory-wide properties.

Evidence reviewed:

- Product scope records support for regular/compact files, compression, and sealed verification.
- Existing SOW-status keeps SOW-0020 and SOW-0022 open for directory and compatibility test gaps.
- Current tests include file-level compression, compact, and FSS coverage but this SOW exists because the user requires mixed-directory coverage explicitly.

Affected contracts and surfaces:

- Rust, Go, Node.js, and Python `DirectoryReader` APIs.
- Rust, Go, Node.js, and Python file-backed journalctl rewrites.
- Verification APIs when run over directories or file sets.
- Shared conformance and interoperability fixtures.
- Netdata reader integrations that scan directories across writer migrations.

Existing patterns to reuse:

- Existing compression matrix fixture generation.
- Existing compact matrix fixture generation.
- Existing sealed verification tests and deterministic test keys.
- Existing directory traversal matrix work from SOW-0020 where it can be shared without merging ownership.

Risk and blast radius:

- Medium. Reader changes affect directory traversal and journalctl output, but should not alter writer file generation.
- Compatibility risk is high if left untested because compact-by-default Netdata writers will create mixed old/new directories.

Sensitive data handling plan:

- Use only synthetic generated fixtures. Do not copy real host journals, customer logs, private endpoints, SNMP communities, bearer tokens, or personal data into durable artifacts.

Implementation plan:

1. Inventory current mixed-directory behavior in Rust, Go, Node.js, and Python readers using synthetic files only.
2. Add a shared mixed-format fixture generator that writes a deterministic directory matrix with regular/compact, compression variants, sealed/unsealed variants, and whole-file `.journal.zst` where supported.
3. Run stock `journalctl --directory` against the fixture and record expected accepted output for supported modes.
4. Update readers and journalctl rewrites so feature detection is per file/object.
5. Add verification tests for mixed sealed/unsealed directories with correct missing-key/wrong-key behavior.

Validation plan:

- Shared mixed-directory matrix across Rust, Go, Node.js, and Python.
- Stock `journalctl --directory` comparison for accepted output.
- `journalctl --verify` or repository verification checks for each generated file feature combination.
- Existing compression, compact, FSS, live, and directory traversal tests remain passing.
- External reviewer pass for compatibility and unwanted side effects.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update journal compatibility skill if mixed-directory matrix becomes mandatory for reader changes.
- Specs: update product scope with the mixed-directory guarantee once implemented.
- End-user/operator docs: update README/journalctl docs if user-visible directory guarantees change.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: pending until activated; coordinate with SOW-0020 and SOW-0022 to avoid duplicate ownership.
- SOW-status.md: updated when created, activated, and closed.

Open-source reference evidence:

- No new external repository inspection was required for SOW creation. Implementation should use `systemd/systemd` v260.1 as the stock behavior authority.

Open decisions:

- None blocking activation. If whole-file `.journal.zst` directory traversal is found to belong entirely to SOW-0020, record evidence and leave this SOW focused on mixed per-file journal feature flags.

## Implications And Decisions

1. Mixed-directory target
   - Decision: the target is per-file feature heterogeneity inside one supported directory, not a replacement for full directory traversal parity.
   - Reason: SOW-0020 already owns broad traversal and ordering parity.
   - Risk: the SOWs must share fixtures carefully so the same directory-order bug is not fixed twice.

## Plan

1. Build mixed-format fixture generator and stock expected output.
2. Run all four directory readers and journalctl rewrites against the fixture.
3. Fix per-file feature detection and verification behavior.
4. Update specs/docs and reviewer evidence.

## Delegation Plan

Implementer:

- Current routing is local implementation by the project manager unless the user explicitly re-enables external implementers.

Reviewers:

- Use read-only reviewers from the approved pool: `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`. Skip `llm-netdata-cloud/mimo-v2.5-pro` while unavailable.

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

- Record implementation failures, reviewer failures, audit failures, fixture-generation uncertainty, and stock-tool incompatibilities in this SOW before changing scope.

## Execution Log

### 2026-05-26

- Created SOW from user request while SOW-0023 review was running.
- Activated after SOW-0020 completed, committed, and pushed.
- Added `tests/interoperability/run_mixed_directory_matrix.py`.
  - Generates a stock-supported mixed directory with regular and compact files, uncompressed and zstd/xz/lz4 DATA-compressed files, sealed and unsealed files, active `.journal` and archived `.journal~` names, deterministic IDs, deterministic timestamps, and a shared deterministic verification key.
  - Generates an unsealed-only mixed directory to prove directory verification succeeds without a key.
  - Generates a repository-extension mixed directory with active and archived whole-file `.journal.zst` / `.journal~.zst` files, including sealed whole-file zstd verification with and without `--verify-key`; stock journalctl is intentionally excluded from this extension check because systemd v260.1 directory enumeration accepts `.journal` and `.journal~` names only.
  - Validates generated fixture flags before running readers: keyed hash, compact flag, DATA compression header/object flags, sealed flag, and whole-file zstd decompression.
  - Compares stock journalctl, Go, Rust, Node.js, and Python file-backed `journalctl --directory` behavior for JSON, export, text, fields, boot listing, repeated same-field OR, cross-field AND, `+` disjunction, missing-key sealed verification failure, correct-key verification success, wrong-key failure, and unsealed no-key verification success.
- No reader implementation changes were required. The existing per-file open/decode/verify behavior already handles the mixed-directory cases once covered by the shared matrix.
- Updated durable artifacts:
  - `.agents/sow/specs/product-scope.md` now records the mixed-directory guarantee and removes SOW-0024 as a current reader limitation.
  - `tests/interoperability/README.md` documents `run_mixed_directory_matrix.py` and marks mixed-format directory readers complete.
  - `go/README.md`, `rust/README.md`, `node/README.md`, and `python/README.md` now describe mixed regular/compact, compressed/uncompressed, sealed/unsealed directory support.
  - `.agents/skills/project-journal-compatibility/SKILL.md` now requires the mixed-directory matrix for future mixed directory feature changes.

## Validation

Local validation completed and read-only external review passed.

- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_mixed_directory_matrix.py --keep-files` - PASS after reviewer hygiene fixes.
  - Stock version recorded by the runner: `systemd 260 (260.1-2-manjaro)`.
  - Summary: 72 total checks, 72 passed, 0 failed.
  - Readers: stock, Go, Rust, Node.js, Python.
  - Covered stock-supported mixed directory reads and verification plus repository whole-file `.journal.zst` extension reads and verification.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_directory_matrix.py --keep-files` - PASS.
- `GOMODCACHE=.local/go/pkg/mod GOCACHE=.local/go-build GOPATH=.local/go go test ./journal ./cmd/journalctl` in `go/` - PASS.
- `CARGO_HOME=.local/cargo-home CARGO_TARGET_DIR=.local/cargo-target cargo test -p journal -p journalctl` in `rust/` - PASS.
- `node --check cmd/journalctl/index.js && node --check src/lib/directory-reader.js && node --check src/lib/reader.js && npm test -- --runInBand` in `node/` - PASS.
- `python3 -m py_compile python/cmd/journalctl.py python/journal/directory_reader.py python/journal/reader.py tests/interoperability/run_directory_matrix.py tests/interoperability/run_mixed_directory_matrix.py && .local/python-venv/bin/python python/test_all.py` - PASS.

Reviewer gate:

- Initial batched read-only external review returned production-grade from minimax, kimi, qwen, and glm with non-blocking findings only.
- Addressed valid hygiene findings before close: removed unused fixture metadata, added archived `.journal~.zst` coverage, added zst no-key verification failure coverage, added per-entry `_BOOT_ID`/`_MACHINE_ID` validation, added an upfront `zstd` dependency check, fixed fixture cleanup when `--keep-files` is absent, and corrected fixture-tree wording.
- Follow-up read-only review after fixes returned production-grade from minimax, kimi, qwen, and glm with only close-gate/doc housekeeping findings.
- Same-failure search: checked `tests/interoperability/` and this SOW for the reviewer finding classes (`stock_supported`, zst no-key verification, `_BOOT_ID`, `_MACHINE_ID`, `shutil.which("zstd")`, cleanup semantics, `68/68`, and one-directory wording). No additional same-failure implementation instances remain.

Sensitive data gate:

- Durable artifacts contain only synthetic fixture IDs, deterministic test keys, and repo-local paths. No raw secrets, credentials, bearer tokens, SNMP communities, customer identifiers, personal data, private endpoints, or proprietary incident details were added.

Artifact maintenance gate:

- `AGENTS.md`: no update needed; project-wide workflow and repository boundary rules were unchanged.
- Runtime project skills: `.agents/skills/project-journal-compatibility/SKILL.md` now requires the mixed-directory matrix for mixed directory feature changes.
- Specs: `.agents/sow/specs/product-scope.md` now records the mixed-directory guarantee and removes SOW-0024 from current reader limitations.
- End-user/operator docs: Go, Rust, Node.js, Python, and interoperability READMEs were updated for the mixed-directory guarantee and matrix.
- End-user/operator skills: none affected; no output/reference skills exist for this project.
- SOW lifecycle: SOW-0024 moved from `pending/` to `current/` on activation and is moved to `done/` at close.
- `SOW-status.md`: updated for activation and close.
- Follow-up mapping: no new SOW is needed. SOW-0022 still tracks broader compatibility gaps; SOW-0009 still tracks benchmark/profile/optimization; SOW-0026 still tracks Netdata integration after performance is acceptable.

## Outcome

Completed. The mixed-format directory matrix now validates stock-supported mixed directories and repository whole-file zstd extension directories across stock journalctl plus Rust, Go, Node.js, and Python rewrites. The final matrix passes 72/72 checks on systemd 260.1-2-manjaro. No reader implementation changes were required because existing readers already detect compact layout, compression, sealing, and zstd wrapping per file/object.

## Lessons Extracted

- Mixed-directory confidence should be tested directly. File-level compact, compression, and FSS matrices were necessary but did not prove that directory readers avoided directory-wide feature assumptions.
- The existing per-file open/decode/verify model was the right design. The SOW mainly added proof, not reader changes.
- Repository-only whole-file `.journal.zst` behavior needs explicit stock-exclusion notes because stock systemd v260.1 directory enumeration does not accept that extension.

## Followup

No new follow-up. Remaining broader verification parity, performance optimization, and Netdata integration work remains in SOW-0022, SOW-0009, and SOW-0026 respectively.

## Regression Log

None yet.
