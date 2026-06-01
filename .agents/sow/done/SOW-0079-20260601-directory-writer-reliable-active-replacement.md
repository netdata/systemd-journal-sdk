# SOW-0079 - Directory Writer Reliable Active Replacement

## Status

Status: completed

Sub-state: implemented, validated, second whole-SOW reviewer batch passed,
SOW audit clean, and closed.

## Requirements

### Purpose

Make high-level directory writers behave like journald when an existing active
journal file is not safe to append: move the old active file out of the normal
`.journal` set with a disposed `*.journal~` name and create a fresh active file
instead of returning a low-level unsupported-file error to the caller.

### User Request

The user asked whether journald creates a new file instead of appending to an
old incompatible file, then asked to open and fix the behavior for all
implementations.

### Assistant Understanding

Facts:

- `systemd` `journal_file_open()` is a low-level file open API and can return
  errors for unsupported/corrupt files.
- `systemd` `journal_file_open_reliably()` wraps low-level open for writable
  `*.journal` files: on errors including `-EPROTONOSUPPORT`, it disposes the
  existing file and creates a new one.
- `journald` also calls `journal_file_rotate_suggested()` before append; that
  function suggests rotation when the active file header is older than the
  current `Header` size.
- SOW-0077 changed the Rust low-level append-open path to return a controlled
  unsupported-file error for unkeyed files instead of panicking.
- Go, Python, and Node.js low-level writer opens already return controlled
  errors for unkeyed files.

Inferences:

- The low-level writer open APIs may keep returning controlled unsupported
  errors because they do not own a directory chain or replacement policy.
- The high-level directory writers do own that context and should handle
  append-incompatible or outdated active files by moving the old active file
  away and creating a fresh active file.

Unknowns:

- None blocking. systemd-style disposed naming is the target behavior for
  replaceable active-open failures.

### Acceptance Criteria

- Rust, Go, Python, and Node.js high-level directory writers do not fail when
  an existing active file is unkeyed or otherwise rejected by the low-level
  writer as unsupported.
- Rust, Go, Python, and Node.js high-level directory writers do not append to
  active files whose on-disk header is older than the implementation's current
  header size; they rotate/replace before append.
- Existing replaceable active files are moved away using a disposed,
  collision-safe `*.journal~` name rather than overwritten or kept as normal
  directory history.
- Low-level writer APIs still return controlled errors when called directly on
  unsupported active files.
- Tests cover strict systemd active naming and non-strict chain naming where
  each implementation supports both.
- Existing writer, reader, directory, and interoperability tests keep passing
  for the affected slices.

## Analysis

Sources checked:

- `systemd/systemd @ cf3156842209`
  - `src/shared/journal-file-util.c:492`: `journal_file_open_reliably()`.
  - `src/shared/journal-file-util.c:516`: replaceable open errors include
    `-EPROTONOSUPPORT`.
  - `src/shared/journal-file-util.c:549`: existing file is disposed before
    replacement.
  - `src/libsystemd/sd-journal/journal-file.c:4624`: outdated header suggests
    rotation.
  - `src/journal/journald-manager.c:974`: journald checks rotation suggestion
    before append.
- `rust/src/crates/journal-log-writer/src/log/mod.rs`: high-level Rust
  directory writer currently propagates `ActiveFile::open()` errors.
- `go/journal/log.go`: Go directory writer currently propagates
  `OpenWithOptions()` errors when attaching existing active files.
- `python/journal/directory_writer.py`: Python directory writer currently
  propagates `Writer.open()` errors when attaching existing active files.
- `node/src/lib/directory-writer.js`: Node.js directory writer currently
  propagates `Writer.open()` errors when attaching existing active files.

Current state:

- Low-level writer opens are controlled, but high-level directory writers do
  not yet reliably replace unsupported/outdated active files in all languages.

Risks:

- Incorrect replacement can lose readable history by overwriting or deleting
  old files.
- Incorrect sequence/boot identity propagation can create awkward chains.
- Changing open behavior affects startup and lazy-open paths for production
  writers.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The SDK split low-level writer append-open from high-level directory writer
  lifecycle. SOW-0077 correctly made low-level append-open return
  `UnsupportedJournalFile`, but the high-level directory writer still treated
  that as fatal. systemd separates the same concerns: low-level open may fail,
  while `journal_file_open_reliably()` and journald rotation policy move the
  old active file away and create a new one.

Evidence reviewed:

- `systemd/systemd @ cf3156842209`
  - `src/shared/journal-file-util.c:492`
  - `src/shared/journal-file-util.c:516`
  - `src/shared/journal-file-util.c:549`
  - `src/libsystemd/sd-journal/journal-file.c:4624`
  - `src/journal/journald-manager.c:974`
- Rust, Go, Python, and Node.js directory writer attach/open paths listed in
  the Analysis section.

Affected contracts and surfaces:

- Rust `journal-log-writer` directory/high-level writer.
- Go `journal.Log` directory/high-level writer.
- Python `journal.directory_writer.DirectoryWriter`.
- Node.js `DirectoryWriter`.
- Tests and docs/specs that describe writer replacement behavior.

Existing patterns to reuse:

- Existing strict systemd active naming and chain archive naming helpers.
- Existing low-level writer unsupported errors.
- Existing directory-writer rotation and retention tests.
- Existing SOW-0077 controlled low-level error behavior.

Risk and blast radius:

- Medium: startup/open behavior changes across all writer SDKs.
- Data-loss risk is controlled by renaming/moving old active files rather than
  truncating or overwriting them.
- Compatibility risk is controlled by keeping low-level direct-open behavior as
  controlled errors and only changing high-level directory lifecycle behavior.

Sensitive data handling plan:

- Use synthetic test journals only. Do not copy raw host journals, field
  payloads, credentials, SNMP communities, personal data, private endpoints, or
  customer identifiers into durable artifacts.

Implementation plan:

1. Add or reuse helpers per language to detect append-incompatible and
   outdated active files.
2. Update directory-writer attach/open paths to move those active files to
   disposed `*.journal~` names and continue with fresh active file creation.
3. Add tests in Rust, Go, Python, and Node.js for strict and chain naming where
   practical.
4. Update specs/docs/project skill if the behavior becomes a reusable contract.

Validation plan:

- Language-local writer tests for Rust, Go, Python, and Node.js.
- Existing directory writer tests.
- Cross-language directory/interoperability smoke where practical.
- `git diff --check` and `.agents/sow/audit.sh`.
- Whole-SOW reviewer batch after complete implementation and local validation.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: update journal compatibility skill with reliable
  high-level writer replacement rule.
- Specs: update product scope with low-level versus high-level writer behavior.
- End-user/operator docs: update language docs if directory writer behavior is
  documented.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: normal current to done lifecycle after validation/review.
- SOW-status.md: update when opened/completed.

Open-source reference evidence:

- `systemd/systemd @ cf3156842209`
  - `src/shared/journal-file-util.c:492`
  - `src/shared/journal-file-util.c:516`
  - `src/shared/journal-file-util.c:549`
  - `src/libsystemd/sd-journal/journal-file.c:4624`
  - `src/journal/journald-manager.c:974`

Open decisions:

- None. The user selected journald-like high-level replacement behavior.

## Implications And Decisions

User decision:

1. Apply journald-like replacement behavior to all high-level directory
   writers, while low-level direct writer opens keep returning controlled
   errors.

## Plan

1. Implement Rust high-level directory writer reliable replacement.
2. Port equivalent behavior to Go, Python, and Node.js.
3. Add tests and update specs/docs/skills.
4. Validate locally and run whole-SOW reviewers.

## Delegation Plan

Implementer:

- Local implementation unless the user explicitly changes routing.

Reviewers:

- Reviewer pool after complete implementation and local validation: minimax,
  kimi, qwen, glm, and mimo.

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

- If sequence continuity and disposed-file replacement conflict in a specific
  language, pause with evidence and options.

## Execution Log

### 2026-06-01

- Opened SOW from user request after systemd source inspection.
- Corrected implementation target after checking `journal_file_dispose()`:
  replaceable active-open failures move the old active file to `*.journal~`,
  not to normal archived history.
- Implemented low-level outdated-header append-open rejection and high-level
  disposed active replacement in Rust, Go, Python, and Node.js.
- Added strict `<source>.journal` and default chain-active replacement tests in
  Rust, Go, Python, and Node.js.
- Updated product scope, project journal compatibility skill, and language
  README files to document the low-level/direct versus high-level/directory
  behavior split.
- First reviewer batch returned five `PRODUCTION GRADE` votes and low-risk
  findings. Fixed the actionable findings: Rust raw-header fallback for tail
  identity when a replaceable active file cannot be fully opened, Rust ENOENT
  handling during dispose rename, Rust append-open dead sanitized-header state,
  Python second-open replaceable-error handling, and Go/Python/Node.js
  stale-online-file identity guards.
- Second reviewer batch returned five `PRODUCTION GRADE` votes. One reviewer
  raised a medium offset concern for Rust raw-header fallback; this was
  dispositioned as a false positive after checking systemd's `Header` field
  order and the existing Rust, Go, Python, and Node.js parsers.

## Validation

Acceptance criteria evidence:

- Rust:
  - `rust/src/crates/journal-core/src/file/file.rs`: append-open rejects
    headers older than the current writer header and unkeyed historical files.
  - `rust/src/crates/journal-core/src/file/writer.rs`: direct writer
    construction rejects unkeyed/outdated append targets before mutation.
  - `rust/src/crates/journal-log-writer/src/log/chain.rs`: replaceable active
    files are moved to collision-safe `*.journal~` names.
  - `rust/src/crates/journal-log-writer/tests/log_writer.rs`: covers default
    chain active unkeyed replacement and strict active outdated-header
    replacement, preserving next sequence number `3`.
- Go:
  - `go/journal/writer.go`: direct append-open rejects outdated headers.
  - `go/journal/log.go`: `NewLog()` replaces low-level unsupported active
    open failures with disposed `*.journal~` files and fresh active creation.
  - `go/journal/log_test.go`: covers default chain active unkeyed replacement
    and strict active outdated-header replacement, preserving next sequence
    number `3`.
- Python:
  - `python/journal/writer.py`: direct append-open rejects outdated headers.
  - `python/journal/directory_writer.py`: strict and default active attach/open
    replace low-level unsupported active open failures with disposed
    `*.journal~` files and fresh active creation.
  - `python/test_all.py`: covers default chain active unkeyed replacement and
    strict active outdated-header replacement, preserving next sequence number
    `3`.
- Node.js:
  - `node/src/lib/writer.js`: direct append-open rejects outdated headers.
  - `node/src/lib/directory-writer.js`: strict and default active attach/open
    replace low-level unsupported active open failures with disposed
    `*.journal~` files and fresh active creation.
  - `node/test/all.js`: covers default chain active unkeyed replacement and
    strict active outdated-header replacement, preserving next sequence number
    `3`.

Tests or equivalent validation:

- PASS: `cargo fmt`
- PASS: `gofmt -w journal/log.go journal/writer.go journal/log_test.go`
- PASS: `python3 -m compileall python/journal python/test_all.py`
- PASS: `cargo test -p journal-log-writer replaces_ -- --nocapture`
- PASS: `cargo test -p journal-core open_for_append_rejects_unkeyed_journal_without_mutation -- --nocapture`
- PASS: `cargo test -p journal-core writer_constructor_rejects_unkeyed_journal_without_panic -- --nocapture`
- PASS: `go test ./journal -run 'TestLogReplaces'`
- PASS: Python targeted replacement tests:
  `test_directory_writer_replaces_unsupported_chain_active()` and
  `test_directory_writer_replaces_outdated_strict_active()`.
- PASS: `node --check node/src/lib/directory-writer.js`
- PASS: `node --check node/src/lib/writer.js`
- PASS: `node node/test/all.js`
- PASS: `cargo test -p journal-core -p journal-log-writer`
- PASS: `go test ./...`
- PASS after reviewer fixes: `cargo fmt`
- PASS after reviewer fixes: `gofmt -w journal/log.go`
- PASS after reviewer fixes: `python3 -m compileall python/journal python/test_all.py`
- PASS after reviewer fixes: `node --check node/src/lib/directory-writer.js`
- PASS after reviewer fixes: `cargo test -p journal-log-writer replaces_ -- --nocapture`
- PASS after reviewer fixes: `go test ./journal -run 'TestLogReplaces' -v`
- PASS after reviewer fixes: Python targeted replacement tests:
  `test_directory_writer_replaces_unsupported_chain_active()` and
  `test_directory_writer_replaces_outdated_strict_active()`.
- PASS after reviewer fixes: `node node/test/all.js`
- PASS after reviewer fixes: `cargo test -p journal-core -p journal-log-writer`
- PASS after reviewer fixes: `go test ./...`
- PASS: `python3 tests/interoperability/run_directory_matrix.py` with stock
  `journalctl`, Rust, Go, Node.js, and Python readers; status `PASS`.
- PASS: `python3 tests/interoperability/run_matrix.py --writers rust go python node --readers rust go python node stock --entries 10`; 104/104 checks passed.
- PASS after reviewer fixes: `python3 tests/interoperability/run_matrix.py --writers rust go python node --readers rust go python node stock --entries 10`; 104/104 checks passed.
- PASS after reviewer fixes: `python3 tests/interoperability/run_directory_matrix.py` with stock
  `journalctl`, Rust, Go, Node.js, and Python readers; status `PASS`.
- PASS: `git diff --check`
- PASS: `.agents/sow/audit.sh`

Blocked / limited validation:

- `PYTHONPATH=.local/python-deps:python python3 python/test_all.py` was
  attempted after the first full-suite attempt failed without `.local` LZ4
  dependencies. The dependency-enabled full suite was interrupted after more
  than eight minutes because it spawned `python/adapter.py run` and remained
  CPU-bound with no output. The replacement-specific Python tests, compile
  check, directory matrix, and cross-language matrix passed.

Real-use evidence:

- The directory matrix uses stock `journalctl --directory` from
  `systemd 260 (260.1-2-manjaro)` plus repository Rust, Go, Node.js, and Python
  journalctl rewrites against generated directory fixtures.

Reviewer findings:

- Round 1:
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`. Low findings covered
    Rust dead sanitized-header state, Rust/other-language identity-path
    differences, Python second-open defensive handling, and naming differences.
    Actionable items were fixed or dispositioned.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`. Low finding
    covered Rust dispose rename ENOENT parity with Go/Python/Node.js. Fixed.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`. Low findings covered
    Python/Node.js string error matching and naming differences. Disposition:
    accepted as non-blocking because the replaceable low-level errors are local
    SDK errors and systemd reliable-open also treats protocol-unsupported
    errors broadly.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`. No blocking
    findings.
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`. Low findings covered
    Rust tail identity using full reader open instead of raw header fallback,
    Go/Python/Node.js stale online file identity overwrite in strict-mode
    archive, Python second-open handling, Rust dead sanitized-header state, and
    Node.js disposed filename wall-clock readability. Actionable items were
    fixed; Node.js naming is non-blocking because disposed names require
    collision safety and `*.journal~`, not sortable wall-clock timestamps.

Second reviewer batch:

- `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`. Non-blocking observations
  covered Rust identity-path differences, Python/Node.js string matching, and
  Rust raw-header offset constants. No blocker.
- `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`. Non-blocking
  observations covered disposed-name timestamp/suffix variation and
  Python/Node.js string matching. No blocker.
- `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`. Non-blocking
  observations covered hardcoded offsets, string matching, duplicated
  Python/Node.js replacement control flow, and safe continuation after
  replacement. No blocker.
- `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`. Non-blocking
  observations covered hardcoded offsets, Node.js high-resolution timestamp
  naming, and string matching. The reviewer also reran targeted language tests
  after correcting initial command working directories.
- `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`. Raised a medium Rust
  raw-header offset concern. Disposition: false positive. Evidence:
  `systemd/systemd @ cf3156842209`
  `src/libsystemd/sd-journal/journal-def.h:223` has
  `entry_array_offset`, `:224` has `head_entry_realtime`, `:225` has
  `tail_entry_realtime`, `:226` has `tail_entry_monotonic`, and `:228` has
  `n_data`. Local parsers match this layout:
  `rust/src/crates/journal-core/src/file/object.rs:322`,
  `rust/src/crates/journal-core/src/file/object.rs:323`,
  `rust/src/crates/journal-core/src/file/object.rs:325`,
  `go/journal/format.go:319`,
  `go/journal/format.go:320`,
  `go/journal/format.go:322`,
  `python/journal/header.py:122`,
  `python/journal/header.py:123`,
  `python/journal/header.py:125`,
  `node/src/lib/header.js:147`,
  `node/src/lib/header.js:148`, and
  `node/src/lib/header.js:150`.

Same-failure scan:

- Same failure class checked in all four low-level writer open paths and all
  four high-level directory writer active attach/open paths.

Sensitive data gate:

- PASS. Tests use synthetic journal directories, synthetic machine/boot IDs,
  and synthetic field payloads only.

Artifact maintenance gate:

- AGENTS.md: no update needed; repository-wide layer separation and SOW rules
  already cover this behavior.
- Runtime project skills: updated
  `.agents/skills/project-journal-compatibility/SKILL.md` with the reliable
  high-level directory replacement rule.
- Specs: updated `.agents/sow/specs/product-scope.md` with the low-level versus
  high-level writer behavior split.
- End-user/operator docs: updated `rust/README.md`, `go/README.md`,
  `node/README.md`, and `python/README.md`.
- End-user/operator skills: no output/reference skills are affected.
- SOW lifecycle: moved from `current/` to `done/` with `Status:
  completed`.
- SOW-status.md: updated when opened and when closed.

Specs update:

- Updated `.agents/sow/specs/product-scope.md`.

Project skills update:

- Updated `.agents/skills/project-journal-compatibility/SKILL.md`.

End-user/operator docs update:

- Updated language READMEs for Rust, Go, Node.js, and Python.

End-user/operator skills update:

- No end-user/operator skills are present for this behavior.

Lessons:

- `journal_file_open_reliably()` does not archive replaceable active-open
  failures as normal history. It calls `journal_file_dispose()`, moving the old
  file to `*.journal~`. Future agents should not conflate normal rotation with
  reliable-open replacement.

Lessons extracted:

- `journal_file_open_reliably()` and journald rotation behavior must be kept
  distinct from low-level writer open APIs. Direct writer open may reject an
  unsupported append target; high-level directory writers own replacement.
- Header offset constants in raw fallback code should stay tied to the shared
  header layout because they are used specifically when the full reader refuses
  to open the file.

Follow-up mapping:

- No new follow-up SOW is required for the implemented behavior. The
  dependency-enabled Python full-suite adapter hang is recorded as a validation
  limitation here; it should be investigated only if it reproduces outside this
  SOW's replacement-specific tests or blocks a later Python-focused SOW.

## Outcome

Rust, Go, Python, and Node.js high-level directory writers now replace
append-incompatible or outdated active files with disposed `*.journal~` files
and create fresh active files. Low-level writer opens still return controlled
errors when called directly on unsupported append targets.

## Lessons Extracted

- Reliable active replacement is high-level directory writer behavior, not a
  low-level file-format writer behavior.
- Disposed active files must leave the normal `.journal` readable set; normal
  archive naming is reserved for readable history.
- Raw-header fallback is required to preserve chain identity when the full
  reader rejects the old active file for the same reason the writer cannot
  append to it.

## Followup

None.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
