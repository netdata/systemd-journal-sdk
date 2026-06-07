# SOW-0097 - Go Codacy Metric Debt Refactor

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: opened from SOW-0096 Codacy file metrics audit; blocked on user
priority decision before implementation.

## Requirements

### Purpose

Reduce Go production complexity and duplication metrics only where doing so
improves maintainability without hurting journal reader/writer performance,
compatibility, or Netdata integration behavior.

### User Request

The user asked for Codacy file-by-file analysis of Rust and Go complexity and
duplication, with interest in whether indicators are reasonable. SOW-0096 found
real Go production file-size and ownership pressure that needs a dedicated
refactor decision.

### Assistant Understanding

Facts:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md` classifies Go production
  files using Codacy file metrics plus local Lizard max function CCN.
- Top Go production complexity files are:
  - `go/journal/netdata.go`: Codacy complexity `870`, local max CCN `12`.
  - `go/journal/explorer.go`: Codacy complexity `763`, local max CCN `12`,
    duplication `111`.
  - `go/cmd/journalctl/main.go`: Codacy complexity `304`, local max CCN `12`.
  - `go/journal/verify_graph.go`: Codacy complexity `276`, local max CCN `12`.
  - `go/journal/directory_reader.go`: Codacy complexity `263`, local max CCN
    `12`, duplication `101`.
- Local function-level complexity did not exceed the current CCN gate in the
  tracked Rust/Go file set.

Inferences:

- The Go metric problem is primarily file-size and responsibility pressure, not
  one obviously dangerous function.
- Refactoring should be structural and benchmark-backed. Splitting files
  mechanically just to reduce Codacy percentages would be metric gaming unless
  ownership boundaries become clearer and tests/benchmarks remain stable.

Unknowns:

- Which Go files can be split cleanly without fragmenting hot-path logic.
- Whether Codacy duplication drops meaningfully after structural splits, or if
  repeated journal-format validation logic remains inherently similar.

### Acceptance Criteria

- User-approved Go refactor target list and priority order are recorded before
  implementation.
- Refactors preserve public Go APIs unless a separate user decision allows API
  changes.
- Go unit, interoperability, journalctl, and benchmark smoke validation pass.
- Codacy file metrics are rechecked after push and compared against SOW-0096
  baseline.

## Analysis

Sources checked:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md`.
- `.local/codacy/file-metrics-rust-go.validation.json` as scratch evidence.
- `.local/codacy/lizard-rust-go.csv` as scratch local CCN evidence.

Current state:

- Go has production file ownership pressure in the SDK, Explorer, directory
  reader, verifier, journalctl, and Netdata function wrapper surfaces.
- No Go function exceeded local max CCN `12` in the SOW-0096 tracked file set.

Risks:

- Splitting hot-path files can reduce Codacy file metrics while making runtime
  behavior harder to reason about.
- Refactoring the Explorer or Netdata wrapper before Netdata integration can
  destabilize recently validated parity and performance.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- Codacy file complexity is high because several Go files own large API,
  query, verification, or compatibility surfaces. Local Lizard evidence shows
  this is not currently a single-function complexity problem.

Evidence reviewed:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md`: Go top complexity and
  duplication tables.

Affected contracts and surfaces:

- Go public SDK APIs.
- Go Explorer and Netdata function compatibility behavior.
- Go file-backed journalctl behavior.
- Go reader/writer benchmarks and interoperability tests.

Existing patterns to reuse:

- Existing Go package boundary under `go/journal/`.
- Existing focused files such as `writer_objects.go`, `writer_arrays.go`, and
  `writer_compression.go` as examples of split-by-format-responsibility.

Risk and blast radius:

- Medium-to-high for Explorer/Netdata wrapper files because they are recent and
  Netdata-facing.
- Medium for verifier/journalctl files because behavior is CLI/compatibility
  visible.

Sensitive data handling plan:

- Do not commit raw Codacy API exports. Durable artifacts may include file
  paths and numeric metrics only.

Implementation plan:

1. Ask the user to approve the first Go file group to refactor.
2. Split by stable ownership boundaries, not by arbitrary line count.
3. Run Go tests, interoperability smoke, benchmarks where hot paths changed,
   and Codacy metric recheck.

Validation plan:

- `go test ./...`.
- Relevant interoperability scripts for touched surfaces.
- Benchmark smoke for touched hot paths.
- Codacy file metrics export after push.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: no expected update unless a new Codacy refactor
  workflow becomes a repeated rule.
- Specs: update if public Go behavior changes; otherwise record metric outcome
  in this SOW.
- End-user/operator docs: update only if APIs or CLI help change.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: complete this SOW after implementation, review, validation,
  and remote Codacy evidence.
- SOW-status.md: update on activation and close.

Open-source reference evidence:

- None checked yet. This SOW is pending and blocked on user priority.

Open decisions:

1. Whether to refactor Go metric debt before Netdata integration or defer it
   until after integration performance gates.
2. Which Go file group to tackle first.

## Implications And Decisions

Pending user decision.

## Plan

1. Decide priority and target file group.
2. Refactor one coherent group.
3. Validate behavior, performance, and Codacy metric movement.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Run the approved reviewer pool after the complete SOW implementation and
  local validation.

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

- If refactor risk exceeds metric value, record evidence and ask the user before
  continuing.

## Execution Log

Pending.

## Validation

Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

Pending.
