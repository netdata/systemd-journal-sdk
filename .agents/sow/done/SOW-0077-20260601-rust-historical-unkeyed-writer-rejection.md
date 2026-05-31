# SOW-0077 - Rust Historical Unkeyed Writer Rejection

## Status

Status: completed

Sub-state: implementation, validation, reviewer batch, follow-up mapping, and SOW close complete.

## Requirements

### Purpose

Prevent a historical-reader compatibility fix from leaving a writer API footgun:
attempting to append through the Rust writer to an old unkeyed journal should
fail with a controlled error, not an assertion panic.

### User Request

The user asked that technical debt not be left behind as journal compatibility
work progresses. During SOW-0073 review, a non-blocking Rust writer behavior
difference was identified and accepted as out of scope for that reader-only SOW.

### Assistant Understanding

Facts:

- SOW-0073 removed reader-only keyed-hash open gates for historical unkeyed
  journal files.
- Go, Python, and Node.js writer append-open paths explicitly reject historical
  unkeyed files.
- Rust-created files are keyed-hash by default.
- The Rust append hot path asserts that the opened journal has the keyed-hash
  flag before writing an entry.

Inferences:

- Rust currently prevents writing unkeyed append data, but the failure mode may
  be an assertion panic instead of a controlled API error.
- This is a writer API hardening/parity item, not a reader compatibility item.

Unknowns:

- None for the current implementation slice. The audit found the two guard
  points needed for this SOW.

### Acceptance Criteria

- Rust writer append-open behavior for historical unkeyed files is audited from
  public API entry points down to `JournalWriter`.
- Attempting to append through supported Rust writer APIs to an unkeyed
  historical file returns a documented error before entry mutation, without
  panicking.
- Go, Python, and Node.js behavior is rechecked so the SOW records the
  cross-language writer contract.
- Tests cover Rust historical unkeyed append rejection using a synthetic
  fixture, without committing raw host journals.
- Reader support from SOW-0073 remains unchanged.

## Analysis

Sources checked:

- `rust/src/crates/journal-core/src/file/writer.rs`: the append hot path asserts
  `HeaderIncompatibleFlags::KeyedHash`.
- `rust/src/crates/journal-log-writer/src/log/mod.rs`: Rust creates new journal
  files with keyed-hash enabled.
- SOW-0073 reviewer batch: all reviewers accepted the reader work as
  production-grade and treated this writer item as non-blocking.

Current state:

- Rust now rejects append-open on unkeyed files before returning a mutable
  append handle and rejects direct `JournalWriter` construction on unkeyed files
  before the append assertion can be reached.

Risks:

- Changing writer open behavior can affect directory writer reopen paths and
  append-to-existing workflows.
- This work must not weaken reader support for historical unkeyed files.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The Rust writer has a keyed-hash assertion in the entry append path. If a
  public append-open API can reach that path with a historical unkeyed file, the
  caller may see a panic instead of a controlled unsupported-file error.

Evidence reviewed:

- `rust/src/crates/journal-core/src/file/writer.rs`: keyed-hash assertion in
  `add_entry_fields_with_options`.
- `rust/src/crates/journal-log-writer/src/log/mod.rs`: new files use
  `.with_keyed_hash(true)`.
- `rust/src/crates/journal-core/src/file/file.rs`: `open_for_append()` opens
  the file read/write, maps the header, and currently returns a mutable
  `JournalFile` before any keyed-hash check.
- `rust/src/crates/journal-log-writer/src/log/mod.rs`: `ActiveFile::open()`
  calls `open_for_append()` and then mutates the file state to online before
  constructing `JournalWriter`.
- `rust/src/crates/journal-core/src/file/writer.rs`: `JournalWriter::new()`
  delegates through `new_with_compression()`, so that constructor is the common
  low-level writer guard.
- SOW-0073 reviewer notes.

Affected contracts and surfaces:

- Rust writer append-open API.
- Rust directory writer reopen behavior, if it can reopen historical files.
- Cross-language writer unsupported-file behavior.
- Tests and documentation for historical unkeyed writer behavior.

Existing patterns to reuse:

- Historical 240-byte header fixtures from SOW-0073.
- Existing writer append-open rejection tests in Go, Python, and Node.js.
- Rust `JournalError` unsupported/invalid-file errors.

Risk and blast radius:

- Low if the change is limited to an earlier explicit Rust error.
- Medium if directory writer active-file discovery needs to distinguish old
  historical files from current active SDK files.

Sensitive data handling plan:

- Use synthetic historical headers or generated fixtures only. Do not copy raw
  host journal payloads or durable field values into this repository.

Implementation plan:

1. Add a `JournalError` variant for unsupported journal files.
2. Reject unkeyed files in `JournalFile::<MmapMut>::open_for_append()` before
   returning a mutable append handle, so `ActiveFile::open()` cannot mutate
   state first.
3. Reject unkeyed files in `JournalWriter::new_with_compression()` as a
   defense for direct construction from a mutable `JournalFile`.
4. Add Rust tests for both append-open and direct writer-construction
   rejection using synthetic unkeyed files.
5. Recheck Go, Python, and Node.js writer behavior and document the shared
   contract.

Validation plan:

- Rust targeted tests for append rejection.
- Existing Rust reader historical unkeyed test/matrix still passes.
- Relevant writer smoke tests still pass.
- Whole-SOW read-only reviewer batch after implementation.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: update only if a new mandatory writer rule is
  established.
- Specs: update historical writer behavior if the public contract changes.
- End-user/operator docs: update Rust writer docs if append-open behavior is
  documented there.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: normal pending to current to done lifecycle.
- SOW-status.md: update when started/completed.

Open-source reference evidence:

- None needed yet; this is a local SDK API hardening item.

Open decisions:

- None. The controlled error should be returned at append-open and also at
  writer construction as defense in depth.

## Implications And Decisions

No user decision is required before investigation. If the audit finds multiple
publicly visible failure locations with different compatibility implications,
the options should be brought back to the user before implementation.

## Plan

1. Audit Rust append-open paths and writer construction.
2. Implement controlled error and tests.
3. Validate reader support remains unchanged.
4. Run whole-SOW reviewer batch.

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

- If a clean error path requires a public API decision, pause and ask the user
  with evidence and options.

## Execution Log

### 2026-06-01

- Created as follow-up tracking from SOW-0073 reviewer findings.
- Started after the SOW-0073 close commit was pushed. Audit found two necessary
  Rust guard points: append-open before state mutation and direct
  `JournalWriter` construction before the append assertion can be reached.
- Added `JournalError::UnsupportedJournalFile`.
- Updated `JournalFile::<MmapMut>::open_for_append()` to reject unkeyed files
  before returning a mutable append handle.
- Updated `JournalWriter::new_with_compression()` to reject unkeyed files as a
  low-level defense for direct writer construction.
- Added Rust tests covering append-open rejection without mutation and direct
  writer-construction rejection without panic.
- Updated product scope, Rust README, project compatibility skill, and SOW
  status to record the controlled writer rejection contract.
- Ran the whole-SOW reviewer batch. All five reviewers voted
  `PRODUCTION GRADE`.
- Found one related legacy `jf` writer assertion path outside this SOW's
  supported writer stack and tracked it as SOW-0078.

## Validation

Acceptance criteria evidence:

- Rust writer append-open behavior was audited from
  `journal-log-writer::ActiveFile::open()` through
  `JournalFile::<MmapMut>::open_for_append()` and
  `JournalWriter::new_with_compression()`.
- Attempting to append-open an unkeyed file now returns
  `JournalError::UnsupportedJournalFile` before the mutable append handle is
  returned and before `ActiveFile::open()` can set the state to online.
- Direct low-level Rust writer construction on an unkeyed `JournalFile<MmapMut>`
  now returns `JournalError::UnsupportedJournalFile` before the append hot-path
  keyed-hash assertion can be reached.
- Go, Python, and Node.js writer behavior was rechecked in source:
  `go/journal/writer.go` rejects append-open without `incompatibleKeyedHash`;
  `python/journal/writer.py` raises `ValueError('unsupported journal: keyed
  hash required')`; `node/src/lib/writer.js` throws `unsupported journal:
  keyed hash required`.
- Reader support from SOW-0073 remained unchanged and was revalidated with the
  v239 historical unkeyed/LZ4 offline matrix.

Tests or equivalent validation:

- `cargo fmt`: passed.
- `cargo test -p journal-core unkeyed`: passed, 2 tests.
- `cargo test -p journal-core`: passed, 69 tests.
- `cargo test -p journal-log-writer`: passed, 48 integration tests plus crate
  tests and doc test.
- `python3 tests/systemd_matrix/run_systemd_matrix.py test --version v239 --case historical-unkeyed-lz4-offline --journal .local/systemd-matrix/versions/old-enterprise/corpus/v239/v239-compressed-offline.journal --version-journalctl .local/systemd-matrix/versions/old-enterprise/build/v239/journalctl --timeout 300`:
  passed with `status: ok`, no discrepancies, and the expected
  `VERSION_EXPORT_METADATA_DRIFT` observation.
- `git diff --check`: passed after SOW close updates.
- `.agents/sow/audit.sh`: passed after SOW close updates with clean verdict.

Real-use evidence:

- Not applicable beyond the SOW-0073 RHEL 8.10 historical-reader evidence.
  This SOW uses synthetic unkeyed files for writer rejection so no raw host
  journals or payloads are copied into durable artifacts.

Reviewer findings:

- `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`. Verified append-open
  rejection before mutation, direct writer-construction rejection before the
  assertion path, unchanged historical reader support, cross-language writer
  behavior, and docs/spec updates.
- `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`. Verified both Rust
  guard points and Go/Python/Node.js parity. Non-blocking observation: the
  legacy `rust/src/crates/jf/journal_file` writer still publicly re-exports a
  writer with an unkeyed-file assertion path; this is outside the supported
  current writer stack and is tracked by SOW-0078.
- `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`. Verified the reader open
  path still has no keyed-hash gate and that `open_for_append()` and
  `JournalWriter::new_with_compression()` are the relevant supported Rust
  writer guards.
- `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`. Verified
  no-mutation behavior, direct construction guard, unchanged reader support,
  cross-language parity, and artifact consistency.
- `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`. Verified the same
  acceptance criteria and noted only the normal explicit-staging requirement
  for the moved SOW file.

Same-failure scan:

- Rust append-open and direct writer-construction paths were searched. The
  remaining append hot-path assertion is now defended by earlier controlled
  errors and remains an internal invariant for keyed writer state.
- Go, Python, and Node.js append-open keyed-hash gates were rechecked and
  already return controlled errors.
- Legacy `rust/src/crates/jf/journal_file/src/lib.rs` publicly re-exports
  `writer::JournalWriter`, and
  `rust/src/crates/jf/journal_file/src/writer.rs` still has a keyed-hash
  assertion in `add_entry()`. This is not part of the current supported writer
  stack hardened by this SOW, but it is real related debt and is tracked by
  SOW-0078.

Sensitive data gate:

- Durable artifacts contain no raw host journals, field payloads, credentials,
  SNMP communities, personal data, private endpoints, or customer identifiers.
  Tests use synthetic unkeyed files created under temporary directories.

Artifact maintenance gate:

- AGENTS.md: no update required; repository-wide workflow and runtime purity
  rules did not change.
- Runtime project skills: updated
  `.agents/skills/project-journal-compatibility/SKILL.md` to require
  controlled writer rejection for historical unkeyed append-open.
- Specs: updated `.agents/sow/specs/product-scope.md` with the writer
  rejection contract.
- End-user/operator docs: updated `rust/README.md` deferred writer scope with
  the controlled-error behavior.
- End-user/operator skills: none affected.
- SOW lifecycle: moved to `.agents/sow/done/` with `Status: completed`; the
  implementation, artifact updates, follow-up mapping, and lifecycle close are
  committed together.
- SOW-status.md: updated for completed SOW-0077 state and pending SOW-0078.

Specs update:

- Updated `.agents/sow/specs/product-scope.md`.

Project skills update:

- Updated `.agents/skills/project-journal-compatibility/SKILL.md`.

End-user/operator docs update:

- Updated `rust/README.md`.

End-user/operator skills update:

- No exported operator skill changed.

Lessons:

- A reader compatibility SOW can expose a writer failure-mode issue even when
  the writer correctly refuses the unsupported format. Writer rejection must be
  explicit and early enough to prevent partial state mutation.

Follow-up mapping:

- Legacy `jf` writer assertion behavior: tracked by pending SOW-0078. This is
  intentionally separate because SOW-0077 targeted the current supported Rust
  writer stack and cross-language writer contract.

## Outcome

Completed. The current Rust writer stack now rejects historical unkeyed
append-open and direct writer construction with `UnsupportedJournalFile` before
entry mutation or assertion panic, while historical unkeyed reader support
remains intact.

## Lessons Extracted

- Writer unsupported-format failures must be explicit at public construction
  and append-open boundaries; keeping assertions only as internal invariants is
  acceptable only after those boundaries are guarded.
- Legacy compatibility crates can retain similar hazards even after the current
  supported stack is fixed, so same-failure scans must track those paths
  explicitly.

## Followup

- SOW-0078 tracks the legacy `jf` writer assertion path.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
