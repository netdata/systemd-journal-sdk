# SOW-0121 - File-Backed Journalctl Full Parity And Ship Decision

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: opened after user approval on 2026-06-21; pending implementation.
This SOW is release-relevant for any non-Linux package that claims an official
portable `journalctl` command.

## Requirements

### Purpose

Provide a portable non-Linux `journalctl` command that behaves like official
systemd `journalctl` for offline/file-backed journal files and directories,
without pretending to implement Linux daemon, namespace, machine, or host-local
journal services that do not exist on non-Linux systems.

### User Request

The user said non-Linux targets need an official `journalctl` command in full
parity with the official command, proposed implementing it in both Rust and Go,
then deciding which one to ship. After the current-state check showed the
existing commands are file-backed subsets, the user agreed to create focused
work for full offline/file-backed parity.

### Assistant Understanding

Facts:

- Project compatibility baseline is `systemd/systemd` tag `v260.1`.
- The current product scope already has file-backed Rust and Go `journalctl`
  rewrites, but only for a limited query surface.
- Current Go command supports `--file`, `--directory`, default/json/export
  output, `--list-boots`, `--fields`, `-F/--field`, `--head`, `--tail`,
  `--follow`, `--no-tail`, `-b/--boot`, `-S/--since`, `-U/--until`,
  `--verify`, `--verify-only`, `--verify-key`, and unsupported placeholders
  for `--sync`, `--flush`, `--rotate`, and `--relinquish-var`.
- Current Rust command supports the same practical file-backed subset.
- Official systemd `journalctl` v260.1 has a much wider source, filter,
  output, FSS, and command surface.
- Daemon-only operations remain outside the SDK contract and must not be
  implemented by invoking systemd, journald, host journals, host identity, or
  platform-specific host services.

Inferences:

- Full non-Linux parity must mean full offline/file-backed parity plus exact
  unsupported handling for daemon/host-only features.
- Implementing both Rust and Go first is the right way to preserve optionality:
  ship choice can be based on parity completeness, binary size, speed,
  dependency footprint, maintainability, and cross-platform reliability.
- This SOW is larger than the existing subset work because it touches CLI
  parsing, output formatting, filtering semantics, cursor semantics, validation
  matrices, docs, specs, and packaging decisions.

Unknowns:

- Which official v260.1 options should be classified as portable file-backed,
  accepted no-op, explicit unsupported, or not applicable.
- Whether Rust or Go will be the better shipped command after parity work.
- Whether both language CLIs should remain product artifacts after a ship
  decision, or whether one should become test/reference-only.

### Acceptance Criteria

- A committed v260.1 `journalctl` parity matrix classifies every official
  option/action as supported, unsupported with exact behavior, not applicable,
  or deferred with user approval.
- The portable Rust and Go command-line parsers recognize 100% of the official
  systemd v260.1 `journalctl` option/action surface, even when the feature is
  unsupported in non-daemon mode.
- Rust and Go implement all approved offline/file-backed behavior from the
  parity matrix.
- Unsupported daemon/host-only options fail intentionally and consistently
  without touching host journal state or invoking systemd services.
- Shared interoperability tests compare stock systemd `journalctl` against Rust
  and Go for file and directory inputs across query, filter, output, cursor,
  tail/head/reverse, follow, verification, header, field, boot, and error
  behavior.
- Cross-platform validation covers Linux, Windows, macOS, and FreeBSD for the
  portable command surface, with stock systemd oracle checks generated on Linux
  from repository-local fixtures only.
- A final ship recommendation records whether Rust, Go, or both should be
  shipped, with evidence.

## Analysis

Sources checked:

- `.agents/sow/specs/product-scope.md`.
- `rust/src/cmd/journalctl/main.rs`.
- `go/cmd/journalctl/main.go`.
- `tests/interoperability/run_journalctl_query_matrix.py`.
- `tests/interoperability/run_directory_matrix.py`.
- `tests/interoperability/run_verify_matrix.py`.
- `systemd/systemd @ c0a5a2516d28` (`v260.1`), especially
  `src/journal/journalctl.c`, `src/journal/journalctl.h`,
  `src/shared/output-mode.h`, and `src/shared/output-mode.c`.

Current state:

- Product scope currently promises file-backed Go `journalctl` behavior for
  `--file`, `--directory`, text/json/export output, field listing, boot
  listing, `--since`/`--until`, `--boot`, `--follow`, repeated same-field OR,
  and `+` disjunction.
- Product scope makes the same promise for Rust.
- The current Rust and Go commands are useful file-backed subsets, not full
  official `journalctl` parity.
- Existing interoperability tests cover the current subset, not every official
  option/action.

Risks:

- Over-scoping to Linux daemon parity would violate the project purity and
  non-Linux purpose.
- CLI option compatibility is deceptively broad: matching option parsing is not
  enough; output formatting, timestamps, cursor state, exit codes, stderr text,
  and match semantics all matter.
- Implementing both languages increases short-term work, but reduces ship-risk
  by preserving evidence-based choice.
- Cursor-file behavior writes caller-provided files. That is a legitimate CLI
  behavior but needs careful tests and documentation because most current SDK
  paths are reader-only.

## Pre-Implementation Gate

Status: needs-user-decision-before-activation

Problem / root-cause model:

- Non-Linux packages need a portable `journalctl` command. Current Rust and Go
  implementations are intentionally limited file-backed subsets. Official
  systemd `journalctl` v260.1 exposes a much larger surface, so a portable
  command cannot honestly be called full parity until every official option and
  action is classified and the approved offline/file-backed surface is
  implemented and tested.

Evidence reviewed:

- `.agents/sow/specs/product-scope.md`: current Rust and Go file-backed
  `journalctl` target.
- `go/cmd/journalctl/main.go`: current Go CLI flags and dispatch.
- `rust/src/cmd/journalctl/main.rs`: current Rust CLI flags and dispatch.
- `tests/interoperability/run_journalctl_query_matrix.py`: current query and
  follow parity coverage.
- `tests/interoperability/run_directory_matrix.py`: current directory coverage.
- `tests/interoperability/run_verify_matrix.py`: current verify coverage.
- `systemd/systemd @ c0a5a2516d28`
  - `src/journal/journalctl.c:241`: official v260.1 help and option surface.
  - `src/journal/journalctl.c:420`: official v260.1 long option table.
  - `src/journal/journalctl.c:1085`: official v260.1 option interaction
    validation.
  - `src/journal/journalctl.h:7`: official v260.1 action enum.
  - `src/shared/output-mode.h:6`: official output mode enum.
  - `src/shared/output-mode.c:26`: official output mode names.

Affected contracts and surfaces:

- Rust `journalctl` CLI under `rust/src/cmd/journalctl/`.
- Go `journalctl` CLI under `go/cmd/journalctl/`.
- Rust and Go reader facade APIs if missing operations are needed by the CLI.
- Shared interoperability tests under `tests/interoperability/`.
- Product scope spec and README documentation.
- Release/package decision for non-Linux `journalctl`.

Existing patterns to reuse:

- Current Rust and Go file-backed command structure.
- Current shared query, directory, verify, mixed-directory, compression, compact,
  and live interoperability harnesses.
- Existing `SdJournal*`/`sd_journal_*` facade layers for match trees,
  enumeration, unique values, cursors, output processing, and verification.
- Current repository-local fixture generation under `.local/interoperability/`.

Risk and blast radius:

- High user-facing compatibility risk: CLI output, option parsing, exit codes,
  and error text are visible behavior.
- Medium implementation risk in both languages because missing output modes and
  cursor semantics may require reader/facade extensions.
- Medium performance risk if tail/reverse/cursor behavior falls back to row
  scans where indexes or offset arrays should be used.
- Security/purity risk if host-only options accidentally inspect the local
  system or invoke external programs. This must be forbidden.
- Low data-loss risk for journal files if the implementation remains read-only;
  cursor-file support may write only the explicitly supplied cursor file.

Sensitive data handling plan:

- Use synthetic repository-local journals and sanitized fixtures.
- Do not read live host journals.
- Do not write raw host/customer/user data to SOWs, specs, docs, logs, or
  committed fixtures.
- `.local/` may contain generated test outputs; durable artifacts may include
  only sanitized counts, paths, option names, and summarized results.

Implementation plan:

1. Build a committed v260.1 option/action parity matrix from official systemd
   source and manpage.
2. Classify every option/action into:
   - portable file/directory behavior to implement;
   - explicit unsupported daemon/host-only behavior;
   - accepted no-op compatibility behavior;
   - separate user decision required.
3. Extend shared parity tests before or alongside implementation.
4. Implement missing Rust behavior.
5. Implement missing Go behavior.
6. Run Linux stock-oracle parity tests for both commands.
7. Run non-Linux portability tests on Windows, macOS, and FreeBSD after user
   approval for those systems.
8. Record ship recommendation and update release docs/specs.

Validation plan:

- `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir .local/cargo-target`.
- `cd go && GOCACHE="$PWD/.local/go-build" GOMODCACHE="$PWD/.local/go-mod-cache" go test ./cmd/journalctl ./journal`.
- `python3 tests/interoperability/run_journalctl_query_matrix.py`.
- `python3 tests/interoperability/run_directory_matrix.py`.
- `python3 tests/interoperability/run_verify_matrix.py`.
- Add and run new parity matrix coverage for official v260.1 option/action
  classification.
- Run focused cross-platform command tests on Windows, macOS, and FreeBSD after
  user approval.
- Run external reviewer pool after local implementation and validation.

Artifact impact plan:

- AGENTS.md: likely unaffected unless the project-wide `journalctl` target
  changes beyond this SOW.
- Runtime project skills: update `project-journal-compatibility` if this SOW
  establishes a durable parity workflow future agents must follow.
- Specs: update `.agents/sow/specs/product-scope.md` with the final supported
  command contract and ship decision.
- End-user/operator docs: update README and Rust/Go README command docs.
- End-user/operator skills: likely unaffected; record evidence if none exist.
- SOW lifecycle: keep this SOW pending until explicitly activated; complete only
  after implementation, validation, reviews, spec/docs updates, and ship
  recommendation.
- SOW-status.md: update on create, activation, and close.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28` (`v260.1`)
  - `src/journal/journalctl.c:241`
  - `src/journal/journalctl.c:420`
  - `src/journal/journalctl.c:1085`
  - `src/journal/journalctl.h:7`
  - `src/shared/output-mode.h:6`
  - `src/shared/output-mode.c:26`

Open decisions:

1. Confirm activation timing relative to SOW-0048, SOW-0049, SOW-0050, and
   SOW-0066.
2. Decide any disputed option/action classifications after the parity matrix is
   drafted.
3. Decide the shipped command implementation after Rust and Go evidence exists.

## Implications And Decisions

User decision on 2026-06-21:

- Create focused SOW work for a non-Linux official `journalctl` command.
- Scope the target as full official offline/file-backed parity, not Linux
  daemon/system parity.
- Require 100% official v260.1 command-line surface recognition: daemon-only
  and host-only options/actions must be parsed and classified, then rejected
  with intentional portable-mode unsupported behavior rather than being absent
  from the CLI.
- Implement or complete both Rust and Go commands first.
- Decide which command to ship only after parity, portability, performance,
  size, dependency, and maintainability evidence exists.

Implications:

- This is not SOW-0097 or SOW-0098 cleanup; it is product/release-relevant
  compatibility work.
- Existing subset commands are not enough for an "official portable journalctl"
  claim.
- Daemon-only operations must remain unsupported unless the user explicitly
  changes the project purity and non-Linux target, which is not part of this
  decision.

## Plan

1. Draft v260.1 parity matrix and option classification.
2. Review classification with the user only for real product decisions.
3. Extend shared tests to cover classified file-backed behavior and explicit
   unsupported behavior.
4. Implement Rust parity gaps.
5. Implement Go parity gaps.
6. Validate both commands against stock systemd on Linux and portable fixtures
   on non-Linux targets.
7. Run reviewers.
8. Recommend ship command.

## Delegation Plan

Implementer:

- Pending user activation. Default project routing applies when activated:
  implementation delegated to `llm-netdata-cloud/minimax-m3-coder` unless the
  user changes routing.

Reviewers:

- Use the approved reviewer pool after complete local implementation and
  validation. The implementer model must not review its own work.

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

- If the parity matrix shows this SOW is too large, split into phase SOWs before
  implementation.
- If Rust or Go cannot reach parity without violating runtime purity or adding
  unacceptable dependencies, record evidence and ask the user before changing
  ship criteria.
- If cross-platform validation needs Windows, macOS, or FreeBSD access, ask the
  user before using those systems.

## Execution Log

### 2026-06-21

- Opened SOW after user accepted the recommendation to create focused
  file-backed `journalctl` full-parity work.
- Recorded current-state evidence and official systemd v260.1 source evidence.
- No source, tests, specs, or docs were changed by this SOW creation.

## Validation

Acceptance criteria evidence:

- Pending implementation.

Tests or equivalent validation:

- SOW creation only; no product tests required yet.

Real-use evidence:

- Pending implementation.

Reviewer findings:

- Pending implementation.

Same-failure scan:

- Current Rust and Go command surfaces were checked before SOW creation.

Sensitive data gate:

- Durable artifacts contain only option names, file paths, commit IDs, and
  summarized evidence. No raw sensitive data was needed.

Artifact maintenance gate:

- AGENTS.md: no update needed for SOW creation.
- Runtime project skills: no update needed for SOW creation.
- Specs: no product behavior changed yet; spec update is required when this SOW
  implements or changes the command contract.
- End-user/operator docs: no product behavior changed yet; docs update is
  required when this SOW implements or changes command behavior.
- End-user/operator skills: no affected output/reference skills identified for
  SOW creation.
- SOW lifecycle: new open SOW created under `.agents/sow/pending/`.
- SOW-status.md: updated with new pending SOW.

Specs update:

- Pending implementation.

Project skills update:

- Pending implementation; update if a durable parity workflow is established.

End-user/operator docs update:

- Pending implementation.

End-user/operator skills update:

- No affected output/reference skills identified yet.

Lessons:

- None yet.

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

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
