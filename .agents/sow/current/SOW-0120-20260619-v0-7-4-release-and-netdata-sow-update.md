# SOW-0120 - v0.7.4 release and Netdata SOW update

## Status

Status: in-progress

Sub-state: release planning active; no irreversible publication action taken yet.

## Requirements

### Purpose

Release the Rust and Go SDK changes that landed after `v0.7.3`, then update the
Netdata integration SOW so Netdata consumes the latest released SDK version.

Fit-for-purpose target:

- Rust crates and Go module tags point to the same released commit.
- Published Rust package metadata and user-facing install snippets name the same
  patch version.
- Netdata integration planning points to the latest release, not stale `0.7.3`
  text.
- The Go language version directive is not changed.
- Release evidence is concrete enough for downstream Netdata implementation to
  proceed without guessing.

### User Request

The user stated that because Rust and Go were touched after the prior release,
the SDK should be released again and the Netdata SOW should be updated to use
the latest release.

### Assistant Understanding

Facts:

- Latest existing release tags are `v0.7.3` and `go/v0.7.3`.
- `v0.7.4` and `go/v0.7.4` do not exist locally or remotely at the start of
  this SOW.
- The unreleased commit range `v0.7.3..HEAD` includes files under `go/` and
  `rust/`.
- The Rust workspace package version is still `0.7.3`.
- The active Netdata SOW
  `.agents/sow/active/SOW-20260618-systemd-journal-sdk-0.7.3-host-helper.md`
  in the Netdata worktree
  points at SDK `0.7.3`.
- The user explicitly requested updating the Netdata SOW pointer.

Inferences:

- The next patch release should be `0.7.4`.
- A real Rust release requires changing Rust package metadata from `0.7.3` to
  `0.7.4`, publishing crates, and tagging both the root repository and Go
  submodule tag on the same commit.
- The Netdata SOW update is a narrow cross-repository exception authorized by
  the user for this task.

Unknowns:

- crates.io publication may need index propagation delays between dependent
  packages.

### Acceptance Criteria

- Rust workspace/package dependency metadata and install snippets target
  `0.7.4`.
- Go install snippets target `go/v0.7.4`.
- `go/go.mod` keeps its existing Go directive.
- Local Rust and Go tests pass on the release commit.
- Read-only external reviewers covering Rust/Go release readiness vote
  production-grade before publication.
- Rust crates are published to crates.io at `0.7.4`.
- Annotated tags `v0.7.4` and `go/v0.7.4` are pushed and peel to the same
  commit.
- `go list -m github.com/netdata/systemd-journal-sdk/go@v0.7.4` resolves.
- The Netdata SOW points at SDK `0.7.4`, `go/v0.7.4`, and the new release
  commit.
- SOW audit and whitespace checks pass.

## Analysis

Sources checked:

- `git tag --list --sort=-version:refname`
- `git ls-remote --tags origin refs/tags/v0.7.4 refs/tags/go/v0.7.4`
- `git log --oneline v0.7.3..HEAD`
- `git diff --name-only v0.7.3..HEAD`
- `rust/Cargo.toml`
- `.agents/sow/done/SOW-0118-20260617-host-helper-release-and-netdata-integration.md`
- Netdata worktree:
  `.agents/sow/active/SOW-20260618-systemd-journal-sdk-0.7.3-host-helper.md`

Current state:

- `v0.7.3` and `go/v0.7.3` exist.
- `v0.7.4` and `go/v0.7.4` do not exist locally or remotely.
- `rust/Cargo.toml` has workspace package version `0.7.3` and internal
  published dependency versions `0.7.3`.
- The Netdata SOW names `0.7.3` throughout its purpose, facts, acceptance
  criteria, dependency update plan, and open-source evidence.

Risks:

- Tags and crates.io package versions are effectively irreversible once pushed
  or published.
- A missing Go submodule tag would break downstream `go get`.
- A Rust metadata/docs mismatch would confuse downstream consumers.
- Updating only this repository while leaving the Netdata SOW stale would point
  implementation agents at the wrong release.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The SDK released `0.7.3`, then accepted additional Rust/Go changes. Netdata
  integration planning references the previous release. Downstream consumers
  need a stable release containing the current Rust/Go state and an updated
  integration SOW reference.

Evidence reviewed:

- `git log --oneline v0.7.3..HEAD` shows commits after the `0.7.3` release.
- `git diff --name-only v0.7.3..HEAD` includes `go/journal/...` and
  `rust/src/...`.
- `rust/Cargo.toml` currently records `0.7.3`.
- No local or remote `0.7.4` tags exist at the start of the SOW.
- Netdata SOW path above references `0.7.3`.

Affected contracts and surfaces:

- Rust crates.io packages:
  `systemd-journal-sdk-common`, `systemd-journal-sdk-registry`,
  `systemd-journal-sdk-core`, `systemd-journal-sdk-host`,
  `systemd-journal-sdk-log-writer`, `systemd-journal-sdk-index`,
  `systemd-journal-sdk-engine`, and `systemd-journal-sdk`.
- Go module tag `go/v0.7.4`.
- Root release tag `v0.7.4`.
- SDK README and wiki install snippets.
- Product-scope version examples.
- Netdata host-helper integration SOW.

Existing patterns to reuse:

- `SOW-0118` release flow for `0.7.3`.
- `project-release-tagging` skill for root and Go submodule tags plus Rust
  publish order.
- Existing docs version snippets under `README.md` and `docs/`.

Risk and blast radius:

- Medium release risk because publishing and tags are permanent.
- Low code risk for version metadata changes, but release scope includes all
  Rust/Go code already pushed after `0.7.3`.
- Cross-repository write risk is limited to one user-authorized Netdata SOW file.

Sensitive data handling plan:

- No raw secrets, credentials, customer data, SNMP communities, trap payloads,
  private endpoints, or host-specific private values are needed. Durable
  artifacts record only sanitized file paths, commit hashes, package versions,
  and command summaries.

Implementation plan:

1. Update SDK version metadata and install snippets from `0.7.3` to `0.7.4`.
2. Validate locally before review: Rust tests, Go tests, docs examples, version
   searches, whitespace checks, and SOW audit.
3. Run read-only external reviewers against the complete release diff and this
   SOW, including the user requirement for Rust/Go production-grade review.
4. Resolve any reviewer blockers, revalidate, and repeat review if needed.
5. Publish Rust crates in dependency order, push `master`, create and push
   annotated `v0.7.4` and `go/v0.7.4` tags.
6. Verify crates.io visibility, Go module lookup, and remote peeled tag targets.
7. Update the Netdata SOW pointer to `0.7.4` and the new release commit.
8. Close this SOW and commit release/SOW updates.

Validation plan:

- `go test ./...`
- `cargo test --workspace --all-targets`
- `python3 tests/docs/verify_examples.py --timeout 60`
- `python3 tests/docs/check_wiki_docs.py`
- `git diff --check`
- `.agents/sow/audit.sh`
- `cargo publish --dry-run` and `cargo publish` in dependency order.
- `go list -m github.com/netdata/systemd-journal-sdk/go@v0.7.4`
- `git ls-remote --tags origin refs/tags/v0.7.4 refs/tags/v0.7.4^{} refs/tags/go/v0.7.4 refs/tags/go/v0.7.4^{}`

Artifact impact plan:

- AGENTS.md: no expected update; release workflow unchanged.
- Runtime project skills: no expected update; release skill already covers this
  flow.
- Specs: update product-scope version examples.
- End-user/operator docs: update SDK install snippets.
- End-user/operator skills: no expected update unless docs/spec changes expose a
  skill gap.
- SOW lifecycle: create active SOW, complete after release and Netdata SOW
  pointer update.
- SOW-status.md: update active and completed release status.

Open-source reference evidence:

- No external open-source implementation research is needed; this is a release
  and downstream SOW pointer update for this repository's own SDK. The Netdata
  SOW update will cite `netdata/systemd-journal-sdk @ <release-commit>` rather
  than workstation paths.

Open decisions:

- Resolved by user request: release again after Rust/Go changes and update the
  Netdata SOW.
- Resolved by semver evidence: use next patch version `0.7.4`.
- Resolved cross-repository exception: update only the Netdata SOW named above,
  not Netdata implementation files.

## Implications And Decisions

1. Release version
   - Decision: release `0.7.4`.
   - Evidence: latest release is `0.7.3`; no `0.7.4` tags exist; the unreleased
     range after `0.7.3` contains Rust and Go changes.
   - Classification: long-term-best.
   - Implications: Rust package metadata, install docs, crates.io publication,
     root tag, and Go submodule tag all target `0.7.4`.
   - Risks: tag or crate publication mistakes cannot be fixed by moving the same
     version without explicit destructive approval.

2. Netdata SOW update
   - Decision: update only the active Netdata host-helper SOW to point at
     `0.7.4`; do not edit Netdata code in this SDK release SOW.
   - Evidence: the user requested updating the Netdata SOW, and this SDK repo's
     repository boundary otherwise forbids writes outside this repository.
   - Classification: surgical.
   - Implications: the Netdata implementation SOW remains aligned with the
     released SDK; actual Netdata dependency/code changes stay in the Netdata
     work item.
   - Risks: the Netdata repository has many untracked files, so staging or
     committing there is explicitly excluded from this SDK SOW.

## Plan

1. Prepare `0.7.4` metadata and documentation.
2. Validate local tests and release preconditions.
3. Run read-only external production-grade review.
4. Publish Rust crates and push tags.
5. Update the Netdata SOW release references.
6. Close and commit the SDK SOW updates.

## Delegation Plan

Implementer:

- Local project-manager edits only. This SOW changes release metadata, docs,
  SOW evidence, and one Netdata SOW reference; no product logic implementation
  is delegated.

Reviewers:

- Read-only external reviewers for Rust/Go release readiness: claude, codex,
  glm, minimax, kimi, mimo, deepseek, and qwen, matching the user's earlier
  Rust/Go review requirement.
- This is an explicit user-requested reviewer override for this release. It
  expands the project-local default reviewer pool for Rust/Go changes, while
  keeping every reviewer read-only and non-interactive.

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

- Stop before publication if validation or reviewer blockers remain.
- Stop and ask before moving, deleting, or force-pushing any tag.
- Record crates.io indexing delays without credentials or tokens.

## Execution Log

### 2026-06-19

- Created SOW after user requested another release and Netdata SOW update.
- Updated Rust workspace metadata, lockfile package versions, SDK install
  snippets, wiki docs, and product-scope examples to `0.7.4`.
- Preserved `go/go.mod` unchanged, including the existing Go language
  directive.
- First external review round found one release blocker:
  `rust/README.md` still referenced `0.7.3`.
- Fixed the stale `rust/README.md` install snippet to `0.7.4`.
- Second full-scope external review round completed after the fix. Claude,
  Codex, glm, minimax, kimi, mimo, deepseek, and qwen all voted
  `PRODUCTION GRADE: YES`.

## Validation

Acceptance criteria evidence:

- Version metadata and install snippets now target `0.7.4` in:
  `rust/Cargo.toml`, `rust/Cargo.lock`, `README.md`, `rust/README.md`,
  `docs/Getting-Started.md`, `docs/Go-API.md`, `docs/Rust-API.md`,
  `docs/Rust-Crates-And-Packages.md`, and
  `.agents/sow/specs/product-scope.md`.
- Go install snippets target `go/v0.7.4`.
- `go/go.mod` remains unchanged and keeps its existing Go directive.
- Pre-publication remote tag check with an explicit SSH config bypass returned
  no `v0.7.4` or `go/v0.7.4` tags.
- The reviewer gate is satisfied. Rust crate publication, pushed tags, Go
  module lookup, and Netdata SOW update remain to be executed.

Tests or equivalent validation:

- `go test ./...` passed from `go/`.
- `cargo test --workspace --all-targets` passed from `rust/`.
- `python3 tests/docs/verify_examples.py --timeout 60` passed 31 of 31
  examples.
- `python3 tests/docs/check_wiki_docs.py` validated 15 wiki markdown files.
- `git diff --check` passed.
- `.agents/sow/audit.sh` passed.
- `cargo publish --manifest-path src/crates/journal-common/Cargo.toml
  --dry-run` passed for the first publishable Rust crate.

Real-use evidence:

- The first publishable Rust package dry-run verified packaging for
  `systemd-journal-sdk-common v0.7.4`.
- Dependent Rust package dry-runs intentionally wait until their `0.7.4`
  dependencies are published and indexed.
- `go list -m github.com/netdata/systemd-journal-sdk/go@v0.7.4` cannot resolve
  until the `go/v0.7.4` tag is pushed.

Reviewer findings:

- First external review round found one release blocker: `rust/README.md` still
  referenced `0.7.3`.
- The stale `rust/README.md` snippet was fixed to `0.7.4`.
- The SOW now records the user-requested reviewer override for claude and codex
  in addition to the project-local default model pool.
- Second full-scope reviewer round results:
  - Claude: `PRODUCTION GRADE: YES`.
  - Codex: `PRODUCTION GRADE: YES`.
  - glm: `PRODUCTION GRADE: YES`.
  - minimax: `PRODUCTION GRADE: YES`.
  - kimi: `PRODUCTION GRADE: YES`.
  - mimo: `PRODUCTION GRADE: YES`.
  - deepseek: `PRODUCTION GRADE: YES`.
  - qwen: `PRODUCTION GRADE: YES`.

Same-failure scan:

- `rg -n "0\\.7\\.3|v0\\.7\\.3|go/v0\\.7\\.3" README.md rust/README.md docs
  .agents/sow/specs/product-scope.md rust/Cargo.toml rust/Cargo.lock`
  returned no live version-snippet matches after the fix.

Sensitive data gate:

- Pre-publication audit passed. The active SOW cites the Netdata SOW by
  repository-relative path instead of recording workstation user-name paths.

Artifact maintenance gate:

- AGENTS.md: no workflow or responsibility change found so far.
- Runtime project skills: no release-skill change needed so far.
- Specs: product-scope version examples updated to `0.7.4`.
- End-user/operator docs: SDK install snippets updated to `0.7.4`.
- End-user/operator skills: no affected output/reference skills found so far.
- SOW lifecycle: active SOW remains in progress until publication and Netdata
  SOW update complete.
- SOW-status.md: active status updated.

Specs update:

- `.agents/sow/specs/product-scope.md` updated to `0.7.4`.

Project skills update:

- No project-skill update needed for the pre-publication changes; existing
  release skill already covers the publish and tag flow.

End-user/operator docs update:

- README and wiki docs install snippets updated to `0.7.4`.

End-user/operator skills update:

- No output/reference skill is affected by this release metadata change.

Lessons:

- Rust crate README surfaces include `rust/README.md`; version sweeps must cover
  both root and language-specific READMEs.

Follow-up mapping:

- Publication, tag verification, Go module lookup, Netdata SOW update, and SOW
  close remain in this SOW after reviewer approval.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
