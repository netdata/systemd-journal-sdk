# SOW-0118 - Host helper release and Netdata integration planning

## Status

Status: in-progress

Sub-state: release `0.7.3` preparation in progress; Netdata integration inspection follows after the SDK release.

## Requirements

### Purpose

Release the Rust/Go strict writer and optional host-helper work from SOW-0115 in a version Netdata can consume, then plan the Netdata-side adoption for NetFlow and SNMP traps without changing files outside this repository.

### User Request

After finishing the portable writer identity/helper work, release the next SDK version and then discuss how the resulting SDK version should be integrated into Netdata for NetFlow and SNMP traps on Windows, FreeBSD, and macOS.

### Assistant Understanding

Facts:

- SOW-0115 changed the Rust and Go writer/helper API surface after the already completed `0.7.2` release.
- SOW-0115 is completed and committed in this repository.
- `0.7.3` is the next patch release after the public `0.7.2` release.
- This repository cannot modify the Netdata repository under the repository-boundary rule.
- NetFlow is a Rust consumer and SNMP traps is a Go consumer.

Inferences:

- A follow-up SDK release is needed before Netdata can consume the new helper APIs from a published version.
- Netdata-side code changes should be planned with explicit caller-owned identity choices, not automatic SDK writer fallback behavior.

Unknowns:

- Netdata-side integration details must be confirmed in the Netdata repository under a separate user-approved work item.

### Acceptance Criteria

- Execute or explicitly defer the SDK `0.7.3` release containing SOW-0115.
- Produce a concrete Netdata integration plan for Rust NetFlow and Go SNMP traps, with no writes outside this repository unless the user starts a Netdata-repo SOW.
- Record any required Netdata-repo follow-up with exact files/surfaces after read-only inspection.

## Analysis

Sources checked:

- SOW-0115 outcome and follow-up mapping.
- Existing pending release/integration SOW list.

Current state:

- SOW-0115 is completed and committed.
- The local branch is ahead of `origin/master` with SOW-0115 implementation and validation commits that must be included in the `0.7.3` release.
- Local and remote `v0.7.3` / `go/v0.7.3` tags do not exist as of the initial check.
- The Rust workspace now includes a publishable `systemd-journal-sdk-host` crate; the project release skill's publish order must include it before publication.

Risks:

- Releasing without confirming downstream import paths may force another SDK release.
- Netdata event sources may need different identity semantics: local collector host, remote network device, or synthetic per-flow/per-trap anchors.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- SOW-0115 introduced new public Rust/Go helper packages and stricter writer behavior that downstream Netdata consumers cannot use until a release containing those changes is published and planned.

Evidence reviewed:

- SOW-0115 completed SOW and validation.
- Release tagging skill and prior SOW-0117 release process.
- `rust/Cargo.toml` workspace package/dependency versions and crate membership.
- `go/go.mod` module path.

Affected contracts and surfaces:

- Rust crates, including the new `systemd-journal-sdk-host` crate.
- Go module tag `go/v0.7.3`.
- Root release tag `v0.7.3`.
- Consumer install docs and README snippets.
- Project release skill publish order.
- Downstream Netdata integration guidance.

Existing patterns to reuse:

- Project release SOWs, especially SOW-0117.
- Prior Netdata integration SOWs, especially SOW-0047 through SOW-0050.
- Project release-tagging skill, updated in this SOW to include the new Rust host helper crate in dependency order.

Risk and blast radius:

- Release/versioning mistakes, especially missing or mismatched root and Go submodule tags.
- Rust crates.io publication order mistakes, especially omitting `systemd-journal-sdk-host`.
- Downstream build breakage if install docs or package versions lag the actual release.
- Incorrect Netdata event identity semantics if local host helper values are used where the event source is a remote network device.

Sensitive data handling plan:

- No raw Netdata customer or production data is needed. Any Netdata examples must use synthetic IDs and redacted paths.

Implementation plan:

1. Update SOW-0118 and `SOW-status.md` to active `in-progress` state.
2. Update the release skill publish order to include `systemd-journal-sdk-host`.
3. Prepare `0.7.3` versioned Rust/docs changes.
4. Run local release validation.
5. Run the read-only reviewer gate before irreversible publication.
6. Publish Rust crates in dependency order, push `master`, create/push annotated `v0.7.3` and `go/v0.7.3` tags, and verify remote tags plus Go module lookup.
7. Inspect Netdata consumers read-only and write a concrete integration plan.

Validation plan:

- Release validation follows the project release-tagging skill.
- Rust and Go test/doc validation must pass before publication.
- Integration planning cites file paths and line numbers from read-only Netdata inspection.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: update `project-release-tagging` because release process changes to include `systemd-journal-sdk-host`.
- Specs: update only if release/integration changes public contracts.
- End-user/operator docs: update release/install docs if version changes.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: move from pending to current for release work, then move to done only after release and integration plan are complete.
- SOW-status.md: updated when this SOW becomes active or changes status.

Open-source reference evidence:

- None yet; release preparation uses this repository. Netdata inspection will cite the Netdata repository commit and repository-relative file paths.

Open decisions:

- Whether actual Netdata code changes should start in the Netdata repository after this release and read-only integration plan.

## Implications And Decisions

### Decision 1 - SDK release version

Decision: release `0.7.3`.

Evidence:

- `0.7.2` was already published and tagged by SOW-0117.
- SOW-0115 landed after `0.7.2` and adds the Rust/Go strict writer and optional host helper surfaces needed by Netdata.
- No incompatible `1.0.0` or minor-version scope was requested for this release.

Classification: long-term-best.

Implications:

- Rust workspace versions, install docs, crates.io publication, and root/Go tags all target `0.7.3`.
- The `go/v0.7.3` submodule tag must point to the same commit as `v0.7.3`.

### Decision 2 - Repository boundary for Netdata work

Decision: this SOW performs read-only Netdata inspection and records an integration plan; actual Netdata edits require a separate Netdata-repository work item.

Evidence:

- `AGENTS.md` repository-boundary policy forbids writes outside this repository.
- The Netdata repository has its own untracked local work risk and must not be edited from this SDK SOW.

Classification: long-term-best.

Implications:

- This SOW can identify exact Netdata files and dependency changes.
- No Netdata code is changed until the user starts or approves work in the Netdata repository.

## Plan

1. Activate SOW-0118 and repair release-process docs for the new host crate.
2. Prepare and validate release `0.7.3`.
3. Run release reviewers.
4. Publish Rust crates and push release tags.
5. Inspect Netdata Rust/Go consumers read-only.
6. Produce integration plan and follow-up mapping.

## Delegation Plan

Implementer:

- Local project-manager edits for release metadata, docs, SOW updates, validation, publication, and read-only Netdata inspection. No delegated implementer is needed because the release work should not add product code.

Reviewers:

- Read-only release reviewers run after local validation and before irreversible publication.

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

- Record release, validation, or integration-inspection blockers in this SOW.

## Execution Log

### 2026-06-17

- Pending SOW created as SOW-0115 follow-up mapping for release and Netdata integration planning.
- SOW activated after SOW-0115 completion; release target recorded as `0.7.3`.
- Updated Rust workspace/docs install versions to `0.7.3`.
- Updated `project-release-tagging` so the Rust publish order includes `systemd-journal-sdk-host`.
- Added the Rust host helper crate to the published-package docs table and dependency example.
- Fixed a stale Rust facade test expectation: the low-level writer now clamps same-boot backwards monotonic timestamps, so the test now asserts clamping to `last + 1` and successful verification.
- Ran read-only release reviewers: `glm`, `minimax`, `mimo`, `kimi`, `qwen`, and `deepseek` all voted `READY TO RELEASE` with no blockers.
- Fixed non-blocking reviewer cleanup: product-scope now shows `0.7.3`, lists `systemd-journal-sdk-host`, and a retired Python experiment test no longer carries the active Rust stale test name.

## Validation

Acceptance criteria evidence:

- Release preparation complete for `0.7.3`; publication and Netdata integration plan are still pending.

Tests or equivalent validation:

- `git diff --check`: passed.
- `sed -n '1,20p' go/go.mod`: module path confirmed as `github.com/netdata/systemd-journal-sdk/go`.
- `git tag -l 'v0.7.3' 'go/v0.7.3'` and remote `git ls-remote --tags ...`: no local or remote `0.7.3` tags existed before release prep.
- `rg -n "0\\.7\\.2" README.md rust/README.md docs rust/Cargo.toml`: no stale install-version references after release prep.
- `cargo metadata --manifest-path rust/Cargo.toml --no-deps --format-version 1`: publishable `systemd-journal-sdk-*` packages reported version `0.7.3`, including `systemd-journal-sdk-host`.
- `go test ./...`: passed. Output included `github.com/netdata/systemd-journal-sdk/go/journal` and `github.com/netdata/systemd-journal-sdk/go/journalhost`.
- Initial `cargo test --manifest-path rust/Cargo.toml`: failed only `tests::raw_writer_backward_monotonic_pass_through_fails_verification`, because the current writer clamps same-boot backwards monotonic timestamps.
- Focused repair validation: `cargo test --manifest-path rust/Cargo.toml -p systemd-journal-sdk raw_writer_backward_monotonic_is_clamped_and_verifies`: passed.
- Final Rust validation: `cargo test --manifest-path rust/Cargo.toml`: passed.
- `python3 tests/docs/check_wiki_docs.py`: passed, `validated 15 wiki markdown files`.
- `python3 tests/docs/verify_examples.py`: passed, `31 of 31` verified examples.
- `.agents/sow/audit.sh`: passed, `SOW initialization complete and clean`.

Real-use evidence:

- Pending crates.io publication, Git tag verification, Go module lookup, and read-only Netdata inspection.

Reviewer findings:

- `glm`: `READY TO RELEASE`; no blockers. Non-blocking notes: patch-level release contains 0.x breaking strict-writer changes by design; `go 1.26` was pre-existing; `LogIdentityMode::Auto` wording is acceptable.
- `minimax`: `READY TO RELEASE`; no blockers. Non-blocking notes: final SOW closeout gates are expected to remain pending before publication; host publish order is safe.
- `qwen`: `READY TO RELEASE`; no blockers. Non-blocking notes: product-scope spec had stale `0.6.4` and omitted `systemd-journal-sdk-host`; fixed in this SOW.
- `kimi`: `READY TO RELEASE`; no blockers. Non-blocking notes: retired Python experiment carried the old stale Rust test name; renamed. `journal-common` dry-run passed; downstream crate dry-runs are expected to fail before `0.7.3` dependencies are published. Rust `journal-host` unsafe blocks are expected platform FFI and documented.
- `deepseek`: `READY TO RELEASE`; no blockers. Non-blocking notes: SOW final gates remain pending until publication and Netdata inspection; host crate dry-run failure before `common` publication is expected.
- `mimo`: `READY TO RELEASE`; no blockers. Non-blocking notes: final SOW gates are pending by design; `go 1.26` is pre-existing; optional writer-lock helper `/proc` reads are opt-in and excluded from core reader/writer runtime paths by the runtime-purity contract.

Same-failure scan:

- Version-reference scan found no stale `0.7.2` install snippets in `README.md`, `rust/README.md`, `docs/`, or `rust/Cargo.toml`.
- Same-failure evidence for the Rust monotonic test: `journal-core` already has `same_boot_monotonic_is_clamped_by_low_level_writer`, and the facade test now matches that contract.
- Reviewer same-failure cleanup: `.agents/sow/specs/product-scope.md` was checked for stale `0.6.4` release examples and missing `systemd-journal-sdk-host`; both were fixed.

Sensitive data gate:

- Reviewer and audit scans found no raw secrets, credentials, bearer tokens, SNMP communities, community member names, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details. Final scan still runs before close.

Artifact maintenance gate:

- AGENTS.md: pending final assessment.
- Runtime project skills: `project-release-tagging` updated to include `systemd-journal-sdk-host` in the Rust publish order.
- Specs: product scope updated to include `systemd-journal-sdk-host` and current `0.7.3` package example.
- End-user/operator docs: install-version snippets updated to `0.7.3`; final publication evidence still pending.
- End-user/operator skills: pending final assessment.
- SOW lifecycle: moved from pending to current; final close requires `Status: completed` and movement to `.agents/sow/done/`.
- SOW-status.md: updated for current in-progress state.

Specs update:

- `.agents/sow/specs/product-scope.md` updated for the new host helper crate and current release example.

Project skills update:

- `project-release-tagging` updated because the release process now includes `systemd-journal-sdk-host`.

End-user/operator docs update:

- `README.md`, `rust/README.md`, and `docs/` install snippets updated to `0.7.3`.

End-user/operator skills update:

- Pending final assessment.

Lessons:

- Pending.

Follow-up mapping:

- Pending Netdata integration-plan outcome.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

Pending.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
