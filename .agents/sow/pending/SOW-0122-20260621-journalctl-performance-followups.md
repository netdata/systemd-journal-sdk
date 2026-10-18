# SOW-0122 - Journalctl Performance Followups

## Status

Status: open

Sub-state: pending activation; prerequisite SOW-0121 is completed. This separate
scalability opportunity does not replace mandatory performance preservation in
the user-requested SDK/journalctl workstreams prepared on 2026-10-14.

## Requirements

### Purpose

Keep the portable Rust and Go `journalctl` commands fit for large offline
journal files and directories while preserving the SOW-0121 stock-compatible
file-backed behavior.

### User Request

SOW-0121 reviewer and local close-out evidence identified remaining
performance-contract work that is not a file-backed correctness blocker but
should be tracked before release-quality performance claims.

### Assistant Understanding

Facts:

- SOW-0121 passed its shared stock-oracle parser, directory, and query matrices
  at its earlier close. New combinations failed the 2026-10-14 audit; the
  originating SOW was reopened, repaired those failures and is now completed.
- SOW-0121's 2026-10-19 unit/invocation audit supersedes the old row-scan
  description. Its provisional implementation enumerated indexed unique IDs
  and reopened readers for unit-restricted bounds. That approximation had
  confirmed correctness gaps. Completed SOW-0121 delivers native directional
  discovery, global ID bounds, reader reuse and performance evidence.
- `--follow` uses portable polling over explicit file/directory inputs. That
  preserves non-Linux correctness and purity, but may be more expensive than a
  daemon-backed Linux journal wait path on very large inputs.

Inferences:

- Broader input-size envelopes and portable polling costs are scalability
  opportunities. Confirmed invocation correctness gaps and the performance of
  their repairs were resolved in SOW-0121 and are not deferred here.
- A production-grade release decision should either improve these paths or
  record measured evidence that the existing shape is acceptable for the target
  package.

Unknowns:

- The target large-journal envelope for the portable command has not been set.
- Native macOS, Windows, and FreeBSD runtime performance has not been measured
  for these exact paths.

### Acceptance Criteria

- Measure `--list-invocations`, `--follow`, reverse output, and large
  file/directory selection on representative synthetic fixtures.
- Evaluate remaining large-input costs using journal-native index/facade paths
  against SOW-0121's delivered indexed invocation baseline.
- Preserve SOW-0121 parser and query parity results.
- Record any intentionally retained slower path with measured cost and release
  implications.

## Analysis

Sources checked:

- `.agents/sow/done/SOW-0121-20260621-file-backed-journalctl-full-parity.md`
- `.agents/sow/specs/product-scope.md`
- `.agents/sow/specs/journalctl-v260-parity-matrix.md`
- `docs/Journalctl-CLI.md`

Current state:

- Confirmed correctness gaps and mandatory performance evidence are resolved
  in completed SOW-0121; SDK primitive repairs completed in SOW-0159.
- Performance work remains for invocation listing and follow scalability.
- This SOW remains pending and is not a third active workstream in the new
  two-SOW request. Broader optimizations here are not prerequisites for measuring
  or preserving performance of fixes in SOW-0121 and SOW-0159.

Risks:

- A premature release claim could overpromise performance on large archives.
- A rushed optimization could break stock-compatible filtering, boot context,
  invocation context, or cursor order.

## Pre-Implementation Gate

Status: pending activation

Problem / root-cause model:

- Portable polling and the large-input envelope need measurement after the
  active correctness work. Activation must inspect the delivered indexed
  implementation rather than assume the historical invocation algorithm.

Evidence reviewed:

- SOW-0121 residual performance note and reviewer dispositions.

Affected contracts and surfaces:

- Rust `journalctl` CLI.
- Go `journalctl` CLI.
- Go and Rust facade/index helper APIs if needed.
- Shared parser and interoperability matrices.
- Public CLI documentation and product scope specs.

Existing patterns to reuse:

- FIELD/DATA indexed unique-value enumeration.
- Cursor seek and previous/next facade primitives.
- Shared stock-oracle interoperability tests.

Risk and blast radius:

- Medium. The work touches user-visible ordering, filtering, boot/invocation
  context, and follow behavior.

Sensitive data handling plan:

- Use only synthetic repository-local fixtures. Do not read host journals or
  record raw host/customer data.

Implementation plan:

1. Build large synthetic fixtures for invocation, boot, unit, and follow
   scenarios.
2. Benchmark current Rust and Go behavior against stock file-backed
   `journalctl` where stock can run.
3. Implement indexed or bounded-state paths where measurement justifies them.
4. Preserve correctness with parser, directory, and query matrices.

Validation plan:

- `python tests/parser-parity/check_v260_manifest.py`
- `python tests/parser-parity/run_parser_parity.py`
- `python tests/interoperability/run_directory_matrix.py`
- `python tests/interoperability/run_journalctl_query_matrix.py`
- Targeted large-fixture performance measurements for Rust and Go.
- Read-only external review after local validation.

Artifact impact plan:

- AGENTS.md: expected unaffected.
- Runtime project skills: update only if the work changes durable workflow.
- Specs: update product scope and parity matrix if behavior or performance
  guarantees change.
- End-user/operator docs: update `docs/Journalctl-CLI.md` if limits or claims
  change.
- End-user/operator skills: none expected.
- SOW lifecycle: this SOW tracks SOW-0121 follow-up performance work.
- SOW-status.md: update on activation and close.

Open-source reference evidence:

- None checked yet in this pending SOW. Activation should inspect the CLI's
  `systemd/systemd` v262 target paths
  for invocation listing and follow behavior.

Open decisions:

- Pending activation priority and target performance envelope.

## Implications And Decisions

Pending activation.

## Plan

1. Define fixture sizes and target envelopes.
2. Measure current behavior.
3. Optimize only paths with measurable payoff or release risk.
4. Validate parity and performance.

## Implementation And Review Plan

Implementation:

- The implementation plan remains pending user activation and completion of the
  pre-implementation gate.

Reviewers:

- External review authorization is pending activation. Recommend whole-SOW
  review after local validation and before GitHub submission; when authorized,
  follow the system-wide `external-reviewers` skill.

Repository boundary block for every external-reviewer prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- Do not make changes outside this repository for any reason.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- Record benchmark, parity, reviewer, and audit failures before any close.

## Execution Log

### 2026-06-21

- Created as the explicit follow-up mapping for SOW-0121 residual performance
  work.

### 2026-10-14

- Updated the dependency/reference after SOW-0121 regression reopening. The
  user requires performance as-is or better for both requested workstreams;
  their correctness and no-regression gates cannot be deferred to this SOW.
- Kept broad invocation/follow scalability work pending without activation or
  merging it into the two requested scopes.

### 2026-10-19

- Reconciled the historical invocation description with the confirmed native
  correctness repair in SOW-0121. This SOW retains only broader envelope and
  polling opportunities; it does not defer required behavior or performance
  preservation and remains pending.

## Validation

Pending activation.

## Outcome

Pending.

## Lessons Extracted

Pending activation.

## Followup

Pending activation.
