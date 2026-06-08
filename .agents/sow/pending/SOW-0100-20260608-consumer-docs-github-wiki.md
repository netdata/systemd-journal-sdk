# SOW-0100 - Consumer Docs And GitHub Wiki Publication

## Status

Status: open

Sub-state: requirements captured; implementation not started.

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

- No external repository was checked yet. Implementation should inspect current
  GitHub Actions documentation and, if useful, comparable wiki publish workflows
  from open-source repositories.

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

- AGENTS.md: Pending.
- Runtime project skills: Pending.
- Specs: Pending.
- End-user/operator docs: Pending.
- End-user/operator skills: Pending.
- SOW lifecycle: Pending.
- SOW-status.md: Pending.

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
