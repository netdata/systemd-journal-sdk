# SOW-0011 - Live Concurrency Compatibility Gate

## Status

Status: completed

Sub-state: completed. The reusable live compatibility gate is now in place for the Go writer feature slice and is mandatory for future writer and reader SOWs.

## Requirements

### Purpose

Make live one-writer/multiple-reader compatibility a confirmed and validated contract for every journal writer and reader.

### User Request

The user clarified that compatibility requires stock `journalctl` and libsystemd journal readers to work concurrently with repository writers, and requires repository readers to handle the corresponding live writer cases. This is a MUST for all writers and readers.

### Assistant Understanding

Facts:

- Closed-file `journalctl --verify` is not enough to claim compatibility.
- Every writer must be readable by stock `journalctl --file` while it is appending.
- Every writer must be readable by stock libsystemd reader APIs while it is appending.
- Every reader must safely read live files being appended by repository writers.
- Reader compatibility with stock systemd writers must be tested where the environment can provide that safely without violating repository-boundary rules.
- The existing Go writer has closed-file stock `journalctl` evidence and one temporary live smoke test, but it does not yet have committed production-grade live concurrency validation.

Inferences:

- This SOW should run before additional language writers are accepted, so every future implementation inherits the same compatibility gate.
- The shared conformance harness needs live-concurrency adapters, not only closed-file fixtures.
- The Go writer may need append-publication-order hardening before it can be called production-compatible.

Unknowns:

- The exact stock libsystemd reader helper shape must be designed during activation.
- A safe stock systemd writer source for repository reader tests may require a fixture generator, a systemd test helper, or an explicitly documented environment capability.

### Acceptance Criteria

- Shared live-concurrency harness exists and is committed.
- Harness records stock systemd version, helper commands, reader count, append count, duration, failure criteria, and logs.
- Go writer passes stock `journalctl --file` live-read tests while appending.
- Go writer passes stock libsystemd live-reader tests while appending.
- Go writer passes clean-close `journalctl --verify --file` after live-read stress.
- Go writer passes interruption/reopen live-read tests for the feature slice it claims.
- Harness can be reused by Rust, Node.js, Python, and the final cross-language matrix.
- Reader-side live test contract is defined for repository readers and stock writer evidence.
- Product scope, project compatibility skill, and pending implementation SOWs record that this gate is mandatory.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/specs/product-scope.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`
- `.agents/sow/done/SOW-0005-20260523-go-sdk-and-journalctl.md`
- `go/journal/writer.go`
- `go/journal/writer_test.go`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`

Current state:

- Go writer SOW is completed and committed.
- Go writer has closed-file stock `journalctl` tests.
- Go writer has no committed stock libsystemd live-reader test.
- Go writer has no committed live stress test with multiple stock readers while appending.
- Go reader does not exist yet.

Risks:

- A writer can pass closed-file verification but still expose inconsistent append publication windows to live stock readers.
- A reader can parse closed fixtures but fail on online journal state, tail metadata changes, file growth, or entry-array growth.
- Without this gate, later language ports can copy a subtly incompatible writer order or reader refresh model.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Compatibility must cover live operation because systemd journal files are designed for one writer and multiple concurrent readers.
- Current validation proves only a narrower closed-file subset for the Go writer.
- A reusable live-concurrency harness is needed before other writers and readers can be accepted as compatible.

Evidence reviewed:

- `.agents/sow/done/SOW-0005-20260523-go-sdk-and-journalctl.md`
- `go/journal/writer.go`
- `go/journal/writer_test.go`
- `.agents/sow/specs/product-scope.md`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced src/libsystemd/sd-journal/journal-file.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced src/libsystemd/sd-journal/sd-journal.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced src/libsystemd/sd-journal/journal-def.h`

Affected contracts and surfaces:

- Shared conformance harness.
- Go writer append publication behavior.
- Future Rust, Node.js, Python writer acceptance gates.
- Future Go, Rust, Node.js, Python reader acceptance gates.
- Product scope spec.
- Project compatibility skill.
- SOW status and phase ordering.

Existing patterns to reuse:

- Existing Go writer package tests.
- Existing systemd conformance manifest structure.
- Stock `journalctl --file` validation already used by Go writer tests.
- systemd live reader behavior and append ordering as compatibility authority.

Risk and blast radius:

- This SOW may expose real Go writer defects requiring code changes.
- Live stress tests can be flaky if they depend on timing alone; they need deterministic failure criteria and repeatable helper protocols.
- Direct stock libsystemd reader helpers may require build tooling in tests, but must not make SDK implementations link to libsystemd.
- Reader tests against a stock systemd writer may be environment-sensitive and must not write outside this repository except `/tmp`.

Sensitive data handling plan:

- Test journals use synthetic entries only.
- Test logs must not include host journal data, real service names from the workstation, credentials, bearer tokens, SNMP communities, customer data, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details.
- Stock writer evidence must use synthetic test data or generated fixtures, not host production journals.

Sensitive data gate:

- Before review and close, scan changed durable artifacts for raw secrets, credentials, bearer tokens, SNMP communities, community member names, customer names, personal data, non-private customer-identifying IPs, private endpoints, and proprietary incident details.
- Do not commit generated journal files, live harness logs, compiled helper binaries, or local scratch output.

Implementation plan:

1. Add a shared live-concurrency test harness under `tests/conformance/`.
2. Add a stock `journalctl --file` live-reader adapter that repeatedly reads or follows a file while a repository writer appends.
3. Add a stock libsystemd reader helper or adapter for live file-backed reading.
4. Add Go writer live stress tests with multiple stock readers, clean-close verify, interruption, and reopen.
5. Define the reader-side live contract for repository readers and stock writer evidence.
6. Repair Go writer append publication behavior if live tests expose failures.
7. Record the reusable gate in docs/specs/skills and pending language SOWs.

Validation plan:

- Go package tests pass.
- Shared live-concurrency harness passes against the Go writer.
- Stock `journalctl --file` live tests pass against Go writer.
- Stock libsystemd live-reader tests pass against Go writer.
- `journalctl --verify --file` passes after live stress and after tested interruption/reopen.
- `CGO_ENABLED=0 go test ./...` confirms SDK remains pure Go.
- SOW audit and sensitive-data audit pass.
- External reviewers inspect the whole SOW and implementation until production-grade.

Artifact impact plan:

- AGENTS.md: likely unchanged unless SOW lifecycle rules need tightening.
- Runtime project skills: update `.agents/skills/project-journal-compatibility/SKILL.md`.
- Specs: update `.agents/sow/specs/product-scope.md`.
- End-user/operator docs: update only if Go writer docs need a compatibility warning or guarantee.
- End-user/operator skills: none expected.
- SOW lifecycle: this SOW should be activated before accepting more writers as production-compatible.
- SOW-status.md: update next SOW recommendation to this SOW.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced src/libsystemd/sd-journal/journal-file.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced src/libsystemd/sd-journal/sd-journal.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced src/libsystemd/sd-journal/journal-def.h`

Open decisions:

- No user decision is currently needed. The user has made the live concurrency requirement mandatory.

## Implications And Decisions

1. Live concurrency compatibility is mandatory
   - Current state: resolved by user clarification on 2026-05-23.
   - Selection: no writer or reader can be called production-compatible without live one-writer/multiple-reader validation.
   - Implication: closed-file verification alone is insufficient.
   - Risk: this may delay language ports, but skipping it would make the SDKs incompatible with real systemd journal operation.

2. Go writer production claim
   - Current state: the Go writer is implemented for the first feature slice, but full live compatibility is not proven.
   - Selection: treat the Go writer as not yet production-compatible for concurrent stock-reader operation until this SOW passes.
   - Implication: the Netdata plugin use case should wait for this SOW before relying on concurrent stock readers.
   - Risk: live tests may require writer changes and another external review cycle.

## Plan

1. Activate this SOW before the next language implementation SOW.
2. Implement the live-concurrency harness and stock reader adapters.
3. Run the harness against the Go writer and fix any compatibility gaps.
4. Update all future language SOWs to use this harness as a required acceptance gate.
5. Review with external agents until no blocking findings remain.

## Delegation Plan

Implementer:

- Preferred implementer is `llm-netdata-cloud/minimax-m2.7-coder`, unless direct implementation is faster for harness mechanics; if direct implementation is used, Minimax must be switched to reviewer.

Reviewers:

- Use at least four reviewers from `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`.

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

- Record implementer or reviewer model failure in this SOW.
- Substitute only from the approved model list.
- Rerun full-scope review after fixes.
- Do not close if `.agents/sow/audit.sh` fails.

## Execution Log

### 2026-05-23

- Created this SOW after the user clarified live concurrency is mandatory for compatibility.
- Activated as the next SOW after the user approved implementing the live concurrency compatibility gate.
- Added reusable live harness files:
  - `tests/conformance/live/run_live_concurrency.py`
  - `tests/conformance/live/libsystemd_live_reader.c`
  - `tests/conformance/live/README.md`
- Added Go live writer test command at `go/internal/testcmd/livewriter/main.go`.
- Added Go live tests at `go/journal/live_concurrency_test.go`:
  - stock `journalctl --file` polling readers;
  - stock `journalctl --file --follow --no-tail --boot=all` reader;
  - stock libsystemd reader via `sd_journal_open_files`, `sd_journal_next`, and `sd_journal_wait`;
  - clean-close verify;
  - interruption, verify, reopen, append, verify;
  - zero-delay live stress;
  - second-writer rejection while a live writer holds the lock.
- Updated `tests/conformance/ADAPTER_CONTRACT.md`, `tests/conformance/manifest-schema.json`, and `tests/conformance/runner/manifest_checker.py` to record the `live-concurrency` category and reusable reader/writer contract.
- Updated `go/README.md` to record the current Go writer live stock-reader validation scope.
- External review round 1:
  - Minimax: production-grade for the original harness.
  - Mimo: conditional production-grade; blocking lifecycle gap was stale SOW validation.
  - Qwen: not production-grade; valid findings included hardcoded C helper field validation, sequence/order validation gap, interrupted-file verify gap, and stale SOW validation. The `--boot=all` objection was rejected because systemd documents `--boot=all` as negating earlier boot filtering and `journalctl --follow` enables current-boot filtering by default for synthetic journals.
  - Kimi: not production-grade; valid findings included C helper numeric parsing, missing second-writer live test, missing stress/race coverage, and stale SOW validation.
  - GLM: production-grade for the original harness with minor documentation recommendations.
- Fixed review findings:
  - C helper now validates numeric arguments and accepts a configurable sequence field instead of hardcoding `MESSAGE=live-`.
  - Harness validates ordered `LIVE_SEQ` output for stock `journalctl` and stock libsystemd readers.
  - Interruption test now runs `journalctl --verify --file` before reopen and after final close.
  - Added zero-delay live stress test with sync on every entry.
  - Added live second-writer rejection test.
  - Documented `--boot=all` use and polling retry semantics.
- The new sequence validation exposed a real Go writer publication-window issue: stock `journalctl --file PRIORITY=6` could transiently observe matched entries without the `LIVE_SEQ` field while the writer was active.
- Repaired Go writer live publication order:
  - object metadata (`arena_size`, `tail_object_offset`, `n_objects`) is published before entry links can expose new objects;
  - entry metadata is published with `n_entries` last;
  - object metadata publication is repeated when entry-array objects are allocated.
- Kimi review found a real follow-reader startup race: `journalctl --follow` can return active-writer `ENODATA` while the writer is still appending. The harness now retries this specific active-writer condition and still requires a complete ordered final stream.
- Local validation then exposed the same active-writer `ENODATA` shape in stock libsystemd `sd_journal_open_files`. The harness now retries that specific active-writer open failure and still requires the final libsystemd reader to observe the complete ordered sequence.
- Kimi review found a real Go test data-race risk in the second-writer test's `strings.Builder` stdout/stderr capture. The test now uses a mutex-protected buffer, and `go test -race` passes.
- Updated `.agents/sow/specs/product-scope.md` and `.agents/skills/project-journal-compatibility/SKILL.md` with the `tests/conformance/live/` harness path, `LIVE_SEQ` contract, active-writer retry semantics, final ordered-read requirement, and `journalctl --verify --file` requirement.
- Reviewed the header publication-order concern against `systemd/systemd @ v260.1 src/libsystemd/sd-journal/journal-file.c`:
  - `journal_file_append_object` updates object reachability fields individually.
  - `link_entry_into_array` writes the entry array item before incrementing the relevant entry count.
  - `journal_file_link_entry` uses an ordering fence and updates header fields individually, not a multi-field atomic header transaction.
  - The earlier `PIPE_BUF` objection was rejected because `PIPE_BUF` is a pipe/FIFO write guarantee, not a regular-file header transaction guarantee.
- Final full-scope review round:
  - Minimax: PRODUCTION GRADE; rejected the header atomicity concern and accepted the active-writer retry contract.
  - Mimo: PRODUCTION GRADE; independently compared systemd publication order from a `/tmp` reference clone and accepted the implementation.
  - Qwen: PRODUCTION GRADE; rejected the header atomicity concern and accepted the current feature-slice compatibility claim.
  - Kimi: PRODUCTION GRADE; left only non-blocking notes about one extra bounded libsystemd retry, optional `stop_event` propagation, and a pre-existing `Open()` state-validation follow-up.
  - GLM final review attempt stalled without a final verdict and was stopped by terminating only the exact stale reviewer command; it was replaced by Qwen in the final review round.

## Validation

Final local validation on the completed files:

- `go test -count=1 ./...` from `go/`: PASS.
- `go test -race -count=1 ./...` from `go/`: PASS.
- `CGO_ENABLED=0 go test -count=1 ./...` from `go/`: PASS.
- `go vet ./...` from `go/`: PASS.
- `go list -deps -f '{{if .CgoFiles}}{{.ImportPath}} {{.CgoFiles}}{{end}}' ./...` from `go/`: no output.
- Repeated targeted live suite 10 consecutive times:
  - Command: `for i in 1 2 3 4 5 6 7 8 9 10; do go test -count=1 -run 'TestGoWriterLiveStockReaders|TestGoWriterLiveStockReadersStress|TestGoWriterLiveInterruptionReopenAndVerify|TestGoWriterLiveRejectsSecondWriter' ./journal || exit 1; done`
  - Result: PASS 10/10.
- Final reviewer Kimi independently repeated the targeted live suite 10 consecutive times:
  - Result: PASS 10/10.
- `python3 -m py_compile tests/conformance/live/run_live_concurrency.py`: PASS.
- `cc tests/conformance/live/libsystemd_live_reader.c -o /tmp/libsystemd_live_reader.check $(pkg-config --cflags --libs libsystemd)`: PASS.
- `python3 tests/conformance/runner/manifest_checker.py validate tests/conformance/manifests/conformance-v01.json`: PASS.
- `python3 tests/conformance/runner/manifest_checker.py dry-run tests/conformance/manifests/conformance-v01.json`: PASS.
- `bash .agents/sow/audit.sh`: PASS.
- `SOW_AUDIT_SENSITIVE_CHANGED=1 bash .agents/sow/audit.sh`: PASS.
- `git diff --check`: PASS.
- ASCII scan of changed durable files: no output.
- User-personal-name scan of changed durable files: no output.

## Outcome

Completed.

This SOW adds the reusable live one-writer/multiple-reader compatibility gate and applies it to the Go writer feature slice. The Go writer can now claim live stock-reader compatibility for its current feature slice only: regular uncompressed journal files without FSS/compression support. Future writers and readers must reuse and extend this live harness before making production-compatible claims.

## Lessons Extracted

- Live compatibility tests must validate ordered content, not only final entry counts.
- Stock `journalctl --file` polling can observe transient active-writer snapshots; the harness may retry those while the writer is active, but the final post-writer snapshot must be complete and ordered.
- Stock `journalctl --follow` and stock libsystemd open can also return active-writer `ENODATA`; the harness may retry only while the writer is active, and final reads must pass.
- The Go writer needed object metadata publication before entry links and `n_entries` publication last. Closed-file verification did not expose this class of bug.
- Race-detector validation belongs in this gate because live test process plumbing can fail independently from journal file correctness.
- `PIPE_BUF` is not evidence for regular-file header transaction atomicity; compare against systemd's actual journal-file publication order when reviewing this class of concern.

## Followup

- Future Rust, Go reader, Node.js, and Python SOWs must reuse the live harness and keep `LIVE_SEQ` or an explicitly configured equivalent sequence field in their writer test commands.
- Repository reader SOWs must add the corresponding live reader-side matrix against every repository writer available in that phase.
- Track Go `Open()` state validation for archived or unknown-state journals in the next Go reader/journalctl SOW or a dedicated writer-hardening SOW. This is outside SOW-0011 because the current interruption/reopen path intentionally reopens online files after a writer interruption.

## Regression Log

No regressions remain open for this SOW.
