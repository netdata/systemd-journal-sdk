# SOW-0020 - Directory Traversal Parity

## Status

Status: open

Sub-state: pending after SOW-0008 interoperability closeout.

## Requirements

### Purpose

Bring SDK directory readers and file-backed journalctl `--directory` behavior to parity with stock journalctl for supported journal directory layouts.

### User Request

The user requires journalctl rewrites and SDK readers to interoperate across files produced by all writers. SOW-0008 proved live file compatibility, but full directory traversal parity remains separate.

### Assistant Understanding

Facts:

- Current directory readers handle active/archive files and simple machine-id subdirectories.
- The live matrix discovers a generated file and validates file-backed readers; it does not prove complete `--directory` traversal parity.
- Product scope records sequential directory iteration limitations for overlapping multi-file directories.

Inferences:

- Full parity needs fixtures with multiple machine IDs, namespaces where applicable, active and archived files, whole-file `.zst` files, overlapping realtime ranges, boot IDs, and corrupted files.
- Ordering semantics should be compared against stock journalctl, not invented independently.

Unknowns:

- Exact subset of namespace and subdirectory behavior expected for file-backed SDK use versus daemon-only behavior.

### Acceptance Criteria

- A directory traversal fixture suite covers active files, archived files, machine-id subdirectories, namespace-like layouts where file-backed stock journalctl supports them, whole-file `.zst` journal files, overlapping realtime ranges, multiple boot IDs, and corrupted/unreadable files.
- Rust, Go, Node.js, and Python directory readers return the same supported entries and ordering as stock `journalctl --directory` for accepted fixtures.
- File-backed journalctl rewrites in all four languages match stock filtering, boot listing, field listing, JSON/export/text output, repeated-match OR, `+` disjunction, and error behavior for directory inputs.
- Unsupported daemon-only or environment-dependent behavior is documented and tested as controlled unsupported behavior.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0008-20260523-interoperability-and-full-writer-features.md`
- `.agents/sow/specs/product-scope.md`
- `tests/interoperability/run_live_matrix.py`
- `tests/interoperability/README.md`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `man/journalctl.xml`

Current state:

- Directory iteration exists but is validated primarily for non-overlapping active/archive files.
- The live matrix validates file-backed reader compatibility by discovering specific generated files, not by requiring complete directory traversal parity.

Risks:

- Incorrect ordering can silently change query results for overlapping files.
- Recursing too broadly can read unrelated journals or artifacts.
- Error parity is subtle for corrupted or unreadable files.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The project has strong file-level compatibility, but directory traversal semantics remain weaker than stock journalctl behavior. This can affect user-visible journalctl rewrites and SDK directory readers when users point them at real journal directories.

Evidence reviewed:

- SOW-0008 explicitly records directory traversal parity as a remaining gap.
- Product scope records sequential directory iteration and overlapping multi-file ordering limitations.
- Interoperability README states live matrix does not prove directory traversal parity.

Affected contracts and surfaces:

- DirectoryReader APIs in all languages.
- File-backed journalctl `--directory` behavior in all languages.
- Boot listing, field listing, matching, ordering, and output formatting.
- Interoperability fixtures and docs.

Existing patterns to reuse:

- `tests/interoperability/run_matrix.py` query checks.
- `tests/interoperability/run_live_matrix.py` generated directory layouts.
- Stock journalctl comparison harness.
- Shared conformance manifest patterns.

Risk and blast radius:

- Medium. Directory traversal touches reader selection/order logic but should not affect writer object format.

Sensitive data handling plan:

- Use synthetic generated directory fixtures under `.local/` or committed sanitized fixtures only. Do not inspect or commit real system journal content.

Implementation plan:

1. Inventory stock journalctl `--directory` behavior for supported layouts.
2. Build synthetic directory fixtures and expected stock outputs.
3. Update readers and journalctl rewrites to match supported stock behavior.
4. Add shared directory parity matrix.
5. Update docs/specs with supported and unsupported directory behavior.

Validation plan:

- Directory parity matrix comparing stock and repository outputs.
- Existing file-level, live, binary, compression, and lock matrices remain passing.
- Tests cover ordering, filters, boot listing, fields, JSON/export/text, and controlled corrupt-file behavior.
- External reviewers check unwanted recursion, deletion, or artifact side effects.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update if directory matrix becomes mandatory for reader changes.
- Specs: update product scope with exact directory behavior.
- End-user/operator docs: update README and journalctl help/behavior docs.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: pending until activated; may split fixture inventory from implementation if needed.
- SOW-status.md: update when activated or closed.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `man/journalctl.xml`
- `src/libsystemd/sd-journal/sd-journal.c`

Open decisions:

- None blocking activation. If namespace directory behavior depends on daemon state rather than file-backed stock behavior, record evidence and exclude daemon-only behavior from this SOW.

## Implications And Decisions

1. Directory parity boundary
   - Decision: match stock file-backed `journalctl --directory` behavior, not daemon-only discovery or live daemon state.
   - Reason: project scope is file-backed SDK/journalctl behavior.
   - Risk: unsupported daemon-dependent behavior must be explicit to avoid false parity claims.

## Plan

1. Inventory stock directory traversal behavior.
2. Build directory fixtures and expected outputs.
3. Implement reader and journalctl parity fixes.
4. Add shared matrix and docs.
5. Review and commit verified chunks.

## Delegation Plan

Implementer:

- Preferred implementer is `llm-netdata-cloud/minimax-m2.7-coder`.

Reviewers:

- At least two reviewers from the approved pool.

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

- Record implementer failure, reviewer failure, audit failure, fixture uncertainty, or model unavailability before changing plan or model.

## Execution Log

Pending activation.

## Validation

Pending activation and implementation.

## Outcome

Pending.

## Lessons Extracted

Pending activation.

## Followup

Pending activation.

## Regression Log

None yet.
