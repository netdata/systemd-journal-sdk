# SOW-0130 - Log-Writer Sync-On-Archive Opt-Out

## Status

Status: completed

Sub-state: created 2026-07-06, formalizing the upstream gap recorded during
the Netdata netflow tier-offload work; awaiting design decision
(opt-out flag vs async archive sync). Completed 2026-07-06 by local
project-manager implementation per user routing decision.

## Requirements

### Purpose

Stop the writer's rotation path from stalling the caller thread on the full
fsync of the just-archived file, for latency-sensitive high-throughput
writers, without weakening default durability.

### User Request

The user directed (2026-07-06, during the Netdata vendored-journal
elimination work): SDK functionality gaps found by consumer integration are
filled in the SDK via SDK SOWs. This gap was first identified during the
Netdata netflow tier-offload work (2026-06) and recorded as an upstream
`with_sync_on_archive(false)` need.

### Assistant Understanding

Facts:

- In published `systemd-journal-sdk-log-writer` 0.7.6 the rotation path
  syncs the outgoing file on the caller thread
  (`src/log/mod.rs:701` `old_file.journal_file.sync()`; also `:561`, `:595`
  on adjacent paths). No `sync_on_archive` (or equivalent) option exists in
  `log/config.rs` (verified by grep over the published 0.7.6 sources).
- Netdata evidence that this matters: during the netflow migration, fsync
  stalls — not CPU — were the measured throughput ceiling; netdata disabled
  periodic active-file sync (`live` fsync default 1024 → 0) but could not
  remove the rotate-path archive sync at the published-SDK boundary. It was
  explicitly recorded as an upstream need: `netdata/netdata` local SOW
  "netflow-tier-offload-followups" item 4 (`with_sync_on_archive(false)`
  plus a consumer-side raw-sync worker pattern).
- Any consumer whose write path runs on a latency-sensitive thread inherits
  the stall at every rotation (file-size-proportional fsync).

Inferences:

- Two viable shapes: (a) `Config::with_sync_on_archive(false)` — consumer
  opts out and owns durability of archived files (may sync on its own
  worker); (b) SDK-internal async archive sync (background completion,
  default-on) — keeps durability semantics without caller stalls but adds a
  thread/queue to the SDK. Shape (a) is smaller and matches the consumer
  pattern Netdata already built (dedicated sync worker).

Unknowns:

- Ordering guarantees consumers need between archive-sync completion and
  retention deletion of the same file (must not delete an archived file
  whose data is not yet durable if the consumer opted out and crashed —
  document the contract).

### Acceptance Criteria

- A documented mechanism exists to avoid caller-thread archive fsync
  (opt-out flag or async sync per the design decision).
- Default behavior unchanged (sync on archive remains the default).
- Contract documented: who owns durability of archived files when opted
  out; interaction with retention deletion.
- Tests: rotation under opt-out does not fsync on the caller path
  (observable via a mock/counter), files remain readable; default path
  unchanged.
- Go writer parity assessed (implement or record why Go is out of scope).

## Analysis

Sources checked:

- Published `systemd-journal-sdk-log-writer` 0.7.6 `src/log/mod.rs`,
  `src/log/config.rs`.
- Netdata-side measurements and follow-up records from the netflow
  migration and tier-offload work (2026-06).

Current state:

- Rotate path fsyncs the archived file synchronously on the caller thread;
  no configuration surface.

Risks:

- Opt-out shifts durability responsibility to consumers; must be explicit
  and documented, never default.
- Async variant adds SDK-internal threading — larger blast radius.

## Pre-Implementation Gate

Status: ready (user authorized local project-manager implementation on 2026-07-06)

Problem / root-cause model:

- Rotation couples archive durability to the writer's caller thread; there
  is no way to decouple them at the published-API boundary.

Evidence reviewed:

- See facts (file:line above).

Affected contracts and surfaces:

- `Config` builder API (additive), rotation path, durability documentation.

Existing patterns to reuse:

- `live_publish_every_entries` / periodic-sync configuration style in
  `log/config.rs`.

Risk and blast radius:

- Low for the flag variant; medium for the async variant.

Sensitive data handling plan:

- Synthetic fixtures only.

Implementation plan:

1. User decision on shape.
2. Implement + tests + docs.
3. Go parity assessment.

Validation plan:

- Rotation tests with sync counters; durability contract review; benchmark
  showing caller-thread stall removal.

Artifact impact plan:

- Options-Reference / writer docs updated; specs updated; SOW-status
  ledgers updated.

Open-source reference evidence:

- Netdata consumer pattern: `netdata/netdata @ 17a7eb31da`
  `src/crates/netflow-plugin/` (dedicated raw-sync worker approach built
  because this option was missing).

Open decisions:

- 2026-07-06 user routing/design decision: the project manager implements and
  orchestrates this SOW directly; no separate external implementer model is
  used. Existing SOW analysis is planning evidence and hints, not a frozen
  design.
- Implement the explicit opt-out flag first (`sync_on_archive` /
  `with_sync_on_archive(false)` equivalent) for Rust and Go. This is the
  surgical option because it preserves default durability and avoids adding an
  SDK-owned background thread/queue.
- SDK-internal async archive sync is out of scope for this SOW. SDK-managed
  durability offload would require a separate SOW and consumer evidence.

## Implications And Decisions

- 2026-07-06: created per user direction that SDK gaps found during Netdata
  integration are filled in the SDK.

## Plan

1. Resolve the shape decision.
2. Implement, test, document.

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
  (netdata-side SOW: eliminate-vendored-journal-crates), formalizing the
  upstream need recorded during netflow tier-offload work (2026-06).
- Implemented Rust `Config::with_sync_on_archive(false)` and Go
  `LogConfig.SyncOnArchive: journal.SyncOnArchive(false)`.
- Preserved default archive-file sync behavior.
- Added tests with sync counters proving default sync and opt-out skip paths,
  plus readability of archived files after opt-out.
- Documented caller durability responsibility and retention interaction.
- Second reviewer pass found that rotation/close tests did not prove the
  opt-out for Rust Drop or stale-active startup archive. Rust now has focused
  tests for best-effort Drop and strict startup archive; Go now has focused
  tests for strict startup archive with default sync and explicit opt-out.
- Release workflow skill now records that public Rust struct-field additions
  require an explicit semver/source-compatibility decision before tagging.
- Read-only Netdata impact check at `netdata/netdata @ 93d4f98c65b4` found
  existing Rust NetFlow and Go SNMP traps writer constructors continue compiling
  with the default sync behavior. Optional adoption can set
  `with_sync_on_archive(false)` or `SyncOnArchive(false)` where Netdata already
  owns archived-file durability.

## Validation

Acceptance criteria evidence:

- Rust archive sync now goes through `sync_archive_journal_file()` and respects
  `Config::sync_on_archive` during rotation, explicit close, drop, and
  stale-active startup archive:
  `rust/src/crates/journal-log-writer/src/log/config.rs`,
  `rust/src/crates/journal-log-writer/src/log/mod.rs`,
  `rust/src/crates/journal-log-writer/src/log/startup.rs`.
- Go high-level `Log` respects `LogConfig.SyncOnArchive` for rotation, close,
  and stale-active archive while direct `Writer.ArchiveTo()` keeps default sync:
  `go/journal/log.go`, `go/journal/writer.go`.
- Docs/spec record that opt-out shifts archived-file durability to the caller
  before side-index reliance or retention deletion.

Tests or equivalent validation:

- `cargo test --manifest-path rust/Cargo.toml --workspace` - passed.
- `go test ./...` from `go/` - passed.
- `python3 tests/docs/check_wiki_docs.py` - passed.
- `python3 tests/docs/verify_examples.py` - passed 31/31 examples.
- Focused repair tests passed:
  `sync_on_archive_false_skips_drop_archive_sync`,
  `strict_startup_sync_on_archive_policy_applies_to_online_chain_active`, and
  `TestLogSyncOnArchivePolicyAppliesToStrictStartupArchive`.

Real-use evidence:

- Tests count caller-path archive sync invocations and reopen generated
  archived files after opt-out. No production files were used.

Reviewer findings:

- 2026-07-06: read-only reviewers were run with the SOW filename and complete
  changed surface. Reviewers found no SOW-0130 blocker. Non-blocking notes were
  recorded: direct Go `Writer.ArchiveTo()` intentionally keeps default sync
  behavior, and a pre-existing Go archive failure rollback edge case is outside
  this opt-out SOW because it was not introduced by the sync flag. Reviewers
  also found that `docs/Options-Reference.md` needed the new durability option;
  it is now updated.
- 2026-07-06 repeat review: Claude voted not production-grade as closed SOWs
  because Drop/startup sync-on-archive evidence and public API contract
  recording were incomplete. GLM timed out before a final verdict after
  partially reproducing real concerns. Minimax, Deepseek, Kimi, and Qwen
  returned production-grade or production-grade with non-blocking notes. The
  missing sync path tests and release-skill semver reminder are now in place.
- 2026-07-06 second repeat review: Claude failed with an API connection reset;
  GLM and Minimax timed out without final verdicts; Deepseek, Kimi, and Qwen
  returned production-grade with non-blocking notes. Their remaining durability
  notes were explicit trade-offs of the opt-out API: callers that disable
  archive sync own archive-file durability.

Same-failure scan:

- Rust archive sync call sites in log writer were updated for rotation,
  explicit close, drop, and strict startup archive. Go high-level archive paths
  route through the same internal `archiveTo(..., syncOnArchive)` control, and
  tests now cover strict startup archive policy directly.

Sensitive data gate:

- Synthetic journal fixtures only; no sensitive data was used or recorded.

Artifact maintenance gate:

- `AGENTS.md`: no project-wide workflow change required.
- Runtime project skills:
  `.agents/skills/project-release-tagging/SKILL.md` updated so release work
  explicitly checks Rust public struct-field additions for semver/source
  compatibility impact.
- Specs: product scope updated.
- End-user/operator docs: Rust API, Go API, Writer APIs, and Go README updated.
- SOW lifecycle: completed and moved to done.
- `.agents/sow/SOW-status.md`: updated.

Specs update:

- `.agents/sow/specs/product-scope.md` records default archive sync and the
  explicit Rust/Go opt-out contract.

Project skills update:

- `.agents/skills/project-release-tagging/SKILL.md` now records the public API
  compatibility check that reviewers identified as reusable release workflow.

End-user/operator docs update:

- `docs/Rust-API.md`, `docs/Go-API.md`, `docs/Writer-APIs.md`, and
  `docs/Options-Reference.md`, and `go/README.md` document the opt-out and
  durability responsibility.

End-user/operator skills update:

- No output/operator skills are maintained for this SDK surface.

Lessons:

- Latency opt-outs must be explicit and default-off for risk. The durability
  ownership shift must be documented next to the API.

Follow-up mapping:

- SDK-internal async archive sync remains intentionally out of scope and is not
  tracked as required follow-up; it can be proposed as a separate SOW if
  consumers need SDK-owned background durability.
- Pending SOW-0066 release planning must classify Rust
  `Config::sync_on_archive` as a public struct-field addition and Go
  `LogConfig.SyncOnArchive` as an additive field. Existing Netdata writer call
  sites keep current default durability unless Netdata intentionally opts into
  caller-owned archive durability.

## Outcome

Completed. Rust and Go high-level directory writers now support explicit
archive-file sync opt-out while preserving default durability behavior.

## Lessons Extracted

The surgical opt-out is enough for consumers that already own a sync worker;
an SDK-managed background sync queue would be a separate design with a larger
operational contract.

## Followup

None.

## Regression Log

None yet.
