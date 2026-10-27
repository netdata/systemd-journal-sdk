# Releasing Rust and Go

The **Release** workflow validates Rust and Go, then publishes all eight Rust
crates from one selected merged commit. After it succeeds, a maintainer with
tag-push permission creates the repository and Go tags at that same commit.
A maintainer starts CI manually after release preparation and the workflow
have merged.

The team's full-process project skill is
[`project-release`](.agents/skills/project-release/SKILL.md). Ask to use this
skill when preparing, publishing, verifying or resuming a release.

## One-time setup

1. In the canonical repository, open **Settings → Environments** and create
   an environment named **release**.
2. Set **Deployment branches and tags** to **Selected branches and tags**.
   Add one **Branch** rule named **master**. Do not add a tag rule or a wildcard.
   The workflow checks this configuration before any publication.
3. For each crate below, a crate owner opens **Settings → Trusted Publishing →
   Add → GitHub** on crates.io and saves the same configuration:

   | Setting | Value |
   | --- | --- |
   | Repository owner | `netdata` |
   | Repository name | `systemd-journal-sdk` |
   | Workflow filename | `release.yml` |
   | Environment | `release` |

   All eight existing crates need this binding:

   - [systemd-journal-sdk-common](https://crates.io/crates/systemd-journal-sdk-common/settings)
   - [systemd-journal-sdk-registry](https://crates.io/crates/systemd-journal-sdk-registry/settings)
   - [systemd-journal-sdk-core](https://crates.io/crates/systemd-journal-sdk-core/settings)
   - [systemd-journal-sdk-host](https://crates.io/crates/systemd-journal-sdk-host/settings)
   - [systemd-journal-sdk-log-writer](https://crates.io/crates/systemd-journal-sdk-log-writer/settings)
   - [systemd-journal-sdk-index](https://crates.io/crates/systemd-journal-sdk-index/settings)
   - [systemd-journal-sdk-engine](https://crates.io/crates/systemd-journal-sdk-engine/settings)
   - [systemd-journal-sdk](https://crates.io/crates/systemd-journal-sdk/settings)

No permanent crates.io token is stored in GitHub. The official Rust publishing
action obtains a temporary permission for each crate, after its dry-run, and
revokes that permission when the job ends. See the official
[Trusted Publishing guide](https://crates.io/docs/trusted-publishing).

Required environment reviewers are optional. If configured, GitHub will also
ask for that approval before the publishing job starts.

Any maintainer with repository write access can start the workflow; they do
not need to be a crates.io owner. The crate owner configures Trusted Publishing
once. Tag pushes use each maintainer's existing GitHub access. See GitHub's
[manual workflow permissions](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow).

## Prepare the release

- Review language parity and choose the version in the release preparation
  SOW. Update Rust workspace/internal package versions and active Rust/Go
  installation examples together.
- Include migration notes for any Rust source compatibility changes, including
  additions to exhaustive public enums.
- Complete preparation validation and its SOW in the release PR. Merge that
  PR before publishing. Track actual publication in its own execution SOW.
- Copy the full 40-character lowercase hash of the merged release source.
  That commit must be reachable from canonical `master` and contain the
  prepared version. It can precede the workflow's own merge commit.

## Start a release

1. Open **Actions → Release → Run workflow** in
   `netdata/systemd-journal-sdk`.
2. Select **master** as the workflow branch.
3. Enter the version as **X.Y.Z**, without `v`, and the full merged source hash.
   The current workflow supports major versions 0 and 1. Major version 2 or
   later requires a reviewed Go module-path migration first.
4. Start the workflow. This authorizes publication of that selected release.

The workflow code comes from the dispatch's master commit. The release source
is checked out separately at the supplied immutable hash. Ordinary pushes and
PRs run the release-helper tests; they do not publish anything.

## What CI verifies and publishes

- Confirms the canonical source, merged ancestry, clean checkout, version
  alignment, published dependency pins, installation examples, Go module path,
  release environment, and any existing release tags or package versions.
- Runs the full Rust and Go suites using the compiler minimums declared by the
  selected source. Go SDK tests run with CGO disabled; separate Go CLI tests
  enable CGO. CLI-inclusive Rust/Go checks install PCRE2 development files and
  pkg-config on the CI runner. Publishing and SDK consumer builds remain pure.
- Dry-runs and publishes serially: **common → registry → core → host →
  log-writer → index → engine → public SDK**. It waits for each uploaded version
  to appear in both the registry API and index before continuing.
- Verifies each version is not yanked, checks the downloaded archive checksum,
  and confirms its clean Cargo VCS metadata identifies the selected source.
- Builds a fresh Rust consumer requiring the exact published SDK version.

The final job writes a Rust publication summary with the version and exact
source after all these checks pass. That summary hands tag creation to a
maintainer; the paired release is complete only after the tags and Go consumer
checks below pass. Public crate publication is not atomic: an interruption can
leave an already-published prefix of the eight crates.

## Push the release tags

After the Release run succeeds, a maintainer with permission to push canonical
repository tags performs this step using their normal GitHub access.

1. Copy the version and full source SHA from the successful run summary. Both
   tags must identify that source, even if master has advanced.
2. Fetch canonical master and check existing local/remote tags. If a tag is
   already present, confirm it is annotated and peels to the selected source.
   Preserve a correct existing tag; a conflicting tag requires an explicit
   release/version decision.
3. For a new tag pair, replace the two placeholders below, then run:

   ```bash
   RELEASE_VERSION='COPY_VERSION_FROM_CI_SUMMARY'
   RELEASE_COMMIT='COPY_FULL_SOURCE_SHA_FROM_CI_SUMMARY'
   (
     set -eux
     python3 tests/release/release.py check-inputs --version "$RELEASE_VERSION" --commit "$RELEASE_COMMIT"
     git fetch git@github.com:netdata/systemd-journal-sdk.git master
     git merge-base --is-ancestor "$RELEASE_COMMIT" FETCH_HEAD
     git tag --annotate "v$RELEASE_VERSION" "$RELEASE_COMMIT" --message "v$RELEASE_VERSION"
     git tag --annotate "go/v$RELEASE_VERSION" "$RELEASE_COMMIT" --message "go/v$RELEASE_VERSION"
     git push --atomic git@github.com:netdata/systemd-journal-sdk.git "refs/tags/v$RELEASE_VERSION" "refs/tags/go/v$RELEASE_VERSION"
   )
   ```

   The block prints each command and stops on failure. Use Python 3.11+.
   The explicit canonical remote also works from a
   fork clone. An atomic push creates both new tags together. If a correct
   remote sibling already exists, push only the missing tag after checking
   its selected source; do not recreate the existing sibling.
4. From a checkout containing this release helper and the fetched source
   commit, use Python 3.11+ and a Go compiler meeting that source's minimum:

   ```bash
   CGO_ENABLED=0 python3 tests/release/release.py verify-go --version "$RELEASE_VERSION" --commit "$RELEASE_COMMIT"
   ```

   This verifies both canonical annotated tag targets and all eight Rust
   archive source hashes, downloads the exact Go module, checks any provided
   origin hash, and builds a fresh consumer
   under `.local/`. Its module/compiler declarations come from the selected
   release commit, so the verification also works when master has advanced.
   The helper verifies tags and consumption; maintainers perform the tag push.

## Resume an interrupted release

- Rerun the failed jobs, or start a new run with the **same version and source
  hash**. A verified existing crate is skipped, so an accepted upload is never
  blindly repeated.
- A Cargo upload can succeed even when its command times out. CI checks the
  registry before deciding whether that crate completed.
- If a crate is visible in only the API or only the index, CI waits for the
  other view. An HTTP failure is an error, not evidence that a version is absent.
- If publishing permission is missing for a later crate, complete that crate's
  Trusted Publishing setup and rerun with the original inputs.
- If Rust CI succeeds before tags are pushed, complete the maintainer tag
  handoff using its recorded source. A rerun with those same inputs verifies
  already-published crates without replacing them.
- A rejected atomic push changes neither tag. Re-check the canonical tags;
  another maintainer may have created them. Preserve correct annotated tags
  and push only missing tags after resolving the reported rejection. If the
  matching local tags already exist, retry only the push command.
- If Go lookup has not propagated after the tags are pushed, rerun `verify-go`.
  It preserves both tags and checks consumption again.

A crate published from another commit, a yanked version, or a conflicting tag
stops the release. Resolve that conflict with an explicit version/release
decision; do not move tags or attempt to replace published crate versions.

After Rust CI, maintainer tagging and Go verification succeed, record the
workflow run, all eight versions and the paired tag source hash in the
publication SOW and complete that execution record.
