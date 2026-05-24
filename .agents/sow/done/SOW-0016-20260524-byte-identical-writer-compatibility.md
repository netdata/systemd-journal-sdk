# SOW-0016 - Byte Identical Writer Compatibility

## Status

Status: completed

Sub-state: implementation, validation, external review, and closeout complete.

## Requirements

### Purpose

Prove that deterministic writer outputs from Rust, Go, Node.js, Python, and the systemd reference writer are byte-for-byte identical for the frozen accepted corpus, or stop with evidence for any impossible equivalence condition before weakening the goal.

### User Request

The user requested running all language ingesters and the systemd ingester over the same dataset, then requiring the generated binary journal files to be byte-for-byte identical.

### Assistant Understanding

Facts:

- Byte-for-byte identity is stronger than semantic journal compatibility.
- systemd-exact writer alignment and object placement are required for this project; stricter-but-compatible alignment is not acceptable for SDK writer output.
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
- The byte-identity harness covers systemd final-state variants for the deterministic corpus:
  - online/plain close, matching `journal_file_close()`;
  - offline close, matching `journal_file_offline_close()` without archive rotation;
  - archived close, matching `journal_file_archive()` followed by offlining, where this is meaningful for repository writer APIs.
- The deterministic accepted corpus contains deliberate DATA hash-bucket collisions under the fixed keyed-hash file ID and default systemd v260 data hash table size, so byte identity exercises `next_hash_offset` chain traversal and `data_hash_chain_depth` publication.
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
- Reusing the imported Rust writer's stricter alignment rules would preserve broad reader compatibility but fail this SOW's stronger systemd-exact byte-layout requirement.
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
2. Alignment policy
   - Decision: SDK writers must match systemd's alignment and object placement rules for the targeted file-format slice.
   - Reason: the imported Rust implementation may use stricter alignment while remaining readable by systemd, but this project requires byte-identical writer output where deterministic controls make identity meaningful.
   - Risk: code shared or copied from the imported Rust writer may need format-level changes rather than direct porting, and every such change must be validated against stock `journalctl`, stock libsystemd readers, and cross-language readers.
3. Final-state policy
   - Decision: byte-identity evidence must distinguish online/plain-close, offline-close, and archived-close journals.
   - Reason: systemd has separate persisted header states and close paths. Plain `journal_file_close()` leaves an online file; `journal_file_offline_close()` writes `OFFLINE`; archiving queues `ARCHIVED` and offlining commits it.
   - Risk: current byte-identity evidence only covers online/plain-close output, so closing this SOW before a final-state matrix would miss a real compatibility surface.
4. DATA hash-collision coverage
   - Decision: DATA hash-bucket collisions are required before SOW-0016 can close.
   - Reason: systemd updates `data_hash_chain_depth` while traversing an existing DATA hash chain in a writable file. A corpus with `data_hash_chain_depth = 0` proves only the no-collision path and can hide writer drift in `next_hash_offset` and chain-depth publication.
   - Evidence: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-file.c:1489`, `src/libsystemd/sd-journal/journal-file.c:1621`, and `src/libsystemd/sd-journal/journal-file.c:1680`.
   - Risk: adding deliberate collisions may expose byte deltas in Rust, Go, Node.js, or Python writers that previous byte-identity validation did not exercise.

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
- User clarification:
  - The imported Rust implementation is reported to be stricter in alignment than systemd while remaining compatible.
  - The project requirement is systemd-exact alignment and object placement, not stricter-compatible layout.
  - Any implementer prompt or writer repair must include this requirement before changing writer format code.
- Stopped the in-progress writer repair implementer run that had been launched before the alignment clarification. The run had already modified writer sources, but its premise was incomplete and its changes require inspection before reuse or discard.
- Relaunched the preferred implementer with the corrected systemd-exact alignment requirement, then stopped that run before completion because it was deriving an incorrect format model from the evidence:
  - It treated `tail_object_offset` as an end offset, but systemd `journal_file_append_object()` writes the offset of the last object's start.
  - It repeatedly misread hash-table object-header bytes while the prompt already provided the verified field/data hash-table offsets.
  - This is recorded as a preferred-implementer failure for this repair chunk; the next run should use the approved fallback implementer hierarchy.
- Launched the fallback implementer with `qwen3.6-plus`, then stopped it because the command incorrectly used the reviewer agent mode. The run had already replaced some Go files before it was stopped, so the next implementer run must treat the worktree as dirty and repair or replace those partial edits before validation.
- Relaunched `qwen3.6-plus` without reviewer mode, then stopped it before completion because it set a Rust initial `tail_object_offset` to the data hash item-array offset instead of the DATA_HASH_TABLE object start. The exact systemd requirement is `tail_object_offset = data_hash_table_offset - sizeof(ObjectHeader)` after initial hash-table setup.
- User correction: implementer agents must not be run with `--agent code-reviewer`; that flag is only for read-only reviewer runs.
- Updated `.agents/skills/project-agent-orchestration/SKILL.md` so future implementer runs explicitly use normal coding mode and reviewer runs remain read-only.
- Locally repaired the partially modified writer layout after repeated implementer failures:
  - Rust, Go, Node.js, and Python writers now emit v260-size headers for new files.
  - New files use the systemd v260 field-hash-table then data-hash-table object order.
  - New files preallocate the same 8 MiB initial arena size used by the deterministic systemd helper.
  - Writer metadata now includes v260 counters and tail entry-array fields needed for byte identity.
  - Entry-array and data-entry-array growth now follows the observed systemd allocation policy for the accepted corpus.
  - `Close()` keeps the deterministic online state used by the systemd helper instead of forcing offline state.
  - Rust no longer updates `data_hash_chain_depth` immediately after appending a DATA object; systemd updates this header during hash-chain lookup traversal.
- Fixed Rust reader hash-table mapping for historical files whose on-disk `header_size` is smaller than the v260 Rust `JournalHeader` struct. Evidence: committed no-RTC fixtures have `header_size = 256` and `field_hash_table_offset = 272`, so the hash-table object starts exactly at the on-disk header boundary and is valid.
- Fixed `tests/interoperability/run_compression_matrix.py` to inspect objects from the on-disk `header_size` instead of the older hard-coded 208-byte header.
- User added a required final-state test dimension: open/online journals versus closed journals. Investigation evidence:
  - `systemd/systemd @ cf3156842209` reports persisted states as `STATE_OFFLINE = 0`, `STATE_ONLINE = 1`, and `STATE_ARCHIVED = 2` in `src/libsystemd/sd-journal/journal-def.h:160`.
  - `journal_file_set_online()` changes `OFFLINE` back to `ONLINE` when writable files are opened for appending in `src/libsystemd/sd-journal/journal-file.c:215`.
  - `journal_file_set_offline_internal()` fsyncs, then writes `STATE_OFFLINE` or `STATE_ARCHIVED` in `src/shared/journal-file-util.c:184`.
  - `journal_file_offline_close()` writes final tags when enabled, sets the file offline, and then closes in `src/shared/journal-file-util.c:420`.
  - `journal_file_archive()` renames the file and queues archived state for the subsequent offlining path instead of directly setting `STATE_ARCHIVED` in `src/libsystemd/sd-journal/journal-file.c:4359`.
  - The current systemd dataset ingester uses `journal_file_set_offline_thread_join()` followed by `journal_file_close()`, not `journal_file_offline_close()`, so current byte-identity evidence covers only the online/plain-close state in `tests/datasets/ingesters/systemd/dataset_ingester.c`.
- Reviewer and user feedback promoted DATA hash-collision coverage to a SOW-0016 closeout requirement. Evidence:
  - systemd `get_next_hash_offset()` increments chain depth and updates `data_hash_chain_depth` only when following a non-zero `next_hash_offset` in writable files.
  - systemd `journal_file_find_data_object_with_hash()` passes the DATA object's `next_hash_offset` and the header `data_hash_chain_depth` to `get_next_hash_offset()`.
  - The current accepted corpus validates `data_hash_chain_depth = 0`, so it has not exercised this path.
  - The SOW remains in progress until a deterministic accepted corpus produces deliberate DATA hash-bucket collisions and all byte-identity/final-state validation still passes.
- Writer repair review round:
  - Minimax verdict: `PRODUCTION GRADE`; no blocking findings.
  - GLM verdict: `PRODUCTION GRADE`; no blocking findings. Non-blocking findings: an unused legacy Rust source path still reflects old layout assumptions, Rust writer files retain broad allow attributes, and a few Rust block expressions omit optional semicolons.
  - Mimo verdict: `NOT PRODUCTION GRADE`; one blocking finding. Rust readers mapped a 272-byte v260 `JournalHeader` for all files, so `journal_header_ref()` could expose bytes beyond an older on-disk header as v260-only fields on historical files. Non-blocking findings: unused Rust `boot_id` option and a stale claim about missing writer sync that was rejected because current `JournalFile::create()` calls `sync()` before returning.
  - Qwen verdict: `PRODUCTION GRADE`; no blocking findings. Non-blocking findings: two stale comments, historical Rust header mapping should be tightened, Node.js stores one tail field as `Number` while related offsets are `BigInt`, and SOW closeout was still pending.
- Dispositioned writer repair review findings:
  - Added sanitized read-only headers to both Rust reader implementations so fields beyond the on-disk `header_size` are zeroed for historical files while mutable writer headers remain mmap-backed.
  - Added Rust unit coverage for 256-byte historical headers preserving valid chain-depth fields while clearing absent v260 tail fields.
  - Updated the stale Go `Close()` and Node.js header serialization comments.
  - Left the unused Rust `boot_id` option as a recorded residual API-compatibility item for successor-file behavior; it is not used by this deterministic writer slice.
  - Left the dead legacy Rust source path and optional Rust style comments unchanged because they are not compiled or behavior-affecting in this SOW; they do not change the production compatibility gate for the active implementation paths.
  - Left the Node.js tail-field `Number` representation unchanged because the current writer file-size envelope is far below JavaScript's safe integer limit and byte-identity validation covers the serialized field.
- Implemented final-state byte-identity coverage:
  - Added `--final-state online|offline|archived` to the systemd, Rust, Go, Node.js, and Python dataset ingesters.
  - Added explicit SDK writer finalization APIs where needed: Go `CloseOffline()` and `ArchiveTo()`, Node.js `closeOffline()`, and Python `close_offline()`.
  - Preserved `Close()` as the systemd plain-close equivalent that leaves the file `ONLINE`.
  - Added archived output path derivation using systemd's `<name>@<seqnum-id>-<head-seqnum>-<head-realtime>.journal` pattern.
  - Expanded the byte-identity harness to support `--final-state all` and to compare all `10` language pairs instead of relying on transitivity through a subset of pairs.
  - Reordered Rust entry publication so `n_entries` is written after tail metadata, matching the publication order already used by Go, Node.js, and Python.
  - Tightened Node.js created file permissions to an explicit `0640`.

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
- Final closeout review after DATA hash-collision corpus expansion:
  - Minimax verdict: `PRODUCTION GRADE`; no blocking findings. Non-blocking concerns about archive naming and rotation/FSS were either validated by the final-state byte-identity matrix or tracked by follow-up SOWs outside this uncompressed regular-file slice.
  - Mimo verdict: `PRODUCTION GRADE`; no blocking findings. Verified the four full DATA payloads hash to bucket `85984`, verified exact `data_hash_chain_depth = 3`, and found no unwanted side effects.
  - Kimi verdict: `PRODUCTION GRADE`; no blocking findings. Reran byte identity, closed-file matrix, binary matrix, compression matrix, live matrix, dataset validation, package tests, `git diff --check`, and the SOW audit.

Writer byte-identity repair evidence:

- Passed: `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile python/journal/header.py python/journal/writer.py python/journal/reader.py tests/interoperability/run_byte_identity.py`.
- Passed: `PYTHONDONTWRITEBYTECODE=1 python3 python/test_all.py`.
- Passed: `GOMODCACHE=$PWD/../.local/go/pkg/mod GOPATH=$PWD/../.local/go GOCACHE=$PWD/../.local/go-build go test ./...` from `go/`.
- Passed: `node --check src/lib/header.js`, `node --check src/lib/writer.js`, `node --check src/lib/reader.js`, and `npm test` from `node/`.
- Passed: `CARGO_HOME=$PWD/.local/cargo-home CARGO_TARGET_DIR=$PWD/.local/cargo-target cargo test --workspace --manifest-path rust/Cargo.toml`.
  - Residual warning: `JournalFileOptions.boot_id` is unused in the Rust writer options after matching systemd's initial zero `tail_entry_boot_id`; the public option remains for API compatibility and future successor-file behavior review.
- Passed after writer repair before DATA hash-collision corpus expansion: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_byte_identity.py --diff-limit 8`.
  - Result: `all_equal: true`.
  - Compared outputs: systemd, Rust, Go, Node.js, Python.
  - Size for every accepted-corpus output: `8388608` bytes.
  - Shared header evidence: `header_size = 272`, `arena_size = 8388336`, `n_entries = 347`, `n_data = 1483`, `n_fields = 170`, `n_entry_arrays = 88`, `tail_entry_offset = tail_object_offset = 2206256`, `data_hash_chain_depth = 0`, `field_hash_chain_depth = 1`. This evidence predated the explicit DATA hash-collision closeout gate and is superseded by the later collision validation.
  - Every ingester returned `0`.
  - Every generated accepted-corpus file passed stock `journalctl --verify --file`.
- Passed: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_matrix.py --entries 64`.
  - systemd version: `systemd 260 (260.1-2-manjaro)`.
  - Result: `104` checks, `104` passed, `0` failed.
  - Covered stock journalctl plus Go, Rust, Node.js, and Python readers for every repository writer.
- Passed: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_binary_matrix.py`.
  - systemd version: `systemd 260 (260.1-2-manjaro)`.
  - Result: `52` checks, `52` passed, `0` failed.
  - Covered stock verify, stock JSON/export, stock export match, stock libsystemd binary-field reader, and every repository reader.
- Passed: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_live_matrix.py --entries 24 --poll-readers 1 --writer-delay-ms 5`.
  - systemd version: `systemd 260 (260.1-2-manjaro)`.
  - Result: `4` writer cases, `4` passed, `0` failed.
  - Covered active polling readers, final reads, and stock verify for Go, Rust, Node.js, and Python writers.
- Initial optional compression matrix attempt failed only the `compression-flags` inspection because the harness scanned objects from hard-coded offset `208` after new v260 files moved `header_size` to `272`.
- Passed after harness repair: `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile tests/interoperability/run_compression_matrix.py`.
- Passed after harness repair: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_compression_matrix.py --entries 8`.
  - systemd version: `systemd 260 (260.1-2-manjaro)`.
  - Result: `72` checks, `72` passed, `0` failed.
  - Covered compressed DATA-object flags, stock verify, stock JSON/export, stock export match, stock libsystemd reader, and every repository reader.
- Passed: `git diff --check`.
- Passed after historical-header sanitizer fix: `CARGO_HOME=$PWD/.local/cargo-home CARGO_TARGET_DIR=$PWD/.local/cargo-target cargo test --workspace --manifest-path rust/Cargo.toml`.
  - New evidence: `file::file::tests::sanitize_header_for_historical_size_clears_absent_v260_tail_fields` passes in both Rust reader crates.
  - Residual warning remains: `JournalFileOptions.boot_id` is unused in the Rust writer options after matching systemd's initial zero `tail_entry_boot_id`.
- Passed after historical-header sanitizer fix: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_byte_identity.py --diff-limit 8`.
  - Result: `all_equal: true`.
  - Size for every accepted-corpus output: `8388608` bytes.
  - Every generated accepted-corpus file passed stock `journalctl --verify --file`.
- First rerun of `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_live_matrix.py --entries 24 --poll-readers 1 --writer-delay-ms 5` while other interoperability harnesses were running in parallel produced one active-poll timing miss for Node.js reading the Go writer. Final reads still saw all `24` entries and stock verify passed. This result was not accepted as final live-concurrency evidence.
- Passed when rerun standalone: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_live_matrix.py --entries 24 --poll-readers 1 --writer-delay-ms 5`.
  - systemd version: `systemd 260 (260.1-2-manjaro)`.
  - Result: `4` writer cases, `4` passed, `0` failed.
  - Covered active polling readers, final reads, and stock verify for Go, Rust, Node.js, and Python writers.
- Passed after historical-header sanitizer fix: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_compression_matrix.py --entries 8`.
  - systemd version: `systemd 260 (260.1-2-manjaro)`.
  - Result: `72` checks, `72` passed, `0` failed.
- Passed after historical-header sanitizer fix: `git diff --check`.
- Passed after final-state matrix implementation: `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile python/journal/writer.py python/cmd/dataset_ingester.py tests/datasets/ingesters/run_dataset_ingesters.py tests/interoperability/run_byte_identity.py`.
- Passed after final-state matrix implementation: `node --check node/src/lib/writer.js && node --check node/cmd/dataset_ingester.js`.
- Passed after final-state matrix implementation: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_byte_identity.py --final-state all --diff-limit 8`.
  - Result: `all_equal: true`.
  - Covered final states: `online` (`state = 1`), `offline` (`state = 0`), and `archived` (`state = 2`).
  - Each final state compared all `10` language pairs across systemd, Rust, Go, Node.js, and Python.
  - Every generated accepted-corpus file passed stock `journalctl --verify --file`.
- Passed summary check: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_byte_identity.py --skip-run --final-state all --diff-limit 1`.
  - Result: `all_equal True`.
  - `archived`: `10` pairs, systemd/Rust header state sample `2`.
  - `offline`: `10` pairs, systemd/Rust header state sample `0`.
  - `online`: `10` pairs, systemd/Rust header state sample `1`.
- Passed after final-state matrix implementation: `PYTHONDONTWRITEBYTECODE=1 python3 python/test_all.py`.
- Passed after final-state matrix implementation: `GOMODCACHE=$PWD/../.local/go/pkg/mod GOPATH=$PWD/../.local/go GOCACHE=$PWD/../.local/go-build go test ./...` from `go/`.
- Passed after final-state matrix implementation: `npm test` from `node/`.
- Passed after final-state matrix implementation: `CARGO_HOME=$PWD/.local/cargo-home CARGO_TARGET_DIR=$PWD/.local/cargo-target cargo test --workspace --manifest-path rust/Cargo.toml`.
  - Residual warning remains: `JournalFileOptions.boot_id` is unused in the Rust writer options after matching systemd's initial zero `tail_entry_boot_id`.
- Passed after final-state matrix implementation: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_matrix.py --entries 64`.
  - systemd version: `systemd 260 (260.1-2-manjaro)`.
  - Result: `104` checks, `104` passed, `0` failed.
- Passed after final-state matrix implementation: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_binary_matrix.py`.
  - systemd version: `systemd 260 (260.1-2-manjaro)`.
  - Result: `52` checks, `52` passed, `0` failed.
- Passed after final-state matrix implementation: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_compression_matrix.py --entries 8`.
  - systemd version: `systemd 260 (260.1-2-manjaro)`.
  - Result: `72` checks, `72` passed, `0` failed.
- Passed after final-state matrix implementation: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_live_matrix.py --entries 24 --poll-readers 1 --writer-delay-ms 5`.
  - systemd version: `systemd 260 (260.1-2-manjaro)`.
  - Result: `4` writer cases, `4` passed, `0` failed.
- DATA hash-collision closeout:
  - Added deterministic accepted-corpus coverage tag `hash-collision-chain`.
  - Collision DATA payloads under fixed file ID `33333333333333333333333333333333` and default v260 DATA bucket count `116508`:
    - `AA=cv-0299`
    - `AC=cv-0163`
    - `AZ=cv-0168`
    - `BB=cv-0245`
  - All four payloads map to DATA bucket `85984`.
  - Regenerated `tests/datasets/correctness/corpus.jsonl` and `tests/datasets/ingestion-manifest.json`; accepted corpus now has `349` records.
  - Added byte-identity harness validation requiring exact `data_hash_chain_depth = 3` for every language and final state.
  - Initial collision-gated run failed as intended for Rust: systemd, Go, Node.js, and Python wrote `data_hash_chain_depth = 3`; Rust wrote `0`.
  - Fixed Rust writer chain-depth publication in both Rust writer crates by updating the DATA hash-chain depth from the writer path while keeping read-only reader lookup immutable.
  - Passed after Rust chain-depth repair: `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile tests/interoperability/run_byte_identity.py`.
  - Passed after Rust chain-depth repair: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_byte_identity.py --final-state all --diff-limit 8`.
    - Result: `all_equal True`.
    - `online`: `10` comparison pairs, `data_hash_chain_depth = 3` for systemd, Rust, Go, Node.js, and Python.
    - `offline`: `10` comparison pairs, `data_hash_chain_depth = 3` for systemd, Rust, Go, Node.js, and Python.
    - `archived`: `10` comparison pairs, `data_hash_chain_depth = 3` for systemd, Rust, Go, Node.js, and Python.
  - Passed after collision corpus expansion: `PYTHONDONTWRITEBYTECODE=1 python3 tests/datasets/validate.py`.
  - Passed after collision corpus expansion: `PYTHONDONTWRITEBYTECODE=1 python3 python/test_all.py`.
  - Passed after collision corpus expansion: `GOMODCACHE=$PWD/../.local/go/pkg/mod GOPATH=$PWD/../.local/go GOCACHE=$PWD/../.local/go-build go test ./...` from `go/`.
  - Passed after collision corpus expansion: `npm test` from `node/`.
  - Passed after collision corpus expansion: `CARGO_HOME=$PWD/.local/cargo-home CARGO_TARGET_DIR=$PWD/.local/cargo-target cargo test --workspace --manifest-path rust/Cargo.toml`.
    - Residual warning remains: `JournalFileOptions.boot_id` is unused in both Rust writer option structs.
  - Passed after collision corpus expansion: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_matrix.py --entries 64` (`104/104`).
  - Passed after collision corpus expansion: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_binary_matrix.py` (`52/52`).
  - Passed after collision corpus expansion: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_compression_matrix.py --entries 8` (`72/72`).
  - Passed after collision corpus expansion: `PYTHONDONTWRITEBYTECODE=1 python3 tests/interoperability/run_live_matrix.py --entries 24 --poll-readers 1 --writer-delay-ms 5` (`4/4`).
  - Passed after external review: `git diff --check`.
  - Passed after external review: `bash .agents/sow/audit.sh`.

Sensitive data gate:

- Activation edits contain only SOW status, synthetic dataset references, and upstream source references.
- No secrets, credentials, bearer tokens, SNMP communities, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details are present.

Artifact maintenance gate:

- AGENTS.md: no update needed; repository-wide policy already covers systemd v260.1 baseline, SOW sequencing, pure-language constraints, and reviewer/implementer roles.
- Runtime project skills: updated `.agents/skills/project-agent-orchestration/SKILL.md` to prevent implementers from running in read-only reviewer mode; updated `.agents/skills/project-journal-compatibility/SKILL.md` to require byte-identity validation for deterministic writer layout changes, on-disk `header_size` use for reader object-location validation, and DATA hash-chain depth parity.
- Specs: updated `.agents/sow/specs/product-scope.md` with the current shared writer layout contract, final-state byte-identity requirement, historical header-size reader rule, and DATA hash-collision coverage requirement.
- End-user/operator docs: updated `tests/datasets/README.md` and `tests/interoperability/README.md` to document deliberate DATA hash-chain collision coverage and exact `data_hash_chain_depth` validation. Updated Go, Node.js, and Python README writer API notes for online/offline/archive finalization behavior.
- End-user/operator skills: no output/reference skill produced during activation.
- SOW lifecycle: marked `Status: completed` and ready to move from `.agents/sow/current/` to `.agents/sow/done/` with the implementation commit.
- `SOW-status.md`: updated to remove SOW-0016 from current work and list it as completed.

## Outcome

Implementation, validation, external review, and closeout are complete.

The accepted deterministic uncompressed corpus now produces byte-for-byte
identical journal files across systemd, Rust, Go, Node.js, and Python for
online, offline, and archived final states. The corpus includes deliberate DATA
hash-bucket collisions, and the byte-identity harness requires exact
`data_hash_chain_depth = 3` for every language and final state before accepting
the result.

## Lessons Extracted

- The no-collision corpus gave a false sense of completeness for DATA hash table behavior. Byte-identity validation must include deliberate collisions and must assert the affected header fields, not just whole-file equality after an unchallenging corpus.
- Reviewer prompts and SOW gates should treat "passes byte identity" as corpus-dependent evidence. The corpus coverage itself is part of the compatibility claim.

## Followup

- Compression byte identity remains tracked by `SOW-0017-20260524-xz-lz4-data-writing.md`.
- Compact journal support remains tracked by `SOW-0018-20260524-compact-journal-format.md`.
- Forward Secure Sealing remains tracked by `SOW-0019-20260524-forward-secure-sealing.md`.
- Directory traversal parity remains tracked by `SOW-0020-20260524-directory-traversal-parity.md`.

## Regression Log

None yet.
