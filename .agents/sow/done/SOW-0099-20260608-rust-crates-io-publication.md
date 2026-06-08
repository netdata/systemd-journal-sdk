# SOW-0099 - Rust crates.io Publication

## Status

Status: completed

Sub-state: published to crates.io; release tags pending commit/tag push.

## Requirements

### Purpose

Publish the Rust SDK in a form Netdata and other Rust consumers can depend on
without vendoring source code, while preserving the current SDK architecture and
avoiding confusing ownership of the already-taken `journal` crate name.

### User Request

The user asked whether the Rust SDK can be renamed to `systemd-journal-sdk` and
published to crates.io, then approved the recommended implementation.

### Assistant Understanding

Facts:

- The public Rust SDK package is currently named `journal`.
- `journal` already exists on crates.io and is not owned by this project.
- `systemd-journal-sdk` and the planned `systemd-journal-sdk-*` internal
  package names were not found in the current crates.io index on 2026-06-08.
- The public SDK depends on internal workspace crates through path/workspace
  dependencies that are not publishable as-is.
- Netdata Rust consumers use internal journal crates today, not only the public
  SDK facade.

Inferences:

- Publishing only one renamed public crate would not be enough for current
  Netdata Rust integration unless the public crate is refactored to hide every
  required internal API first.
- Publishing the internal crates under project-prefixed package names is the
  least disruptive path for `v0.6.0`.

Unknowns:

- None remaining for this SOW. Crates.io accepted every planned package name
  and version.

### Acceptance Criteria

- Rust publishable package names are project-prefixed:
  `systemd-journal-sdk`, `systemd-journal-sdk-common`,
  `systemd-journal-sdk-core`, `systemd-journal-sdk-registry`,
  `systemd-journal-sdk-log-writer`, `systemd-journal-sdk-index`, and
  `systemd-journal-sdk-engine`.
- Existing Rust crate/import names remain compatible through dependency aliases
  and package metadata.
- Publishable internal dependencies have explicit package names and version
  requirements accepted by `cargo publish --dry-run`.
- Non-publishable test commands, adapters, legacy FFI/staticlib crates, and CLI
  packages are explicitly marked `publish = false`.
- Rust tests and publish dry-runs pass.
- A temporary consumer can depend on the registry-shaped public crate identity.
- Crates are published to crates.io if credentials are available; otherwise the
  SOW records the exact blocker and leaves publish-ready dry-run evidence.
- Release tags follow the repository release-tagging contract if publication
  succeeds.

## Analysis

Sources checked:

- `rust/Cargo.toml`
- `rust/src/journal/Cargo.toml`
- `rust/src/crates/journal-common/Cargo.toml`
- `rust/src/crates/journal-core/Cargo.toml`
- `rust/src/crates/journal-registry/Cargo.toml`
- `rust/src/crates/journal-log-writer/Cargo.toml`
- `rust/src/crates/journal-index/Cargo.toml`
- `rust/src/crates/journal-engine/Cargo.toml`
- `~/src/netdata-ktsaou.git/src/crates/Cargo.toml` read-only, for current
  Netdata dependency shape.
- `cargo info` against crates.io for package-name availability.
- `cargo help package` and `cargo help publish` for publish rules.

Current state:

- `cargo publish --manifest-path rust/src/journal/Cargo.toml --dry-run
  --allow-dirty` fails because `journal@0.1.0` already exists and because
  internal dependencies have no publishable version requirement.
- A git dependency on package `journal` from tag `v0.5.1` resolves today, but
  that does not solve crates.io publication or package-name ownership.

Risks:

- Publishing package names is irreversible in normal crates.io workflow.
- Renaming only package names while preserving dependency aliases must be
  tested carefully because it affects every internal Rust dependency edge.
- Publishing internal crates exposes a larger public surface than a single
  facade crate, but it matches current Netdata integration needs and avoids a
  risky API consolidation before publication.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The current Rust package identity was designed for a workspace and git
  dependency use, not registry publication. crates.io requires unique package
  names and versioned dependencies; the current public crate name is already
  occupied and internal dependencies are path-only.

Evidence reviewed:

- `rust/src/journal/Cargo.toml:2` names the public package `journal`.
- `rust/src/journal/Cargo.toml:11` through `rust/src/journal/Cargo.toml:14`
  use internal workspace dependencies.
- `rust/Cargo.toml:153` through `rust/Cargo.toml:158` define internal crates as
  path-only workspace dependencies.
- crates.io index lookup did not find the planned `systemd-journal-sdk*`
  package names on 2026-06-08.
- `cargo publish --dry-run` reported that path dependencies must also have
  version requirements when publishing.

Affected contracts and surfaces:

- Rust package names and versions.
- Rust public dependency snippets in README/API docs.
- Internal crate dependency aliases.
- crates.io release order.
- Root release tags and Go submodule tags if a root `v0.6.0` release is made.

Existing patterns to reuse:

- Existing Rust workspace package metadata and dependency aliases.
- Existing project release-tagging process for root and Go submodule tags.
- Existing SOW validation and reviewer process.

Risk and blast radius:

- Medium-high. Registry package identity is public and effectively permanent,
  but the source-code change should be mostly manifest/package metadata.
- Compatibility risk is bounded by `cargo test` and a consumer smoke test.
- Netdata integration risk is reduced by keeping the `journal-*` dependency
  aliases in the workspace and publishing project-prefixed package names.

Sensitive data handling plan:

- Do not record crates.io tokens, local credential paths, or token presence
  details beyond "available" or "not available".
- Durable artifacts contain only package names, versions, commands, and
  sanitized publish results.

Implementation plan:

1. Rename publishable Rust packages to project-prefixed crates.io names while
   preserving dependency aliases.
2. Add explicit versions and `package = ...` metadata to internal workspace
   dependency edges.
3. Add package metadata required or recommended for crates.io publication.
4. Mark non-publishable workspace crates `publish = false`.
5. Update README/docs with crates.io dependency examples.
6. Validate tests, package dry-runs, and consumer dependency resolution.
7. Run whole-SOW read-only reviewers.
8. Publish crates and release tags if credentials are available.

Validation plan:

- `cargo test` for affected Rust workspace.
- `cargo publish --dry-run` for every publishable package in dependency order.
- Temporary consumer smoke under `/tmp`.
- `git diff --check`.
- `.agents/sow/audit.sh`.
- Read-only reviewer pool after local validation.

Artifact impact plan:

- AGENTS.md: no project-wide workflow change expected.
- Runtime project skills: release-tagging skill may need an update only if the
  Rust publish procedure becomes durable operator workflow.
- Specs: update product scope with Rust crates.io package names and package
  split.
- End-user/operator docs: update README and Rust README with crates.io usage.
- End-user/operator skills: no output/reference skills expected.
- SOW lifecycle: close this SOW with the implementation and release evidence.
- SOW-status.md: update current and final status.

Open-source reference evidence:

- Cargo documentation was checked through local `cargo help package` and
  `cargo help publish`; no external source checkout was needed.

Open decisions:

- 2026-06-08: The user approved the recommended path to publish under
  `systemd-journal-sdk` with project-prefixed internal crates. Version target
  is `0.6.0` unless validation exposes a reason to stop.

## Implications And Decisions

1. Package-name strategy
   - Decision: publish project-prefixed package names instead of trying to use
     `journal`.
   - Implication: crates.io ownership is clear and does not collide with the
     existing `journal` crate.

2. Internal crate publication
   - Decision: publish the internal Rust crates needed by current SDK and
     Netdata consumers.
   - Implication: the first registry release exposes a larger crate set, but it
     avoids a risky consolidation refactor before Netdata integration.

3. Version strategy
   - Decision: use `0.6.0`.
   - Implication: Rust package identity changes are represented as a new minor
     pre-1.0 release; root and Go tags should be considered together by the
     release-tagging contract.

## Plan

1. Update manifests and package metadata.
2. Update docs/specs/status.
3. Run local validation and dry-runs.
4. Run read-only reviewer pool.
5. Publish crates if authenticated.
6. Commit, push, and tag according to the release result.

## Delegation Plan

Implementer:

- Local implementation by the project manager, matching current routing.

Reviewers:

- Read-only reviewer pool after local validation:
  `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/kimi-k2.6`,
  `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`,
  `llm-netdata-cloud/minimax-m3-coder`, and
  `llm-netdata-cloud/deepseek-v4-pro`.

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

- If publish credentials are not available, stop after successful dry-run,
  commit the publish-ready changes, and report the exact remaining user action.
- If any package name is unavailable at publish time, stop before publishing
  dependent packages and record a naming decision requirement.
- If reviewers find blockers, repair and rerun the same whole-SOW review scope.

## Execution Log

### 2026-06-08

- Created the SOW and recorded the user-approved packaging strategy.
- Renamed publishable Rust package identities to project-prefixed crates.io
  names while preserving source crate names through `[lib]` and dependency
  aliases.
- Marked non-publishable Rust tools, adapters, legacy FFI/staticlib crates,
  and test commands with `publish = false`.
- Updated Rust package documentation, product scope, release-tagging workflow,
  and SOW status ledgers.
- Ran local validation and first reviewer cycle.
- Reran local validation after SOW evidence cleanup:
  `cargo check --manifest-path rust/Cargo.toml --workspace`, `cargo test
  --manifest-path rust/Cargo.toml --workspace`, first-crate publish dry-run,
  `git diff --check`, and `.agents/sow/audit.sh` all passed.
- Ran the whole-SOW reviewer cycle again against the updated SOW and changed
  surface.
- Checked crates.io credential state without recording secrets:
  `CARGO_REGISTRY_TOKEN` is not set, `~/.cargo/credentials.toml` is absent, and
  `~/.cargo/config.toml` is absent in this environment.
- Rechecked planned package-name availability; all planned names still returned
  not found in the crates.io index.
- Paused before publishing because Cargo has no registry credential configured.
- Resumed after the user configured Cargo registry credentials locally.
- Published crates in dependency order:
  `systemd-journal-sdk-common`, `systemd-journal-sdk-registry`,
  `systemd-journal-sdk-core`, `systemd-journal-sdk-log-writer`,
  `systemd-journal-sdk-index`, `systemd-journal-sdk-engine`, and
  `systemd-journal-sdk`.
- Crates.io rate-limited new crate publication twice. The release waited until
  the registry-provided retry time and retried the affected package without
  changing package contents.
- Verified all seven packages with `cargo info`.

## Validation

Acceptance criteria evidence:

- Package names and library import names were verified with `cargo metadata`:
  - `systemd-journal-sdk` exposes Rust lib crate `journal`.
  - `systemd-journal-sdk-common` exposes Rust lib crate `journal_common`.
  - `systemd-journal-sdk-core` exposes Rust lib crate `journal_core`.
  - `systemd-journal-sdk-registry` exposes Rust lib crate
    `journal_registry`.
  - `systemd-journal-sdk-log-writer` exposes Rust lib crate
    `journal_log_writer`.
  - `systemd-journal-sdk-index` exposes Rust lib crate `journal_index`.
  - `systemd-journal-sdk-engine` exposes Rust lib crate `journal_engine`.
- Publishable internal dependency edges in `rust/Cargo.toml` have explicit
  `package = ...`, `version = "0.6.0"`, and `path = ...` metadata.
- `rg` over Rust manifests confirmed non-publishable command, adapter, internal
  test, and legacy `jf` crates are marked `publish = false`.
- crates.io lookup on 2026-06-08 did not find the planned package names:
  `systemd-journal-sdk`, `systemd-journal-sdk-common`,
  `systemd-journal-sdk-core`, `systemd-journal-sdk-registry`,
  `systemd-journal-sdk-log-writer`, `systemd-journal-sdk-index`, and
  `systemd-journal-sdk-engine`. Publish-time registry checks remain
  authoritative.
- `cargo info` confirms all planned packages now exist at version `0.6.0`.

Tests or equivalent validation:

- `cargo check --manifest-path rust/Cargo.toml --workspace`: PASS.
- `cargo test --manifest-path rust/Cargo.toml --workspace`: PASS.
- `cargo publish --manifest-path rust/src/crates/journal-common/Cargo.toml
  --dry-run --allow-dirty`: PASS.
- `cargo publish --manifest-path rust/src/crates/journal-registry/Cargo.toml
  --dry-run --allow-dirty`: blocked before publish because
  `systemd-journal-sdk-common` is not yet present in crates.io. This is Cargo's
  expected registry dependency-order behavior for dependent crates, not an
  accepted manifest defect.
- Publication credential check: blocked. No Cargo registry token or Cargo
  credential file is configured in this environment.
- Crates.io publication: PASS. All seven planned crates were accepted and are
  visible to `cargo info` at version `0.6.0`.
- `git diff --check`: PASS.
- `.agents/sow/audit.sh`: PASS.

Real-use evidence:

- A temporary consumer under `/tmp/systemd-journal-sdk-consumer` successfully
  used:

  ```toml
  journal = { package = "systemd-journal-sdk", path = "$REPO/rust/src/journal" }
  ```

  and compiled/ran code using `journal::ReaderOptions` and
  `journal::FieldNamePolicy`.

Reviewer findings:

- First reviewer cycle, before this validation section was completed:
  - `llm-netdata-cloud/glm-5.1`: PRODUCTION GRADE: YES. Low-severity
    metadata suggestions only; package categories were added.
  - `llm-netdata-cloud/mimo-v2.5-pro`: PRODUCTION GRADE: YES. Low-severity
    README consistency note for the registry helper crate; accepted as
    non-blocking because the crate has a package-local README and the public
    crates point to the root README.
  - `llm-netdata-cloud/minimax-m3-coder`: PRODUCTION GRADE: NO. Blocking
    finding was that this SOW's Validation section still said `Pending` despite
    implementation validation already being run. This update fixes that process
    blocker. Low-severity package metadata suggestions were dispositioned:
    categories were added; `authors` was not added because Cargo does not
    require it and public ownership/contact metadata is a separate release
    policy decision.
  - `llm-netdata-cloud/kimi-k2.6`: unavailable due quota during this cycle.
  - `llm-netdata-cloud/qwen3.6-plus`: stopped after repeated read loop without
    a final vote; it will be rerun after this SOW evidence update.
  - `llm-netdata-cloud/deepseek-v4-pro`: final vote was not captured; it will
    be rerun after this SOW evidence update.
- Second reviewer cycle, after SOW evidence update:
  - `llm-netdata-cloud/glm-5.1`: PRODUCTION GRADE: YES. Low-severity findings
    only: `journal_reader_ffi` uses explicit `edition = "2021"` in a
    non-publishable legacy crate; `journal-registry` intentionally uses a
    package-local README; `authors` metadata is absent by release policy.
  - `llm-netdata-cloud/mimo-v2.5-pro`: PRODUCTION GRADE: YES. Low-severity
    findings only: `journal-registry` README differs from siblings;
    `journal-engine` has an explicit dev-dependency that is also transitively
    available; qwen/deepseek first-cycle capture gaps were documented.
  - `llm-netdata-cloud/minimax-m3-coder`: PRODUCTION GRADE: YES. Low-severity
    findings only: release-tagging examples still use generic `v0.2.0`;
    `zerocopy = "0.9.0-alpha.0"` is pre-existing and crates.io-legal; no
    workspace alias exists for the public SDK crate, matching the existing
    manifest shape.
  - `llm-netdata-cloud/deepseek-v4-pro`: PRODUCTION GRADE: YES. Low-severity
    findings only: pre-existing alpha `zerocopy`, GPL license policy, and
    `journal-registry` package-local README.
  - `llm-netdata-cloud/kimi-k2.6`: unavailable due quota in both cycles.
  - `llm-netdata-cloud/qwen3.6-plus`: rerun performed useful manifest and
    `cargo metadata` checks and found no visible blocker, but stalled before a
    final vote. The exact reviewer process was terminated after no output for
    multiple polling intervals. This is recorded as inconclusive rather than a
    production-grade vote.
- Reviewer dispositions:
  - No reviewer found a publication blocker.
  - `journal_reader_ffi` edition style is out of scope because the crate is
    `publish = false` and the value is functionally equivalent to its
    sub-workspace edition today.
  - `journal-registry` keeps its package-local README because that crate has a
    focused registry-specific README and Cargo validated the path.
  - `authors` remains absent because Cargo does not require it and public
    owner/contact metadata needs an explicit release policy decision.
  - `zerocopy = "0.9.0-alpha.0"` is pre-existing dependency debt and not a
    crates.io publication blocker; no release-blocking evidence was found.
  - Generic `v0.2.0` release-tagging examples remain examples. The SOW and
    commands for this release use `0.6.0`.

Same-failure scan:

- `rg` over Rust manifests was used to check package aliases, `publish = false`
  coverage, and stale direct `journal-*` workspace dependency shapes.
- A reviewer concern that publishable crates might depend on the legacy `jf`
  `error` crate was checked and rejected: publishable crates use their own
  package-local `error.rs` modules, and the legacy `jf` crates are marked
  `publish = false`.

Sensitive data gate:

- No registry tokens, credential paths, or credential values were written to
  durable artifacts. Publication credential state will be reported only as
  available or unavailable.

Artifact maintenance gate:

- AGENTS.md: no workflow or project-wide guardrail change needed.
- Runtime project skills: `.agents/skills/project-release-tagging/SKILL.md`
  updated with Rust package names and publish order.
- Specs: `.agents/sow/specs/product-scope.md` updated with the Rust registry
  package contract.
- End-user/operator docs: `README.md` and `rust/README.md` updated with Rust
  crates.io dependency examples.
- End-user/operator skills: no output/reference skill is affected.
- SOW lifecycle: `Status: completed`; this file is moved to `.agents/sow/done/`
  together with the release evidence commit.
- SOW-status.md: root and detailed SOW status ledgers updated.

Specs update:

- `.agents/sow/specs/product-scope.md` records the Rust crates.io package split
  and recommended dependency alias.

Project skills update:

- `.agents/skills/project-release-tagging/SKILL.md` records the Rust publish
  order and dry-run/publish workflow.

End-user/operator docs update:

- `README.md` and `rust/README.md` record the Rust dependency syntax and
  internal package list.

End-user/operator skills update:

- No end-user/operator skill exists for consuming the Rust SDK.

Lessons:

- Cargo package rename and Rust source crate rename are separate controls:
  `package.name` controls crates.io identity, while `[lib].name` preserves Rust
  import names.
- Dependent crates cannot fully publish-dry-run against crates.io until their
  registry dependencies exist. This affects the release sequence and must be
  handled one crate at a time.

Follow-up mapping:

- No deferred implementation item remains for this SOW. Consumer documentation
  and GitHub wiki publication are tracked separately by SOW-0100.

## Outcome

Completed: Rust SDK packages were published to crates.io at version `0.6.0`:

- `systemd-journal-sdk`
- `systemd-journal-sdk-common`
- `systemd-journal-sdk-core`
- `systemd-journal-sdk-registry`
- `systemd-journal-sdk-log-writer`
- `systemd-journal-sdk-index`
- `systemd-journal-sdk-engine`

## Lessons Extracted

- For a multi-crate workspace, the only full pre-publication dry-run available
  before the first registry upload is the first dependency crate. Later crates
  must be dry-run and published in dependency order after each upstream package
  exists in crates.io.
- Publication credentials are an operational prerequisite and must be checked
  before the irreversible publish step.
- crates.io enforces a rate limit on new crate creation. Multi-crate first
  publication may need timed waits between uploads even when all package
  manifests are valid.

## Followup

- None for this SOW.
