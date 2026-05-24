# SOW-0016 - Byte Identical Writer Compatibility

## Status

Status: in-progress

Sub-state: active after SOW-0014 and SOW-0015 completion commits.

## Requirements

### Purpose

Prove that deterministic writer outputs from Rust, Go, Node.js, Python, and the systemd reference writer are byte-for-byte identical for the frozen accepted corpus, or stop with evidence for any impossible equivalence condition before weakening the goal.

### User Request

The user requested running all language ingesters and the systemd ingester over the same dataset, then requiring the generated binary journal files to be byte-for-byte identical.

### Assistant Understanding

Facts:

- Byte-for-byte identity is stronger than semantic journal compatibility.
- systemd initializes some fields through runtime state by default, including a randomized file ID and host machine ID.
- systemd append APIs can accept deterministic timestamps and boot IDs.
- Compression frame output may depend on compression implementation and version unless exact settings and frame properties are controlled.

Inferences:

- The first implementation target should be uncompressed deterministic journals, because compression and FSS can introduce valid byte differences unrelated to writer format bugs.
- The SOW must not silently downgrade to normalized or semantic comparison. If byte identity cannot be achieved for a feature slice, the SOW must record exact byte deltas and ask the user for a decision.

Unknowns:

- Whether zstd, xz, lz4, compact, or FSS slices can be made byte-identical to systemd across installed library versions without unacceptable coupling.

### Acceptance Criteria

- A committed byte-comparison harness runs the systemd, Rust, Go, Node.js, and Python ingesters against the same accepted corpus.
- For the deterministic uncompressed corpus, generated journal files are byte-for-byte identical across systemd and all SDK writers.
- On mismatch, the harness reports exact offsets, object type context, header field context, and probable source of the delta.
- The harness verifies that matching files also pass stock `journalctl --verify --file`, stock `journalctl --file` reads, stock libsystemd reads, and all repository readers.
- The harness refuses to treat normalized equality as pass unless a recorded user decision changes this SOW.
- Compression, compact, and FSS byte identity are either achieved in explicit feature slices or represented by concrete follow-up SOWs with mismatch evidence and user decisions.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/done/SOW-0014-20260524-deterministic-ingestion-dataset.md`
- `.agents/sow/done/SOW-0015-20260524-deterministic-ingesters.md`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/journal-file.c:401`
- `src/libsystemd/sd-journal/journal-file.c:431`
- `src/libsystemd/sd-journal/journal-file.c:435`
- `src/libsystemd/sd-journal/journal-file.c:2527`

Current state:

- Existing interoperability tests prove semantic and stock-reader compatibility for current writer slices, not byte-for-byte equality with systemd.
- SDK writers currently choose their own deterministic or random metadata depending on language options.

Risks:

- Byte-for-byte comparison can falsely fail because of allowed nondeterminism unless IDs, timestamps, compression, state, and allocation policies are controlled.
- Matching systemd byte layout too tightly can force implementation choices that harm maintainability or performance.
- Compression and FSS may require separate handling because valid files can differ byte-for-byte while remaining compatible.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The project has strong interoperability evidence but not byte-level proof that SDK writers match systemd's exact object construction. A deterministic corpus and deterministic ingesters are required before byte comparison can be meaningful.

Evidence reviewed:

- systemd `journal_file_init_header()` randomizes `file_id`, reads machine ID, and sets `seqnum_id`.
- systemd `journal_file_append_entry()` accepts deterministic timestamps, boot IDs, sequence numbers, and sequence IDs when provided by the caller.
- Existing matrix tests compare readable content, verification, and cross-language behavior rather than whole-file bytes.

Affected contracts and surfaces:

- Writer object allocation, ordering, deduplication, hash table linking, entry arrays, data entry arrays, header metadata, compression flags, and close state.
- Test helper CLIs.
- Byte diff diagnostic tooling.
- Specs and docs describing compatibility claims.

Existing patterns to reuse:

- `tests/interoperability/run_matrix.py`
- `tests/interoperability/run_compression_matrix.py`
- stock reader and libsystemd helper checks.

Risk and blast radius:

- High. This SOW may expose writer-format differences across all languages and force changes in shared writer algorithms.

Sensitive data handling plan:

- Use only synthetic SOW-0014 data. Diff logs must avoid dumping large binary payloads into durable artifacts; record offsets, object types, hashes, and short redacted hex windows only when needed.

Implementation plan:

1. Consume completed SOW-0014 dataset and completed SOW-0015 ingesters.
2. Build byte-comparison runner and object-aware diff diagnostics.
3. Run uncompressed deterministic corpus and fix writer mismatches.
4. Validate stock and repository readers on byte-identical files.
5. Inventory compression, compact, and FSS byte-identity feasibility and split follow-up SOWs where needed.

Validation plan:

- Byte-for-byte comparison passes for the uncompressed deterministic corpus across all writers.
- Stock verification and stock/repository reads pass for all generated files.
- Mismatch diagnostics are reviewed by external reviewers.
- SOW audit passes before close.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update if byte-comparison becomes mandatory for future writer changes.
- Specs: update compatibility claims with exact byte-identity scope.
- End-user/operator docs: update only if user-facing compatibility claims change.
- End-user/operator skills: no update expected.
- SOW lifecycle: active after SOW-0014 and SOW-0015 completion.
- SOW-status.md: update when created, activated, or closed.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/journal-file.c:401`
- `src/libsystemd/sd-journal/journal-file.c:431`
- `src/libsystemd/sd-journal/journal-file.c:435`
- `src/libsystemd/sd-journal/journal-file.c:2527`

Open decisions:

- None blocking SOW creation. If byte identity is impossible for a feature slice after deterministic controls are applied, implementation must stop and present evidence before accepting normalized equality.

## Implications And Decisions

1. Byte identity versus semantic compatibility
   - Decision: byte identity is the pass condition for the deterministic uncompressed corpus.
   - Reason: this is the strongest evidence that SDK writers match systemd object construction.
   - Risk: some valid feature slices, especially compression and FSS, may require separate user decisions if byte identity depends on external library internals.

## Plan

1. Build byte-comparison harness after SOW-0014 and SOW-0015.
2. Compare uncompressed deterministic output first.
3. Fix writer deltas until files match exactly.
4. Split compression, compact, and FSS byte identity work if needed.

## Delegation Plan

Implementer:

- Preferred implementer is `llm-netdata-cloud/minimax-m2.7-coder`.

Reviewers:

- At least two reviewer agents from the approved pool.

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

- Record implementer failure, reviewer failure, audit failure, or model unavailability in this SOW before changing plan or model.

## Execution Log

### 2026-05-24

- Activated after:
  - SOW-0014 completion commit `72d936f`.
  - SOW-0015 completion commit `cdd3795`.
- Updated baseline systemd evidence to the project compatibility target `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced` (`v260.1`).
- Confirmed SOW-0015 now provides deterministic ingesters for systemd C, Rust, Go, Node.js, and Python.
- Added the first byte-identity diagnostic harness at `tests/interoperability/run_byte_identity.py`.
- The harness runs the deterministic ingesters, compares generated `correctness.journal` files byte-for-byte, and reports exact offsets with header-field and object-span context.
- Initial full harness run result is an expected failure because the current writers are not byte-identical yet:
  - systemd output size: 8388608 bytes.
  - Rust, Go, Node.js, and Python output size: 427624 bytes.
  - systemd versus SDK first mismatch: offset 8, header field `compatible_flags`, systemd value `2`, SDK value `0`.
  - Go, Node.js, and Python are byte-identical to each other.
  - Go versus Rust first mismatch: offset 16, header field `state`, Go value `0`, Rust value `1`.
- Review round 1:
  - Minimax verdict: `PRODUCTION GRADE` for the diagnostic harness chunk. Non-blocking observations: add later verification after byte match, hash table diagnostics could become richer, truncated header diagnostics could be clearer.
  - Mimo verdict: `PRODUCTION GRADE` for the diagnostic harness chunk. Non-blocking observations: `probable_source` had an unreachable EOF branch, ingester subprocess lacked a timeout, and bytes 17-23 of the header were unnamed reserved padding.
- Dispositioned review round 1 low-severity findings:
  - Added a 300-second ingester subprocess timeout with structured timeout output.
  - Added the reserved header byte range to the header-field table.
  - Reworked `probable_source` to use already-computed contexts so EOF size mismatches are classified correctly.
- Review round 2:
  - Minimax verdict: `PRODUCTION GRADE` for the diagnostic harness chunk. Non-blocking observations: post-byte-match stock verification remains a later SOW-0016 phase; timeout configurability can be added later if needed.
  - Mimo verdict: `PRODUCTION GRADE` for the diagnostic harness chunk. Non-blocking observations: mixed EOF/header diagnostics could prioritize EOF more clearly; `--reference go` produced duplicate comparison pairs; object span recomputation is acceptable for diagnostic limits.
- Dispositioned review round 2 low-severity findings:
  - Moved EOF classification before header/object classification for clearer file-size mismatch diagnostics.
  - Added comparison-pair de-duplication so alternate references do not repeat equivalent pairs.
  - Left object-span memoization unchanged because the diagnostic loop is capped by `--diff-limit`; no reviewer considered it blocking.
- Review round 3:
  - Minimax verdict: `PRODUCTION GRADE` for the diagnostic harness chunk. Non-blocking observations: object-span parsing could be misleading for pathological corrupted object sizes, but this does not affect the deterministic corpus diagnostic purpose.
  - Mimo verdict: `PRODUCTION GRADE` for the diagnostic harness chunk. Non-blocking observations: `--diff-limit 0` returns the first byte difference, size-mismatch entries can exceed the byte-diff limit by one, object-span recomputation is acceptable, the file is invoked through Python rather than executable bit, and byte values are printed as integers.
  - No round 3 blocking findings.

## Validation

Activation evidence:

- Passed: SOW-0014 is completed in `.agents/sow/done/`.
- Passed: SOW-0015 is completed in `.agents/sow/done/`.
- Passed: SOW-0015 completion commit `cdd3795` exists before activation.
- Passed: no implementation changes made during activation.

Harness evidence:

- Passed: `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile tests/interoperability/run_byte_identity.py`.
- Expected failure: `python3 tests/interoperability/run_byte_identity.py --skip-run --diff-limit 4`.
  - Evidence stored outside committed artifacts at `.local/validation/sow-0016/byte-identity-skip-run.json`.
  - Result: `all_equal: false`.
  - Result: Go equals Node.js and Python byte-for-byte; Go differs from Rust only at header `state` in the first reported difference.
- Expected failure after regenerating journals: `python3 tests/interoperability/run_byte_identity.py --diff-limit 2`.
  - Evidence stored outside committed artifacts at `.local/validation/sow-0016/byte-identity-full-run.json`.
  - Ingesters returned `0`.
  - Result: `all_equal: false`.
  - Result: systemd versus every SDK first differs at header `compatible_flags`.
- Passed after reviewer cleanup: `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile tests/interoperability/run_byte_identity.py`.
- Expected failure after reviewer cleanup: `python3 tests/interoperability/run_byte_identity.py --skip-run --diff-limit 4`.
- Expected failure after reviewer cleanup and regenerated journals: `python3 tests/interoperability/run_byte_identity.py --diff-limit 2`.
  - Ingesters returned `0`.
  - Result: `all_equal: false`.
  - Result: current mismatch classification remains stable after cleanup.
- Passed after round 2 cleanup: `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile tests/interoperability/run_byte_identity.py`.
- Passed round 2 cleanup spot checks:
  - `comparison_pairs("systemd")` returns 7 unique pairs.
  - `comparison_pairs("go")` returns 4 unique pairs with no duplicates.
  - EOF-vs-header source classification returns `file size, allocation, or truncation policy`.
- Expected failure after round 2 cleanup: `python3 tests/interoperability/run_byte_identity.py --skip-run --diff-limit 4`.
- Expected failure after round 2 cleanup and regenerated journals: `python3 tests/interoperability/run_byte_identity.py --diff-limit 2`.
  - Ingesters returned `0`.
  - Result: `all_equal: false`.
  - Result: comparison count remains 7 for the default `systemd` reference.

Reviewer evidence:

- Round 1 Minimax: `PRODUCTION GRADE` for this diagnostic harness chunk; no blocking findings.
- Round 1 Mimo: `PRODUCTION GRADE` for this diagnostic harness chunk; no blocking findings.
- Round 1 low-severity findings were fixed before commit.
- Round 2 Minimax: `PRODUCTION GRADE` for this diagnostic harness chunk; no blocking findings.
- Round 2 Mimo: `PRODUCTION GRADE` for this diagnostic harness chunk; no blocking findings.
- Round 2 low-severity findings were fixed before commit.
- Round 3 Minimax: `PRODUCTION GRADE` for this diagnostic harness chunk; no blocking findings.
- Round 3 Mimo: `PRODUCTION GRADE` for this diagnostic harness chunk; no blocking findings.

Sensitive data gate:

- Activation edits contain only SOW status, synthetic dataset references, and upstream source references.
- No secrets, credentials, bearer tokens, SNMP communities, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details are present.

Artifact maintenance gate:

- AGENTS.md: no update needed for activation.
- Runtime project skills: no update needed for activation.
- Specs: no shipped product behavior changed during activation.
- End-user/operator docs: no update needed for activation.
- End-user/operator skills: no output/reference skill produced during activation.
- SOW lifecycle: moved from pending to current with `Status: in-progress`.
- `SOW-status.md`: updated for SOW-0016 activation.

## Outcome

Active implementation SOW.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
