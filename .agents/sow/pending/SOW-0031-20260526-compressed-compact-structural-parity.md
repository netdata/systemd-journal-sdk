# SOW-0031 - Compressed Compact Structural Parity

## Status

Status: open

Sub-state: Split from SOW-0022 Gap 7. Ready for implementation after activation.

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
