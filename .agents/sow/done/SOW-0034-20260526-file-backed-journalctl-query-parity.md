# SOW-0034 - File-Backed Journalctl Query Parity

## Status

Status: completed

Sub-state: Completed and moved to done. Split from SOW-0022 Gap 6.

## Requirements

### Purpose

Complete file-backed journalctl query behavior across Rust, Go, Node.js, and Python without implementing daemon-only journalctl operations.

### User Request

Implement journalctl rewrites in all languages with file-backed/query behavior, including repeated same-key OR matching and `+` disjunction semantics.

### Assistant Understanding

Facts:

- Directory traversal, `--list-boots`, repeated same-field OR matches, `+` disjunction, `--verify`, and `--verify-key` have been implemented by earlier SOWs.
- Remaining query/follow gaps are `--follow`, `--boot`, `--since`, and `--until`.
- Daemon-only operations remain out of scope and must stay unsupported.

Inferences:

- Go is currently riskier than the others for `--boot`, `--since`, and `--until` because it parses some flags but appears to ignore them instead of failing or implementing them.

Unknowns:

- Exact stock `--boot` semantics to replicate for file-backed fixtures with synthetic boot IDs must be verified against systemd during implementation.

### Acceptance Criteria

- Rust, Go, Node.js, and Python file-backed journalctl rewrites support `--follow`, `--boot`, `--since`, and `--until` for repository-supported file/directory inputs.
- Existing daemon-only operations remain controlled unsupported behavior.
- Conformance tests compare output against stock journalctl for representative file and directory fixtures.
- Follow-mode tests prove live appended entries are emitted in order and do not require closing the writer.

## Analysis

Sources checked:

- `SOW-0022-20260525-compatibility-test-gap-audit.md`
- `SOW-0019-20260524-forward-secure-sealing.md`
- `SOW-0020-20260524-directory-traversal-parity.md`
- `SOW-0027-20260526-netdata-reader-api-and-jf-facade.md`
- `product-scope.md`
- Rust, Go, Node.js, and Python journalctl command sources
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`

Current state:

- Rust rejects `--follow` and does not expose `--boot`, `--since`, or `--until` in the clap struct.
- Go rejects `--follow` but defines `--boot`, `--since`, and `--until` in ignored flag variables.
- Node.js and Python reject `--follow`, `--boot`, `--since`, and `--until`.
- File-backed verification and directory traversal are no longer the main gap.

Risks:

- `--follow` has live concurrency behavior and must not be reduced to a closed-file loop.
- Time parsing and boot filtering can diverge subtly from stock journalctl.
- Silently ignored flags are worse than controlled unsupported behavior because users get wrong output.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The journalctl rewrites now cover core file-backed reading and verification, but still lack several query options that stock journalctl provides for file/directory inputs.

Evidence reviewed:

- `go/cmd/journalctl/main.go:32-49`
- `go/cmd/journalctl/main.go:66-75`
- `rust/src/cmd/journalctl/main.rs:24-49`
- `rust/src/cmd/journalctl/main.rs:81-92`
- `node/cmd/journalctl/index.js:20-72`
- `python/cmd/journalctl.py:146-181`
- `product-scope.md` journalctl target
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `man/journalctl.xml`

Affected contracts and surfaces:

- Journalctl command-line behavior in all four languages.
- Reader facade seek/filter behavior if needed to implement efficiently.
- Live follow harnesses.
- CLI docs/help text.

Existing patterns to reuse:

- Existing file-backed journalctl tests.
- Existing reader facade seek head/tail/realtime/cursor APIs.
- Existing live matrix writer/reader ordering checks.

Risk and blast radius:

- Medium CLI behavior risk, high user-visible correctness risk for ignored flags.

Sensitive data handling plan:

- Use synthetic fixtures only.

Implementation plan:

1. Build stock journalctl comparison fixtures for multiple boot IDs and time ranges.
2. Implement `--since` and `--until` parsing/filtering consistently.
3. Implement `--boot` for file-backed fixtures according to stock behavior.
4. Implement `--follow` using active-file polling/follow semantics, not daemon APIs.
5. Keep daemon-only operations unsupported.
6. Update CLI help/docs as needed.

Validation plan:

- Run conformance journalctl cases for all four languages.
- Run stock output comparison for file and directory fixtures.
- Run live follow tests against active SDK writers.
- Run read-only reviewers after implementation.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: no expected update unless a new journalctl parity gate is introduced.
- Specs: update `product-scope.md` with newly supported options.
- End-user/operator docs: update CLI README/help docs if present.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: activate only this SOW when implementing.
- SOW-status.md: update on activation and completion.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  - `man/journalctl.xml`
  - `src/libsystemd/sd-journal/sd-journal.c`

Open decisions:

- None. Daemon-only operations remain out of scope.

## Implications And Decisions

- No user decision is required before implementation unless stock file-backed behavior proves impossible to represent without daemon state.

## Plan

1. Add stock comparison tests.
2. Implement query/follow options.
3. Validate all languages.
4. Review as one batch.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current routing decision.

Reviewers:

- Read-only reviewers from the active pool after implementation: minimax, kimi, qwen, glm. Mimo is skipped.

Failure handling:

- Record any stock parity exception with evidence before changing scope.

## Execution Log

- 2026-05-27: Activated for local implementation with read-only external reviewers after implementation.
- 2026-05-27: Implemented file-backed `--since`, `--until`, `--boot`, and `--follow` support in Rust, Go, Node.js, and Python journalctl rewrites.
- 2026-05-27: Added `tests/interoperability/run_journalctl_query_matrix.py` to compare stock journalctl, Rust, Go, Node.js, and Python on static query cases plus live appended follow cases.
- 2026-05-27: Ran read-only reviewer cycle across glm, kimi, minimax, and qwen. Kimi reported a real Node.js local timestamp fractional precision issue and missing follow matrix coverage; the stock `--follow --until` finding was rejected after local stock journalctl evidence showed stock waits indefinitely for file-backed follow with an until bound.
- 2026-05-27: Fixed Node.js local datetime microsecond precision, explicit follow tail handling, negative tail/head validation, and expanded follow coverage for no-tail, default-tail, boot-plus-since, and directory inputs.
- 2026-05-27: Ran second read-only reviewer cycle across glm, kimi, minimax, and qwen. All usable reports rated the batch production grade with non-blocking notes. Cheap non-blocking cleanup was applied for Go regex reuse, Go follow stdout error propagation, and directory follow coverage.

## Validation

Acceptance criteria evidence:

- Rust, Go, Node.js, and Python rewrites now expose file-backed `--since`, `--until`, `--boot`, and `--follow` behavior for supported `--file` and `--directory` inputs.
- Daemon-only operations remain unsupported: sync, flush, rotate, and relinquish-var continue to return controlled unsupported behavior.
- `tests/interoperability/run_journalctl_query_matrix.py` generated synthetic repo-local fixtures under `.local/interoperability/journalctl-query` and compared every reader against stock journalctl.
- The matrix passed against `systemd 260 (260.1-2-manjaro)` with no failures. Covered static `--since`/`--until` ranges, local datetime fractional seconds, `--boot=all`, implicit/latest/numeric boot descriptors, explicit boot IDs, boot ID offsets, combined boot/time filters, file and directory inputs, and live follow while a repository writer appended entries.
- Follow coverage includes file `--follow --no-tail --boot=all`, file default-tail `--follow`, file `--follow --no-tail --boot=0 --since @...`, and directory `--follow --no-tail --boot=all`.

Tests and commands:

- `python3 -m py_compile python/cmd/journalctl.py tests/interoperability/run_journalctl_query_matrix.py` - pass.
- `node --check node/cmd/journalctl/index.js` - pass.
- `env GOCACHE=.local/go-cache GOMODCACHE=.local/go-mod-cache GOPATH=.local/go-path go test ./cmd/journalctl` from `go/` - pass.
- `env CARGO_HOME=.local/cargo-home CARGO_TARGET_DIR=.local/cargo-target cargo check --manifest-path rust/Cargo.toml -p journalctl` - pass.
- `env GOCACHE=.local/go-cache GOMODCACHE=.local/go-mod-cache GOPATH=.local/go-path CARGO_HOME=.local/cargo-home CARGO_TARGET_DIR=.local/cargo-target npm_config_cache=.local/npm-cache PIP_CACHE_DIR=.local/pip-cache PYTHONPATH=.local/python-deps:python python3 tests/interoperability/run_journalctl_query_matrix.py` - pass, `status: PASS`, `systemd: systemd 260 (260.1-2-manjaro)`.
- `env GOCACHE=.local/go-cache GOMODCACHE=.local/go-mod-cache GOPATH=.local/go-path go test ./journal ./cmd/journalctl` from `go/` - pass.
- `env CARGO_HOME=.local/cargo-home CARGO_TARGET_DIR=.local/cargo-target cargo test --manifest-path rust/Cargo.toml -p journalctl` - pass, 9 tests.
- `env npm_config_cache=.local/npm-cache npm test` from `node/` - pass.
- `env PYTHONPATH=.local/python-deps:python PIP_CACHE_DIR=.local/pip-cache python3 -m pytest python/test_all.py` - pass, 61 tests.

Real-use evidence:

- Stock `journalctl --file` and `journalctl --directory` were used only against repo-local generated fixtures.
- The follow harness started a reader process, appended records through the repository Python writer while the journal remained active, synced after appends, and terminated the reader after observing the expected output.
- The directory follow case wrote to an active `.journal` file inside a directory and read through `--directory`, proving the supported file-backed directory path.

Reviewer findings and disposition:

- Round 1 glm: production-grade with non-blocking notes about follow coverage and polling performance. Coverage was expanded; polling performance is mapped to SOW-0009.
- Round 1 kimi: reported Node.js fractional microsecond loss, explicit tail-zero/default-tail ambiguity, and a claim that stock `--follow --until` exits. Node.js precision and tail handling were fixed. The stock until claim was rejected after `timeout 3 journalctl --file .local/interoperability/journalctl-query/multi-boot-file.journal --output=json --no-pager --quiet --follow --no-tail --until @1700004100.001 TEST_ID=journalctl-query` timed out with exit 124, matching the project behavior of waiting in follow mode.
- Round 1 minimax: production-grade with non-blocking notes. Cheap cleanup was applied where in scope.
- Round 1 qwen: no usable final report was produced.
- Round 2 glm, kimi, minimax, and qwen: all usable reports rated the batch production grade. Non-blocking notes about polling efficiency and broader performance are mapped to SOW-0009. The pre-existing Rust `--list-boots` formatting note is outside this SOW's query/follow surface.
- A third external review cycle was not run after the final cleanup because the second cycle already reached production-grade consensus, and the final changes were limited to Go regex reuse, Go stdout write-error propagation, and stronger directory follow test coverage. The full local validation suite and query matrix were rerun after those changes.

Same-failure search results:

- CLI docs and compatibility skill were updated so old statements that follow mode is unsupported for these rewrites are no longer present in the updated surfaces.
- The shared query matrix now protects the specific fixed regressions: local fractional timestamps, explicit/default follow tail behavior, boot plus since filtering, and file plus directory follow.

Sensitive data gate:

- All generated journal data is synthetic. No host live journal was queried, no `/var/log/journal` or `/run/log/journal` path was used, and no raw sensitive data was written to durable artifacts.

Artifact maintenance gate:

- `AGENTS.md`: no update needed; project-wide workflow and scope rules did not change.
- Runtime project skills: `.agents/skills/project-journal-compatibility/SKILL.md` updated with the new query/follow matrix requirement.
- Specs: `.agents/sow/specs/product-scope.md` updated to describe supported file-backed query/follow behavior.
- End-user/operator docs: Rust, Go, Node.js, Python READMEs and `tests/interoperability/README.md` updated.
- End-user/operator skills: no exported end-user/operator skill exists for this surface.
- SOW lifecycle: SOW-0034 completed as the last compatibility gap split from SOW-0022.
- `SOW-status.md`: updated during activation and completion.

Spec update:

- `product-scope.md` now records file-backed `--since`, `--until`, `--boot`, and `--follow` support and keeps daemon-only operations out of scope.

Project skill update:

- `project-journal-compatibility` now requires `run_journalctl_query_matrix.py` for file-backed journalctl query or follow changes.

End-user/operator docs update:

- Language READMEs and interoperability docs now identify the supported query/follow options and shared validation runner.

Lessons extracted:

- Stock journalctl follow behavior must be verified directly for file-backed fixtures; reviewer claims about daemon-backed behavior can be wrong for `--file`.
- Local datetime tests need fractional microsecond boundaries, not only epoch `@...` timestamps.
- Follow parity needs both file and directory inputs because directory traversal can hide active-file follow bugs.

Followup mapping:

- Follow polling efficiency, allocation behavior, and reader performance tuning are tracked by SOW-0009.
- Netdata integration remains tracked by SOW-0026 after SOW-0009.
- No additional valid deferred item remains in this SOW.

## Outcome

Completed. File-backed journalctl query/follow parity for the SOW-0034 surface is implemented and validated across Rust, Go, Node.js, Python, and stock journalctl comparison fixtures.

## Lessons Extracted

- Verify stock journalctl behavior on repo-local `--file`/`--directory` fixtures before accepting reviewer statements about option interactions.
- Include fractional local-time boundaries in time-filter parity tests.
- Treat live follow as a file and directory behavior, not only a single-file behavior.

## Followup

- SOW-0009: benchmark/profile/optimize reader and writer performance, including the polling and allocation notes reviewers raised.
- SOW-0026: Netdata integration after performance is acceptable.

## Regression Log

None yet.
