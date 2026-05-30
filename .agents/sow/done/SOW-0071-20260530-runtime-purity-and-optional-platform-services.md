# SOW-0071 - Runtime Purity And Optional Platform Services

## Status

Status: completed

Sub-state: implementation, local/macOS/Windows validation, three whole-SOW
review rounds, final cleanup, and final local validation are complete.

## Requirements

### Purpose

Make the SDK architecture unambiguous: core journal readers and writers are
file-format implementations only, while host identity discovery and cooperating
writer locks are separate optional services that callers explicitly choose.

This protects backend consumers from hidden runtime side effects, keeps the
file-format SDK portable and predictable, and prevents future agents from
confusing systemd compatibility with host probing or writer locking.

### User Request

The user decided:

- boot ID determination should be a separate crate/library that does whatever
  is needed per operating system;
- boot ID discovery is a systemd/journald compatibility concern, not a core
  journal file-format concern;
- systemd compatibility layers should require caller-provided boot and machine
  identities, while callers may opt into supplied identity helpers;
- the journal file-format contract is one writer per file, but this is not
  enforced by the file format and must not be enforced by the core SDK;
- any cross-platform writer-lock convenience should be a separate optional
  library, independent from systemd compatibility;
- core journal readers and writers must not call external programs, must not
  probe host-specific files or registries, and must focus exclusively on the
  journal file format;
- these rules must be made prominent in `AGENTS.md`, specs, project skills,
  docs, and implementation tests so the architecture is not lost after context
  compaction.

### Assistant Understanding

Facts:

- The journal file format stores boot IDs and machine IDs in headers and entry
  metadata, so the core writer must be able to write them when provided.
- Discovering a host's current boot ID or machine ID is not a journal file
  parsing/writing requirement.
- The systemd journal file format assumes one writer per file, but systemd does
  not enforce this with a portable lock protocol in the file format.
- The reviewed portability worktrees currently contain convenience host probing
  and stale-lock behavior inside or adjacent to SDK runtime paths.
- The user wants the reviewed work merged first, then this architectural split
  SOW applied before closing portability or publishing a stable API.

Inferences:

- SOW-0071 is a blocker for closing SOW-0063 and for any release/API
  stabilization.
- The implementation should be done after the reviewed branches are merged to
  avoid rebasing seven already reviewed worktrees.
- Runtime scans must become part of validation so future changes cannot
  accidentally reintroduce subprocess execution, host identity probing, or
  automatic locking into the core SDK.

Unknowns:

- Exact package names and public API names for optional identity and lock
  helpers in each language should be chosen during implementation by following
  each language's existing package/module style.
- Whether optional host identity helper implementations should avoid external
  commands entirely on every target or allow documented command fallback when no
  native API exists. This SOW should prefer native APIs and return unsupported
  errors rather than running commands unless the user explicitly changes that
  policy.

### Acceptance Criteria

- `AGENTS.md` prominently states the runtime-purity architecture and future
  agents cannot miss it.
- Runtime project skills under `.agents/skills/project-*` state the same
  architecture where journal compatibility or orchestration prompts are
  affected.
- Product specs under `.agents/sow/specs/` distinguish:
  - core journal file-format SDK;
  - systemd/journald compatibility layer;
  - optional identity helper service;
  - optional writer-lock helper service.
- Core readers and writers in Rust, Go, Python, and Node.js do not execute
  external programs at runtime.
- Core readers and writers in Rust, Go, Python, and Node.js do not read host
  identity sources at runtime, including `/proc`, `/etc/machine-id`,
  `/host/proc`, platform registries, `sysctl`, `system_profiler`, `ps`, or
  equivalent host-specific identity sources.
- Core writers in Rust, Go, Python, and Node.js require explicit identity inputs
  for format fields that need machine ID, boot ID, seqnum ID, and related
  metadata, or generate only SDK-local non-host identities when the public
  contract explicitly allows it.
- Systemd compatibility/high-level APIs require caller-provided machine and
  boot IDs where systemd/journald semantics need them. These APIs may expose a
  clear opt-in path to the optional identity helper but must not silently probe
  host identity by default.
- Writer lock behavior is removed from core writer runtime paths. If retained,
  it exists only as an optional independent helper/wrapper and is never
  described as systemd compatibility behavior.
- Tests or static runtime scans fail if core SDK runtime code imports/runs
  subprocess APIs or host identity probes.
- Existing compatibility, writer, reader, directory, live, and lock tests are
  updated to use explicit identities or optional helper paths.
- macOS validation is run on `PlakaM4mini` after merge.
- Windows validation is run on `win11` after merge.
- The parent SOW-0063 remains open until this SOW and real OS validation are
  complete.

## Analysis

Sources checked:

- User decisions in this thread.
- `ktsaou/netdata @ 445dd8eb845c`
  `src/crates/journal-log-writer/src/log/mod.rs`
- `ktsaou/netdata @ 445dd8eb845c`
  `src/crates/journal-common/src/system.rs`
- `ktsaou/netdata @ 445dd8eb845c`
  `src/crates/journal-core/src/file/writer.rs`
- `ktsaou/netdata @ 445dd8eb845c`
  `src/crates/journal-core/src/file/file.rs`
- Reviewed local branch evidence:
  - `codex/sow-0067-go-portability @ 60566a1`
  - `codex/sow-0068-rust-portability @ 57e3dc6`
  - `codex/sow-0069-python-portability @ de5e9dc`
  - `codex/sow-0070-node-portability @ dfa3af4`

Current state:

- Netdata's vendored high-level Rust log writer calls host identity helpers:
  - `src/crates/journal-log-writer/src/log/mod.rs:26` loads machine ID.
  - `src/crates/journal-log-writer/src/log/mod.rs:266` loads boot ID.
  - `src/crates/journal-log-writer/src/log/mod.rs:292` injects `_BOOT_ID`.
- Netdata's vendored low-level writer is cleaner:
  - `src/crates/journal-core/src/file/file.rs:94` takes `machine_id`,
    `boot_id`, and `seqnum_id` explicitly in `JournalFileOptions::new`.
  - `src/crates/journal-core/src/file/writer.rs:144` takes `boot_id`
    explicitly in `JournalWriter::new`.
  - `src/crates/journal-core/src/file/writer.rs:219` writes the provided boot
    ID to entry headers.
  - `src/crates/journal-core/src/file/writer.rs:279` writes the provided boot
    ID to the file header tail metadata.
- The current reviewed portability work uses platform probing and lock helpers:
  - Go portability branch uses `/proc` on Linux and `ps` on FreeBSD/macOS.
  - Rust portability branch uses `/proc`, `system_profiler`, and `sysctl`.
  - Python portability branch uses `/proc` helpers and standard lock helpers.
  - Node portability branch uses `/proc` helpers and probes `/etc/machine-id`
    before fallback.

Risks:

- Hidden host probing makes SDK behavior environment-dependent and hard to use
  in backend ingestion paths, containers, tests, restricted sandboxes, Windows
  services, and embedded environments.
- External commands in runtime SDK paths can hang, be unavailable, produce
  locale-dependent output, add latency, expand attack surface, or violate
  consumer expectations.
- Automatic stale-lock cleanup can delete a lock incorrectly if process identity
  evidence is weak. The safe core behavior is fail-closed or caller-managed
  locking.
- If these rules are not written into `AGENTS.md`, specs, and project skills,
  future agents may reintroduce the same architecture mistake.

## Pre-Implementation Gate

Status: ready after reviewed branch merge

Problem / root-cause model:

- The portability branches solved cross-platform build/runtime gaps by adding
  host identity probing and cooperative writer-lock convenience inside or near
  SDK runtime paths. That made platform tests pass inside the SOW scope, but it
  blurred four distinct responsibilities: journal file-format read/write,
  systemd/journald conventions, host identity discovery, and optional
  cooperating-writer locking.

Evidence reviewed:

- User architectural decision in this thread.
- Netdata vendored Rust evidence showing the low-level writer accepts explicit
  boot ID while the high-level log writer probes host identity.
- Reviewed portability branch evidence listed under Analysis.
- Project compatibility rules in `.agents/skills/project-journal-compatibility/SKILL.md`.

Affected contracts and surfaces:

- Rust, Go, Python, and Node.js core writer APIs.
- Rust, Go, Python, and Node.js directory/high-level writer APIs.
- Rust, Go, Python, and Node.js reader APIs where platform helpers are imported.
- File-backed journalctl rewrites only if they import runtime helpers
  unnecessarily.
- Product specs and SDK README/API docs.
- Project agent instructions and project skills.
- Conformance, interoperability, live, lock, portability, and cross-platform
  tests.

Existing patterns to reuse:

- Netdata vendored low-level writer pattern: explicit `machine_id`, `boot_id`,
  and `seqnum_id` inputs.
- Existing RAW/JOURNALD/JOURNAL-APP field policy split: keep policy layers
  separate from core file-format mechanics.
- Existing strict identity modes in the current SDK branches can be adapted
  into explicit default requirements.
- Existing lock matrix can be retargeted to optional lock helpers rather than
  core writers.

Risk and blast radius:

- High. This changes public API expectations and may affect every language.
- Medium merge risk because the SOW should run after seven reviewed branches
  are merged and their docs/spec changes reconciled.
- High release risk if skipped: consumers could depend on hidden host probing
  or implicit locking that should not be part of the core contract.

Sensitive data handling plan:

- Do not read host live journals.
- Do not record real machine IDs, boot IDs, process data, user names, command
  lines, private paths, customer data, credentials, bearer tokens, SNMP
  communities, or raw log payloads in durable artifacts.
- Use synthetic IDs in tests and docs.
- macOS/Windows validation reports must record only commands, pass/fail status,
  sanitized OS/runtime versions, and synthetic fixture results.

Implementation plan:

1. Merge the seven reviewed worktree branches in the orchestrator-selected
   order.
2. Update `AGENTS.md` first with the runtime-purity architecture so every
   follow-on agent inherits the rule.
3. Update project skills and product specs to make the four-layer split
   mandatory.
4. Refactor Rust:
   - keep core file-format writer explicit-ID only;
   - move identity discovery into optional helper module/crate;
   - move writer locking into optional helper/wrapper;
   - remove external commands and host identity probing from core runtime.
5. Refactor Go:
   - keep core file-format writer explicit-ID only;
   - move identity discovery and cooperating lock helpers outside core writer
     runtime;
   - remove `ps` and host identity probing from core runtime.
6. Refactor Python:
   - keep core file-format writer explicit-ID only;
   - move identity and lock helpers into optional modules;
   - remove core runtime `/proc` and `/etc/machine-id` probing.
7. Refactor Node.js:
   - keep core file-format writer explicit-ID only;
   - move identity and lock helpers into optional modules;
   - remove core runtime `/proc` and `/etc/machine-id` probing.
8. Update tests:
   - static runtime scans for forbidden core dependencies;
   - explicit-ID construction tests;
   - optional identity helper tests;
   - optional lock helper tests;
   - existing compatibility matrices adjusted to explicit IDs or opt-in
     wrappers.
9. Run Linux validation plus macOS validation on `PlakaM4mini` and Windows
   validation on `win11`.

Validation plan:

- Linux full affected test suites for Rust, Go, Python, and Node.js.
- Static scans that fail on forbidden core-runtime patterns:
  - subprocess APIs in core runtime;
  - `/proc`, `/etc/machine-id`, `/host/proc`, registry, `sysctl`,
    `system_profiler`, `ps`, and equivalent host identity paths in core
    runtime;
  - automatic lock acquisition in core writer constructors.
- Interoperability matrices for files written with explicit synthetic IDs.
- Optional identity helper tests on Linux, macOS, and Windows.
- Optional lock helper tests on Linux, macOS, and Windows.
- `ssh PlakaM4mini` validation using a repo clone at
  `~/src/systemd-journal-sdk/`.
- `ssh win11` validation using a repo clone at `~/src/systemd-journal-sdk/`
  under `MSYSTEM=MSYS`.
- `git diff --check`.
- `.agents/sow/audit.sh`.
- Whole-SOW read-only reviewer cycle with all approved reviewers voting
  `PRODUCTION GRADE` before closure.

Artifact impact plan:

- AGENTS.md: must be updated prominently.
- Runtime project skills: update journal compatibility and orchestration skills
  if prompt rules or compatibility checks change.
- Specs: update product scope and platform behavior specs.
- End-user/operator docs: update language READMEs and API docs.
- End-user/operator skills: no output/reference skills currently exist, but
  record that explicitly during validation.
- SOW lifecycle: SOW-0063 remains open until SOW-0071 and real OS validation
  complete; child SOW close-out must not overclaim portability.
- SOW-status.md: update both root and `.agents/sow/` status indexes.

Open-source reference evidence:

- `ktsaou/netdata @ 445dd8eb845c`
  `src/crates/journal-log-writer/src/log/mod.rs`
- `ktsaou/netdata @ 445dd8eb845c`
  `src/crates/journal-common/src/system.rs`
- `ktsaou/netdata @ 445dd8eb845c`
  `src/crates/journal-core/src/file/writer.rs`
- `ktsaou/netdata @ 445dd8eb845c`
  `src/crates/journal-core/src/file/file.rs`

Open decisions:

- Decision 1: The user accepted the four-layer architecture:
  core journal SDK, systemd compatibility layer, optional identity helper, and
  optional writer-lock helper.
- Decision 2: Writer-lock helper is independent from systemd compatibility and
  must not be bundled into a systemd-style high-level writer by default.
- Decision 3: Create this SOW before merging the reviewed worktree branches to
  prevent the requirement from being lost during compaction.

## Implications And Decisions

1. Runtime host probing
   - Selected: core SDK runtime must not host-probe.
   - Implication: callers must supply IDs or explicitly use an identity helper.
   - Risk: some existing convenience APIs become stricter, but behavior becomes
     predictable and portable.

2. External commands
   - Selected: no external commands in core SDK runtime.
   - Implication: macOS/BSD identity discovery must use native APIs or return
     unsupported from optional helpers until a user-approved command fallback is
     accepted.
   - Risk: identity helper coverage may initially be narrower, but core SDK
     purity is preserved.

3. Writer locking
   - Selected: core writer does not enforce locks; optional lock helper is
     independent from systemd compatibility.
   - Implication: one-writer correctness is caller responsibility unless the
     caller explicitly wraps the writer with the lock helper.
   - Risk: callers can misuse core writer, so docs must state the one-writer
     contract plainly.

4. Merge ordering
   - Selected: create SOW-0071 now, merge reviewed branches next, then execute
     SOW-0071 before closing SOW-0063 or publishing any stable API.
   - Implication: the reviewed branches are not discarded, but their technical
     debt is tracked and corrected before release.

## Plan

1. Update `AGENTS.md`, runtime project skills, product specs, and language docs
   with the four-layer runtime-purity architecture.
2. Remove implicit host identity probing from high-level writer auto modes and
   make auto modes use explicit caller IDs or SDK-local synthetic IDs only.
3. Remove lock-enable options from core writer constructors and high-level log
   writer configs. Keep lock behavior as explicit helper acquisition around
   writer use.
4. Update livewriter and lock matrix harnesses to acquire optional locks
   explicitly.
5. Add a static runtime-purity scan that fails on forbidden core-runtime host
   probing, subprocess APIs, and lock-enable options.
6. Run Linux, macOS, Windows, interoperability, directory, lock, and stock
   verification validation before reviewer handoff.

## Delegation Plan

Implementer:

- Local implementation by the orchestrator or a user-approved implementation
  agent after the reviewed branch merge. If parallelized again, split by
  language only after `AGENTS.md` and project skills are updated.

Reviewers:

- Whole-SOW read-only review by the approved reviewer pool:
  `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/qwen3.6-plus`,
  `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/minimax-m2.7-coder`, and
  `llm-netdata-cloud/mimo-v2.5-pro`.

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

- If macOS or Windows validation cannot run on the provided hosts, record the
  exact SSH/runtime blocker and stop before closing SOW-0063.
- If a language cannot represent the split cleanly without an API break, return
  with evidence and options before weakening the architecture.
- If existing compatibility tests depend on implicit identity/locking, update
  the tests to use explicit IDs or optional helpers rather than restoring
  hidden core behavior.

## Execution Log

### 2026-05-30

- Created before merging the seven reviewed worktree branches so the runtime
  purity and optional-platform-services correction is tracked durably.
- Recorded the user decisions separating core file-format SDK, systemd
  compatibility, identity discovery, and writer-lock convenience.
- Recorded Netdata vendored Rust evidence showing explicit low-level writer
  boot ID inputs and separate high-level host identity probing.
- Activated after the reviewed portability child SOWs SOW-0067 through
  SOW-0070 were reconciled and moved to `done/`.
- Updated `AGENTS.md`, `.agents/skills/project-agent-orchestration/SKILL.md`,
  `.agents/skills/project-journal-compatibility/SKILL.md`, and
  `.agents/sow/specs/product-scope.md` so future agents must preserve the
  core/systemd-compatibility/identity-helper/lock-helper split.
- Updated Rust, Go, Node.js, and Python docs to describe caller-provided or
  SDK-local synthetic identities and explicit optional lock helpers.
- Removed integrated writer-lock options from core writer constructors:
  Rust `JournalFileOptions::with_writer_lock` and `Config::with_writer_lock`,
  Go `Options.EnableLock`, Node `enableLock` / `enable_lock`, and Python
  `enable_lock` / `enableLock`.
- Kept optional lock helpers explicit and independent:
  Rust `journal_core::file::lock::WriterLock`, Go
  `journal.AcquireWriterLock(path)`, Node.js `WriterLock.acquire(path)`, and
  Python `journal.lock.WriterLock.acquire(path)`.
- Updated Rust, Go, Node.js, and Python livewriter harnesses plus lock tests to
  acquire optional locks explicitly around writer use.
- Removed Go BSD/macOS optional lock helper use of external `ps`; it now uses
  native boot-time evidence plus conservative process-liveness checks.
- Added `tests/runtime_purity/test_core_runtime_purity.py` static scan for
  core-runtime forbidden host probing, subprocess APIs, and lock-enable option
  names.
- Ran whole-SOW reviewer round 1 with Kimi, Qwen, GLM, Minimax, and Mimo; all
  five voted `PRODUCTION GRADE`.
- Resolved reviewer findings by:
  - strengthening the runtime-purity static guard with import-boundary checks
    and additional host-helper names;
  - splitting Python core file-I/O helpers into `python/journal/_platform_io.py`
    so core writers no longer import from the optional host-observation helper
    module;
  - removing the dead Go `readHostBootID()` implementation and the unused
    `readUUIDFile()` core helper;
  - replacing Rust optional identity-helper subprocess calls with native
    macOS `gethostuuid(3)` and macOS/FreeBSD `sysctlbyname("kern.boottime")`;
  - adding native macOS/FreeBSD boot-time evidence to the Rust optional lock
    helper so stale-lock checks are not PID-only on those targets.
- Ran same-scope whole-SOW reviewer round 2.
  - `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/minimax-m2.7-coder`,
    and `llm-netdata-cloud/mimo-v2.5-pro` voted `PRODUCTION GRADE` with only
    non-blocking observations.
  - `llm-netdata-cloud/kimi-k2.6` voted `PRODUCTION GRADE` but identified the
    legacy Rust `jf` crate as a same-pattern miss because it still exposed
    host identity helper code.
  - `llm-netdata-cloud/glm-5.1` voted `NOT PRODUCTION GRADE` because the
    runtime-purity static scan did not cover public facade files and because
    the legacy Rust `jf` crate still had subprocess identity helpers.
- Fixed round 2 findings by:
  - removing legacy `jf` host identity helper functions and the public
    `load_boot_id` re-export from `rust/src/crates/jf/journal_file`;
  - changing the legacy `jf` writer test to use synthetic boot ID bytes;
  - removing now-unused legacy `jf` `hex` dependencies;
  - expanding `tests/runtime_purity/test_core_runtime_purity.py` to scan public
    Rust/Go/Node.js/Python facade files and legacy `jf` file/writer runtime
    files;
  - expanding import-boundary checks to cover all scanned reader, writer,
    facade, and legacy `jf` runtime files.

## Validation

Acceptance criteria evidence:

- Core file-format writers no longer acquire locks implicitly:
  - Rust core writer has no `with_writer_lock` option and optional lock helper
    is exposed separately through `journal_core::file::lock::WriterLock`.
  - Go core writer `Options` no longer contains `EnableLock`; optional helper
    is `journal.AcquireWriterLock(path)`.
  - Node.js `Writer.create/open` and `Log` no longer accept or forward
    `enableLock` / `enable_lock`; optional helper is `WriterLock.acquire(path)`.
  - Python `Writer.create/open` and `Log` no longer accept or forward
    `enable_lock` / `enableLock`; optional helper is
    `journal.lock.WriterLock.acquire(path)`.
- High-level auto identity paths use explicit caller IDs or SDK-local synthetic
  UUIDs only; no core/high-level writer path reads `/etc/machine-id`, host boot
  ID, `/proc`, platform registries, or external commands to discover identity.
- `tests/runtime_purity/test_core_runtime_purity.py` scans the core runtime
  files for `/proc`, `/host/proc`, `/etc/machine-id`, `system_profiler`,
  `sysctl`, subprocess/spawn/exec APIs, host identity helper names, and
  lock-enable option names.

Tests or equivalent validation:

- PASS: `python -m unittest tests.runtime_purity.test_core_runtime_purity`.
- PASS: `cargo test -p journal-common -p journal-core -p journal-log-writer --tests`.
- PASS after round 2 fixes: `cargo test -p journal_file --tests`.
- PASS after round 2 fixes: `cargo test -p journal --tests`.
- PASS after final cleanup: `go test ./...` in `go/`.
- PASS: `npm test` in `node/` with repo-local npm cache configured.
- PASS: Python package tests with repo-local virtual environment.
- PASS: `python -m compileall python`.
- PASS: `python3 tests/interoperability/run_matrix.py --writers rust go node python --readers rust go node python stock --entries 10`; result: 104/104.
- PASS: `python3 tests/interoperability/run_directory_matrix.py --readers rust go node python stock`; result status `PASS`.
- PASS: `python3 tests/interoperability/run_lock_matrix.py`; result: 8/8.

Real-use evidence:

- macOS validation on `PlakaM4mini`, using
  `~/src/systemd-journal-sdk-sow71-validation`:
  - PASS runtime purity scan.
  - PASS Rust `journal-common` / `journal-core` / `journal-log-writer` tests,
    including native `gethostuuid(3)` and `sysctlbyname("kern.boottime")`
    compilation paths.
  - PASS Go `go test ./...`.
  - PASS Node package tests.
  - PASS Python 3.14 package tests with repo-local `lz4==4.4.5`.
- Windows validation on `win11` under `MSYSTEM=MSYS`, using
  `~/src/systemd-journal-sdk-sow71-validation`:
  - PASS runtime purity scan.
  - PASS Go `go test ./...`.
  - PASS Node package tests with repo-local Node v26.2.0.
  - PASS Python package tests in a repo-local venv with `lz4==4.4.5`.
  - Rust was not rerun in the post-review cleanup pass because `cargo` was not
    available in the current Windows SSH/MSYS path. The post-review Rust
    changes are `cfg(target_os = "macos")` / `cfg(target_os = "freebsd")`
    helper code plus Linux-validated shared code; pre-cleanup Windows Rust
    validation for the affected portability slice remains recorded in SOW-0063.
- Non-Linux generated-file stock verification:
  - Generated 3-entry journal files on macOS with Rust, Go, Node.js, and
    Python livewriters.
  - Generated 3-entry journal files on Windows with Rust, Go, Node.js, and
    Python livewriters.
  - Copied all 8 generated files back to Linux and verified each with stock
    `journalctl --verify --file`.

Reviewer findings:

- Round 1:
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`; findings covered
    runtime-purity scan/import gaps, dead Go identity helper scaffolding, Rust
    optional identity-helper subprocess use, Node helper module proximity,
    Python `_platform` import risk, and legacy `jf` host-probing context.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`; findings covered
    dead Go/Python host-helper code, Rust non-Linux lock boot evidence, and
    static scan scope.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`; findings covered dead Go
    helper code and scan coverage limitations.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`; findings
    covered Rust non-Linux optional lock boot evidence, runtime-scan platform
    notes, and optional-helper proximity risks.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`; findings covered
    helper-module scan gaps, indirect host-probing risk, and Rust test-section
    scan fragility.
- Round 1 dispositions:
  - Strengthened `tests/runtime_purity/test_core_runtime_purity.py` with
    additional forbidden helper names and explicit import-boundary checks.
  - Split Python core I/O helpers into `_platform_io.py` and retargeted core
    writers/tests to that module.
  - Removed dead Go `readHostBootID()` bodies and unused `readUUIDFile()` from
    core `log.go`.
  - Replaced Rust optional identity-helper subprocesses with native
    `gethostuuid(3)` and `sysctlbyname`.
  - Added macOS/FreeBSD boot-time evidence to Rust optional lock helper.
  - Left Node `platform.js` grouping as non-blocking because it is only used by
    optional lock/test code and the strengthened import-boundary test prevents
    core writers from importing it.
- Round 2:
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`; observations only.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`; observations
    covered legacy `jf` subprocess helper scope, `journal-common` helper
    re-exports, and platform lock-helper limitations.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`; observations
    covered hardcoded static-scan scope and non-blocking platform lock-helper
    limitations.
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`; identified the legacy
    Rust `jf` crate host identity helpers as a same-pattern miss and requested
    a fix or explicit scope carve-out.
  - `llm-netdata-cloud/glm-5.1`: `NOT PRODUCTION GRADE`; required runtime
    purity scan coverage for public facade files and explicit handling of the
    legacy Rust `jf` crate identity helpers.
- Round 2 dispositions:
  - Public facade files are now included in the static runtime-purity scan:
    `rust/src/journal/src/lib.rs`, `rust/src/journal/src/facade.rs`,
    `go/journal/facade.go`, `node/src/facade.js`, and
    `python/journal/facade.py`.
  - Legacy Rust `jf` runtime files are now included in the static scan:
    `rust/src/crates/jf/journal_file/src/file.rs` and
    `rust/src/crates/jf/journal_file/src/writer.rs`.
  - Legacy Rust `jf` host identity helper functions and public re-export were
    removed instead of carving out an exception.
  - Legacy Rust `jf` tests now use synthetic identity bytes.
  - `python -m unittest tests.runtime_purity.test_core_runtime_purity`,
    `cargo test -p journal_file --tests`, and `cargo test -p journal --tests`
    passed after the fixes.
- Round 3:
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`; observations covered
    Rust test-section scan fragility, `journal-common` optional identity helper
    re-exports, and the already-recorded Windows Rust validation gap.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`; no blocking
    findings.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`; observations covered
    optional lock-helper access consistency and a dead Go helper.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`; observations
    covered manual static-scan maintenance, dead Go helper/stub files, and
    lock-helper API consistency.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`; observations
    covered adding Python `_platform_io.py` to scan coverage and
    `journal-common` optional identity helper re-exports.
- Round 3 dispositions:
  - Removed the unused Go `unlockAndClose()` helper and reran `go test ./...`.
  - Added `python/journal/_platform_io.py` to runtime-purity scan coverage and
    reran the scan plus `python -m compileall python`.
  - Kept Go `boot_id_*` stub files as explicit architecture markers. They have
    no runtime behavior, and deleting files requires explicit user approval.
  - Kept `journal-common` identity helper re-exports as the Rust optional
    identity-helper API surface. Core runtime files are forbidden from using
    those names by the static scan.
  - Kept the Rust test-section scanner as-is because it strips the large
    terminal `#[cfg(test)] mod tests` blocks that contain stock-tool
    subprocess checks. Inline `#[cfg(test)]` helpers before that block remain
    scanned, which is conservative and can only create a false positive.
  - Treated cross-language lock-helper access differences as acceptable API
    idioms because every language keeps acquisition explicit and outside core
    writer constructors.
  - Windows Rust post-cleanup validation gap remains mapped to SOW-0063.

Same-failure scan:

- Static scan passes for core reader, writer, facade, Python core I/O helper,
  and legacy Rust `jf` runtime files.
- Text search found no remaining host-observation code in legacy Rust `jf`
  runtime files.
- Remaining host-observation code is limited to optional helper, benchmark,
  test, and documentation contexts. The scanned runtime files do not contain
  the forbidden patterns.

Sensitive data gate:

- Validation used synthetic generated journals and did not inspect live host
  journals. Durable artifacts record only source paths, commands, pass/fail
  status, sanitized host labels, and synthetic fixture outcomes. No real boot
  IDs, machine IDs, process data, log payloads, credentials, SNMP communities,
  customer identifiers, personal data, private endpoints, or proprietary
  incident details were written.

Artifact maintenance gate:

- AGENTS.md: updated with the runtime-purity architecture and explicit
  prohibition on lock-enable options in core writer constructors.
- Runtime project skills: updated orchestration and journal compatibility
  skills with runtime-purity and reviewer prompt requirements.
- Specs: updated `.agents/sow/specs/product-scope.md` with core/systemd
  compatibility/identity-helper/lock-helper split and per-language behavior.
- End-user/operator docs: updated Rust, Go, Node.js, and Python README/API docs.
- End-user/operator skills: no output/reference skills currently exist.
- SOW lifecycle: moved from `pending/` to `current/` during implementation and
  from `current/` to `done/` at closure.
- SOW-status.md: root and `.agents/sow/` status indexes updated.

Lessons extracted:

- Cross-platform portability can easily drift into host-observation behavior.
  The project needs explicit architecture rules and static checks, not just
  reviewer judgment.
- "Disabled by default" is not strong enough for the core lock contract. The
  core writer APIs must avoid lock-enable options entirely; optional locks are
  separate acquire/release helpers around writer use.

Follow-up mapping:

- Parent blocker: `SOW-0063-20260530-cross-platform-portability.md`.
- Release blocker: `SOW-0066-20260530-v1-release-and-registry-publication.md`.
- Netdata integration blocker until API behavior is stable:
  `SOW-0047`, `SOW-0048`, `SOW-0049`, and `SOW-0050`.

## Outcome

Implemented and locally/macOS/Windows validated. Round 1 and round 2 findings
were fixed. Round 3 returned five `PRODUCTION GRADE` votes; non-blocking
observations were either fixed, intentionally retained with rationale, or
mapped to existing SOW-0063 validation. Final local validation passed.

## Lessons Extracted

- Runtime-purity requirements must be encoded in tests and instructions, not
  only remembered from discussion.
- Optional platform services need API separation, not just boolean toggles on
  core constructors.

## Followup

None. Non-blocking reviewer observations were fixed, rejected with rationale,
or mapped to existing SOW-0063/SOW-0066 work where applicable.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
