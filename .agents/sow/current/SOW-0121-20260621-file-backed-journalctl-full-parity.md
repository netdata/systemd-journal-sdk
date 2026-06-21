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
- Implemented the next file-backed unit filter chunk directly in Rust and Go:
  - `--unit=` / `-u` now adds the official v260.1 file-backed system-unit
    match groups for `_SYSTEMD_UNIT`, PID 1 `UNIT` messages under
    `_SYSTEMD_CGROUP=/init.scope`, root-owned `OBJECT_SYSTEMD_UNIT` messages,
    coredump `COREDUMP_UNIT` messages with `MESSAGE_ID=fc2e...`, and
    `_SYSTEMD_SLICE` for `.slice` units;
  - `--user-unit=` now adds the official v260.1 user-unit match groups for
    `_SYSTEMD_USER_UNIT`, `USER_UNIT`, `OBJECT_SYSTEMD_USER_UNIT`,
    `COREDUMP_USER_UNIT`, and `_SYSTEMD_USER_SLICE`;
  - repeated exact unit arguments and glob unit patterns are expanded from the
    existing unit-related FIELD/DATA indexes and ORed like stock
    `journalctl`;
  - unit names without a recognized unit suffix are mangled to `.service`,
    matching the tested stock behavior for `-u alpha`;
  - `--user-unit` uses the current process UID in the CLI compatibility layer
    on Unix hosts, matching stock `journalctl`; non-Unix builds compile with a
    no-host-UID fallback that omits UID restrictions because stock
    `journalctl` has no non-Unix UID oracle.

Official source evidence for the unit-filter chunk:

- `systemd/systemd @ c0a5a2516d28`
  - `src/journal/journalctl-filter.c:75`: `journal_add_unit_matches()` expands
    exact and glob system/user units, adds each group as a disjunction, and
    finishes with a conjunction.
  - `src/journal/journalctl-filter.c:184`: file/directory inputs clear
    `MATCH_UNIT_COREDUMP_UID`, so file-backed system coredump unit matching
    does not depend on the host `systemd-coredump` UID.
  - `src/shared/logs-show.c:1726`: `add_matches_for_unit_full()` match groups
    for `_SYSTEMD_UNIT`, `UNIT`, `OBJECT_SYSTEMD_UNIT`, `COREDUMP_UNIT`, and
    `_SYSTEMD_SLICE`.
  - `src/shared/logs-show.c:1767`: `add_matches_for_user_unit_full()` match
    groups for `_SYSTEMD_USER_UNIT`, `USER_UNIT`,
    `OBJECT_SYSTEMD_USER_UNIT`, `COREDUMP_USER_UNIT`, and
    `_SYSTEMD_USER_SLICE`, with the current UID default.
  - `src/journal/journalctl-util.h:8`: official possible-unit field lists for
    system and user glob expansion.
- Updated `.agents/sow/specs/product-scope.md`, `docs/Journalctl-CLI.md`,
  `go/README.md`, and `rust/README.md` so the documented file-backed
  `journalctl` contract includes the implemented cursor, identifier, priority,
  facility, grep, dmesg, and system/user unit filter behavior.
- Added portable utility/action parity for `--new-id128` and explicit-input
  `--disk-usage` in both Rust and Go:
  - `--new-id128` generates v4 ID128 values and prints the stock v260.1 string,
    UUID, `SD_ID128_MAKE()`, and Python constant blocks.
  - `--disk-usage` remains unsupported without explicit `--file` or
    `--directory`, because that would require host journal discovery.
  - With explicit input, `--disk-usage` sums allocated filesystem blocks for
    selected journal files and formats the size with systemd's IEC
    `FORMAT_BYTES()` convention.
  - Go uses Unix allocated block counts on Unix builds and logical size on
    non-Unix builds where stock `journalctl` has no oracle.
  - Rust uses Unix allocated block counts on Unix builds and logical size on
    non-Unix builds.
- Updated `.agents/sow/specs/product-scope.md`, `docs/Journalctl-CLI.md`,
  `go/README.md`, and `rust/README.md` so the documented file-backed
  `journalctl` contract includes `--new-id128` and explicit-input
  `--disk-usage`.
- Implemented the file-backed output-mode parity chunk directly in Rust and
  Go:
  - the short-family modes `short`, `short-full`, `short-iso`,
    `short-iso-precise`, `short-precise`, `short-monotonic`, `short-delta`,
    and `short-unix`;
  - `with-unit`, `cat`, `verbose`, `export`, `json`, `json-pretty`,
    `json-sse`, and `json-seq`;
  - `--output-fields` projection for `verbose`, `export`, JSON modes, and
    `cat`, with stock metadata retention for JSON/export modes;
  - Rust local timezone-name formatting for stock-style `short-full`,
    `with-unit`, and `verbose` output on Unix hosts, with UTC and non-Unix
    fallback behavior kept explicit;
  - deterministic selected-field order in Rust/Go for `cat --output-fields`,
    while the interoperability oracle accepts stock's Set iteration order for
    that one mode.

Official source evidence for the output-mode chunk:

- `systemd/systemd @ c0a5a2516d28`
  - `src/journal/journalctl.c:546`: parses `--output` and maps official output
    mode strings.
  - `src/journal/journalctl.c:1044`: parses `--output-fields` as a comma list
    and stores it in `arg_output_fields`.
  - `src/shared/logs-show.c:783`: `output_verbose()` prints timestamp/cursor
    header and filters row data through `field_set_test()`.
  - `src/shared/logs-show.c:1410`: `output_cat()` prints `MESSAGE` by default
    and iterates the `output_fields` Set when fields are supplied, so field
    order is not a stable byte contract for stock `cat --output-fields`.
- Updated `.agents/sow/specs/product-scope.md`, `docs/Journalctl-CLI.md`,
  `go/README.md`, and `rust/README.md` so the documented file-backed
  `journalctl` contract includes the full output-mode family and
  `--output-fields`.
- Implemented the file-backed header, invocation, and short-label parity chunk
  directly in Rust and Go:
  - `--header` prints stock-style journal header fields from explicit file and
    directory inputs, using reader-exposed header metadata in both languages;
  - `--invocation=` and `-I` resolve explicit invocation IDs and unit-context
    invocation offsets from explicit file/directory input, with
    `--invocation=all` preserving stock no-filter behavior;
  - invocation filters add the official OR field set for
    `_SYSTEMD_INVOCATION_ID`, `OBJECT_SYSTEMD_INVOCATION_ID`, `INVOCATION_ID`,
    and `USER_INVOCATION_ID`;
  - `--list-invocations` lists invocation IDs for the selected unit context,
    including stock `--lines` row selection, header suppression under
    `--quiet`, and stock quiet-mode index padding;
  - short-style output now renders stock-compatible hostname,
    identifier/unit, and PID labels from journal fields, with `--no-hostname`
    suppressing only the hostname component;
  - `--setup-keys` was reclassified from portable utility work to
    recognized-unsupported because the official v260.1 action checks
    `/var/log/journal`, reads host machine and boot IDs, and writes
    `/var/log/journal/<machine-id>/fss`.

Official source evidence for the header/invocation/label chunk:

- `systemd/systemd @ c0a5a2516d28`
  - `src/journal/journalctl-misc.c:22`: `action_print_header()` opens the
    journal and prints headers.
  - `src/libsystemd/sd-journal/sd-journal.c:3285`: header printing iterates
    journal files.
  - `src/libsystemd/sd-journal/journal-file.c:3888`: stock header field output
    shape.
  - `src/journal/journalctl-filter.c:426`: invocation filters replace the
    normal boot/unit filter path when an invocation descriptor is selected.
  - `src/shared/logs-show.c:1670`: invocation match fields.
  - `src/journal/journalctl-util.c:180`: unit-context validation for
    invocation offset/list actions.
  - `src/shared/logs-show.c:2271`: invocation/boot ID listing helper.
  - `src/shared/logs-show.c:552`: short output parses `_PID`.
  - `src/shared/logs-show.c:557`: short output parses `_HOSTNAME`.
  - `src/shared/logs-show.c:558`: short output parses `SYSLOG_PID`.
  - `src/shared/logs-show.c:625`: `OUTPUT_NO_HOSTNAME` suppresses hostname.
  - `src/journal/journalctl-authenticate.c:80`: `--setup-keys` checks
    `/var/log/journal`.
  - `src/journal/journalctl-authenticate.c:89`: `--setup-keys` reads the host
    machine ID.
  - `src/journal/journalctl-authenticate.c:93`: `--setup-keys` reads the host
    boot ID.
  - `src/journal/journalctl-authenticate.c:97`: `--setup-keys` writes the host
    FSS path under `/var/log/journal/<machine-id>/fss`.
- Updated `.agents/sow/specs/journalctl-v260-parity-matrix.md`,
  `tests/parser-parity/v260-manifest.py`,
  `tests/parser-parity/v260-manifest.json`,
  `.agents/sow/specs/product-scope.md`, `docs/Journalctl-CLI.md`,
  `go/README.md`, and `rust/README.md` so the documented file-backed
  `journalctl` contract includes header, invocation, stock short labels, and
  the corrected `--setup-keys` unsupported classification.
- Implemented explicit-directory vacuum maintenance in Rust and Go:
  - `--vacuum-size`, `--vacuum-files`, and `--vacuum-time` remain unsupported
    without explicit `--directory` input;
  - explicit-directory vacuum scans only direct regular files in that
    directory, matching the `journal_directory_vacuum()` contract;
  - only stock-recognized archived `.journal` and `.journal~` filenames are
    candidates for deletion;
  - active/current `.journal` files, non-matching `.journal`/`.journal~`
    files, unknown files, symlinks, and subdirectories are protected;
  - deletion order follows stock seqnum/realtime/filename ordering;
  - `--vacuum-files` follows the stock total-count rule: protected active files
    plus remaining archived candidates must be at or below the requested count;
  - empty recognized archived files are removed before applying normal
    retention constraints.

Official source evidence for the vacuum chunk:

- `systemd/systemd @ c0a5a2516d28`
  - `src/journal/journalctl-varlink.c:99`: `action_vacuum()` handles
    `ACTION_VACUUM` and `ACTION_ROTATE_AND_VACUUM`.
  - `src/journal/journalctl-varlink.c:110`: official action passes each
    selected directory path to `journal_directory_vacuum()`.
  - `src/libsystemd/sd-journal/journal-vacuum.c:35`: candidate sort prefers
    seqnum when both archived files share the same seqnum ID.
  - `src/libsystemd/sd-journal/journal-vacuum.c:129`: directory vacuum takes
    one explicit directory path plus size/file/time constraints.
  - `src/libsystemd/sd-journal/journal-vacuum.c:148`: all-zero constraints are
    a no-op.
  - `src/libsystemd/sd-journal/journal-vacuum.c:154`: vacuum opens only the
    supplied directory.
  - `src/libsystemd/sd-journal/journal-vacuum.c:167`: direct entries are
    statted without following symlinks.
  - `src/libsystemd/sd-journal/journal-vacuum.c:179`: `.journal` active files
    are protected unless the archived suffix shape is present and valid.
  - `src/libsystemd/sd-journal/journal-vacuum.c:217`: `.journal~` corrupted
    archive candidates use the separate realtime/tmp suffix shape.
  - `src/libsystemd/sd-journal/journal-vacuum.c:260`: empty archived files are
    always deleted.
  - `src/libsystemd/sd-journal/journal-vacuum.c:300`: `--vacuum-files`
    compares protected active files plus remaining candidates against the
    requested count.
- Updated `.agents/sow/specs/journalctl-v260-parity-matrix.md`,
  `.agents/sow/specs/product-scope.md`, `docs/Journalctl-CLI.md`,
  `go/README.md`, and `rust/README.md` so the documented file-backed
  `journalctl` contract includes explicit-directory vacuum behavior.

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
- Rust and Go implementation remains in progress.
- Rust and Go full parser recognition is implemented and locally validated.
- Rust and Go file-backed behavior is partially advanced for `--reverse`,
  `--show-cursor`, `--lines` direction/default semantics, `--identifier`,
  `--priority`, `--facility`, `--grep`, `--case-sensitive`, `--dmesg`,
  `--this-boot`, `--cursor`, `--after-cursor`, `--cursor-file`, `--unit`,
  `--user-unit`, `--invocation`, `-I`, `--list-invocations`, `--header`,
  stock short labels including `--no-hostname`, `--new-id128`, explicit-input
  `--disk-usage`, explicit-directory `--vacuum-size`/`--vacuum-files`/
  `--vacuum-time`, full output-mode rendering, `--output-fields`, and
  portable path-match rejection.
- Remaining file-backed parity is still pending, including exact empty-result
  exit semantics and final whole-SOW cross-platform/reviewer/ship-decision
  gates.

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
- `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir
  .local/cargo-target`: passed after the unit-filter chunk; 24 Rust
  `journalctl` tests passed.
- `cd go && GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test ./cmd/journalctl
  ./journal`: passed after the unit-filter chunk.
- `python3 tests/parser-parity/check_v260_manifest.py`: passed after the
  unit-filter chunk; manifest still matches the official systemd v260.1
  surface.
- `python3 tests/parser-parity/run_parser_parity.py --rust-bin
  .local/cargo-target/debug/journalctl`: passed after the unit-filter chunk;
  Rust `ok=93 skipped=0 failed=0`, Go `ok=93 skipped=0 failed=0`.
- `python3 tests/interoperability/run_journalctl_query_matrix.py
  --skip-follow`: passed against stock `journalctl` from systemd 260
  `(260.1-2-manjaro)` after the unit-filter chunk, including exact system
  units, short `-u` mangling, system unit glob expansion, PID 1 unit messages,
  root-owned object unit messages, coredump unit messages, slice unit messages,
  exact user units, user-unit glob expansion, user manager/object/coredump
  groups, and user slices.
- `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed
  against stock `journalctl` from systemd 260 `(260.1-2-manjaro)` after the
  unit-filter chunk, including the live follow checks.
- `GOOS=windows GOARCH=amd64 GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test -c -o
  ../.local/go-journalctl-windows.test.exe ./cmd/journalctl`: passed after the
  unit-filter chunk.
- `GOOS=darwin GOARCH=arm64 GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test -c -o
  ../.local/go-journalctl-darwin-arm64.test ./cmd/journalctl`: passed after
  the unit-filter chunk.
- `GOOS=freebsd GOARCH=amd64 GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test -c -o
  ../.local/go-journalctl-freebsd-amd64.test ./cmd/journalctl`: passed after
  the unit-filter chunk.
- `python3 tests/docs/check_wiki_docs.py`: passed after the unit-filter docs
  update; validated 15 wiki markdown files.
- `CARGO_HOME="$PWD/.local/cargo-home"
  CARGO_TARGET_DIR="$PWD/.local/cargo-target"
  GOCACHE="$PWD/.local/go-build"
  GOMODCACHE="$PWD/.local/go-mod-cache"
  python3 tests/docs/verify_examples.py`: passed after the unit-filter docs
  update; 31 of 31 verified examples passed.
- `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir
  .local/cargo-target`: passed after the utility/action chunk; 24 Rust
  `journalctl` tests passed.
- `cd go && GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test ./cmd/journalctl
  ./journal`: passed after the utility/action chunk.
- `python3 tests/interoperability/run_journalctl_query_matrix.py
  --skip-follow`: passed against stock `journalctl` from systemd 260
  `(260.1-2-manjaro)` after the utility/action chunk, including stock-shape
  `--new-id128`, exact `--disk-usage --file`, and exact
  `--disk-usage --directory` output.
- `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed
  against stock `journalctl` from systemd 260 `(260.1-2-manjaro)` after the
  utility/action chunk, including the live follow checks.
- `GOOS=windows GOARCH=amd64 GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test -c -o
  ../.local/go-journalctl-windows.test.exe ./cmd/journalctl`: passed after the
  utility/action chunk.
- `GOOS=darwin GOARCH=arm64 GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test -c -o
  ../.local/go-journalctl-darwin-arm64.test ./cmd/journalctl`: passed after
  the utility/action chunk.
- `GOOS=freebsd GOARCH=amd64 GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test -c -o
  ../.local/go-journalctl-freebsd-amd64.test ./cmd/journalctl`: passed after
  the utility/action chunk.
- `python3 tests/docs/check_wiki_docs.py`: passed after the utility/action docs
  update; validated 15 wiki markdown files.
- `CARGO_HOME="$PWD/.local/cargo-home"
  CARGO_TARGET_DIR="$PWD/.local/cargo-target"
  GOCACHE="$PWD/.local/go-build"
  GOMODCACHE="$PWD/.local/go-mod-cache"
  python3 tests/docs/verify_examples.py`: passed after the utility/action docs
  update; 31 of 31 verified examples passed.
- `python3 tests/parser-parity/check_v260_manifest.py`: passed after the
  utility/action chunk; manifest still matches the official systemd v260.1
  surface.
- `python3 tests/parser-parity/run_parser_parity.py --rust-bin
  .local/cargo-target/debug/journalctl`: passed after the utility/action chunk;
  Rust `ok=93 skipped=0 failed=0`, Go `ok=93 skipped=0 failed=0`.
- `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir
  .local/cargo-target`: passed after the output-mode chunk; 24 Rust
  `journalctl` tests passed.
- `cd go && GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test ./cmd/journalctl
  ./journal`: passed after the output-mode chunk.
- `python3 tests/interoperability/run_journalctl_query_matrix.py
  --skip-follow`: passed against stock `journalctl` from systemd 260
  `(260.1-2-manjaro)` after the output-mode chunk, including exact stock
  comparisons for the short-family, `with-unit`, `cat`, `verbose`, and
  `export` modes, parsed-object parity for JSON frame modes, and
  `--output-fields` projection.
- `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed
  against stock `journalctl` from systemd 260 `(260.1-2-manjaro)` after the
  output-mode chunk; saved report
  `.local/interoperability/journalctl-query-full-output-mode.json` recorded
  `PASS results=248 failures=0`.
- `python3 tests/parser-parity/check_v260_manifest.py`: passed after the
  output-mode chunk; manifest still matches the official systemd v260.1
  surface.
- `python3 tests/parser-parity/run_parser_parity.py --rust-bin
  .local/cargo-target/debug/journalctl`: passed after the output-mode chunk;
  Rust `ok=93 skipped=0 failed=0`, Go `ok=93 skipped=0 failed=0`.
- `python3 tests/docs/check_wiki_docs.py`: passed after the output-mode docs
  update; validated 15 wiki markdown files.
- `CARGO_HOME="$PWD/.local/cargo-home"
  CARGO_TARGET_DIR="$PWD/.local/cargo-target"
  GOCACHE="$PWD/.local/go-build"
  GOMODCACHE="$PWD/.local/go-mod-cache"
  python3 tests/docs/verify_examples.py`: passed after the output-mode docs
  update; 31 of 31 verified examples passed.
- `GOOS=windows GOARCH=amd64 GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test -c -o
  ../.local/go-journalctl-windows.test.exe ./cmd/journalctl`: passed after the
  output-mode chunk.
- `GOOS=darwin GOARCH=arm64 GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test -c -o
  ../.local/go-journalctl-darwin-arm64.test ./cmd/journalctl`: passed after
  the output-mode chunk.
- `GOOS=freebsd GOARCH=amd64 GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test -c -o
  ../.local/go-journalctl-freebsd-amd64.test ./cmd/journalctl`: passed after
  the output-mode chunk.
- `git diff --check`: passed after the output-mode chunk.
- Sensitive string scan over changed durable artifacts and candidate code:
  passed after the output-mode chunk; the only matches were benign placeholder
  paths/phrasing, parser token wording, accounting terminology, source-name
  text, and sanitized SOW policy text.
- `.agents/sow/audit.sh`: passed after the output-mode chunk.
- `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir
  .local/cargo-target`: passed after the header/invocation/label chunk; 24
  Rust `journalctl` tests passed.
- `cd go && GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test ./cmd/journalctl
  ./journal`: passed after the header/invocation/label chunk.
- `python3 tests/interoperability/run_journalctl_query_matrix.py
  --skip-follow`: passed against stock `journalctl` from systemd 260
  `(260.1-2-manjaro)` after the header/invocation/label chunk, with
  `failures=[]` and status `PASS`, including `--header`,
  `--list-invocations`, `--invocation`, `-I`, stock short hostname/PID labels,
  `--no-hostname`, and the previously covered output/query cases.
- `python3 tests/parser-parity/check_v260_manifest.py`: passed after
  reclassifying `--setup-keys`; manifest still matches the official systemd
  v260.1 surface.
- `python3 tests/parser-parity/run_parser_parity.py --rust-bin
  .local/cargo-target/debug/journalctl`: passed after reclassifying
  `--setup-keys`; Rust `ok=93 skipped=0 failed=0`, Go `ok=93 skipped=0
  failed=0`.
- `python3 tests/docs/check_wiki_docs.py`: passed after the
  header/invocation/label docs update; validated 15 wiki markdown files.
- `CARGO_HOME="$PWD/.local/cargo-home"
  CARGO_TARGET_DIR="$PWD/.local/cargo-target"
  GOCACHE="$PWD/.local/go-build"
  GOMODCACHE="$PWD/.local/go-mod-cache"
  python3 tests/docs/verify_examples.py`: passed after the
  header/invocation/label docs update; 31 of 31 verified examples passed.
- `git diff --check`: passed after the header/invocation/label chunk.
- `.agents/sow/audit.sh`: passed after the header/invocation/label chunk.
- `go test ./cmd/journalctl`: passed after the explicit-directory vacuum chunk,
  including active/current protection and oldest archived-file deletion for
  `--vacuum-files`.
- `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir
  .local/cargo-target vacuum`: passed after the explicit-directory vacuum
  chunk, including active/current protection and oldest archived-file deletion
  for `--vacuum-files`.
- `python3 tests/interoperability/run_journalctl_query_matrix.py
  --skip-follow`: passed against stock `journalctl` from systemd 260
  `(260.1-2-manjaro)` after the explicit-directory vacuum chunk, including
  side-effect parity for `--vacuum-files`, `--vacuum-time`, and
  `--vacuum-size`; final rerun reported `status=PASS failures=0 results=262`.
- `go test ./cmd/journalctl ./journal`: passed after final
  explicit-directory vacuum validation.
- `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir
  .local/cargo-target`: passed after final explicit-directory vacuum
  validation; 26 Rust `journalctl` tests passed, including the
  `--vacuum-time=0s` no-op case.
- `python3 tests/parser-parity/check_v260_manifest.py`: passed after final
  explicit-directory vacuum validation; manifest still matches the official
  systemd v260.1 surface.
- `python3 tests/parser-parity/run_parser_parity.py --rust-bin
  .local/cargo-target/debug/journalctl`: passed after final explicit-directory
  vacuum validation; Rust `ok=93 skipped=0 failed=0`, Go `ok=93 skipped=0
  failed=0`.
- `python3 tests/docs/check_wiki_docs.py`: passed after final
  explicit-directory vacuum docs update; validated 15 wiki markdown files.
- `CARGO_HOME="$PWD/.local/cargo-home"
  CARGO_TARGET_DIR="$PWD/.local/cargo-target"
  GOCACHE="$PWD/.local/go-build"
  GOMODCACHE="$PWD/.local/go-mod-cache"
  python3 tests/docs/verify_examples.py`: passed after final
  explicit-directory vacuum docs update; 31 of 31 verified examples passed.
- `GOOS=windows GOARCH=amd64 go test -c -o
  ../.local/go-journalctl-windows.test.exe ./cmd/journalctl`: passed after the
  explicit-directory vacuum chunk with repo-local Go caches.
- `GOOS=darwin GOARCH=arm64 go test -c -o
  ../.local/go-journalctl-darwin-arm64.test ./cmd/journalctl`: passed after
  the explicit-directory vacuum chunk with repo-local Go caches.
- `GOOS=freebsd GOARCH=amd64 go test -c -o
  ../.local/go-journalctl-freebsd-amd64.test ./cmd/journalctl`: passed after
  the explicit-directory vacuum chunk with repo-local Go caches.
- `git diff --check`: passed after the explicit-directory vacuum chunk.
- `.agents/sow/audit.sh`: passed after the explicit-directory vacuum chunk.

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
  shape while seek/test still accept the older SDK cursor shape, and again
  after the unit-filter chunk to record the current file-backed filter
  contract, after the output-mode chunk to record full output-mode and
  `--output-fields` behavior, and after the header/invocation/label chunk to
  record invocation, `--list-invocations`, `--header`, and stock short-label
  behavior, and after the explicit-directory vacuum chunk to record
  `--vacuum-size`/`--vacuum-files`/`--vacuum-time` behavior.
  `.agents/sow/specs/journalctl-v260-parity-matrix.md` and
  `tests/parser-parity/v260-manifest.*` were updated to reclassify
  `--setup-keys` as recognized-unsupported based on official source evidence.
- End-user/operator docs: `rust/README.md`, `go/README.md`, `go/API.md`, and
  `docs/Reader-APIs.md` were updated for the cursor string format contract.
  `docs/Journalctl-CLI.md`, `go/README.md`, and `rust/README.md` were updated
  after the unit-filter chunk for current file-backed filter behavior and again
  after the output-mode chunk for output rendering and `--output-fields`, and
  after the header/invocation/label chunk for invocation filters,
  `--list-invocations`, `--header`, stock short labels, and the
  `--setup-keys` unsupported behavior, and after the explicit-directory vacuum
  chunk for `--vacuum-size`/`--vacuum-files`/`--vacuum-time`.
- End-user/operator skills: no affected output/reference skills identified for
  this chunk.
- SOW lifecycle: active SOW remains `in-progress` under `.agents/sow/current/`.
- SOW-status.md: no update needed for this non-terminal chunk.

Specs update:

- Added `.agents/sow/specs/journalctl-v260-parity-matrix.md` for the active
  SOW implementation contract. Updated `.agents/sow/specs/product-scope.md`
  for the shipped cursor string contract change and current file-backed filter
  contract, then for the output-mode and `--output-fields` contract, and then
  for invocation, `--list-invocations`, `--header`, and stock short-label
  behavior.
  Additional product-scope updates remain pending final shipped behavior and
  ship recommendation.

Project skills update:

- Parser parity workflow added under `tests/parser-parity/`. Project skill
  update remains pending until the full SOW establishes the durable final
  workflow and ship contract.

End-user/operator docs update:

- Updated `rust/README.md`, `go/README.md`, `go/API.md`, and
  `docs/Reader-APIs.md` for the cursor string format contract. Updated
  `docs/Journalctl-CLI.md`, `go/README.md`, and `rust/README.md` after the
  unit-filter chunk, after the output-mode chunk, and after the
  header/invocation/label chunk, and after the explicit-directory vacuum
  chunk. Final journalctl command documentation remains pending full
  implementation.

End-user/operator skills update:

- No affected output/reference skills identified yet.

Lessons:

- Parser tests must distinguish parsing, validation, and dispatch. A test that
  passes option names as the program name can create false confidence.
- Optional-argument CLI compatibility must be tested with a following match
  token. Stock `journalctl --lines FIELD=value` does not consume the match as a
  lines value.
- Stock `cat --output-fields` stores selected fields in a Set and can expose
  non-stable field order. Rust and Go keep deterministic requested-field order,
  and the shared oracle compares the selected line multiset for that one stock
  nondeterministic text mode.

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
