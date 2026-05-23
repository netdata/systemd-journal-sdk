# SOW-0011 - Live Concurrency Compatibility Gate

## Status

Status: open

Sub-state: pending as the next recommended compatibility hardening SOW. This SOW blocks production-compatible claims for any writer or reader until stock-reader and live cross-language concurrency evidence exists.

## Requirements

### Purpose

Make live one-writer/multiple-reader compatibility a confirmed and validated contract for every journal writer and reader.

### User Request

The user clarified that compatibility requires stock `journalctl` and libsystemd journal readers to work concurrently with repository writers, and requires repository readers to handle the corresponding live writer cases. This is a MUST for all writers and readers.

### Assistant Understanding

Facts:

- Closed-file `journalctl --verify` is not enough to claim compatibility.
- Every writer must be readable by stock `journalctl --file` while it is appending.
- Every writer must be readable by stock libsystemd reader APIs while it is appending.
- Every reader must safely read live files being appended by repository writers.
- Reader compatibility with stock systemd writers must be tested where the environment can provide that safely without violating repository-boundary rules.
- The existing Go writer has closed-file stock `journalctl` evidence and one temporary live smoke test, but it does not yet have committed production-grade live concurrency validation.

Inferences:

- This SOW should run before additional language writers are accepted, so every future implementation inherits the same compatibility gate.
- The shared conformance harness needs live-concurrency adapters, not only closed-file fixtures.
- The Go writer may need append-publication-order hardening before it can be called production-compatible.

Unknowns:

- The exact stock libsystemd reader helper shape must be designed during activation.
- A safe stock systemd writer source for repository reader tests may require a fixture generator, a systemd test helper, or an explicitly documented environment capability.

### Acceptance Criteria

- Shared live-concurrency harness exists and is committed.
- Harness records stock systemd version, helper commands, reader count, append count, duration, failure criteria, and logs.
- Go writer passes stock `journalctl --file` live-read tests while appending.
- Go writer passes stock libsystemd live-reader tests while appending.
- Go writer passes clean-close `journalctl --verify --file` after live-read stress.
- Go writer passes interruption/reopen live-read tests for the feature slice it claims.
- Harness can be reused by Rust, Node.js, Python, and the final cross-language matrix.
- Reader-side live test contract is defined for repository readers and stock writer evidence.
- Product scope, project compatibility skill, and pending implementation SOWs record that this gate is mandatory.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/specs/product-scope.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`
- `.agents/sow/done/SOW-0005-20260523-go-sdk-and-journalctl.md`
- `go/journal/writer.go`
- `go/journal/writer_test.go`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`

Current state:

- Go writer SOW is completed and committed.
- Go writer has closed-file stock `journalctl` tests.
- Go writer has no committed stock libsystemd live-reader test.
- Go writer has no committed live stress test with multiple stock readers while appending.
- Go reader does not exist yet.

Risks:

- A writer can pass closed-file verification but still expose inconsistent append publication windows to live stock readers.
- A reader can parse closed fixtures but fail on online journal state, tail metadata changes, file growth, or entry-array growth.
- Without this gate, later language ports can copy a subtly incompatible writer order or reader refresh model.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Compatibility must cover live operation because systemd journal files are designed for one writer and multiple concurrent readers.
- Current validation proves only a narrower closed-file subset for the Go writer.
- A reusable live-concurrency harness is needed before other writers and readers can be accepted as compatible.

Evidence reviewed:

- `.agents/sow/done/SOW-0005-20260523-go-sdk-and-journalctl.md`
- `go/journal/writer.go`
- `go/journal/writer_test.go`
- `.agents/sow/specs/product-scope.md`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced src/libsystemd/sd-journal/journal-file.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced src/libsystemd/sd-journal/sd-journal.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced src/libsystemd/sd-journal/journal-def.h`

Affected contracts and surfaces:

- Shared conformance harness.
- Go writer append publication behavior.
- Future Rust, Node.js, Python writer acceptance gates.
- Future Go, Rust, Node.js, Python reader acceptance gates.
- Product scope spec.
- Project compatibility skill.
- SOW status and phase ordering.

Existing patterns to reuse:

- Existing Go writer package tests.
- Existing systemd conformance manifest structure.
- Stock `journalctl --file` validation already used by Go writer tests.
- systemd live reader behavior and append ordering as compatibility authority.

Risk and blast radius:

- This SOW may expose real Go writer defects requiring code changes.
- Live stress tests can be flaky if they depend on timing alone; they need deterministic failure criteria and repeatable helper protocols.
- Direct stock libsystemd reader helpers may require build tooling in tests, but must not make SDK implementations link to libsystemd.
- Reader tests against a stock systemd writer may be environment-sensitive and must not write outside this repository except `/tmp`.

Sensitive data handling plan:

- Test journals use synthetic entries only.
- Test logs must not include host journal data, real service names from the workstation, credentials, bearer tokens, SNMP communities, customer data, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details.
- Stock writer evidence must use synthetic test data or generated fixtures, not host production journals.

Implementation plan:

1. Add a shared live-concurrency test harness under `tests/conformance/`.
2. Add a stock `journalctl --file` live-reader adapter that repeatedly reads or follows a file while a repository writer appends.
3. Add a stock libsystemd reader helper or adapter for live file-backed reading.
4. Add Go writer live stress tests with multiple stock readers, clean-close verify, interruption, and reopen.
5. Define the reader-side live contract for repository readers and stock writer evidence.
6. Repair Go writer append publication behavior if live tests expose failures.
7. Record the reusable gate in docs/specs/skills and pending language SOWs.

Validation plan:

- Go package tests pass.
- Shared live-concurrency harness passes against the Go writer.
- Stock `journalctl --file` live tests pass against Go writer.
- Stock libsystemd live-reader tests pass against Go writer.
- `journalctl --verify --file` passes after live stress and after tested interruption/reopen.
- `CGO_ENABLED=0 go test ./...` confirms SDK remains pure Go.
- SOW audit and sensitive-data audit pass.
- External reviewers inspect the whole SOW and implementation until production-grade.

Artifact impact plan:

- AGENTS.md: likely unchanged unless SOW lifecycle rules need tightening.
- Runtime project skills: update `.agents/skills/project-journal-compatibility/SKILL.md`.
- Specs: update `.agents/sow/specs/product-scope.md`.
- End-user/operator docs: update only if Go writer docs need a compatibility warning or guarantee.
- End-user/operator skills: none expected.
- SOW lifecycle: this SOW should be activated before accepting more writers as production-compatible.
- SOW-status.md: update next SOW recommendation to this SOW.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced src/libsystemd/sd-journal/journal-file.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced src/libsystemd/sd-journal/sd-journal.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced src/libsystemd/sd-journal/journal-def.h`

Open decisions:

- No user decision is currently needed. The user has made the live concurrency requirement mandatory.

## Implications And Decisions

1. Live concurrency compatibility is mandatory
   - Current state: resolved by user clarification on 2026-05-23.
   - Selection: no writer or reader can be called production-compatible without live one-writer/multiple-reader validation.
   - Implication: closed-file verification alone is insufficient.
   - Risk: this may delay language ports, but skipping it would make the SDKs incompatible with real systemd journal operation.

2. Go writer production claim
   - Current state: the Go writer is implemented for the first feature slice, but full live compatibility is not proven.
   - Selection: treat the Go writer as not yet production-compatible for concurrent stock-reader operation until this SOW passes.
   - Implication: the Netdata plugin use case should wait for this SOW before relying on concurrent stock readers.
   - Risk: live tests may require writer changes and another external review cycle.

## Plan

1. Activate this SOW before the next language implementation SOW.
2. Implement the live-concurrency harness and stock reader adapters.
3. Run the harness against the Go writer and fix any compatibility gaps.
4. Update all future language SOWs to use this harness as a required acceptance gate.
5. Review with external agents until no blocking findings remain.

## Delegation Plan

Implementer:

- Preferred implementer is `llm-netdata-cloud/minimax-m2.7-coder`, unless direct implementation is faster for harness mechanics; if direct implementation is used, Minimax must be switched to reviewer.

Reviewers:

- Use at least four reviewers from `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`.

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

- Record implementer or reviewer model failure in this SOW.
- Substitute only from the approved model list.
- Rerun full-scope review after fixes.
- Do not close if `.agents/sow/audit.sh` fails.

## Execution Log

### 2026-05-23

- Created this SOW after the user clarified live concurrency is mandatory for compatibility.

## Validation

Pending activation and implementation.

## Outcome

Pending.

## Lessons Extracted

Pending activation.

## Followup

None yet.

## Regression Log

None yet.
