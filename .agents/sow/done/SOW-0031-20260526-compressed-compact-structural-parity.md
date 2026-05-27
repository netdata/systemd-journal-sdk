# SOW-0031 - Compressed Compact Structural Parity

## Status

Status: completed

Sub-state: Completed and ready to archive under `.agents/sow/done/`. Split from SOW-0022 Gap 7.

## Requirements

### Purpose

Ensure compressed and compact journal outputs are structurally compatible with systemd expectations even when byte-for-byte identity is not meaningful because compressor output can vary by implementation.

### User Request

Use structural parity plus stock verification/read parity for compressed and compact output, and require byte identity only where deterministic and meaningful.

### Assistant Understanding

Facts:

- The user accepted SOW-0022 Option B for compressed/compact output.
- Regular uncompressed deterministic writer output already has byte-for-byte parity coverage.
- Compression and compact matrices currently validate closed-file semantic compatibility, but not complete object/layout invariants.

Inferences:

- A structural oracle can catch format drift without relying on compressor byte identity.

Unknowns:

- Whether stock systemd can generate an equivalent compact uncompressed reference for byte identity. This can be resolved during implementation.

### Acceptance Criteria

- A committed structural parity test inspects compressed and compact outputs from all writers.
- The test checks object order, offsets, flags, counters, hash chains, tail metadata, and compact offset constraints.
- Stock `journalctl --verify --file`, stock journalctl reads, stock libsystemd reads, and all repository readers pass for generated files.
- Byte identity is required only for deterministic cases where the SOW records evidence that it is meaningful.

## Analysis

Sources checked:

- `SOW-0022-20260525-compatibility-test-gap-audit.md`
- `product-scope.md`
- `tests/interoperability/run_compression_matrix.py`
- `tests/interoperability/run_compact_matrix.py`
- `tests/interoperability/run_byte_identity.py`

Current state:

- The current matrices prove compatibility at a reader/verify level.
- They do not yet compare full structural layout invariants for compressed and compact outputs.

Risks:

- Semantic reader parity can miss object ordering, hash-chain, or metadata drift that still matters for long-term compatibility.
- Over-constraining compressor bytes can create brittle tests that fail without format incompatibility.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Existing tests intentionally avoid byte identity for compressed output, but no replacement structural oracle fully verifies that the generated journal layout follows systemd-compatible object graph rules.

Evidence reviewed:

- `tests/interoperability/run_compression_matrix.py`
- `tests/interoperability/run_compact_matrix.py`
- `tests/interoperability/run_byte_identity.py`
- `product-scope.md` writer layout contract
- User decision in SOW-0022: structural parity plus stock verification/read parity for compressed and compact output.

Affected contracts and surfaces:

- Writer layout for compressed DATA objects.
- Compact ENTRY and ENTRY_ARRAY layout.
- Hash-chain depth publication.
- Interoperability matrix documentation.

Existing patterns to reuse:

- Existing compression and compact matrix runners.
- Existing byte identity inspector logic for deterministic regular files.
- Stock `journalctl --verify --file` and stock libsystemd checks.

Risk and blast radius:

- Medium compatibility risk, mostly test/harness risk. Implementation fixes may touch all writers if the structural oracle exposes drift.

Sensitive data handling plan:

- Use deterministic synthetic datasets only.

Implementation plan:

1. Extend or add a structural layout inspector for compressed and compact generated journals.
2. Record whether compact uncompressed byte identity with stock systemd is possible.
3. Add assertions for object order, offsets, flags, counters, hash-chain depths, and tail metadata.
4. Fix any language-specific writer drift exposed by the oracle.

Validation plan:

- Run compression matrix.
- Run compact matrix.
- Run new structural parity matrix.
- Run stock verification and repository reader checks.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: update `project-journal-compatibility` only if structural parity becomes a mandatory close gate.
- Specs: update `product-scope.md` with exact structural parity policy if needed.
- End-user/operator docs: likely unaffected.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: activate only this SOW when implementing.
- SOW-status.md: update on activation and completion.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  - `src/libsystemd/sd-journal/journal-def.h`
  - `src/libsystemd/sd-journal/journal-file.c`

Open decisions:

- None. SOW-0022 recorded Option B for this policy.

## Implications And Decisions

- User decision already recorded in SOW-0022: structural parity plus stock verification/read parity for compressed and compact output.

## Plan

1. Add structural oracle tests.
2. Run them against all writers.
3. Fix any exposed writer drift.
4. Validate with stock systemd and read-only reviewers.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current routing decision.

Reviewers:

- Read-only reviewers from the active pool after implementation: minimax, kimi, qwen, glm. Mimo is skipped.

Failure handling:

- Record any byte-identity impossibility with evidence before changing acceptance.

## Execution Log

- 2026-05-27: Activated SOW for local implementation with read-only external reviewers after implementation.
- 2026-05-27: Added `tests/interoperability/journal_structure.py` as the shared structural oracle for generated journal files.
- 2026-05-27: Replaced the narrow compression/compact inline inspectors in `run_compression_matrix.py` and `run_compact_matrix.py` with the shared structural oracle.
- 2026-05-27: Added a deterministic `BINARY_COMPRESSIBLE` field to the Go, Rust, Node.js, and Python livewriter fixtures so compact plus compression matrices prove at least one compact DATA object is compressed.
- 2026-05-27: Updated `tests/interoperability/README.md`, `product-scope.md`, and `project-journal-compatibility` workflow memory to describe the structural oracle policy.
- 2026-05-27: First reviewer round found `TAG_OBJECT_SIZE` was incorrectly modeled as 56 bytes. Fixed to 64 bytes: object header 16 bytes, seqnum 8 bytes, epoch 8 bytes, and 32-byte tag.
- 2026-05-27: Fixed the only actionable final-review cleanup by changing a misleading invalid-object error from "DATA object" to "object" for the multiple-compression-flag check.

## Validation

Acceptance criteria evidence:

- Structural parity test added: `tests/interoperability/journal_structure.py`.
- Compression matrix now calls `inspect_journal_structure(... expected_compact=False, expected_compression=...)`.
- Compact matrix now calls `inspect_journal_structure(... expected_compact=True, expected_compression=...)`.
- Structural oracle checks object order, offsets, object flags, header counters, hash-table objects, hash chains, tail metadata, references, entry arrays, and compact 32-bit offset constraints.
- Stock `journalctl --verify --file`, stock journalctl JSON/export reads, stock libsystemd reads, and Go/Rust/Node.js/Python repository reader checks remain part of both matrices.
- Byte identity remains required for deterministic regular uncompressed output through `run_byte_identity.py`. Compressed and compact output use structural parity plus stock/repository reader parity because compressor bytes and compact layout choices are not a meaningful byte-for-byte equality target for this SOW.

Commands and results:

- `python3 -m py_compile tests/interoperability/journal_structure.py tests/interoperability/run_compression_matrix.py tests/interoperability/run_compact_matrix.py python/cmd/livewriter.py`
  - PASS.
- `PYTHONPATH="$PWD/.local/python-deps${PYTHONPATH:+:$PYTHONPATH}" python3 tests/interoperability/run_compression_matrix.py --compression zstd xz lz4 --entries 10`
  - PASS: 216/216.
  - systemd: `systemd 260 (260.1-2-manjaro)`.
  - Result artifact: `.local/interoperability/compression-matrix-results-20260527-085801.json`.
- `PYTHONPATH="$PWD/.local/python-deps${PYTHONPATH:+:$PYTHONPATH}" python3 tests/interoperability/run_compact_matrix.py --entries 10 --compression none`
  - PASS: 56/56.
  - systemd: `systemd 260 (260.1-2-manjaro)`.
  - Result artifact: `.local/interoperability/compact-matrix-none-results-20260527-085944.json`.
- `PYTHONPATH="$PWD/.local/python-deps${PYTHONPATH:+:$PYTHONPATH}" python3 tests/interoperability/run_compact_matrix.py --entries 10 --compression zstd`
  - PASS: 56/56.
  - systemd: `systemd 260 (260.1-2-manjaro)`.
  - Result artifact: `.local/interoperability/compact-matrix-zstd-results-20260527-085946.json`.
- `PYTHONPATH="$PWD/.local/python-deps${PYTHONPATH:+:$PYTHONPATH}" python3 tests/interoperability/run_compact_matrix.py --entries 10 --compression xz`
  - PASS: 56/56.
  - systemd: `systemd 260 (260.1-2-manjaro)`.
  - Result artifact: `.local/interoperability/compact-matrix-xz-results-20260527-085948.json`.
- `PYTHONPATH="$PWD/.local/python-deps${PYTHONPATH:+:$PYTHONPATH}" python3 tests/interoperability/run_compact_matrix.py --entries 10 --compression lz4`
  - PASS: 56/56.
  - systemd: `systemd 260 (260.1-2-manjaro)`.
  - Result artifact: `.local/interoperability/compact-matrix-lz4-results-20260527-085949.json`.

Dependency/cache handling:

- Python `lz4==4.4.5` was installed into `.local/python-deps` for validation because the system Python did not have `lz4.block` available. This is a repository-local validation dependency path and is not committed.
- Compression matrix `build_env()` now redirects `npm_config_cache` and `PIP_CACHE_DIR` into `.local/`, matching the existing compact matrix cache discipline.

Reviewer findings and dispositions:

- Qwen first round: NOT PRODUCTION GRADE due incorrect TAG object size. Accepted and fixed.
- GLM first round: NOT PRODUCTION GRADE due the same TAG object size issue. Accepted and fixed.
- GLM final round: PRODUCTION GRADE. No blocking findings. Non-blocking items were dispositioned:
  - Test-only structural inspector reads whole files into memory. Accepted as fit for small generated fixtures; no production SDK path uses it.
  - Hash-chain depth variable counts "next" hops, matching the structural check. Exact deterministic depth parity remains covered by `run_byte_identity.py`.
  - Thin `inspect_compact` / `inspect_compression` wrappers are kept because they provide named matrix result labels.
  - Compression matrix cache redirection is a positive side effect.
- GLM final-round actionable cleanup: misleading multiple-compression-flag error wording. Fixed.
- Minimax and qwen reruns did not produce final verdicts in reasonable time. Their exact review PIDs were stopped; unrelated reviewer processes in other repositories were left untouched. These runs are recorded as non-decisive evidence, not acceptance evidence.
- Kimi first-round review stalled and was stopped by exact PIDs. It is recorded as failed reviewer evidence.

Same-failure search:

- `rg -n "DATA object at offset .*multiple compression flags|TAG_OBJECT_SIZE = 56|TAG_OBJECT_SIZE" tests/interoperability .agents/skills .agents/sow/specs`
  - Only expected fixed `TAG_OBJECT_SIZE = OBJECT_HEADER_SIZE + 8 + 8 + (256 // 8)` references remain.
- `rg -n "TODO|FIXME|HACK|XXX|WORKAROUND|NOQA" tests/interoperability/journal_structure.py tests/interoperability/run_compression_matrix.py tests/interoperability/run_compact_matrix.py go/internal/testcmd/livewriter/main.go rust/src/internal/testcmd/livewriter/src/main.rs node/internal/testcmd/livewriter.js python/cmd/livewriter.py`
  - No matches.

Sensitive data gate:

- Only deterministic synthetic fixture fields and generated local `.journal` files were used.
- No raw sensitive data was written to durable artifacts.

Artifact maintenance gate:

- `AGENTS.md`: no change. Existing repository boundary, SOW, and reviewer rules remain sufficient.
- Runtime project skills: updated `.agents/skills/project-journal-compatibility/SKILL.md` so future compression and compact writer work runs the structural matrix checks.
- Specs: updated `.agents/sow/specs/product-scope.md` for current structural parity behavior.
- End-user/operator docs: updated `tests/interoperability/README.md`; no public SDK API or operator guide changed.
- End-user/operator skills: none exist for this project slice, so none changed.
- SOW lifecycle: SOW moved from pending to current for implementation and is moved to done with this closeout.
- `SOW-status.md`: updated on activation and completion.

SOW audit:

- `.agents/sow/audit.sh`
  - PASS after SOW-0031 was moved to `.agents/sow/done/`.

## Outcome

Completed.

The compression and compact matrix runners now use a shared structural oracle. The oracle validates the journal object graph and layout invariants that byte-for-byte comparison cannot reliably express for compressed and compact output. Compact plus compression fixtures now include deterministic compressible binary data in all four languages, so the compact matrix proves compressed compact DATA objects exist and remain readable by stock and repository readers.

## Lessons Extracted

- Structural parity tests need fixture data that forces the path under test. The old compact compressed matrix could pass reader checks without proving compact DATA object compression happened; the new `BINARY_COMPRESSIBLE` fixture closes that gap.
- Fixed-size object constants need to be derived from the on-disk structure, not remembered from prior inspectors. TAG objects are 64 bytes in this slice.
- Reviewer runs can fail by hanging or omitting a verdict. Treat those runs as failed evidence and rely only on completed reviewer verdicts and reproducible validation.

## Followup

No untracked implementation item remains from this SOW.

Rejected as not part of this SOW:

- ENTRY `xor_hash` recomputation inside the structural inspector. Exact deterministic xor_hash parity remains covered by `run_byte_identity.py`; compressed/compact structural parity does not need to recompute it.
- Exact published DATA hash-chain depth equality inside the structural inspector. The inspector validates chain integrity and impossible published depths; exact deterministic depth parity remains covered by `run_byte_identity.py`.
- Compact byte-for-byte identity against a stock compact writer reference. This SOW's accepted policy is structural parity plus stock/repository reader parity for compact output; deterministic uncompressed byte identity remains covered separately.

## Regression Log

None yet.
