# SOW-0127 - Journal-Host Container-Aware Machine-ID Resolution

## Status

Status: completed

Sub-state: created 2026-07-06 from gaps surfaced by the Netdata
vendored-journal elimination analysis; awaiting design decisions and
prioritization. Activated 2026-07-06 after user routing decision for local
project-manager implementation. Completed 2026-07-06.

## Requirements

### Purpose

Let containerized consumers (Netdata docker deployments and similar) resolve
the HOST machine identity through the host helpers, restoring the
container-awareness the pre-SDK Netdata code had, as a cross-platform SDK
feature rather than a per-consumer workaround.

### User Request

The user directed (2026-07-06, during the Netdata vendored-journal
elimination work): SDK functionality gaps are filled in the SDK — the host
crate exists precisely to own cross-platform machine-id / boot-id /
monotonic support. Netdata consumers must not carry local identity
workarounds.

### Assistant Understanding

Facts:

- Rust `systemd-journal-sdk-host` 0.7.6 Linux machine-id resolution tries
  `/etc/machine-id` then `/var/lib/dbus/machine-id`, then returns a hard
  `io::Error` ("linux machine-id not found") — `rust/src/crates/journal-host/
  src/platform/linux.rs:42-51` (crate published as
  `systemd-journal-sdk-host` 0.7.6). There is no machine-id synthesis; the
  state-backed `Degraded` path covers boot-id only (`src/state.rs`).
- Go `go/journalhost/load_linux.go:51` has the identical two-path list —
  same gap, cross-language.
- The behavior this replaced: Netdata's vendored `journal-common` fell back
  to a `/host/`-prefixed path when the primary path was ABSENT
  (NotFound only, not on empty/invalid content):
  `netdata/netdata @ 17a7eb31da`
  `src/crates/journal-common/src/system.rs:8-31` (`read_host_file`).
  Netdata containers commonly bind-mount the host filesystem at `/host`.
- Consumers already live on the new (gapped) behavior: netflow-plugin since
  its 0.7.4 migration, otel-plugin as of the vendored-journal elimination —
  `netdata/netdata @ 17a7eb31da src/crates/netflow-plugin/src/
  local_journal_host.rs`.
- Related design lineage: SOW-0115 (portable writer identity helpers)
  defined the strict caller-provided identity contract plus the optional
  local-host helper layer. Container awareness is an extension of that
  helper layer, not a contract change.

Inferences:

- Impact window: containers whose image lacks `/etc/machine-id` AND mount
  the host at `/host`. In that case the old code resolved the host identity;
  the SDK helper errors, which under the strict writer contract means the
  writer cannot start.
- A second, subtler case: containers whose image HAS its own
  `/etc/machine-id` resolve the CONTAINER identity where the operator may
  want the HOST identity. The old Netdata fallback did not handle this case
  either (it only fired on NotFound), so fixing it is optional scope.

Unknowns:

- Whether the mechanism should be opt-in (LoadOptions builder), env-driven,
  or auto-detected; and the prefix contract (fixed `/host`, configurable, or
  a list).

### Acceptance Criteria

- `LoadOptions` (Rust) and the Go equivalent gain a documented mechanism to
  resolve host identity from a mounted host filesystem prefix.
- Linux resolution order with the mechanism enabled is explicit, documented,
  and covered by tests (including: container file absent, container file
  present, host prefix absent).
- Rust/Go parity per project conventions; docs updated
  (Options-Reference or host-helper docs).
- No default-behavior change for non-container consumers unless the user
  explicitly decides otherwise.

## Analysis

Sources checked:

- `rust/src/crates/journal-host/src/platform/linux.rs`, `src/state.rs`,
  `src/lib.rs` (LoadOptions surface).
- `go/journalhost/load_linux.go`.
- `netdata/netdata @ 17a7eb31da` `src/crates/journal-common/src/system.rs`
  (replaced behavior), `src/crates/netflow-plugin/src/local_journal_host.rs`
  (current consumer pattern).

Current state:

- Hard failure on containers without a machine-id; no host-prefix support in
  either language.

Risks:

- Auto-detection magic (silently preferring `/host`) could pick the wrong
  identity for non-Netdata consumers; an explicit opt-in avoids this.
- Identity changes across upgrades alter the journal directory naming
  (`<machine_id>/`) for writers — mechanism must be stable per deployment.

## Pre-Implementation Gate

Status: ready (user authorized local project-manager implementation on 2026-07-06)

Problem / root-cause model:

- The host crate was extracted from Netdata's `journal-common` for
  cross-platform identity, but the extraction dropped the container
  (`/host` prefix) resolution path, and no equivalent exists.

Evidence reviewed:

- See Assistant Understanding facts (file:line above).

Affected contracts and surfaces:

- `LoadOptions` public API (additive), Go `journalhost` options, host-helper
  documentation. No writer/reader contract changes.

Existing patterns to reuse:

- `LoadOptions` builder style (`with_state_dir`, `with_state_file_name`);
  state-backed boot-id design from SOW-0115 for "explicit, no magic"
  precedent.

Risk and blast radius:

- Low: additive option; default path unchanged.

Sensitive data handling plan:

- Synthetic machine-id fixtures only; no real host identifiers in tests or
  docs.

Implementation plan:

1. User decision on mechanism shape (builder option vs env vs auto-detect).
2. Implement Rust + Go with tests.
3. Docs + parity evidence.

Validation plan:

- Unit tests for all resolution branches in both languages; doc build.

Artifact impact plan:

- Specs: host-helper spec updated with the container mechanism.
- Docs: Options-Reference / host docs updated.
- SOW-status ledgers updated.

Open-source reference evidence:

- `netdata/netdata @ 17a7eb31da`
  `src/crates/journal-common/src/system.rs:8-31`
  `src/crates/netflow-plugin/src/local_journal_host.rs`

Open decisions:

- 2026-07-06 user routing/design decision: the project manager implements and
  orchestrates this SOW directly; no separate external implementer model is
  used. Existing SOW analysis is planning evidence and hints, not a frozen
  design.
- Mechanism shape: implement an explicit `LoadOptions` builder option, e.g.
  `with_host_filesystem_prefix(path)`, off by default. This is the
  long-term-best option because it avoids silent identity magic while giving
  containerized consumers a supported SDK-owned path.
- Scope: when the option is set, prefixed host machine-id paths take precedence
  over container-local paths so callers can intentionally request host identity
  even if the container image has its own `/etc/machine-id`.

## Implications And Decisions

- 2026-07-06: created per user direction that SDK gaps found during Netdata
  integration are filled in the SDK.

## Plan

1. Resolve open decisions with the user.
2. Implement, test, document (Rust + Go).

## Delegation Plan

Implementer:

- Local project-manager implementation per user routing decision on
  2026-07-06. No separate external implementer model is used for this SOW.

Reviewers:

- Read-only reviewers from the approved pool.

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

- Record blockers and missing evidence before changing scope.

## Execution Log

### 2026-07-06

- Created from the Netdata vendored-journal elimination deep analysis
  (netdata-side SOW: eliminate-vendored-journal-crates).
- Implemented explicit host filesystem prefix support for Rust
  `journal_host::LoadOptions` and Go `journalhost.LoadOptions`.
- Added Linux tests for default container-local behavior, explicit host-prefix
  precedence, fallback to container-local paths, and host DBus machine-id
  precedence.
- Updated product scope and consumer docs for explicit container host-prefix
  identity loading.
- Second reviewer pass found real closeout gaps: host-prefixed invalid
  `/etc/machine-id` could fall through to host DBus, empty Rust prefix did not
  match Go's disabled-prefix behavior, and Go accepted all-zero machine IDs.
  Rust and Go now fail on the first present invalid host-prefixed file, treat an
  empty Rust prefix as disabled, and reject all-zero machine IDs.
- Read-only Netdata impact check at `netdata/netdata @ 93d4f98c65b4` found
  existing Rust and Go host-helper call sites continue compiling because the new
  host-prefix API is opt-in.

## Validation

Acceptance criteria evidence:

- Rust `LoadOptions::with_host_filesystem_prefix(path)` checks
  `<path>/etc/machine-id` and `<path>/var/lib/dbus/machine-id` before
  container-local paths only when configured:
  `rust/src/crates/journal-host/src/lib.rs`,
  `rust/src/crates/journal-host/src/platform/linux.rs`.
- Go `journalhost.LoadOptions.HostFilesystemPrefix` implements the same
  Linux candidate ordering and diagnostics:
  `go/journalhost/load.go`, `go/journalhost/load_linux.go`.

Tests or equivalent validation:

- `cargo test --manifest-path rust/Cargo.toml --workspace` - passed.
- `go test ./...` from `go/` - passed.
- `python3 tests/docs/check_wiki_docs.py` - passed.
- `python3 tests/docs/verify_examples.py` - passed 31/31 examples.
- Focused repair tests passed:
  `TestLinuxMachineIDErrorsOnInvalidFirstHostFileEvenWhenHostDBusExists`,
  `TestLinuxMachineIDRejectsAllZeroHostMachineID`,
  `TestLinuxMachineIDEmptyHostPrefixKeepsContainerDefault`,
  `machine_id_errors_on_invalid_first_host_file_even_when_host_dbus_exists`,
  and `machine_id_empty_host_prefix_keeps_container_default`.

Real-use evidence:

- Synthetic Linux host-prefix tests cover the consumer container layout
  without reading workstation or production identity files.

Reviewer findings:

- 2026-07-06: read-only reviewers were run with the SOW filename and complete
  changed surface. Reviewers found no SOW-0127 blocker. Host-prefix loading was
  classified as an opt-in API addition; Netdata can adopt it at the Rust and Go
  host-helper call sites, but existing callers keep default behavior. A reviewer
  found that an invalid host-prefixed machine-id file could silently fall back to
  container identity; Rust and Go now fail on present invalid host-prefixed files
  while still falling back when host-prefixed files are absent.
- 2026-07-06 repeat review: Claude voted not production-grade as closed SOWs and
  found the additional host-prefix/zero-ID gaps listed above. GLM timed out
  before a final verdict after partially reproducing real concerns. Minimax,
  Deepseek, Kimi, and Qwen returned production-grade or production-grade with
  non-blocking notes. The real host-helper findings are fixed and covered by
  focused tests.
- 2026-07-06 second repeat review: Claude failed with an API connection reset;
  GLM and Minimax timed out without final verdicts; Deepseek, Kimi, and Qwen
  returned production-grade with non-blocking notes. Their remaining
  host-helper notes were documentation or adoption considerations, not SOW-0127
  blockers.

Same-failure scan:

- Rust and Go host helper implementations were both updated; no remaining
  machine-id host-prefix APIs were found outside these helper surfaces. Tests
  cover default local lookup, explicit host prefix lookup, absent-host fallback,
  host DBus priority, invalid-host-file failure, empty-prefix disabled
  behavior, invalid-first-host-file failure before DBus fallback, and all-zero
  machine-id rejection.

Sensitive data gate:

- SOW, docs, tests, and code use synthetic machine IDs only; no raw sensitive
  data was recorded.

Artifact maintenance gate:

- `AGENTS.md`: no project-wide workflow change required.
- Runtime project skills: no HOW-to-work rule changed.
- Specs: product scope updated.
- End-user/operator docs: Rust API, Go API, Writer APIs, and Go README updated.
- SOW lifecycle: completed and moved to done.
- `.agents/sow/SOW-status.md`: updated.

Specs update:

- `.agents/sow/specs/product-scope.md` records explicit Linux host filesystem
  prefix identity loading.

Project skills update:

- No project skill update needed; this changed SDK behavior, not repository
  workflow.

End-user/operator docs update:

- `docs/Rust-API.md`, `docs/Go-API.md`, `docs/Writer-APIs.md`,
  `docs/Options-Reference.md`, and `go/README.md` document explicit host-prefix
  identity loading and invalid-host-file behavior.

End-user/operator skills update:

- No output/operator skills are maintained for this SDK surface.

Lessons:

- Keep container host identity as explicit opt-in. Silent host probing would
  violate the runtime purity contract and surprise container-local callers.

Follow-up mapping:

- No implementation follow-up remains for this SOW. Pending SOW-0066 release
  planning must classify `journal_host::LoadOptions::host_filesystem_prefix` as
  a Rust public struct-field addition and
  `journalhost.LoadOptions.HostFilesystemPrefix` as a Go additive field.
  Existing Netdata call sites can adopt the host prefix explicitly but do not
  have to change to keep current behavior.

## Outcome

Completed. Rust and Go host helpers now support explicit host filesystem
prefix machine-id resolution while preserving container-local defaults.

## Lessons Extracted

Explicit opt-in preserves runtime purity and avoids ambiguous host/container
identity selection.

## Followup

None.

## Regression Log

None yet.
