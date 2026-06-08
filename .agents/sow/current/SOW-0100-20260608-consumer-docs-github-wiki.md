# SOW-0100 - Consumer Docs And GitHub Wiki Publication

## Status

Status: in-progress

Sub-state: reopened for GitHub code-scanning stale alert closeout regression.

## Requirements

### Purpose

Create consumer documentation that makes the SDK safe to use correctly and fast
by default. The documentation must explain the API layers, hot paths, and
performance-sensitive options so consumers do not accidentally select slow or
debug-oriented paths in production.

### User Request

The user requested a `docs/` GitHub wiki source, CI publishing to the GitHub
wiki on every merge to `master`, and consumer documentation explaining the
different APIs, hot paths, key options that affect them, and non-optimal option
combinations to avoid.

### Assistant Understanding

Facts:

- The repository currently has `documentation/` and CI workflows, but no
  `docs/` GitHub wiki source directory.
- The SDK exposes multiple reader, writer, directory, explorer, facade,
  verification, and Netdata-oriented API paths.
- Several options deliberately trade compatibility, visibility, allocation,
  decompression, mmap behavior, locking, live publication, compression, FSS,
  retention, and debug behavior for performance or operational guarantees.

Inferences:

- Consumer documentation must be organized around intended use cases and
  performance contracts, not only around crate/module names.
- The documentation should identify recommended production defaults and
  explicitly label debugging or compatibility-only paths that should not be used
  as benchmark evidence.

Unknowns:

- Which exact GitHub wiki publication action should be used. This must be
  selected during implementation after checking current GitHub Actions and
  security best practices.

### Acceptance Criteria

- A committed `docs/` directory contains GitHub wiki source pages.
- CI publishes `docs/` to the repository GitHub wiki on every successful merge
  to `master`.
- Consumer docs explain Rust package/crate names and the intended dependency
  syntax.
- Consumer docs explain reader API layers and the hot path differences between
  core file reader, public SDK reader, directory reader, facade, explorer, and
  verification/export paths.
- Consumer docs explain writer API layers and performance-sensitive options:
  structured vs raw payload, compact format, compression, FSS, live publication
  cadence, mmap strategy, rotation, retention, field-name policy, and optional
  locking/identity helpers.
- Consumer docs explain query/explorer hot paths and the cost of decompression,
  debug row traversal, FTS, facets, histograms, filtering, and returned-row
  expansion.
- Docs include production recommendations and anti-patterns for high-throughput
  ingestion and high-throughput query/explorer use.
- Docs are validated for internal links and the wiki CI workflow is validated
  without exposing credentials or writing raw secrets to durable artifacts.

## Analysis

Sources checked:

- `.github/workflows/codeql.yml`
- `.github/workflows/codacy-sarif.yml`
- `.github/workflows/coverage.yml`
- `documentation/code-scanning.md`
- `README.md`
- `rust/README.md`
- `AGENTS.md`
- `.agents/sow/specs/product-scope.md`

Current state:

- No `docs/` directory exists.
- No GitHub wiki publication workflow exists.
- Consumer-facing README coverage exists but is not enough for API selection,
  hot-path behavior, or performance-sensitive option guidance.

Risks:

- Poor docs can make consumers accidentally enable debug or compatibility paths
  that void the SDK performance goals.
- A wiki publication workflow can leak credentials or allow unsafe writes if not
  restricted to trusted `master` merges.
- Documentation that over-promises performance without tying claims to options
  and data shape will be misleading.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The SDK now has enough API layers and performance-sensitive options that
  README snippets are insufficient. Without a structured consumer guide,
  consumers can easily select slow paths, enable debug options, or miss the
  intended hot path for their use case.

Evidence reviewed:

- `.github/workflows/` contains CodeQL, Codacy SARIF, and coverage workflows,
  but no wiki publication workflow.
- `documentation/code-scanning.md` exists, but there is no `docs/` wiki source.
- `README.md` and `rust/README.md` contain package/dependency information, but
  not full hot-path guidance.
- `AGENTS.md` and `.agents/sow/specs/product-scope.md` define the performance
  contract that consumer docs must expose.

Affected contracts and surfaces:

- Public documentation.
- GitHub Actions workflow permissions and trigger behavior.
- Rust consumer onboarding.
- Future Go/Python/Node documentation structure.
- Netdata integration guidance.

Existing patterns to reuse:

- Existing `.github/workflows/*.yml` style.
- Existing `documentation/` Markdown style where applicable.
- Project performance contract in `AGENTS.md`.
- Rust package names and dependency syntax from SOW-0099.

Risk and blast radius:

- Medium. The code path is unchanged, but docs and CI workflow can influence
  public usage and repository automation.
- Security risk is concentrated in the wiki publication workflow. It must run
  only on trusted branch events and use minimum permissions.

Sensitive data handling plan:

- Do not record GitHub tokens, Cargo tokens, secrets, private repository URLs,
  customer data, or local-only paths in docs or workflow comments.
- CI docs may refer to GitHub-provided tokens generically, but must not include
  raw secret values.

Implementation plan:

1. Design the `docs/` wiki structure and navigation.
2. Write consumer docs for package selection, reader APIs, writer APIs,
   explorer/query APIs, hot paths, option costs, and recommended profiles.
3. Add GitHub Actions workflow to publish `docs/` to the GitHub wiki on
   trusted `master` merges.
4. Add documentation validation for links and workflow syntax where practical.
5. Update specs/status and run review.

Validation plan:

- Markdown link check or equivalent local validation.
- GitHub Actions workflow syntax check where practical.
- Review docs against current Rust APIs and product performance contract.
- Read-only reviewer pool after the whole SOW implementation.
- `.agents/sow/audit.sh`.

Artifact impact plan:

- AGENTS.md: likely no update unless documentation workflow changes project
  process rules.
- Runtime project skills: likely update release/docs workflow skill if wiki
  publication becomes a durable operator process.
- Specs: update product scope to identify `docs/` as the consumer wiki source.
- End-user/operator docs: add `docs/` pages.
- End-user/operator skills: no output/reference skills expected unless docs are
  mirrored to skills later.
- SOW lifecycle: close with docs and workflow evidence.
- SOW-status.md: update when implementation starts and closes.

Open-source reference evidence:

- GitHub documentation was checked for wiki and Actions behavior:
  - GitHub Docs, `communities/documenting-your-project-with-wikis/about-wikis`:
    wikis host long-form project documentation, can be edited locally, and are
    public or private according to repository access.
  - GitHub Docs,
    `communities/documenting-your-project-with-wikis/adding-or-editing-wiki-pages`:
    wikis are Git repositories and can be cloned with
    `https://github.com/YOUR-USERNAME/YOUR-REPOSITORY.wiki.git` after the first
    wiki page exists.
  - GitHub Docs, `actions/concepts/security/github_token`: `GITHUB_TOKEN`
    permissions are limited to the repository that contains the workflow.
- Related repository pattern checked read-only:
  - `netdata/ai-agent`, `.github/workflows/wiki-sync.yml`: checks out
    `${{ github.repository }}.wiki` with `actions/checkout` and
    `token: ${{ secrets.GITHUB_TOKEN }}`, with job-level `contents: write`.

Open decisions:

- None blocking. The user has requested the `docs/` GitHub wiki source model
  and CI publication on merge to `master`.

## Implications And Decisions

1. Documentation location
   - Decision: use `docs/` as the committed GitHub wiki source.
   - Implication: `documentation/` may remain for internal/project notes unless
     a later SOW consolidates it.

2. Documentation focus
   - Decision: document hot paths and option costs as first-class content.
   - Implication: docs must call out slow/debug paths and explain why they are
     not production defaults.

## Plan

1. Build the `docs/` structure and initial navigation.
2. Write API and hot-path guides.
3. Add secure wiki publication CI.
4. Validate docs/workflow and run reviewers.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Whole-SOW read-only reviewer pool after implementation and local validation.

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

- If workflow credentials or permissions cannot be validated locally, record the
  exact GitHub-side validation requirement and do not claim full workflow proof.
- If reviewers find docs that contradict the performance contract, fix the docs
  and rerun the whole-SOW review scope.

## Execution Log

### 2026-06-08

- Created the SOW from the user's documentation and GitHub wiki publication
  request.
- Activated the SOW for local implementation.
- Added the committed `docs/` GitHub wiki source, wiki publication workflow,
  and local wiki docs validator.
- Replaced the initial dedicated-token wiki publish design with the
  `GITHUB_TOKEN`-based wiki checkout pattern already used in `netdata/ai-agent`.
- Added an explicit options reference page so performance-sensitive options are
  visible in one place.
- First reviewer pass found documentation ambiguity and workflow hardening
  issues. Fixed policy-name mapping, Rust low-level type locations, PR-time
  docs validation, pinned checkout actions, missing-`docs/` publish guard,
  Node/Python performance warnings, `documentation/` vs `docs/` scope, reader
  facade hot-path wording, Explorer anchor wording, and validator edge cases.

## Validation

Acceptance criteria evidence:

- `docs/` contains GitHub wiki source pages:
  - `Home.md`
  - `_Sidebar.md`
  - `Getting-Started.md`
  - `Rust-Crates-And-Packages.md`
  - `Reader-APIs.md`
  - `Writer-APIs.md`
  - `Explorer-And-Netdata-Queries.md`
  - `Hot-Path-Guide.md`
  - `Production-Profiles.md`
  - `Options-Reference.md`
  - `Wiki-Publishing.md`
- `.github/workflows/wiki.yml` publishes `docs/` to
  `${{ github.repository }}.wiki` on trusted `master` pushes using the
  `GITHUB_TOKEN` pattern already used by `netdata/ai-agent`.
- `README.md` points consumers to `docs/Home.md`.
- `.agents/sow/specs/product-scope.md` records `docs/` as the committed
  consumer documentation and wiki source.

Tests or equivalent validation:

- `python3 tests/docs/check_wiki_docs.py`: passed; validates 11 wiki Markdown
  files.
- Workflow YAML parse for all `.github/workflows/*.yml`: passed.
- Sensitive/local-path scan over changed durable artifacts: passed; no raw
  token assignments, local user path, or personal-name artifacts found.
- `git diff --check`: passed.

Real-use evidence:

- Full GitHub wiki publication cannot be executed locally. The workflow matches
  the known working pattern from `netdata/ai-agent` and will be validated by
  the first trusted `master` workflow run after merge/push.

Reviewer findings:

- First pass, `llm-netdata-cloud/glm-5.1`: PRODUCTION GRADE with minor
  documentation concerns. Disposition: fixed Rust low-level type location
  wording and pinned checkout actions.
- First pass, `llm-netdata-cloud/kimi-k2.6`: unavailable because the provider
  returned a usage-limit error. Disposition: recorded as unavailable; no result
  was fabricated.
- First pass, `llm-netdata-cloud/mimo-v2.5-pro`: NOT PRODUCTION GRADE.
  Findings: language-neutral field policy names looked like exact API names;
  PR-time docs validation was missing; Node/Python performance warning was too
  vague. Disposition: fixed with spec/Rust/Go policy-name mapping, split
  validate/publish workflow jobs, and measured performance warning text.
- First pass, `llm-netdata-cloud/minimax-m3-coder`: PRODUCTION GRADE with
  minor recommendations. Disposition: pinned checkout actions and verified the
  required wiki pages list already includes `_Sidebar.md`.
- First pass, `llm-netdata-cloud/deepseek-v4-pro`: NOT PRODUCTION GRADE.
  Findings: `SealOptions` / writer-lock Rust locations were ambiguous, checkout
  actions were not pinned, and `documentation/` vs `docs/` scope was not clear.
  Disposition: fixed docs and workflow.
- First pass, `llm-netdata-cloud/qwen3.6-plus`: NOT PRODUCTION GRADE.
  Findings: field policy names were ambiguous, wiki publish should fail before
  wiping if `docs/` is missing, validator should handle forbidden-text and
  query-string link edge cases, reader facade hot-path wording needed more
  nuance, and Explorer anchor behavior was not documented. Disposition: fixed.
- Second pass, `llm-netdata-cloud/glm-5.1`: PRODUCTION GRADE.
  Disposition: no blocking findings; minor observations were non-blocking.
- Second pass, `llm-netdata-cloud/kimi-k2.6`: unavailable because the provider
  returned the same usage-limit error. Disposition: recorded as unavailable; no
  result was fabricated.
- Second pass, `llm-netdata-cloud/mimo-v2.5-pro`: PRODUCTION GRADE.
  Disposition: low/info observations only; no blocking fix required.
- Second pass, `llm-netdata-cloud/qwen3.6-plus`: PRODUCTION GRADE.
  Disposition: low observations only; no blocking fix required.
- Second pass, `llm-netdata-cloud/minimax-m3-coder`: PRODUCTION GRADE.
  Disposition: minor observations only; no blocking fix required.
- Second pass, `llm-netdata-cloud/deepseek-v4-pro`: PRODUCTION GRADE.
  Disposition: no blocking findings; minor observations were non-blocking.

Same-failure scan:

- Checked existing repo docs/workflows and the related `netdata/ai-agent`
  wiki sync workflow. The relevant existing pattern uses `GITHUB_TOKEN`, not a
  custom wiki token.

Sensitive data gate:

- Passed. No raw secrets or local personal paths were written. The workflow
  uses `secrets.GITHUB_TOKEN` only through `actions/checkout`.

Artifact maintenance gate:

- AGENTS.md: no update needed; repository-wide workflow rules did not change.
- Runtime project skills: no update needed; release/orchestration process did
  not change.
- Specs: updated `.agents/sow/specs/product-scope.md`.
- End-user/operator docs: added `docs/` wiki pages and README pointer.
- End-user/operator skills: no output/reference skills exist for this SDK.
- SOW lifecycle: SOW moved from `pending/` to `current/` for implementation
  and then to `done/` for completion.
- SOW-status.md: updated root and canonical ledgers for completed state.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` with consumer documentation and
  wiki source contract.

Project skills update:

- No project skill update required. This SOW adds consumer docs and a CI wiki
  sync workflow; it does not change how future assistants perform repository
  work beyond normal SOW artifact maintenance.

End-user/operator docs update:

- Added 11 wiki source pages under `docs/` and linked them from `README.md`.

End-user/operator skills update:

- No output/reference skills exist for this SDK, so none were affected.

Lessons:

- Wiki publication workflows are easier to reason about when validation and
  publishing are separate jobs. Pull requests should validate docs, while only
  trusted branch events publish.
- Consumer docs should separate language-neutral spec names from exact
  per-language API identifiers.

Follow-up mapping:

- No follow-up is required from this SOW. Reviewers raised only non-blocking
  observations after the second pass.

## Outcome

Completed. The repository now has a committed `docs/` GitHub wiki source,
local docs validation, and a secure wiki publication workflow that validates
pull requests and publishes from trusted non-PR events with `GITHUB_TOKEN`.

## Lessons Extracted

- Split validation and publication jobs for documentation publishing workflows.
  This lets pull requests prove docs health without exposing write-capable
  publication steps.
- Use language-neutral spec names only when the docs also show exact
  per-language API identifiers.

## Followup

None.

## Regression Log

### Regression - 2026-06-08

What broke:

- The `Publish Wiki` workflow on commit `c5caa984` failed in the `Checkout
  wiki` step. GitHub Actions log evidence from run `27128997916` shows
  `fatal: repository 'https://github.com/netdata/systemd-journal-sdk.wiki/' not
  found`.
- GitHub repository metadata reports `hasWikiEnabled: true`, so the wiki feature
  is enabled, but the backing `.wiki` repository had not been initialized.
- The docs validator contained a hardcoded personal-name forbidden term, used
  `Path.resolve()` without ensuring Markdown links stay under `docs/`, and
  reported forbidden-text failures without enough category/line context.

Why previous validation missed it:

- Local validation could prove `docs/` contents and workflow syntax, but could
  not prove that the remote GitHub wiki repository existed.
- The workflow used `actions/checkout` for `${{ github.repository }}.wiki`,
  which cannot create the initial wiki repository when it is missing.
- Reviewer checks focused on credential and PR safety, but did not require the
  publish path to detect and report an uninitialized wiki repository before the
  checkout step failed.

Repair plan:

1. Replace the wiki checkout step with an authenticated shell clone and an
   explicit preflight check for the backing `.wiki` repository.
2. Fail with a clear setup error when GitHub reports that the wiki feature is
   enabled but the backing wiki Git repository has not been initialized by the
   first Wiki UI page.
3. Push with an ephemeral `GITHUB_TOKEN` authorization header instead of storing
   a tokenized remote URL.
4. Remove hardcoded personal terms from the validator and support optional
   comma-separated forbidden terms through `DOCS_FORBIDDEN_TERMS`.
5. Restrict `.md` link targets to paths under `docs/`.
6. Improve forbidden-text diagnostics with category and line number, without
   printing the matched sensitive text.
7. Record that GitHub UI first-page creation is the remaining external setup
   requirement if the wiki repository is still missing.

Validation:

- `python3 tests/docs/check_wiki_docs.py`: passed.
- `DOCS_FORBIDDEN_TERMS='private-term' python3
  tests/docs/check_wiki_docs.py`: passed.
- Markdown path-containment probe for `docs/Home.md` links: passed; valid
  in-docs links are accepted and `../README.md` plus `/etc/passwd` are
  rejected.
- Workflow YAML parse for all `.github/workflows/*.yml`: passed.
- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed.
- Remote wiki preflight after one-time GitHub Wiki UI initialization: passed.
  Authenticated `git ls-remote
  https://github.com/netdata/systemd-journal-sdk.wiki.git HEAD` returned HEAD
  `457257cfdf063daa32c689b0f927d1af5f31f0ca`.

GitHub Actions validation:

- Repair commit `1b5fdc1a4c70155a0f8ce50ef381ff28ae84bbb3` was pushed to
  `master`.
- `Publish Wiki` run `27134486131`: passed. `Validate wiki docs` and `Publish
  docs to GitHub wiki` both completed successfully.
- Wiki HEAD after publication:
  `96544031d0d540c1137a415d7cea7f7ab7eab50f`.
- Same-commit CI summary:
  - `CodeQL` run `27134486174`: passed.
  - `Codacy SARIF` run `27134486132`: passed.
  - `Coverage` run `27134486232`: passed.
  - `Code Quality: Push on master` run `27134484892`: passed.

Closure note:

- The repair commit kept this SOW in-progress because the final publication
  proof required a pushed GitHub Actions run. This closure commit records the
  successful remote evidence and moves the SOW back to `done/`.

Artifact updates needed:

- `.github/workflows/wiki.yml`
- `tests/docs/check_wiki_docs.py`
- `docs/Wiki-Publishing.md`
- SOW status ledgers and this regression section.

### Regression - 2026-06-08 - Post-Closure Codacy Markdownlint

What broke:

- The closure commit `d0ff0462` triggered `Codacy SARIF` run `27134943299`,
  which failed after local Codacy Analysis CLI reported four markdownlint
  findings.
- GitHub Actions log evidence from run `27134943299` shows:
  - `markdownlint`: 4 issues in 154 files;
  - `markdownlint_MD022`: headings not surrounded by blank lines;
  - `markdownlint_MD032`: lists not surrounded by blank lines.
- Codacy Cloud issue evidence identifies the affected durable artifacts:
  - `SOW-status.md`, line 11: `## Pending`;
  - `SOW-status.md`, line 10: current SOW bullet before the heading;
  - `.agents/sow/SOW-status.md`, line 10: `## Pending`;
  - `.agents/sow/SOW-status.md`, line 9: wrapped current SOW bullet before the
    heading.

Why previous validation missed it:

- The closure validation ran the wiki docs checker, `git diff --check`, and
  the SOW audit. It did not run the Codacy Analysis CLI markdownlint path that
  the pushed workflow runs.
- The status ledger edits were small lifecycle edits, but they still affected
  Markdown files scanned by Codacy.

Repair plan:

1. Add the required blank line between the current SOW list and the `Pending`
   heading in both SOW status ledgers.
2. Reopen this SOW while the CI regression is repaired.
3. Push the repair and verify the `Codacy SARIF` workflow is green before
   closing the SOW again.

Validation:

- `python3 tests/docs/check_wiki_docs.py`: passed.
- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed.
- `markdownlint --disable MD013 -- SOW-status.md .agents/sow/SOW-status.md`:
  passed. `MD013` is disabled for this local spot check because the failed
  Codacy SARIF run did not enable/report line-length findings; the repaired
  failure class was heading/list blank-line spacing.
- Repair commit `4bdfa0984f695a151c6ee94f8b217b3bc777f370` was pushed to
  `master`.
- `Codacy SARIF` run `27135538465`: passed. The `Generate Codacy SARIF` step
  completed successfully and the `Fail on Codacy analysis findings` step was
  skipped because no local Codacy Analysis CLI findings were reported.
- `CodeQL` run `27135538488`: passed.
- `Code Quality: Push on master` run `27135537592`: passed.

Artifact updates needed:

- `SOW-status.md`
- `.agents/sow/SOW-status.md`
- This SOW regression section.

### Regression - 2026-06-08 - Code Scanning Stale Tool Alerts

What broke:

- Final close commit `6e22884a` passed the `Codacy SARIF` workflow, but GitHub
  code scanning still reported four open alerts:
  - `markdownlint_MD032` in `SOW-status.md`;
  - `markdownlint_MD022` in `SOW-status.md`;
  - `markdownlint_MD032` in `.agents/sow/SOW-status.md`;
  - `markdownlint_MD022` in `.agents/sow/SOW-status.md`.
- GitHub code-scanning analysis evidence shows why they stayed open:
  - failed commit `d0ff0462` uploaded category `codacy-analysis-cli` with tool
    `markdownlint` and `results_count: 4`;
  - clean commit `6e22884a` uploaded category `codacy-analysis-cli` with tool
    `codacy-analysis` and `results_count: 0`.
- GitHub treats those as different tools, so the clean `codacy-analysis`
  upload did not close the stale `markdownlint` alerts.

Why previous validation missed it:

- The workflow passed after the Codacy Analysis CLI returned zero findings, but
  the code-scanning alert list was not checked until after the SOW was closed.
- The existing empty-SARIF closeout helper was only used by the no-token
  fallback path, not by the normal clean analysis path.

Repair plan:

1. When enforced Codacy analysis is clean, write and upload an explicit empty
   SARIF file for the known analyzer-specific tool names.
2. Keep this closeout conditional on `codacy_status == 0`, so it cannot hide
   current findings when Codacy Analysis CLI reports problems.
3. Push and verify GitHub code scanning open alerts return to zero.

Validation:

- `python3 tests/docs/check_wiki_docs.py`: passed.
- Workflow YAML parse for all `.github/workflows/*.yml`: passed.
- `python3 tests/code_scanning/write_empty_codacy_sarif.py
  .local/codacy/test-closeout.sarif`: passed.
- Closeout SARIF sanity check: passed. The helper wrote 10 empty tool runs,
  including `markdownlint`.
- `markdownlint --disable MD013 -- SOW-status.md .agents/sow/SOW-status.md`:
  passed.
- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed.

Artifact updates needed:

- `.github/workflows/codacy-sarif.yml`
- This SOW regression section.
