# SOW-0026 - Netdata SDK Integration

## Status

Status: open

Sub-state: Created from user request on 2026-05-26. This is the last pending integration SOW. Implementation is blocked until SOW-0009 proves acceptable performance or the user explicitly accepts a staged exception, and until the user authorizes the exact Netdata repository work target because this SDK repository forbids changes outside this repository by default.

## Requirements

### Purpose

Integrate this SDK into Netdata's journal-producing and journal-reading plugins so Netdata uses one maintained SDK implementation instead of plugin-local or vendored journal logic. Writers must default to compact journal format after integration, while readers must continue reading existing regular, compressed, compact, sealed, and mixed historical directories.

### User Request

Create an SOW for integrating the SDK into Netdata `netflow.plugin` reader/writer paths, `otel.plugin` writer path, `otel-signal-viewer.plugin` reader path, and `systemd-journal.plugin` reader path for systems compiled without libsystemd. The writers should default to the new compact format. After this SOW, all Netdata code should use the SDK.

### Assistant Understanding

Facts:

- Current SDK repository rules forbid writes outside this repository unless the user explicitly changes the boundary for a task.
- The Netdata source evidence used by SOW-0023 was inspected at `ktsaou/netdata @ 00305266364e`.
- NetFlow currently uses `journal-log-writer` for raw and tier writers and uses journal reader/index/query crates for replay/query paths.
- OTEL logs currently use `journal-log-writer` for writer paths.
- Netdata log-viewer crates include `otel-signal-viewer-plugin` and journal-facing reader/query crates.
- The user requires writers to default to compact format for the Netdata integration.
- The user clarified on 2026-05-26 that actual Netdata integration should happen last because the SDK does not yet perform well enough to replace Netdata's faster older vendored libraries. Remaining compatibility feature/gap SOWs should complete first, then SOW-0009 performance work, then this SOW.

Inferences:

- This integration depends on SOW-0023 API stabilization for high-level writer behavior.
- Netdata reader integration depends on SOW-0027 reader API and `jf` facade parity, because Netdata's `jf` crate provides the existing libsystemd-like compatibility layer used by static builds.
- Compact-by-default Netdata writers depend on SOW-0024 mixed-directory reader coverage so existing regular files and new compact files can coexist safely.
- Restart disk-budget behavior depends on SOW-0025 retention-on-open if Netdata configuration changes should apply immediately after plugin restart.
- Production replacement depends on SOW-0009 benchmark/profile/optimize evidence. On 2026-05-26 the user reported the Go SDK writer at about 5k logs/s in the SNMP traps ingestion worker, compared with about 25k logs/s for Netdata NetFlow with the vendored Rust implementation. This is user-reported until reproduced by SOW-0009, but it is large enough to make performance a production gate. This SOW must remain after SOW-0009 unless the user explicitly accepts a staged exception.
- The integration should not change user configuration paths or lose existing journals.

Unknowns:

- The exact dependency strategy for Netdata integration: git dependency, versioned tag, workspace replacement, vendored source removal, or staged compatibility shim.
- The exact implementation language and boundary for `systemd-journal.plugin` reader replacement on systems without libsystemd.
- Whether integration should happen in the Netdata repository directly or through a branch/worktree approved by the user.

### Acceptance Criteria

- SOW-0009 shows acceptable writer/reader performance for the relevant Netdata hot paths before implementation starts, or the user explicitly accepts a staged performance exception.
- The user authorizes the exact Netdata repository path/branch/worktree before implementation starts, or the SOW is split into SDK-side packaging work and a separate Netdata-side integration SOW.
- NetFlow writer paths use this SDK high-level writer API, default to compact output, preserve existing effective directories and machine-id layout, preserve lifecycle events needed by facet side artifacts, preserve sync cadence, and continue reading old files.
- NetFlow reader/query/rebuild/facet paths use this SDK reader/index/query APIs or an approved SDK facade, and no NetFlow path keeps a plugin-local journal parser when an SDK API exists.
- OTEL logs writer paths use this SDK high-level writer API, default to compact output, preserve current directory/config behavior, source realtime handling, batching, and sync semantics.
- `otel-signal-viewer.plugin` reader paths use this SDK reader/query APIs or an approved SDK facade.
- `systemd-journal.plugin` uses this SDK reader path when compiled without libsystemd, with no runtime link to libsystemd in that mode.
- Existing Netdata users keep existing journals and configuration paths. The integration must read pre-existing regular-format files and new compact-format files in the same directories.
- Netdata writers default to compact journal format, while the SDK still allows explicit regular output if a Netdata compatibility or rollback option is required.
- Netdata reader integrations start only after SOW-0027 defines and validates the accepted `jf`/libsystemd-compatible reader facade and unified idiomatic reader API across Rust, Go, Node.js, and Python.
- Production replacement of NetFlow/OTEL vendored journal logic and no-libsystemd `systemd-journal.plugin` reader readiness starts only after SOW-0009 shows acceptable writer/reader performance for the relevant Netdata hot paths, or after the user explicitly accepts a staged performance exception.
- Integration tests cover synthetic NetFlow-shaped and OTEL-shaped files, mixed old/new directories, compact writer output, reader fallback without libsystemd, and stock `journalctl` readback of files written by Netdata through the SDK.
- No durable artifact records secrets, SNMP community strings, customer logs, private endpoints, bearer tokens, or personal data.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0023-20260525-netdata-ingestion-writer-api.md`
- `SOW-status.md`
- `ktsaou/netdata @ 00305266364e`
  - `src/crates/netflow-plugin/src/ingest/service/init.rs`
  - `src/crates/netflow-plugin/src/ingest/service/runtime.rs`
  - `src/crates/netflow-plugin/src/query/scan/direct.rs`
  - `src/crates/netdata-otel/otel-plugin/src/logs_service.rs`
  - `src/crates/Cargo.toml`

Current state:

- NetFlow writer construction is already identified in SOW-0023 as direct `Log::new` usage for raw and tier writers.
- NetFlow runtime reads the active file path after append and controls sync cadence.
- NetFlow query/rebuild paths scan journal files directly through journal crates.
- OTEL logs construct a high-level writer and sync per exported batch.
- The SDK already has compact writer support and high-level compact selection, but the Netdata integration must make compact the Netdata writer default.
- The SDK currently has partial `SdJournal` facades, but SOW-0027 now owns the `jf` parity and reader API analysis required before replacing Netdata reader consumers.

Risks:

- Changing writer defaults to compact can strand old readers if mixed-directory support is incomplete.
- Replacing Netdata reader paths can regress query latency, facet rebuilding, or memory use.
- Integrating by a moving git URL without API stabilization can break Netdata builds.
- `systemd-journal.plugin` no-libsystemd mode has platform and packaging risks that must be tested in build configurations without libsystemd.
- Removing vendored/local logic too early can leave gaps if SDK APIs do not expose a needed reader/query/index behavior.

## Pre-Implementation Gate

Status: blocked until SOW-0009 performance gate and Netdata repository decision

Problem / root-cause model:

- Netdata currently has direct journal writer/reader consumers. The SDK is being built to become the shared implementation. Integration cannot safely start from this repository until the user approves the Netdata repository write target and dependency strategy.

Evidence reviewed:

- SOW-0023 integration analysis identified NetFlow and OTEL writer API consumers.
- Read-only Netdata source scan at `ktsaou/netdata @ 00305266364e` confirms journal crates and plugin integration points remain present.
- Current project AGENTS rules forbid writes outside this repository.

Affected contracts and surfaces:

- Netdata Rust workspace dependencies and build graph.
- NetFlow writer, replay, query, and facet paths.
- OTEL logs writer path.
- OTEL signal viewer reader path.
- systemd journal plugin reader path in no-libsystemd builds.
- Netdata packaging/build options involving libsystemd.
- SDK public API, versioning, and release/tag strategy.
- SDK `jf`/libsystemd-compatible reader facade from SOW-0027.

Existing patterns to reuse:

- Existing Netdata `journal-log-writer::Log` integration shape for NetFlow and OTEL.
- SDK high-level `Log` APIs from SOW-0023.
- SDK compact writer option from SOW-0018.
- SDK mixed-directory reader work from SOW-0024 once completed.
- SDK reader API and `jf` facade parity from SOW-0027 once completed.
- Netdata lifecycle observer pattern for NetFlow facet side artifacts.

Risk and blast radius:

- High. This touches Netdata production ingestion and query plugins, storage format defaults, build dependencies, and no-libsystemd fallback behavior.
- Data loss risk exists if storage paths, retention, active-file protection, or migration behavior changes.
- Performance risk exists for NetFlow query/rebuild and ingestion hot paths.
- The current user-reported Go writer throughput gap is severe enough that correctness-only integration would not be fit for Netdata production replacement.

Sensitive data handling plan:

- Use synthetic NetFlow, OTEL, and systemd journal fixtures. Do not copy production Netdata caches, customer logs, private endpoints, SNMP communities, tokens, account IDs, or personal data into this repository or Netdata commits.

Implementation plan:

1. Obtain user decision on Netdata repository target and dependency strategy.
2. Inventory every Netdata journal reader and writer consumer at the chosen Netdata commit.
3. Stabilize SDK version/tag/API used by Netdata and document the integration contract.
4. Replace NetFlow writer paths with SDK compact-default high-level writer while preserving directory, lifecycle, sync, timestamp, and retention behavior.
5. Replace NetFlow reader/query/rebuild paths with SDK reader/query/index APIs or record missing SDK API gaps and split follow-up SOWs.
6. Replace OTEL logs writer with SDK compact-default high-level writer.
7. Replace OTEL signal viewer reader paths with SDK reader/query APIs.
8. Add systemd-journal plugin no-libsystemd reader integration using SDK reader APIs and build tests without libsystemd.
9. Remove or isolate obsolete Netdata-local/vendored journal logic after all consumers pass tests.

Validation plan:

- Netdata build/test matrix for affected plugins.
- SDK conformance and interoperability suites remain passing.
- NetFlow synthetic ingestion, restart, replay, query, and facet rebuild tests against mixed regular/compact directories.
- OTEL synthetic logs writer tests with stock `journalctl` readback.
- OTEL signal viewer reader tests against mixed-format fixtures.
- systemd-journal plugin build and reader tests with libsystemd disabled.
- SOW-0009 performance gate for NetFlow ingestion/query paths, OTEL writer paths, SNMP traps writer paths, and no-libsystemd `systemd-journal.plugin` reader paths before production replacement.
- External reviewer passes focused on data migration, build/dependency, and performance risks.

Artifact impact plan:

- AGENTS.md: may need a temporary repository-boundary update only if the user authorizes Netdata-side edits from this project thread.
- Runtime project skills: update if Netdata integration becomes a repeatable SDK release workflow.
- Specs: update product scope with Netdata integration status and compact-default Netdata writer policy after completion.
- End-user/operator docs: update SDK README/API docs and any Netdata docs/config help affected by compact default or no-libsystemd reader mode.
- End-user/operator skills: update only if Netdata operator skills consume changed docs.
- SOW lifecycle: pending until the user authorizes repository target and prerequisites are complete.
- SOW-status.md: updated when created, activated, and closed.

Open-source reference evidence:

- `ktsaou/netdata @ 00305266364e`
  - `src/crates/netflow-plugin/src/ingest/service/init.rs`
  - `src/crates/netflow-plugin/src/ingest/service/runtime.rs`
  - `src/crates/netflow-plugin/src/query/scan/direct.rs`
  - `src/crates/netdata-otel/otel-plugin/src/logs_service.rs`
  - `src/crates/Cargo.toml`

Open decisions:

1. Netdata work target
   - Option A: implement in a user-approved Netdata repository branch/worktree.
   - Option B: keep this SOW SDK-side only and create a separate Netdata-repo SOW later.
   - Recommendation: A, after SOW-0023/SOW-0024/SOW-0025 prerequisites are settled, because real integration must change Netdata code.
2. Dependency strategy
   - Option A: Netdata consumes this SDK by versioned git tag/URL.
   - Option B: Netdata vendors a snapshot.
   - Option C: staged compatibility shim first, direct SDK dependency later.
   - Recommendation: A, if the public API is stable and CI pins a tag/commit; it matches the user's stated plan for external consumption and avoids local drift.

## Implications And Decisions

1. Repository boundary
   - Decision: implementation is blocked until the user authorizes the Netdata repository work target.
   - Reason: current project instructions forbid changes outside this repository.
   - Risk: starting implementation without that decision would violate the repository boundary and could mix SDK work with Netdata integration state unsafely.

2. Compact default
   - Decision: Netdata writers must default to compact journal format in this integration.
   - Reason: the user explicitly required compact default for Netdata writers.
   - Risk: mixed old/new directory reader support must be validated first so old regular files remain readable.

## Plan

1. Resolve Netdata repository target and dependency strategy.
2. Wait for SOW-0027 to define and validate the reader API and `jf` facade contract.
3. Inventory all Netdata journal consumers at the implementation commit.
4. Integrate SDK writers with compact default for NetFlow and OTEL.
5. Integrate SDK readers for NetFlow, OTEL signal viewer, and no-libsystemd systemd journal plugin mode through the SOW-0027 reader contract.
6. Remove obsolete Netdata-local journal logic only after replacement coverage passes.
7. Run Netdata and SDK validation, review, and commit.

## Delegation Plan

Implementer:

- Current routing is local implementation by the project manager unless the user explicitly re-enables external implementers.

Reviewers:

- Use read-only reviewers from the approved pool: `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`. Skip `llm-netdata-cloud/mimo-v2.5-pro` while unavailable.

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

- Record SDK API gaps, Netdata build failures, reviewer failures, benchmark regressions, and repository-boundary blockers in this SOW before changing scope.

## Execution Log

### 2026-05-26

- Created SOW from user request while SOW-0023 review was running.
- Performed read-only Netdata commit check: `ktsaou/netdata @ 00305266364e`.
- Updated after user clarified that Netdata reader integration must account for the existing `jf` libsystemd-like compatibility layer. SOW-0027 now owns reader API analysis and `jf` facade parity before this integration starts.

## Validation

Pending activation and implementation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
