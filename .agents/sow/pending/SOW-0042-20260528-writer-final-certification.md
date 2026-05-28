# SOW-0042 - Writer Final Certification

## Status

Status: open

Sub-state: created on 2026-05-28 as the writer side of SOW-0009.

## Requirements

### Purpose

Certify writer correctness and performance across Rust, Go, Python, and Node.js
after all writer feature and parity SOWs are complete.

### User Request

The user wants writer benchmarks first, with compact format, compression
disabled, and FSS disabled as the baseline. The baseline must reflect
production settings and compare against systemd C and Netdata's current
vendored Rust behavior where applicable.

### Assistant Understanding

Facts:

- Writer performance should be measured independently from reader performance.
- The writer baseline must use fixed, explicit settings such as 128 MiB
  max-size when measuring single-file behavior.
- Directory rotation benchmarks should be separate from single-file benchmarks.
- The user-reported SNMP traps result on `v0.3.0` is strong integration
  evidence but not a substitute for controlled SDK benchmarks.

Inferences:

- This SOW should run after SOW-0037, SOW-0040, and SOW-0041 so writer API and
  behavior no longer move under the benchmark.

Unknowns:

- Final pass/fail thresholds per language and consumer.

### Acceptance Criteria

- Writer benchmarks cover Rust, Go, Python, Node.js, and systemd C.
- Baseline writer mode is compact format, compression disabled, FSS disabled,
  explicit max-size, explicit live publication cadence, and one writer.
- Reports separately cover single-file and directory-rotation writer behavior.
- Reports include rows/sec, bytes/sec, output size, CPU time, wall time, memory
  allocation behavior where available, syscall/file-access behavior where
  available, sync/flush cadence, and validation status.
- Benchmarks compare raw full-payload append and structured field append where
  public APIs expose both.
- Generated outputs pass shared conformance and stock systemd verification
  where the selected field-name policy is systemd-friendly.
- Performance issues are profiled before optimization.
- Any residual performance gap is either fixed or explicitly accepted by the
  user with evidence.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`
- `.agents/sow/specs/product-scope.md`

Current state:

- Writer certification waits for writer closure and parity SOWs.

Risks:

- Benchmark settings that do not match production would invalidate results.
- Optimizing writers after this SOW closes would require re-running the full
  certification matrix.

## Pre-Implementation Gate

Status: blocked until SOW-0037, SOW-0040, and SOW-0041 close

Problem / root-cause model:

- Writer performance must be measured only after the writer contract is stable.

Evidence reviewed:

- SOW-0009 umbrella performance requirements.
- User-provided SNMP traps benchmark context.

Affected contracts and surfaces:

- Writer performance claims, public docs, Netdata integration readiness, and
  release notes.

Existing patterns to reuse:

- SOW-0014 deterministic dataset.
- SOW-0015 ingesters.
- Existing `.local/benchmarks/` result convention.
- Shared conformance and interoperability suites.

Risk and blast radius:

- High for Netdata integration readiness.

Sensitive data handling plan:

- Use generated or sanitized datasets only. Do not record real logs, SNMP
  communities, customer data, credentials, bearer tokens, personal data, private
  endpoints, or production incident details.

Implementation plan:

1. Freeze writer benchmark settings.
2. Run writer baselines.
3. Profile underperforming implementations.
4. Apply optimization batches and re-run full writer validation.
5. Publish final benchmark report in repo artifacts.

Validation plan:

- Benchmark command logs.
- Conformance/interoperability tests.
- Stock systemd verification for compatible outputs.
- Read-only reviewer passes.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update if benchmark process becomes durable.
- Specs: update writer performance and certification status.
- End-user/operator docs: update benchmark/API docs if public.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: close only with final report and validation.
- SOW-status.md: update when activated and closed.

Open-source reference evidence:

- systemd evidence will be recorded when the benchmark harness is run.

Open decisions:

- Final thresholds require evidence from the first valid run.

## Implications And Decisions

- 2026-05-28: user agreed writer performance should be separate from reader
  performance and happen after writer feature/parity work.

## Plan

1. Wait for writer closure SOWs.
2. Run writer baseline.
3. Optimize based on profiles.
4. Certify and document.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Read-only reviewers from the approved pool.

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

- Record benchmark invalidation, profiler findings, reviewer findings, and
  residual performance risks before close.

## Execution Log

### 2026-05-28

- Created from the agreed SOW restructuring.

## Validation

Acceptance criteria evidence:

- Pending.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending.

Reviewer findings:

- Pending.

Same-failure scan:

- Pending.

Sensitive data gate:

- Pending implementation; planning text contains no raw sensitive data.

Artifact maintenance gate:

- Pending implementation.

Specs update:

- Pending implementation.

Project skills update:

- Pending implementation.

End-user/operator docs update:

- Pending implementation.

End-user/operator skills update:

- Pending implementation.

Lessons:

- Pending.

Follow-up mapping:

- Pending implementation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
