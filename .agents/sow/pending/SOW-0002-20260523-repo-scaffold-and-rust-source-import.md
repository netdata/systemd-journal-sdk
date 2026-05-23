# SOW-0002 - Repository Scaffold And Rust Source Import

## Status

Status: open

Sub-state: pending implementation after SOW-0001 closes and reviewers approve bootstrap.

## Requirements

### Purpose

Create the initial repository structure and import the selected Rust reader/writer sources without changing behavior.

### User Request

Copy the Netdata Rust journal reader and writer implementation into this project as the canonical Rust starting point.

### Assistant Understanding

Facts:

- Netdata Rust journal reader and writer sources are the canonical starting point.
- This phase must copy sources and set up repository structure without changing SDK behavior.

Inferences:

- Rust workspace layout must preserve provenance and minimize behavior drift.

Unknowns:

- The exact Rust workspace/package layout is not selected yet.

### Acceptance Criteria

- Repository has language/package layout for Rust, Go, Node.js, Python, CLIs, benchmarks, and documentation.
- Repository has preliminary shared fixtures/tests directories that SOW-0003 may refine after the shared harness schema is selected.
- Rust sources are copied from Netdata with provenance recorded.
- Imported Rust code builds or has all build blockers recorded with concrete evidence.
- No SDK behavior is rewritten in this phase unless required only to make the copied code build in this repo.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `ktsaou/netdata @ 6a515000ac89`, `src/crates/jf/`
- `ktsaou/netdata @ 6a515000ac89`, `src/crates/journal-core/`
- `ktsaou/netdata @ 6a515000ac89`, `src/crates/journal-log-writer/`

Current state:

- Pending activation after SOW-0001 closes.
- Rust layout decision remains open.

Risks:

- A broad import can carry unnecessary workspace coupling.
- A narrow import can accidentally rewrite behavior while trying to make copied code build.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- The repository is empty and needs structure plus Rust source import before conformance tests and other language ports can start.

Evidence reviewed:

- `ktsaou/netdata @ 6a515000ac89`, `src/crates/jf/`
- `ktsaou/netdata @ 6a515000ac89`, `src/crates/journal-core/`
- `ktsaou/netdata @ 6a515000ac89`, `src/crates/journal-log-writer/`

Affected contracts and surfaces:

- Repository layout.
- Rust crate/package boundaries.
- Build tooling.
- Provenance documentation.

Existing patterns to reuse:

- Netdata `jf` reader compatibility layer.
- Netdata `journal-core` and `journal-log-writer` writer stack.

Risk and blast radius:

- Importing too much unrelated Netdata workspace code could create maintenance and dependency noise.
- Importing too little could break the copied crates or hide behavior changes.

Sensitive data handling plan:

- No sensitive runtime data is expected.
- Source evidence must cite upstream repository plus commit and relative paths.

Implementation plan:

1. Create repo layout.
2. Copy required Rust sources into the repo.
3. Record source provenance.
4. Make minimal build-setup adjustments inside this repo.
5. Run Rust build checks and record blockers.
6. Leave shared fixture/test schema decisions to SOW-0003 unless a minimal placeholder is needed.

Validation plan:

- Rust build/check command succeeds or blockers are recorded.
- File provenance list is complete.
- `git status --short` shows only files inside this repo.

Artifact impact plan:

- AGENTS.md: likely unchanged.
- Runtime project skills: update only if import teaches a durable workflow.
- Specs: update if source import changes public scope.
- End-user/operator docs: create placeholder only if needed by repo layout.
- SOW lifecycle: move to current before implementation.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

1. Rust workspace/package layout must be selected before implementation.
   - Option A: Preserve Netdata crate boundaries under a Rust workspace, for example separate imported crates for `jf`, `journal-core`, and `journal-log-writer`.
     - Pros: maximizes source provenance clarity and minimizes accidental behavior rewrites during import.
     - Cons: may carry more internal crate structure than the final public SDK needs.
     - Implication: SOW-0004 can add the public Rust SDK/facade on top after the copied code builds.
     - Risk: workspace dependency wiring may be noisier initially.
   - Option B: Flatten imported Rust code into one SDK crate immediately.
     - Pros: simpler top-level package for downstream Rust users.
     - Cons: high chance of rewriting behavior while claiming an as-is copy.
     - Implication: provenance and future upstream comparison become harder.
     - Risk: reviewers may reject the import as not copied as-is.
   - Option C: Create a public SDK crate plus internal imported crates in the same workspace.
     - Pros: prepares the final API while preserving imported implementation boundaries.
     - Cons: larger first phase and more build setup before tests exist.
     - Implication: SOW-0002 and SOW-0004 boundaries become less clean.
     - Risk: public API decisions may be made before the shared harness constrains them.
   - Recommendation: Option A for SOW-0002, then add the Rust SDK facade in SOW-0004 after SOW-0003 defines the shared harness.
   - Selection: pending activation decision.

## Implications And Decisions

1. Rust source import scope
   - Current state: unresolved.
   - Required before activation: select the Rust workspace/package layout and record why it preserves copied Netdata behavior with minimal drift.
- Implication: this decision determines how the Rust import, future bindings, shared tests, and provenance documentation are organized.
   - Risk: a broad import can carry unnecessary Netdata workspace coupling; a narrow import can accidentally rewrite behavior while trying to make the copied code build.

## Plan

1. Resolve and record the Rust workspace/package layout decision before implementation.
2. Activate this SOW by moving it to `current/` and setting `Status: in-progress`.
3. Delegate implementation to the selected implementer using the repository-boundary block.
4. Run build checks, review, audit, and commit only after validation is complete.

## Delegation Plan

- Implementer: `llm-netdata-cloud/minimax-m2.7-coder`, or fallback to `llm-netdata-cloud/qwen3.6-plus` then `llm-netdata-cloud/glm-5.1` if minimax fails or is unavailable.
- Reviewers: at least two from `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/glm-5.1`.
- Every prompt must include the canonical repository-boundary block from `AGENTS.md`.
- Failure handling: record implementer or reviewer model failure in this SOW, substitute only from the approved model list, rerun full-scope review after fixes, and do not close if `.agents/sow/audit.sh` fails.

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
