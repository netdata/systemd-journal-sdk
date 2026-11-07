# SOW-0097 - Single Ownership Of Journal Behavior (CLI And SDK, Go And Rust)

Originally opened as "Go Codacy Metric Debt Refactor"; rescoped by user
decision on 2026-11-07 (see Implications And Decisions).

## Status

Status: in-progress

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: activated 2026-11-07 on branch `sow-0097/cli-sdk-single-owner`;
decisions recorded; implementation starts test-first.

## Requirements

### Purpose

The SDK will be maintained for years. Duplicated journal behavior costs every
future fix twice and, worse, invites missed fixes when nobody remembers the
copy. Give every journal behavior exactly one owner, remove the copies the
journalctl rewrites carry, fix the SDK divergences the inventory exposed, and
stop new copies from appearing.

### User Request

- 2026-11-07: the user rejected a lines-only view of duplication: the real cost
  is duplicate fixes over the years and the misses; duplication is itself a big
  risk. Asked for sane maintainer choices.
- Approved "one owner per journal behavior" as a project rule ("should be there
  from the beginning"), a public SDK function in both languages as the home
  for code the CLIs and SDK share, and tests before each change.
- Explicitly did not approve a broader refactor of the Netdata integration
  surface: trivial changes are acceptable, refactoring Netdata consumers
  (netflow, SNMP traps plugins, etc.) is out of scope; the line is drawn per
  case.
- journalctl rewrites may be refactored freely. The reader, the writer and the
  Netdata function layer are the primary concern.
- After reviewing the per-case inventory, the user decided all items belong in
  this SOW, with the risk adjustments recorded below, a duplication gate in CI,
  and the CLI file split tracked separately (SOW-0175).

### Assistant Understanding

Facts:

- A 2026-11-07 jscpd scan (min 70 tokens; raw output in `.local/dup-scan/`)
  measured Go 69k lines at 2.3% duplicated (546 production-to-production
  lines) and Rust 93k lines at 6.4% (4,374 production lines, about 3,200 of
  them the legacy `jf` crates tracked by SOW-0098).
- Two read-only inventories found 27 Go and 17 Rust places where a journalctl
  rewrite reimplements SDK journal behavior. Claims used below were spot
  checked against source.
- The CLI vacuum emptiness check (Go `cmd/journalctl/main.go:3452-3473`, Rust
  `main.rs:1755-1764`) is a faithful copy of systemd/systemd @ v262
  (8cc40e0c5e9234bf45084751ac53b1fbfe70b492)
  `src/libsystemd/sd-journal/journal-vacuum.c:95-123`; it is not a duplicate
  of any SDK behavior.
- Netdata's C plugin uses `_SOURCE_REALTIME_TIMESTAMP` only when non-zero and
  earlier than the journal realtime (ktsaou/netdata @ 3483002881
  `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:48-53`);
  the SDK explorer (`go/journal/explorer.go:2904-2917`) applies the same rule.
  The journalctl rewrites apply systemd's display rule. Different contracts,
  both correct.
- systemd v262 neither writes nor reads whole-file `.journal.zst` files (no
  match in `src/journal` or `src/libsystemd/sd-journal`); they appear only as
  compressed test fixtures. Netdata's C plugin has no `.zst` handling.
- systemd names a disposed file with `now(CLOCK_REALTIME)` microseconds
  (`src/libsystemd/sd-journal/journal-file.c:4447-4450`); both SDK writers use
  nanoseconds (Go `go/journal/log.go:640-647`, Rust
  `journal-log-writer/src/log/chain.rs:402-410`).
- `journal.ParseUUID` tolerates whitespace and dashes; `go/journalhost/uuid.go:12`
  depends on that to read a machine-id file with its trailing newline.

Inferences:

- The CLI output engines (export, JSON) cannot be replaced by the current SDK
  `ExportEntry`/`JSONEntry` without first making those systemd-faithful and
  streaming; otherwise the CLI regresses in output and per-row cost.

Unknowns:

- Whether systemd rejects over-long hex in `--verify-key` the way the SDK
  parser does (I7). Resolved by an oracle run before I7 is implemented.

### Acceptance Criteria

- Every in-scope item below lands test-first: new tests are added and pass on
  the pre-change code, and still pass after it, unless the item is a recorded
  behavior fix, in which case the test proves the new behavior against the
  owning oracle (stock journalctl v262 or the Netdata plugin rule).
- Each CLI copy listed in scope is deleted and the CLI calls the SDK owner, in
  both languages.
- New public SDK functions exist in both languages with equivalent behavior and
  are recorded in this SOW and the API docs.
- `go test ./...`, the Rust workspace tests, the directory, mixed-directory,
  journalctl query and verify matrices, and the docs checks pass.
- Benchmarks show no regression for the export/JSON paths (F1) and record the
  cost of the corrected boot list (F2).
- The duplication gate runs in CI against a committed baseline and fails on
  new production clones.
- `AGENTS.md` records the single-ownership rule and the review question.

## Analysis

Sources checked:

- jscpd 5.3.1 scan over `go/` and `rust/` (`.local/dup-scan/`).
- Read-only inventory agents (Go and Rust), with spot checks of D23/D16 vacuum,
  disposed names, Rust `export.rs:54-69`, `ParseUUID` callers, Netdata plugin
  timestamp rule, systemd `.zst` handling.
- Existing tests: `go/cmd/journalctl/main_test.go:348,373` (verify directory),
  `tests/interoperability/run_directory_matrix.py:189-204` (machine-id
  subdirectories against stock journalctl), `run_journalctl_query_matrix.py:1484`
  (verify only with `--file`), export digests in the directory matrices and
  `tests/corpus_eval/run_corpus_eval.py:161`.

Current state: see the per-case scope table in the Pre-Implementation Gate.

Risks: see Risk and blast radius.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The journalctl rewrites were built alongside the SDK and copied or
  reimplemented journal rules (file discovery, header constants, export/JSON
  encoding, boot listing, key syntax) instead of calling an SDK owner. Some
  SDK public functions drifted from systemd (export/JSON, boot list), so the
  CLI could not call them, which kept the copies alive. Nothing stops new
  copies.

Evidence reviewed: Analysis sources above.

Affected contracts and surfaces:

- New public SDK functions in both languages (directory file list, header flag
  constants, SipHash for Go parity with Rust's public `siphash24_parts`, a
  verification-key parser if I7 holds).
- Public behavior changes, both languages: export and JSON entry output (F1),
  boot list (F2), random ID version bits (F3), disposed file names (F6), Rust
  Netdata non-UTF-8 file names (F5).
- journalctl rewrites in both languages (behavior unchanged except F8).
- Unchanged by design: reader and writer hot paths, Netdata function behavior
  (except the F5 alignment), `ParseUUID`, registry suffix rules, explorer
  effective timestamp.

Per-case scope (Go / Rust locations at activation):

| ID | Item | Decision |
|---|---|---|
| I1 | Directory file discovery copied in both CLIs (Go `main.go:3942-4032` = `directory_reader.go:197-287`; Rust `main.rs:1366-1419,2227-2239` = `directory.rs:1120-1206`) | Public `journal.DirectoryFiles` / `journal::directory_files` over the existing private function; CLIs call it; `OpenDirectory` unchanged. |
| I2 | Header flag constants copied (Go `main.go:35-47`; Rust `main.rs:42-53`) | Go exports flag constants; Rust re-exports `HeaderCompatibleFlags`; CLIs use them. |
| I3 | Go CLI SipHash copy (`unit_name.go:166-204`) | Exported Go wrapper over `sipHash24`; hash code untouched. Rust already reuses core. |
| I4 | Go CLI path dedupe (`main.go:970-988`) | CLI uses the SDK owner; copy deleted. |
| I5 | Rust CLI seqnum ID from cursor text (`output.rs:1183-1198`) | Use the SDK value as Go does. |
| I6 | Unused output-mode wiring (Go `main.go:1915`; Rust `main.rs:1266`) | Remove. |
| I7 | Verify-key syntax copy (Go `main.go:4063-4118` vs `verify.go:139`; Rust `main.rs:2241-2276` vs `sealed_verify.rs:125`) | Public SDK parser; CLI keeps systemd messages. Only if the oracle confirms the SDK grammar; otherwise record and keep. |
| F1 | SDK export/JSON differ from systemd (order, `__SEQNUM`, 4096-byte null rule, Rust ASCII-only text test) | Make SDK output systemd-faithful and streaming in both languages, benchmark, then CLIs render through it. |
| F2 | SDK `ListBoots` one boot per file | Correct multi-boot listing in both languages; CLIs use it; record cost. |
| F3 | ID128 grammars | `ParseUUID` unchanged (machine-id reading depends on it). Only `NewUUID` sets v4 version/variant bits as `sd_id128_randomize` does; Rust equivalent checked for parity. |
| F4 | Explorer vs CLI effective timestamp | No code change: different contracts (Netdata plugin vs systemd). Document in code and spec. |
| F5 | Rust Netdata skips non-UTF-8 file names; registry lacks `.zst` | Align Rust Netdata with Go (accept, display lossily). Registry unchanged: systemd and Netdata do not produce `.zst`; document. |
| F6 | Disposed `.journal~` names use nanoseconds | Use microseconds in both writers, as systemd does. |
| F7 | Field-name validators duplicated inside the writer crates | Mechanical merge of identical rule sets, behavior unchanged. CLI `-F` rules stay oracle-checked. |
| F8 | Rust CLI writes non-UTF-8 field names lossily in export | Match systemd (stop at invalid field names), oracle-checked. |
| G | Duplication gate | Pinned jscpd in CI against a committed baseline; fails on new production clones; test clones reported. |
| R | Single-ownership rule | `AGENTS.md` rule plus review question "does the SDK already do this?". |

Kept, not duplicates (different contracts): vacuum emptiness and vacuum name
grammar (systemd), size formatting (CLI `1.5K` vs Netdata `1.5KiB`), priority
and facility tables, Netdata recursive scan, Netdata boot first-time,
`saturatingAdd`.

Out of scope: `jf` (SOW-0098), Netdata consumers (netflow, SNMP traps,
plugins), CLI file split (SOW-0175).

Existing patterns to reuse: SDK owners already used by the CLIs (matches,
cursors, unique values, verification); oracle matrices under
`tests/interoperability/`.

Risk and blast radius:

- I1-I7: low; CLI-side plus thin public entry points over unchanged code.
- F1: medium; public output change for facade users; per-row hot path, so
  benchmark before and after, and the CLI switches only when the SDK path is
  at least as fast.
- F2: low-medium; public result change; correct listing costs a seek per boot.
- F6: low; Go retention never parses `.journal~` names; Rust registry parsing
  of disposed names checked during implementation.
- F7: low; same rules, proven by tests on the existing accept/reject sets.
- F5: low; one function in the Rust Netdata layer.
- Netdata function behavior and performance: unchanged except F5.

Sensitive data handling plan:

- Synthetic fixtures only; no host journals; raw scanner output stays in
  `.local/`.

Implementation plan:

1. Rule and gate (R, G) with the baseline taken before code changes.
2. I1-I6, each test-first, both languages.
3. I7 after the oracle check.
4. F3, F6, F7, F5 (small SDK fixes), test-first.
5. F1 and F2 with benchmarks; then switch the CLIs.
6. F8.
7. Docs, spec, API docs, rebaseline the gate; full validation; self-review.

Validation plan:

- `go test ./...` with caches under `.local/`; Rust workspace tests.
- New unit tests per item in both languages; the verify-directory oracle case.
- `tests/interoperability/run_directory_matrix.py`,
  `run_mixed_directory_matrix.py`, `run_journalctl_query_matrix.py`,
  `run_verify_matrix.py`, each with the pinned v262 `--journalctl` oracle
  where supported.
- Export/JSON and boot-list benchmarks before and after.
- `python3 tests/docs/check_wiki_docs.py` and `verify_examples.py` if docs
  change.

Artifact impact plan:

- AGENTS.md: single-ownership rule (R).
- Project skills: journal-compatibility skill gains the gate command if it
  becomes a validation step.
- Specs: export/JSON, boot list and effective-timestamp contracts.
- End-user docs: new public functions and changed output.
- SOW-status.md: activation and close.

Open-source reference evidence: systemd/systemd @ v262 as cited above;
ktsaou/netdata @ 3483002881 as cited above.

Open decisions: none. I7 is conditional on the oracle result, as decided.

## Implications And Decisions

History: opened 2026-06-07 from SOW-0096; refreshed 2026-06-15 and 2026-06-21;
parked by user decision on 2026-06-21. Those refreshes were Go-only and
Codacy-driven; their evidence is superseded by the 2026-11-07 inventory.

User decisions on 2026-11-07:

1. Single ownership of journal behavior becomes a project rule.
2. Shared CLI/SDK code lives in a public SDK function in both languages.
3. Tests before each change.
4. No broader Netdata integration refactor; per-case line; journalctl may be
   refactored freely; reader, writer and Netdata function layer are protected.
5. All I1-I7 and F1-F8 in this SOW, with: F3 reduced to `NewUUID` version bits
   (the user asked why stricter parsing would be needed; it is not); F4 handled
   carefully as performance-sensitive (resolved as no change: same rule as
   Netdata); F5 `.zst` resolved as no change (systemd does not create such
   files); F7 accepted as mechanical.
6. Duplication gate: yes.
7. CLI file split: separate pending SOW (SOW-0175).

## Plan

See Implementation plan.

## Implementation And Review Plan

Implementation: the project manager implements test-first, one item at a
time, committing verified chunks.

Review: test suite plus the project manager's whole-SOW self-review, per
`AGENTS.md`. No external reviewers.

Failure handling: if an item cannot be done without changing protected
behavior, stop and ask the user.

## Execution Log

### 2026-11-07

- Duplication scan and two read-only inventories; spot checks recorded in
  Assistant Understanding.
- User decisions recorded; SOW activated.

## Validation

Pending.

Sensitive data gate:

- Pending.

## Outcome

Pending.

## Lessons Extracted

- Agent inventories overstate risk without the owning oracle: the vacuum
  emptiness check reported as the riskiest item is a verbatim systemd rule.

## Followup

- SOW-0175: split the journalctl rewrites into files by responsibility.
- SOW-0098: `jf` duplication.

## Regression Log

None.
