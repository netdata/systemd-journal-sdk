# SOW-0055 - Rust Seek Cursor Systemd Parity

## Status

Status: in-progress

Sub-state: round-1 external review findings fixed locally and validation
passed; whole-SOW reviewer rerun pending. Keep `Status: in-progress`.

## Requirements

### Purpose

Keep the Rust libsystemd-compatible reader facade aligned with actual
`sd_journal_seek_cursor()` semantics so Rust remains the behavioral reference
for the other SDKs.

### User Request

The user requested Go reader parity with Rust. SOW-0045 review exposed that Go,
Python, and Node.js follow upstream systemd's no-existence-proof
`seek_cursor()` behavior, while current Rust still scans for an existing cursor
and returns an error when the cursor is not found.

### Assistant Understanding

Facts:

- Upstream systemd `sd_journal_seek_cursor()` parses a cursor, stores the
  requested location, and returns success without scanning for an existing
  entry.
- Current Rust `JournalReader::seek_cursor()` scans entries and returns
  `SdkError::InvalidCursor` when no exact entry is found.
- Python and Node.js SOWs already recorded the systemd no-existence-proof
  behavior as the accepted facade behavior.

Inferences:

- Rust should be corrected to systemd behavior, not used to pull Go/Python/Node
  back to the stricter scan-and-error behavior.

Unknowns:

- Whether the Rust idiomatic SDK API should expose both strict exact-cursor
  search and libsystemd-compatible seek-location behavior, or whether only the
  facade should change.

### Acceptance Criteria

- Rust libsystemd-compatible facade `SdJournalSeekCursor()` matches upstream
  systemd no-existence-proof behavior for syntactically valid cursors.
- Rust retains or adds an explicit exact-cursor helper only if needed by
  existing tests or documented SDK use cases.
- Shared cursor tests cover found cursor, syntactically invalid cursor, and
  syntactically valid but nonexistent cursor behavior across Rust, Go, Python,
  and Node.js.
- Docs/specs state the accepted `seek_cursor()` contract consistently.

## Analysis

Sources checked:

- `rust/src/journal/src/lib.rs`
- `python/journal/facade.py`
- `node/src/facade.js`
- `go/journal/facade.go`
- `.agents/sow/done/SOW-0053-20260529-python-reader-writer-rust-port.md`
- `.agents/sow/done/SOW-0054-20260529-node-reader-writer-rust-port.md`
- `systemd/systemd @ cf3156842209`
  `src/libsystemd/sd-journal/sd-journal.c:1263`

Current state:

- Rust file reader `seek_cursor()` scans entries and returns
  `SdkError::InvalidCursor` on no exact match.
- Rust directory reader treats file-level `seek_cursor()` error as not found
  and returns `SdkError::InvalidCursor` if no file has the cursor.
- Python, Node.js, and Go accept a syntactically valid but nonexistent cursor as
  a seek location rather than an existence proof, matching systemd.

Risks:

- Changing Rust may affect tests that currently assume exact-cursor proof.
- Keeping Rust divergent weakens its role as the cross-language reference.

## Pre-Implementation Gate

Status: in-progress

Problem / root-cause model:

- Rust kept an exact-cursor-search implementation from earlier SDK phases.
  Later Python and Node.js work verified upstream systemd behavior and adopted
  no-existence-proof seek semantics, but Rust was not realigned.
- The implementation prompt for this worktree selects SOW-0055 as the active
  task, resolving the earlier prioritization blocker for this worker.

Evidence reviewed:

- `systemd/systemd @ cf3156842209`,
  `src/libsystemd/sd-journal/sd-journal.c:1263-1363`: parses cursor fields,
  stores `current_location`, and returns `0` without scanning entries.
- `systemd/systemd` tag `v260.1`, official GitHub source
  `src/libsystemd/sd-journal/sd-journal.c`: checked as the current project
  compatibility baseline for `sd_journal_seek_cursor()`.
- `rust/src/journal/src/lib.rs`: file and directory `seek_cursor()` return an
  error when no exact cursor is found.
- `.agents/sow/done/SOW-0053-20260529-python-reader-writer-rust-port.md`:
  records the upstream systemd evidence and accepted no-existence-proof Python
  behavior.
- `.agents/sow/done/SOW-0054-20260529-node-reader-writer-rust-port.md`:
  records the same behavior for Node.js.

Affected contracts and surfaces:

- Rust idiomatic reader API.
- Rust libsystemd-compatible facade.
- Shared cross-language cursor conformance tests.
- Reader docs/specs.

Existing patterns to reuse:

- Python and Node.js no-existence-proof facade behavior.
- Go SOW-0045 implementation and tests after review disposition.
- Existing Rust `test_cursor()` exact-match helper.

Risk and blast radius:

- Medium: cursor navigation is a public facade contract and affects journalctl
  query behavior.

Sensitive data handling plan:

- Use generated journal fixtures only. Do not record real logs, credentials,
  SNMP communities, bearer tokens, personal data, customer identifiers, or
  private endpoints.

Implementation plan:

1. Decide whether Rust idiomatic `seek_cursor()` changes directly or whether a
   facade-specific no-existence-proof wrapper is added.
2. Update Rust code and shared tests.
3. Update docs/specs and run Rust plus cross-language cursor tests.

Validation plan:

- Rust tests covering found, invalid, and syntactically valid nonexistent
  cursor cases.
- Shared facade cursor tests across Rust, Go, Python, and Node.js.
- File-backed journalctl cursor/query regression where applicable.
- Whole-SOW read-only reviewer pass.

Artifact impact plan:

- AGENTS.md: no workflow update expected.
- Runtime project skills: no update expected.
- Specs: update reader/facade cursor behavior.
- End-user/operator docs: update Rust docs if public behavior changes.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: follow-up from SOW-0045.
- SOW-status.md: not edited in this worktree per implementation prompt;
  reconciliation is left to the orchestrator.

Open-source reference evidence:

- `systemd/systemd @ cf3156842209`
  `src/libsystemd/sd-journal/sd-journal.c:1263-1363`

Open decisions:

- Prioritization is resolved by `.local/agent-prompts/sow-0055-rust-cursor.md`.
- Rust API shape for this SOW: make Rust `seek_cursor()` use the accepted
  no-existence-proof behavior for syntactically valid cursors; keep
  `test_cursor()` as the exact-current cursor check. No separate strict
  seek helper is required unless tests reveal an existing caller contract that
  needs it.

## Implications And Decisions

- No user decision recorded yet.
- 2026-05-30: implementation prompt selected SOW-0055 for this dedicated
  worker. Implication: this SOW may proceed despite its original pending
  prioritization note; `SOW-status.md` reconciliation is intentionally left to
  the orchestrator per prompt.
- 2026-05-30: implementation will align Rust `seek_cursor()` with systemd
  no-existence-proof behavior and rely on `test_cursor()` for exact-current
  checks. Risk: callers that accidentally used `seek_cursor()` as an existence
  proof must switch to `test_cursor()` after positioning or another explicit
  search.
- 2026-05-30: shared conformance coverage exposed that Python and Node.js
  accepted malformed seek cursors by defaulting missing cursor fields to zero.
  Because the accepted contract says invalid cursor syntax fails, the parser
  tightening was applied in Python and Node.js too; Go already failed malformed
  segments, and was tightened only for empty required IDs.

## Plan

1. Confirm desired Rust API shape for strict exact-cursor proof versus
   libsystemd-compatible seek-location behavior.
2. Implement and test Rust alignment.
3. Update shared tests, docs, and specs.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Read-only reviewers from the approved pool after implementation and local
  validation.

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

- Record any review disagreement about exact-cursor semantics with upstream
  systemd source evidence.

## Execution Log

### 2026-05-29

- Created from SOW-0045 reviewer finding and verified against upstream systemd
  source.

### 2026-05-30

- Read `AGENTS.md`, this SOW, `.agents/skills/project-agent-orchestration/SKILL.md`,
  and `.agents/skills/project-journal-compatibility/SKILL.md`.
- Checked overlap: `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`
  is `paused` as an umbrella program; no active implementation conflict found
  for this worker.
- Checked `.agents/sow/specs/product-scope.md`: the reader target lists
  libsystemd-compatible seek cursor and test cursor surfaces, but does not yet
  state no-existence-proof cursor semantics.
- Per `.local/agent-prompts/sow-0055-rust-cursor.md`, `SOW-status.md` is not
  edited in this worktree; status reconciliation is left to the orchestrator.
- Changed `rust/src/journal/src/lib.rs` so file and directory `seek_cursor()`
  reject malformed cursor syntax but return success for syntactically valid
  cursors even when no exact entry is found. Directory cursor seeking now uses
  merged iteration, so exact found cursors leave the merged reader positioned on
  the matching entry.
- Kept exact-current matching on `test_cursor()`; no separate strict seek helper
  was needed.
- Updated `rust/src/adapter/main.rs`, `go/adapter/main.go`,
  `python/adapter.py`, and `node/adapter/index.js` so the shared
  `journal-cursor-test` covers current cursor match, invalid cursor mismatch,
  invalid seek rejection, found seek, and valid missing seek acceptance.
- Tightened parser behavior in `python/journal/facade.py`,
  `node/src/facade.js`, and `go/journal/reader.go` so the global
  invalid-cursor contract is true across the shared facade test surface.
- Updated `.agents/sow/specs/product-scope.md`, `rust/README.md`,
  `go/API.md`, `python/README.md`, `node/README.md`, and
  `tests/conformance/manifests/conformance-v01.json` with the accepted cursor
  contract.
- Ran whole-SOW read-only review against implementation commit
  `ee2a3365ff2e` with the approved reviewer pool:
  `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/qwen3.6-plus`,
  `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/minimax-m2.7-coder`,
  and `llm-netdata-cloud/mimo-v2.5-pro`.
- Round-1 review votes: `qwen3.6-plus` and `minimax-m2.7-coder` voted
  `PRODUCTION GRADE`; `glm-5.1` and `mimo-v2.5-pro` voted
  `NOT PRODUCTION GRADE`; `kimi-k2.6` produced contradictory text, with an
  early explicit `NOT PRODUCTION GRADE` vote and later `PRODUCTION GRADE`
  text, so it was dispositioned conservatively as not clean.
- Dispositioned the real round-1 findings by adding missing-cursor post-seek
  position assertions to Rust, Go, Python, and Node shared cursor adapter
  checks; adding direct Rust multi-file directory cursor positioning coverage;
  adding Go empty `s=`/`j=` parser unit coverage; and normalizing Python/Node
  empty `c=`/`n=` parser rejection before numeric conversion.
- Revalidated the fixed work locally. Whole-SOW reviewer rerun remains pending.

## Validation

Acceptance criteria evidence:

- Rust `SdJournalSeekCursor()` accepts a valid missing cursor and leaves the
  reader at a current entry at or after the requested realtime instead of
  staying on the original cursor:
  `rust/src/journal/src/lib.rs` test `jf_facade_stateful_reader_operations`
  builds `missing_cursor` by changing the cursor `n=` segment, expects
  `SdJournalSeekCursor()` success, asserts `SdJournalTestCursor()` is false for
  the original cursor, checks realtime did not move backward, and checks the
  single-file fixture lands on the next entry.
- Rust directory cursor seeking is positioned after found and valid-missing
  cursor seeks: `jf_facade_stateful_reader_operations` opens multiple files,
  seeks back to a captured directory cursor, confirms `SdJournalTestCursor()`
  against that cursor, then seeks a valid missing cursor and confirms it does
  not remain on the original cursor or move backward in realtime.
- Rust invalid cursor syntax still fails:
  `rust/src/journal/src/lib.rs` test `jf_facade_stateful_reader_operations`
  expects `Err(FacadeError::InvalidCursor)` for `invalid-cursor`.
- Shared cursor conformance now covers Rust, Go, Python, and Node.js through
  `tests/conformance/manifests/conformance-v01.json` and the four adapters,
  including missing-cursor post-seek position checks.
- Docs/specs state the accepted no-existence-proof contract in
  `.agents/sow/specs/product-scope.md`, `rust/README.md`, `go/API.md`,
  `python/README.md`, and `node/README.md`.
- Exact-cursor proof remains available through `test_cursor()`; no strict seek
  helper was needed.

Tests or equivalent validation:

- OK after round-1 fixes: `cargo test -p journal jf_facade_stateful_reader_operations`
  from `rust/` (`1` targeted test passed).
- OK: shared `journal-cursor-test` adapter case in Rust, Go, Python, and
  Node.js. Each adapter returned PASS with boolean actual true; Go, Python, and
  Node.js evidence confirmed found cursor seek, invalid seek rejection, invalid
  cursor mismatch, missing cursor seek acceptance, and missing cursor position.
- OK after round-1 fixes: `cargo test -p journal -p adapter` from `rust/`
  (`22` journal tests passed; adapter has `0` unit tests).
- OK after round-1 fixes: `go test ./...` from `go/`.
- OK after round-1 fixes: `python3 python/test_all.py`, with Python import
  path pointed at the
  local `.local/python-deps` validation dependency directory.
  The first run exposed a missing local validation dependency (`lz4`); it was
  installed under `.local/python-deps` with `.local/pip-cache`, then the full
  Python package tests passed.
- OK after round-1 fixes: `npm ci --prefix node --cache "$PWD/.local/npm-cache"`
  and `node node/test/all.js`.
- OK after round-1 fixes:
  `python3 tests/conformance/runner/manifest_checker.py validate tests/conformance/manifests/conformance-v01.json`.
- OK after round-1 fixes: `git diff --check`.
- OK after round-1 fixes: `.agents/sow/audit.sh`.

Real-use evidence:

- Shared cursor adapter validation ran against the repository systemd v260.1
  `no-rtc` fixture directory through the Rust, Go, Python, and Node.js facade
  paths.
- File-backed journalctl cursor/query regression was checked for applicability:
  the repository journalctl implementations use cursors for follow-mode de-dupe,
  not `SdJournalSeekCursor()` navigation. The broader Rust, Go, Python, and
  Node package tests include the file-backed journalctl cases present in their
  package test suites.

Reviewer findings:

- Round 1, `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  Non-blocking notes: add direct Go empty ID parser coverage and consider an
  explicit beyond-last-cursor test. Disposition: Go empty `s=`/`j=` parser unit
  coverage was added; beyond-last cursor positioning remains non-blocking
  because the accepted shared contract is valid-missing acceptance plus
  at-or-after positioning, now tested.
- Round 1, `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
  Non-blocking/contradictory notes included a claimed Rust directory found
  cursor positioning risk. Disposition: source review shows `step_merged()`
  sets `self.index` and `self.current_key` to the selected candidate before
  returning; direct Rust multi-file found cursor assertions were added anyway
  to prevent regression.
- Round 1, `llm-netdata-cloud/glm-5.1`: `NOT PRODUCTION GRADE`.
  Blocking finding: missing-cursor seek tests only proved no error, not
  post-seek position. Disposition: fixed by adding missing-cursor post-seek
  position assertions in Rust unit coverage and all four adapters.
- Round 1, `llm-netdata-cloud/mimo-v2.5-pro`: `NOT PRODUCTION GRADE`.
  Findings: post-seek position after valid missing cursors was untested;
  Python/Node empty `c=`/`n=` values were rejected only by numeric conversion
  errors; docs/specs should avoid overclaiming exact systemd lazy-location
  internals. Disposition: added position assertions, normalized Python/Node
  empty `c=`/`n=` validation, and kept docs/specs scoped to the accepted SDK
  facade contract rather than claiming lazy internal implementation parity.
- Round 1, `llm-netdata-cloud/kimi-k2.6`: output was self-contradictory. It
  first emitted `NOT PRODUCTION GRADE`, later emitted `PRODUCTION GRADE`, and
  still listed missing post-seek position validation as a gap. Disposition:
  treated conservatively as not clean and fixed the shared position coverage.

Same-failure scan:

- `rg` scan for `Seek to an exact cursor`, `exact cursor`,
  `SdJournalSeekCursor`, `invalid seek cursor`, and `missing_seek` found no
  remaining public docs that describe `SdJournalSeekCursor()` as an existence
  proof. Remaining exact-cursor references are tests, SOW context, or
  `test_cursor()` semantics.

Sensitive data gate:

- Durable artifacts contain only generated-fixture paths, public upstream
  source references, command names, and code paths. No real logs, credentials,
  personal data, customer identifiers, private endpoints, or live host journal
  data were used or recorded.

Artifact maintenance gate:

- `AGENTS.md`: no update needed; workflow and routing rules were unchanged.
- Runtime project skills: no update needed; no durable HOW-to workflow changed.
- Specs: updated `.agents/sow/specs/product-scope.md` with the accepted cursor
  contract.
- End-user/operator docs: updated `rust/README.md`, `go/API.md`,
  `python/README.md`, and `node/README.md`.
- End-user/operator skills: no output/reference skills exist or were affected.
- SOW lifecycle: moved SOW-0055 from `pending/` to `current/`, kept
  `Status: in-progress` per implementation prompt, and recorded validation.
- `SOW-status.md`: intentionally not edited in this worktree per prompt; status
  reconciliation remains with the orchestrator.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` to state that `seek_cursor()`
  accepts syntactically valid missing cursors, invalid syntax fails, and
  `test_cursor()` remains the exact-current-position check.

Project skills update:

- No project skill update needed; the work changed SDK behavior and tests, not
  repository operating procedure.

End-user/operator docs update:

- Updated Rust, Go, Python, and Node public docs to avoid describing
  `seek_cursor()` as an exact-cursor proof.

End-user/operator skills update:

- No end-user/operator skills exist for this cursor contract, and no docs/spec
  change requires a copied reference skill update.

Lessons:

- Shared conformance must test both successful and failing parts of a facade
  contract. The no-existence-proof seek behavior and invalid syntax rejection
  are separate requirements.

Follow-up mapping:

- No new implementation follow-up remains from round-1 findings. Remaining
  work is whole-SOW reviewer rerun and final review disposition.

## Outcome

Implemented locally; round-1 reviewer findings fixed and local validation
passed. The SOW intentionally remains in `current/` with `Status: in-progress`
while whole-SOW reviewer rerun is pending.

## Lessons Extracted

- When a global facade contract is documented, the shared adapter should assert
  both acceptance/rejection and relevant post-operation state in every
  language, not rely on per-language unit tests.

## Followup

- Rerun the configured whole-SOW read-only reviewer batch after the round-1
  fixes commit.
- After clean reviewer disposition, reconcile `SOW-status.md` and leave final
  merge/closure to the orchestrator.
