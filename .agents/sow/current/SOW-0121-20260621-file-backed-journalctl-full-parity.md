# SOW-0121 - File-Backed Journalctl Full Parity And Ship Decision

## Status

Status: in-progress

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: activated by user goal on 2026-06-21; implementation in progress.
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

Status: satisfied for activation on 2026-06-21.

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

1. Activation timing: resolved by user goal on 2026-06-21. SOW-0121 is active
   before SOW-0048, SOW-0049, SOW-0050, and SOW-0066 because a portable
   official `journalctl` command is release-relevant compatibility work.
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
- Activate SOW-0121 and complete everything that can be done toward full
  file-backed operational parity, full command-line understanding, and proper
  unsupported messages for impossible daemon/host-only features.

User routing decision on 2026-06-21:

- The user explicitly requested direct implementation by the project manager for
  this SOW and explicitly requested no delegated implementation.
- Implementation for SOW-0121 will be performed locally in this session instead
  of using the default external implementer routing.
- External reviewers remain planned after complete local implementation and
  validation, following the whole-SOW review cadence.

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

- Activated by user goal on 2026-06-21. The user changed the default routing on
  2026-06-21 and requested direct local implementation by the project manager,
  with no delegated implementation for this SOW.

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
- Activated SOW after user requested completion of SOW-0121 with full
  file-backed operational parity, full command-line understanding, and proper
  unsupported behavior for daemon/host-only features.
- Added `.agents/sow/specs/journalctl-v260-parity-matrix.md` as the committed
  implementation contract for official systemd v260.1 command-line surface
  recognition, file-backed behavior, portable no-ops, and unsupported
  daemon/host-only behavior.
- Ran mechanical coverage checks against `systemd/systemd @ c0a5a2516d28`:
  71 official long options covered, all official short options covered, all 16
  official output modes covered, and all 20 official actions covered.
- Delegated first implementation chunk to the configured implementer model
  `llm-netdata-cloud/minimax-m3-coder`. The implementer run reached its
  timeout after adding the initial parser manifest/harness and Rust/Go parser
  recognition changes.
- Integrated and corrected the chunk locally:
  - removed a hardcoded workstation path from the manifest checker;
  - fixed the Rust parser unit test so it actually passes `journalctl
    --option[=value]` to clap instead of treating option names as the program
    name;
  - aligned Rust `--synchronize-on-exit` with the official v260.1
    `required_argument` definition;
  - added shared parser parity checks for all 71 official long options, all 16
    output modes, and all six parser interaction rules;
  - added Rust and Go recognition for the full official v260.1 option surface;
  - added intentional portable-mode unsupported messages for daemon/host-only
    actions;
  - implemented low-risk file-backed behavior for `--reverse`, `--show-cursor`,
    and `--lines`/`--lines=N`/`--lines=+N` in Rust and Go;
  - extended the journalctl query matrix to compare stock systemd, Rust, and Go
    for reverse, lines, oldest-lines, no-value lines, and cursor printing.
- Fixed a real Rust optional-argument parity bug found by the expanded matrix:
  `--lines TEST_ID=...` must not consume the match as the optional lines
  argument. Rust now consumes the next token only when it looks like `all`, `N`,
  or `+N`.
- User changed implementation routing and explicitly requested direct local
  implementation for this SOW with no delegated implementation.
- Implemented the next file-backed filter chunk directly in Rust and Go:
  - `--identifier=` / `-t` as `SYSLOG_IDENTIFIER=` alternatives with repeated
    values ORed;
  - `--priority=` / `-p` numeric, named, and `from..to` expansion;
  - `--facility=` numeric/named comma lists and `help`;
  - `--grep=` / `-g` with v260.1 case auto-detection and
    `--case-sensitive[=BOOL]`;
  - `--dmesg` / `-k` as `_TRANSPORT=kernel`;
  - `--this-boot` as the file-backed current-boot alias;
  - portable unsupported handling for positional path match arguments, without
    inspecting host filesystem metadata.
- Corrected the parity matrix for `--exclude-identifier=` / `-T`: verified
  `systemd/systemd @ c0a5a2516d28` stores `exclude_syslog_identifiers` in
  `src/journal/journalctl-filter.c`, but no v260.1 file-backed show path uses
  that set. Stock `journalctl --file ... --exclude-identifier=...` output is
  unchanged, so the portable commands now preserve that no-op behavior for
  official v260.1 parity.
- Implemented the next file-backed cursor chunk directly in Rust and Go:
  - `--cursor=` / `-c` starts at the supplied cursor and includes that entry
    when it matches filters;
  - `--after-cursor=` starts after the supplied cursor only when the current
    filtered entry still tests equal to that cursor, preserving the v260.1
    filtered-skip rule from `src/journal/journalctl-show.c`;
  - `--cursor-file=` reads the first line if present, starts after it, and
    atomically writes the final emitted cursor with the stock trailing newline;
  - live follow snapshots honor cursor starts;
  - Rust and Go facade cursor parsing now accepts official systemd cursor
    strings (`s=`, `i=`, `b=`, `m=`, `t=`, `x=`) while preserving older SDK
    cursor input compatibility (`s=`, `j=`, `c=`, `n=`);
  - Rust and Go cursor emitters now produce official systemd cursor strings for
    facade `get_cursor()`, JSON/export metadata, and file-backed journalctl
    cursor output.

## Validation

Acceptance criteria evidence:

- Parity matrix created at `.agents/sow/specs/journalctl-v260-parity-matrix.md`.
- Mechanical matrix coverage against systemd v260.1:
  - 71 official long options found in `src/journal/journalctl.c` and all are
    present in the matrix.
  - Official short option set `DFIMNSTUWabcefghiklmnopqrtux` is fully present in
    the matrix.
  - Official output modes `short`, `short-full`, `short-iso`,
    `short-iso-precise`, `short-precise`, `short-monotonic`, `short-delta`,
    `short-unix`, `verbose`, `export`, `json`, `json-pretty`, `json-sse`,
    `json-seq`, `cat`, and `with-unit` are fully present in the matrix.
  - All 20 `JournalctlAction` enum values are represented by matrix command or
    parser rows.
- Rust and Go implementation pending.
- Rust and Go full parser recognition is implemented and locally validated.
- Rust and Go file-backed behavior is partially advanced for `--reverse`,
  `--show-cursor`, `--lines` direction/default semantics, `--identifier`,
  `--priority`, `--facility`, `--grep`, `--case-sensitive`, `--dmesg`,
  `--this-boot`, `--cursor`, `--after-cursor`, `--cursor-file`, and portable
  path-match rejection.
- Remaining file-backed parity is still pending, including full short-family
  formatting, verbose/cat/with-unit/JSON variant exact framing,
  `--output-fields`, unit/user-unit and invocation filters, `--header`,
  `--disk-usage`, `--list-invocations`, `--new-id128`, `--setup-keys`, exact
  empty-result exit semantics, and directory vacuum actions where approved by
  the parity matrix.

Tests or equivalent validation:

- Implementation remains partial; product validation is recorded per completed
  chunk until the whole SOW can be reviewed and closed.
- Matrix coverage checked with read-only Python extraction from the v260.1
  upstream source and local matrix text.
- `python3 tests/parser-parity/check_v260_manifest.py`: passed; manifest
  matches systemd v260.1 official long options, short options, output modes,
  and action enum.
- `python3 tests/parser-parity/run_parser_parity.py --rust-bin
  .local/cargo-target/debug/journalctl`: passed; Rust `ok=93 skipped=0
  failed=0`, Go `ok=93 skipped=0 failed=0`. Rerun after the direct filter
  chunk stayed clean.
- `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir
  .local/cargo-target`: passed; 24 Rust `journalctl` tests passed.
- `cd go && GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test ./cmd/journalctl
  ./journal`: passed in local validation after the direct filter chunk.
- `python3 tests/interoperability/run_journalctl_query_matrix.py
  --skip-follow`: passed against stock `journalctl` from systemd 260
  `(260.1-2-manjaro)`: `PASS results=69 failures=0`, including the new
  reverse, lines, oldest-lines, no-value lines, and show-cursor cases.
- `python3 tests/interoperability/run_journalctl_query_matrix.py
  --skip-follow`: passed against stock `journalctl` from systemd 260
  `(260.1-2-manjaro)`: `PASS results=107 failures=0`, including identifier,
  priority, facility, grep/case-sensitive, dmesg, this-boot, and portable
  path-match unsupported cases.
- `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed
  against stock `journalctl` from systemd 260 `(260.1-2-manjaro)`: `PASS
  results=119 failures=0`, including live follow checks after the filter-path
  changes.
- `cargo test --manifest-path rust/Cargo.toml -p journalctl -p
  systemd-journal-sdk --target-dir .local/cargo-target`: passed after the
  cursor chunk; 24 Rust `journalctl` tests and 121 Rust SDK tests passed,
  including official systemd cursor parser coverage.
- `cd go && GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test ./cmd/journalctl
  ./journal ./adapter`: passed after the cursor chunk, including official
  systemd cursor parser coverage and cursor adapter coverage.
- `python3 tests/interoperability/run_journalctl_query_matrix.py
  --skip-follow`: passed against stock `journalctl` from systemd 260
  `(260.1-2-manjaro)`: `PASS results=122 failures=0`, including official
  `--cursor`, `--after-cursor`, filtered after-cursor skip, and
  `--cursor-file` byte-parity cases.
- `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed
  against stock `journalctl` from systemd 260 `(260.1-2-manjaro)`: `PASS
  results=134 failures=0`, including live follow checks after the cursor-path
  changes.
- `python3 tests/docs/check_wiki_docs.py`: passed after cursor documentation
  updates; validated 15 wiki markdown files.
- `CARGO_HOME="$PWD/.local/cargo-home"
  CARGO_TARGET_DIR="$PWD/.local/cargo-target"
  GOCACHE="$PWD/.local/go-build"
  GOMODCACHE="$PWD/.local/go-mod-cache"
  python3 tests/docs/verify_examples.py`: passed after cursor documentation
  updates; 31 of 31 verified examples passed.
- `python3 tests/interoperability/run_directory_matrix.py`: passed against
  stock `journalctl` from systemd 260 `(260.1-2-manjaro)`: `PASS checks=34`.
- `python3 tests/interoperability/run_verify_matrix.py`: passed against stock
  `journalctl` from systemd 260 `(260.1-2-manjaro)`: `PASS results=63
  failures=0`.
- `git diff --check`: passed.
- Sensitive string scan over changed durable artifacts and candidate code:
  passed after the cursor chunk; the only match was the existing sanitized
  sensitive-data policy text in this SOW.
- `.agents/sow/audit.sh`: passed after the direct filter chunk and again after
  the cursor chunk.

Real-use evidence:

- Partial implementation evidence exists through stock-systemd comparison
  matrices using repository-local fixtures only. The whole SOW remains
  in-progress.
- No external reviewer findings yet. Per project review cadence, external
  reviewers remain deferred until complete local implementation and validation
  for the whole SOW.

Reviewer findings:

- Pending implementation.

Same-failure scan:

- Current Rust and Go command surfaces were checked before SOW creation.

Sensitive data gate:

- Durable artifacts contain only option names, file paths, commit IDs, and
  summarized evidence. No raw sensitive data was needed.

Artifact maintenance gate:

- AGENTS.md: no update needed for this chunk; the routing exception is recorded
  in this SOW.
- Runtime project skills: no update needed for this chunk.
- Specs: `.agents/sow/specs/journalctl-v260-parity-matrix.md` was updated to
  correct `--exclude-identifier=` as a v260.1 file-backed no-op based on source
  and stock-command evidence. `.agents/sow/specs/product-scope.md` was updated
  to record that emitted cursor strings now use the official systemd cursor
  shape while seek/test still accept the older SDK cursor shape.
- End-user/operator docs: `rust/README.md`, `go/README.md`, `go/API.md`, and
  `docs/Reader-APIs.md` were updated for the cursor string format contract.
- End-user/operator skills: no affected output/reference skills identified for
  this chunk.
- SOW lifecycle: active SOW remains `in-progress` under `.agents/sow/current/`.
- SOW-status.md: no update needed for this non-terminal chunk.

Specs update:

- Added `.agents/sow/specs/journalctl-v260-parity-matrix.md` for the active
  SOW implementation contract. Updated `.agents/sow/specs/product-scope.md`
  for the shipped cursor string contract change. Additional product-scope
  updates remain pending final shipped behavior and ship recommendation.

Project skills update:

- Parser parity workflow added under `tests/parser-parity/`. Project skill
  update remains pending until the full SOW establishes the durable final
  workflow and ship contract.

End-user/operator docs update:

- Updated `rust/README.md`, `go/README.md`, `go/API.md`, and
  `docs/Reader-APIs.md` for the cursor string format contract. Final
  journalctl command documentation remains pending full implementation.

End-user/operator skills update:

- No affected output/reference skills identified yet.

Lessons:

- Parser tests must distinguish parsing, validation, and dispatch. A test that
  passes option names as the program name can create false confidence.
- Optional-argument CLI compatibility must be tested with a following match
  token. Stock `journalctl --lines FIELD=value` does not consume the match as a
  lines value.

Follow-up mapping:

- Remaining parity gaps are tracked inside this active SOW and must not be
  treated as deferred outside the SOW.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
