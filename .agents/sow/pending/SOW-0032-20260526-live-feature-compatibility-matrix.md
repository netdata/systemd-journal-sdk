# SOW-0032 - Live Feature Compatibility Matrix

## Status

Status: open

Sub-state: Split from SOW-0022 Gap 1. Ready for implementation after activation.

## Requirements

### Purpose

Prove that stock journalctl, stock libsystemd readers, and repository readers can safely read active journal files while SDK writers append with compression, compact layout, and Forward Secure Sealing enabled.

### User Request

Compatibility must be confirmed and validated for all writers and readers, including concurrent operation with stock systemd tooling.

### Assistant Understanding

Facts:

- The project requires one writer and multiple readers concurrently on the same file.
- Current live compatibility coverage exists for the regular current writer slice.
- Later SOWs completed compression, compact, FSS, and mixed-directory closed-file validation, but live coverage for those feature slices remains separate.

Inferences:

- Closed-file verification is not enough for production compatibility because publication windows can differ while a writer is active.

Unknowns:

- Exact runtime duration and entry counts should be tuned during implementation to catch races without making routine validation too slow.

### Acceptance Criteria

- Live tests cover regular plus zstd/xz/lz4 DATA compression, compact uncompressed, compact plus compression, and sealed/FSS files.
- Each live feature matrix includes stock `journalctl --file`, stock libsystemd, and every repository reader while the writer is active.
- Final closed-file validation includes `journalctl --verify --file`, with `--verify-key` where required.
- The test records stock systemd version and commands/helpers used.

## Analysis

Sources checked:

- `product-scope.md`
- `SOW-0022-20260525-compatibility-test-gap-audit.md`
- `tests/conformance/live/`
- `tests/interoperability/run_live_matrix.py`
- `tests/interoperability/run_compression_matrix.py`
- `tests/interoperability/run_compact_matrix.py`
- `tests/interoperability/run_mixed_directory_matrix.py`

Current state:

- Existing live tests cover the regular live writer slice.
- Compression, compact, FSS, and mixed-directory tests are primarily closed-file or directory-reader matrices.

Risks:

- Writers can pass closed-file checks but expose inconsistent active metadata while appending.
- Sealed files require careful key handling so live readers do not require verification keys for normal reads but verification does.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Feature-specific writer paths can affect append publication order, object flags, tail metadata, and verification state. The current live harness does not yet exercise those paths under concurrent stock/systemd readers.

Evidence reviewed:

- `product-scope.md` live concurrency contract
- `tests/interoperability/run_live_matrix.py`
- `tests/interoperability/run_compression_matrix.py`
- `tests/interoperability/run_compact_matrix.py`
- `tests/interoperability/run_mixed_directory_matrix.py`
- SOW-0019, SOW-0024, and SOW-0027 status notes in `SOW-status.md`

Affected contracts and surfaces:

- Writer append publication windows.
- Reader live state handling.
- Stock systemd interoperability claims.
- Compatibility matrix documentation.

Existing patterns to reuse:

- Current live matrix runner and `LIVE_SEQ` ordered visibility checks.
- Current compression/compact/FSS generator options.
- Current stock journalctl and libsystemd adapters.

Risk and blast radius:

- High compatibility value, medium runtime/test complexity. Failures may require writer publication changes in all languages.

Sensitive data handling plan:

- Use synthetic fields and deterministic test keys only.

Implementation plan:

1. Extend or add live matrix runners for compression, compact, compact+compression, and sealed files.
2. Reuse existing writer adapters and repository reader adapters.
3. Add stock journalctl and stock libsystemd live reader checks for each feature slice.
4. Run final stock verification after clean close.
5. Document matrix scope and expected runtime.

Validation plan:

- Run the new live feature matrix.
- Run existing live, compression, compact, mixed-directory, and FSS verification matrices as relevant.
- Use read-only reviewers after implementation.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: update `project-journal-compatibility` if feature-live matrices become a mandatory close gate.
- Specs: update `product-scope.md` with the validated feature-live coverage.
- End-user/operator docs: likely unaffected.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: activate only this SOW when implementing.
- SOW-status.md: update on activation and completion.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  - `man/journalctl.xml`
  - `src/libsystemd/sd-journal/sd-journal.c`

Open decisions:

- None.

## Implications And Decisions

- No user decision is required before implementation.

## Plan

1. Build the live feature matrix.
2. Run against stock and repository readers.
3. Fix any compatibility failures.
4. Validate and review as one batch.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current routing decision.

Reviewers:

- Read-only reviewers from the active pool after implementation: minimax, kimi, qwen, glm. Mimo is skipped.

Failure handling:

- Record feature-specific failures and do not claim production compatibility for failed slices.

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
