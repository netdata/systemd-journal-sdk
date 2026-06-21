# SOW-0122 - Journalctl Performance Followups

## Status

Status: open

Sub-state: pending activation after SOW-0121 close.

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

- SOW-0121 brought Rust and Go file-backed `journalctl` correctness parity to
  the shared stock-oracle parser, directory, and query matrices.
- `--list-invocations` still derives invocation ranges by iterating matching
  entries instead of using a more systemd-like indexed discovery path.
- `--follow` uses portable polling over explicit file/directory inputs. That
  preserves non-Linux correctness and purity, but may be more expensive than a
  daemon-backed Linux journal wait path on very large inputs.

Inferences:

- These are performance and scalability risks, not observed SOW-0121
  correctness regressions.
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
- Replace entry-scan behavior with journal-native index/facade paths where the
  journal format can answer the request safely and measurably faster.
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

- Correctness parity is validated by SOW-0121.
- Performance work remains for invocation listing and follow scalability.

Risks:

- A premature release claim could overpromise performance on large archives.
- A rushed optimization could break stock-compatible filtering, boot context,
  invocation context, or cursor order.

## Pre-Implementation Gate

Status: pending activation

Problem / root-cause model:

- Some portable CLI actions still use correctness-first row iteration or
  polling where an optimized implementation should prefer journal-native
  indexes, cursor seeks, and bounded state.

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

- None checked yet. Activation should inspect `systemd/systemd` v260.1 paths
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

## Delegation Plan

Implementer:

- Pending user activation and routing decision.

Reviewers:

- Pending user activation.

Repository boundary block for every external-agent prompt:

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

## Validation

Pending activation.

## Outcome

Pending.

## Lessons Extracted

Pending activation.

## Followup

Pending activation.
