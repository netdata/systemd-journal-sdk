# SOW-0024 - Mixed Format Directory Readers

## Status

Status: open

Sub-state: Created from user request on 2026-05-26. Pending activation after the current SOW completes.

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

## Validation

Pending activation and implementation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
