# SOW-0065 - Parallel Language Parity Closure

## Status

Status: open

Sub-state: pending future multi-agent/per-language parity and performance
closure after Rust is stable and corpus validation is complete.

## Requirements

### Purpose

Close all remaining cross-language SDK parity gaps against Rust as the reference
implementation, while preserving file-format compatibility, reader data
lifetime guarantees, byte identity where required, and best practical
performance for each language.

### User Request

The user stated that at a later point they want to run multiple implementation
agents in parallel, probably in git worktrees, one per SDK language, with each
agent authorized to work on a single language. The goal is to bring each
language to:

1. parity with Rust;
2. the same guarantees Rust provides, especially row-level reader data lifetime;
3. output file-format compatibility, including byte-for-byte identity where
   required;
4. best reader and writer performance the language can provide.

The user also stated that this should happen only after SOW-0055, SOW-0063,
SOW-0064, and probably SOW-0026 for Rust/Go integration readiness.

### Assistant Understanding

Facts:

- Rust is the reference implementation for SDK semantics and compatibility.
- Go has already had substantial writer and reader performance work, but still
  needs a final parity audit after Rust and corpus validation settle.
- Python and Node.js still need the deeper reader/writer performance and parity
  push after the core contracts are stable.
- SOW-0055 must close a known Rust seek-cursor parity gap before using Rust as
  the final cross-language reference.
- SOW-0063 must make the SDK portable across Linux, FreeBSD, macOS, and
  Windows before language parity can be considered final.
- SOW-0064 must validate Rust/Go/systemd against the real corpus before later
  language parity work can claim production readiness.
- Repository rules currently forbid creating git worktrees unless the user
  explicitly asks or approves them at implementation time.
- Repository rules currently route implementation locally and external models
  as reviewers only, unless the user explicitly changes that decision.

Inferences:

- This work should probably split into one child SOW per language or per
  language/surface once it starts, to avoid conflicting edits and to keep review
  batches meaningful.
- Parallel worktrees can be useful here, but they need a clean orchestration
  contract: one worktree, one language, one agent, no cross-language edits
  except shared tests/spec changes through an integration owner.
- Rust must be frozen enough for the parity target to be stable before Python
  and Node.js agents chase it.

Unknowns:

- Exact language split at activation time: Go/Python/Node.js, or only
  Python/Node.js if Go is already accepted after SOW-0064.
- Whether external implementer agents will be re-enabled for this phase.
- Whether worktrees will be created locally by the project manager or by an
  approved harness.
- Final performance targets for Python and Node.js, given language/runtime
  constraints.

### Acceptance Criteria

- A final Rust reference contract exists and includes reader/writer APIs,
  field-name policy behavior, row-level reader data lifetime, output formats,
  byte-identity requirements, portability behavior, and performance-relevant
  options.
- For every target language in scope, parity gaps against Rust are inventoried,
  fixed, tested, and reviewed.
- Reader data lifetime guarantees match the Rust row-level contract or document
  an approved language-specific representation that provides equivalent
  consumer safety.
- Writer output compatibility matches Rust and systemd expectations, including
  byte-for-byte identity for deterministic slices where required.
- Reader and writer performance is profiled and optimized for each language
  until remaining gaps are explained by language/runtime constraints or accepted
  by user decision.
- Shared conformance, interoperability, compact, compression, FSS, live,
  directory, mixed-directory, byte-identity, verification, and corpus-derived
  tests pass for all affected languages.
- Parallel work, if used, is isolated by worktree and language, with explicit
  user approval recorded before worktree creation.
- No language implementation regresses Rust/Go production paths or shared
  compatibility tests.
- All changes are reviewed as meaningful language-level batches before the SOW
  closes.

## Analysis

Sources checked:

- User request in this thread.
- Current SOW dependency state in `.agents/sow/current/` and
  `.agents/sow/pending/`.
- Project orchestration rules in `AGENTS.md` and
  `.agents/skills/project-agent-orchestration/SKILL.md`.
- Project compatibility rules in
  `.agents/skills/project-journal-compatibility/SKILL.md`.

Current state:

- SOW-0055, SOW-0063, SOW-0064, and SOW-0026 are not completed.
- Worktree creation is not authorized by default.
- External implementer agents are not currently enabled by default.
- Python and Node.js large-scale performance work remains a future concern
  after core Rust/Go validation and portability.

Risks:

- Starting this before Rust is stable would make agents chase a moving target.
- Parallel agents can conflict in shared tests, specs, docs, and format
  contracts.
- Performance changes in slower languages can overcomplicate code for marginal
  gains if not measured.
- Byte-identity requirements can be accidentally weakened if every language
  defines its own exception.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Cross-language parity work is needed eventually, but the reference target is
  not stable enough until known Rust parity, portability, real-corpus
  validation, and integration inventory work close.

Evidence reviewed:

- SOW-0055 remains open for Rust seek-cursor systemd parity.
- SOW-0063 remains open for cross-platform portability.
- SOW-0064 remains open for real-world corpus evaluation.
- SOW-0026 remains open for Netdata SDK integration inventory and cut plan.
- Project rules require explicit user approval for git worktree creation and
  external implementer routing changes.

Affected contracts and surfaces:

- Go, Python, and Node.js SDK APIs.
- Rust reference SDK APIs and docs.
- Reader data lifetime guarantees.
- Writer byte identity and file-format compatibility.
- Shared conformance and interoperability suites.
- Performance benchmark/profiling workflows.
- Project orchestration and possible worktree layout.

Existing patterns to reuse:

- SOW-0061 row-scoped current-entry facade lifetime contract.
- SOW-0062 Rust/Go writer performance workflow.
- SOW-0059 standardized benchmark reporting.
- Existing compact/compression/FSS/live/directory/verify/byte-identity
  matrices.
- Current reviewer cadence: implement whole SOW batches, then run read-only
  reviewers.

Risk and blast radius:

- High. This touches multiple languages and public SDK semantics.
- Work must be split and merged carefully to avoid shared-test conflicts.
- The SOW should not start until dependencies close and the user explicitly
  authorizes any parallel worktree/agent execution.

Sensitive data handling plan:

- Use synthetic fixtures and sanitized corpus-derived discrepancy cases only.
- Do not record real journal data, customer data, SNMP communities, credentials,
  bearer tokens, personal data, private endpoints, or proprietary incident
  details.

Implementation plan:

1. Wait for prerequisite SOWs to close: SOW-0055, SOW-0063, SOW-0064, and the
   relevant SOW-0026 inventory/cut-plan gate.
2. Freeze the Rust reference contract from specs, docs, tests, and code.
3. Inventory gaps by language against the Rust reference.
4. Ask the user to approve the execution topology:
   - sequential local implementation; or
   - parallel worktrees with one language per worktree and one authorized
     implementer per language.
5. Create child SOWs or implementation tasks for each language/surface.
6. Run language-specific implementation, validation, and performance profiling.
7. Merge results through shared tests/spec/docs, then run cross-language
   integration validation and read-only reviews.

Validation plan:

- Full shared conformance matrix for all affected languages.
- Byte-identity validation for deterministic writer slices.
- Live one-writer/multiple-reader tests.
- Row-level reader data lifetime tests.
- Corpus-derived regression tests from SOW-0064 discrepancies.
- Language-specific benchmark and profiling reports.
- Whole-SOW read-only reviewer passes.
- `.agents/sow/audit.sh` and `git diff --check`.

Artifact impact plan:

- AGENTS.md: update only if parallel worktree/agent workflow becomes a durable
  project rule.
- Runtime project skills: likely update orchestration/compatibility skills if
  per-language parallel work becomes a reusable pattern.
- Specs: update cross-language API and lifetime contracts.
- End-user/operator docs: update language SDK docs for parity guarantees and
  performance options.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: likely split into child SOWs when activated.
- SOW-status.md: add this SOW to Pending.

Open-source reference evidence:

- No external open-source repositories were checked while creating this SOW.

Open decisions:

- Blocked until prerequisite SOWs close.
- Blocked until the user explicitly approves whether this runs sequentially or
  with parallel git worktrees and external implementer agents.

## Implications And Decisions

1. 2026-05-30 future parallel parity phase
   - Decision: record a future parity closure phase that may use one worktree
     and one implementation agent per target language.
   - Implication: this is not authorized to start yet; it depends on Rust
     stability, portability, corpus validation, and integration inventory.
   - Risk: starting early would create churn and conflicting changes.

2. 2026-05-30 Rust as reference
   - Decision: Rust remains the reference implementation for API semantics,
     reader lifetime, compatibility guarantees, and performance options.
   - Implication: SOW-0055 must close before this phase uses Rust as final
     truth.
   - Risk: a remaining Rust semantic gap could be copied into every language.

## Plan

1. Close prerequisites.
2. Freeze and document Rust reference contract.
3. Inventory language gaps.
4. Get explicit execution-topology approval.
5. Run per-language parity/performance implementation.
6. Run full cross-language validation and reviews.

## Delegation Plan

Implementer:

- Pending user decision. Current default remains local implementation with no
  external implementer agents. Parallel worktrees and external implementers must
  be explicitly approved before use.

Reviewers:

- Use read-only reviewers from the approved pool after each meaningful language
  batch and final integration validation.

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

- If parallel agents conflict, stop merging and reconcile through a single
  integration pass.
- If a language cannot match Rust guarantees directly, return with evidence and
  user decision options.
- If performance optimization threatens maintainability or compatibility, keep
  the measured safe path and record the rejected approach.

## Execution Log

### 2026-05-30

- Created this pending SOW from the user's future plan for parallel per-language
  parity and performance closure.

## Validation

Acceptance criteria evidence:

- Pending implementation.

Tests or equivalent validation:

- Pending implementation.

Real-use evidence:

- Pending implementation.

Reviewer findings:

- Pending implementation.

Same-failure scan:

- Pending implementation.

Sensitive data gate:

- Planning artifact contains no raw sensitive data.

Artifact maintenance gate:

- AGENTS.md: no update during SOW creation.
- Runtime project skills: no update during SOW creation.
- Specs: pending implementation.
- End-user/operator docs: pending implementation.
- End-user/operator skills: no output/reference skill affected during SOW
  creation.
- SOW lifecycle: created as `Status: open` under `.agents/sow/pending/`.
- SOW-status.md: updated to list this SOW as pending.

Specs update:

- Pending implementation.

Project skills update:

- Pending implementation.

End-user/operator docs update:

- Pending implementation.

End-user/operator skills update:

- Pending implementation.

Lessons:

- Parallel language work should happen only after the reference contract is
  stable enough to prevent churn.

Follow-up mapping:

- None yet. Child SOWs should be created when this phase is activated.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
