# SOW-0103 - Docs API Perception Restructure And Verified Examples

## Status

Status: in-progress

Sub-state: user decisions recorded; pre-implementation gate ready; implementation
starting.

## Requirements

### Purpose

Improve end-user (developer) documentation for this SDK so that developers
understand how to perceive and choose between the SDK's API surfaces, and make
every documentation code example machine-verified so examples cannot rot.

This SOW is the first of a four-SOW program approved by the user on 2026-06-11:

1. SOW-0103 (this SOW): docs API-perception restructure plus verified-examples
   harness for the current production languages (Rust, Go).
2. SOW-0104: Python parity to Rust (Explorer, Netdata function APIs).
3. SOW-0105: Node.js parity to Rust (Explorer, Netdata function APIs).
4. SOW-0106: Python and Node.js end-user docs with verified examples.

### User Request

On 2026-06-11 the user requested (faithful summary):

- The project is released (v0.6.4); Rust and Go are used in production and
  should have 100% parity in API and features.
- Goal 1: improve end-user (developer) documentation, with verified examples,
  focused on how developers should perceive the various APIs.
- Goal 2: bring Python and Node.js to parity with Rust; Rust is the source of
  truth in this repository.
- The assistant may create SOWs, write documentation, and plan everything.
- One of the available models must be the code implementer and all the others
  must be reviewers. The assistant remains exclusively responsible for the
  outcome.
- Use only `llm-netdata-cloud` models for implementer and reviewer runs.
- Rust and Go must not be touched. Problems found in them become pending SOWs.
- All questions were to be asked before starting; none may interrupt the work.

### Assistant Understanding

Facts:

- `docs/` is the published GitHub wiki source (SOW-0100), validated by
  `tests/docs/check_wiki_docs.py` and published by
  `.github/workflows/wiki.yml`.
- `docs/Rust-API.md` and `docs/Go-API.md` carry many code examples; none are
  compiled or executed anywhere. No CI workflow extracts or verifies doc
  examples.
- `docs/API-Overview.md` describes layers, but no page is organized around the
  developer decision "which API surface should I use, and why".
- The SDK has five consumption surfaces in Rust and Go: idiomatic SDK API,
  libsystemd-compatible facade, Explorer API, Netdata function API, and the
  file-backed journalctl rewrite.
- Python and Node.js wiki API pages do not exist; their coverage arrives in
  SOW-0106 after parity SOW-0104 and SOW-0105.

Inferences:

- A marker convention for verified examples must be invisible in the rendered
  wiki; HTML comments above fenced code blocks satisfy this.
- Rust examples must compile against the local workspace (not crates.io) so
  verification works on every commit; Go examples must use a `replace`
  directive to the local module.

Unknowns:

- None blocking. Harness ergonomics are design work inside this SOW.

### Acceptance Criteria

- A documented marker convention exists for verified examples in `docs/*.md`;
  markers are invisible in rendered wiki output. Verification: rendered
  markdown inspection plus validator pass.
- A harness extracts marked Rust and Go examples, materializes them under
  `.local/`, compiles and runs them against synthetic fixtures, and fails on
  any example error. Verification: local run output recorded in this SOW.
- Every code example in `docs/Rust-API.md` and `docs/Go-API.md` is verified by
  the harness. Examples in other pages are either verified or explicitly
  marked illustrative-only with a recorded reason. Verification: harness
  manifest output.
- `docs/API-Overview.md` (and pages it anchors) is restructured around API
  perception: the five consumption surfaces, when to use each, data lifetime,
  live/snapshot bounds, field-name policies, and the performance contract.
  Verification: reviewer pass plus user-visible diff.
- CI runs the harness when `docs/**`, the harness, or affected SDK surfaces
  change. Verification: green workflow run.
- `tests/docs/check_wiki_docs.py` keeps passing; wiki publishing keeps working.
- Rust and Go source trees are not modified.
- All reviewer findings are resolved or dispositioned; final reviewer batch
  returns production-grade.

## Analysis

Sources checked:

- `.agents/sow/done/SOW-0100-20260608-consumer-docs-github-wiki.md` (wiki
  structure, validator, publishing).
- `docs/*.md` (14 pages), `tests/docs/check_wiki_docs.py`,
  `.github/workflows/wiki.yml`.
- Rust public surface: `rust/src/journal/src/lib.rs`, `facade.rs`,
  `explorer.rs`, `netdata.rs`, `export.rs`, `sealed_verify.rs`.
- Go public surface: `go/journal/` including `explorer.go`, `netdata.go`.
- Shared fixtures and matrices under `tests/`.

Current state:

- Wiki pages are current for v0.6.4 but Rust/Go-centric and example-unverified.
- The validator only checks links, required pages, and forbidden content.
- `Production-Profiles.md` already gives honest performance guidance; it stays.

Risks:

- Harness flakiness in CI (toolchain setup, network for Go modules). Mitigated
  by vendor/offline-friendly generation: path/replace dependencies, no new
  third-party dependencies, `GOFLAGS=-mod=mod` with local replace only.
- Marker convention drift; mitigated by validator extension that rejects
  unmarked fenced blocks in API pages unless explicitly marked illustrative.
- Doc restructure could remove content consumers rely on; mitigated by
  restructure-not-delete policy and reviewer checks.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Doc examples rot because nothing executes them; developers cannot see which
  API surface fits their use case because docs describe layers, not decisions.
  Evidence: no example-verification step exists in any workflow under
  `.github/workflows/`; `docs/API-Overview.md` has no decision-path content.

Evidence reviewed:

- All 14 `docs/*.md` pages, the validator, the wiki workflow, Rust/Go public
  API entry points listed in Analysis, and SOW-0100 lessons.

Affected contracts and surfaces:

- `docs/*.md` content and structure; `tests/docs/` harness additions;
  `.github/workflows/wiki.yml` (or a sibling workflow) for CI verification;
  `docs/Wiki-Publishing.md` for the local validation contract.
- No Rust, Go, Python, or Node.js source changes.

Existing patterns to reuse:

- `tests/docs/check_wiki_docs.py` stdlib-only style and CLI shape.
- Synthetic fixture generation patterns from `tests/` (deterministic dataset
  ingestion, synthetic machine/boot identities per runtime purity rules).
- Wiki link style `[[Target|Label]]` enforced by the validator.

Risk and blast radius:

- Docs-only plus test/CI additions. No production code paths. Worst case is a
  broken wiki publish or a red CI job, both recoverable.

Sensitive data handling plan:

- Examples use synthetic identities and repository-local fixture paths only.
  No host journal access, no real identities, no secrets. Durable artifacts
  keep sanitized evidence only.

Implementation plan:

1. Harness (delegated to implementer model): `tests/docs/verify_examples.py`
   stdlib-only; marker convention `<!-- verify-example: <lang> id=<slug>
   fixture=<name> -->` above fenced blocks and `<!-- illustrative-only:
   <reason> -->` for excluded blocks; extraction; Rust project generation with
   path dependencies to `rust/` workspace crates; Go module generation with
   `replace` to `go/`; fixture builder producing synthetic journal files under
   `.local/docs-examples/fixtures/`; run-and-report with a manifest summary;
   validator extension that rejects unmarked fenced `rust`/`go` blocks in API
   pages.
2. CI (delegated): add an examples-verification job triggered by `docs/**`,
   `tests/docs/**`, `rust/**`, `go/**` changes; cache toolchains; publish gate
   unchanged.
3. Docs restructure (written by the project manager): API-Overview rebuilt
   around the five consumption surfaces and decision paths; Reader-APIs and
   Writer-APIs selection tables sharpened; Rust-API and Go-API examples
   converted to verified blocks and adjusted until green; Getting-Started,
   Options-Reference, Hot-Path-Guide, Explorer-And-Netdata-Queries,
   Production-Profiles aligned; Wiki-Publishing documents the new local
   verification command.
4. Validation, reviewer batch, fixes, audit, commit.

Validation plan:

- Local: `python3 tests/docs/check_wiki_docs.py` and
  `python3 tests/docs/verify_examples.py` both green; harness manifest shows
  every Rust-API and Go-API example verified.
- CI: examples-verification job green on the PR/commit.
- Reviewer batch: all five non-implementer `llm-netdata-cloud` models, read
  only, whole-SOW scope, iterated until clean.
- `.agents/sow/audit.sh` clean before close.

Artifact impact plan:

- AGENTS.md: routing decision update (done with SOW creation).
- Runtime project skills: orchestration skill routing update (done with SOW
  creation); add docs-verification knowledge if it becomes a durable workflow.
- Specs: no product behavior change; spec update not expected. If the harness
  becomes a durable contract, record it in a spec or skill at close.
- End-user/operator docs: this SOW is the docs change.
- End-user/operator skills: none exist; not affected.
- SOW lifecycle: SOW-0065 closed as superseded by SOW-0103..0106.
- SOW-status.md: updated with program state.

Open-source reference evidence:

- None checked for SOW creation; harness design follows repository-local
  patterns. If external doc-test prior art is consulted during implementation,
  it will be cited as `owner/repo @ commit`.

Open decisions:

- None. All user decisions recorded below.

## Implications And Decisions

1. 2026-06-11 program order (user decision)
   - Options: A) docs first, then Python parity, then Node parity, then
     Python/Node docs; B) parity first, docs once at the end.
   - Decision: A. SOW-0103 -> SOW-0104 -> SOW-0105 -> SOW-0106.
   - Implication: shared pages get a small second pass in SOW-0106 to add
     Python/Node columns.

2. 2026-06-11 implementation routing (user decision)
   - Decision: external implementer model enabled for code; all other pool
     models are read-only reviewers; only `llm-netdata-cloud` models are
     allowed. The project manager writes documentation prose and remains
     exclusively responsible for the outcome.
   - Selected implementer: `llm-netdata-cloud/minimax-m3-coder` (coding mode,
     no reviewer agent flag). Fallback on unavailability:
     `llm-netdata-cloud/glm-5.1`, with the failure recorded here.
   - Reviewers: `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/kimi-k2.6`,
     `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.7-plus`,
     `llm-netdata-cloud/deepseek-v4-pro` (the implementer model never reviews
     its own work).
   - Note: the reviewer pool entry `qwen3.6-plus` in AGENTS.md was stale; the
     available pool model is `qwen3.7-plus` and AGENTS.md was updated.

3. 2026-06-11 Rust/Go freeze (user decision)
   - Decision: Rust and Go sources must not be modified by this program. Any
     problem found in them becomes a pending SOW.

4. 2026-06-11 Node TypeScript definitions (user decision, applies to SOW-0105)
   - Decision: add hand-written `.d.ts` with CI type-check in the Node parity
     SOW.

5. 2026-06-11 Python packaging (user decision, applies to SOW-0104)
   - Decision: add `pyproject.toml` (metadata only, no publication; publication
     stays in SOW-0066).

6. 2026-06-11 version alignment (user decision, applies at next release)
   - Decision: align Python and Node.js package versions to the repository
     release version at the next release tag, after parity SOWs complete.

## Plan

1. Implementer chunk 1: harness plus validator extension plus CI job.
2. Project-manager chunk 2: docs restructure and example conversion, iterating
   against the harness until green.
3. Whole-SOW reviewer batch; fix; repeat until clean.
4. Audit, close, commit.

## Delegation Plan

Implementer:

- `llm-netdata-cloud/minimax-m3-coder` via
  `timeout 1800 opencode run -m "llm-netdata-cloud/minimax-m3-coder" "<prompt>"`
  (normal coding mode). Docs prose is written by the project manager.

Reviewers:

- `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/kimi-k2.6`,
  `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.7-plus`,
  `llm-netdata-cloud/deepseek-v4-pro`, read-only
  (`--agent code-reviewer`), run in parallel after the whole SOW is locally
  implemented and validated; same whole-SOW scope on every iteration with fix
  notes appended.

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

- Implementer unavailability or persistent failure: record here, fall back to
  `llm-netdata-cloud/glm-5.1`, adjust the reviewer set so the implementer
  never reviews its own work.
- Reviewer unavailability (quota): record and proceed with remaining
  reviewers, matching SOW-0093 precedent.
- Audit failure: repair in-repo, rerun, record clean result before close.

## Execution Log

### 2026-06-11

- Created this SOW with all user decisions recorded; updated AGENTS.md routing
  and reviewer pool; updated the orchestration skill; created pending
  SOW-0104, SOW-0105, SOW-0106; closed SOW-0065 as superseded.

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

- This SOW contains no raw sensitive data.

Artifact maintenance gate:

- Pending close.

Specs update:

- Pending close.

Project skills update:

- Pending close.

End-user/operator docs update:

- Pending close (this SOW is the docs change).

End-user/operator skills update:

- Pending close.

Lessons:

- Pending close.

Follow-up mapping:

- Pending close.

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
