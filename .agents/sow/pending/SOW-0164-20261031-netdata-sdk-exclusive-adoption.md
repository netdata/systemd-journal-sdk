# SOW-0164 - Netdata SDK-Exclusive Adoption And Vendored Journal Removal

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: created on 2026-10-31 by user decision; supersedes SOW-0049 and
SOW-0050 (SOW-0048 closed as obsolete). Blocked on SOW-0162 and the v0.11.0
release (SOW-0163). Changes the Netdata repository, which needs explicit user
authorization of that target at activation.

## Requirements

### Purpose

Netdata uses this SDK for journal files wherever it does not use libsystemd,
and every vendored or local journal implementation in Netdata is removed.
Builds that have libsystemd keep using it during a transition period (user
decision, 2026-11-03).

### User Request

"The idea is the old crates in netdata to vanish and use the SDK exclusively."
"One SOW for the removal of the old and the use of the new." The user also
asked to port the legacy logs viewer to the SDK.

### Assistant Understanding

Facts (Netdata master, `netdata/netdata @ eaeb95f5c4`, 2026-10-31):

- `systemd-journal.plugin` uses vendored `src/crates/jf` (`journal_reader_ffi`)
  in Docker and static builds (`packaging/docker/Dockerfile:42`,
  `packaging/makeself/jobs/70-netdata-git.install.sh:40`); other builds link
  libsystemd. Netdata documents a broken no-libsystemd fallback
  (`packaging/cmake/Modules/NetdataPluginSystemdJournal.cmake:16-22`).
- `otel-legacy-logs` (read-only viewer of the former OTel plugin's journal
  files, compiled into `otel-plugin`, `src/crates/otel-legacy-logs/src/lib.rs:1-10`) uses
  vendored `journal-function` and `journal-index`; `journal-function` uses
  vendored `journal-core`, `journal-engine`, `journal-index`, `journal-registry`.
- `ng-ingest` (experiment) uses vendored `journal-common` only for
  `load_machine_id` / `load_boot_id` (`src/crates/ng-ingest/src/main.rs:148,155`).
- Vendored `journal-log-writer` has no consumer.
- `netflow-plugin` already uses the published SDK at `0.8.1`; Go consumers use
  `v0.10.0`.
- The SDK publishes `journal-core`, `journal-index`, `journal-engine`,
  `journal-registry` with the types `journal-function` uses (`FileIndex`,
  `Direction`, `Histogram`, `Table`). `journal-function` itself is
  Netdata-side plugin glue and stays in Netdata.

Unknowns:

- API drift between vendored and SDK crates for `journal-function`'s use.
- Whether `netflow-plugin` needs code changes moving from `0.8.1`.

### Acceptance Criteria

- `systemd-journal.plugin` keeps libsystemd in builds where libsystemd is
  available, and uses the SDK's published C reader crate (SOW-0162) instead of
  vendored `jf` in builds without it (today Docker and static builds,
  `packaging/docker/Dockerfile:42`,
  `packaging/makeself/jobs/70-netdata-git.install.sh:40`).
- Feature parity between the libsystemd and SDK reader paths is verified on
  the same journal inputs (queries, facets, histograms, boots, cursors and
  directory reading), with every difference fixed or recorded with a user
  decision.
- A mixed-format journal directory (regular, compact and compressed files,
  carried over from superseded SOW-0049) is read correctly through the SDK
  reader path, including ordered directory reading.
- `otel-legacy-logs` / `journal-function` and `ng-ingest` use SDK crates.
- `netflow-plugin` and Go consumers move to the new release.
- Netdata's `src/crates/{jf,journal-common,journal-core,journal-index,journal-engine,journal-registry,journal-log-writer}`
  are deleted, and a fresh search proves no references remain.
- Netdata builds, tests and Docker/static packaging pass, including the
  internal-reader path.

## Analysis

Sources checked:

- Netdata master as cited above; superseded SOW-0049 and SOW-0050.

Current state:

- See Facts.

Risks:

- Behavior change in the system logs viewer for Docker/static users.
- Cross-repository change; Netdata review and CI apply.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Netdata ships three journal implementations; the vendored ones drift from
  the SDK and miss its fixes.

Evidence reviewed:

- Listed under Facts.

Affected contracts and surfaces:

- Netdata Rust workspace, CMake and packaging, `systemd-journal.plugin`,
  `otel-plugin`, `netflow-plugin`, Go plugins.

Existing patterns to reuse:

- `netflow-plugin`'s published-crate dependency (`src/crates/Cargo.toml:176-182`).

Risk and blast radius:

- See Risks.

Sensitive data handling plan:

- No sensitive data expected.

Implementation plan:

1. To be written at activation.

Validation plan:

- To be written at activation.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected.
- Specs: Netdata integration status in product scope.
- End-user/operator docs: integration docs if they mention vendored code.
- End-user/operator skills: none exist.
- SOW lifecycle: supersedes SOW-0049/0050; unblocks SOW-0066 Netdata gate.
- SOW-status.md: updated on activation and close.

Open-source reference evidence:

- `netdata/netdata @ eaeb95f5c4`: the repository-relative `path:line` citations under Facts.

Open decisions:

1. Netdata repository write authorization (required before any Netdata change).

## Implications And Decisions

- 2026-10-31: user chose one SOW for adoption plus removal, a published C
  reader crate (SOW-0162), and porting the legacy logs viewer to the SDK.
- 2026-11-03 (user): keep libsystemd where it is available; this may be a
  transition period. The SDK replaces only the vendored reader. Feature parity
  must be verified. Rewriting `systemd-journal.plugin` in Rust, so it uses the
  SDK without FFI, is the right long-term solution but not now; it is tracked
  by pending SOW-0170.
- 2026-11-03 (user): carry SOW-0049's mixed-format directory gate forward as
  an acceptance criterion.

## Plan

1. Activate after SOW-0163; refresh the Netdata inventory first.

## Implementation And Review Plan

Implementation:

- Project manager implements once the Netdata target is authorized.

Reviewers:

- Whole-SOW self-review after local validation; tests are the gate. External
  reviewers are not used.

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

- Record failures here; stop for the user on design changes.

## Execution Log

### 2026-10-31

- Created from the SOW backlog review; SOW-0049 and SOW-0050 closed as superseded.

## Validation

Acceptance criteria evidence:

- Pending.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending.

Reviewer findings:

- Pending.

Same-failure scan:

- Pending.

Sensitive data gate:

- Pending.

Artifact maintenance gate:

- Pending.

Specs update:

- Pending.

Project skills update:

- Pending.

End-user/operator docs update:

- Pending.

End-user/operator skills update:

- Pending.

Lessons:

- Pending.

Follow-up mapping:

- Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
