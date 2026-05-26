# SOW-0034 - File-Backed Journalctl Query Parity

## Status

Status: open

Sub-state: Split from SOW-0022 Gap 6. Ready for implementation after activation.

## Requirements

### Purpose

Complete file-backed journalctl query behavior across Rust, Go, Node.js, and Python without implementing daemon-only journalctl operations.

### User Request

Implement journalctl rewrites in all languages with file-backed/query behavior, including repeated same-key OR matching and `+` disjunction semantics.

### Assistant Understanding

Facts:

- Directory traversal, `--list-boots`, repeated same-field OR matches, `+` disjunction, `--verify`, and `--verify-key` have been implemented by earlier SOWs.
- Remaining query/follow gaps are `--follow`, `--boot`, `--since`, and `--until`.
- Daemon-only operations remain out of scope and must stay unsupported.

Inferences:

- Go is currently riskier than the others for `--boot`, `--since`, and `--until` because it parses some flags but appears to ignore them instead of failing or implementing them.

Unknowns:

- Exact stock `--boot` semantics to replicate for file-backed fixtures with synthetic boot IDs must be verified against systemd during implementation.

### Acceptance Criteria

- Rust, Go, Node.js, and Python file-backed journalctl rewrites support `--follow`, `--boot`, `--since`, and `--until` for repository-supported file/directory inputs.
- Existing daemon-only operations remain controlled unsupported behavior.
- Conformance tests compare output against stock journalctl for representative file and directory fixtures.
- Follow-mode tests prove live appended entries are emitted in order and do not require closing the writer.

## Analysis

Sources checked:

- `SOW-0022-20260525-compatibility-test-gap-audit.md`
- `SOW-0019-20260524-forward-secure-sealing.md`
- `SOW-0020-20260524-directory-traversal-parity.md`
- `SOW-0027-20260526-netdata-reader-api-and-jf-facade.md`
- `product-scope.md`
- Rust, Go, Node.js, and Python journalctl command sources
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`

Current state:

- Rust rejects `--follow` and does not expose `--boot`, `--since`, or `--until` in the clap struct.
- Go rejects `--follow` but defines `--boot`, `--since`, and `--until` in ignored flag variables.
- Node.js and Python reject `--follow`, `--boot`, `--since`, and `--until`.
- File-backed verification and directory traversal are no longer the main gap.

Risks:

- `--follow` has live concurrency behavior and must not be reduced to a closed-file loop.
- Time parsing and boot filtering can diverge subtly from stock journalctl.
- Silently ignored flags are worse than controlled unsupported behavior because users get wrong output.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The journalctl rewrites now cover core file-backed reading and verification, but still lack several query options that stock journalctl provides for file/directory inputs.

Evidence reviewed:

- `go/cmd/journalctl/main.go:32-49`
- `go/cmd/journalctl/main.go:66-75`
- `rust/src/cmd/journalctl/main.rs:24-49`
- `rust/src/cmd/journalctl/main.rs:81-92`
- `node/cmd/journalctl/index.js:20-72`
- `python/cmd/journalctl.py:146-181`
- `product-scope.md` journalctl target
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `man/journalctl.xml`

Affected contracts and surfaces:

- Journalctl command-line behavior in all four languages.
- Reader facade seek/filter behavior if needed to implement efficiently.
- Live follow harnesses.
- CLI docs/help text.

Existing patterns to reuse:

- Existing file-backed journalctl tests.
- Existing reader facade seek head/tail/realtime/cursor APIs.
- Existing live matrix writer/reader ordering checks.

Risk and blast radius:

- Medium CLI behavior risk, high user-visible correctness risk for ignored flags.

Sensitive data handling plan:

- Use synthetic fixtures only.

Implementation plan:

1. Build stock journalctl comparison fixtures for multiple boot IDs and time ranges.
2. Implement `--since` and `--until` parsing/filtering consistently.
3. Implement `--boot` for file-backed fixtures according to stock behavior.
4. Implement `--follow` using active-file polling/follow semantics, not daemon APIs.
5. Keep daemon-only operations unsupported.
6. Update CLI help/docs as needed.

Validation plan:

- Run conformance journalctl cases for all four languages.
- Run stock output comparison for file and directory fixtures.
- Run live follow tests against active SDK writers.
- Run read-only reviewers after implementation.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: no expected update unless a new journalctl parity gate is introduced.
- Specs: update `product-scope.md` with newly supported options.
- End-user/operator docs: update CLI README/help docs if present.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: activate only this SOW when implementing.
- SOW-status.md: update on activation and completion.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  - `man/journalctl.xml`
  - `src/libsystemd/sd-journal/sd-journal.c`

Open decisions:

- None. Daemon-only operations remain out of scope.

## Implications And Decisions

- No user decision is required before implementation unless stock file-backed behavior proves impossible to represent without daemon state.

## Plan

1. Add stock comparison tests.
2. Implement query/follow options.
3. Validate all languages.
4. Review as one batch.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current routing decision.

Reviewers:

- Read-only reviewers from the active pool after implementation: minimax, kimi, qwen, glm. Mimo is skipped.

Failure handling:

- Record any stock parity exception with evidence before changing scope.

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
