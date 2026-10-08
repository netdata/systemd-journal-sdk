---
name: project-release
description: "Prepare, publish, verify, or resume a Rust/Go SDK release using this repository's CI and maintainer tag handoff."
---
# Project Release

CI publishes Rust crates to crates.io. A maintainer then pushes the repository
and Go module tags at the same selected source commit. A paired release is
complete after both languages can consume that exact version.

Read [RELEASING.md](../../../RELEASING.md) for the maintained setup, commands
and recovery procedure. Use
[project-release-tagging](../project-release-tagging/SKILL.md) for focused
package, semver and tag rules, and
[project-agent-orchestration](../project-agent-orchestration/SKILL.md) for SOW
and review gates. The release contract is in
[product-scope.md](../../sow/specs/product-scope.md#paired-ci-releases).

Follow the user's actual request and existing authorization. Preparing a PR
or editing instructions does not authorize publication or tag pushes. An
explicit release request can authorize those steps; do not ask again for
actions already authorized. Loading this skill does not start a release.

## Prepare and merge

1. Check the active/pending SOWs and the previous published APIs in **both**
   languages. The user chooses scope and version; record the semver decision
   before implementation. Apply the **Go/Rust Feature Parity** rule in
   `AGENTS.md`; resolve any recorded parity gap with the user before publishing.
2. Prepare aligned release metadata, Rust internal dependency pins and
   lockfile, current installation examples and migration notes. Assess Rust
   exhaustive public structs/enums as well as Go source compatibility. Follow
   the compiler-minimum contract in `AGENTS.md`: never raise SDK minimums above
   Netdata's consuming-module declarations.
3. Complete local validation, review and scanner disposition under the active
   SOW. Complete the preparation SOW in the release PR and merge it before
   publication. Track actual publication in its own execution SOW.
4. Record the version and the full 40-character lowercase source SHA that
   contains the prepared release and is reachable from canonical `master`.
   Keep these inputs fixed through publication, tagging and recovery.

## Confirm setup and responsibilities

- The GitHub `release` environment allows exactly **Branch master**, with no
  tag rule or wildcard. Optional required reviewers add an environment
  approval; they are not a requirement of this process.
- A crate owner configures Trusted Publishing **on each of the eight existing
  crates**, using owner `netdata`, repository `systemd-journal-sdk`, workflow
  filename `release.yml` and environment `release`. See the guide's crate list.
  A new crate needs separately authorized initial publication before this
  binding can be configured.
- A maintainer with repository write access can start the workflow without
  being a crates.io owner. A maintainer with tag-push permission performs the
  tag handoff using existing GitHub access. This process requires no extra
  GitHub App or permanent crates.io token.

## Publish Rust through CI

1. Follow the guide's **Start a release** instructions: manually dispatch
   **Actions → Release** on canonical `master`, with version `X.Y.Z` without
   `v` and the recorded source SHA. The current workflow supports major
   versions 0 and 1; version 2 or later needs a reviewed Go module-path
   migration first.
2. Distinguish the workflow's master SHA from the supplied release source SHA.
   The source can precede the workflow's merge commit. Ordinary PRs/pushes do
   not publish, and later master changes do not change the selected source.
3. Require successful source checks and both full language suites at the
   source-declared compiler minimums, with CGO disabled for Go.
4. Require all eight Rust crates to pass in dependency order: **common →
   registry → core → host → log-writer → index → engine → public SDK**. For
   each new version, CI dry-runs before obtaining temporary publishing
   permission and uploading it. Verified existing versions are preserved.
5. Require API/index availability, non-yanked status, archive checksum and
   clean Cargo VCS metadata matching the selected source, followed by a fresh
   exact-version Rust consumer build. Save the successful CI run and its Rust
   publication summary before the tag handoff.

## Push paired tags and verify Go

1. Copy the exact version/source from the successful CI summary. Check local
   and canonical remote tags before creating anything. The root `vVERSION`
   and Go submodule `go/vVERSION` must be annotated and peel to that source,
   even when master has advanced.
2. Use the guide's tag procedure. Push a new pair atomically to the canonical
   repository. Preserve correct existing tags and push only missing siblings;
   stop for a conflicting target or a lightweight tag. Do not move, delete or
   force-push release tags. Signing is optional; if claiming signed tags,
   verify the signatures and that those tag objects match the remote objects.
3. Run the guide's `verify-go` command from a checkout containing the release
   helper and selected source, with Python 3.11+ and a Go compiler meeting that
   source's minimum. It checks both canonical annotated tags, matching Rust
   archive sources and a fresh exact-version Go consumer. It does not mutate
   tags. Go distribution uses the `go/vVERSION` module tag; there is no
   separate crates.io-style Go upload.
4. Close publication only after Rust CI, paired tags and Go verification all
   succeed. Completing the automation or preparation PR alone does not prove
   that a version is published.

## Resume or stop

- For an interrupted Rust run, rerun failed jobs or dispatch again with the
  **same version and source SHA**. CI verifies an existing version before
  skipping it. An upload timeout can still mean the registry accepted it.
- API-only/index-only availability needs propagation checks. An HTTP failure
  does not prove absence; follow the guide's bounded recovery procedure.
- Missing Trusted Publishing on a later crate requires its owner to complete
  setup, then resume with the original inputs. Publication of the eight crates
  is serial and can leave an accepted prefix when interrupted.
- If Rust succeeded, resume the recorded tag handoff. After a rejected atomic
  push, re-check both remote tags; preserve matching siblings and retry only
  missing pushes. If local tags already match, retry the push, not creation.
- If Go lookup has not propagated, rerun `verify-go` while preserving tags.
- A different crate source, yanked version or conflicting tag requires the
  user's release/version decision. Stop; do not replace published artifacts
  or change the source/version during a partially published release.

## Record completion

Record the actual CI run, eight published versions and source checks, paired
tag objects and peeled source, and exact Rust/Go consumer results in the
execution SOW. Keep raw reports and credentials out of durable artifacts.
Require the SOW audit's explicit **complete-and-clean** verdict. Complete and
move the execution SOW with its release evidence in the same commit, update
both SOW ledgers, and report any separately tracked follow-ups.

Maintain this skill and `RELEASING.md` together when release behavior changes;
keep executable recipes in the guide and tested helper.
