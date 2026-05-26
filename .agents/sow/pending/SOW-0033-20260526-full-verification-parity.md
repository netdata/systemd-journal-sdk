# SOW-0033 - Full Verification Parity

## Status

Status: open

Sub-state: Split from SOW-0022 Gap 2. Ready for implementation after activation.

## Requirements

### Purpose

Make repository verification APIs reject the same practical corruption classes as stock systemd verification for the supported journal feature slices.

### User Request

The SDKs must not falsely claim compatibility when stock `journalctl --verify --file` would reject a journal file.

### Assistant Understanding

Facts:

- SOW-0019 added useful unsealed and sealed verification APIs in all four languages.
- SOW-0022 found those APIs still shallower than systemd object-graph verification.
- Full parity requires corrupted fixture families and stock systemd as an oracle.

Inferences:

- This is a larger validation and implementation SOW than header parsing or threshold work.

Unknowns:

- Some systemd verification classes may be impractical or unsafe to generate as committed fixtures. Each exception must be recorded with evidence.

### Acceptance Criteria

- Shared corrupted fixtures cover practical object type, size, hash, chain, entry-array, header-counter, main-entry-array, seqnum, monotonic, and TAG/FSS corruption classes.
- Stock `journalctl --verify --file` rejects each negative fixture.
- Rust, Go, Node.js, and Python verification APIs reject each negative fixture with controlled verification errors.
- Positive fixtures for supported feature slices still pass.

## Analysis

Sources checked:

- `SOW-0022-20260525-compatibility-test-gap-audit.md`
- `SOW-0019-20260524-forward-secure-sealing.md`
- `product-scope.md`
- Current `verify` implementations in all four languages
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`

Current state:

- Verification APIs walk important entry/data/TAG paths.
- They are not yet documented as full systemd object-graph verification parity.

Risks:

- Shallow verification can accept files that stock systemd rejects.
- Corruption fixture generation must avoid fragile binary hacks without a clear oracle.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Repository verification covers core readable paths and sealed TAG checks, but systemd verification also checks object graph reachability, hash-table membership, entry-array sortedness, counter consistency, and multiple metadata invariants.

Evidence reviewed:

- Go `go/journal/verify.go`
- Rust `rust/src/journal/src/lib.rs`
- Node.js `node/src/lib/verify.js`
- Python `python/journal/verify.py`
- `tests/conformance/manifests/conformance-v01.json`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-verify.c`

Affected contracts and surfaces:

- Verification APIs and controlled error types.
- File-backed journalctl `--verify` and `--verify-key`.
- Conformance fixtures/manifests/adapters.
- Product compatibility claims.

Existing patterns to reuse:

- Existing verification APIs from SOW-0019.
- Existing conformance adapter negative cases.
- Stock journalctl verification oracle.

Risk and blast radius:

- High compatibility value and high implementation complexity. Changes can affect all readers and CLI verification paths.

Sensitive data handling plan:

- Generate synthetic fixtures only. Do not use live host journals or private data.

Implementation plan:

1. Inventory systemd verification classes and map each to a practical fixture plan.
2. Add fixture generators and manifest cases with stock systemd oracle checks.
3. Implement missing checks in Rust, Go, Node.js, and Python.
4. Preserve controlled errors and avoid panics/crashes on malformed input.
5. Update docs/specs to state the verified parity envelope and any explicit exceptions.

Validation plan:

- Run full conformance verification cases.
- Run stock `journalctl --verify --file` oracle for every negative and positive fixture.
- Run file-backed journalctl `--verify` in all languages.
- Run read-only reviewers after implementation.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: update `project-journal-compatibility` if full verification fixture coverage becomes a mandatory gate.
- Specs: update `product-scope.md` with verification parity envelope and exceptions.
- End-user/operator docs: update CLI/API docs if verification behavior changes.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: activate only this SOW when implementing.
- SOW-status.md: update on activation and completion.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  - `src/libsystemd/sd-journal/journal-verify.c`
  - `src/libsystemd/sd-journal/journal-def.h`

Open decisions:

- None. SOW-0022 recorded that full verification parity remains in follow-up work.

## Implications And Decisions

- No user decision is required before implementation, unless fixture feasibility exposes a parity class that cannot be safely generated.

## Plan

1. Build the corruption fixture inventory.
2. Add stock-oracle negative tests.
3. Implement missing verification checks.
4. Validate and review as one batch.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current routing decision.

Reviewers:

- Read-only reviewers from the active pool after implementation: minimax, kimi, qwen, glm. Mimo is skipped.

Failure handling:

- Record any unsupported verification class with evidence and a follow-up decision before closing.

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
