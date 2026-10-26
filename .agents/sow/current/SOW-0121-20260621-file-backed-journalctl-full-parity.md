# SOW-0121 - File-Backed Journalctl Full Parity And Ship Decision

## Status

Status: in-progress

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: reopened 2026-10-26 for confirmed PR12 review findings. SDK
SOW-0159 facade cursor EOF prerequisite is completed; CLI work resumes. The appended
regression gate records the approved fixes. The prior vacuum correction remains
accepted. PR12 stays draft; no merge or release is authorized.

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
- Implemented parser-level `--output=help` parity directly in Rust and Go:
  - Go prints the official v260.1 output mode list immediately after parsing,
    before requiring `--file` or `--directory`;
  - Rust accepts `help` as the official parser pseudo-mode and exits before
    validation/dispatch;
  - the shared query matrix now compares the exact stdout from stock
    `journalctl --output=help` against Go and Rust.
- Implemented file source selection parity directly in Rust and Go:
  - `--file` may be repeated and the official `-i` short option is supported;
  - `--file` values are expanded as glob patterns with stock
    `GLOB_NOCHECK`-style no-match preservation;
  - multi-file inputs are opened through existing `SdJournalOpenFiles` facade
    paths, so normal output, follow snapshots, invocation resolution,
    `--list-invocations`, `--verify`, `--disk-usage`, and `--header` all use
    the same resolved file set;
  - `--file=-` returns a specific portable unsupported message because
    seekable stdin-backed mmap-capable descriptors are not implemented;
  - no explicit `--file` or `--directory` now returns the portable unsupported
    default-host-journal message instead of a generic missing-input error.
- Implemented parser-level action argument restriction parity directly in Rust
  and Go:
  - official source evidence from `systemd/systemd @ c0a5a2516d28`:
    `src/journal/journalctl.c:1112-1115` rejects extraneous positional
    arguments for every action except show, list catalog, and dump catalog;
    `src/journal/journalctl.h:7-27` defines the action enum that drives this
    check;
  - Rust and Go now run the same parser-stage order for this rule: parser
    interaction checks first, early `--output=help`/`--version`/facility-help
    exits where applicable, then action-extra argument rejection before
    portable unsupported dispatch;
  - the shared query matrix now compares stock parser errors against Go and
    Rust for `--new-id128 foo`, `--fields TEST_ID=...`,
    `--field=MESSAGE TEST_ID=...`, `--verify TEST_ID=...`,
    `--disk-usage TEST_ID=...`, and `--sync foo`.
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
- Rust and Go implementation is complete for the active SOW scope.
- Rust and Go full parser recognition is implemented and locally validated.
- Rust and Go file-backed behavior is implemented and validated for `--reverse`,
  `--show-cursor`, `--lines` direction/default semantics, `--identifier`,
  `--priority`, `--facility`, `--grep`, `--case-sensitive`, `--dmesg`,
  `--this-boot`, `--cursor`, `--after-cursor`, `--cursor-file`, `--unit`,
  `--user-unit`, `--invocation`, `-I`, `--list-invocations`, `--header`,
  stock short labels including `--no-hostname`, `--new-id128`, explicit-input
  `--disk-usage`, explicit-directory `--vacuum-size`/`--vacuum-files`/
  `--vacuum-time`, full output-mode rendering, `--output-fields`, output
  controls (`--all`, `--full`, `--no-full`, `--pager-end`), exact stock
  empty-result output behavior, and portable path-match rejection.
- SOW lifecycle move, project status summary update, and local audit completed.

Tests or equivalent validation:

- Latest local validation after the second-review fixes:
  - `go test ./cmd/journalctl ./journal`: passed.
  - `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir
    .local/cargo-target`: passed; 26 Rust `journalctl` tests passed.
  - `python3 tests/parser-parity/check_v260_manifest.py`: passed; manifest
    matches systemd v260.1 official surface.
  - `python3 tests/parser-parity/run_parser_parity.py --rust-bin
    .local/cargo-target/debug/journalctl`: passed; Rust `ok=122 skipped=0
    failed=0`, Go `ok=122 skipped=0 failed=0`.
  - `python3 tests/interoperability/run_journalctl_query_matrix.py
    --skip-follow`: passed against stock `journalctl` from systemd 260
    `(260.1-2-manjaro)`: `PASS failures=0`.
  - `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed
    against stock `journalctl` from systemd 260 `(260.1-2-manjaro)`:
    `PASS results=384 failures=0`, including live follow checks.
  - `python3 tests/docs/check_wiki_docs.py`: passed; validated 15 wiki markdown
    files.
  - `CARGO_HOME="$PWD/.local/cargo-home"
    CARGO_TARGET_DIR="$PWD/.local/cargo-target"
    GOCACHE="$PWD/.local/go-build"
    GOMODCACHE="$PWD/.local/go-mod-cache"
    python3 tests/docs/verify_examples.py`: passed; 31 of 31 verified examples
    passed.
  - `python3 -m py_compile tests/parser-parity/run_parser_parity.py
    tests/interoperability/run_journalctl_query_matrix.py
    tests/parser-parity/v260-manifest.py`: passed.
  - `git diff --check`: passed.
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
- `go test ./cmd/journalctl`: passed after the output-control/empty-result
  chunk.
- `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir
  .local/cargo-target`: passed after the output-control/empty-result chunk; 26
  Rust `journalctl` tests passed.
- Manual repo-local stock probes against systemd 260 `(260.1-2-manjaro)` passed
  for binary-message blob output, `--all` NUL-containing text behavior,
  `--no-full` short ellipsization and verbose blob suppression, JSON large-field
  thresholding, empty-result output, and `--pager-end` selecting entries
  `pager-0005` through `pager-1004` from a 1005-entry fixture.
- `python3 tests/interoperability/run_journalctl_query_matrix.py
  --skip-follow`: passed against stock `journalctl` from systemd 260
  `(260.1-2-manjaro)` after the output-control/empty-result chunk, including
  binary/long output-control cases, empty-result cases, and `--pager-end`
  implicit 1000-line tail behavior; report `/tmp/journalctl-query-matrix-output-control.json`
  recorded `PASS results=319 failures=0`.
- `go test ./cmd/journalctl ./journal`: passed after the output-control/
  empty-result docs/spec update.
- `cargo fmt --manifest-path rust/Cargo.toml -p journalctl --all` followed by
  `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir
  .local/cargo-target`: passed after aligning Rust `--pager-end` dispatch with
  explicit `--head`; 26 Rust `journalctl` tests passed.
- `python3 tests/parser-parity/check_v260_manifest.py` and
  `python3 tests/parser-parity/run_parser_parity.py --rust-bin
  .local/cargo-target/debug/journalctl`: passed after the
  output-control/empty-result chunk; Rust `ok=93 skipped=0 failed=0`, Go
  `ok=93 skipped=0 failed=0`.
- `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed after
  the output-control/empty-result chunk including live follow checks; report
  `/tmp/journalctl-query-matrix-output-control-full.json` recorded `PASS
  results=331 failures=0`.
- `python3 tests/interoperability/run_journalctl_query_matrix.py
  --skip-follow`: rerun after the Rust `--pager-end` dispatch alignment passed
  with `PASS results=319 failures=0`.
- `python3 tests/interoperability/run_journalctl_query_matrix.py`: final rerun
  after the Rust `--pager-end` dispatch alignment passed including live follow
  checks; report `/tmp/journalctl-query-matrix-output-control-final.json`
  recorded `PASS results=331 failures=0`.
- `python3 tests/docs/check_wiki_docs.py`: passed after the
  output-control/empty-result docs update; validated 15 wiki markdown files.
- `CARGO_HOME="$PWD/.local/cargo-home"
  CARGO_TARGET_DIR="$PWD/.local/cargo-target"
  GOCACHE="$PWD/.local/go-build"
  GOMODCACHE="$PWD/.local/go-mod-cache"
  python3 tests/docs/verify_examples.py`: passed after the
  output-control/empty-result docs update; 31 of 31 verified examples passed.
- `GOOS=windows GOARCH=amd64 GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test -c -o
  ../.local/go-journalctl-windows.test.exe ./cmd/journalctl`: passed after the
  output-control/empty-result chunk.
- `GOOS=darwin GOARCH=arm64 GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test -c -o
  ../.local/go-journalctl-darwin-arm64.test ./cmd/journalctl`: passed after the
  output-control/empty-result chunk.
- `GOOS=freebsd GOARCH=amd64 GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test -c -o
  ../.local/go-journalctl-freebsd-amd64.test ./cmd/journalctl`: passed after the
  output-control/empty-result chunk.
- `git diff --check`: passed after the output-control/empty-result chunk.
- `.agents/sow/audit.sh`: passed after the output-control/empty-result chunk.
- `go test ./cmd/journalctl`: passed after the `--output=help` chunk.
- `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir
  .local/cargo-target`: passed after the `--output=help` chunk; 26 Rust
  `journalctl` tests passed.
- Exact stdout diff checks against stock `journalctl --output=help` passed for
  both `go run ./cmd/journalctl --output=help` and
  `.local/cargo-target/debug/journalctl --output=help`.
- `python3 tests/interoperability/run_journalctl_query_matrix.py
  --skip-follow`: passed after the `--output=help` chunk, including the new
  exact `output-help` stock/Go/Rust comparison; report showed `failures=[]`
  and `status=PASS`.
- `go test ./cmd/journalctl ./journal`: passed after the repeated/globbed
  `--file` chunk.
- `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir
  .local/cargo-target`: passed after the repeated/globbed `--file` chunk; 26
  Rust `journalctl` tests passed.
- `python3 tests/interoperability/run_journalctl_query_matrix.py
  --skip-follow`: passed after the repeated/globbed `--file` chunk, including
  repeated long `--file`, repeated short `-i`, globbed file input, no-match
  glob preservation failure behavior, `--file=-` unsupported behavior, and
  default-host-source unsupported behavior; report showed `failures=[]` and
  `status=PASS`.
- `python3 tests/parser-parity/check_v260_manifest.py` and
  `python3 tests/parser-parity/run_parser_parity.py --rust-bin
  .local/cargo-target/debug/journalctl`: passed after the repeated/globbed
  `--file` chunk; Rust `ok=93 skipped=0 failed=0`, Go `ok=93 skipped=0
  failed=0`.
- `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed after
  the repeated/globbed `--file` chunk including live follow checks; report
  showed `failures=[]` and `status=PASS`.
- `python3 tests/docs/check_wiki_docs.py`: passed after the repeated/globbed
  `--file` docs update; validated 15 wiki markdown files.
- `GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test ./cmd/journalctl
  ./journal`: passed after the action-argument restriction chunk.
- `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir
  .local/cargo-target`: passed after the action-argument restriction chunk; 26
  Rust `journalctl` tests passed.
- `python3 tests/interoperability/run_journalctl_query_matrix.py
  --skip-follow`: passed after the action-argument restriction chunk,
  including stock-oracle checks for extraneous positional arguments on
  `--new-id128`, `--fields`, `--field`, `--verify`, `--disk-usage`, and
  `--sync`; report showed `failures=[]` and `status=PASS`.
- `python3 tests/parser-parity/check_v260_manifest.py` and
  `python3 tests/parser-parity/run_parser_parity.py --rust-bin
  .local/cargo-target/debug/journalctl`: passed after the action-argument
  restriction chunk; Rust `ok=93 skipped=0 failed=0`, Go `ok=93 skipped=0
  failed=0`.
- `python3 tests/docs/check_wiki_docs.py`: passed after the action-argument
  restriction docs update; validated 15 wiki markdown files.
- `git diff --check`: passed after the action-argument restriction chunk.
- `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed after
  the action-argument restriction chunk including live follow checks; report
  showed `failures=[]` and `status=PASS`.
- `CARGO_HOME="$PWD/.local/cargo-home"
  CARGO_TARGET_DIR="$PWD/.local/cargo-target"
  GOCACHE="$PWD/.local/go-build"
  GOMODCACHE="$PWD/.local/go-mod-cache"
  python3 tests/docs/verify_examples.py`: passed after the action-argument
  restriction docs update; 31 of 31 verified examples passed.
- `GOOS=windows GOARCH=amd64 GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test -c -o
  ../.local/go-journalctl-windows.test.exe ./cmd/journalctl`: passed after the
  action-argument restriction chunk.
- `GOOS=darwin GOARCH=arm64 GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test -c -o
  ../.local/go-journalctl-darwin-arm64.test ./cmd/journalctl`: passed after
  the action-argument restriction chunk.
- `GOOS=freebsd GOARCH=amd64 GOCACHE="$PWD/../.local/go-build"
  GOMODCACHE="$PWD/../.local/go-mod-cache" go test -c -o
  ../.local/go-journalctl-freebsd-amd64.test ./cmd/journalctl`: passed after
  the action-argument restriction chunk.
- `CRATE_CC_NO_DEFAULTS=1 CC_x86_64_pc_windows_gnu="zig cc -target
  x86_64-windows-gnu" AR_x86_64_pc_windows_gnu="zig ar" cargo check
  --manifest-path rust/Cargo.toml -p journalctl --target
  x86_64-pc-windows-gnu --target-dir .local/cargo-target`: passed after the
  action-argument restriction chunk. A plain Windows Rust cross-check first
  failed because the workstation does not have `x86_64-w64-mingw32-gcc`; the
  Zig-based no-install path passed.
- `go test ./cmd/journalctl ./journal`: passed after the reviewer fix chunk.
- `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir
  .local/cargo-target`: passed after the reviewer fix chunk; 26 Rust
  `journalctl` tests passed.
- `python3 tests/parser-parity/check_v260_manifest.py`: passed after the
  reviewer fix chunk; manifest reports 71 official long options, 28 short
  option letters, 16 output modes, and 20 actions matching systemd v260.1.
- `python3 tests/parser-parity/run_parser_parity.py --rust-bin
  .local/cargo-target/debug/journalctl`: passed after short-option parser
  coverage was added; Rust `ok=121 skipped=0 failed=0`, Go `ok=121 skipped=0
  failed=0`.
- `python3 tests/interoperability/run_journalctl_query_matrix.py
  --skip-follow`: passed after the reviewer fix chunk; new stock-oracle cases
  cover `--user --unit=` rewrite, grep tail reverse ordering,
  short-output `--exclude-identifier`, and exact file-backed `--list-boots`
  default/tail/head/reverse output.
- `python3 tests/docs/check_wiki_docs.py`: passed after the reviewer fix chunk;
  validated 15 wiki markdown files.
- `CARGO_HOME="$PWD/.local/cargo-home"
  CARGO_TARGET_DIR="$PWD/.local/cargo-target"
  GOCACHE="$PWD/.local/go-build"
  GOMODCACHE="$PWD/.local/go-mod-cache"
  python3 tests/docs/verify_examples.py`: passed after the reviewer fix chunk;
  31 of 31 verified examples passed.
- `git diff --check`: passed after the reviewer fix chunk.
- `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed after
  the reviewer fix chunk including live follow checks; report showed
  `failures=[]` and `status=PASS`.
- `go test ./cmd/journalctl ./journal`: passed after the third reviewer fix
  chunk.
- `cargo test --manifest-path rust/Cargo.toml -p journalctl --target-dir
  .local/cargo-target`: passed after the third reviewer fix chunk; 26 Rust
  `journalctl` tests passed.
- `cargo build --manifest-path rust/Cargo.toml -p journalctl --target-dir
  .local/cargo-target`: passed after the third reviewer fix chunk to refresh
  the CLI binary used by parser parity.
- `python3 tests/parser-parity/check_v260_manifest.py`: passed after the
  third reviewer fix chunk; manifest reports 71 official long options, 28
  short option letters, 16 output modes, and 20 actions matching systemd
  v260.1.
- `python3 tests/parser-parity/run_parser_parity.py --rust-bin
  .local/cargo-target/debug/journalctl`: passed after the third reviewer fix
  chunk; Rust `ok=124 skipped=0 failed=0`, Go `ok=124 skipped=0 failed=0`.
- `python3 tests/interoperability/run_journalctl_query_matrix.py
  --skip-follow`: passed after the third reviewer fix chunk.
- `python3 tests/interoperability/run_journalctl_query_matrix.py >
  .local/sow-0121-query-full.json`: passed after the third reviewer fix chunk;
  summary was `status=PASS`, `failures=0`, `results=403`, `systemd=systemd
  260 (260.1-2-manjaro)`.
- `python3 tests/docs/check_wiki_docs.py`: passed after the third reviewer fix
  chunk; validated 15 wiki markdown files.
- `CARGO_HOME="$PWD/.local/cargo-home"
  CARGO_TARGET_DIR="$PWD/.local/cargo-target"
  GOCACHE="$PWD/.local/go-build"
  GOMODCACHE="$PWD/.local/go-mod-cache"
  python3 tests/docs/verify_examples.py`: passed after the third reviewer fix
  chunk; 31 of 31 verified examples passed.
- `python3 -m py_compile tests/parser-parity/run_parser_parity.py
  tests/interoperability/run_journalctl_query_matrix.py
  tests/parser-parity/v260-manifest.py`: passed after the third reviewer fix
  chunk.
- `python3 tests/parser-parity/v260-manifest.py | diff -u
  tests/parser-parity/v260-manifest.json -`: passed after the third reviewer
  fix chunk.
- `git diff --check`: passed after the third reviewer fix chunk.
- Final post-documentation validation before close:
  - `python3 tests/parser-parity/check_v260_manifest.py`: passed.
  - `python3 tests/parser-parity/v260-manifest.py | diff -u
    tests/parser-parity/v260-manifest.json -`: passed.
  - `python3 tests/docs/check_wiki_docs.py`: passed; validated 15 wiki
    markdown files.
  - `git diff --check`: passed.
  - `.agents/sow/audit.sh`: passed after moving SOW-0121 to
    `.agents/sow/done/`; audit reported `current: (empty)` and SOW-0121
    status/directory consistency `OK`.

Real-use evidence:

- Partial implementation evidence exists through stock-systemd comparison
  matrices using repository-local fixtures only. The final full matrix report
  `.local/sow-0121-query-full.json` recorded `status=PASS`, `failures=0`,
  `results=403`, and `systemd=systemd 260 (260.1-2-manjaro)`.
- Go local cross-compilation covers Windows, macOS arm64, and FreeBSD for the
  action-argument restriction chunk. Rust local cross-check currently covers
  Windows via the installed `x86_64-pc-windows-gnu` target and Zig. Rust macOS
  and FreeBSD runtime validation still requires user-authorized access to the
  target hosts or a separate user-approved toolchain setup.
- First read-only reviewer pass started after complete local implementation
  evidence. Two reviewer sessions were lost or stopped before final verdict
  and will be rerun after local fixes. Completed reviewer outputs found real
  parity gaps in `--user --unit=` rewriting, short-option parser coverage,
  grep tail reverse ordering, short-output `--exclude-identifier`, and
  `--list-boots` file-backed output. Each confirmed finding was validated
  against `systemd/systemd @ c0a5a2516d28` and local stock `journalctl` before
  implementation.
- Third reviewer-cycle findings were validated against local stock `journalctl`
  `systemd 260 (260.1-2-manjaro)` and repository-local fixture inputs before
  implementation. The follow-up stock-oracle matrix passed with 403 results and
  no failures.
- Final read-only reviewer recheck results:
  - Mimo: `PRODUCTION-GRADE: YES`; only non-blocking observations.
  - Qwen: `PRODUCTION-GRADE: YES`; only non-blocking observations.
  - Deepseek: `PRODUCTION-GRADE: YES`; only non-blocking observations.
  - GLM: `PRODUCTION-GRADE: YES`; only non-blocking observations.
  - Minimax: `PRODUCTION-GRADE: YES`; only non-blocking observations.
  - Kimi: the pre-compaction session handle was no longer available for a final
    transcript. The close decision is based on the five available completed
    reviewer verdicts plus local validation and audit evidence.

Reviewer findings:

- Confirmed: `--user --unit=` was not rewritten to user-unit filters in Rust
  or Go. Evidence: official `src/journal/journalctl.c:1121-1129` extends
  `arg_user_units` from `arg_system_units` when the journal type is current
  user. Fixed in both CLIs through shared effective-unit helpers that feed
  normal filtering and invocation unit resolution. Added stock-oracle query
  matrix case `file-user-plus-unit-rewrite`.
- Confirmed: grep with tail-style `--lines=N` did not imply reverse traversal.
  Evidence: official `src/journal/journalctl.c:1137-1143` sets
  `arg_reverse = true` when `arg_pattern` is set, `arg_lines_needs_seek_end()`
  is true, and follow is not set. Fixed Go dispatch to call reverse output for
  this case; fixed Rust dispatch and reverse-tail slicing. Added stock-oracle
  query matrix case `file-grep-tail-reverse`.
- Confirmed: short-option parser parity was not exercised, and Rust missed
  multiple official short aliases while Go missed `-g`. Added short-option
  probes to `tests/parser-parity/run_parser_parity.py`; fixed Rust `-D`, `-o`,
  `-N`, `-f`, `-M`, `-m`, `-c`, `-t`, `-T`, `-p`, `-g`, `-k`, and `-l`, and
  fixed Go `-g`. Parser parity now reports Rust `ok=122 failed=0` and Go
  `ok=122 failed=0` after adding the interspersed-option parser case.
- Confirmed: the SOW/spec analysis for `--exclude-identifier` was wrong for
  short-family outputs. Evidence: official `src/shared/logs-show.c:597-598`
  skips short-output entries whose `SYSLOG_IDENTIFIER` is in the exclude set,
  while JSON output is unchanged. Fixed Rust and Go post-filters to apply this
  only to `short*` and `with-unit` modes. Added raw short-output stock-oracle
  case `file-exclude-identifier-short`; kept the JSON oracle unchanged.
- Confirmed: file-backed `--list-boots` was incomplete. Go returned unsupported
  for explicit file input, and Rust printed a single header-derived boot with
  raw microseconds. Evidence: official `src/journal/journalctl-misc.c:108-150`
  prints `idx`, `boot id`, `first entry`, and `last entry` table rows. Fixed
  Rust and Go CLIs to collect boots from explicit file/directory entries,
  preserve official default/tail/head line selection, support `--reverse`, and
  print stock-formatted timestamps. Added exact stock-oracle cases
  `list-boots-file`, `list-boots-file-tail`, `list-boots-file-head`, and
  `list-boots-file-reverse`.
- Dispositioned as not blocking for this SOW chunk: maintainability comments
  about large CLI files, Rust vacuum regex allocation, and cursor-file
  directory fsync. These were explicitly rejected as required follow-up work
  for this SOW because no reviewer tied them to a file-backed parity failure,
  security issue, data-loss risk, or failing stock-oracle test.

Second reviewer pass findings and fixes:

- Completed second-pass reviewers split between production-grade verdicts and
  process/spec findings; two sessions timed out or exited without a final
  useful verdict. One read-only reviewer violated instructions by rebuilding
  the already-untracked `go/journalctl` helper binary; that untracked artifact
  is excluded from the SOW changes and commit scope.
- Confirmed: official `journalctl` accepts recognized options before or after
  show-action match arguments, but both rewrites rejected `FIELD=value
  --lines=2` / `FIELD=value --show-cursor` as matches or path-match
  unsupported errors. Fixed Go with a small argument permutation layer before
  `flag.Parse`; fixed Rust by letting clap parse known options after positional
  matches instead of treating every token after the first match as positional.
  Added parser parity interaction `interspersed-show-option` and stock-oracle
  query case `file-interspersed-lines`.
- Confirmed: stock treats bare `--boot` as the current boot but explicit empty
  `--boot=` as a parse error. The rewrites previously collapsed both to the
  current boot. Fixed Go optional-argument tracking and Rust boot preprocessing
  so bare `--boot` maps to current boot while explicit `--boot=` fails with
  `failed to parse boot descriptor`. Added stock-oracle error case
  `explicit-empty-boot`.
- Dispositioned as false positive: short-output boot separator differences are
  suppressed by stock when the matrix runs with `--quiet`, and the existing raw
  short-output oracle intentionally compares the quiet stock path.
- Dispositioned as documentation/spec hygiene: `--head` and `--tail` remain SDK
  extension aliases, not official v260.1 options. `docs/Journalctl-CLI.md` now
  labels them as SDK extension aliases while keeping stock `--lines[=[+]N]` as
  the official paging option.

Third reviewer pass findings and fixes:

- Confirmed: Go accepted negative explicit `--lines=-N` values and could panic
  in tail/list selection. Evidence: local stock `journalctl --lines=-2
  --file=/dev/null` reports `Failed to parse --lines='-2'.`. Fixed Go lines
  parsing to reject negative values before dispatch. Rust already rejected
  negative values through unsigned parsing.
- Confirmed: explicit empty `--lines=` must be rejected, while bare
  `--lines`/`-n` defaults to the stock 10-entry tail. Evidence: local stock
  `journalctl --lines= --file=/dev/null` reports `Failed to parse
  --lines=''`, while bare `--lines` parses and proceeds to file open. Fixed
  Rust and Go preprocessing so bare forms normalize to `10`, and explicit
  empty values fail before input dispatch. Added stock-oracle error case
  `explicit-empty-lines`.
- Confirmed: Go treated `--case-sensitive=` as true, but stock and Rust reject
  explicit empty boolean values. Evidence: local stock `journalctl
  --case-sensitive= --grep=x --file=/dev/null` reports a bad
  `--case-sensitive` argument. Fixed Go boolean parsing to reject empty values.
  Added stock-oracle error case `explicit-empty-case-sensitive`.
- Confirmed: Go did not parse stock short attached values and clusters such as
  `-n2`, `-ball`, and `-rn2`. Evidence: local stock accepts these forms, while
  stock-invalid forms such as `-n=2` and `-b=true` keep the leading `=` in the
  value and fail. Fixed Go short-option normalization before argument
  permutation. Added parser parity interaction `short-attached-values` and
  stock-oracle cases `file-short-attached-lines` and
  `file-short-cluster-reverse-lines`.
- Confirmed: Go ignored explicit `--reverse` when `--lines=N` was set, even
  though Rust and stock print newest-first in that combination. Fixed Go
  dispatch to route explicit reverse plus tail-style lines through reverse
  output. Added stock-oracle case `file-lines-reverse`.
- Final reviewer non-blocking observations were dispositioned:
  - Rust `--synchronize-on-exit=0` boolean spelling divergence: rejected as
    not worth blocking close because the option is a daemon-mode portable
    unsupported no-op and does not affect file-backed behavior.
  - Tail/head materialization performance in journalctl output paths: rejected
    as not worth a new SOW from this close because the CLI behavior is
    stock-correct under the current matrix and no reviewer supplied measured
    regression evidence. Core reader performance work remains governed by the
    existing performance contract and completed reader-performance SOWs.
  - Cursor-file directory fsync: rejected as not worth a new SOW from this
    close because the cursor file itself is atomically replaced and no
    stock-oracle or durability contract requires directory fsync for the
    portable CLI.
  - Missing extra tests for `-n=2`, `--list-invocations --reverse`, and
    clustered `-l`: rejected as close blockers because the stock-oracle matrix
    already covers the parser class, `--list-invocations` behavior, and short
    option recognition. These are test-depth suggestions, not uncovered
    behavior in the accepted surface.
  - Rust `-b=true` error text strips the leading `=` in the displayed value:
    accepted as a minor text divergence caused by clap value handling. Both
    rewrites reject the input with the expected parse-error class, and the
    matrix covers that class.

Same-failure scan:

- Current Rust and Go command surfaces were checked before SOW creation.

Sensitive data gate:

- Durable artifacts contain only option names, file paths, commit IDs, and
  summarized evidence. No raw sensitive data was needed.

Artifact maintenance gate:

- AGENTS.md: no update needed for this chunk; the routing exception is recorded
  in this SOW.
- Runtime project skills: `.agents/skills/project-journal-compatibility/SKILL.md`
  was updated after the second and third reviewer passes to require parser
  parity coverage for option interspersing, stock short attached values and
  clusters, and optional-argument edge cases in future journalctl parser
  changes.
- Specs: `.agents/sow/specs/journalctl-v260-parity-matrix.md` was corrected
  after reviewer/source validation to classify `--exclude-identifier=` as
  file-backed-required for short-family outputs and unchanged for
  JSON/export/verbose/cat, and to require stock `--list-boots` table and
  `--lines`/`--reverse` behavior. `.agents/sow/specs/product-scope.md` was updated
  to record that emitted cursor strings now use the official systemd cursor
  shape while seek/test still accept the older SDK cursor shape, and again
  after the unit-filter chunk to record the current file-backed filter
  contract, after the output-mode chunk to record full output-mode and
  `--output-fields` behavior, and after the header/invocation/label chunk to
  record invocation, `--list-invocations`, `--header`, and stock short-label
  behavior, after the explicit-directory vacuum chunk to record
  `--vacuum-size`/`--vacuum-files`/`--vacuum-time` behavior, and after the
  output-control/empty-result chunk to record `--all`, `--full`, `--no-full`,
  `--pager-end`, JSON threshold, and empty-result behavior, and after the
  `--output=help` chunk to record parser-level output mode list behavior, and
  after the repeated/globbed `--file` chunk to record file-source selection
  behavior, and after the action-argument restriction chunk to record
  non-show positional argument rejection behavior.
  `.agents/sow/specs/journalctl-v260-parity-matrix.md` and
  `tests/parser-parity/v260-manifest.*` were updated to reclassify
  `--setup-keys` as recognized-unsupported based on official source evidence.
  After the second reviewer pass they were updated again for stock option
  interspersing and explicit empty `--boot=` parser behavior. After the third
  reviewer pass they were updated for explicit empty `--lines=`, explicit
  empty `--case-sensitive=`, stock short attached values/clusters, and explicit
  reverse plus `--lines=N` behavior.
- End-user/operator docs: `rust/README.md`, `go/README.md`, `go/API.md`, and
  `docs/Reader-APIs.md` were updated for the cursor string format contract.
  `docs/Journalctl-CLI.md`, `go/README.md`, and `rust/README.md` were updated
  after the unit-filter chunk for current file-backed filter behavior and again
  after the output-mode chunk for output rendering and `--output-fields`, and
  after the header/invocation/label chunk for invocation filters,
  `--list-invocations`, `--header`, stock short labels, and the
  `--setup-keys` unsupported behavior, after the explicit-directory vacuum
  chunk for `--vacuum-size`/`--vacuum-files`/`--vacuum-time`, and after the
  output-control/empty-result chunk for `--all`, `--full`, `--no-full`,
  `--pager-end`, and empty-result behavior, and after the `--output=help`
  chunk for the parser-level output mode list, and after the repeated/globbed
  `--file` chunk for file-source selection behavior, and after the
  action-argument restriction chunk for non-show positional argument
  rejection, and after the reviewer fix chunk for `--exclude-identifier` and
  the `--user --unit=` rewrite, and after the second reviewer pass for the
  `--boot` versus `--boot=` distinction and SDK-extension status of
  `--head`/`--tail`, and after the third reviewer pass for bare
  `--lines`/`-n` defaulting and explicit empty `--lines=` rejection.
- End-user/operator skills: no affected output/reference skills identified for
  this chunk.
- SOW lifecycle: SOW-0121 is marked `completed` and moved to
  `.agents/sow/done/`.
- SOW-status.md: updated at close to move SOW-0121 from Current to Recently
  Closed Or Completed.

Specs update:

- Added `.agents/sow/specs/journalctl-v260-parity-matrix.md` for the active
  SOW implementation contract. Updated `.agents/sow/specs/product-scope.md`
  for the shipped cursor string contract change and current file-backed filter
  contract, then for the output-mode and `--output-fields` contract, then for
  invocation, `--list-invocations`, `--header`, and stock short-label behavior,
  then for the output-control/empty-result contract, and then for the
  `--output=help` parser-level output mode list, and then for the
  repeated/globbed `--file` source-selection contract, then for the
  action-argument restriction contract, and then for reviewer-discovered
  `--exclude-identifier`, grep reverse, `--user --unit=`, short-option, and
  `--list-boots` corrections, and then for second-review option interspersing
  and explicit empty `--boot=` behavior, and then for third-review explicit
  empty `--lines=`, explicit empty `--case-sensitive=`, and stock short
  attached/cluster behavior. No additional product-scope updates are pending
  before close.

Project skills update:

- Parser parity workflow added under `tests/parser-parity/`.
  `.agents/skills/project-journal-compatibility/SKILL.md` now requires that
  future journalctl parser or option-surface changes run the parser parity
  checks and cover option interspersing, stock short attached values/clusters,
  and optional-argument edge cases.

End-user/operator docs update:

- Updated `rust/README.md`, `go/README.md`, `go/API.md`, and
  `docs/Reader-APIs.md` for the cursor string format contract. Updated
  `docs/Journalctl-CLI.md`, `go/README.md`, and `rust/README.md` after the
  unit-filter chunk, after the output-mode chunk, after the
  header/invocation/label chunk, after the explicit-directory vacuum chunk, and
  after the output-control/empty-result chunk, and after the `--output=help`
  chunk, after the repeated/globbed `--file` chunk, and after the
  action-argument restriction chunk, and after the reviewer fix chunk for
  `--exclude-identifier` and the `--user --unit=` rewrite, and after the second
  reviewer pass for `--boot`/`--boot=` and `--head`/`--tail` extension wording.
  `docs/Journalctl-CLI.md` was updated again after the third reviewer pass for
  bare `--lines`/`-n` defaulting and explicit empty `--lines=` rejection.
  No final journalctl command documentation update remains pending before close.

End-user/operator skills update:

- No affected output/reference skills identified.

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
- Matrix cases that force JSON output can hide text-renderer-specific stock
  behavior. `--exclude-identifier` must be tested separately for short-family
  output because stock v260.1 applies it in `output_short()`, not JSON/export.
- Count-only action tests can hide format and content regressions. `--list-boots`
  now has exact stock-oracle checks for file-backed default, tail, head, and
  reverse output.
- Parser parity must cover option placement, not only option names. Stock
  `journalctl` accepts recognized options after show-action match arguments.
- Optional-argument tests need explicit empty-value cases. Bare `--boot` and
  explicit `--boot=` are different stock CLI inputs.
- Optional arguments need separate tests for bare, explicit empty, attached
  short, and stock-invalid equals forms. `--lines`, `--lines=`, `-n2`, and
  `-n=2` are distinct stock CLI inputs.
- Go's standard `flag` parser is not a stock `journalctl` parser. Short-option
  clusters and attached values must be normalized before `flag.Parse` when
  preserving systemd CLI parity.

Follow-up mapping:

- No remaining parity gaps are deferred outside this SOW.
- Reviewer hardening suggestions were either fixed or explicitly rejected above
  as not required for file-backed stock parity, security, runtime purity, or
  close readiness.

## Outcome

Completed.

Rust and Go now recognize the full official systemd v260.1 `journalctl`
command-line surface and implement the accepted portable file-backed behavior
covered by the parity matrix. Daemon/host-only behavior remains intentionally
unsupported in portable mode.

Ship recommendation:

- Long-term-best recommendation: keep both Rust and Go `journalctl` commands as
  product artifacts and keep them under the same parser and stock-oracle parity
  suite.
- Packaging recommendation when one standalone non-Linux executable must be
  selected first: ship the Go command as the default portable binary because
  this SOW has broader local cross-compilation evidence for Go
  (Windows/macOS/FreeBSD), while the Rust command remains the parity peer and
  Rust-package command.
- Risk: Rust macOS and FreeBSD native runtime validation was not run in this SOW
  because using those systems requires user authorization. The Linux stock
  oracle and local Rust tests passed, so this is a packaging-validation gap, not
  an implementation failure.

## Lessons Extracted

- Parser parity has to test behavior classes, not only option-name existence.
  The real bugs were in optional values, short attached forms, interspersed
  options, and action-order validation.
- Text-output options need raw output oracle checks. JSON-only checks can hide
  short-renderer behavior such as `--exclude-identifier`.
- Whole-SOW reviewer batching worked here because the final fixes were
  validated against stock systemd and then rechecked as one full surface.

## Followup

None required for SOW-0121 close.

## Regression Log

## Regression - 2026-06-21

Status: completed.

Trigger:

- The user requested read-only external gap analysis for Rust and Go
  file-backed `journalctl` parity after this SOW was closed.
- Reviewers and local verification found material gaps in the closed full
  parity claim.
- The user then instructed: "fix them".

User decision:

- Implement the verified gaps in this SOW regression repair.
- Do the implementation locally in this session; do not delegate
  implementation.
- When rerunning reviewers after fixes, provide an unbiased fresh review
  prompt only. Do not list the fixes, do not say it is a repeated review or
  round 2, and do not give reviewers steering context from prior findings.

What broke:

- Short-family output does not emit official boot separator lines between
  different boots when `--quiet` is not set.
- Go accepts invalid `--output=<unknown>` values and silently falls back to
  short output.
- Rust and Go accept explicit empty `--case-sensitive=` when `--grep` is not
  present.
- Rust and Go reject official boolean spellings `--case-sensitive=t` and
  `--case-sensitive=f`.
- Rust accepts stock-invalid `-n=2`.
- Rust and Go reject stock-supported relative timestamp text such as
  `--since="1 hour ago"`.
- Rust forward/head paths and Rust/Go tail paths materialize all matching
  entries before output selection. Rust and Go follow paths rescan snapshots
  every 100 ms and retain all seen cursors.
- Rust and Go `--list-boots` / `--list-invocations` discover rows by scanning
  entries and applying selection after collection.

Evidence:

- Local stock oracle: systemd `260.1-2-manjaro` with repository-local
  `.local/interoperability/journalctl-query/multi-boot-file.journal`.
- Stock printed two `-- Boot <id> --` separator lines for
  `journalctl --file <fixture> --boot=all --output=short TEST_ID=journalctl-query`;
  Rust and Go printed zero.
- Stock rejected `--output=jzon`; Go exited successfully with short output;
  Rust rejected.
- Stock rejected bare `--case-sensitive=` without `--grep`; Rust and Go
  accepted.
- Stock accepted `--case-sensitive=t` and `--case-sensitive=f`; Rust and Go
  rejected.
- Stock and Go rejected `-n=2`; Rust accepted it.
- Stock accepted `--since="1 hour ago"`; Rust and Go rejected.
- Source evidence:
  - `rust/src/cmd/journalctl/output.rs` renders short-family entries without
    boot separator state.
  - `go/cmd/journalctl/output.go` renders short-family entries without boot
    separator state and defaults unknown output modes to short.
  - `rust/src/cmd/journalctl/main.rs` validates `--case-sensitive=` only when
    compiling a grep filter and normalizes optional `-n` before clap parsing.
  - `go/cmd/journalctl/main.go` validates `--case-sensitive=` only when
    compiling a grep filter.
  - `tests/interoperability/run_journalctl_query_matrix.py` uses `--quiet` for
    stock raw comparisons, hiding boot separator output.

Why previous validation missed it:

- Raw stock text comparisons were run with `--quiet`, which suppresses boot
  separator lines.
- Parser parity checked explicit empty `--case-sensitive=` only with
  `--grep`, not as a standalone option.
- Parser parity did not include invalid output modes or stock-invalid `-n=2`.
- Timestamp cases covered a useful subset but not stock relative text.
- Output equality tests were optimized for correctness over large-journal
  memory behavior and did not include performance-contract assertions.

Pre-implementation gate:

- Problem / root-cause model: the closed SOW validated a broad parity matrix
  but missed several parser and renderer interactions plus hot-path allocation
  behavior.
- Affected contracts and surfaces: Rust CLI, Go CLI, parser parity harness,
  interoperability query matrix, `journalctl` parity spec, docs, SOW status.
- Existing patterns to reuse: current output renderer state objects, current
  `SdJournal*`/Go facade seek/next/previous primitives, current stock-oracle
  query matrix, current parser parity interaction table.
- Sensitive data plan: use only synthetic repository-local fixtures and
  sanitized command summaries; write no raw host journal data.
- Implementation plan:
  - Add boot separator emission for short-family output modes, suppressed by
    `--quiet`.
  - Validate Go output modes before rendering.
  - Validate `--case-sensitive=` independently of grep and extend bool parsing
    to `t`/`f`.
  - Preserve stock rejection for Rust `-n=2`.
  - Add relative `"N unit ago"` timestamp parsing where stock accepts it.
  - Stream Rust forward/head output, keep bounded tail buffers for Rust and Go,
    and reduce follow initial/tail memory where practical without changing the
    portable polling design in this repair.
  - Add shared parser/interoperability tests for the missed gaps.
- Validation plan:
  - `python tests/parser-parity/check_v260_manifest.py`
  - `python tests/parser-parity/run_parser_parity.py`
  - `python tests/interoperability/run_journalctl_query_matrix.py`
  - targeted Rust and Go tests for changed code
  - unbiased read-only external review from scratch, with no fix list and no
    repeated-review framing
  - `git diff --check`
  - `.agents/sow/audit.sh`
- Artifact impact plan: update the SOW, SOW status, tests, and specs/docs only
  if behavior wording changes. No end-user/operator skills are expected to
  change unless validation exposes a workflow gap.
- Open decisions: none. The user explicitly requested fixing the verified
  gaps.

Implementation repair - 2026-06-21:

- Rust `journalctl`:
  - rejects stock-invalid `-n=...` before clap normalization;
  - validates `--case-sensitive=` independently of `--grep`;
  - accepts stock boolean spellings `t` and `f`;
  - parses stock relative timestamp text in the `"N unit ago"` form;
  - emits boot separators for short-family, verbose, and with-unit modes when
    not quiet;
  - moves `--exclude-identifier` to render-time filtering so boot separator
    emission matches stock short output behavior;
  - streams forward/head rendering, bounds tail storage, keeps follow renderer
    state across polls, and resumes follow polling from the last cursor;
  - uses indexed `_BOOT_ID` unique-value enumeration plus per-boot head/tail
    seeks for `--list-boots`, with row-scan fallback for malformed or
    non-indexed compatibility cases.
- Go `journalctl`:
  - rejects unknown output modes before rendering;
  - validates `--case-sensitive=` independently of `--grep`;
  - accepts stock boolean spellings `t` and `f`;
  - parses stock relative timestamp text in the `"N unit ago"` form;
  - emits boot separators for short-family, verbose, and with-unit modes when
    not quiet;
  - moves `--exclude-identifier` to render-time filtering so boot separator
    emission matches stock short output behavior;
  - bounds tail storage, keeps follow renderer state across polls, and resumes
    follow polling from the last cursor;
  - uses indexed `_BOOT_ID` unique-value enumeration plus per-boot head/tail
    seeks for `--list-boots`, with row-scan fallback for malformed or
    non-indexed compatibility cases.
- Shared test harness:
  - parser parity now builds the Rust binary before testing, instead of
    reusing a stale binary;
  - parser parity covers `-n=2`, standalone explicit empty
    `--case-sensitive=`, boolean `--case-sensitive=t/f`, and invalid output
    mode behavior;
  - the stock query matrix no longer forces `--quiet` for raw stock output,
    exposing boot separator regressions;
  - the query matrix includes relative `"ago"` timestamps, `t/f`
    case-sensitive values, invalid output mode rejection, quiet/default boot
    separator output, and multiline short-message indentation/truncation.
- Rust workspace compile hygiene:
  - `rust/src/journal/src/netdata.rs` had a stale test-only `FileHeader`
    initializer that blocked full workspace validation. The fixture was
    updated with zero-valued fields matching the current `FileHeader` shape.

Validation evidence - 2026-06-21:

- `python -m py_compile tests/parser-parity/v260-manifest.py
  tests/parser-parity/run_parser_parity.py
  tests/interoperability/run_journalctl_query_matrix.py`: passed.
- `python tests/parser-parity/check_v260_manifest.py`: passed; official long
  options 71/71, short options 28/28, output modes 16/16, actions 20/20.
- `python tests/parser-parity/run_parser_parity.py`: passed; Rust 127/127,
  Go 127/127, zero failures, zero skips.
- `python tests/interoperability/run_journalctl_query_matrix.py`: passed
  against stock `systemd 260 (260.1-2-manjaro)`; 427 comparisons, zero
  failures; result captured at `.local/sow-0121/query-matrix-latest.json`.
- `cargo fmt --manifest-path rust/Cargo.toml --all --check`: passed.
- `cargo test --manifest-path rust/Cargo.toml --workspace`: passed.
- `gofmt -w cmd/journalctl/main.go cmd/journalctl/output.go` and
  `go test ./cmd/journalctl`: passed.
- `go test ./...` from `go/`: passed.
- `git diff --check`: passed.

Residual performance note:

- `--list-invocations` still discovers invocation ranges by iterating matching
  rows. Correctness parity is covered by the stock query matrix, including
  unit-scoped list-invocations cases. This is a remaining performance-contract
  limitation, not an observed CLI correctness failure. A systemd-like indexed
  implementation would need to preserve boot/unit match context while
  enumerating candidate invocation IDs and seeking first/last per ID; doing
  that safely is a separate shared CLI/facade design change if reviewers or the
  user classify it as release-blocking.

Reviewer plan:

- Run read-only external reviewers with a neutral from-scratch prompt. The
  prompt must not list these fixes, must not say this is a repeated review, and
  must not steer reviewers toward earlier findings.

Final close evidence - 2026-06-21:

- Read-only reviewers were rerun with neutral from-scratch prompts that did not
  list fixes, did not describe a review round, and did not steer reviewers
  toward prior findings, per user instruction.
- Tangible reviewer findings were fixed or dispositioned:
  - unlimited bare `--reverse` now streams in Rust and Go instead of
    materializing all matching rows;
  - Go vacuum opens Unix candidates with `O_NOFOLLOW` and verifies the opened
    file still matches the directory entry before deletion decisions;
  - field-name, field-value, repeated `--file`, `+` disjunction, reverse grep
    boot separators, version output, and vacuum edge cases are covered by
    stock-oracle tests;
  - false findings about Go export cursor output and Go Windows rename
    replacement were rejected with local source/test evidence.
- Final validation passed:
  - `python tests/parser-parity/check_v260_manifest.py`: 71 long options, 28
    short options, 16 output modes, and 20 actions matched the v260.1 manifest.
  - `python tests/parser-parity/run_parser_parity.py`: Rust 132/132 and Go
    132/132 passed.
  - `python tests/interoperability/run_directory_matrix.py`: passed.
  - `python tests/interoperability/run_journalctl_query_matrix.py`: passed
    against stock `systemd 260 (260.1-2-manjaro)` with 581 results and zero
    failures.
  - `go test ./...` from `go/`: passed with repo-local Go caches.
  - `cargo test --manifest-path rust/Cargo.toml --workspace`: passed with
    repo-local Cargo paths.
  - `python3 tests/docs/check_wiki_docs.py`: passed; 15 wiki markdown files
    validated.
  - `python3 tests/docs/verify_examples.py`: passed; 31/31 verified examples
    passed.
  - `git diff --check`: passed.
  - `.agents/sow/audit.sh`: passed; audit reported seven pending SOWs,
    empty current SOW directory, SOW-0121 completed in `done/`, and SOW-0122
    open in `pending/`.

Final reviewer disposition:

- `glm`, `kimi`, `mimo`, `deepseek`, `qwen`, and `minimax` were asked for
  read-only fresh review of SOW-0121 scope and changed journalctl surfaces.
- Blocking correctness and security findings from the reviewer reports were
  resolved before close.
- Remaining non-blocking findings are tracked or rejected:
  - `--list-invocations` indexed/range performance and portable follow
    scalability are tracked by pending SOW-0122.
  - Rust macOS, Windows, and FreeBSD native runtime validation remains release
    packaging evidence, not a Linux-local correctness blocker for SOW-0121.
  - Monolithic-file and regex-precompile comments are quality/performance
    follow-ups only; no stock-oracle correctness failure remains.
  - Go export cursor-output concern was rejected because exact export
    stock-oracle comparisons pass.

Final artifact maintenance gate:

- AGENTS.md: unchanged; the project-wide workflow already covers journalctl
  parity, daemon-only exclusions, review cadence, and SOW lifecycle.
- Runtime project skills: unchanged; existing project orchestration,
  docs-authoring, and journal-compatibility skills covered the work.
- Specs: updated `.agents/sow/specs/product-scope.md` and
  `.agents/sow/specs/journalctl-v260-parity-matrix.md` to record current
  portable-mode performance reality and SOW-0122 tracking.
- End-user/operator docs: updated `docs/Journalctl-CLI.md` with portable-mode
  follow and `--list-invocations` limits.
- End-user/operator skills: none exist for this project, so none changed.
- SOW lifecycle: SOW-0121 is completed and will be moved to
  `.agents/sow/done/` in the same commit as the code and artifact updates.
- SOW-status.md: updated at close to move SOW-0121 from Current to Recently
  Closed Or Completed and add SOW-0122 to Pending.

Final follow-up mapping:

- Implemented in SOW-0121: verified correctness and security gaps from
  reviewers, plus final stock-oracle coverage for the affected CLI surfaces.
- Tracked by pending SOW-0122: `--list-invocations` indexed/range performance,
  portable follow scalability, and large-fixture performance evidence.
- Rejected as not separate follow-up work: Go export cursor concern, because
  exact stock export comparisons pass; monolithic-code cleanup, because
  SOW-0097 and SOW-0098 already track parked maintainability debt.

## Regression - 2026-06-21 P0/P1/P2 Review Gate

Status: completed.

Trigger:

- The user set a stricter acceptance goal: run `glm`, `minimax`, `kimi`,
  `mimo`, `deepseek`, and `qwen` as unbiased first-time reviewers until no
  P0, P1, or P2 findings remain. Only P3 cosmetic findings are allowed.

Reviewer run:

- Prompt file: `.local/sow-0121/review-p0-gate-prompt.txt`.
- Reports:
  - `.local/sow-0121/review-p0gate-current-glm.txt`.
  - `.local/sow-0121/review-p0gate-current-minimax.txt`.
  - `.local/sow-0121/review-p0gate-current-kimi.txt`.
  - `.local/sow-0121/review-p0gate-current-mimo.txt`.
  - `.local/sow-0121/review-p0gate-current-deepseek.txt`.
  - `.local/sow-0121/review-p0gate-current-qwen.txt`.

Verified findings:

- P1: Rust `--help` hides most official options, while the parity matrix
  requires the full v260.1 option surface to be visible in help.
- P2: the parser parity help test only checks generic help headers, so it did
  not catch missing official options.
- P1: `--pager-end --reverse` without an explicit `--lines=N` uses tail output
  in both Rust and Go and prints oldest-to-newest, while stock v260.1 prints
  newest-to-oldest.
- P2: the ignored `go/journalctl` workspace binary was stale and diverged from
  current source for `--merge` boot-separator behavior. It was removed as a
  build artifact.
- P2: reviewers still classify `--list-invocations` row scanning and portable
  follow polling as non-cosmetic performance gaps under the stricter gate.

Validation evidence for the pager-end/reverse bug:

- Stock command:
  `journalctl --no-pager --file .local/interoperability/journalctl-query/multi-boot-file.journal --pager-end --reverse --boot=all TEST_ID=journalctl-query`
  starts with boot `cccc...`.
- Rust and Go current binaries printed boot `aaaa...` first for the same
  command because the `--pager-end` branch forced non-reverse tail output.

Pre-implementation gate:

- Problem / root-cause model: the prior close validated broad file-backed
  behavior but still left hidden-help surface gaps, a `--pager-end` dispatch
  ordering bug, weak help-surface validation, and performance paths that the
  stricter user gate no longer accepts as P2 follow-ups.
- Evidence reviewed: six reviewer reports listed above, local stock
  `journalctl` v260.1 probes, Rust and Go dispatch source, parser parity
  harness, product scope and parity specs.
- Affected contracts and surfaces: Rust CLI help, Go CLI help/test oracle,
  Rust/Go show dispatch, query matrix, parser parity, `--list-invocations`,
  follow polling, SOW lifecycle, SOW status.
- Existing patterns to reuse: manifest-driven parser parity, stock-oracle query
  matrix, existing facade cursor seek primitives, FIELD-index unique-value
  enumeration, current output renderer state.
- Risk and blast radius: medium. Help/test changes are low risk; show dispatch
  affects ordering; invocation/follow performance work touches required CLI
  paths and must preserve stock output.
- Sensitive data handling plan: use only synthetic repository-local fixtures
  and `.local/` reviewer reports. Do not read or write host journal data.
- Implementation plan:
  1. Make Rust and Go `--help` visibly list the full official v260.1 option
     surface and strengthen parser parity to assert it.
  2. Fix `--pager-end --reverse` dispatch in Rust and Go and add a
     stock-oracle matrix case.
  3. Remove stale ignored `go/journalctl` build artifact.
  4. Reduce follow polling P2 risk with adaptive idle backoff while preserving
     current cursor resume behavior.
  5. Replace `--list-invocations` row-scan discovery with indexed candidate
     enumeration and treat indexed traversal errors as real errors instead of
     silently scanning rows.
  6. Run local validation and rerun the same six unbiased reviewers from
     scratch.
- Validation plan:
  - `python tests/parser-parity/check_v260_manifest.py`
  - `python tests/parser-parity/run_parser_parity.py`
  - `python tests/interoperability/run_journalctl_query_matrix.py`
  - targeted Rust and Go tests
  - `go test ./...`
  - `cargo test --manifest-path rust/Cargo.toml --workspace`
  - `git diff --check`
  - six unbiased external reviewers with the same scope and no fix list
- Artifact impact plan: update this SOW, SOW status, specs/docs only if the
  final behavior or accepted limitations change.
- Open decisions: none. The user explicitly requested iteration until no
  P0/P1/P2 findings remain.

Regression repair implementation - 2026-06-21:

- Rust and Go help output now includes a visible `Official systemd v260.1
  option surface` block listing every official long option in the manifest.
- Parser parity now asserts that `--help` output contains every official
  manifest long option, so hidden-option regressions fail the test.
- Rust and Go dispatch now preserve `--reverse` when `--pager-end` is used
  without an explicit `--lines=N`.
- The query matrix now includes `--pager-end --reverse --boot=all` normal and
  raw-output cases.
- Portable follow now uses adaptive idle backoff from 100ms up to 1s in both
  languages, resetting immediately when new entries are emitted.
- `--list-invocations` and `--invocation` offset resolution now discover
  invocation ID candidates through indexed FIELD/DATA unique-value traversal in
  both languages. Indexed traversal errors now fail the action; the previous
  row-scan collector is no longer present in either implementation.
- The stale ignored `go/journalctl` build artifact was removed. It is already
  covered by `.gitignore`, so no committed ignore change was required.

Regression repair validation - 2026-06-21:

- `cargo check --manifest-path rust/Cargo.toml --bin journalctl`: passed.
- `go test ./...` from `go/`: passed.
- `python3 tests/parser-parity/run_parser_parity.py`: passed; Rust 132/132 and
  Go 132/132.
- `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed
  against stock `systemd 260 (260.1-2-manjaro)` with `failures: []`.

Reviewer gate status - 2026-06-21:

- Local fixes and validation are complete.
- The next gate is a fresh read-only reviewer run with the same six requested
  models. The prompt must not mention prior findings, fixes, or review rounds.

Second regression repair pass - 2026-06-21:

- Fresh reviewer gate was not clean:
  - `minimax` reported P2 for `--list-invocations --reverse` not reversing the
    invocation table in Rust or Go and for the missing stock-oracle case.
  - `kimi` timed out and cannot be counted as a clean unbiased report, but its
    partial output independently found the same `--list-invocations --reverse`
    issue.
  - `minimax` also reported the stricter `--merge --boot=all` rejection as
    non-cosmetic. Stock systemd v260.1 allows `--boot=all` with `--merge`.
- Implemented fixes:
  - Rust and Go invocation table selection now assigns row indexes before
    applying `--reverse`, matching systemd table behavior.
  - Rust and Go now allow `--merge --boot=all` while still rejecting specific
    boot selectors, `--this-boot`, and `--list-boots` with `--merge`.
  - The stock query matrix now includes `list-invocations-alpha-reverse` and
    `output-short-merge-boot-all` oracle cases.
  - Parser parity now asserts that `--boot --merge` is rejected but
    `--boot=all --merge` is not rejected as a boot/merge conflict.
  - The v260 parser manifest and parity spec now document the `--boot=all`
    exception.
- Validation after this pass:
  - `python3 -m py_compile tests/parser-parity/v260-manifest.py
    tests/parser-parity/run_parser_parity.py
    tests/interoperability/run_journalctl_query_matrix.py`: passed.
  - `cargo test --manifest-path rust/Cargo.toml -p journalctl`: passed; 28/28
    tests.
  - `go test ./cmd/journalctl` from `go/`: passed.
  - `python3 tests/parser-parity/check_v260_manifest.py`: passed; 71 long
    options, 28 short options, 16 output modes, 20 actions.
  - `python3 tests/parser-parity/run_parser_parity.py`: passed; Rust 132/132
    and Go 132/132.
  - `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed
    against stock `systemd 260 (260.1-2-manjaro)` with `failures: []`.
  - `cargo test --manifest-path rust/Cargo.toml --workspace`: passed.
  - `go test ./...` from `go/`: passed.
  - `git diff --check`: passed.

Third regression repair pass - 2026-06-21:

- Fresh reviewer gate was still not clean:
  - `deepseek` labeled daemon rotate-and-vacuum handling as P2 because
    `--rotate --vacuum-*` was not presented as the official combined action.
  - `deepseek` and `qwen` labeled `--list-invocations` scan fallback/performance
    wording as P2-class risk.
  - `kimi` returned an invalid non-review response and `minimax` timed out, so
    neither can be counted as a clean reviewer report.
- Implemented fixes:
  - Rust and Go now reject `--rotate` plus any `--vacuum-*` option through a
    distinct `--rotate with --vacuum-*` portable-unsupported path, preserving
    the official `ACTION_ROTATE_AND_VACUUM` distinction while keeping daemon
    rotation unsupported.
  - Rust and Go removed the old `--list-invocations` row-scan fallback. If
    indexed unique-value traversal fails, the action fails instead of scanning
    all entries.
  - Rust and Go unit tests now cover the distinct rotate-and-vacuum unsupported
    message.
- Validation after this pass:
  - `cargo test --manifest-path rust/Cargo.toml -p journalctl`: passed; 29/29
    tests.
  - `go test ./cmd/journalctl` from `go/`: passed.
  - `python3 -m py_compile tests/parser-parity/v260-manifest.py
    tests/parser-parity/run_parser_parity.py
    tests/interoperability/run_journalctl_query_matrix.py`: passed.
  - `python3 tests/parser-parity/check_v260_manifest.py`: passed; 71 long
    options, 28 short options, 16 output modes, 20 actions.
  - `python3 tests/parser-parity/run_parser_parity.py`: passed; Rust 132/132
    and Go 132/132.
  - `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed
    against stock `systemd 260 (260.1-2-manjaro)` with `failures: []`.
  - `cargo test --manifest-path rust/Cargo.toml --workspace`: passed.
  - `go test ./...` from `go/`: passed.
  - `git diff --check`: passed.

Fourth regression repair pass - 2026-06-21:

- Fresh reviewer gate was still not clean:
  - `deepseek` reported P1 for stock-accepted ISO `T` timestamp forms being
    rejected by Rust and Go.
  - `minimax` reported P2 for applying the `--lines=+N` reverse/follow
    conflict to non-show actions such as `--list-boots`.
  - `glm` timed out and cannot be counted as a clean report, but its partial
    output identified two real issues: cursor seek ordering incorrectly used
    the optional `x=` hash, and Rust `--directory=` accepted a regular file as
    file input.
  - `kimi` timed out and cannot be counted as a clean report. Its partial
    vacuum accounting concern was inspected and rejected because empty archived
    candidates are removed before retained usage is added.
- Implemented fixes:
  - Rust and Go timestamp parsing now accepts stock ISO `T` forms with local,
    `Z`, and colon-offset timezone forms while still rejecting compact offset
    forms that stock rejects.
  - Rust and Go now apply the `--lines=+N` reverse/follow conflict only to the
    show action, matching stock parser interaction behavior.
  - Rust `--directory=` now uses the directory-open path and rejects regular
    files instead of silently treating them like `--file=`.
  - Rust and Go cursor seek ordering now ignores the optional `x=` hash for
    candidate location while preserving exact cursor matching semantics.
  - Rust help output now exposes official options directly by removing hidden
    clap entries and keeping the visible systemd v260.1 option reference.
  - The stock-oracle query matrix now covers ISO `T` timestamps, mismatched
    cursor `x=` seek behavior, regular-file `--directory=` rejection,
    `--list-boots --lines=+N --reverse`, and
    `--list-invocations --lines=+N --reverse`.
  - Parser parity now covers the show-action-only `--lines=+N` conflict.
- Validation after this pass:
  - `python3 -m py_compile tests/parser-parity/v260-manifest.py
    tests/parser-parity/run_parser_parity.py
    tests/interoperability/run_journalctl_query_matrix.py`: passed.
  - `python3 tests/parser-parity/check_v260_manifest.py`: passed; 71 long
    options, 28 short options, 16 output modes, 20 actions.
  - `go test ./cmd/journalctl ./journal` from `go/`: passed.
  - `cargo test --manifest-path rust/Cargo.toml -p journalctl`: passed; 32/32
    tests.
  - `cargo test --manifest-path rust/Cargo.toml -p systemd-journal-sdk
    cursor_seek_order_ignores_x_hash_but_exact_match_checks_it`: passed.
  - `python3 tests/parser-parity/run_parser_parity.py`: passed; Rust 132/132
    and Go 132/132.
  - `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed
    against stock `systemd 260 (260.1-2-manjaro)` with `failures: []`.
  - `cargo test --manifest-path rust/Cargo.toml --workspace`: passed.
  - `go test ./...` from `go/`: passed.
  - `git diff --check`: passed.

Reviewer gate status after fourth pass:

- Local fixes and validation are complete.
- The fresh read-only reviewer gate is complete:
  - `glm`: `P0/P1/P2 FINDINGS: NO`; P3-only observations.
  - `minimax`: `P0/P1/P2 FINDINGS: NO`; P3-only observations.
  - `mimo`: `P0/P1/P2 FINDINGS: NO`; P3-only observations.
  - `deepseek`: `P0/P1/P2 FINDINGS: NO`; P3-only observations.
  - `qwen`: `P0/P1/P2 FINDINGS: NO`; P3-only observations.
  - `kimi`: first run timed out and was not counted; rerun completed with
    `P0/P1/P2 FINDINGS: NO`; P3-only observations.
- P3 review observations were inspected:
  - Rust cursor-file newline concern was a false positive; Rust writes
    `cursor\n`.
  - `--verify-only` is an SDK extension alias and remains documented as
    outside the official surface requirement.
  - Vacuum race and non-Unix `O_NOFOLLOW` comments are defense-in-depth
    hardening notes inside explicit user-supplied directories, not P0/P1/P2
    blockers.

Final validation gate after strict reviewer closure - 2026-06-21:

- Acceptance criteria evidence: Rust and Go recognize the complete official
  v260.1 option/action surface; parser parity reports 132/132 passing cases per
  language; stock-oracle query matrix reports no failures against local
  `systemd 260 (260.1-2-manjaro)`.
- Tests or equivalent validation:
  - `python3 -m py_compile tests/parser-parity/v260-manifest.py
    tests/parser-parity/run_parser_parity.py
    tests/interoperability/run_journalctl_query_matrix.py`: passed.
  - `python3 tests/parser-parity/check_v260_manifest.py`: passed; 71 long
    options, 28 short options, 16 output modes, 20 actions.
  - `go test ./cmd/journalctl ./journal` from `go/`: passed.
  - `cargo test --manifest-path rust/Cargo.toml -p journalctl`: passed; 32/32
    tests.
  - `cargo test --manifest-path rust/Cargo.toml -p systemd-journal-sdk
    cursor_seek_order_ignores_x_hash_but_exact_match_checks_it`: passed.
  - `python3 tests/parser-parity/run_parser_parity.py`: passed; Rust 132/132
    and Go 132/132.
  - `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed
    against stock `systemd 260 (260.1-2-manjaro)` with `failures: []`.
  - `cargo test --manifest-path rust/Cargo.toml --workspace`: passed.
  - `go test ./...` from `go/`: passed.
  - `git diff --check`: passed.
- Real-use evidence: the interoperability matrix built and compared synthetic
  repository-local journal fixtures against stock `journalctl` without reading
  or mutating live host journals.
- Reviewer findings and handling: every P0/P1/P2 finding from the reopened
  gate was either fixed and covered by tests or rejected with evidence as not
  P0/P1/P2. The final six requested reviewers all reported no P0/P1/P2
  findings.
- Same-failure search results: parser parity help checks now assert all
  official long options; stock-oracle matrix covers the repaired timestamp,
  cursor, `--directory`, `--pager-end --reverse`, list-boots/list-invocations,
  merge, and rotate-vacuum cases.
- Sensitive data gate: only synthetic repository-local fixtures and `.local/`
  reviewer reports were used. Durable artifacts contain sanitized command names,
  model names, paths, and aggregate results only.
- Artifact maintenance gate:
  - `AGENTS.md`: no workflow or project-wide guardrail change was needed.
  - Runtime project skills: no HOW-to-work-here change was needed.
  - Specs: `.agents/sow/specs/journalctl-v260-parity-matrix.md` was updated for
    `--boot=all --merge`, strict `--directory=`, ISO `T` timestamps, cursor
    `x=` seek ordering, and show-action-only `--lines=+N` conflicts.
  - End-user/operator docs: no user-facing behavior changed beyond the existing
    documented portable `journalctl` scope.
  - End-user/operator skills: none exist for this project.
  - SOW lifecycle: this reopened SOW is completed and will move from
    `.agents/sow/current/` to `.agents/sow/done/` in the same commit.
  - `.agents/sow/SOW-status.md`: updated for completion.
- Spec update: completed as listed above.
- Project skill update: no skill behavior changed; no update needed.
- End-user/operator docs update: no update needed because the docs already
  describe portable file/directory scope, supported query surfaces, and SDK
  extension aliases.
- End-user/operator skill update: no output/reference skills exist.
- Lessons extracted: strict reviewer gates need stock-oracle tests for parser
  edge cases, not just option-presence checks. Timed-out or malformed reviewer
  runs cannot count as clean approvals even if partial output looks favorable.
- Follow-up mapping:
  - Implemented: all reviewer-confirmed P0/P1/P2 gaps in this reopened SOW.
  - Tracked: SOW-0122 remains the pending performance follow-up for large
    archive evidence and scalability claims.
  - Rejected: P3-only vacuum hardening, SDK extension alias cleanup, and
    cosmetic error text differences are not required for this SOW acceptance
    gate.

## Regression - 2026-07-27 Mixed-Directory Boot-Row Oracle

Status: completed.

### What Broke

`tests/interoperability/run_mixed_directory_matrix.py::run_boots_check()` counts
every non-empty output line. Stock is invoked with `--quiet` and emits eight
boot rows. The Go and Rust rewrites are invoked without `--quiet`; both
correctly emit the stock-compatible `IDX BOOT ID ...` header followed by the
same eight rows. The harness therefore reports two false failures:

```text
FAIL: go list-boots: boot count mismatch: got 9, expected 8
FAIL: rust list-boots: boot count mismatch: got 9, expected 8
```

The sibling `run_directory_matrix.py` already normalizes this exact header and
compares the complete normalized rows against stock output. The mixed harness
was missed when SOW-0121 added stock-compatible action headers.

### User Decision And Repair Plan

The user selected option 1B, the long-term-best validation repair:

1. Normalize the exact `IDX BOOT ID` header for all readers.
2. Obtain expected normalized rows from stock systemd once.
3. Compare every reader's complete normalized boot rows, including index, boot
   ID, first timestamp, last timestamp, order, and row count.
4. Do not change Go/Rust journalctl behavior, SDK APIs, or production defaults.
5. Run the complete mixed-directory matrix, the sibling directory matrix, the
   journalctl query matrix, focused Rust/Go journalctl tests, Python syntax,
   whitespace, same-pattern search, and SOW audit.

### Artifact Impact

- `AGENTS.md`, project skills, specs, user docs, and operator skills: no update
  needed because this is test-oracle normalization only.
- SOW lifecycle: originating SOW-0121 returned to completed; SOW-0141 resumed
  after the repair.
- Sensitive data: synthetic fixture IDs and public command output only.

### Repair And Evidence

- Added the same exact table-header normalization already used by the sibling
  directory matrix.
- Replaced the weak row-count assertion with exact normalized stock-oracle row
  comparison. Index, boot ID, first timestamp, last timestamp, ordering, and
  row count must all agree.
- Added an oracle-completeness precondition so a stock binary missing support
  for one of the generated fixture formats cannot produce false SDK failures.
- Same-pattern search found no other count-only `--list-boots` comparison:
  `run_directory_matrix.py` already compares exact normalized rows, and the
  query matrix compares complete stock and SDK command output.

Validation:

- `python3 -m py_compile
  tests/interoperability/run_mixed_directory_matrix.py`: passed.
- Full-feature systemd `255 (255.4-1ubuntu8.16)` mixed-directory matrix:
  42/42 passed across stock, Go, and Rust, including exact eight-row boot
  output and sealed/XZ fixtures.
- Tagged systemd `260 (260.1)` directory matrix: passed.
- Tagged systemd `260 (260.1)` journalctl query matrix: passed with
  `failures: []`, including the list-boots cases.
- The repository-local tagged v260.1 binary is not a valid full mixed-format
  oracle: its version features report `-XZ -GCRYPT`, so it omits the XZ boot
  and reports that it was compiled without FSS. The new completeness
  precondition identifies this as an oracle limitation rather than attributing
  false failures to Go or Rust.
- Exact Go `1.26.2` `go test ./cmd/journalctl`: passed.
- Exact Rust `1.91` `cargo test --manifest-path rust/Cargo.toml -p journalctl
  --locked`: passed.
- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed.

No Rust or Go production source, API, behavior, file format, durability,
default, or compatibility contract changed.

## Regression - 2026-10-14 File-Backed Parity And Performance Preservation

Status: in-progress after signed SDK commit 25accee. Implementation authorized on 2026-10-15;
the user selected the complete systemd v262 CLI contract.

### User Request And Workstream Ownership

The user requested two SOWs, SDK and journalctl, with performance maintained or
improved and no performance harm. The project regression rule requires reusing
this originating SOW rather than creating a replacement journalctl SOW.
SOW-0159 owns SDK traversal/decoder/recovery primitives; this record owns CLI
parsing, dispatch, query controls, rendering, and exit behavior in Rust and Go.

The user approved SDK-first execution on 2026-10-15 because the commands consume
those primitives. Only one implementation SOW may execute at a time. This CLI
SOW remains paused until the SDK deliverable passes its gates; this reopening
does not authorize concurrent implementation.

### What Broke And Evidence

The completed claim of full portable v260.1 parity is contradicted by synthetic
file-backed comparisons. The command sources below were unchanged between
audited SDK commits `2b33553` and `dafcdac4b971d04e48768a1f831974f9252061e2`.
Runtime probes used stock systemd 255.4; expected behavior was also checked in
exact v260.1 source. They establish the listed failures, not a fresh full
target-version runtime certification.

| Case | Stock behavior | Observed Rust/Go failure |
| --- | --- | --- |
| `-F PRIORITY -n1` / `-n0` | Limit unique values / emit none | Rust ignores the limit; Go dispatches ordinary entry output instead of values. |
| `-N -nN`, combined action options | Preserve selected enumeration; last action wins | Go line dispatch overrides enumeration; both use fixed action priority. |
| Cursor or after-cursor plus `-n1` | Emit cursor row or its successor | Both return the final row instead. |
| Follow with `-S @0` or `--lines=all` | Emit all 1005 initial fixture rows | Both emit only the final ten. |
| `--no-tail -n1` | Override limit and emit all rows | Both emit one row. |
| Valid PCRE2 lookahead, for example `(?=.)` | Compile and match | Both current regex engines reject it. |
| Grep with no matches | Exit 1 | Both exit 0; empty grep/missing MESSAGE has an additional Go mismatch. |
| `--list-boots -o json` | JSON table data | Both emit a text table. Invocation-list JSON requires the same audit. |
| Empty/repeated output-field selection | Default/union/set semantics | Empty selection suppresses cat output; Go loses earlier selections; Rust rejects repeats. |
| Repeated flags/scalars | Stock repeat/override semantics | Rust rejects repeated `-q`, output, lines, and other setters. |
| Leading/trailing `+` | Parse failure | Both accept malformed disjunction placement. |
| Empty `--since=` or UTC-suffixed timestamp | Reject empty / accept valid UTC text | Both silently ignore empty time; both reject the valid UTC form. |
| `--system --user --unit=alpha.service` | Retain system-unit interpretation | Both apply the user-unit rewrite and lose matching rows. |

Source evidence:

- `go/cmd/journalctl/main.go:380`, `:1039`, `:1431`, `:1613`, `:2099`,
  `:2684`, `:2736`; `go/cmd/journalctl/output.go:49`, `:180`.
- `rust/src/cmd/journalctl/main.rs:458`, `:548`, `:574`, `:1939`, `:2702`,
  `:2980`, `:3097`, `:4009`; `rust/src/cmd/journalctl/output.rs:143`.
- `.local/journalctl-parity-2rx8qht8/behavior-probes.json` and
  `supplemental-probes.json` contain generated diagnostic results. The counts
  and cases above are the durable sanitized evidence summary.
- Both language CLI suites and all 132 parser cases per language passed during
  the audit. Those successes did not cover these behavioral combinations.

Time bounds with `-F` are not a v260.1 defect: stock and both rewrites enumerate
all stored values, including outside an empty time interval. v262 changes that
contract by rejecting scoped `-F`/`-N` combinations. Do not silently implement
time-windowed enumeration; the compatibility-target decision below is explicit.

### Why Previous Validation Missed It

- Parser probes generally accept any recognized-option result, even a later
  validation/open error. Name recognition is not operational parity.
- Grep cases use literal patterns and normally require stock success; no-match
  exit status and PCRE2-specific syntax were absent from the oracle cases.
- Follow-since tests supplied `--no-tail` or explicit small limits, masking the
  default-ten history loss. Cursor/line combinations and enumeration/line
  combinations were not exercised together.
- ID listings, repeated options, action order, empty field selections, and
  malformed separator positions lacked complete semantic/status assertions.

### Pre-Implementation Gate For This Regression

Gate status: target, regex direction and execution order approved; prerequisite
SOW-0108 and SDK SOW-0159 are complete in signed commits 4c49205 and 25accee. The earlier activation gate
above records historical work; this gate controls this regression implementation.

Problem / root-cause model:

- Independent action booleans and shared show-line handling do not reproduce
  stock action/argument order. Cursor paths retain the last selected rows rather
  than stopping after the cursor-relative limit. Follow defaults erase explicit
  all/since intent. Output and timestamp parsers implement incomplete stock
  semantics; regex dialect and no-match status differ from PCRE2 journalctl.

Evidence reviewed:

- The cases/source above; current product scope and parity matrix; prior SOW
  outcomes; shared parser/query/directory/verify harnesses; pending SOW-0122;
  SDK findings and ownership in SOW-0159; benchmark/reporting contracts.

Affected contracts and surfaces:

- Rust and Go file-backed commands, parser normalization, action dispatch,
  cursor/line/follow behavior, grep, text/JSON/export output, exit status, public
  help/docs, shared oracle fixtures, and direct CLI performance measurement.
- SDK core repairs belong to SOW-0159 and must be consumed without duplicating
  their decoder or recovery logic in command wrappers.

Existing patterns to reuse:

- FIELD/DATA indexed catalogs and unique visitors; native cursor/ENTRY-array
  seeking; bounded row selection; shared stock-oracle query/output/follow cases;
  existing binary-safe rendering and facade lifetime rules.

Risk and blast radius:

- High compatibility risk for scripts consuming exit codes, cursors, limits,
  JSON, and repeat semantics. High performance risk from extra scans/buffers or
  a slower universal regex engine. Pure-language/non-Linux constraints remain.

Sensitive data handling plan:

- Use synthetic repository-local files/directories only. Never query host
  journals or invoke daemon operations, logger, or systemd-cat. Place scratch,
  caches, profiles, cursor files, and isolated oracle builds under `.local/`.
  Durable artifacts contain sanitized counts, relative paths, and public commits.

Implementation plan:

1. Resolve target/regex design and ordering; preserve release baseline binaries
   and fixture/output identities before runtime edits.
2. Add paired stock-oracle regressions for every case family above, including
   explicit expected nonzero statuses and complete output/cursor assertions.
3. Repair parser/dispatch, enumeration/line precedence, cursor-limited seeking,
   and follow history while preserving indexed paths and bounded state.
4. Repair approved grep, timestamp, source-scope, and rendering semantics;
   validate the complete approved option surface after the last material fix.
5. Run the performance/portability/interoperability gates, update artifacts,
   obtain whole-SOW review or explicit waiver, audit, and close in one commit.

Validation plan:

- Run `tests/parser-parity/check_v260_manifest.py` and
  `run_parser_parity.py`, then the query, directory, mixed-directory, and verify
  matrices plus affected Rust/Go CLI unit/subprocess suites.
- Require an actual full-feature stock runtime for the approved target; record
  version, commit, compression and sealing features. PATH systemd 255.4 is
  supplementary evidence and cannot certify a v260.1 or v262 target.
- Cover text/binary/repeated fields, all affected output framings, first/last
  cursor identity, chronological/reverse order, limits zero/one/all, and live
  file/directory appends. Check both values and return codes without masking
  reader failures or sorting away order-sensitive mismatches.
- Preserve Windows/macOS/FreeBSD support; existing toolchains and repository
  caches only. Additional installations or remote execution need authorization.
- Apply the performance gate below, repeat the same-failure search, then run
  `.agents/sow/audit.sh` and require its complete-and-clean verdict.

Artifact impact plan:

- AGENTS.md: existing performance/purity/parity rules suffice unless a baseline
  or dependency-policy change is explicitly approved.
- Runtime project skills: update the compatibility workflow for added semantic
  oracle and CLI performance gates when implemented.
- Specs: correct the parity matrix/product scope to the accepted shipped target
  and behavior; do not advertise unimplemented outcomes during preparation.
- End-user/operator docs: maintain `docs/Journalctl-CLI.md`, help, and related
  guides for approved behavior, limitations, and migration implications.
- End-user/operator skills: none exist; check again at close.
- SOW lifecycle: reopen the original record, preserve its full historical
  narrative, and append this regression. SOW-0159 owns SDK repairs. SOW-0122
  remains separate broad scalability work, not an excuse to omit this gate.
- SOW-status.md: update canonical/root indexes and completion together with the
  eventual implementation commit, not as a later lifecycle-only commit.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced` (`v260.1`):
  `src/journal/journalctl.c:744`, `:964`, `:1072`, `:1121`;
  `journalctl-misc.c:187`, `:214`; `journalctl-show.c:73`, `:601`;
  `src/shared/pcre2-util.c:58`; `src/shared/logs-show.c:1452`.
- `systemd/systemd @ 8cc40e0c5e9234bf45084751ac53b1fbfe70b492` (`v262`):
  `src/journal/journalctl.c:958`; `journalctl-filter.c:26`; `NEWS`.

Open decisions:

1. Resolved on 2026-10-16: the user authorized read-only external review after
   complete local validation, including further rounds after material fixes.
   This SOW remains paused for SOW-0108 then SOW-0159; review has not run here.

Resolved on 2026-10-15: the user selected the complete v262 CLI contract,
including a full option/surface delta audit, fixtures and migration documentation.
The user approved SDK-first sequential execution. The SDK binary-format target
remains v260.1; the newer CLI target does not change journal bytes or compiler
minimums.
The user also approved native PCRE2 in the CLIs only, including a Go CGO
exception, retaining the performance gate. Core SDK purity and the prohibition
on system journal linkage remain intact. Dependency/version and portable build
details require source review and validation during this SOW, not a new product
design fork. The user selected prerequisite SOW-0108 repair before SDK/CLI work
resumes; preserve all existing working changes.

### Acceptance Criteria And Performance Gate

- Implement and test every confirmed CLI case family above in both languages;
  language-specific code differences do not justify different observable results.
- The user's no-performance-harm instruction is a hard acceptance condition. It
  overrides generic allowances for a documented slower path in this workstream.
- Before runtime changes, retain baseline release binaries, compiler/dependency
  metadata, immutable file/directory fixtures, output sinks, and baseline
  variance. Alternate baseline/candidate runs with warmups and repeated samples.
- Require unchanged or improved performance per affected valid-input case. No
  permissible slowdown percentage or aggregate-score offset is authorized.
  Inconclusive/noisy results need investigation before claiming acceptance.
- Measure complete-command elapsed/CPU time, first-output latency, rows/bytes
  per second, peak RSS, allocation/profile evidence where available, and follow
  catch-up/append/idle costs. SDK loop benchmarks alone do not certify startup.
- Include indexed `-N/-F`, low/high-cardinality values, cursor `-n1`, tail/reverse
  small limits, narrow time ranges, literal/approved complex grep, projected
  JSON, complete scans, and follow history on small/large synthetic inputs.
- Validate counts, order, cursors, and output checksums before interpreting
  timings. A ten-row broken follow result cannot be the performance standard
  for a correct 1005-row backlog; use equivalent complete work and controls.
- Preserve FIELD/DATA enumeration, seek/index paths, bounded row selection,
  reusable DATA, and requested-scope decompression. Do not insert complete
  archive scans, full-result buffering, repeated parsing/render allocation,
  full verification, or a disabled fast path to simplify the fixes.
- Add a focused repeatable process-measurement runner around shared fixtures;
  no dedicated CLI benchmark currently proves these cases. If core/facade
  paths change, also run the shared reader-core benchmarks and render reports
  through `tests/benchmarks/report_benchmarks.py`.
- Compare equivalent stock and fastest-sibling behavior where practical; record
  versions and limitations. Profile adverse/uncertain results. Reproducible
  regressions block acceptance and require redesign rather than a silent waiver.

### Implementation Review And Failure Handling

- CLI implementation is authorized after SOW-0108 and SOW-0159. Native PCRE2
  direction and the CLI-only Go CGO exception are recorded above; validate
  dependencies and performance before acceptance. Keep implementation sequential.
- Standing external review is authorized as recorded in Open decisions above.
  Review the complete repaired SOW after local validation using the system-wide
  reviewer skill, retaining failed invocations and their coverage gaps.
- Every reviewer remains read-only and receives the canonical repository-boundary
  block above, the complete SOW scope, source versions, performance evidence,
  unresolved findings, and prohibition on host-journal probing.
- Record failed correctness, performance, audit, and review gates. Do not mark
  this SOW completed or submit an unreviewed parity claim while they remain open.

### Preparation Validation And Artifact Maintenance

- Accepted decisions recorded: two sequential workstreams, SDK first, the
  complete v262 CLI target and no accepted performance regression. Regex
  direction and the Go CLI CGO exception are approved; SDK recovery termination
  belongs to SOW-0159 and uses normal EOF plus explicit status.
- Tests/evidence: the diagnostic comparisons and 132/132 parser results above
  are existing investigation evidence; no final implementation PASS is claimed.
- Preparation validation on 2026-10-14: whitespace, gate/state/index, and exact
  historical-narrative preservation checks passed. `.agents/sow/audit.sh`
  reported `SOW initialization complete and clean`, with no sensitive-data
  patterns in the scanned durable artifacts. Independent read-only planning
  review found no blocking consistency flaw; this is not implementation approval.
- Same-failure search: parser/dispatch, show loops, regex construction, field
  selection, time grammar, and ID-list rendering were inspected in both CLIs.
  Finish reachability and semantic-interaction scans during implementation.
- Real-use evidence: synthetic explicit-file reads/follow only; no live host
  journal, daemon operation, production service, or software install was used.

Sensitive data gate:

- Appended text contains sanitized counts, public source commits, and relative
  paths only. No raw private data or workstation mirror paths are copied here.

- Artifact maintenance: both SOW indexes and the SOW-0122 dependency/reference
  notes are updated during preparation. Product behavior/specs/docs are not
  changed before implementation. There are no output/reference skills to edit.
- Lessons: recognized options, passing parser probes, and valid closed-file
  reads do not establish semantic parity or preserved end-to-end performance.
- Follow-up mapping: all current CLI defects and their performance gate belong
  here; SDK causes belong to SOW-0159. Indexed invocation listing and general
  polling scalability remain SOW-0122. Unrelated pending work stays separate.
- Close only after full implementation, approved target/design, paired
  correctness/performance evidence, review or waiver, artifact gates, and clean
  audit. Move back to `done/` and commit with the implementation together.

### Review Authorization - 2026-10-16

The user selected option A, granting standing read-only external-review
authorization for SOW-0108, SOW-0159 and this CLI SOW at their complete local
validation milestones, including further rounds after material fixes. Use the
six named reviewers and the system-wide reviewer skill. Earlier unauthorized
review notes describe the prior preparation checkpoint and are superseded by
this decision. Keep complete v262 parity, the CLI-only native PCRE2 decision,
the no-performance-harm gate and sequential prerequisite execution unchanged.

### Resumption And Detailed Implementation Gate - 2026-10-17

Gate status: satisfied. The SDK prerequisite is delivered in signed commit
25accee. Only this CLI SOW executes; independent SOW-0045, SOW-0071, SOW-0160
and SOW-0161 remain paused or pending. No new product decision is outstanding.
The approved complete v262 target, CLI-only native PCRE2 and portable command
contract govern the routine implementation choices below.

Problem and evidence:

- Ordered upstream getopt processing cannot be represented by independent action
  booleans, a global help prescan or short-argument rewriting. Required values
  must remain verbatim. Setters validate in encounter order; terminal utilities
  return immediately. Final grep compiles after global checks and before every
  nonterminal action. The final action and scope decide enumeration conflicts.
- Cursor-relative limits, explicit all/since follow history, first raw MESSAGE
  matching and cat output, repeated projections, ID-table JSON, timestamp grammar
  and no-match status need paired behavioral controls rather than recognition.
- Exact authority: systemd/systemd @ 8cc40e0c5e9234bf45084751ac53b1fbfe70b492
  (v262), especially src/journal/journalctl.c, journalctl-filter.c,
  journalctl-show.c, journalctl-misc.c, src/shared/pcre2-util.c,
  time-util.c, parse-util.c and cli-util.h. The isolated full-feature oracle
  has PCRE2 10.42, XZ, LZ4, Zstandard and OpenSSL. The host binary is supplementary
  only. SDK binary-format target v260.1 and compiler floors remain unchanged.
- Baseline release-build-8/retained preserves 11 binaries, 359 source files and
  verified hashes. Its CLI binaries are the pre-CLI comparison; later SDK timing
  reports are not substituted for command startup or rendering evidence.

Affected surfaces and reused patterns:

- Both command parsers, action selection, post-filters, timestamp conversion,
  output tables/renderers and selection loops; existing parser/query/follow,
  directory, mixed and verify harnesses; CLI build callers, CI, docs and specs.
- Reuse SDK FIELD/DATA enumeration, cursor and realtime seeks, bounded row
  selection and binary-safe entry payloads. SDK algorithms and dependencies
  are not redesigned here. Reuse current flag storage, output-mode enums,
  validation helpers and fixture generators; no general parser framework.

Implementation sequence and bounded costs:

1. Advance the existing manifest/checker to pinned v262 OPTION macros, actions,
   output modes and hidden --introspect-cli. Add semantic oracle cases before
   repairs, preserving raw exit status, complete outputs and cursor identity.
2. Add one ordered CLI-private pass per language. Preserve required values,
   interspersed operands, --, clusters, attached optional values, native special
   boot/line lookahead, unique long prefixes and per-occurrence validation.
   Use an explicit action with last-action/rotate-vacuum semantics. Reuse Go
   flag storage and Rust Args/help storage without rebuilding parsers per row.
3. Implement PCRE2-8 directly in CLI-private wrappers. Compile the final raw byte
   pattern once; mirror stock casing and default byte mode without JIT/UTF/UCP.
   Rust owns native handles with RAII. Go reuses compiled code; one CGO call per
   requested MESSAGE creates match data, matches borrowed bytes, copies scalar
   results and frees match data before returning. This respects implicit CGO
   argument pinning without copying subjects or retaining Go pointers. Empty
   subjects use a native empty byte. Propagate matching errors and close every
   handle. SDK runtime retains CGO=0 and no PCRE2 dependency.
4. Apply v262 final-action -F/-N scope rejection. Keep native unique enumeration
   and value limits; fix cursor and follow limit precedence without scans or
   unbounded buffering. Preserve first raw MESSAGE, binary cat bytes, projection
   union/defaults, JSON ID tables and grep exit 1 with empty-success stderr.
5. Complete portable timestamp grammar once during command parsing. Use checked
   integer duration/epoch arithmetic and calendar fraction rounding; preserve
   local, UTC, offsets, named zones and native DST selection. Rust Jiff 0.2.38
   is a CLI-private pure-Rust zone provider (MSRV 1.70, Unlicense OR MIT), with
   existing Go time zone facilities. Retain glibc-compatible inverse conversion
   and narrow TZif metadata/leap handling where the provider omits required
   information. No process-global TZ mutation or new Go native exception.
6. Keep native PCRE2 build requirements confined to CLIs: Go cgo build tags and
   explicit CLI CGO=1 build calls; pure SDK builds/tests remain CGO=0. Maintain
   Linux/Windows/macOS/FreeBSD build paths with existing toolchains and caches.
   Native package installation on this server is not authorized or required by
   the plan. Review ABI/version/platform details before each dependency change.

Risk, validation and artifacts:

- High compatibility blast radius for scripts: encounter order, empty versus
  absent options, binary bytes, status codes and cursor positions. Dedicated
  stock-oracle negative controls and language-local regressions verify these.
  Additional per-message native-call cost needs focused complete-command
  evidence. No extra archive scan, MESSAGE copy, full-result buffer, SDK graph
  verification or universal per-row parser is permitted.
- Run both supported-floor CLI suites, the advanced parser manifest/parity,
  complete query/follow and directory/mixed/verify matrices, binary outputs,
  and available portable builds. Keep failures and exact source/tool hashes.
  Use synthetic repository fixtures only; never query host journals or alter
  services. Scratch/caches remain repository-local, durable evidence sanitized.
- The user's later performance decision 11 in SOW-0159 supersedes this record's
  earlier mechanical measurement list. Trace actual added work; careful review
  and representative high-level tests suffice for cold-only/equivalent paths.
  Use existing helpers and focused elapsed/profile controls for native grep,
  startup and credible costs. No large matrix/framework is required. Retain
  adverse results and investigate reproducible slowdowns; an uncertain interval
  alone proves neither equality nor regression. No slowdown waiver is granted.
- Update AGENTS.md and compatibility skill for the separate CLI target/native
  exception; parity matrix, product spec, CLI docs/help and build guidance for
  actual delivered behavior. Keep SDK target/compiler floors unchanged. No
  consumer output/reference skills exist. Reconcile follow-ups and both status
  indexes, complete whole-SOW review under standing authorization, run the
  complete-and-clean audit, then commit implementation and completed lifecycle
  together with an explicit signed commit. No push is authorized.

Open decisions: none. Portable implementation details implement existing approved
contracts; any genuinely incompatible design fork must be presented before a
change that would require it.

Initial semantic controls retain 116 baseline observations with 103 comparison
failures, including field-catalog ordering differences. Catalog comparisons now
compare names as sets, matching the existing unordered SDK catalog contract;
entry/cursor sequences and unique-value limits remain ordered and exact. The
first paired implementation reduces this batch to 14 failures: Rust ID-table
JSON and separator placement, plus two timestamp forms in each language.
Native PCRE2 syntax, raw-byte matching, no-match status, action order and cursor
limits pass in that intermediate batch. These are development results, not
final acceptance. Go CLI tests pass; a Rust subprocess banner test still names
the historical target and is updated to assert the approved v262 banner.

Timestamp occurrences retain one command-local inverse-conversion context and
cached numeric bounds. Reparse/reordering would change relative-now values and
DST fold choices; execution reuses parsed values. Jiff 0.2.38 and required pure
zone packages are fetched only into the repository Cargo cache. Cargo lock
changes add CLI dependencies and disambiguate the pre-existing bitflags entry;
no existing dependency version or SDK compiler minimum is raised. Native build
calls explicitly enable CGO only for the command; SDK/test fixture builds stay
pure. Private embedded Go tzdata keeps named zones available in standalone
non-Linux commands.

The initial existing harness run passes 300 static, 6 cursor-file, 34 portable
error, 60 utility and 18 empty-result observations. Of 174 output checks, two
expose the inherited C-locale ellipsis difference: stock uses three ASCII dots,
while both commands always use one Unicode ellipsis. Both renderers now select
the marker by character locale and account for its width. Each parser passes
its 134 recognition/interaction checks; the newly listed scope check is then
implemented rather than left skipped.

Development timestamp fixture failures are retained: stock rejects raw journal
header times outside VALID_REALTIME (zero or >= 2^55). The fixture now uses only
valid journal times and requires stock verification before comparisons. The
valid fixture yields 164 observations with three failures (spaced Z in both
commands, lowercase IANA name acceptance in Rust); fresh/seeded Berlin/New York
folds, both spring gaps and forced seasonal abbreviations pass. Rust's provider
lookup is constrained to exact IANA spelling, and spaced Z follows the separate
zone path instead of the failed welded-Z attempt.

The expanded follow check first exposes test setup limits, not product evidence:
initial history is individually synced before its five-second ready deadline.
The fixture writer now syncs initial history once before publishing ready, while
live appends still sync individually. Large stdout pipes then block at about
144 JSON rows before harness termination. File-backed output capture removes
that collection limit and retains actual exit codes plus intentional-stop
metadata. All 33 follow observations pass, including 1,005-row explicit-all,
since-default and no-tail backlogs and explicit zero. Earlier failed setup and
collection runs remain retained; message equality cannot mask child exit 1.

Additional bounded controls reproduce three timezone-provider differences in
both commands: a positive TZif leap changes a calendar bound by one second;
fresh TZif names can include a future standard abbreviation absent from the
current seasons; and a zero-transition TZif ignores its conflicting footer in
glibc. The CLI-private timestamp adapter will retain validated TZif metadata
alongside the existing provider, initialize and update the abbreviation pair,
apply leap corrections to UTC/local probes, and use the first standard type
before any transition. This is the narrow metadata supplement already specified
in the implementation gate. It runs once per timestamp context, never per row,
and introduces no SDK change or additional native binding. Named zones use the
installed database when one exists, so a bundled alias absent from that database
does not silently become a valid native spelling. Portable bundled fallback
remains for platforms without an installed zoneinfo database.

Focused release comparisons retain identical complete outputs across nine
representative commands, using the unchanged SDK and immutable pre-CLI binaries.
The first 100,000-row grep comparison is faster (Go median 0.819s to 0.511s;
Rust 0.215s to 0.206s), while tiny-command repeats expose a startup cost that
requires investigation before performance acceptance. Parent-fork high-water
RSS contaminates the initial process runner's RSS field; those numbers are not
accepted memory evidence. Dedicated hyperfine startup repeats and syscall
summaries isolate startup without expanding the benchmark framework. Static
link variants are diagnostic builds only. Independently, four Go CLI regular
expressions currently compile during every invocation although they serve only
boot descriptors or vacuum options. Move those constant compilations behind
standard sync.OnceValue helpers; this changes no matching semantics and removes
unrequested cold-path work. Validate boot/vacuum controls and compare complete
commands again. No performance waiver or linking-policy change is assumed.

### Build Entry-Point Review And Outstanding Decisions - 2026-10-17

The real binary interoperability entry point builds both writers and commands
from current sources, then passes all 18 binary JSON/export/verification checks
against stock v262 and the libsystemd test adapter. Earlier semantic wrappers
supplied prepared binaries and did not validate these build entry points.
Review found missing environment forwarding in the binary/basic builders and
hard-coded Rust artifact directories despite inherited CARGO_TARGET_DIR. Those
two builders now forward environments and select the actual target directory.

The same-pattern search found six further builders that conditionally copy a
Rust artifact, then accept an existing destination when the source is absent.
The repair uses the live runner's existing explicit source-file check followed
by unconditional copying. Resolve relative target-directory overrides against
Cargo's repository-root working directory in all affected builders, so build
and artifact selection refer to the same location. This changes test setup
only, adds no SDK/CLI runtime cost, and needs no new framework. Validate with
the existing actual-build binary matrix plus bounded builder failure controls.

Open decisions at the 2026-10-17 checkpoint, subsequently resolved below:

1. Authorize a separate signed CI snapshot on a new ci/journalctl-v262 branch
   to run the prepared Linux/macOS/Windows/FreeBSD workflow, or retain local-only
   work with native non-Linux validation outstanding. No push is yet authorized.
2. Accept the measured CLI-only fixed startup cost of native PCRE2/CGO and the
   portable timestamp provider, or retain strict startup parity and approve
   development of static native build profiles. Repeated tiny-command controls
   show Go approximately 3.0ms to 3.8ms and Rust 1.5ms to 1.8ms; complete
   100,000-row grep improves in both. Static builds are diagnostic evidence
   only. The no-regression requirement has not been waived, and dynamic builds
   remain the current source default pending this decision.

### Local Implementation And Validation Ledger - 2026-10-17

Implementation ownership remains one local writer. Read-only investigations
checked timestamp-provider differences, parser contracts and native build setup;
these are implementation evidence, not the authorized whole-SOW acceptance
review. No SDK runtime file changes belong to this CLI change set.

Implemented behavior:

- Both ordered parsers preserve required argument bytes, encounter-order
  validation, immediate terminal options, repeated setters and final-action
  dispatch. Empty source/cursor options preserve native presence semantics.
  The manifest, its generator and source checker now target exact v262,
  including hidden introspection, 72 long options, 28 short options, 16 output
  modes and 21 action enum values. The generator reproduces committed JSON.
- Final -F/-N actions reject entry/time/cursor/grep scope while retaining
  unrestricted indexed FIELD/DATA enumeration and unique-value limits.
  Forward cursor limits begin at the cursor. Follow honors resolved cursor
  history, since-only/all/no-tail backlog and explicit zero without retaining
  an unbounded output buffer. An empty cursor file retains the ordinary tail.
- CLI-private PCRE2 uses raw first-MESSAGE bytes, stock automatic casing and
  native compile/match failures. No-match show returns 1 with empty stderr.
  Patterns compile once. Rust reuses owned match data; Go borrows subjects for
  one native call that frees match data before returning, with no retained Go
  pointer or subject copy. SDK builds retain CGO=0 and no PCRE2 dependency.
- ID tables support four JSON framings and numeric microsecond bounds.
  Projection repetition forms a union without trimming field-name bytes.
  Cat preserves first raw values, NUL/newline bytes and delayed field-name
  validation. Locale-dependent text ellipses use the correct marker width.
- Timestamp parsing caches each occurrence once, preserving command-local
  inverse-conversion state. Checked durations/epoch arithmetic, calendar
  normalization/fraction rounding, offsets, exact named zones, folds/gaps,
  seasonal abbreviation state, TZif leap metadata and POSIX local zones are
  covered by the expanded native-oracle controls. No timestamp work is added
  to SDK row traversal. Go boot/vacuum constant regexes initialize on demand.

Validation uses the full-feature stock v262 executable built from
systemd/systemd @ 8cc40e0c5e9234bf45084751ac53b1fbfe70b492, with PCRE2 10.42,
XZ/LZ4/Zstd and OpenSSL. Host systemd 255.4 is not substituted for this oracle.
Private evidence root: .local/sow-0121-v262-krauk7l5/.

| Check | Result / retained evidence |
| --- | --- |
| Go CLI at supported Go 1.26.2 floor, with race detection | PASS; go-floor-race-3.log |
| Rust CLI at supported Rust 1.91 floor | 21 unit and 11 subprocess tests PASS; rust-floor-1.log |
| Parser recognition/interactions | 134/134 per language, no skips; parser-final-1.log |
| Additional v262 semantic observations | 160/160; v262-final-1.log |
| Timestamp semantic observations | 222/222; timestamp-final-1.log |
| Existing query/cursor-file/error/output/empty controls | 300/6/34/174/18 PASS; query-final-1.log |
| File-backed utility actions | 60/60; utilities-final-2.log |
| Follow history/cursors/live appends | 48/48; follow-4.log |
| Verifier/directory/mixed-directory observations | 63/34/42 PASS; integration-final-1/ |
| Actual builders plus cross-language binary JSON/export | 18/18 twice; binary-actual-2.log also exercises a relative target override |
| Fresh/missing Rust artifacts, relative target overrides and CLI-only CGO in all nine builders | 18/18; builder-controls-9pnv6kng/results.txt |
| Portable smoke contract on Linux | 28/28; portable-smoke-linux-1.log |
| Pure Go SDK build and runtime-purity guard controls | PASS; runtime-purity-1.log; SDK runtime diff empty |
| Executable documentation examples | 35/35; docs-validation/results.json |
| Documentation structure, Python syntax, changed workflow actionlint and diff whitespace | PASS |
| In-progress project audit | Complete-and-clean verdict; audit-in-progress-2.log |

Failed development/collection observations remain retained. In particular,
query-final-1 reused a vacuum fixture directory and encountered FileExistsError;
the utility collection was rerun in a fresh private workspace and all 60 passed.
The first follow setup synced each historical row and exceeded its ready
deadline; batched initial sync fixed setup. Pipe-backed collection then blocked
large histories; file-backed capture now drains complete output and records the
actual child exit plus intentional-stop status. Neither incomplete output nor
child exit 1 is accepted as a passing follow observation.

The build-entry-point same-failure search covered every interoperability CLI
builder. Missing env forwarding affected two; stale-copy fallback affected
seven including the binary builder, and all are repaired. The existing manifest
generator also retained v260.1 despite the updated JSON; its target, hidden
option, action and interaction additions now exactly regenerate v262 JSON.
These are validation-infrastructure consequences of the CLI target/native
dependency changes, without a new framework or product surface.

Performance evidence remains provisional while decision 2 above is open. Nine
complete-command comparisons retain equivalent complete output and immutable
pre-CLI binaries. Complete scan medians are Go 0.514s to 0.493s and Rust 0.220s
to 0.219s; 100,000-row grep is Go 0.819s to 0.511s and Rust 0.215s to 0.206s.
Short-command and projected-output adverse samples remain in command-perf-1/;
the follow-up cold-start repeats and loader/syscall inspection isolate the
fixed native startup cost. Unchanged SDK/index and ordinary row-render paths
retain their algorithms; no full scan, unnecessary decompression, full-result
buffer, per-row option parser or subject copy is introduced. The initial
parent-contaminated RSS field is rejected as memory evidence. Static diagnostic
builds demonstrate an alternative but do not establish a new build policy.

Dependency/build review:

- Native PCRE2 is confined to command wrappers and requires 8-bit ABI >=10.42;
  Rust uses size_t-compatible usize, not platform-dependent C long. PCRE2's
  declared license is BSD-3-Clause. SDK manifests/runtime do not link PCRE2 or
  system journal libraries. The libsystemd adapter above is a test oracle only.
- Jiff =0.2.38 and bundled timezone data are CLI-private pure Rust, declared
  Unlicense OR MIT and rust-version 1.70, below the supported Rust 1.91 floor.
  Lock additions do not upgrade existing dependencies. Optional defmt/bitflags
  lock entries do not become SDK runtime dependencies. The saved SDK Cargo
  dependency tree contains neither Jiff nor PCRE2. Go's embedded tzdata stays
  command-private. Compiler minimums and SDK journal-format target are unchanged.
- CLI build callers explicitly enable CGO; coverage/release retain independent
  pure SDK checks. CI declares PCRE2 development prerequisites. The prepared
  native workflow covers Linux, macOS ARM64, Windows CLANG64/GNU LLVM and
  FreeBSD. Local SDK cross-builds cannot certify these native CLI dependencies;
  actual non-Linux runtime evidence remains open under decision 1 above.
  The workflow selects the image-provided python3 on Unix and python on
  Windows, avoiding reliance on an unverified macOS unversioned alias.
  Changed workflows pass actionlint after that adjustment.

Artifact maintenance at this local checkpoint:

- AGENTS.md distinguishes the CLI v262 target and approved native exception
  from pure SDK v260.1 behavior. No additional runtime-purity exception exists.
- Compatibility skill records semantic/status/binary/follow and native build
  evidence rules. Release skill and RELEASING.md distinguish pure SDK checks
  from command-inclusive native tests; publication/tagging policy is unchanged.
- Product scope and parity matrix record delivered CLI semantics, native build
  requirements and the v262 migration. Historical filenames remain usable.
- Rust/Go README and CLI wiki explain the target, SDK/CLI dependency boundary,
  scoped enumeration rejection, cursor/follow fixes, binary projection, JSON
  tables and timestamp rules. Interoperability/coverage guides record build
  prerequisites and accurate output/status collection.
- No end-user/operator output skills exist in this repository. No copied
  output skill requires an update. Both status indexes retain this sole active
  implementation SOW and the completed signed SDK dependency.
- Durable changes use synthetic identities, public commits, relative source
  paths and sanitized counts. No private journal, identity, credentials or
  customer material is used. Scratch/logs/caches remain repository-local.
- Lessons: option recognition does not establish semantics; validate full
  cursor/backlog output and actual status; provider timezone metadata can
  change native parsing; actual builder entry points need fresh artifacts;
  separate SDK purity from CLI native prerequisites; trace added work and
  investigate reproducible costs without a large measurement framework.
- Follow-up mapping: current target/parser/query/output/timestamp/native-build
  work remains here. Indexed invocation listing and broad polling scalability
  remain the real pending SOW-0122. Reopened SOW-0045/0071 and pending
  SOW-0160/0161 remain separate, with no implementation mixed into this SOW.

Still required before close: explicit disposition of the two open decisions,
native platform evidence or a user-approved qualification limit, whole-SOW
external review under standing authorization with findings resolved, final
artifact/audit gates and one signed implementation-plus-lifecycle commit.
No whole-SOW readiness, performance PASS, native platform PASS or completion
claim is made at this checkpoint. No push, release or deployment has occurred.

### User Decisions And Normal PR Validation - 2026-10-18

The user agrees with the recommended bounded CLI startup allowance and asks
why normal PR CI would not cover the requested native platform checks. The
prepared workflow already runs on pull_request, so no separate CI-only branch
or special workflow dispatch is necessary. Use the normal PR's Linux, macOS,
Windows and FreeBSD jobs, with PCRE2 development prerequisites and CLI-only
CGO, while retaining independent pure SDK checks. Restrict the additional push
trigger to master, matching ordinary post-merge coverage and avoiding duplicate
native jobs for each PR-branch push. This is CI configuration within this SOW,
not a request for additional server software, services or remote infrastructure.

The accepted performance exception is specific: measured fixed startup on the
tiny Linux command is approximately Go 3.0ms to 3.8ms and Rust 1.5ms to 1.8ms,
for native PCRE2/CGO and the portable timestamp provider. Keep default native
linking; static builds remain diagnostics rather than a new packaging policy.
This allowance does not permit SDK or sustained query-throughput regressions,
and faster grep does not offset an unrelated regression. Existing reviewed
native-index/row paths and representative complete-command evidence remain the
performance basis under the user's engineering-judgment rule. No new timing
framework or universal per-case measurement requirement is introduced.

Both earlier product/authorization decisions are resolved. Complete the local
artifact/audit gate and authorized whole-SOW external review, then use a normal
PR for native CI. Keep this SOW open until platform results, reviewer findings
and final implementation/lifecycle gates are resolved. No merge, release or
deployment is authorized by this decision. The default docs/specs must describe
the accepted fixed CLI cost and retain the SDK/native dependency boundary.

### Post-Decision Primary Gate - 2026-10-18

The complete internal implementation audit and local validation are finished
for the CLI candidate against signed SDK base 25accee. Runtime code is unchanged
since the passing supported-floor, semantic/output/timestamp/follow and actual
builder runs above. The final changes record the accepted bounded startup
allowance in this SOW, product scope and CLI docs; update both status indexes;
and use the ordinary PR trigger plus master-only post-merge push coverage.
There is no additional CI-only branch or special remote execution procedure.

All 35 executable documentation examples pass again in the private
docs-validation-20261018 workspace, and all 16 wiki pages validate. Changed
workflows pass actionlint, changed Python sources parse, whitespace checks
pass, and the added-source identity scan is clean. The pre-review audit reports
SOW initialization complete and clean. No SDK runtime file differs from the
signed base. The candidate manifest retains all changed source files and both
dynamic CLI binaries under the private review-candidate directory referenced
by .local/sow-0121-v262-krauk7l5/current-review-candidate.txt.

No verified in-scope implementation blocker remains in the primary audit. The
candidate is ready for the authorized independent whole-SOW static review;
native non-Linux runtime qualification will run through the normal PR checks
and remains a pre-close gate. This local checkpoint does not claim those remote
results. Runtime source is frozen while review runs; only review evidence may
be appended. Any verified material defect requires repair plus complete renewed
internal validation before another full-scope round.

### Whole-SOW External Review - 2026-10-18

After the complete primary local audit, validation and post-decision artifact
updates, the full neutral prompt was displayed and all six authorized static
reviewers started in parallel in session b3ee3abf5edc47409040187cb33fbeb3.
The prompt covers this entire SOW, all tracked/untracked changes against signed
SDK base 25accee, indirect build/CI/consumer consequences, native reference
behavior, accepted fixed startup cost and the remaining normal PR platform
gate. Review is read-only: no tests, builds, writes, delegation or host-journal
access. This CLI round is distinct from both completed SDK rounds. Retain each
result/failure and independently adjudicate findings before PR submission and
closure; failed invocations supply no technical verdict.

All six initial invocations terminate after about 31-33 seconds with empty
stdout, repeated reconnect messages and the provider's temporary high-demand
error. No reviewer produces a technical verdict. These are concrete transient
transport failures under the reviewer policy; each receives its sole permitted
retry in the same session, using the identical full prompt displayed above.
There are no successful reviewers to rerun. Runtime source remains unchanged.

The six permitted retries also terminate with empty stdout and the same
temporary high-demand/reconnect failure after approximately 42-46 seconds.
All twelve attempts are retained; this round supplies zero technical verdicts
and six coverage gaps. No further retry is allowed in this round, and no
approval or review waiver is inferred. The technical review gate remains open.

### Normal Draft PR And Native Validation Plan - 2026-10-18

Continue the already-authorized normal PR validation while keeping this SOW
in-progress. Use one ordinary feature branch, fix/journalctl-v262-parity, and
a signed provisional CLI commit so GitHub can run the prepared platform jobs.
This is a draft for review/CI, not a completed SOW, merge or release. The PR
also carries the two earlier signed prerequisite/SDK commits not yet on origin.

After all remaining gates are resolved, fold validation/closure evidence and
the completed SOW move into the signed final CLI commit, preserving the
provisional commit as a local rollback point. Do not rewrite master or other
contributors' work. Any update of the task's own provisional PR head must
verify the exact captured remote commit and preserve the previous commit.
The final SOW lifecycle must remain coherent with its implementation; a
standalone completion-only commit is not the delivery plan.

### First Normal PR Native Results And Windows Checkout Repair - 2026-10-18

Normal draft PR #12 is open with signed provisional CLI commit c3b4b79,
following signed SDK/prerequisite commits. Native workflow run 39134674439
passes Linux and macOS ARM64, including both CLI suites and 28 portable smoke
controls. FreeBSD remains in progress at this checkpoint. Windows fails before
any build during actions/checkout: two upstream corrupted fuzz fixture names
contain colons and cannot be represented as Windows filenames. This is a
checkout setup failure, not a demonstrated CLI runtime failure.

The complete tracked-path inventory finds exactly two Windows-invalid names,
both under fixtures/systemd/test-data/corrupted/id:*.zst. Preserve all upstream
files in the repository; use Windows-only non-cone sparse checkout to omit
those two inputs. Do not omit the full corrupted directory: the Rust CLI's
verify_corrupted_fixture test requires zstd-truncated-frame.zst there.
The Go CLI verification tests require that same retained fixture. Independent
read-only reachability review confirms that neither CLI suite nor the portable
smoke uses either colon-named input. Git sparse-checkout check-rules, evaluated
against all 867 tracked paths, excludes exactly those two inputs and retains
the other 865, including all required fixtures. Actionlint and whitespace
checks pass; actual Windows qualification remains outstanding.
Linux/macOS/FreeBSD retain their ordinary checkouts. No SDK/CLI runtime changes
or test exclusions are needed. Validate Git's actual sparse-rule selection
against every tracked path, retain the required malformed-stream fixture,
run actionlint and obtain the real Windows CI result. Capture current CI
results before pushing this correction.

### Native Checkout And Independent Review Repairs - 2026-10-18

Normal PR CI at f5063ba passes Linux, macOS ARM64 and FreeBSD again.
Windows still fails before building: sparse selection is correct, but Git for
Windows validates index paths before omitting worktree files. Source evidence
is git-for-windows/git @ cc4dbf752a05efdc0e04e71fd3e8110d11bdd35c,
compat/mingw.c:4093-4099. Preserve the exact two sparse exclusions and disable
core.protectNTFS only through the checkout step's command-scoped environment.
Do not persist the setting, rename fixtures or omit other corrupted fixtures.
The pinned checkout action forwards that environment to each Git invocation;
subsequent status/log and sparse-index refresh paths do not rebuild entries.
Local rule inventory is evidence for selection, not proof of Windows success;
the ordinary PR Windows job must supply that result.

Independent static review finds five concrete gaps in the full v262 contract.
Pinned stock and both retained dynamic candidates reproduce the first four;
the metadata zero-limit case is also reproduced separately. Private before
records are retained under the current-review-reproductions.txt pointer.

- Go parseBoolOption trims whitespace, so invalid boolean values before help
  succeed while Rust/v262 reject them. Remove only that normalization.
- Bounded forward output performs PCRE matching after the requested final row:
  a valid first MESSAGE followed by a non-UTF8 MESSAGE makes -n+1 with
  --grep=(*UTF). print the row then fail, while v262 succeeds. Stop callbacks
  immediately after consuming the final selected row. Search and repair the
  corresponding chronological-tail collectors, and avoid Rust head-zero
  traversal. Preserve unlimited, cursor, render-exclusion and follow behavior.
- Both field-value actions accept invalid names, including an empty name with
  -n0. Validate at field enumeration time using the existing native field-name
  predicate, before the zero-limit shortcut. Preserve earlier terminal help
  and keep SDK APIs unchanged.
- Rust positional matches coerce raw OS arguments into String and reject a
  valid binary MESSAGE match that Go/v262 accept. Keep OsString through the
  CLI, test '+' and '=' as bytes, then use the existing parse_match_bytes and
  facade AddMatch. Lossy conversion is allowed only for diagnostics. Preserve
  current portable path-argument rejection and generated textual filters.
- Both metadata tables turn -n0 into successful empty output, while v262 exits
  one with no table. Apply no-data handling after limit selection, respecting
  quiet/JSON suppression of the diagnostic. Preserve input, unit and boot
  validation before that result. Check actual empty metadata selections too.

These are required consequences of the already-approved full CLI contract,
not new product choices or SDK work. Add paired shared controls before runtime
changes, retain their red results, repair the minimal CLI paths, and rerun both
supported-floor CLI suites plus parser/query/output and portable controls.
Removing trimming and excess traversal adds no hot-path work; the native
indexes and SDK row loops remain unchanged. Retain complete-command
performance evidence rather than adding another timing framework.

The initial primary checkpoint did not discover these gaps and is superseded
by this repair gate. Complete independent full-scope review and local repair
validation before a fresh whole-SOW external round. Provider failures from the
earlier round remain coverage gaps. The SOW remains in-progress.

The completed independent changed-surface audit adds a finite adjacent repair
set. Source comparison and pinned-oracle probes reproduce both CLI no-full
multiline/suffix loss, quiet verification PASS diagnostics, nonnative priority
aliases/case, facility numeric bounds and native integer syntax/width. Private
output-option-before.json and integer-before.json retain the exact results.
The original tests covered a 500-byte identical-message threshold case, not
ordinary multiline/suffix shortening, and decimal parser values only.

- Short output must process original lines before indentation, retain stock's
  90-percent prefix/suffix behavior, and apply its three-line/300-byte threshold
  and UTF8 cell-width rules. Keep full/show-all bypasses and avoid extra work
  in the default full-width path. Reuse existing output fixture/control tools.
- Pass effective quiet to verification reporting and suppress informational
  PASS lines only. Preserve malformed-file, missing-key and invalid-key errors.
- Use one small CLI-private native-integer parser per language for the six
  stock safe_ato consumers: lines, boot/invocation offsets, priority, facility
  and vacuum-files. Preserve leading ASCII whitespace, base prefixes/octal,
  signed 32-bit limits where applicable and unsigned 64-bit vacuum counts.
  Reject trailing whitespace/underscores and nonnative priority names. Numeric
  facilities extend through127. Boot-ID suffixes require an explicit sign and
  the descriptor's ID form follows the pinned native parser. No SDK parsing
  contract changes.
- Reproduce three-boot hit/miss/hit filtering in both directions. Observe boot
  transitions before grep, preserve intervening/trailing transition events,
  and keep quiet/merge/rendering and selected-row limits intact. Audit show,
  tail and follow callers rather than changing SDK traversal.
- Derive grep direction from the effective pager/line limit, including the
  implicit pager-end1000 count, and preserve explicit all in Go. Reuse the
  existing1005-row fixture for the latter control.
- Preserve an explicitly supplied empty Go cursor-file path through action
  use, rather than ignoring the requested update; earlier terminal help must
  still succeed. Named empty-content cursor files remain valid.

The last three source findings require native reproductions before repair.
These are required full-contract fixes, with no additional user design fork.
The reviewer found no SDK runtime changes or new native dependency leakage.
The verdict applies to the frozen provisional candidate, not the moving repair
work; complete validation and renewed whole-SOW review remain mandatory.

The expanded shared suite records280 observations and96 failures on the
retained provisional binaries, covering the complete finite repair list.
Three-boot separators and pager effective-limit controls reproduce the source
findings. The empty cursor-file probe additionally shows Rust rejecting before
rows while stock prints rows then reports the requested write failure; retain
OS-string parsing for that path in Rust and explicit presence in Go.
Follow is part of the boot-observation repair: select its initial tail from
native-filtered rows before grep, and retain a separate last-visited resume
cursor so nonmatching rows do not repeat boot transitions across polls.
Keep displayed/cursor-file state separate and preserve owned Rust cursor
strings where possible to avoid a new allocation per skipped row.

The first repaired-binary pass reduces shared failures from96 to10, all in
short output. The new controls expose two adjacent native rendering details:
v262 prints the last repeated MESSAGE in short mode (while grep/cat still use
the first), and its literal unknown-label fallback is printed without adding
those eight bytes to the indentation/width counter. Source logs-show.c:649-
733 and retained expected/actual bytes establish both. Preserve those exact
mode-specific rules, keep custom multiline fixtures free of the unrelated
repeated-MESSAGE control, and add a separate repeated-MESSAGE short case.
Return the actual label and its fallback-width adjustment together to avoid
repeating field lookups in each rendered row. No SDK change is required.

### Repair Validation And Normal PR Update - 2026-10-18

The complete finite independent review batch is implemented in the CLIs.
Supported floors Go1.26.2 (race) and Rust1.91 pass their full CLI suites;
Rust retains21 unit and11 subprocess controls. Shared repaired binaries pass
286/286 v262 semantic controls, both134-case parser suites,592/592 existing
query/cursor/error/utility/output/empty controls and54/54 follow controls.
The follow additions compare native-tail-before-grep behavior and complete
short output with intervening boot events across polls. New controls retain
old-binary red evidence; primary review also checks every matching caller.

The first new follow run times out awaiting the stock fixture writer's ready
file, before reader execution. The exact saved three-row input completes in
0.514 seconds on a fresh isolated run; the fresh complete54-case follow run
then passes. Preserve the earlier failure rather than treating it as CLI or
performance evidence. No process or service outside test children is stopped.
One repaired Rust fallback-label error is caught in the paired semantic pass
and corrected before the286-case clean result.

No SDK runtime file differs from signed base25accee. Native DATA/FIELD/ENTRY
index use, SDK allocation behavior and decompression scope are unchanged.
Native integer checks are cold; the boot-descriptor regex is eliminated.
Grep still runs once per visited eligible row; follow retains a visited cursor
without adding a Rust string allocation per rejected row. Shortening work runs
only with no-full; the default full-width branch adds no scans or width-table
work. A targeted release command comparison will check the changed show/grep
paths using the existing fixture and runner; no new measurement framework is
introduced. The previously accepted tiny startup allowance remains the sole
intentional CLI cost exception.

Normal PR quality findings are adjudicated individually. Deterministic
bootstrap sampling keeps its seeded generator with a local B311 annotation.
The explicit stock-oracle version invocation gets B603 on the actual call,
replacing its misplaced annotation. Five newly introduced blank-line findings
are removed. The eighth aggregate Pylint result is an unused expected local
in the already-changed follow harness; remove it without changing behavior.
These are test/tooling-only adjustments, including two comment-only SDK test
helpers, not a reopening of SDK runtime work. Raw reports remain private.

Docs/specs now describe integer/empty/quiet rules, mode-specific repeated
MESSAGE selection, native shortening and follow boot/position behavior. The
compatibility skill records the missed test classes. No new public SDK API,
compiler minimum, cache version, host service, release or installation changes
are involved. Both status indexes remain in-progress. Native Windows and the
complete final review remain outstanding before lifecycle closure.

### Final Short-Header Same-Pattern Audit - 2026-10-18

Primary source review of the changed short-prefix helpers finds and reproduces
five adjacent cases in both languages: `_EXE` incorrectly replaces the native
unknown label; an explicitly empty identifier is treated as absent; duplicate
identifiers select the first rather than last value; with-unit drops the user
unit when both units exist; and an invalid identifier is printed instead of
falling back to `_COMM`. The retained label-audit-current.txt pointer identifies
all15 before observations. Authority is systemd/systemd at the pinned v262
commit, src/shared/logs-show.c:193-205,649-675.

Repair the existing CLI-private header builder in both languages rather than
adding per-caller patches. Read the last raw value through borrowed field-map
views, distinguish absent/empty, apply native printable/length fallback rules,
combine the native system/user unit fields, and retain native width accounting
including its unknown/user-unit quirks. Construct Rust headers as bytes so
show-all never loses raw bytes through lossy String conversion. Keep grep/cat
first-MESSAGE behavior and all SDK code unchanged. Add paired controls for
these cases, raw show-all and the fallback width, then repeat affected CLI,
semantic/output and representative command checks. Existing header allocation
can be reduced by avoiding temporary value lists and cloned label strings.
This is part of the same approved native rendering contract and introduces
no new product choice or dependency.

The same label audit also reproduces exclusion and opaque-option consequences:
-Tfirst wrongly suppresses a row whose last identifier is second. Rust rejects
raw-byte identifier/exclusion arguments and silently drops a non-UTF8
projection, expanding JSON to all fields or making cat succeed. Pinned native
and Go observations are retained in exclude-before.json/opaque-before.json.
Preserve native last-identifier/C-string exclusion and raw OS arguments for
these opaque CLI values. Normalize projection names once at option construction:
retain valid UTF8 names for field lookups, retain the first invalid cat name
for delayed rendering-time errors, and keep explicit projection presence even
when every name is unmatchable. This avoids a UTF8 check for every projected
field on every returned row. Identifier filters can use existing byte AddMatch;
exclusions can compare borrowed raw bytes without lossy conversion/allocation.
No SDK API, dependency, or new user-visible syntax is introduced.

Full-output newline ownership is checked directly against native output before
freezing. Both languages lose the indentation of a trailing blank line and
drop lines after a NUL in show-all because they truncate the whole message
as one C string. Six native/candidate observations are retained under the
newline-boundaries-current.txt pointer. Trim the renderer-owned final newline
after indentation, and apply C-string truncation separately to each original
line only on the show-all/NUL path. Ordinary full output avoids an extra scan;
Rust reuses the borrowed last-MESSAGE view. Add paired full/shortened/trailing
blank/NUL controls, and remove the exclusion visitor made unused by the
borrowed last-value repair. These are the final same-path newline consequences
of the approved per-line native rendering contract.

### Complete Second Primary Review Repair Plan - 2026-10-18

The second independent whole-scope pass reports six finite classes, with no
P0/P1 finding. Its FAIL applies to the inspected candidate, not later repairs.
The scope includes parser, native regex, time, output, cursor/follow,
maintenance, consumers, dependencies, CI and artifact/performance contracts.

- Rust's empty-source guard uses failed UTF8 conversion as an empty value.
  Pinned-oracle probes reproduce nonempty raw root/image silently cleared.
  Test actual OsStr emptiness; preserve namespace presence and the approved
  portable-unsupported classification. Native ignores namespace with an
  explicit file, so that intentional qualification must not be represented
  as native rejecting the raw value.
- Both short renderers omit native tab/ANSI/CR preparation. Mirror the native
  byte state machine before newline truncation and printable/length checks;
  keep incomplete or unsupported controls intact, bypass show-all, and leave
  cat/grep raw. Borrow ordinary messages and allocate only when a control
  needs processing. The unchanged printable scan can detect those controls
  without adding another scan of ordinary ASCII messages.
- Both cursor checkpoints retain the last grep-selected row. Reproduce and
  retain terminal visited position independently of shown counts, including
  bounds, direction, EOF and zero-line positioning. Stop immediately at a
  counted limit and do no excess regex work. Reuse existing facade cursors;
  check show-cursor and cursor-file through the same finalization path.
- Replace single float/suffix vacuum-size parsing with native ordered compound
  uppercase E/P/T/G/M/K/B grammar, decimal fractional handling and checked
  u64 accumulation. This is cold CLI parsing; no SDK parser changes.
- JSON aggregation confuses a single binary byte array with repeated field
  values. Preserve that distinction in the existing aggregation path and
  check binary-first, multiple-binary and text-first values in every JSON
  framing. No additional per-row occurrence map is necessary: this encoder
  emits scalar strings/null or byte arrays, never scalar numbers.
- Rust converts native input paths lossily before facade opening. A concrete
  two-file reproduction reads the replacement-named journal instead of the
  requested journal, while native/Go read the requested four rows. Existing
  native readers cannot be injected into the private facade. A narrow SDK
  native-path exception versus an explicit CLI parity qualification is
  presented to the user; dependent implementation remains pending that
  decision. This is inherited behavior, not a regression from recent changes.

Authority is systemd/systemd @8cc40e0c5e9234bf45084751ac53b1fbfe70b492:
src/shared/logs-show.c:600-604,1115-1169,1310-1318;
src/basic/string-util.c:704-841; src/basic/parse-util.c:157-284;
src/journal/journalctl-show.c:73-208,496-520. Private raw-sources and
native-path-proof pointers retain direct observations. Add shared controls
and retain red results before runtime repairs, then validate the complete
finite batch and repeat whole-scope review. No installation, service change,
new measurement framework or additional startup allowance is proposed.

### Second Review Repairs Validated; Ordinary PR CI Update - 2026-10-18

The five CLI-private classes are repaired. Shared controls pass518/518,
including the explicitly qualified raw-namespace result, native cursor-file
contents, zero positioning, terminal after-cursor cases and all JSON framings.
Before repair, the first472-control expansion records90 failures on retained
binaries. One intermediate pretty-JSON comparison incorrectly parses multiple
objects as a single JSON document; streaming decode corrects the test without
loosening object/value comparison. The repaired binary-first nested arrays
match the native values in every framing.

Supported-floor Go1.26.2 race and Rust1.91 CLI suites pass again; Rust has21
unit and11 subprocess controls with no compiler warnings. Both134-case parser
suites,592/592 existing query/output controls and60/60 follow controls pass.
Follow adds skipped-suffix and no-grep-match cases that compare the actual
cursor-file content with the final native row after controlled child shutdown.
All28 portable smoke controls and CGO-disabled Go SDK compilation pass.
The complete-and-clean SOW audit, changed Python parsing, workflow actionlint,
16-page wiki structure and diff whitespace checks pass. The35 executed docs
examples remain unchanged by these prose-only documentation additions.

The broader query run catches a Go checkpoint repair consequence: the raw
facade getter exposes an internal end marker after a partial-seqnum seek.
Use the existing TestCursor no-entry mapping on the relevant error path and
retain a pre-advance checkpoint only after a successful after-cursor test.
The fresh complete592-control pass validates the correction. Ordinary query
opens retain their original concrete facade; the small CLI cursor adapter is
allocated only when cursor output/persistence is requested. It calls the SDK
metadata cursor accessor rather than expanding another row. Rust retains a
reusable cursor string only for requested checkpoints. No extra regex call or
row traversal is introduced after a finite match limit.

The matched-compiler release comparison uses Go1.27.1 and Rust1.97.1 on both
sides, matching the retained SDK-final binaries. Rust dynamically links PCRE2.
Three representative100,000-row commands use the existing five alternating
paired-sample runner and verify complete output equivalence before timing:

| Command | Go baseline / candidate median | Rust baseline / candidate median |
| --- | --- | --- |
| Cat scan |0.546942 /0.548073 s|0.241363 /0.242167 s|
| Literal grep |0.822826 /0.514308 s|0.222886 /0.214755 s|
| Short output |0.663984 /0.666907 s|0.352176 /0.339566 s|

The tiny adverse scan/Go-short median differences are inconclusive within the
observed sample variation; they are not a measured-equivalence claim. Primary
review finds unchanged native SDK traversal/decompression and removes a Go
UTF8 validation pass, temporary header values and unnecessary Rust clones on
ordinary output. The practical performance decision accepts this combined
representative evidence and review; no reproducible sustained slowdown is
established. The earlier comparison built with different compiler versions is
explicitly marked invalid and is not used as acceptance evidence. No new
benchmark framework or additional startup-cost allowance is introduced.

Native-path SDK implementation still awaits the explicit user decision.
SDK runtime diff against25accee remains empty. Keep this SOW in-progress and
update the ordinary draft PR with the validated CLI repairs, quality-comment
corrections and Windows checkout fix so native CI can supply actual evidence.
This checkpoint does not satisfy final review, native Windows or SOW closure.

### Actual Windows Build-Step Evidence - 2026-10-18

Normal PR run39143540947 at signed provisional0fc3d827 checks out Windows
successfully with the scoped index/sparse correction. PCRE2 prerequisites and
Go1.26.2 native compilation succeed. The Go CLI suite reaches one failure:
TestDirectoryInputDoesNotOpenRegularFileAsFile assumes the POSIX wording
"not a directory", while Windows returns its path-not-found OS diagnostic
for ReadDir on the existing regular file. The command does reject the source;
this is an inherited platform-specific assertion, not successful file fallback.

Strengthen the same test before accepting the platform: create a valid journal
with the existing fixture helper, prove --file reads its expected MESSAGE,
then prove --directory on that same file fails without output. This directly
detects accidental file fallback on every OS without depending on translated
or platform-specific OS text. No runtime or SDK change is needed. Run the
focused local control and then the ordinary PR's complete Windows suite.
The first native build failure is retained, and Windows remains unqualified
until the real job passes. No special CI service is required.

The same PR's Go analysis reports two unused initial header value assignments
and two integer-conversion warnings. The latter are false positives: the
shared mutable limit already bounds negative magnitudes to2^31 and positive
values to2^31-1, so every conversion fits even a32-bit int. Make those literal
bounds explicit in their respective branches to expose the existing proof;
do not change accepted values or suppress the check. Discard unused shortField
return values before the existing raw-unit lookups. A new markdown finding is
unquoted underscore-prefixed field names in the appended SOW; format those as
code. Run the affected CLI/parser controls and retain current CI results before
updating the task's provisional PR commit. SDK runtime remains unchanged.

The strengthened directory control passes locally. The Go floor race suite,
518 shared semantic controls and both134-case parser suites pass after the
literal-bound/unused-assignment cleanup. Twelve direct native observations
cover both signed32-bit limits and their immediate out-of-range neighbors.
This cleanup adds no hot-path operation; the existing matched-compiler timing
and complete-output evidence remain applicable without another timing batch.

Run39143540947 passes complete Linux, macOS ARM64 and FreeBSD checks at
0fc3d827; Windows's single wording assertion fails before Rust tests run.
The quality pipeline reports zero current Bandit/Pylint findings, one newly
fixed SOW markdown finding, and one unexplained Bandit tool error. The error
was also present in earlier PR runs; retain it separately from findings and
do not report the quality gate as passed. Its zero-result tool reports do not
yet close the three older annotations, whose latest instances still point to
an older merge commit. Current local source fixes those recorded cases.
All seven earlier inline threads were individually answered and resolved;
the newly reported equivalent findings will be answered in their own threads
after this correction is pushed. Final review and native-path scope remain
open. No SDK change, tool installation or service action is authorized here.
The same-failure search finds Rust's analogous wording assertion, but its
DirectoryReader emits that literal diagnostic before OS directory access;
it is platform-independent. No other CLI test relies on these POSIX OS error
phrases. The next Windows job still must execute Rust's full suite.

### Ordinary PR Native Qualification And Remaining Path Decisions - 2026-10-18

Normal draft PR #12 at signed provisional837873a7 now passes all four native
jobs in run39144771573: Linux ubuntu24.04, macOS15 ARM64, Windows2022 and
FreeBSD14.4. Windows completes both language CLI suites and portable smoke
checks after the platform-independent directory assertion repair. The scoped
checkout correction is also proven by this actual native job. No separate
CI-only branch, manual dispatch, service or remote execution procedure is
needed. Every executable PR check is successful, neutral or intentionally
skipped; only the draft WIP check remains in progress. Release publication and
wiki publication are intentionally skipped on PRs.

The current PR reports zero open analysis alerts and all14 inline review
threads individually answered and resolved. However, the green Codacy workflow
still logs one internal Bandit tool error and zero reported findings. Retain
that coverage limitation separately from the successful workflow status;
zero findings do not prove that every Bandit input was analyzed. This error
also occurred in earlier PR runs. Its cause is not established, and no tool
installation or unrelated analysis-framework change is proposed here.
Private native-ci-repair-4.json, pr-current-ci-complete.json,
pr-review-threads-4.json, pr-analysis-alerts-4.json and codacy-pr-ci-4.log retain
the results. Final external technical review remains outstanding; the earlier
transport failures supplied no verdicts.

The user has two concrete path decisions pending, with no dependent SDK or
glob implementation started:

1. Native Rust paths: the existing lossy conversion can open a replacement-
   named journal instead of the requested raw-byte parent path. Recommend an
   additive native-path facade API and native filename checks in this SOW,
   preserving existing string APIs and reader traversal. The surgical
   alternative explicitly rejects such inputs and documents the parity
   qualification. Evidence is rust/src/cmd/journalctl/main.rs:748-756 and
   rust/src/journal/src/facade.rs:88-138, plus the retained two-file oracle
   reproduction. Go already supports literal native-byte paths.
2. File patterns: Go filepath.Glob and Rust's Unicode/lossy matcher differ
   from native locale-aware file expansion. Under LC_ALL=C, the valid
   unicode-?.journal pattern does not select a two-byte UTF8 basename in
   native journalctl, while both candidates select it. Native brace-{a,b}
   expansion selects both files; both candidates fail. A raw-byte bracket
   class also demonstrates Go rejecting a native-valid pattern, and a
   replacement-character class selecting additional raw-byte files.
   Recommend native locale-aware Unix globbing confined to the CLIs, with
   an explicit Unicode filename contract on Windows. The surgical alternative
   retains a documented subset and rejects cases that could select the wrong
   files. SDK purity, journal indexes and row traversal stay unchanged by
   either file-pattern choice. The separate native-path choice controls any
   Rust SDK API addition.

File-pattern source evidence: go/cmd/journalctl/main.go:942 and
rust/src/cmd/journalctl/main.rs:800-862. Upstream authority is
systemd/systemd @8cc40e0c5e9234bf45084751ac53b1fbfe70b492,
src/journal/journalctl.c:424,1016 and src/basic/glob-util.c:102,160: locale
initialization plus native glob with NOCHECK, NOSORT and BRACE. Private
native-glob-proof-current.txt points to the retained literal, bracket, locale
and brace observations. Preserve ordinary text/unit matching independently
from the file-pattern decision. A limitation-only option must also cover
raw-byte names discovered through a Unicode directory or pattern.

This is evidence and design tracking only. SDK runtime still matches25accee.
Keep this SOW in-progress, preserve the successful PR head, and fold this
record into the next authorized implementation/lifecycle update rather than
invalidating native CI with an evidence-only push. Final path decisions,
their implementation or explicit qualification, and final review remain
required before coherent closure.

After recording this checkpoint, the local SOW audit reports initialization
complete and clean; diff whitespace checks pass. Evidence is retained in
audit-ci-complete-1.log. No implementation file changes in this checkpoint.

### User-Requested Review Retry After Provider Restart - 2026-10-18

The user reports restarting the model provider and explicitly requests retrying
the six failed reviewers. This authorizes one additional attempt per reviewer
in existing session b3ee3abf5edc47409040187cb33fbeb3 despite the earlier automatic
retry limit. No successful reviewer is rerun; all six prior results were empty
transport failures. Preserve both previous attempts and use the same complete
SOW scope against SDK base25accee, with factual updates for current837873a7,
validated CLI repairs, actual native CI results and recorded coverage limits.

The complete prompt is displayed before execution and retained privately as
whole-sow-review-retry-after-restart-20261018.txt. The user-requested recovery
of this interrupted review does not claim the current SOW is ready to close.
The two path-contract decisions remain pending, with no dependent SDK or glob
implementation authorized by this retry. Reviewers remain static and read-only,
with no tests, writes, delegation, dependency fetching or host-journal access.
Collect every result and independently validate any finding before action.

All six requested attempts launch in parallel as turn3 of the existing session.
Collection confirms every invocation exits1 with zero stdout after32.9-36.1
seconds. Each stderr again contains repeated reconnects and the temporary
high-demand message. These are invocation failures, not code findings or
technical verdicts. The continuing error's origin is not established by that
generic message; the user's provider restart does not prove recovery of the
entire request path. All18 attempts remain retained in the runner's session
logs. The result remains zero technical verdicts and six coverage gaps.
No further retry is started automatically. Final review and both pending path
decisions remain open; CLI/SDK implementation and the passing PR head are
unchanged by this recovery attempt.

### User-Requested Review Retry After Provider Repair - 2026-10-18

After identifying a provider bug and asking to wait, the user explicitly
requests another retry because the model service should now be working.
Resume all six failed reviewers in session b3ee3abf5edc47409040187cb33fbeb3
using the same full-scope, static-read-only prompt displayed before execution.
This is one further explicitly authorized attempt for each reviewer; retain
all earlier failures. Implementation remains at837873a7 with evidence-only
SOW/status edits. The pending path decisions and final acceptance gates are
unchanged. Collect and independently adjudicate every result.

The repaired-provider attempt reaches active source review for multiple
reviewers. Qwen alone exits after5.98seconds with zero stdout and explicit
HTTP429 Too Many Requests, rather than the previous generic reconnect error.
After more than three minutes of backoff, retry only Qwen once under the
renewed user request and the skill's transient-failure policy. Reuse the
identical displayed full prompt. Leave the five running reviewers untouched;
retain Qwen's failed turn4 and do not automatically retry its replacement.

Qwen's targeted turn5 also fails with HTTP429 after4.81seconds and no output;
its review coverage remains unavailable. Mimo's turn4 exits0 after308.53seconds
but produces no final agent message, a zero-byte last_message.txt and no
technical verdict. Treat that empty completion as a failed review, not a pass.
Retry only Mimo once for this concrete empty-output failure, reusing the same
displayed full prompt and preserving the other running reviews. No subsequent
automatic Mimo retry is allowed if this replacement fails or remains empty.

### Recovered Review Results And Primary Adjudication - 2026-10-18

Minimax turn4 returns PRODUCTION GRADE after553.8seconds with five P3 notes.
Its result is preserved in the session; no successful reviewer is rerun.
Primary adjudication does not adopt that verdict as SOW acceptance:

- Native paths and globbing: the underlying issues are already independently
  reproduced and remain blocking gaps against the requested complete contract.
  The P3/outside-implementation classification does not remove them from scope.
  No product qualification or reject-on-invalid-native-path repair has shipped;
  the two user decisions remain required before implementation and closure.
  The cited Rust code performs lossy conversion, not canonicalization.
- Review coverage: the zero-verdict statement describes the earlier failed
  attempts and is superseded by this actual review result. Remaining reviewers
  and Qwen's explicit availability gap must still be collected/dispositioned;
  no process success substitutes for a technical verdict.
- Performance: no concrete regression is established. Inconclusive timing
  differences remain part of the combined representative evidence and source
  review, rather than proof of measured equivalence. The user's engineering-
  judgment rule remains controlling; no larger timing framework is required.
- Boolean whitespace guard: already implemented at
  tests/interoperability/journalctl_v262_cases.py:187-190 for both supported
  boolean options, with leading/trailing padding and help ordering. The518
  shared controls execute those cases. The suggestion adds no uncovered
  behavioral requirement. PCRE whitespace is literal pattern content, not an
  invalid-boolean rule; do not implement the review's conflation of the two.
- Bandit coverage: the internal tool error is real and remains recorded.
  The latest retained PR alert inventory is empty, so the note about still-open
  older annotations is stale. The actual combined SARIF contains no detailed
  execution notification explaining the error. A focused syntax check of all
  116 tracked Python sources passes under Python3.12.3; this narrows the
  investigation but does not replace Bandit coverage or establish its cause.
  Evidence: codacy-current-combined.sarif and
  python-all-ast-analysis-error.json in the private validation directory.

No implementation change follows from this first recovered result. Other
running reviews remain independent; the active SOW remains in-progress.

At the recovered-review checkpoint, GLM, Kimi, DeepSeek and Mimo's replacement
remain running; their durable jobs continue while the two path decisions await
the user. The refreshed complete-and-clean SOW audit and diff whitespace check
pass. Evidence: audit-review-recovered-1.log and review-recovered-status-1.log.
Collect the next terminal notification as new results for turn4/turn5, even
though its session ID matches the earlier failed notifications.

### Recovered Round Collection And Timeout Continuations - 2026-10-18

The new terminal notification is collected as new results, not dismissed as
an earlier duplicate. The recovered attempt has the following actual coverage:

| Reviewer | Final collected attempt | Result |
| --- | --- | --- |
| Minimax | turn4,553.8seconds | Usable PRODUCTION GRADE review; five notes adjudicated above |
| GLM | turn4,1801.1seconds | Execution timeout, no final verdict |
| Kimi | turn4,1801.2seconds | Execution timeout, no final verdict |
| DeepSeek | turn4,1801.1seconds | Execution timeout, no final verdict |
| Qwen | turn5,4.81seconds | HTTP429 after its targeted retry; no verdict |
| Mimo | turn5,682.2seconds | Final text is a serialized tool-call fragment, not a review |

The three timeouts come from the runner's1800-second execution budgets, not
from the caller's shorter polling waits. They had been reading source before
termination. Use their single permitted timeout retry under the renewed user
authorization, continuing the exact existing client sessions in parallel.
The source remains unchanged; the full original scope is preserved, with an
instruction to reuse completed inspection and reserve time for a final report.
The complete continuation prompt is displayed before execution and retained as
whole-sow-review-timeout-continuation-20261018.txt. Do not retry any of these
three automatically again if its replacement fails or times out.

Qwen and Mimo have already used their targeted retries in this repaired-provider
attempt. Neither supplies a verdict, and neither is retried automatically.
Mimo's malformed final text is not executed. Its exact cause is not established
by the output alone. Preserve both coverage gaps and all prior results. No
service, client configuration, dependency or implementation changes follow
from these invocation failures. Both path decisions remain pending.

All three continuations launch successfully as turn5 and remain running at
the first collection checkpoint. The complete-and-clean SOW audit and diff
whitespace check pass after recording the terminal results and continuation
plan. Evidence: audit-review-timeout-continuations-1.log and
review-timeout-continuation-first-1.log. Their durable jobs continue; a later
terminal notification for this session must collect these new turn5 results.

### Completed Continuations And Repair Analysis - 2026-10-18

All three continuations return usable NEEDS CHANGES reviews: GLM in1404.4s,
Kimi in444.3s and DeepSeek in382.6s. Together with Minimax this gives four
technical reviews; Qwen and Mimo remain the two recorded coverage gaps.
No further automatic invocation is needed in this round. The reviewers
independently confirm the already reproduced native-path and glob gaps;
their differing P1/P2 labels do not change the pending user decisions.

New source-supported candidates are investigated before implementation:

- Display timestamps currently choose the first source-realtime value and
  accept all positive u64 values. Native selects the last C-string value,
  parses its native integer grammar, and requires0 < value <2^55. Existing
  last-value and C-string helpers can implement this without allocation;
  Rust's temporary value vector can be removed. Native deliberately ignores
  source-monotonic adjustment in commented code, so do not add that feature.
- Follow defaults combine pager-end and since with inconsistent precedence.
  Native chooses10 first when follow has no since, otherwise1000 for pager-end,
  otherwise all. Explicit lines/no-tail/cursor rules still apply. Resolve
  initial backlog and the all-history branch from the same selected policy.
- No-full rendering ignores actual stdout width when COLUMNS is absent or
  invalid. Reuse current native integer validation, then query stdout's size
  once per renderer using CLI-private platform APIs, then fall back to80.
  Existing x/sys and libc dependencies suffice; no SDK dependency change.
- Vacuum age uses only mtime. Native also considers access/change timestamps
  and the earlier available creation time, including Linux user.crtime_usec.
  Metadata reads belong only in explicit CLI maintenance. Inspect available
  platform metadata before adding calls; preserve active/non-journal handling
  and file identity checks. Header probes must preserve atime like native
  O_NOATIME on Linux. Validate retained/deleted files and metadata side effects
  on isolated synthetic copies, never host journals. Any measurable command
  cost needs disposition; SDK/query traversal remains unaffected.
- Rust trims facility list items where native preserves significant spaces.
  Retain native help/empty-item/number handling while removing the name trim.
  The unpublished CLI package description must also reflect native PCRE2.

Baseline inspection against25accee shows the source-realtime, vacuum-mtime
and Rust facility-trim patterns are inherited. The follow/pager and cached
width findings involve paths changed in this CLI SOW. Native references are
systemd/systemd @8cc40e0c5e9234bf45084751ac53b1fbfe70b492:
src/shared/logs-show.c:118-147,448-527; src/basic/terminal-util.c:1263-1308;
src/journal/journalctl.c:588-606,916-919;
src/libsystemd/sd-journal/journal-vacuum.c:59-101;
src/basic/xattr-util.c:419-477. Exact review results remain private in the
existing runner session and review-continuations-final-collection.log.

The existing full pre-implementation gate still applies. First reproduce
these candidates using the pinned oracle and unchanged binaries, then extend
the existing shared output/follow/utility controls and repair only confirmed
behavior. No new measurement framework, compiler minimum, public SDK API or
source/path contract choice is introduced by this independent repair batch.
Use the supported-floor CLI suites, affected complete matrices, practical
performance evidence and native PR CI after material changes. Repeat the
complete primary audit before a later whole-SOW review; do not use reviewers
as an iterative debugging loop. The two path choices remain independent and
unapproved. The color-mode P3 and deliberately non-stock RAW-name behavior
are still being checked against the actual contract before disposition.

### Reboot Checkpoint - 2026-10-18

The user explicitly requested a pause for a reboot. Implementation and review
work stop here. Preserve the working tree, including all untracked CLI platform
helpers. No commit, push, process termination or reboot is part of this pause.

- SDK SOW-0159 is completed in signed commit
  25accee6cbca3a14ce14cd4ec3eb92d34dab3681. The current repairs do not change
  SDK runtime code.
- CLI branch is fix/journalctl-v262-parity. Local and last recorded remote HEAD
  are 837873a7a297015b71b91d5ca0cb34be48707ff0; draft PR #12 is open. Its four
  native CI jobs passed before the latest uncommitted repairs. Those results
  do not validate the latest working tree.
- The latest uncommitted CLI repairs address source-realtime selection and
  parsing, follow backlog defaults, stdout terminal width, vacuum timestamp
  metadata and Linux atime preservation, Rust facility whitespace and package
  description. New Go terminal/vacuum metadata files and Rust terminal.rs and
  vacuum_metadata.rs are required parts of these repairs.
- All external-review continuations have finished. Four reviewers returned
  technical reviews; Qwen's HTTP429 and Mimo's malformed final output remain
  explicit coverage gaps. No further automatic retry remains in this round.
- The focused native comparison records 126 observations. Before repairs,
  there were 57 candidate differences; after repairs only the two forced-color
  differences remain. This fixes 55 observed differences, not a claim that all
  CLI acceptance criteria pass. Source timestamp, follow, terminal width,
  facility and vacuum probes now match the pinned v262 oracle in these cases.
- Supported-floor Go CLI race tests passed. Rust CLI tests passed with 21 unit
  and 11 subprocess tests; its final rebuild after formatting passed. Shared
  committed regression controls, complete matrix reruns, cross-platform CI,
  practical performance assessment, artifact updates and final review still
  need completion for these latest changes.
- Two user decisions remain unanswered: native Rust path support versus an
  explicit qualification, and native locale/brace file glob support versus an
  explicit qualification. Dependent design changes have not begun. Forced
  color output and invalid RAW-name behavior still need contract disposition.

Private restart evidence under .local/sow-0121-v262-krauk7l5/:

- recovered-review-red-1.log and recovered-probe-before-current.txt preserve
  the before-repair comparison and frozen binaries.
- recovered-review-after-1.log and recovered-probe-after-current.txt locate
  the latest comparison; each probe directory contains results.json.
- go-recovered-repairs-2.log, rust-recovered-repairs-1.log and
  rust-recovered-build-2.log record the supported-floor validation.
- review-continuations-final-collection.log contains the completed review
  collection. All current tests, builds and review workers have finished.

On resumption, inspect this checkpoint and the preserved git diff first. Add
focused regressions to the existing shared harnesses, resolve the outstanding
contract decisions, then complete proportionate validation and whole-SOW
review. Capture current PR CI before any later push. Keep the SDK completed;
do not close the CLI SOW or goal until all remaining gates are actually met.

### Resumption After Reboot - 2026-10-19

The user explicitly requested resumption. HEAD837873a7 and all recorded local
changes, including the untracked platform helpers, survived the reboot.
Continue the existing approved CLI repair batch and shared regression coverage.
The two outstanding path/glob design questions were presented again with
evidence, alternatives and implications; neither is approved by resumption
alone. No dependent SDK or glob design change begins without that decision.

### Resumed Regression Coverage And Vacuum Cost Analysis - 2026-10-19

Shared regression controls now cover source-realtime duplicates, native integer
grammar and bounds, facility item whitespace, stdout PTY widths, follow/pager
precedence, access time and legacy creation attributes. Frozen pre-repair CLI
binaries reproduce 50 differences in the 594-control output batch; the repaired
commands pass 594/594. Existing query/output/utility controls pass 604/604 and
the expanded follow batch passes 66/66. One new test initially misclassified a
space-prefixed numeric facility as invalid; native accepts it, so the test now
compares terminal success instead of implementation-specific help text.

Review of the pending atime repair also identifies a missing native fallback:
retry a header open without `O_NOATIME` when the first attempt fails, preserving
`O_NONBLOCK`/`O_NOFOLLOW`. This is corrected in both commands. Go 1.26.2 race tests
and Rust 1.91 unit/subprocess tests and rebuilds pass after that correction.
SDK runtime sources remain unchanged.

The practical whole-command comparison retains 1000 private archive names as
hardlinks to one synthetic journal, with matching floor binaries before/after,
seven alternating paired samples and no deletions. Go median changes from
17.566636 ms to 24.840289 ms; Rust from 14.978942 ms to 18.756488 ms. Native v262's
three-sample median is 52.503579 ms. The sibling/native comparison does not excuse
the observed regression. Preserve the adverse samples under the directory named
by resume-vacuum-cost-current.txt; do not accept the current unconditional
creation-metadata calls as meeting the performance requirement.

The repair follows from the existing native comparison contract, without a new
public API or relaxed result: cached filesystem timestamps can be read from
the existing metadata. Additional creation-time I/O is unnecessary when no
retention/size/count constraint can remove a nonempty candidate. When every
candidate has the same sequence ID, native sorting compares sequence numbers
alone; creation time can then affect only a nonzero retention cutoff, and a
cached timestamp already below that cutoff establishes eligibility without
another read. Mixed sequence IDs or unsequenced candidates still need complete
creation metadata when age can determine deletion order. Apply these checks
after collecting candidates, without retaining another metadata array or
adding an allocation. Add native controls for both same-ID sequence ordering
and mixed-ID creation-time ordering, then repeat the existing focused timing
comparison rather than adding a measurement framework.

Forced-color probing confirms effects beyond priority text: short/cat,
verbose, all JSON framings, grep highlights and boot/invocation headers. The
user was presented a third decision: complete native color behavior within
this SOW, or explicitly qualify and separately track plain-output delivery.
No color implementation begins before that answer. RAW invalid field-name
differences remain outside the existing stock-compatibility promise:
docs/API-Overview.md and docs/Writer-APIs.md explicitly exclude RAW field names
from that promise; do not expand the SDK contract implicitly.

### Validation Checkpoint And Remaining Decisions - 2026-10-19

Creation metadata now loads only when it can affect deletion or age eligibility,
using the existing native sequence-order rule. The new same-ID and mixed-ID
controls prove that the optimization preserves the right survivor. All 76
utility controls pass; the complete query/output/utility batch passes 608/608.
The unchanged frozen binaries reproduce six differences in the 16 new metadata
controls, including the mixed-ID creation-time ordering case. Supported-floor
Go race and Rust CLI tests/rebuilds pass after the optimization. Shared output
controls pass 594/594 again, both parser batches pass 134/134, portable smoke
passes 28/28, and the Go SDK compiles independently with CGO disabled.

The existing seven-pair retained-file control now measures Go 18.426862 ms
before versus 17.758796 ms after, and Rust 14.349798 ms versus 14.559356 ms.
The small adverse Rust median overlaps the individual sample ranges; this
does not establish timing equality. Review confirms no additional filesystem
I/O or allocation on that optimized path. Retain both the original adverse
results and optimized-results.json; do not discard the former comparison.

A separate equal-result control with 1000 recent archive names and
`--vacuum-time=90days` still needs creation metadata for every candidate. It
measures Go 17.653034 ms before versus 24.696577 ms after and Rust 14.837644 ms
versus 18.616836 ms after. recent-retention-results.json retains all seven
alternating pairs. These are warm metadata controls using private hardlinks,
not a general large-archive throughput claim. The remaining cost is real and
is not accepted under the existing no-regression requirement.

The user was presented decision 4 with the measured costs: continue optimizing
the CLI metadata/header path while preserving identity checks and atime
fallback (recommended under the strict gate), or explicitly accept only this
cold vacuum cost. No further filesystem-path redesign or performance exception
is inferred before the answer. The three preceding decisions remain native
Rust paths, native Unix file globbing and complete native color output versus
explicit qualifications. All four questions remain unanswered at this
checkpoint. Preserve completed independent work and do not start dependent
changes, a fresh whole-SOW review, a push or SOW closure before resolution.

Validation evidence under .local/sow-0121-v262-krauk7l5/: resume-v262-3.log,
resume-query-2.log, resume-follow-1.log, resume-parser-1.log, resume-go-2.log,
resume-rust-2.log, resume-sdk-pure-1.log, resume-smoke-1.log and
resume-metadata-red.log. Docs and product scope now describe the corrected
source timestamp, follow, terminal width and maintenance semantics. The wiki
structure check passes; executed-example results and the final local audit
are collected separately before reporting the checkpoint.

All 35 executed documentation examples pass in a new private workspace using
the existing repository caches; no older generated workspace was removed.
All 16 wiki pages validate. Rust/Go formatting, changed-Python syntax and diff
whitespace checks pass. resume-audit-1.log records the complete-and-clean SOW
audit verdict. The native CI result at837873a7 remains historical evidence,
not qualification of these local repairs. No commit, push or new review round
was started while the four decisions remain pending. All commands launched
for this checkpoint have finished.

### Decision Context Clarification - 2026-10-19

The user requested context for the four presented choices. No option was
selected by that request. Distinguish actual technical-boundary approvals from
work already required by the approved contract:

- Decision 1 asks for an additive native-path Rust facade extension inside the
  CLI SOW instead of rejecting valid native filenames. The published SDK scope
  and API surface are the approval boundary; no existing string API is removed.
- Decision 2 asks to extend the CLI-only native-call allowance beyond PCRE2 to
  platform filename expansion. SDK purity and system journal linkage rules
  remain unchanged. The alternative narrows file-pattern compatibility.
- Choice 3A implements the already approved full v262 output contract. Only
  choosing 3B would introduce a new plain-output qualification. A separate
  approval to repair the existing color parity gap is unnecessary.
- Choice 4A continues work required by the existing performance requirement.
  Only choosing 4B would authorize a new vacuum-only cost exception. No such
  exception is approved, and the measured slowdown remains an open finding.

The earlier blanket implementation hold on all four questions was too broad.
Withdraw the self-imposed approval holds on 3A and 4A; they do not represent new
user decisions. Decisions 1 and 2 still await explicit answers before their
dependent API/native-call changes. Keep the original questions and measurements
above as historical evidence; do not report any unanswered option as selected.

### Approved Path Scope And Vacuum Exception - 2026-10-19

After the contextual explanation and recommendation of 1A and 2A, the user
agreed and proposed accepting the small vacuum delta. Proceed with the two
recommended compatibility repairs and record that specific performance
exception. Native color completion remains part of the already approved full
v262 target. No further product decision remains for these four items.

The accepted exception is explicit CLI age-based vacuum maintenance requiring
creation metadata for recent archive candidates. The retained warm control
measures approximately 7.04 ms Go and 3.78 ms Rust extra for 1000 archive names.
These measurements are evidence for this case, not universal timing bounds.
Keep the already implemented conditional metadata reads. Do not expand the
exception to SDK reads/writes, ordinary CLI queries, or unnecessary I/O in
other maintenance cases. Additional vacuum-path redesign is unnecessary merely
to eliminate this accepted delta. Preserve the original adverse evidence.

Implementation plan within the existing pre-implementation gate:

- Add native-path facade constructors for automatic, file, directory and
  explicit-file-list opens, including existing options forms. Keep every old
  string signature and delegate through the same readers. Use borrowed native
  paths, preserve canonical deduplication and avoid allocations/canonicalization
  on a single explicit file. Go strings already preserve native Unix bytes.
- Rust journal filename and whole-file Zstandard suffix checks must inspect
  native encoded filename bytes. Reuse directory discovery, strict machine-ID
  subdirectory checks, staging and cleanup. Wire the CLI to native facade
  constructors without lossy string conversions.
- Confine Unix glob calls and thread-local locale selection to CLI modules,
  using the already available platform C library and native BRACE/NOCHECK/
  NOSORT behavior. Restore the calling thread's locale and free result storage
  on every exit. Preserve a literal-path fast path and existing deduplication.
  Keep file expansion separate from unit/text patterns. Windows uses an
  explicit Unicode filename contract. No new package or compiler minimum.
- Add shared native-oracle controls for raw parent paths, raw discovered
  basenames, replacement-named decoys, braces, bracket/range matching, dotfiles,
  escaping, no-match behavior and locale selection. Add focused SDK constructor
  coverage for raw filenames, directory/list forms, flags/options and whole-file
  compression; reuse existing fixture helpers.
- Document the additive SDK surface, CLI native-call allowance and accepted
  vacuum cost. Run supported-floor suites, affected native/shared matrices,
  independent pure SDK checks and ordinary native PR CI. Whole-SOW review
  remains after complete local implementation and validation.

### Native Filename Backend Consequence - 2026-10-19

The first new facade test fails before row iteration with "invalid filename".
The core path opener constructs a registry File through from_raw_path, whose
path.to_str gate rejects non-UTF8 paths. This is part of the approved native
reader-path blast radius; the facade and suffix changes alone are insufficient.
Evidence: native-path-sdk-1.log; journal-core/src/file/file.rs:568,592 and
journal-registry/src/repository/file.rs:263-285.

Preserve the approved source-compatible design: retain existing text accessors
and constructors, add a borrowed native as_path accessor, and keep an optional
boxed native path only for newly supported non-UTF8 inputs. Normal paths retain
their current strings and allocation count. Raw paths use an unambiguous
reserved NUL-prefixed, hex-encoded Unix path key; that prefix cannot identify
a valid ordinary filesystem path. Core I/O must use as_path. Legacy software
cannot accidentally open a replacement-named file from the encoded key.
The metadata serializer retains its original fields and normal-path bytes;
deserialization reconstructs native bytes from the reserved key and rejects
malformed native keys. Do not serialize an ignored optional native field.

Cache disposition: keep CACHE_VERSION at4 only if tests demonstrate unchanged
ordinary-file serialization and Hash input, and exact native-key round trips.
New raw paths were previously unsupported, and their encoded keys cannot alias
an ordinary valid path. No existing index contents or UTF8 cache keys change.
Do not force a broad cache invalidation for this additive path support. The
optional pointer adds one word to per-file metadata, not per-row iterators;
record that small storage consequence and retain the single-file facade's
removed Vec/HashSet/canonicalization work. No new dependency, compiler floor,
row traversal, decompression or allocation inside query iteration is required.

### Color Implementation Evidence - 2026-10-19

Native source inspection confirms that the approved color contract includes
priority/audit text, first-match grep spans, verbose trusted fields and cursors,
JSON primitives, boot separators, and table headers. The implementation reuses
the existing renderers and PCRE2 match, selects color policy once per output
command, and adds formatting only when that policy enables colors. Buffered
reverse output retains spans only for colored grep; plain output retains its
existing entry buffers and does not repeat matching. Native tab/ANSI offset
adjustment and multiline/ellipsization rules are the oracle, including their
observable edge cases. Shared oracle controls cover environment selection,
priority levels, rendering modes, binary values, multiline and buffered grep.
No SDK parsing, indexing, compression, or query algorithm changes are needed.

### Native Filename And Rendering Validation - 2026-10-19

Implementation and blast radius:

- Additive Rust facade native-path constructors replace the CLI's lossy path
  conversions. String constructors retain their signatures and now share the
  single-file fast path before canonical-deduplication storage. Native journal
  and whole-file Zstandard suffixes survive directory discovery. Core physical
  file opens use the new borrowed registry File::as_path accessor.
- Metadata retains its three serialized fields and ordinary-file hash input.
  New raw Unix paths use the documented reserved key and reconstruct their
  original bytes on deserialization. Engine cache identity, serialized bytes,
  map lookup, malformed-key rejection and replacement-name separation pass.
  CACHE_VERSION remains 4 for these demonstrated compatibility reasons.
- Unix CLI globbing uses the platform C library, restores its temporary
  thread-local locale and frees result storage. Literal paths bypass it.
  Windows retains Unicode paths with brace alternatives and wildcard matching.
  Raw header output and vacuum filename tie ordering retain original bytes.
- Color policy follows fixed, boolean and automatic modes, both stdout and
  stderr terminal state, absent versus empty TERM, NO_COLOR and COLORTERM.
  Linux-console headers remain plain. Short/cat priority and audit colors,
  verbose timestamp/cursor/field styles, JSON primitives and boot separators
  use the existing renderers. Buffered colored grep retains its own spans from
  the existing PCRE2 call; it does not repeat the match. Plain matches do not
  allocate an output offset buffer or store per-row highlight coordinates.
- Embedded DOCUMENTATION and CONFIG_FILE links use SYSTEMD_URLIFY. Optional
  file links read command-local hostname lazily; no SDK code observes host
  identity. Native's implicit host-catalog link fallback stays outside portable
  mode under the existing parity-matrix catalog rule. The boot-color control
  uses synthetic messages without catalog IDs so host databases cannot affect
  its expected output.
- New controls also exposed pre-existing verbose continuation indentation and
  negative short-delta differences on reverse output. Both implementations now
  use native multiline indentation and saturating timestamp subtraction.
  Cat projection order is normalized only for a one-row, single-line control:
  native output-fields is an unordered set. Other cat controls compare complete
  raw bytes, including multiline, NUL and repeated MESSAGE values.

Reference evidence: systemd/systemd @
8cc40e0c5e9234bf45084751ac53b1fbfe70b492;
src/basic/ansi-color.c, src/basic/terminal-util.c:1453,1844,
src/basic/string-util.c:704, src/shared/logs-show.c:208,533,790,1361,
src/shared/pretty-print.c:59,98,155 and
src/libsystemd/sd-journal/journal-vacuum.c:35.

Supported-floor and executable evidence, retained under the private CLI
validation workspace:

- Go 1.26.2 CLI race suite passes; Rust 1.91 CLI passes 23 unit and 11
  subprocess tests. Native-glob brace/Unicode/dotfile controls are included.
- Rust SDK passes all 201 library tests; registry passes 13 tests plus its
  documentation test. The five engine cache tests pass, including both new
  compatibility controls. Go's complete SDK package compilation passes with
  CGO disabled independently from the CLI build.
- Expanded v262 oracle controls pass all 906 observations and cover native filenames, raw header output,
  locale/glob rules, every renderer, colors, embedded links, grep spans,
  terminal policy and the previous parser/query controls. A later retained
  run replaces the over-strict unordered cat-projection expectation; this is
  a test correction, not a changed product ordering guarantee.
- Existing query/output/utility controls pass 610 observations, including raw
  vacuum tie ordering. Follow passes 66. Parser parity passes 134 cases per
  language. Native portable smoke now passes 46 controls and is wired through
  the existing four-OS workflow. This local smoke run does not certify a new
  remote native build.
- All 35 executable documentation examples pass. All 16 wiki pages validate.
  Changed Go/Rust formatting and Python syntax checks pass. Remaining final
  audit/review/CI evidence is recorded after its actual completion.

Artifact maintenance: AGENTS records the approved CLI native-call exception;
the journal compatibility runtime skill records filename/color controls and
plain-output discipline; product scope and the CLI parity matrix record the
current behavior and cache disposition. Journalctl and Reader APIs wiki pages
document native filenames, additive constructors, low-level path access,
colors/links and the accepted vacuum-only cost. No output/reference skill
exists or needs a new artifact. Both SOW status indexes reflect this state.
The original SDK SOW remains completed; this explicitly approved additive
native-path blast radius belongs to the current CLI SOW.

Same-failure search covers facade opens, directory and compression suffixes,
core physical file operations, native header output, vacuum tie ordering,
streamed and buffered highlights, short-family variants, cat, verbose, all
JSON framings, tables and separators. Existing log-writer repository paths use
the strict UTF8 registry constructors and retain that contract. No SDK row
traversal, index population, journal bytes, compiler minimum or dependency
version changes are mixed into this batch. Added durable evidence uses
synthetic data and relative references; raw command output remains private.

Lessons: filename bytes must survive the complete opener stack; stable cache
serialization and hash identity are separate compatibility checks; test native
terminal policy with both descriptors; retain match offsets with buffered
entries; distinguish unordered projection order from value correctness; and
keep optional host presentation helpers outside SDK runtime code.

Follow-up mapping is unchanged: indexed invocation ranges and broad polling
scalability remain SOW-0122. Reopened SOW-0045/0071 and pending
SOW-0153/0156/0160/0161 remain separate. These repairs are implemented here;
no new deferred implementation item or product decision is created.

### Final Portability And Plain-Output Audit - 2026-10-19

The primary audit identified three remaining consequences inside the approved
filename/rendering scope before final review:

- Windows Go file selection used filepath.Glob, while Rust's existing Unicode
  matcher accepts native-style negated classes and literal unmatched brackets.
  Reuse the existing matching grammar through a small component walker for Go,
  with the same brace and dotfile behavior. Unix still uses libc glob. Preserve
  drive/root prefixes and literal-path bypasses; add shared native smoke cases
  and focused fallback tests instead of a new glob dependency.
- Rust's optional file-link hostname used the Windows NetBIOS accessor, while
  Go 1.26.2 os.Hostname uses ComputerNamePhysicalDnsHostname. Match that Windows
  accessor and its grow-buffer protocol. Relative links must also preserve a
  valid logical PWD, matching the native get_current_dir_name path and Go's
  existing Getwd behavior. Verify with an isolated symlink cwd control. This
  remains CLI-only optional presentation logic.
- Inspection of the supported-floor development binary found a separate call
  to append_cat_field on the plain Rust cat path. Keep the original byte copy
  inline and the plain Pattern::matches body unchanged. Add the offset getter
  only after successful colored matches; avoid unconditional highlight stores.
  The production-profile comparison uses the retained matching compiler and
  profile. Preserve development timing outliers; they are not a production
  equality claim and do not authorize a query slowdown.

These changes implement the approved contract and remove inconsistent or
unnecessary work. They do not introduce a new product decision, dependency,
compiler floor, SDK host-observation path or measurement framework.

### Final Local Performance And Readiness Disposition - 2026-10-19

The final primary validation pass is complete. Supported-floor CLI suites pass,
the latest expanded oracle matrix passes 906/906, and portable smoke passes
46/46 including the Windows fallback's negated classes and literal unmatched
brackets. The logical-PWD link control also passes against native journalctl.
The earlier 610 query/output/utility, 66 follow and 134 parser cases per
language remain valid; their query, follow and parser algorithms did not change
in the final presentation/fallback refinements. All 35 documented examples and
16 wiki pages pass. Rust SDK/registry/cache and independent pure Go SDK evidence
remain as recorded above.

Performance evidence follows the user's engineering-judgment rule. Reuse the
existing 100000-entry fixture and small alternating whole-command comparisons;
do not create a new benchmark framework. Both compiler-matched production Rust
binaries use Rust 1.97.1, release opt3, fat LTO and one codegen unit. Identical
output hashes cover both full scans and a 75000-result grep. Median scan time
is 318.0ms before / 314.5ms after; grep is 304.7ms / 309.1ms. Sample ranges
overlap and paired results vary in both directions. Preserve that small
positive grep median and all development-profile outliers; these intervals do
not establish timing equality or a reproducible slowdown. Go's matched-floor
grep control has median wall time 674.5ms / 660.8ms and child CPU 964.2ms /
906.0ms. These controls are bounded observations, not universal guarantees.

The engineering disposition rests on tracing the work as well as these
controls: SDK row/index/decompression paths do not change; native path metadata
adds no per-row allocation; ordinary literal opens avoid extra facade
canonicalization/storage; color policy is selected once; plain Rust cat keeps
its byte copy inline; plain regex matching retains its original body; Go's
uncaptured matches allocate no offset buffer; and buffered highlight storage is
enabled only for colored/linked grep. No second regex call, extra row scan,
DATA expansion or filesystem operation is introduced into ordinary query
iteration. No query slowdown allowance is granted. The accepted explicit
startup and age-based vacuum allowances retain their original narrow scope.
No further measurement work is needed merely to force noisy medians below 1.0.

Same-failure, artifact, source-boundary and synthetic-data checks are complete.
Final SOW audit, whole-SOW review under standing authorization, ordinary PR CI
and coherent signed lifecycle closure remain. Local readiness does not claim
that the older remote native results certify the new helpers.

Final local SOW audit reports "SOW initialization complete and clean" in
native-colors-audit-final-1.log. Whitespace, changed-language formatting and
Python syntax checks pass. The remaining follow-up words refer to implemented
work, superseded historical checkpoints, or the existing real SOW files mapped
above. No new unresolved product decision remains. Proceed to the authorized
whole-SOW static review; fresh native PR CI and lifecycle closure follow it.

### Whole-SOW Collection And Native Qualification - 2026-10-19

The 97-file candidate matched the frozen review snapshot and was signed/pushed
as 1f84cca1 for ordinary draft PR #12 qualification. This changed no reviewed
file contents. Native run 39263286705 passes Linux and Windows; macOS and
FreeBSD both fail Rust compilation at file_glob.rs:64 because libc 0.2.189
does not export GLOB_BRACE for those targets. Their Go command tests pass.
The earlier four-platform success at 837873a7 does not qualify this candidate.

Review round 0ee095a6bd15499ab664a7633a7c140d is collected:

- MiniMax returns PRODUCTION GRADE. Its five P3 notes cover fresh native CI,
  review coverage, the existing internal analysis-tool error, accepted startup
  cost and approved CLI-only host presentation calls. These are existing gate
  tasks or documented allowances, not additional code requirements. Its
  assertion that the round has zero usable verdicts is stale; the collected
  MiniMax and DeepSeek reports are usable technical verdicts.
- DeepSeek returns NEEDS CHANGES. The macOS/FreeBSD binding omission is
  independently reproduced by native CI. Its no-match unit-glob finding is
  independently reproduced in both commands against the pinned v262 oracle.
  Both are required repairs within this SOW's approved contract.
- DeepSeek's Rust forward-count finding is rejected: render_matching_entry
  ignores render_entry's presentation boolean and returns true after a
  grep-selected entry. Frozen binaries match stock for excluded +2 head rows
  and a +1 head entry without MESSAGE, including output and exit status.
  Neither a counter change nor a speculative counting repair is justified.
- GLM times out twice without a verdict. Mimo, Kimi and Qwen each return
  HTTP429 on the original invocation and its single retry. These four gaps
  supply no verdict. No further retry of this unchanged candidate is allowed.

The unit-glob mismatch predates this repair program: SDK base 25accee contains
both addImpossibleMatch/add_impossible_match helpers and their unit-glob
callers. With `--unit=no-such-*` or `--user-unit=no-such-*`, stock exits 1 with
"Failed to add filter for units: No data available" and no output; both CLIs
exit 0 and print the no-entries marker. An absent exact unit correctly exits
0 and must retain that behavior. The matchable sentinel field is unnecessary.

Quality run 39263286606 reports two Bandit_B108 findings at the color fixture's
literal MESSAGE/CONFIG_FILE values. These are synthetic log data, not temporary
file operations. Actual fixture paths use private repository scratch. Replace
the two payload literals with the standard /var/log/journal-sdk example prefix
without weakening the absolute file-link assertion. The separate pre-existing
internal tool error remains a coverage limitation; do not infer complete
analysis from zero reported findings or expand this PR into tool maintenance.
Raw reports and executable reproduction results remain under .local/.

### Pre-Implementation Repair Gate - 2026-10-19

Problem and evidence: the Unix adapter assumed every libc binding exported a
non-POSIX flag; the unit filter used a synthetic match for an empty wildcard
expansion instead of native ENODATA behavior. Review current adapter/filter
callers, pinned v262 journalctl-filter.c:105-198, the locked dependency's target
modules, native CI logs and frozen-binary oracle reproductions. The earlier
primary audit omitted native binding qualification and empty unit expansions;
the internal audit must cover those surfaces before another whole-SOW review.

Platform reference evidence:

- apple-oss-distributions/Libc @ 71bbe350ab79eef58113991d817ccc6165061a64,
  include/glob.h:104 defines GLOB_BRACE as 0x0080.
- freebsd/freebsd-src @ f19009020a33d07d7cad9393b4bf0a36f267cd07,
  include/glob.h:94 defines the same BSD flag value.
- systemd/systemd @ 8cc40e0c5e9234bf45084751ac53b1fbfe70b492,
  src/journal/journalctl-filter.c:195 returns ENODATA when neither exact units
  nor wildcard expansion added a unit.

Affected surfaces and existing patterns: Rust CLI Unix glob flags, both CLI
unit-filter dispatchers and the shared oracle/portable smoke controls. Reuse
the approved native libc glob/locale path, ordinary command error handling,
FIELD/DATA unique enumeration and existing shared fixture writers. The SDK,
journal bytes, cache representation and compiler floors do not change.

Risk and implementation: supply the BSD flag only for macOS/FreeBSD and retain
the GNU Linux binding. Remove the two sentinel helpers and return the native
no-data diagnostic only when neither system nor user unit expansion adds a
match. Preserve exact-unit and mixed exact/glob behavior. Replace only the two
synthetic color-fixture literals. No new dependency or runtime algorithm is
needed. Other Linux C-library profiles are not qualified by the four native
jobs; this repair does not claim new packaging coverage for them.

Validation: supported-floor CLI suites; shared oracle cases for system/user
empty wildcards, combined empty wildcards, zero-line selection, exact absent
units, mixed exact/glob selection and a real sentinel-valued field; ordinary
portable smoke negatives; existing query/follow/parser controls; fresh native
Linux/macOS/Windows/FreeBSD CI and quality output. Compare error status, class
and absence of row output. No extra benchmark is warranted: the flag is a
compile-time constant and no-match unit selection exits earlier; successful
unit enumeration and query iteration retain the same work.

Artifact and data plan: update the CLI parity/spec record, both status indexes
and this SOW with actual validation/review outcomes. Existing runtime-skill
rules already require native OS qualification; no new workflow or consumer API
is introduced. Use synthetic fields/paths only and retain raw logs privately.
Open decisions: none; these repairs deliver the approved native platform and
stock CLI behavior. Closure remains blocked until validation and review pass.

### Native And Unit-Filter Repair Checkpoint - 2026-10-19

The BSD constant adapter and paired native no-data error are implemented.
The sentinel helpers have no remaining runtime callers and are removed;
the sentinel name remains only in the synthetic regression fixture. Exact
unit selectors and successful wildcard expansion retain their indexed path.
The two absolute link fixture payloads use the project example prefix.

New shared controls reproduce eight failures on frozen 1f84cca1 binaries:
system/user/both empty wildcard selections and the zero-line case, in both
languages. All 924 comparisons pass on the repaired binaries, including the
ten exact/mixed-selector controls. Error comparisons also retain complete
stdout. All 52 portable smoke observations pass; the printed total was
updated with the three new cases per language. Supported-floor Go CLI race
tests and Rust CLI's 23 unit/11 subprocess tests pass. Parser parity passes
134 cases per language, query/output/utility passes 610, and all 16 wiki
pages validate. The timed follow batch is still running at this checkpoint.

The source/performance audit finds no new successful-query work: no SDK
reader/index/decompression change, no additional unit-value enumeration, no
extra regex evaluation and no row-path allocation. GNU Linux uses the same
constant; BSD uses its header's compile-time constant. Empty wildcard queries
return before the former synthetic filter/query path. No new timing exception
or measurement framework is warranted. Existing performance evidence and the
user's narrow startup/vacuum allowances retain their scope.

Same-failure inspection covers both system/user selectors, combined selectors,
exact absent names, zero limits, the sentinel field, both Unix binding targets
and both command error handlers. Metadata/spec/docs/index updates match the
implemented behavior. No compiler, dependency, SDK API, cache or journal-byte
change is added by this repair. Sensitive-data checks cover only synthetic
fixtures and sanitized summaries. Native CI and fresh whole-SOW review remain
gates; no completion is claimed from local success alone.

Repair pre-push audit reports "SOW initialization complete and clean" in
native-platform-fix-audit.log. Whitespace and changed-language syntax/format
checks pass. The PR's prior three failed checks and two open analysis threads
were captured before pushing; both native failures share the repaired binding
omission and both analysis findings concern the replaced fixture literals.

### Follow Collection And Focused Rechecks - 2026-10-19

The repeated follow batch records 62 passing observations and four failures.
All four failures originate in the synthetic live writer's readiness/completion
deadlines: Go boot-event output is exactly expected but the writer is stopped
at its deadline; stock sees only the first append before the writer deadline;
and the Rust live-append and Go cursor cases report writer-not-ready. A
read-only inspection of the task's exact log-owning process finds its current
fixture-writer child waiting for a filesystem log commit. This is evidence of
fixture I/O delay, not proof of a host-wide cause or a CLI regression.

Retain the full failed batch. Recheck only its four failed reader/case pairs,
using the existing case table and run_follow_reader helper in fresh private
directories. All four pass without source, binary, timeout or expectation
changes, including stock's complete three-append output. The evidence is
62 initial passes plus four focused passes; do not mislabel the original run
as 66/66. No test timeout is relaxed and no broad rerun is needed.

Both new analysis threads are answered individually and resolved after signed
repair commit 833e635a. The normal native run is 39267869667. Its outcomes and
the fresh whole-SOW review are recorded after collection, before closure.

### Native Qualification And Final Primary Audit - 2026-10-19

Ordinary PR native run 39267869667 succeeds on signed source 833e635a:
Linux/glibc, macOS ARM64, Windows CLANG64/GNU LLVM and FreeBSD 14.4 all build
both commands and pass the 52 portable smoke observations. Pure Go SDK builds
remain independent. This qualifies the repaired BSD flag on both affected
platforms; no SDK compiler minimum changes. Other Linux C-library profiles are
outside this native qualification and no new packaging profile is claimed.
The earlier decision excluded developing separate static native profiles.

Quality run 39267869602 confirms zero Bandit findings after the fixture-payload
change, while retaining the known internal tool error. Its sole current
finding is Markdown MD037 on the newly written wildcard example in this SOW.
The example is corrected to inline code. The final metadata push must obtain
the actual green quality result and clear the stale prior alert instances;
the current red check is not represented as passed.

The primary audit and executable validation are complete for the repaired
runtime: 924 oracle comparisons; 610 query/output/utility controls; 52 native
smoke observations per OS; 134 parser cases per language; supported-floor Go
race and Rust unit/subprocess suites; 62 first-run follow passes plus four
focused passes, with original fixture-writer timeouts preserved. Prior SDK,
registry/cache, pure-Go and 35 executable-documentation evidence remains
applicable because those paths/examples did not change in this repair. All
16 wiki pages and changed-language syntax/format checks pass.

The whole changed source and direct/indirect consumers were re-inspected after
the verified findings, including successful/empty/mixed unit paths, cursor and
zero-limit error timing, sentinel removal, Unix binding flags, platform build
separation, rendering, indexed access and error cleanup. No additional
blocking code defect was established. Complete this meaningful batch with a
fresh whole-SOW review under standing authorization. Keep the source frozen
during collection; do not repeat reviews merely for individual metadata edits.

The final primary audit again reports "SOW initialization complete and clean"
in native-platform-fix-audit-final.log. No unresolved product decision remains.
The long-term-best ship recommendation remains to retain both commands under
the shared parity suite. Both now have four-OS native evidence; the historical
Go-first recommendation's broader-platform-evidence rationale is superseded.
This does not select a new packaging default or authorize a release. Existing
large-archive/polling work stays SOW-0122; the mapped paused/pending SDK work
remains separate. No additional implementation is deferred from this repair.

### Whole-Unit And Invocation Audit - 2026-10-19

The repaired candidate 2b689a98 has clean ordinary PR checks, all four native
jobs and zero open PR analysis alerts. Both fixture-string threads were answered
and resolved individually. The quality tool's pre-existing internal error
remains a coverage limitation despite its green workflow and zero findings.

The next whole-SOW review round is collected: MiniMax returns PRODUCTION GRADE;
DeepSeek returns NEEDS CHANGES; GLM reaches its 1800-second limit without a
verdict; Mimo, Kimi and Qwen each return HTTP429 twice. Do not retry this known
incomplete candidate. MiniMax's old remote-quality and prior-review-count notes
are stale. Its approved CLI-helper note is not a defect. DeepSeek's diagnostic
prefix note is nonblocking under the existing error-class comparison contract.
The unqualified musl/uClibc profile remains outside the accepted native matrix.

DeepSeek's two P2 findings reproduce in both commands against the pinned v262
oracle: suffixless/partial-suffix unit patterns are incorrectly given a service
suffix, and invocation unit discovery searches the full show-only field set.
The preceding primary audit was too narrow around units. Stop the review cycle
and audit the complete component before another review.

Confirmed direct/indirect failures on frozen binaries:

- `--unit=app-*` misses the fixture's slice, and `--unit=alpha.*` misses its
  service. POSIX character-class patterns also fail. Exact empty selectors
  incorrectly succeed. Native path, escaped and raw-byte unit arguments fail
  to select their corresponding canonical unit names.
- Current-user conversion does not apply native duplicate removal when
  system-unit selectors are moved to the user-unit list. A duplicate selector
  therefore incorrectly makes invocation context appear to contain two units.
  The separate system/user flag combination was checked and is not a new
  last-setter repair: native combines those journal-type bits.
- Both wildcard and exact invocation unit selection include coredump/slice
  groups that native invocation discovery excludes. Normal show filtering must
  retain those full groups.
- Invocation first/last timestamps are restricted to the unit-discovery rows;
  native bounds include all boot-scoped rows carrying that invocation ID.
  A three-row fixture has native bounds T..T+2 but both CLIs report T+1..T+1.
- A row containing two invocation-ID fields creates an extra table entry.
  Native chooses the first valid nonzero ID in its documented field order.
  Explicit ID selection without an offset must still match any ID field.
- Invocation show queries ignore identifier and other non-unit filters.
  Explicit-ID boot validation was checked separately and already agrees with
  native; do not add a speculative boot-validation repair.
- Relative invocation follow changes its selected ID after another invocation
  is appended. Native retains the initially resolved ID. The private live
  control reproduces this in Rust with a normally completed writer and in Go
  alongside a separately recorded writer deadline.

These are CLI correctness gaps inside the approved v262 contract. They do not
reopen SDK SOW-0159. Reference: systemd/systemd @
8cc40e0c5e9234bf45084751ac53b1fbfe70b492;
src/basic/unit-name.c:676-826, src/basic/path-util.c:355-429,
src/journal/journalctl-util.c:131-239,
src/journal/journalctl-util.h:9-27,
src/journal/journalctl-filter.c:429-481 and
src/shared/logs-show.c:1733-1819,1948-2363.
Raw oracle outputs and synthetic fixtures remain under .local/.

### Pre-Implementation Gate - Unit And Invocation Repair - 2026-10-19

Problem/root cause: simplified name conversion, shared full unit fields/matches,
global unique-ID enumeration plus separately unit-restricted bounds, and
repeated invocation resolution do not implement native unit/invocation state.
The SDK's indexed matching, metadata getters and cursor seeks are available;
replacing them with row expansion or an unbounded journal scan is prohibited.

Evidence and existing patterns: inspect the complete native routines cited
above, both CLI callers, SDK cursor/reset behavior, current shared query/follow
builders and frozen oracle reproductions. Reuse the approved CLI-native
globbing/locale pattern on Unix and the Unicode fallback on Windows. Reuse
existing SipHash primitives for long native unit path names where accessible;
keep any necessary Go helper private to the CLI. SDK public APIs need no new
capability or native dependency.

Implementation plan:

1. Normalize unit arguments once per command with native suffix/glob, byte
   escaping, path conversion, empty/error and quiet-warning behavior. Preserve
   raw Unix argument bytes and native user-list duplicate semantics. Cache
   only command-local immutable selection results; do not change parser
   terminal-action precedence or SDK APIs.
2. Match unit wildcards with native NOESCAPE behavior and proper character
   classes; compile/prepare once outside unique-value callbacks. Enumerate
   FIELD/DATA indexes, truncate C-string values where native does, and retain
   literal fast paths.
3. Separate full show unit groups from invocation's base field/group set.
   Keep the native current-UID rule. Apply non-unit show filters regardless
   of whether invocation selection replaced boot/unit show filters.
4. Replace the unique-ID/sort/bounds approximation with native-direction
   invocation discovery and indexed jumps. Use first-valid ID precedence,
   boot-scoped global ID bounds, explicit-reference offsets, duplicate/broken
   ID termination and bounded selection. Reuse readers and metadata getters;
   use a private Go CLI adapter for existing facade function calls rather
   than adding a public SDK API. Account explicitly for SDK match resets and
   cursor positioning instead of assuming native internal cursor state.
5. Preserve performance on sparse-ID journals by deriving an indexed valid-ID
   eligibility predicate from FIELD/DATA values before discovery. Do not scan
   unrelated rows or expand MESSAGE fields. Resolve the invocation once for
   follow, matching native state.

Risk/blast radius: both unit show paths, invocation tables/relative selectors,
follow selection, unit-pattern helpers and shared regression fixtures. SDK
source, journal bytes, recovery policy, cache schema, compiler floors and
dependency versions remain unchanged. Rust may promote its existing
journal-core test dependency to a runtime dependency to reuse the already
linked native-name hash primitive; this introduces no new package/version.

Validation plan: shared native-oracle fixtures for every confirmed failure;
unit naming boundaries, raw/Unicode names, long paths, glob classes and mixed
selectors; full/base groups; invocation ID precedence, overlap, bounds,
explicit/relative references and non-unit filters; a focused invocation-follow
control; supported-floor suites and existing full parser/query/follow controls.
Use representative whole-command timing and work tracing for changed query
work, including sparse-ID and many-ID inputs. No large measurement framework
or broader performance envelope is needed. Repeat complete primary validation
and whole-SOW review only after the cohesive repair is ready.

Artifact/sensitive-data plan: update the parity/product specs, CLI docs, runtime
compatibility guidance and both status indexes. Use synthetic identities and
metadata; unit path conversion performs no host filesystem discovery. Raw logs
stay private. Reconcile SOW-0122's stale invocation description before coding:
required correctness and no-regression work remains here; large-input envelope
and broad polling work stay pending, without activating another SOW.
Open product decisions: none; native behavior and the performance/purity
constraints are already approved. This gate does not authorize unrelated
features, packaging profiles or release work.

### Unit/Invocation Implementation Evidence - 2026-10-19

The first 290 new oracle observations reproduce 176 failures in the retained
2b689a98 binaries. The first repair reduces these to ten; the remaining failures
identify the native long-name hash's trailing NUL and broken-ID cursor restore.
After those repairs and expanded broken-ID/raw-field controls, all 348
observations pass. Keep both adverse result sets. Additional naming boundaries,
fallback matcher and live controls are included in final validation below.

The complete native traversal uses forward discovery for unlimited tables,
forward for oldest limits, and backward for newest limits. Bounds use the
boot-scoped ID predicate without unit restrictions. Broken-ID recovery restores
the reference boundary and temporarily clears unit/boot discovery matches,
matching the upstream routine, with local broken-ID and global duplicate guards.
The indexed eligibility predicate remains active to skip rows without valid ID
fields. Two readers are reused; metadata getters never expand MESSAGE data.
Sparse/no-ID inputs therefore do not become full row-expansion scans.

The live audit also proves ordinary unit wildcard follow must retain its initial
expansion. The pinned v262 reader outputs initial-alpha/late-alpha; both
provisional commands additionally output new-beta. Expanded unit matches are
now cached once per command, alongside the once-resolved invocation ID.
This corrects behavior and avoids repeated FIELD-chain expansion during polling.
The original private follow probe accidentally selected the host oracle through
an old command-builder helper; exclude it from acceptance evidence. The three
shared command builders now honor an explicitly supplied stock_journalctl.
The repeated pinned probe confirms the wildcard failure and retains its
independent fixture-writer timeouts. No deadline is increased.

Unix unit wildcard matching uses native fnmatch with NOESCAPE in a temporary
thread locale, prepared once per expansion. Both commands preserve raw selected
unit values. Windows uses a once-compiled byte matcher with POSIX character
classes, C-locale single-character collating elements, literal brackets and
literal backslashes; dedicated tests exercise this fallback on Linux too.
Normal unit names are mangled to ASCII; raw payload bytes retain byte matching.
Rust reuses the existing core SipHash primitive, including the native trailing
NUL; Go's small CLI-private primitive introduces no SDK API or dependency.

Same-pattern audit additionally covers empty selectors, suffix-only and invalid
prefix names, oversized names, path simplification/hash boundaries, user-list
deduplication, ID precedence, overlapping/nested ranges, explicit secondary-ID
offsets, retained non-unit filters and follow state. The already disproved
line-count and system/user last-setter reports remain rejected. SDK source and
format contracts are unchanged. SOW-0122 is reconciled and remains pending.

### Final Unit/Invocation Validation - 2026-10-20

The retained candidate `unit-repair-6/bin` passes 1,364/1,364 pinned v262
comparisons, 610/610 existing query/output/utility controls, 134 parser cases
per language and 52/52 portable smoke controls. Supported-floor Go CLI race
tests and Rust CLI tests (24 unit and 11 subprocess cases) pass. Evidence under
`.local/sow-0121-v262-krauk7l5/`: `unit-repair-final-6-v262.json`,
`unit-repair-final-6-query.log`, `unit-repair-final-6-parser.log`,
`unit-repair-final-6-smoke.log`, `unit-repair-final-go-race.log` and
`unit-repair-final-rust-tests.log`. The final zero-line invocation-table shortcut
validates source/unit/boot before returning; it skips ID enumeration and reader
setup that cannot contribute any requested output. Ordinary queries without
unit/invocation selectors retain immediate fast exits.

The full follow batch has 70 initial passes and two fixture-writer readiness
timeouts before either CLI starts. Both unchanged focused rechecks pass; no
deadline or expectation was changed. Preserve `unit-repair-follow-first.json`,
`unit-repair-follow-full.log`, and `unit-repair-follow-recheck.json`. The six
new pinned-oracle frozen-invocation/frozen-unit-glob observations pass in
`unit-repair-follow-after.json`. Subsequent changes affect raw warning output,
empty-selector/no-ID shortcuts and zero-line tables, not the exercised live
selection/cursor paths; the full follow batch is not repeated for those edits.

Pure Go SDK checks pass (`unit-repair-pure-sdk.log`). All 35 executable docs
examples pass (`unit-repair-docs.log`), and all 16 wiki pages validate
(`unit-repair-wiki-final.log`). SDK code and contracts are unchanged by this
repair. Earlier native CI qualifies the preceding source only; ordinary PR
native CI must qualify this source before closure. Fresh whole-SOW review is
also pending; previous provider failures do not count as technical verdicts.

Performance evidence uses existing whole-command controls, equivalent outputs,
supported-floor matched build profiles and five alternating pairs. The retained
`invocation-cost-m3v_wqu3/results.json` records baseline/candidate medians:

| Control | Go baseline -> candidate | Rust baseline -> candidate | Stock v262 |
| --- | --- | --- | --- |
| 64 IDs / 4,096 rows, full table | 56.65 -> 10.50 ms | 74.69 -> 19.47 ms | 4.34 ms |
| Same fixture, oldest one | 61.65 -> 7.76 ms | 69.24 -> 4.68 ms | 2.79 ms |
| Two sparse IDs / 20,000 rows | 11.97 -> 7.49 ms | 13.02 -> 8.44 ms | 28.75 ms |
| No IDs / 20,000 rows, initial | 5.81 -> 6.47 ms | 4.66 -> 4.49 ms | 32.82 ms |

The initial Go no-ID +0.67 ms observation is retained and investigated, not
waived. The absence shortcut avoids a second reader and predicates; syscall
counts fall from 12 to 8 pread64 calls and 13 to 9 fstat calls. Nine alternating
focused pairs yield 5.633 -> 5.658 ms with mixed directions; the later final
comparison yields 7.355 -> 6.510 ms (CPU 7.070 -> 6.186 ms). The small initial
increase is not reproducible. Evidence: `unit-repair-no-id-focused.json`,
`unit-repair-no-id-final.json`, and `unit-repair-no-id-{before,after}.strace`.
This is engineering judgment supported by work tracing and representative
measurements, not a claim that overlapping intervals prove exact equality.
The captured ru_maxrss includes the Python parent's fork peak and is excluded
from application memory/allocation claims. Ownership review instead confirms
two reused readers, metadata-only access, indexed eligibility, prepared matchers
and once-only follow expansion. The zero-line shortcut only removes work from
the measured implementation; no further timing framework is required.

Artifact maintenance: both status indexes, parity/product specs, CLI docs,
compatibility skill and pending SOW-0122 are updated. AGENTS.md needs no change:
the approved scope, purity and evidence policies already cover this repair.
There are no output/operator skills to update. Sensitive-data checks use
synthetic fixtures and sanitized durable evidence; detailed logs remain private.
Same-failure coverage includes naming boundaries, raw bytes, fallback matching,
ID precedence, global bounds, broken IDs, equal timestamps, offsets, non-unit
filters and frozen follow selection. Broader performance-envelope work remains
explicitly tracked by pending SOW-0122; no new implementation SOW is activated.

### Native CI And Final Review Progress - 2026-10-20

Signed candidate 587981b7ac5925f6f29946d8ccd416a16667b3ae is pushed to draft
PR 12. Native run 39401925131 passes Linux, macOS, Windows and FreeBSD.
Coverage test generation/upload, documentation examples and CodeQL complete.
All 16 existing review threads remain resolved. The full CI snapshot is retained
as `unit-repair-ci-completed-587981b.json` in the private validation workspace.
Codacy's sole Markdown finding is an extra blank line before the final evidence
section; the local correction removes only that line. The pre-existing Bandit
internal error remains a separate analysis coverage limitation.

Codacy Coverage Variation reports -1.16 percentage points against a -1.0 limit.
The ordinary coverage scripts run language tests, not the external oracle
comparisons (`tests/coverage/run_go_coverage.sh:20` and
`tests/coverage/run_rust_coverage.sh:23`). The retained Rust report shows zero
hits on 319 invocation-module lines despite the separate executable oracle
controls. This is an instrumentation gap; behavioral test success does not
make the coverage check green. A user decision is requested: count representative
existing CLI controls in both coverage reports (recommended long-term-best,
without a new framework), or explicitly accept/document this PR's coverage
exception. No coverage-script implementation or exception is assumed before
that answer. Reports are retained under `unit-repair-ci-coverage-{go,rust}/`.

Whole-SOW review session 38c2b9beccf04384a5e70a58fae8036e uses the fully displayed
prompt `whole-sow-unit-review-20261020.txt` and the 108-file snapshot
`unit-repair-review-snapshot.json`. GLM, MiniMax, Qwen and DeepSeek are still
reviewing. Mimo and Kimi returned HTTP 429 without a verdict, including their
single permitted retries; these are coverage gaps, not negative technical
verdicts. Source files remain identical to the snapshot; subsequent local
changes are this progress record and the Markdown blank-line correction.
The SOW remains in-progress until review disposition, the coverage decision
and coherent lifecycle/commit closure are complete.

### Review Timeout Collection - 2026-10-20

Session 38c2b9beccf04384a5e70a58fae8036e turn 1 ends without a final technical
verdict from GLM, MiniMax, Qwen or DeepSeek: each reaches its 1,800-second
execution limit with zero final-output bytes. These are incomplete reviews,
not approvals or verified defect reports. The collection is retained in
`unit-review-final-collection.log`. Each receives its single permitted retry
as a continuation of the saved inspection, with unchanged whole-SOW scope and
the fully displayed prompt `whole-sow-unit-review-retry-20261020.txt`.
Mimo and Kimi have exhausted their retries and are not invoked again.
The coverage choice remains unanswered; no exception or coverage-script change
is authorized by elapsed time or review completion notifications.

### Final Review Disposition - 2026-10-20

Session 38c2b9beccf04384a5e70a58fae8036e completes its permitted continuations.
MiniMax returns PRODUCTION GRADE after 458.4 seconds with three P3 notes and
no blocking finding. GLM, Qwen and DeepSeek each reach their second 1,800-second
limit without final output. Mimo and Kimi have already exhausted their HTTP 429
retries. The round therefore supplies one technical verdict and five coverage
gaps, not six approvals. No further automatic retry is permitted. Retained
collection: `unit-review-retry-final-collection.log`.

Independent adjudication:

- Optional-argument concern: disproved. The reviewer inspected an older mirror
  rather than the supplied pinned source. systemd/systemd @
  8cc40e0c5e9234bf45084751ac53b1fbfe70b492,
  `src/journal/journalctl.c:512-539,620-628,673-686` explicitly peeks/consumes a
  parseable separate boot or line argument, while case-sensitive uses only its
  attached optional argument. Both parsers follow that rule. Seven executable
  comparisons against the pinned v262 binary cover separate boot 0/1, a short
  option bundle, short/long line limits and bare case-sensitive forms; all Go
  and Rust exit codes, output and diagnostic-presence classes match. Evidence:
  `unit-review-optional-argv-check.json`. No code change is warranted.
- Rust introspection data: accepted existing monorepo dependency, no current
  defect. `parser.rs:378` embeds the shared Go-owned JSON; Cargo.toml declares
  `publish = false`. Standalone CLI source packaging is outside the approved
  outcome. A speculative packaging redesign is rejected from this SOW rather
  than creating unsolicited work.
- Coverage variation: confirmed policy decision remains open. Language-test
  instrumentation includes the CLI but does not include external oracle runs.
  Thus its unit-test-only percentage is valid for what it measures; it does
  not describe all executed behavioral validation. The reviewer cannot grant
  the requested user exception or authorize coverage changes.

The supplied pinned upstream source is available locally; the reviewer's
statement that upstream is unavailable applies only to its chosen older mirror,
not this project's reference. Its suggestion to prune private historical logs
is unrelated and rejected from scope: retained adverse evidence is useful and
file deletion is not authorized. No implementation finding remains from the
returned review. Coverage disposition and coherent final SOW closure remain.

### Coverage Decision And Pre-Implementation Gate - 2026-10-21

The user selects 1A: count representative existing CLI controls in both coverage
reports, without introducing a new test framework. The coverage exception is
not accepted. Root cause: the reports measure language tests but omit external
CLI subprocess controls. Evidence: both coverage scripts, CI reports and the
existing portable smoke and unit/invocation fixture tables.

Plan: reuse the portable smoke runner with one or both language binaries;
replay a small native-qualified subset of the existing unit/invocation controls
there. Build coverage-instrumented CLI binaries, run those controls, and merge
hits into each language's existing report. Preserve pure Go checks, all existing
workspace tests, report normalization and test-source exclusion. Go uses native
covdata plus its existing text profile; Rust uses cargo-llvm-cov's shared profile
collection and final report. Rust coverage needs the existing Go toolchain for
synthetic fixture generation, explicitly configured in CI. Do not install tools
on the workstation or change released binaries, runtime dependencies or floors.

Blast radius: coverage scripts, shared portable controls, coverage workflow and
coverage documentation. Product runtime code is unchanged. Main risks are lost
unit coverage, stale profile contamination, and measuring fixture-writer work
as CLI work. Use private per-run profiles; validate final reports retain SDK
coverage and contain new CLI invocation hits. Fixture generation remains outside
CLI instrumentation. Verify expected outputs against the pinned v262 oracle,
execute both complete report-generation paths, and inspect real CI results.
Existing representative performance evidence remains applicable because only
test binaries are instrumented. Record source/tool failures separately.

Artifact plan: update coverage README, SOW and status evidence; no product spec
or consumer-doc change is needed for test-only instrumentation. Use synthetic
fixtures and private logs; no sensitive input. No remaining design decision
blocks this approved coverage implementation.

### Coverage Implementation And Validation - 2026-10-21

The portable smoke runner accepts either or both language binaries and replays
13 native-qualified unit/invocation cases from the existing shared fixture
table, alongside its original 26 controls per language. Both instrumented
commands pass all 39 controls; the paired run passes 78. Each smoke run owns
its fixture and builder directories. No SDK or CLI runtime source changes.

Go runs the original pure SDK and instrumented language suites, then builds an
atomic instrumented CLI, runs the portable controls, converts native covdata,
and merges source blocks before existing filtering. It rejects incompatible
profile modes or statement counts. All 13,279 unit-profile blocks survive with
unchanged statement counts and nondecreasing hits. Invocation coverage rises
from 2/321 to 221/321 statements; unit naming from 0/125 to 79/125. SDK reader
coverage remains present (500/601 to 502/601 statements). Final Go report is
69.9 percent statement coverage; Codacy's separate line-based PR threshold must
still be checked remotely rather than inferred from this number.

Rust runs workspace tests and portable subprocess controls in one explicit
cargo-llvm-cov profile directory before LCOV export. The integration tests
already build the instrumented CLI. Invocation coverage rises from 0/319 to
240/319 lines; unit naming from 0/105 to 83/105. Covered SDK crate lines remain
present (16,509 in the preceding CI report, 16,555 in this local report).
The Rust coverage job explicitly sets up Go for synthetic fixtures. Python uses
only standard-library modules, with an optional PYTHON interpreter override.

Retained evidence under `.local/sow-0121-v262-krauk7l5/`:
`coverage-go-verified.log`, `coverage-rust-verified.log`, their corresponding
report directories, `coverage-merge-verification.json`, and the unchanged
source hashes in `coverage-validation-source.json`. ShellCheck with sourced
helpers and shell syntax checks pass. The full fixed scripts execute end to
end; SDK tests and report exclusions remain enabled.

Adverse observations remain recorded. The first concurrent Go instrumented
run hits the existing live-writer test's 45-second deadline. The full sequential
rerun passes without changing deadlines, test expectations or runtime code.
The first Rust attempt exposes differing default profile directories between
test and show-env operations, followed by an invalid run after the script was
edited while Bash was reading it. That run is rejected. Explicit directory
selection and a stable, unchanged-script rerun produce the accepted report.
No failed attempt is counted as validation success.

Performance impact: only test binaries are instrumented; shipped source and
build behavior are unchanged, so existing runtime performance evidence remains
applicable. The smoke runner and coverage README document the new collection.
No product spec, consumer documentation, runtime project skill or output skill
change is needed: this implements the approved reporting decision without a
new product contract or reusable workflow beyond the coverage README. Existing
SOW-0122 performance scope remains pending and separate. The user coverage
choice is resolved by implementation, not by an exception.

### Coverage CI Result And FreeBSD Matcher Finding - 2026-10-21

Candidate b12c8fffb2457fdb8627947aa08230d3e17cf09a passes both coverage
jobs and upload in run 39581079338. Codacy coverage variation passes at
+1.99 percentage points. The approved coverage decision is delivered.
Native run 39581079490 passes Linux, macOS and Windows, but FreeBSD fails
the newly included unit-name-8---unit control in both commands. The pattern
`[[:alpha:]]lpha.service` must match the fixture's `alpha.service` units;
the native FreeBSD matcher does not provide that named-character-class
behavior. Both command implementations call libc fnmatch directly:
`go/cmd/journalctl/unit_pattern_unix.go:20` and
`rust/src/cmd/journalctl/unit_pattern.rs:28`.

This is a runtime compatibility gap exposed by expanded validation, not a
runtime regression caused by coverage instrumentation. The existing Linux
oracle results do not establish this behavior on FreeBSD. Retained evidence:
`coverage-native-freebsd.log` under the current private validation directory,
lines 1101 and 1140, and native CI job 119158555362. A controlled C-locale
probe of the FreeBSD source also differs from glibc for a single-character
collating-element pattern; it is source-level supporting evidence, not a
native FreeBSD execution claim.

The previously approved native-matcher design cannot supply the required
behavior by directly calling FreeBSD fnmatch. No runtime repair has been
implemented. Pending design choice: permit a FreeBSD-specific compatibility
path for unsupported unit-pattern syntax, preserving ordinary native paths,
or explicitly accept and document platform-native pattern differences. The
former preserves the requested parity outcome and is recommended; the latter
would change the CLI compatibility contract. A replacement must be checked
for locale, raw-byte and malformed-pattern behavior before implementation;
a generic regex substitution has not been established as equivalent.
SOW closure remains blocked on this finding and final review disposition.

### Coverage Review Collection - 2026-10-21

Collected session 38c2b9beccf04384a5e70a58fae8036e, turn 3 for GLM,
MiniMax, Kimi, Qwen and DeepSeek, and Mimo's already-used retry, turn 4.
GLM returns NEEDS CHANGES for the independently reproduced FreeBSD matcher
gap above. MiniMax and Kimi return PRODUCTION GRADE but rely on older native
qualification for unchanged runtime code; those verdicts do not override the
new native failure. Qwen and DeepSeek each time out after 1,801.3 seconds
without verdicts. Mimo's retry fails with HTTP 429. These three failures are
review coverage gaps, not approvals. No further review is launched while the
verified matcher blocker awaits a design decision and repair.

Nonblocking notes are adjudicated as follows. Failure output could show more
detail, and awk can leave a partial raw profile after its nonzero exit; neither
can turn a failed check into a successful upload. Hard-coded smoke selection
and case-name coupling are intentional representative sampling; current names
and counts are verified, and a missing name fails loudly. Additional empty-hit
or per-run Rust retention checks are optional reporting defenses, not evidence
of current lost coverage: retained final reports and successful CI establish
the delivered result. A further reporting framework is rejected from this
minimal coverage change. Automatic scratch deletion is rejected: evidence is
retained intentionally and deletion is not authorized. Existing local stress
test timing sensitivity remains recorded, with the unchanged sequential run
and isolated CI passing. Standalone Rust CLI packaging is not in scope; its
shared introspection input is deliberate and the crate is not published.

Stale observations are corrected by current evidence: Codacy variation now
passes at +1.99 points, and git signature verification reports G for b12c8ff.
The pre-existing malformed-JSON smoke diagnostic can fail with a traceback;
it still fails closed and is not required to repair this coverage outcome.
These optional diagnostic and housekeeping changes are rejected from the
current change, not deferred implementation obligations. Full collected output
is retained privately as coverage-review-collection.log in the validation
directory; no reviewer transport log is copied into durable evidence.

### FreeBSD Compatibility Repair Gate - 2026-10-21

The user approves option 1A: repair FreeBSD unit-pattern compatibility, keeping
ordinary native paths. The user also replaces external-model review with an
internal subagent applying the final-review skill. Do not launch further
external reviewers. Review the whole final implementation and causal blast
radius after primary validation, and independently adjudicate findings.

Root cause and evidence are recorded above. Affected callers include system
and user unit expansion, invocation selection and frozen follow expansion.
SDK behavior, file-pattern globbing and compiler floors are unchanged.
Implement a CLI-private prepared compatibility matcher only for unsupported
extended bracket syntax on FreeBSD. Investigate native POSIX regex compilation
as the existing-libc implementation of classes and collating elements, with
glob translation and explicit locale/raw-byte handling; do not assume regex
grammar is interchangeable with glob grammar. Reuse ordinary fnmatch for all
other patterns. Share the compatibility implementation between the commands
to prevent language drift; retain per-command prepared state and teardown.

Before accepting that implementation, differential-test the fallback against
glibc fnmatch for C/UTF-8, positive/negative/malformed brackets, literal
backslashes, Unicode, raw bytes and wildcard boundaries. Compile the same
fallback in primary-host tests so FreeBSD-only code is not untested locally.
Extend the existing portable controls and use native FreeBSD CI to qualify
both command integrations. Compare actual work and focused timings for added
costs; no new benchmark framework. If the libc regex approach fails semantic
qualification, resolve it within the approved compatibility-path outcome
before adoption, rather than accepting a parity exception.

Artifact impact: update unit-pattern contract notes, compatibility skill,
portable tests, CLI build prerequisites if needed, SOW and status evidence.
Use only synthetic fixtures and ignored local probe output. No sensitive
inputs, production journal access, service changes or installations.

### FreeBSD Repair Implementation And Primary Validation - 2026-10-21

Both CLIs now use one CLI-private compatibility implementation for extended
unit brackets on FreeBSD. Ordinary patterns allocate no compatibility state
and retain native fnmatch. The fallback prepares byte/wide pattern forms once,
reuses its value buffer, handles named classes through native character
classification, and preserves malformed-bracket and multibyte/byte retry
behavior. It does not scan journal rows or change indexed unit discovery.
Go owns the state through the existing matcher lifecycle; Rust owns the same
C implementation through an RAII wrapper. Linux/macOS compile the wrapper for
direct fallback tests while their production matching stays native. Windows
keeps its existing byte matcher. File globbing and SDK code are unchanged.

The initial POSIX regex translation is rejected after 688 mismatches in
237,150 bracket comparisons: rejecting an entire malformed regex differs from
fnmatch's successful-member and unterminated-bracket handling. The replacement
direct wildcard matcher has zero differences across 355,725 comparisons in
C, C.UTF-8 and en_US.UTF-8 for the retained corpus. This is targeted evidence,
not a claim that different operating systems share identical locale databases.
The 237,150-case C/UTF-8 corpus also passes address/undefined-behavior checks.
Byte ranges use byte collation, rather than treating invalid bytes as Unicode
characters; the broader locale probe exposed and corrected that distinction.

Supported-floor Go CLI tests pass with race detection; Rust passes 25 unit
tests and 11 subprocess tests. Pure Go SDK build passes with CGO disabled.
All 1,372 pinned-v262 observations pass, including new system/user bracket
invocation discovery and selection controls. Expanded portable smoke passes
96/96 locally. Native FreeBSD qualification remains pending before acceptance.
Wiki validation passes for 16 pages; all 35 executable documentation examples
pass. The raw example spelling initially triggered the wiki-link validator;
consumer prose now names the feature without introducing a false wiki link.

Performance evidence follows the changed work: ordinary matching retains its
native loop and avoids compatibility allocations; SDK/query traversal is
unchanged. The previously incorrect FreeBSD extended-bracket path has no
correct pre-change timing baseline. A small alternating C-level comparison
with working Linux/glibc matching measures about 48 ns versus 37 ns per value
in C locale, and about 93 ns versus 125 ns in UTF-8. These are isolated matcher
costs, not whole-command measurements or FreeBSD timing claims. The C-locale
difference is about 0.011 ms per 1,000 distinct unit values; the UTF-8 path is
faster in this control. Pattern allocations occur once and the value buffer
grows only when needed. No performance framework or ordinary-query exception
is introduced for this compatibility repair.

Rust adds a direct CLI build dependency on the already locked cc 1.4.0
(declared Rust minimum 1.63); no dependency version or SDK compiler floor
changes. Existing CLI prerequisites already require a C toolchain. The shared
source belongs to the Go command directory and is consumed by the unpublished
Rust command, consistent with the existing shared CLI contract input.
Specs, CLI consumer prose and the compatibility skill explain the FreeBSD
path. No output skills exist; no SDK docs or public SDK contract changes.

Evidence under the current private validation directory:
freebsd-go-cli-final.log, freebsd-rust-cli-final.log,
freebsd-final-v262.json/log, freebsd-portable-final.log,
freebsd-differential-direct.log, freebsd-differential-locales.log,
freebsd-differential-asan.log, freebsd-pattern-cost.log and
freebsd-doc-examples.log. The rejected translation observations are retained
in freebsd-differential.log. Internal final review follows native qualification.

### FreeBSD Native Qualification And Locale Edge Cases - 2026-10-21

Native run 39659065166 on signed candidate e3ed6ec passes FreeBSD, Linux and
Windows, including all 96 portable controls. macOS fails the new direct
fallback Unicode-class test. Its default MB_CUR_MAX macro observes global
locale state unless the extended-locale header is included; the fallback now
includes xlocale.h on Darwin. This affects direct fallback validation there,
not ordinary macOS production matching, which still calls native fnmatch.

The remaining primary audit adds accented/case equivalence-class controls.
The compatibility path compares primary weights from native wide collation
keys (level separator 1 on the qualified FreeBSD/glibc implementations), and
preserves glibc's byte-retry consumption for multibyte equivalence matches.
All 718,200 retained comparisons across C, C.UTF-8 and en_US.UTF-8 now agree
with local glibc 2.39, under address/undefined-behavior instrumentation. Both
CLI suites pass again; the explicit UTF-8 tests also require the selected
locale rather than silently skipping an unavailable locale. Latest logs:
freebsd-differential-final.log, freebsd-go-cli-qualified.log and
freebsd-rust-cli-qualified.log. Native rerun qualifies these final additions.

Inspection references (reference code is not copied into the implementation):

- freebsd/freebsd-src @ bd31ee522f8eb5bc2a3208accc2f6976e5d45b5d:
  lib/libc/locale/collate.c:394-419, lib/libc/gen/fnmatch.c,
  lib/libc/regex/regcomp.c:1182-1193.
- apple-oss-distributions/Libc @ 71bbe350ab79eef58113991d817ccc6165061a64:
  include/_stdlib.h:123-139, include/__xlocale.h:27,
  locale/xlocale.c:142-195.
- bminor/glibc @ 04e750e75b73957cf1c791535a3f4319534a52fc:
  posix/fnmatch.c:234-263 and posix/fnmatch_loop.c (bracket grammar).

Static analysis reports two generic strlen terminator alerts in the new
private header. Both are false positives after inspecting every caller:
patterns come from Go C.CString or Rust CString, and both value wrappers append
a NUL before calling C. Localized Flawfinder directives record these exact
invariants; no global rule is disabled. The local level-4 scan reports no hits.
The pre-existing Bandit internal error remains separately recorded. Raw logs
stay private (freebsd-first-macos.log, freebsd-first-codacy.log and
freebsd-flawfinder.log). Ordinary coverage jobs and Codacy variation pass on
e3ed6ec; new runtime review still awaits final native qualification.

Native run 39660136110 passes all four platforms on f705c21, including the
Darwin thread-locale fix and accented equivalence controls. Coverage also
passes. The sole new analysis finding is a missing blank line before the
reference list in this SOW; it is repaired. The pre-existing Bandit execution
error is unchanged. Later primary checks add unterminated named/collating
brackets and self-matching malformed patterns. Those controls expose and fix
trailing-range and unfinished-special-member handling. Range members now
delegate to a bounded, escaped native fnmatch pattern, retaining the platform's
ordinary range ordering rather than approximating it with full collation-key
ordering. This prevents mixed native/compatibility patterns from disagreeing
within the same locale. Native ranges and primary equivalence weights serve
different contracts and must not be conflated.

The final 1,026,675-comparison corpus passes under address/undefined-behavior
instrumentation. Both CLI suites pass again; permanent tests include the
reproduced malformed cases and compare mixed class/range patterns with the
platform's ordinary range matcher. Collation-key resize and bounded conversion
failure paths are checked before use. Latest retained evidence:
freebsd-differential-final-audit.log, freebsd-go-cli-final-audit.log,
freebsd-rust-cli-final-audit.log and freebsd-flawfinder-final.log. Final source
is frozen for native qualification and independent internal review; no more
external-model review rounds are authorized.

### Internal Final Review And Timestamp Rendering Repair Gate - 2026-10-21

The user-requested internal final-review subagent reviewed frozen revision
6752fc7 and returned FAIL with one proven P2 failure class. Independent probes
passed 931 option combinations and 450 rendering combinations. Calendar output
bypasses the existing CLI timezone resolver: Go time.Local ignores POSIX TZ
rules; both ordinary rendering providers ignore custom TZDIR and TZif leap
records and apply transition-free footers prematurely. Rust can combine an
incorrect clock with a libc-derived abbreviation. Entry output, verbose output,
headers and boot/invocation tables share the affected paths. Raw epoch output
is correct. This is an existing parity gap, not caused by the FreeBSD repair.
Evidence: go/cmd/journalctl/output.go and main.go formatHeaderTimestamp;
rust/src/cmd/journalctl/output.rs; private final-review-6752-cli zones/leaps
reproducers and results. Native CI 39661284599 passes all four platforms on
6752fc7; coverage, CodeQL, documentation and SARIF jobs pass. The SARIF log
still contains the pre-existing Bandit execution error, an analysis coverage
limit rather than proof of complete Bandit coverage.

Repair applies the approved portable timestamp semantics already implemented
for bound parsing. Reuse each CLI's timezone resolver and parsed TZif metadata
for read-only calendar conversion; retain parser state semantics unchanged.
Resolve the rendering timezone lazily once per command process, so cat/JSON/raw
clock output does not load timezone data and rows do not reread files. Borrow
zone names and search transition/leap arrays without rebuilding parser names
or allocating a timezone per row. Calendar output uses one consistent source
for civil fields, zone offset and abbreviation, including leap corrections.
Preserve stock's local header/table behavior with --utc and raw short-unix
values. No SDK/API/dependency/platform architecture change is needed.

Validation covers the reproduced POSIX/custom TZDIR/leap/footer controls in
both languages against pinned v262, all calendar modes, tables/headers, UTC,
raw epoch controls and ordinary named zones. Run focused CLI suites and existing
full oracle, inspect changed work for allocation/I/O cost, and re-review the
whole SOW through the same internal reviewer. Specs/compatibility instructions
will record the parser/rendering consistency requirement. Detailed evidence
remains in ignored scratch; fixtures are synthetic. No new user design decision
is required to apply the existing approved timestamp contract.

### Timestamp Repair Implementation And Primary Evidence - 2026-10-21

Both renderers now reuse command-local timezone resolution and TZif metadata,
with read-only rendering state initialized on first calendar output. Go caches
fixed locations per TZif kind once and keeps ordinary formatting on time.Time;
only files with leap records need the explicit leap conversion. Rust borrows
names and produces calendar fields from the same offset source. Neither path
rebuilds parser abbreviation state or reads files per row. SDK code is unchanged.
Empty/malformed TZ names and native offset-component limits are retained during
resolver fallback. Source: bminor/glibc @ 04e750e75b73957cf1c791535a3f4319534a52fc,
time/tzset.c:141-208 and 303-309. Valid POSIX providers remain in use; named-zone
fallback remains available on platforms without an installed zoneinfo database.
The expanded fixture exposed a separate small-epoch presentation gap: short-unix
now uses stock's ten-column seconds width; its epoch value stays unchanged.

Primary evidence: time-render-check-final.log passes 668 controls covering
parsing and all calendar displays, including raw epoch, UTC, tables/headers,
custom TZDIR, POSIX rules, historical transitions, leap hits and footer handling.
time-final-v262.log passes 1,372 existing v262 observations. time-portable.log
passes 102 controls, adding POSIX/custom-directory/leap rendering to native CI.
Supported-floor Go race and Rust suites pass; all 35 executable documentation
examples and 16 wiki pages pass. The SOW audit is complete and clean. Private
invalid-zone probes cover 60 render/bound cases without differences after repair.

Performance review identified an initial Go UTC-rendering increase (119.66 to
128.51 ms for 20,000 rows). It is retained in time-cost.json and was not accepted.
Ordinary Go rows now use cached locations and the original standard formatter;
explicit leap conversion is confined to files with leap metadata. A subsequent
alternating comparison removes that increase; isolated final evidence is retained
separately. Rust avoids the previous separate libc abbreviation lookup. No new
performance framework, SDK cost or ordinary-query exception is introduced.

Artifact maintenance: parity spec, CLI consumer prose and compatibility skill
now require parsing/rendering timezone consistency. No AGENTS change or output
skill is needed; existing policy covers this repair and no output skills exist.
Synthetic fixtures and sanitized records satisfy the sensitive-data gate. This
repair and its performance evidence remain in SOW-0121, with no correctness
item transferred to the broader pending scalability SOW-0122.

The final isolated five-pair comparison (20,000 rows, equivalent output) records
UTC medians Go 117.10 -> 119.91 ms and Rust 111.48 -> 100.05 ms; New York medians
Go 117.98 -> 119.08 ms and Rust 107.06 -> 99.04 ms. Stock is 34.82/34.98 ms.
Go paired directions are mixed (UTC three increases/two decreases; New York
three increases/two decreases), and the preceding comparison has lower candidate
medians in both zones. This does not prove equality; it does not reproduce the
initial sustained formatter cost either. Code review confirms one existing-style
transition lookup/format, cached locations and no per-row filesystem work or
zone allocation. These representative results and work tracing establish the
performance disposition without expanding a measurement framework. Raw samples
and initial adverse results remain in time-cost*.json.

### Default Daylight Rule Repair Gate - 2026-10-21

Internal re-review of eff0231 confirms the original timestamp repairs and
independent 931 option/450 rendering probes pass, but returns FAIL for omitted
POSIX daylight transition rules. Rust treats XYZ5ABC as fixed standard time;
both providers bypass a custom TZDIR/posixrules file. This changes displayed
times and local query boundaries by an hour in reproduced ordinary fixtures.
The correction stays within the existing resolver contract: intercept rules
that supply a DST name without dates, load/transform the default TZif once using
its standard/universal transition flags and caller names/offsets, and use the
native US transition defaults only if the default file is unavailable/unusable.
Explicit-date rules and ordinary rendering paths are unchanged. Retain the
original footer offset behavior and override its displayed names as native
conversion does. TZif parsing gains the two existing per-kind flags; this is
CLI-only metadata, not journal SDK work. Native offset components also undergo
unsigned-short conversion before their documented clamp, covering two extreme
input discrepancies proven by review. No dependency or architecture change.

Reference: bminor/glibc @ 04e750e75b73957cf1c791535a3f4319534a52fc,
time/tzfile.c:459-613 and time/tzset.c:248-268,317-329. Private default-dst and
default-rules evidence remains under final-review-eff0231-cli. Validation adds
ordinary/default-file/missing-file, historical/future and explicit-rule controls
to the existing timestamp suite, including local bound selection and rendering.
No per-row file access or new benchmark framework is introduced. Whole-SOW
internal re-review and native CI remain required after the repair.

### Default Rule Implementation And Primary Validation - 2026-10-21

Both resolvers now prepare omitted-date rules from TZDIR/posixrules, with
standard/universal transition flags, native name/offset replacement and a
checked timestamp adjustment. An unusable/missing default file uses explicit
native US dates. Components wrap through the native unsigned-short conversion
before clamping. Installed named zones and explicit-date rules retain their
existing paths. Rendering only sees the resulting cached metadata; no ordinary
per-row path or SDK code changes in this repair.

Expanded checks exposed two details that reading upstream comments alone missed:
native default-file conversion starts with zero remembered daylight offset,
and footer conversion's tm_zone retains the footer abbreviation even though
its parser abbreviation pair is restored to user names. The implementation
matches observed pinned-oracle output, including historical/future dates; the
prior gate's planned displayed-name override is superseded by this evidence.
A first synthetic universal-flag fixture violated TZif's universal-implies-
standard constraint; the permanent flag fixture is valid and exercises both
branches. The initial 916-case failure log is retained with final passing output.

Default timestamp controls pass 1,052/1,052, including local query boundaries,
ordered UTC/local bounds, all calendar modes and tables, omitted/explicit dates,
custom/missing default files, flags and future footer behavior. Existing v262
controls pass 1,372/1,372; portable smoke passes 106/106. Supported-floor Go CLI
race and Rust 25-unit/11-subprocess tests pass. Wiki validation passes; prior
35 executable examples remain unchanged, and normal CI verifies them again.
Evidence: default-timestamp-final.log, default-final-v262.log,
default-portable.log and default-{go,rust}-tests.log in the private validation
directory. Native run 39665146987 and coverage/analysis pass on preceding
eff0231; the final default-rule changes require their own native qualification.
No further timing run is needed: this repair changes resolver setup and only
the previously incorrect omitted-date path; ordinary hot rendering is identical
to the measured candidate. The same whole-SOW internal reviewer rechecks the
final revision. Specs, consumer prose and the compatibility skill record the
missed default-file requirement; no new follow-up is deferred.

### Final CI Integer Conversion Disposition - 2026-10-21

Internal whole-SOW review passes bed8c1152fb6 with no remaining P0-P2 finding.
Native CLI run 39667258392 passes Linux, macOS, Windows and FreeBSD; both
coverage jobs and documentation examples pass. CodeQL flags the intentional
uint64-to-uint16-to-uint64 TZ component conversion. Native compatibility requires
modulo 65536 before clamping; rejecting overflow would break tested parity.
Replace the narrowing cast with an explicit 0xffff mask: equivalent for every
uint64 input, no allocation or algorithm change, and directly expresses the
required bounded result. Re-run the existing timestamp controls and internal
impact review, then collect final CI. This is an implementation clarification
of the approved native rule behavior, not a new parsing policy.

### Final Closure - 2026-10-21

- Acceptance: paired file-backed v262 CLI behavior is implemented; SDK format,
  compiler floors and pure SDK dependency boundaries remain unchanged. The
  SDK scope completed separately in SOW-0159 after prerequisite SOW-0108.
- Validation: 1,372 v262 observations, 1,052 timestamp observations, 106 portable
  controls, supported-floor CLI suites and documentation checks pass. The
  final explicit mask repeats all 1,052 timestamp observations with zero
  differences. Native run 39667258392 passes all four supported platforms;
  coverage run 39667258353 and CodeQL workflow 39667258404 pass. The aggregate
  CodeQL annotation concerns intentional narrowing; the equivalent explicit
  mask resolves its cause without changing the native compatibility contract.
  Normal PR CI revalidates the combined closure revision.
- Independent final review: the user-selected internal final-review process
  passes bed8c1152fb6 with no P0-P2 blocker. Its independent 931 option and 450
  rendering comparisons, retained failure reproducers and 42 additional
  timezone-history comparisons pass. Closure and the explicit mask receive
  independent impact review before publication. No external model was invoked.
- Same-failure search: shared timestamp parsing/rendering, query bounds,
  headers, boot/invocation tables, TZif flags/footer/leaps/default files,
  unit matching and native platform paths are covered by the recorded audits.
- Performance: retain all adverse observations and accepted narrow startup/
  vacuum exceptions. Cached timezone state avoids per-row file access and
  location allocation; explicit masking is equivalent to the prior cast.
  The representative measurements and hot-path review above establish the
  disposition without claiming inconclusive intervals prove equality.
- Analysis limitation: Codacy run 39667258303 passes its aggregate job but
  Bandit reports one execution error; complete Bandit coverage is not claimed.
  This is retained infrastructure coverage evidence, not a hidden pass.
- Sensitive data gate: fixtures and durable evidence use synthetic identities
  and sanitized summaries; raw logs stay in ignored .local/.
- Artifact gate: specs, consumer CLI documentation and runtime compatibility
  skill were updated with shipped behavior and lessons. AGENTS.md already
  states the approved CLI dependency/target boundaries and needs no further
  change. No separately distributed operator skill exists to update. Both
  status indexes and SOW-0122's dependency reference are reconciled. This SOW
  moves to done with implementation in the same signed commit.
- Follow-up mapping: all confirmed CLI correctness findings above are fixed;
  historical pending/decision wording is execution history. Broader large-input
  invocation/polling/archive optimization remains in real pending SOW-0122,
  unactivated. Independent SDK work remains in paused SOW-0045/SOW-0071 and
  pending SOW-0160/SOW-0161; none is silently included or required to repeat
  this accepted scope. No new implementation obligation is deferred.
- Lessons: oracle-check calendar rendering separately from bound parsing,
  qualify native dependencies on each target OS, and preserve scanner execution
  limitations even when aggregate jobs pass. These are reflected in the
  compatibility skill and the evidence above.

The final lifecycle audit reports complete and clean (closure-audit.log).

## Regression - 2026-10-23

### What Broke And Evidence

The user approved reopening after internal final-review risk analysis of
45e0de2832f3bb886551896287b6ddc1e3f5edb4. Twenty-one valid, nonempty archived
journals span three sequence IDs and backward realtime ordering. Every filename
agrees with its header/ENTRY identity and timestamps; pinned v262 verifies all
21. With --vacuum-files=2, stock retains two and exits zero; Go retains a
different pair and exits zero; Rust exits 101 with a non-total-order sort panic
and retains all 21. Earlier empty-file cleanup may already mutate a mixed
archive directory. Synthetic reproducers/results remain under ignored
.local/risk-cli-review/ (write_valid.go, valid-names.json, valid_check.py,
valid-result.json). No live host journal was accessed.

Rust main.rs:1644,1753 and Go main.go:3381,3454 use sequence order within one
ID and realtime across IDs. These comparisons can cycle. The comparator and
Rust sort already exist in the PR base: this is an inherited defect missed by
the requalification, not a new SDK regression. Earlier vacuum controls lacked
valid cross-ID clock-rollback ordering. The independent SDK and upgrade reviews
pass their scopes; their verdicts do not override this maintenance blocker.

### Repair Gate And Plan

The user approved investigating native ordering and a shared Rust/Go repair.
No archive-deletion policy has yet been selected. Inspect pinned upstream
comparison/sort/enumeration and library guarantees; determine whether matching
one oracle's accidental order is a portable contract. Go also pre-sorts directory
names through os.ReadDir, unlike native enumeration, so changing only its sort
algorithm does not establish parity. Present any irreducible deletion-policy
choice with concrete tradeoffs before implementation. Do not catch the panic
and continue, or silently choose a different retention order.

Validate the selected policy with the retained valid 21-file fixture, ordinary
monotonic archives, multiple sequence IDs, corrupt-style names, and applicable
count/size/age combinations. Trace added sorting/allocation work and use a
focused existing-style timing control if needed; do not expand a benchmark
framework. Preserve the user-approved narrow startup/vacuum costs. Update the
parity spec, consumer maintenance documentation, compatibility skill and both
status indexes; rerun the local audit and internal full-scope review before
closure. All fixtures are synthetic and raw evidence stays ignored. No SDK
source or unrelated SOW is activated by this repair.

### Native Ordering Investigation And Open Decision - 2026-10-23

Pinned reference: systemd/systemd @ 8cc40e0c5e9234bf45084751ac53b1fbfe70b492,
src/libsystemd/sd-journal/journal-vacuum.c:35-49,153-160,293 and
src/basic/sort-util.h:31-36. Native enumeration feeds qsort through its wrapper;
Go os.ReadDir sorts names first. POSIX qsort requires total ordering (Open Group
qsort specification, https://pubs.opengroup.org/onlinepubs/009695399/functions/qsort.html).
The native comparator violates that prerequisite for a cycle such as A=(ID1,
seq1,time300), B=(ID1,seq2,time100), C=(ID2,seq1,time200): A precedes B, B precedes
C and C precedes A. A read-only in-memory libc experiment over all six input
permutations produces three different output orders: ABC, BCA and CAB. This
experiment performs no journal operations or deletions. Emulating one libc's
sorting algorithm alone cannot establish portable stock parity.

Open policy decision, before code: recommend a deterministic shared order that
preserves increasing sequence within each history and chooses the oldest-time
available history head across histories (with explicit deterministic tie
breaks). This defines behavior where native comparisons are contradictory,
rather than promising the oracle's incidental survivor pair. It requires a
narrow documented parity exception and targeted cost validation; the plan must
cover missing sequence IDs, ties, empty-file handling and count/size/age stop
conditions before implementation. Alternatives are conservative refusal on
clock-rollback histories before deletion, or freezing a selected libc/order
emulation with its portability and maintenance limitations. None is approved
by the earlier approval to investigate. The local lifecycle audit reports
complete and clean after reopening; no production source has changed.

### Approved Decision 1A And Implementation Gate - 2026-10-24

The user selected 1A: deterministic sequence-preserving ordering in both CLIs.
Sort each sequenced history by sequence number, breaking equal sequence numbers
by filename; treat each candidate without sequence metadata as an independent
history. Merge available history heads by adjusted realtime, then sequence ID
when both heads have one, then filename. To make ties a total order, sequenced
heads precede unsequenced heads at equal realtime; names compare native Unix
bytes / Unicode Windows names in both commands. This explicit tie convention
extends the approved deterministic policy. Filename is unique within a directory.

Implementation uses a contiguous history sort and a min-heap of history ranges,
consuming only candidates needed by the existing retention loop. No journal
payload decoding or new metadata syscall is added. Time is O(n log n + k log h)
for n candidates, h histories and k consumed candidates; auxiliary state is
O(h). A single history needs no heap comparisons. Empty cleanup, protected files,
metadata adjustment and count/size/age stop conditions retain their existing
contracts; an age stop does not skip an earlier-sequence file to delete its
later successor. Contradictory cross-history comparisons are the approved
parity exception. Test ties, unsequenced archives, reversed insertion order,
clock rollback, ordinary stock-compatible order and real command survivors.
Add portable controls and targeted sorting/command cost evidence. Update docs,
spec and compatibility skill. No dependencies or SDK changes are required.

### Decision 1A Implementation And Local Validation - 2026-10-24

Both commands now use a valid contiguous history sort and a lazy min-heap of
available heads. The old pairwise cyclic comparators are removed. Missing
sequence metadata is represented by singleton histories. Tests enumerate all
120 permutations of a cyclic/tied/mixed candidate set in each language. Shared
portable controls generate the original 21 valid archives using caller-provided
head sequence numbers; reversed creation order, count two/ten, size, age and
combined limits pass. A recent earlier sequence protects its old successor
under age-only limits, while an eligible old sibling history is removed.

Local validation passes 130/130 portable observations, 78/78 existing utility
oracle observations, Go CLI race tests, Rust 26 unit/11 subprocess tests,
35 executable documentation examples and 16 wiki pages. Synthetic fixtures
and raw results are under .local/risk-cli-review/; no SDK source changed.

Performance evidence uses seven alternating measured runs over 1,000 nonempty
archive names, with one, 50 and 1,000 histories, against retained binaries and
stock. The first Rust many-history median increased (14.67 to16.26ms); retained
as adverse evidence in vacuum-cost-initial.json, not accepted as equality.
An isolated repeat yields Go before/after20.43/20.53,20.41/20.44,20.76/20.41ms;
Rust16.85/16.83,16.46/16.55,16.56/16.81ms; stock75.91-79.49ms. The original large
increase did not reproduce. These small mixed/reduced differences do not prove
equality; work tracing shows unchanged file/header/syscall work, one in-place
sort, O(h) heap metadata and lazy O(log h) selection. No per-row payload work or
new metadata syscall is introduced. Go baseline is retained final45e0de2;
Rust retained unit-repair-6 precedes unrelated timestamp changes, so its
comparison is representative, not an exact whole-binary before/after claim.
No additional performance waiver is requested. Final internal review assesses
this evidence and the complete changed behavior before publication.

Consumer maintenance docs, parity spec and compatibility skill record the
approved exception and future tests. AGENTS.md needs no change: this is a
CLI behavior decision within its existing dependency boundary. No distributed
operator skills exist. Pending0122 remains unactivated and owns only broader
scalability work. Internal review/native qualification and lifecycle close
remain outstanding; previous completion/PASS is superseded for vacuum ordering.

### Regression Closure - 2026-10-24

Decision 1A is implemented in Rust and Go. Internal final-review passes exact
repair fcd918bcaab05b29e2bd18cf297ce2273b2059d8, with matching reviewed hashes.
Independent checks add 10,000 randomized candidate sets per language against
an eligibility model and 12 real-command controls using the original 21 verified
journals (two languages, two creation orders, limits 2/10/21). No remaining
P0-P2 failure or reproducible blocking slowdown was established. The whole
relevant scope includes maintenance, platform/native dependency boundaries,
shared fixture generation, documentation and previous CLI findings. Review
findings are fixed, not waived; final lifecycle changes receive impact review.

Native run 40076505728 passes Linux, macOS, Windows and FreeBSD with 130 portable
observations across both commands. Both coverage jobs in 40076505657 pass;
documentation examples and wiki validation pass. The Codacy SARIF job initially
failed fetching remote configuration with an upstream Internal Server Error,
before source analysis; a retry failed identically. Coverage generation passes
but its Codacy upload reports Request URL not found / no data sent. Neither
failure establishes an implementation finding; the external configuration/API
path remains unverified and no CI configuration is changed under this repair.
Go/Rust CodeQL run 40076505661 passes. The earlier Bandit execution-coverage limitation
remains explicit; aggregate success alone does not remove it. Normal PR CI
revalidates the final lifecycle amendment.

Acceptance evidence also includes 78 existing stock-oracle utility observations,
Go CLI race tests, Rust 26 unit/11 subprocess tests, 120 insertion permutations
per language, 35 documentation examples and 16 wiki pages. The local SOW audit
reports complete and clean. Cost results and their uncertainty remain above.
Same-failure review covers mixed/unsequenced histories, ties, backward clocks,
heap successor replacement, age stopping and reversed enumeration; the old
cyclic comparators no longer serve either command.

Artifact maintenance: consumer maintenance docs, parity spec and compatibility
skill are updated; AGENTS.md and output/operator skills need no changes for the
reasons recorded above. Both status indexes and SOW-0122's prerequisite
reference are reconciled. Synthetic identities and ignored raw evidence satisfy
the sensitive-data gate. The lesson is to test valid cross-history clock rollback
and require a total ordering rather than treating one libc output as a portable
contract. Historical pending decisions are resolved by 1A; broader scalability
remains SOW-0122, with unrelated SDK issues still explicitly tracked by their
own SOWs. No new deferred repair remains. Implementation and completed SOW
lifecycle are amended into the same signed repair commit.

## Regression - 2026-10-26

### Pre-Implementation Gate

The user approved handling the confirmed reviews and CI findings without another
design decision. At a1144dcbf537, Go invocation listing treats facade TestCursor
false,nil at EOF as a positioned mismatch (invocation.go:345-353), then reads
missing data. Existing invocation-bounds portable smoke fails all four OS jobs.
Check position using cursor metadata before returning a candidate, retaining the
approved facade false,nil EOF contract and avoiding row expansion.

Rust file_glob.rs:190-222 resurrects an exhausted wildcard candidate list.
Stop expansion when a wildcard yields no candidates; test literal and parent
suffixes. Go already preserves empty candidates. Both CLIs concatenate Windows
paths into invalid file URIs. Normalize drive/UNC paths using standard file URI
forms, percent-encode Windows path bytes, retain Unix output behavior, and render
unsupported device-path forms as plain text. This is CLI-only formatting; no SDK
or dependency change. Test drive, UNC, relative resolution and escaped characters.

Query-matrix helpers silently mix PATH journalctl with injected stock commands.
Require one explicit version/feature-validated v262 binary, propagate it to every
case and version report, and update documented invocation. Fix relative Go CLI
example paths. Parser rejection tests must name a fixture instead of host defaults.
These harness corrections preserve existing cases rather than creating a new
framework. Prior explicitly injected oracle results are not relabelled invalid.

Validation: Go/Rust CLI unit tests, existing portable smoke including invocation
bounds, focused v262 oracle cases, query harness unit/plumbing controls, wiki/docs,
compile checks and native OS CI. Review same patterns (TestCursor callers, empty
glob branches, all Windows link surfaces, every stock command builder). No live
host journals. Keep source/fixture/cache outputs under .local and raw analysis
exports ignored. Preserve plain-output hot paths; URI encoding runs only when
links are enabled. No new synchronization or per-entry SDK work.

Artifact impact: update relevant parity/product specs, compatibility skill and
consumer docs only where behavior or test invocation changes; both SOW indexes
track this lifecycle. Runtime purity/CGO boundary unchanged. Three internal final
reviewers will assess SDK blast radius, CLI and validation after local fixes;
external models remain disabled by user instruction. Codacy now reports13 minor
Markdown findings and one Bandit execution error, not the old configuration
failure. Investigate actual causes before editing CI; unrelated baseline problems
must be explicitly dispositioned, not swept into this PR. No open product choice
blocks the confirmed minimal-complete fixes.
