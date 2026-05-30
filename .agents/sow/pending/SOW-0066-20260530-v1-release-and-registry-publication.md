# SOW-0066 - V1 Release And Registry Publication

## Status

Status: open

Sub-state: pending final production release and language registry publication
after SDK compatibility, portability, corpus validation, and integration gates
are complete.

## Requirements

### Purpose

Publish the SDK as a stable `v1.0.0` multi-language project so downstream
consumers can depend on language registry packages or versioned module tags
without vendoring SDK source.

### User Request

The user stated that once all work is done, the project needs to publish
`v1.0.0` and register the project in language registries so consumers,
including Netdata, can use it without vendoring anything.

### Assistant Understanding

Facts:

- The SDK has four language surfaces: Rust, Go, Node.js, and Python.
- Go consumers need versioned module tags, including language-specific Go module
  tags when the Go module lives below the repository root.
- Node.js consumers need npm package publication.
- Python consumers need Python package metadata and registry publication.
- Rust consumers need crate publication or a documented versioned git strategy
  if the workspace/crate structure is not yet registry-ready.
- Secrets, registry tokens, account names, and credentials must not be written
  into durable artifacts.
- `v1.0.0` should not ship until compatibility, portability, real-corpus
  validation, and cross-language parity gates are complete.

Inferences:

- This SOW should run after SOW-0055, SOW-0063, SOW-0064, SOW-0065, and the
  Netdata integration readiness work are complete or explicitly waived by the
  user.
- Registry publication requires package metadata, README/API docs, license
  metadata, version alignment, changelog/release notes, and clean install tests
  from registries or registry dry-runs.
- A final release should include immutable git tags and registry package
  versions that all point to the same source commit.

Unknowns:

- Exact registry names and organization ownership for Rust, npm, and Python.
- Whether every Rust crate in the workspace should be published or only a
  curated public subset.
- Whether registry credentials are already configured locally or must be
  provided interactively by the user.
- Whether Netdata should consume `v1.0.0` before or after all language registry
  packages are public.

### Acceptance Criteria

- Release readiness checklist passes for Rust, Go, Node.js, and Python.
- Public package metadata is complete: package names, versions, license,
  repository URL, descriptions, keywords/classifiers, README links, minimum
  runtime/compiler versions, dependency versions, and included files.
- Installation instructions are documented for every language.
- Registry dry-run/package validation passes for every package type before any
  real publish.
- `v1.0.0` source tag is created only after tests, compatibility matrices,
  corpus evaluation, and release checks pass.
- Go module tags are created as required, including `go/v1.0.0` if the Go
  module path still requires a submodule tag.
- Rust crates are published to the selected registry or the SOW records an
  explicit user-approved alternative.
- Node.js package is published to npm under the approved package name.
- Python package is published to the approved Python registry package name.
- A clean consumer install test proves each language can import/use the SDK from
  the published artifact or module tag without vendoring.
- Release notes identify compatibility guarantees, platform support, known
  limitations, and migration guidance without leaking internal sensitive data.

## Analysis

Sources checked:

- User request in this thread.
- Existing release tagging project skill:
  `.agents/skills/project-release-tagging/SKILL.md`.
- Existing package roots: `rust/`, `go/`, `node/`, and `python/`.
- Existing project SOW/status dependency graph.

Current state:

- The project has previous pre-1.0 tags and Go module tags, but no final
  `v1.0.0` publication workflow recorded as a SOW.
- Cross-platform portability and real-world corpus evaluation are pending.
- Future language parity closure is now tracked by SOW-0065.
- Netdata integration SOWs are pending and depend on SDK readiness.

Risks:

- Publishing before compatibility/corpus/portability gates close can create a
  stable API around defects.
- Registry names may be unavailable or controlled by a different account.
- Publishing credentials are sensitive and must be handled outside committed
  artifacts.
- Multi-language version skew can confuse consumers.
- Go module subdirectory tags are easy to miss, causing consumers to resolve the
  wrong version.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- A final release requires more than tagging. The repository needs stable APIs,
  completed compatibility and portability gates, complete package metadata,
  registry validation, docs, clean install tests, and coordinated version tags.

Evidence reviewed:

- User request for final `v1.0.0` and language registry publication.
- Pending SOWs that still affect release readiness: SOW-0055, SOW-0063,
  SOW-0064, SOW-0065, and Netdata integration SOWs.
- Project release-tagging skill exists and should be used when this SOW starts.

Affected contracts and surfaces:

- Public API stability for Rust, Go, Node.js, and Python.
- Package metadata and registry artifacts.
- Git tags and release notes.
- Consumer installation docs.
- Netdata dependency strategy.
- Security and credential handling.

Existing patterns to reuse:

- `project-release-tagging` skill for tags and Go module release tags.
- Existing language README files.
- Existing tests and conformance matrices.
- Existing SOW release/tagging history for `v0.2.0` and `v0.3.0` style
  rollback points.

Risk and blast radius:

- High. Registry publication is externally visible and hard to undo cleanly.
- Do not publish until all release blockers are closed or explicitly waived by
  the user.

Sensitive data handling plan:

- Do not write registry tokens, credentials, account IDs, or session cookies to
  repo files, SOWs, scripts, docs, command logs, or commit messages.
- If registry credentials are required, stop and ask the user for an interactive
  secure path.
- Release artifacts must not include generated corpora, local journals, `.local`
  caches, or sensitive reports.

Implementation plan:

1. Confirm release prerequisites and blockers.
2. Audit package metadata and public API docs in all four languages.
3. Add or repair packaging files and registry metadata.
4. Run full test, conformance, interoperability, portability, corpus, and
   consumer-install validation.
5. Prepare release notes and changelog.
6. Run registry dry-runs/package checks.
7. Ask the user to confirm publish action and credential path.
8. Create and push `v1.0.0` and language-specific tags.
9. Publish packages to approved registries.
10. Verify clean installs from registries/module tags and update docs/status.

Validation plan:

- Full repository test suite and all compatibility matrices.
- Cross-platform build/import/install checks.
- Real-world corpus evaluation gate from SOW-0064.
- Package dry-run validation for Rust, Node.js, and Python.
- `go list` / clean module install test for Go at the release tag.
- Clean consumer projects for all languages, using only registry/package/module
  sources.
- `.agents/sow/audit.sh` and `git diff --check`.
- Whole-SOW read-only reviewer pass before publishing and after package metadata
  changes.

Artifact impact plan:

- AGENTS.md: no update expected unless release workflow becomes a project-wide
  rule.
- Runtime project skills: use and possibly update `project-release-tagging` if
  gaps are found.
- Specs: update product scope with `v1.0.0` support guarantees.
- End-user/operator docs: update install, usage, platform support, compatibility
  guarantees, and release notes.
- End-user/operator skills: no output/reference skill expected unless package
  publication creates a reusable downstream workflow.
- SOW lifecycle: final release SOW; do not close before tags/packages are
  verified.
- SOW-status.md: add this SOW to Pending.

Open-source reference evidence:

- No external open-source repositories were checked while creating this SOW.
  Implementation should use official registry documentation for current publish
  commands and metadata requirements.

Open decisions:

- Blocked until prerequisite quality gates close or the user explicitly waives
  them.
- User decision required before real registry publication and before handling
  credentials.
- User decision required for exact package names/organizations if they are not
  already reserved.

## Implications And Decisions

1. 2026-05-30 final release goal
   - Decision: record a future `v1.0.0` release and registry publication SOW.
   - Implication: consumers should eventually depend on published packages or
     module tags instead of vendoring source.
   - Risk: publishing too early creates long-lived compatibility and support
     obligations.

2. 2026-05-30 credentials boundary
   - Decision: no registry credentials or tokens may be written to durable
     artifacts.
   - Implication: publishing requires an interactive or preconfigured secure
     credential path.
   - Risk: accidental token capture would require incident handling.

## Plan

1. Close release blockers.
2. Audit package metadata and docs.
3. Run full release validation.
4. Dry-run packages.
5. Confirm publish decision and credentials.
6. Tag, publish, verify, and document.

## Delegation Plan

Implementer:

- Local implementation by default. External implementers are not enabled unless
  the user explicitly changes routing.

Reviewers:

- Use read-only reviewers from the approved pool before real publication.

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

- If any registry package name is unavailable, stop and present options.
- If any package dry-run fails, fix metadata and rerun before publishing.
- If any post-publish install test fails, stop, record the issue, and prepare a
  patch release plan instead of silently republishing.

## Execution Log

### 2026-05-30

- Created this pending SOW from the user's final release and registry
  publication requirement.

## Validation

Acceptance criteria evidence:

- Pending implementation.

Tests or equivalent validation:

- Pending implementation.

Real-use evidence:

- Pending implementation.

Reviewer findings:

- Pending implementation.

Same-failure scan:

- Pending implementation.

Sensitive data gate:

- Planning artifact contains no registry credentials, tokens, account IDs, or
  sensitive runtime data.

Artifact maintenance gate:

- AGENTS.md: no update during SOW creation.
- Runtime project skills: no update during SOW creation.
- Specs: pending implementation.
- End-user/operator docs: pending implementation.
- End-user/operator skills: no output/reference skill affected during SOW
  creation.
- SOW lifecycle: created as `Status: open` under `.agents/sow/pending/`.
- SOW-status.md: updated to list this SOW as pending.

Specs update:

- Pending implementation.

Project skills update:

- Pending implementation.

End-user/operator docs update:

- Pending implementation.

End-user/operator skills update:

- Pending implementation.

Lessons:

- Registry publication should be treated as a product release gate, not just a
  tag.

Follow-up mapping:

- None yet.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
