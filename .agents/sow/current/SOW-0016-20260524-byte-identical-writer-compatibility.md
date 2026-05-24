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

## Validation

Activation evidence:

- Passed: SOW-0014 is completed in `.agents/sow/done/`.
- Passed: SOW-0015 is completed in `.agents/sow/done/`.
- Passed: SOW-0015 completion commit `cdd3795` exists before activation.
- Passed: no implementation changes made during activation.

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
