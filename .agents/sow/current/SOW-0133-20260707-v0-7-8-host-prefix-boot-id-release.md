# SOW-0133 - v0.7.8 Host-Prefix Boot-ID Release

## Status

Status: in-progress

Sub-state: pre-implementation gate ready; implementation starting.

## Requirements

### Purpose

Make Netdata journal writers keep existing behavior by default, while
containerized Netdata instances that expose `NETDATA_HOST_PREFIX` write host
machine-id and host boot-id values into SDK-created journals.

### User Request

Release `systemd-journal-sdk` `0.7.8` and update the Netdata integration to use
that release.

### Assistant Understanding

Facts:

- `0.7.7` added Linux host-prefix machine-id lookup for Rust and Go
  journalhost helpers.
- `0.7.7` Linux boot-id lookup still reads unprefixed
  `/proc/sys/kernel/random/boot_id` in Go and Rust.
- Netdata's native C helper reads boot-id from
  `<netdata_configured_host_prefix>/proc/sys/kernel/random/boot_id`.
- Netdata integration already passes `NETDATA_HOST_PREFIX` to the SDK helper for
  NetFlow and SNMP traps.

Inferences:

- To meet the Netdata container requirement as a code guarantee, the SDK
  journalhost helpers must apply host-prefix to Linux boot-id lookup too.
- The default empty-prefix path must remain byte-for-byte behavior-compatible
  with `0.7.7`.

Unknowns:

- None blocking. Release publication depends on registry/network availability.

### Acceptance Criteria

- Go journalhost Linux tests prove empty prefix uses default boot-id behavior and
  non-empty prefix prefers `<prefix>/proc/sys/kernel/random/boot_id`.
- Rust journal-host Linux tests prove empty prefix uses default boot-id behavior
  and non-empty prefix prefers `<prefix>/proc/sys/kernel/random/boot_id`.
- Rust and Go public helper docs state that host prefix affects Linux machine-id
  and boot-id.
- Rust workspace package versions and internal dependency versions are bumped to
  `0.7.8`.
- Rust crates are published as `0.7.8`; root tag `v0.7.8` and Go submodule tag
  `go/v0.7.8` point to the same release commit.
- Netdata integration is updated from `0.7.7` to `0.7.8` for Rust NetFlow and Go
  SNMP traps.

## Analysis

Sources checked:

- `go/journalhost/load.go`
- `go/journalhost/load_linux.go`
- `go/journalhost/load_linux_test.go`
- `rust/src/crates/journal-host/src/lib.rs`
- `rust/src/crates/journal-host/src/platform/linux.rs`
- Netdata `src/libnetdata/os/boot_id.c`
- Netdata `src/daemon/environment.c`

Current state:

- Go `LoadOptions.HostFilesystemPrefix` documentation mentions only
  machine-id.
- Go `loadLinuxMachineID` uses `opts.HostFilesystemPrefix`.
- Go `loadLinuxBootID` reads `/proc/sys/kernel/random/boot_id` directly.
- Rust `LoadOptions::with_host_filesystem_prefix` documentation mentions only
  machine-id.
- Rust Linux helper has the same host-prefix machine-id-only shape.

Risks:

- Changing boot-id source can change journal entry `_BOOT_ID` and header
  `tail_entry_boot_id` only for callers that explicitly pass a host prefix.
- If the host-prefix boot-id file exists but is invalid, the safest behavior is
  to return an explicit error rather than silently mixing host machine-id with
  container boot-id.
- This is an optional helper change, not a core reader/writer runtime change.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- `0.7.7` satisfies host machine-id resolution but not a strict host boot-id
  guarantee. The helper still reads the Linux boot-id from the caller's
  `/proc/sys/kernel/random/boot_id` path.

Evidence reviewed:

- `go/journalhost/load_linux.go`: boot-id reads `/proc/sys/kernel/random/boot_id`
  without the configured host prefix.
- `rust/src/crates/journal-host/src/platform/linux.rs`: boot-id reads the same
  unprefixed path.
- Netdata `src/libnetdata/os/boot_id.c`: native behavior prefixes the boot-id
  path with `netdata_configured_host_prefix`.

Affected contracts and surfaces:

- Go `journalhost.LoadOptions.HostFilesystemPrefix`.
- Rust `journal_host::LoadOptions::with_host_filesystem_prefix`.
- Linux optional identity helper behavior.
- Rust crates.io packages and Go module release tags.
- Netdata NetFlow and SNMP trap SDK dependency versions.

Existing patterns to reuse:

- Existing `loadLinuxMachineIDFromRoot` / `linuxMachineIDPaths` test style in Go.
- Existing Rust Linux helper path construction and tests.
- Existing release workflow from SOW-0132.
- Netdata integration already passes host prefix into SDK helper options.

Risk and blast radius:

- Default empty-prefix behavior remains unchanged.
- Explicit host-prefix callers get a stricter host identity guarantee.
- Invalid prefixed boot-id files should fail fast, matching invalid prefixed
  machine-id behavior and avoiding mixed host/container identity.
- No core journal reader/writer path may gain host probing.

Sensitive data handling plan:

- No secrets, credentials, bearer tokens, SNMP communities, customer data,
  personal data, private endpoints, or proprietary incident details are needed.
- Durable artifacts will cite file paths and behavior only.

Implementation plan:

1. Add prefixed Linux boot-id resolution in Go journalhost helper and tests.
2. Add prefixed Linux boot-id resolution in Rust journal-host helper and tests.
3. Update helper docs to mention boot-id.
4. Bump SDK versions to `0.7.8`, update locks, run validation.
5. Commit, publish Rust crates, tag `v0.7.8` and `go/v0.7.8`.
6. Update Netdata Rust/Go dependencies to `0.7.8` and rerun targeted tests.

Validation plan:

- Go targeted journalhost tests.
- Rust targeted journal-host tests.
- Full Go tests if time permits before release; at minimum full package tests
  for changed helper.
- Rust workspace tests or the release-equivalent validation used in SOW-0132.
- Release tag verification with peeled tag targets.
- Netdata targeted Rust NetFlow and Go SNMP trap tests.
- Same-failure search for remaining unprefixed Linux boot-id helper reads.

Artifact impact plan:

- AGENTS.md: no workflow or guardrail change expected.
- Runtime project skills: no workflow change expected.
- Specs: product scope should mention host-prefix boot-id in optional helper.
- End-user/operator docs: README/API helper docs should mention boot-id.
- End-user/operator skills: no output/reference skills affected.
- SOW lifecycle: new current SOW-0133; close after release and Netdata update.
- SOW-status.md: update current/recent state.

Open-source reference evidence:

- No external OSS reference needed. This mirrors Netdata's own host-prefix
  identity helper behavior.

Open decisions:

- User decision is explicit: release `0.7.8` and update Netdata integration.
- Implementation routing decision for this SOW: local assistant implements and
  orchestrates, per user's standing request not to use a separate implementer.

## Implications And Decisions

1. Decision: apply host prefix to Linux boot-id in the optional helper now.
   - Selected option: now.
   - Reason: this is required for a strict host identity guarantee in
     containerized Netdata.

## Plan

1. SDK helper behavior and tests.
2. Version bump and release.
3. Netdata dependency update and targeted validation.

## Delegation Plan

Implementer:

- Local assistant, per user routing decision.

Reviewers:

- Local validation first. External read-only reviewers may be run if release
  validation exposes uncertainty or if requested before final publish.

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

- If release publication or tag creation fails, record exact failure and stop
  before moving or recreating any pushed tag.

## Execution Log

### 2026-07-07

- Created SOW after confirming `0.7.7` host prefix covers machine-id but not a
  strict prefixed boot-id path.
- Implemented Go Linux boot-id host-prefix lookup in `go/journalhost` and
  added tests for default, host-prefixed, missing-host-prefix fallback, and
  invalid-host-file behavior.
- Implemented Rust Linux boot-id host-prefix lookup in `journal-host` and
  added matching tests.
- Updated helper API docs, README snippets, Rust package versions, Cargo.lock,
  and product-scope helper behavior.
- Verified no `v0.7.8` or `go/v0.7.8` tag exists locally or remotely before
  release.
- `systemd-journal-sdk-common 0.7.8` publish dry-run passed before the
  release-prep commit. The remaining publish dry-runs must run one package at a
  time immediately before upload, because dependent crates need the previous
  `0.7.8` package to be visible in the crates.io index.

## Validation

Acceptance criteria evidence:

- Go `LoadOptions.HostFilesystemPrefix` now documents Linux machine-id and
  boot-id lookup, and `loadPlatform` passes options into `loadLinuxBootID`.
- Go tests cover default container boot-id, explicit host-prefixed boot-id,
  missing host-prefix fallback, and invalid host-prefixed boot-id failure.
- Rust `LoadOptions::with_host_filesystem_prefix` now documents Linux
  machine-id and boot-id lookup, and Linux `load()` passes the prefix into
  `load_boot_id`.
- Rust tests cover default container boot-id, explicit host-prefixed boot-id,
  missing host-prefix fallback, and invalid host-prefixed boot-id failure.
- `rust/Cargo.toml` records workspace version `0.7.8` and internal publishable
  crate dependencies at `0.7.8`.
- `README.md`, `rust/README.md`, and `go/README.md` describe the `0.7.8`
  release/API behavior.

Tests or equivalent validation:

- PASS: `gofmt -w go/journalhost/load.go go/journalhost/load_linux.go go/journalhost/load_linux_test.go`
- PASS: `git diff --check`
- PASS: `cargo fmt --manifest-path rust/Cargo.toml --all --check`
- PASS: `go test ./...` from `go/`
- PASS: `cargo test --manifest-path rust/Cargo.toml --workspace --locked`
- PASS: `python3 tests/docs/check_wiki_docs.py`
- PASS: `python3 tests/docs/verify_examples.py --timeout 60`
- Passed: `.agents/sow/audit.sh`
- PASS: `cargo publish --manifest-path rust/src/crates/journal-common/Cargo.toml --dry-run --allow-dirty`

Real-use evidence:

- Pre-release real-use evidence is local only. Public package, tag, and Go
  module resolution evidence will be recorded after release publication.

Reviewer findings:

- No external reviewer run was performed for this narrow patch release. The
  change is covered by focused Go/Rust tests, full Go/Rust test suites, docs
  verification, SOW audit, and the `0.7.7` release precedent.

Same-failure scan:

- `rg` scan for `HostFilesystemPrefix`, `with_host_filesystem_prefix`, `0.7.7`,
  and direct Linux boot-id reads found no stale `0.7.7` references in the
  release-facing SDK files that were updated.
- Remaining direct Linux boot-id reads are in Go and Rust writer-lock-owner
  helpers:
  - `go/journal/lock_owner_linux.go`
  - `rust/src/crates/journal-core/src/file/lock.rs`
- These helpers are layer-4 writer-lock staleness tokens. They are not the
  journalhost identity helper used by Netdata to write `_BOOT_ID` and journal
  header boot identity, so no release-blocking change is needed there.

Sensitive data gate:

- No secrets, credentials, bearer tokens, SNMP communities, private endpoints,
  customer data, personal data, or raw production logs were used or written.

Artifact maintenance gate:

- AGENTS.md: no update needed; workflow and guardrails are unchanged.
- Runtime project skills: no update needed; release-tagging and compatibility
  skills already covered this workflow.
- Specs: updated `product-scope.md` for host-prefixed Linux boot-id lookup.
- End-user/operator docs: updated README/helper docs for host machine and boot
  identity.
- End-user/operator skills: no output/reference skills exist or were affected.
- SOW lifecycle: current SOW remains in-progress until release and Netdata
  integration are complete.
- SOW-status.md: updated to show SOW-0133 current.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` to describe default Linux
  boot-id lookup and explicit host-prefix boot-id precedence.

Project skills update:

- No project skill update is needed. The existing release-tagging skill covers
  root tags, Go submodule tags, and Rust crate publication order.

End-user/operator docs update:

- Updated `README.md`, `rust/README.md`, and `go/README.md`.

End-user/operator skills update:

- No end-user/operator skills exist in this repository.

Lessons:

- The `0.7.7` machine-id release left boot-id as a separate code path. Host
  identity options must be checked for every identity component, not just
  machine-id.

Follow-up mapping:

- No valid deferred SDK follow-up remains for the Netdata host identity goal.

## Outcome

Pending release publication and Netdata dependency update.

## Lessons Extracted

- Host identity is a pair for journald compatibility: machine-id and boot-id.
  Future helper changes should validate both together when container behavior is
  involved.

## Followup

None for the SDK behavior. Netdata dependency update is part of this same SOW.
