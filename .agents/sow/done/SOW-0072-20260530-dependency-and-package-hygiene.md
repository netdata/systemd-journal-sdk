# SOW-0072 - Dependency And Package Hygiene

## Status

Status: completed

Sub-state: completed; implementation, validation, reviewer gates, and artifact updates finished.

## Requirements

### Purpose

Keep the SDK publishable and maintainable without hidden dependency, licensing, or native-install debt. The work must preserve journal compatibility while making dependency behavior explicit and testable.

### User Request

After native portability validation, the user raised concern that the validation fix may have solved a Windows Cargo checkout path problem by copying or changing dependency code, and may also have downgraded packages. The user approved a focused cleanup pass to verify the issue and fix the debt.

### Assistant Understanding

Facts:

- SOW-0063 changed Rust `serde-api` from the `flatten-serde-json` Git dependency to an in-crate flattener.
- SOW-0063 added `node/.npmrc` with `ignore-scripts=true` to avoid `node-liblzma` native build hooks, while keeping `node-liblzma` as a dependency.
- `node-liblzma@5.0.1` has a `postinstall` native build hook and native prebuild payloads even though this SDK imports only its WASM path.
- `npm pack --dry-run --json` from the Node package currently includes `node/.local/npm-cache/...` when local cache files exist below the package root.
- The final dependency diff from the pre-validation baseline did not show package version downgrades; it showed dependency removals and a Node engine floor increase.

Inferences:

- The Windows "serde monorepo" problem was the `flatten-serde-json` dependency being fetched from the Meilisearch Git monorepo tag, not a serde upstream problem.
- Removing the Git dependency is reasonable for Windows path stability, but the replacement must have explicit parity tests and provenance.
- Relying on package-local `.npmrc` is not a sufficient published package guarantee; the Node package should not need a dependency with native install hooks for the accepted runtime path.

Unknowns:

- None blocking implementation. Dependency behavior can be verified locally through lockfile diffs, npm package metadata, package tarball smoke tests, and affected test suites.

### Acceptance Criteria

- Rust `serde-api` flattening has broader parity tests covering the removed dependency's behavior.
- Rust provenance records the Meilisearch `flatten-serde-json` source, commit, and MIT license status, or records that the local code is independent.
- Node XZ compression/decompression no longer depends on a package with native install hooks.
- Node package metadata prevents `.local`, package caches, and test scratch files from entering published tarballs.
- Node install-from-tarball smoke proves the package can install and run without native build hooks.
- Local validation passes for affected Rust and Node tests, package checks, `git diff --check`, and `.agents/sow/audit.sh`.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0063-20260530-cross-platform-portability.md`
- `.agents/sow/specs/product-scope.md`
- `rust/Cargo.toml`
- `rust/Cargo.lock`
- `rust/src/crates/journal-log-writer/src/log/mod.rs`
- `node/package.json`
- `node/package-lock.json`
- `node/.npmrc`
- `node/src/lib/xz-block.js`
- npm registry metadata for `node-liblzma@5.0.1`, `lzma-wasm@1.0.7`, `wasm-xz-sys@1.1.0`, and `compress-utils@0.7.1`
- Meilisearch local Cargo Git checkout for `flatten-serde-json` at commit `077ec2ab11bb4daefcb57f89eab9cff16e075fdc`

Current state:

- Before this SOW, `node/package-lock.json` recorded `node-liblzma@5.0.1`
  with `hasInstallScript: true`.
- `node/package.json` has no `files` whitelist.
- `node/.npmrc` sets `ignore-scripts=true`, but npm package tarball behavior must not rely on that file.
- `rust/src/crates/journal-log-writer/src/log/mod.rs` has one narrow `flatten_json_map` test.
- `PROVENANCE.md` still describes `flatten-serde-json` as an imported dependency, which is stale after SOW-0063.

Risks:

- Native build hooks in Node dependencies violate the intended no-native-runtime posture for users even if the SDK runtime imports only WASM files.
- Package tarballs can accidentally include `.local` scratch/cache artifacts without an explicit whitelist.
- Copying or adapting MIT code without attribution creates licensing and maintenance debt.
- Changing XZ compression library can alter compressed bytes; this is acceptable only if structural/read compatibility remains covered by tests.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- SOW-0063 fixed native validation failures but left two cleanup issues. Rust removed a problematic Git monorepo dependency without enough parity/provenance evidence. Node avoided a native install hook through package-local `.npmrc`, but the dependency still carries native hooks and prebuilds, and package packing lacks a whitelist.

Evidence reviewed:

- `rust/Cargo.toml`: previous Git dependency removed under SOW-0063.
- `rust/Cargo.lock`: `flatten-serde-json`, `nix`, and `cfg_aliases` were removed; no version downgrade was found in final diff.
- `node/package-lock.json`: `node-liblzma@5.0.1` has `hasInstallScript: true`.
- `node/.npmrc`: `ignore-scripts=true`.
- `npm pack --dry-run --json`: package includes `node/.local/npm-cache/...` without a whitelist.
- `meilisearch/meilisearch @ 077ec2ab11bb4daefcb57f89eab9cff16e075fdc`
  - `crates/flatten-serde-json/src/lib.rs`
  - `LICENSE`
- npm registry metadata:
  - `node-liblzma@5.0.1`: LGPL-3.0, `postinstall: node-gyp-build`, native dependencies.
  - `lzma-wasm@1.0.7`: latest from search, `(MIT OR Apache-2.0)`, no install/postinstall hook, inline WASM package.

Affected contracts and surfaces:

- Rust `journal-log-writer` optional `serde-api` behavior.
- Node XZ DATA compression/decompression.
- Node package installation, package tarball contents, dependency license/runtime posture.
- `PROVENANCE.md`, `node/README.md`, `.agents/sow/specs/product-scope.md`, and SOW status files.

Existing patterns to reuse:

- Existing Rust `serde-api` tests in `journal-log-writer`.
- Existing Node XZ tests and native-addon load check in `node/test/all.js`.
- Existing project dependency constraints in `.agents/sow/specs/product-scope.md`.
- Existing SOW validation and audit workflow.

Risk and blast radius:

- Rust risk is limited to optional `serde-api` JSON flattening before journal field generation.
- Node risk touches XZ DATA compression/decompression and package installation. It does not affect zstd/lz4 or core journal object layout.
- Package whitelist errors could omit needed CLI/runtime files, so tarball install smoke must run against the packed artifact.

Sensitive data handling plan:

- No sensitive host journal data, credentials, SNMP communities, customer data, personal data, private endpoints, or proprietary incident details are needed.
- SOW evidence records dependency names, versions, source repositories, and sanitized command outcomes only.

Implementation plan:

1. Add Rust parity coverage for the removed `flatten-serde-json` behavior and update provenance.
2. Replace Node XZ dependency path with a no-native-install package/runtime path, update package metadata, and add package whitelist.
3. Update docs/spec/status artifacts to reflect the package/runtime dependency contract.
4. Validate affected Rust/Node tests and package tarball install behavior.

Validation plan:

- `cargo test --manifest-path rust/Cargo.toml -p journal-log-writer --features serde-api`
- `npm_config_cache="$PWD/../.local/npm-cache" npm test` from `node/`
- `npm pack --dry-run --json` from `node/`, inspect for `.local`, cache, native `.node`, and expected files.
- Install the packed Node tarball into a repo-local scratch project and run XZ compress/decompress plus SDK import smoke.
- `git diff --check`
- `.agents/sow/audit.sh`
- External read-only reviewer pass against the complete SOW before completion.

Artifact impact plan:

- AGENTS.md: likely no update; existing dependency/runtime rules are sufficient.
- Runtime project skills: likely no update unless validation exposes a repeated workflow rule.
- Specs: update `.agents/sow/specs/product-scope.md` for Node XZ dependency/runtime packaging guarantee.
- End-user/operator docs: update `node/README.md`; update `PROVENANCE.md` for Rust flattener provenance.
- End-user/operator skills: no output/reference skills are affected.
- SOW lifecycle: this SOW starts in `current/` because the user approved immediate cleanup.
- SOW-status.md: update root and project status summaries.

Open-source reference evidence:

- `meilisearch/meilisearch @ 077ec2ab11bb4daefcb57f89eab9cff16e075fdc`
  - `crates/flatten-serde-json/src/lib.rs`
  - `LICENSE`
- `Wu-Yijun/lzma-wasm` package metadata from npm for `lzma-wasm@1.0.7`.
- `oorabona/node-liblzma` package metadata from npm for `node-liblzma@5.0.1`.

Open decisions:

- User approved the cleanup recommendation. Initial implementation tried
  `lzma-wasm@1.0.7` because it avoids native install hooks, but local tests
  rejected it because it emitted XZ stream flag byte `4` instead of the
  systemd-compatible `CHECK_NONE` flag `0`. Final implementation vendors only
  the `node-liblzma@5.0.1` WASM runtime files and removes the full
  `node-liblzma` dependency.

## Implications And Decisions

1. Dependency cleanup direction:
   - Selected: remove the full `node-liblzma` dependency and vendor only its
     WASM runtime files, while keeping `.npmrc` only as a local development
     guard.
   - Reasoning: users gain a dependency graph with no native install hooks, and
     the SDK preserves the existing XZ `CHECK_NONE` output required by
     compatibility tests.
   - Risk: vendored WASM files require explicit provenance and license
     handling. This SOW records that in `PROVENANCE.md` and
     `node/vendor/node-liblzma-wasm/README.md`.

2. Rust flattener handling:
   - Selected: keep the local flattener introduced by SOW-0063, but add upstream parity tests and provenance.
   - Reasoning: restoring the Git monorepo dependency would reintroduce the Windows path-depth failure. The local code is small, but must be proven and attributed.
   - Risk: future behavior drift if tests do not cover upstream edge cases; this SOW expands tests to reduce that risk.

## Plan

1. Rust parity/provenance cleanup.
2. Node dependency/package cleanup.
3. Documentation/spec/status updates.
4. Validation and reviewer pass.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current repository routing.

Reviewers:

- Run read-only external reviewers after implementation and local validation: `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/minimax-m2.7-coder`, and `llm-netdata-cloud/mimo-v2.5-pro`.

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

- Any failing test or reviewer finding is recorded in this SOW and fixed before completion, unless the user explicitly accepts the risk.

## Execution Log

### 2026-05-30

- Created SOW from user-approved cleanup recommendation.
- Tried `lzma-wasm@1.0.7`; rejected it when Node package tests showed XZ stream
  flag byte `4` instead of `CHECK_NONE` byte `0`.
- Removed the full `node-liblzma` dependency and vendored only the WASM runtime
  files needed to preserve the existing `CHECK_NONE` behavior.
- Added a Node `files` whitelist and package-lock tests for install-script
  dependencies.
- Added Rust JSON flattener parity tests covering the removed upstream
  dependency's edge cases.
- Addressed reviewer observations by adding file-level SHA-256 values for the
  vendored WASM files, setting `liblzma.wasm` to non-executable mode, adding a
  Node package license field, restoring upstream debug-only Rust flattener
  assertions, and adding an empty-object flattener test.
- Addressed final reviewer packaging hygiene observations by adding
  `node/LICENSE`, including it in the Node package tarball, adding an executable
  test for vendored WASM SHA-256 hashes, documenting `node/.npmrc` as a local
  development guard, ignoring `.cate/`, and updating the reviewer pool docs now
  that `mimo-v2.5-pro` is available again.

## Validation

Acceptance criteria evidence:

- Rust flattener parity:
  - `rust/src/crates/journal-log-writer/src/log/mod.rs` now records provenance
    beside `flatten_json_map()` and includes upstream-equivalent tests for
    empty documents, unflattened documents, object flattening, arrays,
    collisions, nested arrays, nested arrays plus objects, and preserved
    original nested values.
  - `PROVENANCE.md` records `meilisearch/meilisearch @
    077ec2ab11bb4daefcb57f89eab9cff16e075fdc`, path
    `crates/flatten-serde-json/src/lib.rs`, and MIT license status.
- Node dependency/package hygiene:
  - `node/package-lock.json` now has only `lz4js@0.2.0` as a runtime
    dependency and no `hasInstallScript` package.
  - `node/src/lib/xz-block.js` imports bundled
    `node/vendor/node-liblzma-wasm/` files and still uses
    `LZMA_CHECK_NONE`.
  - `node/package.json` now has a `files` whitelist including `vendor/`.
  - `node/vendor/node-liblzma-wasm/README.md` and `PROVENANCE.md` record
    source, version, package integrity, file-level SHA-256 values, included
    files, and LGPL-3.0 license.

Tests or equivalent validation:

- `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo test --manifest-path rust/Cargo.toml -p journal-log-writer --features serde-api`: PASS, including 10 unit tests, 48 integration tests, and 2 doc tests.
- `npm_config_cache="$PWD/../.local/npm-cache" npm ci && npm_config_cache="$PWD/../.local/npm-cache" npm test` from `node/`: PASS.
- `python3 tests/interoperability/run_compression_matrix.py --writers node --readers node stock --compression xz --entries 20`: PASS, 9/9 checks against systemd `260 (260.1-2-manjaro)`.
- `npm pack --dry-run --json` from `node/`: PASS; 30 packaged files, `bad []` for `.local`, `node_modules`, and `.node` paths; `LICENSE` and vendor files included.
- Packed tarball install smoke under `.local/npm-install-smoke-0072`: PASS; installed packed package, wrote an XZ-compressed journal entry, read it back through the installed package.
- `git diff --check`: PASS.
- `.agents/sow/audit.sh`: PASS; clean verdict.

Real-use evidence:

- The packed Node artifact `.local/npm-packs/sdk/netdata-systemd-journal-sdk-0.1.0.tgz` installed into a separate repo-local scratch package and successfully imported `@netdata/systemd-journal-sdk`, wrote an XZ-compressed journal file, and read back the expected `MESSAGE` value.

Reviewer findings:

- Round 1 read-only reviewers:
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`; no blocking findings.
  - `llm-netdata-cloud/qwen3.6-plus`: first session ended without returning a
    final message to the shell session, so the same full-scope prompt was rerun;
    rerun vote was `PRODUCTION GRADE` with no blocking findings.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`; no blocking findings.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`; no blocking
    findings.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`; no blocking
    findings.
- Round 2 read-only reviewers after low-risk hardening edits:
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`; no blocking findings.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`; no blocking
    findings.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`; no blocking findings.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`; no blocking
    findings.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`; no blocking
    findings.
- Non-blocking observations and dispositions:
  - Vendored WASM file-level integrity was useful provenance. Added SHA-256
    values to `node/vendor/node-liblzma-wasm/README.md` and `PROVENANCE.md`.
  - `node/vendor/node-liblzma-wasm/liblzma.wasm` had executable mode. Changed
    it to `0644`; it is loaded as data by Node.js.
  - `node/package.json` lacked an npm license field. Added
    `GPL-3.0-or-later`, matching the Rust workspace license, and refreshed
    `node/package-lock.json`.
  - The local Rust flattener lacked upstream debug-only scalar assertions.
    Added the `debug_assert!` checks.
  - Empty-object flattening was not explicitly tested. Added
    `flatten_json_map_preserves_empty_documents`.
  - `.npmrc` remains as a local development guard. No code change needed
    because package correctness is now enforced by the dependency graph,
    package whitelist, and tests.
  - Physical `LICENSE` file in the Node package was useful for scanners. Added
    `node/LICENSE` and included it in the package whitelist.
  - Documented vendored hashes needed executable enforcement. Added the check to
    `node/test/all.js`.
  - `.npmrc` could confuse future contributors. Added an inline comment to the
    file.
  - `.cate/` was an unrelated untracked local tool directory. Added it to
    `.gitignore`; it remains untracked and unstaged.
  - The reviewer pool docs still said `mimo-v2.5-pro` was skipped. Updated
    `AGENTS.md` and the project orchestration skill to match the restored
    reviewer pool.

Same-failure scan:

- `rg` scan confirmed `node/package-lock.json` no longer contains
  `node-liblzma`, `node-gyp-build`, or `hasInstallScript`.
- Node package test now fails if a future dependency reintroduces
  `hasInstallScript`, `node-liblzma`, or `node-gyp-build`.
- `npm pack --dry-run --json` scan confirmed no `.local`, `node_modules`, or
  `.node` artifacts are included in the package.
- `rg` scan confirmed remaining `node-liblzma` mentions are documentation,
  provenance, SOW history, vendor path, or tests that assert the full package is
  not a dependency.
- Reviewer-identified same-class issues were checked and addressed: package
  license metadata and file inclusion, vendored file integrity documentation and
  executable checks, vendored file mode, local scratch ignore rules, and
  upstream debug assertion parity.

Sensitive data gate:

- No sensitive data was required or written. Dependency names, versions,
  package integrity strings, source repositories, and synthetic test paths are
  non-sensitive. The project audit sensitive-data scan passed.

Artifact maintenance gate:

- AGENTS.md: updated the reviewer pool to include `mimo-v2.5-pro` now that the
  user restored its availability.
- Runtime project skills: `.agents/skills/project-agent-orchestration/SKILL.md`
  updated to match the reviewer pool in `AGENTS.md`.
- Specs: `.agents/sow/specs/product-scope.md` updated for Node.js bundled XZ
  WASM runtime files and no-native-install package posture.
- End-user/operator docs: `node/README.md`, `node/LICENSE`, `PROVENANCE.md`,
  and `node/vendor/node-liblzma-wasm/README.md` updated.
- End-user/operator skills: no output/reference skills exist for this surface.
- SOW lifecycle: this SOW is marked `completed` and moved to `done/` with the
  implementation commit.
- SOW-status.md: root `SOW-status.md` and `.agents/sow/SOW-status.md` updated.

Specs update:

- `.agents/sow/specs/product-scope.md` updated.

Project skills update:

- No project skill update needed; no repeated workflow rule changed.

End-user/operator docs update:

- `node/README.md`, `node/LICENSE`, `PROVENANCE.md`, and
  `node/vendor/node-liblzma-wasm/README.md` updated.

End-user/operator skills update:

- No end-user/operator skill exists or was affected.

Lessons:

- A no-native-install dependency is not automatically compatible with systemd's
  XZ writer behavior. `lzma-wasm@1.0.7` avoided install hooks but emitted XZ
  check flag `4`, so the test-proven `CHECK_NONE` contract had to win over
  dependency aesthetics.
- Package-local `.npmrc` is useful as a local guard but is not a substitute for
  a dependency graph and package tarball that are safe by construction.
- Reviewers can find low-cost hardening items after production-grade approval.
  Addressing file hashes, file mode, license metadata, and debug assertions in
  the same SOW kept the dependency cleanup debt-free.
- Reviewer pool availability is operational knowledge, not code behavior, but
  stale runtime instructions still create process risk. Keep `AGENTS.md` and
  project skills synchronized when model availability changes.

Follow-up mapping:

- SOW-0071 still owns the larger runtime-purity split. No new follow-up SOW is
  needed for this cleanup; reviewer observations were addressed in this SOW.

## Outcome

SOW-0072 removed the hidden Node native-install dependency risk, added package
tarball hygiene, preserved systemd-compatible XZ `CHECK_NONE` behavior, and
strengthened Rust JSON flattener parity/provenance. All local validation passed
and all read-only reviewers voted `PRODUCTION GRADE`.

## Lessons Extracted

- Compatibility must be tested at the byte/header behavior level, not inferred
  from dependency descriptions. `lzma-wasm@1.0.7` looked cleaner but emitted the
  wrong XZ check type for journal compatibility.
- Package-local install guards are not enough for published SDKs. The package
  dependency graph and packed artifact must be safe by construction.
- Vendored runtime files need source, version, license, package integrity, and
  file-level hashes so future updates are auditable.

## Followup

None.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
